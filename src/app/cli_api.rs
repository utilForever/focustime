use crate::app::{
    App, BlockingPreview, ProfileId, SessionInterruptionReason, TimerPhase, TimerState,
    TimerStatus, normalize_task_label, task_label_index,
};

impl App {
    pub fn start_focus_for_cli(&mut self) -> Result<(), String> {
        if self.timer.phase != TimerPhase::Focus || self.timer.status != TimerStatus::Idle {
            return Err("Cannot start focus: timer is not idle in focus phase.".to_string());
        }
        if !self.has_selectable_task_label_for_focus() {
            return Err(
                "Cannot start focus: select a task label first (run TUI and press [t])."
                    .to_string(),
            );
        }
        self.update_timer_and_sync(TimerState::toggle_pause);
        Ok(())
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

    pub fn selected_task_label_for_cli(&self) -> Option<String> {
        self.selected_task_label.clone()
    }

    pub fn timer_state_for_cli(&self) -> (TimerPhase, TimerStatus, u64, u32) {
        (
            self.timer.phase,
            self.timer.status,
            self.timer.remaining_secs,
            self.timer.pomodoros_completed,
        )
    }
}
