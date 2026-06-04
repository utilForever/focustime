use std::collections::HashSet;

use chrono::{DateTime, Datelike, Duration, Local, LocalResult, NaiveDate, TimeZone, Timelike};

use super::{OneTimeWindow, RecurringWindow, WindowOccurrence, WindowOccurrenceKind};

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
