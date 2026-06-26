mod options;
mod value;

pub(super) use options::{
    parse_watch_interval_option, parse_watch_interval_secs, require_nonempty_key_value,
};
pub(super) use value::{
    parse_goal_carry_value, parse_goal_value, parse_monthly_goal_value, parse_profile_id,
    parse_schedule_value, parse_site_edit_value, parse_strict_value, parse_task_goal_value,
    parse_theme_preset, parse_weekly_goal_value,
};

use crate::cli::{
    BlocklistProfileCommandKind, BlocklistSiteCommandKind, CliAction, CliCommand, CommandKind,
    HistoryDashboardCommandKind, OutputMode, ParsedToken, PrimaryCommand, SiteListTarget,
    USAGE_TEXT,
};

pub(super) fn parse_global_tokens(tokens: &[ParsedToken]) -> Result<(bool, OutputMode), String> {
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
                return Err(invalid_usage(&unknown_option_message(option)));
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
            | ParsedToken::FocusIntention(_)
            | ParsedToken::TaskNote(_)
            | ParsedToken::Status
            | ParsedToken::Watch(_)
            | ParsedToken::Profile(_)
            | ParsedToken::Theme(_)
            | ParsedToken::Goal(_)
            | ParsedToken::GoalWeekly(_)
            | ParsedToken::GoalMonthly(_)
            | ParsedToken::GoalCarry(_)
            | ParsedToken::GoalCarryWeekly(_)
            | ParsedToken::GoalCarryMonthly(_)
            | ParsedToken::Strict(_)
            | ParsedToken::Schedule
            | ParsedToken::ScheduleSet(_)
            | ParsedToken::BreakGlassTrigger
            | ParsedToken::BreakGlassCancel
            | ParsedToken::Diagnostics
            | ParsedToken::Backup(_)
            | ParsedToken::Restore(_)
            | ParsedToken::Export(_)
            | ParsedToken::BlocklistProfile(_)
            | ParsedToken::BlocklistProfileCreate(_)
            | ParsedToken::BlocklistProfileRename(_)
            | ParsedToken::BlocklistProfileDelete
            | ParsedToken::HistoryDashboard
            | ParsedToken::BlocklistSites
            | ParsedToken::AllowlistSites
            | ParsedToken::BlocklistSiteAdd(_)
            | ParsedToken::AllowlistSiteAdd(_)
            | ParsedToken::AllowlistSiteAddTemporary(_)
            | ParsedToken::BlocklistSiteEdit(_)
            | ParsedToken::AllowlistSiteEdit(_)
            | ParsedToken::BlocklistSiteDelete(_)
            | ParsedToken::AllowlistSiteDelete(_) => {}
        }
    }
    Ok((show_help, output))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RemovedOptionGuidance {
    pub(super) summary: &'static str,
    pub(super) replacement: &'static str,
}

pub(super) fn first_removed_option_guidance(
    tokens: &[ParsedToken],
) -> Option<RemovedOptionGuidance> {
    for token in tokens {
        match token {
            ParsedToken::UnknownOption(option) => {
                return removed_option_replacement_guidance(option);
            }
            ParsedToken::Positional(_) => return None,
            _ => {}
        }
    }
    None
}

fn unknown_option_message(option: &str) -> String {
    let guidance = removed_option_replacement_guidance(option);
    match guidance {
        Some(guidance) => format!("Unknown option `{option}`. {}", guidance.summary),
        None => format!("Unknown option `{option}`."),
    }
}

fn removed_option_replacement_guidance(option: &str) -> Option<RemovedOptionGuidance> {
    let flag = option.split('=').next().unwrap_or(option);
    match flag {
        "--automation-triggers" | "--automation-triggers-set" => Some(RemovedOptionGuidance {
            summary: "Standalone automation trigger commands were removed.",
            replacement: "Use `--schedule`/`--schedule-set` for schedule-driven focus starts, supported timer controls for active windows, and task/profile/blocklist commands for defaults.",
        }),
        "--session-template"
        | "--session-template-apply"
        | "--session-template-create"
        | "--session-template-rename"
        | "--session-template-delete" => Some(RemovedOptionGuidance {
            summary: "Session template commands were removed.",
            replacement: "Use `--task`, `--profile`, `--schedule`/`--schedule-set`, and blocklist profile commands directly.",
        }),
        "--config-doctor" | "--config-migrate" | "--config-migrate-apply" => {
            Some(RemovedOptionGuidance {
                summary: "Dedicated config diagnostics commands were removed.",
                replacement: "Use `focustime --diagnostics` for setup checks, config health, and migration guidance.",
            })
        }
        _ => None,
    }
}

