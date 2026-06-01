use crate::app::{
    App, BlockingPreview, FocusStartOutcome, FocusStartTemplateMode, Local, ProfileId,
    SessionInterruptionReason, ShortcutAction, TimerPhase, TimerState, TimerStatus,
    normalize_task_label, task_label_index,
};

impl App {
    pub fn record_command_usage_for_cli(&mut self, surface_id: &str) {
        if self.stats.record_command_usage(surface_id) {
            self.mark_stats_dirty();
        }
    }

    pub fn start_focus_for_cli(&mut self) -> Result<(), String> {
        match self.try_start_focus_session(FocusStartTemplateMode::ApplySelectedTemplate)? {
            FocusStartOutcome::Started => Ok(()),
            FocusStartOutcome::MissingTaskLabel => Err(format!(
                "Cannot start focus: select a task label first (run TUI and press {}).",
                self.shortcut_hint(ShortcutAction::OpenSessionPlanner)
            )),
            FocusStartOutcome::NotIdleFocusPhase => {
                Err("Cannot start focus: timer is not idle in focus phase.".to_string())
            }
        }
    }

    pub fn pause_for_cli(&mut self) -> Result<(), String> {
        if self.timer.status != TimerStatus::Running {
            return Err("Cannot pause: timer is not running.".to_string());
        }
        self.update_timer_and_sync(TimerState::toggle_pause);
        Ok(())
    }

    pub fn resume_for_cli(&mut self) -> Result<(), String> {
        if self.timer.status != TimerStatus::Paused {
            return Err("Cannot resume: timer is not paused.".to_string());
        }
        self.update_timer_and_sync(TimerState::toggle_pause);
        Ok(())
    }

    pub fn stop_for_cli(&mut self) -> Result<(), String> {
        if self.strict_mode_enforced_for_focus() {
            return Err("Cannot stop: strict mode is active during focus.".to_string());
        }
        if self.timer.status == TimerStatus::Idle {
            return Err("Cannot stop: timer is already idle.".to_string());
        }
        self.update_timer_and_sync_with_reason(
            TimerState::reset,
            Some(SessionInterruptionReason::ManualStop),
        );
        Ok(())
    }

    pub fn next_phase_for_cli(&mut self) -> Result<(), String> {
        if self.strict_mode_enforced_for_focus() {
            return Err(
                "Cannot skip to next phase: strict mode is active during focus.".to_string(),
            );
        }
        self.update_timer_and_sync_with_reason(
            TimerState::next_phase,
            Some(SessionInterruptionReason::ManualSkip),
        );
        Ok(())
    }

    pub fn schedule_delay_for_cli(&mut self) -> Result<String, String> {
        let now = Local::now();
        self.current_frame_now = now;
        let delayed_until = self.delay_active_schedule_start_for_workflow(now)?;
        self.sync_cli_workflow_state()?;
        Ok(delayed_until.format("%H:%M").to_string())
    }

    pub fn trigger_break_glass_for_cli(&mut self) -> Result<(), String> {
        if self.break_glass_confirmation_pending() {
            let result = self.confirm_break_glass_override_for_workflow();
            self.sync_wakatime_tracking_for_state();
            result?;
            self.sync_cli_workflow_state()?;
            return Ok(());
        }
        self.arm_break_glass_override_for_workflow()?;
        self.sync_cli_workflow_state()
    }

    pub fn cancel_break_glass_for_cli(&mut self) -> Result<(), String> {
        if !self.break_glass_confirmation_pending() {
            return Err("Cannot cancel break-glass: no confirmation is pending.".to_string());
        }
        self.pending_timer_action = None;
        self.sync_cli_workflow_state()
    }

    pub fn add_temporary_allowlist_for_cli(
        &mut self,
        input: &str,
    ) -> Result<(usize, usize), String> {
        self.add_temporary_allowlist_entries_for_active_profile_from_input(input)
    }

    pub fn blocking_preview_for_cli(&self) -> Result<BlockingPreview, String> {
        self.compute_blocking_preview()
            .map_err(|error| format!("Failed to generate blocking preview: {error}"))
    }

