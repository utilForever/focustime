use std::{
    ffi::OsString,
    io::{self, Write},
    path::PathBuf,
};

use chrono::{Datelike, NaiveDate};
use serde::Serialize;

#[cfg(test)]
use crate::app::App;
use crate::app::{SetupCheck, SetupCheckLevel, SetupDiagnostics};
use crate::blocker::{BlockingPreviewAction, EditSiteResult, InvalidSiteInput, SiteBlocker};
use crate::config::{
    AppConfig, AutomationTriggerRuleConfig, BlocklistProfileConfig, BreakTemplateConfig,
    CustomProfileConfig, DailyGoalConfig, MonthlyGoalConfig, OneTimeFocusWindowConfig, ProfileId,
    RecurringFocusWindowConfig, RecurringScheduleConfig, ThemePreset, WeekdayProfileRuleConfig,
    WeeklyGoalConfig,
};
use crate::schedule::{format_schedule_conflict, inspect_schedule_conflicts_from_config};
use crate::session_recovery;
use crate::stats::{
    DailyGoalSnapshot, FocusStats, SessionInterruptionEvent, StatsGrowthSummary,
    StatsRetentionPruneResult, carry_over_goal_target, current_day_key,
};
use crate::timer::{
    DEFAULT_FOCUS_SECS, DEFAULT_LONG_BREAK_INTERVAL, DEFAULT_LONG_BREAK_SECS,
    DEFAULT_SHORT_BREAK_SECS, TimerPhase, TimerStatus,
};

mod args;
mod execute;
mod output;
mod parsing;
mod status;
#[cfg(test)]
use args::classify_key_value_arg;
use args::{classify_args, infer_output_mode_from_os_args};
use execute::execute_cli_command;
#[cfg(test)]
use execute::{
    apply_blocklist_profile_command, apply_site_add_command, apply_site_delete_command,
    apply_site_edit_command,
};
use output::{
    build_blocking_preview_command_output, build_diagnostics_command_output,
    build_schedule_inspection_output, display_input_value, effective_blocked_sites_for_profile,
    flush_stdout, print_automation_triggers_command_output, print_backup_output,
    print_blocking_preview_command_output, print_blocklist_profile_command_output,
    print_break_glass_command_output, print_diagnostics_command_output, print_export_output,
    print_goal_carry_command_output, print_goal_command_output, print_json, print_json_compact,
    print_profile_output, print_restore_output, print_schedule_command_output,
    print_schedule_delay_command_output, print_session_metadata_command_output,
    print_session_template_command_output, print_site_add_command_output,
    print_site_delete_command_output, print_site_edit_command_output,
    print_site_list_command_output, print_status_output, print_strict_command_output,
    print_task_goal_command_output, print_temporary_site_add_command_output,
    print_theme_command_output, print_timer_state_output, print_weekday_rules_command_output,
};
use parsing::{
    finalize_cli_action, invalid_usage, parse_automation_triggers_value, parse_global_tokens,
    parse_goal_carry_value, parse_goal_value, parse_monthly_goal_value, parse_primary_command,
    parse_profile_id, parse_schedule_value, parse_site_edit_value, parse_strict_value,
    parse_task_goal_value, parse_theme_preset, parse_watch_interval_option,
    parse_watch_interval_secs, parse_weekday_rules_value, parse_weekly_goal_value,
    require_nonempty_key_value,
};
use status::{
    available_break_template_views, available_theme_preset_views, build_status_output,
    build_task_goal_output, profile_id, profile_view, selected_break_template_view,
    theme_preset_view, timer_phase_id, timer_status_id,
};

