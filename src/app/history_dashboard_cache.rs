use chrono::Local;

use crate::app::{
    App, HistoryDashboardComparisonSnapshot, HistoryDashboardComparisonSnapshotKey,
    HistoryDashboardStaticSnapshot, HistoryDashboardStaticSnapshotKey, HistoryDashboardViewData,
    parse_day_key,
};

impl App {
    pub fn history_dashboard_view_data(&self) -> HistoryDashboardViewData {
        let static_key = self.history_dashboard_static_snapshot_key();
        let comparison_key = self.history_dashboard_comparison_snapshot_key();
        let (rebuild_static, rebuild_comparison) = {
            let cache = self.history_dashboard_cache.borrow();
            (
                cache.static_key.as_ref() != Some(&static_key),
                cache.comparison_key.as_ref() != Some(&comparison_key),
            )
        };

        if rebuild_static {
            let static_snapshot = self.build_history_dashboard_static_snapshot(&static_key.day_key);
            let mut cache = self.history_dashboard_cache.borrow_mut();
            cache.static_key = Some(static_key.clone());
            cache.static_snapshot = Some(static_snapshot);
            #[cfg(test)]
            {
                cache.cache_stats.static_rebuilds =
                    cache.cache_stats.static_rebuilds.saturating_add(1);
            }
        }

        if rebuild_comparison {
            let comparison_snapshot = self.build_history_dashboard_comparison_snapshot();
            let mut cache = self.history_dashboard_cache.borrow_mut();
            cache.comparison_key = Some(comparison_key);
            cache.comparison_snapshot = Some(comparison_snapshot);
            #[cfg(test)]
            {
                cache.cache_stats.comparison_rebuilds =
                    cache.cache_stats.comparison_rebuilds.saturating_add(1);
            }
        }

        let cache = self.history_dashboard_cache.borrow();
        let static_snapshot = cache.static_snapshot.as_ref().cloned().unwrap_or_else(|| {
            self.build_history_dashboard_static_snapshot(&crate::stats::current_day_key())
        });
        let comparison_snapshot = cache
            .comparison_snapshot
            .as_ref()
            .cloned()
            .unwrap_or_else(|| self.build_history_dashboard_comparison_snapshot());

        HistoryDashboardViewData {
            session_stats: static_snapshot.session_stats,
            today_stats: static_snapshot.today_stats,
            daily_goal_progress: static_snapshot.daily_goal_progress,
            weekly_goal_progress: static_snapshot.weekly_goal_progress,
            monthly_goal_progress: static_snapshot.monthly_goal_progress,
            latest_weekly_focus_score: static_snapshot.latest_weekly_focus_score,
            goal_streak: static_snapshot.goal_streak,
            focus_risk_forecast: static_snapshot.focus_risk_forecast,
            weekly_daily_goal_allocation: static_snapshot.weekly_daily_goal_allocation,
            latest_session_interruption: static_snapshot.latest_session_interruption,
            stats_growth_summary: static_snapshot.stats_growth_summary,
            stats_retention_config: static_snapshot.stats_retention_config,
            stats_retention_preview: static_snapshot.stats_retention_preview,
            comparison_filter_summary: comparison_snapshot.comparison_filter_summary,
            comparison_rows: comparison_snapshot.comparison_rows,
            task_trends: static_snapshot.task_trends,
            profile_effectiveness: static_snapshot.profile_effectiveness,
            break_glass_overrides: static_snapshot.break_glass_overrides,
            monthly_stats: static_snapshot.monthly_stats,
            monthly_heatmap: static_snapshot.monthly_heatmap,
        }
    }

    pub(super) fn mark_stats_dirty(&mut self) {
        self.stats_dirty = true;
        self.stats_revision = self.stats_revision.saturating_add(1);
    }

    #[cfg(test)]
    pub fn history_dashboard_cache_stats(&self) -> crate::app::HistoryDashboardCacheStats {
        self.history_dashboard_cache.borrow().cache_stats
    }

    fn history_dashboard_static_snapshot_key(&self) -> HistoryDashboardStaticSnapshotKey {
        HistoryDashboardStaticSnapshotKey {
            stats_revision: self.stats_revision,
            day_key: crate::stats::current_day_key(),
            retention: self.stats_retention,
        }
    }

    fn history_dashboard_comparison_snapshot_key(&self) -> HistoryDashboardComparisonSnapshotKey {
        HistoryDashboardComparisonSnapshotKey {
            stats_revision: self.stats_revision,
            dimension: self.history_comparison_dimension,
            task_filter: self.history_task_filter.clone(),
            profile_filter: self.history_profile_filter,
            time_of_day_filter: self.history_time_of_day_filter,
        }
    }

    fn build_history_dashboard_static_snapshot(
        &self,
        day_key: &str,
    ) -> HistoryDashboardStaticSnapshot {
        let day = parse_day_key(day_key).unwrap_or_else(|| Local::now().date_naive());
        let canonical_day_key = day.format("%Y-%m-%d").to_string();
        let daily_goal = self.effective_daily_goal_snapshot_for_day(day);
        let weekly_goal = self.effective_weekly_goal_snapshot_for_day(day);
        let monthly_goal = self.effective_monthly_goal_snapshot_for_day(day);

        HistoryDashboardStaticSnapshot {
            session_stats: self.stats.session(),
            today_stats: self.stats.daily_for(&canonical_day_key),
            daily_goal_progress: self.today_goal_progress(),
            weekly_goal_progress: self.current_week_goal_progress(),
            monthly_goal_progress: self.current_month_goal_progress(),
            latest_weekly_focus_score: self.stats.latest_weekly_focus_score(),
            goal_streak: self.goal_streak_for_day_key(&canonical_day_key),
            focus_risk_forecast: self.stats.focus_risk_forecast_for_day(
                day,
                daily_goal,
                weekly_goal,
                monthly_goal,
            ),
            weekly_daily_goal_allocation: self.weekly_daily_goal_allocation_for_day(day),
            latest_session_interruption: self.stats.latest_session_interruption(),
            stats_growth_summary: self.stats.growth_summary(),
            stats_retention_config: self.stats_retention,
            stats_retention_preview: self.stats.retention_preview(self.stats_retention, day),
            task_trends: self.stats.recent_task_trends(6),
            profile_effectiveness: self.stats.profile_effectiveness(),
            break_glass_overrides: self.stats.recent_break_glass_overrides(6),
            monthly_stats: self.stats.recent_monthly(4),
            monthly_heatmap: self.stats.latest_monthly_heatmap(),
        }
    }

    fn build_history_dashboard_comparison_snapshot(&self) -> HistoryDashboardComparisonSnapshot {
        let filter = crate::stats::ProductivityComparisonFilter {
            task_label: self.history_task_filter.clone(),
            profile: self.history_profile_filter,
            time_of_day: self.history_time_of_day_filter,
        };
        HistoryDashboardComparisonSnapshot {
            comparison_filter_summary: self.history_comparison_filter_summary(),
            comparison_rows: self.stats.productivity_comparison(
                self.history_comparison_dimension,
                &filter,
                6,
            ),
        }
    }
}
