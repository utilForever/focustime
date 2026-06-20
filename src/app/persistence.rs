use crate::app::{
    App, AppConfig, BlocklistProfileConfig, DEFAULT_BLOCKLIST_PROFILE_NAME, HistoryDashboardConfig,
    Local, PendingTimerAction, TimerPhase, TimerState, TimerStatus,
    blocking_backend_config_for_persistence, format_duration_label, occurrence_key, profile_index,
    profile_spec_for, task_label_index,
};
use crate::session_recovery::{
    self, InProgressSessionSnapshot, WorkflowStateSnapshot,
    WorkflowTemporaryAllowlistEntrySnapshot, WorkflowTemporaryOverrideKind,
    WorkflowTemporaryOverrideSnapshot,
};
use crate::temporary_allowlist::{
    TemporaryAllowlistEntry, prune_expired_temporary_allowlist_entries,
};
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

        let recovered_snapshot = match self.try_apply_recovery_snapshot(snapshot) {
            Ok(snapshot) => snapshot,
            Err(reason) => {
                self.phase_notification =
                    Some(format!("Ignored saved in-progress session: {reason}."));
                if let Err(clear_error) = session_recovery::clear() {
                    self.config_error = Some(format!(
                        "session recovery cleanup failed after invalid state: {clear_error}"
                    ));
                }
                return;
            }
        };

        if recovered_snapshot.status() == TimerStatus::Idle {
            self.phase_notification = Some(format!(
                "Recovered elapsed timer state into {} phase ({} remaining).",
                recovered_snapshot.phase().label(),
                format_duration_label(recovered_snapshot.remaining_secs)
            ));
        } else {
            self.phase_notification = Some(format!(
                "Recovered in-progress {} session ({} remaining).",
                recovered_snapshot.phase().label(),
                format_duration_label(recovered_snapshot.remaining_secs)
            ));
        }
    }

    pub(super) fn restore_cli_workflow_state(&mut self) {
        let Some(snapshot) = self.load_cli_workflow_snapshot_for_restore() else {
            return;
        };
        let temporary_overrides = snapshot.temporary_overrides_with_legacy_fallback();
        let WorkflowStateSnapshot {
            schedule_delayed_occurrence_key,
            schedule_delay_until_epoch_secs,
            schedule_armed_occurrence_key,
            last_schedule_occurrence_key,
            strict_reset_confirmation_pending,
            ..
        } = snapshot;

        self.reset_cli_workflow_runtime_state_for_restore();
        let mut ignored_runtime_artifacts: Vec<&'static str> = Vec::new();
        self.restore_schedule_delay_runtime_state(
            schedule_delayed_occurrence_key,
            schedule_delay_until_epoch_secs,
            &mut ignored_runtime_artifacts,
        );

        let active_occurrence_key = self
            .active_schedule_occurrence_at(self.current_frame_now)
            .map(|occurrence| occurrence_key(&occurrence));
        self.restore_schedule_continuity_runtime_state(
            schedule_armed_occurrence_key,
            last_schedule_occurrence_key,
            active_occurrence_key.as_deref(),
            &mut ignored_runtime_artifacts,
        );
        self.restore_break_glass_runtime_state(
            &temporary_overrides,
            &mut ignored_runtime_artifacts,
        );
        self.restore_temporary_allowlist_runtime_state(
            &temporary_overrides,
            &mut ignored_runtime_artifacts,
        );
        self.recompute_blocker_sites_from_active_profile();
        self.restore_strict_reset_runtime_state(
            strict_reset_confirmation_pending,
            &mut ignored_runtime_artifacts,
        );
        self.sync_restored_cli_workflow_state();
        self.append_ignored_runtime_artifact_notice(&ignored_runtime_artifacts);
    }

    fn load_cli_workflow_snapshot_for_restore(&mut self) -> Option<WorkflowStateSnapshot> {
        match session_recovery::load_workflow_state() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.config_error = Some(format!("workflow state load failed: {error}"));
                if let Err(clear_error) = session_recovery::clear_workflow_state() {
                    self.config_error = Some(format!(
                        "workflow state cleanup failed after load error: {clear_error}"
                    ));
                }
                None
            }
        }
    }

    fn reset_cli_workflow_runtime_state_for_restore(&mut self) {
        self.current_frame_now = Local::now();
        self.schedule_delayed_occurrence_key = None;
        self.schedule_delay_until = None;
        self.schedule_armed_occurrence_key = None;
        self.last_schedule_occurrence_key = None;
        self.pending_timer_action = None;
        self.break_glass_expires_at = None;
        self.temporary_allowlist_entries.clear();
    }

    fn restore_schedule_delay_runtime_state(
        &mut self,
        schedule_delayed_occurrence_key: Option<String>,
        schedule_delay_until_epoch_secs: Option<i64>,
        ignored_runtime_artifacts: &mut Vec<&'static str>,
    ) {
        let has_saved_schedule_delay_state =
            schedule_delayed_occurrence_key.is_some() || schedule_delay_until_epoch_secs.is_some();
        if let (Some(delayed_key), Some(delay_until_epoch_secs)) = (
            schedule_delayed_occurrence_key,
            schedule_delay_until_epoch_secs,
        ) && let Some(delay_until) = local_datetime_from_epoch_secs(delay_until_epoch_secs)
            && delay_until > self.current_frame_now
        {
            self.schedule_delayed_occurrence_key = Some(delayed_key);
            self.schedule_delay_until = Some(delay_until);
        } else if has_saved_schedule_delay_state {
            push_ignored_artifact(ignored_runtime_artifacts, "schedule delay state");
        }
    }

    fn restore_schedule_continuity_runtime_state(
        &mut self,
        schedule_armed_occurrence_key: Option<String>,
        last_schedule_occurrence_key: Option<String>,
        active_occurrence_key: Option<&str>,
        ignored_runtime_artifacts: &mut Vec<&'static str>,
    ) {
        if let Some(armed_key) = schedule_armed_occurrence_key {
            if !self.focus_session_active_for_current_state()
                && active_occurrence_key == Some(armed_key.as_str())
            {
                self.schedule_armed_occurrence_key = Some(armed_key);
            } else {
                push_ignored_artifact(ignored_runtime_artifacts, "schedule arm state");
            }
        }
        if let Some(last_key) = last_schedule_occurrence_key {
            if active_occurrence_key == Some(last_key.as_str()) {
                self.last_schedule_occurrence_key = Some(last_key);
            } else {
                push_ignored_artifact(ignored_runtime_artifacts, "schedule trigger continuity");
            }
        }
    }

    fn restore_break_glass_runtime_state(
        &mut self,
        temporary_overrides: &[WorkflowTemporaryOverrideSnapshot],
        ignored_runtime_artifacts: &mut Vec<&'static str>,
    ) {
        let break_glass_expires_at_epoch_secs = temporary_overrides
            .iter()
            .filter(|override_entry| {
                override_entry.kind == WorkflowTemporaryOverrideKind::BreakGlass
            })
            .filter_map(|override_entry| override_entry.expires_at_epoch_secs)
            .max();
        let break_glass_confirmation_pending = temporary_overrides.iter().any(|override_entry| {
            override_entry.kind == WorkflowTemporaryOverrideKind::BreakGlass
                && override_entry.confirmation_pending
        });
        if !self.focus_session_active_for_current_state() {
            if break_glass_expires_at_epoch_secs.is_some() {
                push_ignored_artifact(ignored_runtime_artifacts, "break-glass override timer");
            }
            if break_glass_confirmation_pending {
                push_ignored_artifact(ignored_runtime_artifacts, "break-glass confirmation");
            }
            return;
        }

        if let Some(expires_at_epoch_secs) = break_glass_expires_at_epoch_secs
            && let Some(expires_at) = local_datetime_from_epoch_secs(expires_at_epoch_secs)
            && expires_at > self.current_frame_now
        {
            let remaining = expires_at
                .signed_duration_since(self.current_frame_now)
                .to_std()
                .ok();
            self.break_glass_expires_at = remaining.map(|remaining| Instant::now() + remaining);
        } else if break_glass_expires_at_epoch_secs.is_some() {
            push_ignored_artifact(ignored_runtime_artifacts, "break-glass override timer");
        }

        if break_glass_confirmation_pending && self.break_glass_expires_at.is_none() {
            self.pending_timer_action = Some(PendingTimerAction::BreakGlassOverride);
        } else if break_glass_confirmation_pending {
            push_ignored_artifact(ignored_runtime_artifacts, "break-glass confirmation");
        }
    }

    fn restore_strict_reset_runtime_state(
        &mut self,
        strict_reset_confirmation_pending: bool,
        ignored_runtime_artifacts: &mut Vec<&'static str>,
    ) {
        if !strict_reset_confirmation_pending {
            return;
        }

        if self.strict_mode_enforced_for_focus() && self.pending_timer_action.is_none() {
            self.pending_timer_action = Some(PendingTimerAction::Reset);
        } else {
            push_ignored_artifact(ignored_runtime_artifacts, "strict reset confirmation");
        }
    }

    fn restore_temporary_allowlist_runtime_state(
        &mut self,
        temporary_overrides: &[WorkflowTemporaryOverrideSnapshot],
        ignored_runtime_artifacts: &mut Vec<&'static str>,
    ) {
        let mut restored = Vec::new();
        let mut ignored_count = 0usize;
        for entry in temporary_overrides
            .iter()
            .filter(|entry| entry.kind == WorkflowTemporaryOverrideKind::AllowlistSite)
        {
            let profile = entry
                .profile
                .as_deref()
                .unwrap_or_default()
                .trim()
                .to_string();
            let site = entry.site.as_deref().unwrap_or_default().trim().to_string();
            let expires_at_epoch_secs = entry.expires_at_epoch_secs.unwrap_or_default();
            if profile.is_empty()
                || site.is_empty()
                || expires_at_epoch_secs <= self.current_frame_now.timestamp()
            {
                ignored_count += 1;
                continue;
            }
            restored.push(TemporaryAllowlistEntry {
                profile,
                site,
                expires_at_epoch_secs,
            });
        }
        self.temporary_allowlist_entries = restored;
        if ignored_count > 0 {
            push_ignored_artifact(ignored_runtime_artifacts, "temporary allowlist entries");
        }
    }

    fn sync_restored_cli_workflow_state(&mut self) {
        if let Err(error) = self.sync_cli_workflow_state() {
            self.config_error = Some(error);
        }
    }

    fn append_ignored_runtime_artifact_notice(&mut self, ignored_runtime_artifacts: &[&str]) {
        if ignored_runtime_artifacts.is_empty() {
            return;
        }

        let notice = format!(
            "Ignored saved runtime artifacts: {}.",
            ignored_runtime_artifacts.join(", ")
        );
        if let Some(existing_notice) = self.phase_notification.as_mut() {
            existing_notice.push(' ');
            existing_notice.push_str(&notice);
        } else {
            self.phase_notification = Some(notice);
        }
    }

    fn try_apply_recovery_snapshot(
        &mut self,
        snapshot: InProgressSessionSnapshot,
    ) -> Result<InProgressSessionSnapshot, String> {
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
        snapshot.validate_for_timer(&recovered_timer)?;
        let reconciled_snapshot = snapshot.reconcile_elapsed_for_timer(&recovered_timer);
        recovered_timer.phase = reconciled_snapshot.phase();
        recovered_timer.status = reconciled_snapshot.status();
        recovered_timer.remaining_secs = reconciled_snapshot.remaining_secs;
        recovered_timer.pomodoros_completed = reconciled_snapshot.pomodoros_completed;

        let task_label = reconciled_snapshot
            .normalized_task_label()
            .ok_or_else(|| "saved task label is missing or invalid".to_string())?;
        let selected_task_label =
            if let Some(existing_index) = task_label_index(&self.task_labels, &task_label) {
                self.task_labels[existing_index].clone()
            } else {
                self.task_labels.push(task_label.clone());
                task_label
            };

        self.selected_profile = reconciled_snapshot.selected_profile;
        self.profile_selection_index = profile_index(reconciled_snapshot.selected_profile);
        self.load_automation_runtime_for_profile(reconciled_snapshot.selected_profile);
        self.current_frame_now = Local::now();
        self.timer = recovered_timer;
        self.selected_task_label = Some(selected_task_label);
        self.pending_timer_action = None;
        self.break_glass_expires_at = None;
        let focus_active =
            self.timer.phase == TimerPhase::Focus && self.timer.status != TimerStatus::Idle;
        self.active_focus_task_label = if focus_active {
            self.selected_task_label.clone()
        } else {
            None
        };
        self.active_focus_intention = if focus_active {
            reconciled_snapshot.normalized_focus_intention()
        } else {
            None
        };
        self.active_focus_task_note = if focus_active {
            reconciled_snapshot.normalized_task_note()
        } else {
            None
        };
        self.active_focus_profile = if focus_active {
            Some(self.selected_profile)
        } else {
            None
        };
        self.sync_task_planner_state();
        Ok(reconciled_snapshot)
    }

    pub(super) fn sync_recovery_snapshot(&mut self) {
        let focus_active = self.focus_session_active_for_current_state();
        let recovery_task_label = if focus_active {
            self.active_focus_task_label
                .clone()
                .or_else(|| self.selected_task_label.clone())
        } else {
            self.selected_task_label.clone()
        };
        let recovery_focus_intention = if focus_active {
            self.active_focus_intention.clone()
        } else {
            None
        };
        let recovery_task_note = if focus_active {
            self.active_focus_task_note.clone()
        } else {
            None
        };

        let snapshot = InProgressSessionSnapshot::from_timer_state_with_metadata(
            &self.timer,
            recovery_task_label,
            recovery_focus_intention,
            recovery_task_note,
            self.selected_profile,
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
        prune_expired_temporary_allowlist_entries(
            &mut self.temporary_allowlist_entries,
            now.timestamp(),
        );
        let focus_active = self.focus_session_active_for_current_state();
        let active_occurrence_key = self
            .active_schedule_occurrence_at(now)
            .map(|occurrence| occurrence_key(&occurrence));
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

        let break_glass_confirmation_pending =
            self.break_glass_confirmation_pending() && focus_active;
        let strict_reset_confirmation_pending =
            self.strict_reset_confirmation_pending() && self.strict_mode_enforced_for_focus();
        let temporary_allowlist_entries = self
            .temporary_allowlist_entries
            .iter()
            .filter(|entry| {
                !entry.profile.trim().is_empty()
                    && !entry.site.trim().is_empty()
                    && entry.expires_at_epoch_secs > now.timestamp()
            })
            .map(|entry| WorkflowTemporaryAllowlistEntrySnapshot {
                profile: entry.profile.clone(),
                site: entry.site.clone(),
                expires_at_epoch_secs: entry.expires_at_epoch_secs,
            })
            .collect::<Vec<_>>();
        let mut temporary_overrides = Vec::new();
        if let Some(expires_at_epoch_secs) = break_glass_expires_at_epoch_secs {
            temporary_overrides.push(WorkflowTemporaryOverrideSnapshot::break_glass_active(
                expires_at_epoch_secs,
            ));
        }
        if break_glass_confirmation_pending {
            temporary_overrides
                .push(WorkflowTemporaryOverrideSnapshot::break_glass_pending_confirmation());
        }
        temporary_overrides.extend(temporary_allowlist_entries.iter().map(|entry| {
            WorkflowTemporaryOverrideSnapshot::temporary_allowlist(
                entry.profile.clone(),
                entry.site.clone(),
                entry.expires_at_epoch_secs,
            )
        }));
        let schedule_armed_occurrence_key = if !focus_active {
            self.schedule_armed_occurrence_key
                .clone()
                .filter(|armed_key| active_occurrence_key.as_deref() == Some(armed_key.as_str()))
        } else {
            None
        };
        let delayed_occurrence_key = schedule_state
            .as_ref()
            .and_then(|(delayed_key, _)| delayed_key.as_deref());
        let last_schedule_occurrence_key =
            self.last_schedule_occurrence_key
                .clone()
                .filter(|last_key| {
                    active_occurrence_key.as_deref() == Some(last_key.as_str())
                        && delayed_occurrence_key != Some(last_key.as_str())
                });
        let snapshot = WorkflowStateSnapshot {
            schedule_delayed_occurrence_key: schedule_state
                .as_ref()
                .and_then(|(delayed_key, _)| delayed_key.clone()),
            schedule_delay_until_epoch_secs: schedule_state
                .as_ref()
                .and_then(|(_, delayed_until)| *delayed_until),
            schedule_armed_occurrence_key,
            last_schedule_occurrence_key,
            break_glass_expires_at_epoch_secs,
            break_glass_confirmation_pending,
            strict_reset_confirmation_pending,
            temporary_allowlist_entries,
            temporary_overrides,
        };

        let should_persist = snapshot.schedule_delayed_occurrence_key.is_some()
            || snapshot.schedule_armed_occurrence_key.is_some()
            || snapshot.last_schedule_occurrence_key.is_some()
            || snapshot.break_glass_expires_at_epoch_secs.is_some()
            || snapshot.break_glass_confirmation_pending
            || snapshot.strict_reset_confirmation_pending
            || !snapshot.temporary_overrides.is_empty()
            || !snapshot.temporary_allowlist_entries.is_empty();
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
        let mut profile_automation = self.profile_automation.clone();
        profile_automation
            .set_for_profile(self.selected_profile, self.selected_profile_automation());
        let (backend_policy, command_backend) = self.blocker.backend_config();
        let blocking_backend =
            blocking_backend_config_for_persistence(backend_policy, &command_backend);
        AppConfig {
            // Keep legacy fields aligned with the editable custom profile so
            // older releases retain user-configured values.
            focus_secs: custom_profile.focus_secs,
            short_break_secs: custom_profile.short_break_secs,
            long_break_secs: custom_profile.long_break_secs,
            long_break_interval: custom_profile.long_break_interval,
            blocked_sites: Vec::new(),
            blocklist_profiles,
            selected_blocklist_profile,
            blocking_backend,
            selected_profile: self.selected_profile,
            custom_profile: Some(custom_profile),
            session_templates: self.session_templates.clone(),
            selected_session_template: self.selected_session_template_for_persistence(),
            selected_theme_preset: self.selected_theme_preset,
            notifications: self.notification_settings,
            auto_start: self.auto_start,
            recurring_schedule: self.recurring_schedule.clone(),
            schedule_runtime: self.schedule_runtime,
            calendar_sync: self.calendar_sync.clone(),
            profile_automation: Some(profile_automation),
            strict_mode: self.strict_mode,
            break_glass_duration_secs: self.break_glass_duration_secs,
            daily_goal: self.daily_goal,
            weekly_goal: self.weekly_goal,
            monthly_goal: self.monthly_goal,
            goal_carry_over: self.goal_carry_over,
            stats_retention: self.stats_retention,
            history_dashboard: HistoryDashboardConfig::default(),
            wakatime: self.wakatime_metadata.clone(),
            wakatime_runtime: self.wakatime_runtime.clone(),
            feature_flags: self.feature_flags.clone(),
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
        if let Err(e) = self
            .stats
            .save_with_options(crate::stats::StatsSaveOptions::default())
        {
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
            self.mark_stats_dirty();
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

fn push_ignored_artifact(ignored_artifacts: &mut Vec<&'static str>, artifact: &'static str) {
    if !ignored_artifacts.contains(&artifact) {
        ignored_artifacts.push(artifact);
    }
}
