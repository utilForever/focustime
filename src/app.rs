use std::{
    collections::{BTreeSet, HashSet},
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use chrono::{DateTime, Local, NaiveDate};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::blocker::{
    BulkAddResult, EditSiteResult, HostsFileDiagnostics, InvalidSiteInput, SiteBlocker,
};
use crate::config::{
    AppConfig, AutoStartConfig, BlocklistProfileConfig, CustomProfileConfig, DailyGoalConfig,
    NotificationConfig, ProfileId, RecurringFocusWindowConfig, RecurringScheduleConfig,
    WakatimeMetadataConfig,
};
use crate::notifications::PhaseNotifier;
use crate::schedule::{
    RecurringWindow, WindowOccurrence, active_occurrence, compile_exception_dates, compile_windows,
    next_occurrence_after, occurrence_key,
};
use crate::session_recovery::{self, InProgressSessionSnapshot};
use crate::stats::{
    BreakGlassOverrideEvent, DailyGoalSnapshot, DailyStats, ExportedStatsFiles, FocusStats,
    GoalStreak, MonthlyHeatmap, MonthlyStats, ProfileTotals, SessionStats, TaskTotals, TaskTrend,
    WeeklyStats, current_day_key,
};
use crate::task_labels::{normalize_task_label, task_label_index};
use crate::timer::{
    DEFAULT_FOCUS_SECS, DEFAULT_LONG_BREAK_INTERVAL, DEFAULT_LONG_BREAK_SECS,
    DEFAULT_SHORT_BREAK_SECS, TimerPhase, TimerState, TimerStatus,
};
use crate::wakatime::{WakatimeConfigStatus, WakatimeHeartbeatMetadata, WakatimeTracker};

pub const PROFILE_IDS: [ProfileId; 3] =
    [ProfileId::Classic, ProfileId::DeepWork, ProfileId::Custom];
pub const PROFILE_EDIT_FIELD_LABELS: [&str; 22] = [
    "Focus",
    "Short Break",
    "Long Break",
    "Long-break cadence",
    "Phase notifications",
    "Sound alert",
    "Auto-start break",
    "Auto-start focus",
    "Strict focus mode",
    "Goal minutes",
    "Goal pomodoros",
    "WakaTime project",
    "WakaTime language",
    "Schedule window",
    "Schedule day",
    "Schedule day enabled",
    "Schedule start",
    "Schedule end",
    "Schedule add/remove",
    "Schedule exception",
    "Exception date",
    "Exception add/remove",
];
const PROFILE_EDIT_WAKATIME_PROJECT_INDEX: usize = 11;
const PROFILE_EDIT_WAKATIME_LANGUAGE_INDEX: usize = 12;
const PROFILE_EDIT_SCHEDULE_WINDOW_INDEX: usize = 13;
const PROFILE_EDIT_SCHEDULE_DAY_INDEX: usize = 14;
const PROFILE_EDIT_SCHEDULE_DAY_ENABLED_INDEX: usize = 15;
const PROFILE_EDIT_SCHEDULE_START_INDEX: usize = 16;
const PROFILE_EDIT_SCHEDULE_END_INDEX: usize = 17;
const PROFILE_EDIT_SCHEDULE_ADD_REMOVE_INDEX: usize = 18;
const PROFILE_EDIT_SCHEDULE_EXCEPTION_INDEX: usize = 19;
const PROFILE_EDIT_SCHEDULE_EXCEPTION_DATE_INDEX: usize = 20;
const PROFILE_EDIT_SCHEDULE_EXCEPTION_ADD_REMOVE_INDEX: usize = 21;
const CUSTOM_DURATION_STEP_SECS: u64 = 60;
const DAILY_GOAL_MINUTES_STEP: u64 = 5;
const DEFAULT_BLOCKLIST_PROFILE_NAME: &str = "Default";
pub(crate) const PLANNER_RECENT_LABEL_LIMIT: usize = 5;
const SCHEDULE_TIME_STEP_MINUTES: u16 = 15;
const SCHEDULE_DAY_TOKENS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
const SCHEDULE_DAY_LABELS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Timer,
    SiteManager,
    ProfileManager,
    SessionPlanner,
    StatsHistory,
    SetupDiagnostics,
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

fn blocklist_profile_index(profiles: &[BlocklistProfileConfig], selected_name: &str) -> usize {
    profiles
        .iter()
        .position(|profile| profile.name.eq_ignore_ascii_case(selected_name))
        .unwrap_or(0)
}

#[derive(Debug, Clone)]
struct ProfileEditSnapshot {
    custom_profile: CustomProfileConfig,
    notification_settings: NotificationConfig,
    auto_start: AutoStartConfig,
    recurring_schedule: RecurringScheduleConfig,
    strict_mode: bool,
    daily_goal: DailyGoalConfig,
    wakatime_metadata: WakatimeMetadataConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingTimerAction {
    Reset,
    BreakGlassOverride,
}

#[derive(Debug, Clone)]
struct ScheduleDisplayState {
    has_schedule_windows: bool,
    active_window: Option<WindowOccurrence>,
    next_window: Option<WindowOccurrence>,
    is_exception_today: bool,
    is_armed: bool,
    has_selected_task: bool,
    timer_phase: TimerPhase,
    timer_status: TimerStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteInputMode {
    Add,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlocklistProfileInputMode {
    Create,
    Rename,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlannerInputMode {
    Add,
    Rename,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteFeedbackLevel {
    Success,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryFeedbackLevel {
    Success,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlannerFeedbackLevel {
    Success,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteFeedback {
    pub level: SiteFeedbackLevel,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannerFeedback {
    pub level: PlannerFeedbackLevel,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryFeedback {
    pub level: HistoryFeedbackLevel,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GoalProgress {
    pub completed: u64,
    pub target: u64,
    pub ratio: f64,
}

impl GoalProgress {
    pub fn is_configured(self) -> bool {
        self.target > 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DailyGoalProgress {
    pub minutes: GoalProgress,
    pub pomodoros: GoalProgress,
}

impl DailyGoalProgress {
    pub fn has_any_target(self) -> bool {
        self.minutes.is_configured() || self.pomodoros.is_configured()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupCheckLevel {
    Ok,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupCheck {
    pub level: SetupCheckLevel,
    pub message: String,
}

impl SetupCheck {
    fn ok(message: impl Into<String>) -> Self {
        Self {
            level: SetupCheckLevel::Ok,
            message: message.into(),
        }
    }

    fn warning(message: impl Into<String>) -> Self {
        Self {
            level: SetupCheckLevel::Warning,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupDiagnostics {
    pub hosts_file_path: String,
    pub blocking_permissions: SetupCheck,
    pub hosts_write_capability: SetupCheck,
    pub wakatime_config: SetupCheck,
}

impl SetupDiagnostics {
    fn collect(blocker: &SiteBlocker) -> Self {
        let hosts_diagnostics = blocker.hosts_file_diagnostics();
        let blocking_permissions = blocking_permissions_check(&hosts_diagnostics);
        let hosts_write_capability = hosts_write_capability_check(&hosts_diagnostics);
        let hosts_file_path = hosts_diagnostics.path.clone();
        let wakatime_diagnostics = WakatimeTracker::config_diagnostics();
        let wakatime_config = match wakatime_diagnostics.status {
            WakatimeConfigStatus::Configured => SetupCheck::ok(wakatime_diagnostics.detail),
            WakatimeConfigStatus::MissingConfigFile
            | WakatimeConfigStatus::MissingApiKey
            | WakatimeConfigStatus::UnreadableConfig
            | WakatimeConfigStatus::HomeDirectoryUnavailable => {
                SetupCheck::warning(wakatime_diagnostics.detail)
            }
        };
        Self {
            hosts_file_path,
            blocking_permissions,
            hosts_write_capability,
            wakatime_config,
        }
    }
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
    pub blocklist_profiles: Vec<BlocklistProfileConfig>,
    active_blocklist_profile: usize,
    pub blocklist_profile_input: String,
    pub blocklist_profile_input_active: bool,
    blocklist_profile_input_mode: Option<BlocklistProfileInputMode>,
    pub site_feedback: Option<SiteFeedback>,
    pub task_labels: Vec<String>,
    pub selected_task_label: Option<String>,
    pub planner_selection_index: usize,
    pub planner_input: String,
    pub planner_input_active: bool,
    pub planner_input_mode: Option<PlannerInputMode>,
    pub planner_feedback: Option<PlannerFeedback>,
    active_focus_task_label: Option<String>,
    active_focus_profile: Option<ProfileId>,
    /// Index of the highlighted site in the SiteManager list.
    pub selected_site: usize,
    /// Last error from a block/unblock operation (e.g. permission denied).
    pub block_error: Option<String>,
    pub setup_diagnostics: SetupDiagnostics,
    /// Last error from persisting timer/site configuration.
    pub config_error: Option<String>,
    /// Last error from persisting focus stats.
    pub stats_error: Option<String>,
    pub history_feedback: Option<HistoryFeedback>,
    pub phase_notification: Option<String>,
    pub wakatime: WakatimeTracker,
    pub selected_profile: ProfileId,
    pub custom_profile: CustomProfileConfig,
    pub profile_selection_index: usize,
    pub profile_edit_active: bool,
    pub profile_edit_field: usize,
    profile_edit_schedule_window: usize,
    profile_edit_schedule_day: usize,
    profile_edit_schedule_exception: usize,
    profile_edit_snapshot: Option<ProfileEditSnapshot>,
    notification_settings: NotificationConfig,
    auto_start: AutoStartConfig,
    recurring_schedule: RecurringScheduleConfig,
    recurring_windows: Vec<RecurringWindow>,
    recurring_exception_dates: HashSet<NaiveDate>,
    schedule_armed_occurrence_key: Option<String>,
    last_schedule_occurrence_key: Option<String>,
    current_frame_now: DateTime<Local>,
    pub strict_mode: bool,
    break_glass_duration_secs: u64,
    break_glass_expires_at: Option<Instant>,
    daily_goal: DailyGoalConfig,
    wakatime_metadata: WakatimeMetadataConfig,
    pending_timer_action: Option<PendingTimerAction>,
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
        let config = config.normalized();
        let selected_profile = config.selected_profile;
        let custom_profile = config.effective_custom_profile();
        let notification_settings = config.notifications;
        let auto_start = config.auto_start;
        let recurring_schedule = config.recurring_schedule.clone();
        let recurring_windows = compile_windows(&recurring_schedule.windows);
        let recurring_exception_dates =
            compile_exception_dates(&recurring_schedule.exception_dates);
        let strict_mode = config.strict_mode;
        let break_glass_duration_secs = config.break_glass_duration_secs;
        let daily_goal = config.daily_goal;
        let wakatime_metadata = config.wakatime;
        let blocklist_profiles = config.blocklist_profiles.clone();
        let active_blocklist_profile =
            blocklist_profile_index(&blocklist_profiles, &config.selected_blocklist_profile);
        let profile_spec = profile_spec_for(selected_profile, &custom_profile);
        let (stats, stats_error) = match FocusStats::load() {
            Ok(stats) => (stats, None),
            Err(e) => (FocusStats::default(), Some(e)),
        };
        let (task_labels, selected_task_label) = stats.task_planner_state();
        let planner_selection_index = selected_task_label
            .as_ref()
            .and_then(|label| task_label_index(&task_labels, label))
            .unwrap_or(0);
        let timer = TimerState::with_profile(
            profile_spec.focus_secs,
            profile_spec.short_break_secs,
            profile_spec.long_break_secs,
            profile_spec.long_break_interval,
        );
        let mut blocker = SiteBlocker::new();
        for site in &blocklist_profiles[active_blocklist_profile].sites {
            blocker.add_site(site.clone());
        }
        let setup_diagnostics = SetupDiagnostics::collect(&blocker);
        let mut app = Self {
            timer,
            should_quit: false,
            mode: AppMode::Timer,
            blocker,
            site_input: String::new(),
            site_input_active: false,
            site_edit_index: None,
            blocklist_profiles,
            active_blocklist_profile,
            blocklist_profile_input: String::new(),
            blocklist_profile_input_active: false,
            blocklist_profile_input_mode: None,
            site_feedback: None,
            task_labels,
            selected_task_label,
            planner_selection_index,
            planner_input: String::new(),
            planner_input_active: false,
            planner_input_mode: None,
            planner_feedback: None,
            active_focus_task_label: None,
            active_focus_profile: None,
            selected_site: 0,
            block_error: None,
            setup_diagnostics,
            config_error: None,
            stats_error,
            history_feedback: None,
            phase_notification: None,
            wakatime: WakatimeTracker::new_with_metadata(WakatimeHeartbeatMetadata {
                project: wakatime_metadata.project.clone(),
                language: wakatime_metadata.language.clone(),
            }),
            selected_profile,
            custom_profile,
            profile_selection_index: profile_index(selected_profile),
            profile_edit_active: false,
            profile_edit_field: 0,
            profile_edit_schedule_window: 0,
            profile_edit_schedule_day: 0,
            profile_edit_schedule_exception: 0,
            profile_edit_snapshot: None,
            notification_settings,
            auto_start,
            recurring_schedule,
            recurring_windows,
            recurring_exception_dates,
            schedule_armed_occurrence_key: None,
            last_schedule_occurrence_key: None,
            current_frame_now: Local::now(),
            strict_mode,
            break_glass_duration_secs,
            break_glass_expires_at: None,
            daily_goal,
            wakatime_metadata,
            pending_timer_action: None,
            notifier: PhaseNotifier::new(notification_settings),
            stats,
            stats_dirty: false,
            stats_has_unsaved_elapsed: false,
        };
        app.restore_in_progress_session();
        app.sync_recovery_snapshot();
        app
    }

    pub fn on_tick(&mut self, is_catchup: bool) {
        let completed_phase = self.timer.phase;
        let completed_focus_secs = self.timer.focus_secs;
        if self.should_record_focus_elapsed(is_catchup) {
            self.record_focus_elapsed(1);
        }

        let phase_changed = self.timer.tick();
        if phase_changed {
            self.handle_phase_change(completed_phase, completed_focus_secs, is_catchup);
        }
        self.sync_recovery_snapshot();
        self.flush_stats_if_dirty(false);
    }

    fn should_record_focus_elapsed(&self, is_catchup: bool) -> bool {
        !is_catchup && self.focus_running_for_current_state()
    }

    fn should_record_completed_focus_session(
        &self,
        completed_phase: TimerPhase,
        is_catchup: bool,
    ) -> bool {
        !is_catchup && completed_phase == TimerPhase::Focus && self.timer.phase != TimerPhase::Focus
    }

    fn should_block_focus_autostart(&self) -> bool {
        self.timer.phase == TimerPhase::Focus && self.selected_task_label.is_none()
    }

    fn handle_phase_change(
        &mut self,
        completed_phase: TimerPhase,
        completed_focus_secs: u64,
        is_catchup: bool,
    ) {
        self.pending_timer_action = None;
        if self.should_record_completed_focus_session(completed_phase, is_catchup) {
            self.record_completed_focus_session(completed_focus_secs);
            self.active_focus_task_label = None;
            self.active_focus_profile = None;
        }

        let blocked_focus_autostart =
            self.apply_auto_start_after_phase_change(completed_phase, is_catchup);
        self.update_phase_notification_after_phase_change(
            completed_phase,
            is_catchup,
            blocked_focus_autostart,
        );

        if self.timer.phase != TimerPhase::Focus {
            self.active_focus_task_label = None;
            self.active_focus_profile = None;
            self.break_glass_expires_at = None;
        }
        self.apply_blocking_for_phase();
    }

    fn apply_auto_start_after_phase_change(
        &mut self,
        completed_phase: TimerPhase,
        is_catchup: bool,
    ) -> bool {
        if is_catchup || !self.should_auto_start_transition(completed_phase, self.timer.phase) {
            return false;
        }

        if self.should_block_focus_autostart() {
            return true;
        }

        self.timer.status = TimerStatus::Running;
        if self.timer.phase == TimerPhase::Focus {
            self.active_focus_task_label = self.selected_task_label.clone();
            self.active_focus_profile = Some(self.selected_profile);
        }
        false
    }

    fn update_phase_notification_after_phase_change(
        &mut self,
        completed_phase: TimerPhase,
        is_catchup: bool,
        blocked_focus_autostart: bool,
    ) {
        if !is_catchup {
            self.phase_notification = self
                .notifier
                .notify_phase_completion(completed_phase, self.timer.phase);
        }
        if blocked_focus_autostart {
            self.phase_notification =
                Some("Select a task label with [t] before starting focus.".to_string());
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

    /// Applies any completed async WakaTime heartbeat results to tracker state.
    /// Intended to be called once per UI frame.
    pub fn poll_wakatime_status(&mut self) {
        let now = Local::now();
        self.current_frame_now = now;
        self.wakatime.poll_events();
        self.sync_break_glass_override();
        self.sync_recurring_schedule(now);
    }

    pub fn start_focus_for_cli(&mut self) -> Result<(), String> {
        if self.timer.phase != TimerPhase::Focus || self.timer.status != TimerStatus::Idle {
            return Err("Cannot start focus: timer is not idle in focus phase.".to_string());
        }
        if self.selected_task_label.is_none() {
            return Err(
                "Cannot start focus: select a task label first (run TUI and press [t])."
                    .to_string(),
            );
        }
        self.update_timer_and_sync(TimerState::toggle_pause);
        Ok(())
    }

    pub fn pause_for_cli(&mut self) -> Result<(), String> {
        if self.timer.status != TimerStatus::Running {
            return Err("Cannot pause: timer is not running.".to_string());
        }
        self.update_timer_and_sync(TimerState::toggle_pause);
        Ok(())
    }

    pub fn resume_for_cli(&mut self) -> Result<(), String> {
        if self.timer.status != TimerStatus::Paused {
            return Err("Cannot resume: timer is not paused.".to_string());
        }
        self.update_timer_and_sync(TimerState::toggle_pause);
        Ok(())
    }

    pub fn stop_for_cli(&mut self) -> Result<(), String> {
        if self.strict_mode_enforced_for_focus() {
            return Err("Cannot stop: strict mode is active during focus.".to_string());
        }
        if self.timer.status == TimerStatus::Idle {
            return Err("Cannot stop: timer is already idle.".to_string());
        }
        self.update_timer_and_sync(TimerState::reset);
        Ok(())
    }

    pub fn next_phase_for_cli(&mut self) -> Result<(), String> {
        if self.strict_mode_enforced_for_focus() {
            return Err(
                "Cannot skip to next phase: strict mode is active during focus.".to_string(),
            );
        }
        self.update_timer_and_sync(TimerState::next_phase);
        Ok(())
    }

    pub fn select_task_label_for_cli(&mut self, label: &str) -> Result<bool, String> {
        let Some(label) = normalize_task_label(label) else {
            return Err("Cannot select task label: label cannot be empty.".to_string());
        };

        if let Some(existing_index) = task_label_index(&self.task_labels, &label) {
            self.planner_selection_index = existing_index;
            self.selected_task_label = self.task_labels.get(existing_index).cloned();
            self.sync_task_planner_state();
            self.sync_recovery_snapshot();
            return Ok(false);
        }

        self.task_labels.push(label.clone());
        self.planner_selection_index = self.task_labels.len().saturating_sub(1);
        self.selected_task_label = Some(label);
        self.sync_task_planner_state();
        self.sync_recovery_snapshot();
        Ok(true)
    }

    pub fn selected_profile_id(&self) -> ProfileId {
        self.selected_profile
    }

    pub fn selected_task_label_for_cli(&self) -> Option<String> {
        self.selected_task_label.clone()
    }

    pub fn planner_recent_labels(&self, limit: usize) -> Vec<String> {
        if limit == 0 || self.task_labels.is_empty() {
            return Vec::new();
        }

        let mut source_limit = limit;
        loop {
            let source = self.stats.recent_task_labels(source_limit);
            let exhausted = source.len() < source_limit;
            let mut recent = Vec::new();
            let mut seen = BTreeSet::new();

            for label in source {
                let Some(existing_index) = task_label_index(&self.task_labels, &label) else {
                    continue;
                };
                let canonical = self.task_labels[existing_index].clone();
                let key = canonical.to_ascii_lowercase();
                if !seen.insert(key) {
                    continue;
                }
                recent.push(canonical);
                if recent.len() >= limit {
                    return recent;
                }
            }

            if exhausted {
                return recent;
            }

            let next_limit = source_limit.saturating_mul(2);
            if next_limit == source_limit {
                return recent;
            }
            source_limit = next_limit;
        }
    }

    pub fn timer_state_for_cli(&self) -> (TimerPhase, TimerStatus, u64, u32) {
        (
            self.timer.phase,
            self.timer.status,
            self.timer.remaining_secs,
            self.timer.pomodoros_completed,
        )
    }

    pub fn selected_profile_name(&self) -> &'static str {
        self.selected_profile.label()
    }

    pub fn recurring_schedule_display_texts(&self) -> (String, String) {
        self.recurring_schedule_texts_at(self.current_frame_now)
    }

    fn recurring_schedule_texts_at(&self, now: DateTime<Local>) -> (String, String) {
        let state = self.schedule_display_state_at(now);
        (
            schedule_next_window_text_from_state(&state, now),
            schedule_status_text_from_state(&state),
        )
    }

    fn schedule_display_state_at(&self, now: DateTime<Local>) -> ScheduleDisplayState {
        let today = now.date_naive();
        ScheduleDisplayState {
            has_schedule_windows: !self.recurring_windows.is_empty(),
            active_window: active_occurrence(
                now,
                &self.recurring_windows,
                &self.recurring_exception_dates,
            ),
            next_window: next_occurrence_after(
                now,
                &self.recurring_windows,
                &self.recurring_exception_dates,
            ),
            is_exception_today: self.recurring_exception_dates.contains(&today),
            is_armed: self.schedule_armed_occurrence_key.is_some(),
            has_selected_task: self.selected_task_label.is_some(),
            timer_phase: self.timer.phase,
            timer_status: self.timer.status,
        }
    }

    pub fn current_task_label(&self) -> Option<&str> {
        if self.focus_session_active_for_current_state() {
            self.active_focus_task_label
                .as_deref()
                .or(self.selected_task_label.as_deref())
        } else {
            self.selected_task_label.as_deref()
        }
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

    pub fn today_goal_progress(&self) -> DailyGoalProgress {
        self.daily_goal_progress_for(self.today_stats())
    }

    pub fn goal_streak(&self) -> GoalStreak {
        self.goal_streak_for_day_key(&current_day_key())
    }

    fn goal_streak_for_day_key(&self, day_key: &str) -> GoalStreak {
        let Some(day) = parse_day_key(day_key) else {
            return GoalStreak::default();
        };

        self.stats.goal_streak(
            day,
            self.current_goal_snapshot(),
            self.stats.daily_for(day_key),
        )
    }

    pub fn daily_goal_progress_for(&self, stats: DailyStats) -> DailyGoalProgress {
        DailyGoalProgress {
            minutes: goal_progress(stats.focused_minutes(), self.daily_goal.minutes),
            pomodoros: goal_progress(
                u64::from(stats.pomodoros_completed),
                u64::from(self.daily_goal.pomodoros),
            ),
        }
    }

    pub fn recent_daily_stats(&self, limit: usize) -> Vec<(String, DailyStats)> {
        self.stats.recent_daily(limit)
    }

    #[allow(dead_code)]
    pub fn recent_weekly_stats(&self, limit: usize) -> Vec<WeeklyStats> {
        self.stats.recent_weekly(limit)
    }

    pub fn recent_monthly_stats(&self, limit: usize) -> Vec<MonthlyStats> {
        self.stats.recent_monthly(limit)
    }

    pub fn latest_monthly_heatmap(&self) -> MonthlyHeatmap {
        self.stats.latest_monthly_heatmap()
    }

    pub fn profile_focus_totals(&self) -> Vec<ProfileTotals> {
        self.stats.profile_totals()
    }

    pub fn task_focus_totals(&self, limit: usize) -> Vec<TaskTotals> {
        self.stats.task_totals(limit)
    }

    pub fn recent_task_trends(&self, limit: usize) -> Vec<TaskTrend> {
        self.stats.recent_task_trends(limit)
    }

    #[cfg(test)]
    pub fn insert_daily_stats_for_tests(&mut self, day_key: &str, stats: DailyStats) {
        self.stats.insert_daily_for_tests(day_key, stats);
    }

    pub fn profile_edit_field_value(&self, field_index: usize) -> String {
        if (PROFILE_EDIT_SCHEDULE_WINDOW_INDEX..=PROFILE_EDIT_SCHEDULE_EXCEPTION_ADD_REMOVE_INDEX)
            .contains(&field_index)
        {
            return self.profile_edit_schedule_field_value(field_index);
        }

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
            6 => bool_label(self.auto_start.focus_to_break).to_string(),
            7 => bool_label(self.auto_start.break_to_focus).to_string(),
            8 => bool_label(self.strict_mode).to_string(),
            9 => format_daily_goal_minutes_label(self.daily_goal.minutes),
            10 => format_daily_goal_pomodoros_label(self.daily_goal.pomodoros),
            PROFILE_EDIT_WAKATIME_PROJECT_INDEX => self.wakatime_metadata.project.clone(),
            PROFILE_EDIT_WAKATIME_LANGUAGE_INDEX => self.wakatime_metadata.language.clone(),
            _ => String::new(),
        }
    }

    fn profile_edit_schedule_field_value(&self, field_index: usize) -> String {
        match field_index {
            PROFILE_EDIT_SCHEDULE_WINDOW_INDEX => {
                if self.recurring_schedule.windows.is_empty() {
                    "none".to_string()
                } else {
                    format!(
                        "{}/{}",
                        self.profile_edit_schedule_window.saturating_add(1),
                        self.recurring_schedule.windows.len()
                    )
                }
            }
            PROFILE_EDIT_SCHEDULE_DAY_INDEX => {
                if let Some(window) = self.selected_schedule_window() {
                    let day_label = self.selected_schedule_day_label();
                    let days = format_schedule_days_for_display(&window.days);
                    format!("{day_label} ({days})")
                } else {
                    "n/a".to_string()
                }
            }
            PROFILE_EDIT_SCHEDULE_DAY_ENABLED_INDEX => self
                .selected_schedule_day_enabled()
                .map(|enabled| bool_label(enabled).to_string())
                .unwrap_or_else(|| "n/a".to_string()),
            PROFILE_EDIT_SCHEDULE_START_INDEX => self
                .selected_schedule_window()
                .map(|window| window.start.clone())
                .unwrap_or_else(|| "n/a".to_string()),
            PROFILE_EDIT_SCHEDULE_END_INDEX => self
                .selected_schedule_window()
                .map(|window| window.end.clone())
                .unwrap_or_else(|| "n/a".to_string()),
            PROFILE_EDIT_SCHEDULE_ADD_REMOVE_INDEX => {
                if self.recurring_schedule.windows.is_empty() {
                    "→ Add window".to_string()
                } else {
                    "← Remove · → Add".to_string()
                }
            }
            PROFILE_EDIT_SCHEDULE_EXCEPTION_INDEX => {
                if self.recurring_schedule.exception_dates.is_empty() {
                    "none".to_string()
                } else {
                    format!(
                        "{}/{}",
                        self.profile_edit_schedule_exception.saturating_add(1),
                        self.recurring_schedule.exception_dates.len()
                    )
                }
            }
            PROFILE_EDIT_SCHEDULE_EXCEPTION_DATE_INDEX => self
                .selected_schedule_exception_date()
                .cloned()
                .unwrap_or_else(|| "n/a".to_string()),
            PROFILE_EDIT_SCHEDULE_EXCEPTION_ADD_REMOVE_INDEX => {
                if self.recurring_schedule.exception_dates.is_empty() {
                    "→ Add date".to_string()
                } else {
                    "← Remove · → Add".to_string()
                }
            }
            _ => String::new(),
        }
    }

    fn selected_schedule_window(&self) -> Option<&RecurringFocusWindowConfig> {
        self.recurring_schedule
            .windows
            .get(self.profile_edit_schedule_window)
    }

    fn selected_schedule_window_mut(&mut self) -> Option<&mut RecurringFocusWindowConfig> {
        self.recurring_schedule
            .windows
            .get_mut(self.profile_edit_schedule_window)
    }

    fn selected_schedule_exception_date(&self) -> Option<&String> {
        self.recurring_schedule
            .exception_dates
            .get(self.profile_edit_schedule_exception)
    }

    fn selected_schedule_day_token(&self) -> &'static str {
        SCHEDULE_DAY_TOKENS[self
            .profile_edit_schedule_day
            .min(SCHEDULE_DAY_TOKENS.len() - 1)]
    }

    fn selected_schedule_day_label(&self) -> &'static str {
        SCHEDULE_DAY_LABELS[self
            .profile_edit_schedule_day
            .min(SCHEDULE_DAY_LABELS.len() - 1)]
    }

    fn selected_schedule_day_enabled(&self) -> Option<bool> {
        let day = self.selected_schedule_day_token();
        self.selected_schedule_window().map(|window| {
            window
                .days
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(day))
        })
    }

    fn clamp_profile_edit_schedule_selection(&mut self) {
        if self.recurring_schedule.windows.is_empty() {
            self.profile_edit_schedule_window = 0;
        } else {
            self.profile_edit_schedule_window = self
                .profile_edit_schedule_window
                .min(self.recurring_schedule.windows.len().saturating_sub(1));
        }
        self.profile_edit_schedule_day = self
            .profile_edit_schedule_day
            .min(SCHEDULE_DAY_TOKENS.len().saturating_sub(1));
        if self.recurring_schedule.exception_dates.is_empty() {
            self.profile_edit_schedule_exception = 0;
        } else {
            self.profile_edit_schedule_exception = self.profile_edit_schedule_exception.min(
                self.recurring_schedule
                    .exception_dates
                    .len()
                    .saturating_sub(1),
            );
        }
    }

    fn cycle_schedule_window(&mut self, increase: bool) {
        if self.recurring_schedule.windows.is_empty() {
            return;
        }
        let total = self.recurring_schedule.windows.len();
        if increase {
            self.profile_edit_schedule_window = (self.profile_edit_schedule_window + 1) % total;
        } else if self.profile_edit_schedule_window == 0 {
            self.profile_edit_schedule_window = total - 1;
        } else {
            self.profile_edit_schedule_window = self.profile_edit_schedule_window.saturating_sub(1);
        }
    }

    fn cycle_schedule_day(&mut self, increase: bool) {
        let total = SCHEDULE_DAY_TOKENS.len();
        if increase {
            self.profile_edit_schedule_day = (self.profile_edit_schedule_day + 1) % total;
        } else if self.profile_edit_schedule_day == 0 {
            self.profile_edit_schedule_day = total - 1;
        } else {
            self.profile_edit_schedule_day = self.profile_edit_schedule_day.saturating_sub(1);
        }
    }

    fn cycle_schedule_exception(&mut self, increase: bool) {
        if self.recurring_schedule.exception_dates.is_empty() {
            return;
        }
        let total = self.recurring_schedule.exception_dates.len();
        if increase {
            self.profile_edit_schedule_exception =
                (self.profile_edit_schedule_exception + 1) % total;
        } else if self.profile_edit_schedule_exception == 0 {
            self.profile_edit_schedule_exception = total - 1;
        } else {
            self.profile_edit_schedule_exception =
                self.profile_edit_schedule_exception.saturating_sub(1);
        }
    }

    fn set_schedule_day_enabled(&mut self, enabled: bool) {
        let selected_day = self.selected_schedule_day_token().to_string();
        let Some(window) = self.selected_schedule_window_mut() else {
            return;
        };

        let currently_enabled = window
            .days
            .iter()
            .any(|day| day.eq_ignore_ascii_case(&selected_day));
        if enabled == currently_enabled {
            return;
        }

        if enabled {
            window.days.push(selected_day);
            sort_schedule_days(&mut window.days);
            return;
        }

        if window.days.len() <= 1 {
            return;
        }
        window
            .days
            .retain(|day| !day.eq_ignore_ascii_case(&selected_day));
        sort_schedule_days(&mut window.days);
    }

    fn adjust_selected_schedule_time(&mut self, is_start: bool, increase: bool) {
        let Some(window) = self.selected_schedule_window_mut() else {
            return;
        };

        let mut start = parse_hhmm_minutes(&window.start).unwrap_or(9 * 60);
        let mut end = parse_hhmm_minutes(&window.end).unwrap_or(10 * 60);
        if end <= start {
            end = start.saturating_add(1).min(23 * 60 + 59);
        }

        if is_start {
            if increase {
                start = start
                    .saturating_add(SCHEDULE_TIME_STEP_MINUTES)
                    .min(end.saturating_sub(1));
            } else {
                start = start.saturating_sub(SCHEDULE_TIME_STEP_MINUTES);
            }
        } else if increase {
            end = end
                .saturating_add(SCHEDULE_TIME_STEP_MINUTES)
                .min(23 * 60 + 59)
                .max(start.saturating_add(1));
        } else {
            end = end
                .saturating_sub(SCHEDULE_TIME_STEP_MINUTES)
                .max(start.saturating_add(1));
        }

        window.start = format_hhmm(start);
        window.end = format_hhmm(end);
    }

    fn adjust_selected_schedule_exception_date(&mut self, increase: bool) {
        let Some(current_value) = self.selected_schedule_exception_date().cloned() else {
            return;
        };
        let Some(current_date) = parse_schedule_exception_date(&current_value) else {
            return;
        };
        let next_date = if increase {
            current_date.succ_opt().unwrap_or(current_date)
        } else {
            current_date.pred_opt().unwrap_or(current_date)
        };
        let next_value = next_date.format("%Y-%m-%d").to_string();
        if let Some(target) = self
            .recurring_schedule
            .exception_dates
            .get_mut(self.profile_edit_schedule_exception)
        {
            *target = next_value.clone();
        }
        sort_schedule_exception_dates(&mut self.recurring_schedule.exception_dates);
        if let Some(position) = self
            .recurring_schedule
            .exception_dates
            .iter()
            .position(|value| value == &next_value)
        {
            self.profile_edit_schedule_exception = position;
        }
    }

    fn adjust_schedule_windows_collection(&mut self, increase: bool) {
        if increase {
            self.recurring_schedule
                .windows
                .push(RecurringFocusWindowConfig::default());
            self.profile_edit_schedule_window = self.recurring_schedule.windows.len() - 1;
            return;
        }

        if self.recurring_schedule.windows.is_empty() {
            return;
        }
        self.recurring_schedule
            .windows
            .remove(self.profile_edit_schedule_window);
        self.clamp_profile_edit_schedule_selection();
    }

    fn adjust_schedule_exceptions_collection(&mut self, increase: bool) {
        if increase {
            let mut candidate = self.current_frame_now.date_naive();
            while self
                .recurring_schedule
                .exception_dates
                .iter()
                .any(|value| value == &candidate.format("%Y-%m-%d").to_string())
            {
                candidate = candidate.succ_opt().unwrap_or(candidate);
            }
            let candidate_value = candidate.format("%Y-%m-%d").to_string();
            self.recurring_schedule
                .exception_dates
                .push(candidate_value.clone());
            sort_schedule_exception_dates(&mut self.recurring_schedule.exception_dates);
            self.profile_edit_schedule_exception = self
                .recurring_schedule
                .exception_dates
                .iter()
                .position(|value| value == &candidate_value)
                .unwrap_or(0);
            return;
        }

        if self.recurring_schedule.exception_dates.is_empty() {
            return;
        }
        self.recurring_schedule
            .exception_dates
            .remove(self.profile_edit_schedule_exception);
        self.clamp_profile_edit_schedule_selection();
    }

    fn profile_edit_metadata_field_mut(&mut self) -> Option<&mut String> {
        match self.profile_edit_field {
            PROFILE_EDIT_WAKATIME_PROJECT_INDEX => Some(&mut self.wakatime_metadata.project),
            PROFILE_EDIT_WAKATIME_LANGUAGE_INDEX => Some(&mut self.wakatime_metadata.language),
            _ => None,
        }
    }

    fn handle_profile_edit_metadata_input(&mut self, key: &KeyEvent) -> bool {
        let Some(field_value) = self.profile_edit_metadata_field_mut() else {
            return false;
        };

        match key.code {
            KeyCode::Backspace => {
                field_value.pop();
                true
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                field_value.push(c);
                true
            }
            _ => false,
        }
    }

    pub fn strict_mode_enforced_for_focus(&self) -> bool {
        self.strict_mode && self.focus_session_active_for_current_state()
    }

    pub fn strict_reset_confirmation_pending(&self) -> bool {
        self.pending_timer_action == Some(PendingTimerAction::Reset)
    }

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

    pub fn site_input_mode(&self) -> SiteInputMode {
        if self.site_edit_index.is_some() {
            SiteInputMode::Edit
        } else {
            SiteInputMode::Add
        }
    }

    pub fn blocklist_profile_input_mode(&self) -> Option<BlocklistProfileInputMode> {
        self.blocklist_profile_input_mode
    }

    pub fn active_blocklist_profile_name(&self) -> &str {
        self.blocklist_profiles
            .get(self.active_blocklist_profile)
            .map(|profile| profile.name.as_str())
            .unwrap_or(DEFAULT_BLOCKLIST_PROFILE_NAME)
    }

    pub fn active_blocklist_profile_position(&self) -> usize {
        self.active_blocklist_profile.saturating_add(1)
    }

    pub fn blocklist_profile_count(&self) -> usize {
        self.blocklist_profiles.len()
    }

    fn restore_in_progress_session(&mut self) {
        let loaded_snapshot = match session_recovery::load() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.phase_notification =
                    Some(format!("Ignored saved in-progress session: {error}."));
                return;
            }
        };

        let Some(snapshot) = loaded_snapshot else {
            return;
        };

        if let Err(reason) = self.try_apply_recovery_snapshot(snapshot.clone()) {
            self.phase_notification = Some(format!("Ignored saved in-progress session: {reason}."));
            if let Err(clear_error) = session_recovery::clear() {
                self.config_error = Some(format!(
                    "session recovery cleanup failed after invalid state: {clear_error}"
                ));
            }
            return;
        }

        self.phase_notification = Some(format!(
            "Recovered in-progress {} session ({} remaining).",
            snapshot.phase().label(),
            format_duration_label(snapshot.remaining_secs)
        ));
    }

    fn try_apply_recovery_snapshot(
        &mut self,
        snapshot: InProgressSessionSnapshot,
    ) -> Result<(), String> {
        let profile_spec = profile_spec_for(snapshot.selected_profile, &self.custom_profile);
        let mut recovered_timer = TimerState::with_profile(
            profile_spec.focus_secs,
            profile_spec.short_break_secs,
            profile_spec.long_break_secs,
            profile_spec.long_break_interval,
        );
        recovered_timer.phase = snapshot.phase();
        recovered_timer.status = snapshot.status();
        recovered_timer.remaining_secs = snapshot.remaining_secs;
        recovered_timer.pomodoros_completed = snapshot.pomodoros_completed;
        snapshot.validate_for_timer(&recovered_timer)?;

        let task_label = snapshot
            .normalized_task_label()
            .ok_or_else(|| "saved task label is missing or invalid".to_string())?;
        let selected_task_label =
            if let Some(existing_index) = task_label_index(&self.task_labels, &task_label) {
                self.task_labels[existing_index].clone()
            } else {
                self.task_labels.push(task_label.clone());
                task_label
            };

        self.selected_profile = snapshot.selected_profile;
        self.profile_selection_index = profile_index(snapshot.selected_profile);
        self.timer = recovered_timer;
        self.selected_task_label = Some(selected_task_label);
        self.pending_timer_action = None;
        self.break_glass_expires_at = None;
        self.schedule_armed_occurrence_key = None;
        self.last_schedule_occurrence_key = None;
        self.active_focus_task_label = if self.timer.phase == TimerPhase::Focus {
            self.selected_task_label.clone()
        } else {
            None
        };
        self.active_focus_profile = if self.timer.phase == TimerPhase::Focus {
            Some(self.selected_profile)
        } else {
            None
        };
        self.apply_blocking_for_phase();
        self.sync_task_planner_state();
        Ok(())
    }

    fn sync_recovery_snapshot(&mut self) {
        let recovery_task_label = if self.focus_session_active_for_current_state() {
            self.active_focus_task_label
                .clone()
                .or_else(|| self.selected_task_label.clone())
        } else {
            self.selected_task_label.clone()
        };

        let snapshot = InProgressSessionSnapshot::from_timer_state(
            &self.timer,
            recovery_task_label,
            self.selected_profile,
        );

        match snapshot {
            Some(snapshot) => {
                if let Err(error) = session_recovery::save(&snapshot) {
                    self.config_error = Some(format!("session recovery save failed: {error}"));
                }
            }
            None => {
                if let Err(error) = session_recovery::clear() {
                    self.config_error = Some(format!("session recovery clear failed: {error}"));
                }
            }
        }
    }

    /// Persist the current blocked-sites list and timer preferences to disk.
    /// Failures are best-effort; the error is surfaced through `config_error`.
    fn persisted_config(&self) -> AppConfig {
        let custom_profile = self.custom_profile.normalized();
        let mut blocklist_profiles = self.blocklist_profiles.clone();
        if blocklist_profiles.is_empty() {
            blocklist_profiles.push(BlocklistProfileConfig::default());
        }
        let active_index = self
            .active_blocklist_profile
            .min(blocklist_profiles.len().saturating_sub(1));
        if let Some(active_profile) = blocklist_profiles.get_mut(active_index) {
            active_profile.sites = self.blocker.sites.clone();
        }
        let selected_blocklist_profile = blocklist_profiles
            .get(active_index)
            .or_else(|| blocklist_profiles.first())
            .map(|profile| profile.name.clone())
            .unwrap_or_else(|| DEFAULT_BLOCKLIST_PROFILE_NAME.to_string());
        AppConfig {
            // Keep legacy fields aligned with the editable custom profile so
            // older releases retain user-configured values.
            focus_secs: custom_profile.focus_secs,
            short_break_secs: custom_profile.short_break_secs,
            long_break_secs: custom_profile.long_break_secs,
            long_break_interval: custom_profile.long_break_interval,
            blocked_sites: self.blocker.sites.clone(),
            blocklist_profiles,
            selected_blocklist_profile,
            selected_profile: self.selected_profile,
            custom_profile: Some(custom_profile),
            notifications: self.notification_settings,
            auto_start: self.auto_start,
            recurring_schedule: self.recurring_schedule.clone(),
            strict_mode: self.strict_mode,
            break_glass_duration_secs: self.break_glass_duration_secs,
            daily_goal: self.daily_goal,
            wakatime: self.wakatime_metadata.clone(),
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
            AppMode::SessionPlanner => self.handle_key_session_planner(key),
            AppMode::StatsHistory => self.handle_key_stats_history(key),
            AppMode::SetupDiagnostics => self.handle_key_setup_diagnostics(key),
        }
    }

    pub fn handle_paste(&mut self, text: String) {
        if self.mode != AppMode::SiteManager {
            return;
        }

        if self.blocklist_profile_input_active {
            self.blocklist_profile_input.push_str(&text);
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

        if self.strict_reset_confirmation_pending() {
            if key.code == KeyCode::Char('s') {
                self.pending_timer_action = None;
                self.update_timer_and_sync(TimerState::reset);
                return;
            }
            self.pending_timer_action = None;
        }

        if self.break_glass_confirmation_pending() {
            if key.code == KeyCode::Char('u') {
                self.confirm_break_glass_override();
                return;
            }
            self.pending_timer_action = None;
        }

        match key.code {
            // Start / pause
            KeyCode::Char(' ') => {
                if self.timer.phase == TimerPhase::Focus
                    && self.timer.status == TimerStatus::Idle
                    && self.selected_task_label.is_none()
                {
                    self.phase_notification =
                        Some("Select a task label with [t] before starting focus.".to_string());
                    return;
                }
                self.update_timer_and_sync(TimerState::toggle_pause);
            }
            // Stop / reset current phase
            KeyCode::Char('s') => {
                if self.strict_mode_enforced_for_focus() {
                    self.pending_timer_action = Some(PendingTimerAction::Reset);
                    return;
                }
                self.update_timer_and_sync(TimerState::reset);
            }
            // Skip to next phase
            KeyCode::Char('n') => {
                if self.strict_mode_enforced_for_focus() {
                    return;
                }
                self.update_timer_and_sync(TimerState::next_phase);
            }
            // Open site manager
            KeyCode::Char('b') => {
                self.open_site_manager();
            }
            // Open profile manager
            KeyCode::Char('p') => {
                if self.strict_mode_enforced_for_focus() {
                    return;
                }
                self.open_profile_manager();
            }
            // Open session planner
            KeyCode::Char('t') => {
                self.open_session_planner();
            }
            // Open stats history
            KeyCode::Char('h') => {
                self.open_stats_history();
            }
            // Open setup diagnostics
            KeyCode::Char('d') => {
                self.open_setup_diagnostics();
            }
            // Break-glass override (temporary unblock)
            KeyCode::Char('u') => {
                self.handle_break_glass_key();
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
            KeyCode::Char('e') => {
                self.export_stats_history();
            }
            _ => {}
        }
    }

    fn handle_key_setup_diagnostics(&mut self, key: KeyEvent) {
        if self.handle_quit_key(&key, false) {
            return;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('d') => {
                self.mode = AppMode::Timer;
            }
            KeyCode::Char('r') => {
                self.refresh_setup_diagnostics();
            }
            _ => {}
        }
    }

    fn handle_key_profile_manager(&mut self, key: KeyEvent) {
        if self.profile_edit_active {
            if self.handle_quit_key(&key, false) {
                return;
            }

            if self.handle_profile_edit_metadata_input(&key) {
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
                if self.apply_profile(selected) {
                    self.exit_profile_manager();
                }
            }
            KeyCode::Char('e') => {
                self.begin_profile_edit();
            }
            _ => {}
        }
    }

    fn handle_key_session_planner(&mut self, key: KeyEvent) {
        if self.planner_input_active {
            match key.code {
                KeyCode::Enter => self.commit_planner_input(),
                KeyCode::Esc => self.cancel_planner_input(),
                KeyCode::Backspace => {
                    self.planner_input.pop();
                }
                KeyCode::Char(c) => {
                    self.planner_input.push(c);
                }
                _ => {}
            }
            return;
        }

        if self.handle_quit_key(&key, false) {
            return;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('t') => {
                self.mode = AppMode::Timer;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.planner_selection_index = self.planner_selection_index.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') if !self.task_labels.is_empty() => {
                self.planner_selection_index =
                    (self.planner_selection_index + 1).min(self.task_labels.len() - 1);
            }
            KeyCode::Char('a') => self.start_planner_input(),
            KeyCode::Char('e') => self.start_planner_rename_input(),
            KeyCode::Char('d') | KeyCode::Delete => self.remove_planner_label(),
            KeyCode::Char('r') => self.select_recent_planner_label(0),
            KeyCode::Char(c @ '1'..='9') => {
                let index = (c as usize).saturating_sub('1' as usize);
                self.select_recent_planner_label(index);
            }
            KeyCode::Enter => self.select_planner_label(),
            _ => {}
        }
    }

    fn start_planner_input(&mut self) {
        self.planner_input.clear();
        self.planner_input_active = true;
        self.planner_input_mode = Some(PlannerInputMode::Add);
        self.planner_feedback = None;
    }

    fn start_planner_rename_input(&mut self) {
        if self.task_labels.is_empty() {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        }

        self.clamp_planner_selection();
        let Some(label) = self.task_labels.get(self.planner_selection_index).cloned() else {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        };

        self.planner_input = label;
        self.planner_input_active = true;
        self.planner_input_mode = Some(PlannerInputMode::Rename);
        self.planner_feedback = None;
    }

    fn cancel_planner_input(&mut self) {
        self.planner_input.clear();
        self.planner_input_active = false;
        self.planner_input_mode = None;
    }

    fn commit_planner_input(&mut self) {
        let Some(mode) = self.planner_input_mode else {
            self.set_planner_feedback(
                PlannerFeedbackLevel::Warning,
                "Planner input mode unavailable",
            );
            return;
        };
        let Some(label) = normalize_task_label(&self.planner_input) else {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "Task label cannot be empty");
            return;
        };

        match mode {
            PlannerInputMode::Add => self.commit_planner_add_input(label),
            PlannerInputMode::Rename => self.commit_planner_rename_input(label),
        }
    }

    fn commit_planner_add_input(&mut self, label: String) {
        if let Some(existing_index) = task_label_index(&self.task_labels, &label) {
            self.planner_selection_index = existing_index;
            self.selected_task_label = self.task_labels.get(existing_index).cloned();
            self.sync_task_planner_state();
            self.sync_recovery_snapshot();
            self.cancel_planner_input();
            self.set_planner_feedback(
                PlannerFeedbackLevel::Warning,
                format!("`{label}` already exists, selected existing label"),
            );
            return;
        }

        self.task_labels.push(label.clone());
        self.clamp_planner_selection();
        self.planner_selection_index = self.task_labels.len().saturating_sub(1);
        self.selected_task_label = Some(label.clone());
        self.sync_task_planner_state();
        self.sync_recovery_snapshot();
        self.cancel_planner_input();
        self.set_planner_feedback(
            PlannerFeedbackLevel::Success,
            format!("Added and selected `{label}`"),
        );
    }

    fn commit_planner_rename_input(&mut self, label: String) {
        if self.task_labels.is_empty() {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        }

        self.clamp_planner_selection();
        let Some(current_label) = self.task_labels.get(self.planner_selection_index).cloned()
        else {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        };

        if current_label == label {
            self.set_planner_feedback(
                PlannerFeedbackLevel::Warning,
                format!("No change for `{current_label}`"),
            );
            return;
        }

        if let Some(existing_index) = task_label_index(&self.task_labels, &label)
            && existing_index != self.planner_selection_index
        {
            self.set_planner_feedback(
                PlannerFeedbackLevel::Warning,
                format!("`{label}` already exists"),
            );
            return;
        }

        if let Some(target) = self.task_labels.get_mut(self.planner_selection_index) {
            *target = label.clone();
        }
        if self
            .selected_task_label
            .as_ref()
            .is_some_and(|selected| selected.eq_ignore_ascii_case(&current_label))
        {
            self.selected_task_label = Some(label.clone());
        }
        if self
            .active_focus_task_label
            .as_ref()
            .is_some_and(|active| active.eq_ignore_ascii_case(&current_label))
        {
            self.active_focus_task_label = Some(label.clone());
        }

        self.sync_task_planner_state();
        self.sync_recovery_snapshot();
        self.cancel_planner_input();
        self.set_planner_feedback(
            PlannerFeedbackLevel::Success,
            format!("Renamed `{current_label}` -> `{label}`"),
        );
    }

    fn remove_planner_label(&mut self) {
        if self.task_labels.is_empty() {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        }

        self.clamp_planner_selection();
        let removed = self.task_labels.remove(self.planner_selection_index);
        self.clamp_planner_selection();

        let removed_was_selected = self
            .selected_task_label
            .as_ref()
            .is_some_and(|selected| selected.eq_ignore_ascii_case(&removed));
        let selected_label_missing = self
            .selected_task_label
            .as_ref()
            .is_some_and(|selected| task_label_index(&self.task_labels, selected).is_none());
        if removed_was_selected || selected_label_missing {
            self.selected_task_label = self.task_labels.get(self.planner_selection_index).cloned();
        }

        self.sync_task_planner_state();
        self.sync_recovery_snapshot();
        let feedback = if removed_was_selected {
            if let Some(selected) = self.selected_task_label.as_ref() {
                format!("Deleted `{removed}` (selected `{selected}`)")
            } else {
                format!("Deleted `{removed}` (no selected label)")
            }
        } else {
            format!("Deleted `{removed}`")
        };
        self.set_planner_feedback(PlannerFeedbackLevel::Success, feedback);
    }

    fn select_recent_planner_label(&mut self, index: usize) {
        let recent = self.planner_recent_labels(PLANNER_RECENT_LABEL_LIMIT);
        if recent.is_empty() {
            self.set_planner_feedback(
                PlannerFeedbackLevel::Warning,
                "No recent task labels available",
            );
            return;
        }
        if index >= recent.len() {
            self.set_planner_feedback(
                PlannerFeedbackLevel::Warning,
                format!("Recent quick-pick {} is unavailable", index + 1),
            );
            return;
        }

        let label = recent[index].clone();
        let Some(existing_index) = task_label_index(&self.task_labels, &label) else {
            self.set_planner_feedback(
                PlannerFeedbackLevel::Warning,
                format!("Recent label `{label}` is no longer available"),
            );
            return;
        };
        self.planner_selection_index = existing_index;
        self.selected_task_label = self.task_labels.get(existing_index).cloned();
        self.sync_task_planner_state();
        self.sync_recovery_snapshot();
        self.set_planner_feedback(
            PlannerFeedbackLevel::Success,
            format!("Selected recent `{label}`"),
        );
    }

    fn select_planner_label(&mut self) {
        if self.task_labels.is_empty() {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        }

        self.clamp_planner_selection();
        if let Some(label) = self.task_labels.get(self.planner_selection_index).cloned() {
            self.selected_task_label = Some(label.clone());
            self.sync_task_planner_state();
            self.sync_recovery_snapshot();
            self.set_planner_feedback(PlannerFeedbackLevel::Success, format!("Selected `{label}`"));
        }
    }

    fn begin_profile_edit(&mut self) {
        self.profile_edit_snapshot = Some(ProfileEditSnapshot {
            custom_profile: self.custom_profile.clone(),
            notification_settings: self.notification_settings,
            auto_start: self.auto_start,
            recurring_schedule: self.recurring_schedule.clone(),
            strict_mode: self.strict_mode,
            daily_goal: self.daily_goal,
            wakatime_metadata: self.wakatime_metadata.clone(),
        });
        self.profile_edit_active = true;
        self.profile_edit_field = 0;
        self.profile_edit_schedule_window = 0;
        self.profile_edit_schedule_day = 0;
        self.profile_edit_schedule_exception = 0;
        self.clamp_profile_edit_schedule_selection();
    }

    fn cancel_profile_edit(&mut self) {
        if let Some(snapshot) = self.profile_edit_snapshot.take() {
            self.custom_profile = snapshot.custom_profile;
            self.notification_settings = snapshot.notification_settings;
            self.auto_start = snapshot.auto_start;
            self.recurring_schedule = snapshot.recurring_schedule;
            self.strict_mode = snapshot.strict_mode;
            self.daily_goal = snapshot.daily_goal;
            self.wakatime_metadata = snapshot.wakatime_metadata;
            self.sync_wakatime_metadata_to_tracker();
            self.rebuild_notifier();
            self.rebuild_recurring_schedule_runtime();
        }
        self.profile_edit_active = false;
        self.profile_edit_field = 0;
        self.profile_edit_schedule_window = 0;
        self.profile_edit_schedule_day = 0;
        self.profile_edit_schedule_exception = 0;
        self.clamp_profile_edit_schedule_selection();
    }

    fn commit_profile_edit(&mut self) {
        let custom_profile_changed = self.profile_edit_snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.custom_profile.normalized() != self.custom_profile.normalized()
        });
        let normalized_schedule = self.recurring_schedule.normalized();
        let schedule_changed = self.profile_edit_snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.recurring_schedule.normalized() != normalized_schedule
        });
        let daily_goal_changed = self
            .profile_edit_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.daily_goal != self.daily_goal);
        self.custom_profile = self.custom_profile.normalized();
        self.recurring_schedule = normalized_schedule;
        self.wakatime_metadata = self.wakatime_metadata.normalized();
        if self.selected_profile == ProfileId::Custom {
            if custom_profile_changed {
                if !self.apply_profile(ProfileId::Custom) {
                    return;
                }
            } else {
                self.save_config();
            }
        } else {
            self.save_config();
        }
        self.sync_wakatime_metadata_to_tracker();
        self.rebuild_notifier();
        self.rebuild_recurring_schedule_runtime();
        if schedule_changed {
            self.schedule_armed_occurrence_key = None;
            self.last_schedule_occurrence_key = None;
            let now = Local::now();
            self.current_frame_now = now;
            self.sync_recurring_schedule(now);
        }
        if daily_goal_changed {
            self.sync_today_goal_snapshot();
        }
        self.profile_edit_active = false;
        self.profile_edit_field = 0;
        self.profile_edit_schedule_window = 0;
        self.profile_edit_schedule_day = 0;
        self.profile_edit_schedule_exception = 0;
        self.clamp_profile_edit_schedule_selection();
        self.profile_edit_snapshot = None;
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
            6 => {
                self.auto_start.focus_to_break = increase;
            }
            7 => {
                self.auto_start.break_to_focus = increase;
            }
            8 => {
                if !increase && self.strict_mode_enforced_for_focus() {
                    return;
                }
                self.strict_mode = increase;
            }
            9 => {
                adjust_daily_goal_minutes(&mut self.daily_goal.minutes, increase);
            }
            10 => {
                adjust_daily_goal_pomodoros(&mut self.daily_goal.pomodoros, increase);
            }
            PROFILE_EDIT_SCHEDULE_WINDOW_INDEX => {
                self.cycle_schedule_window(increase);
            }
            PROFILE_EDIT_SCHEDULE_DAY_INDEX => {
                self.cycle_schedule_day(increase);
            }
            PROFILE_EDIT_SCHEDULE_DAY_ENABLED_INDEX => {
                self.set_schedule_day_enabled(increase);
            }
            PROFILE_EDIT_SCHEDULE_START_INDEX => {
                self.adjust_selected_schedule_time(true, increase);
            }
            PROFILE_EDIT_SCHEDULE_END_INDEX => {
                self.adjust_selected_schedule_time(false, increase);
            }
            PROFILE_EDIT_SCHEDULE_ADD_REMOVE_INDEX => {
                self.adjust_schedule_windows_collection(increase);
            }
            PROFILE_EDIT_SCHEDULE_EXCEPTION_INDEX => {
                self.cycle_schedule_exception(increase);
            }
            PROFILE_EDIT_SCHEDULE_EXCEPTION_DATE_INDEX => {
                self.adjust_selected_schedule_exception_date(increase);
            }
            PROFILE_EDIT_SCHEDULE_EXCEPTION_ADD_REMOVE_INDEX => {
                self.adjust_schedule_exceptions_collection(increase);
            }
            PROFILE_EDIT_WAKATIME_PROJECT_INDEX | PROFILE_EDIT_WAKATIME_LANGUAGE_INDEX => {}
            _ => {}
        }
    }

    fn sync_wakatime_metadata_to_tracker(&mut self) {
        self.wakatime
            .set_heartbeat_metadata(WakatimeHeartbeatMetadata {
                project: self.wakatime_metadata.project.clone(),
                language: self.wakatime_metadata.language.clone(),
            });
    }

    fn rebuild_recurring_schedule_runtime(&mut self) {
        self.recurring_windows = compile_windows(&self.recurring_schedule.windows);
        self.recurring_exception_dates =
            compile_exception_dates(&self.recurring_schedule.exception_dates);
    }

    fn apply_profile(&mut self, profile: ProfileId) -> bool {
        if self.strict_mode_enforced_for_focus() {
            self.config_error = Some(
                "strict focus active: finish focus before applying profile changes".to_string(),
            );
            return false;
        }
        let profile_spec = profile_spec_for(profile, &self.custom_profile);
        self.timer = TimerState::with_profile(
            profile_spec.focus_secs,
            profile_spec.short_break_secs,
            profile_spec.long_break_secs,
            profile_spec.long_break_interval,
        );
        self.active_focus_task_label = None;
        self.active_focus_profile = None;
        self.selected_profile = profile;
        self.profile_selection_index = profile_index(profile);
        self.pending_timer_action = None;
        self.save_config();
        self.apply_blocking_for_phase();
        self.sync_recovery_snapshot();
        true
    }

    fn handle_key_site_manager(&mut self, key: KeyEvent) {
        if self.blocklist_profile_input_active {
            match key.code {
                KeyCode::Enter => {
                    self.commit_blocklist_profile_input();
                }
                KeyCode::Esc => {
                    self.cancel_blocklist_profile_input();
                }
                KeyCode::Backspace => {
                    self.blocklist_profile_input.pop();
                }
                KeyCode::Char(c) => {
                    self.blocklist_profile_input.push(c);
                }
                _ => {}
            }
            return;
        }

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
            KeyCode::Down | KeyCode::Char('j') if !self.blocker.sites.is_empty() => {
                self.selected_site = (self.selected_site + 1).min(self.blocker.sites.len() - 1);
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
            // Previous blocklist profile
            KeyCode::Char('[') => {
                self.select_previous_blocklist_profile();
            }
            // Next blocklist profile
            KeyCode::Char(']') => {
                self.select_next_blocklist_profile();
            }
            // Create blocklist profile
            KeyCode::Char('n') => {
                self.start_blocklist_profile_input(BlocklistProfileInputMode::Create);
            }
            // Rename active blocklist profile
            KeyCode::Char('r') => {
                self.start_blocklist_profile_input(BlocklistProfileInputMode::Rename);
            }
            // Delete active blocklist profile
            KeyCode::Char('x') => {
                self.delete_active_blocklist_profile();
            }
            _ => {}
        }
    }

    fn start_site_input(&mut self, mode: SiteInputMode) {
        self.cancel_blocklist_profile_input();
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

    fn start_blocklist_profile_input(&mut self, mode: BlocklistProfileInputMode) {
        self.cancel_site_input();
        self.blocklist_profile_input_active = true;
        self.blocklist_profile_input_mode = Some(mode);
        self.site_feedback = None;
        match mode {
            BlocklistProfileInputMode::Create => {
                self.blocklist_profile_input.clear();
            }
            BlocklistProfileInputMode::Rename => {
                self.blocklist_profile_input = self.active_blocklist_profile_name().to_string();
            }
        }
    }

    fn cancel_blocklist_profile_input(&mut self) {
        self.blocklist_profile_input.clear();
        self.blocklist_profile_input_active = false;
        self.blocklist_profile_input_mode = None;
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

    fn commit_blocklist_profile_input(&mut self) {
        let Some(mode) = self.blocklist_profile_input_mode else {
            return;
        };

        let name = self.blocklist_profile_input.trim().to_string();
        if name.is_empty() {
            self.set_site_feedback(SiteFeedbackLevel::Warning, "Profile name cannot be empty");
            return;
        }

        let has_duplicate = self
            .blocklist_profiles
            .iter()
            .enumerate()
            .any(|(index, profile)| {
                let is_current = mode == BlocklistProfileInputMode::Rename
                    && index == self.active_blocklist_profile;
                !is_current && profile.name.eq_ignore_ascii_case(&name)
            });
        if has_duplicate {
            self.set_site_feedback(
                SiteFeedbackLevel::Warning,
                format!("Profile `{name}` already exists"),
            );
            return;
        }

        match mode {
            BlocklistProfileInputMode::Create => {
                self.sync_active_profile_sites_from_blocker();
                self.blocklist_profiles.push(BlocklistProfileConfig {
                    name: name.clone(),
                    sites: Vec::new(),
                });
                self.active_blocklist_profile = self.blocklist_profiles.len().saturating_sub(1);
                self.load_active_profile_sites_into_blocker();
                self.clamp_selection();
                self.cancel_blocklist_profile_input();
                self.save_config();
                self.sync_blocking_after_site_mutation();
                self.set_site_feedback(
                    SiteFeedbackLevel::Success,
                    format!("Created profile `{name}`"),
                );
            }
            BlocklistProfileInputMode::Rename => {
                let old_name = self.active_blocklist_profile_name().to_string();
                if old_name == name {
                    self.set_site_feedback(
                        SiteFeedbackLevel::Warning,
                        format!("No change for profile `{name}`"),
                    );
                    return;
                }
                if let Some(profile) = self
                    .blocklist_profiles
                    .get_mut(self.active_blocklist_profile)
                {
                    profile.name = name.clone();
                }
                self.cancel_blocklist_profile_input();
                self.save_config();
                self.set_site_feedback(
                    SiteFeedbackLevel::Success,
                    format!("Renamed profile `{old_name}` -> `{name}`"),
                );
            }
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

    fn select_previous_blocklist_profile(&mut self) {
        if self.blocklist_profiles.len() <= 1 {
            return;
        }
        let next = if self.active_blocklist_profile == 0 {
            self.blocklist_profiles.len().saturating_sub(1)
        } else {
            self.active_blocklist_profile.saturating_sub(1)
        };
        self.switch_blocklist_profile(next);
    }

    fn select_next_blocklist_profile(&mut self) {
        if self.blocklist_profiles.len() <= 1 {
            return;
        }
        let next = (self.active_blocklist_profile + 1) % self.blocklist_profiles.len();
        self.switch_blocklist_profile(next);
    }

    fn switch_blocklist_profile(&mut self, next_index: usize) {
        if next_index >= self.blocklist_profiles.len()
            || next_index == self.active_blocklist_profile
        {
            return;
        }

        self.sync_active_profile_sites_from_blocker();
        self.active_blocklist_profile = next_index;
        self.load_active_profile_sites_into_blocker();
        self.clamp_selection();
        self.save_config();
        self.sync_blocking_after_site_mutation();
        self.set_site_feedback(
            SiteFeedbackLevel::Success,
            format!(
                "Switched to profile `{}`",
                self.active_blocklist_profile_name()
            ),
        );
    }

    fn delete_active_blocklist_profile(&mut self) {
        if self.blocklist_profiles.len() <= 1 {
            self.set_site_feedback(
                SiteFeedbackLevel::Warning,
                "At least one blocklist profile is required",
            );
            return;
        }

        self.sync_active_profile_sites_from_blocker();
        let removed = self
            .blocklist_profiles
            .remove(self.active_blocklist_profile);
        if self.active_blocklist_profile >= self.blocklist_profiles.len() {
            self.active_blocklist_profile = self.blocklist_profiles.len().saturating_sub(1);
        }
        self.load_active_profile_sites_into_blocker();
        self.clamp_selection();
        self.save_config();
        self.sync_blocking_after_site_mutation();
        self.set_site_feedback(
            SiteFeedbackLevel::Success,
            format!(
                "Deleted profile `{}` (active: `{}`)",
                removed.name,
                self.active_blocklist_profile_name()
            ),
        );
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

    fn clamp_blocklist_profile_selection(&mut self) {
        if self.blocklist_profiles.is_empty() {
            self.blocklist_profiles
                .push(BlocklistProfileConfig::default());
            self.active_blocklist_profile = 0;
            return;
        }
        self.active_blocklist_profile = self
            .active_blocklist_profile
            .min(self.blocklist_profiles.len().saturating_sub(1));
    }

    fn sync_active_profile_sites_from_blocker(&mut self) {
        self.clamp_blocklist_profile_selection();
        if let Some(active_profile) = self
            .blocklist_profiles
            .get_mut(self.active_blocklist_profile)
        {
            active_profile.sites = self.blocker.sites.clone();
        }
    }

    fn load_active_profile_sites_into_blocker(&mut self) {
        self.clamp_blocklist_profile_selection();
        self.blocker.sites.clear();
        if let Some(active_profile) = self.blocklist_profiles.get(self.active_blocklist_profile) {
            for site in &active_profile.sites {
                self.blocker.add_site(site.clone());
            }
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

    fn clamp_planner_selection(&mut self) {
        if self.task_labels.is_empty() {
            self.planner_selection_index = 0;
        } else {
            self.planner_selection_index = self
                .planner_selection_index
                .min(self.task_labels.len().saturating_sub(1));
        }
    }

    fn sync_task_planner_state(&mut self) {
        if self
            .stats
            .update_task_planner_state(self.task_labels.clone(), self.selected_task_label.clone())
        {
            self.stats_dirty = true;
            self.flush_stats_if_dirty(false);
        }
    }

    fn handle_quit_key(&mut self, key: &KeyEvent, esc_quits: bool) -> bool {
        let is_quit_key = matches!(
            key.code,
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('c')
        ) && (key.code != KeyCode::Esc || esc_quits)
            && (key.code != KeyCode::Char('c') || key.modifiers.contains(KeyModifiers::CONTROL));
        if is_quit_key && self.strict_mode_enforced_for_focus() {
            self.phase_notification =
                Some("Strict mode active. Finish or stop focus before quitting.".to_string());
            return true;
        }

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
        let was_focus_active = self.focus_session_active_for_current_state();
        self.pending_timer_action = None;
        action(&mut self.timer);
        let is_focus_active = self.focus_session_active_for_current_state();
        if !was_focus_active && is_focus_active {
            self.active_focus_task_label = self.selected_task_label.clone();
            self.active_focus_profile = Some(self.selected_profile);
            self.schedule_armed_occurrence_key = None;
        } else if was_focus_active && !is_focus_active {
            self.active_focus_task_label = None;
            self.active_focus_profile = None;
            self.break_glass_expires_at = None;
        }
        self.apply_blocking_for_phase();
        self.sync_recovery_snapshot();
    }

    fn open_site_manager(&mut self) {
        self.pending_timer_action = None;
        self.mode = AppMode::SiteManager;
        self.cancel_site_input();
        self.cancel_blocklist_profile_input();
        self.clamp_blocklist_profile_selection();
        self.clamp_selection();
    }

    fn open_profile_manager(&mut self) {
        self.mode = AppMode::ProfileManager;
        self.profile_edit_active = false;
        self.profile_edit_field = 0;
        self.profile_edit_schedule_window = 0;
        self.profile_edit_schedule_day = 0;
        self.profile_edit_schedule_exception = 0;
        self.profile_edit_snapshot = None;
        self.profile_selection_index = profile_index(self.selected_profile);
        self.clamp_profile_selection();
        self.clamp_profile_edit_schedule_selection();
    }

    fn open_session_planner(&mut self) {
        self.mode = AppMode::SessionPlanner;
        self.planner_feedback = None;
        self.planner_input.clear();
        self.planner_input_active = false;
        self.planner_input_mode = None;
        if let Some(selected) = self.selected_task_label.as_ref()
            && let Some(index) = task_label_index(&self.task_labels, selected)
        {
            self.planner_selection_index = index;
        }
        self.clamp_planner_selection();
    }

    fn open_stats_history(&mut self) {
        self.pending_timer_action = None;
        self.history_feedback = None;
        self.mode = AppMode::StatsHistory;
    }

    fn open_setup_diagnostics(&mut self) {
        self.pending_timer_action = None;
        self.refresh_setup_diagnostics();
        self.mode = AppMode::SetupDiagnostics;
    }

    fn exit_profile_manager(&mut self) {
        self.mode = AppMode::Timer;
        self.profile_edit_snapshot = None;
    }

    fn finalize_site_mutation(&mut self) {
        self.clamp_selection();
        self.sync_active_profile_sites_from_blocker();
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
        self.focus_session_active_for_current_state() && !self.break_glass_override_active_now()
    }

    fn focus_running_for_current_state(&self) -> bool {
        self.timer.phase == TimerPhase::Focus && self.timer.status == TimerStatus::Running
    }

    fn focus_session_active_for_current_state(&self) -> bool {
        self.timer.phase == TimerPhase::Focus && self.timer.status != TimerStatus::Idle
    }

    fn break_glass_override_active_now(&self) -> bool {
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

    fn should_auto_start_transition(
        &self,
        completed_phase: TimerPhase,
        next_phase: TimerPhase,
    ) -> bool {
        match (completed_phase, next_phase) {
            (TimerPhase::Focus, TimerPhase::ShortBreak | TimerPhase::LongBreak) => {
                self.auto_start.focus_to_break
            }
            (TimerPhase::ShortBreak | TimerPhase::LongBreak, TimerPhase::Focus) => {
                self.auto_start.break_to_focus
            }
            _ => false,
        }
    }

    fn record_focus_elapsed(&mut self, elapsed_secs: u64) {
        if elapsed_secs == 0 {
            return;
        }

        let day_key = current_day_key();
        let goal_snapshot = self.current_goal_snapshot();
        let session_minutes_before = self.stats.session().focused_minutes();
        let today_minutes_before = self.stats.daily_for(&day_key).focused_minutes();

        self.stats
            .record_focus_elapsed(&day_key, elapsed_secs, goal_snapshot);
        self.stats_has_unsaved_elapsed = true;

        let session_minutes_after = self.stats.session().focused_minutes();
        let today_minutes_after = self.stats.daily_for(&day_key).focused_minutes();
        if session_minutes_before != session_minutes_after
            || today_minutes_before != today_minutes_after
        {
            self.stats_dirty = true;
        }
    }

    fn record_completed_focus_session(&mut self, focused_seconds: u64) {
        let day_key = current_day_key();
        let goal = self.current_goal_snapshot();
        if self.active_focus_task_label.is_some() {
            self.stats.record_completed_pomodoro_with_task(
                &day_key,
                goal,
                self.active_focus_task_label.as_deref(),
                focused_seconds,
                self.active_focus_profile,
            );
        } else {
            self.stats.record_completed_pomodoro(&day_key, goal);
        }
        self.stats_dirty = true;
    }

    fn current_goal_snapshot(&self) -> DailyGoalSnapshot {
        DailyGoalSnapshot {
            minutes: self.daily_goal.minutes,
            pomodoros: self.daily_goal.pomodoros,
        }
    }

    fn sync_today_goal_snapshot(&mut self) {
        let day_key = current_day_key();
        if self
            .stats
            .sync_goal_snapshot(&day_key, self.current_goal_snapshot())
        {
            self.stats_dirty = true;
            self.flush_stats_if_dirty(false);
        }
    }

    fn export_stats_history(&mut self) {
        let current_dir = match std::env::current_dir() {
            Ok(path) => path,
            Err(e) => {
                self.set_history_feedback(
                    HistoryFeedbackLevel::Warning,
                    format!("Export failed: cannot determine current directory ({e})"),
                );
                return;
            }
        };
        self.export_stats_to_dir(&current_dir);
    }

    fn export_stats_to_dir(&mut self, dir: &Path) {
        self.history_feedback = None;
        match self.stats.export_to_dir(dir) {
            Ok(paths) => self.set_history_feedback_for_export(paths),
            Err(e) => self
                .set_history_feedback(HistoryFeedbackLevel::Warning, format!("Export failed: {e}")),
        }
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

    fn set_planner_feedback(&mut self, level: PlannerFeedbackLevel, message: impl Into<String>) {
        self.planner_feedback = Some(PlannerFeedback {
            level,
            message: message.into(),
        });
    }

    fn set_history_feedback(&mut self, level: HistoryFeedbackLevel, message: impl Into<String>) {
        self.history_feedback = Some(HistoryFeedback {
            level,
            message: message.into(),
        });
    }

    fn set_history_feedback_for_export(&mut self, paths: ExportedStatsFiles) {
        let export_dir = paths
            .json_path
            .parent()
            .or_else(|| paths.csv_path.parent())
            .unwrap_or_else(|| Path::new("."));
        let json_name = paths
            .json_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("focustime-stats.json");
        let csv_name = paths
            .csv_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("focustime-stats.csv");
        self.set_history_feedback(
            HistoryFeedbackLevel::Success,
            format!(
                "Exported to {}: JSON {}, CSV {}",
                export_dir.display(),
                json_name,
                csv_name
            ),
        );
    }

    fn refresh_setup_diagnostics(&mut self) {
        self.setup_diagnostics = SetupDiagnostics::collect(&self.blocker);
    }

    fn rebuild_notifier(&mut self) {
        self.notifier = PhaseNotifier::new(self.notification_settings);
    }

    fn handle_break_glass_key(&mut self) {
        if !self.focus_session_active_for_current_state() {
            self.phase_notification =
                Some("Break-glass override is available only during active focus.".to_string());
            return;
        }
        if self.blocker.sites.is_empty() {
            self.phase_notification = Some(
                "Break-glass override unavailable: active blocklist profile has no sites."
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
            "Confirm break-glass with [u] to unblock for {}.",
            format_duration_label(self.break_glass_duration_secs)
        ));
    }

    fn confirm_break_glass_override(&mut self) {
        self.pending_timer_action = None;
        if !self.focus_session_active_for_current_state() {
            self.phase_notification =
                Some("Break-glass override is available only during active focus.".to_string());
            return;
        }
        if self.blocker.sites.is_empty() {
            self.phase_notification = Some(
                "Break-glass override unavailable: active blocklist profile has no sites."
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
        let day_key = current_day_key();
        let task_label = self
            .active_focus_task_label
            .as_deref()
            .or(self.selected_task_label.as_deref());
        self.stats.record_break_glass_override_event(
            &day_key,
            current_epoch_secs(),
            task_label,
            self.break_glass_duration_secs,
        );
        self.stats_dirty = true;
        self.flush_stats_if_dirty(false);
    }

    fn sync_break_glass_override(&mut self) {
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

    fn sync_recurring_schedule(&mut self, now: DateTime<Local>) {
        if self.recurring_windows.is_empty() {
            self.schedule_armed_occurrence_key = None;
            return;
        }

        if self.recurring_exception_dates.contains(&now.date_naive()) {
            self.schedule_armed_occurrence_key = None;
            return;
        }

        let Some(active_window) = active_occurrence(
            now,
            &self.recurring_windows,
            &self.recurring_exception_dates,
        ) else {
            self.schedule_armed_occurrence_key = None;
            return;
        };

        let active_occurrence_key = occurrence_key(&active_window);
        if self.last_schedule_occurrence_key.as_deref() != Some(active_occurrence_key.as_str()) {
            self.last_schedule_occurrence_key = Some(active_occurrence_key.clone());
            self.handle_schedule_window_start(&active_occurrence_key);
        } else if self.focus_session_active_for_current_state() {
            self.schedule_armed_occurrence_key = None;
        }
    }

    fn handle_schedule_window_start(&mut self, active_occurrence_key: &str) {
        if self.focus_session_active_for_current_state() {
            self.schedule_armed_occurrence_key = None;
            return;
        }

        // Scheduled windows always target focus. If we're currently in a break phase
        // (idle/paused/running), advance to focus first so schedule auto-start checks
        // and manual [Space] behavior both operate on the focus phase.
        if self.timer.phase != TimerPhase::Focus {
            self.update_timer_and_sync(TimerState::next_phase);
        }

        if self.can_auto_start_focus_for_schedule() {
            self.update_timer_and_sync(TimerState::toggle_pause);
            self.phase_notification =
                Some("Scheduled window started. Focus auto-started.".to_string());
            self.schedule_armed_occurrence_key = None;
            return;
        }

        self.schedule_armed_occurrence_key = Some(active_occurrence_key.to_string());
        self.phase_notification = Some(self.schedule_arm_notification());
    }

    fn can_auto_start_focus_for_schedule(&self) -> bool {
        self.timer.phase == TimerPhase::Focus
            && self.timer.status == TimerStatus::Idle
            && self.selected_task_label.is_some()
    }

    fn schedule_arm_notification(&self) -> String {
        if self.selected_task_label.is_none() {
            "Scheduled window started. Select a task label with [t], then press [Space] to start focus."
                .to_string()
        } else {
            "Scheduled window started. Press [Space] to start focus.".to_string()
        }
    }

    pub fn recent_break_glass_overrides(&self, limit: usize) -> Vec<BreakGlassOverrideEvent> {
        self.stats.recent_break_glass_overrides(limit)
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

fn adjust_daily_goal_minutes(value: &mut u64, increase: bool) {
    if increase {
        *value = if *value == 0 {
            DAILY_GOAL_MINUTES_STEP
        } else {
            value.saturating_add(DAILY_GOAL_MINUTES_STEP)
        };
    } else if *value <= DAILY_GOAL_MINUTES_STEP {
        *value = 0;
    } else {
        *value = value.saturating_sub(DAILY_GOAL_MINUTES_STEP);
    }
}

fn adjust_daily_goal_pomodoros(value: &mut u32, increase: bool) {
    if increase {
        *value = value.saturating_add(1);
    } else {
        *value = value.saturating_sub(1);
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

fn format_daily_goal_minutes_label(minutes: u64) -> String {
    if minutes == 0 {
        "Off".to_string()
    } else {
        format!("{minutes}m")
    }
}

fn format_daily_goal_pomodoros_label(pomodoros: u32) -> String {
    if pomodoros == 0 {
        "Off".to_string()
    } else {
        format!("{pomodoros}")
    }
}

fn bool_label(value: bool) -> &'static str {
    if value { "On" } else { "Off" }
}

fn parse_hhmm_minutes(value: &str) -> Option<u16> {
    let (hours, minutes) = value.split_once(':')?;
    if hours.len() != 2 || minutes.len() != 2 {
        return None;
    }
    let hour = hours.parse::<u16>().ok()?;
    let minute = minutes.parse::<u16>().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some(hour * 60 + minute)
}

fn parse_schedule_exception_date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d").ok()
}

fn format_hhmm(total_minutes: u16) -> String {
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    format!("{hours:02}:{minutes:02}")
}

fn sort_schedule_days(days: &mut Vec<String>) {
    let mut sorted = Vec::new();
    for token in SCHEDULE_DAY_TOKENS {
        if days.iter().any(|day| day.eq_ignore_ascii_case(token)) {
            sorted.push(token.to_string());
        }
    }
    if sorted.is_empty() {
        sorted.push(SCHEDULE_DAY_TOKENS[0].to_string());
    }
    *days = sorted;
}

fn sort_schedule_exception_dates(dates: &mut Vec<String>) {
    let mut sorted = BTreeSet::new();
    for value in dates.iter() {
        let Some(date) = parse_schedule_exception_date(value) else {
            continue;
        };
        sorted.insert(date.format("%Y-%m-%d").to_string());
    }
    *dates = sorted.into_iter().collect();
}

fn format_schedule_days_for_display(days: &[String]) -> String {
    let mut labels = Vec::new();
    for (index, token) in SCHEDULE_DAY_TOKENS.iter().enumerate() {
        if days.iter().any(|day| day.eq_ignore_ascii_case(token)) {
            labels.push(SCHEDULE_DAY_LABELS[index]);
        }
    }
    if labels.is_empty() {
        "none".to_string()
    } else {
        labels.join(",")
    }
}

fn schedule_next_window_text_from_state(
    state: &ScheduleDisplayState,
    now: DateTime<Local>,
) -> String {
    if !state.has_schedule_windows {
        return "🗓  Next schedule: none configured".to_string();
    }

    if let Some(active) = state.active_window.as_ref() {
        let mut text = format!(
            "🗓  Next schedule: in progress until {}",
            active.end.format("%H:%M")
        );
        if let Some(next) = state.next_window.as_ref() {
            text.push_str(&format!(
                " · then {}",
                format_schedule_occurrence_for_display(next, now)
            ));
        }
        return text;
    }

    if let Some(next) = state.next_window.as_ref() {
        return format!(
            "🗓  Next schedule: {}",
            format_schedule_occurrence_for_display(next, now)
        );
    }

    "🗓  Next schedule: no upcoming window".to_string()
}

fn schedule_status_text_from_state(state: &ScheduleDisplayState) -> String {
    if !state.has_schedule_windows {
        return "⚙  Schedule status: off".to_string();
    }

    if state.is_exception_today {
        return "⚙  Schedule status: skipped today (exception date)".to_string();
    }

    if state.active_window.is_some() {
        return schedule_active_window_status_text(state);
    }

    if state.is_armed {
        return schedule_armed_status_text(state.has_selected_task);
    }

    "⚙  Schedule status: ready for next window".to_string()
}

fn schedule_active_window_status_text(state: &ScheduleDisplayState) -> String {
    if state.timer_phase != TimerPhase::Focus {
        return "⚙  Schedule status: window active; press [n] to switch to focus".to_string();
    }

    match state.timer_status {
        TimerStatus::Running => "⚙  Schedule status: in window; focus running".to_string(),
        TimerStatus::Paused => {
            "⚙  Schedule status: window active; press [Space] to resume focus".to_string()
        }
        TimerStatus::Idle => {
            schedule_idle_focus_status_text(state.has_selected_task, state.is_armed)
        }
    }
}

fn schedule_idle_focus_status_text(has_selected_task: bool, is_armed: bool) -> String {
    if !has_selected_task {
        "⚙  Schedule status: window active; select [t], then press [Space]".to_string()
    } else if is_armed {
        "⚙  Schedule status: armed; press [Space] to start focus".to_string()
    } else {
        "⚙  Schedule status: window active; press [Space] to start focus".to_string()
    }
}

fn schedule_armed_status_text(has_selected_task: bool) -> String {
    if has_selected_task {
        "⚙  Schedule status: armed; press [Space] to start focus".to_string()
    } else {
        "⚙  Schedule status: armed; select [t], then press [Space]".to_string()
    }
}

fn format_schedule_occurrence_for_display(
    occurrence: &WindowOccurrence,
    now: DateTime<Local>,
) -> String {
    let start_date = occurrence.start.date_naive();
    let today = now.date_naive();
    let day = if start_date == today {
        "today".to_string()
    } else if today.succ_opt() == Some(start_date) {
        "tomorrow".to_string()
    } else {
        occurrence.start.format("%a").to_string()
    };
    format!(
        "{day} {}-{}",
        occurrence.start.format("%H:%M"),
        occurrence.end.format("%H:%M")
    )
}

fn parse_day_key(day_key: &str) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(day_key, "%Y-%m-%d").ok()
}

fn goal_progress(completed: u64, target: u64) -> GoalProgress {
    let ratio = if target == 0 {
        0.0
    } else {
        (completed as f64 / target as f64).clamp(0.0, 1.0)
    };
    GoalProgress {
        completed,
        target,
        ratio,
    }
}

fn ceil_duration_secs(duration: Duration) -> u64 {
    let secs = duration.as_secs();
    if duration.subsec_nanos() > 0 {
        secs.saturating_add(1)
    } else {
        secs
    }
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

fn current_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

impl Drop for App {
    fn drop(&mut self) {
        self.flush_stats_if_dirty(true);
        // Ensure hosts-file block entries are removed on every exit path,
        // including early returns caused by I/O errors in run_app.
        self.blocker.cleanup();
    }
}

fn blocking_permissions_check(hosts_diagnostics: &HostsFileDiagnostics) -> SetupCheck {
    if hosts_diagnostics.can_write() {
        SetupCheck::ok("Ready: hosts file can be opened for write access")
    } else {
        let reason = hosts_diagnostics
            .write_error
            .as_deref()
            .unwrap_or("unknown write error");
        SetupCheck::warning(format!("Blocked: write permission unavailable ({reason})"))
    }
}

fn hosts_write_capability_check(hosts_diagnostics: &HostsFileDiagnostics) -> SetupCheck {
    let can_read = hosts_diagnostics.can_read();
    let can_write = hosts_diagnostics.can_write();
    match (
        can_read,
        can_write,
        hosts_diagnostics.read_error.as_deref(),
        hosts_diagnostics.write_error.as_deref(),
    ) {
        (true, true, _, _) => SetupCheck::ok("Ready: hosts file is readable and writable"),
        (false, true, Some(read_error), _) => {
            SetupCheck::warning(format!("Blocked: cannot read hosts file ({read_error})"))
        }
        (true, false, _, Some(write_error)) => {
            SetupCheck::warning(format!("Blocked: cannot write hosts file ({write_error})"))
        }
        (false, false, Some(read_error), Some(write_error)) => SetupCheck::warning(format!(
            "Blocked: read error ({read_error}); write error ({write_error})"
        )),
        _ => SetupCheck::warning("Blocked: hosts access diagnostics unavailable"),
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn should_handle_key(key: &KeyEvent) -> bool {
    key.kind == KeyEventKind::Press
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_recovery::{
        self, InProgressSessionSnapshot, RecoveryTimerPhase, RecoveryTimerStatus,
    };
    use chrono::{Datelike, Duration as ChronoDuration, Local, LocalResult, TimeZone, Weekday};
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    fn local_datetime_today(hour: u32, minute: u32) -> DateTime<Local> {
        let today = Local::now().date_naive();
        match Local.with_ymd_and_hms(today.year(), today.month(), today.day(), hour, minute, 0) {
            LocalResult::Single(dt) => dt,
            LocalResult::Ambiguous(earliest, _) => earliest,
            LocalResult::None => panic!("local datetime should be representable in tests"),
        }
    }

    fn weekday_token(day: Weekday) -> &'static str {
        match day {
            Weekday::Mon => "mon",
            Weekday::Tue => "tue",
            Weekday::Wed => "wed",
            Weekday::Thu => "thu",
            Weekday::Fri => "fri",
            Weekday::Sat => "sat",
            Weekday::Sun => "sun",
        }
    }

    fn snapshot_for_tests(
        phase: TimerPhase,
        status: TimerStatus,
        remaining_secs: u64,
        task_label: Option<&str>,
        selected_profile: ProfileId,
    ) -> InProgressSessionSnapshot {
        InProgressSessionSnapshot {
            phase: RecoveryTimerPhase::from_timer_phase(phase),
            status: RecoveryTimerStatus::from_timer_status(status),
            remaining_secs,
            pomodoros_completed: 0,
            selected_task_label: task_label.map(str::to_string),
            selected_profile,
        }
    }

    #[test]
    fn app_default_uses_canonical_config_in_tests() {
        let app = App::default();

        assert!(app.blocker.sites.is_empty());
        assert_eq!(app.blocklist_profile_count(), 1);
        assert_eq!(app.active_blocklist_profile_name(), "Default");
        assert_eq!(app.timer.focus_secs, DEFAULT_FOCUS_SECS);
        assert_eq!(app.timer.short_break_secs, DEFAULT_SHORT_BREAK_SECS);
        assert_eq!(app.timer.long_break_secs, DEFAULT_LONG_BREAK_SECS);
        assert_eq!(app.timer.long_break_interval, DEFAULT_LONG_BREAK_INTERVAL);
        assert_eq!(app.auto_start, AutoStartConfig::default());
        assert_eq!(app.recurring_schedule, RecurringScheduleConfig::default());
        assert!(!app.strict_mode);
        assert_eq!(app.daily_goal, DailyGoalConfig::default());
    }

    #[test]
    fn selected_builtin_profile_is_applied_on_startup() {
        let config = AppConfig {
            focus_secs: 10 * 60,
            short_break_secs: 2 * 60,
            long_break_secs: 8 * 60,
            long_break_interval: 2,
            blocked_sites: Vec::new(),
            blocklist_profiles: vec![BlocklistProfileConfig::default()],
            selected_blocklist_profile: "Default".to_string(),
            selected_profile: ProfileId::Classic,
            custom_profile: Some(CustomProfileConfig {
                focus_secs: 40 * 60,
                short_break_secs: 8 * 60,
                long_break_secs: 16 * 60,
                long_break_interval: 2,
            }),
            notifications: NotificationConfig::default(),
            auto_start: AutoStartConfig::default(),
            recurring_schedule: RecurringScheduleConfig::default(),
            strict_mode: false,
            break_glass_duration_secs: 5 * 60,
            daily_goal: DailyGoalConfig::default(),
            wakatime: WakatimeMetadataConfig::default(),
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
    fn export_stats_to_dir_writes_files_and_reports_success() {
        let mut app = App::default();
        app.insert_daily_stats_for_tests(
            "2026-04-06",
            DailyStats {
                pomodoros_completed: 2,
                focused_seconds: 50 * 60,
                goal: Some(DailyGoalSnapshot {
                    minutes: 25,
                    pomodoros: 1,
                }),
            },
        );

        let export_dir = unique_temp_dir("app-export");
        app.export_stats_to_dir(&export_dir);

        let feedback = app.history_feedback.as_ref().unwrap();
        assert_eq!(feedback.level, HistoryFeedbackLevel::Success);
        assert!(feedback.message.contains("Exported to "));
        assert!(!feedback.message.contains('\n'));
        assert!(feedback.message.contains("JSON focustime-stats.json"));
        assert!(feedback.message.contains("CSV focustime-stats.csv"));
        assert!(feedback.message.contains("focustime-stats.json"));
        assert!(feedback.message.contains("focustime-stats.csv"));
        assert!(export_dir.join("focustime-stats.json").exists());
        assert!(export_dir.join("focustime-stats.csv").exists());

        fs::remove_dir_all(export_dir).unwrap();
    }

    #[test]
    fn export_stats_to_dir_reports_warning_for_invalid_target() {
        let mut app = App::default();
        let export_root = unique_temp_dir("app-export-error");
        let not_a_directory = export_root.join("occupied-file");
        fs::write(&not_a_directory, "occupied").unwrap();

        app.export_stats_to_dir(&not_a_directory);

        let feedback = app.history_feedback.as_ref().unwrap();
        assert_eq!(feedback.level, HistoryFeedbackLevel::Warning);
        assert!(feedback.message.starts_with("Export failed:"));

        fs::remove_dir_all(export_root).unwrap();
    }

    fn unique_temp_dir(label: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("focustime-{label}-{}-{unique}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path
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
            strict_mode: true,
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
        assert_eq!(persisted.auto_start, AutoStartConfig::default());
        assert!(persisted.strict_mode);
        assert_eq!(persisted.break_glass_duration_secs, 5 * 60);
        assert_eq!(persisted.daily_goal, DailyGoalConfig::default());
        assert_eq!(persisted.wakatime, WakatimeMetadataConfig::default());
        assert_eq!(persisted.selected_blocklist_profile, "Default");
        assert_eq!(persisted.blocklist_profiles.len(), 1);
        assert_eq!(persisted.blocklist_profiles[0].name, "Default");
    }

    #[test]
    fn persisted_config_preserves_wakatime_metadata() {
        let config = AppConfig {
            wakatime: WakatimeMetadataConfig {
                project: "Team Focus".to_string(),
                language: "Deep Work".to_string(),
            },
            ..AppConfig::default()
        };
        let app = App::from_config(config);

        let persisted = app.persisted_config();
        assert_eq!(
            persisted.wakatime,
            WakatimeMetadataConfig {
                project: "Team Focus".to_string(),
                language: "Deep Work".to_string(),
            }
        );
    }

    #[test]
    fn persisted_config_seeds_fallback_profile_with_active_sites() {
        let mut app = App::default();
        app.blocklist_profiles.clear();
        app.blocker.sites = vec!["example.com".to_string(), "github.com".to_string()];

        let persisted = app.persisted_config();

        assert_eq!(persisted.selected_blocklist_profile, "Default");
        assert_eq!(persisted.blocklist_profiles.len(), 1);
        assert_eq!(persisted.blocklist_profiles[0].name, "Default");
        assert_eq!(
            persisted.blocklist_profiles[0].sites,
            vec!["example.com".to_string(), "github.com".to_string()]
        );
        assert_eq!(
            persisted.blocked_sites,
            vec!["example.com".to_string(), "github.com".to_string()]
        );
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
        assert_eq!(app.profile_edit_field_value(6), "Off");
        assert_eq!(app.profile_edit_field_value(7), "Off");
        assert_eq!(app.profile_edit_field_value(8), "Off");
        assert_eq!(app.profile_edit_field_value(9), "Off");
        assert_eq!(app.profile_edit_field_value(10), "Off");
        assert_eq!(app.profile_edit_field_value(11), "focustime");
        assert_eq!(app.profile_edit_field_value(12), "Pomodoro");
    }

    #[test]
    fn editing_strict_mode_field_updates_and_persists_setting() {
        let mut app = App::default();

        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Char('e')));
        for _ in 0..8 {
            app.handle_key(key(KeyCode::Down));
        }
        app.handle_key(key(KeyCode::Right));
        app.handle_key(key(KeyCode::Enter));

        assert!(app.strict_mode);
        let persisted = app.persisted_config();
        assert!(persisted.strict_mode);
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
    fn editing_auto_start_fields_updates_and_persists_settings() {
        let mut app = App::default();

        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Char('e')));
        for _ in 0..6 {
            app.handle_key(key(KeyCode::Down));
        }
        app.handle_key(key(KeyCode::Right)); // auto-start break -> On
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Right)); // auto-start focus -> On
        app.handle_key(key(KeyCode::Enter));

        assert!(app.auto_start.focus_to_break);
        assert!(app.auto_start.break_to_focus);

        let persisted = app.persisted_config();
        assert!(persisted.auto_start.focus_to_break);
        assert!(persisted.auto_start.break_to_focus);
    }

    #[test]
    fn editing_recurring_schedule_fields_updates_and_persists_settings() {
        let mut app = App::default();

        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Char('e')));
        app.profile_edit_field = PROFILE_EDIT_SCHEDULE_ADD_REMOVE_INDEX;
        app.handle_key(key(KeyCode::Right)); // add default window
        app.profile_edit_schedule_day = 6; // Sun
        app.profile_edit_field = PROFILE_EDIT_SCHEDULE_DAY_ENABLED_INDEX;
        app.handle_key(key(KeyCode::Right)); // enable Sunday
        app.profile_edit_field = PROFILE_EDIT_SCHEDULE_START_INDEX;
        app.handle_key(key(KeyCode::Right)); // 09:00 -> 09:15
        app.profile_edit_field = PROFILE_EDIT_SCHEDULE_END_INDEX;
        app.handle_key(key(KeyCode::Left)); // 10:00 -> 09:45
        app.handle_key(key(KeyCode::Enter));

        assert_eq!(app.recurring_schedule.windows.len(), 1);
        let window = &app.recurring_schedule.windows[0];
        assert_eq!(window.start, "09:15");
        assert_eq!(window.end, "09:45");
        assert!(
            window
                .days
                .iter()
                .any(|day| day.eq_ignore_ascii_case("sun"))
        );

        let persisted = app.persisted_config();
        assert_eq!(persisted.recurring_schedule, app.recurring_schedule);
    }

    #[test]
    fn editing_schedule_exception_fields_updates_and_persists_settings() {
        let mut app = App::default();
        let base_date = local_datetime_today(10, 15).date_naive();
        app.current_frame_now = local_datetime_today(10, 15);

        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Char('e')));
        app.profile_edit_field = PROFILE_EDIT_SCHEDULE_EXCEPTION_ADD_REMOVE_INDEX;
        app.handle_key(key(KeyCode::Right)); // add exception on base date
        app.profile_edit_field = PROFILE_EDIT_SCHEDULE_EXCEPTION_DATE_INDEX;
        app.handle_key(key(KeyCode::Right)); // shift to next day
        app.handle_key(key(KeyCode::Enter));

        let expected_date = base_date
            .succ_opt()
            .expect("next day should be representable")
            .format("%Y-%m-%d")
            .to_string();
        assert_eq!(
            app.recurring_schedule.exception_dates,
            vec![expected_date.clone()]
        );

        let persisted = app.persisted_config();
        assert_eq!(
            persisted.recurring_schedule.exception_dates,
            vec![expected_date]
        );
    }

    #[test]
    fn editing_daily_goal_fields_updates_and_persists_settings() {
        let mut app = App::default();

        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Char('e')));
        for _ in 0..9 {
            app.handle_key(key(KeyCode::Down));
        }
        app.handle_key(key(KeyCode::Right)); // minutes goal -> 5m
        app.handle_key(key(KeyCode::Right)); // minutes goal -> 10m
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Right)); // pomodoros goal -> 1
        app.handle_key(key(KeyCode::Enter));

        assert_eq!(app.daily_goal.minutes, 10);
        assert_eq!(app.daily_goal.pomodoros, 1);

        let persisted = app.persisted_config();
        assert_eq!(persisted.daily_goal.minutes, 10);
        assert_eq!(persisted.daily_goal.pomodoros, 1);
    }

    #[test]
    fn cancelling_profile_edit_restores_recurring_schedule_settings() {
        let original_schedule = RecurringScheduleConfig {
            windows: vec![RecurringFocusWindowConfig {
                days: vec!["tue".to_string(), "thu".to_string()],
                start: "13:00".to_string(),
                end: "14:30".to_string(),
            }],
            ..RecurringScheduleConfig::default()
        };
        let config = AppConfig {
            recurring_schedule: original_schedule.clone(),
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);

        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Char('e')));
        app.profile_edit_field = PROFILE_EDIT_SCHEDULE_ADD_REMOVE_INDEX;
        app.handle_key(key(KeyCode::Right)); // add extra window
        app.profile_edit_field = PROFILE_EDIT_SCHEDULE_START_INDEX;
        app.handle_key(key(KeyCode::Right));
        app.handle_key(key(KeyCode::Esc)); // cancel edit

        assert_eq!(app.recurring_schedule, original_schedule);
        assert_eq!(app.recurring_windows.len(), 1);
    }

    #[test]
    fn editing_wakatime_metadata_fields_updates_and_persists_settings() {
        let config = AppConfig {
            wakatime: WakatimeMetadataConfig {
                project: "A".to_string(),
                language: "B".to_string(),
            },
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);

        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Char('e')));
        for _ in 0..11 {
            app.handle_key(key(KeyCode::Down));
        }
        app.handle_key(key(KeyCode::Backspace));
        for c in "Team Focus".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Backspace));
        for c in "Deep Work".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));

        let persisted = app.persisted_config();
        assert_eq!(
            persisted.wakatime,
            WakatimeMetadataConfig {
                project: "Team Focus".to_string(),
                language: "Deep Work".to_string(),
            }
        );
        assert_eq!(
            app.wakatime.heartbeat_metadata_for_tests(),
            WakatimeHeartbeatMetadata {
                project: "Team Focus".to_string(),
                language: "Deep Work".to_string(),
            }
        );
    }

    #[test]
    fn editing_wakatime_metadata_blank_values_fall_back_to_defaults() {
        let config = AppConfig {
            wakatime: WakatimeMetadataConfig {
                project: "A".to_string(),
                language: "B".to_string(),
            },
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);

        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Char('e')));
        for _ in 0..11 {
            app.handle_key(key(KeyCode::Down));
        }
        app.handle_key(key(KeyCode::Backspace));
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Backspace));
        app.handle_key(key(KeyCode::Enter));

        let defaults = WakatimeMetadataConfig::default();
        assert_eq!(app.wakatime_metadata, defaults);
        assert_eq!(app.persisted_config().wakatime, defaults);
        assert_eq!(
            app.wakatime.heartbeat_metadata_for_tests(),
            WakatimeHeartbeatMetadata::default()
        );
    }

    #[test]
    fn cancelling_profile_edit_restores_daily_goal_settings() {
        let config = AppConfig {
            daily_goal: DailyGoalConfig {
                minutes: 25,
                pomodoros: 3,
            },
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);

        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Char('e')));
        for _ in 0..9 {
            app.handle_key(key(KeyCode::Down));
        }
        app.handle_key(key(KeyCode::Right)); // minutes goal -> 30m
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Right)); // pomodoros goal -> 4
        app.handle_key(key(KeyCode::Esc)); // cancel

        assert_eq!(app.daily_goal.minutes, 25);
        assert_eq!(app.daily_goal.pomodoros, 3);
    }

    #[test]
    fn cancelling_profile_edit_restores_auto_start_settings() {
        let config = AppConfig {
            auto_start: AutoStartConfig {
                focus_to_break: true,
                break_to_focus: true,
            },
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);

        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Char('e')));
        for _ in 0..6 {
            app.handle_key(key(KeyCode::Down));
        }
        app.handle_key(key(KeyCode::Left)); // auto-start break -> Off
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Left)); // auto-start focus -> Off
        app.handle_key(key(KeyCode::Esc)); // cancel

        assert!(!app.profile_edit_active);
        assert!(app.auto_start.focus_to_break);
        assert!(app.auto_start.break_to_focus);
    }

    #[test]
    fn today_goal_progress_reports_ratios_for_minutes_and_pomodoros() {
        let config = AppConfig {
            daily_goal: DailyGoalConfig {
                minutes: 60,
                pomodoros: 4,
            },
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        let day_key = current_day_key();
        app.stats
            .record_focus_elapsed(&day_key, 30 * 60, app.current_goal_snapshot());
        app.stats
            .record_completed_pomodoro(&day_key, app.current_goal_snapshot());

        let progress = app.today_goal_progress();
        assert_eq!(progress.minutes.completed, 30);
        assert_eq!(progress.minutes.target, 60);
        assert!((progress.minutes.ratio - 0.5).abs() < f64::EPSILON);
        assert_eq!(progress.pomodoros.completed, 1);
        assert_eq!(progress.pomodoros.target, 4);
        assert!((progress.pomodoros.ratio - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn goal_streak_counts_yesterday_until_today_is_missed() {
        let config = AppConfig {
            daily_goal: DailyGoalConfig {
                minutes: 60,
                pomodoros: 1,
            },
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        let goal = app.current_goal_snapshot();
        let yesterday = chrono::Local::now().date_naive().pred_opt().unwrap();
        let day_before = yesterday.pred_opt().unwrap();

        for day in [day_before, yesterday] {
            let day_key = day.format("%Y-%m-%d").to_string();
            app.stats.record_focus_elapsed(&day_key, 60 * 60, goal);
            app.stats.record_completed_pomodoro(&day_key, goal);
        }

        let streak = app.goal_streak();
        assert_eq!(streak.current, 2);
        assert_eq!(streak.best, 2);
    }

    #[test]
    fn committing_goal_edit_updates_today_goal_snapshot() {
        let config = AppConfig {
            daily_goal: DailyGoalConfig {
                minutes: 60,
                pomodoros: 1,
            },
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        let day_key = current_day_key();

        app.stats
            .record_focus_elapsed(&day_key, 30 * 60, app.current_goal_snapshot());

        app.begin_profile_edit();
        app.daily_goal.minutes = 90;
        app.commit_profile_edit();

        assert_eq!(
            app.today_stats().goal,
            Some(DailyGoalSnapshot {
                minutes: 90,
                pomodoros: 1,
            })
        );
    }

    #[test]
    fn goal_streak_for_day_key_uses_the_same_day_for_date_and_stats_lookup() {
        let config = AppConfig {
            daily_goal: DailyGoalConfig {
                minutes: 60,
                pomodoros: 1,
            },
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        let goal = app.current_goal_snapshot();

        for day_key in ["2026-04-08", "2026-04-09"] {
            app.stats.record_focus_elapsed(day_key, 60 * 60, goal);
            app.stats.record_completed_pomodoro(day_key, goal);
        }

        let streak = app.goal_streak_for_day_key("2026-04-09");
        assert_eq!(streak.current, 2);
        assert_eq!(streak.best, 2);
    }

    #[test]
    fn goal_streak_for_day_key_fails_closed_for_invalid_day_keys() {
        let app = App::from_config(AppConfig {
            daily_goal: DailyGoalConfig {
                minutes: 60,
                pomodoros: 1,
            },
            ..AppConfig::default()
        });

        let streak = app.goal_streak_for_day_key("not-a-day");
        assert_eq!(streak, GoalStreak::default());
    }

    #[test]
    fn goal_streak_for_day_key_handles_legacy_entries_without_goal_snapshots() {
        let config = AppConfig {
            daily_goal: DailyGoalConfig {
                minutes: 60,
                pomodoros: 1,
            },
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);

        app.stats.insert_daily_for_tests(
            "2026-04-08",
            DailyStats {
                pomodoros_completed: 1,
                focused_seconds: 60 * 60,
                goal: None,
            },
        );
        app.stats.insert_daily_for_tests(
            "2026-04-09",
            DailyStats {
                pomodoros_completed: 1,
                focused_seconds: 60 * 60,
                goal: None,
            },
        );

        let streak = app.goal_streak_for_day_key("2026-04-09");
        assert_eq!(streak.current, 2);
        assert_eq!(streak.best, 2);
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
    fn cancelling_profile_edit_restores_wakatime_metadata() {
        let config = AppConfig {
            wakatime: WakatimeMetadataConfig {
                project: "Team Focus".to_string(),
                language: "Deep Work".to_string(),
            },
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);

        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Char('e')));
        for _ in 0..11 {
            app.handle_key(key(KeyCode::Down));
        }
        app.handle_key(key(KeyCode::Char('X')));
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Char('Y')));
        app.handle_key(key(KeyCode::Esc));

        assert_eq!(
            app.wakatime_metadata,
            WakatimeMetadataConfig {
                project: "Team Focus".to_string(),
                language: "Deep Work".to_string(),
            }
        );
        assert_eq!(
            app.wakatime.heartbeat_metadata_for_tests(),
            WakatimeHeartbeatMetadata {
                project: "Team Focus".to_string(),
                language: "Deep Work".to_string(),
            }
        );
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
    fn site_manager_paste_targets_blocklist_profile_input_when_active() {
        let mut app = App::default();
        app.handle_key(key(KeyCode::Char('b')));
        app.handle_key(key(KeyCode::Char('n')));

        app.handle_paste("Work".to_string());

        assert!(app.blocklist_profile_input_active);
        assert_eq!(app.blocklist_profile_input, "Work");
        assert!(!app.site_input_active);
        assert!(app.site_input.is_empty());
    }

    #[test]
    fn site_manager_input_modes_are_mutually_exclusive() {
        let mut app = App::default();
        app.mode = AppMode::SiteManager;

        app.start_blocklist_profile_input(BlocklistProfileInputMode::Create);
        assert!(app.blocklist_profile_input_active);
        assert!(!app.site_input_active);

        app.start_site_input(SiteInputMode::Add);
        assert!(app.site_input_active);
        assert!(!app.blocklist_profile_input_active);
        assert!(app.blocklist_profile_input.is_empty());

        app.start_blocklist_profile_input(BlocklistProfileInputMode::Rename);
        assert!(app.blocklist_profile_input_active);
        assert!(!app.site_input_active);
        assert!(app.site_input.is_empty());
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
    fn site_manager_switches_between_blocklist_profiles() {
        let config = AppConfig {
            blocklist_profiles: vec![
                BlocklistProfileConfig {
                    name: "Work".to_string(),
                    sites: vec!["a.com".to_string()],
                },
                BlocklistProfileConfig {
                    name: "Study".to_string(),
                    sites: vec!["b.com".to_string(), "c.com".to_string()],
                },
            ],
            selected_blocklist_profile: "Work".to_string(),
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.handle_key(key(KeyCode::Char('b')));

        assert_eq!(app.active_blocklist_profile_name(), "Work");
        assert_eq!(app.blocker.sites, vec!["a.com".to_string()]);

        app.handle_key(key(KeyCode::Char(']')));

        assert_eq!(app.active_blocklist_profile_name(), "Study");
        assert_eq!(
            app.blocker.sites,
            vec!["b.com".to_string(), "c.com".to_string()]
        );
    }

    #[test]
    fn site_manager_create_rename_and_delete_blocklist_profile() {
        let mut app = App::default();
        app.handle_key(key(KeyCode::Char('b')));

        app.handle_key(key(KeyCode::Char('n')));
        for c in "Work".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));

        assert_eq!(app.blocklist_profile_count(), 2);
        assert_eq!(app.active_blocklist_profile_name(), "Work");
        assert!(app.blocker.sites.is_empty());

        app.handle_key(key(KeyCode::Char('r')));
        for _ in 0.."Work".len() {
            app.handle_key(key(KeyCode::Backspace));
        }
        for c in "Deep Work".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));

        assert_eq!(app.active_blocklist_profile_name(), "Deep Work");

        app.handle_key(key(KeyCode::Char('x')));

        assert_eq!(app.blocklist_profile_count(), 1);
        assert_eq!(app.active_blocklist_profile_name(), "Default");
    }

    #[test]
    fn persisted_config_mirrors_active_profile_sites_to_legacy_blocked_sites() {
        let mut app = App::default();
        app.handle_key(key(KeyCode::Char('b')));
        app.handle_key(key(KeyCode::Char('a')));
        for c in "example.com".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));

        let persisted = app.persisted_config();
        assert_eq!(persisted.selected_blocklist_profile, "Default");
        assert_eq!(persisted.blocked_sites, vec!["example.com".to_string()]);
        assert_eq!(persisted.blocklist_profiles.len(), 1);
        assert_eq!(
            persisted.blocklist_profiles[0].sites,
            vec!["example.com".to_string()]
        );
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
        assert_eq!(app.timer.status, TimerStatus::Idle);
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
    fn natural_focus_completion_auto_starts_break_when_enabled() {
        let config = AppConfig {
            auto_start: AutoStartConfig {
                focus_to_break: true,
                break_to_focus: false,
            },
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Running;
        app.timer.remaining_secs = 1;

        app.on_tick(false);

        assert_eq!(app.timer.phase, TimerPhase::ShortBreak);
        assert_eq!(app.timer.status, TimerStatus::Running);
    }

    #[test]
    fn natural_break_completion_auto_starts_focus_when_enabled() {
        let config = AppConfig {
            auto_start: AutoStartConfig {
                focus_to_break: false,
                break_to_focus: true,
            },
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.task_labels = vec!["Auto Task".to_string()];
        app.selected_task_label = Some("Auto Task".to_string());
        app.timer.phase = TimerPhase::ShortBreak;
        app.timer.status = TimerStatus::Running;
        app.timer.remaining_secs = 1;

        app.on_tick(false);

        assert_eq!(app.timer.phase, TimerPhase::Focus);
        assert_eq!(app.timer.status, TimerStatus::Running);
    }

    #[test]
    fn manual_skip_keeps_next_phase_idle_when_auto_start_is_enabled() {
        let config = AppConfig {
            auto_start: AutoStartConfig {
                focus_to_break: true,
                break_to_focus: true,
            },
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);

        app.handle_key(key(KeyCode::Char('n')));

        assert_eq!(app.timer.phase, TimerPhase::ShortBreak);
        assert_eq!(app.timer.status, TimerStatus::Idle);
        assert!(app.phase_notification.is_none());
    }

    #[test]
    fn recurring_schedule_next_window_text_shows_upcoming_window_for_today() {
        let now = local_datetime_today(10, 15);
        let config = AppConfig {
            recurring_schedule: RecurringScheduleConfig {
                windows: vec![crate::config::RecurringFocusWindowConfig {
                    days: vec![weekday_token(now.weekday()).to_string()],
                    start: "11:00".to_string(),
                    end: "12:00".to_string(),
                }],
                ..RecurringScheduleConfig::default()
            },
            ..AppConfig::default()
        };
        let app = App::from_config(config);

        assert_eq!(
            app.recurring_schedule_texts_at(now).0,
            "🗓  Next schedule: today 11:00-12:00"
        );
    }

    #[test]
    fn recurring_schedule_next_window_text_shows_active_window_then_next_window() {
        let now = local_datetime_today(10, 15);
        let config = AppConfig {
            recurring_schedule: RecurringScheduleConfig {
                windows: vec![
                    crate::config::RecurringFocusWindowConfig {
                        days: vec![weekday_token(now.weekday()).to_string()],
                        start: "10:00".to_string(),
                        end: "11:00".to_string(),
                    },
                    crate::config::RecurringFocusWindowConfig {
                        days: vec![weekday_token(now.weekday()).to_string()],
                        start: "14:00".to_string(),
                        end: "15:00".to_string(),
                    },
                ],
                ..RecurringScheduleConfig::default()
            },
            ..AppConfig::default()
        };
        let app = App::from_config(config);

        assert_eq!(
            app.recurring_schedule_texts_at(now).0,
            "🗓  Next schedule: in progress until 11:00 · then today 14:00-15:00"
        );
    }

    #[test]
    fn recurring_schedule_display_texts_use_current_frame_timestamp() {
        let simulated_now = local_datetime_today(10, 15) + ChronoDuration::days(1);
        let config = AppConfig {
            recurring_schedule: RecurringScheduleConfig {
                windows: vec![crate::config::RecurringFocusWindowConfig {
                    days: vec![weekday_token(simulated_now.weekday()).to_string()],
                    start: "10:00".to_string(),
                    end: "11:00".to_string(),
                }],
                ..RecurringScheduleConfig::default()
            },
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.current_frame_now = simulated_now;
        app.task_labels.clear();
        app.selected_task_label = None;
        app.schedule_armed_occurrence_key = None;

        let (next_text, status_text) = app.recurring_schedule_display_texts();
        assert!(next_text.starts_with("🗓  Next schedule: in progress until 11:00"));
        assert_eq!(
            status_text,
            "⚙  Schedule status: window active; select [t], then press [Space]"
        );
    }

    #[test]
    fn recurring_schedule_status_text_guides_task_selection_when_active_and_armed() {
        let now = local_datetime_today(10, 15);
        let config = AppConfig {
            recurring_schedule: RecurringScheduleConfig {
                windows: vec![crate::config::RecurringFocusWindowConfig {
                    days: vec![weekday_token(now.weekday()).to_string()],
                    start: "10:00".to_string(),
                    end: "11:00".to_string(),
                }],
                ..RecurringScheduleConfig::default()
            },
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.sync_recurring_schedule(now);

        assert_eq!(
            app.recurring_schedule_texts_at(now).1,
            "⚙  Schedule status: window active; select [t], then press [Space]"
        );
    }

    #[test]
    fn recurring_schedule_status_text_guides_start_when_active_and_idle() {
        let now = local_datetime_today(10, 15);
        let config = AppConfig {
            recurring_schedule: RecurringScheduleConfig {
                windows: vec![crate::config::RecurringFocusWindowConfig {
                    days: vec![weekday_token(now.weekday()).to_string()],
                    start: "10:00".to_string(),
                    end: "11:00".to_string(),
                }],
                ..RecurringScheduleConfig::default()
            },
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.task_labels = vec!["Coding".to_string()];
        app.selected_task_label = Some("Coding".to_string());

        assert_eq!(
            app.recurring_schedule_texts_at(now).1,
            "⚙  Schedule status: window active; press [Space] to start focus"
        );
    }

    #[test]
    fn recurring_schedule_status_text_guides_resume_when_active_and_paused() {
        let now = local_datetime_today(10, 15);
        let config = AppConfig {
            recurring_schedule: RecurringScheduleConfig {
                windows: vec![crate::config::RecurringFocusWindowConfig {
                    days: vec![weekday_token(now.weekday()).to_string()],
                    start: "10:00".to_string(),
                    end: "11:00".to_string(),
                }],
                ..RecurringScheduleConfig::default()
            },
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.task_labels = vec!["Coding".to_string()];
        app.selected_task_label = Some("Coding".to_string());
        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Paused;

        assert_eq!(
            app.recurring_schedule_texts_at(now).1,
            "⚙  Schedule status: window active; press [Space] to resume focus"
        );
    }

    #[test]
    fn recurring_schedule_status_text_guides_switch_to_focus_when_active_in_break() {
        let now = local_datetime_today(10, 15);
        let config = AppConfig {
            recurring_schedule: RecurringScheduleConfig {
                windows: vec![crate::config::RecurringFocusWindowConfig {
                    days: vec![weekday_token(now.weekday()).to_string()],
                    start: "10:00".to_string(),
                    end: "11:00".to_string(),
                }],
                ..RecurringScheduleConfig::default()
            },
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.timer.phase = TimerPhase::ShortBreak;
        app.timer.status = TimerStatus::Running;

        assert_eq!(
            app.recurring_schedule_texts_at(now).1,
            "⚙  Schedule status: window active; press [n] to switch to focus"
        );
    }

    #[test]
    fn recurring_schedule_status_text_shows_ready_for_upcoming_window() {
        let now = local_datetime_today(10, 15);
        let config = AppConfig {
            recurring_schedule: RecurringScheduleConfig {
                windows: vec![crate::config::RecurringFocusWindowConfig {
                    days: vec![weekday_token(now.weekday()).to_string()],
                    start: "11:00".to_string(),
                    end: "12:00".to_string(),
                }],
                ..RecurringScheduleConfig::default()
            },
            ..AppConfig::default()
        };
        let app = App::from_config(config);

        assert_eq!(
            app.recurring_schedule_texts_at(now).1,
            "⚙  Schedule status: ready for next window"
        );
    }

    #[test]
    fn recurring_schedule_status_text_shows_exception_skip_for_today() {
        let now = local_datetime_today(10, 15);
        let today = now.date_naive();
        let tomorrow = today.succ_opt().expect("tomorrow should be representable");
        let config = AppConfig {
            recurring_schedule: RecurringScheduleConfig {
                windows: vec![
                    crate::config::RecurringFocusWindowConfig {
                        days: vec![weekday_token(today.weekday()).to_string()],
                        start: "10:00".to_string(),
                        end: "11:00".to_string(),
                    },
                    crate::config::RecurringFocusWindowConfig {
                        days: vec![weekday_token(tomorrow.weekday()).to_string()],
                        start: "09:00".to_string(),
                        end: "10:00".to_string(),
                    },
                ],
                exception_dates: vec![today.format("%Y-%m-%d").to_string()],
            },
            ..AppConfig::default()
        };
        let app = App::from_config(config);

        assert_eq!(
            app.recurring_schedule_texts_at(now).0,
            "🗓  Next schedule: tomorrow 09:00-10:00"
        );
        assert_eq!(
            app.recurring_schedule_texts_at(now).1,
            "⚙  Schedule status: skipped today (exception date)"
        );
    }

    #[test]
    fn recurring_schedule_does_not_trigger_on_exception_date() {
        let now = local_datetime_today(10, 15);
        let today = now.date_naive();
        let config = AppConfig {
            recurring_schedule: RecurringScheduleConfig {
                windows: vec![crate::config::RecurringFocusWindowConfig {
                    days: vec![weekday_token(today.weekday()).to_string()],
                    start: "10:00".to_string(),
                    end: "11:00".to_string(),
                }],
                exception_dates: vec![today.format("%Y-%m-%d").to_string()],
            },
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.task_labels = vec!["Coding".to_string()];
        app.selected_task_label = Some("Coding".to_string());

        app.sync_recurring_schedule(now);

        assert_eq!(app.timer.phase, TimerPhase::Focus);
        assert_eq!(app.timer.status, TimerStatus::Idle);
        assert!(app.schedule_armed_occurrence_key.is_none());
        assert!(app.phase_notification.is_none());
    }

    #[test]
    fn recurring_schedule_auto_starts_focus_when_window_begins_and_task_is_selected() {
        let now = local_datetime_today(10, 15);
        let config = AppConfig {
            recurring_schedule: RecurringScheduleConfig {
                windows: vec![crate::config::RecurringFocusWindowConfig {
                    days: vec![weekday_token(now.weekday()).to_string()],
                    start: "10:00".to_string(),
                    end: "11:00".to_string(),
                }],
                ..RecurringScheduleConfig::default()
            },
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.task_labels = vec!["Coding".to_string()];
        app.selected_task_label = Some("Coding".to_string());

        app.sync_recurring_schedule(now);

        assert_eq!(app.timer.phase, TimerPhase::Focus);
        assert_eq!(app.timer.status, TimerStatus::Running);
        assert_eq!(
            app.phase_notification.as_deref(),
            Some("Scheduled window started. Focus auto-started.")
        );
        assert!(app.schedule_armed_occurrence_key.is_none());
    }

    #[test]
    fn recurring_schedule_arms_when_window_begins_without_task_label() {
        let now = local_datetime_today(10, 15);
        let config = AppConfig {
            recurring_schedule: RecurringScheduleConfig {
                windows: vec![crate::config::RecurringFocusWindowConfig {
                    days: vec![weekday_token(now.weekday()).to_string()],
                    start: "10:00".to_string(),
                    end: "11:00".to_string(),
                }],
                ..RecurringScheduleConfig::default()
            },
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);

        app.sync_recurring_schedule(now);

        assert_eq!(app.timer.phase, TimerPhase::Focus);
        assert_eq!(app.timer.status, TimerStatus::Idle);
        assert!(app.schedule_armed_occurrence_key.is_some());
        assert_eq!(
            app.phase_notification.as_deref(),
            Some(
                "Scheduled window started. Select a task label with [t], then press [Space] to start focus."
            )
        );
    }

    #[test]
    fn recurring_schedule_auto_start_switches_idle_break_to_focus() {
        let now = local_datetime_today(10, 15);
        let config = AppConfig {
            recurring_schedule: RecurringScheduleConfig {
                windows: vec![crate::config::RecurringFocusWindowConfig {
                    days: vec![weekday_token(now.weekday()).to_string()],
                    start: "10:00".to_string(),
                    end: "11:00".to_string(),
                }],
                ..RecurringScheduleConfig::default()
            },
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.task_labels = vec!["Coding".to_string()];
        app.selected_task_label = Some("Coding".to_string());
        app.timer.phase = TimerPhase::ShortBreak;
        app.timer.status = TimerStatus::Idle;

        app.sync_recurring_schedule(now);

        assert_eq!(app.timer.phase, TimerPhase::Focus);
        assert_eq!(app.timer.status, TimerStatus::Running);
        assert_eq!(
            app.phase_notification.as_deref(),
            Some("Scheduled window started. Focus auto-started.")
        );
        assert!(app.schedule_armed_occurrence_key.is_none());
    }

    #[test]
    fn recurring_schedule_does_not_retrigger_within_same_window_occurrence() {
        let first_tick = local_datetime_today(10, 15);
        let second_tick = local_datetime_today(10, 16);
        let config = AppConfig {
            recurring_schedule: RecurringScheduleConfig {
                windows: vec![crate::config::RecurringFocusWindowConfig {
                    days: vec![weekday_token(first_tick.weekday()).to_string()],
                    start: "10:00".to_string(),
                    end: "11:00".to_string(),
                }],
                ..RecurringScheduleConfig::default()
            },
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.task_labels = vec!["Coding".to_string()];
        app.selected_task_label = Some("Coding".to_string());

        app.sync_recurring_schedule(first_tick);
        assert_eq!(app.timer.status, TimerStatus::Running);

        app.timer.status = TimerStatus::Idle;
        app.sync_recurring_schedule(second_tick);

        assert_eq!(app.timer.status, TimerStatus::Idle);
    }

    #[test]
    fn strict_mode_blocks_skip_during_active_focus() {
        let config = AppConfig {
            strict_mode: true,
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Running;
        app.timer.remaining_secs = app.timer.focus_secs;

        app.handle_key(key(KeyCode::Char('n')));

        assert_eq!(app.timer.phase, TimerPhase::Focus);
        assert_eq!(app.timer.status, TimerStatus::Running);
        assert_eq!(app.timer.remaining_secs, app.timer.focus_secs);
    }

    #[test]
    fn strict_mode_requires_second_stop_press_during_active_focus() {
        let config = AppConfig {
            strict_mode: true,
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Running;
        app.timer.remaining_secs = app.timer.focus_secs.saturating_sub(10);

        app.handle_key(key(KeyCode::Char('s')));

        assert_eq!(app.timer.status, TimerStatus::Running);
        assert_eq!(
            app.timer.remaining_secs,
            app.timer.focus_secs.saturating_sub(10)
        );
        assert!(app.strict_reset_confirmation_pending());

        app.handle_key(key(KeyCode::Char('s')));

        assert_eq!(app.timer.status, TimerStatus::Idle);
        assert_eq!(app.timer.remaining_secs, app.timer.focus_secs);
        assert!(!app.strict_reset_confirmation_pending());
    }

    #[test]
    fn pending_strict_reset_confirmation_clears_when_opening_site_manager() {
        let config = AppConfig {
            strict_mode: true,
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Running;

        app.handle_key(key(KeyCode::Char('s')));
        assert!(app.strict_reset_confirmation_pending());

        app.handle_key(key(KeyCode::Char('b')));

        assert_eq!(app.mode, AppMode::SiteManager);
        assert!(!app.strict_reset_confirmation_pending());
    }

    #[test]
    fn pending_strict_reset_confirmation_clears_when_opening_history() {
        let config = AppConfig {
            strict_mode: true,
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Running;

        app.handle_key(key(KeyCode::Char('s')));
        assert!(app.strict_reset_confirmation_pending());

        app.handle_key(key(KeyCode::Char('h')));

        assert_eq!(app.mode, AppMode::StatsHistory);
        assert!(!app.strict_reset_confirmation_pending());
    }

    #[test]
    fn stop_resets_immediately_when_strict_mode_is_disabled() {
        let mut app = App::default();
        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Running;
        app.timer.remaining_secs = app.timer.focus_secs.saturating_sub(5);

        app.handle_key(key(KeyCode::Char('s')));

        assert_eq!(app.timer.status, TimerStatus::Idle);
        assert_eq!(app.timer.remaining_secs, app.timer.focus_secs);
    }

    #[test]
    fn strict_mode_blocks_profile_manager_access_during_active_focus() {
        let config = AppConfig {
            strict_mode: true,
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Running;

        app.handle_key(key(KeyCode::Char('p')));

        assert_eq!(app.mode, AppMode::Timer);
    }

    #[test]
    fn strict_mode_blocks_profile_apply_during_active_focus() {
        let config = AppConfig {
            strict_mode: true,
            selected_profile: ProfileId::Custom,
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Running;
        app.timer.remaining_secs = app.timer.focus_secs.saturating_sub(20);
        app.mode = AppMode::ProfileManager;
        app.profile_selection_index = profile_index(ProfileId::Classic);

        app.handle_key(key(KeyCode::Enter));

        assert_eq!(app.selected_profile, ProfileId::Custom);
        assert_eq!(app.mode, AppMode::ProfileManager);
        assert_eq!(app.timer.phase, TimerPhase::Focus);
        assert_eq!(app.timer.status, TimerStatus::Running);
        assert_eq!(
            app.timer.remaining_secs,
            app.timer.focus_secs.saturating_sub(20)
        );
        assert!(
            app.config_error
                .as_deref()
                .is_some_and(|err| err.contains("strict focus active"))
        );
    }

    #[test]
    fn strict_mode_cannot_be_disabled_during_active_focus_profile_edit() {
        let config = AppConfig {
            strict_mode: true,
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Running;
        app.mode = AppMode::ProfileManager;
        app.profile_edit_active = true;
        app.profile_edit_field = 8;

        app.handle_key(key(KeyCode::Left));

        assert!(app.strict_mode);
    }

    #[test]
    fn strict_mode_blocks_custom_profile_commit_during_active_focus() {
        let config = AppConfig {
            strict_mode: true,
            selected_profile: ProfileId::Custom,
            custom_profile: Some(CustomProfileConfig::default()),
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Running;
        app.mode = AppMode::ProfileManager;
        app.profile_edit_active = true;
        app.profile_edit_field = 0;
        app.profile_edit_snapshot = Some(ProfileEditSnapshot {
            custom_profile: app.custom_profile.clone(),
            notification_settings: app.notification_settings,
            auto_start: app.auto_start,
            recurring_schedule: app.recurring_schedule.clone(),
            strict_mode: app.strict_mode,
            daily_goal: app.daily_goal,
            wakatime_metadata: app.wakatime_metadata.clone(),
        });
        app.custom_profile.focus_secs = app.custom_profile.focus_secs.saturating_add(60);
        app.notification_settings.enabled = false;

        app.handle_key(key(KeyCode::Enter));

        assert!(app.profile_edit_active);
        assert!(app.profile_edit_snapshot.is_some());
        assert!(
            app.config_error
                .as_deref()
                .is_some_and(|err| err.contains("strict focus active"))
        );

        app.timer.remaining_secs = 1;
        app.on_tick(false);
        assert!(app.phase_notification.is_some());
    }

    #[test]
    fn enabling_strict_mode_saves_during_active_focus_for_custom_profile_without_reset() {
        let config = AppConfig {
            strict_mode: false,
            selected_profile: ProfileId::Custom,
            custom_profile: Some(CustomProfileConfig::default()),
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Running;
        app.timer.remaining_secs = app.timer.focus_secs.saturating_sub(30);
        app.mode = AppMode::ProfileManager;
        app.profile_edit_active = true;
        app.profile_edit_field = 8;
        app.profile_edit_snapshot = Some(ProfileEditSnapshot {
            custom_profile: app.custom_profile.clone(),
            notification_settings: app.notification_settings,
            auto_start: app.auto_start,
            recurring_schedule: app.recurring_schedule.clone(),
            strict_mode: app.strict_mode,
            daily_goal: app.daily_goal,
            wakatime_metadata: app.wakatime_metadata.clone(),
        });

        app.handle_key(key(KeyCode::Right));
        app.handle_key(key(KeyCode::Enter));

        assert!(app.strict_mode);
        assert!(!app.profile_edit_active);
        assert!(app.profile_edit_snapshot.is_none());
        assert!(app.config_error.is_none());
        assert_eq!(app.timer.phase, TimerPhase::Focus);
        assert_eq!(app.timer.status, TimerStatus::Running);
        assert_eq!(
            app.timer.remaining_secs,
            app.timer.focus_secs.saturating_sub(30)
        );
        assert!(app.persisted_config().strict_mode);
    }

    #[test]
    fn strict_mode_blocks_quit_keys_during_active_focus() {
        let config = AppConfig {
            strict_mode: true,
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Running;

        app.handle_key(key(KeyCode::Char('q')));
        assert!(!app.should_quit);

        app.handle_key(key(KeyCode::Esc));
        assert!(!app.should_quit);

        app.handle_key(ctrl_key(KeyCode::Char('c')));
        assert!(!app.should_quit);
        assert!(
            app.phase_notification
                .as_deref()
                .is_some_and(|msg| msg.contains("Strict mode active"))
        );
    }

    #[test]
    fn strict_mode_allows_quit_when_focus_not_active() {
        let config = AppConfig {
            strict_mode: true,
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Idle;

        app.handle_key(key(KeyCode::Char('q')));

        assert!(app.should_quit);
    }

    #[test]
    fn strict_mode_stays_enforced_during_break_glass_override() {
        let config = AppConfig {
            strict_mode: true,
            blocked_sites: vec!["example.com".to_string()],
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Running;
        app.break_glass_expires_at = Some(Instant::now() + Duration::from_secs(120));

        assert!(app.strict_mode_enforced_for_focus());
    }

    #[test]
    fn break_glass_requires_confirmation_before_unblocking() {
        let config = AppConfig {
            blocked_sites: vec!["example.com".to_string()],
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Running;
        app.apply_blocking_for_phase();
        assert!(app.should_block_for_current_state());

        app.handle_key(key(KeyCode::Char('u')));
        assert!(app.break_glass_confirmation_pending());
        assert!(app.should_block_for_current_state());

        app.handle_key(key(KeyCode::Char('u')));
        assert!(!app.break_glass_confirmation_pending());
        if app.break_glass_override_active() {
            assert!(!app.should_block_for_current_state());
        } else {
            assert!(app.should_block_for_current_state());
            assert!(
                app.phase_notification
                    .as_deref()
                    .is_some_and(|message| message.contains("failed"))
            );
        }
    }

    #[test]
    fn break_glass_expiry_reapplies_blocking_and_logs_notification() {
        let mut app = App::default();
        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Running;
        app.break_glass_expires_at = Some(Instant::now() - Duration::from_secs(1));
        app.poll_wakatime_status();

        assert!(!app.break_glass_override_active());
        assert!(app.should_block_for_current_state());
        assert!(
            app.phase_notification
                .as_deref()
                .is_some_and(|message| message.contains("expired"))
        );
    }

    #[test]
    fn break_glass_is_rejected_when_focus_is_not_active() {
        let config = AppConfig {
            blocked_sites: vec!["example.com".to_string()],
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Idle;

        app.handle_key(key(KeyCode::Char('u')));

        assert!(!app.break_glass_confirmation_pending());
        assert!(
            app.phase_notification
                .as_deref()
                .is_some_and(|message| message.contains("only during active focus"))
        );
    }

    #[test]
    fn break_glass_records_audit_event_in_stats() {
        let config = AppConfig {
            blocked_sites: vec!["example.com".to_string()],
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.task_labels = vec!["Project A".to_string()];
        app.selected_task_label = Some("Project A".to_string());
        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Running;
        app.apply_blocking_for_phase();

        app.handle_key(key(KeyCode::Char('u')));
        app.handle_key(key(KeyCode::Char('u')));

        let overrides = app.recent_break_glass_overrides(1);
        if app.break_glass_override_active() {
            assert_eq!(overrides.len(), 1);
            assert_eq!(overrides[0].date.len(), 10);
            assert_eq!(overrides[0].task_label.as_deref(), Some("Project A"));
            assert_eq!(overrides[0].duration_seconds, 5 * 60);
        } else {
            assert!(overrides.is_empty());
        }
    }

    #[test]
    fn break_glass_without_sites_does_not_record_audit_event() {
        let mut app = App::default();
        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Running;

        app.handle_key(key(KeyCode::Char('u')));

        assert!(!app.break_glass_confirmation_pending());
        assert!(app.recent_break_glass_overrides(1).is_empty());
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
    fn completed_focus_session_tracks_active_profile_for_history_totals() {
        let config = AppConfig {
            selected_profile: ProfileId::DeepWork,
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.task_labels = vec!["Project A".to_string()];
        app.selected_task_label = Some("Project A".to_string());

        app.handle_key(key(KeyCode::Char(' ')));
        app.timer.remaining_secs = 1;
        app.on_tick(false);

        let totals = app.profile_focus_totals();
        assert_eq!(totals.len(), 1);
        assert_eq!(totals[0].profile, crate::stats::ProfileBucket::DeepWork);
        assert_eq!(totals[0].pomodoros_completed, 1);
        assert_eq!(totals[0].focused_minutes(), 50);
    }

    #[test]
    fn completed_focus_session_updates_task_totals_and_trends() {
        let mut app = App::default();
        app.task_labels = vec!["Project A".to_string()];
        app.selected_task_label = Some("Project A".to_string());

        app.handle_key(key(KeyCode::Char(' ')));
        app.timer.remaining_secs = 1;
        app.on_tick(false);

        let task_totals = app.task_focus_totals(5);
        assert_eq!(task_totals.len(), 1);
        assert_eq!(task_totals[0].task_label, "Project A");
        assert_eq!(task_totals[0].pomodoros_completed, 1);

        let trends = app.recent_task_trends(5);
        assert_eq!(trends.len(), 1);
        assert_eq!(trends[0].task_label, "Project A");
        assert!(trends[0].delta_focused_minutes() > 0);
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
    fn diagnostics_view_toggles_from_timer_mode() {
        let mut app = App::default();

        app.handle_key(key(KeyCode::Char('d')));
        assert_eq!(app.mode, AppMode::SetupDiagnostics);

        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.mode, AppMode::Timer);
    }

    #[test]
    fn pending_strict_reset_confirmation_clears_when_opening_setup_diagnostics() {
        let config = AppConfig {
            strict_mode: true,
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Running;

        app.handle_key(key(KeyCode::Char('s')));
        assert!(app.strict_reset_confirmation_pending());

        app.handle_key(key(KeyCode::Char('d')));

        assert_eq!(app.mode, AppMode::SetupDiagnostics);
        assert!(!app.strict_reset_confirmation_pending());
    }

    #[test]
    fn catchup_tick_does_not_increment_focus_stats() {
        let config = AppConfig {
            auto_start: AutoStartConfig {
                focus_to_break: true,
                break_to_focus: true,
            },
            ..AppConfig::default()
        };
        let mut app = App::from_config(config);
        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Running;
        app.timer.remaining_secs = 1;

        app.on_tick(true);

        assert_eq!(app.timer.phase, TimerPhase::ShortBreak);
        assert_eq!(app.timer.status, TimerStatus::Idle);
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
    fn q_quits_during_wakatime_metadata_edit() {
        let mut app = App::default();
        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Char('e')));
        for _ in 0..PROFILE_EDIT_WAKATIME_PROJECT_INDEX {
            app.handle_key(key(KeyCode::Down));
        }

        app.handle_key(key(KeyCode::Char('q')));

        assert!(app.should_quit);
        assert_eq!(app.wakatime_metadata.project, "focustime");
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

    #[test]
    fn focus_does_not_start_without_selected_task_label() {
        let mut app = App::default();

        app.handle_key(key(KeyCode::Char(' ')));

        assert_eq!(app.timer.status, TimerStatus::Idle);
        assert_eq!(
            app.phase_notification.as_deref(),
            Some("Select a task label with [t] before starting focus.")
        );
    }

    #[test]
    fn cli_start_fails_without_selected_task_label() {
        let mut app = App::default();

        let result = app.start_focus_for_cli();

        assert!(result.is_err());
        assert_eq!(app.timer.status, TimerStatus::Idle);
    }

    #[test]
    fn cli_start_begins_focus_when_task_label_exists() {
        let mut app = App::default();
        app.selected_task_label = Some("Docs".to_string());

        let result = app.start_focus_for_cli();

        assert!(result.is_ok());
        assert_eq!(app.timer.status, TimerStatus::Running);
        assert_eq!(app.active_focus_task_label, Some("Docs".to_string()));
    }

    #[test]
    fn cli_pause_and_resume_transitions_timer_state() {
        let mut app = App::default();
        app.selected_task_label = Some("Docs".to_string());
        app.start_focus_for_cli().unwrap();

        app.pause_for_cli().unwrap();
        assert_eq!(app.timer.status, TimerStatus::Paused);

        app.resume_for_cli().unwrap();
        assert_eq!(app.timer.status, TimerStatus::Running);
    }

    #[test]
    fn cli_pause_requires_running_timer() {
        let mut app = App::default();

        let error = app.pause_for_cli().unwrap_err();

        assert_eq!(error, "Cannot pause: timer is not running.");
    }

    #[test]
    fn cli_stop_respects_strict_mode_during_focus() {
        let mut app = App::default();
        app.strict_mode = true;
        app.selected_task_label = Some("Docs".to_string());
        app.start_focus_for_cli().unwrap();

        let error = app.stop_for_cli().unwrap_err();

        assert_eq!(error, "Cannot stop: strict mode is active during focus.");
        assert_eq!(app.timer.status, TimerStatus::Running);
    }

    #[test]
    fn cli_next_respects_strict_mode_during_focus() {
        let mut app = App::default();
        app.strict_mode = true;
        app.selected_task_label = Some("Docs".to_string());
        app.start_focus_for_cli().unwrap();

        let error = app.next_phase_for_cli().unwrap_err();

        assert_eq!(
            error,
            "Cannot skip to next phase: strict mode is active during focus."
        );
        assert_eq!(app.timer.phase, TimerPhase::Focus);
        assert_eq!(app.timer.status, TimerStatus::Running);
    }

    #[test]
    fn cli_task_selection_auto_creates_and_reuses_labels() {
        let mut app = App::default();

        let created = app.select_task_label_for_cli("  Docs  ").unwrap();
        assert!(created);
        assert_eq!(app.task_labels, vec!["Docs".to_string()]);
        assert_eq!(app.selected_task_label.as_deref(), Some("Docs"));

        let created = app.select_task_label_for_cli("docs").unwrap();
        assert!(!created);
        assert_eq!(app.task_labels, vec!["Docs".to_string()]);
        assert_eq!(app.selected_task_label.as_deref(), Some("Docs"));
    }

    #[test]
    fn session_planner_adds_label_and_allows_focus_start() {
        let mut app = App::default();

        app.handle_key(key(KeyCode::Char('t')));
        assert_eq!(app.mode, AppMode::SessionPlanner);

        app.handle_key(key(KeyCode::Char('a')));
        for c in "Docs".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));

        assert_eq!(app.selected_task_label.as_deref(), Some("Docs"));
        assert_eq!(app.current_task_label(), Some("Docs"));

        app.handle_key(key(KeyCode::Char('t')));
        assert_eq!(app.mode, AppMode::Timer);

        app.handle_key(key(KeyCode::Char(' ')));
        assert_eq!(app.timer.status, TimerStatus::Running);
    }

    #[test]
    fn session_planner_renames_highlighted_label_and_updates_selection() {
        let mut app = App::default();
        app.task_labels = vec!["Docs".to_string(), "Review".to_string()];
        app.selected_task_label = Some("Docs".to_string());

        app.handle_key(key(KeyCode::Char('t')));
        app.handle_key(key(KeyCode::Char('e')));
        assert!(app.planner_input_active);
        assert_eq!(app.planner_input_mode, Some(PlannerInputMode::Rename));

        app.planner_input = "Writing".to_string();
        app.handle_key(key(KeyCode::Enter));

        assert_eq!(
            app.task_labels,
            vec!["Writing".to_string(), "Review".to_string()]
        );
        assert_eq!(app.selected_task_label.as_deref(), Some("Writing"));
        assert!(!app.planner_input_active);
        assert_eq!(app.planner_input_mode, None);
        assert!(
            app.planner_feedback
                .as_ref()
                .is_some_and(|feedback| feedback.message.contains("Renamed"))
        );
    }

    #[test]
    fn session_planner_delete_selected_label_selects_nearest_remaining() {
        let mut app = App::default();
        app.task_labels = vec![
            "Docs".to_string(),
            "Review".to_string(),
            "Planning".to_string(),
        ];
        app.selected_task_label = Some("Review".to_string());

        app.handle_key(key(KeyCode::Char('t')));
        app.planner_selection_index = 1;
        app.handle_key(key(KeyCode::Char('d')));

        assert_eq!(
            app.task_labels,
            vec!["Docs".to_string(), "Planning".to_string()]
        );
        assert_eq!(app.selected_task_label.as_deref(), Some("Planning"));
        assert_eq!(app.planner_selection_index, 1);
    }

    #[test]
    fn session_planner_delete_last_label_clears_selection() {
        let mut app = App::default();
        app.task_labels = vec!["Docs".to_string()];
        app.selected_task_label = Some("Docs".to_string());

        app.handle_key(key(KeyCode::Char('t')));
        app.handle_key(key(KeyCode::Delete));

        assert!(app.task_labels.is_empty());
        assert!(app.selected_task_label.is_none());
        assert_eq!(app.planner_selection_index, 0);
    }

    #[test]
    fn session_planner_recent_quick_pick_selects_recent_labels() {
        let mut app = App::default();
        app.task_labels = vec![
            "Docs".to_string(),
            "Review".to_string(),
            "Bugfix".to_string(),
        ];
        app.selected_task_label = Some("Docs".to_string());
        let goal = DailyGoalSnapshot {
            minutes: 25,
            pomodoros: 1,
        };
        app.stats.record_completed_pomodoro_with_task(
            "2026-04-07",
            goal,
            Some("Docs"),
            25 * 60,
            None,
        );
        app.stats.record_completed_pomodoro_with_task(
            "2026-04-08",
            goal,
            Some("Review"),
            25 * 60,
            None,
        );
        app.stats.record_completed_pomodoro_with_task(
            "2026-04-09",
            goal,
            Some("Bugfix"),
            25 * 60,
            None,
        );

        app.handle_key(key(KeyCode::Char('t')));
        app.handle_key(key(KeyCode::Char('r')));
        assert_eq!(app.selected_task_label.as_deref(), Some("Bugfix"));

        app.handle_key(key(KeyCode::Char('2')));
        assert_eq!(app.selected_task_label.as_deref(), Some("Review"));
    }

    #[test]
    fn planner_recent_labels_finds_older_valid_labels_after_stale_entries() {
        let mut app = App::default();
        app.task_labels = vec!["Keep A".to_string(), "Keep B".to_string()];
        let goal = DailyGoalSnapshot {
            minutes: 25,
            pomodoros: 1,
        };

        app.stats.record_completed_pomodoro_with_task(
            "2026-04-01",
            goal,
            Some("Keep A"),
            25 * 60,
            None,
        );
        app.stats.record_completed_pomodoro_with_task(
            "2026-04-02",
            goal,
            Some("Keep B"),
            25 * 60,
            None,
        );
        for i in 0..20 {
            let label = format!("Stale {i}");
            app.stats.record_completed_pomodoro_with_task(
                "2026-04-10",
                goal,
                Some(&label),
                25 * 60,
                None,
            );
        }

        let recent = app.planner_recent_labels(2);
        assert_eq!(recent, vec!["Keep B".to_string(), "Keep A".to_string()]);
    }

    #[test]
    fn startup_restores_valid_in_progress_snapshot() {
        session_recovery::set_test_load_snapshot(Some(snapshot_for_tests(
            TimerPhase::Focus,
            TimerStatus::Running,
            42,
            Some("Docs"),
            ProfileId::DeepWork,
        )));

        let app = App::from_config(AppConfig::default());

        assert_eq!(app.timer.phase, TimerPhase::Focus);
        assert_eq!(app.timer.status, TimerStatus::Running);
        assert_eq!(app.timer.remaining_secs, 42);
        assert_eq!(app.selected_profile, ProfileId::DeepWork);
        assert_eq!(app.selected_task_label.as_deref(), Some("Docs"));
        assert!(
            app.phase_notification
                .as_deref()
                .is_some_and(|message| message.contains("Recovered in-progress Focus session"))
        );
    }

    #[test]
    fn startup_restores_pomodoro_count_for_phase_cadence() {
        session_recovery::set_test_load_snapshot(Some(InProgressSessionSnapshot {
            phase: RecoveryTimerPhase::Focus,
            status: RecoveryTimerStatus::Running,
            remaining_secs: 1,
            pomodoros_completed: 3,
            selected_task_label: Some("Docs".to_string()),
            selected_profile: ProfileId::Classic,
        }));

        let mut app = App::from_config(AppConfig::default());
        assert_eq!(app.timer.pomodoros_completed, 3);

        app.on_tick(false);

        assert_eq!(app.timer.phase, TimerPhase::LongBreak);
    }

    #[test]
    fn startup_ignores_invalid_snapshot_and_starts_fresh() {
        session_recovery::set_test_load_snapshot(Some(snapshot_for_tests(
            TimerPhase::Focus,
            TimerStatus::Idle,
            60,
            Some("Docs"),
            ProfileId::Classic,
        )));

        let app = App::from_config(AppConfig::default());

        assert_eq!(app.timer.phase, TimerPhase::Focus);
        assert_eq!(app.timer.status, TimerStatus::Idle);
        assert_eq!(app.timer.remaining_secs, DEFAULT_FOCUS_SECS);
        assert!(
            app.phase_notification
                .as_deref()
                .is_some_and(|message| message.contains("Ignored saved in-progress session"))
        );
        assert!(session_recovery::test_saved_snapshot().is_none());
    }

    #[test]
    fn startup_reports_recovery_load_error() {
        session_recovery::set_test_load_error("simulated read failure");

        let app = App::from_config(AppConfig::default());

        assert!(
            app.phase_notification
                .as_deref()
                .is_some_and(|message| message.contains("Ignored saved in-progress session"))
        );
    }

    #[test]
    fn reset_clears_saved_recovery_snapshot() {
        let mut app = App::default();
        app.selected_task_label = Some("Docs".to_string());

        app.handle_key(key(KeyCode::Char(' ')));
        assert_eq!(app.timer.status, TimerStatus::Running);
        assert!(session_recovery::test_saved_snapshot().is_some());

        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(app.timer.status, TimerStatus::Idle);
        assert!(session_recovery::test_saved_snapshot().is_none());
    }

    #[test]
    fn recovery_snapshot_prefers_active_focus_label_over_selected_label() {
        let mut app = App::default();
        app.task_labels = vec!["Task A".to_string(), "Task B".to_string()];
        app.selected_task_label = Some("Task A".to_string());

        app.handle_key(key(KeyCode::Char(' ')));
        assert_eq!(app.active_focus_task_label.as_deref(), Some("Task A"));

        app.selected_task_label = Some("Task B".to_string());
        app.sync_recovery_snapshot();

        let snapshot = session_recovery::test_saved_snapshot().expect("snapshot should be saved");
        assert_eq!(snapshot.selected_task_label.as_deref(), Some("Task A"));
    }

    #[test]
    fn planner_label_change_during_running_break_updates_recovery_snapshot() {
        let mut app = App::default();
        app.task_labels = vec!["Task A".to_string(), "Task B".to_string()];
        app.selected_task_label = Some("Task A".to_string());
        app.sync_task_planner_state();

        app.handle_key(key(KeyCode::Char(' '))); // focus running
        app.handle_key(key(KeyCode::Char('n'))); // short break idle
        app.handle_key(key(KeyCode::Char(' '))); // short break running
        assert_eq!(app.timer.phase, TimerPhase::ShortBreak);
        assert_eq!(app.timer.status, TimerStatus::Running);

        app.open_session_planner();
        app.planner_selection_index = 1;
        app.select_planner_label();

        let snapshot = session_recovery::test_saved_snapshot().expect("snapshot should be saved");
        assert_eq!(snapshot.selected_task_label.as_deref(), Some("Task B"));
        assert_eq!(snapshot.phase, RecoveryTimerPhase::ShortBreak);
        assert_eq!(snapshot.status, RecoveryTimerStatus::Running);
    }

    #[test]
    fn prevent_double_input_on_windows() {
        let mut press_event = KeyEvent::from(KeyCode::Char('a'));
        press_event.kind = KeyEventKind::Press;

        let mut release_event = KeyEvent::from(KeyCode::Char('a'));
        release_event.kind = KeyEventKind::Release;

        assert!(
            should_handle_key(&press_event),
            "Press event should be handled"
        );
        assert!(
            !should_handle_key(&release_event),
            "Release event should NOT be handled"
        );
    }
}
