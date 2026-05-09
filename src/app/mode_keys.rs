use crate::app::{
    App, AppMode, KeyCode, KeyEvent, KeyModifiers, PendingTimerAction, SCHEDULE_DELAY_SECS,
    SessionInterruptionReason, ShortcutAction, TimerPhase, TimerState, TimerStatus,
    format_duration_label,
};

const TIMER_SHORTCUT_ACTIONS: [ShortcutAction; 11] = [
    ShortcutAction::TimerTogglePause,
    ShortcutAction::TimerStopReset,
    ShortcutAction::TimerNextPhase,
    ShortcutAction::OpenSiteManager,
    ShortcutAction::OpenProfileManager,
    ShortcutAction::OpenSessionPlanner,
    ShortcutAction::OpenStatsHistory,
    ShortcutAction::OpenSetupDiagnostics,
    ShortcutAction::TimerEditNote,
    ShortcutAction::BreakGlassOverride,
    ShortcutAction::DelayScheduleStart,
];

impl App {
    pub(super) fn handle_key_timer(&mut self, key: KeyEvent) {
        if self.timer_note_input_active {
            self.handle_timer_note_input_key(key);
            return;
        }

        if self.handle_quit_key(&key, true) {
            return;
        }

        if self.handle_pending_timer_confirmations(&key) {
            return;
        }

        if let Some(action) = self.timer_shortcut_action(&key) {
            self.execute_timer_shortcut_action(action);
        }
    }

    fn timer_shortcut_action(&self, key: &KeyEvent) -> Option<ShortcutAction> {
        TIMER_SHORTCUT_ACTIONS
            .into_iter()
            .find(|action| self.shortcut_matches(*action, key))
    }

    fn execute_timer_shortcut_action(&mut self, action: ShortcutAction) {
        match action {
            ShortcutAction::TimerTogglePause => self.handle_timer_toggle_pause_key(),
            ShortcutAction::TimerStopReset => self.handle_timer_stop_reset_key(),
            ShortcutAction::TimerNextPhase => self.handle_timer_next_phase_key(),
            ShortcutAction::OpenSiteManager => self.open_site_manager(),
            ShortcutAction::OpenProfileManager => {
                if self.strict_mode_enforced_for_focus() {
                    return;
                }
                self.open_profile_manager();
            }
            ShortcutAction::OpenSessionPlanner => self.open_session_planner(),
            ShortcutAction::OpenStatsHistory => self.open_stats_history(),
            ShortcutAction::OpenSetupDiagnostics => self.open_setup_diagnostics(),
            ShortcutAction::TimerEditNote => self.start_timer_note_input(),
            ShortcutAction::BreakGlassOverride => self.handle_break_glass_key(),
            ShortcutAction::DelayScheduleStart => self.delay_active_schedule_start(),
            _ => {}
        }
    }

    fn handle_pending_timer_confirmations(&mut self, key: &KeyEvent) -> bool {
        if self.strict_reset_confirmation_pending() {
            if self.shortcut_matches(ShortcutAction::TimerStopReset, key) {
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
            if self.shortcut_matches(ShortcutAction::BreakGlassOverride, key) {
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
            self.phase_notification = Some(format!(
                "Select a task label with {} before starting focus.",
                self.shortcut_hint(ShortcutAction::OpenSessionPlanner)
            ));
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
        let now = self.current_frame_now;
        match self.delay_active_schedule_start_for_workflow(now) {
            Ok(delayed_until) => {
                self.phase_notification = Some(format!(
                    "Scheduled start delayed for {} (until {}).",
                    format_duration_label(SCHEDULE_DELAY_SECS),
                    delayed_until.format("%H:%M")
                ));
            }
            Err(message) => {
                self.phase_notification = Some(message);
            }
        }
    }

    pub(super) fn handle_key_stats_history(&mut self, key: KeyEvent) {
        if self.handle_quit_key(&key, false) {
            return;
        }

        if key.code == KeyCode::Esc || self.shortcut_matches(ShortcutAction::BackStatsHistory, &key)
        {
            self.mode = AppMode::Timer;
        } else if self.shortcut_matches(ShortcutAction::ExportStatsHistory, &key) {
            self.export_stats_history();
        }
    }

    pub(super) fn handle_key_setup_diagnostics(&mut self, key: KeyEvent) {
        if self.handle_quit_key(&key, false) {
            return;
        }

        if key.code == KeyCode::Esc
            || self.shortcut_matches(ShortcutAction::BackSetupDiagnostics, &key)
        {
            self.mode = AppMode::Timer;
        } else if self.shortcut_matches(ShortcutAction::RefreshSetupDiagnostics, &key) {
            self.refresh_setup_diagnostics();
        }
    }
}
