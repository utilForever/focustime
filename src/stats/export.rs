use chrono::{Datelike, NaiveDate};

use crate::stats::{
    ComparisonDimension, DailyExportRow, DailyGoalSnapshot, EXPORT_SCHEMA_VERSION,
    ExportedStatsFiles, FocusRiskForecast, FocusScoreExportRow, FocusStats,
    HistoryKpiComparisonFilters, HistoryKpiExport, HistoryKpiExportContext, HistoryKpiFocusRisk,
    HistoryKpiFocusScore, HistoryKpiGoalPeriodProgress, HistoryKpiGoalStreak,
    HistoryKpiLastInterruption, HistoryKpiRetention, HistoryKpiSessionSummary,
    HistoryKpiStatsGrowth, JSON_EXPORT_FILE_NAME, Path, ProductivityComparisonExportRow,
    ProductivityComparisonFilter, ProfileEffectivenessExportRow, SessionExportRow,
    SessionInterruptionExportRow, StatsExport, TaskTotalsExportRow, TaskTrendExportRow,
    WeeklyConsistencyExportRow, WeeklyExportRow, WeeklyFocusScore, average_two_percentages,
    consistency_score_from_active_days, format_week_label, fs, io, weekly_completion_score_pct,
    write_atomic_bytes,
};

impl FocusStats {
    #[allow(dead_code)]
    pub(crate) fn export_to_dir(&self, dir: &Path) -> io::Result<ExportedStatsFiles> {
        self.export_to_dir_with_context(dir, &HistoryKpiExportContext::default())
    }

    pub(crate) fn export_to_dir_with_context(
        &self,
        dir: &Path,
        context: &HistoryKpiExportContext,
    ) -> io::Result<ExportedStatsFiles> {
        fs::create_dir_all(dir)?;
        let export = self.export_data_with_context(context);
        let json_path = dir.join(JSON_EXPORT_FILE_NAME);

        let json_content = serde_json::to_vec_pretty(&export)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        write_atomic_bytes(&json_path, &json_content)?;

        Ok(ExportedStatsFiles { json_path })
    }

    #[allow(dead_code)]
    pub(super) fn export_data(&self) -> StatsExport {
        self.export_data_with_context(&HistoryKpiExportContext::default())
    }

    pub(super) fn export_data_with_context(
        &self,
        context: &HistoryKpiExportContext,
    ) -> StatsExport {
        StatsExport {
            schema_version: EXPORT_SCHEMA_VERSION,
            daily: self.export_daily_rows(),
            weekly: self.export_weekly_rows(),
            sessions: self.export_session_rows(),
            interruptions: self.export_session_interruption_rows(),
            task_totals: self.export_task_totals_rows(),
            task_trends: self.export_task_trend_rows(),
            weekly_consistency: self.export_weekly_consistency_rows(),
            focus_scores: self.export_focus_score_rows(),
            profile_effectiveness: self.export_profile_effectiveness_rows(),
            productivity_comparisons: self.export_productivity_comparison_rows(),
            history_kpis: self.export_history_kpis(context),
        }
    }

