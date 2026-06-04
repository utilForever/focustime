use crate::cli::{
    KeyValueParser, ParsedToken, PathBuf, parse_automation_triggers_value, parse_compare_by_value,
    parse_compare_limit_value, parse_compare_profile_value, parse_compare_time_of_day_value,
    parse_daemon_port, parse_goal_carry_value, parse_goal_value,
    parse_history_dashboard_order_value, parse_history_kpi_card_id, parse_monthly_goal_value,
    parse_profile_id, parse_schedule_value, parse_site_edit_value, parse_strict_value,
    parse_task_goal_value, parse_theme_preset, parse_watch_interval_secs,
    parse_weekday_rules_value, parse_weekly_goal_value, require_nonempty_key_value,
};

pub(in crate::cli) fn classify_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    let parsers: [KeyValueParser; 47] = [
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
        parse_export_key_value_arg,
        parse_feature_inventory_key_value_arg,
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

fn parse_feature_inventory_key_value_arg(arg: &str) -> Result<Option<ParsedToken>, String> {
    if let Some(value) = arg.strip_prefix("--feature-inventory=") {
        let value = require_nonempty_key_value(
            value,
            "`--feature-inventory=` requires a target directory.",
        )?;
        return Ok(Some(ParsedToken::FeatureInventory(Some(PathBuf::from(
            value,
        )))));
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
            "`--compare-profile=` requires `basic`, `standard`, `advanced`, `unknown`, or `all`.",
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