const USAGE_TEXT: &str = r#"Usage:
  focustime
  focustime --start [--json]
  focustime --pause [--json]
  focustime --resume [--json]
  focustime --stop [--json]
  focustime --next [--json]
  focustime --task=LABEL [--json]
  focustime --task-goal [LABEL|LABEL:MINUTES,POMODOROS] [--json]
  focustime --task-goal=LABEL[:MINUTES,POMODOROS] [--json]
  focustime --focus-intention [TEXT] [--json]
  focustime --focus-intention=TEXT [--json]
  focustime --task-note [TEXT] [--json]
  focustime --task-note=TEXT [--json]
  focustime --profile [classic|deep-work|custom] [--json]
  focustime --theme [classic|high-contrast|deuteranopia-friendly] [--json]
  focustime --goal [--json]
  focustime --goal=MINUTES,POMODOROS [--json]
  focustime --goal-weekly [--json]
  focustime --goal-weekly=MINUTES,POMODOROS [--json]
  focustime --goal-monthly [--json]
  focustime --goal-monthly=MINUTES,POMODOROS [--json]
  focustime --goal-carry [--json]
  focustime --goal-carry=on|off [--json]
  focustime --goal-carry-weekly [--json]
  focustime --goal-carry-weekly=on|off [--json]
  focustime --goal-carry-monthly [--json]
  focustime --goal-carry-monthly=on|off [--json]
  focustime --strict [--json]
  focustime --strict=on|off [--json]
  focustime --schedule [--json]
  focustime --schedule-set=JSON_PAYLOAD [--json]
  focustime --weekday-rules [--json]
  focustime --weekday-rules-set=JSON_PAYLOAD [--json]
  focustime --automation-triggers [--json]
  focustime --automation-triggers-set=JSON_PAYLOAD [--json]
  focustime --schedule-delay [--json]
  focustime --break-glass-trigger [--json]
  focustime --break-glass-cancel [--json]
  focustime --blocklist-profile [PROFILE_NAME] [--json]
  focustime --blocklist-profile-create=PROFILE_NAME [--json]
  focustime --blocklist-profile-rename=PROFILE_NAME [--json]
  focustime --blocklist-profile-delete [--json]
  focustime --session-template [TEMPLATE_NAME] [--json]
  focustime --session-template-apply [TEMPLATE_NAME] [--json]
  focustime --session-template-create=TEMPLATE_NAME [--json]
  focustime --session-template-rename=TEMPLATE_NAME [--json]
  focustime --session-template-delete [--json]
  focustime --blocklist-sites [--json]
  focustime --allowlist-sites [--json]
  focustime --blocklist-site-add=HOSTNAMES [--json]
  focustime --allowlist-site-add=HOSTNAMES [--json]
  focustime --allowlist-site-add-temporary=HOST_DURATIONS [--json]
  focustime --blocklist-site-edit=OLD=NEW [--json]
  focustime --allowlist-site-edit=OLD=NEW [--json]
  focustime --blocklist-site-delete=HOSTNAME [--json]
  focustime --allowlist-site-delete=HOSTNAME [--json]
  focustime --diagnostics [--json]
  focustime --blocking-preview [--json]
  focustime --status [--watch[=SECONDS]] [--json]
  focustime --backup[=DIR] [--json]
  focustime --restore[=DIR] [--json]
  focustime --export[=DIR] [--json]

