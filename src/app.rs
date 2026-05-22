use std::{
    collections::{BTreeSet, HashMap, HashSet},
    time::{Duration, Instant},
};

use chrono::{DateTime, Datelike, Local, NaiveDate};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::blocker::{
    BlockingBackendKind, BlockingBackendPolicy, BlockingIntent, BlockingPreview,
    BlockingPreviewAction, BulkAddResult, CommandBlockingBackend, EditSiteResult,
    HostsFileDiagnostics, InvalidSiteInput, SiteBlocker,
};
use crate::config::{
    AppConfig, AutoStartConfig, AutomationTriggerRuleConfig, BlockingBackendConfig,
    BlockingBackendPolicyConfig, BlocklistCategoryConfig, BlocklistProfileConfig,
    BreakTemplateConfig, CommandBlockingBackendConfig, CustomProfileConfig, DailyGoalConfig,
    FeatureFlagsConfig, GoalCarryOverConfig, MonthlyGoalConfig, NotificationConfig,
    OneTimeFocusWindowConfig, ProfileAutomationConfig, ProfileAutomationSettingsConfig, ProfileId,
    RecurringFocusWindowConfig, RecurringScheduleConfig, ScheduleRuntimeConfig,
    SessionTemplateConfig, StatsRetentionConfig, ThemePreset, WakatimeMetadataConfig,
    WakatimeRuntimeConfig, WeekdayProfileRuleConfig, WeeklyGoalConfig,
    validate_automation_trigger_rules,
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
    SessionStats, StatsGrowthSummary, StatsRetentionPruneResult, TaskGoalProgress, TaskTotals,
    TaskTrend, WeeklyConsistency, WeeklyFocusScore, WeeklyStats, carry_over_goal_target,
    current_day_key,
};
use crate::task_labels::{normalize_task_label, task_label_index};
use crate::temporary_allowlist::{
    ActiveTemporaryAllowlistEntry, TemporaryAllowlistEntry,
    active_temporary_allowlist_status_entries_for_profile,
};
use crate::timer::{
    DEFAULT_FOCUS_SECS, DEFAULT_LONG_BREAK_INTERVAL, DEFAULT_LONG_BREAK_SECS,
    DEFAULT_SHORT_BREAK_SECS, TimerPhase, TimerState, TimerStatus,
};
use crate::wakatime::{
    WakatimeConfigStatus, WakatimeHeartbeatMetadata, WakatimeRuntimeOptions, WakatimeTracker,
};

mod automation_triggers;
mod break_glass;
mod cli_api;
mod feedback_diagnostics;
mod history_goals;
mod mode_keys;
mod persistence;
mod planner_labels;
mod profile_management;
mod schedule_editor;
mod schedule_runtime;
mod session_planner;
mod session_templates;
mod shortcuts;
mod site_manager;
mod temporary_allowlist;
mod timer_flow;
mod weekday_rules;
pub(crate) use history_goals::weekly_daily_goal_allocation_for_context;
use shortcuts::ShortcutBindings;
pub use shortcuts::{NavigationAction, ShortcutAction};

pub const PROFILE_IDS: [ProfileId; 3] =
    [ProfileId::Classic, ProfileId::DeepWork, ProfileId::Custom];
