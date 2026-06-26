use crate::stats::*;
use chrono::{Datelike, LocalResult, TimeZone};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

fn local_timestamp_today(hour: u32, minute: u32) -> u64 {
    let today = chrono::Local::now().date_naive();
    match chrono::Local.with_ymd_and_hms(today.year(), today.month(), today.day(), hour, minute, 0)
    {
        LocalResult::Single(dt) => dt.timestamp() as u64,
        LocalResult::Ambiguous(earliest, _) => earliest.timestamp() as u64,
        LocalResult::None => panic!("local datetime should be representable for tests"),
    }
}

#[test]
fn carry_over_goal_target_returns_base_when_disabled() {
    let base = DailyGoalSnapshot {
        minutes: 60,
        pomodoros: 2,
    };
    let carried = carry_over_goal_target(base, false, Some((base, 0, 0)));
    assert_eq!(carried, base);
}

#[test]
fn carry_over_goal_target_adds_previous_period_deficit() {
    let base = DailyGoalSnapshot {
        minutes: 60,
        pomodoros: 2,
    };
    let previous_target = DailyGoalSnapshot {
        minutes: 50,
        pomodoros: 3,
    };
    let carried = carry_over_goal_target(base, true, Some((previous_target, 30, 1)));
    assert_eq!(
        carried,
        DailyGoalSnapshot {
            minutes: 80,
            pomodoros: 4,
        }
    );
}

#[test]
fn carry_over_goal_target_keeps_disabled_metrics_off() {
    let base = DailyGoalSnapshot {
        minutes: 0,
        pomodoros: 2,
    };
    let previous_target = DailyGoalSnapshot {
        minutes: 120,
        pomodoros: 5,
    };
    let carried = carry_over_goal_target(base, true, Some((previous_target, 0, 1)));
    assert_eq!(
        carried,
        DailyGoalSnapshot {
            minutes: 0,
            pomodoros: 6,
        }
    );
}

#[test]
fn weekly_and_monthly_goal_snapshot_sync_and_lookup_are_idempotent() {
    let mut stats = FocusStats::default();
    let day = chrono::NaiveDate::from_ymd_opt(2026, 4, 9).expect("day should be valid");
    let weekly_goal = DailyGoalSnapshot {
        minutes: 300,
        pomodoros: 10,
    };
    let monthly_goal = DailyGoalSnapshot {
        minutes: 1200,
        pomodoros: 40,
    };

    assert!(stats.sync_weekly_goal_snapshot(day, weekly_goal));
    assert!(!stats.sync_weekly_goal_snapshot(day, weekly_goal));
    assert_eq!(stats.weekly_goal_snapshot_for_day(day), Some(weekly_goal));

    assert!(stats.sync_monthly_goal_snapshot(day, monthly_goal));
    assert!(!stats.sync_monthly_goal_snapshot(day, monthly_goal));
    assert_eq!(stats.monthly_goal_snapshot_for_day(day), Some(monthly_goal));
}

#[test]
fn recording_updates_session_and_daily_totals() {
    let mut stats = FocusStats::default();
    let goal = DailyGoalSnapshot {
        minutes: 25,
        pomodoros: 1,
    };

    stats.record_focus_elapsed("2026-04-09", 125, goal);
    stats.record_completed_pomodoro("2026-04-09", goal);

    let session = stats.session();
    assert_eq!(session.pomodoros_completed, 1);
    assert_eq!(session.focused_seconds, 125);
    assert_eq!(session.focused_minutes(), 2);

    let day = stats.daily_for("2026-04-09");
    assert_eq!(day.pomodoros_completed, 1);
    assert_eq!(day.focused_seconds, 125);
    assert_eq!(day.focused_minutes(), 2);
    assert_eq!(day.goal, Some(goal));
}

#[test]
fn usage_signal_recording_normalizes_and_accumulates_counts() {
    let mut stats = FocusStats::default();
    assert!(stats.record_command_usage("  STATUS  "));
    assert!(stats.record_command_usage("status"));
    assert!(stats.record_screen_usage(" Site-Manager "));

    assert_eq!(stats.command_usage_counts.get("status"), Some(&2));
    assert_eq!(stats.screen_usage_counts.get("site-manager"), Some(&1));
    assert_eq!(stats.command_usage_count("status"), 2);
    assert_eq!(stats.command_usage_count("unknown"), 0);
}

#[test]
fn usage_signal_summary_reports_top_and_rare_entries() {
    let mut stats = FocusStats::default();
    stats.record_command_usage("status");
    stats.record_command_usage("status");
    stats.record_command_usage("status");
    stats.record_command_usage("backup");
    stats.record_command_usage("backup");
    stats.record_command_usage("profile");
    stats.record_screen_usage("timer");
    stats.record_screen_usage("timer");
    stats.record_screen_usage("site-manager");

    let summary = stats.usage_signal_summary(2);
    assert_eq!(summary.commands.total_events, 6);
    assert_eq!(summary.commands.unique_surfaces, 3);
    assert_eq!(summary.commands.top[0].surface, "status");
    assert_eq!(summary.commands.top[0].count, 3);
    assert_eq!(summary.commands.top[0].share_pct, 50);
    assert_eq!(summary.commands.rare[0].surface, "profile");
    assert_eq!(summary.commands.rare[0].count, 1);
    assert_eq!(summary.commands.rare[0].share_pct, 17);
    assert_eq!(summary.screens.total_events, 3);
    assert_eq!(summary.screens.unique_surfaces, 2);
    assert_eq!(summary.screens.top[0].surface, "timer");
    assert_eq!(summary.screens.rare[0].surface, "site-manager");
}

#[test]
fn task_planner_state_normalizes_and_deduplicates_labels() {
    let mut stats = FocusStats::default();
    let changed = stats.update_task_planner_state(
        vec![
            "  Docs  ".to_string(),
            "docs".to_string(),
            "".to_string(),
            "Bugfix".to_string(),
        ],
        Some("  docs ".to_string()),
    );
    assert!(changed);

    let (labels, selected) = stats.task_planner_state();
    assert_eq!(labels, vec!["Docs".to_string(), "Bugfix".to_string()]);
    assert_eq!(selected, Some("Docs".to_string()));
}

#[test]
fn task_planner_label_states_normalize_and_drop_archived_selection() {
    let mut stats = FocusStats::default();
    let changed = stats.update_task_planner_state_with_label_states(
        vec!["  Docs ".to_string(), "Bugfix".to_string()],
        Some("docs".to_string()),
        vec![
            " docs ".to_string(),
            "Docs".to_string(),
            "Planning".to_string(),
        ],
        vec![" DOCS ".to_string()],
    );
    assert!(changed);

    let (labels, selected) = stats.task_planner_state();
    assert_eq!(
        labels,
        vec![
            "Docs".to_string(),
            "Bugfix".to_string(),
            "Planning".to_string()
        ]
    );
    assert_eq!(selected, None);
    assert_eq!(
        stats.task_label_favorites(),
        vec!["Docs".to_string(), "Planning".to_string()]
    );
    assert_eq!(stats.task_label_archived(), vec!["Docs".to_string()]);
}

#[test]
fn setting_task_goal_target_does_not_reselect_archived_label() {
    let mut stats = FocusStats::default();
    stats.update_task_planner_state_with_label_states(
        vec!["Docs".to_string(), "Review".to_string()],
        Some("Review".to_string()),
        Vec::new(),
        vec!["Docs".to_string()],
    );

    let target = DailyGoalSnapshot {
        minutes: 30,
        pomodoros: 1,
    };
    let canonical = stats
        .set_task_goal_target("Docs", target)
        .expect("task goal should be set");

    assert_eq!(canonical, "Docs");
    assert_eq!(stats.task_planner_state().1, Some("Review".to_string()));
    assert_eq!(
        stats
            .task_goal_progress_for_label("Docs")
            .expect("progress should be available")
            .target,
        target
    );
}

#[test]
fn recent_task_labels_returns_newest_first_unique_labels() {
    let mut stats = FocusStats::default();
    let goal = DailyGoalSnapshot {
        minutes: 25,
        pomodoros: 1,
    };
    stats.record_completed_pomodoro_with_task("2026-04-07", goal, Some("Docs"), 25 * 60, None);
    stats.record_completed_pomodoro_with_task("2026-04-08", goal, Some("Bugfix"), 25 * 60, None);
    stats.record_completed_pomodoro_with_task("2026-04-09", goal, Some("docs"), 25 * 60, None);
    stats.record_completed_pomodoro_with_task("2026-04-10", goal, Some("Planning"), 25 * 60, None);

    let recent = stats.recent_task_labels(3);
    assert_eq!(
        recent,
        vec![
            "Planning".to_string(),
            "docs".to_string(),
            "Bugfix".to_string()
        ]
    );
}

#[test]
fn recent_task_labels_respects_zero_limit() {
    let mut stats = FocusStats::default();
    let goal = DailyGoalSnapshot {
        minutes: 25,
        pomodoros: 1,
    };
    stats.record_completed_pomodoro_with_task("2026-04-10", goal, Some("Docs"), 25 * 60, None);

    assert!(stats.recent_task_labels(0).is_empty());
}

