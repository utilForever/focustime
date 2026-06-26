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
#[cfg(test)]
use crate::config::HistoryKpiCardId;
use crate::config::{
    AppConfig, BlocklistProfileConfig, ConfigDoctorReport, ConfigMigrationReport,
    CustomProfileConfig, DailyGoalConfig, MonthlyGoalConfig, ProfileId, RecurringFocusWindowConfig,
    RecurringScheduleConfig, ThemePreset, WeeklyGoalConfig,
};
use crate::error::UserMessage;
use crate::schedule::{format_schedule_conflict, inspect_schedule_conflicts_from_config};
use crate::session_recovery;
use crate::stats::{
    DailyGoalSnapshot, FocusRiskForecast, FocusStats, SessionInterruptionEvent, StatsGrowthSummary,
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
    apply_blocklist_profile_command, apply_history_dashboard_command, apply_site_add_command,
    apply_site_delete_command, apply_site_edit_command,
};
use output::{
    build_diagnostics_blocking_preview_error, build_diagnostics_blocking_preview_output,
    build_diagnostics_command_output, build_schedule_inspection_output, display_input_value,
    effective_blocked_sites_for_profile, flush_stdout, print_backup_output,
    print_blocklist_profile_command_output, print_break_glass_command_output,
    print_diagnostics_command_output, print_export_output, print_goal_carry_command_output,
    print_goal_command_output, print_history_dashboard_command_output, print_json,
    print_json_compact, print_profile_output, print_restore_output, print_schedule_command_output,
    print_site_add_command_output, print_site_delete_command_output,
    print_site_edit_command_output, print_site_list_command_output, print_status_output,
    print_strict_command_output, print_task_goal_command_output,
    print_temporary_site_add_command_output, print_theme_command_output, print_timer_state_output,
};
use parsing::{
    finalize_cli_action, first_removed_option_guidance, invalid_usage, parse_global_tokens,
    parse_goal_carry_value, parse_goal_value, parse_monthly_goal_value, parse_primary_command,
    parse_profile_id, parse_schedule_value, parse_site_edit_value, parse_strict_value,
    parse_task_goal_value, parse_theme_preset, parse_watch_interval_option,
    parse_watch_interval_secs, parse_weekly_goal_value, require_nonempty_key_value,
};
use status::{
    available_theme_preset_views, build_status_output, build_task_goal_output, profile_id,
    profile_view, theme_preset_view, timer_phase_id, timer_status_id,
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
  focustime --profile [basic|standard|advanced] [--json]
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
  focustime --break-glass-trigger [--json]
  focustime --break-glass-cancel [--json]
  focustime --blocklist-profile [PROFILE_NAME] [--json]
  focustime --blocklist-profile-create=PROFILE_NAME [--json]
  focustime --blocklist-profile-rename=PROFILE_NAME [--json]
  focustime --blocklist-profile-delete [--json]
  focustime --history-dashboard [--json]
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
  --schedule-set  Replace selected profile schedule from JSON payload
  --break-glass-trigger  Trigger temporary override workflow for break-glass (first call arms, second confirms)
  --break-glass-cancel   Cancel a pending break-glass confirmation
  --blocklist-profile         Show active blocklist profile, or set active profile
  --blocklist-profile-create  Create a blocklist profile and select it
  --blocklist-profile-rename  Rename the active blocklist profile
  --blocklist-profile-delete  Delete the active blocklist profile
  --history-dashboard       Show the stable default KPI dashboard layout
  --blocklist-sites           List blocklist sites for the active blocklist profile
  --allowlist-sites           List allowlist sites for the active blocklist profile
  --blocklist-site-add        Add/import blocklist hostnames for the active blocklist profile
  --allowlist-site-add        Add/import allowlist hostnames for the active blocklist profile
  --allowlist-site-add-temporary  Add temporary override host exceptions with inline duration (HOST=30m,HOST=45s)
  --blocklist-site-edit       Replace blocklist hostname for the active blocklist profile using OLD=NEW
  --allowlist-site-edit       Replace allowlist hostname for the active blocklist profile using OLD=NEW
  --blocklist-site-delete     Delete blocklist hostname from the active blocklist profile
  --allowlist-site-delete     Delete allowlist hostname from the active blocklist profile
  --diagnostics   Show setup diagnostics, blocking preview details, config health, and migration guidance
  --status        Print status summary (includes live timer/session fields and latest interruption)
  --watch         Stream periodic status updates (status command only; default 1s)
  --backup        Back up config.toml and stats.toml to current directory or DIR
  --restore       Restore config.toml and stats.toml from current directory or DIR
  --export        Export stats to current directory or DIR

  --json          Emit machine-readable JSON output
  -h, --help      Show this help"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputMode {
    Text,
    Json,
}

