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

    pub(crate) fn selected_task_label_for_cli(&self) -> Option<String> {
        self.selected_task_label.clone()
    }

    pub(crate) fn timer_state_for_cli(&self) -> (TimerPhase, TimerStatus, u64, u32) {
        (
            self.timer.phase,
            self.timer.status,
            self.timer.remaining_secs,
            self.timer.pomodoros_completed,
        )
    }
}
