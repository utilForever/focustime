use std::{
    collections::HashSet,
    env,
    ffi::OsString,
    io::{self, Write},
    path::PathBuf,
    thread,
    time::Duration,
};

use chrono::{Datelike, NaiveDate};
use serde::Serialize;

use crate::app::{App, SetupCheck, SetupCheckLevel, SetupDiagnostics};
use crate::blocker::{BlockingPreviewAction, EditSiteResult, InvalidSiteInput, SiteBlocker};
use crate::config::{
    AppConfig, BlocklistProfileConfig, CustomProfileConfig, DailyGoalConfig, MonthlyGoalConfig,
    OneTimeFocusWindowConfig, ProfileId, RecurringFocusWindowConfig, RecurringScheduleConfig,
    WeeklyGoalConfig,
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
struct ProfileOutput {
    updated: bool,
    selected: ProfileView,
    available: Vec<ProfileView>,
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

fn infer_output_mode_from_os_args(args: &[OsString]) -> OutputMode {
    let parsed_args: Vec<String> = args
        .iter()
        .filter_map(|arg| arg.to_str().map(ToString::to_string))
        .collect();
    infer_output_mode_from_args(&parsed_args)
}

fn infer_output_mode_from_args(args: &[String]) -> OutputMode {
    if args.iter().any(|arg| arg == "--json") {
        OutputMode::Json
    } else {
        OutputMode::Text
    }
}

fn classify_args(args: &[String]) -> Result<Vec<ParsedToken>, String> {
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < args.len() {
        let (token, consumed) = classify_arg(args, index)?;
        tokens.push(token);
        index += consumed;
    }
    Ok(tokens)
}

fn classify_arg(args: &[String], index: usize) -> Result<(ParsedToken, usize), String> {
    let arg = &args[index];
    if let Some(token) = classify_simple_flag(arg) {
        return Ok((token, 1));
    }
    if let Some(result) = classify_value_arg(args, index, arg)? {
        return Ok(result);
    }
    if let Some(token) = classify_key_value_arg(arg)? {
        return Ok((token, 1));
    }
    if arg.starts_with('-') {
        return Ok((ParsedToken::UnknownOption(arg.clone()), 1));
    }
    Ok((ParsedToken::Positional(arg.clone()), 1))
}

fn classify_value_arg(
    args: &[String],
    index: usize,
    arg: &str,
) -> Result<Option<(ParsedToken, usize)>, String> {
    let parsers: [(&str, ValueArgParser); 22] = [
        ("--task", classify_task_arg),
        ("--task-goal", classify_task_goal_arg),
        ("--profile", classify_profile_arg),
        ("--goal", classify_goal_arg),
        ("--goal-weekly", classify_goal_weekly_arg),
        ("--goal-monthly", classify_goal_monthly_arg),
        ("--goal-carry", classify_goal_carry_arg),
        ("--goal-carry-weekly", classify_goal_carry_weekly_arg),
        ("--goal-carry-monthly", classify_goal_carry_monthly_arg),
        ("--strict", classify_strict_arg),
        ("--schedule-set", classify_schedule_set_arg),
        ("--watch", classify_watch_arg),
        ("--export", classify_export_arg),
        ("--blocklist-profile", classify_blocklist_profile_arg),
        (
            "--blocklist-profile-create",
            classify_blocklist_profile_create_arg,
        ),
        (
            "--blocklist-profile-rename",
            classify_blocklist_profile_rename_arg,
        ),
        ("--blocklist-site-add", classify_blocklist_site_add_arg),
        ("--allowlist-site-add", classify_allowlist_site_add_arg),
        ("--blocklist-site-edit", classify_blocklist_site_edit_arg),
        ("--allowlist-site-edit", classify_allowlist_site_edit_arg),
        (
            "--blocklist-site-delete",
            classify_blocklist_site_delete_arg,
        ),
        (
            "--allowlist-site-delete",
            classify_allowlist_site_delete_arg,
        ),
    ];

    for (flag, parser) in parsers {
        if arg == flag {
            return parser(args, index).map(Some);
        }
    }

    Ok(None)
}

fn classify_simple_flag(arg: &str) -> Option<ParsedToken> {
    match arg {
        "-h" | "--help" => Some(ParsedToken::Help),
        "--json" => Some(ParsedToken::Json),
        "--start" => Some(ParsedToken::Start),
        "--pause" => Some(ParsedToken::Pause),
        "--resume" => Some(ParsedToken::Resume),
        "--stop" => Some(ParsedToken::Stop),
        "--next" => Some(ParsedToken::Next),
        "--status" => Some(ParsedToken::Status),
        "--schedule" => Some(ParsedToken::Schedule),
        "--diagnostics" => Some(ParsedToken::Diagnostics),
        "--blocking-preview" => Some(ParsedToken::BlockingPreview),
        "--blocklist-profile-delete" => Some(ParsedToken::BlocklistProfileDelete),
        "--blocklist-sites" => Some(ParsedToken::BlocklistSites),
        "--allowlist-sites" => Some(ParsedToken::AllowlistSites),
        _ => None,
    }
}

fn classify_task_arg(args: &[String], index: usize) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        if next.trim().is_empty() {
            return Err(invalid_usage(
                "`--task` requires a task label. Use `--task=LABEL` or `--task LABEL`.",
            ));
        }
        return Ok((ParsedToken::Task(next.clone()), 2));
    }
    Err(invalid_usage(
        "`--task` requires a task label. Use `--task=LABEL` or `--task LABEL`.",
    ))
}

fn classify_task_goal_arg(args: &[String], index: usize) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let (label, goal) = parse_task_goal_value(next)?;
        return Ok((
            ParsedToken::TaskGoal {
                label: Some(label),
                goal,
            },
            2,
        ));
    }
    Ok((
        ParsedToken::TaskGoal {
            label: None,
            goal: None,
        },
        1,
    ))
}

fn classify_profile_arg(args: &[String], index: usize) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let selected = parse_profile_id(next)?;
        return Ok((ParsedToken::Profile(Some(selected)), 2));
    }
    Ok((ParsedToken::Profile(None), 1))
}

fn classify_export_arg(args: &[String], index: usize) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        return Ok((ParsedToken::Export(Some(PathBuf::from(next))), 2));
    }
    Ok((ParsedToken::Export(None), 1))
}

fn classify_watch_arg(args: &[String], index: usize) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        return Ok((
            ParsedToken::Watch(Some(parse_watch_interval_secs(next)?)),
            2,
        ));
    }
    Ok((ParsedToken::Watch(None), 1))
}

fn classify_blocklist_profile_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        if next.trim().is_empty() {
            return Err(invalid_usage(
                "`--blocklist-profile` requires a profile name when a value is provided.",
            ));
        }
        return Ok((ParsedToken::BlocklistProfile(Some(next.clone())), 2));
    }
    Ok((ParsedToken::BlocklistProfile(None), 1))
}

fn classify_blocklist_profile_create_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value = require_nonempty_key_value(
            next,
            "`--blocklist-profile-create` requires a profile name.",
        )?;
        return Ok((ParsedToken::BlocklistProfileCreate(value.to_string()), 2));
    }
    Err(invalid_usage(
        "`--blocklist-profile-create` requires a profile name. Use `--blocklist-profile-create=NAME` or `--blocklist-profile-create NAME`.",
    ))
}

fn classify_blocklist_profile_rename_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value = require_nonempty_key_value(
            next,
            "`--blocklist-profile-rename` requires a profile name.",
        )?;
        return Ok((ParsedToken::BlocklistProfileRename(value.to_string()), 2));
    }
    Err(invalid_usage(
        "`--blocklist-profile-rename` requires a profile name. Use `--blocklist-profile-rename=NAME` or `--blocklist-profile-rename NAME`.",
    ))
}

fn classify_blocklist_site_add_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value =
            require_nonempty_key_value(next, "`--blocklist-site-add` requires hostnames input.")?;
        return Ok((ParsedToken::BlocklistSiteAdd(value.to_string()), 2));
    }
    Err(invalid_usage(
        "`--blocklist-site-add` requires hostnames input. Use `--blocklist-site-add=HOSTNAMES` or `--blocklist-site-add HOSTNAMES`.",
    ))
}

fn classify_allowlist_site_add_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value =
            require_nonempty_key_value(next, "`--allowlist-site-add` requires hostnames input.")?;
        return Ok((ParsedToken::AllowlistSiteAdd(value.to_string()), 2));
    }
    Err(invalid_usage(
        "`--allowlist-site-add` requires hostnames input. Use `--allowlist-site-add=HOSTNAMES` or `--allowlist-site-add HOSTNAMES`.",
    ))
}

fn classify_blocklist_site_edit_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        return Ok((
            ParsedToken::BlocklistSiteEdit(parse_site_edit_value(next)?),
            2,
        ));
    }
    Err(invalid_usage(
        "`--blocklist-site-edit` requires `OLD=NEW`. Use `--blocklist-site-edit=OLD=NEW` or `--blocklist-site-edit OLD=NEW`.",
    ))
}

fn classify_allowlist_site_edit_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        return Ok((
            ParsedToken::AllowlistSiteEdit(parse_site_edit_value(next)?),
            2,
        ));
    }
    Err(invalid_usage(
        "`--allowlist-site-edit` requires `OLD=NEW`. Use `--allowlist-site-edit=OLD=NEW` or `--allowlist-site-edit OLD=NEW`.",
    ))
}

fn classify_blocklist_site_delete_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value =
            require_nonempty_key_value(next, "`--blocklist-site-delete` requires a hostname.")?;
        return Ok((ParsedToken::BlocklistSiteDelete(value.to_string()), 2));
    }
    Err(invalid_usage(
        "`--blocklist-site-delete` requires a hostname. Use `--blocklist-site-delete=HOSTNAME` or `--blocklist-site-delete HOSTNAME`.",
    ))
}

fn classify_allowlist_site_delete_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value =
            require_nonempty_key_value(next, "`--allowlist-site-delete` requires a hostname.")?;
        return Ok((ParsedToken::AllowlistSiteDelete(value.to_string()), 2));
    }
    Err(invalid_usage(
        "`--allowlist-site-delete` requires a hostname. Use `--allowlist-site-delete=HOSTNAME` or `--allowlist-site-delete HOSTNAME`.",
    ))
}

fn classify_goal_arg(args: &[String], index: usize) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        return Ok((ParsedToken::Goal(Some(parse_goal_value(next)?)), 2));
    }
    Ok((ParsedToken::Goal(None), 1))
}

fn classify_goal_weekly_arg(args: &[String], index: usize) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        return Ok((
            ParsedToken::GoalWeekly(Some(parse_weekly_goal_value(next)?)),
            2,
        ));
    }
    Ok((ParsedToken::GoalWeekly(None), 1))
}

fn classify_goal_monthly_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        return Ok((
            ParsedToken::GoalMonthly(Some(parse_monthly_goal_value(next)?)),
            2,
        ));
    }
    Ok((ParsedToken::GoalMonthly(None), 1))
}

fn classify_strict_arg(args: &[String], index: usize) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        return Ok((ParsedToken::Strict(Some(parse_strict_value(next)?)), 2));
    }
    Ok((ParsedToken::Strict(None), 1))
}

fn classify_goal_carry_arg(args: &[String], index: usize) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        return Ok((
            ParsedToken::GoalCarry(Some(parse_goal_carry_value(next)?)),
            2,
        ));
    }
    Ok((ParsedToken::GoalCarry(None), 1))
}

fn classify_goal_carry_weekly_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        return Ok((
            ParsedToken::GoalCarryWeekly(Some(parse_goal_carry_value(next)?)),
            2,
        ));
    }
    Ok((ParsedToken::GoalCarryWeekly(None), 1))
}

fn classify_goal_carry_monthly_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        return Ok((
            ParsedToken::GoalCarryMonthly(Some(parse_goal_carry_value(next)?)),
            2,
        ));
    }
    Ok((ParsedToken::GoalCarryMonthly(None), 1))
}

fn classify_schedule_set_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        return Ok((ParsedToken::ScheduleSet(parse_schedule_value(next)?), 2));
    }
    Err(invalid_usage(
        "`--schedule-set` requires a JSON payload. Use `--schedule-set='{\"windows\":[...],\"exception_dates\":[...]}'`.",
    ))
}

fn classify_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    let parsers: [KeyValueParser; 22] = [
        parse_task_key_value_arg,
        parse_task_goal_key_value_arg,
        parse_profile_key_value_arg,
        parse_goal_key_value_arg,
        parse_goal_weekly_key_value_arg,
        parse_goal_monthly_key_value_arg,
        parse_goal_carry_key_value_arg,
        parse_goal_carry_weekly_key_value_arg,
        parse_goal_carry_monthly_key_value_arg,
        parse_strict_key_value_arg,
        parse_schedule_set_key_value_arg,
        parse_watch_key_value_arg,
        parse_export_key_value_arg,
        parse_blocklist_profile_key_value_arg,
        parse_blocklist_profile_create_key_value_arg,
        parse_blocklist_profile_rename_key_value_arg,
        parse_blocklist_site_add_key_value_arg,
        parse_allowlist_site_add_key_value_arg,
        parse_blocklist_site_edit_key_value_arg,
        parse_allowlist_site_edit_key_value_arg,
        parse_blocklist_site_delete_key_value_arg,
        parse_allowlist_site_delete_key_value_arg,
    ];

    for parser in parsers {
        if let Some(token) = parser(arg)? {
            return Ok(Some(token));
        }
    }

    Ok(None)
}

fn parse_task_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--task=") {
        let value = require_nonempty_key_value(value, "`--task=` requires a task label.")?;
        return Ok(Some(ParsedToken::Task(value.to_string())));
    }
    Ok(None)
}

fn parse_task_goal_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--task-goal=") {
        let value = require_nonempty_key_value(
            value,
            "`--task-goal=` requires `LABEL` or `LABEL:MINUTES,POMODOROS`.",
        )?;
        let (label, goal) = parse_task_goal_value(value)?;
        return Ok(Some(ParsedToken::TaskGoal {
            label: Some(label),
            goal,
        }));
    }
    Ok(None)
}

fn parse_profile_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--profile=") {
        let value = require_nonempty_key_value(value, "`--profile=` requires a profile value.")?;
        return Ok(Some(ParsedToken::Profile(Some(parse_profile_id(value)?))));
    }
    Ok(None)
}

fn parse_goal_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--goal=") {
        let value = require_nonempty_key_value(
            value,
            "`--goal=` requires values in `MINUTES,POMODOROS` format.",
        )?;
        return Ok(Some(ParsedToken::Goal(Some(parse_goal_value(value)?))));
    }
    Ok(None)
}

fn parse_goal_weekly_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--goal-weekly=") {
        let value = require_nonempty_key_value(
            value,
            "`--goal-weekly=` requires values in `MINUTES,POMODOROS` format.",
        )?;
        return Ok(Some(ParsedToken::GoalWeekly(Some(
            parse_weekly_goal_value(value)?,
        ))));
    }
    Ok(None)
}

fn parse_goal_monthly_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--goal-monthly=") {
        let value = require_nonempty_key_value(
            value,
            "`--goal-monthly=` requires values in `MINUTES,POMODOROS` format.",
        )?;
        return Ok(Some(ParsedToken::GoalMonthly(Some(
            parse_monthly_goal_value(value)?,
        ))));
    }
    Ok(None)
}

fn parse_strict_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--strict=") {
        let value = require_nonempty_key_value(value, "`--strict=` requires `on` or `off`.")?;
        return Ok(Some(ParsedToken::Strict(Some(parse_strict_value(value)?))));
    }
    Ok(None)
}

fn parse_goal_carry_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--goal-carry=") {
        let value = require_nonempty_key_value(value, "`--goal-carry=` requires `on` or `off`.")?;
        return Ok(Some(ParsedToken::GoalCarry(Some(parse_goal_carry_value(
            value,
        )?))));
    }
    Ok(None)
}

fn parse_goal_carry_weekly_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--goal-carry-weekly=") {
        let value =
            require_nonempty_key_value(value, "`--goal-carry-weekly=` requires `on` or `off`.")?;
        return Ok(Some(ParsedToken::GoalCarryWeekly(Some(
            parse_goal_carry_value(value)?,
        ))));
    }
    Ok(None)
}

fn parse_goal_carry_monthly_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--goal-carry-monthly=") {
        let value =
            require_nonempty_key_value(value, "`--goal-carry-monthly=` requires `on` or `off`.")?;
        return Ok(Some(ParsedToken::GoalCarryMonthly(Some(
            parse_goal_carry_value(value)?,
        ))));
    }
    Ok(None)
}

fn parse_schedule_set_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--schedule-set=") {
        let value =
            require_nonempty_key_value(value, "`--schedule-set=` requires a JSON payload.")?;
        return Ok(Some(ParsedToken::ScheduleSet(parse_schedule_value(value)?)));
    }
    Ok(None)
}

fn parse_export_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--export=") {
        let value = require_nonempty_key_value(value, "`--export=` requires a target directory.")?;
        return Ok(Some(ParsedToken::Export(Some(PathBuf::from(value)))));
    }
    Ok(None)
}

fn parse_watch_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--watch=") {
        let value = require_nonempty_key_value(
            value,
            "`--watch=` requires a positive whole number of seconds.",
        )?;
        return Ok(Some(ParsedToken::Watch(Some(parse_watch_interval_secs(
            value,
        )?))));
    }
    Ok(None)
}

fn parse_blocklist_profile_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--blocklist-profile=") {
        let value =
            require_nonempty_key_value(value, "`--blocklist-profile=` requires a profile name.")?;
        return Ok(Some(ParsedToken::BlocklistProfile(Some(value.to_string()))));
    }
    Ok(None)
}

fn parse_blocklist_profile_create_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--blocklist-profile-create=") {
        let value = require_nonempty_key_value(
            value,
            "`--blocklist-profile-create=` requires a profile name.",
        )?;
        return Ok(Some(ParsedToken::BlocklistProfileCreate(value.to_string())));
    }
    Ok(None)
}

