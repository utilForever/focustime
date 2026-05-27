use crate::cli::{
    KeyValueParser, OsString, OutputMode, ParsedToken, PathBuf, ValueArgParser, invalid_usage,
    parse_automation_triggers_value, parse_compare_by_value, parse_compare_limit_value,
    parse_compare_profile_value, parse_compare_time_of_day_value, parse_daemon_port,
    parse_goal_carry_value, parse_goal_value, parse_history_dashboard_order_value,
    parse_history_kpi_card_id, parse_monthly_goal_value, parse_profile_id, parse_schedule_value,
    parse_site_edit_value, parse_strict_value, parse_task_goal_value, parse_theme_preset,
    parse_watch_interval_secs, parse_weekday_rules_value, parse_weekly_goal_value,
    require_nonempty_key_value,
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

fn classify_value_arg(
    args: &[String],
    index: usize,
    arg: &str,
) -> Result<Option<(ParsedToken, usize)>, String> {
    let parsers: [(&str, ValueArgParser); 49] = [
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
        ("--weekday-rules-set", classify_weekday_rules_set_arg),
        (
            "--automation-triggers-set",
            classify_automation_triggers_set_arg,
        ),
        ("--watch", classify_watch_arg),
        ("--daemon-port", classify_daemon_port_arg),
        ("--compare-by", classify_compare_by_arg),
        ("--compare-task", classify_compare_task_arg),
        ("--compare-profile", classify_compare_profile_arg),
        ("--compare-time", classify_compare_time_of_day_arg),
        ("--compare-limit", classify_compare_limit_arg),
        ("--backup", classify_backup_arg),
        ("--restore", classify_restore_arg),
        ("--sync-backup", classify_sync_backup_arg),
        ("--sync-restore", classify_sync_restore_arg),
        ("--sync-passphrase", classify_sync_passphrase_arg),
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
        ("--blocklist-category", classify_blocklist_category_arg),
        (
            "--blocklist-category-create",
            classify_blocklist_category_create_arg,
        ),
        (
            "--blocklist-category-rename",
            classify_blocklist_category_rename_arg,
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
        (
            "--history-dashboard-pin",
            classify_history_dashboard_pin_arg,
        ),
        (
            "--history-dashboard-unpin",
            classify_history_dashboard_unpin_arg,
        ),
        (
            "--history-dashboard-order",
            classify_history_dashboard_order_arg,
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
        "--weekday-rules" => Some(ParsedToken::WeekdayRules),
        "--automation-triggers" => Some(ParsedToken::AutomationTriggers),
        "--schedule-delay" => Some(ParsedToken::ScheduleDelay),
        "--break-glass-trigger" => Some(ParsedToken::BreakGlassTrigger),
        "--break-glass-cancel" => Some(ParsedToken::BreakGlassCancel),
        "--diagnostics" => Some(ParsedToken::Diagnostics),
        "--blocking-preview" => Some(ParsedToken::BlockingPreview),
        "--blocklist-profile-delete" => Some(ParsedToken::BlocklistProfileDelete),
        "--blocklist-category-delete" => Some(ParsedToken::BlocklistCategoryDelete),
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
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        return Ok((ParsedToken::Export(Some(PathBuf::from(next))), 2));
    }
    Ok((ParsedToken::Export(None), 1))
}

fn classify_backup_arg(args: &[String], index: usize) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value = require_nonempty_key_value(next, "`--backup` requires a target directory.")?;
        return Ok((ParsedToken::Backup(Some(PathBuf::from(value))), 2));
    }
    Ok((ParsedToken::Backup(None), 1))
}

fn classify_restore_arg(args: &[String], index: usize) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value = require_nonempty_key_value(next, "`--restore` requires a source directory.")?;
        return Ok((ParsedToken::Restore(Some(PathBuf::from(value))), 2));
    }
    Ok((ParsedToken::Restore(None), 1))
}

fn classify_sync_backup_arg(args: &[String], index: usize) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value =
            require_nonempty_key_value(next, "`--sync-backup` requires a target directory.")?;
        return Ok((ParsedToken::SyncBackup(Some(PathBuf::from(value))), 2));
    }
    Ok((ParsedToken::SyncBackup(None), 1))
}

