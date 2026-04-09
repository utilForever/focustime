#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerPhase {
    Focus,
    ShortBreak,
    LongBreak,
}

pub const DEFAULT_FOCUS_SECS: u64 = 25 * 60;
pub const DEFAULT_SHORT_BREAK_SECS: u64 = 5 * 60;
pub const DEFAULT_LONG_BREAK_SECS: u64 = 15 * 60;
pub const DEFAULT_LONG_BREAK_INTERVAL: u32 = 4;

impl TimerPhase {
    pub fn label(self) -> &'static str {
        match self {
            TimerPhase::Focus => "Focus",
            TimerPhase::ShortBreak => "Short Break",
            TimerPhase::LongBreak => "Long Break",
        }
    }

    /// Duration of the phase in seconds.
    pub fn duration_secs(self) -> u64 {
        match self {
            TimerPhase::Focus => DEFAULT_FOCUS_SECS,
            TimerPhase::ShortBreak => DEFAULT_SHORT_BREAK_SECS,
            TimerPhase::LongBreak => DEFAULT_LONG_BREAK_SECS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerStatus {
    Idle,
    Running,
    Paused,
}

#[derive(Debug)]
pub struct TimerState {
    pub phase: TimerPhase,
    pub status: TimerStatus,
    /// Seconds remaining in the current phase.
    pub remaining_secs: u64,
    /// Number of completed focus sessions.
    pub pomodoros_completed: u32,
    /// Configured duration for the focus phase.
    pub focus_secs: u64,
    /// Configured duration for the short-break phase.
    pub short_break_secs: u64,
    /// Configured duration for the long-break phase.
    pub long_break_secs: u64,
    /// Number of completed focus sessions before a long break.
    pub long_break_interval: u32,
}

impl TimerState {
    pub fn new() -> Self {
        Self::with_profile(
            TimerPhase::Focus.duration_secs(),
            TimerPhase::ShortBreak.duration_secs(),
            TimerPhase::LongBreak.duration_secs(),
            DEFAULT_LONG_BREAK_INTERVAL,
        )
    }

    /// Create a timer with custom phase durations (all in seconds).
    #[cfg(test)]
    pub fn with_durations(focus_secs: u64, short_break_secs: u64, long_break_secs: u64) -> Self {
        Self::with_profile(
            focus_secs,
            short_break_secs,
            long_break_secs,
            DEFAULT_LONG_BREAK_INTERVAL,
        )
    }

    /// Create a timer with custom phase durations and long-break cadence.
    pub fn with_profile(
        focus_secs: u64,
        short_break_secs: u64,
        long_break_secs: u64,
        long_break_interval: u32,
    ) -> Self {
        let focus_secs = nonzero_or_default(focus_secs, DEFAULT_FOCUS_SECS);
        let short_break_secs = nonzero_or_default(short_break_secs, DEFAULT_SHORT_BREAK_SECS);
        let long_break_secs = nonzero_or_default(long_break_secs, DEFAULT_LONG_BREAK_SECS);
        let long_break_interval =
            nonzero_or_default_u32(long_break_interval, DEFAULT_LONG_BREAK_INTERVAL);
        let phase = TimerPhase::Focus;
        Self {
            phase,
            status: TimerStatus::Idle,
            remaining_secs: focus_secs,
            pomodoros_completed: 0,
            focus_secs,
            short_break_secs,
            long_break_secs,
            long_break_interval,
        }
    }

    /// Returns the configured duration for `phase`.
    fn phase_duration(&self, phase: TimerPhase) -> u64 {
        match phase {
            TimerPhase::Focus => self.focus_secs,
            TimerPhase::ShortBreak => self.short_break_secs,
            TimerPhase::LongBreak => self.long_break_secs,
        }
    }

    fn effective_long_break_interval(&self) -> u32 {
        nonzero_or_default_u32(self.long_break_interval, DEFAULT_LONG_BREAK_INTERVAL)
    }

    fn break_phase_for_focus_count(&self, focus_count: u32) -> TimerPhase {
        if focus_count.is_multiple_of(self.effective_long_break_interval()) {
            TimerPhase::LongBreak
        } else {
            TimerPhase::ShortBreak
        }
    }

    fn set_phase_idle(&mut self, phase: TimerPhase) {
        self.phase = phase;
        self.remaining_secs = self.phase_duration(self.phase);
        self.status = TimerStatus::Idle;
    }

    /// Advance the timer by one second. Returns true if the phase just ended.
    pub fn tick(&mut self) -> bool {
        if self.status != TimerStatus::Running {
            return false;
        }
        if self.remaining_secs > 0 {
            self.remaining_secs -= 1;
        }
        if self.remaining_secs == 0 {
            self.advance_phase();
            return true;
        }
        false
    }

    /// Move to the next phase automatically (called when timer reaches zero).
    fn advance_phase(&mut self) {
        let next_phase = match self.phase {
            TimerPhase::Focus => {
                self.pomodoros_completed = self.pomodoros_completed.saturating_add(1);
                self.break_phase_for_focus_count(self.pomodoros_completed)
            }
            TimerPhase::ShortBreak | TimerPhase::LongBreak => TimerPhase::Focus,
        };
        self.set_phase_idle(next_phase);
    }

    /// Skip to the next phase immediately (does not count as a completed session).
    pub fn next_phase(&mut self) {
        let next_phase = if self.phase == TimerPhase::Focus {
            let next_focus_count = self.pomodoros_completed.saturating_add(1);
            self.break_phase_for_focus_count(next_focus_count)
        } else {
            TimerPhase::Focus
        };
        self.set_phase_idle(next_phase);
    }

    /// Reset the current phase back to its full duration and stop.
    pub fn reset(&mut self) {
        self.set_phase_idle(self.phase);
    }

    pub fn toggle_pause(&mut self) {
        self.status = match self.status {
            TimerStatus::Idle | TimerStatus::Paused => TimerStatus::Running,
            TimerStatus::Running => TimerStatus::Paused,
        };
    }

    /// Progress as a value in [0.0, 1.0] where 1.0 means full time remaining.
    pub fn progress(&self) -> f64 {
        let total_secs = self.phase_duration(self.phase);
        if total_secs == 0 {
            return 0.0;
        }
        self.remaining_secs as f64 / total_secs as f64
    }
}

fn nonzero_or_default(value: u64, default: u64) -> u64 {
    if value == 0 { default } else { value }
}

fn nonzero_or_default_u32(value: u32, default: u32) -> u32 {
    if value == 0 { default } else { value }
}

impl Default for TimerState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_timer_is_idle_focus() {
        let t = TimerState::new();
        assert_eq!(t.phase, TimerPhase::Focus);
        assert_eq!(t.status, TimerStatus::Idle);
        assert_eq!(t.remaining_secs, 25 * 60);
        assert_eq!(t.pomodoros_completed, 0);
    }

    #[test]
    fn tick_does_nothing_when_idle() {
        let mut t = TimerState::new();
        t.tick();
        assert_eq!(t.remaining_secs, 25 * 60);
    }

    #[test]
    fn tick_decrements_when_running() {
        let mut t = TimerState::new();
        t.toggle_pause(); // start
        t.tick();
        assert_eq!(t.remaining_secs, 25 * 60 - 1);
    }

    #[test]
    fn phase_transitions_after_four_focus_sessions() {
        let mut t = TimerState::new();
        // Simulate 3 completed focus sessions via auto-advance (tick to 0).
        for _ in 0..3 {
            t.status = TimerStatus::Running;
            t.remaining_secs = 1;
            t.tick(); // completes focus → short break
            assert_eq!(t.phase, TimerPhase::ShortBreak);
            t.status = TimerStatus::Running;
            t.remaining_secs = 1;
            t.tick(); // completes short break → focus
            assert_eq!(t.phase, TimerPhase::Focus);
        }
        // 4th focus: manual skip should go to long break (pomodoros_completed == 3).
        t.next_phase();
        assert_eq!(t.phase, TimerPhase::LongBreak);
        assert_eq!(t.pomodoros_completed, 3); // skip doesn't increment counter
    }

    #[test]
    fn next_phase_skip_does_not_increment_counter() {
        let mut t = TimerState::new();
        t.next_phase(); // skip focus without completing it
        assert_eq!(t.pomodoros_completed, 0);
        assert_eq!(t.phase, TimerPhase::ShortBreak);
    }

    #[test]
    fn reset_restores_full_duration() {
        let mut t = TimerState::new();
        t.toggle_pause();
        for _ in 0..10 {
            t.tick();
        }
        t.reset();
        assert_eq!(t.remaining_secs, 25 * 60);
        assert_eq!(t.status, TimerStatus::Idle);
    }

    #[test]
    fn progress_starts_at_one() {
        let t = TimerState::new();
        assert!((t.progress() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn with_durations_uses_custom_values() {
        let t = TimerState::with_durations(10 * 60, 2 * 60, 8 * 60);
        assert_eq!(t.remaining_secs, 10 * 60);
        assert_eq!(t.focus_secs, 10 * 60);
        assert_eq!(t.short_break_secs, 2 * 60);
        assert_eq!(t.long_break_secs, 8 * 60);
        assert_eq!(t.long_break_interval, DEFAULT_LONG_BREAK_INTERVAL);
        assert_eq!(t.phase, TimerPhase::Focus);
        assert_eq!(t.status, TimerStatus::Idle);
    }

    #[test]
    fn with_durations_respects_custom_reset() {
        let mut t = TimerState::with_durations(10 * 60, 2 * 60, 8 * 60);
        t.toggle_pause();
        for _ in 0..30 {
            t.tick();
        }
        t.reset();
        assert_eq!(t.remaining_secs, 10 * 60);
    }

    #[test]
    fn with_durations_used_on_phase_advance() {
        let mut t = TimerState::with_durations(10 * 60, 2 * 60, 8 * 60);
        t.status = TimerStatus::Running;
        t.remaining_secs = 1;
        t.tick(); // completes focus → short break
        assert_eq!(t.phase, TimerPhase::ShortBreak);
        assert_eq!(t.remaining_secs, 2 * 60);
    }

    #[test]
    fn with_durations_zero_values_fall_back_to_defaults() {
        let t = TimerState::with_durations(0, 0, 0);
        assert_eq!(t.focus_secs, DEFAULT_FOCUS_SECS);
        assert_eq!(t.short_break_secs, DEFAULT_SHORT_BREAK_SECS);
        assert_eq!(t.long_break_secs, DEFAULT_LONG_BREAK_SECS);
        assert_eq!(t.long_break_interval, DEFAULT_LONG_BREAK_INTERVAL);
        assert_eq!(t.remaining_secs, DEFAULT_FOCUS_SECS);
    }

    #[test]
    fn with_profile_supports_custom_long_break_cadence() {
        let mut t = TimerState::with_profile(25 * 60, 5 * 60, 15 * 60, 2);
        t.status = TimerStatus::Running;
        t.remaining_secs = 1;
        t.tick(); // focus #1 -> short break
        assert_eq!(t.phase, TimerPhase::ShortBreak);
        t.status = TimerStatus::Running;
        t.remaining_secs = 1;
        t.tick(); // short break -> focus
        assert_eq!(t.phase, TimerPhase::Focus);
        t.status = TimerStatus::Running;
        t.remaining_secs = 1;
        t.tick(); // focus #2 -> long break (cadence 2)
        assert_eq!(t.phase, TimerPhase::LongBreak);
    }

    #[test]
    fn with_profile_zero_long_break_interval_falls_back_to_default() {
        let t = TimerState::with_profile(25 * 60, 5 * 60, 15 * 60, 0);
        assert_eq!(t.long_break_interval, DEFAULT_LONG_BREAK_INTERVAL);
    }

    #[test]
    fn tick_from_max_pomodoro_count_does_not_overflow() {
        let mut t = TimerState::new();
        t.phase = TimerPhase::Focus;
        t.pomodoros_completed = u32::MAX;
        t.status = TimerStatus::Running;
        t.remaining_secs = 1;

        t.tick();

        assert_eq!(t.pomodoros_completed, u32::MAX);
        assert_eq!(t.phase, TimerPhase::ShortBreak);
    }

    #[test]
    fn next_phase_from_max_pomodoro_count_does_not_overflow() {
        let mut t = TimerState::new();
        t.phase = TimerPhase::Focus;
        t.pomodoros_completed = u32::MAX;

        t.next_phase();

        assert_eq!(t.pomodoros_completed, u32::MAX);
        assert_eq!(t.phase, TimerPhase::ShortBreak);
    }

    #[test]
    fn tick_with_zero_long_break_interval_does_not_panic_and_uses_default_cadence() {
        let mut t = TimerState::new();
        t.phase = TimerPhase::Focus;
        t.long_break_interval = 0;
        t.status = TimerStatus::Running;
        t.remaining_secs = 1;

        t.tick();

        assert_eq!(t.phase, TimerPhase::ShortBreak);
        assert_eq!(t.pomodoros_completed, 1);
    }

    #[test]
    fn next_phase_with_zero_long_break_interval_uses_default_cadence() {
        let mut t = TimerState::new();
        t.phase = TimerPhase::Focus;
        t.long_break_interval = 0;
        t.pomodoros_completed = 3;

        t.next_phase();

        assert_eq!(t.phase, TimerPhase::LongBreak);
        assert_eq!(t.pomodoros_completed, 3);
    }
}