fn parse_blocklist_profile_rename_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--blocklist-profile-rename=") {
        let value = require_nonempty_key_value(
            value,
            "`--blocklist-profile-rename=` requires a profile name.",
        )?;
        return Ok(Some(ParsedToken::BlocklistProfileRename(value.to_string())));
    }
    Ok(None)
}

fn parse_blocklist_site_add_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--blocklist-site-add=") {
        let value =
            require_nonempty_key_value(value, "`--blocklist-site-add=` requires hostnames input.")?;
        return Ok(Some(ParsedToken::BlocklistSiteAdd(value.to_string())));
    }
    Ok(None)
}

fn parse_allowlist_site_add_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--allowlist-site-add=") {
        let value =
            require_nonempty_key_value(value, "`--allowlist-site-add=` requires hostnames input.")?;
        return Ok(Some(ParsedToken::AllowlistSiteAdd(value.to_string())));
    }
    Ok(None)
}

fn parse_blocklist_site_edit_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--blocklist-site-edit=") {
        return Ok(Some(ParsedToken::BlocklistSiteEdit(parse_site_edit_value(
            value,
        )?)));
    }
    Ok(None)
}

fn parse_allowlist_site_edit_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--allowlist-site-edit=") {
        return Ok(Some(ParsedToken::AllowlistSiteEdit(parse_site_edit_value(
            value,
        )?)));
    }
    Ok(None)
}

fn parse_blocklist_site_delete_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--blocklist-site-delete=") {
        let value =
            require_nonempty_key_value(value, "`--blocklist-site-delete=` requires a hostname.")?;
        return Ok(Some(ParsedToken::BlocklistSiteDelete(value.to_string())));
    }
    Ok(None)
}

fn parse_allowlist_site_delete_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--allowlist-site-delete=") {
        let value =
            require_nonempty_key_value(value, "`--allowlist-site-delete=` requires a hostname.")?;
        return Ok(Some(ParsedToken::AllowlistSiteDelete(value.to_string())));
    }
    Ok(None)
}

fn require_nonempty_key_value<'a>(value: &'a str, message: &str) -> Result<&'a str, String> {
    if value.trim().is_empty() {
        return Err(invalid_usage(message));
    }
    Ok(value)
}

fn parse_global_tokens(tokens: &[ParsedToken]) -> Result<(bool, OutputMode), String> {
    let show_help = tokens
        .iter()
        .any(|token| matches!(token, ParsedToken::Help));
    if show_help {
        return Ok((true, OutputMode::Text));
    }

    let mut output = OutputMode::Text;
    for token in tokens {
        match token {
            ParsedToken::Help => {}
            ParsedToken::Json => output = OutputMode::Json,
            ParsedToken::UnknownOption(option) => {
                return Err(invalid_usage(&format!("Unknown option `{option}`.")));
            }
            ParsedToken::Positional(value) => {
                return Err(invalid_usage(&format!(
                    "Unexpected positional argument `{value}`."
                )));
            }
            ParsedToken::Start
            | ParsedToken::Pause
            | ParsedToken::Resume
            | ParsedToken::Stop
            | ParsedToken::Next
            | ParsedToken::Task(_)
            | ParsedToken::TaskGoal { .. }
            | ParsedToken::Status
            | ParsedToken::Watch(_)
            | ParsedToken::Profile(_)
            | ParsedToken::Goal(_)
            | ParsedToken::GoalWeekly(_)
            | ParsedToken::GoalMonthly(_)
            | ParsedToken::GoalCarry(_)
            | ParsedToken::GoalCarryWeekly(_)
            | ParsedToken::GoalCarryMonthly(_)
            | ParsedToken::Strict(_)
            | ParsedToken::Schedule
            | ParsedToken::ScheduleSet(_)
            | ParsedToken::Diagnostics
            | ParsedToken::BlockingPreview
            | ParsedToken::Export(_)
            | ParsedToken::BlocklistProfile(_)
            | ParsedToken::BlocklistProfileCreate(_)
            | ParsedToken::BlocklistProfileRename(_)
            | ParsedToken::BlocklistProfileDelete
            | ParsedToken::BlocklistSites
            | ParsedToken::AllowlistSites
            | ParsedToken::BlocklistSiteAdd(_)
            | ParsedToken::AllowlistSiteAdd(_)
            | ParsedToken::BlocklistSiteEdit(_)
            | ParsedToken::AllowlistSiteEdit(_)
            | ParsedToken::BlocklistSiteDelete(_)
            | ParsedToken::AllowlistSiteDelete(_) => {}
        }
    }
    Ok((show_help, output))
}

fn parse_primary_command(tokens: &[ParsedToken]) -> Result<Option<PrimaryCommand>, String> {
    let mut primary: Option<PrimaryCommand> = None;
    for token in tokens {
        match token {
            ParsedToken::Start => set_primary_command(&mut primary, PrimaryCommand::Start)?,
            ParsedToken::Pause => set_primary_command(&mut primary, PrimaryCommand::Pause)?,
            ParsedToken::Resume => set_primary_command(&mut primary, PrimaryCommand::Resume)?,
            ParsedToken::Stop => set_primary_command(&mut primary, PrimaryCommand::Stop)?,
            ParsedToken::Next => set_primary_command(&mut primary, PrimaryCommand::Next)?,
            ParsedToken::Task(label) => {
                set_primary_command(&mut primary, PrimaryCommand::Task(label.clone()))?
            }
            ParsedToken::TaskGoal { label, goal } => set_primary_command(
                &mut primary,
                PrimaryCommand::TaskGoal {
                    label: label.clone(),
                    goal: *goal,
                },
            )?,
            ParsedToken::Status => set_primary_command(&mut primary, PrimaryCommand::Status)?,
            ParsedToken::Watch(_) => {}
            ParsedToken::Profile(profile) => {
                set_primary_command(&mut primary, PrimaryCommand::Profile(*profile))?
            }
            ParsedToken::Goal(goal) => {
                set_primary_command(&mut primary, PrimaryCommand::Goal(*goal))?
            }
            ParsedToken::GoalWeekly(goal) => {
                set_primary_command(&mut primary, PrimaryCommand::GoalWeekly(*goal))?
            }
            ParsedToken::GoalMonthly(goal) => {
                set_primary_command(&mut primary, PrimaryCommand::GoalMonthly(*goal))?
            }
            ParsedToken::GoalCarry(enabled) => {
                set_primary_command(&mut primary, PrimaryCommand::GoalCarry(*enabled))?
            }
            ParsedToken::GoalCarryWeekly(enabled) => {
                set_primary_command(&mut primary, PrimaryCommand::GoalCarryWeekly(*enabled))?
            }
            ParsedToken::GoalCarryMonthly(enabled) => {
                set_primary_command(&mut primary, PrimaryCommand::GoalCarryMonthly(*enabled))?
            }
            ParsedToken::Strict(enabled) => {
                set_primary_command(&mut primary, PrimaryCommand::Strict(*enabled))?
            }
            ParsedToken::Schedule => set_primary_command(&mut primary, PrimaryCommand::Schedule)?,
            ParsedToken::ScheduleSet(schedule) => {
                set_primary_command(&mut primary, PrimaryCommand::ScheduleSet(schedule.clone()))?
            }
            ParsedToken::Diagnostics => {
                set_primary_command(&mut primary, PrimaryCommand::Diagnostics)?
            }
            ParsedToken::BlockingPreview => {
                set_primary_command(&mut primary, PrimaryCommand::BlockingPreview)?
            }
            ParsedToken::Export(dir) => {
                set_primary_command(&mut primary, PrimaryCommand::Export(dir.clone()))?
            }
            ParsedToken::BlocklistProfile(profile) => set_primary_command(
                &mut primary,
                PrimaryCommand::BlocklistProfile(profile.clone()),
            )?,
            ParsedToken::BlocklistProfileCreate(name) => set_primary_command(
                &mut primary,
                PrimaryCommand::BlocklistProfileCreate(name.clone()),
            )?,
            ParsedToken::BlocklistProfileRename(name) => set_primary_command(
                &mut primary,
                PrimaryCommand::BlocklistProfileRename(name.clone()),
            )?,
            ParsedToken::BlocklistProfileDelete => {
                set_primary_command(&mut primary, PrimaryCommand::BlocklistProfileDelete)?
            }
            ParsedToken::BlocklistSites => {
                set_primary_command(&mut primary, PrimaryCommand::BlocklistSites)?
            }
            ParsedToken::AllowlistSites => {
                set_primary_command(&mut primary, PrimaryCommand::AllowlistSites)?
            }
            ParsedToken::BlocklistSiteAdd(input) => set_primary_command(
                &mut primary,
                PrimaryCommand::BlocklistSiteAdd(input.clone()),
            )?,
            ParsedToken::AllowlistSiteAdd(input) => set_primary_command(
                &mut primary,
                PrimaryCommand::AllowlistSiteAdd(input.clone()),
            )?,
            ParsedToken::BlocklistSiteEdit(value) => set_primary_command(
                &mut primary,
                PrimaryCommand::BlocklistSiteEdit(value.clone()),
            )?,
            ParsedToken::AllowlistSiteEdit(value) => set_primary_command(
                &mut primary,
                PrimaryCommand::AllowlistSiteEdit(value.clone()),
            )?,
            ParsedToken::BlocklistSiteDelete(site) => set_primary_command(
                &mut primary,
                PrimaryCommand::BlocklistSiteDelete(site.clone()),
            )?,
            ParsedToken::AllowlistSiteDelete(site) => set_primary_command(
                &mut primary,
                PrimaryCommand::AllowlistSiteDelete(site.clone()),
            )?,
            ParsedToken::Help
            | ParsedToken::Json
            | ParsedToken::UnknownOption(_)
            | ParsedToken::Positional(_) => {}
        }
    }
    Ok(primary)
}

fn finalize_cli_action(
    show_help: bool,
    output: OutputMode,
    primary: Option<PrimaryCommand>,
    watch_interval_secs: Option<u64>,
) -> Result<CliAction, String> {
    if show_help {
        return Ok(CliAction::ShowHelp);
    }

    if watch_interval_secs.is_some() && !matches!(primary, Some(PrimaryCommand::Status)) {
        return Err(invalid_usage("`--watch` is only valid with `--status`."));
    }

    match primary {
        None => {
            if output == OutputMode::Json {
                return Err(invalid_usage(
                    "`--json` is only valid with non-interactive commands.",
                ));
            }
            Ok(CliAction::RunTui {
                start_immediately: false,
            })
        }
        Some(PrimaryCommand::Start) => {
            if output == OutputMode::Json {
                return Err(invalid_usage("`--json` is not supported with `--start`."));
            }
            Ok(CliAction::RunTui {
                start_immediately: true,
            })
        }
        Some(PrimaryCommand::Profile(profile)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::Profile { profile },
            output,
        })),
        Some(PrimaryCommand::Goal(goal)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::Goal { goal },
            output,
        })),
        Some(PrimaryCommand::GoalWeekly(goal)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::GoalWeekly { goal },
            output,
        })),
        Some(PrimaryCommand::GoalMonthly(goal)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::GoalMonthly { goal },
            output,
        })),
        Some(PrimaryCommand::GoalCarry(enabled)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::GoalCarry { enabled },
            output,
        })),
        Some(PrimaryCommand::GoalCarryWeekly(enabled)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::GoalCarryWeekly { enabled },
            output,
        })),
        Some(PrimaryCommand::GoalCarryMonthly(enabled)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::GoalCarryMonthly { enabled },
            output,
        })),
        Some(PrimaryCommand::Strict(enabled)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::Strict { enabled },
            output,
        })),
        Some(PrimaryCommand::Schedule) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::Schedule { schedule: None },
            output,
        })),
        Some(PrimaryCommand::ScheduleSet(schedule)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::Schedule {
                schedule: Some(schedule),
            },
            output,
        })),
        Some(PrimaryCommand::Diagnostics) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::Diagnostics,
            output,
        })),
        Some(PrimaryCommand::BlockingPreview) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::BlockingPreview,
            output,
        })),
        Some(PrimaryCommand::Pause) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::Pause,
            output,
        })),
        Some(PrimaryCommand::Resume) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::Resume,
            output,
        })),
        Some(PrimaryCommand::Stop) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::Stop,
            output,
        })),
        Some(PrimaryCommand::Next) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::Next,
            output,
        })),
        Some(PrimaryCommand::Task(label)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::Task { label },
            output,
        })),
        Some(PrimaryCommand::TaskGoal { label, goal }) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::TaskGoal { label, goal },
            output,
        })),
        Some(PrimaryCommand::Status) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::Status {
                watch_interval_secs,
            },
            output,
        })),
        Some(PrimaryCommand::Export(dir)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::Export { dir },
            output,
        })),
        Some(PrimaryCommand::BlocklistProfile(profile)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::BlocklistProfile {
                command: BlocklistProfileCommandKind::Select { profile },
            },
            output,
        })),
        Some(PrimaryCommand::BlocklistProfileCreate(name)) => {
            Ok(CliAction::RunCommand(CliCommand {
                kind: CommandKind::BlocklistProfile {
                    command: BlocklistProfileCommandKind::Create { name },
                },
                output,
            }))
        }
        Some(PrimaryCommand::BlocklistProfileRename(name)) => {
            Ok(CliAction::RunCommand(CliCommand {
                kind: CommandKind::BlocklistProfile {
                    command: BlocklistProfileCommandKind::Rename { name },
                },
                output,
            }))
        }
        Some(PrimaryCommand::BlocklistProfileDelete) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::BlocklistProfile {
                command: BlocklistProfileCommandKind::Delete,
            },
            output,
        })),
        Some(PrimaryCommand::BlocklistSites) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::BlocklistSites {
                target: SiteListTarget::Blocklist,
                command: BlocklistSiteCommandKind::List,
            },
            output,
        })),
        Some(PrimaryCommand::AllowlistSites) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::BlocklistSites {
                target: SiteListTarget::Allowlist,
                command: BlocklistSiteCommandKind::List,
            },
            output,
        })),
        Some(PrimaryCommand::BlocklistSiteAdd(input)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::BlocklistSites {
                target: SiteListTarget::Blocklist,
                command: BlocklistSiteCommandKind::Add { input },
            },
            output,
        })),
        Some(PrimaryCommand::AllowlistSiteAdd(input)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::BlocklistSites {
                target: SiteListTarget::Allowlist,
                command: BlocklistSiteCommandKind::Add { input },
            },
            output,
        })),
        Some(PrimaryCommand::BlocklistSiteEdit(value)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::BlocklistSites {
                target: SiteListTarget::Blocklist,
                command: BlocklistSiteCommandKind::Edit { value },
            },
            output,
        })),
        Some(PrimaryCommand::AllowlistSiteEdit(value)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::BlocklistSites {
                target: SiteListTarget::Allowlist,
                command: BlocklistSiteCommandKind::Edit { value },
            },
            output,
        })),
        Some(PrimaryCommand::BlocklistSiteDelete(site)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::BlocklistSites {
                target: SiteListTarget::Blocklist,
                command: BlocklistSiteCommandKind::Delete { site },
            },
            output,
        })),
        Some(PrimaryCommand::AllowlistSiteDelete(site)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::BlocklistSites {
                target: SiteListTarget::Allowlist,
                command: BlocklistSiteCommandKind::Delete { site },
            },
            output,
        })),
    }
}

pub fn execute_command(cli_command: CliCommand) -> Result<(), String> {
    match cli_command.kind {
        CommandKind::Pause => execute_pause_command(cli_command.output),
        CommandKind::Resume => execute_resume_command(cli_command.output),
        CommandKind::Stop => execute_stop_command(cli_command.output),
        CommandKind::Next => execute_next_command(cli_command.output),
        CommandKind::Task { label } => execute_task_command(label, cli_command.output),
        CommandKind::TaskGoal { label, goal } => {
            execute_task_goal_command(label, goal, cli_command.output)
        }
        CommandKind::Profile { profile } => execute_profile_command(profile, cli_command.output),
        CommandKind::Goal { goal } => execute_goal_command(goal, cli_command.output),
        CommandKind::GoalWeekly { goal } => execute_weekly_goal_command(goal, cli_command.output),
        CommandKind::GoalMonthly { goal } => execute_monthly_goal_command(goal, cli_command.output),
        CommandKind::GoalCarry { enabled } => {
            execute_goal_carry_command(enabled, cli_command.output)
        }
        CommandKind::GoalCarryWeekly { enabled } => {
            execute_weekly_goal_carry_command(enabled, cli_command.output)
        }
        CommandKind::GoalCarryMonthly { enabled } => {
            execute_monthly_goal_carry_command(enabled, cli_command.output)
        }
        CommandKind::Strict { enabled } => execute_strict_command(enabled, cli_command.output),
        CommandKind::Schedule { schedule } => {
            execute_schedule_command(schedule, cli_command.output)
        }
        CommandKind::Diagnostics => execute_diagnostics_command(cli_command.output),
        CommandKind::BlockingPreview => execute_blocking_preview_command(cli_command.output),
        CommandKind::Status {
            watch_interval_secs,
        } => execute_status_command(cli_command.output, watch_interval_secs),
        CommandKind::Export { dir } => execute_export_command(dir, cli_command.output),
        CommandKind::BlocklistProfile { command } => {
            execute_blocklist_profile_command(command, cli_command.output)
        }
        CommandKind::BlocklistSites { target, command } => {
            execute_blocklist_sites_command(target, command, cli_command.output)
        }
    }
}

fn execute_pause_command(output: OutputMode) -> Result<(), String> {
    let mut app = App::new();
    app.pause_for_cli()?;
    emit_timer_command_output("pause", &app, output)
}

fn execute_resume_command(output: OutputMode) -> Result<(), String> {
    let mut app = App::new();
    app.resume_for_cli()?;
    emit_timer_command_output("resume", &app, output)
}

