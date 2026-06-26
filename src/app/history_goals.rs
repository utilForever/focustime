use chrono::{Datelike, Duration, NaiveDate, Weekday};

use crate::app::{
    App, AppMode, BreakGlassOverrideEvent, DailyGoalSnapshot, FocusInterruptionContext,
    FocusSessionMetadata, Local, SessionInterruptionEvent, SessionInterruptionReason,
    WeeklyDailyAllocationDay, WeeklyDailyGoalAllocation, carry_over_goal_target, current_day_key,
    previous_month_reference_day,
};
use crate::config::RecurringScheduleConfig;
use crate::stats::WeeklyStats;

impl App {
    pub(super) fn build_focus_interruption_context(
        &self,
        reason: SessionInterruptionReason,
    ) -> FocusInterruptionContext {
        let now = Local::now();
        let day_key = now.date_naive().format("%Y-%m-%d").to_string();
        let timestamp_epoch_secs = now.timestamp().max(0) as u64;
        let task_label = self
            .active_focus_task_label
            .clone()
            .or_else(|| self.selected_task_label.clone());
        FocusInterruptionContext {
            day_key,
            timestamp_epoch_secs,
            reason,
            task_label: task_label.clone(),
            remaining_secs: self.timer.remaining_secs,
            profile: self.active_focus_profile.or(Some(self.selected_profile)),
        }
    }

    pub(super) fn record_session_interruption_event(&mut self, context: FocusInterruptionContext) {
        self.stats.record_session_interruption_event(
            &context.day_key,
            context.timestamp_epoch_secs,
            context.reason,
            FocusSessionMetadata {
                task_label: context.task_label.as_deref(),
            },
            context.remaining_secs,
            context.profile,
        );
        self.mark_stats_dirty();
    }

    pub(super) fn open_stats_history(&mut self) {
        self.pending_timer_action = None;
        self.history_feedback = None;
        self.set_mode(AppMode::StatsHistory);
    }

    pub(super) fn record_completed_focus_session(&mut self, focused_seconds: u64) {
        let day_key = current_day_key();
        let goal = self.current_goal_snapshot();
        if let Some(active_task_label) = self.active_focus_task_label.clone() {
            self.stats.record_completed_pomodoro_with_metadata(
                &day_key,
                goal,
                FocusSessionMetadata {
                    task_label: Some(active_task_label.as_str()),
                },
                focused_seconds,
                self.active_focus_profile,
            );
        } else {
            self.stats.record_completed_pomodoro(&day_key, goal);
        }
        self.mark_stats_dirty();
    }

    pub(super) fn current_goal_snapshot(&self) -> DailyGoalSnapshot {
        DailyGoalSnapshot {
            minutes: self.daily_goal.minutes,
            pomodoros: self.daily_goal.pomodoros,
        }
    }

    fn current_week_goal_snapshot(&self) -> DailyGoalSnapshot {
        DailyGoalSnapshot {
            minutes: self.weekly_goal.minutes,
            pomodoros: self.weekly_goal.pomodoros,
        }
    }

    fn current_month_goal_snapshot(&self) -> DailyGoalSnapshot {
        DailyGoalSnapshot {
            minutes: self.monthly_goal.minutes,
            pomodoros: self.monthly_goal.pomodoros,
        }
    }

    pub(super) fn effective_daily_goal_snapshot_for_day(
        &self,
        day: NaiveDate,
    ) -> DailyGoalSnapshot {
        let day_key = day.format("%Y-%m-%d").to_string();
        let base = self
            .stats
            .daily_entry(&day_key)
            .and_then(|stats| stats.goal)
            .unwrap_or_else(|| self.current_goal_snapshot());
        let previous = day.pred_opt().and_then(|previous_day| {
            let previous_day_key = previous_day.format("%Y-%m-%d").to_string();
            self.stats.daily_entry(&previous_day_key).and_then(|stats| {
                stats
                    .goal
                    .map(|goal| (goal, stats.focused_minutes(), stats.pomodoros_completed))
            })
        });
        carry_over_goal_target(base, self.goal_carry_over.daily, previous)
    }

