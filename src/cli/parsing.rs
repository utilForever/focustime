use crate::cli::{
    AutomationTriggerRuleConfig, BlocklistCategoryCommandKind, BlocklistProfileCommandKind,
    BlocklistSiteCommandKind, CliAction, CliCommand, CommandKind, DEFAULT_WATCH_INTERVAL_SECS,
    DailyGoalConfig, MonthlyGoalConfig, NaiveDate, OneTimeFocusWindowConfig, OutputMode,
    ParsedToken, PrimaryCommand, ProfileId, RecurringFocusWindowConfig, RecurringScheduleConfig,
    SessionTemplateCommandKind, SiteEditValue, SiteListTarget, ThemePreset, USAGE_TEXT,
    WeekdayProfileRuleConfig, WeeklyGoalConfig,
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
            | ParsedToken::WeekdayRules
            | ParsedToken::WeekdayRulesSet(_)
            | ParsedToken::AutomationTriggers
            | ParsedToken::AutomationTriggersSet(_)
            | ParsedToken::ScheduleDelay
            | ParsedToken::BreakGlassTrigger
            | ParsedToken::BreakGlassCancel
            | ParsedToken::Diagnostics
            | ParsedToken::BlockingPreview
            | ParsedToken::Backup(_)
            | ParsedToken::Restore(_)
            | ParsedToken::Export(_)
            | ParsedToken::BlocklistProfile(_)
            | ParsedToken::BlocklistProfileCreate(_)
            | ParsedToken::BlocklistProfileRename(_)
            | ParsedToken::BlocklistProfileDelete
            | ParsedToken::BlocklistCategory(_)
            | ParsedToken::BlocklistCategoryCreate(_)
            | ParsedToken::BlocklistCategoryRename(_)
            | ParsedToken::BlocklistCategoryDelete
            | ParsedToken::SessionTemplate(_)
            | ParsedToken::SessionTemplateApply(_)
            | ParsedToken::SessionTemplateCreate(_)
            | ParsedToken::SessionTemplateRename(_)
            | ParsedToken::SessionTemplateDelete
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
            ParsedToken::WeekdayRules => {
                set_primary_command(&mut primary, PrimaryCommand::WeekdayRules)?
            }
            ParsedToken::WeekdayRulesSet(rules) => {
                set_primary_command(&mut primary, PrimaryCommand::WeekdayRulesSet(rules.clone()))?
            }
            ParsedToken::AutomationTriggers => {
                set_primary_command(&mut primary, PrimaryCommand::AutomationTriggers)?
            }
            ParsedToken::AutomationTriggersSet(rules) => set_primary_command(
                &mut primary,
                PrimaryCommand::AutomationTriggersSet(rules.clone()),
            )?,
            ParsedToken::ScheduleDelay => {
                set_primary_command(&mut primary, PrimaryCommand::ScheduleDelay)?
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
            ParsedToken::BlockingPreview => {
                set_primary_command(&mut primary, PrimaryCommand::BlockingPreview)?
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
            ParsedToken::BlocklistCategory(category) => set_primary_command(
                &mut primary,
                PrimaryCommand::BlocklistCategory(category.clone()),
            )?,
            ParsedToken::BlocklistCategoryCreate(name) => set_primary_command(
                &mut primary,
                PrimaryCommand::BlocklistCategoryCreate(name.clone()),
            )?,
            ParsedToken::BlocklistCategoryRename(name) => set_primary_command(
                &mut primary,
                PrimaryCommand::BlocklistCategoryRename(name.clone()),
            )?,
            ParsedToken::BlocklistCategoryDelete => {
                set_primary_command(&mut primary, PrimaryCommand::BlocklistCategoryDelete)?
            }
            ParsedToken::SessionTemplate(name) => {
                set_primary_command(&mut primary, PrimaryCommand::SessionTemplate(name.clone()))?
            }
            ParsedToken::SessionTemplateApply(name) => set_primary_command(
                &mut primary,
                PrimaryCommand::SessionTemplateApply(name.clone()),
            )?,
            ParsedToken::SessionTemplateCreate(name) => set_primary_command(
                &mut primary,
                PrimaryCommand::SessionTemplateCreate(name.clone()),
            )?,
            ParsedToken::SessionTemplateRename(name) => set_primary_command(
                &mut primary,
                PrimaryCommand::SessionTemplateRename(name.clone()),
            )?,
            ParsedToken::SessionTemplateDelete => {
                set_primary_command(&mut primary, PrimaryCommand::SessionTemplateDelete)?
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
        Some(PrimaryCommand::WeekdayRules) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::WeekdayRules { rules: None },
            output,
        })),
        Some(PrimaryCommand::WeekdayRulesSet(rules)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::WeekdayRules { rules: Some(rules) },
            output,
        })),
        Some(PrimaryCommand::AutomationTriggers) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::AutomationTriggers { rules: None },
            output,
        })),
        Some(PrimaryCommand::AutomationTriggersSet(rules)) => {
            Ok(CliAction::RunCommand(CliCommand {
                kind: CommandKind::AutomationTriggers { rules: Some(rules) },
                output,
            }))
        }
        Some(PrimaryCommand::ScheduleDelay) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::ScheduleDelay,
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
        Some(PrimaryCommand::BlocklistCategory(category)) => {
            Ok(CliAction::RunCommand(CliCommand {
                kind: CommandKind::BlocklistCategory {
                    command: BlocklistCategoryCommandKind::Select { category },
                },
                output,
            }))
        }
        Some(PrimaryCommand::BlocklistCategoryCreate(name)) => {
            Ok(CliAction::RunCommand(CliCommand {
                kind: CommandKind::BlocklistCategory {
                    command: BlocklistCategoryCommandKind::Create { name },
                },
                output,
            }))
        }
        Some(PrimaryCommand::BlocklistCategoryRename(name)) => {
            Ok(CliAction::RunCommand(CliCommand {
                kind: CommandKind::BlocklistCategory {
                    command: BlocklistCategoryCommandKind::Rename { name },
                },
                output,
            }))
        }
        Some(PrimaryCommand::BlocklistCategoryDelete) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::BlocklistCategory {
                command: BlocklistCategoryCommandKind::Delete,
            },
            output,
        })),
        Some(PrimaryCommand::SessionTemplate(name)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::SessionTemplate {
                command: SessionTemplateCommandKind::Select { name },
            },
            output,
        })),
        Some(PrimaryCommand::SessionTemplateApply(name)) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::SessionTemplate {
                command: SessionTemplateCommandKind::Apply { name },
            },
            output,
        })),
        Some(PrimaryCommand::SessionTemplateCreate(name)) => {
            Ok(CliAction::RunCommand(CliCommand {
                kind: CommandKind::SessionTemplate {
                    command: SessionTemplateCommandKind::Create { name },
                },
                output,
            }))
        }
        Some(PrimaryCommand::SessionTemplateRename(name)) => {
            Ok(CliAction::RunCommand(CliCommand {
                kind: CommandKind::SessionTemplate {
                    command: SessionTemplateCommandKind::Rename { name },
                },
                output,
            }))
        }
        Some(PrimaryCommand::SessionTemplateDelete) => Ok(CliAction::RunCommand(CliCommand {
            kind: CommandKind::SessionTemplate {
                command: SessionTemplateCommandKind::Delete,
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

pub(super) fn parse_profile_id(value: &str) -> Result<ProfileId, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "classic" => Ok(ProfileId::Classic),
        "deep-work" | "deep_work" | "deepwork" => Ok(ProfileId::DeepWork),
        "custom" => Ok(ProfileId::Custom),
        _ => Err(invalid_usage(&format!(
            "Invalid profile `{value}`. Use `classic`, `deep-work`, or `custom`."
        ))),
    }
}

pub(super) fn parse_theme_preset(value: &str) -> Result<ThemePreset, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "classic" => Ok(ThemePreset::Classic),
        "high-contrast" | "high_contrast" | "highcontrast" => Ok(ThemePreset::HighContrast),
        "deuteranopia-friendly"
        | "deuteranopia_friendly"
        | "deuteranopiafriendly"
        | "colorblind-friendly"
        | "colorblind_friendly"
        | "colorblindfriendly" => Ok(ThemePreset::DeuteranopiaFriendly),
        _ => Err(invalid_usage(&format!(
            "Invalid theme preset `{value}`. Use `classic`, `high-contrast`, or `deuteranopia-friendly`."
        ))),
    }
}