fn execute_stop_command(output: OutputMode) -> Result<(), String> {
    let mut app = App::new();
    app.stop_for_cli()?;
    emit_timer_command_output("stop", &app, output)
}

fn execute_next_command(output: OutputMode) -> Result<(), String> {
    let mut app = App::new();
    app.next_phase_for_cli()?;
    emit_timer_command_output("next", &app, output)
}

fn execute_task_command(label: String, output: OutputMode) -> Result<(), String> {
    let mut app = App::new();
    let created = app.select_task_label_for_cli(&label)?;
    let selected_task_label = app
        .selected_task_label_for_cli()
        .ok_or_else(|| "Task selection failed: no selected task label after update.".to_string())?;
    let payload = TaskCommandOutput {
        action: "task",
        created,
        selected_task_label,
        timer: build_timer_state_output(&app),
    };

    match output {
        OutputMode::Text => {
            if payload.created {
                println!(
                    "Task label added and selected: {}",
                    payload.selected_task_label
                );
            } else {
                println!("Task label selected: {}", payload.selected_task_label);
            }
            print_timer_state_output(&payload.timer);
        }
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_task_goal_command(
    label: Option<String>,
    goal: Option<DailyGoalConfig>,
    output: OutputMode,
) -> Result<(), String> {
    let mut stats = FocusStats::load()?;
    let selected_task_label = stats.task_planner_state().1;
    let requested_label = label.or(selected_task_label).ok_or_else(|| {
        "No task label selected. Use `--task LABEL` first or pass `--task-goal LABEL`.".to_string()
    })?;

    let mut updated = false;
    let task_label = if let Some(goal) = goal {
        let target = DailyGoalSnapshot {
            minutes: goal.minutes,
            pomodoros: goal.pomodoros,
        };
        let canonical = stats.set_task_goal_target(&requested_label, target)?;
        stats
            .save()
            .map_err(|error| format!("Failed to save task goals: {error}"))?;
        updated = true;
        canonical
    } else {
        requested_label
    };

    let TaskGoalOutput {
        task_label,
        configured,
        minutes_target,
        pomodoros_target,
        focused_minutes,
        pomodoros_completed,
        met,
    } = build_task_goal_output(&stats, &task_label);
    let payload = TaskGoalCommandOutput {
        updated,
        task_label,
        configured,
        minutes_target,
        pomodoros_target,
        focused_minutes,
        pomodoros_completed,
        met,
    };

    match output {
        OutputMode::Text => print_task_goal_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_blocklist_profile_command(
    command: BlocklistProfileCommandKind,
    output: OutputMode,
) -> Result<(), String> {
    let mut config = AppConfig::load().normalized();
    let payload = apply_blocklist_profile_command(&mut config, command)?;
    if payload.updated {
        config
            .save()
            .map_err(|error| format!("Failed to save blocklist profile settings: {error}"))?;
    }

    match output {
        OutputMode::Text => print_blocklist_profile_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_blocklist_sites_command(
    target: SiteListTarget,
    command: BlocklistSiteCommandKind,
    output: OutputMode,
) -> Result<(), String> {
    let mut config = AppConfig::load().normalized();
    match command {
        BlocklistSiteCommandKind::List => {
            let payload = build_site_list_command_output(&config, target, "site-list");
            match output {
                OutputMode::Text => print_site_list_command_output(&payload),
                OutputMode::Json => print_json(&payload)?,
            }
        }
        BlocklistSiteCommandKind::Add { input } => {
            let payload = apply_site_add_command(&mut config, target, &input)?;
            if payload.updated {
                config
                    .save()
                    .map_err(|error| format!("Failed to save site changes: {error}"))?;
            }
            match output {
                OutputMode::Text => print_site_add_command_output(&payload),
                OutputMode::Json => print_json(&payload)?,
            }
        }
        BlocklistSiteCommandKind::Edit { value } => {
            let payload = apply_site_edit_command(&mut config, target, &value)?;
            if payload.updated {
                config
                    .save()
                    .map_err(|error| format!("Failed to save site changes: {error}"))?;
            }
            match output {
                OutputMode::Text => print_site_edit_command_output(&payload),
                OutputMode::Json => print_json(&payload)?,
            }
        }
        BlocklistSiteCommandKind::Delete { site } => {
            let payload = apply_site_delete_command(&mut config, target, &site)?;
            if payload.updated {
                config
                    .save()
                    .map_err(|error| format!("Failed to save site changes: {error}"))?;
            }
            match output {
                OutputMode::Text => print_site_delete_command_output(&payload),
                OutputMode::Json => print_json(&payload)?,
            }
        }
    }
    Ok(())
}

fn apply_blocklist_profile_command(
    config: &mut AppConfig,
    command: BlocklistProfileCommandKind,
) -> Result<BlocklistProfileCommandOutput, String> {
    ensure_blocklist_profiles(config);

    let (action, updated) = match command {
        BlocklistProfileCommandKind::Select { profile } => {
            handle_select_blocklist_profile(config, profile)?
        }
        BlocklistProfileCommandKind::Create { name } => {
            handle_create_blocklist_profile(config, name)?
        }
        BlocklistProfileCommandKind::Rename { name } => {
            handle_rename_blocklist_profile(config, name)?
        }
        BlocklistProfileCommandKind::Delete => handle_delete_blocklist_profile(config)?,
    };

    if updated {
        sync_legacy_blocked_sites(config);
    }
    Ok(build_blocklist_profile_command_output(
        config, action, updated,
    ))
}

fn handle_select_blocklist_profile(
    config: &mut AppConfig,
    profile: Option<String>,
) -> Result<(&'static str, bool), String> {
    let mut updated = false;
    if let Some(profile) = profile {
        let index = blocklist_profile_index_by_name(&config.blocklist_profiles, &profile)
            .ok_or_else(|| format!("Unknown blocklist profile `{profile}`."))?;
        let selected = config.blocklist_profiles[index].name.clone();
        if !config
            .selected_blocklist_profile
            .eq_ignore_ascii_case(&selected)
        {
            config.selected_blocklist_profile = selected;
            updated = true;
        }
    }
    Ok(("blocklist-profile", updated))
}

fn handle_create_blocklist_profile(
    config: &mut AppConfig,
    name: String,
) -> Result<(&'static str, bool), String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Profile name cannot be empty.".to_string());
    }
    if blocklist_profile_index_by_name(&config.blocklist_profiles, &name).is_some() {
        return Err(format!("Profile `{name}` already exists."));
    }
    config.blocklist_profiles.push(BlocklistProfileConfig {
        name: name.clone(),
        sites: Vec::new(),
        allowlist_sites: Vec::new(),
    });
    config.selected_blocklist_profile = name;
    Ok(("blocklist-profile-create", true))
}

fn handle_rename_blocklist_profile(
    config: &mut AppConfig,
    name: String,
) -> Result<(&'static str, bool), String> {
    let index = selected_blocklist_profile_index(config);
    let current_name = config.blocklist_profiles[index].name.clone();
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Profile name cannot be empty.".to_string());
    }
    if current_name == name {
        return Ok(("blocklist-profile-rename", false));
    }

    let duplicate =
        config
            .blocklist_profiles
            .iter()
            .enumerate()
            .any(|(candidate_index, profile)| {
                candidate_index != index && profile.name.eq_ignore_ascii_case(&name)
            });
    if duplicate {
        return Err(format!("Profile `{name}` already exists."));
    }

    config.blocklist_profiles[index].name = name.clone();
    config.selected_blocklist_profile = name;
    Ok(("blocklist-profile-rename", true))
}

fn handle_delete_blocklist_profile(config: &mut AppConfig) -> Result<(&'static str, bool), String> {
    if config.blocklist_profiles.len() <= 1 {
        return Err("At least one blocklist profile is required.".to_string());
    }
    let index = selected_blocklist_profile_index(config);
    config.blocklist_profiles.remove(index);
    let next_index = index.min(config.blocklist_profiles.len().saturating_sub(1));
    config.selected_blocklist_profile = config.blocklist_profiles[next_index].name.clone();
    Ok(("blocklist-profile-delete", true))
}

fn apply_site_add_command(
    config: &mut AppConfig,
    target: SiteListTarget,
    input: &str,
) -> Result<SiteAddCommandOutput, String> {
    ensure_blocklist_profiles(config);
    let index = selected_blocklist_profile_index(config);
    let mut working = SiteBlocker::new();
    let existing_sites = active_profile_sites(config, index, target).to_vec();
    for site in existing_sites {
        working.add_site(site);
    }
    let result = working.add_sites_from_input(input);
    let updated = !result.added.is_empty();
    *active_profile_sites_mut(config, index, target) = working.sites.clone();
    if updated {
        sync_legacy_blocked_sites(config);
    }

    let active_profile = &config.blocklist_profiles[index];
    Ok(SiteAddCommandOutput {
        action: "site-add",
        updated,
        profile: active_profile.name.clone(),
        target,
        added: result.added,
        duplicates: result.duplicates,
        invalid: invalid_site_entries_output(&result.invalid),
        sites: active_profile_sites(config, index, target).to_vec(),
        effective_blocked_sites_count: effective_blocked_sites_for_profile(active_profile).len(),
    })
}

fn apply_site_edit_command(
    config: &mut AppConfig,
    target: SiteListTarget,
    value: &SiteEditValue,
) -> Result<SiteEditCommandOutput, String> {
    ensure_blocklist_profiles(config);
    let index = selected_blocklist_profile_index(config);
    let previous = value.previous.trim();

    let mut working = SiteBlocker::new();
    for site in active_profile_sites(config, index, target) {
        working.add_site(site.clone());
    }
    let edit_index = working
        .sites
        .iter()
        .position(|site| site.eq_ignore_ascii_case(previous))
        .ok_or_else(|| {
            format!(
                "Site `{}` was not found in {}.",
                value.previous,
                target.id()
            )
        })?;
    let result = working.edit_site_from_input(edit_index, &value.next);
    match result {
        EditSiteResult::Updated { old, new } => {
            *active_profile_sites_mut(config, index, target) = working.sites.clone();
            sync_legacy_blocked_sites(config);
            let active_profile = &config.blocklist_profiles[index];
            Ok(SiteEditCommandOutput {
                action: "site-edit",
                updated: true,
                profile: active_profile.name.clone(),
                target,
                previous: old,
                current: new,
                sites: active_profile_sites(config, index, target).to_vec(),
                effective_blocked_sites_count: effective_blocked_sites_for_profile(active_profile)
                    .len(),
            })
        }
        EditSiteResult::Unchanged { hostname } => {
            let active_profile = &config.blocklist_profiles[index];
            Ok(SiteEditCommandOutput {
                action: "site-edit",
                updated: false,
                profile: active_profile.name.clone(),
                target,
                previous: hostname.clone(),
                current: hostname,
                sites: active_profile_sites(config, index, target).to_vec(),
                effective_blocked_sites_count: effective_blocked_sites_for_profile(active_profile)
                    .len(),
            })
        }
        EditSiteResult::Duplicate { hostname } => {
            Err(format!("`{hostname}` already exists in {}.", target.id()))
        }
        EditSiteResult::Invalid(invalid) => Err(format!(
            "Invalid hostname `{}` ({})",
            display_input_value(&invalid.input),
            invalid.reason.message()
        )),
        EditSiteResult::MissingSelection => Err("No site selected to edit.".to_string()),
    }
}

fn apply_site_delete_command(
    config: &mut AppConfig,
    target: SiteListTarget,
    site: &str,
) -> Result<SiteDeleteCommandOutput, String> {
    ensure_blocklist_profiles(config);
    let index = selected_blocklist_profile_index(config);
    let site = site.trim();

    let mut working = SiteBlocker::new();
    for current in active_profile_sites(config, index, target) {
        working.add_site(current.clone());
    }
    let delete_index = working
        .sites
        .iter()
        .position(|value| value.eq_ignore_ascii_case(site))
        .ok_or_else(|| format!("Site `{site}` was not found in {}.", target.id()))?;
    let removed = working
        .remove_site(delete_index)
        .ok_or_else(|| format!("Site `{site}` was not found in {}.", target.id()))?;
    *active_profile_sites_mut(config, index, target) = working.sites.clone();
    sync_legacy_blocked_sites(config);

    let active_profile = &config.blocklist_profiles[index];
    Ok(SiteDeleteCommandOutput {
        action: "site-delete",
        updated: true,
        profile: active_profile.name.clone(),
        target,
        removed,
        sites: active_profile_sites(config, index, target).to_vec(),
        effective_blocked_sites_count: effective_blocked_sites_for_profile(active_profile).len(),
    })
}

fn ensure_blocklist_profiles(config: &mut AppConfig) {
    if config.blocklist_profiles.is_empty() {
        config
            .blocklist_profiles
            .push(BlocklistProfileConfig::default());
        config.selected_blocklist_profile = config.blocklist_profiles[0].name.clone();
    }
}

fn blocklist_profile_index_by_name(
    profiles: &[BlocklistProfileConfig],
    name: &str,
) -> Option<usize> {
    profiles
        .iter()
        .position(|profile| profile.name.eq_ignore_ascii_case(name.trim()))
}

fn selected_blocklist_profile_index(config: &AppConfig) -> usize {
    blocklist_profile_index_by_name(
        &config.blocklist_profiles,
        &config.selected_blocklist_profile,
    )
    .unwrap_or(0)
}

fn active_profile_sites(
    config: &AppConfig,
    profile_index: usize,
    target: SiteListTarget,
) -> &[String] {
    match target {
        SiteListTarget::Blocklist => &config.blocklist_profiles[profile_index].sites,
        SiteListTarget::Allowlist => &config.blocklist_profiles[profile_index].allowlist_sites,
    }
}

fn active_profile_sites_mut(
    config: &mut AppConfig,
    profile_index: usize,
    target: SiteListTarget,
) -> &mut Vec<String> {
    match target {
        SiteListTarget::Blocklist => &mut config.blocklist_profiles[profile_index].sites,
        SiteListTarget::Allowlist => &mut config.blocklist_profiles[profile_index].allowlist_sites,
    }
}

fn sync_legacy_blocked_sites(config: &mut AppConfig) {
    ensure_blocklist_profiles(config);
    let index = selected_blocklist_profile_index(config);
    config.selected_blocklist_profile = config.blocklist_profiles[index].name.clone();
    config.blocked_sites = effective_blocked_sites_for_profile(&config.blocklist_profiles[index]);
}

fn build_blocklist_profile_command_output(
    config: &AppConfig,
    action: &'static str,
    updated: bool,
) -> BlocklistProfileCommandOutput {
    let selected_name = config
        .blocklist_profiles
        .get(selected_blocklist_profile_index(config))
        .map(|profile| profile.name.clone())
        .unwrap_or_else(|| config.selected_blocklist_profile.clone());
    let profiles = config
        .blocklist_profiles
        .iter()
        .map(|profile| BlocklistProfileSummaryOutput {
            name: profile.name.clone(),
            active: profile.name.eq_ignore_ascii_case(&selected_name),
            blocklist_sites_count: profile.sites.len(),
            allowlist_sites_count: profile.allowlist_sites.len(),
            effective_blocked_sites_count: effective_blocked_sites_for_profile(profile).len(),
        })
        .collect();

    BlocklistProfileCommandOutput {
        action,
        updated,
        selected_blocklist_profile: selected_name,
        profiles,
    }
}

fn build_site_list_command_output(
    config: &AppConfig,
    target: SiteListTarget,
    action: &'static str,
) -> SiteListCommandOutput {
    if config.blocklist_profiles.is_empty() {
        let fallback = if config.selected_blocklist_profile.trim().is_empty() {
            "Default".to_string()
        } else {
            config.selected_blocklist_profile.clone()
        };
        return SiteListCommandOutput {
            action,
            profile: fallback,
            target,
            sites: Vec::new(),
            effective_blocked_sites_count: 0,
        };
    }
    let index = selected_blocklist_profile_index(config);
    let profile = &config.blocklist_profiles[index];
    SiteListCommandOutput {
        action,
        profile: profile.name.clone(),
        target,
        sites: active_profile_sites(config, index, target).to_vec(),
        effective_blocked_sites_count: effective_blocked_sites_for_profile(profile).len(),
    }
}

fn invalid_site_entries_output(values: &[InvalidSiteInput]) -> Vec<InvalidSiteEntryOutput> {
    values
        .iter()
        .map(|invalid| InvalidSiteEntryOutput {
            input: invalid.input.clone(),
            reason: invalid.reason.message().to_string(),
        })
        .collect()
}

fn execute_profile_command(profile: Option<ProfileId>, output: OutputMode) -> Result<(), String> {
    let mut config = AppConfig::load().normalized();
    let mut updated = false;
    if let Some(profile) = profile {
        config.selected_profile = profile;
        config.align_legacy_automation_with_selected_profile();
        config
            .save()
            .map_err(|error| format!("Failed to save selected profile: {error}"))?;
        updated = true;
    }

    let custom = config.effective_custom_profile();
    let selected = profile_view(config.selected_profile, &custom);
    let available = [ProfileId::Classic, ProfileId::DeepWork, ProfileId::Custom]
        .into_iter()
        .map(|candidate| profile_view(candidate, &custom))
        .collect();
    let payload = ProfileOutput {
        updated,
        selected,
        available,
    };

    match output {
        OutputMode::Text => print_profile_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_goal_command(goal: Option<DailyGoalConfig>, output: OutputMode) -> Result<(), String> {
    let mut config = AppConfig::load().normalized();
    let mut updated = false;
    if let Some(goal) = goal {
        config.daily_goal = goal;
        config
            .save()
            .map_err(|error| format!("Failed to save daily goal: {error}"))?;
        updated = true;
    }

    let payload = GoalCommandOutput {
        updated,
        configured: config.daily_goal.minutes > 0 || config.daily_goal.pomodoros > 0,
        minutes_target: config.daily_goal.minutes,
        pomodoros_target: config.daily_goal.pomodoros,
    };

    match output {
        OutputMode::Text => print_goal_command_output("Daily", &payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_weekly_goal_command(
    goal: Option<WeeklyGoalConfig>,
    output: OutputMode,
) -> Result<(), String> {
    let mut config = AppConfig::load().normalized();
    let mut updated = false;
    if let Some(goal) = goal {
        config.weekly_goal = goal;
        config
            .save()
            .map_err(|error| format!("Failed to save weekly goal: {error}"))?;
        updated = true;
    }

    let payload = GoalCommandOutput {
        updated,
        configured: config.weekly_goal.minutes > 0 || config.weekly_goal.pomodoros > 0,
        minutes_target: config.weekly_goal.minutes,
        pomodoros_target: config.weekly_goal.pomodoros,
    };

    match output {
        OutputMode::Text => print_goal_command_output("Weekly", &payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_monthly_goal_command(
    goal: Option<MonthlyGoalConfig>,
    output: OutputMode,
) -> Result<(), String> {
    let mut config = AppConfig::load().normalized();
    let mut updated = false;
    if let Some(goal) = goal {
        config.monthly_goal = goal;
        config
            .save()
            .map_err(|error| format!("Failed to save monthly goal: {error}"))?;
        updated = true;
    }

    let payload = GoalCommandOutput {
        updated,
        configured: config.monthly_goal.minutes > 0 || config.monthly_goal.pomodoros > 0,
        minutes_target: config.monthly_goal.minutes,
        pomodoros_target: config.monthly_goal.pomodoros,
    };

    match output {
        OutputMode::Text => print_goal_command_output("Monthly", &payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_goal_carry_command(enabled: Option<bool>, output: OutputMode) -> Result<(), String> {
    let mut config = AppConfig::load().normalized();
    let mut updated = false;
    if let Some(enabled) = enabled {
        config.goal_carry_over.daily = enabled;
        config
            .save()
            .map_err(|error| format!("Failed to save daily goal carry-over: {error}"))?;
        updated = true;
    }

    let payload = GoalCarryCommandOutput {
        updated,
        carry_over: config.goal_carry_over.daily,
    };
    match output {
        OutputMode::Text => print_goal_carry_command_output("Daily", &payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_weekly_goal_carry_command(
    enabled: Option<bool>,
    output: OutputMode,
) -> Result<(), String> {
    let mut config = AppConfig::load().normalized();
    let mut updated = false;
    if let Some(enabled) = enabled {
        config.goal_carry_over.weekly = enabled;
        config
            .save()
            .map_err(|error| format!("Failed to save weekly goal carry-over: {error}"))?;
        updated = true;
    }

    let payload = GoalCarryCommandOutput {
        updated,
        carry_over: config.goal_carry_over.weekly,
    };
    match output {
        OutputMode::Text => print_goal_carry_command_output("Weekly", &payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_monthly_goal_carry_command(
    enabled: Option<bool>,
    output: OutputMode,
) -> Result<(), String> {
    let mut config = AppConfig::load().normalized();
    let mut updated = false;
    if let Some(enabled) = enabled {
        config.goal_carry_over.monthly = enabled;
        config
            .save()
            .map_err(|error| format!("Failed to save monthly goal carry-over: {error}"))?;
        updated = true;
    }

    let payload = GoalCarryCommandOutput {
        updated,
        carry_over: config.goal_carry_over.monthly,
    };
    match output {
        OutputMode::Text => print_goal_carry_command_output("Monthly", &payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_strict_command(enabled: Option<bool>, output: OutputMode) -> Result<(), String> {
    let mut config = AppConfig::load().normalized();
    let mut updated = false;
    if let Some(enabled) = enabled {
        let mut automation = config.profile_automation_for(config.selected_profile);
        automation.strict_mode = enabled;
        config.set_profile_automation_for(config.selected_profile, automation);
        config
            .save()
            .map_err(|error| format!("Failed to save strict mode: {error}"))?;
        updated = true;
    }

    let payload = StrictCommandOutput {
        updated,
        strict_mode: config
            .profile_automation_for(config.selected_profile)
            .strict_mode,
    };

    match output {
        OutputMode::Text => print_strict_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_schedule_command(
    schedule: Option<RecurringScheduleConfig>,
    output: OutputMode,
) -> Result<(), String> {
    let mut config = AppConfig::load().normalized();
    let mut updated = false;
    if let Some(schedule) = schedule {
        let mut automation = config.profile_automation_for(config.selected_profile);
        automation.recurring_schedule = schedule.normalized();
        config.set_profile_automation_for(config.selected_profile, automation);
        config
            .save()
            .map_err(|error| format!("Failed to save recurring schedule: {error}"))?;
        updated = true;
    }
    let selected_automation = config.profile_automation_for(config.selected_profile);
    let inspection = build_schedule_inspection_output(&selected_automation.recurring_schedule);
    let payload = ScheduleCommandOutput {
        updated,
        schedule: selected_automation.recurring_schedule,
        inspection,
    };

    match output {
        OutputMode::Text => print_schedule_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_diagnostics_command(output: OutputMode) -> Result<(), String> {
    let app = App::new();
    let payload = build_diagnostics_command_output(&app.setup_diagnostics);

    match output {
        OutputMode::Text => print_diagnostics_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_blocking_preview_command(output: OutputMode) -> Result<(), String> {
    let app = App::new();
    let preview = app.blocking_preview_for_cli()?;
    let payload = build_blocking_preview_command_output(&preview);

    match output {
        OutputMode::Text => print_blocking_preview_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_status_command(
    output: OutputMode,
    watch_interval_secs: Option<u64>,
) -> Result<(), String> {
    if let Some(interval_secs) = watch_interval_secs {
        return execute_status_watch_command(output, interval_secs);
    }

    let payload = load_status_output()?;
    emit_status_output(&payload, output, false)
}

fn execute_status_watch_command(output: OutputMode, interval_secs: u64) -> Result<(), String> {
    if interval_secs == 0 {
        return Err("`--watch` interval must be greater than 0 seconds.".to_string());
    }

    loop {
        let payload = load_status_output()?;
        emit_status_output(&payload, output, true)?;
        flush_stdout()?;
        thread::sleep(Duration::from_secs(interval_secs));
    }
}

fn load_status_output() -> Result<StatusOutput, String> {
    let config = AppConfig::load().normalized();
    let stats = FocusStats::load().map_err(|error| format!("Failed to load stats: {error}"))?;
    Ok(build_status_output(&config, &stats))
}

fn emit_status_output(
    payload: &StatusOutput,
    output: OutputMode,
    watch_mode: bool,
) -> Result<(), String> {
    match output {
        OutputMode::Text => {
            print_status_output(payload);
            if watch_mode {
                println!();
            }
        }
        OutputMode::Json => {
            if watch_mode {
                print_json_compact(payload)?;
            } else {
                print_json(payload)?;
            }
        }
    }
    Ok(())
}

fn execute_export_command(dir: Option<PathBuf>, output: OutputMode) -> Result<(), String> {
    let stats = FocusStats::load().map_err(|error| format!("Failed to load stats: {error}"))?;
    let target_dir = match dir {
        Some(path) => path,
        None => env::current_dir()
            .map_err(|error| format!("Failed to determine current directory: {error}"))?,
    };
    let exported = stats
        .export_to_dir(&target_dir)
        .map_err(|error| format!("Export failed: {error}"))?;

    let payload = ExportOutput {
        export_dir: target_dir,
        json_path: exported.json_path,
        csv_path: exported.csv_path,
    };
    match output {
        OutputMode::Text => print_export_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn emit_timer_command_output(
    action: &'static str,
    app: &App,
    output: OutputMode,
) -> Result<(), String> {
    let payload = TimerCommandOutput {
        action,
        timer: build_timer_state_output(app),
    };

    match output {
        OutputMode::Text => {
            println!("Timer action applied: {}", payload.action);
            print_timer_state_output(&payload.timer);
        }
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn build_timer_state_output(app: &App) -> TimerStateOutput {
    let (phase, status, remaining_secs, pomodoros_completed) = app.timer_state_for_cli();
    let profile = app.selected_profile_id();
    let (focus_secs, short_break_secs, long_break_secs, long_break_interval) =
        app.profile_values(profile);
    let (selected_task_label, focus_intention, task_note) =
        mirror_metadata_from_task_label(app.selected_task_label_for_cli());

    TimerStateOutput {
        phase: timer_phase_id(phase),
        status: timer_status_id(status),
        remaining_secs,
        pomodoros_completed,
        selected_profile: ProfileView {
            id: profile_id(profile),
            label: profile.label(),
            focus_secs,
            short_break_secs,
            long_break_secs,
            long_break_interval,
        },
        selected_task_label,
        focus_intention,
        task_note,
    }
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

fn parse_profile_id(value: &str) -> Result<ProfileId, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "classic" => Ok(ProfileId::Classic),
        "deep-work" | "deep_work" | "deepwork" => Ok(ProfileId::DeepWork),
        "custom" => Ok(ProfileId::Custom),
        _ => Err(invalid_usage(&format!(
            "Invalid profile `{value}`. Use `classic`, `deep-work`, or `custom`."
        ))),
    }
}

fn parse_task_goal_value(value: &str) -> Result<(String, Option<DailyGoalConfig>), String> {
    let trimmed = value.trim();
    if let Some((label_raw, goal_raw)) = trimmed.rsplit_once(':') {
        if goal_raw.contains(',') {
            let label = require_nonempty_key_value(
                label_raw,
                "Task goal requires a task label before `:`.",
            )?
            .to_string();
            let (minutes, pomodoros) = parse_goal_components(goal_raw, "--task-goal")?;
            return Ok((label, Some(DailyGoalConfig { minutes, pomodoros })));
        }
    }

    let label = require_nonempty_key_value(
        trimmed,
        "`--task-goal` requires `LABEL` or `LABEL:MINUTES,POMODOROS`.",
    )?
    .to_string();
    Ok((label, None))
}

fn parse_goal_value(value: &str) -> Result<DailyGoalConfig, String> {
    let (minutes, pomodoros) = parse_goal_components(value, "--goal")?;
    Ok(DailyGoalConfig { minutes, pomodoros })
}

fn parse_weekly_goal_value(value: &str) -> Result<WeeklyGoalConfig, String> {
    let (minutes, pomodoros) = parse_goal_components(value, "--goal-weekly")?;
    Ok(WeeklyGoalConfig { minutes, pomodoros })
}

fn parse_monthly_goal_value(value: &str) -> Result<MonthlyGoalConfig, String> {
    let (minutes, pomodoros) = parse_goal_components(value, "--goal-monthly")?;
    Ok(MonthlyGoalConfig { minutes, pomodoros })
}

fn parse_goal_components(value: &str, flag: &str) -> Result<(u64, u32), String> {
    let trimmed = value.trim();
    let (minutes_raw, pomodoros_raw) = trimmed.split_once(',').ok_or_else(|| {
        invalid_usage(&format!(
            "Invalid goal `{value}`. Use `{flag}=MINUTES,POMODOROS` (for example `{flag}=120,4`)."
        ))
    })?;
    let minutes = minutes_raw.trim().parse::<u64>().map_err(|_| {
        invalid_usage(&format!(
            "Invalid goal minutes in `{value}` for `{flag}`. Use a non-negative integer."
        ))
    })?;
    let pomodoros = pomodoros_raw.trim().parse::<u32>().map_err(|_| {
        invalid_usage(&format!(
            "Invalid goal pomodoros in `{value}` for `{flag}`. Use a non-negative integer."
        ))
    })?;
    Ok((minutes, pomodoros))
}

fn parse_strict_value(value: &str) -> Result<bool, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err(invalid_usage(&format!(
            "Invalid strict mode `{value}`. Use `--strict=on` or `--strict=off`."
        ))),
    }
}

fn parse_goal_carry_value(value: &str) -> Result<bool, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err(invalid_usage(&format!(
            "Invalid goal carry-over `{value}`. Use `on` or `off`."
        ))),
    }
}

fn parse_site_edit_value(value: &str) -> Result<SiteEditValue, String> {
    let trimmed = value.trim();
    let (previous, next) = trimmed.split_once('=').ok_or_else(|| {
        invalid_usage(&format!(
            "Invalid site edit `{value}`. Use `OLD=NEW` (for example `--blocklist-site-edit=old.com=new.com`)."
        ))
    })?;
    let previous = previous.trim();
    let next = next.trim();
    if previous.is_empty() || next.is_empty() {
        return Err(invalid_usage(
            "Site edit values must include both `OLD` and `NEW` hostnames.",
        ));
    }
    Ok(SiteEditValue {
        previous: previous.to_string(),
        next: next.to_string(),
    })
}

fn parse_schedule_value(value: &str) -> Result<RecurringScheduleConfig, String> {
    let schedule = serde_json::from_str::<RecurringScheduleConfig>(value).map_err(|error| {
        invalid_usage(&format!(
            "Invalid schedule JSON payload: {error}. Use `--schedule-set='{{\"windows\":[...],\"exception_dates\":[...],\"one_time_windows\":[...]}}'`."
        ))
    })?;
    validate_schedule_value(&schedule)?;
    Ok(schedule)
}

fn validate_schedule_value(schedule: &RecurringScheduleConfig) -> Result<(), String> {
    for (index, window) in schedule.windows.iter().enumerate() {
        validate_schedule_window(window, index)?;
    }
    for (index, date) in schedule.exception_dates.iter().enumerate() {
        validate_schedule_exception_date(date, index)?;
    }
    for (index, window) in schedule.one_time_windows.iter().enumerate() {
        validate_one_time_schedule_window(window, index)?;
    }
    Ok(())
}

fn validate_schedule_window(
    window: &RecurringFocusWindowConfig,
    index: usize,
) -> Result<(), String> {
    if window.days.is_empty() {
        return Err(invalid_usage(&format!(
            "Invalid schedule window at index {index}: `days` must include at least one weekday."
        )));
    }
    for day in &window.days {
        if !is_valid_schedule_weekday(day) {
            return Err(invalid_usage(&format!(
                "Invalid schedule window at index {index}: unknown weekday `{day}`."
            )));
        }
    }

    let start_minutes = parse_schedule_minutes(&window.start).ok_or_else(|| {
        invalid_usage(&format!(
            "Invalid schedule window at index {index}: start `{}` must be HH:MM in 24-hour format.",
            window.start
        ))
    })?;
    let end_minutes = parse_schedule_minutes(&window.end).ok_or_else(|| {
        invalid_usage(&format!(
            "Invalid schedule window at index {index}: end `{}` must be HH:MM in 24-hour format.",
            window.end
        ))
    })?;

    if start_minutes >= end_minutes {
        return Err(invalid_usage(&format!(
            "Invalid schedule window at index {index}: start must be earlier than end."
        )));
    }
    Ok(())
}

fn validate_schedule_exception_date(value: &str, index: usize) -> Result<(), String> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d").map_err(|_| {
        invalid_usage(&format!(
            "Invalid exception date at index {index}: `{value}` must be YYYY-MM-DD."
        ))
    })?;
    Ok(())
}

fn validate_one_time_schedule_window(
    window: &OneTimeFocusWindowConfig,
    index: usize,
) -> Result<(), String> {
    NaiveDate::parse_from_str(window.date.trim(), "%Y-%m-%d").map_err(|_| {
        invalid_usage(&format!(
            "Invalid one-time window at index {index}: date `{}` must be YYYY-MM-DD.",
            window.date
        ))
    })?;

    let start_minutes = parse_schedule_minutes(&window.start).ok_or_else(|| {
        invalid_usage(&format!(
            "Invalid one-time window at index {index}: start `{}` must be HH:MM in 24-hour format.",
            window.start
        ))
    })?;
    let end_minutes = parse_schedule_minutes(&window.end).ok_or_else(|| {
        invalid_usage(&format!(
            "Invalid one-time window at index {index}: end `{}` must be HH:MM in 24-hour format.",
            window.end
        ))
    })?;

    if start_minutes >= end_minutes {
        return Err(invalid_usage(&format!(
            "Invalid one-time window at index {index}: start must be earlier than end."
        )));
    }

    Ok(())
}

fn is_valid_schedule_weekday(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "mon"
            | "monday"
            | "tue"
            | "tues"
            | "tuesday"
            | "wed"
            | "wednesday"
            | "thu"
            | "thurs"
            | "thursday"
            | "fri"
            | "friday"
            | "sat"
            | "saturday"
            | "sun"
            | "sunday"
    )
}

fn parse_schedule_minutes(value: &str) -> Option<u16> {
    let trimmed = value.trim();
    let (hours, minutes) = trimmed.split_once(':')?;
    if hours.len() != 2 || minutes.len() != 2 {
        return None;
    }
    let hours = hours.parse::<u16>().ok()?;
    let minutes = minutes.parse::<u16>().ok()?;
    if hours > 23 || minutes > 59 {
        return None;
    }
    Some(hours * 60 + minutes)
}

fn set_primary_command(
    primary: &mut Option<PrimaryCommand>,
    next: PrimaryCommand,
) -> Result<(), String> {
    if let Some(existing) = primary {
        return Err(invalid_usage(&format!(
            "Multiple primary commands are not supported (`{}` and `{}`).",
            primary_name(existing),
            primary_name(&next)
        )));
    }
    *primary = Some(next);
    Ok(())
}

fn primary_name(command: &PrimaryCommand) -> &'static str {
    match command {
        PrimaryCommand::Start => "--start",
        PrimaryCommand::Pause => "--pause",
        PrimaryCommand::Resume => "--resume",
        PrimaryCommand::Stop => "--stop",
        PrimaryCommand::Next => "--next",
        PrimaryCommand::Task(_) => "--task",
        PrimaryCommand::TaskGoal { .. } => "--task-goal",
        PrimaryCommand::Profile(_) => "--profile",
        PrimaryCommand::Goal(_) => "--goal",
        PrimaryCommand::GoalWeekly(_) => "--goal-weekly",
        PrimaryCommand::GoalMonthly(_) => "--goal-monthly",
        PrimaryCommand::GoalCarry(_) => "--goal-carry",
        PrimaryCommand::GoalCarryWeekly(_) => "--goal-carry-weekly",
        PrimaryCommand::GoalCarryMonthly(_) => "--goal-carry-monthly",
        PrimaryCommand::Strict(_) => "--strict",
        PrimaryCommand::Schedule => "--schedule",
        PrimaryCommand::ScheduleSet(_) => "--schedule-set",
        PrimaryCommand::Diagnostics => "--diagnostics",
        PrimaryCommand::BlockingPreview => "--blocking-preview",
        PrimaryCommand::Status => "--status",
        PrimaryCommand::Export(_) => "--export",
        PrimaryCommand::BlocklistProfile(_) => "--blocklist-profile",
        PrimaryCommand::BlocklistProfileCreate(_) => "--blocklist-profile-create",
        PrimaryCommand::BlocklistProfileRename(_) => "--blocklist-profile-rename",
        PrimaryCommand::BlocklistProfileDelete => "--blocklist-profile-delete",
        PrimaryCommand::BlocklistSites => "--blocklist-sites",
        PrimaryCommand::AllowlistSites => "--allowlist-sites",
        PrimaryCommand::BlocklistSiteAdd(_) => "--blocklist-site-add",
        PrimaryCommand::AllowlistSiteAdd(_) => "--allowlist-site-add",
        PrimaryCommand::BlocklistSiteEdit(_) => "--blocklist-site-edit",
        PrimaryCommand::AllowlistSiteEdit(_) => "--allowlist-site-edit",
        PrimaryCommand::BlocklistSiteDelete(_) => "--blocklist-site-delete",
        PrimaryCommand::AllowlistSiteDelete(_) => "--allowlist-site-delete",
    }
}

fn print_profile_output(payload: &ProfileOutput) {
    if payload.updated {
        println!("Active profile updated.");
    }
    println!(
        "Selected profile: {} ({})",
        payload.selected.label, payload.selected.id
    );
    println!(
        "Durations: focus {}, short break {}, long break {}, cadence every {} focus",
        format_duration(payload.selected.focus_secs),
        format_duration(payload.selected.short_break_secs),
        format_duration(payload.selected.long_break_secs),
        payload.selected.long_break_interval
    );
    println!("Available profiles:");
    for profile in &payload.available {
        println!(
            "  - {} ({}): {}/{}/{} every {} focus",
            profile.label,
            profile.id,
            format_duration(profile.focus_secs),
            format_duration(profile.short_break_secs),
            format_duration(profile.long_break_secs),
            profile.long_break_interval
        );
    }
}

fn print_blocklist_profile_command_output(payload: &BlocklistProfileCommandOutput) {
    if payload.updated {
        println!("Blocklist profile updated.");
    }
    println!(
        "Selected blocklist profile: {}",
        payload.selected_blocklist_profile
    );
    if payload.profiles.is_empty() {
        println!("Profiles: none");
        return;
    }
    println!("Profiles:");
    for profile in &payload.profiles {
        let marker = if profile.active { "*" } else { " " };
        println!(
            "  {marker} {} (blocklist {}, allowlist {}, effective {})",
            profile.name,
            profile.blocklist_sites_count,
            profile.allowlist_sites_count,
            profile.effective_blocked_sites_count
        );
    }
}

fn print_site_list_command_output(payload: &SiteListCommandOutput) {
    println!(
        "Active profile `{}` {} entries: {}",
        payload.profile,
        payload.target.id(),
        payload.sites.len()
    );
    for site in &payload.sites {
        println!("  - {site}");
    }
    println!(
        "Effective blocked sites: {}",
        payload.effective_blocked_sites_count
    );
}

fn print_site_add_command_output(payload: &SiteAddCommandOutput) {
    if payload.updated {
        println!(
            "Added {} hostname(s) to {} in profile `{}`.",
            payload.added.len(),
            payload.target.id(),
            payload.profile
        );
    } else {
        println!(
            "No {} hostnames were added in profile `{}`.",
            payload.target.id(),
            payload.profile
        );
    }
    if !payload.duplicates.is_empty() {
        println!("Skipped duplicates: {}", payload.duplicates.join(", "));
    }
    if !payload.invalid.is_empty() {
        println!("Rejected invalid hostnames:");
        for invalid in &payload.invalid {
            println!(
                "  - {} ({})",
                display_input_value(&invalid.input),
                invalid.reason
            );
        }
    }
    println!(
        "{} entries now: {}",
        payload.target.id(),
        payload.sites.join(", ")
    );
    println!(
        "Effective blocked sites: {}",
        payload.effective_blocked_sites_count
    );
}

fn print_site_edit_command_output(payload: &SiteEditCommandOutput) {
    if payload.updated {
        println!(
            "Updated {} hostname in profile `{}`: {} -> {}",
            payload.target.id(),
            payload.profile,
            payload.previous,
            payload.current
        );
    } else {
        println!(
            "No change for {} hostname `{}` in profile `{}`.",
            payload.target.id(),
            payload.current,
            payload.profile
        );
    }
    println!(
        "{} entries now: {}",
        payload.target.id(),
        payload.sites.join(", ")
    );
    println!(
        "Effective blocked sites: {}",
        payload.effective_blocked_sites_count
    );
}

fn print_site_delete_command_output(payload: &SiteDeleteCommandOutput) {
    println!(
        "Deleted {} hostname `{}` from profile `{}`.",
        payload.target.id(),
        payload.removed,
        payload.profile
    );
    println!(
        "{} entries now: {}",
        payload.target.id(),
        payload.sites.join(", ")
    );
    println!(
        "Effective blocked sites: {}",
        payload.effective_blocked_sites_count
    );
}

fn print_status_output(payload: &StatusOutput) {
    println!("Date: {}", payload.day);
    println!(
        "Selected profile: {} ({})",
        payload.selected_profile.label, payload.selected_profile.id
    );
    println!(
        "Task label: {}",
        payload.selected_task_label.as_deref().unwrap_or("none")
    );
    println!(
        "Focus intention: {}",
        payload.focus_intention.as_deref().unwrap_or("none")
    );
    println!(
        "Task note: {}",
        payload.task_note.as_deref().unwrap_or("none")
    );
    println!(
        "Blocklist profile: {} ({} sites)",
        payload.selected_blocklist_profile, payload.blocked_sites_count
    );
    println!(
        "Strict mode: {}",
        if payload.strict_mode { "on" } else { "off" }
    );
    println!(
        "Today: {} focused minutes, {} pomodoros",
        payload.today.focused_minutes, payload.today.pomodoros_completed
    );
    print_status_goal_line("Daily goal", &payload.goal);
    print_status_goal_line("Weekly goal", &payload.weekly_goal);
    print_status_goal_line("Monthly goal", &payload.monthly_goal);
    print_status_task_goal_line(payload.selected_task_goal.as_ref());
    println!(
        "Session: {} focused minutes, {} pomodoros",
        payload.session.focused_minutes, payload.session.pomodoros_completed
    );
    if let Some(interruption) = payload.latest_interruption.as_ref() {
        println!(
            "Last interruption: {} ({}, {} remaining, task: {})",
            interruption.reason.label(),
            interruption.date,
            format_duration(interruption.remaining_secs),
            interruption.task_label.as_deref().unwrap_or("none")
        );
    } else {
        println!("Last interruption: none");
    }
    print_status_focus_score_line(&payload.focus_score);
    println!(
        "Live timer: {} {} ({} remaining, source: {})",
        payload.live.phase,
        payload.live.status,
        format_duration(payload.live.remaining_secs),
        payload.live.state_source
    );
    println!(
        "Live focus intention: {}",
        payload.live.focus_intention.as_deref().unwrap_or("none")
    );
    println!(
        "Live task note: {}",
        payload.live.task_note.as_deref().unwrap_or("none")
    );
    if let Some(error) = payload.live.recovery_error.as_deref() {
        println!("Live timer warning: {error}");
    }
}

fn print_status_goal_line(label: &str, goal: &GoalOutput) {
    if goal.configured {
        println!(
            "{label}: {} min, {} pomodoros ({}, carry-over: {})",
            goal.minutes_target,
            goal.pomodoros_target,
            if goal.met { "met" } else { "in progress" },
            if goal.carry_over { "on" } else { "off" }
        );
    } else {
        println!(
            "{label}: off (carry-over: {})",
            if goal.carry_over { "on" } else { "off" }
        );
    }
}

fn print_status_task_goal_line(task_goal: Option<&TaskGoalOutput>) {
    let Some(task_goal) = task_goal else {
        println!("Selected task goal: none");
        return;
    };

    if task_goal.configured {
        println!(
            "Selected task goal (`{}`): {} min, {} pomodoros ({})",
            task_goal.task_label,
            task_goal.minutes_target,
            task_goal.pomodoros_target,
            if task_goal.met { "met" } else { "in progress" }
        );
    } else {
        println!("Selected task goal (`{}`): off", task_goal.task_label);
    }
    println!(
        "Selected task progress (`{}`): {} min, {} pomodoros",
        task_goal.task_label, task_goal.focused_minutes, task_goal.pomodoros_completed
    );
}

fn print_status_focus_score_line(focus_score: &FocusScoreOutput) {
    if focus_score.available {
        println!(
            "Focus score: {}% (consistency {}%, completion {}%)",
            focus_score.focus_score_pct.unwrap_or(0),
            focus_score.consistency_score_pct,
            focus_score.completion_score_pct.unwrap_or(0)
        );
    } else {
        println!(
            "Focus score: n/a (weekly goal off; consistency {}%)",
            focus_score.consistency_score_pct
        );
    }
}

fn print_timer_state_output(timer: &TimerStateOutput) {
    println!(
        "Timer: {} {} ({} remaining)",
        timer.phase,
        timer.status,
        format_duration(timer.remaining_secs)
    );
    println!("Pomodoros completed: {}", timer.pomodoros_completed);
    println!(
        "Task label: {}",
        timer.selected_task_label.as_deref().unwrap_or("none")
    );
    println!(
        "Focus intention: {}",
        timer.focus_intention.as_deref().unwrap_or("none")
    );
    println!(
        "Task note: {}",
        timer.task_note.as_deref().unwrap_or("none")
    );
    println!(
        "Profile: {} ({})",
        timer.selected_profile.label, timer.selected_profile.id
    );
}

fn print_export_output(payload: &ExportOutput) {
    println!("Exported stats to {}", payload.export_dir.display());
    println!("JSON: {}", payload.json_path.display());
    println!("CSV: {}", payload.csv_path.display());
}

fn print_goal_command_output(label: &str, payload: &GoalCommandOutput) {
    if payload.updated {
        println!("{label} goal updated.");
    }
    if payload.configured {
        println!(
            "{label} goal: {} min, {} pomodoros",
            payload.minutes_target, payload.pomodoros_target
        );
    } else {
        println!("{label} goal: off");
    }
}

fn print_task_goal_command_output(payload: &TaskGoalCommandOutput) {
    if payload.updated {
        println!("Task goal updated for `{}`.", payload.task_label);
    }
    if payload.configured {
        println!(
            "Task goal (`{}`): {} min, {} pomodoros ({})",
            payload.task_label,
            payload.minutes_target,
            payload.pomodoros_target,
            if payload.met { "met" } else { "in progress" }
        );
    } else {
        println!("Task goal (`{}`): off", payload.task_label);
    }
    println!(
        "Task progress (`{}`): {} min, {} pomodoros",
        payload.task_label, payload.focused_minutes, payload.pomodoros_completed
    );
}

fn print_goal_carry_command_output(label: &str, payload: &GoalCarryCommandOutput) {
    if payload.updated {
        println!("{label} goal carry-over updated.");
    }
    println!(
        "{label} goal carry-over: {}",
        if payload.carry_over { "on" } else { "off" }
    );
}

fn print_strict_command_output(payload: &StrictCommandOutput) {
    if payload.updated {
        println!("Strict mode updated.");
    }
    println!(
        "Strict mode: {}",
        if payload.strict_mode { "on" } else { "off" }
    );
}

fn print_schedule_command_output(payload: &ScheduleCommandOutput) {
    if payload.updated {
        println!("Schedule updated.");
    }
    if payload.schedule.windows.is_empty() {
        println!("Recurring windows: none");
    } else {
        println!("Recurring windows:");
        for window in &payload.schedule.windows {
            println!(
                "  - [{}] {}-{}",
                window.days.join(","),
                window.start,
                window.end
            );
        }
    }
    if payload.schedule.exception_dates.is_empty() {
        println!("Exception dates: none");
    } else {
        println!(
            "Exception dates: {}",
            payload.schedule.exception_dates.join(", ")
        );
    }
    if payload.schedule.one_time_windows.is_empty() {
        println!("One-time windows: none");
    } else {
        println!("One-time windows:");
        for window in &payload.schedule.one_time_windows {
            println!("  - {} {}-{}", window.date, window.start, window.end);
        }
    }
    if payload.inspection.conflicts.is_empty() {
        println!("Schedule conflicts: none");
    } else {
        println!(
            "Schedule conflicts: {} detected",
            payload.inspection.conflict_count
        );
        for conflict in &payload.inspection.conflicts {
            println!("  - {conflict}");
        }
    }
}

fn build_schedule_inspection_output(
    schedule: &RecurringScheduleConfig,
) -> ScheduleInspectionOutput {
    let conflicts = inspect_schedule_conflicts_from_config(schedule)
        .into_iter()
        .map(|conflict| format_schedule_conflict(&conflict))
        .collect::<Vec<_>>();
    ScheduleInspectionOutput {
        conflict_count: conflicts.len(),
        conflicts,
    }
}

fn print_diagnostics_command_output(payload: &DiagnosticsCommandOutput) {
    println!("Hosts file: {}", payload.hosts_file_path);
    print_diagnostics_check("Blocking permissions", &payload.blocking_permissions);
    print_diagnostics_check("Hosts write capability", &payload.hosts_write_capability);
    print_diagnostics_check("WakaTime config", &payload.wakatime_config);
}

fn print_diagnostics_check(label: &str, check: &SetupCheckOutput) {
    println!("{label}: {} ({})", check.message, check.level);
}

fn build_diagnostics_command_output(diagnostics: &SetupDiagnostics) -> DiagnosticsCommandOutput {
    DiagnosticsCommandOutput {
        hosts_file_path: diagnostics.hosts_file_path.clone(),
        blocking_permissions: setup_check_output(&diagnostics.blocking_permissions),
        hosts_write_capability: setup_check_output(&diagnostics.hosts_write_capability),
        wakatime_config: setup_check_output(&diagnostics.wakatime_config),
    }
}

fn print_blocking_preview_command_output(payload: &BlockingPreviewCommandOutput) {
    println!("Hosts file: {}", payload.hosts_file_path);
    println!(
        "Preview action: {} (changes: {})",
        payload.action,
        if payload.would_change { "yes" } else { "no" }
    );
    println!(
        "Effective blocked sites: {}",
        payload.effective_blocked_sites_count
    );
    if !payload.effective_blocked_sites.is_empty() {
        println!("Sites: {}", payload.effective_blocked_sites.join(", "));
    }
    if let Some(section) = payload.section.as_deref() {
        println!("Section preview:");
        print!("{section}");
    } else {
        println!("Section preview: none");
    }
}

fn build_blocking_preview_command_output(
    preview: &crate::blocker::BlockingPreview,
) -> BlockingPreviewCommandOutput {
    let action = match preview.action {
        BlockingPreviewAction::Block => "block",
        BlockingPreviewAction::Unblock => "unblock",
        BlockingPreviewAction::NoChange => "no_change",
    };
    BlockingPreviewCommandOutput {
        hosts_file_path: preview.hosts_file_path.clone(),
        action,
        would_change: preview.would_change,
        effective_blocked_sites_count: preview.effective_blocked_sites.len(),
        effective_blocked_sites: preview.effective_blocked_sites.clone(),
        section: preview.section_for_display().map(ToString::to_string),
    }
}

fn setup_check_output(check: &SetupCheck) -> SetupCheckOutput {
    SetupCheckOutput {
        level: setup_check_level_id(check.level),
        message: check.message.clone(),
    }
}

fn setup_check_level_id(level: SetupCheckLevel) -> &'static str {
    match level {
        SetupCheckLevel::Ok => "ok",
        SetupCheckLevel::Warning => "warning",
    }
}

fn print_json<T: Serialize>(payload: &T) -> Result<(), String> {
    let json = serde_json::to_string_pretty(payload)
        .map_err(|error| format!("Failed to encode JSON output: {error}"))?;
    println!("{json}");
    Ok(())
}

fn print_json_compact<T: Serialize>(payload: &T) -> Result<(), String> {
    let json = serde_json::to_string(payload)
        .map_err(|error| format!("Failed to encode JSON output: {error}"))?;
    println!("{json}");
    Ok(())
}

fn flush_stdout() -> Result<(), String> {
    io::stdout()
        .flush()
        .map_err(|error| format!("Failed to flush stdout: {error}"))
}

fn format_duration(secs: u64) -> String {
    let minutes = secs / 60;
    let seconds = secs % 60;
    match (minutes, seconds) {
        (0, s) => format!("{s}s"),
        (m, 0) => format!("{m}m"),
        (m, s) => format!("{m}m {s}s"),
    }
}

fn display_input_value(value: &str) -> String {
    let trimmed = value.trim();
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

fn invalid_usage(message: &str) -> String {
    format!("{message}\n\n{USAGE_TEXT}")
}

fn parse_watch_interval_option(tokens: &[ParsedToken]) -> Result<Option<u64>, String> {
    let mut interval: Option<u64> = None;
    for token in tokens {
        if let ParsedToken::Watch(value) = token {
            if interval.is_some() {
                return Err(invalid_usage("`--watch` can only be specified once."));
            }
            interval = Some(value.unwrap_or(DEFAULT_WATCH_INTERVAL_SECS));
        }
    }
    Ok(interval)
}

fn parse_watch_interval_secs(value: &str) -> Result<u64, String> {
    let trimmed = value.trim();
    let secs = trimmed
        .parse::<u64>()
        .map_err(|_| invalid_usage("`--watch` requires a positive whole number of seconds."))?;
    if secs == 0 {
        return Err(invalid_usage(
            "`--watch` requires a positive whole number of seconds.",
        ));
    }
    Ok(secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_recovery::{
        self, InProgressSessionSnapshot, RecoveryTimerPhase, RecoveryTimerStatus,
    };
    use chrono::{Datelike, Duration};
    #[cfg(unix)]
    use std::os::unix::ffi::OsStringExt;

    fn parse(values: &[&str]) -> Result<CliAction, String> {
        parse_args(values.iter().map(OsString::from))
    }

    fn parse_with_contract(values: &[&str]) -> Result<CliAction, CliError> {
        parse_args_with_contract(values.iter().map(OsString::from))
    }

    #[test]
    fn parse_without_arguments_runs_default_tui() {
        let parsed = parse(&[]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunTui {
                start_immediately: false
            }
        );
    }

    #[test]
    fn parse_start_runs_tui_immediately() {
        let parsed = parse(&["--start"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunTui {
                start_immediately: true
            }
        );
    }

    #[test]
    fn parse_status_supports_json_mode() {
        let parsed = parse(&["--status", "--json"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::Status {
                    watch_interval_secs: None
                },
                output: OutputMode::Json
            })
        );
    }

    #[test]
    fn parse_status_watch_without_interval_uses_default_cadence() {
        let parsed = parse(&["--status", "--watch"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::Status {
                    watch_interval_secs: Some(DEFAULT_WATCH_INTERVAL_SECS)
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_status_watch_with_equals_interval() {
        let parsed = parse(&["--status", "--watch=3"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::Status {
                    watch_interval_secs: Some(3)
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_status_watch_with_space_interval() {
        let parsed = parse(&["--status", "--watch", "2"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::Status {
                    watch_interval_secs: Some(2)
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_pause_supports_json_mode() {
        let parsed = parse(&["--pause", "--json"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::Pause,
                output: OutputMode::Json
            })
        );
    }

    #[test]
    fn parse_task_with_equals_sets_label() {
        let parsed = parse(&["--task=Docs"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::Task {
                    label: "Docs".to_string()
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_task_with_value_sets_label() {
        let parsed = parse(&["--task", "Docs"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::Task {
                    label: "Docs".to_string()
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_task_goal_without_value_reads_selected_label_goal() {
        let parsed = parse(&["--task-goal"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::TaskGoal {
                    label: None,
                    goal: None
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_task_goal_with_label_reads_specific_goal() {
        let parsed = parse(&["--task-goal", "Docs"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::TaskGoal {
                    label: Some("Docs".to_string()),
                    goal: None
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_task_goal_with_label_and_target_sets_goal() {
        let parsed = parse(&["--task-goal", "Docs:120,4"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::TaskGoal {
                    label: Some("Docs".to_string()),
                    goal: Some(DailyGoalConfig {
                        minutes: 120,
                        pomodoros: 4
                    })
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_task_goal_with_colon_in_label_reads_specific_goal() {
        let parsed = parse(&["--task-goal", "Docs:API"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::TaskGoal {
                    label: Some("Docs:API".to_string()),
                    goal: None
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_profile_supports_json_mode() {
        let parsed = parse(&["--profile", "--json"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::Profile { profile: None },
                output: OutputMode::Json
            })
        );
    }

    #[test]
    fn parse_profile_without_value_reads_current_profile() {
        let parsed = parse(&["--profile"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::Profile { profile: None },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_profile_with_value_sets_profile() {
        let parsed = parse(&["--profile", "deep-work"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::Profile {
                    profile: Some(ProfileId::DeepWork)
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_profile_with_equals_sets_profile() {
        let parsed = parse(&["--profile=classic"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::Profile {
                    profile: Some(ProfileId::Classic)
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_goal_without_value_reads_current_goal() {
        let parsed = parse(&["--goal"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::Goal { goal: None },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_goal_with_equals_sets_goal() {
        let parsed = parse(&["--goal=120,4"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::Goal {
                    goal: Some(DailyGoalConfig {
                        minutes: 120,
                        pomodoros: 4
                    })
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_goal_with_value_sets_goal() {
        let parsed = parse(&["--goal", "45,2"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::Goal {
                    goal: Some(DailyGoalConfig {
                        minutes: 45,
                        pomodoros: 2
                    })
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_weekly_goal_without_value_reads_current_goal() {
        let parsed = parse(&["--goal-weekly"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::GoalWeekly { goal: None },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_weekly_goal_with_equals_sets_goal() {
        let parsed = parse(&["--goal-weekly=420,14"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::GoalWeekly {
                    goal: Some(WeeklyGoalConfig {
                        minutes: 420,
                        pomodoros: 14
                    })
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_monthly_goal_with_value_sets_goal() {
        let parsed = parse(&["--goal-monthly", "1800,60"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::GoalMonthly {
                    goal: Some(MonthlyGoalConfig {
                        minutes: 1800,
                        pomodoros: 60
                    })
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_strict_without_value_reads_current_state() {
        let parsed = parse(&["--strict"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::Strict { enabled: None },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_strict_with_equals_sets_state() {
        let parsed = parse(&["--strict=on"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::Strict {
                    enabled: Some(true)
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_goal_carry_without_value_reads_current_state() {
        let parsed = parse(&["--goal-carry"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::GoalCarry { enabled: None },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_goal_carry_weekly_with_equals_sets_state() {
        let parsed = parse(&["--goal-carry-weekly=on"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::GoalCarryWeekly {
                    enabled: Some(true)
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_goal_carry_monthly_with_value_sets_state() {
        let parsed = parse(&["--goal-carry-monthly", "off"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::GoalCarryMonthly {
                    enabled: Some(false)
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_schedule_reads_current_schedule() {
        let parsed = parse(&["--schedule"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::Schedule { schedule: None },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_schedule_set_accepts_json_payload() {
        let payload = r#"{"windows":[{"days":["mon","wed"],"start":"09:00","end":"11:00"}],"exception_dates":["2026-12-25"]}"#;
        let parsed =
            parse_args([OsString::from("--schedule-set"), OsString::from(payload)]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::Schedule {
                    schedule: Some(RecurringScheduleConfig {
                        windows: vec![RecurringFocusWindowConfig {
                            days: vec!["mon".to_string(), "wed".to_string()],
                            start: "09:00".to_string(),
                            end: "11:00".to_string(),
                        }],
                        exception_dates: vec!["2026-12-25".to_string()],
                        one_time_windows: Vec::new(),
                    }),
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_schedule_set_accepts_one_time_windows_payload() {
        let payload = r#"{"windows":[],"exception_dates":[],"one_time_windows":[{"date":"2026-05-02","start":"14:00","end":"15:30"}]}"#;
        let parsed =
            parse_args([OsString::from("--schedule-set"), OsString::from(payload)]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::Schedule {
                    schedule: Some(RecurringScheduleConfig {
                        windows: Vec::new(),
                        exception_dates: Vec::new(),
                        one_time_windows: vec![OneTimeFocusWindowConfig {
                            date: "2026-05-02".to_string(),
                            start: "14:00".to_string(),
                            end: "15:30".to_string(),
                        }],
                    }),
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn schedule_inspection_output_reports_detected_conflicts() {
        let schedule = RecurringScheduleConfig {
            windows: vec![
                RecurringFocusWindowConfig {
                    days: vec!["mon".to_string()],
                    start: "09:00".to_string(),
                    end: "11:00".to_string(),
                },
                RecurringFocusWindowConfig {
                    days: vec!["mon".to_string()],
                    start: "10:30".to_string(),
                    end: "12:00".to_string(),
                },
            ],
            ..RecurringScheduleConfig::default()
        };

        let output = build_schedule_inspection_output(&schedule);

        assert_eq!(output.conflict_count, 1);
        assert_eq!(output.conflicts.len(), 1);
        assert!(output.conflicts[0].contains("recurring #1 overlaps recurring #2"));
    }

    #[test]
    fn schedule_inspection_output_reports_no_conflicts() {
        let schedule = RecurringScheduleConfig {
            windows: vec![RecurringFocusWindowConfig {
                days: vec!["mon".to_string()],
                start: "09:00".to_string(),
                end: "10:00".to_string(),
            }],
            ..RecurringScheduleConfig::default()
        };

        let output = build_schedule_inspection_output(&schedule);

        assert_eq!(output.conflict_count, 0);
        assert!(output.conflicts.is_empty());
    }

    #[test]
    fn parse_diagnostics_supports_json_mode() {
        let parsed = parse(&["--diagnostics", "--json"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::Diagnostics,
                output: OutputMode::Json
            })
        );
    }

    #[test]
    fn parse_blocking_preview_supports_json_mode() {
        let parsed = parse(&["--blocking-preview", "--json"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::BlockingPreview,
                output: OutputMode::Json
            })
        );
    }

    #[test]
    fn parse_export_accepts_optional_directory() {
        let parsed = parse(&["--export", "reports"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::Export {
                    dir: Some(PathBuf::from("reports"))
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_export_with_equals_accepts_directory() {
        let parsed = parse(&["--export=reports"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::Export {
                    dir: Some(PathBuf::from("reports"))
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_blocklist_profile_without_value_reads_current_profile() {
        let parsed = parse(&["--blocklist-profile"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::BlocklistProfile {
                    command: BlocklistProfileCommandKind::Select { profile: None }
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_blocklist_profile_with_value_selects_profile() {
        let parsed = parse(&["--blocklist-profile", "Work"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::BlocklistProfile {
                    command: BlocklistProfileCommandKind::Select {
                        profile: Some("Work".to_string())
                    }
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_blocklist_profile_rename_with_equals() {
        let parsed = parse(&["--blocklist-profile-rename=Deep Work"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::BlocklistProfile {
                    command: BlocklistProfileCommandKind::Rename {
                        name: "Deep Work".to_string()
                    }
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_blocklist_profile_delete_runs_command() {
        let parsed = parse(&["--blocklist-profile-delete"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::BlocklistProfile {
                    command: BlocklistProfileCommandKind::Delete
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_blocklist_site_add_with_equals() {
        let parsed = parse(&["--blocklist-site-add=github.com,news.ycombinator.com"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::BlocklistSites {
                    target: SiteListTarget::Blocklist,
                    command: BlocklistSiteCommandKind::Add {
                        input: "github.com,news.ycombinator.com".to_string()
                    }
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_allowlist_site_delete_with_equals() {
        let parsed = parse(&["--allowlist-site-delete=reddit.com"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::BlocklistSites {
                    target: SiteListTarget::Allowlist,
                    command: BlocklistSiteCommandKind::Delete {
                        site: "reddit.com".to_string()
                    }
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn parse_allowlist_site_edit_with_equals() {
        let parsed = parse(&["--allowlist-site-edit=old.com=new.com"]).unwrap();
        assert_eq!(
            parsed,
            CliAction::RunCommand(CliCommand {
                kind: CommandKind::BlocklistSites {
                    target: SiteListTarget::Allowlist,
                    command: BlocklistSiteCommandKind::Edit {
                        value: SiteEditValue {
                            previous: "old.com".to_string(),
                            next: "new.com".to_string()
                        }
                    }
                },
                output: OutputMode::Text
            })
        );
    }

    #[test]
    fn classify_key_value_arg_accepts_profile_equals_value() {
        let parsed = classify_key_value_arg("--profile=deep-work").unwrap();
        assert_eq!(
            parsed,
            Some(ParsedToken::Profile(Some(ProfileId::DeepWork)))
        );
    }

    #[test]
    fn classify_key_value_arg_accepts_task_equals_value() {
        let parsed = classify_key_value_arg("--task=Docs").unwrap();
        assert_eq!(parsed, Some(ParsedToken::Task("Docs".to_string())));
    }

    #[test]
    fn classify_key_value_arg_accepts_task_goal_equals_label() {
        let parsed = classify_key_value_arg("--task-goal=Docs").unwrap();
        assert_eq!(
            parsed,
            Some(ParsedToken::TaskGoal {
                label: Some("Docs".to_string()),
                goal: None
            })
        );
    }

    #[test]
    fn classify_key_value_arg_accepts_task_goal_equals_label_and_target() {
        let parsed = classify_key_value_arg("--task-goal=Docs:90,3").unwrap();
        assert_eq!(
            parsed,
            Some(ParsedToken::TaskGoal {
                label: Some("Docs".to_string()),
                goal: Some(DailyGoalConfig {
                    minutes: 90,
                    pomodoros: 3
                })
            })
        );
    }

    #[test]
    fn classify_key_value_arg_rejects_empty_task_equals_value() {
        let error = classify_key_value_arg("--task=").unwrap_err();
        assert!(error.contains("`--task=` requires a task label."));
    }

    #[test]
    fn classify_key_value_arg_rejects_empty_profile_equals_value() {
        let error = classify_key_value_arg("--profile=").unwrap_err();
        assert!(error.contains("`--profile=` requires a profile value."));
    }

    #[test]
    fn classify_key_value_arg_accepts_export_equals_value() {
        let parsed = classify_key_value_arg("--export=reports").unwrap();
        assert_eq!(
            parsed,
            Some(ParsedToken::Export(Some(PathBuf::from("reports"))))
        );
    }

    #[test]
    fn classify_key_value_arg_accepts_goal_equals_value() {
        let parsed = classify_key_value_arg("--goal=90,3").unwrap();
        assert_eq!(
            parsed,
            Some(ParsedToken::Goal(Some(DailyGoalConfig {
                minutes: 90,
                pomodoros: 3
            })))
        );
    }

    #[test]
    fn classify_key_value_arg_accepts_weekly_goal_equals_value() {
        let parsed = classify_key_value_arg("--goal-weekly=420,14").unwrap();
        assert_eq!(
            parsed,
            Some(ParsedToken::GoalWeekly(Some(WeeklyGoalConfig {
                minutes: 420,
                pomodoros: 14
            })))
        );
    }

    #[test]
    fn classify_key_value_arg_accepts_monthly_goal_equals_value() {
        let parsed = classify_key_value_arg("--goal-monthly=1800,60").unwrap();
        assert_eq!(
            parsed,
            Some(ParsedToken::GoalMonthly(Some(MonthlyGoalConfig {
                minutes: 1800,
                pomodoros: 60
            })))
        );
    }

    #[test]
    fn classify_key_value_arg_accepts_strict_equals_value() {
        let parsed = classify_key_value_arg("--strict=off").unwrap();
        assert_eq!(parsed, Some(ParsedToken::Strict(Some(false))));
    }

    #[test]
    fn classify_key_value_arg_accepts_goal_carry_equals_value() {
        let parsed = classify_key_value_arg("--goal-carry=on").unwrap();
        assert_eq!(parsed, Some(ParsedToken::GoalCarry(Some(true))));
    }

    #[test]
    fn classify_key_value_arg_accepts_goal_carry_weekly_equals_value() {
        let parsed = classify_key_value_arg("--goal-carry-weekly=off").unwrap();
        assert_eq!(parsed, Some(ParsedToken::GoalCarryWeekly(Some(false))));
    }

    #[test]
    fn classify_key_value_arg_accepts_goal_carry_monthly_equals_value() {
        let parsed = classify_key_value_arg("--goal-carry-monthly=on").unwrap();
        assert_eq!(parsed, Some(ParsedToken::GoalCarryMonthly(Some(true))));
    }

    #[test]
    fn classify_key_value_arg_accepts_schedule_set_equals_value() {
        let payload = "--schedule-set={\"windows\":[{\"days\":[\"fri\"],\"start\":\"10:00\",\"end\":\"11:00\"}],\"exception_dates\":[]}";
        let parsed = classify_key_value_arg(payload).unwrap();
        assert_eq!(
            parsed,
            Some(ParsedToken::ScheduleSet(RecurringScheduleConfig {
                windows: vec![RecurringFocusWindowConfig {
                    days: vec!["fri".to_string()],
                    start: "10:00".to_string(),
                    end: "11:00".to_string(),
                }],
                exception_dates: Vec::new(),
                one_time_windows: Vec::new(),
            }))
        );
    }

    #[test]
    fn classify_key_value_arg_rejects_empty_export_equals_value() {
        let error = classify_key_value_arg("--export=").unwrap_err();
        assert!(error.contains("`--export=` requires a target directory."));
    }

    #[test]
    fn classify_key_value_arg_rejects_empty_goal_equals_value() {
        let error = classify_key_value_arg("--goal=").unwrap_err();
        assert!(error.contains("`--goal=` requires values"));
    }

    #[test]
    fn classify_key_value_arg_rejects_empty_weekly_goal_equals_value() {
        let error = classify_key_value_arg("--goal-weekly=").unwrap_err();
        assert!(error.contains("`--goal-weekly=` requires values"));
    }

    #[test]
    fn classify_key_value_arg_rejects_empty_monthly_goal_equals_value() {
        let error = classify_key_value_arg("--goal-monthly=").unwrap_err();
        assert!(error.contains("`--goal-monthly=` requires values"));
    }

    #[test]
    fn classify_key_value_arg_rejects_empty_strict_equals_value() {
        let error = classify_key_value_arg("--strict=").unwrap_err();
        assert!(error.contains("`--strict=` requires `on` or `off`"));
    }

    #[test]
    fn classify_key_value_arg_rejects_empty_goal_carry_equals_value() {
        let error = classify_key_value_arg("--goal-carry=").unwrap_err();
        assert!(error.contains("`--goal-carry=` requires `on` or `off`"));
    }

    #[test]
    fn classify_key_value_arg_rejects_empty_goal_carry_weekly_equals_value() {
        let error = classify_key_value_arg("--goal-carry-weekly=").unwrap_err();
        assert!(error.contains("`--goal-carry-weekly=` requires `on` or `off`"));
    }

    #[test]
    fn classify_key_value_arg_rejects_empty_goal_carry_monthly_equals_value() {
        let error = classify_key_value_arg("--goal-carry-monthly=").unwrap_err();
        assert!(error.contains("`--goal-carry-monthly=` requires `on` or `off`"));
    }

    #[test]
    fn classify_key_value_arg_rejects_empty_schedule_set_equals_value() {
        let error = classify_key_value_arg("--schedule-set=").unwrap_err();
        assert!(error.contains("`--schedule-set=` requires a JSON payload."));
    }

    #[test]
    fn parse_help_returns_show_help_action() {
        let parsed = parse(&["--help"]).unwrap();
        assert_eq!(parsed, CliAction::ShowHelp);
    }

    #[test]
    fn parse_help_short_circuits_unknown_arguments() {
        let parsed = parse(&["--help", "--unknown"]).unwrap();
        assert_eq!(parsed, CliAction::ShowHelp);
    }

    #[test]
    fn parse_rejects_task_without_value() {
        let error = parse(&["--task"]).unwrap_err();
        assert!(error.contains("`--task` requires a task label"));
    }

    #[test]
    fn parse_rejects_task_with_blank_value() {
        let error = parse(&["--task", "   "]).unwrap_err();
        assert!(error.contains("`--task` requires a task label"));
    }

    #[test]
    fn parse_rejects_task_goal_with_blank_label() {
        let error = parse(&["--task-goal=:120,4"]).unwrap_err();
        assert!(error.contains("Task goal requires a task label before `:`."));
    }

    #[test]
    fn parse_rejects_task_goal_with_non_numeric_pomodoros_suffix() {
        let error = parse(&["--task-goal=Docs:120,abc"]).unwrap_err();
        assert!(error.contains("Invalid goal pomodoros"));
    }

    #[test]
    fn parse_rejects_task_goal_with_extra_goal_components() {
        let error = parse(&["--task-goal=Docs:120,4,5"]).unwrap_err();
        assert!(error.contains("Invalid goal pomodoros"));
    }

    #[test]
    fn parse_rejects_goal_without_two_numbers() {
        let error = parse(&["--goal=120"]).unwrap_err();
        assert!(error.contains("Invalid goal"));
    }

    #[test]
    fn parse_rejects_weekly_goal_without_two_numbers() {
        let error = parse(&["--goal-weekly=120"]).unwrap_err();
        assert!(error.contains("Invalid goal"));
    }

    #[test]
    fn parse_rejects_monthly_goal_without_two_numbers() {
        let error = parse(&["--goal-monthly=120"]).unwrap_err();
        assert!(error.contains("Invalid goal"));
    }

    #[test]
    fn parse_rejects_strict_with_unknown_value() {
        let error = parse(&["--strict=enabled"]).unwrap_err();
        assert!(error.contains("Invalid strict mode"));
    }

    #[test]
    fn parse_rejects_goal_carry_with_unknown_value() {
        let error = parse(&["--goal-carry=enabled"]).unwrap_err();
        assert!(error.contains("Invalid goal carry-over"));
    }

    #[test]
    fn parse_rejects_schedule_set_without_payload() {
        let error = parse(&["--schedule-set"]).unwrap_err();
        assert!(error.contains("`--schedule-set` requires a JSON payload"));
    }

    #[test]
    fn parse_rejects_blocklist_profile_create_without_value() {
        let error = parse(&["--blocklist-profile-create"]).unwrap_err();
        assert!(error.contains("`--blocklist-profile-create` requires a profile name"));
    }

    #[test]
    fn parse_rejects_blocklist_site_edit_without_old_new_separator() {
        let error = parse(&["--blocklist-site-edit=example.com"]).unwrap_err();
        assert!(error.contains("Invalid site edit"));
    }

    #[test]
    fn parse_rejects_schedule_set_with_invalid_weekday() {
        let payload = r#"{"windows":[{"days":["nonday"],"start":"09:00","end":"10:00"}],"exception_dates":[]}"#;
        let error =
            parse_args([OsString::from("--schedule-set"), OsString::from(payload)]).unwrap_err();
        assert!(error.contains("unknown weekday"));
    }

    #[test]
    fn parse_rejects_schedule_set_with_invalid_exception_date() {
        let payload = r#"{"windows":[{"days":["mon"],"start":"09:00","end":"10:00"}],"exception_dates":["2026-99-99"]}"#;
        let error =
            parse_args([OsString::from("--schedule-set"), OsString::from(payload)]).unwrap_err();
        assert!(error.contains("must be YYYY-MM-DD"));
    }

    #[test]
    fn parse_rejects_schedule_set_with_invalid_one_time_date() {
        let payload = r#"{"windows":[],"exception_dates":[],"one_time_windows":[{"date":"2026-99-99","start":"09:00","end":"10:00"}]}"#;
        let error =
            parse_args([OsString::from("--schedule-set"), OsString::from(payload)]).unwrap_err();
        assert!(error.contains("Invalid one-time window"));
    }

    #[test]
    fn parse_rejects_multiple_primary_commands() {
        let error = parse(&["--status", "--export"]).unwrap_err();
        assert!(error.contains("Multiple primary commands"));
    }

    #[test]
    fn parse_rejects_unknown_option() {
        let error = parse(&["--unknown"]).unwrap_err();
        assert!(error.contains("Unknown option"));
    }

    #[test]
    fn parse_rejects_json_without_noninteractive_command() {
        let error = parse(&["--json"]).unwrap_err();
        assert!(error.contains("`--json` is only valid"));
    }

    #[test]
    fn parse_rejects_json_with_start() {
        let error = parse(&["--start", "--json"]).unwrap_err();
        assert!(error.contains("not supported with `--start`"));
    }

    #[test]
    fn parse_rejects_watch_without_status() {
        let error = parse(&["--watch"]).unwrap_err();
        assert!(error.contains("`--watch` is only valid with `--status`"));
    }

    #[test]
    fn parse_rejects_watch_with_non_status_command() {
        let error = parse(&["--export", "--watch"]).unwrap_err();
        assert!(error.contains("`--watch` is only valid with `--status`"));
    }

    #[test]
    fn parse_rejects_watch_with_zero_seconds() {
        let error = parse(&["--status", "--watch=0"]).unwrap_err();
        assert!(error.contains("positive whole number of seconds"));
    }

    #[test]
    fn parse_rejects_watch_with_non_numeric_seconds() {
        let error = parse(&["--status", "--watch=abc"]).unwrap_err();
        assert!(error.contains("positive whole number of seconds"));
    }

    #[test]
    fn parse_rejects_duplicate_watch_flags() {
        let error = parse(&["--status", "--watch", "--watch=2"]).unwrap_err();
        assert!(error.contains("can only be specified once"));
    }

    #[test]
    fn parse_with_contract_marks_json_usage_errors() {
        let error = parse_with_contract(&["--status", "--unknown", "--json"]).unwrap_err();
        assert_eq!(error.kind, CliErrorKind::Usage);
        assert_eq!(error.output, OutputMode::Json);
        assert_eq!(error.exit_code(), EXIT_CODE_USAGE_ERROR);
        assert!(error.message.contains("Unknown option"));
    }

    #[test]
    fn parse_with_contract_detects_json_on_early_parse_failures() {
        let error = parse_with_contract(&["--schedule-set", "--json"]).unwrap_err();
        assert_eq!(error.kind, CliErrorKind::Usage);
        assert_eq!(error.output, OutputMode::Json);
        assert_eq!(error.exit_code(), EXIT_CODE_USAGE_ERROR);
        assert!(
            error
                .message
                .contains("`--schedule-set` requires a JSON payload")
        );
    }

    #[cfg(unix)]
    #[test]
    fn parse_rejects_non_utf8_arguments() {
        let invalid = OsString::from_vec(vec![0x66, 0x6f, 0x80]);
        let error = parse_args(vec![invalid]).unwrap_err();
        assert!(error.contains("Arguments must be valid UTF-8."));
    }

    #[test]
    fn apply_blocklist_profile_select_updates_selection_case_insensitively() {
        let mut config = AppConfig {
            blocklist_profiles: vec![
                crate::config::BlocklistProfileConfig {
                    name: "Work".to_string(),
                    sites: vec!["a.com".to_string()],
                    allowlist_sites: Vec::new(),
                },
                crate::config::BlocklistProfileConfig {
                    name: "Study".to_string(),
                    sites: vec!["study.com".to_string(), "news.com".to_string()],
                    allowlist_sites: vec!["news.com".to_string()],
                },
            ],
            selected_blocklist_profile: "work".to_string(),
            ..AppConfig::default()
        }
        .normalized();

        let payload = apply_blocklist_profile_command(
            &mut config,
            BlocklistProfileCommandKind::Select {
                profile: Some("STUDY".to_string()),
            },
        )
        .unwrap();

        assert!(payload.updated);
        assert_eq!(payload.selected_blocklist_profile, "Study");
        assert_eq!(config.selected_blocklist_profile, "Study");
        assert_eq!(config.blocked_sites, vec!["study.com".to_string()]);
    }

    #[test]
    fn apply_blocklist_profile_rename_updates_selection_and_name() {
        let mut config = AppConfig {
            blocklist_profiles: vec![
                crate::config::BlocklistProfileConfig {
                    name: "Work".to_string(),
                    sites: vec!["a.com".to_string()],
                    allowlist_sites: Vec::new(),
                },
                crate::config::BlocklistProfileConfig {
                    name: "Study".to_string(),
                    sites: vec!["study.com".to_string()],
                    allowlist_sites: Vec::new(),
                },
            ],
            selected_blocklist_profile: "Work".to_string(),
            ..AppConfig::default()
        }
        .normalized();

        let payload = apply_blocklist_profile_command(
            &mut config,
            BlocklistProfileCommandKind::Rename {
                name: "Deep Work".to_string(),
            },
        )
        .unwrap();

        assert!(payload.updated);
        assert_eq!(payload.selected_blocklist_profile, "Deep Work");
        assert_eq!(config.selected_blocklist_profile, "Deep Work");
        assert_eq!(config.blocklist_profiles[0].name, "Deep Work");
        assert_eq!(config.blocked_sites, vec!["a.com".to_string()]);
    }

    #[test]
    fn apply_blocklist_profile_delete_switches_selection() {
        let mut config = AppConfig {
            blocklist_profiles: vec![
                crate::config::BlocklistProfileConfig {
                    name: "Work".to_string(),
                    sites: vec!["a.com".to_string()],
                    allowlist_sites: Vec::new(),
                },
                crate::config::BlocklistProfileConfig {
                    name: "Study".to_string(),
                    sites: vec!["study.com".to_string(), "news.com".to_string()],
                    allowlist_sites: vec!["news.com".to_string()],
                },
            ],
            selected_blocklist_profile: "Work".to_string(),
            ..AppConfig::default()
        }
        .normalized();

        let payload =
            apply_blocklist_profile_command(&mut config, BlocklistProfileCommandKind::Delete)
                .unwrap();

        assert!(payload.updated);
        assert_eq!(payload.selected_blocklist_profile, "Study");
        assert_eq!(config.selected_blocklist_profile, "Study");
        assert_eq!(config.blocklist_profiles.len(), 1);
        assert_eq!(config.blocked_sites, vec!["study.com".to_string()]);
    }

    #[test]
    fn apply_allowlist_site_add_updates_effective_blocking() {
        let mut config = AppConfig {
            blocklist_profiles: vec![crate::config::BlocklistProfileConfig {
                name: "Default".to_string(),
                sites: vec!["a.com".to_string(), "b.com".to_string()],
                allowlist_sites: vec!["b.com".to_string()],
            }],
            selected_blocklist_profile: "Default".to_string(),
            ..AppConfig::default()
        }
        .normalized();

        let payload =
            apply_site_add_command(&mut config, SiteListTarget::Allowlist, "a.com").unwrap();

        assert!(payload.updated);
        assert_eq!(payload.target, SiteListTarget::Allowlist);
        assert_eq!(
            config.blocklist_profiles[0].allowlist_sites,
            vec!["b.com".to_string(), "a.com".to_string()]
        );
        assert!(config.blocked_sites.is_empty());
        assert_eq!(payload.effective_blocked_sites_count, 0);
    }

    #[test]
    fn apply_site_edit_command_updates_blocklist_sites() {
        let mut config = AppConfig {
            blocklist_profiles: vec![crate::config::BlocklistProfileConfig {
                name: "Default".to_string(),
                sites: vec!["a.com".to_string(), "b.com".to_string()],
                allowlist_sites: Vec::new(),
            }],
            selected_blocklist_profile: "Default".to_string(),
            ..AppConfig::default()
        }
        .normalized();

        let payload = apply_site_edit_command(
            &mut config,
            SiteListTarget::Blocklist,
            &SiteEditValue {
                previous: "a.com".to_string(),
                next: "news.ycombinator.com".to_string(),
            },
        )
        .unwrap();

        assert!(payload.updated);
        assert_eq!(payload.previous, "a.com");
        assert_eq!(payload.current, "news.ycombinator.com");
        assert_eq!(
            config.blocklist_profiles[0].sites,
            vec!["news.ycombinator.com".to_string(), "b.com".to_string()]
        );
        assert_eq!(
            config.blocked_sites,
            vec!["news.ycombinator.com".to_string(), "b.com".to_string()]
        );
    }

    #[test]
    fn apply_site_edit_command_handles_duplicate_case_entries() {
        let mut config = AppConfig {
            blocklist_profiles: vec![crate::config::BlocklistProfileConfig {
                name: "Default".to_string(),
                sites: vec![
                    "A.com".to_string(),
                    "a.com".to_string(),
                    "b.com".to_string(),
                ],
                allowlist_sites: Vec::new(),
            }],
            selected_blocklist_profile: "Default".to_string(),
            ..AppConfig::default()
        };

        let payload = apply_site_edit_command(
            &mut config,
            SiteListTarget::Blocklist,
            &SiteEditValue {
                previous: "a.com".to_string(),
                next: "news.com".to_string(),
            },
        )
        .unwrap();

        assert!(payload.updated);
        assert_eq!(payload.previous, "a.com");
        assert_eq!(payload.current, "news.com");
        assert_eq!(
            config.blocklist_profiles[0].sites,
            vec!["news.com".to_string(), "b.com".to_string()]
        );
        assert_eq!(
            config.blocked_sites,
            vec!["news.com".to_string(), "b.com".to_string()]
        );
    }

    #[test]
    fn apply_site_delete_command_updates_allowlist_and_effective_blocking() {
        let mut config = AppConfig {
            blocklist_profiles: vec![crate::config::BlocklistProfileConfig {
                name: "Default".to_string(),
                sites: vec!["a.com".to_string(), "b.com".to_string()],
                allowlist_sites: vec!["b.com".to_string()],
            }],
            selected_blocklist_profile: "Default".to_string(),
            ..AppConfig::default()
        }
        .normalized();

        let payload =
            apply_site_delete_command(&mut config, SiteListTarget::Allowlist, "b.com").unwrap();

        assert!(payload.updated);
        assert_eq!(payload.removed, "b.com");
        assert!(config.blocklist_profiles[0].allowlist_sites.is_empty());
        assert_eq!(
            config.blocked_sites,
            vec!["a.com".to_string(), "b.com".to_string()]
        );
        assert_eq!(payload.effective_blocked_sites_count, 2);
    }

    #[test]
    fn apply_site_delete_command_handles_duplicate_case_entries() {
        let mut config = AppConfig {
            blocklist_profiles: vec![crate::config::BlocklistProfileConfig {
                name: "Default".to_string(),
                sites: vec![
                    "example.com".to_string(),
                    "Example.com".to_string(),
                    "other.com".to_string(),
                ],
                allowlist_sites: Vec::new(),
            }],
            selected_blocklist_profile: "Default".to_string(),
            ..AppConfig::default()
        };

        let payload =
            apply_site_delete_command(&mut config, SiteListTarget::Blocklist, " EXAMPLE.COM ")
                .unwrap();

        assert!(payload.updated);
        assert_eq!(payload.removed, "example.com");
        assert_eq!(
            config.blocklist_profiles[0].sites,
            vec!["other.com".to_string()]
        );
        assert_eq!(config.blocked_sites, vec!["other.com".to_string()]);
        assert_eq!(payload.effective_blocked_sites_count, 1);
    }

    #[test]
    fn build_status_output_matches_blocklist_profile_case_insensitively() {
        let config = AppConfig {
            blocklist_profiles: vec![crate::config::BlocklistProfileConfig {
                name: "Work".to_string(),
                sites: vec!["youtube.com".to_string(), "reddit.com".to_string()],
                allowlist_sites: Vec::new(),
            }],
            selected_blocklist_profile: "work".to_string(),
            ..AppConfig::default()
        };
        let stats = FocusStats::default();

        let output = build_status_output(&config, &stats);

        assert_eq!(output.blocked_sites_count, 2);
    }

    #[test]
    fn build_status_output_excludes_allowlist_from_blocked_sites_count() {
        let config = AppConfig {
            blocklist_profiles: vec![crate::config::BlocklistProfileConfig {
                name: "Work".to_string(),
                sites: vec!["youtube.com".to_string(), "reddit.com".to_string()],
                allowlist_sites: vec!["reddit.com".to_string()],
            }],
            selected_blocklist_profile: "Work".to_string(),
            ..AppConfig::default()
        };
        let stats = FocusStats::default();

        let output = build_status_output(&config, &stats);

        assert_eq!(output.blocked_sites_count, 1);
    }

    #[test]
    fn build_status_output_reports_daily_weekly_monthly_goal_state() {
        let mut stats = FocusStats::default();
        let today = current_day_key();
        let today_date = NaiveDate::parse_from_str(&today, "%Y-%m-%d")
            .expect("current day key should parse as a date");
        let weekly_day = (-6..=6)
            .filter(|offset| *offset != 0)
            .map(|offset| today_date + Duration::days(i64::from(offset)))
            .find(|candidate| candidate.iso_week() == today_date.iso_week())
            .expect("there should be at least one nearby day in the current ISO week");
        let monthly_day = (-31..=31)
            .filter(|offset| *offset != 0)
            .map(|offset| today_date + Duration::days(i64::from(offset)))
            .find(|candidate| {
                candidate.year() == today_date.year()
                    && candidate.month() == today_date.month()
                    && candidate.iso_week() != today_date.iso_week()
            })
            .expect("there should be at least one nearby day in the current month");
        let outside_period_day = today_date - Duration::days(40);

        let daily_snapshot = DailyGoalSnapshot {
            minutes: 30,
            pomodoros: 1,
        };
        stats.record_focus_elapsed(&today, 30 * 60, daily_snapshot);
        stats.record_completed_pomodoro(&today, daily_snapshot);
        let weekly_day_key = weekly_day.format("%Y-%m-%d").to_string();
        stats.record_focus_elapsed(&weekly_day_key, 20 * 60, daily_snapshot);
        stats.record_completed_pomodoro(&weekly_day_key, daily_snapshot);
        let monthly_day_key = monthly_day.format("%Y-%m-%d").to_string();
        stats.record_focus_elapsed(&monthly_day_key, 25 * 60, daily_snapshot);
        stats.record_completed_pomodoro(&monthly_day_key, daily_snapshot);
        let outside_period_day_key = outside_period_day.format("%Y-%m-%d").to_string();
        stats.record_focus_elapsed(&outside_period_day_key, 200 * 60, daily_snapshot);
        stats.record_completed_pomodoro(&outside_period_day_key, daily_snapshot);

        let in_period_config = AppConfig {
            daily_goal: DailyGoalConfig {
                minutes: 30,
                pomodoros: 1,
            },
            weekly_goal: WeeklyGoalConfig {
                minutes: 50,
                pomodoros: 2,
            },
            monthly_goal: MonthlyGoalConfig {
                minutes: 55,
                pomodoros: 2,
            },
            ..AppConfig::default()
        };

        let in_period_output = build_status_output(&in_period_config, &stats);

        assert!(in_period_output.goal.configured);
        assert!(in_period_output.goal.met);
        assert!(in_period_output.weekly_goal.configured);
        assert!(in_period_output.weekly_goal.met);
        assert!(in_period_output.monthly_goal.configured);
        assert!(in_period_output.monthly_goal.met);
        assert!(in_period_output.focus_score.available);
        assert_eq!(in_period_output.focus_score.completion_score_pct, Some(100));
        assert!(in_period_output.focus_score.focus_score_pct.is_some());

        let boundary_config = AppConfig {
            daily_goal: DailyGoalConfig {
                minutes: 30,
                pomodoros: 1,
            },
            weekly_goal: WeeklyGoalConfig {
                minutes: 120,
                pomodoros: 3,
            },
            monthly_goal: MonthlyGoalConfig {
                minutes: 120,
                pomodoros: 3,
            },
            ..AppConfig::default()
        };

        let boundary_output = build_status_output(&boundary_config, &stats);

        assert!(boundary_output.goal.met);
        assert!(!boundary_output.weekly_goal.met);
        assert!(!boundary_output.monthly_goal.met);
        assert!(boundary_output.focus_score.available);
    }

    #[test]
    fn build_status_output_daily_goal_uses_persisted_same_day_snapshot() {
        let mut stats = FocusStats::default();
        let today = current_day_key();
        let persisted_snapshot = DailyGoalSnapshot {
            minutes: 30,
            pomodoros: 1,
        };
        stats.record_focus_elapsed(&today, 30 * 60, persisted_snapshot);
        stats.record_completed_pomodoro(&today, persisted_snapshot);

        let config = AppConfig {
            daily_goal: DailyGoalConfig {
                minutes: 120,
                pomodoros: 4,
            },
            ..AppConfig::default()
        };

        let output = build_status_output(&config, &stats);

        assert_eq!(output.goal.minutes_target, 30);
        assert_eq!(output.goal.pomodoros_target, 1);
        assert!(output.goal.met);
    }

    #[test]
    fn build_status_output_includes_unconfigured_selected_task_goal() {
        let mut stats = FocusStats::default();
        let changed =
            stats.update_task_planner_state(vec!["Docs".to_string()], Some("Docs".to_string()));
        assert!(changed);
        let config = AppConfig::default();

        let output = build_status_output(&config, &stats);
        let selected_task_goal = output
            .selected_task_goal
            .expect("selected task goal should exist when a task is selected");

        assert_eq!(selected_task_goal.task_label, "Docs");
        assert!(!selected_task_goal.configured);
        assert_eq!(selected_task_goal.minutes_target, 0);
        assert_eq!(selected_task_goal.pomodoros_target, 0);
        assert_eq!(selected_task_goal.focused_minutes, 0);
        assert_eq!(selected_task_goal.pomodoros_completed, 0);
        assert!(!selected_task_goal.met);
        assert!(!output.focus_score.available);
        assert!(output.focus_score.focus_score_pct.is_none());
    }

    #[test]
    fn build_status_output_applies_carry_over_to_goal_targets_when_enabled() {
        let mut stats = FocusStats::default();
        let today = current_day_key();
        let today_date = NaiveDate::parse_from_str(&today, "%Y-%m-%d")
            .expect("current day key should parse as a date");
        let yesterday = today_date.pred_opt().expect("yesterday should exist");
        let yesterday_key = yesterday.format("%Y-%m-%d").to_string();
        let month_start = NaiveDate::from_ymd_opt(today_date.year(), today_date.month(), 1)
            .expect("month start should be representable");
        let previous_month_day = month_start
            .pred_opt()
            .expect("previous month day should be representable");
        let previous_month_key = previous_month_day.format("%Y-%m-%d").to_string();
        let previous_daily_goal = DailyGoalSnapshot {
            minutes: 50,
            pomodoros: 3,
        };

        stats.record_focus_elapsed(&yesterday_key, 30 * 60, previous_daily_goal);
        stats.record_completed_pomodoro(&yesterday_key, previous_daily_goal);

        let base_daily_goal = DailyGoalSnapshot {
            minutes: 60,
            pomodoros: 2,
        };
        stats.record_focus_elapsed(&today, 40 * 60, base_daily_goal);
        stats.record_completed_pomodoro(&today, base_daily_goal);

        let config = AppConfig {
            daily_goal: DailyGoalConfig {
                minutes: 60,
                pomodoros: 2,
            },
            weekly_goal: WeeklyGoalConfig {
                minutes: 100,
                pomodoros: 3,
            },
            monthly_goal: MonthlyGoalConfig {
                minutes: 300,
                pomodoros: 10,
            },
            goal_carry_over: crate::config::GoalCarryOverConfig {
                daily: true,
                weekly: true,
                monthly: true,
            },
            ..AppConfig::default()
        };

        let output = build_status_output(&config, &stats);
        assert_eq!(output.goal.minutes_target, 80);
        assert_eq!(output.goal.pomodoros_target, 4);
        assert!(output.goal.carry_over);

        stats.sync_monthly_goal_snapshot(
            previous_month_day,
            DailyGoalSnapshot {
                minutes: 200,
                pomodoros: 6,
            },
        );
        stats.record_focus_elapsed(&previous_month_key, 120 * 60, base_daily_goal);
        for _ in 0..4 {
            stats.record_completed_pomodoro(&previous_month_key, base_daily_goal);
        }
        let output = build_status_output(&config, &stats);
        assert_eq!(output.monthly_goal.minutes_target, 380);
        assert_eq!(output.monthly_goal.pomodoros_target, 12);
        assert!(output.monthly_goal.carry_over);
    }

    #[test]
    fn build_status_output_daily_carry_over_does_not_reapply_older_day_debt() {
        let mut stats = FocusStats::default();
        let today = current_day_key();
        let today_date = NaiveDate::parse_from_str(&today, "%Y-%m-%d")
            .expect("current day key should parse as a date");
        let yesterday = today_date.pred_opt().expect("yesterday should exist");
        let yesterday_key = yesterday.format("%Y-%m-%d").to_string();
        let day_before = yesterday.pred_opt().expect("day before should exist");
        let day_before_key = day_before.format("%Y-%m-%d").to_string();

        stats.record_focus_elapsed(
            &day_before_key,
            30 * 60,
            DailyGoalSnapshot {
                minutes: 50,
                pomodoros: 2,
            },
        );
        stats.record_completed_pomodoro(
            &day_before_key,
            DailyGoalSnapshot {
                minutes: 50,
                pomodoros: 2,
            },
        );
        stats.insert_daily_for_tests(
            &yesterday_key,
            crate::stats::DailyStats {
                pomodoros_completed: 0,
                focused_seconds: 0,
                goal: Some(DailyGoalSnapshot {
                    minutes: 60,
                    pomodoros: 2,
                }),
            },
        );

        let config = AppConfig {
            daily_goal: DailyGoalConfig {
                minutes: 60,
                pomodoros: 2,
            },
            goal_carry_over: crate::config::GoalCarryOverConfig {
                daily: true,
                ..crate::config::GoalCarryOverConfig::default()
            },
            ..AppConfig::default()
        };

        let output = build_status_output(&config, &stats);
        assert_eq!(output.goal.minutes_target, 120);
        assert_eq!(output.goal.pomodoros_target, 4);
    }

    #[test]
    fn build_status_output_daily_carry_over_skips_when_previous_day_goal_is_absent() {
        let mut stats = FocusStats::default();
        let today = current_day_key();
        let today_date = NaiveDate::parse_from_str(&today, "%Y-%m-%d")
            .expect("current day key should parse as a date");
        let yesterday = today_date.pred_opt().expect("yesterday should exist");
        let yesterday_key = yesterday.format("%Y-%m-%d").to_string();
        stats.insert_daily_for_tests(
            &yesterday_key,
            crate::stats::DailyStats {
                pomodoros_completed: 4,
                focused_seconds: 120 * 60,
                goal: None,
            },
        );

        let config = AppConfig {
            daily_goal: DailyGoalConfig {
                minutes: 60,
                pomodoros: 2,
            },
            goal_carry_over: crate::config::GoalCarryOverConfig {
                daily: true,
                ..crate::config::GoalCarryOverConfig::default()
            },
            ..AppConfig::default()
        };

        let output = build_status_output(&config, &stats);
        assert_eq!(output.goal.minutes_target, 60);
        assert_eq!(output.goal.pomodoros_target, 2);
    }

    #[test]
    fn build_status_output_applies_weekly_carry_over_to_goal_targets_when_enabled() {
        let mut stats = FocusStats::default();
        let today = current_day_key();
        let today_date = NaiveDate::parse_from_str(&today, "%Y-%m-%d")
            .expect("current day key should parse as a date");
        let previous_week_day = today_date - Duration::days(7);
        let previous_week_key = previous_week_day.format("%Y-%m-%d").to_string();
        let goal = DailyGoalSnapshot {
            minutes: 60,
            pomodoros: 2,
        };
        stats.sync_weekly_goal_snapshot(
            previous_week_day,
            DailyGoalSnapshot {
                minutes: 50,
                pomodoros: 2,
            },
        );
        stats.record_focus_elapsed(&previous_week_key, 20 * 60, goal);
        stats.record_completed_pomodoro(&previous_week_key, goal);
        stats.record_focus_elapsed(&today, 40 * 60, goal);
        stats.record_completed_pomodoro(&today, goal);

        let config = AppConfig {
            weekly_goal: WeeklyGoalConfig {
                minutes: 100,
                pomodoros: 3,
            },
            goal_carry_over: crate::config::GoalCarryOverConfig {
                weekly: true,
                ..crate::config::GoalCarryOverConfig::default()
            },
            ..AppConfig::default()
        };

        let output = build_status_output(&config, &stats);
        assert_eq!(output.weekly_goal.minutes_target, 130);
        assert_eq!(output.weekly_goal.pomodoros_target, 4);
        assert!(output.weekly_goal.carry_over);
    }

    #[test]
    fn build_status_output_weekly_carry_over_skips_when_previous_period_has_no_snapshot() {
        let mut stats = FocusStats::default();
        let today = current_day_key();
        let today_date = NaiveDate::parse_from_str(&today, "%Y-%m-%d")
            .expect("current day key should parse as a date");
        let previous_week_day = today_date - Duration::days(7);
        let previous_week_key = previous_week_day.format("%Y-%m-%d").to_string();
        let goal = DailyGoalSnapshot {
            minutes: 60,
            pomodoros: 2,
        };
        stats.record_focus_elapsed(&previous_week_key, 70 * 60, goal);
        stats.record_completed_pomodoro(&previous_week_key, goal);

        let config = AppConfig {
            weekly_goal: WeeklyGoalConfig {
                minutes: 100,
                pomodoros: 3,
            },
            goal_carry_over: crate::config::GoalCarryOverConfig {
                weekly: true,
                ..crate::config::GoalCarryOverConfig::default()
            },
            ..AppConfig::default()
        };

        let output = build_status_output(&config, &stats);
        assert_eq!(output.weekly_goal.minutes_target, 100);
        assert_eq!(output.weekly_goal.pomodoros_target, 3);
    }

    #[test]
    fn build_status_output_monthly_carry_over_uses_previous_snapshot_after_goal_change() {
        let mut stats = FocusStats::default();
        let today = current_day_key();
        let today_date = NaiveDate::parse_from_str(&today, "%Y-%m-%d")
            .expect("current day key should parse as a date");
        let month_start = NaiveDate::from_ymd_opt(today_date.year(), today_date.month(), 1)
            .expect("month start should be representable");
        let previous_month_day = month_start
            .pred_opt()
            .expect("previous month day should be representable");
        let previous_month_key = previous_month_day.format("%Y-%m-%d").to_string();
        let goal = DailyGoalSnapshot {
            minutes: 60,
            pomodoros: 2,
        };
        stats.sync_monthly_goal_snapshot(
            previous_month_day,
            DailyGoalSnapshot {
                minutes: 200,
                pomodoros: 6,
            },
        );
        stats.record_focus_elapsed(&previous_month_key, 120 * 60, goal);
        for _ in 0..4 {
            stats.record_completed_pomodoro(&previous_month_key, goal);
        }

        let config = AppConfig {
            monthly_goal: MonthlyGoalConfig {
                minutes: 300,
                pomodoros: 10,
            },
            goal_carry_over: crate::config::GoalCarryOverConfig {
                monthly: true,
                ..crate::config::GoalCarryOverConfig::default()
            },
            ..AppConfig::default()
        };

        let output = build_status_output(&config, &stats);
        assert_eq!(output.monthly_goal.minutes_target, 380);
        assert_eq!(output.monthly_goal.pomodoros_target, 12);
    }

    #[test]
    fn build_status_output_uses_recovery_snapshot_for_live_state() {
        session_recovery::set_test_load_snapshot(Some(InProgressSessionSnapshot {
            phase: RecoveryTimerPhase::Focus,
            status: RecoveryTimerStatus::Running,
            remaining_secs: 42,
            pomodoros_completed: 3,
            selected_task_label: Some("Docs".to_string()),
            focus_intention: Some("Write docs".to_string()),
            task_note: Some("API section".to_string()),
            selected_profile: ProfileId::DeepWork,
        }));
        let config = AppConfig::default();
        let stats = FocusStats::default();

        let output = build_status_output(&config, &stats);

        assert!(output.live.in_progress);
        assert_eq!(output.live.state_source, "recovery");
        assert_eq!(output.live.phase, "focus");
        assert_eq!(output.live.status, "running");
        assert_eq!(output.live.remaining_secs, 42);
        assert_eq!(output.live.pomodoros_completed, 3);
        assert_eq!(output.live.selected_task_label.as_deref(), Some("Docs"));
        assert_eq!(output.live.focus_intention.as_deref(), Some("Write docs"));
        assert_eq!(output.live.task_note.as_deref(), Some("API section"));
        assert_eq!(output.live.selected_profile.id, "deep-work");
        assert!(output.live.recovery_error.is_none());
    }

    #[test]
    fn build_status_output_includes_latest_session_interruption() {
        let mut stats = FocusStats::default();
        stats.record_session_interruption_event(
            "2026-04-09",
            1_711_000_123,
            crate::stats::SessionInterruptionReason::ManualSkip,
            crate::stats::FocusSessionMetadata {
                task_label: Some("Docs"),
                focus_intention: Some("Write API docs"),
                task_note: Some("Skipped due urgent review"),
            },
            600,
            Some(ProfileId::Classic),
        );

        let output = build_status_output(&AppConfig::default(), &stats);
        let interruption = output
            .latest_interruption
            .expect("latest interruption should be present");
        assert_eq!(
            interruption.reason,
            crate::stats::SessionInterruptionReason::ManualSkip
        );
        assert_eq!(interruption.task_label.as_deref(), Some("Docs"));
        assert_eq!(interruption.remaining_secs, 600);
    }

    #[test]
    fn build_status_output_reports_recovery_error_without_failing() {
        session_recovery::set_test_load_error("simulated load failure");
        let config = AppConfig::default();
        let stats = FocusStats::default();

        let output = build_status_output(&config, &stats);

        assert!(!output.live.in_progress);
        assert_eq!(output.live.state_source, "default");
        assert_eq!(
            output.live.recovery_error.as_deref(),
            Some("simulated load failure")
        );
        assert_eq!(output.live.status, "idle");
    }

    #[test]
    fn cli_resume_requires_paused_timer() {
        let mut app = App::default();
        let error = app.resume_for_cli().unwrap_err();
        assert_eq!(error, "Cannot resume: timer is not paused.");
    }

    #[test]
    fn cli_stop_requires_non_idle_timer() {
        let mut app = App::default();
        let error = app.stop_for_cli().unwrap_err();
        assert_eq!(error, "Cannot stop: timer is already idle.");
    }
}
