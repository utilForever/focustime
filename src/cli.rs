use std::{env, ffi::OsString, path::PathBuf};

use serde::Serialize;

use crate::config::{AppConfig, CustomProfileConfig, ProfileId};
use crate::stats::{DailyGoalSnapshot, FocusStats, current_day_key};
use crate::timer::{
    DEFAULT_FOCUS_SECS, DEFAULT_LONG_BREAK_INTERVAL, DEFAULT_LONG_BREAK_SECS,
    DEFAULT_SHORT_BREAK_SECS,
};

const USAGE_TEXT: &str = r#"Usage:
  focustime
  focustime --start
  focustime --profile [classic|deep-work|custom] [--json]
  focustime --status [--json]
  focustime --export[=DIR] [--json]

Options:
  --start    Launch TUI with focus timer already started
  --profile  Show current profile, or set it when value is provided
  --status   Print persisted status summary
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
    Profile(Option<ProfileId>),
    Status,
    Export(Option<PathBuf>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ParsedToken {
    Help,
    Json,
    Start,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ExportOutput {
    export_dir: PathBuf,
    json_path: PathBuf,
    csv_path: PathBuf,
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
        .map(|arg| arg.to_string_lossy().to_string())
        .collect();
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
        "--status" => Some(ParsedToken::Status),
        _ => None,
    }
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
    let mut show_help = false;
    let mut output = OutputMode::Text;
    for token in tokens {
        match token {
            ParsedToken::Help => show_help = true,
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
                    "`--json` is only valid with `--status`, `--profile`, or `--export`.",
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
        CommandKind::Profile { profile } => execute_profile_command(profile, command.output),
        CommandKind::Status => execute_status_command(command.output),
        CommandKind::Export { dir } => execute_export_command(dir, command.output),
    }
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
        .map(|profile| profile.sites.len())
        .unwrap_or_default();

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
    fn parse_help_returns_show_help_action() {
        let parsed = parse(&["--help"]).unwrap();
        assert_eq!(parsed, CliAction::ShowHelp);
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
    fn build_status_output_matches_blocklist_profile_case_insensitively() {
        let config = AppConfig {
            blocklist_profiles: vec![crate::config::BlocklistProfileConfig {
                name: "Work".to_string(),
                sites: vec!["youtube.com".to_string(), "reddit.com".to_string()],
            }],
            selected_blocklist_profile: "work".to_string(),
            ..AppConfig::default()
        };
        let stats = FocusStats::default();

        let output = build_status_output(&config, &stats);

        assert_eq!(output.blocked_sites_count, 2);
    }
}
