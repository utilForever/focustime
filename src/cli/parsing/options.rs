use crate::cli::{
    ComparisonDimension, DEFAULT_STATUS_COMPARISON_LIMIT, DEFAULT_WATCH_INTERVAL_SECS, ParsedToken,
    ProfileBucket, StatusComparisonOptions, TimeOfDayBucket,
};

use super::invalid_usage;

pub(in crate::cli) fn parse_watch_interval_option(
    tokens: &[ParsedToken],
) -> Result<Option<u64>, String> {
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

pub(in crate::cli) fn parse_daemon_port_option(
    tokens: &[ParsedToken],
) -> Result<Option<u16>, String> {
    let mut port: Option<u16> = None;
    for token in tokens {
        if let ParsedToken::DaemonPort(value) = token {
            if port.is_some() {
                return Err(invalid_usage("`--daemon-port` can only be specified once."));
            }
            port = Some(*value);
        }
    }
    Ok(port)
}

#[derive(Default)]
struct StatusComparisonParseFlags {
    has_dimension: bool,
    has_task: bool,
    has_profile: bool,
    has_time_of_day: bool,
    has_limit: bool,
    has_any_option: bool,
}

fn apply_status_comparison_token(
    token: &ParsedToken,
    options: &mut StatusComparisonOptions,
    flags: &mut StatusComparisonParseFlags,
) -> Result<(), String> {
    match token {
        ParsedToken::CompareBy(dimension) => {
            flags.has_any_option = true;
            mark_compare_option_seen(
                &mut flags.has_dimension,
                "`--compare-by` can only be specified once.",
            )?;
            options.dimension = *dimension;
        }
        ParsedToken::CompareTask(task) => {
            flags.has_any_option = true;
            mark_compare_option_seen(
                &mut flags.has_task,
                "`--compare-task` can only be specified once.",
            )?;
            options.task_label = task.clone();
        }
        ParsedToken::CompareProfile(profile) => {
            flags.has_any_option = true;
            mark_compare_option_seen(
                &mut flags.has_profile,
                "`--compare-profile` can only be specified once.",
            )?;
            options.profile = *profile;
        }
        ParsedToken::CompareTimeOfDay(bucket) => {
            flags.has_any_option = true;
            mark_compare_option_seen(
                &mut flags.has_time_of_day,
                "`--compare-time` can only be specified once.",
            )?;
            options.time_of_day = *bucket;
        }
        ParsedToken::CompareLimit(limit) => {
            flags.has_any_option = true;
            mark_compare_option_seen(
                &mut flags.has_limit,
                "`--compare-limit` can only be specified once.",
            )?;
            options.limit = *limit;
        }
        _ => {}
    }
    Ok(())
}

fn mark_compare_option_seen(flag: &mut bool, message: &str) -> Result<(), String> {
    if *flag {
        return Err(invalid_usage(message));
    }
    *flag = true;
    Ok(())
}

pub(in crate::cli) fn parse_status_comparison_options(
    tokens: &[ParsedToken],
) -> Result<(StatusComparisonOptions, bool), String> {
    let mut options = StatusComparisonOptions::default();
    let mut flags = StatusComparisonParseFlags::default();

    for token in tokens {
        apply_status_comparison_token(token, &mut options, &mut flags)?;
    }

    if options.limit == 0 {
        return Err(invalid_usage(
            "`--compare-limit` requires a positive whole number.",
        ));
    }
    if !flags.has_limit {
        options.limit = DEFAULT_STATUS_COMPARISON_LIMIT;
    }
    Ok((options, flags.has_any_option))
}

pub(in crate::cli) fn parse_compare_by_value(value: &str) -> Result<ComparisonDimension, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "task" | "task-label" | "task_label" => Ok(ComparisonDimension::TaskLabel),
        "profile" => Ok(ComparisonDimension::Profile),
        "time-of-day" | "time_of_day" | "timeofday" | "time" => Ok(ComparisonDimension::TimeOfDay),
        _ => Err(invalid_usage(&format!(
            "Invalid compare dimension `{value}`. Use `task`, `profile`, or `time-of-day`."
        ))),
    }
}

pub(in crate::cli) fn parse_compare_profile_value(
    value: &str,
) -> Result<Option<ProfileBucket>, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "all" => Ok(None),
        "basic" | "classic" => Ok(Some(ProfileBucket::Classic)),
        "standard" | "deep-work" | "deep_work" | "deepwork" => Ok(Some(ProfileBucket::DeepWork)),
        "advanced" | "custom" => Ok(Some(ProfileBucket::Custom)),
        "unknown" => Ok(Some(ProfileBucket::Unknown)),
        _ => Err(invalid_usage(&format!(
            "Invalid compare profile `{value}`. Use `basic`, `standard`, `advanced`, `unknown`, or `all`."
        ))),
    }
}

pub(in crate::cli) fn parse_compare_time_of_day_value(
    value: &str,
) -> Result<Option<TimeOfDayBucket>, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "all" => Ok(None),
        "morning" => Ok(Some(TimeOfDayBucket::Morning)),
        "afternoon" => Ok(Some(TimeOfDayBucket::Afternoon)),
        "evening" => Ok(Some(TimeOfDayBucket::Evening)),
        "night" => Ok(Some(TimeOfDayBucket::Night)),
        "unknown" => Ok(Some(TimeOfDayBucket::Unknown)),
        _ => Err(invalid_usage(&format!(
            "Invalid compare time bucket `{value}`. Use `morning`, `afternoon`, `evening`, `night`, `unknown`, or `all`."
        ))),
    }
}

pub(in crate::cli) fn parse_compare_limit_value(value: &str) -> Result<usize, String> {
    let trimmed = value.trim();
    let limit = trimmed
        .parse::<usize>()
        .map_err(|_| invalid_usage("`--compare-limit` requires a positive whole number."))?;
    if limit == 0 {
        return Err(invalid_usage(
            "`--compare-limit` requires a positive whole number.",
        ));
    }
    Ok(limit)
}

pub(in crate::cli) fn parse_watch_interval_secs(value: &str) -> Result<u64, String> {
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

pub(in crate::cli) fn parse_daemon_port(value: &str) -> Result<u16, String> {
    let trimmed = value.trim();
    let port = trimmed
        .parse::<u16>()
        .map_err(|_| invalid_usage("`--daemon-port` requires a port between 1 and 65535."))?;
    if port == 0 {
        return Err(invalid_usage(
            "`--daemon-port` requires a port between 1 and 65535.",
        ));
    }
    Ok(port)
}

pub(in crate::cli) fn require_nonempty_key_value<'a>(
    value: &'a str,
    message: &str,
) -> Result<&'a str, String> {
    if value.trim().is_empty() {
        return Err(invalid_usage(message));
    }
    Ok(value)
}