    pub(super) fn effective_weekly_goal_snapshot_for_day(
        &self,
        day: NaiveDate,
    ) -> DailyGoalSnapshot {
        let base = self
            .stats
            .weekly_goal_snapshot_for_day(day)
            .unwrap_or_else(|| self.current_week_goal_snapshot());
        let previous =
            day.checked_sub_signed(chrono::Duration::weeks(1))
                .and_then(|previous_week_day| {
                    self.stats
                        .weekly_goal_snapshot_for_day(previous_week_day)
                        .map(|previous_target| {
                            let week = self.stats.weekly_for_day(previous_week_day);
                            (
                                previous_target,
                                week.focused_minutes(),
                                week.pomodoros_completed,
                            )
                        })
                });
        carry_over_goal_target(base, self.goal_carry_over.weekly, previous)
    }

    pub(super) fn effective_monthly_goal_snapshot_for_day(
        &self,
        day: NaiveDate,
    ) -> DailyGoalSnapshot {
        let base = self
            .stats
            .monthly_goal_snapshot_for_day(day)
            .unwrap_or_else(|| self.current_month_goal_snapshot());
        let previous = previous_month_reference_day(day).and_then(|previous_month_day| {
            self.stats
                .monthly_goal_snapshot_for_day(previous_month_day)
                .map(|previous_target| {
                    let month = self.stats.monthly_for_day(previous_month_day);
                    (
                        previous_target,
                        month.focused_minutes(),
                        month.pomodoros_completed,
                    )
                })
        });
        carry_over_goal_target(base, self.goal_carry_over.monthly, previous)
    }

    pub(crate) fn weekly_daily_goal_allocation(&self) -> WeeklyDailyGoalAllocation {
        let today = Local::now().date_naive();
        self.weekly_daily_goal_allocation_for_day(today)
    }

    pub(crate) fn weekly_daily_goal_allocation_for_day(
        &self,
        day: NaiveDate,
    ) -> WeeklyDailyGoalAllocation {
        let weekly_target = self.effective_weekly_goal_snapshot_for_day(day);
        let weekly_stats = self.stats.weekly_for_day(day);
        weekly_daily_goal_allocation_for_context(
            day,
            weekly_target,
            weekly_stats,
            &self.recurring_schedule,
        )
    }

    pub(super) fn sync_today_goal_snapshot(&mut self) {
        self.sync_goal_snapshot_for_day(Local::now().date_naive());
    }

    pub(super) fn sync_goal_snapshot_for_day(&mut self, day: NaiveDate) {
        let day_key = day.format("%Y-%m-%d").to_string();
        let daily_changed = self
            .stats
            .sync_goal_snapshot(&day_key, self.current_goal_snapshot());
        let weekly_changed = self
            .stats
            .sync_weekly_goal_snapshot(day, self.current_week_goal_snapshot());
        let monthly_changed = self
            .stats
            .sync_monthly_goal_snapshot(day, self.current_month_goal_snapshot());
        if daily_changed || weekly_changed || monthly_changed {
            self.mark_stats_dirty();
            self.flush_stats_if_dirty(false);
        }
    }

    #[allow(dead_code)]
    pub(crate) fn recent_break_glass_overrides(
        &self,
        limit: usize,
    ) -> Vec<BreakGlassOverrideEvent> {
        self.stats.recent_break_glass_overrides(limit)
    }

    #[cfg(test)]
    pub(crate) fn recent_session_interruptions(
        &self,
        limit: usize,
    ) -> Vec<SessionInterruptionEvent> {
        self.stats.recent_session_interruptions(limit)
    }

    #[allow(dead_code)]
    pub(crate) fn latest_session_interruption(&self) -> Option<SessionInterruptionEvent> {
        self.stats.latest_session_interruption()
    }
}

