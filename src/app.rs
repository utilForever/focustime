use std::{
    collections::{BTreeSet, HashSet},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use chrono::{DateTime, Datelike, Local, NaiveDate};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::blocker::{
    BlockingIntent, BlockingPreview, BlockingPreviewAction, BulkAddResult, EditSiteResult,
    HostsFileDiagnostics, InvalidSiteInput, SiteBlocker,
};
use crate::config::{
    AppConfig, AutoStartConfig, BlocklistProfileConfig, BreakTemplateConfig, CustomProfileConfig,
    DailyGoalConfig, GoalCarryOverConfig, MonthlyGoalConfig, NotificationConfig,
    OneTimeFocusWindowConfig, ProfileAutomationConfig, ProfileAutomationSettingsConfig, ProfileId,
    RecurringFocusWindowConfig, RecurringScheduleConfig, WakatimeMetadataConfig, WeeklyGoalConfig,
};
use crate::notifications::PhaseNotifier;
use crate::schedule::{
    OneTimeWindow, RecurringWindow, WindowOccurrence, active_occurrence,
    active_one_time_occurrence, compile_exception_dates, compile_one_time_windows, compile_windows,
    format_schedule_conflict, inspect_schedule_conflicts_from_config, next_occurrence_after,
    next_one_time_occurrence_after, occurrence_key, pick_active_occurrence, pick_next_occurrence,
};
use crate::stats::{
    BreakGlassOverrideEvent, DailyGoalSnapshot, DailyStats, ExportedStatsFiles,
    FocusSessionMetadata, FocusStats, GoalStreak, MonthlyHeatmap, MonthlyStats,
    ProfileEffectiveness, ProfileTotals, SessionInterruptionEvent, SessionInterruptionReason,
    SessionStats, TaskGoalProgress, TaskTotals, TaskTrend, WeeklyConsistency, WeeklyFocusScore,
    WeeklyStats, carry_over_goal_target, current_day_key,
};
use crate::task_labels::{normalize_task_label, task_label_index};
use crate::timer::{
    DEFAULT_FOCUS_SECS, DEFAULT_LONG_BREAK_INTERVAL, DEFAULT_LONG_BREAK_SECS,
    DEFAULT_SHORT_BREAK_SECS, TimerPhase, TimerState, TimerStatus,
};
use crate::wakatime::{WakatimeConfigStatus, WakatimeHeartbeatMetadata, WakatimeTracker};

mod break_glass;
mod cli_api;
mod feedback_diagnostics;
mod persistence;
mod planner_labels;
mod schedule_editor;
mod schedule_runtime;
mod timer_flow;

pub const PROFILE_IDS: [ProfileId; 3] =
    [ProfileId::Classic, ProfileId::DeepWork, ProfileId::Custom];
pub const PROFILE_EDIT_FIELD_LABELS: [&str; 35] = [
    "Focus",
    "Short Break",
    "Long Break",
    "Long-break cadence",
    "Phase notifications",
    "Sound alert",
    "Auto-start break",
    "Auto-start focus",
    "Strict focus mode",
    "Daily goal minutes",
    "Daily goal pomodoros",
    "Daily goal carry-over",
    "Weekly goal minutes",
    "Weekly goal pomodoros",
    "Weekly goal carry-over",
    "Monthly goal minutes",
    "Monthly goal pomodoros",
    "Monthly goal carry-over",
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
    "One-time window",
    "One-time date",
    "One-time start",
    "One-time end",
    "One-time add/remove",
    "Schedule conflicts",
];
const PROFILE_EDIT_DAILY_GOAL_MINUTES_INDEX: usize = 9;
const PROFILE_EDIT_DAILY_GOAL_POMODOROS_INDEX: usize = 10;
const PROFILE_EDIT_DAILY_GOAL_CARRY_OVER_INDEX: usize = 11;
const PROFILE_EDIT_WEEKLY_GOAL_MINUTES_INDEX: usize = 12;
const PROFILE_EDIT_WEEKLY_GOAL_POMODOROS_INDEX: usize = 13;
const PROFILE_EDIT_WEEKLY_GOAL_CARRY_OVER_INDEX: usize = 14;
const PROFILE_EDIT_MONTHLY_GOAL_MINUTES_INDEX: usize = 15;
const PROFILE_EDIT_MONTHLY_GOAL_POMODOROS_INDEX: usize = 16;
const PROFILE_EDIT_MONTHLY_GOAL_CARRY_OVER_INDEX: usize = 17;
const PROFILE_EDIT_WAKATIME_PROJECT_INDEX: usize = 18;
const PROFILE_EDIT_WAKATIME_LANGUAGE_INDEX: usize = 19;
const PROFILE_EDIT_SCHEDULE_WINDOW_INDEX: usize = 20;
const PROFILE_EDIT_SCHEDULE_DAY_INDEX: usize = 21;
const PROFILE_EDIT_SCHEDULE_DAY_ENABLED_INDEX: usize = 22;
const PROFILE_EDIT_SCHEDULE_START_INDEX: usize = 23;
const PROFILE_EDIT_SCHEDULE_END_INDEX: usize = 24;
const PROFILE_EDIT_SCHEDULE_ADD_REMOVE_INDEX: usize = 25;
const PROFILE_EDIT_SCHEDULE_EXCEPTION_INDEX: usize = 26;
const PROFILE_EDIT_SCHEDULE_EXCEPTION_DATE_INDEX: usize = 27;
const PROFILE_EDIT_SCHEDULE_EXCEPTION_ADD_REMOVE_INDEX: usize = 28;
const PROFILE_EDIT_ONE_TIME_WINDOW_INDEX: usize = 29;
const PROFILE_EDIT_ONE_TIME_DATE_INDEX: usize = 30;
const PROFILE_EDIT_ONE_TIME_START_INDEX: usize = 31;
const PROFILE_EDIT_ONE_TIME_END_INDEX: usize = 32;
const PROFILE_EDIT_ONE_TIME_ADD_REMOVE_INDEX: usize = 33;
const PROFILE_EDIT_SCHEDULE_CONFLICTS_INDEX: usize = 34;
const CUSTOM_DURATION_STEP_SECS: u64 = 60;
const DAILY_GOAL_MINUTES_STEP: u64 = 5;
const DEFAULT_BLOCKLIST_PROFILE_NAME: &str = "Default";
const UNLINKED_BREAK_TEMPLATE_NAME: &str = "Custom";
pub(crate) const PLANNER_RECENT_LABEL_LIMIT: usize = 5;
const SCHEDULE_TIME_STEP_MINUTES: u16 = 15;
const SCHEDULE_DAY_TOKENS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
const SCHEDULE_DAY_LABELS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const SCHEDULE_DELAY_SECS: u64 = 10 * 60;

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

fn break_template_index(templates: &[BreakTemplateConfig], selected_name: &str) -> Option<usize> {
    templates
        .iter()
        .position(|template| template.name.eq_ignore_ascii_case(selected_name))
}

fn break_template_matches_custom_profile(
    template: &BreakTemplateConfig,
    custom_profile: &CustomProfileConfig,
) -> bool {
    let template = template.normalized();
    let custom_profile = custom_profile.normalized();
    template.short_break_secs == custom_profile.short_break_secs
        && template.long_break_secs == custom_profile.long_break_secs
        && template.long_break_interval == custom_profile.long_break_interval
}

fn break_template_index_for_custom_profile(
    templates: &[BreakTemplateConfig],
    custom_profile: &CustomProfileConfig,
) -> Option<usize> {
    templates
        .iter()
        .position(|template| break_template_matches_custom_profile(template, custom_profile))
}

fn resolve_active_break_template(
    templates: &[BreakTemplateConfig],
    selected_name: &str,
    custom_profile: &CustomProfileConfig,
) -> Option<usize> {
    let selected = break_template_index(templates, selected_name);
    if let Some(selected_index) = selected {
        if templates
            .get(selected_index)
            .is_some_and(|template| break_template_matches_custom_profile(template, custom_profile))
        {
            return Some(selected_index);
        }
    }

    break_template_index_for_custom_profile(templates, custom_profile)
}

#[derive(Debug, Clone)]
struct ProfileEditSnapshot {
    custom_profile: CustomProfileConfig,
    notification_settings: NotificationConfig,
    auto_start: AutoStartConfig,
    recurring_schedule: RecurringScheduleConfig,
    strict_mode: bool,
    daily_goal: DailyGoalConfig,
    weekly_goal: WeeklyGoalConfig,
    monthly_goal: MonthlyGoalConfig,
    goal_carry_over: GoalCarryOverConfig,
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
    delayed_until: Option<DateTime<Local>>,
    has_selected_task: bool,
    timer_phase: TimerPhase,
    timer_status: TimerStatus,
}

#[derive(Debug, Clone)]
struct FocusInterruptionContext {
    day_key: String,
    timestamp_epoch_secs: u64,
    reason: SessionInterruptionReason,
    task_label: Option<String>,
    focus_intention: Option<String>,
    task_note: Option<String>,
    remaining_secs: u64,
    profile: Option<ProfileId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteInputMode {
    Add,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteListMode {
    Blocklist,
    Allowlist,
}

impl SiteListMode {
    fn toggle(self) -> Self {
        match self {
            Self::Blocklist => Self::Allowlist,
            Self::Allowlist => Self::Blocklist,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Blocklist => "Blocklist",
            Self::Allowlist => "Allowlist",
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockingPreviewSnapshot {
    pub action: BlockingPreviewAction,
    pub would_change: bool,
    pub effective_blocked_sites_count: usize,
    pub section: Option<String>,
    pub error: Option<String>,
}

impl Default for BlockingPreviewSnapshot {
    fn default() -> Self {
        Self {
            action: BlockingPreviewAction::NoChange,
            would_change: false,
            effective_blocked_sites_count: 0,
            section: None,
            error: None,
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
    pub break_templates: Vec<BreakTemplateConfig>,
    active_break_template: Option<usize>,
    pub blocklist_profile_input: String,
    pub blocklist_profile_input_active: bool,
    blocklist_profile_input_mode: Option<BlocklistProfileInputMode>,
    pub site_feedback: Option<SiteFeedback>,
    pub task_labels: Vec<String>,
    pub selected_task_label: Option<String>,
    task_label_favorites: BTreeSet<String>,
    task_label_archived: BTreeSet<String>,
    pub planner_selection_index: usize,
    pub planner_input: String,
    pub planner_input_active: bool,
    pub planner_input_mode: Option<PlannerInputMode>,
    pub planner_feedback: Option<PlannerFeedback>,
    active_focus_task_label: Option<String>,
    active_focus_intention: Option<String>,
    active_focus_task_note: Option<String>,
    timer_note_input: String,
    timer_note_input_active: bool,
    active_focus_profile: Option<ProfileId>,
    site_list_mode: SiteListMode,
    /// Index of the highlighted site in the SiteManager list.
    pub selected_site: usize,
    /// Last error from a block/unblock operation (e.g. permission denied).
    pub block_error: Option<String>,
    pub setup_diagnostics: SetupDiagnostics,
    pub blocking_preview: BlockingPreviewSnapshot,
    /// Last error from persisting timer/site configuration.
    pub config_error: Option<String>,
    /// Last error from persisting focus stats.
    pub stats_error: Option<String>,
    pub history_feedback: Option<HistoryFeedback>,
    pub phase_notification: Option<String>,
    pub wakatime: WakatimeTracker,
    pub selected_profile: ProfileId,
    profile_automation: ProfileAutomationSettingsConfig,
    pub custom_profile: CustomProfileConfig,
    pub profile_selection_index: usize,
    pub profile_edit_active: bool,
    pub profile_edit_field: usize,
    profile_edit_schedule_window: usize,
    profile_edit_schedule_day: usize,
    profile_edit_schedule_exception: usize,
    profile_edit_one_time_window: usize,
    profile_edit_snapshot: Option<ProfileEditSnapshot>,
    notification_settings: NotificationConfig,
    auto_start: AutoStartConfig,
    recurring_schedule: RecurringScheduleConfig,
    recurring_windows: Vec<RecurringWindow>,
    recurring_exception_dates: HashSet<NaiveDate>,
    one_time_windows: Vec<OneTimeWindow>,
    schedule_armed_occurrence_key: Option<String>,
    schedule_delayed_occurrence_key: Option<String>,
    schedule_delay_until: Option<DateTime<Local>>,
    last_schedule_occurrence_key: Option<String>,
    current_frame_now: DateTime<Local>,
    pub strict_mode: bool,
    break_glass_duration_secs: u64,
    break_glass_expires_at: Option<Instant>,
    daily_goal: DailyGoalConfig,
    weekly_goal: WeeklyGoalConfig,
    monthly_goal: MonthlyGoalConfig,
    goal_carry_over: GoalCarryOverConfig,
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
        let profile_automation = config.profile_automation.clone().unwrap_or_default();
        let selected_automation = config.profile_automation_for(selected_profile);
        let notification_settings = selected_automation.notifications;
        let auto_start = selected_automation.auto_start;
        let recurring_schedule = selected_automation.recurring_schedule.clone();
        let recurring_windows = compile_windows(&recurring_schedule.windows);
        let recurring_exception_dates =
            compile_exception_dates(&recurring_schedule.exception_dates);
        let one_time_windows = compile_one_time_windows(&recurring_schedule.one_time_windows);
        let strict_mode = selected_automation.strict_mode;
        let break_glass_duration_secs = config.break_glass_duration_secs;
        let daily_goal = config.daily_goal;
        let weekly_goal = config.weekly_goal;
        let monthly_goal = config.monthly_goal;
        let goal_carry_over = config.goal_carry_over;
        let wakatime_metadata = config.wakatime;
        let blocklist_profiles = config.blocklist_profiles.clone();
        let active_blocklist_profile =
            blocklist_profile_index(&blocklist_profiles, &config.selected_blocklist_profile);
        let break_templates = config.break_templates.clone();
        let active_break_template = resolve_active_break_template(
            &break_templates,
            &config.selected_break_template,
            &custom_profile,
        );
        let profile_spec = profile_spec_for(selected_profile, &custom_profile);
        let (stats, stats_error) = match FocusStats::load() {
            Ok(stats) => (stats, None),
            Err(e) => (FocusStats::default(), Some(e)),
        };
        let (task_labels, selected_task_label) = stats.task_planner_state();
        let task_label_favorites = task_label_state_keys(stats.task_label_favorites());
        let task_label_archived = task_label_state_keys(stats.task_label_archived());
        let planner_selection_index = 0;
        let timer = TimerState::with_profile(
            profile_spec.focus_secs,
            profile_spec.short_break_secs,
            profile_spec.long_break_secs,
            profile_spec.long_break_interval,
        );
        let blocker = SiteBlocker::new();
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
            break_templates,
            active_break_template,
            blocklist_profile_input: String::new(),
            blocklist_profile_input_active: false,
            blocklist_profile_input_mode: None,
            site_feedback: None,
            task_labels,
            selected_task_label,
            task_label_favorites,
            task_label_archived,
            planner_selection_index,
            planner_input: String::new(),
            planner_input_active: false,
            planner_input_mode: None,
            planner_feedback: None,
            active_focus_task_label: None,
            active_focus_intention: None,
            active_focus_task_note: None,
            timer_note_input: String::new(),
            timer_note_input_active: false,
            active_focus_profile: None,
            site_list_mode: SiteListMode::Blocklist,
            selected_site: 0,
            block_error: None,
            setup_diagnostics,
            blocking_preview: BlockingPreviewSnapshot::default(),
            config_error: None,
            stats_error,
            history_feedback: None,
            phase_notification: None,
            wakatime: WakatimeTracker::new_with_metadata(WakatimeHeartbeatMetadata {
                project: wakatime_metadata.project.clone(),
                language: wakatime_metadata.language.clone(),
            }),
            selected_profile,
            profile_automation,
            custom_profile,
            profile_selection_index: profile_index(selected_profile),
            profile_edit_active: false,
            profile_edit_field: 0,
            profile_edit_schedule_window: 0,
            profile_edit_schedule_day: 0,
            profile_edit_schedule_exception: 0,
            profile_edit_one_time_window: 0,
            profile_edit_snapshot: None,
            notification_settings,
            auto_start,
            recurring_schedule,
            recurring_windows,
            recurring_exception_dates,
            one_time_windows,
            schedule_armed_occurrence_key: None,
            schedule_delayed_occurrence_key: None,
            schedule_delay_until: None,
            last_schedule_occurrence_key: None,
            current_frame_now: Local::now(),
            strict_mode,
            break_glass_duration_secs,
            break_glass_expires_at: None,
            daily_goal,
            weekly_goal,
            monthly_goal,
            goal_carry_over,
            wakatime_metadata,
            pending_timer_action: None,
            notifier: PhaseNotifier::new(notification_settings),
            stats,
            stats_dirty: false,
            stats_has_unsaved_elapsed: false,
        };
        app.clamp_break_template_selection();
        app.recompute_blocker_sites_from_active_profile();
        app.restore_in_progress_session();
        app.sync_planner_selection_to_selected_label();
        app.sync_recovery_snapshot();
        app.apply_blocking_for_phase();
        app.refresh_setup_diagnostics();
        app.sync_today_goal_snapshot();
        app
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
        self.sync_today_goal_snapshot();
        self.wakatime.poll_events();
        self.sync_break_glass_override();
        self.sync_recurring_schedule(now);
    }

    pub fn selected_profile_name(&self) -> &'static str {
        self.selected_profile.label()
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

    pub fn current_task_note(&self) -> Option<&str> {
        if !self.focus_session_active_for_current_state() {
            return None;
        }
        self.active_focus_task_note
            .as_deref()
            .or(self.active_focus_task_label.as_deref())
            .or(self.selected_task_label.as_deref())
    }

    pub fn can_edit_session_note(&self) -> bool {
        self.focus_session_active_for_current_state()
    }

    pub fn timer_note_input_active(&self) -> bool {
        self.timer_note_input_active
    }

    pub fn timer_note_input_value(&self) -> &str {
        &self.timer_note_input
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
        let today = Local::now().date_naive();
        let today_key = today.format("%Y-%m-%d").to_string();
        let today_stats = self.stats.daily_for(&today_key);
        let target = self.effective_daily_goal_snapshot_for_day(today);
        goal_progress_for_totals(
            today_stats.focused_minutes(),
            today_stats.pomodoros_completed,
            target.minutes,
            target.pomodoros,
        )
    }

    pub fn current_week_goal_progress(&self) -> DailyGoalProgress {
        let today = Local::now().date_naive();
        let week = self.stats.weekly_for_day(today);
        let target = self.effective_weekly_goal_snapshot_for_day(today);
        goal_progress_for_totals(
            week.focused_minutes(),
            week.pomodoros_completed,
            target.minutes,
            target.pomodoros,
        )
    }

    pub fn current_month_goal_progress(&self) -> DailyGoalProgress {
        let today = Local::now().date_naive();
        let month = self.stats.monthly_for_day(today);
        let target = self.effective_monthly_goal_snapshot_for_day(today);
        goal_progress_for_totals(
            month.focused_minutes(),
            month.pomodoros_completed,
            target.minutes,
            target.pomodoros,
        )
    }

    pub fn goal_streak(&self) -> GoalStreak {
        self.goal_streak_for_day_key(&current_day_key())
    }

    fn goal_streak_for_day_key(&self, day_key: &str) -> GoalStreak {
        let Some(day) = parse_day_key(day_key) else {
            return GoalStreak::default();
        };

        let current_goal = self.effective_daily_goal_snapshot_for_day(day);
        self.stats
            .goal_streak(day, current_goal, self.stats.daily_for(day_key))
    }

    #[allow(dead_code)]
    pub fn daily_goal_progress_for(&self, stats: DailyStats) -> DailyGoalProgress {
        goal_progress_for_totals(
            stats.focused_minutes(),
            stats.pomodoros_completed,
            self.daily_goal.minutes,
            self.daily_goal.pomodoros,
        )
    }

    pub fn recent_daily_stats(&self, limit: usize) -> Vec<(String, DailyStats)> {
        self.stats.recent_daily(limit)
    }

    #[allow(dead_code)]
    pub fn recent_weekly_stats(&self, limit: usize) -> Vec<WeeklyStats> {
        self.stats.recent_weekly(limit)
    }

    #[allow(dead_code)]
    pub fn recent_weekly_consistency(&self, limit: usize) -> Vec<WeeklyConsistency> {
        self.stats.recent_weekly_consistency(limit)
    }

    pub fn latest_weekly_focus_score(&self) -> Option<WeeklyFocusScore> {
        self.stats.latest_weekly_focus_score()
    }

    pub fn recent_monthly_stats(&self, limit: usize) -> Vec<MonthlyStats> {
        self.stats.recent_monthly(limit)
    }

    pub fn latest_monthly_heatmap(&self) -> MonthlyHeatmap {
        self.stats.latest_monthly_heatmap()
    }

    #[allow(dead_code)]
    pub fn profile_focus_totals(&self) -> Vec<ProfileTotals> {
        self.stats.profile_totals()
    }

    pub fn profile_effectiveness(&self) -> Vec<ProfileEffectiveness> {
        self.stats.profile_effectiveness()
    }

    pub fn task_focus_totals(&self, limit: usize) -> Vec<TaskTotals> {
        self.stats.task_totals(limit)
    }

    pub fn task_goal_progress_for_label(&self, label: &str) -> Option<TaskGoalProgress> {
        self.stats.task_goal_progress_for_label(label)
    }

    pub fn recent_task_trends(&self, limit: usize) -> Vec<TaskTrend> {
        self.stats.recent_task_trends(limit)
    }

    #[cfg(test)]
    pub fn insert_daily_stats_for_tests(&mut self, day_key: &str, stats: DailyStats) {
        self.stats.insert_daily_for_tests(day_key, stats);
    }

    pub fn profile_edit_field_value(&self, field_index: usize) -> String {
        if (PROFILE_EDIT_SCHEDULE_WINDOW_INDEX..=PROFILE_EDIT_SCHEDULE_CONFLICTS_INDEX)
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
            PROFILE_EDIT_DAILY_GOAL_MINUTES_INDEX => {
                format_daily_goal_minutes_label(self.daily_goal.minutes)
            }
            PROFILE_EDIT_DAILY_GOAL_POMODOROS_INDEX => {
                format_daily_goal_pomodoros_label(self.daily_goal.pomodoros)
            }
            PROFILE_EDIT_DAILY_GOAL_CARRY_OVER_INDEX => {
                bool_label(self.goal_carry_over.daily).to_string()
            }
            PROFILE_EDIT_WEEKLY_GOAL_MINUTES_INDEX => {
                format_daily_goal_minutes_label(self.weekly_goal.minutes)
            }
            PROFILE_EDIT_WEEKLY_GOAL_POMODOROS_INDEX => {
                format_daily_goal_pomodoros_label(self.weekly_goal.pomodoros)
            }
            PROFILE_EDIT_WEEKLY_GOAL_CARRY_OVER_INDEX => {
                bool_label(self.goal_carry_over.weekly).to_string()
            }
            PROFILE_EDIT_MONTHLY_GOAL_MINUTES_INDEX => {
                format_daily_goal_minutes_label(self.monthly_goal.minutes)
            }
            PROFILE_EDIT_MONTHLY_GOAL_POMODOROS_INDEX => {
                format_daily_goal_pomodoros_label(self.monthly_goal.pomodoros)
            }
            PROFILE_EDIT_MONTHLY_GOAL_CARRY_OVER_INDEX => {
                bool_label(self.goal_carry_over.monthly).to_string()
            }
            PROFILE_EDIT_WAKATIME_PROJECT_INDEX => self.wakatime_metadata.project.clone(),
            PROFILE_EDIT_WAKATIME_LANGUAGE_INDEX => self.wakatime_metadata.language.clone(),
            _ => String::new(),
        }
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

    pub fn site_input_mode(&self) -> SiteInputMode {
        if self.site_edit_index.is_some() {
            SiteInputMode::Edit
        } else {
            SiteInputMode::Add
        }
    }

    pub fn site_list_mode(&self) -> SiteListMode {
        self.site_list_mode
    }

    pub fn active_policy_sites(&self) -> &[String] {
        self.active_profile_sites_for_mode(self.site_list_mode)
    }

    pub fn active_policy_site_count(&self) -> usize {
        self.active_policy_sites().len()
    }

    pub fn effective_blocked_site_count(&self) -> usize {
        self.blocker.sites.len()
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

    pub fn active_break_template_name(&self) -> &str {
        self.active_break_template
            .and_then(|index| self.break_templates.get(index))
            .map(|template| template.name.as_str())
            .unwrap_or(UNLINKED_BREAK_TEMPLATE_NAME)
    }

    pub fn active_break_template_summary(&self) -> String {
        if let Some(template) = self
            .active_break_template
            .and_then(|index| self.break_templates.get(index))
        {
            format!(
                "{}/{}, every {} focus",
                format_duration_label(template.short_break_secs),
                format_duration_label(template.long_break_secs),
                template.long_break_interval
            )
        } else {
            let custom_profile = self.custom_profile.normalized();
            format!(
                "{}/{}, every {} focus",
                format_duration_label(custom_profile.short_break_secs),
                format_duration_label(custom_profile.long_break_secs),
                custom_profile.long_break_interval
            )
        }
    }

    fn selected_break_template_for_persistence(&self) -> String {
        self.active_break_template
            .and_then(|index| self.break_templates.get(index))
            .map(|template| template.name.clone())
            .unwrap_or_default()
    }

    fn sync_active_break_template_to_custom_profile(&mut self) {
        let selected_name = self
            .active_break_template
            .and_then(|index| self.break_templates.get(index))
            .map(|template| template.name.clone())
            .unwrap_or_default();
        self.active_break_template = resolve_active_break_template(
            &self.break_templates,
            &selected_name,
            &self.custom_profile,
        );
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
        if self.mode == AppMode::Timer && self.timer_note_input_active {
            let sanitized = normalize_timer_note_paste(&text);
            if !sanitized.is_empty() {
                if !self.timer_note_input.is_empty() {
                    self.timer_note_input.push(' ');
                }
                self.timer_note_input.push_str(&sanitized);
            }
            return;
        }

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
        if self.timer_note_input_active {
            self.handle_timer_note_input_key(key);
            return;
        }

        if self.handle_quit_key(&key, true) {
            return;
        }

        if self.strict_reset_confirmation_pending() {
            if key.code == KeyCode::Char('s') {
                self.pending_timer_action = None;
                self.update_timer_and_sync_with_reason(
                    TimerState::reset,
                    Some(SessionInterruptionReason::ManualStop),
                );
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
                    && !self.has_selectable_task_label_for_focus()
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
                self.update_timer_and_sync_with_reason(
                    TimerState::reset,
                    Some(SessionInterruptionReason::ManualStop),
                );
            }
            // Skip to next phase
            KeyCode::Char('n') => {
                if self.strict_mode_enforced_for_focus() {
                    return;
                }
                self.update_timer_and_sync_with_reason(
                    TimerState::next_phase,
                    Some(SessionInterruptionReason::ManualSkip),
                );
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
            // Edit mid-session note
            KeyCode::Char('m') => {
                self.start_timer_note_input();
            }
            // Break-glass override (temporary unblock)
            KeyCode::Char('u') => {
                self.handle_break_glass_key();
            }
            // Delay scheduled start for the current active window
            KeyCode::Char('z') => {
                self.delay_active_schedule_start();
            }
            _ => {}
        }
    }

    fn start_timer_note_input(&mut self) {
        if !self.focus_session_active_for_current_state() {
            self.phase_notification = Some(
                "Mid-session notes are available only during active or paused focus.".to_string(),
            );
            return;
        }

        self.timer_note_input = self
            .current_task_note()
            .map(str::to_string)
            .unwrap_or_default();
        self.timer_note_input_active = true;
        self.phase_notification =
            Some("Editing session note: type text, then press [Enter] to save.".to_string());
    }

    fn handle_timer_note_input_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => self.commit_timer_note_input(),
            KeyCode::Esc => {
                self.clear_timer_note_input();
                self.phase_notification = Some("Session note edit canceled.".to_string());
            }
            KeyCode::Backspace => {
                self.timer_note_input.pop();
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.timer_note_input.push(c);
            }
            _ => {}
        }
    }

    fn commit_timer_note_input(&mut self) {
        if !self.focus_session_active_for_current_state() {
            self.clear_timer_note_input();
            self.phase_notification =
                Some("Session note save failed: focus is no longer active.".to_string());
            return;
        }

        let note = if self.timer_note_input.trim().is_empty() {
            self.active_focus_task_label
                .clone()
                .or_else(|| self.selected_task_label.clone())
        } else {
            Some(self.timer_note_input.trim().to_string())
        };

        self.clear_timer_note_input();
        if let Some(note) = note {
            self.active_focus_task_note = Some(note);
            self.sync_recovery_snapshot();
            self.phase_notification = Some("Session note updated.".to_string());
        } else {
            self.phase_notification =
                Some("Session note save failed: no task selected.".to_string());
        }
    }

    fn clear_timer_note_input(&mut self) {
        self.timer_note_input.clear();
        self.timer_note_input_active = false;
    }

    fn delay_active_schedule_start(&mut self) {
        if self.focus_session_active_for_current_state() {
            self.phase_notification = Some(
                "Schedule delay is unavailable while a focus session is already active."
                    .to_string(),
            );
            return;
        }

        let now = self.current_frame_now;
        let Some(active_window) = self.active_schedule_occurrence_at(now) else {
            self.phase_notification = Some("No active schedule window to delay.".to_string());
            return;
        };
        let active_occurrence_key = occurrence_key(&active_window);
        let delayed_from = match (
            self.schedule_delayed_occurrence_key.as_deref(),
            self.schedule_delay_until,
        ) {
            (Some(existing_key), Some(existing_until))
                if existing_key == active_occurrence_key && existing_until > now =>
            {
                existing_until
            }
            _ => now,
        };
        let delayed_until = delayed_from + chrono::Duration::seconds(SCHEDULE_DELAY_SECS as i64);

        self.schedule_armed_occurrence_key = None;
        self.schedule_delayed_occurrence_key = Some(active_occurrence_key);
        self.schedule_delay_until = Some(delayed_until);
        self.last_schedule_occurrence_key = None;
        self.phase_notification = Some(format!(
            "Scheduled start delayed for {} (until {}).",
            format_duration_label(SCHEDULE_DELAY_SECS),
            delayed_until.format("%H:%M")
        ));
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
            KeyCode::Char('[') => {
                self.select_previous_break_template();
            }
            KeyCode::Char(']') => {
                self.select_next_break_template();
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
            KeyCode::Down | KeyCode::Char('j') if !self.planner_labels_for_display().is_empty() => {
                self.planner_selection_index = (self.planner_selection_index + 1)
                    .min(self.planner_labels_for_display().len().saturating_sub(1));
            }
            KeyCode::Char('a') => self.start_planner_input(),
            KeyCode::Char('e') => self.start_planner_rename_input(),
            KeyCode::Char('f') => self.toggle_planner_favorite(),
            KeyCode::Char('x') => self.toggle_planner_archive(),
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
        let Some(label) = self.planner_selected_label() else {
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
            let Some(existing_label) = self.task_labels.get(existing_index).cloned() else {
                self.set_planner_feedback(
                    PlannerFeedbackLevel::Warning,
                    "No task labels available",
                );
                return;
            };
            if let Some(display_index) = self.planner_display_index_for_label(&existing_label) {
                self.planner_selection_index = display_index;
            }
            if self.is_task_label_archived(&existing_label) {
                self.cancel_planner_input();
                self.set_planner_feedback(
                    PlannerFeedbackLevel::Warning,
                    format!(
                        "`{existing_label}` is archived; unarchive it with [x] before selecting"
                    ),
                );
                return;
            }
            self.selected_task_label = Some(existing_label);
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
        self.selected_task_label = Some(label.clone());
        self.sync_planner_selection_to_selected_label();
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
        let Some(current_label) = self.planner_selected_label() else {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        };
        let Some(current_index) = task_label_index(&self.task_labels, &current_label) else {
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
            && existing_index != current_index
        {
            self.set_planner_feedback(
                PlannerFeedbackLevel::Warning,
                format!("`{label}` already exists"),
            );
            return;
        }

        let source_goal_target = self
            .stats
            .task_goal_progress_for_label(&current_label)
            .map(|progress| progress.target)
            .unwrap_or_default();
        let destination_goal_target = self
            .stats
            .task_goal_progress_for_label(&label)
            .map(|progress| progress.target)
            .unwrap_or_default();
        if !current_label.eq_ignore_ascii_case(&label)
            && source_goal_target.has_any_target()
            && destination_goal_target.has_any_target()
        {
            self.set_planner_feedback(
                PlannerFeedbackLevel::Warning,
                format!(
                    "Cannot rename `{current_label}` -> `{label}`: destination task goal already exists"
                ),
            );
            return;
        }

        if let Some(target) = self.task_labels.get_mut(current_index) {
            *target = label.clone();
        }
        let current_key = task_label_key(&current_label);
        let next_key = task_label_key(&label);
        if self.task_label_favorites.remove(&current_key) {
            self.task_label_favorites.insert(next_key.clone());
        }
        if self.task_label_archived.remove(&current_key) {
            self.task_label_archived.insert(next_key.clone());
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
            self.active_focus_intention = Some(label.clone());
            let should_sync_note_to_label = match self.active_focus_task_note.as_deref() {
                None => true,
                Some(note) => note.eq_ignore_ascii_case(&current_label),
            };
            if should_sync_note_to_label {
                self.active_focus_task_note = Some(label.clone());
            }
        }
        self.stats.rename_task_goal_target(&current_label, &label);
        if let Some(index) = self.planner_display_index_for_label(&label) {
            self.planner_selection_index = index;
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
        let Some(removed_label) = self.planner_selected_label() else {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        };
        let Some(removed_index) = task_label_index(&self.task_labels, &removed_label) else {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        };
        let previous_selection = self.planner_selection_index;
        let removed = self.task_labels.remove(removed_index);
        self.task_label_favorites.remove(&task_label_key(&removed));
        self.task_label_archived.remove(&task_label_key(&removed));
        self.stats.remove_task_goal_target(&removed);

        let removed_was_selected = self
            .selected_task_label
            .as_ref()
            .is_some_and(|selected| selected.eq_ignore_ascii_case(&removed));
        let selected_label_missing = self
            .selected_task_label
            .as_ref()
            .is_some_and(|selected| task_label_index(&self.task_labels, selected).is_none());
        if removed_was_selected || selected_label_missing {
            self.selected_task_label = self.nearest_selectable_task_label(previous_selection);
        }
        let display_labels = self.planner_labels_for_display();
        if display_labels.is_empty() {
            self.planner_selection_index = 0;
        } else if removed_was_selected || selected_label_missing {
            if let Some(selected_label) = self.selected_task_label.as_ref()
                && let Some(index) = self.planner_display_index_for_label(selected_label)
            {
                self.planner_selection_index = index;
            } else {
                self.planner_selection_index = previous_selection.min(display_labels.len() - 1);
            }
        } else {
            self.planner_selection_index = previous_selection.min(display_labels.len() - 1);
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
        let Some(existing_label) = self.task_labels.get(existing_index).cloned() else {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        };
        if let Some(display_index) = self.planner_display_index_for_label(&existing_label) {
            self.planner_selection_index = display_index;
        }
        self.selected_task_label = Some(existing_label);
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
        let Some(label) = self.planner_selected_label() else {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        };
        if self.is_task_label_archived(&label) {
            self.set_planner_feedback(
                PlannerFeedbackLevel::Warning,
                format!("Cannot select archived label `{label}`"),
            );
            return;
        }
        self.selected_task_label = Some(label.clone());
        self.sync_planner_selection_to_selected_label();
        self.sync_task_planner_state();
        self.sync_recovery_snapshot();
        self.set_planner_feedback(PlannerFeedbackLevel::Success, format!("Selected `{label}`"));
    }

    fn toggle_planner_favorite(&mut self) {
        if self.task_labels.is_empty() {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        }

        self.clamp_planner_selection();
        let Some(label) = self.planner_selected_label() else {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        };
        let key = task_label_key(&label);
        let message = if self.task_label_favorites.remove(&key) {
            format!("Removed favorite `{label}`")
        } else {
            self.task_label_favorites.insert(key);
            format!("Favorited `{label}`")
        };
        if let Some(index) = self.planner_display_index_for_label(&label) {
            self.planner_selection_index = index;
        }
        self.sync_task_planner_state();
        self.sync_recovery_snapshot();
        self.set_planner_feedback(PlannerFeedbackLevel::Success, message);
    }

    fn toggle_planner_archive(&mut self) {
        if self.task_labels.is_empty() {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        }

        self.clamp_planner_selection();
        let Some(label) = self.planner_selected_label() else {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        };
        let key = task_label_key(&label);
        let message = if self.task_label_archived.remove(&key) {
            format!("Unarchived `{label}`")
        } else {
            self.task_label_archived.insert(key);
            if self
                .selected_task_label
                .as_ref()
                .is_some_and(|selected| selected.eq_ignore_ascii_case(&label))
            {
                self.selected_task_label =
                    self.nearest_selectable_task_label(self.planner_selection_index);
            }
            format!("Archived `{label}`")
        };
        if let Some(index) = self.planner_display_index_for_label(&label) {
            self.planner_selection_index = index;
        } else {
            self.clamp_planner_selection();
        }
        self.sync_task_planner_state();
        self.sync_recovery_snapshot();
        self.set_planner_feedback(PlannerFeedbackLevel::Success, message);
    }

    fn begin_profile_edit(&mut self) {
        self.profile_edit_snapshot = Some(ProfileEditSnapshot {
            custom_profile: self.custom_profile.clone(),
            notification_settings: self.notification_settings,
            auto_start: self.auto_start,
            recurring_schedule: self.recurring_schedule.clone(),
            strict_mode: self.strict_mode,
            daily_goal: self.daily_goal,
            weekly_goal: self.weekly_goal,
            monthly_goal: self.monthly_goal,
            goal_carry_over: self.goal_carry_over,
            wakatime_metadata: self.wakatime_metadata.clone(),
        });
        self.profile_edit_active = true;
        self.profile_edit_field = 0;
        self.profile_edit_schedule_window = 0;
        self.profile_edit_schedule_day = 0;
        self.profile_edit_schedule_exception = 0;
        self.profile_edit_one_time_window = 0;
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
            self.weekly_goal = snapshot.weekly_goal;
            self.monthly_goal = snapshot.monthly_goal;
            self.goal_carry_over = snapshot.goal_carry_over;
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
        self.profile_edit_one_time_window = 0;
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
        let weekly_goal_changed = self
            .profile_edit_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.weekly_goal != self.weekly_goal);
        let monthly_goal_changed = self
            .profile_edit_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.monthly_goal != self.monthly_goal);
        let goal_carry_over_changed = self
            .profile_edit_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.goal_carry_over != self.goal_carry_over);
        self.custom_profile = self.custom_profile.normalized();
        self.sync_active_break_template_to_custom_profile();
        self.recurring_schedule = normalized_schedule;
        self.wakatime_metadata = self.wakatime_metadata.normalized();
        if self.selected_profile == ProfileId::Custom {
            if custom_profile_changed {
                let original_profile_automation = self.profile_automation.clone();
                self.update_selected_profile_automation();
                if !self.apply_profile(ProfileId::Custom) {
                    self.profile_automation = original_profile_automation;
                    return;
                }
            } else {
                self.update_selected_profile_automation();
                self.save_config();
            }
        } else {
            self.update_selected_profile_automation();
            self.save_config();
        }
        self.sync_wakatime_metadata_to_tracker();
        self.rebuild_notifier();
        self.rebuild_recurring_schedule_runtime();
        if schedule_changed {
            self.schedule_armed_occurrence_key = None;
            self.clear_schedule_delay_state();
            self.last_schedule_occurrence_key = None;
            let now = Local::now();
            self.current_frame_now = now;
            self.sync_recurring_schedule(now);
        }
        if daily_goal_changed
            || weekly_goal_changed
            || monthly_goal_changed
            || goal_carry_over_changed
        {
            self.sync_today_goal_snapshot();
        }
        self.profile_edit_active = false;
        self.profile_edit_field = 0;
        self.profile_edit_schedule_window = 0;
        self.profile_edit_schedule_day = 0;
        self.profile_edit_schedule_exception = 0;
        self.profile_edit_one_time_window = 0;
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
            PROFILE_EDIT_DAILY_GOAL_MINUTES_INDEX => {
                adjust_daily_goal_minutes(&mut self.daily_goal.minutes, increase);
            }
            PROFILE_EDIT_DAILY_GOAL_POMODOROS_INDEX => {
                adjust_daily_goal_pomodoros(&mut self.daily_goal.pomodoros, increase);
            }
            PROFILE_EDIT_DAILY_GOAL_CARRY_OVER_INDEX => {
                self.goal_carry_over.daily = increase;
            }
            PROFILE_EDIT_WEEKLY_GOAL_MINUTES_INDEX => {
                adjust_daily_goal_minutes(&mut self.weekly_goal.minutes, increase);
            }
            PROFILE_EDIT_WEEKLY_GOAL_POMODOROS_INDEX => {
                adjust_daily_goal_pomodoros(&mut self.weekly_goal.pomodoros, increase);
            }
            PROFILE_EDIT_WEEKLY_GOAL_CARRY_OVER_INDEX => {
                self.goal_carry_over.weekly = increase;
            }
            PROFILE_EDIT_MONTHLY_GOAL_MINUTES_INDEX => {
                adjust_daily_goal_minutes(&mut self.monthly_goal.minutes, increase);
            }
            PROFILE_EDIT_MONTHLY_GOAL_POMODOROS_INDEX => {
                adjust_daily_goal_pomodoros(&mut self.monthly_goal.pomodoros, increase);
            }
            PROFILE_EDIT_MONTHLY_GOAL_CARRY_OVER_INDEX => {
                self.goal_carry_over.monthly = increase;
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
            PROFILE_EDIT_ONE_TIME_WINDOW_INDEX => {
                self.cycle_one_time_window(increase);
            }
            PROFILE_EDIT_ONE_TIME_DATE_INDEX => {
                self.adjust_selected_one_time_date(increase);
            }
            PROFILE_EDIT_ONE_TIME_START_INDEX => {
                self.adjust_selected_one_time_time(true, increase);
            }
            PROFILE_EDIT_ONE_TIME_END_INDEX => {
                self.adjust_selected_one_time_time(false, increase);
            }
            PROFILE_EDIT_ONE_TIME_ADD_REMOVE_INDEX => {
                self.adjust_one_time_windows_collection(increase);
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
        self.one_time_windows = compile_one_time_windows(&self.recurring_schedule.one_time_windows);
    }

    fn selected_profile_automation(&self) -> ProfileAutomationConfig {
        ProfileAutomationConfig {
            notifications: self.notification_settings,
            auto_start: self.auto_start,
            strict_mode: self.strict_mode,
            recurring_schedule: self.recurring_schedule.clone(),
        }
        .normalized()
    }

    fn update_selected_profile_automation(&mut self) {
        self.profile_automation
            .set_for_profile(self.selected_profile, self.selected_profile_automation());
    }

    fn load_automation_runtime_for_profile(&mut self, profile: ProfileId) {
        let automation = self
            .profile_automation
            .for_profile(profile, &ProfileAutomationConfig::default());
        self.notification_settings = automation.notifications;
        self.auto_start = automation.auto_start;
        self.strict_mode = automation.strict_mode;
        self.recurring_schedule = automation.recurring_schedule;
        self.rebuild_notifier();
        self.rebuild_recurring_schedule_runtime();
        self.schedule_armed_occurrence_key = None;
        self.clear_schedule_delay_state();
        self.last_schedule_occurrence_key = None;
    }

    fn apply_automation_for_profile(&mut self, profile: ProfileId) {
        self.load_automation_runtime_for_profile(profile);
        let now = Local::now();
        self.current_frame_now = now;
        self.sync_recurring_schedule(now);
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
        self.active_focus_intention = None;
        self.active_focus_task_note = None;
        self.active_focus_profile = None;
        self.selected_profile = profile;
        self.profile_selection_index = profile_index(profile);
        self.apply_automation_for_profile(profile);
        self.pending_timer_action = None;
        self.save_config();
        self.apply_blocking_for_phase();
        self.sync_recovery_snapshot();
        true
    }

    fn toggle_site_list_mode(&mut self) {
        self.site_list_mode = self.site_list_mode.toggle();
        self.clamp_selection();
        self.set_site_feedback(
            SiteFeedbackLevel::Success,
            format!("Switched to {} entries", self.site_list_mode.label()),
        );
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
            KeyCode::Down | KeyCode::Char('j') if !self.active_policy_sites().is_empty() => {
                self.selected_site =
                    (self.selected_site + 1).min(self.active_policy_sites().len() - 1);
            }
            // Navigate up
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected_site = self.selected_site.saturating_sub(1);
            }
            // Toggle between blocklist and allowlist entries
            KeyCode::Char('m') => {
                self.toggle_site_list_mode();
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
                if self.active_policy_sites().is_empty() {
                    self.site_input_active = false;
                    self.set_site_feedback(SiteFeedbackLevel::Warning, "No site selected to edit");
                    return;
                }
                self.clamp_selection();
                self.site_edit_index = Some(self.selected_site);
                self.site_input = self
                    .active_policy_sites()
                    .get(self.selected_site)
                    .cloned()
                    .unwrap_or_default();
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
        let mode = self.site_list_mode;
        let mut working = SiteBlocker::new();
        for site in self.active_profile_sites_for_mode(mode).iter().cloned() {
            working.add_site(site);
        }

        let committed = if let Some(index) = self.site_edit_index {
            let edit_result = working.edit_site_from_input(index, &input);
            if let Some(target_sites) = self.active_profile_sites_for_mode_mut(mode) {
                *target_sites = working.sites.clone();
            }
            self.apply_edit_site_result(edit_result)
        } else {
            let add_result = working.add_sites_from_input(&input);
            if let Some(target_sites) = self.active_profile_sites_for_mode_mut(mode) {
                *target_sites = working.sites.clone();
            }
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
                self.blocklist_profiles.push(BlocklistProfileConfig {
                    name: name.clone(),
                    sites: Vec::new(),
                    allowlist_sites: Vec::new(),
                });
                self.active_blocklist_profile = self.blocklist_profiles.len().saturating_sub(1);
                self.recompute_blocker_sites_from_active_profile();
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
            self.selected_site = self.active_policy_site_count().saturating_sub(1);
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
                    format!(
                        "`{hostname}` is already in the {}",
                        self.site_list_mode.label().to_ascii_lowercase()
                    ),
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
        if self.active_policy_sites().is_empty() {
            self.set_site_feedback(SiteFeedbackLevel::Warning, "No site selected to delete");
            return;
        }

        let mode = self.site_list_mode;
        let selected_site = self.selected_site;
        let list_name = mode.label().to_ascii_lowercase();
        let mut working = SiteBlocker::new();
        for site in self.active_profile_sites_for_mode(mode).iter().cloned() {
            working.add_site(site);
        }
        let removed = working.remove_site(selected_site);
        if let Some(target_sites) = self.active_profile_sites_for_mode_mut(mode) {
            *target_sites = working.sites.clone();
        }

        if let Some(removed) = removed {
            self.finalize_site_mutation();
            self.set_site_feedback(
                SiteFeedbackLevel::Success,
                format!("Removed `{removed}` from {list_name}"),
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

        self.active_blocklist_profile = next_index;
        self.recompute_blocker_sites_from_active_profile();
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

        let removed = self
            .blocklist_profiles
            .remove(self.active_blocklist_profile);
        if self.active_blocklist_profile >= self.blocklist_profiles.len() {
            self.active_blocklist_profile = self.blocklist_profiles.len().saturating_sub(1);
        }
        self.recompute_blocker_sites_from_active_profile();
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

    fn clamp_break_template_selection(&mut self) {
        if self.break_templates.is_empty() {
            self.break_templates.push(BreakTemplateConfig::default());
            return;
        }
        if let Some(active_break_template) = self.active_break_template {
            self.active_break_template =
                Some(active_break_template.min(self.break_templates.len().saturating_sub(1)));
        }
    }

    fn select_previous_break_template(&mut self) {
        self.clamp_break_template_selection();
        if self.break_templates.is_empty() {
            return;
        }
        let last = self.break_templates.len().saturating_sub(1);
        let next = match self.active_break_template {
            None | Some(0) => last,
            Some(current) => current.min(last).saturating_sub(1),
        };
        self.switch_break_template(next);
    }

    fn select_next_break_template(&mut self) {
        self.clamp_break_template_selection();
        if self.break_templates.is_empty() {
            return;
        }
        let last = self.break_templates.len().saturating_sub(1);
        let next = match self.active_break_template {
            None => 0,
            Some(current) => (current.min(last) + 1) % self.break_templates.len(),
        };
        self.switch_break_template(next);
    }

    fn switch_break_template(&mut self, next_index: usize) {
        if next_index >= self.break_templates.len()
            || self.active_break_template == Some(next_index)
        {
            return;
        }

        let previous_index = self.active_break_template;
        let previous_custom_profile = self.custom_profile.clone();
        let Some(template) = self.break_templates.get(next_index).cloned() else {
            return;
        };
        self.active_break_template = Some(next_index);
        let template = template.normalized();
        self.custom_profile.short_break_secs = template.short_break_secs;
        self.custom_profile.long_break_secs = template.long_break_secs;
        self.custom_profile.long_break_interval = template.long_break_interval;
        self.custom_profile = self.custom_profile.normalized();
        let custom_profile_changed = self.custom_profile != previous_custom_profile;

        if self.selected_profile == ProfileId::Custom && custom_profile_changed {
            let original_profile_automation = self.profile_automation.clone();
            if !self.apply_profile(ProfileId::Custom) {
                self.profile_automation = original_profile_automation;
                self.active_break_template = previous_index;
                self.custom_profile = previous_custom_profile;
                return;
            }
        } else {
            self.save_config();
        }

        self.phase_notification = Some(format!(
            "Break template selected: {} ({})",
            self.active_break_template_name(),
            self.active_break_template_summary()
        ));
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
        if self.active_policy_sites().is_empty() {
            self.selected_site = 0;
        } else {
            self.selected_site = self.selected_site.min(self.active_policy_sites().len() - 1);
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

    fn active_profile_sites_for_mode(&self, mode: SiteListMode) -> &[String] {
        self.blocklist_profiles
            .get(self.active_blocklist_profile)
            .map(|profile| match mode {
                SiteListMode::Blocklist => &profile.sites,
                SiteListMode::Allowlist => &profile.allowlist_sites,
            })
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    fn active_profile_sites_for_mode_mut(
        &mut self,
        mode: SiteListMode,
    ) -> Option<&mut Vec<String>> {
        self.clamp_blocklist_profile_selection();
        self.blocklist_profiles
            .get_mut(self.active_blocklist_profile)
            .map(|profile| match mode {
                SiteListMode::Blocklist => &mut profile.sites,
                SiteListMode::Allowlist => &mut profile.allowlist_sites,
            })
    }

    fn recompute_blocker_sites_from_active_profile(&mut self) {
        self.clamp_blocklist_profile_selection();
        self.blocker.sites.clear();
        if let Some(active_profile) = self.blocklist_profiles.get(self.active_blocklist_profile) {
            for site in effective_blocked_sites_for_profile(active_profile) {
                self.blocker.add_site(site);
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
        let display_labels = self.planner_labels_for_display();
        if display_labels.is_empty() {
            self.planner_selection_index = 0;
        } else {
            self.planner_selection_index = self
                .planner_selection_index
                .min(display_labels.len().saturating_sub(1));
        }
    }

    fn sync_task_planner_state(&mut self) {
        if self.stats.update_task_planner_state_with_label_states(
            self.task_labels.clone(),
            self.selected_task_label.clone(),
            task_label_state_labels(&self.task_label_favorites, &self.task_labels),
            task_label_state_labels(&self.task_label_archived, &self.task_labels),
        ) {
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

    fn build_focus_interruption_context(
        &self,
        reason: SessionInterruptionReason,
    ) -> FocusInterruptionContext {
        let now = Local::now();
        let day_key = now.date_naive().format("%Y-%m-%d").to_string();
        let timestamp_epoch_secs = now.timestamp().max(0) as u64;
        let task_label = self
            .active_focus_task_label
            .clone()
            .or_else(|| self.selected_task_label.clone());
        FocusInterruptionContext {
            day_key,
            timestamp_epoch_secs,
            reason,
            task_label: task_label.clone(),
            focus_intention: self
                .active_focus_intention
                .clone()
                .or_else(|| task_label.clone()),
            task_note: self.active_focus_task_note.clone().or(task_label),
            remaining_secs: self.timer.remaining_secs,
            profile: self.active_focus_profile.or(Some(self.selected_profile)),
        }
    }

    fn record_session_interruption_event(&mut self, context: FocusInterruptionContext) {
        self.stats.record_session_interruption_event(
            &context.day_key,
            context.timestamp_epoch_secs,
            context.reason,
            FocusSessionMetadata {
                task_label: context.task_label.as_deref(),
                focus_intention: context.focus_intention.as_deref(),
                task_note: context.task_note.as_deref(),
            },
            context.remaining_secs,
            context.profile,
        );
        self.stats_dirty = true;
    }

    fn open_site_manager(&mut self) {
        self.pending_timer_action = None;
        self.mode = AppMode::SiteManager;
        self.site_list_mode = SiteListMode::Blocklist;
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
        self.profile_edit_one_time_window = 0;
        self.profile_edit_snapshot = None;
        self.profile_selection_index = profile_index(self.selected_profile);
        self.clamp_profile_selection();
        self.clamp_profile_edit_schedule_selection();
        self.clamp_break_template_selection();
    }

    fn open_session_planner(&mut self) {
        self.mode = AppMode::SessionPlanner;
        self.planner_feedback = None;
        self.planner_input.clear();
        self.planner_input_active = false;
        self.planner_input_mode = None;
        self.sync_planner_selection_to_selected_label();
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
        self.recompute_blocker_sites_from_active_profile();
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
        self.focus_session_active_for_current_state() && !self.break_glass_override_active_now()
    }

    fn focus_running_for_current_state(&self) -> bool {
        self.timer.phase == TimerPhase::Focus && self.timer.status == TimerStatus::Running
    }

    fn focus_session_active_for_current_state(&self) -> bool {
        self.timer.phase == TimerPhase::Focus && self.timer.status != TimerStatus::Idle
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

    fn record_completed_focus_session(&mut self, focused_seconds: u64) {
        let day_key = current_day_key();
        let goal = self.current_goal_snapshot();
        if let Some(active_task_label) = self.active_focus_task_label.clone() {
            let focus_intention = self
                .active_focus_intention
                .clone()
                .unwrap_or_else(|| active_task_label.clone());
            let task_note = self
                .active_focus_task_note
                .clone()
                .unwrap_or_else(|| active_task_label.clone());
            self.stats.record_completed_pomodoro_with_metadata(
                &day_key,
                goal,
                FocusSessionMetadata {
                    task_label: Some(active_task_label.as_str()),
                    focus_intention: Some(focus_intention.as_str()),
                    task_note: Some(task_note.as_str()),
                },
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

    fn current_week_goal_snapshot(&self) -> DailyGoalSnapshot {
        DailyGoalSnapshot {
            minutes: self.weekly_goal.minutes,
            pomodoros: self.weekly_goal.pomodoros,
        }
    }

    fn current_month_goal_snapshot(&self) -> DailyGoalSnapshot {
        DailyGoalSnapshot {
            minutes: self.monthly_goal.minutes,
            pomodoros: self.monthly_goal.pomodoros,
        }
    }

    fn effective_daily_goal_snapshot_for_day(&self, day: NaiveDate) -> DailyGoalSnapshot {
        let day_key = day.format("%Y-%m-%d").to_string();
        let base = self
            .stats
            .daily_entry(&day_key)
            .and_then(|stats| stats.goal)
            .unwrap_or_else(|| self.current_goal_snapshot());
        let previous = day.pred_opt().and_then(|previous_day| {
            let previous_day_key = previous_day.format("%Y-%m-%d").to_string();
            self.stats.daily_entry(&previous_day_key).and_then(|stats| {
                stats
                    .goal
                    .map(|goal| (goal, stats.focused_minutes(), stats.pomodoros_completed))
            })
        });
        carry_over_goal_target(base, self.goal_carry_over.daily, previous)
    }

    fn effective_weekly_goal_snapshot_for_day(&self, day: NaiveDate) -> DailyGoalSnapshot {
        let base = DailyGoalSnapshot {
            minutes: self.weekly_goal.minutes,
            pomodoros: self.weekly_goal.pomodoros,
        };
        let previous =
            day.checked_sub_signed(chrono::Duration::weeks(1))
                .and_then(|previous_week_day| {
                    self.stats
                        .weekly_goal_snapshot_for_day(previous_week_day)
                        .map(|previous_target| {
                            let week = self.stats.weekly_for_day(previous_week_day);
                            (
                                previous_target,
                                week.focused_minutes(),
                                week.pomodoros_completed,
                            )
                        })
                });
        carry_over_goal_target(base, self.goal_carry_over.weekly, previous)
    }

    fn effective_monthly_goal_snapshot_for_day(&self, day: NaiveDate) -> DailyGoalSnapshot {
        let base = DailyGoalSnapshot {
            minutes: self.monthly_goal.minutes,
            pomodoros: self.monthly_goal.pomodoros,
        };
        let previous = previous_month_reference_day(day).and_then(|previous_month_day| {
            self.stats
                .monthly_goal_snapshot_for_day(previous_month_day)
                .map(|previous_target| {
                    let month = self.stats.monthly_for_day(previous_month_day);
                    (
                        previous_target,
                        month.focused_minutes(),
                        month.pomodoros_completed,
                    )
                })
        });
        carry_over_goal_target(base, self.goal_carry_over.monthly, previous)
    }

    fn sync_today_goal_snapshot(&mut self) {
        self.sync_goal_snapshot_for_day(Local::now().date_naive());
    }

    fn sync_goal_snapshot_for_day(&mut self, day: NaiveDate) {
        let day_key = day.format("%Y-%m-%d").to_string();
        let daily_changed = self
            .stats
            .sync_goal_snapshot(&day_key, self.current_goal_snapshot());
        let weekly_changed = self
            .stats
            .sync_weekly_goal_snapshot(day, self.current_week_goal_snapshot());
        let monthly_changed = self
            .stats
            .sync_monthly_goal_snapshot(day, self.current_month_goal_snapshot());
        if daily_changed || weekly_changed || monthly_changed {
            self.stats_dirty = true;
            self.flush_stats_if_dirty(false);
        }
    }

    pub fn recent_break_glass_overrides(&self, limit: usize) -> Vec<BreakGlassOverrideEvent> {
        self.stats.recent_break_glass_overrides(limit)
    }

    #[cfg(test)]
    pub fn recent_session_interruptions(&self, limit: usize) -> Vec<SessionInterruptionEvent> {
        self.stats.recent_session_interruptions(limit)
    }

    pub fn latest_session_interruption(&self) -> Option<SessionInterruptionEvent> {
        self.stats.latest_session_interruption()
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

fn normalize_timer_note_paste(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if c == '\r' || c == '\n' || c.is_control() {
                ' '
            } else {
                c
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
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

fn sort_one_time_windows(windows: &mut [OneTimeFocusWindowConfig]) {
    windows.sort_by(|left, right| {
        left.date
            .cmp(&right.date)
            .then(left.start.cmp(&right.start))
            .then(left.end.cmp(&right.end))
    });
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

fn parse_day_key(day_key: &str) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(day_key, "%Y-%m-%d").ok()
}

fn previous_month_reference_day(day: NaiveDate) -> Option<NaiveDate> {
    let month_start = NaiveDate::from_ymd_opt(day.year(), day.month(), 1)?;
    month_start.pred_opt()
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

fn goal_progress_for_totals(
    focused_minutes: u64,
    pomodoros_completed: u32,
    target_minutes: u64,
    target_pomodoros: u32,
) -> DailyGoalProgress {
    DailyGoalProgress {
        minutes: goal_progress(focused_minutes, target_minutes),
        pomodoros: goal_progress(u64::from(pomodoros_completed), u64::from(target_pomodoros)),
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

fn effective_blocked_sites_for_profile(profile: &BlocklistProfileConfig) -> Vec<String> {
    let allowlist: HashSet<String> = profile
        .allowlist_sites
        .iter()
        .map(|site| site.to_ascii_lowercase())
        .collect();
    profile
        .sites
        .iter()
        .filter(|site| !allowlist.contains(&site.to_ascii_lowercase()))
        .cloned()
        .collect()
}

fn permission_remediation_guidance() -> &'static str {
    if cfg!(target_os = "windows") {
        "Run focustime from an Administrator terminal, then open [d] Setup and press [r] Refresh."
    } else {
        "Run focustime with elevated privileges (e.g. sudo), verify hosts-file permissions, then press [r] Refresh."
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
        SetupCheck::warning(format!(
            "Blocked: write permission unavailable ({reason}). {}",
            permission_remediation_guidance()
        ))
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
        (false, true, Some(read_error), _) => SetupCheck::warning(format!(
            "Blocked: cannot read hosts file ({read_error}). {}",
            permission_remediation_guidance()
        )),
        (true, false, _, Some(write_error)) => SetupCheck::warning(format!(
            "Blocked: cannot write hosts file ({write_error}). {}",
            permission_remediation_guidance()
        )),
        (false, false, Some(read_error), Some(write_error)) => SetupCheck::warning(format!(
            "Blocked: read error ({read_error}); write error ({write_error}). {}",
            permission_remediation_guidance()
        )),
        _ => SetupCheck::warning(format!(
            "Blocked: hosts access diagnostics unavailable. {}",
            permission_remediation_guidance()
        )),
    }
}

fn task_label_key(label: &str) -> String {
    label.to_ascii_lowercase()
}

fn task_label_state_keys(labels: Vec<String>) -> BTreeSet<String> {
    labels
        .into_iter()
        .filter_map(|label| normalize_task_label(&label))
        .map(|label| task_label_key(&label))
        .collect()
}

fn task_label_state_labels(keys: &BTreeSet<String>, labels: &[String]) -> Vec<String> {
    if keys.is_empty() {
        return Vec::new();
    }

    let mut values = Vec::new();
    let mut seen = BTreeSet::new();
    for label in labels {
        let key = task_label_key(label);
        if keys.contains(&key) && seen.insert(key.clone()) {
            values.push(label.clone());
        }
    }
    for key in keys {
        if seen.insert(key.clone()) {
            values.push(key.clone());
        }
    }
    values
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
mod tests;
