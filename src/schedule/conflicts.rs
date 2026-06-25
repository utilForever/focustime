use chrono::Weekday;

use crate::config::RecurringScheduleConfig;

use super::{RecurringWindow, ScheduleConflict, compile_windows};

pub(crate) fn inspect_schedule_conflicts_from_config(
    schedule: &RecurringScheduleConfig,
) -> Vec<ScheduleConflict> {
    let recurring_windows = compile_windows(&schedule.windows);
    inspect_schedule_conflicts(&recurring_windows)
}

pub(crate) fn inspect_schedule_conflicts(
    recurring_windows: &[RecurringWindow],
) -> Vec<ScheduleConflict> {
    let mut conflicts = Vec::new();
    conflicts.extend(find_recurring_conflicts(recurring_windows));

    conflicts.sort_by_key(conflict_sort_key);
    conflicts
}

pub(crate) fn format_schedule_conflict(conflict: &ScheduleConflict) -> String {
    let context = day_token(conflict.weekday).to_uppercase();
    format!(
        "{context}: {} #{} overlaps {} #{} at {}-{}",
        "recurring",
        conflict.first_window_index + 1,
        "recurring",
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

pub(super) fn conflict_sort_key(conflict: &ScheduleConflict) -> (i32, u16, u16, usize, usize) {
    (
        conflict.weekday.num_days_from_monday() as i32,
        conflict.overlap_start_minutes,
        conflict.overlap_end_minutes,
        conflict.first_window_index,
        conflict.second_window_index,
    )
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
                    first_window_index: first_index,
                    second_window_index: second_index,
                    weekday: day,
                    overlap_start_minutes: overlap_start,
                    overlap_end_minutes: overlap_end,
                });
            }
        }
    }
    conflicts
}
