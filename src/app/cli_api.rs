use crate::app::{
    App, AppError, AppResult, BlockingPreview, FocusStartOutcome, ProfileId,
    SessionInterruptionReason, ShortcutAction, TimerPhase, TimerState, TimerStatus,
    normalize_task_label, task_label_index,
};

impl App {
    pub(crate) fn record_command_usage_for_cli(&mut self, surface_id: &str) {
        if self.stats.record_command_usage(surface_id) {
            self.mark_stats_dirty();
        }
    }

    pub(crate) fn start_focus_for_cli(&mut self) -> AppResult<()> {
        match self.try_start_focus_session().map_err(AppError::workflow)? {
            FocusStartOutcome::Started => Ok(()),
            FocusStartOutcome::MissingTaskLabel => Err(AppError::MissingTaskLabel {
                shortcut_hint: self.shortcut_hint(ShortcutAction::OpenTaskSetup),
            }),
            FocusStartOutcome::NotIdleFocusPhase => Err(AppError::TimerNotIdleFocusPhase),
        }
    }

    pub(crate) fn pause_for_cli(&mut self) -> AppResult<()> {
        if self.timer.status != TimerStatus::Running {
            return Err(AppError::TimerNotRunning { action: "pause" });
        }
        self.update_timer_and_sync(TimerState::toggle_pause);
        Ok(())
    }

    pub(crate) fn resume_for_cli(&mut self) -> AppResult<()> {
        if self.timer.status != TimerStatus::Paused {
            return Err(AppError::TimerNotPaused);
        }
        self.update_timer_and_sync(TimerState::toggle_pause);
        Ok(())
    }

    pub(crate) fn stop_for_cli(&mut self) -> AppResult<()> {
        if self.strict_mode_enforced_for_focus() {
            return Err(AppError::StrictModeActive { action: "stop" });
        }
        if self.timer.status == TimerStatus::Idle {
            return Err(AppError::TimerAlreadyIdle);
        }
        self.update_timer_and_sync_with_reason(
            TimerState::reset,
            Some(SessionInterruptionReason::ManualStop),
        );
        Ok(())
    }

    pub(crate) fn next_phase_for_cli(&mut self) -> AppResult<()> {
        if self.strict_mode_enforced_for_focus() {
            return Err(AppError::StrictModeActive {
                action: "skip to next phase",
            });
        }
        self.update_timer_and_sync_with_reason(
            TimerState::next_phase,
            Some(SessionInterruptionReason::ManualSkip),
        );
        Ok(())
    }

    pub(crate) fn trigger_break_glass_for_cli(&mut self) -> AppResult<()> {
        if self.break_glass_confirmation_pending() {
            let result = self.confirm_break_glass_override_for_workflow();
            self.sync_wakatime_tracking_for_state();
            result?;
            self.sync_cli_workflow_state()?;
            return Ok(());
        }
        self.arm_break_glass_override_for_workflow()?;
        self.sync_cli_workflow_state()?;
        Ok(())
    }

    pub(crate) fn cancel_break_glass_for_cli(&mut self) -> AppResult<()> {
        if !self.break_glass_confirmation_pending() {
            return Err(AppError::BreakGlassNoConfirmation);
        }
        self.pending_timer_action = None;
        self.sync_cli_workflow_state()?;
        Ok(())
    }

    pub(crate) fn add_temporary_allowlist_for_cli(
        &mut self,
        input: &str,
    ) -> AppResult<(usize, usize)> {
        self.add_temporary_allowlist_entries_for_active_profile_from_input(input)
            .map_err(AppError::workflow)
    }

    pub(crate) fn blocking_preview_for_cli(&self) -> AppResult<BlockingPreview> {
        self.compute_blocking_preview()
            .map_err(|error| AppError::BlockingPreviewFailed {
                source: error.to_string(),
            })
    }

    pub(crate) fn select_task_label_for_cli(&mut self, label: &str) -> AppResult<bool> {
        let Some(label) = normalize_task_label(label) else {
            return Err(AppError::TaskLabelEmpty);
        };

        if let Some(existing_index) = task_label_index(&self.task_labels, &label) {
            let Some(existing_label) = self.task_labels.get(existing_index).cloned() else {
                return Err(AppError::TaskLabelLookupFailed);
            };
            if self.is_task_label_archived(&existing_label) {
                return Err(AppError::ArchivedTaskLabel {
                    label: existing_label,
                });
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

    pub(crate) fn selected_profile_id(&self) -> ProfileId {
        self.selected_profile
    }

    pub(crate) fn selected_blocklist_profile_name_for_cli(&self) -> String {
        self.active_blocklist_profile_name().to_string()
    }

    pub(crate) fn selected_task_label_for_cli(&self) -> Option<String> {
        self.selected_task_label.clone()
    }

    pub(crate) fn task_note_for_cli(&self) -> Option<String> {
        if self.focus_session_active_for_current_state() {
            return self.active_focus_task_note.clone();
        }
        None
    }

    pub(crate) fn set_task_note_for_cli(&mut self, value: &str) -> AppResult<()> {
        self.ensure_focus_active_for_cli_metadata_update("--task-note")?;
        let value = self.resolve_cli_metadata_value(value)?;
        self.active_focus_task_note = Some(value);
        self.sync_recovery_snapshot();
        Ok(())
    }

    pub(crate) fn timer_state_for_cli(&self) -> (TimerPhase, TimerStatus, u64, u32) {
        (
            self.timer.phase,
            self.timer.status,
            self.timer.remaining_secs,
            self.timer.pomodoros_completed,
        )
    }

    fn ensure_focus_active_for_cli_metadata_update(&self, command: &'static str) -> AppResult<()> {
        if self.focus_session_active_for_current_state() {
            Ok(())
        } else {
            Err(AppError::SessionMetadataInactive { command })
        }
    }

    fn resolve_cli_metadata_value(&self, value: &str) -> AppResult<String> {
        normalize_task_label(value)
            .or_else(|| self.active_focus_task_label.clone())
            .or_else(|| self.selected_task_label.clone())
            .ok_or(AppError::SessionMetadataMissingTaskLabel)
    }
}
