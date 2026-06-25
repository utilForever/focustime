use super::{
    ScheduleConflict, active_occurrence, compile_exception_dates, compile_windows,
    format_schedule_conflict, inspect_schedule_conflicts_from_config, next_occurrence_after,
};

use std::collections::HashSet;

use chrono::{DateTime, Datelike, Duration, Local, LocalResult, NaiveDate, TimeZone, Weekday};

use crate::config::{RecurringFocusWindowConfig, RecurringScheduleConfig};

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
fn compile_exception_dates_ignores_invalid_and_deduplicates() {
    let dates = compile_exception_dates(&[
        " 2026-12-25 ".to_string(),
        "2026-12-25".to_string(),
        "not-a-date".to_string(),
        "".to_string(),
    ]);

    assert_eq!(dates.len(), 1);
    assert!(dates.contains(&NaiveDate::from_ymd_opt(2026, 12, 25).expect("valid date literal")));
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

    let next =
        next_occurrence_after(now, &windows, &HashSet::new()).expect("next window should exist");

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
    assert_eq!(conflicts[0].weekday, date.weekday());
    assert_eq!(conflicts[0].overlap_start_minutes, 10 * 60 + 30);
    assert_eq!(conflicts[0].overlap_end_minutes, 11 * 60);
}

#[test]
fn format_schedule_conflict_includes_weekday_and_overlap_window() {
    let conflict = ScheduleConflict {
        first_window_index: 0,
        second_window_index: 1,
        weekday: Weekday::Mon,
        overlap_start_minutes: 14 * 60,
        overlap_end_minutes: 15 * 60,
    };

    assert_eq!(
        format_schedule_conflict(&conflict),
        "MON: recurring #1 overlaps recurring #2 at 14:00-15:00"
    );
}