Options:
  --start         Start a focus timer without launching TUI
  --pause         Pause a running timer
  --resume        Resume a paused timer
  --stop          Stop/reset the current phase
  --next          Skip to the next phase
  --task          Select task label (auto-creates unknown labels)
  --task-goal     Show or set per-task cumulative goal targets
  --focus-intention  Show current focus intention, or set it for the active/paused focus session
  --task-note        Show current task note, or set it for the active/paused focus session
  --profile       Show current profile, or set it when value is provided
  --theme         Show current theme preset, or set it when value is provided
  --goal          Show current daily goal, or set minutes/pomodoros targets
  --goal-weekly   Show current weekly goal, or set minutes/pomodoros targets
  --goal-monthly  Show current monthly goal, or set minutes/pomodoros targets
  --goal-carry          Show daily goal carry-over, or set on/off
  --goal-carry-weekly   Show weekly goal carry-over, or set on/off
  --goal-carry-monthly  Show monthly goal carry-over, or set on/off
  --strict        Show strict mode for selected profile, or set on/off
  --schedule      Show selected profile schedule with overlap/conflict inspection
  --schedule-set  Replace selected profile schedule (recurring + one-time) from JSON payload
  --weekday-rules      Show weekday smart-switch rules
  --weekday-rules-set  Replace weekday smart-switch rules from JSON payload
  --automation-triggers      Show automation trigger rules
  --automation-triggers-set  Replace automation trigger rules from JSON payload
  --schedule-delay  Delay the current active schedule window start by 10 minutes
  --break-glass-trigger  Trigger break-glass workflow (first call arms, second confirms)
  --break-glass-cancel   Cancel a pending break-glass confirmation
  --blocklist-profile         Show active blocklist profile, or set active profile
  --blocklist-profile-create  Create a blocklist profile and select it
  --blocklist-profile-rename  Rename the active blocklist profile
  --blocklist-profile-delete  Delete the active blocklist profile
  --session-template         Show active session template, or set active template
  --session-template-apply   Apply a template by name (or apply active template)
  --session-template-create  Capture current task/profile/blocklist/schedule as a template
  --session-template-rename  Rename the active session template
  --session-template-delete  Delete the active session template
  --blocklist-sites           List blocklist sites in active profile
  --allowlist-sites           List allowlist sites in active profile
  --blocklist-site-add        Add/import blocklist hostnames in active profile
  --allowlist-site-add        Add/import allowlist hostnames in active profile
  --allowlist-site-add-temporary  Add temporary allowlist hostnames with inline duration (HOST=30m,HOST=45s)
  --blocklist-site-edit       Replace blocklist hostname using OLD=NEW
  --allowlist-site-edit       Replace allowlist hostname using OLD=NEW
  --blocklist-site-delete     Delete blocklist hostname in active profile
  --allowlist-site-delete     Delete allowlist hostname in active profile
  --diagnostics   Show setup diagnostics checks
  --blocking-preview  Preview backend-selected blocking changes without writing
  --status        Print status summary (includes live timer/session fields and latest interruption)
  --watch         Stream periodic status updates (status command only; default 1s)
  --backup        Back up config.toml and stats.toml to current directory or DIR
  --restore       Restore config.toml and stats.toml from current directory or DIR
  --export        Export stats to current directory or DIR
  --json          Emit machine-readable JSON output
  -h, --help      Show this help"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Text,
    Json,
}