#[test]
fn task_totals_aggregate_focus_sessions_by_label() {
    let mut stats = FocusStats::default();
    let goal = DailyGoalSnapshot {
        minutes: 25,
        pomodoros: 1,
    };
    stats.record_completed_pomodoro_with_task(
        "2026-04-01",
        goal,
        Some("Project A"),
        30 * 60,
        Some(ProfileId::Classic),
    );
    stats.record_completed_pomodoro_with_task(
        "2026-04-02",
        goal,
        Some("project a"),
        20 * 60,
        Some(ProfileId::Classic),
    );
    stats.record_completed_pomodoro_with_task(
        "2026-04-03",
        goal,
        Some("Project B"),
        40 * 60,
        Some(ProfileId::DeepWork),
    );

    let totals = stats.task_totals(10);
    assert_eq!(totals.len(), 2);
    assert_eq!(totals[0].task_label, "Project A");
    assert_eq!(totals[0].pomodoros_completed, 2);
    assert_eq!(totals[0].focused_minutes(), 50);
    assert_eq!(totals[1].task_label, "Project B");
    assert_eq!(totals[1].pomodoros_completed, 1);
    assert_eq!(totals[1].focused_minutes(), 40);
}

#[test]
fn task_goal_progress_for_label_uses_cumulative_task_totals() {
    let mut stats = FocusStats::default();
    let daily_goal = DailyGoalSnapshot {
        minutes: 25,
        pomodoros: 1,
    };
    stats.record_completed_pomodoro_with_task(
        "2026-04-01",
        daily_goal,
        Some("Docs"),
        30 * 60,
        None,
    );
    stats.record_completed_pomodoro_with_task(
        "2026-04-02",
        daily_goal,
        Some("docs"),
        30 * 60,
        None,
    );

    let task_goal = DailyGoalSnapshot {
        minutes: 60,
        pomodoros: 2,
    };
    let canonical = stats.set_task_goal_target("docs", task_goal).unwrap();
    assert_eq!(canonical, "Docs");

    let progress = stats.task_goal_progress_for_label("DOCS").unwrap();
    assert_eq!(progress.task_label, "Docs");
    assert_eq!(progress.target, task_goal);
    assert_eq!(progress.focused_minutes(), 60);
    assert_eq!(progress.pomodoros_completed, 2);
    assert!(progress.met);
}

#[test]
fn task_goal_progress_lists_configured_targets_without_history() {
    let mut stats = FocusStats::default();
    let task_goal = DailyGoalSnapshot {
        minutes: 120,
        pomodoros: 4,
    };
    stats
        .set_task_goal_target("Project A", task_goal)
        .expect("task goal should be set");

    let progress = stats.task_goal_progress(5);
    assert_eq!(progress.len(), 1);
    assert_eq!(progress[0].task_label, "Project A");
    assert_eq!(progress[0].target, task_goal);
    assert_eq!(progress[0].focused_minutes(), 0);
    assert_eq!(progress[0].pomodoros_completed, 0);
    assert!(!progress[0].met);
}

#[test]
fn renaming_and_removing_task_goals_updates_lookup() {
    let mut stats = FocusStats::default();
    let task_goal = DailyGoalSnapshot {
        minutes: 90,
        pomodoros: 3,
    };
    stats
        .set_task_goal_target("Docs", task_goal)
        .expect("task goal should be set");
    assert!(stats.rename_task_goal_target("Docs", "Writing"));

    let renamed = stats.task_goal_progress_for_label("Writing").unwrap();
    assert_eq!(renamed.target, task_goal);
    assert!(stats.remove_task_goal_target("Writing"));

    let removed = stats.task_goal_progress_for_label("Writing").unwrap();
    assert_eq!(removed.target, DailyGoalSnapshot::default());
}

#[test]
fn recent_task_trends_compare_last_seven_days_vs_previous_window() {
    let mut stats = FocusStats::default();
    let goal = DailyGoalSnapshot {
        minutes: 25,
        pomodoros: 1,
    };
    let today = chrono::Local::now().date_naive();
    let day_prev_a_1 = today
        .checked_sub_signed(chrono::Duration::days(13))
        .unwrap()
        .format("%Y-%m-%d")
        .to_string();
    let day_prev_a_2 = today
        .checked_sub_signed(chrono::Duration::days(10))
        .unwrap()
        .format("%Y-%m-%d")
        .to_string();
    let day_recent_a_1 = today
        .checked_sub_signed(chrono::Duration::days(3))
        .unwrap()
        .format("%Y-%m-%d")
        .to_string();
    let day_recent_a_2 = today.format("%Y-%m-%d").to_string();
    let day_prev_b = today
        .checked_sub_signed(chrono::Duration::days(11))
        .unwrap()
        .format("%Y-%m-%d")
        .to_string();
    let day_recent_b = today
        .checked_sub_signed(chrono::Duration::days(2))
        .unwrap()
        .format("%Y-%m-%d")
        .to_string();

    stats.record_completed_pomodoro_with_task(
        &day_prev_a_1,
        goal,
        Some("Project A"),
        10 * 60,
        None,
    );
    stats.record_completed_pomodoro_with_task(
        &day_prev_a_2,
        goal,
        Some("Project A"),
        20 * 60,
        None,
    );
    stats.record_completed_pomodoro_with_task(
        &day_recent_a_1,
        goal,
        Some("Project A"),
        30 * 60,
        None,
    );
    stats.record_completed_pomodoro_with_task(
        &day_recent_a_2,
        goal,
        Some("Project A"),
        40 * 60,
        None,
    );

    stats.record_completed_pomodoro_with_task(&day_prev_b, goal, Some("Project B"), 30 * 60, None);
    stats.record_completed_pomodoro_with_task(
        &day_recent_b,
        goal,
        Some("Project B"),
        10 * 60,
        None,
    );

    let trends = stats.recent_task_trends(10);
    assert_eq!(trends.len(), 2);
    assert_eq!(trends[0].task_label, "Project A");
    assert_eq!(trends[0].recent_pomodoros_completed, 2);
    assert_eq!(trends[0].previous_pomodoros_completed, 2);
    assert_eq!(trends[0].recent_focused_minutes(), 70);
    assert_eq!(trends[0].previous_focused_minutes(), 30);
    assert_eq!(trends[0].delta_focused_minutes(), 40);

    assert_eq!(trends[1].task_label, "Project B");
    assert_eq!(trends[1].recent_focused_minutes(), 10);
    assert_eq!(trends[1].previous_focused_minutes(), 30);
    assert_eq!(trends[1].delta_focused_minutes(), -20);
}

#[test]
fn recent_task_trends_handles_sparse_data() {
    let mut stats = FocusStats::default();
    let goal = DailyGoalSnapshot {
        minutes: 25,
        pomodoros: 1,
    };
    let today_key = chrono::Local::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string();
    stats.record_completed_pomodoro_with_task(&today_key, goal, Some("Project A"), 25 * 60, None);

    let trends = stats.recent_task_trends(5);
    assert_eq!(trends.len(), 1);
    assert_eq!(trends[0].task_label, "Project A");
    assert_eq!(trends[0].previous_focused_minutes(), 0);
    assert_eq!(trends[0].delta_focused_minutes(), 25);
}

#[test]
fn recent_daily_is_sorted_newest_first() {
    let mut stats = FocusStats::default();
    let goal = DailyGoalSnapshot {
        minutes: 30,
        pomodoros: 1,
    };
    stats.record_focus_elapsed("2026-04-08", 60, goal);
    stats.record_focus_elapsed("2026-04-09", 120, goal);

    let recent = stats.recent_daily(2);
    assert_eq!(recent[0].0, "2026-04-09");
    assert_eq!(recent[1].0, "2026-04-08");
}

#[test]
fn recent_weekly_aggregates_days_in_same_iso_week() {
    let mut stats = FocusStats::default();
    stats.insert_daily_for_tests(
        "2026-04-06",
        DailyStats {
            pomodoros_completed: 1,
            focused_seconds: 30 * 60,
            goal: None,
        },
    );
    stats.insert_daily_for_tests(
        "2026-04-08",
        DailyStats {
            pomodoros_completed: 2,
            focused_seconds: 45 * 60,
            goal: None,
        },
    );

    let recent = stats.recent_weekly(1);
    let iso_week = chrono::NaiveDate::from_ymd_opt(2026, 4, 6)
        .unwrap()
        .iso_week();

    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].year, iso_week.year());
    assert_eq!(recent[0].week, iso_week.week());
    assert_eq!(recent[0].pomodoros_completed, 3);
    assert_eq!(recent[0].focused_minutes(), 75);
}

