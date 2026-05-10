use crate::app::{
    App, AppConfig, BlocklistProfileConfig, DEFAULT_BLOCKLIST_PROFILE_NAME, Local,
    PendingTimerAction, TimerPhase, TimerState, effective_blocked_sites_for_profile,
    format_duration_label, profile_index, profile_spec_for, task_label_index,
};
use crate::session_recovery::{self, InProgressSessionSnapshot, WorkflowStateSnapshot};
use chrono::{LocalResult, TimeZone};
use std::time::Instant;

impl App {
    pub(super) fn restore_in_progress_session(&mut self) {
        let loaded_snapshot = match session_recovery::load() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.phase_notification =
                    Some(format!("Ignored saved in-progress session: {error}."));
                if let Err(clear_error) = session_recovery::clear() {
                    self.config_error = Some(format!(
                        "session recovery cleanup failed after load error: {clear_error}"
                    ));
                }
                return;
            }
        };

        let Some(snapshot) = loaded_snapshot else {
            return;
        };

        if let Err(reason) = self.try_apply_recovery_snapshot(snapshot.clone()) {
            self.phase_notification = Some(format!("Ignored saved in-progress session: {reason}."));
            if let Err(clear_error) = session_recovery::clear() {
                self.config_error = Some(format!(
                    "session recovery cleanup failed after invalid state: {clear_error}"
                ));
            }
            return;
        }

        self.phase_notification = Some(format!(
            "Recovered in-progress {} session ({} remaining).",
            snapshot.phase().label(),
            format_duration_label(snapshot.remaining_secs)
        ));
    }

    pub(super) fn restore_cli_workflow_state(&mut self) {
        let loaded_snapshot = match session_recovery::load_workflow_state() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.config_error = Some(format!("workflow state load failed: {error}"));
                if let Err(clear_error) = session_recovery::clear_workflow_state() {
                    self.config_error = Some(format!(
                        "workflow state cleanup failed after load error: {clear_error}"
                    ));
                }
                return;
            }
        };

        let Some(snapshot) = loaded_snapshot else {
            return;
        };

        self.current_frame_now = Local::now();
        self.schedule_delayed_occurrence_key = None;
        self.schedule_delay_until = None;

        if let (Some(delayed_key), Some(delay_until_epoch_secs)) = (
            snapshot.schedule_delayed_occurrence_key,
            snapshot.schedule_delay_until_epoch_secs,
        ) && let Some(delay_until) = local_datetime_from_epoch_secs(delay_until_epoch_secs)
            && delay_until > self.current_frame_now
        {
            self.schedule_delayed_occurrence_key = Some(delayed_key);
            self.schedule_delay_until = Some(delay_until);
        }

        self.pending_timer_action = None;
        self.break_glass_expires_at = None;
        if self.focus_session_active_for_current_state() {
            if let Some(expires_at_epoch_secs) = snapshot.break_glass_expires_at_epoch_secs
                && let Some(expires_at) = local_datetime_from_epoch_secs(expires_at_epoch_secs)
                && expires_at > self.current_frame_now
            {
                let remaining = expires_at
                    .signed_duration_since(self.current_frame_now)
                    .to_std()
                    .ok();
                self.break_glass_expires_at = remaining.map(|remaining| Instant::now() + remaining);
            }
            if snapshot.break_glass_confirmation_pending && self.break_glass_expires_at.is_none() {
                self.pending_timer_action = Some(PendingTimerAction::BreakGlassOverride);
            }
        }

        if let Err(error) = self.sync_cli_workflow_state() {
            self.config_error = Some(error);
        }
    }

    fn try_apply_recovery_snapshot(
        &mut self,
        snapshot: InProgressSessionSnapshot,
    ) -> Result<(), String> {
        let profile_spec = profile_spec_for(snapshot.selected_profile, &self.custom_profile);
        let mut recovered_timer = TimerState::with_profile(
            profile_spec.focus_secs,
            profile_spec.short_break_secs,
            profile_spec.long_break_secs,
            profile_spec.long_break_interval,
        );
        recovered_timer.phase = snapshot.phase();
        recovered_timer.status = snapshot.status();
        recovered_timer.remaining_secs = snapshot.remaining_secs;
        recovered_timer.pomodoros_completed = snapshot.pomodoros_completed;
        let metadata_fallback_enabled = self.feature_flags.metadata_task_label_fallback;
        snapshot.validate_for_timer_with_fallback(&recovered_timer, metadata_fallback_enabled)?;

        let task_label = snapshot
            .normalized_task_label()
            .ok_or_else(|| "saved task label is missing or invalid".to_string())?;
        let selected_task_label =
            if let Some(existing_index) = task_label_index(&self.task_labels, &task_label) {
                self.task_labels[existing_index].clone()
            } else {
                self.task_labels.push(task_label.clone());
                task_label
            };

        self.selected_profile = snapshot.selected_profile;
        self.profile_selection_index = profile_index(snapshot.selected_profile);
        self.load_automation_runtime_for_profile(snapshot.selected_profile);
        self.current_frame_now = Local::now();
        self.timer = recovered_timer;
        self.selected_task_label = Some(selected_task_label);
        self.pending_timer_action = None;
        self.break_glass_expires_at = None;
        self.active_focus_task_label = if self.timer.phase == TimerPhase::Focus {
            self.selected_task_label.clone()
        } else {
            None
        };
        self.active_focus_intention = if self.timer.phase == TimerPhase::Focus {
            snapshot.normalized_focus_intention_with_fallback(metadata_fallback_enabled)
        } else {
            None
        };
        self.active_focus_task_note = if self.timer.phase == TimerPhase::Focus {
            snapshot.normalized_task_note_with_fallback(metadata_fallback_enabled)
        } else {
            None
        };
        self.active_focus_profile = if self.timer.phase == TimerPhase::Focus {
            Some(self.selected_profile)
        } else {
            None
        };
        self.sync_task_planner_state();
        Ok(())
    }

    fn recovery_snapshot_task_label_fallback(
        recovery_task_label: &Option<String>,
        metadata_fallback_enabled: bool,
    ) -> Option<String> {
        if metadata_fallback_enabled {
            recovery_task_label.clone()
        } else {
            None
        }
    }

    fn recovery_snapshot_metadata_value(
        focus_active: bool,
        active_value: Option<String>,
        recovery_task_label: &Option<String>,
        metadata_fallback_enabled: bool,
    ) -> Option<String> {
        if focus_active {
            active_value.or_else(|| {
                Self::recovery_snapshot_task_label_fallback(
                    recovery_task_label,
                    metadata_fallback_enabled,
                )
            })
        } else {
            Self::recovery_snapshot_task_label_fallback(
                recovery_task_label,
                metadata_fallback_enabled,
            )
        }
    }

    pub(super) fn sync_recovery_snapshot(&mut self) {
        let focus_active = self.focus_session_active_for_current_state();
        let metadata_fallback_enabled = self.feature_flags.metadata_task_label_fallback;
        let recovery_task_label = if focus_active {
            self.active_focus_task_label
                .clone()
                .or_else(|| self.selected_task_label.clone())
        } else {
            self.selected_task_label.clone()
        };
        let recovery_focus_intention = Self::recovery_snapshot_metadata_value(
            focus_active,
            self.active_focus_intention.clone(),
            &recovery_task_label,
            metadata_fallback_enabled,
        );
        let recovery_task_note = Self::recovery_snapshot_metadata_value(
            focus_active,
            self.active_focus_task_note.clone(),
            &recovery_task_label,
            metadata_fallback_enabled,
        );

        let snapshot = InProgressSessionSnapshot::from_timer_state_with_metadata_with_fallback(
            &self.timer,
            recovery_task_label,
            recovery_focus_intention,
            recovery_task_note,
            self.selected_profile,
            metadata_fallback_enabled,
        );

        match snapshot {
            Some(snapshot) => {
                if let Err(error) = session_recovery::save(&snapshot) {
                    self.config_error = Some(format!("session recovery save failed: {error}"));
                }
            }
            None => {
                if let Err(error) = session_recovery::clear() {
                    self.config_error = Some(format!("session recovery clear failed: {error}"));
                }
            }
        }
    }

    pub(super) fn sync_cli_workflow_state(&mut self) -> Result<(), String> {
        let now = Local::now();
        let schedule_state = match (
            self.schedule_delayed_occurrence_key.clone(),
            self.schedule_delay_until,
        ) {
            (Some(delayed_key), Some(delayed_until)) if delayed_until > now => {
                Some((Some(delayed_key), Some(delayed_until.timestamp())))
            }
            _ => None,
        };

        let break_glass_expires_at_epoch_secs = self
            .break_glass_expires_at
            .and_then(|deadline| deadline.checked_duration_since(Instant::now()))
            .and_then(|remaining| chrono::Duration::from_std(remaining).ok())
            .map(|remaining| (now + remaining).timestamp());

        let break_glass_confirmation_pending = self.break_glass_confirmation_pending()
            && self.focus_session_active_for_current_state();
        let snapshot = WorkflowStateSnapshot {
            schedule_delayed_occurrence_key: schedule_state
                .as_ref()
                .and_then(|(delayed_key, _)| delayed_key.clone()),
            schedule_delay_until_epoch_secs: schedule_state
                .as_ref()
                .and_then(|(_, delayed_until)| *delayed_until),
            break_glass_expires_at_epoch_secs,
            break_glass_confirmation_pending,
        };

        let should_persist = snapshot.schedule_delayed_occurrence_key.is_some()
            || snapshot.break_glass_expires_at_epoch_secs.is_some()
            || snapshot.break_glass_confirmation_pending;
        if should_persist {
            session_recovery::save_workflow_state(&snapshot)
                .map_err(|error| format!("workflow state save failed: {error}"))
        } else {
            session_recovery::clear_workflow_state()
                .map_err(|error| format!("workflow state clear failed: {error}"))
        }
    }

    /// Persist the current blocked-sites list and timer preferences to disk.
    /// Failures are best-effort; the error is surfaced through `config_error`.
    pub(super) fn persisted_config(&self) -> AppConfig {
        let custom_profile = self.custom_profile.normalized();
        let mut blocklist_profiles = self.blocklist_profiles.clone();
        if blocklist_profiles.is_empty() {
            blocklist_profiles.push(BlocklistProfileConfig {
                name: DEFAULT_BLOCKLIST_PROFILE_NAME.to_string(),
                sites: self.blocker.sites.clone(),
                allowlist_sites: Vec::new(),
            });
        }
        let active_index = self
            .active_blocklist_profile
            .min(blocklist_profiles.len().saturating_sub(1));
        let selected_blocklist_profile = blocklist_profiles
            .get(active_index)
            .or_else(|| blocklist_profiles.first())
            .map(|profile| profile.name.clone())
            .unwrap_or_else(|| DEFAULT_BLOCKLIST_PROFILE_NAME.to_string());
        let mirrored_blocked_sites = blocklist_profiles
            .get(active_index)
            .map(effective_blocked_sites_for_profile)
            .unwrap_or_default();
        let blocked_sites = if self.feature_flags.legacy_blocked_sites_mirror {
            mirrored_blocked_sites
        } else {
            self.legacy_blocked_sites.clone()
        };
        let mut profile_automation = self.profile_automation.clone();
        profile_automation
            .set_for_profile(self.selected_profile, self.selected_profile_automation());
        AppConfig {
            // Keep legacy fields aligned with the editable custom profile so
            // older releases retain user-configured values.
            focus_secs: custom_profile.focus_secs,
            short_break_secs: custom_profile.short_break_secs,
            long_break_secs: custom_profile.long_break_secs,
            long_break_interval: custom_profile.long_break_interval,
            blocked_sites,
            blocklist_profiles,
            selected_blocklist_profile,
            selected_profile: self.selected_profile,
            custom_profile: Some(custom_profile),
            break_templates: self.break_templates.clone(),
            selected_break_template: self.selected_break_template_for_persistence(),
            selected_theme_preset: self.selected_theme_preset,
            notifications: self.notification_settings,
            auto_start: self.auto_start,
            recurring_schedule: self.recurring_schedule.clone(),
            schedule_runtime: self.schedule_runtime,
            profile_automation: Some(profile_automation),
            strict_mode: self.strict_mode,
            break_glass_duration_secs: self.break_glass_duration_secs,
            daily_goal: self.daily_goal,
            weekly_goal: self.weekly_goal,
            monthly_goal: self.monthly_goal,
            goal_carry_over: self.goal_carry_over,
            stats_retention: self.stats_retention,
            wakatime: self.wakatime_metadata.clone(),
            wakatime_runtime: self.wakatime_runtime.clone(),
            feature_flags: self.feature_flags,
            shortcuts: self.shortcuts.to_config(),
        }
    }

    /// Persist the current blocked-sites list and timer preferences to disk.
    /// Failures are best-effort; the error is surfaced through `config_error`.
    #[cfg(not(test))]
    pub(super) fn save_config(&mut self) {
        let config = self.persisted_config();
        if let Err(e) = config.save() {
            self.config_error = Some(format!("config save failed: {e}"));
        } else {
            self.config_error = None;
        }
    }

    #[cfg(test)]
    pub(super) fn save_config(&mut self) {
        let _ = self.persisted_config();
        self.config_error = None;
    }

    #[cfg(not(test))]
    fn save_stats(&mut self) {
        if let Err(e) = self.stats.save() {
            self.stats_error = Some(format!("stats save failed: {e}"));
        } else {
            self.stats_error = None;
        }
    }

    #[cfg(test)]
    fn save_stats(&mut self) {
        self.stats_error = None;
    }

    pub(super) fn flush_stats_if_dirty(&mut self, force_partial: bool) {
        if !(self.stats_dirty || (force_partial && self.stats_has_unsaved_elapsed)) {
            return;
        }

        if self
            .stats
            .apply_retention_policy(self.stats_retention, Local::now().date_naive())
            .any_removed()
        {
            self.stats_dirty = true;
        }
        self.save_stats();
        if self.stats_error.is_none() {
            self.stats_dirty = false;
            self.stats_has_unsaved_elapsed = false;
        }
    }
}

fn local_datetime_from_epoch_secs(epoch_secs: i64) -> Option<chrono::DateTime<Local>> {
    match Local.timestamp_opt(epoch_secs, 0) {
        LocalResult::Single(value) => Some(value),
        LocalResult::Ambiguous(earliest, _) => Some(earliest),
        LocalResult::None => None,
    }
}