pub(super) fn parse_task_goal_value(
    value: &str,
) -> Result<(String, Option<DailyGoalConfig>), String> {
    let trimmed = value.trim();
    if let Some((label_raw, goal_raw)) = trimmed.rsplit_once(':')
        && goal_raw.contains(',')
    {
        let label =
            require_nonempty_key_value(label_raw, "Task goal requires a task label before `:`.")?
                .to_string();
        let (minutes, pomodoros) = parse_goal_components(goal_raw, "--task-goal")?;
        return Ok((label, Some(DailyGoalConfig { minutes, pomodoros })));
    }

    let label = require_nonempty_key_value(
        trimmed,
        "`--task-goal` requires `LABEL` or `LABEL:MINUTES,POMODOROS`.",
    )?
    .to_string();
    Ok((label, None))
}

pub(super) fn parse_goal_value(value: &str) -> Result<DailyGoalConfig, String> {
    let (minutes, pomodoros) = parse_goal_components(value, "--goal")?;
    Ok(DailyGoalConfig { minutes, pomodoros })
}

pub(super) fn parse_weekly_goal_value(value: &str) -> Result<WeeklyGoalConfig, String> {
    let (minutes, pomodoros) = parse_goal_components(value, "--goal-weekly")?;
    Ok(WeeklyGoalConfig { minutes, pomodoros })
}