fn classify_sync_restore_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value =
            require_nonempty_key_value(next, "`--sync-restore` requires a source directory.")?;
        return Ok((ParsedToken::SyncRestore(Some(PathBuf::from(value))), 2));
    }
    Ok((ParsedToken::SyncRestore(None), 1))
}

fn classify_sync_passphrase_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value =
            require_nonempty_key_value(next, "`--sync-passphrase` requires a non-empty value.")?;
        return Ok((ParsedToken::SyncPassphrase(value.to_string()), 2));
    }
    Err(invalid_usage(
        "`--sync-passphrase` requires a value. Use `--sync-passphrase=VALUE` or `--sync-passphrase VALUE`.",
    ))
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

fn classify_compare_by_arg(args: &[String], index: usize) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value = require_nonempty_key_value(
            next,
            "`--compare-by` requires `task`, `profile`, or `time-of-day`.",
        )?;
        return Ok((ParsedToken::CompareBy(parse_compare_by_value(value)?), 2));
    }
    Err(invalid_usage(
        "`--compare-by` requires a value. Use `--compare-by=task`, `--compare-by=profile`, or `--compare-by=time-of-day`.",
    ))
}

fn classify_compare_task_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value =
            require_nonempty_key_value(next, "`--compare-task` requires a task label or `all`.")?;
        let task = if value.trim().eq_ignore_ascii_case("all") {
            None
        } else {
            Some(value.to_string())
        };
        return Ok((ParsedToken::CompareTask(task), 2));
    }
    Err(invalid_usage(
        "`--compare-task` requires a value. Use `--compare-task=LABEL` or `--compare-task=all`.",
    ))
}

fn classify_compare_profile_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value = require_nonempty_key_value(
            next,
            "`--compare-profile` requires `classic`, `deep-work`, `custom`, `unknown`, or `all`.",
        )?;
        return Ok((
            ParsedToken::CompareProfile(parse_compare_profile_value(value)?),
            2,
        ));
    }
    Err(invalid_usage(
        "`--compare-profile` requires a value. Use `--compare-profile=classic|deep-work|custom|unknown|all`.",
    ))
}

fn classify_compare_time_of_day_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value = require_nonempty_key_value(
            next,
            "`--compare-time` requires `morning`, `afternoon`, `evening`, `night`, `unknown`, or `all`.",
        )?;
        return Ok((
            ParsedToken::CompareTimeOfDay(parse_compare_time_of_day_value(value)?),
            2,
        ));
    }
    Err(invalid_usage(
        "`--compare-time` requires a value. Use `--compare-time=morning|afternoon|evening|night|unknown|all`.",
    ))
}

fn classify_compare_limit_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value = require_nonempty_key_value(
            next,
            "`--compare-limit` requires a positive whole number.",
        )?;
        return Ok((
            ParsedToken::CompareLimit(parse_compare_limit_value(value)?),
            2,
        ));
    }
    Err(invalid_usage(
        "`--compare-limit` requires a value. Use `--compare-limit=NUMBER`.",
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

fn classify_blocklist_category_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        if next.trim().is_empty() {
            return Err(invalid_usage(
                "`--blocklist-category` requires a category name when a value is provided.",
            ));
        }
        return Ok((ParsedToken::BlocklistCategory(Some(next.clone())), 2));
    }
    Ok((ParsedToken::BlocklistCategory(None), 1))
}

fn classify_blocklist_category_create_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value = require_nonempty_key_value(
            next,
            "`--blocklist-category-create` requires a category name.",
        )?;
        return Ok((ParsedToken::BlocklistCategoryCreate(value.to_string()), 2));
    }
    Err(invalid_usage(
        "`--blocklist-category-create` requires a category name. Use `--blocklist-category-create=NAME` or `--blocklist-category-create NAME`.",
    ))
}