    fn export_history_kpis(&self, context: &HistoryKpiExportContext) -> HistoryKpiExport {
        let day = context.reference_day;
        let day_key = day.format("%Y-%m-%d").to_string();
        let today_stats = self.daily_for(&day_key);
        let session_stats = self.session();

        let daily_goal = self.effective_daily_goal_snapshot_for_day(day, context);

        let daily_progress = goal_period_progress(
            today_stats.focused_minutes(),
            today_stats.pomodoros_completed,
            daily_goal,
        );

        let goal_streak =
            self.goal_streak_with_day_goal(day, daily_goal, today_stats, |target_day| {
                self.effective_daily_goal_snapshot_for_day(target_day, context)
            });
        let focus_score = self.focus_score_for_day(day, context);
        let focus_risk = self.focus_risk_forecast_for_day(
            day,
            daily_goal,
            DailyGoalSnapshot::default(),
            DailyGoalSnapshot::default(),
        );
        let (highest_signal_scope, highest_signal_label, highest_signal_value) =
            focus_risk_highest_signal(&focus_risk);

        let latest_interruption = self.latest_session_interruption();
        let growth_summary = self.growth_summary();
        let retention_preview = self.retention_preview(context.stats_retention, day);

        HistoryKpiExport {
            session_summary: HistoryKpiSessionSummary {
                session_pomodoros_completed: session_stats.pomodoros_completed,
                session_focused_minutes: session_stats.focused_minutes(),
                today_pomodoros_completed: today_stats.pomodoros_completed,
                today_focused_minutes: today_stats.focused_minutes(),
            },
            focus_score: HistoryKpiFocusScore {
                week_label: focus_score.as_ref().map(|score| score.week_label.clone()),
                active_days: focus_score.as_ref().map(|score| score.active_days),
                consistency_score_pct: focus_score
                    .as_ref()
                    .map(|score| score.consistency_score_pct),
                completion_score_pct: focus_score
                    .as_ref()
                    .and_then(|score| score.completion_score_pct),
                focus_score_pct: focus_score.as_ref().and_then(|score| score.focus_score_pct),
            },
            goal_streak: HistoryKpiGoalStreak {
                daily: daily_progress,
                current_days: goal_streak.current,
                best_days: goal_streak.best,
            },
            focus_risk: HistoryKpiFocusRisk {
                alert_active: focus_risk.alert_active(),
                highest_risk_level: focus_risk.highest_risk_level(),
                highest_signal_scope,
                highest_signal_label,
                highest_signal_value,
                daily_risk_level: focus_risk.daily_goal.risk_level,
                daily_risk_score_pct: focus_risk.daily_goal.risk_score_pct,
                streak_risk_level: focus_risk.streak.risk_level,
                streak_risk_score_pct: focus_risk.streak.risk_score_pct,
            },
            last_interruption: HistoryKpiLastInterruption {
                timestamp_epoch_secs: latest_interruption
                    .as_ref()
                    .map(|event| event.timestamp_epoch_secs),
                reason: latest_interruption.as_ref().map(|event| event.reason),
                task_label: latest_interruption
                    .as_ref()
                    .and_then(|event| event.task_label.clone()),
                remaining_secs: latest_interruption
                    .as_ref()
                    .map(|event| event.remaining_secs),
                profile_name: latest_interruption
                    .as_ref()
                    .and_then(|event| event.profile.map(|profile| profile.label().to_string())),
            },
            stats_growth: HistoryKpiStatsGrowth {
                total_record_count: growth_summary.total_record_count,
                estimated_bytes: growth_summary.estimated_bytes,
                sections: growth_summary.sections.clone(),
                high_volume_sections: growth_summary.high_volume_sections.clone(),
            },
            retention: HistoryKpiRetention {
                preset_id: context.stats_retention.preset.id().to_string(),
                preview: retention_preview,
                pending_prune: retention_preview.any_removed(),
            },
            comparison_filters: HistoryKpiComparisonFilters {
                dimension: context.comparison_dimension,
                task_filter: context.comparison_task_filter.clone(),
                profile_filter: context.comparison_profile_filter,
                time_of_day_filter: context.comparison_time_of_day_filter,
                summary: comparison_filter_summary(context),
            },
        }
    }

    fn export_daily_rows(&self) -> Vec<DailyExportRow> {
        self.daily
            .iter()
            .map(|(date, stats)| {
                let goal_met = stats.goal.is_some_and(|goal| goal.is_met_by(*stats));
                DailyExportRow {
                    date: date.clone(),
                    pomodoros_completed: stats.pomodoros_completed,
                    focused_seconds: stats.focused_seconds,
                    focused_minutes: stats.focused_minutes(),
                    goal: stats.goal,
                    goal_met,
                }
            })
            .collect()
    }

    fn export_weekly_rows(&self) -> Vec<WeeklyExportRow> {
        self.weekly_stats()
            .into_iter()
            .map(|stats| WeeklyExportRow {
                year: stats.year,
                week: stats.week,
                week_label: format_week_label(stats.year, stats.week),
                pomodoros_completed: stats.pomodoros_completed,
                focused_seconds: stats.focused_seconds,
                focused_minutes: stats.focused_minutes(),
            })
            .collect()
    }

    fn export_session_rows(&self) -> Vec<SessionExportRow> {
        self.focus_sessions
            .iter()
            .map(|session| SessionExportRow {
                date: session.date.clone(),
                task_label: session.task_label.clone(),
                focused_seconds: session.focused_seconds,
                focused_minutes: session.focused_seconds / 60,
                profile: session.profile,
            })
            .collect()
    }

    fn export_session_interruption_rows(&self) -> Vec<SessionInterruptionExportRow> {
        self.session_interruptions
            .iter()
            .map(|event| SessionInterruptionExportRow {
                timestamp_epoch_secs: event.timestamp_epoch_secs,
                date: event.date.clone(),
                reason: event.reason,
                task_label: event.task_label.clone(),
                remaining_secs: event.remaining_secs,
                profile: event.profile,
            })
            .collect()
    }

