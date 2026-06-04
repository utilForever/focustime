use chrono::Datelike;

use crate::stats::{
    DailyGoalSnapshot, DailyStats, FocusRiskLevel, FocusRiskSignal, FocusStats, GoalPeriod,
    GoalRiskForecast, GoalStreak, StreakRiskForecast, WeeklyStats,
    consistency_score_from_active_days, daily_has_activity, days_in_month,
    percentage_round_nearest, weekly_completion_score_pct,
};

pub(super) fn observed_goal_miss_for_candidate(
    stats: &FocusStats,
    candidate: chrono::NaiveDate,
    day_stats: DailyStats,
    daily_goal: DailyGoalSnapshot,
    weekly_goal: DailyGoalSnapshot,
    monthly_goal: DailyGoalSnapshot,
) -> Option<bool> {
    let mut observed_outcome = false;
    let mut observed_miss = false;

    if daily_goal.has_any_target() {
        observed_outcome = true;
        observed_miss |= !daily_goal.is_met_by(day_stats);
    }

    if candidate.weekday().num_days_from_monday() == 6 && weekly_goal.has_any_target() {
        observed_outcome = true;
        let weekly_stats = stats.weekly_for_day(candidate);
        observed_miss |= !weekly_goal.is_met_by_totals(
            weekly_stats.focused_minutes(),
            weekly_stats.pomodoros_completed,
        );
    }

    if candidate.day() == days_in_month(candidate.year(), candidate.month())
        && monthly_goal.has_any_target()
    {
        observed_outcome = true;
        let monthly_stats = stats.monthly_for_day(candidate);
        observed_miss |= !monthly_goal.is_met_by_totals(
            monthly_stats.focused_minutes(),
            monthly_stats.pomodoros_completed,
        );
    }

    observed_outcome.then_some(observed_miss)
}

