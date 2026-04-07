use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::blocker::SiteBlocker;
use crate::config::{AppConfig, CustomProfileConfig, ProfileId};
use crate::timer::{
    DEFAULT_FOCUS_SECS, DEFAULT_LONG_BREAK_INTERVAL, DEFAULT_LONG_BREAK_SECS,
    DEFAULT_SHORT_BREAK_SECS, TimerPhase, TimerState, TimerStatus,
};
use crate::wakatime::WakatimeTracker;

pub const PROFILE_IDS: [ProfileId; 3] =
    [ProfileId::Classic, ProfileId::DeepWork, ProfileId::Custom];
pub const CUSTOM_PROFILE_FIELD_LABELS: [&str; 4] =
    ["Focus", "Short Break", "Long Break", "Long-break cadence"];
const CUSTOM_DURATION_STEP_SECS: u64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Timer,
    SiteManager,
    ProfileManager,
}

#[derive(Debug, Clone, Copy)]
struct ProfileSpec {
    focus_secs: u64,
    short_break_secs: u64,
    long_break_secs: u64,
    long_break_interval: u32,
}

impl ProfileSpec {
    fn from_custom(custom: &CustomProfileConfig) -> Self {
        let custom = custom.normalized();
        Self {
            focus_secs: custom.focus_secs,
            short_break_secs: custom.short_break_secs,
            long_break_secs: custom.long_break_secs,
            long_break_interval: custom.long_break_interval,
        }
    }
}

fn built_in_profile_spec(profile: ProfileId) -> Option<ProfileSpec> {
    match profile {
        ProfileId::Classic => Some(ProfileSpec {
            focus_secs: DEFAULT_FOCUS_SECS,
            short_break_secs: DEFAULT_SHORT_BREAK_SECS,
            long_break_secs: DEFAULT_LONG_BREAK_SECS,
            long_break_interval: DEFAULT_LONG_BREAK_INTERVAL,
        }),
        ProfileId::DeepWork => Some(ProfileSpec {
            focus_secs: 50 * 60,
            short_break_secs: 10 * 60,
            long_break_secs: 30 * 60,
            long_break_interval: 3,
        }),
        ProfileId::Custom => None,
    }
}

fn profile_spec_for(profile: ProfileId, custom_profile: &CustomProfileConfig) -> ProfileSpec {
    built_in_profile_spec(profile).unwrap_or_else(|| ProfileSpec::from_custom(custom_profile))
}

fn profile_index(profile: ProfileId) -> usize {
    PROFILE_IDS
        .iter()
        .position(|candidate| *candidate == profile)
        .unwrap_or(PROFILE_IDS.len() - 1)
}

fn profile_for_index(index: usize) -> ProfileId {
    PROFILE_IDS
        .get(index)
        .copied()
        .unwrap_or(PROFILE_IDS[PROFILE_IDS.len() - 1])
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
    /// Last error from persisting timer/site configuration.
    pub config_error: Option<String>,
    pub wakatime: WakatimeTracker,
    pub selected_profile: ProfileId,
    pub custom_profile: CustomProfileConfig,
    pub profile_selection_index: usize,
    pub profile_edit_active: bool,
    pub profile_edit_field: usize,
}

impl App {
    pub fn new() -> Self {
        Self::from_config(AppConfig::load())
    }

