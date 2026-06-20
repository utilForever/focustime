use crate::cli::{
    DailyGoalConfig, MonthlyGoalConfig, NaiveDate, OneTimeFocusWindowConfig, ProfileId,
    RecurringFocusWindowConfig, RecurringScheduleConfig, SiteEditValue, ThemePreset,
    WeeklyGoalConfig,
};

use super::{invalid_usage, require_nonempty_key_value};

pub(in crate::cli) fn parse_profile_id(value: &str) -> Result<ProfileId, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "basic" | "classic" => Ok(ProfileId::Classic),
        "standard" | "deep-work" | "deep_work" | "deepwork" => Ok(ProfileId::DeepWork),
        "advanced" | "custom" => Ok(ProfileId::Custom),
        _ => Err(invalid_usage(&format!(
            "Invalid profile `{value}`. Use `basic`, `standard`, or `advanced`."
        ))),
    }
}

pub(in crate::cli) fn parse_theme_preset(value: &str) -> Result<ThemePreset, String> {
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

pub(in crate::cli) fn parse_task_goal_value(
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

pub(in crate::cli) fn parse_goal_value(value: &str) -> Result<DailyGoalConfig, String> {
    let (minutes, pomodoros) = parse_goal_components(value, "--goal")?;
    Ok(DailyGoalConfig { minutes, pomodoros })
}

pub(in crate::cli) fn parse_weekly_goal_value(value: &str) -> Result<WeeklyGoalConfig, String> {
    let (minutes, pomodoros) = parse_goal_components(value, "--goal-weekly")?;
    Ok(WeeklyGoalConfig { minutes, pomodoros })
}

pub(in crate::cli) fn parse_monthly_goal_value(value: &str) -> Result<MonthlyGoalConfig, String> {
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

pub(in crate::cli) fn parse_strict_value(value: &str) -> Result<bool, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err(invalid_usage(&format!(
            "Invalid strict mode `{value}`. Use `--strict=on` or `--strict=off`."
        ))),
    }
}

pub(in crate::cli) fn parse_goal_carry_value(value: &str) -> Result<bool, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err(invalid_usage(&format!(
            "Invalid goal carry-over `{value}`. Use `on` or `off`."
        ))),
    }
}

pub(in crate::cli) fn parse_site_edit_value(value: &str) -> Result<SiteEditValue, String> {
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

pub(in crate::cli) fn parse_schedule_value(value: &str) -> Result<RecurringScheduleConfig, String> {
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

/// Checks whether a schedule weekday token is recognized by the CLI parser.
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