pub(crate) fn weekly_daily_goal_allocation_for_context(
    day: NaiveDate,
    weekly_target: DailyGoalSnapshot,
    weekly_stats: WeeklyStats,
    schedule: &RecurringScheduleConfig,
) -> WeeklyDailyGoalAllocation {
    let remaining_days = remaining_days_in_current_iso_week(day);
    let remaining_days_in_week = remaining_days.len();

    let completed_minutes = weekly_stats.focused_minutes();
    let completed_pomodoros = weekly_stats.pomodoros_completed;
    let remaining_minutes = weekly_target.minutes.saturating_sub(completed_minutes);
    let remaining_pomodoros = weekly_target.pomodoros.saturating_sub(completed_pomodoros);

    let mut weights = schedule_weights_for_days(&remaining_days, schedule);
    let mut allocatable_days = weights.iter().filter(|weight| **weight > 0).count();
    let mut uses_schedule_weights = allocatable_days > 0;
    if allocatable_days == 0 {
        weights = vec![1; remaining_days.len()];
        allocatable_days = remaining_days.len();
        uses_schedule_weights = false;
    }

    let minute_targets = distribute_weighted_u64(remaining_minutes, &weights);
    let pomodoro_targets = distribute_weighted_u64(u64::from(remaining_pomodoros), &weights);

    let daily_targets = remaining_days
        .iter()
        .enumerate()
        .map(|(index, target_day)| WeeklyDailyAllocationDay {
            day: *target_day,
            minutes_target: minute_targets.get(index).copied().unwrap_or(0),
            pomodoros_target: pomodoro_targets
                .get(index)
                .copied()
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or(0),
            allocatable: weights.get(index).copied().unwrap_or(0) > 0,
            weight_minutes: weights.get(index).copied().unwrap_or(0),
        })
        .collect();

    WeeklyDailyGoalAllocation {
        week_target: weekly_target,
        completed_minutes,
        completed_pomodoros,
        remaining_minutes,
        remaining_pomodoros,
        remaining_days_in_week,
        allocatable_days,
        uses_schedule_weights,
        daily_targets,
    }
}

fn remaining_days_in_current_iso_week(day: NaiveDate) -> Vec<NaiveDate> {
    let days_until_week_end = 6_u32.saturating_sub(day.weekday().num_days_from_monday());
    (0..=days_until_week_end)
        .map(|offset| day + Duration::days(i64::from(offset)))
        .collect()
}

fn schedule_weights_for_days(days: &[NaiveDate], schedule: &RecurringScheduleConfig) -> Vec<u64> {
    days.iter()
        .map(|day| scheduled_weight_minutes_for_day(*day, schedule))
        .collect()
}

fn scheduled_weight_minutes_for_day(day: NaiveDate, schedule: &RecurringScheduleConfig) -> u64 {
    schedule
        .windows
        .iter()
        .filter(|window| {
            window
                .days
                .iter()
                .any(|token| weekday_token_matches(token, day.weekday()))
        })
        .map(|window| window_duration_minutes(&window.start, &window.end))
        .sum()
}

fn weekday_token_matches(token: &str, weekday: Weekday) -> bool {
    let expected = match weekday {
        Weekday::Mon => "mon",
        Weekday::Tue => "tue",
        Weekday::Wed => "wed",
        Weekday::Thu => "thu",
        Weekday::Fri => "fri",
        Weekday::Sat => "sat",
        Weekday::Sun => "sun",
    };
    token.eq_ignore_ascii_case(expected)
}

fn parse_schedule_time_minutes(value: &str) -> Option<u16> {
    let (hour, minute) = value.trim().split_once(':')?;
    let hour = hour.parse::<u16>().ok()?;
    let minute = minute.parse::<u16>().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some(hour.saturating_mul(60).saturating_add(minute))
}

fn window_duration_minutes(start: &str, end: &str) -> u64 {
    let Some(start_minutes) = parse_schedule_time_minutes(start) else {
        return 0;
    };
    let Some(end_minutes) = parse_schedule_time_minutes(end) else {
        return 0;
    };
    if end_minutes <= start_minutes {
        return 0;
    }
    u64::from(end_minutes.saturating_sub(start_minutes))
}

fn distribute_weighted_u64(total: u64, weights: &[u64]) -> Vec<u64> {
    if total == 0 || weights.is_empty() {
        return vec![0; weights.len()];
    }
    let total_weight: u128 = weights.iter().map(|weight| u128::from(*weight)).sum();
    if total_weight == 0 {
        return vec![0; weights.len()];
    }

    let mut allocations = vec![0_u64; weights.len()];
    let mut fractional_remainders = Vec::with_capacity(weights.len());
    let mut allocated_sum = 0_u64;

    for (index, weight) in weights.iter().enumerate() {
        let scaled = u128::from(total).saturating_mul(u128::from(*weight));
        let whole = (scaled / total_weight) as u64;
        allocations[index] = whole;
        allocated_sum = allocated_sum.saturating_add(whole);
        fractional_remainders.push((index, scaled % total_weight));
    }

    let remaining_units = total.saturating_sub(allocated_sum);
    if remaining_units == 0 {
        return allocations;
    }

    fractional_remainders
        .sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    let distribute_count = remaining_units.min(fractional_remainders.len() as u64) as usize;
    for (index, _) in fractional_remainders.into_iter().take(distribute_count) {
        allocations[index] = allocations[index].saturating_add(1);
    }

    allocations
}
