use std::collections::HashSet;

use chrono::{
    DateTime, Datelike, Duration, Local, LocalResult, NaiveDate, TimeZone, Timelike, Weekday,
};

use crate::config::{
    OneTimeFocusWindowConfig, RecurringFocusWindowConfig, RecurringScheduleConfig,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecurringWindow {
    days: Vec<Weekday>,
    start_minutes: u16,
    end_minutes: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OneTimeWindow {
    pub date: NaiveDate,
    start_minutes: u16,
    end_minutes: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowOccurrenceKind {
    Recurring,
    OneTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowOccurrence {
    pub kind: WindowOccurrenceKind,
    pub window_index: usize,
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleConflictContext {
    Weekday(Weekday),
    Date(NaiveDate),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleConflict {
    pub first_kind: WindowOccurrenceKind,
    pub first_window_index: usize,
    pub second_kind: WindowOccurrenceKind,
    pub second_window_index: usize,
    pub context: ScheduleConflictContext,
    pub overlap_start_minutes: u16,
    pub overlap_end_minutes: u16,
}

pub fn compile_windows(config_windows: &[RecurringFocusWindowConfig]) -> Vec<RecurringWindow> {
    config_windows
        .iter()
        .filter_map(RecurringWindow::from_config)
        .collect()
}

pub fn compile_one_time_windows(config_windows: &[OneTimeFocusWindowConfig]) -> Vec<OneTimeWindow> {
    config_windows
        .iter()
        .filter_map(OneTimeWindow::from_config)
        .collect()
}

pub fn compile_exception_dates(config_dates: &[String]) -> HashSet<NaiveDate> {
    config_dates
        .iter()
        .filter_map(|value| parse_exception_date(value))
        .collect()
}

pub fn inspect_schedule_conflicts_from_config(
    schedule: &RecurringScheduleConfig,
) -> Vec<ScheduleConflict> {
    let recurring_windows = compile_windows(&schedule.windows);
    let one_time_windows = compile_one_time_windows(&schedule.one_time_windows);
    let recurring_exception_dates = compile_exception_dates(&schedule.exception_dates);
    inspect_schedule_conflicts(
        &recurring_windows,
        &one_time_windows,
        &recurring_exception_dates,
    )
}

pub fn inspect_schedule_conflicts(
    recurring_windows: &[RecurringWindow],
    one_time_windows: &[OneTimeWindow],
    recurring_exception_dates: &HashSet<NaiveDate>,
) -> Vec<ScheduleConflict> {
    let mut conflicts = Vec::new();
    conflicts.extend(find_recurring_conflicts(recurring_windows));
    conflicts.extend(find_one_time_conflicts(one_time_windows));
    conflicts.extend(find_recurring_one_time_conflicts(
        recurring_windows,
        one_time_windows,
        recurring_exception_dates,
    ));

    conflicts.sort_by_key(conflict_sort_key);
    conflicts
}

pub fn format_schedule_conflict(conflict: &ScheduleConflict) -> String {
    let context = match conflict.context {
        ScheduleConflictContext::Weekday(day) => day_token(day).to_uppercase(),
        ScheduleConflictContext::Date(date) => date.format("%Y-%m-%d").to_string(),
    };
    format!(
        "{context}: {} #{} overlaps {} #{} at {}-{}",
        conflict_kind_label(conflict.first_kind),
        conflict.first_window_index + 1,
        conflict_kind_label(conflict.second_kind),
        conflict.second_window_index + 1,
        format_minutes(conflict.overlap_start_minutes),
        format_minutes(conflict.overlap_end_minutes)
    )
}

pub fn occurrence_key(occurrence: &WindowOccurrence) -> String {
    let kind_key = match occurrence.kind {
        WindowOccurrenceKind::Recurring => "r",
        WindowOccurrenceKind::OneTime => "o",
    };
    format!(
        "{kind_key}-{}-{}",
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
            kind: WindowOccurrenceKind::Recurring,
            window_index,
            start,
            end,
        };
        let should_replace = selected
            .as_ref()
            .is_none_or(|existing| active_occurrence_is_higher_priority(&candidate, existing));
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
    let future_exception_count = exception_dates
        .iter()
        .filter(|date| **date >= today)
        .count();
    let search_days = 7_i64.saturating_mul((future_exception_count as i64).saturating_add(1));

    for day_offset in 0..=search_days {
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
                kind: WindowOccurrenceKind::Recurring,
                window_index,
                start,
                end,
            };
            let should_replace = selected
                .as_ref()
                .is_none_or(|existing| next_occurrence_is_higher_priority(&candidate, existing));
            if should_replace {
                selected = Some(candidate);
            }
        }
    }

    selected
}

pub fn active_one_time_occurrence(
    now: DateTime<Local>,
    windows: &[OneTimeWindow],
) -> Option<WindowOccurrence> {
    let now_minutes = now.hour() as u16 * 60 + now.minute() as u16;
    let today = now.date_naive();
    let mut selected: Option<WindowOccurrence> = None;

    for (window_index, window) in windows.iter().enumerate() {
        if window.date != today {
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
            kind: WindowOccurrenceKind::OneTime,
            window_index,
            start,
            end,
        };
        let should_replace = selected
            .as_ref()
            .is_none_or(|existing| active_occurrence_is_higher_priority(&candidate, existing));
        if should_replace {
            selected = Some(candidate);
        }
    }

    selected
}

pub fn next_one_time_occurrence_after(
    now: DateTime<Local>,
    windows: &[OneTimeWindow],
) -> Option<WindowOccurrence> {
    let mut selected: Option<WindowOccurrence> = None;

    for (window_index, window) in windows.iter().enumerate() {
        let Some(start) = local_datetime_on(window.date, window.start_minutes) else {
            continue;
        };
        if start <= now {
            continue;
        }
        let Some(end) = local_datetime_on(window.date, window.end_minutes) else {
            continue;
        };
        let candidate = WindowOccurrence {
            kind: WindowOccurrenceKind::OneTime,
            window_index,
            start,
            end,
        };
        let should_replace = selected
            .as_ref()
            .is_none_or(|existing| next_occurrence_is_higher_priority(&candidate, existing));
        if should_replace {
            selected = Some(candidate);
        }
    }

    selected
}

pub fn pick_active_occurrence(
    first: Option<WindowOccurrence>,
    second: Option<WindowOccurrence>,
) -> Option<WindowOccurrence> {
    match (first, second) {
        (Some(first), Some(second)) => {
            if active_occurrence_is_higher_priority(&first, &second) {
                Some(first)
            } else {
                Some(second)
            }
        }
        (Some(first), None) => Some(first),
        (None, Some(second)) => Some(second),
        (None, None) => None,
    }
}

pub fn pick_next_occurrence(
    first: Option<WindowOccurrence>,
    second: Option<WindowOccurrence>,
) -> Option<WindowOccurrence> {
    match (first, second) {
        (Some(first), Some(second)) => {
            if next_occurrence_is_higher_priority(&first, &second) {
                Some(first)
            } else {
                Some(second)
            }
        }
        (Some(first), None) => Some(first),
        (None, Some(second)) => Some(second),
        (None, None) => None,
    }
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

impl OneTimeWindow {
    fn from_config(config: &OneTimeFocusWindowConfig) -> Option<Self> {
        let date = parse_exception_date(&config.date)?;
        let start_minutes = parse_time_minutes(&config.start)?;
        let end_minutes = parse_time_minutes(&config.end)?;
        if start_minutes >= end_minutes {
            return None;
        }
        Some(Self {
            date,
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

fn active_occurrence_is_higher_priority(
    candidate: &WindowOccurrence,
    existing: &WindowOccurrence,
) -> bool {
    candidate.start > existing.start
        || (candidate.start == existing.start && occurrence_tie_break(candidate, existing))
}

fn next_occurrence_is_higher_priority(
    candidate: &WindowOccurrence,
    existing: &WindowOccurrence,
) -> bool {
    candidate.start < existing.start
        || (candidate.start == existing.start && occurrence_tie_break(candidate, existing))
}

fn occurrence_tie_break(candidate: &WindowOccurrence, existing: &WindowOccurrence) -> bool {
    if candidate.kind != existing.kind {
        return candidate.kind == WindowOccurrenceKind::OneTime;
    }
    candidate.window_index < existing.window_index
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

fn shared_weekdays(first: &RecurringWindow, second: &RecurringWindow) -> Vec<Weekday> {
    let mut shared = Vec::new();
    for day in &first.days {
        if second.days.contains(day) {
            shared.push(*day);
        }
    }
    shared.sort_by_key(|day| day.num_days_from_monday());
    shared
}

fn overlap_minutes(
    first_start_minutes: u16,
    first_end_minutes: u16,
    second_start_minutes: u16,
    second_end_minutes: u16,
) -> Option<(u16, u16)> {
    let overlap_start = first_start_minutes.max(second_start_minutes);
    let overlap_end = first_end_minutes.min(second_end_minutes);
    if overlap_start < overlap_end {
        Some((overlap_start, overlap_end))
    } else {
        None
    }
}

fn conflict_sort_key(conflict: &ScheduleConflict) -> (u8, i32, u16, u16, u8, usize, u8, usize) {
    let context_key = match conflict.context {
        ScheduleConflictContext::Weekday(day) => (0_u8, day.num_days_from_monday() as i32),
        ScheduleConflictContext::Date(date) => (1_u8, date.num_days_from_ce()),
    };
    (
        context_key.0,
        context_key.1,
        conflict.overlap_start_minutes,
        conflict.overlap_end_minutes,
        conflict_kind_rank(conflict.first_kind),
        conflict.first_window_index,
        conflict_kind_rank(conflict.second_kind),
        conflict.second_window_index,
    )
}

fn conflict_kind_rank(kind: WindowOccurrenceKind) -> u8 {
    match kind {
        WindowOccurrenceKind::Recurring => 0,
        WindowOccurrenceKind::OneTime => 1,
    }
}

fn conflict_kind_label(kind: WindowOccurrenceKind) -> &'static str {
    match kind {
        WindowOccurrenceKind::Recurring => "recurring",
        WindowOccurrenceKind::OneTime => "one-time",
    }
}

fn day_token(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "mon",
        Weekday::Tue => "tue",
        Weekday::Wed => "wed",
        Weekday::Thu => "thu",
        Weekday::Fri => "fri",
        Weekday::Sat => "sat",
        Weekday::Sun => "sun",
    }
}

fn format_minutes(total_minutes: u16) -> String {
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    format!("{hours:02}:{minutes:02}")
}

fn find_recurring_conflicts(recurring_windows: &[RecurringWindow]) -> Vec<ScheduleConflict> {
    let mut conflicts = Vec::new();
    for (first_index, first) in recurring_windows.iter().enumerate() {
        for (second_index, second) in recurring_windows.iter().enumerate().skip(first_index + 1) {
            let Some((overlap_start, overlap_end)) = overlap_minutes(
                first.start_minutes,
                first.end_minutes,
                second.start_minutes,
                second.end_minutes,
            ) else {
                continue;
            };
            for day in shared_weekdays(first, second) {
                conflicts.push(ScheduleConflict {
                    first_kind: WindowOccurrenceKind::Recurring,
                    first_window_index: first_index,
                    second_kind: WindowOccurrenceKind::Recurring,
                    second_window_index: second_index,
                    context: ScheduleConflictContext::Weekday(day),
                    overlap_start_minutes: overlap_start,
                    overlap_end_minutes: overlap_end,
                });
            }
        }
    }
    conflicts
}

fn find_one_time_conflicts(one_time_windows: &[OneTimeWindow]) -> Vec<ScheduleConflict> {
    let mut conflicts = Vec::new();
    for (first_index, first) in one_time_windows.iter().enumerate() {
        for (second_index, second) in one_time_windows.iter().enumerate().skip(first_index + 1) {
            if first.date != second.date {
                continue;
            }
            let Some((overlap_start, overlap_end)) = overlap_minutes(
                first.start_minutes,
                first.end_minutes,
                second.start_minutes,
                second.end_minutes,
            ) else {
                continue;
            };
            conflicts.push(ScheduleConflict {
                first_kind: WindowOccurrenceKind::OneTime,
                first_window_index: first_index,
                second_kind: WindowOccurrenceKind::OneTime,
                second_window_index: second_index,
                context: ScheduleConflictContext::Date(first.date),
                overlap_start_minutes: overlap_start,
                overlap_end_minutes: overlap_end,
            });
        }
    }
    conflicts
}

fn find_recurring_one_time_conflicts(
    recurring_windows: &[RecurringWindow],
    one_time_windows: &[OneTimeWindow],
    recurring_exception_dates: &HashSet<NaiveDate>,
) -> Vec<ScheduleConflict> {
    let mut conflicts = Vec::new();
    for (recurring_index, recurring_window) in recurring_windows.iter().enumerate() {
        for (one_time_index, one_time_window) in one_time_windows.iter().enumerate() {
            if recurring_exception_dates.contains(&one_time_window.date) {
                continue;
            }
            if !recurring_window
                .days
                .contains(&one_time_window.date.weekday())
            {
                continue;
            }
            let Some((overlap_start, overlap_end)) = overlap_minutes(
                recurring_window.start_minutes,
                recurring_window.end_minutes,
                one_time_window.start_minutes,
                one_time_window.end_minutes,
            ) else {
                continue;
            };
            conflicts.push(ScheduleConflict {
                first_kind: WindowOccurrenceKind::Recurring,
                first_window_index: recurring_index,
                second_kind: WindowOccurrenceKind::OneTime,
                second_window_index: one_time_index,
                context: ScheduleConflictContext::Date(one_time_window.date),
                overlap_start_minutes: overlap_start,
                overlap_end_minutes: overlap_end,
            });
        }
    }
    conflicts
}

#[cfg(test)]
mod tests {
    use crate::schedule::*;

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
    fn compile_one_time_windows_ignores_invalid_entries() {
        let windows = vec![
            OneTimeFocusWindowConfig {
                date: "2026-04-27".to_string(),
                start: "10:00".to_string(),
                end: "11:00".to_string(),
            },
            OneTimeFocusWindowConfig {
                date: "not-a-date".to_string(),
                start: "10:00".to_string(),
                end: "11:00".to_string(),
            },
            OneTimeFocusWindowConfig {
                date: "2026-04-27".to_string(),
                start: "12:00".to_string(),
                end: "11:00".to_string(),
            },
        ];

        let compiled = compile_one_time_windows(&windows);

        assert_eq!(compiled.len(), 1);
        assert_eq!(
            compiled[0].date,
            NaiveDate::from_ymd_opt(2026, 4, 27).expect("valid date literal")
        );
    }

    #[test]
    fn compile_exception_dates_ignores_invalid_and_deduplicates() {
        let dates = compile_exception_dates(&[
            " 2026-12-25 ".to_string(),
            "2026-12-25".to_string(),
            "not-a-date".to_string(),
            "".to_string(),
        ]);

        assert_eq!(dates.len(), 1);
        assert!(
            dates.contains(&NaiveDate::from_ymd_opt(2026, 12, 25).expect("valid date literal"))
        );
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

        assert_eq!(active.kind, WindowOccurrenceKind::Recurring);
        assert_eq!(active.start, local_datetime(date, 10, 0));
        assert_eq!(active.end, local_datetime(date, 11, 0));
    }

    #[test]
    fn active_one_time_occurrence_returns_window_when_inside_range() {
        let date = Local::now().date_naive();
        let now = local_datetime(date, 10, 15);
        let windows = compile_one_time_windows(&[OneTimeFocusWindowConfig {
            date: date.format("%Y-%m-%d").to_string(),
            start: "10:00".to_string(),
            end: "11:00".to_string(),
        }]);

        let active =
            active_one_time_occurrence(now, &windows).expect("one-time window should be active");

        assert_eq!(active.kind, WindowOccurrenceKind::OneTime);
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

    #[test]
    fn active_occurrence_prefers_most_recent_overlap_start() {
        let date = Local::now().date_naive();
        let now = local_datetime(date, 10, 45);
        let windows = compile_windows(&[
            RecurringFocusWindowConfig {
                days: vec![date.weekday().to_string()],
                start: "10:00".to_string(),
                end: "11:30".to_string(),
            },
            RecurringFocusWindowConfig {
                days: vec![date.weekday().to_string()],
                start: "10:30".to_string(),
                end: "12:00".to_string(),
            },
        ]);

        let active =
            active_occurrence(now, &windows, &HashSet::new()).expect("window should be active");

        assert_eq!(active.window_index, 1);
        assert_eq!(active.start, local_datetime(date, 10, 30));
        assert_eq!(active.end, local_datetime(date, 12, 0));
    }

    #[test]
    fn active_occurrence_tie_on_start_prefers_first_window_index() {
        let date = Local::now().date_naive();
        let now = local_datetime(date, 10, 15);
        let windows = compile_windows(&[
            RecurringFocusWindowConfig {
                days: vec![date.weekday().to_string()],
                start: "10:00".to_string(),
                end: "11:00".to_string(),
            },
            RecurringFocusWindowConfig {
                days: vec![date.weekday().to_string()],
                start: "10:00".to_string(),
                end: "10:30".to_string(),
            },
        ]);

        let active =
            active_occurrence(now, &windows, &HashSet::new()).expect("window should be active");

        assert_eq!(active.window_index, 0);
        assert_eq!(active.start, local_datetime(date, 10, 0));
        assert_eq!(active.end, local_datetime(date, 11, 0));
    }

    #[test]
    fn next_occurrence_after_tie_on_start_prefers_first_window_index() {
        let date = Local::now().date_naive();
        let now = local_datetime(date, 9, 45);
        let windows = compile_windows(&[
            RecurringFocusWindowConfig {
                days: vec![date.weekday().to_string()],
                start: "10:00".to_string(),
                end: "11:00".to_string(),
            },
            RecurringFocusWindowConfig {
                days: vec![date.weekday().to_string()],
                start: "10:00".to_string(),
                end: "10:30".to_string(),
            },
        ]);

        let next = next_occurrence_after(now, &windows, &HashSet::new())
            .expect("next window should exist for tied start");

        assert_eq!(next.window_index, 0);
        assert_eq!(next.start, local_datetime(date, 10, 0));
        assert_eq!(next.end, local_datetime(date, 11, 0));
    }

    #[test]
    fn next_occurrence_after_skips_weekly_exception_and_returns_following_week() {
        let date = Local::now().date_naive();
        let now = local_datetime(date, 12, 15);
        let next_week = date + Duration::days(7);
        let following_week = date + Duration::days(14);
        let windows = compile_windows(&[RecurringFocusWindowConfig {
            days: vec![date.weekday().to_string()],
            start: "11:00".to_string(),
            end: "12:00".to_string(),
        }]);
        let exception_dates = compile_exception_dates(&[next_week.format("%Y-%m-%d").to_string()]);

        let next = next_occurrence_after(now, &windows, &exception_dates)
            .expect("next window should skip the excepted weekly occurrence");

        assert_eq!(next.start, local_datetime(following_week, 11, 0));
        assert_eq!(next.end, local_datetime(following_week, 12, 0));
    }

    #[test]
    fn pick_active_occurrence_prefers_one_time_on_equal_start() {
        let date = Local::now().date_naive();
        let recurring = WindowOccurrence {
            kind: WindowOccurrenceKind::Recurring,
            window_index: 0,
            start: local_datetime(date, 10, 0),
            end: local_datetime(date, 11, 0),
        };
        let one_time = WindowOccurrence {
            kind: WindowOccurrenceKind::OneTime,
            window_index: 0,
            start: local_datetime(date, 10, 0),
            end: local_datetime(date, 10, 30),
        };

        let selected = pick_active_occurrence(Some(recurring), Some(one_time))
            .expect("one occurrence should be selected");

        assert_eq!(selected.kind, WindowOccurrenceKind::OneTime);
    }

    #[test]
    fn pick_next_occurrence_prefers_earlier_start() {
        let date = Local::now().date_naive();
        let recurring = WindowOccurrence {
            kind: WindowOccurrenceKind::Recurring,
            window_index: 0,
            start: local_datetime(date, 11, 0),
            end: local_datetime(date, 12, 0),
        };
        let one_time = WindowOccurrence {
            kind: WindowOccurrenceKind::OneTime,
            window_index: 0,
            start: local_datetime(date, 10, 0),
            end: local_datetime(date, 10, 30),
        };

        let selected = pick_next_occurrence(Some(recurring), Some(one_time))
            .expect("one occurrence should be selected");

        assert_eq!(selected.kind, WindowOccurrenceKind::OneTime);
    }

    #[test]
    fn inspect_schedule_conflicts_detects_recurring_overlap_on_shared_weekday() {
        let date = Local::now().date_naive();
        let schedule = RecurringScheduleConfig {
            windows: vec![
                RecurringFocusWindowConfig {
                    days: vec![date.weekday().to_string()],
                    start: "09:00".to_string(),
                    end: "11:00".to_string(),
                },
                RecurringFocusWindowConfig {
                    days: vec![date.weekday().to_string()],
                    start: "10:30".to_string(),
                    end: "12:00".to_string(),
                },
            ],
            ..RecurringScheduleConfig::default()
        };

        let conflicts = inspect_schedule_conflicts_from_config(&schedule);

        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].first_kind, WindowOccurrenceKind::Recurring);
        assert_eq!(conflicts[0].second_kind, WindowOccurrenceKind::Recurring);
        assert_eq!(conflicts[0].overlap_start_minutes, 10 * 60 + 30);
        assert_eq!(conflicts[0].overlap_end_minutes, 11 * 60);
    }

    #[test]
    fn inspect_schedule_conflicts_detects_one_time_overlap_on_same_date() {
        let date = Local::now().date_naive();
        let schedule = RecurringScheduleConfig {
            one_time_windows: vec![
                OneTimeFocusWindowConfig {
                    date: date.format("%Y-%m-%d").to_string(),
                    start: "14:00".to_string(),
                    end: "16:00".to_string(),
                },
                OneTimeFocusWindowConfig {
                    date: date.format("%Y-%m-%d").to_string(),
                    start: "15:30".to_string(),
                    end: "17:00".to_string(),
                },
            ],
            ..RecurringScheduleConfig::default()
        };

        let conflicts = inspect_schedule_conflicts_from_config(&schedule);

        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].first_kind, WindowOccurrenceKind::OneTime);
        assert_eq!(conflicts[0].second_kind, WindowOccurrenceKind::OneTime);
        assert_eq!(conflicts[0].overlap_start_minutes, 15 * 60 + 30);
        assert_eq!(conflicts[0].overlap_end_minutes, 16 * 60);
    }

    #[test]
    fn inspect_schedule_conflicts_detects_one_time_and_recurring_overlap() {
        let date = Local::now().date_naive();
        let schedule = RecurringScheduleConfig {
            windows: vec![RecurringFocusWindowConfig {
                days: vec![date.weekday().to_string()],
                start: "09:00".to_string(),
                end: "11:00".to_string(),
            }],
            one_time_windows: vec![OneTimeFocusWindowConfig {
                date: date.format("%Y-%m-%d").to_string(),
                start: "10:45".to_string(),
                end: "11:30".to_string(),
            }],
            ..RecurringScheduleConfig::default()
        };

        let conflicts = inspect_schedule_conflicts_from_config(&schedule);

        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].first_kind, WindowOccurrenceKind::Recurring);
        assert_eq!(conflicts[0].second_kind, WindowOccurrenceKind::OneTime);
        assert_eq!(conflicts[0].overlap_start_minutes, 10 * 60 + 45);
        assert_eq!(conflicts[0].overlap_end_minutes, 11 * 60);
    }

    #[test]
    fn inspect_schedule_conflicts_skips_recurring_vs_one_time_on_exception_date() {
        let date = Local::now().date_naive();
        let schedule = RecurringScheduleConfig {
            windows: vec![RecurringFocusWindowConfig {
                days: vec![date.weekday().to_string()],
                start: "09:00".to_string(),
                end: "11:00".to_string(),
            }],
            exception_dates: vec![date.format("%Y-%m-%d").to_string()],
            one_time_windows: vec![OneTimeFocusWindowConfig {
                date: date.format("%Y-%m-%d").to_string(),
                start: "10:45".to_string(),
                end: "11:30".to_string(),
            }],
        };

        let conflicts = inspect_schedule_conflicts_from_config(&schedule);

        assert!(conflicts.is_empty());
    }

    #[test]
    fn format_schedule_conflict_includes_context_and_overlap_window() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 2).expect("date literal should be valid");
        let conflict = ScheduleConflict {
            first_kind: WindowOccurrenceKind::Recurring,
            first_window_index: 0,
            second_kind: WindowOccurrenceKind::OneTime,
            second_window_index: 1,
            context: ScheduleConflictContext::Date(date),
            overlap_start_minutes: 14 * 60,
            overlap_end_minutes: 15 * 60,
        };

        assert_eq!(
            format_schedule_conflict(&conflict),
            "2026-05-02: recurring #1 overlaps one-time #2 at 14:00-15:00"
        );
    }
}