#[test]
fn weekly_for_day_aggregates_selected_iso_week_only() {
    let mut stats = FocusStats::default();
    stats.insert_daily_for_tests(
        "2026-04-06",
        DailyStats {
            pomodoros_completed: 1,
            focused_seconds: 30 * 60,
            goal: None,
        },
    );
    stats.insert_daily_for_tests(
        "2026-04-08",
        DailyStats {
            pomodoros_completed: 2,
            focused_seconds: 45 * 60,
            goal: None,
        },
    );
    stats.insert_daily_for_tests(
        "2026-04-14",
        DailyStats {
            pomodoros_completed: 3,
            focused_seconds: 90 * 60,
            goal: None,
        },
    );

    let week = stats.weekly_for_day(chrono::NaiveDate::from_ymd_opt(2026, 4, 7).unwrap());
    let iso_week = chrono::NaiveDate::from_ymd_opt(2026, 4, 7)
        .unwrap()
        .iso_week();
    assert_eq!(week.year, iso_week.year());
    assert_eq!(week.week, iso_week.week());
    assert_eq!(week.pomodoros_completed, 3);
    assert_eq!(week.focused_minutes(), 75);
}

#[test]
fn recent_weekly_is_sorted_newest_first_across_iso_year_boundaries() {
    let mut stats = FocusStats::default();
    stats.insert_daily_for_tests(
        "2020-12-31",
        DailyStats {
            pomodoros_completed: 1,
            focused_seconds: 30 * 60,
            goal: None,
        },
    );
    stats.insert_daily_for_tests(
        "2021-01-01",
        DailyStats {
            pomodoros_completed: 2,
            focused_seconds: 60 * 60,
            goal: None,
        },
    );
    stats.insert_daily_for_tests(
        "2021-01-04",
        DailyStats {
            pomodoros_completed: 1,
            focused_seconds: 15 * 60,
            goal: None,
        },
    );

    let recent = stats.recent_weekly(2);

    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].year, 2021);
    assert_eq!(recent[0].week, 1);
    assert_eq!(recent[0].pomodoros_completed, 1);
    assert_eq!(recent[0].focused_minutes(), 15);
    assert_eq!(recent[1].year, 2020);
    assert_eq!(recent[1].week, 53);
    assert_eq!(recent[1].pomodoros_completed, 3);
    assert_eq!(recent[1].focused_minutes(), 90);
}

#[test]
fn recent_weekly_consistency_scores_active_days() {
    let mut stats = FocusStats::default();
    stats.insert_daily_for_tests(
        "2026-04-06",
        DailyStats {
            pomodoros_completed: 1,
            focused_seconds: 25 * 60,
            goal: None,
        },
    );
    stats.insert_daily_for_tests(
        "2026-04-07",
        DailyStats {
            pomodoros_completed: 0,
            focused_seconds: 15 * 60,
            goal: None,
        },
    );
    stats.insert_daily_for_tests(
        "2026-04-08",
        DailyStats {
            pomodoros_completed: 0,
            focused_seconds: 0,
            goal: None,
        },
    );

    let consistency = stats.recent_weekly_consistency(1);
    let iso_week = chrono::NaiveDate::from_ymd_opt(2026, 4, 6)
        .unwrap()
        .iso_week();
    assert_eq!(consistency.len(), 1);
    assert_eq!(consistency[0].year, iso_week.year());
    assert_eq!(consistency[0].week, iso_week.week());
    assert_eq!(consistency[0].active_days, 2);
    assert_eq!(consistency[0].consistency_score_pct, 29);
}

#[test]
fn weekly_focus_score_combines_consistency_and_completion() {
    let mut stats = FocusStats::default();
    let week_day = chrono::NaiveDate::from_ymd_opt(2026, 4, 6).unwrap();
    let day_key = week_day.format("%Y-%m-%d").to_string();
    let other_day = week_day.succ_opt().unwrap();
    let other_day_key = other_day.format("%Y-%m-%d").to_string();
    let goal = DailyGoalSnapshot {
        minutes: 50,
        pomodoros: 2,
    };

    stats.sync_weekly_goal_snapshot(week_day, goal);
    stats.record_focus_elapsed(&day_key, 25 * 60, goal);
    stats.record_completed_pomodoro(&day_key, goal);
    stats.record_focus_elapsed(&other_day_key, 25 * 60, goal);
    stats.record_completed_pomodoro(&other_day_key, goal);

    let focus_score = stats.latest_weekly_focus_score().unwrap();
    let iso_week = week_day.iso_week();
    assert_eq!(
        focus_score.week_label,
        format_week_label(iso_week.year(), iso_week.week())
    );
    assert_eq!(focus_score.active_days, 2);
    assert_eq!(focus_score.consistency_score_pct, 29);
    assert_eq!(focus_score.completion_score_pct, Some(100));
    assert_eq!(focus_score.focus_score_pct, Some(65));
}

#[test]
fn weekly_focus_score_is_unavailable_when_weekly_goal_is_off() {
    let mut stats = FocusStats::default();
    stats.insert_daily_for_tests(
        "2026-04-06",
        DailyStats {
            pomodoros_completed: 1,
            focused_seconds: 25 * 60,
            goal: None,
        },
    );

    let focus_score = stats.latest_weekly_focus_score().unwrap();
    assert_eq!(focus_score.completion_score_pct, None);
    assert_eq!(focus_score.focus_score_pct, None);
    assert_eq!(focus_score.consistency_score_pct, 14);
}

#[test]
fn weekly_focus_score_includes_goal_enabled_idle_weeks() {
    let mut stats = FocusStats::default();
    let week_day = chrono::NaiveDate::from_ymd_opt(2026, 4, 6).unwrap();
    let goal = DailyGoalSnapshot {
        minutes: 100,
        pomodoros: 4,
    };
    stats.sync_weekly_goal_snapshot(week_day, goal);

    let recent = stats.recent_weekly_focus_scores(1);
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].active_days, 0);
    assert_eq!(recent[0].consistency_score_pct, 0);
    assert_eq!(recent[0].completion_score_pct, Some(0));
    assert_eq!(recent[0].focus_score_pct, Some(0));

    let latest = stats.latest_weekly_focus_score().unwrap();
    assert_eq!(latest.week_label, recent[0].week_label);
    assert_eq!(latest.focus_score_pct, Some(0));
}

#[test]
fn focus_risk_forecast_flags_high_risk_for_unmet_goals_and_streak() {
    let mut stats = FocusStats::default();
    let day = chrono::NaiveDate::from_ymd_opt(2026, 4, 9).unwrap();
    let day_key = day.format("%Y-%m-%d").to_string();
    stats.insert_daily_for_tests(
        &day_key,
        DailyStats {
            pomodoros_completed: 1,
            focused_seconds: 20 * 60,
            goal: None,
        },
    );

    let forecast = stats.focus_risk_forecast_for_day(
        day,
        DailyGoalSnapshot {
            minutes: 120,
            pomodoros: 4,
        },
        DailyGoalSnapshot {
            minutes: 600,
            pomodoros: 24,
        },
        DailyGoalSnapshot {
            minutes: 2400,
            pomodoros: 96,
        },
    );

    assert_eq!(forecast.daily_goal.risk_level, FocusRiskLevel::High);
    assert_eq!(forecast.streak.risk_level, FocusRiskLevel::High);
    assert!(forecast.alert_active());
}

#[test]
fn focus_risk_forecast_avoids_alert_for_borderline_daily_slip() {
    let mut stats = FocusStats::default();
    let day = chrono::NaiveDate::from_ymd_opt(2026, 4, 9).unwrap();
    let daily_goal = DailyGoalSnapshot {
        minutes: 30,
        pomodoros: 1,
    };
    for offset in 0..7 {
        let candidate = day
            .checked_sub_signed(chrono::Duration::days(i64::from(offset)))
            .unwrap();
        let day_key = candidate.format("%Y-%m-%d").to_string();
        let (focused_minutes, pomodoros_completed) = if offset == 0 {
            (20_u64, 1_u32)
        } else {
            (30_u64, 1_u32)
        };
        stats.insert_daily_for_tests(
            &day_key,
            DailyStats {
                pomodoros_completed,
                focused_seconds: focused_minutes * 60,
                goal: Some(daily_goal),
            },
        );
    }

    let forecast = stats.focus_risk_forecast_for_day(
        day,
        daily_goal,
        DailyGoalSnapshot::default(),
        DailyGoalSnapshot::default(),
    );

    assert!(!forecast.alert_active());
    assert_ne!(forecast.highest_risk_level(), FocusRiskLevel::High);
}

#[test]
fn focus_risk_forecast_stays_low_when_goals_are_met() {
    let mut stats = FocusStats::default();
    let day = chrono::NaiveDate::from_ymd_opt(2026, 4, 9).unwrap();
    let day_key = day.format("%Y-%m-%d").to_string();
    stats.insert_daily_for_tests(
        &day_key,
        DailyStats {
            pomodoros_completed: 1,
            focused_seconds: 25 * 60,
            goal: None,
        },
    );

    let forecast = stats.focus_risk_forecast_for_day(
        day,
        DailyGoalSnapshot {
            minutes: 25,
            pomodoros: 1,
        },
        DailyGoalSnapshot {
            minutes: 25,
            pomodoros: 1,
        },
        DailyGoalSnapshot {
            minutes: 25,
            pomodoros: 1,
        },
    );

    assert_eq!(forecast.daily_goal.risk_score_pct, 0);
    assert_eq!(forecast.weekly_goal.risk_score_pct, 0);
    assert_eq!(forecast.monthly_goal.risk_score_pct, 0);
    assert_eq!(forecast.daily_goal.risk_level, FocusRiskLevel::Low);
    assert!(!forecast.alert_active());
}

