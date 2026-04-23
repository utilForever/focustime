use std::collections::HashSet;

use chrono::{
    DateTime, Datelike, Duration, Local, LocalResult, NaiveDate, TimeZone, Timelike, Weekday,
};

use crate::config::RecurringFocusWindowConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecurringWindow {
    days: Vec<Weekday>,
    start_minutes: u16,
    end_minutes: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowOccurrence {
    pub window_index: usize,
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
}

pub fn compile_windows(config_windows: &[RecurringFocusWindowConfig]) -> Vec<RecurringWindow> {
    config_windows
        .iter()
        .filter_map(RecurringWindow::from_config)
        .collect()
}

pub fn compile_exception_dates(config_dates: &[String]) -> HashSet<NaiveDate> {
    config_dates
        .iter()
        .filter_map(|value| parse_exception_date(value))
        .collect()
}

pub fn occurrence_key(occurrence: &WindowOccurrence) -> String {
    format!(
        "{}-{}",
        occurrence.window_index,
        occurrence.start.timestamp()
    )
}

pub fn active_occurrence(
    now: DateTime<Local>,
    windows: &[RecurringWindow],
    exception_dates: &HashSet<NaiveDate>,
) -> Option<WindowOccurrence> {
    let now_minutes = now.hour() as u16 * 60 + now.minute() as u16;
    let today = now.date_naive();
    if exception_dates.contains(&today) {
        return None;
    }
    let mut selected: Option<WindowOccurrence> = None;

    for (window_index, window) in windows.iter().enumerate() {
        if !window.days.contains(&now.weekday()) {
            continue;
        }
        if now_minutes < window.start_minutes || now_minutes >= window.end_minutes {
            continue;
        }
        let Some(start) = local_datetime_on(today, window.start_minutes) else {
            continue;
        };
        let Some(end) = local_datetime_on(today, window.end_minutes) else {
            continue;
        };
        if now < start || now >= end {
            continue;
        }

        let candidate = WindowOccurrence {
            window_index,
            start,
            end,
        };
        let should_replace = selected
            .as_ref()
            .is_none_or(|existing| candidate.start < existing.start);
        if should_replace {
            selected = Some(candidate);
        }
    }

    selected
}

pub fn next_occurrence_after(
    now: DateTime<Local>,
    windows: &[RecurringWindow],
    exception_dates: &HashSet<NaiveDate>,
) -> Option<WindowOccurrence> {
    let mut selected: Option<WindowOccurrence> = None;
    let today = now.date_naive();

    for day_offset in 0..=7 {
        let date = today + Duration::days(day_offset);
        if exception_dates.contains(&date) {
            continue;
        }
        for (window_index, window) in windows.iter().enumerate() {
            if !window.days.contains(&date.weekday()) {
                continue;
            }
            let Some(start) = local_datetime_on(date, window.start_minutes) else {
                continue;
            };
            if start <= now {
                continue;
            }
            let Some(end) = local_datetime_on(date, window.end_minutes) else {
                continue;
            };
            let candidate = WindowOccurrence {
                window_index,
                start,
                end,
            };
            let should_replace = selected
                .as_ref()
                .is_none_or(|existing| candidate.start < existing.start);
            if should_replace {
                selected = Some(candidate);
            }
        }
    }

    selected
}

impl RecurringWindow {
    fn from_config(config: &RecurringFocusWindowConfig) -> Option<Self> {
        let days = parse_weekdays(&config.days);
        if days.is_empty() {
            return None;
        }
        let start_minutes = parse_time_minutes(&config.start)?;
        let end_minutes = parse_time_minutes(&config.end)?;
        if start_minutes >= end_minutes {
            return None;
        }
        Some(Self {
            days,
            start_minutes,
            end_minutes,
        })
    }
}

fn parse_weekdays(raw_days: &[String]) -> Vec<Weekday> {
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

fn parse_weekday(raw: &str) -> Option<Weekday> {
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

fn parse_time_minutes(raw: &str) -> Option<u16> {
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

fn parse_exception_date(raw: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d").ok()
}

fn local_datetime_on(date: NaiveDate, total_minutes: u16) -> Option<DateTime<Local>> {
    let hour = u32::from(total_minutes / 60);
    let minute = u32::from(total_minutes % 60);
    match Local.with_ymd_and_hms(date.year(), date.month(), date.day(), hour, minute, 0) {
        LocalResult::Single(dt) => Some(dt),
        LocalResult::Ambiguous(earliest, _) => Some(earliest),
        LocalResult::None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_datetime(date: NaiveDate, hour: u32, minute: u32) -> DateTime<Local> {
        match Local.with_ymd_and_hms(date.year(), date.month(), date.day(), hour, minute, 0) {
            LocalResult::Single(dt) => dt,
            LocalResult::Ambiguous(earliest, _) => earliest,
            LocalResult::None => panic!("local datetime should be representable in tests"),
        }
    }

    #[test]
    fn compile_windows_ignores_invalid_entries() {
        let windows = vec![
            RecurringFocusWindowConfig {
                days: vec!["mon".to_string()],
                start: "09:00".to_string(),
                end: "10:00".to_string(),
            },
            RecurringFocusWindowConfig {
                days: vec!["???".to_string()],
                start: "09:00".to_string(),
                end: "10:00".to_string(),
            },
            RecurringFocusWindowConfig {
                days: vec!["tue".to_string()],
                start: "22:00".to_string(),
                end: "08:00".to_string(),
            },
        ];

        let compiled = compile_windows(&windows);

        assert_eq!(compiled.len(), 1);
    }

    #[test]
    fn active_occurrence_returns_window_when_inside_range() {
        let date = Local::now().date_naive();
        let now = local_datetime(date, 10, 15);
        let windows = compile_windows(&[RecurringFocusWindowConfig {
            days: vec![date.weekday().to_string()],
            start: "10:00".to_string(),
            end: "11:00".to_string(),
        }]);

        let active =
            active_occurrence(now, &windows, &HashSet::new()).expect("window should be active");

        assert_eq!(active.start, local_datetime(date, 10, 0));
        assert_eq!(active.end, local_datetime(date, 11, 0));
    }

    #[test]
    fn next_occurrence_after_skips_current_or_past_start_times() {
        let date = Local::now().date_naive();
        let now = local_datetime(date, 10, 15);
        let windows = compile_windows(&[
            RecurringFocusWindowConfig {
                days: vec![date.weekday().to_string()],
                start: "09:00".to_string(),
                end: "09:30".to_string(),
            },
            RecurringFocusWindowConfig {
                days: vec![date.weekday().to_string()],
                start: "11:00".to_string(),
                end: "12:00".to_string(),
            },
        ]);

        let next = next_occurrence_after(now, &windows, &HashSet::new())
            .expect("next window should exist");

        assert_eq!(next.start, local_datetime(date, 11, 0));
        assert_eq!(next.end, local_datetime(date, 12, 0));
    }

    #[test]
    fn active_and_next_occurrences_skip_exception_dates() {
        let date = Local::now().date_naive();
        let now = local_datetime(date, 10, 15);
        let tomorrow = date.succ_opt().expect("tomorrow should be representable");
        let windows = compile_windows(&[
            RecurringFocusWindowConfig {
                days: vec![date.weekday().to_string()],
                start: "10:00".to_string(),
                end: "11:00".to_string(),
            },
            RecurringFocusWindowConfig {
                days: vec![tomorrow.weekday().to_string()],
                start: "09:00".to_string(),
                end: "10:00".to_string(),
            },
        ]);
        let exception_dates = compile_exception_dates(&[date.format("%Y-%m-%d").to_string()]);

        assert!(active_occurrence(now, &windows, &exception_dates).is_none());

        let next = next_occurrence_after(now, &windows, &exception_dates)
            .expect("next window should skip exception date");
        assert_eq!(next.start, local_datetime(tomorrow, 9, 0));
        assert_eq!(next.end, local_datetime(tomorrow, 10, 0));
    }
}
