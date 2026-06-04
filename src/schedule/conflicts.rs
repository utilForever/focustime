use std::collections::HashSet;

use chrono::{Datelike, NaiveDate, Weekday};

use crate::config::RecurringScheduleConfig;

use super::{
    OneTimeWindow, RecurringWindow, ScheduleConflict, ScheduleConflictContext,
    WindowOccurrenceKind, compile_exception_dates, compile_one_time_windows, compile_windows,
};

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

pub(super) fn shared_weekdays(first: &RecurringWindow, second: &RecurringWindow) -> Vec<Weekday> {
    let mut shared = Vec::new();
    for day in &first.days {
        if second.days.contains(day) {
            shared.push(*day);
        }
    }
    shared.sort_by_key(|day| day.num_days_from_monday());
    shared
}

pub(super) fn overlap_minutes(
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

pub(super) fn conflict_sort_key(
    conflict: &ScheduleConflict,
) -> (u8, i32, u16, u16, u8, usize, u8, usize) {
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

pub(super) fn conflict_kind_rank(kind: WindowOccurrenceKind) -> u8 {
    match kind {
        WindowOccurrenceKind::Recurring => 0,
        WindowOccurrenceKind::OneTime => 1,
    }
}

pub(super) fn conflict_kind_label(kind: WindowOccurrenceKind) -> &'static str {
    match kind {
        WindowOccurrenceKind::Recurring => "recurring",
        WindowOccurrenceKind::OneTime => "one-time",
    }
}

pub(super) fn day_token(day: Weekday) -> &'static str {
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

pub(super) fn format_minutes(total_minutes: u16) -> String {
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    format!("{hours:02}:{minutes:02}")
}

pub(super) fn find_recurring_conflicts(
    recurring_windows: &[RecurringWindow],
) -> Vec<ScheduleConflict> {
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

pub(super) fn find_one_time_conflicts(one_time_windows: &[OneTimeWindow]) -> Vec<ScheduleConflict> {
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

pub(super) fn find_recurring_one_time_conflicts(
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