    fn export_task_totals_rows(&self) -> Vec<TaskTotalsExportRow> {
        self.task_totals(usize::MAX)
            .into_iter()
            .map(|totals| {
                let focused_minutes = totals.focused_minutes();
                TaskTotalsExportRow {
                    task_label: totals.task_label,
                    pomodoros_completed: totals.pomodoros_completed,
                    focused_seconds: totals.focused_seconds,
                    focused_minutes,
                }
            })
            .collect()
    }

    fn export_task_trend_rows(&self) -> Vec<TaskTrendExportRow> {
        let Some(window) = self.task_trend_window() else {
            return Vec::new();
        };

        let recent_window_start = window.recent_start.format("%Y-%m-%d").to_string();
        let recent_window_end = window.recent_end.format("%Y-%m-%d").to_string();
        let previous_window_start = window.previous_start.format("%Y-%m-%d").to_string();
        let previous_window_end = window.previous_end.format("%Y-%m-%d").to_string();

        self.task_trends_for_window(window)
            .into_iter()
            .map(|trend| {
                let recent_focused_minutes = trend.recent_focused_minutes();
                let previous_focused_minutes = trend.previous_focused_minutes();
                let delta_focused_seconds = trend.delta_focused_seconds();
                let delta_focused_minutes = trend.delta_focused_minutes();
                TaskTrendExportRow {
                    task_label: trend.task_label,
                    recent_window_start: recent_window_start.clone(),
                    recent_window_end: recent_window_end.clone(),
                    previous_window_start: previous_window_start.clone(),
                    previous_window_end: previous_window_end.clone(),
                    recent_pomodoros_completed: trend.recent_pomodoros_completed,
                    recent_focused_seconds: trend.recent_focused_seconds,
                    recent_focused_minutes,
                    previous_pomodoros_completed: trend.previous_pomodoros_completed,
                    previous_focused_seconds: trend.previous_focused_seconds,
                    previous_focused_minutes,
                    delta_focused_seconds,
                    delta_focused_minutes,
                }
            })
            .collect()
    }

    fn export_weekly_consistency_rows(&self) -> Vec<WeeklyConsistencyExportRow> {
        self.weekly_consistency_stats()
            .into_iter()
            .map(|entry| WeeklyConsistencyExportRow {
                year: entry.year,
                week: entry.week,
                week_label: entry.week_label,
                active_days: entry.active_days,
                consistency_score_pct: entry.consistency_score_pct,
            })
            .collect()
    }

    fn export_focus_score_rows(&self) -> Vec<FocusScoreExportRow> {
        self.weekly_focus_score_stats()
            .into_iter()
            .map(|entry| FocusScoreExportRow {
                year: entry.year,
                week: entry.week,
                week_label: entry.week_label,
                active_days: entry.active_days,
                consistency_score_pct: entry.consistency_score_pct,
                completion_score_pct: entry.completion_score_pct,
                focus_score_pct: entry.focus_score_pct,
            })
            .collect()
    }

    fn export_profile_effectiveness_rows(&self) -> Vec<ProfileEffectivenessExportRow> {
        self.profile_effectiveness()
            .into_iter()
            .map(|entry| ProfileEffectivenessExportRow {
                profile: entry.profile.label().to_string(),
                sessions_completed: entry.sessions_completed,
                active_days: entry.active_days,
                focused_seconds: entry.focused_seconds,
                focused_minutes: entry.focused_minutes(),
                average_focused_minutes_per_session: entry.average_focused_minutes_per_session(),
                focus_share_pct: entry.focus_share_pct,
            })
            .collect()
    }

    fn export_productivity_comparison_rows(&self) -> Vec<ProductivityComparisonExportRow> {
        let filter = ProductivityComparisonFilter::default();
        [
            ComparisonDimension::TaskLabel,
            ComparisonDimension::Profile,
            ComparisonDimension::TimeOfDay,
        ]
        .into_iter()
        .flat_map(|dimension| {
            self.productivity_comparison(dimension, &filter, usize::MAX)
                .into_iter()
                .map(move |entry| {
                    let focused_minutes = entry.focused_minutes();
                    let average_focused_minutes_per_session =
                        entry.average_focused_minutes_per_session();
                    ProductivityComparisonExportRow {
                        dimension,
                        label: entry.label,
                        task_label: entry.task_label,
                        profile: entry.profile.map(|profile| profile.label().to_string()),
                        time_of_day: entry.time_of_day,
                        sessions_completed: entry.sessions_completed,
                        focused_seconds: entry.focused_seconds,
                        focused_minutes,
                        average_focused_minutes_per_session,
                        focus_share_pct: entry.focus_share_pct,
                    }
                })
        })
        .collect()
    }