pub(super) fn parse_primary_command(
    tokens: &[ParsedToken],
) -> Result<Option<PrimaryCommand>, String> {
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
            ParsedToken::FocusIntention(value) => {
                set_primary_command(&mut primary, PrimaryCommand::FocusIntention(value.clone()))?
            }
            ParsedToken::TaskNote(value) => {
                set_primary_command(&mut primary, PrimaryCommand::TaskNote(value.clone()))?
            }
            ParsedToken::Status => set_primary_command(&mut primary, PrimaryCommand::Status)?,
            ParsedToken::Watch(_) => {}
            ParsedToken::Profile(profile) => {
                set_primary_command(&mut primary, PrimaryCommand::Profile(*profile))?
            }
            ParsedToken::Theme(preset) => {
                set_primary_command(&mut primary, PrimaryCommand::Theme(*preset))?
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
            ParsedToken::BreakGlassTrigger => {
                set_primary_command(&mut primary, PrimaryCommand::BreakGlassTrigger)?
            }
            ParsedToken::BreakGlassCancel => {
                set_primary_command(&mut primary, PrimaryCommand::BreakGlassCancel)?
            }
            ParsedToken::Diagnostics => {
                set_primary_command(&mut primary, PrimaryCommand::Diagnostics)?
            }
            ParsedToken::Backup(dir) => {
                set_primary_command(&mut primary, PrimaryCommand::Backup(dir.clone()))?
            }
            ParsedToken::Restore(dir) => {
                set_primary_command(&mut primary, PrimaryCommand::Restore(dir.clone()))?
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
            ParsedToken::HistoryDashboard => {
                set_primary_command(&mut primary, PrimaryCommand::HistoryDashboard)?
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
            ParsedToken::AllowlistSiteAddTemporary(input) => set_primary_command(
                &mut primary,
                PrimaryCommand::AllowlistSiteAddTemporary(input.clone()),
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

#[allow(clippy::too_many_arguments)]
pub(super) fn finalize_cli_action(
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
            Ok(CliAction::RunTui)
        }
        Some(PrimaryCommand::Start) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::Start,
            output,
        })),
        Some(PrimaryCommand::Profile(profile)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::Profile { profile },
            output,
        })),
        Some(PrimaryCommand::Theme(preset)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::Theme { preset },
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
        Some(PrimaryCommand::BreakGlassTrigger) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::BreakGlassTrigger,
            output,
        })),
        Some(PrimaryCommand::BreakGlassCancel) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::BreakGlassCancel,
            output,
        })),
        Some(PrimaryCommand::Diagnostics) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::Diagnostics,
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
        Some(PrimaryCommand::FocusIntention(value)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::FocusIntention { value },
            output,
        })),
        Some(PrimaryCommand::TaskNote(value)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::TaskNote { value },
            output,
        })),
        Some(PrimaryCommand::Status) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::Status {
                watch_interval_secs,
            },
            output,
        })),
        Some(PrimaryCommand::Backup(dir)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::Backup { dir },
            output,
        })),
        Some(PrimaryCommand::Restore(dir)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::Restore { dir },
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
        Some(PrimaryCommand::HistoryDashboard) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::HistoryDashboard {
                command: HistoryDashboardCommandKind::Show,
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
        Some(PrimaryCommand::AllowlistSiteAddTemporary(input)) => {
            Ok(CliAction::RunCommand(CliCommand {
                kind: CommandKind::AllowlistSiteAddTemporary { input },
                output,
            }))
        }
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

/// Returns the CLI flag name for a primary command token.
fn primary_name(command: &PrimaryCommand) -> &'static str {
    match command {
        PrimaryCommand::Start => "--start",
        PrimaryCommand::Pause => "--pause",
        PrimaryCommand::Resume => "--resume",
        PrimaryCommand::Stop => "--stop",
        PrimaryCommand::Next => "--next",
        PrimaryCommand::Task(_) => "--task",
        PrimaryCommand::TaskGoal { .. } => "--task-goal",
        PrimaryCommand::FocusIntention(_) => "--focus-intention",
        PrimaryCommand::TaskNote(_) => "--task-note",
        PrimaryCommand::Profile(_) => "--profile",
        PrimaryCommand::Theme(_) => "--theme",
        PrimaryCommand::Goal(_) => "--goal",
        PrimaryCommand::GoalWeekly(_) => "--goal-weekly",
        PrimaryCommand::GoalMonthly(_) => "--goal-monthly",
        PrimaryCommand::GoalCarry(_) => "--goal-carry",
        PrimaryCommand::GoalCarryWeekly(_) => "--goal-carry-weekly",
        PrimaryCommand::GoalCarryMonthly(_) => "--goal-carry-monthly",
        PrimaryCommand::Strict(_) => "--strict",
        PrimaryCommand::Schedule => "--schedule",
        PrimaryCommand::ScheduleSet(_) => "--schedule-set",
        PrimaryCommand::BreakGlassTrigger => "--break-glass-trigger",
        PrimaryCommand::BreakGlassCancel => "--break-glass-cancel",
        PrimaryCommand::Diagnostics => "--diagnostics",
        PrimaryCommand::Status => "--status",
        PrimaryCommand::Backup(_) => "--backup",
        PrimaryCommand::Restore(_) => "--restore",
        PrimaryCommand::Export(_) => "--export",
        PrimaryCommand::BlocklistProfile(_) => "--blocklist-profile",
        PrimaryCommand::BlocklistProfileCreate(_) => "--blocklist-profile-create",
        PrimaryCommand::BlocklistProfileRename(_) => "--blocklist-profile-rename",
        PrimaryCommand::BlocklistProfileDelete => "--blocklist-profile-delete",
        PrimaryCommand::HistoryDashboard => "--history-dashboard",
        PrimaryCommand::BlocklistSites => "--blocklist-sites",
        PrimaryCommand::AllowlistSites => "--allowlist-sites",
        PrimaryCommand::BlocklistSiteAdd(_) => "--blocklist-site-add",
        PrimaryCommand::AllowlistSiteAdd(_) => "--allowlist-site-add",
        PrimaryCommand::AllowlistSiteAddTemporary(_) => "--allowlist-site-add-temporary",
        PrimaryCommand::BlocklistSiteEdit(_) => "--blocklist-site-edit",
        PrimaryCommand::AllowlistSiteEdit(_) => "--allowlist-site-edit",
        PrimaryCommand::BlocklistSiteDelete(_) => "--blocklist-site-delete",
        PrimaryCommand::AllowlistSiteDelete(_) => "--allowlist-site-delete",
    }
}

pub(super) fn invalid_usage(message: &str) -> String {
    format!("{message}\n\n{USAGE_TEXT}")
}
