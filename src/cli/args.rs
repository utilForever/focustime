use crate::cli::{
    KeyValueParser, OsString, OutputMode, ParsedToken, PathBuf, ValueArgParser, invalid_usage,
    parse_goal_carry_value, parse_goal_value, parse_monthly_goal_value, parse_profile_id,
    parse_schedule_value, parse_site_edit_value, parse_strict_value, parse_task_goal_value,
    parse_theme_preset, parse_watch_interval_secs, parse_weekly_goal_value,
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
    let parsers: [(&str, ValueArgParser); 25] = [
        ("--task", classify_task_arg),
        ("--task-goal", classify_task_goal_arg),
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
        ("--watch", classify_watch_arg),
        ("--backup", classify_backup_arg),
        ("--restore", classify_restore_arg),
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

fn classify_theme_arg(args: &[String], index: usize) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        let selected = parse_theme_preset(next)?;
        return Ok((ParsedToken::Theme(Some(selected)), 2));
    }
    Ok((ParsedToken::Theme(None), 1))
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
        return Ok((ParsedToken::Backup(Some(PathBuf::from(next))), 2));
    }
    Ok((ParsedToken::Backup(None), 1))
}

fn classify_restore_arg(args: &[String], index: usize) -> Result<(ParsedToken, usize), String> {
    if let Some(next) = args.get(index + 1)
        && !next.starts_with('-')
    {
        return Ok((ParsedToken::Restore(Some(PathBuf::from(next))), 2));
    }
    Ok((ParsedToken::Restore(None), 1))
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

pub(super) fn classify_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    let parsers: [KeyValueParser; 25] = [
        parse_task_key_value_arg,
        parse_task_goal_key_value_arg,
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
        parse_watch_key_value_arg,
        parse_backup_key_value_arg,
        parse_restore_key_value_arg,
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