#[test]
fn focus_risk_calibration_metrics_track_false_positives_without_end_period_outcomes() {
    let mut stats = FocusStats::default();
    let day = chrono::NaiveDate::from_ymd_opt(2026, 4, 9).unwrap();
    let daily_goal = DailyGoalSnapshot {
        minutes: 30,
        pomodoros: 1,
    };
    for offset in 0..3 {
        let candidate = day
            .checked_sub_signed(chrono::Duration::days(i64::from(offset)))
            .unwrap();
        let day_key = candidate.format("%Y-%m-%d").to_string();
        stats.insert_daily_for_tests(
            &day_key,
            DailyStats {
                pomodoros_completed: 1,
                focused_seconds: 30_u64 * 60,
                goal: Some(daily_goal),
            },
        );
    }

    let metrics = stats.focus_risk_calibration_metrics_for_day(
        day,
        daily_goal,
        DailyGoalSnapshot {
            minutes: 500,
            pomodoros: 20,
        },
        DailyGoalSnapshot {
            minutes: 2000,
            pomodoros: 80,
        },
        3,
    );

    assert_eq!(metrics.sample_count, 3);
    assert!(metrics.alert_count > 0);
    assert_eq!(metrics.true_positive_alerts, 0);
    assert_eq!(metrics.false_positive_alerts, metrics.alert_count);
    assert_eq!(metrics.precision_pct, 0);
    assert_eq!(metrics.missed_warning_count, 0);
    assert_eq!(metrics.missed_warning_rate_pct, 0);
}

#[test]
fn focus_risk_calibration_metrics_count_missing_daily_row_weekly_outcomes() {
    let stats = FocusStats::default();
    let day = chrono::NaiveDate::from_ymd_opt(2026, 4, 12).unwrap();
    assert_eq!(day.weekday().num_days_from_monday(), 6);

    let metrics = stats.focus_risk_calibration_metrics_for_day(
        day,
        DailyGoalSnapshot::default(),
        DailyGoalSnapshot {
            minutes: 60,
            pomodoros: 2,
        },
        DailyGoalSnapshot::default(),
        1,
    );

    assert_eq!(metrics.sample_count, 1);
    assert_eq!(metrics.false_positive_alerts, 0);
    assert_eq!(
        metrics.true_positive_alerts + metrics.missed_warning_count,
        metrics.sample_count
    );
}

#[test]
fn focus_risk_forecast_marks_goals_off_when_targets_are_disabled() {
    let stats = FocusStats::default();
    let day = chrono::NaiveDate::from_ymd_opt(2026, 4, 9).unwrap();
    let forecast = stats.focus_risk_forecast_for_day(
        day,
        DailyGoalSnapshot::default(),
        DailyGoalSnapshot::default(),
        DailyGoalSnapshot::default(),
    );

    assert!(!forecast.daily_goal.configured);
    assert!(!forecast.weekly_goal.configured);
    assert!(!forecast.monthly_goal.configured);
    assert!(!forecast.streak.configured);
    assert_eq!(forecast.daily_goal.signals[0].value, "goal off",);
    assert!(!forecast.alert_active());
}

#[test]
fn persisted_stats_round_trip_preserves_daily_history() {
    let mut original = FocusStats::default();
    let goal = DailyGoalSnapshot {
        minutes: 25,
        pomodoros: 1,
    };
    original.record_focus_elapsed("2026-04-09", 300, goal);
    original.record_completed_pomodoro("2026-04-09", goal);

    let persisted = original.to_persisted();
    let toml_str = toml::to_string_pretty(&persisted).unwrap();
    let restored = FocusStats::try_from_toml(&toml_str).unwrap();

    // Session stats are intentionally runtime-only and reset on startup.
    assert_eq!(restored.session(), SessionStats::default());
    let day = restored.daily_for("2026-04-09");
    assert_eq!(day.pomodoros_completed, 1);
    assert_eq!(day.focused_seconds, 300);
    assert_eq!(day.goal, Some(goal));
}

#[test]
fn persisted_stats_round_trip_preserves_break_glass_overrides() {
    let mut original = FocusStats::default();
    original.record_break_glass_override_event("2026-04-09", 1_711_000_000, Some("Project A"), 300);

    let persisted = original.to_persisted();
    let toml_str = toml::to_string_pretty(&persisted).unwrap();
    let restored = FocusStats::try_from_toml(&toml_str).unwrap();
    let recent = restored.recent_break_glass_overrides(1);

    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].date, "2026-04-09");
    assert_eq!(recent[0].timestamp_epoch_secs, 1_711_000_000);
    assert_eq!(recent[0].task_label.as_deref(), Some("Project A"));
    assert_eq!(recent[0].duration_seconds, 300);
}

#[test]
fn persisted_stats_round_trip_preserves_session_interruptions() {
    let mut original = FocusStats::default();
    original.record_session_interruption_event(
        "2026-04-09",
        1_711_000_123,
        SessionInterruptionReason::ManualStop,
        FocusSessionMetadata {
            task_label: Some("Project A"),
        },
        720,
        Some(ProfileId::DeepWork),
    );

    let persisted = original.to_persisted();
    let toml_str = toml::to_string_pretty(&persisted).unwrap();
    let restored = FocusStats::try_from_toml(&toml_str).unwrap();
    let recent = restored.recent_session_interruptions(1);

    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].date, "2026-04-09");
    assert_eq!(recent[0].timestamp_epoch_secs, 1_711_000_123);
    assert_eq!(recent[0].reason, SessionInterruptionReason::ManualStop);
    assert_eq!(recent[0].task_label.as_deref(), Some("Project A"));
    assert_eq!(recent[0].remaining_secs, 720);
    assert_eq!(recent[0].profile, Some(ProfileId::DeepWork));
}

#[test]
fn latest_session_interruption_prefers_greatest_timestamp() {
    let mut stats = FocusStats::default();
    stats.record_session_interruption_event(
        "2026-04-09",
        200,
        SessionInterruptionReason::ManualStop,
        FocusSessionMetadata {
            task_label: Some("Project A"),
        },
        720,
        Some(ProfileId::Classic),
    );
    stats.record_session_interruption_event(
        "2026-04-09",
        100,
        SessionInterruptionReason::ManualSkip,
        FocusSessionMetadata {
            task_label: Some("Project B"),
        },
        600,
        Some(ProfileId::DeepWork),
    );

    let latest = stats
        .latest_session_interruption()
        .expect("latest interruption should exist");
    assert_eq!(latest.timestamp_epoch_secs, 200);
    assert_eq!(latest.reason, SessionInterruptionReason::ManualStop);
    assert_eq!(latest.task_label.as_deref(), Some("Project A"));
}

#[test]
fn persisted_stats_round_trip_preserves_focus_session_profile() {
    let mut original = FocusStats::default();
    let goal = DailyGoalSnapshot {
        minutes: 25,
        pomodoros: 1,
    };
    original.record_focus_elapsed("2026-04-09", 25 * 60, goal);
    original.record_completed_pomodoro_with_task(
        "2026-04-09",
        goal,
        Some("Project A"),
        25 * 60,
        Some(ProfileId::DeepWork),
    );

    let persisted = original.to_persisted();
    let toml_str = toml::to_string_pretty(&persisted).unwrap();
    let restored = FocusStats::try_from_toml(&toml_str).unwrap();
    let profile_totals = restored.profile_totals();

    assert_eq!(profile_totals.len(), 1);
    assert_eq!(profile_totals[0].profile, ProfileBucket::DeepWork);
    assert_eq!(profile_totals[0].pomodoros_completed, 1);
    assert_eq!(profile_totals[0].focused_minutes(), 25);
}

#[test]
fn persisted_stats_round_trip_preserves_weekly_and_monthly_goal_snapshots() {
    let mut original = FocusStats::default();
    let day = chrono::NaiveDate::from_ymd_opt(2026, 4, 9).expect("day should be valid");
    let weekly_goal = DailyGoalSnapshot {
        minutes: 300,
        pomodoros: 10,
    };
    let monthly_goal = DailyGoalSnapshot {
        minutes: 1200,
        pomodoros: 40,
    };
    original.sync_weekly_goal_snapshot(day, weekly_goal);
    original.sync_monthly_goal_snapshot(day, monthly_goal);

    let toml_str = toml::to_string_pretty(&original.to_persisted()).unwrap();
    let restored = FocusStats::try_from_toml(&toml_str).unwrap();

    assert_eq!(
        restored.weekly_goal_snapshot_for_day(day),
        Some(weekly_goal)
    );
    assert_eq!(
        restored.monthly_goal_snapshot_for_day(day),
        Some(monthly_goal)
    );
}

