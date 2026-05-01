use crate::app::*;

impl App {
    pub(super) fn handle_key_timer(&mut self, key: KeyEvent) {
        if self.timer_note_input_active {
            self.handle_timer_note_input_key(key);
            return;
        }

        if self.handle_quit_key(&key, true) {
            return;
        }

        if self.handle_pending_timer_confirmations(key.code) {
            return;
        }

        match key.code {
            KeyCode::Char(' ') => self.handle_timer_toggle_pause_key(),
            KeyCode::Char('s') => self.handle_timer_stop_reset_key(),
            KeyCode::Char('n') => self.handle_timer_next_phase_key(),
            KeyCode::Char('b') => {
                self.open_site_manager();
            }
            KeyCode::Char('p') => {
                if self.strict_mode_enforced_for_focus() {
                    return;
                }
                self.open_profile_manager();
            }
            KeyCode::Char('t') => {
                self.open_session_planner();
            }
            KeyCode::Char('h') => {
                self.open_stats_history();
            }
            KeyCode::Char('d') => {
                self.open_setup_diagnostics();
            }
            KeyCode::Char('m') => {
                self.start_timer_note_input();
            }
            KeyCode::Char('u') => {
                self.handle_break_glass_key();
            }
            KeyCode::Char('z') => {
                self.delay_active_schedule_start();
            }
            _ => {}
        }
    }

    fn handle_pending_timer_confirmations(&mut self, key_code: KeyCode) -> bool {
        if self.strict_reset_confirmation_pending() {
            if key_code == KeyCode::Char('s') {
                self.pending_timer_action = None;
                self.update_timer_and_sync_with_reason(
                    TimerState::reset,
                    Some(SessionInterruptionReason::ManualStop),
                );
                return true;
            }
            self.pending_timer_action = None;
        }

        if self.break_glass_confirmation_pending() {
            if key_code == KeyCode::Char('u') {
                self.confirm_break_glass_override();
                return true;
            }
            self.pending_timer_action = None;
        }

        false
    }

    fn handle_timer_toggle_pause_key(&mut self) {
        if self.timer.phase == TimerPhase::Focus
            && self.timer.status == TimerStatus::Idle
            && !self.has_selectable_task_label_for_focus()
        {
            self.phase_notification =
                Some("Select a task label with [t] before starting focus.".to_string());
            return;
        }
        self.update_timer_and_sync(TimerState::toggle_pause);
    }

    fn handle_timer_stop_reset_key(&mut self) {
        if self.strict_mode_enforced_for_focus() {
            self.pending_timer_action = Some(PendingTimerAction::Reset);
            return;
        }
        self.update_timer_and_sync_with_reason(
            TimerState::reset,
            Some(SessionInterruptionReason::ManualStop),
        );
    }

    fn handle_timer_next_phase_key(&mut self) {
        if self.strict_mode_enforced_for_focus() {
            return;
        }
        self.update_timer_and_sync_with_reason(
            TimerState::next_phase,
            Some(SessionInterruptionReason::ManualSkip),
        );
    }

    fn start_timer_note_input(&mut self) {
        if !self.focus_session_active_for_current_state() {
            self.phase_notification = Some(
                "Mid-session notes are available only during active or paused focus.".to_string(),
            );
            return;
        }

        self.timer_note_input = self
            .current_task_note()
            .map(str::to_string)
            .unwrap_or_default();
        self.timer_note_input_active = true;
        self.phase_notification =
            Some("Editing session note: type text, then press [Enter] to save.".to_string());
    }

    fn handle_timer_note_input_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => self.commit_timer_note_input(),
            KeyCode::Esc => {
                self.clear_timer_note_input();
                self.phase_notification = Some("Session note edit canceled.".to_string());
            }
            KeyCode::Backspace => {
                self.timer_note_input.pop();
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.timer_note_input.push(c);
            }
            _ => {}
        }
    }

    fn commit_timer_note_input(&mut self) {
        if !self.focus_session_active_for_current_state() {
            self.clear_timer_note_input();
            self.phase_notification =
                Some("Session note save failed: focus is no longer active.".to_string());
            return;
        }

        let note = if self.timer_note_input.trim().is_empty() {
            self.active_focus_task_label
                .clone()
                .or_else(|| self.selected_task_label.clone())
        } else {
            Some(self.timer_note_input.trim().to_string())
        };

        self.clear_timer_note_input();
        if let Some(note) = note {
            self.active_focus_task_note = Some(note);
            self.sync_recovery_snapshot();
            self.phase_notification = Some("Session note updated.".to_string());
        } else {
            self.phase_notification =
                Some("Session note save failed: no task selected.".to_string());
        }
    }

    pub(super) fn clear_timer_note_input(&mut self) {
        self.timer_note_input.clear();
        self.timer_note_input_active = false;
    }

    fn delay_active_schedule_start(&mut self) {
        if self.focus_session_active_for_current_state() {
            self.phase_notification = Some(
                "Schedule delay is unavailable while a focus session is already active."
                    .to_string(),
            );
            return;
        }

        let now = self.current_frame_now;
        let Some(active_window) = self.active_schedule_occurrence_at(now) else {
            self.phase_notification = Some("No active schedule window to delay.".to_string());
            return;
        };
        let active_occurrence_key = occurrence_key(&active_window);
        let delayed_from = match (
            self.schedule_delayed_occurrence_key.as_deref(),
            self.schedule_delay_until,
        ) {
            (Some(existing_key), Some(existing_until))
                if existing_key == active_occurrence_key && existing_until > now =>
            {
                existing_until
            }
            _ => now,
        };
        let delayed_until = delayed_from + chrono::Duration::seconds(SCHEDULE_DELAY_SECS as i64);

        self.schedule_armed_occurrence_key = None;
        self.schedule_delayed_occurrence_key = Some(active_occurrence_key);
        self.schedule_delay_until = Some(delayed_until);
        self.last_schedule_occurrence_key = None;
        self.phase_notification = Some(format!(
            "Scheduled start delayed for {} (until {}).",
            format_duration_label(SCHEDULE_DELAY_SECS),
            delayed_until.format("%H:%M")
        ));
    }

    pub(super) fn handle_key_stats_history(&mut self, key: KeyEvent) {
        if self.handle_quit_key(&key, false) {
            return;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('h') => {
                self.mode = AppMode::Timer;
            }
            KeyCode::Char('e') => {
                self.export_stats_history();
            }
            _ => {}
        }
    }

    pub(super) fn handle_key_setup_diagnostics(&mut self, key: KeyEvent) {
        if self.handle_quit_key(&key, false) {
            return;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('d') => {
                self.mode = AppMode::Timer;
            }
            KeyCode::Char('r') => {
                self.refresh_setup_diagnostics();
            }
            _ => {}
        }
    }
}
