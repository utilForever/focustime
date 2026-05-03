use crate::app::{
    App, Duration, Instant, Local, PendingTimerAction, ShortcutAction, ceil_duration_secs,
    format_duration_label,
};

impl App {
    pub fn break_glass_confirmation_pending(&self) -> bool {
        self.pending_timer_action == Some(PendingTimerAction::BreakGlassOverride)
    }

    pub fn break_glass_override_remaining_secs(&self) -> Option<u64> {
        if !self.focus_session_active_for_current_state() {
            return None;
        }
        self.break_glass_override_remaining_duration()
            .map(ceil_duration_secs)
    }

    pub fn break_glass_override_active(&self) -> bool {
        self.break_glass_override_remaining_secs().is_some()
    }

    pub(super) fn break_glass_override_active_now(&self) -> bool {
        self.break_glass_override_remaining_duration().is_some()
    }

    fn break_glass_override_remaining_duration(&self) -> Option<Duration> {
        if !self.focus_session_active_for_current_state() {
            return None;
        }
        self.break_glass_expires_at
            .and_then(|deadline| deadline.checked_duration_since(Instant::now()))
            .filter(|remaining| !remaining.is_zero())
    }

    pub(super) fn handle_break_glass_key(&mut self) {
        if !self.focus_session_active_for_current_state() {
            self.phase_notification =
                Some("Break-glass override is available only during active focus.".to_string());
            return;
        }
        if self.blocker.sites.is_empty() {
            self.phase_notification = Some(
                "Break-glass override unavailable: active profile has no effective blocked sites."
                    .to_string(),
            );
            return;
        }
        if let Some(remaining_secs) = self.break_glass_override_remaining_secs() {
            self.phase_notification = Some(format!(
                "Break-glass override already active ({} remaining).",
                format_duration_label(remaining_secs)
            ));
            return;
        }

        self.pending_timer_action = Some(PendingTimerAction::BreakGlassOverride);
        self.phase_notification = Some(format!(
            "Confirm break-glass with {} to unblock for {}.",
            self.shortcut_hint(ShortcutAction::BreakGlassOverride),
            format_duration_label(self.break_glass_duration_secs)
        ));
    }

    pub(super) fn confirm_break_glass_override(&mut self) {
        self.pending_timer_action = None;
        if !self.focus_session_active_for_current_state() {
            self.phase_notification =
                Some("Break-glass override is available only during active focus.".to_string());
            return;
        }
        if self.blocker.sites.is_empty() {
            self.phase_notification = Some(
                "Break-glass override unavailable: active profile has no effective blocked sites."
                    .to_string(),
            );
            return;
        }

        match self.blocker.unblock() {
            Ok(()) => {
                self.block_error = None;
                self.break_glass_expires_at =
                    Some(Instant::now() + Duration::from_secs(self.break_glass_duration_secs));
                self.record_break_glass_override_event();
                self.phase_notification = Some(format!(
                    "Break-glass active: blocking paused for {}.",
                    format_duration_label(self.break_glass_duration_secs)
                ));
            }
            Err(err) => {
                self.break_glass_expires_at = None;
                self.block_error = Some(err.to_string());
                self.phase_notification = Some(format!(
                    "Break-glass failed: could not unblock sites ({err})"
                ));
            }
        }
        self.sync_wakatime_tracking_for_state();
    }

    fn record_break_glass_override_event(&mut self) {
        let now = Local::now();
        let day_key = now.format("%Y-%m-%d").to_string();
        let epoch_secs = now.timestamp().max(0) as u64;
        let task_label = self
            .active_focus_task_label
            .as_deref()
            .or(self.selected_task_label.as_deref());
        self.stats.record_break_glass_override_event(
            &day_key,
            epoch_secs,
            task_label,
            self.break_glass_duration_secs,
        );
        self.stats_dirty = true;
        self.flush_stats_if_dirty(false);
    }

    pub(super) fn sync_break_glass_override(&mut self) {
        if !self.focus_session_active_for_current_state() {
            self.break_glass_expires_at = None;
            return;
        }
        if self.break_glass_expires_at.is_none() || self.break_glass_override_active_now() {
            return;
        }

        self.break_glass_expires_at = None;
        self.phase_notification =
            Some("Break-glass override expired. Blocking resumed.".to_string());
        self.apply_blocking_for_phase();
    }
}