#[test]
fn persisted_stats_round_trip_preserves_task_goal_targets() {
    let mut original = FocusStats::default();
    let task_goal = DailyGoalSnapshot {
        minutes: 180,
        pomodoros: 6,
    };
    original
        .set_task_goal_target("Project A", task_goal)
        .expect("task goal should be set");

    let toml_str = toml::to_string_pretty(&original.to_persisted()).unwrap();
    let restored = FocusStats::try_from_toml(&toml_str).unwrap();
    let progress = restored
        .task_goal_progress_for_label("project a")
        .expect("task goal progress should exist");

    assert_eq!(progress.task_label, "Project A");
    assert_eq!(progress.target, task_goal);
}

#[test]
fn persisted_stats_round_trip_preserves_task_label_states() {
    let mut original = FocusStats::default();
    let changed = original.update_task_planner_state_with_label_states(
        vec!["Docs".to_string(), "Review".to_string()],
        Some("Review".to_string()),
        vec!["Review".to_string()],
        vec!["Docs".to_string()],
    );
    assert!(changed);

    let toml_str = toml::to_string_pretty(&original.to_persisted()).unwrap();
    let restored = FocusStats::try_from_toml(&toml_str).unwrap();

    assert_eq!(restored.task_label_favorites(), vec!["Review".to_string()]);
    assert_eq!(restored.task_label_archived(), vec!["Docs".to_string()]);
    assert_eq!(restored.task_planner_state().1, Some("Review".to_string()));
}

#[test]
fn legacy_focus_sessions_keep_empty_metadata() {
    let legacy_toml = r#"
            [[focus_sessions]]
            date = "2026-04-09"
            task_label = "Project A"
            focused_seconds = 1500
        "#;
    let restored = FocusStats::try_from_toml(legacy_toml).unwrap();

    let export = restored.export_data();
    assert_eq!(export.sessions.len(), 1);
    assert_eq!(export.sessions[0].task_label, "Project A");
    let value = serde_json::to_value(&export.sessions[0]).unwrap();
    assert!(value.get("task_note").is_none());
}

#[test]
fn legacy_focus_sessions_backfill_unknown_time_of_day_bucket() {
    let legacy_toml = r#"
            [[focus_sessions]]
            date = "2026-04-09"
            task_label = "Project A"
            focused_seconds = 1500
        "#;
    let restored = FocusStats::try_from_toml(legacy_toml).unwrap();
    let persisted = restored.to_persisted();

    assert_eq!(persisted.focus_sessions.len(), 1);
    assert_eq!(
        persisted.focus_sessions[0].completion_time_of_day_bucket,
        Some(TimeOfDayBucket::Unknown)
    );
}

#[test]
fn legacy_focus_sessions_backfill_time_of_day_bucket_from_timestamp() {
    let day = current_day_key();
    let completion_timestamp_epoch_secs = local_timestamp_today(9, 0);
    let persisted_toml = format!(
        r#"
            [[focus_sessions]]
            date = "{day}"
            task_label = "Project A"
            focused_seconds = 1800
            completion_timestamp_epoch_secs = {completion_timestamp_epoch_secs}
        "#
    );
    let restored = FocusStats::try_from_toml(&persisted_toml).unwrap();
    let persisted = restored.to_persisted();

    assert_eq!(persisted.focus_sessions.len(), 1);
    assert_eq!(
        persisted.focus_sessions[0].completion_time_of_day_bucket,
        Some(TimeOfDayBucket::Morning)
    );

    let rows = restored.productivity_comparison(
        ComparisonDimension::TimeOfDay,
        &ProductivityComparisonFilter::default(),
        10,
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].label, "Morning");
    assert_eq!(rows[0].sessions_completed, 1);
    assert_eq!(rows[0].focused_minutes(), 30);
}

#[test]
fn persisted_focus_session_uses_stored_time_of_day_bucket() {
    let persisted_toml = r#"
            [[focus_sessions]]
            date = "2026-04-09"
            task_label = "Project A"
            focused_seconds = 1800
            completion_timestamp_epoch_secs = 1712617200
            completion_time_of_day_bucket = "morning"
        "#;
    let restored = FocusStats::try_from_toml(persisted_toml).unwrap();

    let rows = restored.productivity_comparison(
        ComparisonDimension::TimeOfDay,
        &ProductivityComparisonFilter::default(),
        10,
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].label, "Morning");
    assert_eq!(rows[0].sessions_completed, 1);
    assert_eq!(rows[0].focused_minutes(), 30);
}

#[test]
fn session_export_omits_task_note_metadata_fields() {
    let mut stats = FocusStats::default();
    let goal = DailyGoalSnapshot {
        minutes: 25,
        pomodoros: 1,
    };
    stats.record_focus_elapsed("2026-04-09", 30 * 60, goal);
    stats.record_completed_pomodoro_with_metadata(
        "2026-04-09",
        goal,
        FocusSessionMetadata {
            task_label: Some("Project A"),
        },
        30 * 60,
        Some(ProfileId::Classic),
    );

    let export = stats.export_data();
    assert_eq!(export.sessions.len(), 1);
    assert_eq!(export.sessions[0].task_label, "Project A");
    let value = serde_json::to_value(&export.sessions[0]).unwrap();
    assert!(value.get("task_note").is_none());
}

#[test]
fn invalid_toml_returns_parse_error() {
    assert!(FocusStats::try_from_toml("this is not valid toml").is_err());
}

#[test]
fn current_day_key_uses_iso_date_format() {
    let key = current_day_key();
    assert_eq!(key.len(), 10);
    assert_eq!(&key[4..5], "-");
    assert_eq!(&key[7..8], "-");
}

#[test]
fn goal_streak_counts_consecutive_completed_days() {
    let mut stats = FocusStats::default();
    let goal = DailyGoalSnapshot {
        minutes: 30,
        pomodoros: 1,
    };

    for day in ["2026-04-07", "2026-04-08", "2026-04-09"] {
        stats.record_focus_elapsed(day, 30 * 60, goal);
        stats.record_completed_pomodoro(day, goal);
    }

    let streak = stats.goal_streak(
        chrono::NaiveDate::from_ymd_opt(2026, 4, 9).unwrap(),
        goal,
        stats.daily_for("2026-04-09"),
    );

    assert_eq!(streak.current, 3);
    assert_eq!(streak.best, 3);
}

#[test]
fn goal_streak_keeps_running_until_today_is_missed() {
    let mut stats = FocusStats::default();
    let goal = DailyGoalSnapshot {
        minutes: 60,
        pomodoros: 1,
    };

    for day in ["2026-04-07", "2026-04-08"] {
        stats.record_focus_elapsed(day, 60 * 60, goal);
        stats.record_completed_pomodoro(day, goal);
    }

    let streak = stats.goal_streak(
        chrono::NaiveDate::from_ymd_opt(2026, 4, 9).unwrap(),
        goal,
        DailyStats::default(),
    );

    assert_eq!(streak.current, 2);
    assert_eq!(streak.best, 2);
}

#[test]
fn goal_streak_uses_current_goal_as_legacy_fallback() {
    let mut stats = FocusStats::default();
    {
        let day = stats.daily.entry("2026-04-09".to_string()).or_default();
        day.focused_seconds = 45 * 60;
        day.pomodoros_completed = 2;
    }

    let goal = DailyGoalSnapshot {
        minutes: 30,
        pomodoros: 1,
    };
    let today_stats = stats.daily_for("2026-04-09");
    let streak = stats.goal_streak(
        chrono::NaiveDate::from_ymd_opt(2026, 4, 9).unwrap(),
        goal,
        today_stats,
    );

    assert_eq!(streak.current, 1);
    assert_eq!(streak.best, 1);
}

#[test]
fn recent_monthly_aggregates_and_orders_newest_first() {
    let mut stats = FocusStats::default();
    stats.insert_daily_for_tests(
        "2026-03-31",
        DailyStats {
            pomodoros_completed: 1,
            focused_seconds: 15 * 60,
            goal: None,
        },
    );
    stats.insert_daily_for_tests(
        "2026-04-01",
        DailyStats {
            pomodoros_completed: 2,
            focused_seconds: 60 * 60,
            goal: None,
        },
    );
    stats.insert_daily_for_tests(
        "2026-04-08",
        DailyStats {
            pomodoros_completed: 1,
            focused_seconds: 30 * 60,
            goal: None,
        },
    );

    let recent = stats.recent_monthly(2);
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].year, 2026);
    assert_eq!(recent[0].month, 4);
    assert_eq!(recent[0].pomodoros_completed, 3);
    assert_eq!(recent[0].focused_minutes(), 90);
    assert_eq!(recent[1].year, 2026);
    assert_eq!(recent[1].month, 3);
    assert_eq!(recent[1].pomodoros_completed, 1);
    assert_eq!(recent[1].focused_minutes(), 15);
}

#[test]
fn monthly_for_day_aggregates_selected_calendar_month_only() {
    let mut stats = FocusStats::default();
    stats.insert_daily_for_tests(
        "2026-04-06",
        DailyStats {
            pomodoros_completed: 1,
            focused_seconds: 30 * 60,
            goal: None,
        },
    );
    stats.insert_daily_for_tests(
        "2026-04-08",
        DailyStats {
            pomodoros_completed: 2,
            focused_seconds: 45 * 60,
            goal: None,
        },
    );
    stats.insert_daily_for_tests(
        "2026-05-01",
        DailyStats {
            pomodoros_completed: 4,
            focused_seconds: 120 * 60,
            goal: None,
        },
    );

    let month = stats.monthly_for_day(chrono::NaiveDate::from_ymd_opt(2026, 4, 10).unwrap());
    assert_eq!(month.year, 2026);
    assert_eq!(month.month, 4);
    assert_eq!(month.pomodoros_completed, 3);
    assert_eq!(month.focused_minutes(), 75);
}