const EXIT_CODE_RUNTIME_ERROR: i32 = 1;
const EXIT_CODE_USAGE_ERROR: i32 = 2;
const DEFAULT_WATCH_INTERVAL_SECS: u64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CliErrorKind {
    Usage,
    Runtime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CliError {
    kind: CliErrorKind,
    output: OutputMode,
    code: &'static str,
    message: String,
    hint: Option<String>,
}

impl CliError {
    pub(crate) fn exit_code(&self) -> i32 {
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
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CommandKind {
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
    BreakGlassTrigger,
    BreakGlassCancel,
    Diagnostics,
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
    HistoryDashboard {
        command: HistoryDashboardCommandKind,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CliCommand {
    pub(crate) kind: CommandKind,
    pub(crate) output: OutputMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CliAction {
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
    BreakGlassTrigger,
    BreakGlassCancel,
    Diagnostics,
    Status,
    Backup(Option<PathBuf>),
    Restore(Option<PathBuf>),
    Export(Option<PathBuf>),
    BlocklistProfile(Option<String>),
    BlocklistProfileCreate(String),
    BlocklistProfileRename(String),
    BlocklistProfileDelete,
    HistoryDashboard,
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
    BreakGlassTrigger,
    BreakGlassCancel,
    Diagnostics,
    Backup(Option<PathBuf>),
    Restore(Option<PathBuf>),
    Export(Option<PathBuf>),
    BlocklistProfile(Option<String>),
    BlocklistProfileCreate(String),
    BlocklistProfileRename(String),
    BlocklistProfileDelete,
    HistoryDashboard,
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
pub(crate) enum SiteListTarget {
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
pub(crate) struct SiteEditValue {
    previous: String,
    next: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BlocklistProfileCommandKind {
    Select { profile: Option<String> },
    Create { name: String },
    Rename { name: String },
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BlocklistSiteCommandKind {
    List,
    Add { input: String },
    Edit { value: SiteEditValue },
    Delete { site: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HistoryDashboardCommandKind {
    Show,
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
struct ThemePresetView {
    id: &'static str,
    label: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ProfileOutput {
    updated: bool,
    selected: ProfileView,
    available: Vec<ProfileView>,
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
    strict_mode_enforced: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TemporaryAllowlistStatusOutput {
    site: String,
    remaining_secs: u64,
    expires_at_epoch_secs: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TemporaryOverrideStatusOutput {
    kind: &'static str,
    profile: Option<String>,
    site: Option<String>,
    remaining_secs: Option<u64>,
    expires_at_epoch_secs: Option<i64>,
    pending_confirmation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct WeeklyAllocationDayOutput {
    date: String,
    minutes_target: u64,
    pomodoros_target: u32,
    allocatable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct WeeklyAllocationOutput {
    available: bool,
    uses_schedule_weights: bool,
    remaining_days_in_week: usize,
    allocatable_days: usize,
    completed_minutes: u64,
    completed_pomodoros: u32,
    remaining_minutes: u64,
    remaining_pomodoros: u32,
    today_minutes_target: u64,
    today_pomodoros_target: u32,
    days: Vec<WeeklyAllocationDayOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct StatusOutput {
    day: String,
    selected_profile: ProfileView,
    selected_theme_preset: ThemePresetView,
    selected_task_label: Option<String>,
    selected_blocklist_profile: String,
    blocked_sites_count: usize,
    temporary_overrides_active_count: usize,
    temporary_overrides: Vec<TemporaryOverrideStatusOutput>,
    strict_mode: bool,
    goal: GoalOutput,
    weekly_goal: GoalOutput,
    weekly_allocation: WeeklyAllocationOutput,
    monthly_goal: GoalOutput,
    selected_task_goal: Option<TaskGoalOutput>,
    session: SessionOutput,
    today: TodayOutput,
    latest_interruption: Option<SessionInterruptionEvent>,
    focus_score: FocusScoreOutput,
    focus_risk: FocusRiskForecast,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TimerCommandOutput {
    action: &'static str,
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
struct SetupCheckOutput {
    level: &'static str,
    message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct DiagnosticsSetupOutput {
    hosts_file_path: String,
    backend_policy: String,
    backend_order: String,
    backend_selection: SetupCheckOutput,
    command_backend: SetupCheckOutput,
    blocking_permissions: SetupCheckOutput,
    hosts_write_capability: SetupCheckOutput,
    wakatime_config: SetupCheckOutput,
    wakatime_runtime: SetupCheckOutput,
    deprecation_warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct DiagnosticsCommandOutput {
    action: &'static str,
    setup: DiagnosticsSetupOutput,
    blocking_preview: DiagnosticsBlockingPreviewOutput,
    config_doctor: ConfigDoctorReport,
    config_migration: ConfigMigrationReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct BlockingPreviewOutput {
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
struct DiagnosticsBlockingPreviewOutput {
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    preview: Option<BlockingPreviewOutput>,
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
struct HistoryDashboardCardOutput {
    id: &'static str,
    label: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HistoryDashboardCommandOutput {
    action: &'static str,
    cards: Vec<HistoryDashboardCardOutput>,
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

pub(crate) fn usage_text() -> &'static str {
    USAGE_TEXT
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn parse_args<I>(args: I) -> Result<CliAction, String>
where
    I: IntoIterator<Item = OsString>,
{
    parse_args_with_contract(args).map_err(|error| error.message)
}

pub(crate) fn parse_args_with_contract<I>(args: I) -> Result<CliAction, CliError>
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
    let (show_help, output) = parse_global_tokens(&tokens).map_err(|message| {
        if let Some(guidance) = first_removed_option_guidance(&tokens) {
            usage_error_with_hint(output_hint, message, guidance.replacement)
        } else {
            usage_error(output_hint, message)
        }
    })?;
    let primary = parse_primary_command(&tokens).map_err(|message| usage_error(output, message))?;
    let watch_interval_secs =
        parse_watch_interval_option(&tokens).map_err(|message| usage_error(output, message))?;
    finalize_cli_action(show_help, output, primary, watch_interval_secs)
        .map_err(|message| usage_error(output, message))
}

pub(crate) fn runtime_error(output: OutputMode, message: impl Into<UserMessage>) -> CliError {
    let message = message.into();
    CliError {
        kind: CliErrorKind::Runtime,
        output,
        code: message.code,
        message: message.message,
        hint: message.hint,
    }
}

pub(crate) fn emit_cli_error(error: &CliError) -> Result<(), String> {
    match error.output {
        OutputMode::Text => {
            eprintln!("{}", error.message);
            if let Some(hint) = &error.hint {
                eprintln!("Hint: {hint}");
            }
            Ok(())
        }
        OutputMode::Json => print_json(&CliErrorEnvelope {
            ok: false,
            error: CliErrorPayload {
                kind: error.kind,
                exit_code: error.exit_code(),
                code: error.code,
                message: error.message.clone(),
                hint: error.hint.clone(),
            },
        }),
    }
}

fn usage_error(output: OutputMode, message: String) -> CliError {
    let message = UserMessage::usage(message);
    CliError {
        kind: CliErrorKind::Usage,
        output,
        code: message.code,
        message: message.message,
        hint: message.hint,
    }
}

fn usage_error_with_hint(output: OutputMode, message: String, hint: impl Into<String>) -> CliError {
    let message = UserMessage::with_hint("cli.usage", message, hint);
    CliError {
        kind: CliErrorKind::Usage,
        output,
        code: message.code,
        message: message.message,
        hint: message.hint,
    }
}

pub(crate) fn execute_command(cli_command: CliCommand) -> Result<(), UserMessage> {
    execute_cli_command(cli_command)
}

#[cfg(test)]
mod tests;
