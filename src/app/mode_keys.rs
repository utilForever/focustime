use crate::app::{
    App, AppMode, FocusStartOutcome, KeyCode, KeyEvent, NavigationAction, PendingTimerAction,
    SessionInterruptionReason, ShortcutAction, TimerState,
};

const TIMER_SHORTCUT_ACTIONS: [ShortcutAction; 8] = [
    ShortcutAction::TimerTogglePause,
    ShortcutAction::TimerStopReset,
    ShortcutAction::TimerNextPhase,
    ShortcutAction::OpenSiteManager,
    ShortcutAction::OpenProfileManager,
    ShortcutAction::OpenTaskSetup,
    ShortcutAction::OpenStatsHistory,
    ShortcutAction::OpenSetupDiagnostics,
];

impl App {
    pub(super) fn handle_key_timer(&mut self, key: KeyEvent) {
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
            ShortcutAction::OpenTaskSetup => self.open_session_planner(),
            ShortcutAction::OpenStatsHistory => self.open_stats_history(),
            ShortcutAction::OpenSetupDiagnostics => self.open_setup_diagnostics(),
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

        false
    }

    fn handle_timer_toggle_pause_key(&mut self) {
        match self.try_start_focus_session() {
            Ok(FocusStartOutcome::Started) => {}
            Ok(FocusStartOutcome::MissingTaskLabel) => {
                self.phase_notification = Some(format!(
                    "Select a task with {} before starting focus.",
                    self.shortcut_hint(ShortcutAction::OpenTaskSetup)
                ));
            }
            Ok(FocusStartOutcome::NotIdleFocusPhase) => {
                self.update_timer_and_sync(TimerState::toggle_pause);
            }
            Err(error) => {
                self.phase_notification = Some(error);
            }
        }
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

    pub(super) fn handle_key_stats_history(&mut self, key: KeyEvent) {
        if self.handle_quit_key(&key, false) {
            return;
        }

        if self.navigation_matches(NavigationAction::Cancel, &key)
            || self.shortcut_matches(ShortcutAction::BackStatsHistory, &key)
        {
            self.set_mode(AppMode::Timer);
            return;
        }

        if self.shortcut_matches(ShortcutAction::ExportStatsHistory, &key) {
            self.export_stats_history();
            return;
        }

        if self.handle_key_stats_history_navigation_shortcuts(&key) {
            return;
        }

        self.handle_key_stats_history_filter_shortcuts(&key);
    }

    fn handle_key_stats_history_navigation_shortcuts(&mut self, key: &KeyEvent) -> bool {
        if self.navigation_matches(NavigationAction::MoveLeft, key) {
            self.cycle_history_comparison_dimension(false);
            return true;
        }
        if self.navigation_matches(NavigationAction::MoveRight, key) {
            self.cycle_history_comparison_dimension(true);
            return true;
        }
        if self.navigation_matches(NavigationAction::MoveUp, key) {
            self.cycle_history_task_filter(false);
            return true;
        }
        if self.navigation_matches(NavigationAction::MoveDown, key) {
            self.cycle_history_task_filter(true);
            return true;
        }
        false
    }

    fn handle_key_stats_history_filter_shortcuts(&mut self, key: &KeyEvent) {
        if key.code == KeyCode::Char('[') {
            self.cycle_history_profile_filter(false);
        } else if key.code == KeyCode::Char(']') {
            self.cycle_history_profile_filter(true);
        } else if key.code == KeyCode::Char(',') {
            self.cycle_history_time_of_day_filter(false);
        } else if key.code == KeyCode::Char('.') {
            self.cycle_history_time_of_day_filter(true);
        }
    }

    pub(super) fn handle_key_setup_diagnostics(&mut self, key: KeyEvent) {
        if self.handle_quit_key(&key, false) {
            return;
        }

        if self.navigation_matches(NavigationAction::Cancel, &key)
            || self.shortcut_matches(ShortcutAction::BackSetupDiagnostics, &key)
        {
            self.set_mode(AppMode::Timer);
        } else if self.shortcut_matches(ShortcutAction::RefreshSetupDiagnostics, &key) {
            self.refresh_setup_diagnostics();
        }
    }
}