#[test]
fn latest_monthly_heatmap_uses_latest_recorded_month_data() {
    let mut stats = FocusStats::default();
    stats.insert_daily_for_tests(
        "2026-03-31",
        DailyStats {
            pomodoros_completed: 1,
            focused_seconds: 15 * 60,
            goal: None,
        },
    );
    stats.insert_daily_for_tests(
        "2026-04-01",
        DailyStats {
            pomodoros_completed: 2,
            focused_seconds: 60 * 60,
            goal: None,
        },
    );
    stats.insert_daily_for_tests(
        "2026-04-03",
        DailyStats {
            pomodoros_completed: 1,
            focused_seconds: 45 * 60,
            goal: None,
        },
    );

    let heatmap = stats.latest_monthly_heatmap();
    assert_eq!(heatmap.year, 2026);
    assert_eq!(heatmap.month, 4);
    assert_eq!(heatmap.days_in_month, 30);
    assert_eq!(heatmap.max_focused_minutes, 60);
    assert_eq!(heatmap.days[0].day, 1);
    assert_eq!(heatmap.days[0].pomodoros_completed, 2);
    assert_eq!(heatmap.days[0].focused_minutes(), 60);
    assert_eq!(heatmap.days[1].day, 2);
    assert_eq!(heatmap.days[1].focused_minutes(), 0);
    assert_eq!(heatmap.days[2].day, 3);
    assert_eq!(heatmap.days[2].focused_minutes(), 45);
}

#[test]
fn profile_totals_groups_by_profile_and_unknown() {
    let mut stats = FocusStats::default();
    let goal = DailyGoalSnapshot {
        minutes: 25,
        pomodoros: 1,
    };
    stats.record_completed_pomodoro_with_task(
        "2026-04-09",
        goal,
        Some("Project A"),
        50 * 60,
        Some(ProfileId::DeepWork),
    );
    stats.record_completed_pomodoro_with_task(
        "2026-04-10",
        goal,
        Some("Project B"),
        25 * 60,
        Some(ProfileId::Classic),
    );
    stats.record_completed_pomodoro_with_task("2026-04-11", goal, Some("Project C"), 40 * 60, None);

    let totals = stats.profile_totals();
    assert_eq!(totals.len(), 3);
    let deep_work = totals
        .iter()
        .find(|entry| entry.profile == ProfileBucket::DeepWork)
        .copied()
        .unwrap();
    let classic = totals
        .iter()
        .find(|entry| entry.profile == ProfileBucket::Classic)
        .copied()
        .unwrap();
    let unknown = totals
        .iter()
        .find(|entry| entry.profile == ProfileBucket::Unknown)
        .copied()
        .unwrap();

    assert_eq!(deep_work.pomodoros_completed, 1);
    assert_eq!(deep_work.focused_minutes(), 50);
    assert_eq!(classic.pomodoros_completed, 1);
    assert_eq!(classic.focused_minutes(), 25);
    assert_eq!(unknown.pomodoros_completed, 1);
    assert_eq!(unknown.focused_minutes(), 40);
}

#[test]
fn profile_effectiveness_reports_share_and_average_minutes() {
    let mut stats = FocusStats::default();
    let goal = DailyGoalSnapshot {
        minutes: 25,
        pomodoros: 1,
    };
    stats.record_completed_pomodoro_with_task(
        "2026-04-09",
        goal,
        Some("Project A"),
        50 * 60,
        Some(ProfileId::DeepWork),
    );
    stats.record_completed_pomodoro_with_task(
        "2026-04-10",
        goal,
        Some("Project D"),
        40 * 60,
        Some(ProfileId::DeepWork),
    );
    stats.record_completed_pomodoro_with_task(
        "2026-04-11",
        goal,
        Some("Project B"),
        25 * 60,
        Some(ProfileId::Classic),
    );
    stats.record_completed_pomodoro_with_task("2026-04-12", goal, Some("Project C"), 40 * 60, None);

    let effectiveness = stats.profile_effectiveness();
    assert_eq!(effectiveness.len(), 3);

    let deep_work = effectiveness
        .iter()
        .find(|entry| entry.profile == ProfileBucket::DeepWork)
        .copied()
        .unwrap();
    let classic = effectiveness
        .iter()
        .find(|entry| entry.profile == ProfileBucket::Classic)
        .copied()
        .unwrap();
    let unknown = effectiveness
        .iter()
        .find(|entry| entry.profile == ProfileBucket::Unknown)
        .copied()
        .unwrap();

    assert_eq!(deep_work.sessions_completed, 2);
    assert_eq!(deep_work.active_days, 2);
    assert_eq!(deep_work.average_focused_minutes_per_session(), 45);
    assert_eq!(deep_work.focus_share_pct, 58);

    assert_eq!(classic.sessions_completed, 1);
    assert_eq!(classic.average_focused_minutes_per_session(), 25);
    assert_eq!(classic.focus_share_pct, 16);

    assert_eq!(unknown.sessions_completed, 1);
    assert_eq!(unknown.average_focused_minutes_per_session(), 40);
    assert_eq!(unknown.focus_share_pct, 26);
}

#[test]
fn productivity_comparison_groups_time_of_day_buckets_and_unknown() {
    let mut stats = FocusStats::default();
    let goal = DailyGoalSnapshot {
        minutes: 25,
        pomodoros: 1,
    };
    let day = current_day_key();

    stats.record_completed_pomodoro_with_metadata_at(
        &day,
        goal,
        FocusSessionMetadata {
            task_label: Some("Docs"),
        },
        30 * 60,
        Some(ProfileId::Classic),
        Some(local_timestamp_today(9, 0)),
    );
    stats.record_completed_pomodoro_with_metadata_at(
        &day,
        goal,
        FocusSessionMetadata {
            task_label: Some("Build"),
        },
        20 * 60,
        Some(ProfileId::DeepWork),
        Some(local_timestamp_today(23, 0)),
    );
    stats.record_completed_pomodoro_with_metadata_at(
        &day,
        goal,
        FocusSessionMetadata {
            task_label: Some("Review"),
        },
        10 * 60,
        Some(ProfileId::Custom),
        None,
    );

    let rows = stats.productivity_comparison(
        ComparisonDimension::TimeOfDay,
        &ProductivityComparisonFilter {
            task_label: None,
            profile: None,
            time_of_day: None,
        },
        10,
    );

    assert_eq!(rows.len(), 3);
    assert!(rows.iter().any(|row| {
        row.label == "Morning" && row.sessions_completed == 1 && row.focused_minutes() == 30
    }));
    assert!(rows.iter().any(|row| row.label == "Night"
        && row.sessions_completed == 1
        && row.focused_minutes() == 20));
    assert!(rows.iter().any(|row| {
        row.label == "Unknown" && row.sessions_completed == 1 && row.focused_minutes() == 10
    }));
}

#[test]
fn productivity_comparison_applies_task_and_time_filters() {
    let mut stats = FocusStats::default();
    let goal = DailyGoalSnapshot {
        minutes: 25,
        pomodoros: 1,
    };
    let day = current_day_key();

    stats.record_completed_pomodoro_with_metadata_at(
        &day,
        goal,
        FocusSessionMetadata {
            task_label: Some("Docs"),
        },
        30 * 60,
        Some(ProfileId::Classic),
        Some(local_timestamp_today(8, 0)),
    );
    stats.record_completed_pomodoro_with_metadata_at(
        &day,
        goal,
        FocusSessionMetadata {
            task_label: Some("Docs"),
        },
        20 * 60,
        Some(ProfileId::DeepWork),
        Some(local_timestamp_today(9, 0)),
    );
    stats.record_completed_pomodoro_with_metadata_at(
        &day,
        goal,
        FocusSessionMetadata {
            task_label: Some("Docs"),
        },
        15 * 60,
        Some(ProfileId::Classic),
        Some(local_timestamp_today(19, 0)),
    );
    stats.record_completed_pomodoro_with_metadata_at(
        &day,
        goal,
        FocusSessionMetadata {
            task_label: Some("Coding"),
        },
        40 * 60,
        Some(ProfileId::Classic),
        Some(local_timestamp_today(10, 0)),
    );

    let rows = stats.productivity_comparison(
        ComparisonDimension::Profile,
        &ProductivityComparisonFilter {
            task_label: Some("Docs".to_string()),
            profile: None,
            time_of_day: Some(TimeOfDayBucket::Morning),
        },
        10,
    );

    assert_eq!(rows.len(), 2);
    let classic = rows
        .iter()
        .find(|row| row.label == "Basic")
        .expect("basic row should be present");
    let deep_work = rows
        .iter()
        .find(|row| row.label == "Standard")
        .expect("standard row should be present");
    assert_eq!(classic.sessions_completed, 1);
    assert_eq!(classic.focused_minutes(), 30);
    assert_eq!(classic.focus_share_pct, 60);
    assert_eq!(deep_work.sessions_completed, 1);
    assert_eq!(deep_work.focused_minutes(), 20);
    assert_eq!(deep_work.focus_share_pct, 40);
}

