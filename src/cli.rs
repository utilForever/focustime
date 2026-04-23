use std::{collections::HashSet, env, ffi::OsString, path::PathBuf};

use serde::Serialize;

use crate::app::App;
use crate::config::{AppConfig, CustomProfileConfig, ProfileId};
use crate::session_recovery;
use crate::stats::{DailyGoalSnapshot, FocusStats, current_day_key};
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
  focustime --profile [classic|deep-work|custom] [--json]
  focustime --status [--json]
  focustime --export[=DIR] [--json]

Options:
  --start    Launch TUI with focus timer already started
  --pause    Pause a running timer
  --resume   Resume a paused timer
  --stop     Stop/reset the current phase
  --next     Skip to the next phase
  --task     Select task label (auto-creates unknown labels)
  --profile  Show current profile, or set it when value is provided
  --status   Print status summary (includes live timer/session fields)
  --export   Export stats to current directory or DIR
  --json     Emit machine-readable JSON output
  -h, --help Show this help"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Text,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandKind {
    Pause,
    Resume,
    Stop,
    Next,
    Task { label: String },
    Profile { profile: Option<ProfileId> },
    Status,
    Export { dir: Option<PathBuf> },
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
    Profile(Option<ProfileId>),
    Status,
    Export(Option<PathBuf>),
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
    Status,
    Profile(Option<ProfileId>),
    Export(Option<PathBuf>),
    UnknownOption(String),
    Positional(String),
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
struct StatusOutput {
    day: String,
    selected_profile: ProfileView,
    selected_task_label: Option<String>,
    selected_blocklist_profile: String,
    blocked_sites_count: usize,
    strict_mode: bool,
    goal: GoalOutput,
    session: SessionOutput,
    today: TodayOutput,
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

pub fn usage_text() -> &'static str {
    USAGE_TEXT
}

pub fn parse_args<I>(args: I) -> Result<CliAction, String>
where
    I: IntoIterator<Item = OsString>,
{
    let args: Vec<String> = args
        .into_iter()
        .map(|arg| {
            arg.into_string()
                .map_err(|_| invalid_usage("Arguments must be valid UTF-8."))
        })
        .collect::<Result<_, _>>()?;
    let tokens = classify_args(&args)?;
    let (show_help, output) = parse_global_tokens(&tokens)?;
    let primary = parse_primary_command(&tokens)?;
    finalize_cli_action(show_help, output, primary)
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
    if arg == "--task" {
        return classify_task_arg(args, index);
    }
    if arg == "--profile" {
        return classify_profile_arg(args, index);
    }
    if arg == "--export" {
        return classify_export_arg(args, index);
    }
    if let Some(token) = classify_key_value_arg(arg)? {
        return Ok((token, 1));
    }
    if arg.starts_with('-') {
        return Ok((ParsedToken::UnknownOption(arg.clone()), 1));
    }
    Ok((ParsedToken::Positional(arg.clone()), 1))
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

fn classify_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--task=") {
        if value.trim().is_empty() {
            return Err(invalid_usage("`--task=` requires a task label."));
        }
        return Ok(Some(ParsedToken::Task(value.to_string())));
    }
    if let Some(value) = arg.strip_prefix("--profile=") {
        if value.trim().is_empty() {
            return Err(invalid_usage("`--profile=` requires a profile value."));
        }
        return Ok(Some(ParsedToken::Profile(Some(parse_profile_id(value)?))));
    }
    if let Some(value) = arg.strip_prefix("--export=") {
        if value.trim().is_empty() {
            return Err(invalid_usage("`--export=` requires a target directory."));
        }
        return Ok(Some(ParsedToken::Export(Some(PathBuf::from(value)))));
    }
    Ok(None)
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
            | ParsedToken::Status
            | ParsedToken::Profile(_)
            | ParsedToken::Export(_) => {}
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
            ParsedToken::Status => set_primary_command(&mut primary, PrimaryCommand::Status)?,
            ParsedToken::Profile(profile) => {
                set_primary_command(&mut primary, PrimaryCommand::Profile(*profile))?
            }
            ParsedToken::Export(dir) => {
                set_primary_command(&mut primary, PrimaryCommand::Export(dir.clone()))?
            }
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
) -> Result<CliAction, String> {
    if show_help {
        return Ok(CliAction::ShowHelp);
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
        Some(PrimaryCommand::Status) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::Status,
            output,
        })),
        Some(PrimaryCommand::Export(dir)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::Export { dir },
            output,
        })),
    }
}

