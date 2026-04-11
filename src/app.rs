use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::blocker::{BulkAddResult, EditSiteResult, InvalidSiteInput, SiteBlocker};
use crate::config::{AppConfig, CustomProfileConfig, NotificationConfig, ProfileId};
use crate::notifications::PhaseNotifier;
use crate::stats::{DailyStats, FocusStats, SessionStats, current_day_key};
use crate::timer::{
    DEFAULT_FOCUS_SECS, DEFAULT_LONG_BREAK_INTERVAL, DEFAULT_LONG_BREAK_SECS,
    DEFAULT_SHORT_BREAK_SECS, TimerPhase, TimerState, TimerStatus,
};
use crate::wakatime::WakatimeTracker;

pub const PROFILE_IDS: [ProfileId; 3] =
    [ProfileId::Classic, ProfileId::DeepWork, ProfileId::Custom];
pub const PROFILE_EDIT_FIELD_LABELS: [&str; 6] = [
    "Focus",
    "Short Break",
    "Long Break",
    "Long-break cadence",
    "Phase notifications",
    "Sound alert",
];
const CUSTOM_DURATION_STEP_SECS: u64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Timer,
    SiteManager,
    ProfileManager,
    StatsHistory,
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

#[derive(Debug, Clone)]
struct ProfileEditSnapshot {
    custom_profile: CustomProfileConfig,
    notification_settings: NotificationConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteInputMode {
    Add,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteFeedbackLevel {
    Success,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteFeedback {
    pub level: SiteFeedbackLevel,
    pub message: String,
}

pub struct App {
    pub timer: TimerState,
    pub should_quit: bool,
    pub mode: AppMode,
    pub blocker: SiteBlocker,
    /// Text being typed for add/import or edit site input.
    pub site_input: String,
    /// Whether the user is currently typing a new site.
    pub site_input_active: bool,
    site_edit_index: Option<usize>,
    pub site_feedback: Option<SiteFeedback>,
    /// Index of the highlighted site in the SiteManager list.
    pub selected_site: usize,
    /// Last error from a block/unblock operation (e.g. permission denied).
    pub block_error: Option<String>,
    /// Last error from persisting timer/site configuration.
    pub config_error: Option<String>,
    /// Last error from persisting focus stats.
    pub stats_error: Option<String>,
    pub phase_notification: Option<String>,
    pub wakatime: WakatimeTracker,
    pub selected_profile: ProfileId,
    pub custom_profile: CustomProfileConfig,
    pub profile_selection_index: usize,
    pub profile_edit_active: bool,
    pub profile_edit_field: usize,
    profile_edit_snapshot: Option<ProfileEditSnapshot>,
    notification_settings: NotificationConfig,
    notifier: PhaseNotifier,
    stats: FocusStats,
    stats_dirty: bool,
    stats_has_unsaved_elapsed: bool,
}

impl App {
    pub fn new() -> Self {
        #[cfg(test)]
        {
            Self::from_config(AppConfig::default())
        }
        #[cfg(not(test))]
        {
            Self::from_config(AppConfig::load())
        }
    }

    fn from_config(config: AppConfig) -> Self {
        let selected_profile = config.selected_profile;
        let custom_profile = config.effective_custom_profile();
        let notification_settings = config.notifications;
        let profile_spec = profile_spec_for(selected_profile, &custom_profile);
        let (stats, stats_error) = match FocusStats::load() {
            Ok(stats) => (stats, None),
            Err(e) => (FocusStats::default(), Some(e)),
        };
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
            site_edit_index: None,
            site_feedback: None,
            selected_site: 0,
            block_error: None,
            config_error: None,
            stats_error,
            phase_notification: None,
            wakatime: WakatimeTracker::new(),
            selected_profile,
            custom_profile,
            profile_selection_index: profile_index(selected_profile),
            profile_edit_active: false,
            profile_edit_field: 0,
            profile_edit_snapshot: None,
            notification_settings,
            notifier: PhaseNotifier::new(notification_settings),
            stats,
            stats_dirty: false,
            stats_has_unsaved_elapsed: false,
        }
    }

    pub fn on_tick(&mut self, is_catchup: bool) {
        let was_focus_running = self.focus_running_for_current_state();
        let was_focus_phase = self.timer.phase == TimerPhase::Focus;
        let completed_phase = self.timer.phase;
        if was_focus_running && !is_catchup {
            self.record_focus_elapsed(1);
        }

        let phase_changed = self.timer.tick();
        if !is_catchup && phase_changed && was_focus_phase && self.timer.phase != TimerPhase::Focus
        {
            self.record_completed_focus_session();
        }
        if phase_changed {
            if !is_catchup {
                self.phase_notification = self
                    .notifier
                    .notify_phase_completion(completed_phase, self.timer.phase);
            }
            self.apply_blocking_for_phase();
        }
        self.flush_stats_if_dirty(false);
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

    /// Applies any completed async WakaTime heartbeat results to tracker state.
    /// Intended to be called once per UI frame.
    pub fn poll_wakatime_status(&mut self) {
        self.wakatime.poll_events();
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
            "{}/{}/{} · every {} focus",
            format_duration_label(focus),
            format_duration_label(short_break),
            format_duration_label(long_break),
            cadence
        )
    }

    pub fn session_stats(&self) -> SessionStats {
        self.stats.session()
    }

    pub fn today_stats(&self) -> DailyStats {
        self.stats.daily_for(&current_day_key())
    }

    pub fn recent_daily_stats(&self, limit: usize) -> Vec<(String, DailyStats)> {
        self.stats.recent_daily(limit)
    }

    pub fn profile_edit_field_value(&self, field_index: usize) -> String {
        match field_index {
            0 => format_duration_label(self.custom_profile.focus_secs),
            1 => format_duration_label(self.custom_profile.short_break_secs),
            2 => format_duration_label(self.custom_profile.long_break_secs),
            3 => format!(
                "every {} focus sessions",
                self.custom_profile.long_break_interval
            ),
            4 => bool_label(self.notification_settings.enabled).to_string(),
            5 => bool_label(self.notification_settings.sound).to_string(),
            _ => String::new(),
        }
    }

    pub fn site_input_mode(&self) -> SiteInputMode {
        if self.site_edit_index.is_some() {
            SiteInputMode::Edit
        } else {
            SiteInputMode::Add
        }
    }

    /// Persist the current blocked-sites list and timer preferences to disk.
    /// Failures are best-effort; the error is surfaced through `config_error`.
    fn persisted_config(&self) -> AppConfig {
        let custom_profile = self.custom_profile.normalized();
        AppConfig {
            // Keep legacy fields aligned with the editable custom profile so
            // older releases retain user-configured values.
            focus_secs: custom_profile.focus_secs,
            short_break_secs: custom_profile.short_break_secs,
            long_break_secs: custom_profile.long_break_secs,
            long_break_interval: custom_profile.long_break_interval,
            blocked_sites: self.blocker.sites.clone(),
            selected_profile: self.selected_profile,
            custom_profile: Some(custom_profile),
            notifications: self.notification_settings,
        }
    }

    /// Persist the current blocked-sites list and timer preferences to disk.
    /// Failures are best-effort; the error is surfaced through `config_error`.
    #[cfg(not(test))]
    fn save_config(&mut self) {
        let config = self.persisted_config();
        if let Err(e) = config.save() {
            self.config_error = Some(format!("config save failed: {e}"));
        } else {
            self.config_error = None;
        }
    }

    #[cfg(test)]
    fn save_config(&mut self) {
        let _ = self.persisted_config();
        self.config_error = None;
    }

    #[cfg(not(test))]
    fn save_stats(&mut self) {
        if let Err(e) = self.stats.save() {
            self.stats_error = Some(format!("stats save failed: {e}"));
        } else {
            self.stats_error = None;
        }
    }

    #[cfg(test)]
    fn save_stats(&mut self) {
        self.stats_error = None;
    }

    fn flush_stats_if_dirty(&mut self, force_partial: bool) {
        if !(self.stats_dirty || (force_partial && self.stats_has_unsaved_elapsed)) {
            return;
        }

        self.save_stats();
        if self.stats_error.is_none() {
            self.stats_dirty = false;
            self.stats_has_unsaved_elapsed = false;
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match self.mode {
            AppMode::Timer => self.handle_key_timer(key),
            AppMode::SiteManager => self.handle_key_site_manager(key),
            AppMode::ProfileManager => self.handle_key_profile_manager(key),
            AppMode::StatsHistory => self.handle_key_stats_history(key),
        }
    }

    pub fn handle_paste(&mut self, text: String) {
        if self.mode != AppMode::SiteManager {
            return;
        }

        if !self.site_input_active {
            self.start_site_input(SiteInputMode::Add);
        }
        self.site_input.push_str(&text);
    }

    fn handle_key_timer(&mut self, key: KeyEvent) {
        if self.handle_quit_key(&key, true) {
            return;
        }

        match key.code {
            // Start / pause
            KeyCode::Char(' ') => {
                self.update_timer_and_sync(TimerState::toggle_pause);
            }
            // Stop / reset current phase
            KeyCode::Char('s') => {
                self.update_timer_and_sync(TimerState::reset);
            }
            // Skip to next phase
            KeyCode::Char('n') => {
                self.update_timer_and_sync(TimerState::next_phase);
            }
            // Open site manager
            KeyCode::Char('b') => {
                self.open_site_manager();
            }
            // Open profile manager
            KeyCode::Char('p') => {
                self.open_profile_manager();
            }
            // Open stats history
            KeyCode::Char('h') => {
                self.open_stats_history();
            }
            _ => {}
        }
    }

    fn handle_key_stats_history(&mut self, key: KeyEvent) {
        if self.handle_quit_key(&key, false) {
            return;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('h') => {
                self.mode = AppMode::Timer;
            }
            _ => {}
        }
    }

    fn handle_key_profile_manager(&mut self, key: KeyEvent) {
        if self.profile_edit_active {
            if self.handle_quit_key(&key, false) {
                return;
            }

            match key.code {
                KeyCode::Esc => {
                    self.cancel_profile_edit();
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.profile_edit_field = self.profile_edit_field.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.profile_edit_field = (self.profile_edit_field + 1)
                        .min(PROFILE_EDIT_FIELD_LABELS.len().saturating_sub(1));
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    self.adjust_profile_edit_field(false);
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    self.adjust_profile_edit_field(true);
                }
                KeyCode::Enter => {
                    self.commit_profile_edit();
                }
                _ => {}
            }
            return;
        }

        if self.handle_quit_key(&key, false) {
            return;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('p') => {
                self.exit_profile_manager();
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
                self.exit_profile_manager();
            }
            KeyCode::Char('e') => {
                self.begin_profile_edit();
            }
            _ => {}
        }
    }

    fn begin_profile_edit(&mut self) {
        self.profile_edit_snapshot = Some(ProfileEditSnapshot {
            custom_profile: self.custom_profile.clone(),
            notification_settings: self.notification_settings,
        });
        self.profile_edit_active = true;
        self.profile_edit_field = 0;
    }

    fn cancel_profile_edit(&mut self) {
        if let Some(snapshot) = self.profile_edit_snapshot.take() {
            self.custom_profile = snapshot.custom_profile;
            self.notification_settings = snapshot.notification_settings;
            self.rebuild_notifier();
        }
        self.profile_edit_active = false;
        self.profile_edit_field = 0;
    }

    fn commit_profile_edit(&mut self) {
        self.profile_edit_active = false;
        self.profile_edit_field = 0;
        self.profile_edit_snapshot = None;
        self.custom_profile = self.custom_profile.normalized();
        self.rebuild_notifier();
        if self.selected_profile == ProfileId::Custom {
            self.apply_profile(ProfileId::Custom);
        } else {
            self.save_config();
        }
    }

    fn adjust_profile_edit_field(&mut self, increase: bool) {
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
            4 => {
                self.notification_settings.enabled = increase;
            }
            5 => {
                self.notification_settings.sound = increase;
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
                    self.commit_site_input();
                }
                KeyCode::Esc => {
                    self.cancel_site_input();
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

        if self.handle_quit_key(&key, false) {
            return;
        }

        match key.code {
            // Back to timer view
            KeyCode::Esc | KeyCode::Char('b') => {
                self.mode = AppMode::Timer;
            }
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
                self.start_site_input(SiteInputMode::Add);
            }
            // Edit selected site
            KeyCode::Char('e') => {
                self.start_site_input(SiteInputMode::Edit);
            }
            // Delete selected site
            KeyCode::Char('d') | KeyCode::Delete => {
                self.remove_selected_site();
            }
            _ => {}
        }
    }

    fn start_site_input(&mut self, mode: SiteInputMode) {
        self.site_input_active = true;
        self.site_feedback = None;
        match mode {
            SiteInputMode::Add => {
                self.site_edit_index = None;
                self.site_input.clear();
            }
            SiteInputMode::Edit => {
                if self.blocker.sites.is_empty() {
                    self.site_input_active = false;
                    self.set_site_feedback(SiteFeedbackLevel::Warning, "No site selected to edit");
                    return;
                }
                self.clamp_selection();
                self.site_edit_index = Some(self.selected_site);
                self.site_input = self.blocker.sites[self.selected_site].clone();
            }
        }
    }

    fn cancel_site_input(&mut self) {
        self.site_input.clear();
        self.site_input_active = false;
        self.site_edit_index = None;
    }

    fn commit_site_input(&mut self) {
        let input = self.site_input.clone();

        let committed = if let Some(index) = self.site_edit_index {
            let edit_result = self.blocker.edit_site_from_input(index, &input);
            self.apply_edit_site_result(edit_result)
        } else {
            let add_result = self.blocker.add_sites_from_input(&input);
            self.apply_bulk_add_result(add_result)
        };

        if committed {
            self.cancel_site_input();
        }
    }

    fn apply_bulk_add_result(&mut self, result: BulkAddResult) -> bool {
        let committed = !result.added.is_empty();
        if committed {
            self.selected_site = self.blocker.sites.len().saturating_sub(1);
            self.finalize_site_mutation();
        }

        let mut parts = Vec::new();
        if !result.added.is_empty() {
            parts.push(format!(
                "Added {}",
                format_count(result.added.len(), "site", "sites")
            ));
        }
        if !result.duplicates.is_empty() {
            parts.push(format!(
                "Skipped {}",
                format_count(result.duplicates.len(), "duplicate", "duplicates")
            ));
        }
        if !result.invalid.is_empty() {
            parts.push(format!(
                "Rejected {} ({})",
                format_count(
                    result.invalid.len(),
                    "invalid hostname",
                    "invalid hostnames"
                ),
                summarize_invalid_inputs(&result.invalid)
            ));
        }

        let level = if result.invalid.is_empty() && result.duplicates.is_empty() {
            SiteFeedbackLevel::Success
        } else {
            SiteFeedbackLevel::Warning
        };
        let message = if parts.is_empty() {
            "No hostnames submitted".to_string()
        } else {
            parts.join(" • ")
        };
        self.set_site_feedback(level, message);
        committed
    }

    fn apply_edit_site_result(&mut self, result: EditSiteResult) -> bool {
        match result {
            EditSiteResult::Updated { old, new } => {
                self.finalize_site_mutation();
                self.set_site_feedback(
                    SiteFeedbackLevel::Success,
                    format!("Updated `{old}` -> `{new}`"),
                );
                true
            }
            EditSiteResult::Unchanged { hostname } => {
                self.set_site_feedback(
                    SiteFeedbackLevel::Warning,
                    format!("No change for `{hostname}`"),
                );
                false
            }
            EditSiteResult::Duplicate { hostname } => {
                self.set_site_feedback(
                    SiteFeedbackLevel::Warning,
                    format!("`{hostname}` is already in the blocklist"),
                );
                false
            }
            EditSiteResult::Invalid(invalid) => {
                self.set_site_feedback(
                    SiteFeedbackLevel::Warning,
                    format!(
                        "Invalid hostname `{}` ({})",
                        display_input_value(&invalid.input),
                        invalid.reason.message()
                    ),
                );
                false
            }
            EditSiteResult::MissingSelection => {
                self.set_site_feedback(SiteFeedbackLevel::Warning, "No site selected to edit");
                false
            }
        }
    }

    fn remove_selected_site(&mut self) {
        if self.blocker.sites.is_empty() {
            self.set_site_feedback(SiteFeedbackLevel::Warning, "No site selected to delete");
            return;
        }

        if let Some(removed) = self.blocker.remove_site(self.selected_site) {
            self.finalize_site_mutation();
            self.set_site_feedback(
                SiteFeedbackLevel::Success,
                format!("Removed `{removed}` from blocklist"),
            );
        } else {
            self.set_site_feedback(SiteFeedbackLevel::Warning, "No site selected to delete");
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
        let block_result = if self.should_block_for_current_state() {
            self.blocker.block()
        } else {
            self.blocker.unblock()
        };
        self.set_block_error_from_result(block_result);
        self.sync_wakatime_tracking_for_state();
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

    fn handle_quit_key(&mut self, key: &KeyEvent, esc_quits: bool) -> bool {
        match key.code {
            KeyCode::Char('q') => {
                self.should_quit = true;
                true
            }
            KeyCode::Esc if esc_quits => {
                self.should_quit = true;
                true
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
                true
            }
            _ => false,
        }
    }

    fn update_timer_and_sync(&mut self, action: fn(&mut TimerState)) {
        action(&mut self.timer);
        self.apply_blocking_for_phase();
    }

    fn open_site_manager(&mut self) {
        self.mode = AppMode::SiteManager;
        self.cancel_site_input();
        self.clamp_selection();
    }

    fn open_profile_manager(&mut self) {
        self.mode = AppMode::ProfileManager;
        self.profile_edit_active = false;
        self.profile_edit_field = 0;
        self.profile_edit_snapshot = None;
        self.profile_selection_index = profile_index(self.selected_profile);
        self.clamp_profile_selection();
    }

    fn open_stats_history(&mut self) {
        self.mode = AppMode::StatsHistory;
    }

    fn exit_profile_manager(&mut self) {
        self.mode = AppMode::Timer;
        self.profile_edit_snapshot = None;
    }

    fn finalize_site_mutation(&mut self) {
        self.clamp_selection();
        self.save_config();
        self.sync_blocking_after_site_mutation();
    }

    fn sync_blocking_after_site_mutation(&mut self) {
        if !self.should_resync_blocking_after_site_mutation() {
            return;
        }

        let should_block = self.should_block_for_current_state();
        let block_result = if should_block {
            if self.blocker.sites.is_empty() {
                self.blocker.unblock()
            } else {
                self.blocker.block()
            }
        } else {
            self.blocker.unblock()
        };
        self.set_block_error_from_result(block_result);
    }

    fn should_resync_blocking_after_site_mutation(&self) -> bool {
        self.should_block_for_current_state() || self.blocker.is_blocking
    }

    fn should_block_for_current_state(&self) -> bool {
        self.timer.phase == TimerPhase::Focus && self.timer.status != TimerStatus::Idle
    }

    fn focus_running_for_current_state(&self) -> bool {
        self.timer.phase == TimerPhase::Focus && self.timer.status == TimerStatus::Running
    }

    fn record_focus_elapsed(&mut self, elapsed_secs: u64) {
        if elapsed_secs == 0 {
            return;
        }

        let day_key = current_day_key();
        let session_minutes_before = self.stats.session().focused_minutes();
        let today_minutes_before = self.stats.daily_for(&day_key).focused_minutes();

        self.stats.record_focus_elapsed(&day_key, elapsed_secs);
        self.stats_has_unsaved_elapsed = true;

        let session_minutes_after = self.stats.session().focused_minutes();
        let today_minutes_after = self.stats.daily_for(&day_key).focused_minutes();
        if session_minutes_before != session_minutes_after
            || today_minutes_before != today_minutes_after
        {
            self.stats_dirty = true;
        }
    }

    fn record_completed_focus_session(&mut self) {
        let day_key = current_day_key();
        self.stats.record_completed_pomodoro(&day_key);
        self.stats_dirty = true;
    }

    fn sync_wakatime_tracking_for_state(&mut self) {
        let focus_running = self.focus_running_for_current_state();
        if focus_running && !self.wakatime.is_tracking() {
            self.wakatime.on_focus_start();
        } else if !focus_running && self.wakatime.is_tracking() {
            self.wakatime.on_focus_stop();
        }
    }

    fn set_block_error_from_result(&mut self, result: std::io::Result<()>) {
        match result {
            Ok(()) => self.block_error = None,
            Err(e) => self.block_error = Some(e.to_string()),
        }
    }

    fn set_site_feedback(&mut self, level: SiteFeedbackLevel, message: impl Into<String>) {
        self.site_feedback = Some(SiteFeedback {
            level,
            message: message.into(),
        });
    }

    fn rebuild_notifier(&mut self) {
        self.notifier = PhaseNotifier::new(self.notification_settings);
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

fn format_duration_label(seconds: u64) -> String {
    let minutes = seconds / 60;
    let remaining_seconds = seconds % 60;
    if remaining_seconds == 0 {
        format!("{minutes}m")
    } else {
        format!("{minutes}:{remaining_seconds:02}")
    }
}

fn bool_label(value: bool) -> &'static str {
    if value { "On" } else { "Off" }
}

fn format_count(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {plural}")
    }
}

fn summarize_invalid_inputs(invalid: &[InvalidSiteInput]) -> String {
    const PREVIEW_LIMIT: usize = 3;
    let preview_count = invalid.len().min(PREVIEW_LIMIT);
    let mut details = invalid
        .iter()
        .take(preview_count)
        .map(|entry| {
            format!(
                "`{}`: {}",
                display_input_value(&entry.input),
                entry.reason.message()
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    if invalid.len() > PREVIEW_LIMIT {
        details.push_str(&format!(", +{} more", invalid.len() - PREVIEW_LIMIT));
    }
    details
}

fn display_input_value(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        "<empty>".to_string()
    } else {
        trimmed.to_string()
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.flush_stats_if_dirty(true);
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

    fn ctrl_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    #[test]
    fn app_default_uses_canonical_config_in_tests() {
        let app = App::default();

        assert!(app.blocker.sites.is_empty());
        assert_eq!(app.timer.focus_secs, DEFAULT_FOCUS_SECS);
        assert_eq!(app.timer.short_break_secs, DEFAULT_SHORT_BREAK_SECS);
        assert_eq!(app.timer.long_break_secs, DEFAULT_LONG_BREAK_SECS);
        assert_eq!(app.timer.long_break_interval, DEFAULT_LONG_BREAK_INTERVAL);
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
            notifications: NotificationConfig::default(),
        };
        let app = App::from_config(config);
        assert_eq!(app.selected_profile, ProfileId::Classic);
        assert_eq!(app.timer.focus_secs, DEFAULT_FOCUS_SECS);
        assert_eq!(app.timer.short_break_secs, DEFAULT_SHORT_BREAK_SECS);
        assert_eq!(app.timer.long_break_secs, DEFAULT_LONG_BREAK_SECS);
        assert_eq!(app.timer.long_break_interval, DEFAULT_LONG_BREAK_INTERVAL);
    }

    #[test]
    fn deep_work_profile_reaches_long_break_on_third_focus() {
        let config = AppConfig {
            selected_profile: ProfileId::DeepWork,
            custom_profile: Some(CustomProfileConfig::default()),
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        assert_eq!(app.timer.long_break_interval, 3);

        for _ in 0..2 {
            app.timer.status = TimerStatus::Running;
            app.timer.remaining_secs = 1;
            app.on_tick(false); // focus -> short break
            assert_eq!(app.timer.phase, TimerPhase::ShortBreak);

            app.timer.status = TimerStatus::Running;
            app.timer.remaining_secs = 1;
            app.on_tick(false); // short break -> focus
            assert_eq!(app.timer.phase, TimerPhase::Focus);
        }

        app.timer.status = TimerStatus::Running;
        app.timer.remaining_secs = 1;
        app.on_tick(false); // third focus completion -> long break
        assert_eq!(app.timer.phase, TimerPhase::LongBreak);
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

    #[test]
    fn cancelling_custom_profile_edit_restores_original_values() {
        let custom = CustomProfileConfig {
            focus_secs: 30 * 60,
            short_break_secs: 6 * 60,
            long_break_secs: 12 * 60,
            long_break_interval: 3,
        };
        let config = AppConfig {
            selected_profile: ProfileId::Custom,
            custom_profile: Some(custom),
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        let original = app.custom_profile.clone();

        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Char('e')));
        app.handle_key(key(KeyCode::Right));
        assert_ne!(app.custom_profile.focus_secs, original.focus_secs);

        app.handle_key(key(KeyCode::Esc));

        assert!(!app.profile_edit_active);
        assert_eq!(app.custom_profile, original);
        assert_eq!(app.timer.focus_secs, original.focus_secs);
    }

    #[test]
    fn persisted_config_keeps_legacy_fields_from_custom_profile() {
        let custom = CustomProfileConfig {
            focus_secs: 35 * 60,
            short_break_secs: 7 * 60,
            long_break_secs: 14 * 60,
            long_break_interval: 5,
        };
        let config = AppConfig {
            selected_profile: ProfileId::DeepWork,
            custom_profile: Some(custom.clone()),
            ..AppConfig::default()
        };
        let app = App::from_config(config);

        let persisted = app.persisted_config();
        assert_eq!(persisted.selected_profile, ProfileId::DeepWork);
        assert_eq!(persisted.focus_secs, custom.focus_secs);
        assert_eq!(persisted.short_break_secs, custom.short_break_secs);
        assert_eq!(persisted.long_break_secs, custom.long_break_secs);
        assert_eq!(persisted.long_break_interval, custom.long_break_interval);
        assert_eq!(persisted.custom_profile, Some(custom));
        assert_eq!(persisted.notifications, NotificationConfig::default());
    }

    #[test]
    fn profile_summary_displays_seconds_when_not_minute_aligned() {
        let config = AppConfig {
            selected_profile: ProfileId::Custom,
            custom_profile: Some(CustomProfileConfig {
                focus_secs: 25 * 60 + 1,
                short_break_secs: 5 * 60 + 2,
                long_break_secs: 15 * 60 + 3,
                long_break_interval: 4,
            }),
            ..AppConfig::default()
        };
        let app = App::from_config(config);
        assert_eq!(
            app.profile_summary(ProfileId::Custom),
            "25:01/5:02/15:03 · every 4 focus"
        );
    }

    #[test]
    fn profile_edit_field_value_displays_second_precision() {
        let config = AppConfig {
            selected_profile: ProfileId::Custom,
            custom_profile: Some(CustomProfileConfig {
                focus_secs: 10 * 60 + 7,
                short_break_secs: 2 * 60,
                long_break_secs: 8 * 60 + 9,
                long_break_interval: 3,
            }),
            ..AppConfig::default()
        };
        let app = App::from_config(config);
        assert_eq!(app.profile_edit_field_value(0), "10:07");
        assert_eq!(app.profile_edit_field_value(1), "2m");
        assert_eq!(app.profile_edit_field_value(2), "8:09");
        assert_eq!(app.profile_edit_field_value(4), "On");
        assert_eq!(app.profile_edit_field_value(5), "Off");
    }

    #[test]
    fn profile_manager_edit_mode_available_for_non_custom_profile() {
        let config = AppConfig {
            selected_profile: ProfileId::Classic,
            notifications: NotificationConfig {
                enabled: false,
                sound: false,
            },
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);

        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Char('e')));

        assert!(app.profile_edit_active);
    }

    #[test]
    fn editing_notification_fields_updates_and_persists_settings() {
        let config = AppConfig {
            selected_profile: ProfileId::DeepWork,
            notifications: NotificationConfig {
                enabled: false,
                sound: false,
            },
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);

        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Char('e')));
        for _ in 0..4 {
            app.handle_key(key(KeyCode::Down));
        }
        app.handle_key(key(KeyCode::Right)); // notifications -> On
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Right)); // sound -> On
        app.handle_key(key(KeyCode::Enter));

        let persisted = app.persisted_config();
        assert!(persisted.notifications.enabled);
        assert!(persisted.notifications.sound);
    }

    #[test]
    fn cancelling_profile_edit_restores_notification_settings() {
        let config = AppConfig {
            selected_profile: ProfileId::Classic,
            notifications: NotificationConfig {
                enabled: true,
                sound: true,
            },
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);

        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Char('e')));
        for _ in 0..4 {
            app.handle_key(key(KeyCode::Down));
        }
        app.handle_key(key(KeyCode::Left)); // notifications -> Off
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Left)); // sound -> Off
        app.handle_key(key(KeyCode::Esc)); // cancel

        assert!(!app.profile_edit_active);
        assert!(app.notification_settings.enabled);
        assert!(app.notification_settings.sound);
    }

    #[test]
    fn timer_escape_quits_application() {
        let mut app = App::default();
        app.handle_key(key(KeyCode::Esc));
        assert!(app.should_quit);
    }

    #[test]
    fn site_manager_add_site_runs_unified_mutation_flow() {
        let mut app = App::default();
        app.handle_key(key(KeyCode::Char('b')));
        app.handle_key(key(KeyCode::Char('a')));
        for c in "Example.com".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));

        assert_eq!(app.mode, AppMode::SiteManager);
        assert!(!app.site_input_active);
        assert_eq!(app.blocker.sites, vec!["example.com"]);
        assert_eq!(app.selected_site, 0);
        assert_eq!(
            app.site_feedback,
            Some(SiteFeedback {
                level: SiteFeedbackLevel::Success,
                message: "Added 1 site".to_string(),
            })
        );
        assert!(app.config_error.is_none());
    }

    #[test]
    fn site_manager_delete_site_clamps_selection() {
        let config = AppConfig {
            blocked_sites: vec![
                "a.com".to_string(),
                "b.com".to_string(),
                "c.com".to_string(),
            ],
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.mode = AppMode::SiteManager;
        app.selected_site = 2;

        app.handle_key(key(KeyCode::Char('d')));

        assert_eq!(
            app.blocker.sites,
            vec!["a.com".to_string(), "b.com".to_string()]
        );
        assert_eq!(app.selected_site, 1);
        assert_eq!(
            app.site_feedback,
            Some(SiteFeedback {
                level: SiteFeedbackLevel::Success,
                message: "Removed `c.com` from blocklist".to_string(),
            })
        );
        assert!(app.config_error.is_none());
    }

    #[test]
    fn site_manager_bulk_add_via_paste_supports_comma_and_newline() {
        let mut app = App::default();
        app.handle_key(key(KeyCode::Char('b')));

        app.handle_paste("Example.com,\ngithub.com\nexam_ple.com".to_string());
        app.handle_key(key(KeyCode::Enter));

        assert_eq!(
            app.blocker.sites,
            vec!["example.com".to_string(), "github.com".to_string()]
        );
        let feedback = app
            .site_feedback
            .as_ref()
            .expect("bulk add should provide feedback");
        assert_eq!(feedback.level, SiteFeedbackLevel::Warning);
        assert!(feedback.message.contains("Added 2 sites"));
        assert!(feedback.message.contains("invalid hostname"));
    }

    #[test]
    fn site_manager_invalid_add_keeps_draft_open() {
        let mut app = App::default();
        app.handle_key(key(KeyCode::Char('b')));
        app.handle_key(key(KeyCode::Char('a')));
        for c in "exam_ple.com".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }

        app.handle_key(key(KeyCode::Enter));

        assert!(app.site_input_active);
        assert!(app.site_edit_index.is_none());
        assert_eq!(app.site_input, "exam_ple.com");
        assert!(app.blocker.sites.is_empty());
    }

    #[test]
    fn site_manager_edit_selected_site() {
        let config = AppConfig {
            blocked_sites: vec!["a.com".to_string(), "b.com".to_string()],
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);

        app.handle_key(key(KeyCode::Char('b')));
        app.handle_key(key(KeyCode::Char('e')));
        for _ in 0.."a.com".len() {
            app.handle_key(key(KeyCode::Backspace));
        }
        for c in "news.ycombinator.com".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));

        assert_eq!(
            app.blocker.sites,
            vec!["news.ycombinator.com".to_string(), "b.com".to_string()]
        );
        assert_eq!(
            app.site_feedback,
            Some(SiteFeedback {
                level: SiteFeedbackLevel::Success,
                message: "Updated `a.com` -> `news.ycombinator.com`".to_string(),
            })
        );
    }

    #[test]
    fn site_manager_invalid_edit_keeps_draft_open() {
        let config = AppConfig {
            blocked_sites: vec!["a.com".to_string(), "b.com".to_string()],
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.handle_key(key(KeyCode::Char('b')));
        app.handle_key(key(KeyCode::Char('e')));
        for _ in 0.."a.com".len() {
            app.handle_key(key(KeyCode::Backspace));
        }
        for c in "b.com".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }

        app.handle_key(key(KeyCode::Enter));

        assert!(app.site_input_active);
        assert_eq!(app.site_edit_index, Some(0));
        assert_eq!(app.site_input, "b.com");
        assert_eq!(
            app.blocker.sites,
            vec!["a.com".to_string(), "b.com".to_string()]
        );
    }

    #[test]
    fn site_manager_reapply_decision_uses_focus_state() {
        let mut app = App::default();
        assert!(!app.should_resync_blocking_after_site_mutation());

        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Running;
        assert!(app.should_resync_blocking_after_site_mutation());
    }

    #[test]
    fn completed_focus_tick_increments_session_pomodoros() {
        let mut app = App::default();
        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Running;
        app.timer.remaining_secs = 1;

        app.on_tick(false);

        assert_eq!(app.session_stats().pomodoros_completed, 1);
        assert_eq!(app.today_stats().pomodoros_completed, 1);
        assert_eq!(
            app.phase_notification.as_deref(),
            Some("Focus complete. Next up: Short Break.")
        );
    }

    #[test]
    fn skipping_focus_does_not_increment_session_pomodoros() {
        let mut app = App::default();
        assert_eq!(app.session_stats().pomodoros_completed, 0);

        app.handle_key(key(KeyCode::Char('n')));

        assert_eq!(app.session_stats().pomodoros_completed, 0);
        assert!(app.phase_notification.is_none());
    }

    #[test]
    fn focus_elapsed_accumulates_session_and_today_minutes() {
        let mut app = App::default();
        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Running;
        app.timer.remaining_secs = app.timer.focus_secs;

        for _ in 0..120 {
            app.on_tick(false);
        }

        assert_eq!(app.session_stats().focused_minutes(), 2);
        assert_eq!(app.today_stats().focused_minutes(), 2);
    }

    #[test]
    fn history_view_toggles_from_timer_mode() {
        let mut app = App::default();

        app.handle_key(key(KeyCode::Char('h')));
        assert_eq!(app.mode, AppMode::StatsHistory);

        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.mode, AppMode::Timer);
    }

    #[test]
    fn catchup_tick_does_not_increment_focus_stats() {
        let mut app = App::default();
        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Running;
        app.timer.remaining_secs = 1;

        app.on_tick(true);

        assert_eq!(app.timer.phase, TimerPhase::ShortBreak);
        assert_eq!(app.session_stats().pomodoros_completed, 0);
        assert_eq!(app.session_stats().focused_seconds, 0);
        assert!(app.phase_notification.is_none());
    }

    #[test]
    fn partial_focus_elapsed_marks_unsaved_flag_for_drop_flush() {
        let mut app = App::default();
        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Running;
        app.timer.remaining_secs = app.timer.focus_secs;

        app.on_tick(false);

        assert!(app.stats_has_unsaved_elapsed);
        assert_eq!(app.session_stats().focused_seconds, 1);
    }

    #[test]
    fn ctrl_c_quits_during_profile_edit() {
        let mut app = App::default();
        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Char('e')));
        app.handle_key(ctrl_key(KeyCode::Char('c')));
        assert!(app.should_quit);
    }

    #[test]
    fn poll_wakatime_status_applies_async_failure_event() {
        let mut app = App::default();
        app.wakatime = WakatimeTracker::new_configured_for_tests();
        app.wakatime.push_failed_event_for_tests("HTTP 503");

        app.poll_wakatime_status();

        assert_eq!(
            app.wakatime.runtime_state(),
            crate::wakatime::WakatimeRuntimeState::Error("HTTP 503".to_string())
        );
    }
}