pub(super) fn classify_calibration_signal(
    alert_active: bool,
    observed_miss: bool,
    alert_count: &mut u32,
    true_positive_alerts: &mut u32,
    false_positive_alerts: &mut u32,
    missed_warning_count: &mut u32,
) {
    if alert_active {
        *alert_count = alert_count.saturating_add(1);
        if observed_miss {
            *true_positive_alerts = true_positive_alerts.saturating_add(1);
        } else {
            *false_positive_alerts = false_positive_alerts.saturating_add(1);
        }
    } else if observed_miss {
        *missed_warning_count = missed_warning_count.saturating_add(1);
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct CadenceWindow {
    window_days: u8,
    active_days: u8,
    focused_minutes: u64,
    pomodoros_completed: u32,
}

impl CadenceWindow {
    fn consistency_pct(self) -> u8 {
        consistency_score_from_active_days(self.active_days)
    }

    fn average_daily_minutes(self) -> u64 {
        self.focused_minutes / u64::from(self.window_days.max(1))
    }

    fn average_daily_pomodoros(self) -> u64 {
        u64::from(self.pomodoros_completed) / u64::from(self.window_days.max(1))
    }
}

pub(super) fn rolling_cadence_window(
    stats: &FocusStats,
    day: chrono::NaiveDate,
    window_days: u8,
) -> CadenceWindow {
    let mut cadence = CadenceWindow {
        window_days: window_days.max(1),
        active_days: 0,
        focused_minutes: 0,
        pomodoros_completed: 0,
    };
    for offset in 0..cadence.window_days {
        let candidate = day
            .checked_sub_signed(chrono::Duration::days(i64::from(offset)))
            .unwrap_or(day);
        let day_key = candidate.format("%Y-%m-%d").to_string();
        let day_stats = stats.daily_for(&day_key);
        if daily_has_activity(day_stats) {
            cadence.active_days = cadence
                .active_days
                .saturating_add(1)
                .min(cadence.window_days);
        }
        cadence.focused_minutes = cadence
            .focused_minutes
            .saturating_add(day_stats.focused_minutes());
        cadence.pomodoros_completed = cadence
            .pomodoros_completed
            .saturating_add(day_stats.pomodoros_completed);
    }
    cadence
}

pub(super) fn goal_risk_forecast(
    period: GoalPeriod,
    goal: DailyGoalSnapshot,
    completed_minutes: u64,
    completed_pomodoros: u32,
    remaining_days: u32,
    cadence: CadenceWindow,
) -> GoalRiskForecast {
    const GOAL_WEIGHT_COMPLETION_GAP: u16 = 45;
    const GOAL_WEIGHT_CONSISTENCY_GAP: u16 = 35;
    const GOAL_WEIGHT_PACE_GAP: u16 = 20;

    if !goal.has_any_target() {
        return GoalRiskForecast {
            period,
            configured: false,
            met: false,
            completion_pct: None,
            risk_score_pct: 0,
            risk_level: FocusRiskLevel::Low,
            signals: vec![risk_signal("status", "goal off")],
        };
    }

    let met = goal.is_met_by_totals(completed_minutes, completed_pomodoros);
    let completion_pct = completion_pct_for_totals(goal, completed_minutes, completed_pomodoros);
    let completion_gap = completion_pct.map_or(0, |pct| 100_u8.saturating_sub(pct));
    let consistency_pct = cadence.consistency_pct();
    let consistency_gap = 100_u8.saturating_sub(consistency_pct);
    let pace_gap = pace_gap_pct(
        goal,
        completed_minutes,
        completed_pomodoros,
        remaining_days,
        cadence,
    );
    let risk_score_pct = if met {
        0
    } else {
        weighted_pct(&[
            (completion_gap, GOAL_WEIGHT_COMPLETION_GAP),
            (consistency_gap, GOAL_WEIGHT_CONSISTENCY_GAP),
            (pace_gap, GOAL_WEIGHT_PACE_GAP),
        ])
    };
    let risk_level = FocusRiskLevel::from_score(risk_score_pct);

    let mut signals = vec![
        risk_signal("completion", &format!("{}%", completion_pct.unwrap_or(0))),
        risk_signal(
            "consistency",
            &format!(
                "{}% ({}/{} days)",
                consistency_pct, cadence.active_days, cadence.window_days
            ),
        ),
    ];
    if met {
        signals.push(risk_signal("pace", "goal already met"));
    } else {
        signals.push(risk_signal(
            "pace",
            &format!(
                "{}% gap with {} day(s) left",
                pace_gap,
                remaining_days.max(1)
            ),
        ));
    }

    GoalRiskForecast {
        period,
        configured: true,
        met,
        completion_pct,
        risk_score_pct,
        risk_level,
        signals,
    }
}

pub(super) fn streak_risk_forecast(
    stats: &FocusStats,
    day: chrono::NaiveDate,
    daily_goal: DailyGoalSnapshot,
    today_stats: DailyStats,
    cadence: CadenceWindow,
    streak: GoalStreak,
) -> StreakRiskForecast {
    const STREAK_WEIGHT_TODAY_PRESSURE: u16 = 25;
    const STREAK_WEIGHT_RELIABILITY_GAP: u16 = 50;
    const STREAK_WEIGHT_CONSISTENCY_GAP: u16 = 25;
    const STREAK_TODAY_PRESSURE_MET: u8 = 15;
    const STREAK_TODAY_PRESSURE_UNMET: u8 = 70;
    const STREAK_ALERT_BONUS_MEDIUM_STREAK: u8 = 4;
    const STREAK_ALERT_BONUS_LONG_STREAK: u8 = 8;

    if !daily_goal.has_any_target() {
        return StreakRiskForecast {
            configured: false,
            current_streak: 0,
            best_streak: 0,
            today_goal_met: false,
            recent_goal_reliability_pct: 0,
            risk_score_pct: 0,
            risk_level: FocusRiskLevel::Low,
            signals: vec![risk_signal("status", "daily goal off")],
        };
    }

    let today_goal_met = daily_goal.is_met_by(today_stats);
    let reliability_pct = rolling_goal_reliability_pct(stats, day, daily_goal, 7);
    let consistency_pct = cadence.consistency_pct();
    let today_pressure = if today_goal_met {
        STREAK_TODAY_PRESSURE_MET
    } else {
        STREAK_TODAY_PRESSURE_UNMET
    };
    let reliability_gap = 100_u8.saturating_sub(reliability_pct);
    let consistency_gap = 100_u8.saturating_sub(consistency_pct);
    let mut risk_score_pct = weighted_pct(&[
        (today_pressure, STREAK_WEIGHT_TODAY_PRESSURE),
        (reliability_gap, STREAK_WEIGHT_RELIABILITY_GAP),
        (consistency_gap, STREAK_WEIGHT_CONSISTENCY_GAP),
    ]);
    risk_score_pct = if streak.current >= 7 {
        risk_score_pct
            .saturating_add(STREAK_ALERT_BONUS_LONG_STREAK)
            .min(100)
    } else if streak.current >= 3 {
        risk_score_pct
            .saturating_add(STREAK_ALERT_BONUS_MEDIUM_STREAK)
            .min(100)
    } else {
        risk_score_pct
    };
    let risk_level = FocusRiskLevel::from_score(risk_score_pct);

    let signals = vec![
        risk_signal(
            "today",
            if today_goal_met {
                "met so far"
            } else {
                "not met yet"
            },
        ),
        risk_signal("recent reliability", &format!("{reliability_pct}%")),
        risk_signal(
            "consistency",
            &format!(
                "{}% ({}/{} days)",
                consistency_pct, cadence.active_days, cadence.window_days
            ),
        ),
        risk_signal(
            "streak",
            &format!("{}d current / {}d best", streak.current, streak.best),
        ),
    ];

    StreakRiskForecast {
        configured: true,
        current_streak: streak.current,
        best_streak: streak.best,
        today_goal_met,
        recent_goal_reliability_pct: reliability_pct,
        risk_score_pct,
        risk_level,
        signals,
    }
}

pub(super) fn rolling_goal_reliability_pct(
    stats: &FocusStats,
    day: chrono::NaiveDate,
    fallback_goal: DailyGoalSnapshot,
    window_days: u8,
) -> u8 {
    let mut eligible_days = 0_u32;
    let mut met_days = 0_u32;
    for offset in 0..window_days.max(1) {
        let candidate = day
            .checked_sub_signed(chrono::Duration::days(i64::from(offset)))
            .unwrap_or(day);
        let day_key = candidate.format("%Y-%m-%d").to_string();
        let day_stats = stats.daily_for(&day_key);
        let has_observed_day =
            candidate == day || stats.daily.contains_key(&day_key) || daily_has_activity(day_stats);
        if !has_observed_day {
            continue;
        }
        let configured_goal = stats
            .daily
            .get(&day_key)
            .and_then(|entry| entry.goal)
            .unwrap_or(fallback_goal);
        if !configured_goal.has_any_target() {
            continue;
        }
        eligible_days = eligible_days.saturating_add(1);
        if configured_goal.is_met_by(day_stats) {
            met_days = met_days.saturating_add(1);
        }
    }

    if eligible_days == 0 {
        0
    } else {
        percentage_round_nearest(u64::from(met_days), u64::from(eligible_days))
    }
}

pub(super) fn completion_pct_for_totals(
    goal: DailyGoalSnapshot,
    focused_minutes: u64,
    pomodoros_completed: u32,
) -> Option<u8> {
    weekly_completion_score_pct(
        goal,
        WeeklyStats {
            pomodoros_completed,
            focused_seconds: focused_minutes.saturating_mul(60),
            ..WeeklyStats::default()
        },
    )
}

pub(super) fn pace_gap_pct(
    goal: DailyGoalSnapshot,
    completed_minutes: u64,
    completed_pomodoros: u32,
    remaining_days: u32,
    cadence: CadenceWindow,
) -> u8 {
    let days_remaining = u64::from(remaining_days.max(1));
    let remaining_minutes = goal.minutes.saturating_sub(completed_minutes);
    let remaining_pomodoros = u64::from(goal.pomodoros.saturating_sub(completed_pomodoros));
    let required_minutes_per_day = if goal.minutes > 0 {
        div_ceil_u64(remaining_minutes, days_remaining)
    } else {
        0
    };
    let required_pomodoros_per_day = if goal.pomodoros > 0 {
        div_ceil_u64(remaining_pomodoros, days_remaining)
    } else {
        0
    };

    let minutes_gap = gap_pct(cadence.average_daily_minutes(), required_minutes_per_day);
    let pomodoros_gap = gap_pct(
        cadence.average_daily_pomodoros(),
        required_pomodoros_per_day,
    );
    minutes_gap.max(pomodoros_gap)
}

pub(super) fn gap_pct(recent_rate: u64, required_rate: u64) -> u8 {
    if required_rate == 0 || recent_rate >= required_rate {
        return 0;
    }
    percentage_round_nearest(required_rate.saturating_sub(recent_rate), required_rate)
}

pub(super) fn weighted_pct(parts: &[(u8, u16)]) -> u8 {
    let total_weight = parts.iter().fold(0_u64, |total, (_, weight)| {
        total.saturating_add(u64::from(*weight))
    });
    if total_weight == 0 {
        return 0;
    }
    let weighted_sum = parts.iter().fold(0_u64, |total, (value, weight)| {
        total.saturating_add(u64::from(*value).saturating_mul(u64::from(*weight)))
    });
    ((weighted_sum.saturating_add(total_weight / 2)) / total_weight).min(u64::from(u8::MAX)) as u8
}

pub(super) fn risk_signal(label: &str, value: &str) -> FocusRiskSignal {
    FocusRiskSignal {
        label: label.to_string(),
        value: value.to_string(),
    }
}

pub(super) fn remaining_days_in_week(day: chrono::NaiveDate) -> u32 {
    7_u32.saturating_sub(day.weekday().num_days_from_monday())
}

pub(super) fn remaining_days_in_month(day: chrono::NaiveDate) -> u32 {
    let total_days = days_in_month(day.year(), day.month());
    total_days.saturating_sub(day.day()).saturating_add(1)
}

pub(super) fn div_ceil_u64(value: u64, divisor: u64) -> u64 {
    if divisor == 0 {
        return value;
    }
    value.div_ceil(divisor)
}