pub(super) fn parse_monthly_goal_value(value: &str) -> Result<MonthlyGoalConfig, String> {
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

pub(super) fn parse_strict_value(value: &str) -> Result<bool, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err(invalid_usage(&format!(
            "Invalid strict mode `{value}`. Use `--strict=on` or `--strict=off`."
        ))),
    }
}

pub(super) fn parse_goal_carry_value(value: &str) -> Result<bool, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err(invalid_usage(&format!(
            "Invalid goal carry-over `{value}`. Use `on` or `off`."
        ))),
    }
}

pub(super) fn parse_site_edit_value(value: &str) -> Result<SiteEditValue, String> {
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

pub(super) fn parse_schedule_value(value: &str) -> Result<RecurringScheduleConfig, String> {
    let schedule = serde_json::from_str::<RecurringScheduleConfig>(value).map_err(|error| {
        invalid_usage(&format!(
            "Invalid schedule JSON payload: {error}. Use `--schedule-set='{{\"windows\":[...],\"exception_dates\":[...],\"one_time_windows\":[...]}}'`."
        ))
    })?;
    validate_schedule_value(&schedule)?;
    Ok(schedule)
}

pub(super) fn parse_weekday_rules_value(
    value: &str,
) -> Result<Vec<WeekdayProfileRuleConfig>, String> {
    let rules = serde_json::from_str::<Vec<WeekdayProfileRuleConfig>>(value).map_err(|error| {
        invalid_usage(&format!(
            "Invalid weekday-rules JSON payload: {error}. Use `--weekday-rules-set='[{{\"day\":\"mon\",\"profile\":\"deep-work\",\"blocklist_profile\":\"Work\",\"session_template\":\"Deep Flow\"}}]'`."
        ))
    })?;
    validate_weekday_rules_value(&rules)?;
    Ok(rules)
}

pub(super) fn parse_automation_triggers_value(
    value: &str,
) -> Result<Vec<AutomationTriggerRuleConfig>, String> {
    serde_json::from_str::<Vec<AutomationTriggerRuleConfig>>(value).map_err(|error| {
        invalid_usage(&format!(
            "Invalid automation-triggers JSON payload: {error}. Use `--automation-triggers-set='[{{\"trigger\":{{\"type\":\"time\",\"days\":[\"mon\"],\"at\":\"09:00\"}},\"action\":{{\"type\":\"start_focus\"}}}}]'`."
        ))
    })
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

fn validate_weekday_rules_value(rules: &[WeekdayProfileRuleConfig]) -> Result<(), String> {
    for (index, rule) in rules.iter().enumerate() {
        if !is_valid_schedule_weekday(&rule.day) {
            return Err(invalid_usage(&format!(
                "Invalid weekday rule at index {index}: unknown day `{}`.",
                rule.day
            )));
        }
        if rule.blocklist_profile.trim().is_empty() {
            return Err(invalid_usage(&format!(
                "Invalid weekday rule at index {index}: `blocklist_profile` cannot be empty."
            )));
        }
        if let Some(template) = rule.session_template.as_deref()
            && template.trim().is_empty()
        {
            return Err(invalid_usage(&format!(
                "Invalid weekday rule at index {index}: `session_template` cannot be empty when provided."
            )));
        }
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
        PrimaryCommand::WeekdayRules => "--weekday-rules",
        PrimaryCommand::WeekdayRulesSet(_) => "--weekday-rules-set",
        PrimaryCommand::AutomationTriggers => "--automation-triggers",
        PrimaryCommand::AutomationTriggersSet(_) => "--automation-triggers-set",
        PrimaryCommand::ScheduleDelay => "--schedule-delay",
        PrimaryCommand::BreakGlassTrigger => "--break-glass-trigger",
        PrimaryCommand::BreakGlassCancel => "--break-glass-cancel",
        PrimaryCommand::Diagnostics => "--diagnostics",
        PrimaryCommand::BlockingPreview => "--blocking-preview",
        PrimaryCommand::Status => "--status",
        PrimaryCommand::Backup(_) => "--backup",
        PrimaryCommand::Restore(_) => "--restore",
        PrimaryCommand::Export(_) => "--export",
        PrimaryCommand::BlocklistProfile(_) => "--blocklist-profile",
        PrimaryCommand::BlocklistProfileCreate(_) => "--blocklist-profile-create",
        PrimaryCommand::BlocklistProfileRename(_) => "--blocklist-profile-rename",
        PrimaryCommand::BlocklistProfileDelete => "--blocklist-profile-delete",
        PrimaryCommand::BlocklistCategory(_) => "--blocklist-category",
        PrimaryCommand::BlocklistCategoryCreate(_) => "--blocklist-category-create",
        PrimaryCommand::BlocklistCategoryRename(_) => "--blocklist-category-rename",
        PrimaryCommand::BlocklistCategoryDelete => "--blocklist-category-delete",
        PrimaryCommand::SessionTemplate(_) => "--session-template",
        PrimaryCommand::SessionTemplateApply(_) => "--session-template-apply",
        PrimaryCommand::SessionTemplateCreate(_) => "--session-template-create",
        PrimaryCommand::SessionTemplateRename(_) => "--session-template-rename",
        PrimaryCommand::SessionTemplateDelete => "--session-template-delete",
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

pub(super) fn parse_watch_interval_option(tokens: &[ParsedToken]) -> Result<Option<u64>, String> {
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

pub(super) fn parse_watch_interval_secs(value: &str) -> Result<u64, String> {
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

pub(super) fn require_nonempty_key_value<'a>(
    value: &'a str,
    message: &str,
) -> Result<&'a str, String> {
    if value.trim().is_empty() {
        return Err(invalid_usage(message));
    }
    Ok(value)
}
