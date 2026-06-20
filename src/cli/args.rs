mod key_value;

pub(super) use key_value::classify_key_value_arg;

use crate::cli::{
    OsString, OutputMode, ParsedToken, PathBuf, ValueArgParser, invalid_usage,
    parse_automation_triggers_value, parse_daemon_port, parse_goal_carry_value, parse_goal_value,
    parse_monthly_goal_value, parse_profile_id, parse_schedule_value, parse_site_edit_value,
    parse_strict_value, parse_task_goal_value, parse_theme_preset, parse_watch_interval_secs,
    parse_weekly_goal_value, require_nonempty_key_value,
};

pub(super) fn infer_output_mode_from_os_args(args: &[OsString]) -> OutputMode {
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

pub(super) fn classify_args(args: &[String]) -> Result<Vec<ParsedToken>, String> {
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

/// Classifies options that require a following value token.
fn classify_value_arg(
    args: &[String],
    index: usize,
    arg: &str,
) -> Result<Option<(ParsedToken, usize)>, String> {
    let parsers: [(&str, ValueArgParser); 35] = [
        ("--task", classify_task_arg),
        ("--task-goal", classify_task_goal_arg),
        ("--focus-intention", classify_focus_intention_arg),
        ("--task-note", classify_task_note_arg),
        ("--profile", classify_profile_arg),
        ("--theme", classify_theme_arg),
        ("--goal", classify_goal_arg),
        ("--goal-weekly", classify_goal_weekly_arg),
        ("--goal-monthly", classify_goal_monthly_arg),
        ("--goal-carry", classify_goal_carry_arg),
        ("--goal-carry-weekly", classify_goal_carry_weekly_arg),
        ("--goal-carry-monthly", classify_goal_carry_monthly_arg),
        ("--strict", classify_strict_arg),
        ("--schedule-set", classify_schedule_set_arg),
        (
            "--automation-triggers-set",
            classify_automation_triggers_set_arg,
        ),
        ("--watch", classify_watch_arg),
        ("--daemon-port", classify_daemon_port_arg),
        ("--backup", classify_backup_arg),
        ("--restore", classify_restore_arg),
        ("--export", classify_export_arg),
        ("--feature-inventory", classify_feature_inventory_arg),
        ("--blocklist-profile", classify_blocklist_profile_arg),
        (
            "--blocklist-profile-create",
            classify_blocklist_profile_create_arg,
        ),
        (
            "--blocklist-profile-rename",
            classify_blocklist_profile_rename_arg,
        ),
        ("--session-template", classify_session_template_arg),
        (
            "--session-template-apply",
            classify_session_template_apply_arg,
        ),
        (
            "--session-template-create",
            classify_session_template_create_arg,
        ),
        (
            "--session-template-rename",
            classify_session_template_rename_arg,
        ),
        ("--blocklist-site-add", classify_blocklist_site_add_arg),
        ("--allowlist-site-add", classify_allowlist_site_add_arg),
        (
            "--allowlist-site-add-temporary",
            classify_allowlist_site_add_temporary_arg,
        ),
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

/// Classifies single-token flags that do not consume a value.
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
        "--daemon-start" => Some(ParsedToken::DaemonStart),
        "--daemon-status" => Some(ParsedToken::DaemonStatus),
        "--daemon-stop" => Some(ParsedToken::DaemonStop),
        "--schedule" => Some(ParsedToken::Schedule),
        "--automation-triggers" => Some(ParsedToken::AutomationTriggers),
        "--schedule-delay" => Some(ParsedToken::ScheduleDelay),
        "--break-glass-trigger" => Some(ParsedToken::BreakGlassTrigger),
        "--break-glass-cancel" => Some(ParsedToken::BreakGlassCancel),
        "--config-doctor" => Some(ParsedToken::ConfigDoctor),
        "--config-migrate" => Some(ParsedToken::ConfigMigrate { apply: false }),
        "--config-migrate-apply" => Some(ParsedToken::ConfigMigrate { apply: true }),
        "--diagnostics" => Some(ParsedToken::Diagnostics),
        "--calendar-sync" => Some(ParsedToken::CalendarSync),
        "--blocklist-profile-delete" => Some(ParsedToken::BlocklistProfileDelete),
        "--session-template-delete" => Some(ParsedToken::SessionTemplateDelete),
        "--history-dashboard" => Some(ParsedToken::HistoryDashboard),
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

fn classify_theme_arg(args: &[String], index: usize) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let selected = parse_theme_preset(next)?;
        return Ok((ParsedToken::Theme(Some(selected)), 2));
    }
    Ok((ParsedToken::Theme(None), 1))
}

fn classify_focus_intention_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value = require_nonempty_key_value(
            next,
            "`--focus-intention` requires a value when one is provided.",
        )?;
        return Ok((ParsedToken::FocusIntention(Some(value.to_string())), 2));
    }
    Ok((ParsedToken::FocusIntention(None), 1))
}

