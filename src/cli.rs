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
    AppConfig, BlocklistProfileConfig, BreakTemplateConfig, CustomProfileConfig, DailyGoalConfig,
    MonthlyGoalConfig, OneTimeFocusWindowConfig, ProfileId, RecurringFocusWindowConfig,
    RecurringScheduleConfig, WeeklyGoalConfig,
};
use crate::schedule::{format_schedule_conflict, inspect_schedule_conflicts_from_config};
use crate::session_recovery;
use crate::stats::{
    DailyGoalSnapshot, FocusStats, SessionInterruptionEvent, carry_over_goal_target,
    current_day_key,
};
use crate::timer::{
    DEFAULT_FOCUS_SECS, DEFAULT_LONG_BREAK_INTERVAL, DEFAULT_LONG_BREAK_SECS,
    DEFAULT_SHORT_BREAK_SECS, TimerPhase, TimerStatus,
};

mod args;
mod execute;
mod output;
mod parsing;
use args::*;
use execute::execute_cli_command;
#[cfg(test)]
use execute::{
    apply_blocklist_profile_command, apply_site_add_command, apply_site_delete_command,
    apply_site_edit_command,
};
use output::*;
use parsing::*;

const USAGE_TEXT: &str = r#"Usage:
  focustime
  focustime --start
  focustime --pause [--json]
  focustime --resume [--json]
  focustime --stop [--json]
  focustime --next [--json]
  focustime --task=LABEL [--json]
  focustime --task-goal [LABEL|LABEL:MINUTES,POMODOROS] [--json]
  focustime --task-goal=LABEL[:MINUTES,POMODOROS] [--json]
  focustime --profile [classic|deep-work|custom] [--json]
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
  focustime --blocklist-profile [PROFILE_NAME] [--json]
  focustime --blocklist-profile-create=PROFILE_NAME [--json]
  focustime --blocklist-profile-rename=PROFILE_NAME [--json]
  focustime --blocklist-profile-delete [--json]
  focustime --blocklist-sites [--json]
  focustime --allowlist-sites [--json]
  focustime --blocklist-site-add=HOSTNAMES [--json]
  focustime --allowlist-site-add=HOSTNAMES [--json]
  focustime --blocklist-site-edit=OLD=NEW [--json]
  focustime --allowlist-site-edit=OLD=NEW [--json]
  focustime --blocklist-site-delete=HOSTNAME [--json]
  focustime --allowlist-site-delete=HOSTNAME [--json]
  focustime --diagnostics [--json]
  focustime --blocking-preview [--json]
  focustime --status [--watch[=SECONDS]] [--json]
  focustime --export[=DIR] [--json]

