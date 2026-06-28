use crate::cli::{
    KeyValueParser, ParsedToken, PathBuf, parse_goal_carry_value, parse_goal_value,
    parse_monthly_goal_value, parse_profile_id, parse_schedule_value, parse_site_edit_value,
    parse_strict_value, parse_theme_preset, parse_watch_interval_secs, parse_weekly_goal_value,
    require_nonempty_key_value,
};

pub(in crate::cli) fn classify_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    let parsers: [KeyValueParser; 21] = [
        parse_task_key_value_arg,
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
    parse_artifact_key_value_arg(
        arg,
        "--export=",
        ParsedToken::Export,
        "`--export=` requires a target directory.",
    )
}

fn parse_backup_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    parse_artifact_key_value_arg(
        arg,
        "--backup=",
        ParsedToken::Backup,
        "`--backup=` requires a target directory.",
    )
}

fn parse_restore_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    parse_artifact_key_value_arg(
        arg,
        "--restore=",
        ParsedToken::Restore,
        "`--restore=` requires a source directory.",
    )
}

fn parse_artifact_key_value_arg(
    arg: &str,
    prefix: &'static str,
    token: fn(Option<PathBuf>) -> ParsedToken,
    empty_value_message: &'static str,
) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix(prefix) {
        let value = require_nonempty_key_value(value, empty_value_message)?;
        return Ok(Some(token(Some(PathBuf::from(value)))));
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