pub const PROFILE_EDIT_FIELD_LABELS: [&str; 52] = [
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
    "Weekday rule",
    "Weekday rule day",
    "Weekday rule profile",
    "Weekday rule blocklist",
    "Weekday rule template",
    "Weekday rule add/remove",
    "Theme preset",
    "Automation trigger",
    "Trigger condition",
    "Trigger time day",
    "Trigger time at",
    "Trigger action",
    "Action profile",
    "Action blocklist",
    "Action template",
    "Action delay",
    "Trigger add/remove",
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
const PROFILE_EDIT_WEEKDAY_RULE_INDEX: usize = 35;
const PROFILE_EDIT_WEEKDAY_RULE_DAY_INDEX: usize = 36;
const PROFILE_EDIT_WEEKDAY_RULE_PROFILE_INDEX: usize = 37;
const PROFILE_EDIT_WEEKDAY_RULE_BLOCKLIST_INDEX: usize = 38;
const PROFILE_EDIT_WEEKDAY_RULE_TEMPLATE_INDEX: usize = 39;
const PROFILE_EDIT_WEEKDAY_RULE_ADD_REMOVE_INDEX: usize = 40;
const PROFILE_EDIT_THEME_PRESET_INDEX: usize = 41;
const PROFILE_EDIT_AUTOMATION_TRIGGER_INDEX: usize = 42;
const PROFILE_EDIT_AUTOMATION_TRIGGER_CONDITION_INDEX: usize = 43;
const PROFILE_EDIT_AUTOMATION_TRIGGER_TIME_DAY_INDEX: usize = 44;
const PROFILE_EDIT_AUTOMATION_TRIGGER_TIME_AT_INDEX: usize = 45;
const PROFILE_EDIT_AUTOMATION_TRIGGER_ACTION_INDEX: usize = 46;
const PROFILE_EDIT_AUTOMATION_TRIGGER_PROFILE_INDEX: usize = 47;
const PROFILE_EDIT_AUTOMATION_TRIGGER_BLOCKLIST_INDEX: usize = 48;
const PROFILE_EDIT_AUTOMATION_TRIGGER_TEMPLATE_INDEX: usize = 49;
const PROFILE_EDIT_AUTOMATION_TRIGGER_DELAY_INDEX: usize = 50;
const PROFILE_EDIT_AUTOMATION_TRIGGER_ADD_REMOVE_INDEX: usize = 51;
const CUSTOM_DURATION_STEP_SECS: u64 = 60;
const DAILY_GOAL_MINUTES_STEP: u64 = 5;
const DEFAULT_BLOCKLIST_PROFILE_NAME: &str = "Default";
const DEFAULT_BLOCKLIST_CATEGORY_NAME: &str = "General";
const UNLINKED_BREAK_TEMPLATE_NAME: &str = "Custom";
#[cfg(not(test))]
const STATS_FILE_NAME: &str = "stats.toml";
pub(crate) const PLANNER_RECENT_LABEL_LIMIT: usize = 5;
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

fn blocklist_category_index(categories: &[BlocklistCategoryConfig], selected_name: &str) -> usize {
    categories
        .iter()
        .position(|category| category.name.eq_ignore_ascii_case(selected_name))
        .unwrap_or(0)
}

fn break_template_index(templates: &[BreakTemplateConfig], selected_name: &str) -> Option<usize> {
    templates
        .iter()
        .position(|template| template.name.eq_ignore_ascii_case(selected_name))
}

fn session_template_index(
    templates: &[SessionTemplateConfig],
    selected_name: &str,
) -> Option<usize> {
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

fn blocking_backend_policy_for_config(
    policy: BlockingBackendPolicyConfig,
) -> BlockingBackendPolicy {
    match policy {
        BlockingBackendPolicyConfig::HostsOnly => BlockingBackendPolicy::HostsOnly,
        BlockingBackendPolicyConfig::HostsThenCommand => BlockingBackendPolicy::HostsThenCommand,
        BlockingBackendPolicyConfig::CommandThenHosts => BlockingBackendPolicy::CommandThenHosts,
        BlockingBackendPolicyConfig::CommandOnly => BlockingBackendPolicy::CommandOnly,
    }
}

fn command_backend_for_config(config: &CommandBlockingBackendConfig) -> CommandBlockingBackend {
    CommandBlockingBackend {
        block_command: config.block_command.clone(),
        unblock_command: config.unblock_command.clone(),
        diagnostics_command: config.diagnostics_command.clone(),
    }
}

fn blocking_backend_policy_to_config(policy: BlockingBackendPolicy) -> BlockingBackendPolicyConfig {
    match policy {
        BlockingBackendPolicy::HostsOnly => BlockingBackendPolicyConfig::HostsOnly,
        BlockingBackendPolicy::HostsThenCommand => BlockingBackendPolicyConfig::HostsThenCommand,
        BlockingBackendPolicy::CommandThenHosts => BlockingBackendPolicyConfig::CommandThenHosts,
        BlockingBackendPolicy::CommandOnly => BlockingBackendPolicyConfig::CommandOnly,
    }
}

fn command_backend_to_config(
    command_backend: &CommandBlockingBackend,
) -> CommandBlockingBackendConfig {
    CommandBlockingBackendConfig {
        block_command: command_backend.block_command.clone(),
        unblock_command: command_backend.unblock_command.clone(),
        diagnostics_command: command_backend.diagnostics_command.clone(),
    }
}

fn blocking_backend_config_for_persistence(
    policy: BlockingBackendPolicy,
    command_backend: &CommandBlockingBackend,
) -> BlockingBackendConfig {
    BlockingBackendConfig {
        policy: blocking_backend_policy_to_config(policy),
        command: command_backend_to_config(command_backend),
    }
}

#[derive(Debug, Clone)]
struct ProfileEditSnapshot {
    custom_profile: CustomProfileConfig,
    notification_settings: NotificationConfig,
    auto_start: AutoStartConfig,
    recurring_schedule: RecurringScheduleConfig,
    automation_triggers: Vec<AutomationTriggerRuleConfig>,
    weekday_profile_rules: Vec<WeekdayProfileRuleConfig>,
    strict_mode: bool,
    daily_goal: DailyGoalConfig,
    weekly_goal: WeeklyGoalConfig,
    monthly_goal: MonthlyGoalConfig,
    goal_carry_over: GoalCarryOverConfig,
    selected_theme_preset: ThemePreset,
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
    CreateCategory,
    RenameCategory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlannerInputMode {
    Add,
    Rename,
    CreateTemplate,
    RenameTemplate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlannerPane {
    Tasks,
    Templates,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeeklyDailyAllocationDay {
    pub day: NaiveDate,
    pub minutes_target: u64,
    pub pomodoros_target: u32,
    pub allocatable: bool,
    pub weight_minutes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeeklyDailyGoalAllocation {
    pub week_target: DailyGoalSnapshot,
    pub completed_minutes: u64,
    pub completed_pomodoros: u32,
    pub remaining_minutes: u64,
    pub remaining_pomodoros: u32,
    pub remaining_days_in_week: usize,
    pub allocatable_days: usize,
    pub uses_schedule_weights: bool,
    pub daily_targets: Vec<WeeklyDailyAllocationDay>,
}

impl WeeklyDailyGoalAllocation {
    pub fn has_any_target(&self) -> bool {
        self.week_target.has_any_target()
    }

    pub fn today_target(&self) -> DailyGoalSnapshot {
        self.daily_targets
            .first()
            .map(|target| DailyGoalSnapshot {
                minutes: target.minutes_target,
                pomodoros: target.pomodoros_target,
            })
            .unwrap_or_default()
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
    pub backend_policy: String,
    pub backend_order: String,
    pub backend_selection: SetupCheck,
    pub blocking_permissions: SetupCheck,
    pub hosts_write_capability: SetupCheck,
    pub command_backend: SetupCheck,
    pub wakatime_config: SetupCheck,
    pub deprecation_warnings: Vec<String>,
}

impl SetupDiagnostics {
    fn collect(blocker: &SiteBlocker, deprecation_warnings: Vec<String>) -> Self {
        let hosts_diagnostics = blocker.hosts_file_diagnostics();
        let backend_status = blocker.backend_status();
        let blocking_permissions = blocking_permissions_check(&hosts_diagnostics);
        let hosts_write_capability = hosts_write_capability_check(&hosts_diagnostics);
        let hosts_file_path = hosts_diagnostics.path.clone();
        let backend_policy = backend_status.policy.id().to_string();
        let backend_order = backend_status
            .order
            .iter()
            .map(|backend| backend.id())
            .collect::<Vec<_>>()
            .join(" -> ");
        let backend_selection = backend_selection_check(
            backend_status.last_backend,
            backend_status.fallback_used,
            backend_status.last_error.as_deref(),
        );
        let command_backend = command_backend_check(blocker);
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
            backend_policy,
            backend_order,
            backend_selection,
            blocking_permissions,
            hosts_write_capability,
            command_backend,
            wakatime_config,
            deprecation_warnings,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockingPreviewSnapshot {
    pub backend: Option<BlockingBackendKind>,
    pub backend_target: Option<String>,
    pub attempted_backends: Vec<BlockingBackendKind>,
    pub fallback_used: bool,
    pub action: BlockingPreviewAction,
    pub would_change: bool,
    pub effective_blocked_sites_count: usize,
    pub section: Option<String>,
    pub error: Option<String>,
}

impl Default for BlockingPreviewSnapshot {
    fn default() -> Self {
        Self {
            backend: None,
            backend_target: None,
            attempted_backends: Vec::new(),
            fallback_used: false,
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
    pub session_templates: Vec<SessionTemplateConfig>,
    active_session_template: Option<usize>,
    pub blocklist_profile_input: String,
    pub blocklist_profile_input_active: bool,
    blocklist_profile_input_mode: Option<BlocklistProfileInputMode>,
    pub site_feedback: Option<SiteFeedback>,
    pub task_labels: Vec<String>,
    pub selected_task_label: Option<String>,
    task_label_favorites: BTreeSet<String>,
    task_label_archived: BTreeSet<String>,
    pub planner_selection_index: usize,
    pub planner_template_selection_index: usize,
    pub planner_pane: PlannerPane,
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
    selected_theme_preset: ThemePreset,
    feature_flags: FeatureFlagsConfig,
    config_deprecation_warnings: Vec<String>,
    profile_automation: ProfileAutomationSettingsConfig,
    automation_triggers: Vec<AutomationTriggerRuleConfig>,
    automation_trigger_last_fired_minute: HashMap<usize, i64>,
    weekday_profile_rules: Vec<WeekdayProfileRuleConfig>,
    pub custom_profile: CustomProfileConfig,
    pub profile_selection_index: usize,
    pub profile_edit_active: bool,
    pub profile_edit_field: usize,
    profile_edit_schedule_window: usize,
    profile_edit_schedule_day: usize,
    profile_edit_schedule_exception: usize,
    profile_edit_one_time_window: usize,
    profile_edit_weekday_rule: usize,
    profile_edit_automation_trigger: usize,
    profile_edit_automation_triggers: Vec<AutomationTriggerRuleConfig>,
    profile_edit_snapshot: Option<ProfileEditSnapshot>,
    notification_settings: NotificationConfig,
    auto_start: AutoStartConfig,
    recurring_schedule: RecurringScheduleConfig,
    schedule_runtime: ScheduleRuntimeConfig,
    recurring_windows: Vec<RecurringWindow>,
    recurring_exception_dates: HashSet<NaiveDate>,
    one_time_windows: Vec<OneTimeWindow>,
    schedule_armed_occurrence_key: Option<String>,
    schedule_delayed_occurrence_key: Option<String>,
    schedule_delay_until: Option<DateTime<Local>>,
    last_schedule_occurrence_key: Option<String>,
    last_active_schedule_occurrence_key: Option<String>,
    last_weekday_profile_sync_day: Option<NaiveDate>,
    current_frame_now: DateTime<Local>,
    pub strict_mode: bool,
    break_glass_duration_secs: u64,
    break_glass_expires_at: Option<Instant>,
    temporary_allowlist_entries: Vec<TemporaryAllowlistEntry>,
    daily_goal: DailyGoalConfig,
    weekly_goal: WeeklyGoalConfig,
    monthly_goal: MonthlyGoalConfig,
    goal_carry_over: GoalCarryOverConfig,
    stats_retention: StatsRetentionConfig,
    wakatime_metadata: WakatimeMetadataConfig,
    wakatime_runtime: WakatimeRuntimeConfig,
    pending_timer_action: Option<PendingTimerAction>,
    notifier: PhaseNotifier,
    stats: FocusStats,
    stats_dirty: bool,
    stats_has_unsaved_elapsed: bool,
    shortcuts: ShortcutBindings,
}

impl App {
    pub fn new() -> Self {
        #[cfg(test)]
        {
            Self::from_config(AppConfig::default())
        }
        #[cfg(not(test))]
        {
            let (config, deprecation_warnings) = AppConfig::load_with_deprecation_warnings();
            Self::from_config_with_deprecation_warnings(config, deprecation_warnings)
        }
    }

    #[cfg(test)]
    fn from_config(config: AppConfig) -> Self {
        Self::from_config_with_deprecation_warnings(config, Vec::new())
    }

    fn from_config_with_deprecation_warnings(
        config: AppConfig,
        config_deprecation_warnings: Vec<String>,
    ) -> Self {
        let config = config.normalized();
        let selected_profile = config.selected_profile;
        let selected_theme_preset = config.selected_theme_preset;
        let feature_flags = config.feature_flags;
        let setup_deprecation_warnings = setup_deprecation_warnings(&config_deprecation_warnings);
        let custom_profile = config.effective_custom_profile();
        let profile_automation = config.profile_automation.clone().unwrap_or_default();
        let mut automation_triggers = config.automation_triggers.clone();
        let weekday_profile_rules = config.weekday_profile_rules.clone();
        let selected_automation = config.profile_automation_for(selected_profile);
        let notification_settings = selected_automation.notifications;
        let auto_start = selected_automation.auto_start;
        let recurring_schedule = selected_automation.recurring_schedule.clone();
        let recurring_windows = compile_windows(&recurring_schedule.windows);
        let recurring_exception_dates =
            compile_exception_dates(&recurring_schedule.exception_dates);
        let one_time_windows = compile_one_time_windows(&recurring_schedule.one_time_windows);
        let strict_mode = selected_automation.strict_mode;
        let schedule_runtime = config.schedule_runtime;
        let break_glass_duration_secs = config.break_glass_duration_secs;
        let daily_goal = config.daily_goal;
        let weekly_goal = config.weekly_goal;
        let monthly_goal = config.monthly_goal;
        let goal_carry_over = config.goal_carry_over;
        let stats_retention = config.stats_retention;
        let wakatime_metadata = config.wakatime;
        let wakatime_runtime = config.wakatime_runtime;
        let blocklist_profiles = config.blocklist_profiles.clone();
        let active_blocklist_profile =
            blocklist_profile_index(&blocklist_profiles, &config.selected_blocklist_profile);
        let break_templates = config.break_templates.clone();
        let (shortcuts, shortcut_diagnostics) =
            ShortcutBindings::from_config_with_diagnostics(&config.shortcuts);
        let shortcut_config_error = (!shortcut_diagnostics.is_empty()).then(|| {
            format!(
                "shortcut config adjusted: {}",
                shortcut_diagnostics.join(" ")
            )
        });
        let automation_trigger_config_error = validate_automation_trigger_rules(
            &automation_triggers,
            &blocklist_profiles,
            &config.session_templates,
        )
        .err()
        .map(|error| format!("automation trigger config ignored: {error}"));
        if automation_trigger_config_error.is_some() {
            automation_triggers.clear();
        }
        let initial_config_error = match (shortcut_config_error, automation_trigger_config_error) {
            (Some(shortcut), Some(trigger)) => Some(format!("{shortcut} {trigger}")),
            (Some(shortcut), None) => Some(shortcut),
            (None, Some(trigger)) => Some(trigger),
            (None, None) => None,
        };
        let active_break_template = resolve_active_break_template(
            &break_templates,
            &config.selected_break_template,
            &custom_profile,
        );
        let session_templates = config.session_templates.clone();
        let active_session_template =
            session_template_index(&session_templates, &config.selected_session_template);
        let profile_spec = profile_spec_for(selected_profile, &custom_profile);
        let (mut stats, stats_error) =
            match FocusStats::load_with_options(crate::stats::StatsLoadOptions::default()) {
                Ok(stats) => (stats, legacy_stats_path_migration_warning()),
                Err(e) => (FocusStats::default(), Some(e)),
            };
        let retained = stats.apply_retention_policy(stats_retention, Local::now().date_naive());
        let (task_labels, selected_task_label) = stats.task_planner_state();
        let task_label_favorites = task_label_state_keys(stats.task_label_favorites());
        let task_label_archived = task_label_state_keys(stats.task_label_archived());
        let planner_selection_index = 0;
        let planner_template_selection_index = active_session_template.unwrap_or(0);
        let timer = TimerState::with_profile(
            profile_spec.focus_secs,
            profile_spec.short_break_secs,
            profile_spec.long_break_secs,
            profile_spec.long_break_interval,
        );
        let blocker = SiteBlocker::with_backend_config(
            blocking_backend_policy_for_config(config.blocking_backend.policy),
            command_backend_for_config(&config.blocking_backend.command),
        );
        let setup_diagnostics =
            SetupDiagnostics::collect(&blocker, setup_deprecation_warnings.clone());
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
            session_templates,
            active_session_template,
            blocklist_profile_input: String::new(),
            blocklist_profile_input_active: false,
            blocklist_profile_input_mode: None,
            site_feedback: None,
            task_labels,
            selected_task_label,
            task_label_favorites,
            task_label_archived,
            planner_selection_index,
            planner_template_selection_index,
            planner_pane: PlannerPane::Tasks,
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
            config_error: initial_config_error,
            stats_error,
            history_feedback: None,
            phase_notification: None,
            wakatime: WakatimeTracker::new_with_settings(
                WakatimeHeartbeatMetadata {
                    project: wakatime_metadata.project.clone(),
                    language: wakatime_metadata.language.clone(),
                },
                WakatimeRuntimeOptions {
                    retry_backoff_secs: wakatime_runtime.retry_backoff_secs.clone(),
                    queue_capacity: wakatime_runtime.queue_capacity,
                    queue_retry_delay_secs: wakatime_runtime.queue_retry_delay_secs,
                },
            ),
            selected_profile,
            selected_theme_preset,
            feature_flags,
            config_deprecation_warnings,
            profile_automation,
            automation_triggers,
            automation_trigger_last_fired_minute: HashMap::new(),
            weekday_profile_rules,
            custom_profile,
            profile_selection_index: profile_index(selected_profile),
            profile_edit_active: false,
            profile_edit_field: 0,
            profile_edit_schedule_window: 0,
            profile_edit_schedule_day: 0,
            profile_edit_schedule_exception: 0,
            profile_edit_one_time_window: 0,
            profile_edit_weekday_rule: 0,
            profile_edit_automation_trigger: 0,
            profile_edit_automation_triggers: Vec::new(),
            profile_edit_snapshot: None,
            notification_settings,
            auto_start,
            recurring_schedule,
            schedule_runtime,
            recurring_windows,
            recurring_exception_dates,
            one_time_windows,
            schedule_armed_occurrence_key: None,
            schedule_delayed_occurrence_key: None,
            schedule_delay_until: None,
            last_schedule_occurrence_key: None,
            last_active_schedule_occurrence_key: None,
            last_weekday_profile_sync_day: None,
            current_frame_now: Local::now(),
            strict_mode,
            break_glass_duration_secs,
            break_glass_expires_at: None,
            temporary_allowlist_entries: Vec::new(),
            daily_goal,
            weekly_goal,
            monthly_goal,
            goal_carry_over,
            stats_retention,
            wakatime_metadata,
            wakatime_runtime,
            pending_timer_action: None,
            notifier: PhaseNotifier::new(notification_settings),
            stats,
            stats_dirty: retained.any_removed(),
            stats_has_unsaved_elapsed: false,
            shortcuts,
        };
        app.clamp_break_template_selection();
        app.recompute_blocker_sites_from_active_profile();
        app.restore_in_progress_session();
        app.restore_cli_workflow_state();
        app.sync_weekday_profile_rules(Local::now());
        app.sync_planner_selection_to_selected_label();
        app.sync_recovery_snapshot();
        app.apply_blocking_for_phase();
        app.refresh_setup_diagnostics();
        app.sync_today_goal_snapshot();
        app
    }

    #[cfg(test)]
    pub fn from_config_for_tests(config: AppConfig) -> Self {
        Self::from_config(config)
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
        self.sync_temporary_allowlist_entries(now);
        self.sync_break_glass_override();
        self.sync_weekday_profile_rules(now);
        self.sync_recurring_schedule(now);
        self.sync_time_based_automation_triggers(now);
    }

    pub fn selected_profile_name(&self) -> &'static str {
        self.selected_profile.label()
    }

    pub fn selected_theme_preset(&self) -> ThemePreset {
        self.selected_theme_preset
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

    pub(super) fn shortcut_matches(&self, action: ShortcutAction, key: &KeyEvent) -> bool {
        self.shortcuts.matches(action, key)
    }

    pub fn shortcut_hint(&self, action: ShortcutAction) -> String {
        self.shortcuts.hint(action)
    }

    pub fn shortcut_label(&self, action: ShortcutAction) -> String {
        self.shortcuts.label(action)
    }

    pub fn navigation_matches(&self, action: NavigationAction, key: &KeyEvent) -> bool {
        self.shortcuts.navigation_matches(action, key)
    }

    pub fn navigation_hint(&self, action: NavigationAction) -> String {
        self.shortcuts.navigation_hint(action)
    }

    pub fn navigation_label(&self, action: NavigationAction) -> String {
        self.shortcuts.navigation_label(action)
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
        self.stats.goal_streak_with_day_goal(
            day,
            current_goal,
            self.stats.daily_for(day_key),
            |day| self.effective_daily_goal_snapshot_for_day(day),
        )
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

    pub fn stats_growth_summary(&self) -> StatsGrowthSummary {
        self.stats.growth_summary()
    }

    pub fn stats_retention_config(&self) -> StatsRetentionConfig {
        self.stats_retention
    }

    pub fn stats_retention_preview(&self) -> StatsRetentionPruneResult {
        self.stats
            .retention_preview(self.stats_retention, Local::now().date_naive())
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
        if (PROFILE_EDIT_SCHEDULE_WINDOW_INDEX..=PROFILE_EDIT_WEEKDAY_RULE_ADD_REMOVE_INDEX)
            .contains(&field_index)
            || (PROFILE_EDIT_AUTOMATION_TRIGGER_INDEX
                ..=PROFILE_EDIT_AUTOMATION_TRIGGER_ADD_REMOVE_INDEX)
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
            PROFILE_EDIT_THEME_PRESET_INDEX => self.selected_theme_preset.label().to_string(),
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

    pub(super) fn handle_profile_edit_metadata_input(&mut self, key: &KeyEvent) -> bool {
        if self.navigation_matches(NavigationAction::Backspace, key) {
            let Some(field_value) = self.profile_edit_metadata_field_mut() else {
                return false;
            };
            field_value.pop();
            return true;
        }

        let Some(field_value) = self.profile_edit_metadata_field_mut() else {
            return false;
        };

        match key.code {
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

    pub fn active_temporary_allowlist_entries(&self) -> Vec<ActiveTemporaryAllowlistEntry> {
        active_temporary_allowlist_status_entries_for_profile(
            &self.temporary_allowlist_entries,
            self.active_blocklist_profile_name(),
            self.current_frame_now.timestamp(),
        )
    }

    pub fn active_temporary_allowlist_count(&self) -> usize {
        self.active_temporary_allowlist_entries().len()
    }

    pub fn next_temporary_allowlist_expiry_remaining_secs(&self) -> Option<u64> {
        self.active_temporary_allowlist_entries()
            .first()
            .map(|entry| entry.remaining_secs)
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

    pub fn active_blocklist_category_name(&self) -> &str {
        let Some(profile) = self.blocklist_profiles.get(self.active_blocklist_profile) else {
            return DEFAULT_BLOCKLIST_CATEGORY_NAME;
        };
        if profile.categories.is_empty() {
            if profile.selected_category.trim().is_empty() {
                return DEFAULT_BLOCKLIST_CATEGORY_NAME;
            }
            return profile.selected_category.as_str();
        }
        let index = blocklist_category_index(&profile.categories, &profile.selected_category)
            .min(profile.categories.len().saturating_sub(1));
        profile
            .categories
            .get(index)
            .map(|category| category.name.as_str())
            .unwrap_or(DEFAULT_BLOCKLIST_CATEGORY_NAME)
    }

    pub fn active_blocklist_category_position(&self) -> usize {
        let Some(profile) = self.blocklist_profiles.get(self.active_blocklist_profile) else {
            return 1;
        };
        if profile.categories.is_empty() {
            return 1;
        }
        blocklist_category_index(&profile.categories, &profile.selected_category).saturating_add(1)
    }

    pub fn blocklist_category_count(&self) -> usize {
        self.blocklist_profiles
            .get(self.active_blocklist_profile)
            .map(|profile| profile.categories.len().max(1))
            .unwrap_or(1)
    }

    pub fn active_break_template_name(&self) -> &str {
        self.active_break_template
            .and_then(|index| self.break_templates.get(index))
            .map(|template| template.name.as_str())
            .unwrap_or(UNLINKED_BREAK_TEMPLATE_NAME)
    }

    pub fn active_session_template_name(&self) -> Option<&str> {
        self.active_session_template
            .and_then(|index| self.session_templates.get(index))
            .map(|template| template.name.as_str())
    }

    pub fn session_template_count(&self) -> usize {
        self.session_templates.len()
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

    fn selected_session_template_for_persistence(&self) -> String {
        self.active_session_template
            .and_then(|index| self.session_templates.get(index))
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
        self.sync_wakatime_metadata_to_tracker();
        self.sync_wakatime_tracking_for_state();
    }

    fn handle_quit_key(&mut self, key: &KeyEvent, esc_quits: bool) -> bool {
        let is_quit_char = self.shortcut_matches(ShortcutAction::Quit, key);
        let is_esc_quit = key.code == KeyCode::Esc && esc_quits;
        let is_ctrl_c =
            matches!(key.code, KeyCode::Char('c')) && key.modifiers.contains(KeyModifiers::CONTROL);
        let is_quit_key = is_quit_char || is_esc_quit || is_ctrl_c;
        if is_quit_key && self.strict_mode_enforced_for_focus() {
            self.phase_notification =
                Some("Strict mode active. Finish or stop focus before quitting.".to_string());
            return true;
        }

        if is_quit_char || is_esc_quit || is_ctrl_c {
            self.should_quit = true;
            true
        } else {
            false
        }
    }

    fn open_setup_diagnostics(&mut self) {
        self.pending_timer_action = None;
        self.refresh_setup_diagnostics();
        self.mode = AppMode::SetupDiagnostics;
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
    crate::config::effective_blocked_sites_for_profile(profile)
}

fn permission_remediation_guidance() -> &'static str {
    if cfg!(target_os = "windows") {
        "Run focustime from an Administrator terminal, then open [d] Setup and press [r] Refresh."
    } else {
        "Run focustime with elevated privileges (e.g. sudo), verify hosts-file permissions, then press [r] Refresh."
    }
}

pub(super) fn setup_deprecation_warnings(config_deprecation_warnings: &[String]) -> Vec<String> {
    let mut warnings = config_deprecation_warnings.to_vec();
    if let Some(stats_warning) = legacy_stats_path_migration_warning() {
        warnings.push(stats_warning);
    }
    warnings
}

pub(super) fn format_legacy_stats_path_migration_warning(
    canonical_path: &std::path::Path,
    legacy_path: &std::path::Path,
) -> String {
    format!(
        "Legacy stats path `{}` is still in use while canonical stats `{}` are missing. Copy `{}` to `{}` (you can use `focustime --backup` and `focustime --restore` to assist).",
        legacy_path.display(),
        canonical_path.display(),
        legacy_path.display(),
        canonical_path.display()
    )
}

#[cfg(not(test))]
fn legacy_stats_path_migration_warning() -> Option<String> {
    let canonical_path = crate::config::stats_data_path(STATS_FILE_NAME)?;
    if canonical_path.exists() {
        return None;
    }
    let legacy_path =
        crate::config::app_data_path(STATS_FILE_NAME).filter(|path| path != &canonical_path)?;
    if !legacy_path.exists() {
        return None;
    }
    Some(format_legacy_stats_path_migration_warning(
        &canonical_path,
        &legacy_path,
    ))
}

#[cfg(test)]
fn legacy_stats_path_migration_warning() -> Option<String> {
    None
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

fn backend_selection_check(
    last_backend: Option<BlockingBackendKind>,
    fallback_used: bool,
    last_error: Option<&str>,
) -> SetupCheck {
    if let Some(error) = last_error {
        return SetupCheck::warning(format!("Blocked: backend selection failed ({error})"));
    }
    if let Some(backend) = last_backend {
        if fallback_used {
            return SetupCheck::warning(format!(
                "Fallback active: using `{}` backend after primary backend failure",
                backend.id()
            ));
        }
        return SetupCheck::ok(format!("Ready: using `{}` backend", backend.id()));
    }
    SetupCheck::warning(
        "Awaiting first block/unblock operation to confirm selected backend".to_string(),
    )
}

fn command_backend_check(blocker: &SiteBlocker) -> SetupCheck {
    match blocker.command_backend_diagnostics() {
        Ok(()) => SetupCheck::ok("Ready: command backend diagnostics passed"),
        Err(error) => SetupCheck::warning(format!(
            "Blocked: command backend unavailable ({error}). Configure commands or use hosts backend."
        )),
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