Options:
  --start         Launch TUI with focus timer already started
  --pause         Pause a running timer
  --resume        Resume a paused timer
  --stop          Stop/reset the current phase
  --next          Skip to the next phase
  --task          Select task label (auto-creates unknown labels)
  --task-goal     Show or set per-task cumulative goal targets
  --profile       Show current profile, or set it when value is provided
  --goal          Show current daily goal, or set minutes/pomodoros targets
  --goal-weekly   Show current weekly goal, or set minutes/pomodoros targets
  --goal-monthly  Show current monthly goal, or set minutes/pomodoros targets
  --goal-carry          Show daily goal carry-over, or set on/off
  --goal-carry-weekly   Show weekly goal carry-over, or set on/off
  --goal-carry-monthly  Show monthly goal carry-over, or set on/off
  --strict        Show strict mode for selected profile, or set on/off
  --schedule      Show selected profile schedule with overlap/conflict inspection
  --schedule-set  Replace selected profile schedule (recurring + one-time) from JSON payload
  --blocklist-profile         Show active blocklist profile, or set active profile
  --blocklist-profile-create  Create a blocklist profile and select it
  --blocklist-profile-rename  Rename the active blocklist profile
  --blocklist-profile-delete  Delete the active blocklist profile
  --blocklist-sites           List blocklist sites in active profile
  --allowlist-sites           List allowlist sites in active profile
  --blocklist-site-add        Add/import blocklist hostnames in active profile
  --allowlist-site-add        Add/import allowlist hostnames in active profile
  --blocklist-site-edit       Replace blocklist hostname using OLD=NEW
  --allowlist-site-edit       Replace allowlist hostname using OLD=NEW
  --blocklist-site-delete     Delete blocklist hostname in active profile
  --allowlist-site-delete     Delete allowlist hostname in active profile
  --diagnostics   Show setup diagnostics checks
  --blocking-preview  Preview focustime hosts-section changes without writing
  --status        Print status summary (includes live timer/session fields and latest interruption)
  --watch         Stream periodic status updates (status command only; default 1s)
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
    Diagnostics,
    BlockingPreview,
    Status {
        watch_interval_secs: Option<u64>,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliCommand {
    pub kind: CommandKind,
    pub output: OutputMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliAction {
    RunTui { start_immediately: bool },
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
    Goal(Option<DailyGoalConfig>),
    GoalWeekly(Option<WeeklyGoalConfig>),
    GoalMonthly(Option<MonthlyGoalConfig>),
    GoalCarry(Option<bool>),
    GoalCarryWeekly(Option<bool>),
    GoalCarryMonthly(Option<bool>),
    Strict(Option<bool>),
    Schedule,
    ScheduleSet(RecurringScheduleConfig),
    Diagnostics,
    BlockingPreview,
    Status,
    Export(Option<PathBuf>),
    BlocklistProfile(Option<String>),
    BlocklistProfileCreate(String),
    BlocklistProfileRename(String),
    BlocklistProfileDelete,
    BlocklistSites,
    AllowlistSites,
    BlocklistSiteAdd(String),
    AllowlistSiteAdd(String),
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
    Goal(Option<DailyGoalConfig>),
    GoalWeekly(Option<WeeklyGoalConfig>),
    GoalMonthly(Option<MonthlyGoalConfig>),
    GoalCarry(Option<bool>),
    GoalCarryWeekly(Option<bool>),
    GoalCarryMonthly(Option<bool>),
    Strict(Option<bool>),
    Schedule,
    ScheduleSet(RecurringScheduleConfig),
    Diagnostics,
    BlockingPreview,
    Export(Option<PathBuf>),
    BlocklistProfile(Option<String>),
    BlocklistProfileCreate(String),
    BlocklistProfileRename(String),
    BlocklistProfileDelete,
    BlocklistSites,
    AllowlistSites,
    BlocklistSiteAdd(String),
    AllowlistSiteAdd(String),
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
struct ProfileOutput {
    updated: bool,
    selected: ProfileView,
    available: Vec<ProfileView>,
    selected_break_template: BreakTemplateView,
    available_break_templates: Vec<BreakTemplateView>,
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
struct StatusOutput {
    day: String,
    selected_profile: ProfileView,
    selected_break_template: BreakTemplateView,
    available_break_templates: Vec<BreakTemplateView>,
    selected_task_label: Option<String>,
    focus_intention: Option<String>,
    task_note: Option<String>,
    selected_blocklist_profile: String,
    blocked_sites_count: usize,
    strict_mode: bool,
    goal: GoalOutput,
    weekly_goal: GoalOutput,
    monthly_goal: GoalOutput,
    selected_task_goal: Option<TaskGoalOutput>,
    session: SessionOutput,
    today: TodayOutput,
    latest_interruption: Option<SessionInterruptionEvent>,
    focus_score: FocusScoreOutput,
    live: LiveStatusOutput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ExportOutput {
    export_dir: PathBuf,
    json_path: PathBuf,
    csv_path: PathBuf,
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
struct DiagnosticsCommandOutput {
    hosts_file_path: String,
    blocking_permissions: SetupCheckOutput,
    hosts_write_capability: SetupCheckOutput,
    wakatime_config: SetupCheckOutput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct BlockingPreviewCommandOutput {
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

fn build_status_output(config: &AppConfig, stats: &FocusStats) -> StatusOutput {
    let day = current_day_key();
    let day_date = NaiveDate::parse_from_str(&day, "%Y-%m-%d")
        .expect("current_day_key should always be a valid ISO date");
    let today = stats.daily_for(&day);
    let week = stats.weekly_for_day(day_date);
    let month = stats.monthly_for_day(day_date);
    let session = stats.session();
    let (_, selected_task_label) = stats.task_planner_state();
    let (selected_task_label, focus_intention, task_note) =
        mirror_metadata_from_task_label(selected_task_label);
    let goal_snapshot = effective_daily_goal_snapshot_for_day(config, stats, day_date);
    let weekly_goal_snapshot = effective_weekly_goal_snapshot(config, stats, day_date);
    let monthly_goal_snapshot = effective_monthly_goal_snapshot(config, stats, day_date);
    let selected_task_goal = selected_task_label
        .as_ref()
        .map(|label| build_task_goal_output(stats, label));
    let active_sites_count = config
        .blocklist_profiles
        .iter()
        .find(|profile| {
            profile
                .name
                .eq_ignore_ascii_case(&config.selected_blocklist_profile)
        })
        .map(|profile| effective_blocked_sites_for_profile(profile).len())
        .unwrap_or_default();
    let live = build_live_status_output(config, selected_task_label.clone());
    let latest_interruption = stats.latest_session_interruption();
    let consistency_score_pct = stats
        .weekly_focus_score_for_day(day_date)
        .consistency_score_pct;
    let completion_score_pct = if weekly_goal_snapshot.has_any_target() {
        weekly_goal_completion_score_pct(
            weekly_goal_snapshot,
            week.focused_minutes(),
            week.pomodoros_completed,
        )
    } else {
        None
    };
    let focus_score_pct = completion_score_pct.map(|completion| {
        (u16::from(consistency_score_pct) + u16::from(completion)).div_ceil(2) as u8
    });

    StatusOutput {
        day,
        selected_profile: profile_view(config.selected_profile, &config.effective_custom_profile()),
        selected_break_template: selected_break_template_view(config),
        available_break_templates: available_break_template_views(config),
        selected_task_label,
        focus_intention,
        task_note,
        selected_blocklist_profile: config.selected_blocklist_profile.clone(),
        blocked_sites_count: active_sites_count,
        strict_mode: config.strict_mode,
        goal: GoalOutput {
            configured: goal_snapshot.has_any_target(),
            minutes_target: goal_snapshot.minutes,
            pomodoros_target: goal_snapshot.pomodoros,
            met: goal_snapshot.is_met_by(today),
            carry_over: config.goal_carry_over.daily,
        },
        weekly_goal: GoalOutput {
            configured: weekly_goal_snapshot.has_any_target(),
            minutes_target: weekly_goal_snapshot.minutes,
            pomodoros_target: weekly_goal_snapshot.pomodoros,
            met: weekly_goal_snapshot
                .is_met_by_totals(week.focused_minutes(), week.pomodoros_completed),
            carry_over: config.goal_carry_over.weekly,
        },
        monthly_goal: GoalOutput {
            configured: monthly_goal_snapshot.has_any_target(),
            minutes_target: monthly_goal_snapshot.minutes,
            pomodoros_target: monthly_goal_snapshot.pomodoros,
            met: monthly_goal_snapshot
                .is_met_by_totals(month.focused_minutes(), month.pomodoros_completed),
            carry_over: config.goal_carry_over.monthly,
        },
        selected_task_goal,
        session: SessionOutput {
            focused_minutes: session.focused_minutes(),
            pomodoros_completed: session.pomodoros_completed,
        },
        today: TodayOutput {
            focused_minutes: today.focused_minutes(),
            pomodoros_completed: today.pomodoros_completed,
        },
        latest_interruption,
        focus_score: FocusScoreOutput {
            available: focus_score_pct.is_some(),
            focus_score_pct,
            consistency_score_pct,
            completion_score_pct,
        },
        live,
    }
}

fn weekly_goal_completion_score_pct(
    goal: DailyGoalSnapshot,
    focused_minutes: u64,
    pomodoros_completed: u32,
) -> Option<u8> {
    let minute_score = if goal.minutes > 0 {
        Some(percentage_round_nearest(
            focused_minutes.min(goal.minutes),
            goal.minutes,
        ))
    } else {
        None
    };
    let pomodoro_score = if goal.pomodoros > 0 {
        Some(percentage_round_nearest(
            u64::from(pomodoros_completed.min(goal.pomodoros)),
            u64::from(goal.pomodoros),
        ))
    } else {
        None
    };
    match (minute_score, pomodoro_score) {
        (None, None) => None,
        (Some(score), None) | (None, Some(score)) => Some(score),
        (Some(left), Some(right)) => Some((u16::from(left) + u16::from(right)).div_ceil(2) as u8),
    }
}

fn percentage_round_nearest(part: u64, total: u64) -> u8 {
    if total == 0 {
        return 0;
    }
    let rounded = (u128::from(part) * 100 + (u128::from(total) / 2)) / u128::from(total);
    rounded.min(u128::from(u8::MAX)) as u8
}

fn effective_daily_goal_snapshot_for_day(
    config: &AppConfig,
    stats: &FocusStats,
    day: NaiveDate,
) -> DailyGoalSnapshot {
    let day_key = day.format("%Y-%m-%d").to_string();
    let base = stats
        .daily_entry(&day_key)
        .and_then(|daily| daily.goal)
        .unwrap_or(DailyGoalSnapshot {
            minutes: config.daily_goal.minutes,
            pomodoros: config.daily_goal.pomodoros,
        });
    let previous = day.pred_opt().and_then(|previous_day| {
        let day_key = previous_day.format("%Y-%m-%d").to_string();
        stats.daily_entry(&day_key).and_then(|daily| {
            daily
                .goal
                .map(|goal| (goal, daily.focused_minutes(), daily.pomodoros_completed))
        })
    });
    carry_over_goal_target(base, config.goal_carry_over.daily, previous)
}

fn build_task_goal_output(stats: &FocusStats, label: &str) -> TaskGoalOutput {
    match stats.task_goal_progress_for_label(label) {
        Some(progress) => {
            let focused_minutes = progress.focused_minutes();
            TaskGoalOutput {
                task_label: progress.task_label,
                configured: progress.target.has_any_target(),
                minutes_target: progress.target.minutes,
                pomodoros_target: progress.target.pomodoros,
                focused_minutes,
                pomodoros_completed: progress.pomodoros_completed,
                met: progress.met,
            }
        }
        None => TaskGoalOutput {
            task_label: label.to_string(),
            configured: false,
            minutes_target: 0,
            pomodoros_target: 0,
            focused_minutes: 0,
            pomodoros_completed: 0,
            met: false,
        },
    }
}

fn effective_weekly_goal_snapshot(
    config: &AppConfig,
    stats: &FocusStats,
    day: NaiveDate,
) -> DailyGoalSnapshot {
    let base = DailyGoalSnapshot {
        minutes: config.weekly_goal.minutes,
        pomodoros: config.weekly_goal.pomodoros,
    };
    let previous =
        day.checked_sub_signed(chrono::Duration::weeks(1))
            .and_then(|previous_week_day| {
                stats
                    .weekly_goal_snapshot_for_day(previous_week_day)
                    .map(|previous_target| {
                        let week = stats.weekly_for_day(previous_week_day);
                        (
                            previous_target,
                            week.focused_minutes(),
                            week.pomodoros_completed,
                        )
                    })
            });
    carry_over_goal_target(base, config.goal_carry_over.weekly, previous)
}

fn effective_monthly_goal_snapshot(
    config: &AppConfig,
    stats: &FocusStats,
    day: NaiveDate,
) -> DailyGoalSnapshot {
    let base = DailyGoalSnapshot {
        minutes: config.monthly_goal.minutes,
        pomodoros: config.monthly_goal.pomodoros,
    };
    let previous = previous_month_reference_day(day).and_then(|previous_month_day| {
        stats
            .monthly_goal_snapshot_for_day(previous_month_day)
            .map(|previous_target| {
                let month = stats.monthly_for_day(previous_month_day);
                (
                    previous_target,
                    month.focused_minutes(),
                    month.pomodoros_completed,
                )
            })
    });
    carry_over_goal_target(base, config.goal_carry_over.monthly, previous)
}

fn previous_month_reference_day(day: NaiveDate) -> Option<NaiveDate> {
    let month_start = NaiveDate::from_ymd_opt(day.year(), day.month(), 1)?;
    month_start.pred_opt()
}

fn build_live_status_output(
    config: &AppConfig,
    fallback_task_label: Option<String>,
) -> LiveStatusOutput {
    let custom = config.effective_custom_profile();
    let (fallback_task_label, fallback_focus_intention, fallback_task_note) =
        mirror_metadata_from_task_label(fallback_task_label);
    match session_recovery::load() {
        Ok(Some(snapshot)) => {
            let phase = snapshot.phase();
            let status = snapshot.status();
            LiveStatusOutput {
                state_source: "recovery",
                recovery_error: None,
                in_progress: true,
                phase: timer_phase_id(phase),
                status: timer_status_id(status),
                remaining_secs: snapshot.remaining_secs,
                pomodoros_completed: snapshot.pomodoros_completed,
                selected_profile: profile_view(snapshot.selected_profile, &custom),
                selected_task_label: snapshot.normalized_task_label(),
                focus_intention: snapshot.normalized_focus_intention(),
                task_note: snapshot.normalized_task_note(),
                strict_mode_enforced: config.strict_mode
                    && phase == TimerPhase::Focus
                    && status != TimerStatus::Idle,
            }
        }
        Ok(None) => {
            let selected_profile = profile_view(config.selected_profile, &custom);
            LiveStatusOutput {
                state_source: "default",
                recovery_error: None,
                in_progress: false,
                phase: timer_phase_id(TimerPhase::Focus),
                status: timer_status_id(TimerStatus::Idle),
                remaining_secs: selected_profile.focus_secs,
                pomodoros_completed: 0,
                selected_profile,
                selected_task_label: fallback_task_label.clone(),
                focus_intention: fallback_focus_intention.clone(),
                task_note: fallback_task_note.clone(),
                strict_mode_enforced: false,
            }
        }
        Err(error) => {
            let selected_profile = profile_view(config.selected_profile, &custom);
            LiveStatusOutput {
                state_source: "default",
                recovery_error: Some(error),
                in_progress: false,
                phase: timer_phase_id(TimerPhase::Focus),
                status: timer_status_id(TimerStatus::Idle),
                remaining_secs: selected_profile.focus_secs,
                pomodoros_completed: 0,
                selected_profile,
                selected_task_label: fallback_task_label,
                focus_intention: fallback_focus_intention,
                task_note: fallback_task_note,
                strict_mode_enforced: false,
            }
        }
    }
}

fn mirror_metadata_from_task_label(
    task_label: Option<String>,
) -> (Option<String>, Option<String>, Option<String>) {
    let task_label = task_label
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let focus_intention = task_label.clone();
    let task_note = task_label.clone();
    (task_label, focus_intention, task_note)
}

fn profile_view(profile: ProfileId, custom: &CustomProfileConfig) -> ProfileView {
    let spec = resolve_profile_spec(profile, custom);
    ProfileView {
        id: profile_id(profile),
        label: profile.label(),
        focus_secs: spec.focus_secs,
        short_break_secs: spec.short_break_secs,
        long_break_secs: spec.long_break_secs,
        long_break_interval: spec.long_break_interval,
    }
}

fn resolve_profile_spec(profile: ProfileId, custom: &CustomProfileConfig) -> ProfileSpec {
    match profile {
        ProfileId::Classic => ProfileSpec {
            focus_secs: DEFAULT_FOCUS_SECS,
            short_break_secs: DEFAULT_SHORT_BREAK_SECS,
            long_break_secs: DEFAULT_LONG_BREAK_SECS,
            long_break_interval: DEFAULT_LONG_BREAK_INTERVAL,
        },
        ProfileId::DeepWork => ProfileSpec {
            focus_secs: 50 * 60,
            short_break_secs: 10 * 60,
            long_break_secs: 30 * 60,
            long_break_interval: 3,
        },
        ProfileId::Custom => {
            let custom = custom.normalized();
            ProfileSpec {
                focus_secs: custom.focus_secs,
                short_break_secs: custom.short_break_secs,
                long_break_secs: custom.long_break_secs,
                long_break_interval: custom.long_break_interval,
            }
        }
    }
}

fn profile_id(profile: ProfileId) -> &'static str {
    match profile {
        ProfileId::Classic => "classic",
        ProfileId::DeepWork => "deep-work",
        ProfileId::Custom => "custom",
    }
}

fn break_template_view(template: &BreakTemplateConfig) -> BreakTemplateView {
    let template = template.normalized();
    BreakTemplateView {
        name: template.name,
        short_break_secs: template.short_break_secs,
        long_break_secs: template.long_break_secs,
        long_break_interval: template.long_break_interval,
    }
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

fn selected_break_template_index(config: &AppConfig) -> Option<usize> {
    let custom_profile = config.effective_custom_profile();
    let selected_name = config.selected_break_template.trim();
    let selected_index = config
        .break_templates
        .iter()
        .position(|template| template.name.eq_ignore_ascii_case(selected_name));

    if let Some(index) = selected_index {
        if config.break_templates.get(index).is_some_and(|template| {
            break_template_matches_custom_profile(template, &custom_profile)
        }) {
            return Some(index);
        }
    }

    config
        .break_templates
        .iter()
        .position(|template| break_template_matches_custom_profile(template, &custom_profile))
}

fn selected_break_template_view(config: &AppConfig) -> BreakTemplateView {
    if let Some(index) = selected_break_template_index(config) {
        return config
            .break_templates
            .get(index)
            .map(break_template_view)
            .unwrap_or_else(|| break_template_view(&BreakTemplateConfig::default()));
    }

    let custom = config.effective_custom_profile();
    BreakTemplateView {
        name: "Custom".to_string(),
        short_break_secs: custom.short_break_secs,
        long_break_secs: custom.long_break_secs,
        long_break_interval: custom.long_break_interval,
    }
}

fn available_break_template_views(config: &AppConfig) -> Vec<BreakTemplateView> {
    config
        .break_templates
        .iter()
        .map(break_template_view)
        .collect()
}

fn timer_phase_id(phase: TimerPhase) -> &'static str {
    match phase {
        TimerPhase::Focus => "focus",
        TimerPhase::ShortBreak => "short-break",
        TimerPhase::LongBreak => "long-break",
    }
}

fn timer_status_id(status: TimerStatus) -> &'static str {
    match status {
        TimerStatus::Idle => "idle",
        TimerStatus::Running => "running",
        TimerStatus::Paused => "paused",
    }
}

#[cfg(test)]
mod tests;
