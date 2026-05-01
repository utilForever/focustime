use super::*;

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
            focus_intention: self
                .active_focus_intention
                .clone()
                .or_else(|| task_label.clone()),
            task_note: self.active_focus_task_note.clone().or(task_label),
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
                focus_intention: context.focus_intention.as_deref(),
                task_note: context.task_note.as_deref(),
            },
            context.remaining_secs,
            context.profile,
        );
        self.stats_dirty = true;
    }

    pub(super) fn open_stats_history(&mut self) {
        self.pending_timer_action = None;
        self.history_feedback = None;
        self.mode = AppMode::StatsHistory;
    }

    pub(super) fn record_completed_focus_session(&mut self, focused_seconds: u64) {
        let day_key = current_day_key();
        let goal = self.current_goal_snapshot();
        if let Some(active_task_label) = self.active_focus_task_label.clone() {
            let focus_intention = self
                .active_focus_intention
                .clone()
                .unwrap_or_else(|| active_task_label.clone());
            let task_note = self
                .active_focus_task_note
                .clone()
                .unwrap_or_else(|| active_task_label.clone());
            self.stats.record_completed_pomodoro_with_metadata(
                &day_key,
                goal,
                FocusSessionMetadata {
                    task_label: Some(active_task_label.as_str()),
                    focus_intention: Some(focus_intention.as_str()),
                    task_note: Some(task_note.as_str()),
                },
                focused_seconds,
                self.active_focus_profile,
            );
        } else {
            self.stats.record_completed_pomodoro(&day_key, goal);
        }
        self.stats_dirty = true;
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
            self.stats_dirty = true;
            self.flush_stats_if_dirty(false);
        }
    }

    pub fn recent_break_glass_overrides(&self, limit: usize) -> Vec<BreakGlassOverrideEvent> {
        self.stats.recent_break_glass_overrides(limit)
    }

    #[cfg(test)]
    pub fn recent_session_interruptions(&self, limit: usize) -> Vec<SessionInterruptionEvent> {
        self.stats.recent_session_interruptions(limit)
    }

    pub fn latest_session_interruption(&self) -> Option<SessionInterruptionEvent> {
        self.stats.latest_session_interruption()
    }
}