const EXIT_CODE_RUNTIME_ERROR: i32 = 1;
const EXIT_CODE_USAGE_ERROR: i32 = 2;
const DEFAULT_WATCH_INTERVAL_SECS: u64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CliErrorKind {
    Usage,
    Runtime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliError {
    kind: CliErrorKind,
    output: OutputMode,
    message: String,
}

impl CliError {
    pub fn exit_code(&self) -> i32 {
        match self.kind {
            CliErrorKind::Usage => EXIT_CODE_USAGE_ERROR,
            CliErrorKind::Runtime => EXIT_CODE_RUNTIME_ERROR,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct CliErrorEnvelope {
    ok: bool,
    error: CliErrorPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct CliErrorPayload {
    kind: CliErrorKind,
    exit_code: i32,
    message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandKind {
    Start,
    Pause,
    Resume,
    Stop,
    Next,
    Task {
        label: String,
    },
    TaskGoal {
        label: Option<String>,
        goal: Option<DailyGoalConfig>,
    },
    FocusIntention {
        value: Option<String>,
    },
    TaskNote {
        value: Option<String>,
    },
    Profile {
        profile: Option<ProfileId>,
    },
    Theme {
        preset: Option<ThemePreset>,
    },
    Goal {
        goal: Option<DailyGoalConfig>,
    },
    GoalWeekly {
        goal: Option<WeeklyGoalConfig>,
    },
    GoalMonthly {
        goal: Option<MonthlyGoalConfig>,
    },
    GoalCarry {
        enabled: Option<bool>,
    },
    GoalCarryWeekly {
        enabled: Option<bool>,
    },
    GoalCarryMonthly {
        enabled: Option<bool>,
    },
    Strict {
        enabled: Option<bool>,
    },
    Schedule {
        schedule: Option<RecurringScheduleConfig>,
    },
    WeekdayRules {
        rules: Option<Vec<WeekdayProfileRuleConfig>>,
    },
    AutomationTriggers {
        rules: Option<Vec<AutomationTriggerRuleConfig>>,
    },
    ScheduleDelay,
    BreakGlassTrigger,
    BreakGlassCancel,
    Diagnostics,
    BlockingPreview,
    Status {
        watch_interval_secs: Option<u64>,
    },
    Backup {
        dir: Option<PathBuf>,
    },
    Restore {
        dir: Option<PathBuf>,
    },
    Export {
        dir: Option<PathBuf>,
    },
    BlocklistProfile {
        command: BlocklistProfileCommandKind,
    },
    BlocklistSites {
        target: SiteListTarget,
        command: BlocklistSiteCommandKind,
    },
    AllowlistSiteAddTemporary {
        input: String,
    },
    SessionTemplate {
        command: SessionTemplateCommandKind,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliCommand {
    pub kind: CommandKind,
    pub output: OutputMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliAction {
    RunTui,
    RunCommand(CliCommand),
    ShowHelp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PrimaryCommand {
    Start,
    Pause,
    Resume,
    Stop,
    Next,
    Task(String),
    TaskGoal {
        label: Option<String>,
        goal: Option<DailyGoalConfig>,
    },
    FocusIntention(Option<String>),
    TaskNote(Option<String>),
    Profile(Option<ProfileId>),
    Theme(Option<ThemePreset>),
    Goal(Option<DailyGoalConfig>),
    GoalWeekly(Option<WeeklyGoalConfig>),
    GoalMonthly(Option<MonthlyGoalConfig>),
    GoalCarry(Option<bool>),
    GoalCarryWeekly(Option<bool>),
    GoalCarryMonthly(Option<bool>),
    Strict(Option<bool>),
    Schedule,
    ScheduleSet(RecurringScheduleConfig),
    WeekdayRules,
    WeekdayRulesSet(Vec<WeekdayProfileRuleConfig>),
    AutomationTriggers,
    AutomationTriggersSet(Vec<AutomationTriggerRuleConfig>),
    ScheduleDelay,
    BreakGlassTrigger,
    BreakGlassCancel,
    Diagnostics,
    BlockingPreview,
    Status,
    Backup(Option<PathBuf>),
    Restore(Option<PathBuf>),
    Export(Option<PathBuf>),
    BlocklistProfile(Option<String>),
    BlocklistProfileCreate(String),
    BlocklistProfileRename(String),
    BlocklistProfileDelete,
    SessionTemplate(Option<String>),
    SessionTemplateApply(Option<String>),
    SessionTemplateCreate(String),
    SessionTemplateRename(String),
    SessionTemplateDelete,
    BlocklistSites,
    AllowlistSites,
    BlocklistSiteAdd(String),
    AllowlistSiteAdd(String),
    AllowlistSiteAddTemporary(String),
    BlocklistSiteEdit(SiteEditValue),
    AllowlistSiteEdit(SiteEditValue),
    BlocklistSiteDelete(String),
    AllowlistSiteDelete(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ParsedToken {
    Help,
    Json,
    Start,
    Pause,
    Resume,
    Stop,
    Next,
    Task(String),
    TaskGoal {
        label: Option<String>,
        goal: Option<DailyGoalConfig>,
    },
    FocusIntention(Option<String>),
    TaskNote(Option<String>),
    Status,
    Watch(Option<u64>),
    Profile(Option<ProfileId>),
    Theme(Option<ThemePreset>),
    Goal(Option<DailyGoalConfig>),
    GoalWeekly(Option<WeeklyGoalConfig>),
    GoalMonthly(Option<MonthlyGoalConfig>),
    GoalCarry(Option<bool>),
    GoalCarryWeekly(Option<bool>),
    GoalCarryMonthly(Option<bool>),
    Strict(Option<bool>),
    Schedule,
    ScheduleSet(RecurringScheduleConfig),
    WeekdayRules,
    WeekdayRulesSet(Vec<WeekdayProfileRuleConfig>),
    AutomationTriggers,
    AutomationTriggersSet(Vec<AutomationTriggerRuleConfig>),
    ScheduleDelay,
    BreakGlassTrigger,
    BreakGlassCancel,
    Diagnostics,
    BlockingPreview,
    Backup(Option<PathBuf>),
    Restore(Option<PathBuf>),
    Export(Option<PathBuf>),
    BlocklistProfile(Option<String>),
    BlocklistProfileCreate(String),
    BlocklistProfileRename(String),
    BlocklistProfileDelete,
    SessionTemplate(Option<String>),
    SessionTemplateApply(Option<String>),
    SessionTemplateCreate(String),
    SessionTemplateRename(String),
    SessionTemplateDelete,
    BlocklistSites,
    AllowlistSites,
    BlocklistSiteAdd(String),
    AllowlistSiteAdd(String),
    AllowlistSiteAddTemporary(String),
    BlocklistSiteEdit(SiteEditValue),
    AllowlistSiteEdit(SiteEditValue),
    BlocklistSiteDelete(String),
    AllowlistSiteDelete(String),
    UnknownOption(String),
    Positional(String),
}

type KeyValueParser = fn(&str) -> Result<Option<ParsedToken>, String>;
type ValueArgParser = fn(&[String], usize) -> Result<(ParsedToken, usize), String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SiteListTarget {
    Blocklist,
    Allowlist,
}

impl SiteListTarget {
    fn id(self) -> &'static str {
        match self {
            Self::Blocklist => "blocklist",
            Self::Allowlist => "allowlist",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteEditValue {
    previous: String,
    next: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlocklistProfileCommandKind {
    Select { profile: Option<String> },
    Create { name: String },
    Rename { name: String },
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlocklistSiteCommandKind {
    List,
    Add { input: String },
    Edit { value: SiteEditValue },
    Delete { site: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionTemplateCommandKind {
    Select { name: Option<String> },
    Apply { name: Option<String> },
    Create { name: String },
    Rename { name: String },
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct ProfileSpec {
    focus_secs: u64,
    short_break_secs: u64,
    long_break_secs: u64,
    long_break_interval: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ProfileView {
    id: &'static str,
    label: &'static str,
    focus_secs: u64,
    short_break_secs: u64,
    long_break_secs: u64,
    long_break_interval: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct BreakTemplateView {
    name: String,
    short_break_secs: u64,
    long_break_secs: u64,
    long_break_interval: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ThemePresetView {
    id: &'static str,
    label: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ProfileOutput {
    updated: bool,
    selected: ProfileView,
    available: Vec<ProfileView>,
    selected_break_template: BreakTemplateView,
    available_break_templates: Vec<BreakTemplateView>,
    selected_theme_preset: ThemePresetView,
    available_theme_presets: Vec<ThemePresetView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct GoalOutput {
    configured: bool,
    minutes_target: u64,
    pomodoros_target: u32,
    met: bool,
    carry_over: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TaskGoalOutput {
    task_label: String,
    configured: bool,
    minutes_target: u64,
    pomodoros_target: u32,
    focused_minutes: u64,
    pomodoros_completed: u32,
    met: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SessionOutput {
    focused_minutes: u64,
    pomodoros_completed: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TodayOutput {
    focused_minutes: u64,
    pomodoros_completed: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct FocusScoreOutput {
    available: bool,
    focus_score_pct: Option<u8>,
    consistency_score_pct: u8,
    completion_score_pct: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct LiveStatusOutput {
    state_source: &'static str,
    recovery_error: Option<String>,
    in_progress: bool,
    phase: &'static str,
    status: &'static str,
    remaining_secs: u64,
    pomodoros_completed: u32,
    selected_profile: ProfileView,
    selected_task_label: Option<String>,
    focus_intention: Option<String>,
    task_note: Option<String>,
    strict_mode_enforced: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TemporaryAllowlistStatusOutput {
    site: String,
    remaining_secs: u64,
    expires_at_epoch_secs: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct StatusOutput {
    day: String,
    selected_profile: ProfileView,
    selected_break_template: BreakTemplateView,
    available_break_templates: Vec<BreakTemplateView>,
    selected_theme_preset: ThemePresetView,
    selected_task_label: Option<String>,
    focus_intention: Option<String>,
    task_note: Option<String>,
    selected_blocklist_profile: String,
    blocked_sites_count: usize,
    temporary_allowlist_active_count: usize,
    temporary_allowlist_active: Vec<TemporaryAllowlistStatusOutput>,
    strict_mode: bool,
    goal: GoalOutput,
    weekly_goal: GoalOutput,
    monthly_goal: GoalOutput,
    selected_task_goal: Option<TaskGoalOutput>,
    session: SessionOutput,
    today: TodayOutput,
    latest_interruption: Option<SessionInterruptionEvent>,
    focus_score: FocusScoreOutput,
    stats_growth: StatsGrowthSummary,
    stats_retention: StatsRetentionStatusOutput,
    live: LiveStatusOutput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct StatsRetentionStatusOutput {
    preset: &'static str,
    keep_daily_days: Option<u16>,
    keep_focus_sessions_days: Option<u16>,
    keep_session_interruptions_days: Option<u16>,
    keep_break_glass_overrides_days: Option<u16>,
    pending_prune: StatsRetentionPruneResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ExportOutput {
    export_dir: PathBuf,
    json_path: PathBuf,
    csv_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct BackupOutput {
    backup_dir: PathBuf,
    config_backup_path: PathBuf,
    stats_backup_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RestoreOutput {
    restore_dir: PathBuf,
    config_restored_path: PathBuf,
    stats_restored_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TimerStateOutput {
    phase: &'static str,
    status: &'static str,
    remaining_secs: u64,
    pomodoros_completed: u32,
    selected_profile: ProfileView,
    selected_task_label: Option<String>,
    focus_intention: Option<String>,
    task_note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TimerCommandOutput {
    action: &'static str,
    timer: TimerStateOutput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ScheduleDelayCommandOutput {
    action: &'static str,
    delayed_until: String,
    timer: TimerStateOutput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct BreakGlassCommandOutput {
    action: &'static str,
    pending_confirmation: bool,
    active: bool,
    remaining_secs: Option<u64>,
    timer: TimerStateOutput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TaskCommandOutput {
    action: &'static str,
    created: bool,
    selected_task_label: String,
    timer: TimerStateOutput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TaskGoalCommandOutput {
    updated: bool,
    task_label: String,
    configured: bool,
    minutes_target: u64,
    pomodoros_target: u32,
    focused_minutes: u64,
    pomodoros_completed: u32,
    met: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SessionMetadataCommandOutput {
    action: &'static str,
    updated: bool,
    focus_intention: Option<String>,
    task_note: Option<String>,
    timer: TimerStateOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct GoalCommandOutput {
    updated: bool,
    configured: bool,
    minutes_target: u64,
    pomodoros_target: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct GoalCarryCommandOutput {
    updated: bool,
    carry_over: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct StrictCommandOutput {
    updated: bool,
    strict_mode: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ThemeCommandOutput {
    updated: bool,
    selected_theme_preset: ThemePresetView,
    available_theme_presets: Vec<ThemePresetView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ScheduleCommandOutput {
    updated: bool,
    schedule: RecurringScheduleConfig,
    inspection: ScheduleInspectionOutput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ScheduleInspectionOutput {
    conflict_count: usize,
    conflicts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct WeekdayRulesCommandOutput {
    updated: bool,
    rules: Vec<WeekdayProfileRuleConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct AutomationTriggersCommandOutput {
    updated: bool,
    rules: Vec<AutomationTriggerRuleConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SetupCheckOutput {
    level: &'static str,
    message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct DiagnosticsCommandOutput {
    hosts_file_path: String,
    backend_policy: String,
    backend_order: String,
    backend_selection: SetupCheckOutput,
    command_backend: SetupCheckOutput,
    blocking_permissions: SetupCheckOutput,
    hosts_write_capability: SetupCheckOutput,
    wakatime_config: SetupCheckOutput,
    deprecation_warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct BlockingPreviewCommandOutput {
    backend: &'static str,
    backend_target: String,
    attempted_backends: Vec<&'static str>,
    fallback_used: bool,
    hosts_file_path: String,
    action: &'static str,
    would_change: bool,
    effective_blocked_sites_count: usize,
    effective_blocked_sites: Vec<String>,
    section: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct BlocklistProfileSummaryOutput {
    name: String,
    active: bool,
    blocklist_sites_count: usize,
    allowlist_sites_count: usize,
    effective_blocked_sites_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct BlocklistProfileCommandOutput {
    action: &'static str,
    updated: bool,
    selected_blocklist_profile: String,
    profiles: Vec<BlocklistProfileSummaryOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SessionTemplateSummaryOutput {
    name: String,
    active: bool,
    task_label: String,
    profile: &'static str,
    blocklist_profile: String,
    schedule_windows_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SessionTemplateCommandOutput {
    action: &'static str,
    updated: bool,
    selected_session_template: Option<String>,
    templates: Vec<SessionTemplateSummaryOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SiteListCommandOutput {
    action: &'static str,
    profile: String,
    target: SiteListTarget,
    sites: Vec<String>,
    effective_blocked_sites_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct InvalidSiteEntryOutput {
    input: String,
    reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SiteAddCommandOutput {
    action: &'static str,
    updated: bool,
    profile: String,
    target: SiteListTarget,
    added: Vec<String>,
    duplicates: Vec<String>,
    invalid: Vec<InvalidSiteEntryOutput>,
    sites: Vec<String>,
    effective_blocked_sites_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TemporarySiteAddCommandOutput {
    action: &'static str,
    updated: bool,
    profile: String,
    added: usize,
    refreshed: usize,
    active: Vec<TemporaryAllowlistStatusOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SiteEditCommandOutput {
    action: &'static str,
    updated: bool,
    profile: String,
    target: SiteListTarget,
    previous: String,
    current: String,
    sites: Vec<String>,
    effective_blocked_sites_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SiteDeleteCommandOutput {
    action: &'static str,
    updated: bool,
    profile: String,
    target: SiteListTarget,
    removed: String,
    sites: Vec<String>,
    effective_blocked_sites_count: usize,
}

pub fn usage_text() -> &'static str {
    USAGE_TEXT
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn parse_args<I>(args: I) -> Result<CliAction, String>
where
    I: IntoIterator<Item = OsString>,
{
    parse_args_with_contract(args).map_err(|error| error.message)
}

pub fn parse_args_with_contract<I>(args: I) -> Result<CliAction, CliError>
where
    I: IntoIterator<Item = OsString>,
{
    let raw_args: Vec<OsString> = args.into_iter().collect();
    let output_hint = infer_output_mode_from_os_args(&raw_args);
    let args: Vec<String> = raw_args
        .into_iter()
        .map(|arg| {
            arg.into_string().map_err(|_| {
                usage_error(output_hint, invalid_usage("Arguments must be valid UTF-8."))
            })
        })
        .collect::<Result<_, _>>()?;
    let tokens = classify_args(&args).map_err(|message| usage_error(output_hint, message))?;
    let (show_help, output) =
        parse_global_tokens(&tokens).map_err(|message| usage_error(output_hint, message))?;
    let primary = parse_primary_command(&tokens).map_err(|message| usage_error(output, message))?;
    let watch_interval_secs =
        parse_watch_interval_option(&tokens).map_err(|message| usage_error(output, message))?;
    finalize_cli_action(show_help, output, primary, watch_interval_secs)
        .map_err(|message| usage_error(output, message))
}

pub fn runtime_error(output: OutputMode, message: String) -> CliError {
    CliError {
        kind: CliErrorKind::Runtime,
        output,
        message,
    }
}

pub fn emit_cli_error(error: &CliError) -> Result<(), String> {
    match error.output {
        OutputMode::Text => {
            eprintln!("{}", error.message);
            Ok(())
        }
        OutputMode::Json => print_json(&CliErrorEnvelope {
            ok: false,
            error: CliErrorPayload {
                kind: error.kind,
                exit_code: error.exit_code(),
                message: error.message.clone(),
            },
        }),
    }
}

fn usage_error(output: OutputMode, message: String) -> CliError {
    CliError {
        kind: CliErrorKind::Usage,
        output,
        message,
    }
}

pub fn execute_command(cli_command: CliCommand) -> Result<(), String> {
    execute_cli_command(cli_command)
}

#[cfg(test)]
mod tests;