    pub fn select_task_label_for_cli(&mut self, label: &str) -> Result<bool, String> {
        let Some(label) = normalize_task_label(label) else {
            return Err("Cannot select task label: label cannot be empty.".to_string());
        };

        if let Some(existing_index) = task_label_index(&self.task_labels, &label) {
            let Some(existing_label) = self.task_labels.get(existing_index).cloned() else {
                return Err("Cannot select task label: label lookup failed.".to_string());
            };
            if self.is_task_label_archived(&existing_label) {
                return Err(format!(
                    "Cannot select archived task label `{existing_label}`. Unarchive it in Session Planner first."
                ));
            }
            self.planner_selection_index = existing_index;
            self.selected_task_label = Some(existing_label.clone());
            if let Some(display_index) = self.planner_display_index_for_label(&existing_label) {
                self.planner_selection_index = display_index;
            }
            self.sync_task_planner_state();
            self.sync_recovery_snapshot();
            return Ok(false);
        }

        self.task_labels.push(label.clone());
        self.selected_task_label = Some(label);
        self.sync_planner_selection_to_selected_label();
        self.sync_task_planner_state();
        self.sync_recovery_snapshot();
        Ok(true)
    }

    pub fn selected_profile_id(&self) -> ProfileId {
        self.selected_profile
    }

    pub fn selected_blocklist_profile_name_for_cli(&self) -> String {
        self.active_blocklist_profile_name().to_string()
    }

    pub fn selected_task_label_for_cli(&self) -> Option<String> {
        self.selected_task_label.clone()
    }

    pub fn focus_intention_for_cli(&self) -> Option<String> {
        if self.focus_session_active_for_current_state() {
            return self.active_focus_intention.clone();
        }
        None
    }

    pub fn task_note_for_cli(&self) -> Option<String> {
        if self.focus_session_active_for_current_state() {
            return self.active_focus_task_note.clone();
        }
        None
    }

    pub fn set_focus_intention_for_cli(&mut self, value: &str) -> Result<(), String> {
        self.ensure_focus_active_for_cli_metadata_update("--focus-intention")?;
        let value = self.resolve_cli_metadata_value(value)?;
        self.active_focus_intention = Some(value);
        self.sync_recovery_snapshot();
        Ok(())
    }

    pub fn set_task_note_for_cli(&mut self, value: &str) -> Result<(), String> {
        self.ensure_focus_active_for_cli_metadata_update("--task-note")?;
        let value = self.resolve_cli_metadata_value(value)?;
        self.active_focus_task_note = Some(value);
        self.sync_recovery_snapshot();
        Ok(())
    }

    pub fn timer_state_for_cli(&self) -> (TimerPhase, TimerStatus, u64, u32) {
        (
            self.timer.phase,
            self.timer.status,
            self.timer.remaining_secs,
            self.timer.pomodoros_completed,
        )
    }

    pub fn select_session_template_for_cli(&mut self, name: Option<&str>) -> Result<bool, String> {
        self.select_session_template(name)
    }

    pub fn apply_session_template_for_cli(&mut self, name: Option<&str>) -> Result<bool, String> {
        self.apply_session_template(name)
    }

    pub fn create_session_template_for_cli(&mut self, name: &str) -> Result<bool, String> {
        self.capture_session_template(name)
    }

    pub fn rename_active_session_template_for_cli(&mut self, name: &str) -> Result<bool, String> {
        self.rename_active_session_template(name)
    }

    pub fn delete_active_session_template_for_cli(&mut self) -> Result<bool, String> {
        self.delete_active_session_template()
    }

    fn ensure_focus_active_for_cli_metadata_update(&self, command: &str) -> Result<(), String> {
        if self.focus_session_active_for_current_state() {
            Ok(())
        } else {
            Err(format!(
                "Cannot set session metadata with `{command}`: focus session is not active or paused."
            ))
        }
    }

    fn resolve_cli_metadata_value(&self, value: &str) -> Result<String, String> {
        normalize_task_label(value)
            .or_else(|| self.active_focus_task_label.clone())
            .or_else(|| self.selected_task_label.clone())
            .ok_or_else(|| "Cannot set session metadata: no task label is selected.".to_string())
    }
}