#[test]
fn export_to_dir_writes_daily_and_weekly_json_and_csv() {
    let mut stats = FocusStats::default();
    let goal = DailyGoalSnapshot {
        minutes: 25,
        pomodoros: 1,
    };
    let today = chrono::Local::now().date_naive();
    let labeled_day = today.format("%Y-%m-%d").to_string();
    let other_day_date = [today.pred_opt(), today.succ_opt()]
        .into_iter()
        .flatten()
        .find(|candidate| candidate.iso_week() == today.iso_week())
        .unwrap_or(today);
    let other_day = other_day_date.format("%Y-%m-%d").to_string();
    let recent_window_start = today
        .checked_sub_signed(chrono::Duration::days(6))
        .unwrap()
        .format("%Y-%m-%d")
        .to_string();
    let recent_window_end = today.format("%Y-%m-%d").to_string();
    let previous_window_end = today
        .checked_sub_signed(chrono::Duration::days(7))
        .unwrap()
        .format("%Y-%m-%d")
        .to_string();
    let previous_window_start = today
        .checked_sub_signed(chrono::Duration::days(13))
        .unwrap()
        .format("%Y-%m-%d")
        .to_string();

    stats.record_focus_elapsed(&labeled_day, 30 * 60, goal);
    stats.record_completed_pomodoro_with_task(
        &labeled_day,
        goal,
        Some("Project A"),
        30 * 60,
        Some(ProfileId::Classic),
    );
    stats.record_session_interruption_event(
        &labeled_day,
        1_711_000_111,
        SessionInterruptionReason::ManualSkip,
        FocusSessionMetadata {
            task_label: Some("Project A"),
        },
        600,
        Some(ProfileId::Classic),
    );
    stats.record_break_glass_override_event(&labeled_day, 1_711_000_000, Some("Project A"), 300);
    stats.record_focus_elapsed(&other_day, 45 * 60, goal);
    stats.record_completed_pomodoro(&other_day, goal);

    let export_dir = unique_temp_dir("stats-export");
    let exported = stats.export_to_dir(&export_dir).unwrap();

    assert_eq!(
        exported.json_path.file_name().unwrap(),
        JSON_EXPORT_FILE_NAME
    );
    assert_eq!(exported.csv_path.file_name().unwrap(), CSV_EXPORT_FILE_NAME);

    let json = fs::read_to_string(&exported.json_path).unwrap();
    let json_value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(json_value["schema_version"], EXPORT_SCHEMA_VERSION);
    let daily = json_value["daily"].as_array().unwrap();
    let weekly = json_value["weekly"].as_array().unwrap();
    let sessions = json_value["sessions"].as_array().unwrap();
    let interruptions = json_value["interruptions"].as_array().unwrap();
    let overrides = json_value["overrides"].as_array().unwrap();
    let task_totals = json_value["task_totals"].as_array().unwrap();
    let task_trends = json_value["task_trends"].as_array().unwrap();
    let weekly_consistency = json_value["weekly_consistency"].as_array().unwrap();
    let focus_scores = json_value["focus_scores"].as_array().unwrap();
    let profile_effectiveness = json_value["profile_effectiveness"].as_array().unwrap();
    let productivity_comparisons = json_value["productivity_comparisons"].as_array().unwrap();
    let history_kpis = json_value["history_kpis"].as_object().unwrap();
    assert_eq!(daily.len(), 2);
    assert!(!weekly.is_empty());
    assert_eq!(sessions.len(), 1);
    assert_eq!(interruptions.len(), 1);
    assert_eq!(overrides.len(), 1);
    assert_eq!(task_totals.len(), 1);
    assert_eq!(task_trends.len(), 1);
    assert!(!weekly_consistency.is_empty());
    assert!(!focus_scores.is_empty());
    assert_eq!(profile_effectiveness.len(), 1);
    assert!(!productivity_comparisons.is_empty());
    for card_id in [
        "session_summary",
        "focus_score",
        "goal_streak",
        "focus_risk",
        "weekly_allocation",
        "last_interruption",
        "stats_growth",
        "retention",
        "comparison_filters",
    ] {
        assert!(
            history_kpis.contains_key(card_id),
            "missing history_kpis entry for {card_id}"
        );
    }
    assert!(
        daily
            .iter()
            .any(|entry| entry["date"] == labeled_day && entry["goal_met"] == true)
    );
    assert!(weekly.iter().any(|entry| entry["focused_minutes"] == 75));
    assert_eq!(sessions[0]["task_label"], "Project A");
    assert!(sessions[0].get("focus_intention").is_none());
    assert!(sessions[0].get("task_note").is_none());
    assert_eq!(sessions[0]["focused_minutes"], 30);
    assert_eq!(sessions[0]["profile"], "basic");
    assert_eq!(interruptions[0]["reason"], "manual_skip");
    assert_eq!(interruptions[0]["remaining_secs"], 600);
    assert_eq!(interruptions[0]["task_label"], "Project A");
    assert_eq!(overrides[0]["duration_seconds"], 300);
    assert_eq!(overrides[0]["task_label"], "Project A");
    assert_eq!(task_totals[0]["task_label"], "Project A");
    assert_eq!(task_totals[0]["focused_minutes"], 30);
    assert_eq!(task_trends[0]["task_label"], "Project A");
    assert_eq!(task_trends[0]["recent_window_start"], recent_window_start);
    assert_eq!(task_trends[0]["recent_window_end"], recent_window_end);
    assert_eq!(
        task_trends[0]["previous_window_start"],
        previous_window_start
    );
    assert_eq!(task_trends[0]["previous_window_end"], previous_window_end);
    assert_eq!(task_trends[0]["delta_focused_minutes"], 30);
    assert!(
        weekly_consistency
            .iter()
            .any(|entry| entry["consistency_score_pct"].as_u64().unwrap_or(0) > 0)
    );
    assert!(
        focus_scores
            .iter()
            .any(|entry| entry.get("focus_score_pct").is_some())
    );
    assert_eq!(profile_effectiveness[0]["profile"], "Basic");
    assert_eq!(
        profile_effectiveness[0]["average_focused_minutes_per_session"],
        30
    );
    assert_eq!(profile_effectiveness[0]["focus_share_pct"], 100);
    assert!(
        productivity_comparisons
            .iter()
            .any(|entry| entry["dimension"] == "task_label")
    );
    assert!(
        productivity_comparisons
            .iter()
            .any(|entry| entry["dimension"] == "profile")
    );
    assert!(
        productivity_comparisons
            .iter()
            .any(|entry| entry["dimension"] == "time_of_day")
    );

    let csv = fs::read_to_string(&exported.csv_path).unwrap();
    let csv_header = csv.lines().next().expect("csv header should be present");
    let focus_session_line = csv
        .lines()
        .find(|line| line.contains(",focus_session,"))
        .expect("focus session row should be present");
    assert!(focus_session_line.contains(",Basic,"));
    let interruption_line = csv
        .lines()
        .find(|line| line.contains(",session_interruption,"))
        .expect("session interruption row should be present");
    assert!(interruption_line.contains(",Basic,"));
    assert!(csv_header.contains("comparison_dimension"));
    assert!(csv_header.contains("comparison_label"));
    assert!(csv_header.contains("time_of_day_bucket"));
    assert!(csv_header.contains("kpi_card_id"));
    assert!(csv_header.contains("kpi_payload_json"));
    assert!(csv.contains(&format!("{},daily,{labeled_day}", EXPORT_SCHEMA_VERSION)));
    assert!(csv.contains(&format!("{},weekly,,", EXPORT_SCHEMA_VERSION)));
    assert!(csv.contains(&format!(
        "{},focus_session,{labeled_day}",
        EXPORT_SCHEMA_VERSION
    )));
    assert!(csv.contains("Project A"));
    assert!(csv.contains(&format!(
        "{},session_interruption,{labeled_day}",
        EXPORT_SCHEMA_VERSION
    )));
    assert!(csv.contains("manual_skip"));
    assert!(csv.contains("1711000111"));
    assert!(csv.contains(&format!(
        "{},break_glass_override,{labeled_day}",
        EXPORT_SCHEMA_VERSION
    )));
    assert!(csv.contains("1711000000"));
    assert!(csv.contains(&format!("{},task_summary", EXPORT_SCHEMA_VERSION)));
    assert!(csv.contains(&format!("{},task_trend", EXPORT_SCHEMA_VERSION)));
    assert!(csv.contains(&format!("{},weekly_consistency,", EXPORT_SCHEMA_VERSION)));
    assert!(csv.contains(&format!("{},focus_score,", EXPORT_SCHEMA_VERSION)));
    assert!(csv.contains(&format!(
        "{},profile_effectiveness,,,,,1,1800,30",
        EXPORT_SCHEMA_VERSION
    )));
    assert!(csv.contains(&format!(
        "{},productivity_comparison",
        EXPORT_SCHEMA_VERSION
    )));
    assert!(csv.contains(&format!("{},history_kpi", EXPORT_SCHEMA_VERSION)));
    assert!(csv.contains("Basic,1,1,"));

    #[derive(serde::Deserialize)]
    struct CsvKpiRow {
        record_type: String,
        kpi_card_id: Option<String>,
        kpi_payload_json: Option<String>,
    }

    let mut csv_reader = csv::Reader::from_reader(csv.as_bytes());
    let mut csv_kpi_payloads = std::collections::BTreeMap::new();
    for row in csv_reader.deserialize::<CsvKpiRow>() {
        let row = row.expect("history kpi row should deserialize");
        if row.record_type != "history_kpi" {
            continue;
        }
        let card_id = row
            .kpi_card_id
            .expect("history_kpi row should include kpi_card_id");
        let payload = row
            .kpi_payload_json
            .expect("history_kpi row should include kpi_payload_json");
        let parsed_payload: serde_json::Value =
            serde_json::from_str(&payload).expect("kpi_payload_json should be valid JSON");
        csv_kpi_payloads.insert(card_id, parsed_payload);
    }

    assert_eq!(csv_kpi_payloads.len(), 9);
    for card_id in [
        "session_summary",
        "focus_score",
        "goal_streak",
        "focus_risk",
        "weekly_allocation",
        "last_interruption",
        "stats_growth",
        "retention",
        "comparison_filters",
    ] {
        assert_eq!(
            csv_kpi_payloads.get(card_id),
            history_kpis.get(card_id),
            "csv/json parity mismatch for {card_id}"
        );
    }

    fs::remove_dir_all(export_dir).unwrap();
}

