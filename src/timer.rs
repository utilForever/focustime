#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerPhase {
    Focus,
    ShortBreak,
    LongBreak,
}

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
            TimerPhase::Focus => 25 * 60,
            TimerPhase::ShortBreak => 5 * 60,
            TimerPhase::LongBreak => 15 * 60,
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
}

impl TimerState {
    pub fn new() -> Self {
        let phase = TimerPhase::Focus;
        Self {
            phase,
            status: TimerStatus::Idle,
            remaining_secs: phase.duration_secs(),
            pomodoros_completed: 0,
        }
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
        match self.phase {
            TimerPhase::Focus => {
                self.pomodoros_completed += 1;
                if self.pomodoros_completed.is_multiple_of(4) {
                    self.phase = TimerPhase::LongBreak;
                } else {
                    self.phase = TimerPhase::ShortBreak;
                }
            }
            TimerPhase::ShortBreak | TimerPhase::LongBreak => {
                self.phase = TimerPhase::Focus;
            }
        }
        self.remaining_secs = self.phase.duration_secs();
        self.status = TimerStatus::Idle;
    }

    /// Skip to the next phase immediately.
    pub fn next_phase(&mut self) {
        if self.phase == TimerPhase::Focus {
            self.pomodoros_completed += 1;
            if self.pomodoros_completed.is_multiple_of(4) {
                self.phase = TimerPhase::LongBreak;
            } else {
                self.phase = TimerPhase::ShortBreak;
            }
        } else {
            self.phase = TimerPhase::Focus;
        }
        self.remaining_secs = self.phase.duration_secs();
        self.status = TimerStatus::Idle;
    }

    /// Reset the current phase back to its full duration and stop.
    pub fn reset(&mut self) {
        self.remaining_secs = self.phase.duration_secs();
        self.status = TimerStatus::Idle;
    }

    pub fn toggle_pause(&mut self) {
        self.status = match self.status {
            TimerStatus::Idle | TimerStatus::Paused => TimerStatus::Running,
            TimerStatus::Running => TimerStatus::Paused,
        };
    }

    /// Progress as a value in [0.0, 1.0] where 1.0 means full time remaining.
    pub fn progress(&self) -> f64 {
        let total = self.phase.duration_secs() as f64;
        if total == 0.0 {
            return 0.0;
        }
        self.remaining_secs as f64 / total
    }
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
        for _ in 0..3 {
            t.next_phase(); // focus -> short break
            assert_eq!(t.phase, TimerPhase::ShortBreak);
            t.next_phase(); // short break -> focus
            assert_eq!(t.phase, TimerPhase::Focus);
        }
        t.next_phase(); // 4th focus -> long break
        assert_eq!(t.phase, TimerPhase::LongBreak);
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
}