fn classify_blocklist_category_rename_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value = require_nonempty_key_value(
            next,
            "`--blocklist-category-rename` requires a category name.",
        )?;
        return Ok((ParsedToken::BlocklistCategoryRename(value.to_string()), 2));
    }
    Err(invalid_usage(
        "`--blocklist-category-rename` requires a category name. Use `--blocklist-category-rename=NAME` or `--blocklist-category-rename NAME`.",
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

fn classify_history_dashboard_pin_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value =
            require_nonempty_key_value(next, "`--history-dashboard-pin` requires a card ID.")?;
        return Ok((
            ParsedToken::HistoryDashboardPin(parse_history_kpi_card_id(value)?),
            2,
        ));
    }
    Err(invalid_usage(
        "`--history-dashboard-pin` requires a card ID. Use `--history-dashboard-pin=CARD_ID` or `--history-dashboard-pin CARD_ID`.",
    ))
}

fn classify_history_dashboard_unpin_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value =
            require_nonempty_key_value(next, "`--history-dashboard-unpin` requires a card ID.")?;
        return Ok((
            ParsedToken::HistoryDashboardUnpin(parse_history_kpi_card_id(value)?),
            2,
        ));
    }
    Err(invalid_usage(
        "`--history-dashboard-unpin` requires a card ID. Use `--history-dashboard-unpin=CARD_ID` or `--history-dashboard-unpin CARD_ID`.",
    ))
}

fn classify_history_dashboard_order_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let value = require_nonempty_key_value(
            next,
            "`--history-dashboard-order` requires a comma-separated card ID list.",
        )?;
        return Ok((
            ParsedToken::HistoryDashboardOrder(parse_history_dashboard_order_value(value)?),
            2,
        ));
    }
    Err(invalid_usage(
        "`--history-dashboard-order` requires a comma-separated card ID list. Use `--history-dashboard-order=id1,id2,...`.",
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

fn classify_weekday_rules_set_arg(
    args: &[String],
    index: usize,
) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        return Ok((
            ParsedToken::WeekdayRulesSet(parse_weekday_rules_value(next)?),
            2,
        ));
    }
    Err(invalid_usage(
        "`--weekday-rules-set` requires a JSON payload. Use `--weekday-rules-set='[{\"day\":\"mon\",\"profile\":\"deep-work\",\"blocklist_profile\":\"Work\",\"session_template\":\"Deep Flow\"}]'`.",
    ))
}

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

pub(super) fn classify_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    let parsers: [KeyValueParser; 49] = [
        parse_task_key_value_arg,
        parse_task_goal_key_value_arg,
        parse_focus_intention_key_value_arg,
        parse_task_note_key_value_arg,
        parse_profile_key_value_arg,
        parse_theme_key_value_arg,
        parse_goal_key_value_arg,
        parse_goal_weekly_key_value_arg,
        parse_goal_monthly_key_value_arg,
        parse_goal_carry_key_value_arg,
        parse_goal_carry_weekly_key_value_arg,
        parse_goal_carry_monthly_key_value_arg,
        parse_strict_key_value_arg,
        parse_schedule_set_key_value_arg,
        parse_weekday_rules_set_key_value_arg,
        parse_automation_triggers_set_key_value_arg,
        parse_watch_key_value_arg,
        parse_daemon_port_key_value_arg,
        parse_compare_by_key_value_arg,
        parse_compare_task_key_value_arg,
        parse_compare_profile_key_value_arg,
        parse_compare_time_of_day_key_value_arg,
        parse_compare_limit_key_value_arg,
        parse_backup_key_value_arg,
        parse_restore_key_value_arg,
        parse_sync_backup_key_value_arg,
        parse_sync_restore_key_value_arg,
        parse_sync_passphrase_key_value_arg,
        parse_export_key_value_arg,
        parse_blocklist_profile_key_value_arg,
        parse_blocklist_profile_create_key_value_arg,
        parse_blocklist_profile_rename_key_value_arg,
        parse_blocklist_category_key_value_arg,
        parse_blocklist_category_create_key_value_arg,
        parse_blocklist_category_rename_key_value_arg,
        parse_session_template_key_value_arg,
        parse_session_template_apply_key_value_arg,
        parse_session_template_create_key_value_arg,
        parse_session_template_rename_key_value_arg,
        parse_history_dashboard_pin_key_value_arg,
        parse_history_dashboard_unpin_key_value_arg,
        parse_history_dashboard_order_key_value_arg,
        parse_blocklist_site_add_key_value_arg,
        parse_allowlist_site_add_key_value_arg,
        parse_allowlist_site_add_temporary_key_value_arg,
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

fn parse_focus_intention_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--focus-intention=") {
        let value =
            require_nonempty_key_value(value, "`--focus-intention=` requires a non-empty value.")?;
        return Ok(Some(ParsedToken::FocusIntention(Some(value.to_string()))));
    }
    Ok(None)
}

