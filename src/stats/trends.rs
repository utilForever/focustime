use crate::stats::{
    BTreeMap, BTreeSet, BreakGlassOverrideEvent, DailyGoalSnapshot, DailyStats, FocusStats,
    GoalStreak, SessionInterruptionEvent, TaskTotals, TaskTrend, TaskTrendAccumulator,
    TaskTrendWindow, best_goal_streak, canonical_task_label, current_goal_streak,
    normalize_task_label,
};

impl FocusStats {
    pub fn recent_task_labels(&self, limit: usize) -> Vec<String> {
        if limit == 0 {
            return Vec::new();
        }

        let mut recent = Vec::new();
        let mut seen = BTreeSet::new();
        for session in self.focus_sessions.iter().rev() {
            let Some(task_label) = normalize_task_label(&session.task_label) else {
                continue;
            };
            let key = task_label.to_ascii_lowercase();
            if !seen.insert(key) {
                continue;
            }
            recent.push(task_label);
            if recent.len() >= limit {
                break;
            }
        }
        recent
    }

    pub(super) fn task_totals_by_key(&self) -> BTreeMap<String, (u32, u64)> {
        let mut by_task: BTreeMap<String, (u32, u64)> = BTreeMap::new();
        for session in &self.focus_sessions {
            let Some(task_label) = normalize_task_label(&session.task_label) else {
                continue;
            };
            let key = task_label.to_ascii_lowercase();
            let entry = by_task.entry(key).or_insert((0, 0));
            entry.0 = entry.0.saturating_add(1);
            entry.1 = entry.1.saturating_add(session.focused_seconds);
        }
        by_task
    }

    pub fn recent_daily(&self, limit: usize) -> Vec<(String, DailyStats)> {
        self.daily
            .iter()
            .rev()
            .take(limit)
            .map(|(day, stats)| (day.clone(), *stats))
            .collect()
    }

    pub fn task_totals(&self, limit: usize) -> Vec<TaskTotals> {
        if limit == 0 {
            return Vec::new();
        }

        let mut by_task: BTreeMap<String, TaskTotals> = BTreeMap::new();
        for session in &self.focus_sessions {
            let Some(task_label) = normalize_task_label(&session.task_label) else {
                continue;
            };
            let task_label =
                canonical_task_label(&self.task_labels, &task_label).unwrap_or(task_label);
            let key = task_label.to_ascii_lowercase();
            let entry = by_task.entry(key).or_insert(TaskTotals {
                task_label,
                pomodoros_completed: 0,
                focused_seconds: 0,
            });
            entry.pomodoros_completed = entry.pomodoros_completed.saturating_add(1);
            entry.focused_seconds = entry
                .focused_seconds
                .saturating_add(session.focused_seconds);
        }

        let mut totals: Vec<TaskTotals> = by_task.into_values().collect();
        totals.sort_by(|left, right| {
            right
                .focused_seconds
                .cmp(&left.focused_seconds)
                .then_with(|| right.pomodoros_completed.cmp(&left.pomodoros_completed))
                .then_with(|| left.task_label.cmp(&right.task_label))
        });
        totals.truncate(limit);
        totals
    }

    pub fn recent_task_trends(&self, limit: usize) -> Vec<TaskTrend> {
        if limit == 0 {
            return Vec::new();
        }

        let Some(window) = self.task_trend_window() else {
            return Vec::new();
        };
        let mut trends = self.task_trends_for_window(window);
        trends.truncate(limit);
        trends
    }

