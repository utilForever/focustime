use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::timer::{TimerState, TimerStatus};

pub struct App {
    pub timer: TimerState,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            timer: TimerState::new(),
            should_quit: false,
        }
    }

    pub fn on_tick(&mut self) {
        self.timer.tick();
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            // Quit
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            // Ctrl-C fallback quit
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            // Start / pause
            KeyCode::Char(' ') => self.timer.toggle_pause(),
            // Stop / reset current phase
            KeyCode::Char('s') => self.timer.reset(),
            // Skip to next phase
            KeyCode::Char('n') => self.timer.next_phase(),
            _ => {}
        }
    }

    pub fn is_running(&self) -> bool {
        self.timer.status == TimerStatus::Running
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
