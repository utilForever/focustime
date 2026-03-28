use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::timer::{TimerPhase, TimerState, TimerStatus};
use crate::{blocker, config};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Timer,
    SiteList,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    AddingSite,
}

pub struct App {
    pub timer: TimerState,
    pub should_quit: bool,

    // Site blocking
    pub active_tab: ActiveTab,
    pub block_list: Vec<String>,
    pub selected_site: usize,
    pub input_mode: InputMode,
    pub input_buffer: String,
    pub blocker_error: Option<String>,
}

impl App {
    pub fn new() -> Self {
        let block_list = config::load_block_list();
        Self {
            timer: TimerState::new(),
            should_quit: false,
            active_tab: ActiveTab::Timer,
            block_list,
            selected_site: 0,
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            blocker_error: None,
        }
    }

    pub fn on_tick(&mut self) {
        let prev_phase = self.timer.phase;
        let phase_ended = self.timer.tick();
        if phase_ended {
            self.on_phase_changed(prev_phase, self.timer.phase);
        }
    }

    /// Called whenever the timer transitions to a new phase.
    fn on_phase_changed(&mut self, _from: TimerPhase, to: TimerPhase) {
        match to {
            TimerPhase::Focus => self.apply_blocking(),
            TimerPhase::ShortBreak | TimerPhase::LongBreak => self.release_blocking(),
        }
    }

    fn apply_blocking(&mut self) {
        if self.block_list.is_empty() {
            return;
        }
        if let Err(e) = blocker::apply_blocking(&self.block_list) {
            self.blocker_error = Some(format!("Block failed: {e}"));
        } else {
            self.blocker_error = None;
        }
    }

    fn release_blocking(&mut self) {
        if let Err(e) = blocker::remove_blocking() {
            self.blocker_error = Some(format!("Unblock failed: {e}"));
        } else {
            self.blocker_error = None;
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match self.input_mode {
            InputMode::AddingSite => self.handle_key_adding(key),
            InputMode::Normal => self.handle_key_normal(key),
        }
    }

    fn handle_key_normal(&mut self, key: KeyEvent) {
        match key.code {
            // Quit
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            // Switch tabs
            KeyCode::Tab => {
                self.active_tab = match self.active_tab {
                    ActiveTab::Timer => ActiveTab::SiteList,
                    ActiveTab::SiteList => ActiveTab::Timer,
                };
            }
            // Timer-specific keys
            KeyCode::Char(' ') if self.active_tab == ActiveTab::Timer => {
                let prev_phase = self.timer.phase;
                let was_running = self.timer.status == TimerStatus::Running;
                self.timer.toggle_pause();
                // Starting Focus → apply blocking; pausing → nothing
                if !was_running && self.timer.phase == TimerPhase::Focus {
                    self.on_phase_changed(prev_phase, TimerPhase::Focus);
                }
            }
            KeyCode::Char('s') if self.active_tab == ActiveTab::Timer => {
                self.timer.reset();
                self.release_blocking();
            }
            KeyCode::Char('n') if self.active_tab == ActiveTab::Timer => {
                let prev = self.timer.phase;
                self.timer.next_phase();
                self.on_phase_changed(prev, self.timer.phase);
            }
            // Site list keys
            KeyCode::Char('a') if self.active_tab == ActiveTab::SiteList => {
                self.input_mode = InputMode::AddingSite;
                self.input_buffer.clear();
            }
            KeyCode::Char('d') | KeyCode::Delete if self.active_tab == ActiveTab::SiteList => {
                self.remove_selected_site();
            }
            KeyCode::Up if self.active_tab == ActiveTab::SiteList => {
                if self.selected_site > 0 {
                    self.selected_site -= 1;
                }
            }
            KeyCode::Down if self.active_tab == ActiveTab::SiteList => {
                if !self.block_list.is_empty() && self.selected_site < self.block_list.len() - 1 {
                    self.selected_site += 1;
                }
            }
            _ => {}
        }
    }

    fn handle_key_adding(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                let site = self.input_buffer.trim().to_string();
                if !site.is_empty() && !self.block_list.contains(&site) {
                    self.block_list.push(site);
                    self.selected_site = self.block_list.len() - 1;
                    self.persist_block_list();
                }
                self.input_mode = InputMode::Normal;
                self.input_buffer.clear();
            }
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.input_buffer.clear();
            }
            KeyCode::Backspace => {
                self.input_buffer.pop();
            }
            KeyCode::Char(c) => {
                self.input_buffer.push(c);
            }
            _ => {}
        }
    }

    fn remove_selected_site(&mut self) {
        if self.block_list.is_empty() {
            return;
        }
        self.block_list.remove(self.selected_site);
        if self.selected_site > 0 && self.selected_site >= self.block_list.len() {
            self.selected_site = self.block_list.len().saturating_sub(1);
        }
        self.persist_block_list();
        // If currently blocking, update the hosts file immediately
        if self.timer.phase == TimerPhase::Focus && self.timer.status == TimerStatus::Running {
            self.apply_blocking();
        }
    }

    fn persist_block_list(&mut self) {
        if let Err(e) = config::save_block_list(&self.block_list) {
            self.blocker_error = Some(format!("Save failed: {e}"));
        }
    }

    pub fn is_running(&self) -> bool {
        self.timer.status == TimerStatus::Running
    }

    /// Clean up: remove blocking entries from hosts file before quitting.
    pub fn cleanup(&mut self) {
        self.release_blocking();
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