    pub fn recent_break_glass_overrides(&self, limit: usize) -> Vec<BreakGlassOverrideEvent> {
        self.break_glass_overrides
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    #[cfg(test)]
    pub fn recent_session_interruptions(&self, limit: usize) -> Vec<SessionInterruptionEvent> {
        self.session_interruptions
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn latest_session_interruption(&self) -> Option<SessionInterruptionEvent> {
        self.session_interruptions
            .iter()
            .max_by_key(|event| event.timestamp_epoch_secs)
            .cloned()
    }

    pub(super) fn task_trend_window(&self) -> Option<TaskTrendWindow> {
        if self.focus_sessions.is_empty() {
            return None;
        }

        let recent_end = chrono::Local::now().date_naive();
        let recent_start = recent_end.checked_sub_signed(chrono::Duration::days(6))?;
        let previous_end = recent_start.checked_sub_signed(chrono::Duration::days(1))?;
        let previous_start = previous_end.checked_sub_signed(chrono::Duration::days(6))?;

        Some(TaskTrendWindow {
            recent_start,
            recent_end,
            previous_start,
            previous_end,
        })
    }

    pub(super) fn task_trends_for_window(&self, window: TaskTrendWindow) -> Vec<TaskTrend> {
        let mut by_task: BTreeMap<String, (String, TaskTrendAccumulator)> = BTreeMap::new();

        for session in &self.focus_sessions {
            let Ok(day) = chrono::NaiveDate::parse_from_str(&session.date, "%Y-%m-%d") else {
                continue;
            };
            if day < window.previous_start || day > window.recent_end {
                continue;
            }

            let Some(task_label) = normalize_task_label(&session.task_label) else {
                continue;
            };
            let task_label =
                canonical_task_label(&self.task_labels, &task_label).unwrap_or(task_label);
            let key = task_label.to_ascii_lowercase();
            let (_, entry) = by_task
                .entry(key)
                .or_insert_with(|| (task_label, TaskTrendAccumulator::default()));
            if day >= window.recent_start {
                entry.recent_pomodoros_completed =
                    entry.recent_pomodoros_completed.saturating_add(1);
                entry.recent_focused_seconds = entry
                    .recent_focused_seconds
                    .saturating_add(session.focused_seconds);
            } else if day <= window.previous_end {
                entry.previous_pomodoros_completed =
                    entry.previous_pomodoros_completed.saturating_add(1);
                entry.previous_focused_seconds = entry
                    .previous_focused_seconds
                    .saturating_add(session.focused_seconds);
            }
        }

        let mut trends: Vec<TaskTrend> = by_task
            .into_iter()
            .filter_map(|(_, (task_label, totals))| {
                let has_data = totals.recent_pomodoros_completed > 0
                    || totals.previous_pomodoros_completed > 0
                    || totals.recent_focused_seconds > 0
                    || totals.previous_focused_seconds > 0;
                has_data.then_some(TaskTrend {
                    task_label,
                    recent_pomodoros_completed: totals.recent_pomodoros_completed,
                    recent_focused_seconds: totals.recent_focused_seconds,
                    previous_pomodoros_completed: totals.previous_pomodoros_completed,
                    previous_focused_seconds: totals.previous_focused_seconds,
                })
            })
            .collect();

        trends.sort_by(|left, right| {
            right
                .recent_focused_seconds
                .cmp(&left.recent_focused_seconds)
                .then_with(|| {
                    right
                        .previous_focused_seconds
                        .cmp(&left.previous_focused_seconds)
                })
                .then_with(|| {
                    right
                        .recent_pomodoros_completed
                        .cmp(&left.recent_pomodoros_completed)
                })
                .then_with(|| left.task_label.cmp(&right.task_label))
        });
        trends
    }

    #[cfg(test)]
    pub fn insert_daily_for_tests(&mut self, day_key: &str, stats: DailyStats) {
        self.daily.insert(day_key.to_string(), stats);
    }

    #[cfg(test)]
    pub fn goal_streak(
        &self,
        today: chrono::NaiveDate,
        current_goal: DailyGoalSnapshot,
        today_stats: DailyStats,
    ) -> GoalStreak {
        self.goal_streak_with_day_goal(today, current_goal, today_stats, |_| current_goal)
    }

    pub(crate) fn goal_streak_with_day_goal<F>(
        &self,
        today: chrono::NaiveDate,
        current_goal: DailyGoalSnapshot,
        today_stats: DailyStats,
        mut goal_for_day: F,
    ) -> GoalStreak
    where
        F: FnMut(chrono::NaiveDate) -> DailyGoalSnapshot,
    {
        let completed_days =
            self.completed_goal_days(today, current_goal, today_stats, &mut goal_for_day);
        GoalStreak {
            current: current_goal_streak(&completed_days, today, current_goal, today_stats),
            best: best_goal_streak(&completed_days),
        }
    }

    fn completed_goal_days<F>(
        &self,
        today: chrono::NaiveDate,
        current_goal: DailyGoalSnapshot,
        today_stats: DailyStats,
        goal_for_day: &mut F,
    ) -> BTreeSet<chrono::NaiveDate>
    where
        F: FnMut(chrono::NaiveDate) -> DailyGoalSnapshot,
    {
        let mut completed_days = BTreeSet::new();

        for (day_key, stats) in &self.daily {
            let Ok(day) = chrono::NaiveDate::parse_from_str(day_key, "%Y-%m-%d") else {
                continue;
            };
            if day == today {
                continue;
            }

            let snapshot = stats.goal.or_else(|| {
                let fallback = goal_for_day(day);
                fallback.has_any_target().then_some(fallback)
            });
            if snapshot.is_some_and(|goal| goal.is_met_by(*stats)) {
                completed_days.insert(day);
            }
        }

        if current_goal.is_met_by(today_stats) {
            completed_days.insert(today);
        }

        completed_days
    }
}