pub fn execute_command(command: CliCommand) -> Result<(), String> {
    match command.kind {
        CommandKind::Pause => execute_pause_command(command.output),
        CommandKind::Resume => execute_resume_command(command.output),
        CommandKind::Stop => execute_stop_command(command.output),
        CommandKind::Next => execute_next_command(command.output),
        CommandKind::Task { label } => execute_task_command(label, command.output),
        CommandKind::Profile { profile } => execute_profile_command(profile, command.output),
        CommandKind::Status => execute_status_command(command.output),
        CommandKind::Export { dir } => execute_export_command(dir, command.output),
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

fn execute_profile_command(profile: Option<ProfileId>, output: OutputMode) -> Result<(), String> {
    let mut config = AppConfig::load().normalized();
    let mut updated = false;
    if let Some(profile) = profile {
        config.selected_profile = profile;
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

fn execute_status_command(output: OutputMode) -> Result<(), String> {
    let config = AppConfig::load().normalized();
    let stats = FocusStats::load().map_err(|error| format!("Failed to load stats: {error}"))?;
    let payload = build_status_output(&config, &stats);

    match output {
        OutputMode::Text => print_status_output(&payload),
        OutputMode::Json => print_json(&payload)?,
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
        selected_task_label: app.selected_task_label_for_cli(),
    }
}

fn build_status_output(config: &AppConfig, stats: &FocusStats) -> StatusOutput {
    let day = current_day_key();
    let today = stats.daily_for(&day);
    let session = stats.session();
    let (_, selected_task_label) = stats.task_planner_state();
    let goal_snapshot = DailyGoalSnapshot {
        minutes: config.daily_goal.minutes,
        pomodoros: config.daily_goal.pomodoros,
    };
    let active_sites_count = config
        .blocklist_profiles
        .iter()
        .find(|profile| {
            profile
                .name
                .eq_ignore_ascii_case(&config.selected_blocklist_profile)
        })
        .map(|profile| {
            let allowlist: HashSet<String> = profile
                .allowlist_sites
                .iter()
                .map(|site| site.to_ascii_lowercase())
                .collect();
            profile
                .sites
                .iter()
                .filter(|site| !allowlist.contains(&site.to_ascii_lowercase()))
                .count()
        })
        .unwrap_or_default();
    let live = build_live_status_output(config, selected_task_label.clone());

    StatusOutput {
        day,
        selected_profile: profile_view(config.selected_profile, &config.effective_custom_profile()),
        selected_task_label,
        selected_blocklist_profile: config.selected_blocklist_profile.clone(),
        blocked_sites_count: active_sites_count,
        strict_mode: config.strict_mode,
        goal: GoalOutput {
            configured: goal_snapshot.has_any_target(),
            minutes_target: goal_snapshot.minutes,
            pomodoros_target: goal_snapshot.pomodoros,
            met: goal_snapshot.is_met_by(today),
        },
        session: SessionOutput {
            focused_minutes: session.focused_minutes(),
            pomodoros_completed: session.pomodoros_completed,
        },
        today: TodayOutput {
            focused_minutes: today.focused_minutes(),
            pomodoros_completed: today.pomodoros_completed,
        },
        live,
    }
}

fn build_live_status_output(
    config: &AppConfig,
    fallback_task_label: Option<String>,
) -> LiveStatusOutput {
    let custom = config.effective_custom_profile();
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
                selected_task_label: fallback_task_label,
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
                strict_mode_enforced: false,
            }
        }
    }
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
        PrimaryCommand::Profile(_) => "--profile",
        PrimaryCommand::Status => "--status",
        PrimaryCommand::Export(_) => "--export",
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
    if payload.goal.configured {
        println!(
            "Goal: {} min, {} pomodoros ({})",
            payload.goal.minutes_target,
            payload.goal.pomodoros_target,
            if payload.goal.met {
                "met"
            } else {
                "in progress"
            }
        );
    } else {
        println!("Goal: off");
    }
    println!(
        "Session: {} focused minutes, {} pomodoros",
        payload.session.focused_minutes, payload.session.pomodoros_completed
    );
    println!(
        "Live timer: {} {} ({} remaining, source: {})",
        payload.live.phase,
        payload.live.status,
        format_duration(payload.live.remaining_secs),
        payload.live.state_source
    );
    if let Some(error) = payload.live.recovery_error.as_deref() {
        println!("Live timer warning: {error}");
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
        "Profile: {} ({})",
        timer.selected_profile.label, timer.selected_profile.id
    );
}

fn print_export_output(payload: &ExportOutput) {
    println!("Exported stats to {}", payload.export_dir.display());
    println!("JSON: {}", payload.json_path.display());
    println!("CSV: {}", payload.csv_path.display());
}

fn print_json<T: Serialize>(payload: &T) -> Result<(), String> {
    let json = serde_json::to_string_pretty(payload)
        .map_err(|error| format!("Failed to encode JSON output: {error}"))?;
    println!("{json}");
    Ok(())
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

fn invalid_usage(message: &str) -> String {
    format!("{message}\n\n{USAGE_TEXT}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_recovery::{
        self, InProgressSessionSnapshot, RecoveryTimerPhase, RecoveryTimerStatus,
    };
    #[cfg(unix)]
    use std::os::unix::ffi::OsStringExt;

    fn parse(values: &[&str]) -> Result<CliAction, String> {
        parse_args(values.iter().map(OsString::from))
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
                kind: CommandKind::Status,
                output: OutputMode::Json
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
    fn classify_key_value_arg_rejects_empty_export_equals_value() {
        let error = classify_key_value_arg("--export=").unwrap_err();
        assert!(error.contains("`--export=` requires a target directory."));
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

    #[cfg(unix)]
    #[test]
    fn parse_rejects_non_utf8_arguments() {
        let invalid = OsString::from_vec(vec![0x66, 0x6f, 0x80]);
        let error = parse_args(vec![invalid]).unwrap_err();
        assert!(error.contains("Arguments must be valid UTF-8."));
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
    fn build_status_output_uses_recovery_snapshot_for_live_state() {
        session_recovery::set_test_load_snapshot(Some(InProgressSessionSnapshot {
            phase: RecoveryTimerPhase::Focus,
            status: RecoveryTimerStatus::Running,
            remaining_secs: 42,
            pomodoros_completed: 3,
            selected_task_label: Some("Docs".to_string()),
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
        assert_eq!(output.live.selected_profile.id, "deep-work");
        assert!(output.live.recovery_error.is_none());
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