fn parse_task_note_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--task-note=") {
        let value =
            require_nonempty_key_value(value, "`--task-note=` requires a non-empty value.")?;
        return Ok(Some(ParsedToken::TaskNote(Some(value.to_string()))));
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

fn parse_theme_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--theme=") {
        let value = require_nonempty_key_value(value, "`--theme=` requires a theme value.")?;
        return Ok(Some(ParsedToken::Theme(Some(parse_theme_preset(value)?))));
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

fn parse_weekday_rules_set_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--weekday-rules-set=") {
        let value =
            require_nonempty_key_value(value, "`--weekday-rules-set=` requires a JSON payload.")?;
        return Ok(Some(ParsedToken::WeekdayRulesSet(
            parse_weekday_rules_value(value)?,
        )));
    }
    Ok(None)
}

fn parse_automation_triggers_set_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--automation-triggers-set=") {
        let value = require_nonempty_key_value(
            value,
            "`--automation-triggers-set=` requires a JSON payload.",
        )?;
        return Ok(Some(ParsedToken::AutomationTriggersSet(
            parse_automation_triggers_value(value)?,
        )));
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

fn parse_backup_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--backup=") {
        let value = require_nonempty_key_value(value, "`--backup=` requires a target directory.")?;
        return Ok(Some(ParsedToken::Backup(Some(PathBuf::from(value)))));
    }
    Ok(None)
}

fn parse_restore_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--restore=") {
        let value = require_nonempty_key_value(value, "`--restore=` requires a source directory.")?;
        return Ok(Some(ParsedToken::Restore(Some(PathBuf::from(value)))));
    }
    Ok(None)
}

fn parse_sync_backup_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--sync-backup=") {
        let value =
            require_nonempty_key_value(value, "`--sync-backup=` requires a target directory.")?;
        return Ok(Some(ParsedToken::SyncBackup(Some(PathBuf::from(value)))));
    }
    Ok(None)
}

fn parse_sync_restore_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--sync-restore=") {
        let value =
            require_nonempty_key_value(value, "`--sync-restore=` requires a source directory.")?;
        return Ok(Some(ParsedToken::SyncRestore(Some(PathBuf::from(value)))));
    }
    Ok(None)
}

fn parse_sync_passphrase_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--sync-passphrase=") {
        let value =
            require_nonempty_key_value(value, "`--sync-passphrase=` requires a non-empty value.")?;
        return Ok(Some(ParsedToken::SyncPassphrase(value.to_string())));
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

fn parse_daemon_port_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--daemon-port=") {
        let value = require_nonempty_key_value(
            value,
            "`--daemon-port=` requires a port between 1 and 65535.",
        )?;
        return Ok(Some(ParsedToken::DaemonPort(parse_daemon_port(value)?)));
    }
    Ok(None)
}

fn parse_compare_by_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--compare-by=") {
        let value = require_nonempty_key_value(
            value,
            "`--compare-by=` requires `task`, `profile`, or `time-of-day`.",
        )?;
        return Ok(Some(ParsedToken::CompareBy(parse_compare_by_value(value)?)));
    }
    Ok(None)
}

fn parse_compare_task_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--compare-task=") {
        let value =
            require_nonempty_key_value(value, "`--compare-task=` requires a task label or `all`.")?;
        let task = if value.trim().eq_ignore_ascii_case("all") {
            None
        } else {
            Some(value.to_string())
        };
        return Ok(Some(ParsedToken::CompareTask(task)));
    }
    Ok(None)
}

fn parse_compare_profile_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--compare-profile=") {
        let value = require_nonempty_key_value(
            value,
            "`--compare-profile=` requires `classic`, `deep-work`, `custom`, `unknown`, or `all`.",
        )?;
        return Ok(Some(ParsedToken::CompareProfile(
            parse_compare_profile_value(value)?,
        )));
    }
    Ok(None)
}

