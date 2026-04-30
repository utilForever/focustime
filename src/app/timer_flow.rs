use super::*;

impl App {
    pub fn on_tick(&mut self, is_catchup: bool) {
        self.sync_today_goal_snapshot();
        let completed_phase = self.timer.phase;
        let completed_focus_secs = self.timer.focus_secs;
        if self.should_record_focus_elapsed(is_catchup) {
            self.record_focus_elapsed(1);
        }

        let phase_changed = self.timer.tick();
        if phase_changed {
            self.handle_phase_change(completed_phase, completed_focus_secs, is_catchup);
        }
        self.sync_recovery_snapshot();
        self.flush_stats_if_dirty(false);
    }

    fn should_record_focus_elapsed(&self, is_catchup: bool) -> bool {
        !is_catchup && self.focus_running_for_current_state()
    }

    fn should_record_completed_focus_session(
        &self,
        completed_phase: TimerPhase,
        is_catchup: bool,
    ) -> bool {
        !is_catchup && completed_phase == TimerPhase::Focus && self.timer.phase != TimerPhase::Focus
    }

    fn should_block_focus_autostart(&self) -> bool {
        self.timer.phase == TimerPhase::Focus && !self.has_selectable_task_label_for_focus()
    }

    fn handle_phase_change(
        &mut self,
        completed_phase: TimerPhase,
        completed_focus_secs: u64,
        is_catchup: bool,
    ) {
        self.pending_timer_action = None;
        if self.should_record_completed_focus_session(completed_phase, is_catchup) {
            self.record_completed_focus_session(completed_focus_secs);
            self.active_focus_task_label = None;
            self.active_focus_intention = None;
            self.active_focus_task_note = None;
            self.clear_timer_note_input();
            self.active_focus_profile = None;
        }

        let blocked_focus_autostart =
            self.apply_auto_start_after_phase_change(completed_phase, is_catchup);
        self.update_phase_notification_after_phase_change(
            completed_phase,
            is_catchup,
            blocked_focus_autostart,
        );

        if self.timer.phase != TimerPhase::Focus {
            self.active_focus_task_label = None;
            self.active_focus_intention = None;
            self.active_focus_task_note = None;
            self.clear_timer_note_input();
            self.active_focus_profile = None;
            self.break_glass_expires_at = None;
        }
        self.apply_blocking_for_phase();
    }

    fn apply_auto_start_after_phase_change(
        &mut self,
        completed_phase: TimerPhase,
        is_catchup: bool,
    ) -> bool {
        if is_catchup || !self.should_auto_start_transition(completed_phase, self.timer.phase) {
            return false;
        }

        if self.should_block_focus_autostart() {
            return true;
        }

        self.timer.status = TimerStatus::Running;
        if self.timer.phase == TimerPhase::Focus {
            self.active_focus_task_label = self.selected_task_label.clone();
            self.active_focus_intention = self.selected_task_label.clone();
            self.active_focus_task_note = self.selected_task_label.clone();
            self.active_focus_profile = Some(self.selected_profile);
        }
        false
    }

    fn update_phase_notification_after_phase_change(
        &mut self,
        completed_phase: TimerPhase,
        is_catchup: bool,
        blocked_focus_autostart: bool,
    ) {
        if !is_catchup {
            self.phase_notification = self
                .notifier
                .notify_phase_completion(completed_phase, self.timer.phase);
        }
        if blocked_focus_autostart {
            self.phase_notification =
                Some("Select a task label with [t] before starting focus.".to_string());
        }
    }

    pub(super) fn update_timer_and_sync(&mut self, action: fn(&mut TimerState)) {
        self.update_timer_and_sync_with_reason(action, None);
    }

    pub(super) fn update_timer_and_sync_with_reason(
        &mut self,
        action: fn(&mut TimerState),
        interruption_reason: Option<SessionInterruptionReason>,
    ) {
        let was_focus_active = self.focus_session_active_for_current_state();
        let interruption_context = interruption_reason
            .filter(|_| was_focus_active)
            .map(|reason| self.build_focus_interruption_context(reason));
        self.pending_timer_action = None;
        action(&mut self.timer);
        let is_focus_active = self.focus_session_active_for_current_state();
        if let Some(context) = interruption_context
            && !is_focus_active
        {
            self.record_session_interruption_event(context);
        }
        if !was_focus_active && is_focus_active {
            self.active_focus_task_label = self.selected_task_label.clone();
            self.active_focus_intention = self.selected_task_label.clone();
            self.active_focus_task_note = self.selected_task_label.clone();
            self.clear_timer_note_input();
            self.active_focus_profile = Some(self.selected_profile);
            self.schedule_armed_occurrence_key = None;
            self.clear_schedule_delay_state();
        } else if was_focus_active && !is_focus_active {
            self.active_focus_task_label = None;
            self.active_focus_intention = None;
            self.active_focus_task_note = None;
            self.clear_timer_note_input();
            self.active_focus_profile = None;
            self.break_glass_expires_at = None;
        }
        self.apply_blocking_for_phase();
        self.sync_recovery_snapshot();
    }

    pub(super) fn record_focus_elapsed(&mut self, elapsed_secs: u64) {
        if elapsed_secs == 0 {
            return;
        }

        let day_key = current_day_key();
        let goal_snapshot = self.current_goal_snapshot();
        let session_minutes_before = self.stats.session().focused_minutes();
        let today_minutes_before = self.stats.daily_for(&day_key).focused_minutes();

        self.stats
            .record_focus_elapsed(&day_key, elapsed_secs, goal_snapshot);
        self.stats_has_unsaved_elapsed = true;

        let session_minutes_after = self.stats.session().focused_minutes();
        let today_minutes_after = self.stats.daily_for(&day_key).focused_minutes();
        if session_minutes_before != session_minutes_after
            || today_minutes_before != today_minutes_after
        {
            self.stats_dirty = true;
        }
    }
}