#[test]
fn history_kpi_focus_score_uses_reference_day_context_week() {
    let mut stats = FocusStats::default();
    let day = chrono::NaiveDate::from_ymd_opt(2026, 4, 9).unwrap();
    let later_week_day = day.checked_add_signed(chrono::Duration::days(14)).unwrap();
    let goal = DailyGoalSnapshot {
        minutes: 30,
        pomodoros: 1,
    };
    let day_key = day.format("%Y-%m-%d").to_string();
    let later_day_key = later_week_day.format("%Y-%m-%d").to_string();

    stats.record_focus_elapsed(&day_key, 30 * 60, goal);
    stats.record_completed_pomodoro(&day_key, goal);
    stats.record_focus_elapsed(&later_day_key, 45 * 60, goal);
    stats.record_completed_pomodoro(&later_day_key, goal);

    let context = HistoryKpiExportContext {
        reference_day: day,
        weekly_goal: DailyGoalSnapshot {
            minutes: 30,
            pomodoros: 1,
        },
        ..HistoryKpiExportContext::default()
    };

    let export = stats.export_data_with_context(&context);
    let json_value = serde_json::to_value(&export).unwrap();
    let expected_week_label = format_week_label(day.iso_week().year(), day.iso_week().week());
    let latest_week_label = format_week_label(
        later_week_day.iso_week().year(),
        later_week_day.iso_week().week(),
    );

    assert_eq!(
        json_value["history_kpis"]["focus_score"]["week_label"],
        expected_week_label
    );
    assert_ne!(
        json_value["history_kpis"]["focus_score"]["week_label"],
        latest_week_label
    );
}

#[test]
fn export_to_dir_returns_error_when_target_is_not_directory() {
    let stats = FocusStats::default();
    let export_root = unique_temp_dir("stats-export-error");
    let not_a_directory = export_root.join("already-a-file");
    fs::write(&not_a_directory, "occupied").unwrap();

    let result = stats.export_to_dir(&not_a_directory);

    assert!(result.is_err());

    fs::remove_dir_all(export_root).unwrap();
}

#[test]
fn growth_summary_reports_sections_and_high_volume_groups() {
    let mut stats = FocusStats::default();
    let goal = DailyGoalSnapshot {
        minutes: 25,
        pomodoros: 1,
    };
    stats.record_focus_elapsed("2026-04-10", 25 * 60, goal);
    stats.record_completed_pomodoro_with_task("2026-04-10", goal, Some("Docs"), 25 * 60, None);
    stats.record_session_interruption_event(
        "2026-04-10",
        1_711_000_123,
        SessionInterruptionReason::ManualStop,
        FocusSessionMetadata {
            task_label: Some("Docs"),
        },
        300,
        None,
    );

    let summary = stats.growth_summary();
    assert!(summary.total_record_count > 0);
    assert!(summary.estimated_bytes > 0);
    assert!(!summary.sections.is_empty());
    assert!(!summary.high_volume_sections.is_empty());
    assert!(
        summary
            .sections
            .iter()
            .any(|section| section.name == "focus_sessions" && section.record_count == 1)
    );
    assert!(
        summary
            .sections
            .iter()
            .any(|section| section.name == "focus_sessions" && section.estimated_bytes > 0)
    );
    assert!(
        summary
            .sections
            .iter()
            .any(|section| section.name == "task_labels" && section.estimated_bytes > 0)
    );
    assert!(
        summary
            .sections
            .iter()
            .any(|section| section.name == "session_interruptions" && section.record_count == 1)
    );
    assert!(
        summary
            .sections
            .iter()
            .any(|section| section.name == "session_interruptions" && section.estimated_bytes > 0)
    );
}

#[test]
fn apply_retention_policy_prunes_old_high_volume_entries() {
    let mut stats = FocusStats::default();
    let goal = DailyGoalSnapshot {
        minutes: 25,
        pomodoros: 1,
    };
    let today = chrono::NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();
    let old_day_date = today
        .checked_sub_signed(chrono::Duration::days(500))
        .unwrap();
    let recent_day_date = today
        .checked_sub_signed(chrono::Duration::days(10))
        .unwrap();
    let old_day = old_day_date.format("%Y-%m-%d").to_string();
    let recent_day = recent_day_date.format("%Y-%m-%d").to_string();

    stats.record_completed_pomodoro_with_task(&old_day, goal, Some("Docs"), 25 * 60, None);
    stats.record_completed_pomodoro_with_task(&recent_day, goal, Some("Docs"), 25 * 60, None);
    stats.sync_weekly_goal_snapshot(old_day_date, goal);
    stats.sync_weekly_goal_snapshot(recent_day_date, goal);
    stats.sync_monthly_goal_snapshot(old_day_date, goal);
    stats.sync_monthly_goal_snapshot(recent_day_date, goal);
    stats.record_session_interruption_event(
        &old_day,
        1_711_000_000,
        SessionInterruptionReason::ManualStop,
        FocusSessionMetadata {
            task_label: Some("Docs"),
        },
        600,
        None,
    );
    stats.record_session_interruption_event(
        &recent_day,
        1_711_000_111,
        SessionInterruptionReason::ManualSkip,
        FocusSessionMetadata {
            task_label: Some("Docs"),
        },
        600,
        None,
    );
    stats.record_break_glass_override_event(&old_day, 1_711_000_222, Some("Docs"), 120);
    stats.record_break_glass_override_event(&recent_day, 1_711_000_333, Some("Docs"), 120);

    let result = stats.apply_retention_policy(
        crate::config::StatsRetentionConfig {
            preset: crate::config::StatsRetentionPreset::Balanced,
        },
        today,
    );
    assert_eq!(result.daily_removed, 0);
    assert_eq!(result.focus_sessions_removed, 1);
    assert_eq!(result.session_interruptions_removed, 1);
    assert_eq!(result.break_glass_overrides_removed, 1);
    assert_eq!(result.weekly_goal_snapshots_removed, 1);
    assert_eq!(result.monthly_goal_snapshots_removed, 1);
    assert_eq!(result.total_removed(), 5);
    assert!(result.any_removed());
    assert_eq!(stats.task_totals(10)[0].pomodoros_completed, 1);
    assert_eq!(stats.recent_break_glass_overrides(10).len(), 1);
    assert_eq!(stats.recent_session_interruptions(10).len(), 1);
}

#[test]
fn retention_preview_reports_changes_without_mutating_stats() {
    let mut stats = FocusStats::default();
    let goal = DailyGoalSnapshot {
        minutes: 25,
        pomodoros: 1,
    };
    stats.record_completed_pomodoro_with_task("2024-01-01", goal, Some("Docs"), 25 * 60, None);
    let today = chrono::NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();

    let preview = stats.retention_preview(
        crate::config::StatsRetentionConfig {
            preset: crate::config::StatsRetentionPreset::Aggressive,
        },
        today,
    );
    assert_eq!(preview.focus_sessions_removed, 1);
    assert!(preview.any_removed());

    let summary = stats.growth_summary();
    assert!(
        summary
            .sections
            .iter()
            .any(|section| section.name == "focus_sessions" && section.record_count == 1)
    );
}

#[test]
fn create_unique_temp_path_changes_between_calls() {
    let target = Path::new("focustime-stats.json");
    let first = create_unique_temp_path(target);
    let second = create_unique_temp_path(target);

    assert_ne!(first, second);
}

fn unique_temp_dir(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("focustime-{label}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}
