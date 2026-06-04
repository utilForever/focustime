use super::{
    ScheduleConflict, ScheduleConflictContext, WindowOccurrence, WindowOccurrenceKind,
    active_occurrence, active_one_time_occurrence, compile_exception_dates,
    compile_one_time_windows, compile_windows, format_schedule_conflict,
    inspect_schedule_conflicts_from_config, next_occurrence_after, pick_active_occurrence,
    pick_next_occurrence,
};

use std::collections::HashSet;

use chrono::{DateTime, Datelike, Duration, Local, LocalResult, NaiveDate, TimeZone};

use crate::config::{
    OneTimeFocusWindowConfig, RecurringFocusWindowConfig, RecurringScheduleConfig,
};

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