fn parse_compare_time_of_day_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--compare-time=") {
        let value = require_nonempty_key_value(
            value,
            "`--compare-time=` requires `morning`, `afternoon`, `evening`, `night`, `unknown`, or `all`.",
        )?;
        return Ok(Some(ParsedToken::CompareTimeOfDay(
            parse_compare_time_of_day_value(value)?,
        )));
    }
    Ok(None)
}

fn parse_compare_limit_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--compare-limit=") {
        let value = require_nonempty_key_value(
            value,
            "`--compare-limit=` requires a positive whole number.",
        )?;
        return Ok(Some(ParsedToken::CompareLimit(parse_compare_limit_value(
            value,
        )?)));
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

fn parse_blocklist_category_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--blocklist-category=") {
        let value =
            require_nonempty_key_value(value, "`--blocklist-category=` requires a category name.")?;
        return Ok(Some(ParsedToken::BlocklistCategory(Some(
            value.to_string(),
        ))));
    }
    Ok(None)
}

fn parse_blocklist_category_create_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--blocklist-category-create=") {
        let value = require_nonempty_key_value(
            value,
            "`--blocklist-category-create=` requires a category name.",
        )?;
        return Ok(Some(ParsedToken::BlocklistCategoryCreate(
            value.to_string(),
        )));
    }
    Ok(None)
}

fn parse_blocklist_category_rename_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--blocklist-category-rename=") {
        let value = require_nonempty_key_value(
            value,
            "`--blocklist-category-rename=` requires a category name.",
        )?;
        return Ok(Some(ParsedToken::BlocklistCategoryRename(
            value.to_string(),
        )));
    }
    Ok(None)
}

fn parse_session_template_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--session-template=") {
        let value =
            require_nonempty_key_value(value, "`--session-template=` requires a template name.")?;
        return Ok(Some(ParsedToken::SessionTemplate(Some(value.to_string()))));
    }
    Ok(None)
}

fn parse_session_template_apply_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--session-template-apply=") {
        let value = require_nonempty_key_value(
            value,
            "`--session-template-apply=` requires a template name.",
        )?;
        return Ok(Some(ParsedToken::SessionTemplateApply(Some(
            value.to_string(),
        ))));
    }
    Ok(None)
}

fn parse_session_template_create_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--session-template-create=") {
        let value =
            require_nonempty_key_value(value, "`--session-template-create=` requires a name.")?;
        return Ok(Some(ParsedToken::SessionTemplateCreate(value.to_string())));
    }
    Ok(None)
}

fn parse_session_template_rename_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--session-template-rename=") {
        let value =
            require_nonempty_key_value(value, "`--session-template-rename=` requires a name.")?;
        return Ok(Some(ParsedToken::SessionTemplateRename(value.to_string())));
    }
    Ok(None)
}

fn parse_history_dashboard_pin_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--history-dashboard-pin=") {
        let value =
            require_nonempty_key_value(value, "`--history-dashboard-pin=` requires a card ID.")?;
        return Ok(Some(ParsedToken::HistoryDashboardPin(
            parse_history_kpi_card_id(value)?,
        )));
    }
    Ok(None)
}

fn parse_history_dashboard_unpin_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--history-dashboard-unpin=") {
        let value =
            require_nonempty_key_value(value, "`--history-dashboard-unpin=` requires a card ID.")?;
        return Ok(Some(ParsedToken::HistoryDashboardUnpin(
            parse_history_kpi_card_id(value)?,
        )));
    }
    Ok(None)
}

fn parse_history_dashboard_order_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--history-dashboard-order=") {
        let value = require_nonempty_key_value(
            value,
            "`--history-dashboard-order=` requires a comma-separated card ID list.",
        )?;
        return Ok(Some(ParsedToken::HistoryDashboardOrder(
            parse_history_dashboard_order_value(value)?,
        )));
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

fn parse_allowlist_site_add_temporary_key_value_arg(
    arg: &str,
) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--allowlist-site-add-temporary=") {
        let value = require_nonempty_key_value(
            value,
            "`--allowlist-site-add-temporary=` requires HOST=30m style input.",
        )?;
        return Ok(Some(ParsedToken::AllowlistSiteAddTemporary(
            value.to_string(),
        )));
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