    fn effective_daily_goal_snapshot_for_day(
        &self,
        day: NaiveDate,
        context: &HistoryKpiExportContext,
    ) -> DailyGoalSnapshot {
        let day_key = day.format("%Y-%m-%d").to_string();
        self.daily_entry(&day_key)
            .and_then(|stats| stats.goal)
            .unwrap_or(context.daily_goal)
    }

    fn effective_weekly_goal_snapshot_for_day(
        &self,
        day: NaiveDate,
        context: &HistoryKpiExportContext,
    ) -> DailyGoalSnapshot {
        self.weekly_goal_snapshot_for_day(day)
            .unwrap_or(context.weekly_goal)
    }

    fn focus_score_for_day(
        &self,
        day: NaiveDate,
        context: &HistoryKpiExportContext,
    ) -> Option<WeeklyFocusScore> {
        let iso_week = day.iso_week();
        let year = iso_week.year();
        let week = iso_week.week();
        let week_label = format_week_label(year, week);
        let consistency = self
            .weekly_consistency_stats()
            .into_iter()
            .find(|entry| entry.year == year && entry.week == week);
        let active_days = consistency
            .as_ref()
            .map(|entry| entry.active_days)
            .unwrap_or(0);
        let consistency_score_pct = consistency
            .as_ref()
            .map(|entry| entry.consistency_score_pct)
            .unwrap_or_else(|| consistency_score_from_active_days(active_days));
        let totals = self.weekly_for_day(day);
        let weekly_goal = self.effective_weekly_goal_snapshot_for_day(day, context);
        let completion_score_pct = weekly_completion_score_pct(weekly_goal, totals);
        let focus_score_pct = completion_score_pct
            .map(|completion| average_two_percentages(consistency_score_pct, completion));

        let has_activity = active_days > 0;
        let has_goal_context =
            weekly_goal.has_any_target() || self.weekly_goal_snapshot_for_day(day).is_some();
        if !has_activity && !has_goal_context {
            return None;
        }

        Some(WeeklyFocusScore {
            year,
            week,
            week_label,
            active_days,
            consistency_score_pct,
            completion_score_pct,
            focus_score_pct,
        })
    }
}

fn goal_period_progress(
    focused_minutes_completed: u64,
    pomodoros_completed: u32,
    target: DailyGoalSnapshot,
) -> HistoryKpiGoalPeriodProgress {
    let target_configured = target.has_any_target();
    let met = target.is_met_by_totals(focused_minutes_completed, pomodoros_completed);
    HistoryKpiGoalPeriodProgress {
        focused_minutes_completed,
        focused_minutes_target: target.minutes,
        pomodoros_completed,
        pomodoros_target: target.pomodoros,
        target_configured,
        met,
    }
}

fn comparison_filter_summary(context: &HistoryKpiExportContext) -> String {
    let task = context
        .comparison_task_filter
        .as_deref()
        .unwrap_or("all")
        .to_string();
    let profile = context
        .comparison_profile_filter
        .map(|value| value.label().to_string())
        .unwrap_or_else(|| "All".to_string());
    let time_of_day = context
        .comparison_time_of_day_filter
        .map(|value| value.label().to_string())
        .unwrap_or_else(|| "All".to_string());
    format!("Slices: task {task} · profile {profile} · time {time_of_day}")
}

fn focus_risk_highest_signal(
    forecast: &FocusRiskForecast,
) -> (Option<String>, Option<String>, Option<String>) {
    let mut highest_scope = "daily";
    let highest_score = forecast.daily_goal.risk_score_pct;
    let mut highest_signal = forecast.daily_goal.signals.first();
    if forecast.streak.risk_score_pct > highest_score {
        highest_scope = "streak";
        highest_signal = forecast.streak.signals.first();
    }

    if !forecast.alert_active() {
        return (None, None, None);
    }

    (
        Some(highest_scope.to_string()),
        highest_signal.map(|signal| signal.label.clone()),
        highest_signal.map(|signal| signal.value.clone()),
    )
}