    fn from_config(config: AppConfig) -> Self {
        let selected_profile = config.selected_profile;
        let custom_profile = config.effective_custom_profile();
        let profile_spec = profile_spec_for(selected_profile, &custom_profile);
        let timer = TimerState::with_profile(
            profile_spec.focus_secs,
            profile_spec.short_break_secs,
            profile_spec.long_break_secs,
            profile_spec.long_break_interval,
        );
        let mut blocker = SiteBlocker::new();
        for site in &config.blocked_sites {
            blocker.add_site(site.clone());
        }
        Self {
            timer,
            should_quit: false,
            mode: AppMode::Timer,
            blocker,
            site_input: String::new(),
            site_input_active: false,
            selected_site: 0,
            block_error: None,
            config_error: None,
            wakatime: WakatimeTracker::new(),
            selected_profile,
            custom_profile,
            profile_selection_index: profile_index(selected_profile),
            profile_edit_active: false,
            profile_edit_field: 0,
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

    pub fn selected_profile_name(&self) -> &'static str {
        self.selected_profile.label()
    }

    pub fn profile_values(&self, profile: ProfileId) -> (u64, u64, u64, u32) {
        let spec = profile_spec_for(profile, &self.custom_profile);
        (
            spec.focus_secs,
            spec.short_break_secs,
            spec.long_break_secs,
            spec.long_break_interval,
        )
    }

    pub fn profile_summary(&self, profile: ProfileId) -> String {
        let (focus, short_break, long_break, cadence) = self.profile_values(profile);
        format!(
            "{:>2}/{:>2}/{:>2}m · every {} focus",
            focus / 60,
            short_break / 60,
            long_break / 60,
            cadence
        )
    }

    pub fn custom_profile_field_value(&self, field_index: usize) -> String {
        match field_index {
            0 => format!("{} min", self.custom_profile.focus_secs / 60),
            1 => format!("{} min", self.custom_profile.short_break_secs / 60),
            2 => format!("{} min", self.custom_profile.long_break_secs / 60),
            3 => format!(
                "every {} focus sessions",
                self.custom_profile.long_break_interval
            ),
            _ => String::new(),
        }
    }

    /// Persist the current blocked-sites list and timer preferences to disk.
    /// Failures are best-effort; the error is surfaced through `config_error`.
    fn save_config(&mut self) {
        let config = AppConfig {
            focus_secs: self.timer.focus_secs,
            short_break_secs: self.timer.short_break_secs,
            long_break_secs: self.timer.long_break_secs,
            long_break_interval: self.timer.long_break_interval,
            blocked_sites: self.blocker.sites.clone(),
            selected_profile: self.selected_profile,
            custom_profile: Some(self.custom_profile.clone()),
        };
        if let Err(e) = config.save() {
            self.config_error = Some(format!("config save failed: {e}"));
        } else {
            self.config_error = None;
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match self.mode {
            AppMode::Timer => self.handle_key_timer(key),
            AppMode::SiteManager => self.handle_key_site_manager(key),
            AppMode::ProfileManager => self.handle_key_profile_manager(key),
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
            // Open profile manager
            KeyCode::Char('p') => {
                self.mode = AppMode::ProfileManager;
                self.profile_edit_active = false;
                self.profile_edit_field = 0;
                self.profile_selection_index = profile_index(self.selected_profile);
                self.clamp_profile_selection();
            }
            _ => {}
        }
    }

    fn handle_key_profile_manager(&mut self, key: KeyEvent) {
        if self.profile_edit_active {
            match key.code {
                KeyCode::Char('q') => self.should_quit = true,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.should_quit = true;
                }
                KeyCode::Esc => {
                    self.profile_edit_active = false;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.profile_edit_field = self.profile_edit_field.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.profile_edit_field = (self.profile_edit_field + 1)
                        .min(CUSTOM_PROFILE_FIELD_LABELS.len().saturating_sub(1));
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    self.adjust_custom_profile_field(false);
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    self.adjust_custom_profile_field(true);
                }
                KeyCode::Enter => {
                    self.profile_edit_active = false;
                    self.custom_profile = self.custom_profile.normalized();
                    if self.selected_profile == ProfileId::Custom {
                        self.apply_profile(ProfileId::Custom);
                    } else {
                        self.save_config();
                    }
                }
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('p') => {
                self.mode = AppMode::Timer;
            }
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.profile_selection_index = self.profile_selection_index.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.profile_selection_index =
                    (self.profile_selection_index + 1).min(PROFILE_IDS.len().saturating_sub(1));
            }
            KeyCode::Enter => {
                let selected = profile_for_index(self.profile_selection_index);
                self.apply_profile(selected);
                self.mode = AppMode::Timer;
            }
            KeyCode::Char('e') => {
                if profile_for_index(self.profile_selection_index) == ProfileId::Custom {
                    self.profile_edit_active = true;
                    self.profile_edit_field = 0;
                }
            }
            _ => {}
        }
    }

    fn adjust_custom_profile_field(&mut self, increase: bool) {
        match self.profile_edit_field {
            0 => adjust_duration_minutes(&mut self.custom_profile.focus_secs, increase),
            1 => adjust_duration_minutes(&mut self.custom_profile.short_break_secs, increase),
            2 => adjust_duration_minutes(&mut self.custom_profile.long_break_secs, increase),
            3 => {
                if increase {
                    self.custom_profile.long_break_interval =
                        self.custom_profile.long_break_interval.saturating_add(1);
                } else {
                    self.custom_profile.long_break_interval = self
                        .custom_profile
                        .long_break_interval
                        .saturating_sub(1)
                        .max(1);
                }
            }
            _ => {}
        }
    }

    fn apply_profile(&mut self, profile: ProfileId) {
        let profile_spec = profile_spec_for(profile, &self.custom_profile);
        self.timer = TimerState::with_profile(
            profile_spec.focus_secs,
            profile_spec.short_break_secs,
            profile_spec.long_break_secs,
            profile_spec.long_break_interval,
        );
        self.selected_profile = profile;
        self.profile_selection_index = profile_index(profile);
        self.save_config();
        self.apply_blocking_for_phase();
    }

    fn handle_key_site_manager(&mut self, key: KeyEvent) {
        if self.site_input_active {
            match key.code {
                KeyCode::Enter => {
                    let site = std::mem::take(&mut self.site_input);
                    self.blocker.add_site(site);
                    self.site_input_active = false;
                    self.clamp_selection();
                    self.save_config();
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
                    self.save_config();
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

    fn clamp_profile_selection(&mut self) {
        if PROFILE_IDS.is_empty() {
            self.profile_selection_index = 0;
        } else {
            self.profile_selection_index = self
                .profile_selection_index
                .min(PROFILE_IDS.len().saturating_sub(1));
        }
    }
}

fn adjust_duration_minutes(value: &mut u64, increase: bool) {
    if increase {
        *value = value.saturating_add(CUSTOM_DURATION_STEP_SECS);
    } else {
        *value = value
            .saturating_sub(CUSTOM_DURATION_STEP_SECS)
            .max(CUSTOM_DURATION_STEP_SECS);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn selected_builtin_profile_is_applied_on_startup() {
        let config = AppConfig {
            focus_secs: 10 * 60,
            short_break_secs: 2 * 60,
            long_break_secs: 8 * 60,
            long_break_interval: 2,
            blocked_sites: Vec::new(),
            selected_profile: ProfileId::Classic,
            custom_profile: Some(CustomProfileConfig {
                focus_secs: 40 * 60,
                short_break_secs: 8 * 60,
                long_break_secs: 16 * 60,
                long_break_interval: 2,
            }),
        };
        let app = App::from_config(config);
        assert_eq!(app.selected_profile, ProfileId::Classic);
        assert_eq!(app.timer.focus_secs, DEFAULT_FOCUS_SECS);
        assert_eq!(app.timer.short_break_secs, DEFAULT_SHORT_BREAK_SECS);
        assert_eq!(app.timer.long_break_secs, DEFAULT_LONG_BREAK_SECS);
        assert_eq!(app.timer.long_break_interval, DEFAULT_LONG_BREAK_INTERVAL);
    }

    #[test]
    fn profile_manager_enter_applies_selection() {
        let config = AppConfig {
            selected_profile: ProfileId::Custom,
            custom_profile: Some(CustomProfileConfig::default()),
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);

        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Up)); // Custom -> Deep Work
        app.handle_key(key(KeyCode::Enter));

        assert_eq!(app.mode, AppMode::Timer);
        assert_eq!(app.selected_profile, ProfileId::DeepWork);
        let (focus, short_break, long_break, cadence) = app.profile_values(ProfileId::DeepWork);
        assert_eq!(app.timer.focus_secs, focus);
        assert_eq!(app.timer.short_break_secs, short_break);
        assert_eq!(app.timer.long_break_secs, long_break);
        assert_eq!(app.timer.long_break_interval, cadence);
    }

    #[test]
    fn editing_custom_profile_updates_timer_when_custom_is_selected() {
        let custom = CustomProfileConfig {
            focus_secs: 30 * 60,
            short_break_secs: 6 * 60,
            long_break_secs: 12 * 60,
            long_break_interval: 3,
        };
        let config = AppConfig {
            selected_profile: ProfileId::Custom,
            custom_profile: Some(custom.clone()),
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        let original_focus = app.timer.focus_secs;

        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Char('e')));
        app.handle_key(key(KeyCode::Right)); // +1 minute focus
        app.handle_key(key(KeyCode::Enter)); // save edit

        assert_eq!(app.selected_profile, ProfileId::Custom);
        assert_eq!(
            app.timer.focus_secs,
            original_focus + CUSTOM_DURATION_STEP_SECS
        );
        assert_eq!(
            app.custom_profile.focus_secs,
            original_focus + CUSTOM_DURATION_STEP_SECS
        );
    }
}
