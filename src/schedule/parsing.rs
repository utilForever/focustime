use chrono::{NaiveDate, Weekday};

pub(super) fn parse_weekdays(raw_days: &[String]) -> Vec<Weekday> {
    let mut weekdays = Vec::new();
    for raw in raw_days {
        let Some(day) = parse_weekday(raw) else {
            continue;
        };
        if !weekdays.contains(&day) {
            weekdays.push(day);
        }
    }
    weekdays
}

pub(super) fn parse_weekday(raw: &str) -> Option<Weekday> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "mon" | "monday" => Some(Weekday::Mon),
        "tue" | "tues" | "tuesday" => Some(Weekday::Tue),
        "wed" | "wednesday" => Some(Weekday::Wed),
        "thu" | "thurs" | "thursday" => Some(Weekday::Thu),
        "fri" | "friday" => Some(Weekday::Fri),
        "sat" | "saturday" => Some(Weekday::Sat),
        "sun" | "sunday" => Some(Weekday::Sun),
        _ => None,
    }
}

pub(super) fn parse_time_minutes(raw: &str) -> Option<u16> {
    let trimmed = raw.trim();
    let (hours, minutes) = trimmed.split_once(':')?;
    if hours.len() != 2 || minutes.len() != 2 {
        return None;
    }
    let hour = hours.parse::<u16>().ok()?;
    let minute = minutes.parse::<u16>().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some(hour * 60 + minute)
}

pub(super) fn parse_exception_date(raw: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d").ok()
}
