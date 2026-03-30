use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::blocker::SiteBlocker;
use crate::timer::{TimerPhase, TimerState, TimerStatus};
use crate::wakatime::WakatimeTracker;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Timer,
    SiteManager,
}

pub struct App {
    pub timer: TimerState,
    pub should_quit: bool,
    pub mode: AppMode,
    pub blocker: SiteBlocker,
    /// Text being typed when adding a new site.
    pub site_input: String,
    /// Whether the user is currently typing a new site.
    pub site_input_active: bool,
    /// Index of the highlighted site in the SiteManager list.
    pub selected_site: usize,
    /// Last error from a block/unblock operation (e.g. permission denied).
    pub block_error: Option<String>,
    pub wakatime: WakatimeTracker,
}

impl App {
    pub fn new() -> Self {
        Self {
            timer: TimerState::new(),
            should_quit: false,
            mode: AppMode::Timer,
            blocker: SiteBlocker::new(),
            site_input: String::new(),
            site_input_active: false,
            selected_site: 0,
            block_error: None,
            wakatime: WakatimeTracker::new(),
        }
    }

    pub fn on_tick(&mut self) {
        let phase_changed = self.timer.tick();
        if phase_changed {
            self.apply_blocking_for_phase();
        }
    }

    /// Advance WakaTime tracking by `elapsed_secs` simulated seconds.
    ///
    /// Must be called **once per main-loop UI frame** (not once per catch-up
    /// tick) so that a burst of back-filled timer ticks after a
    /// suspend/resume cannot trigger multiple rapid heartbeats.
    pub fn on_wakatime_elapsed(&mut self, elapsed_secs: u64) {
        if self.timer.phase == TimerPhase::Focus && self.timer.status == TimerStatus::Running {
            self.wakatime.tick_elapsed(elapsed_secs);
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match self.mode {
            AppMode::Timer => self.handle_key_timer(key),
            AppMode::SiteManager => self.handle_key_site_manager(key),
        }
    }

    fn handle_key_timer(&mut self, key: KeyEvent) {
        match key.code {
            // Quit
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            // Ctrl-C fallback quit
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            // Start / pause
            KeyCode::Char(' ') => {
                self.timer.toggle_pause();
                self.apply_blocking_for_phase();
            }
            // Stop / reset current phase
            KeyCode::Char('s') => {
                self.timer.reset();
                self.apply_blocking_for_phase();
            }
            // Skip to next phase
            KeyCode::Char('n') => {
                self.timer.next_phase();
                self.apply_blocking_for_phase();
            }
            // Open site manager
            KeyCode::Char('b') => {
                self.mode = AppMode::SiteManager;
                self.clamp_selection();
            }
            _ => {}
        }
    }

    fn handle_key_site_manager(&mut self, key: KeyEvent) {
        if self.site_input_active {
            match key.code {
                KeyCode::Enter => {
                    let site = std::mem::take(&mut self.site_input);
                    self.blocker.add_site(site);
                    self.site_input_active = false;
                    self.clamp_selection();
                    // Re-apply block if currently blocking
                    if self.blocker.is_blocking {
                        if let Err(e) = self.blocker.block() {
                            self.block_error = Some(e.to_string());
                        } else {
                            self.block_error = None;
                        }
                    }
                }
                KeyCode::Esc => {
                    self.site_input.clear();
                    self.site_input_active = false;
                }
                KeyCode::Backspace => {
                    self.site_input.pop();
                }
                KeyCode::Char(c) => {
                    self.site_input.push(c);
                }
                _ => {}
            }
            return;
        }

        match key.code {
            // Back to timer view
            KeyCode::Esc | KeyCode::Char('b') => {
                self.mode = AppMode::Timer;
            }
            // Ctrl-C fallback quit
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            // Quit
            KeyCode::Char('q') => self.should_quit = true,
            // Navigate down
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.blocker.sites.is_empty() {
                    self.selected_site = (self.selected_site + 1).min(self.blocker.sites.len() - 1);
                }
            }
            // Navigate up
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected_site = self.selected_site.saturating_sub(1);
            }
            // Start adding a site
            KeyCode::Char('a') => {
                self.site_input_active = true;
            }
            // Delete selected site
            KeyCode::Char('d') | KeyCode::Delete => {
                if !self.blocker.sites.is_empty() {
                    self.blocker.remove_site(self.selected_site);
                    self.clamp_selection();
                    // Re-apply or clear block based on new list
                    if self.blocker.is_blocking {
                        if self.blocker.sites.is_empty() {
                            if let Err(e) = self.blocker.unblock() {
                                self.block_error = Some(e.to_string());
                            } else {
                                self.block_error = None;
                            }
                        } else if let Err(e) = self.blocker.block() {
                            self.block_error = Some(e.to_string());
                        } else {
                            self.block_error = None;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    pub fn is_running(&self) -> bool {
        self.timer.status == TimerStatus::Running
    }

    /// Apply or remove blocks based on the current timer phase and status, and
    /// synchronise WakaTime tracking state.
    ///
    /// Blocks whenever the focus phase is active (Running or Paused) so that
    /// pausing the timer cannot be used to bypass the block.
    /// Unblocks when the phase is a break or the timer has not yet started (Idle).
    fn apply_blocking_for_phase(&mut self) {
        let should_block =
            self.timer.phase == TimerPhase::Focus && self.timer.status != TimerStatus::Idle;
        if should_block {
            if let Err(e) = self.blocker.block() {
                self.block_error = Some(e.to_string());
            } else {
                self.block_error = None;
            }
        } else if let Err(e) = self.blocker.unblock() {
            self.block_error = Some(e.to_string());
        } else {
            self.block_error = None;
        }

        // Keep WakaTime tracking in sync with the focus session state.
        let focus_running =
            self.timer.phase == TimerPhase::Focus && self.timer.status == TimerStatus::Running;
        if focus_running && !self.wakatime.is_tracking() {
            self.wakatime.on_focus_start();
        } else if !focus_running && self.wakatime.is_tracking() {
            self.wakatime.on_focus_stop();
        }
    }

    fn clamp_selection(&mut self) {
        if self.blocker.sites.is_empty() {
            self.selected_site = 0;
        } else {
            self.selected_site = self.selected_site.min(self.blocker.sites.len() - 1);
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        // Ensure hosts-file block entries are removed on every exit path,
        // including early returns caused by I/O errors in run_app.
        self.blocker.cleanup();
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