fn classify_task_note_arg(args: &[String], index: usize) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value = require_nonempty_key_value(
            next,
            "`--task-note` requires a value when one is provided.",
        )?;
        return Ok((ParsedToken::TaskNote(Some(value.to_string())), 2));
    }
    Ok((ParsedToken::TaskNote(None), 1))
}

fn classify_export_arg(args: &[String], index: usize) -> Result<(ParsedToken, usize), String> {
    classify_optional_artifact_path_arg(
        args,
        index,
        ParsedToken::Export,
        "`--export` requires a target directory.",
    )
}

fn classify_feature_inventory_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    classify_optional_artifact_path_arg(
        args,
        index,
        ParsedToken::FeatureInventory,
        "`--feature-inventory` requires a target directory.",
    )
}

fn classify_backup_arg(args: &[String], index: usize) -> Result<(ParsedToken, usize), String> {
    classify_optional_artifact_path_arg(
        args,
        index,
        ParsedToken::Backup,
        "`--backup` requires a target directory.",
    )
}

fn classify_restore_arg(args: &[String], index: usize) -> Result<(ParsedToken, usize), String> {
    classify_optional_artifact_path_arg(
        args,
        index,
        ParsedToken::Restore,
        "`--restore` requires a source directory.",
    )
}

fn classify_optional_artifact_path_arg(
    args: &[String],
    index: usize,
    token: fn(Option<PathBuf>) -> ParsedToken,
    empty_value_message: &'static str,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value = require_nonempty_key_value(next, empty_value_message)?;
        return Ok((token(Some(PathBuf::from(value))), 2));
    }
    Ok((token(None), 1))
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

fn classify_daemon_port_arg(args: &[String], index: usize) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value = require_nonempty_key_value(
            next,
            "`--daemon-port` requires a port between 1 and 65535.",
        )?;
        return Ok((ParsedToken::DaemonPort(parse_daemon_port(value)?), 2));
    }
    Err(invalid_usage(
        "`--daemon-port` requires a value. Use `--daemon-port=PORT`.",
    ))
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

fn classify_session_template_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value =
            require_nonempty_key_value(next, "`--session-template` requires a template name.")?;
        return Ok((ParsedToken::SessionTemplate(Some(value.to_string())), 2));
    }
    Ok((ParsedToken::SessionTemplate(None), 1))
}

fn classify_session_template_apply_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value = require_nonempty_key_value(
            next,
            "`--session-template-apply` requires a template name when a value is provided.",
        )?;
        return Ok((
            ParsedToken::SessionTemplateApply(Some(value.to_string())),
            2,
        ));
    }
    Ok((ParsedToken::SessionTemplateApply(None), 1))
}

fn classify_session_template_create_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value =
            require_nonempty_key_value(next, "`--session-template-create` requires a name.")?;
        return Ok((ParsedToken::SessionTemplateCreate(value.to_string()), 2));
    }
    Err(invalid_usage(
        "`--session-template-create` requires a template name. Use `--session-template-create=NAME` or `--session-template-create NAME`.",
    ))
}

fn classify_session_template_rename_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value =
            require_nonempty_key_value(next, "`--session-template-rename` requires a name.")?;
        return Ok((ParsedToken::SessionTemplateRename(value.to_string()), 2));
    }
    Err(invalid_usage(
        "`--session-template-rename` requires a template name. Use `--session-template-rename=NAME` or `--session-template-rename NAME`.",
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

fn classify_allowlist_site_add_temporary_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value = require_nonempty_key_value(
            next,
            "`--allowlist-site-add-temporary` requires HOST=30m style input.",
        )?;
        return Ok((ParsedToken::AllowlistSiteAddTemporary(value.to_string()), 2));
    }
    Err(invalid_usage(
        "`--allowlist-site-add-temporary` requires HOST=30m style input. Use `--allowlist-site-add-temporary=HOST_DURATIONS` or `--allowlist-site-add-temporary HOST_DURATIONS`.",
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

/// Classifies `--automation-triggers-set` and parses its JSON payload.
fn classify_automation_triggers_set_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        return Ok((
            ParsedToken::AutomationTriggersSet(parse_automation_triggers_value(next)?),
            2,
        ));
    }
    Err(invalid_usage(
        "`--automation-triggers-set` requires a JSON payload. Use `--automation-triggers-set='[{\"trigger\":{\"type\":\"time\",\"days\":[\"mon\"],\"at\":\"09:00\"},\"action\":{\"type\":\"start_focus\"}}]'`.",
    ))
}
