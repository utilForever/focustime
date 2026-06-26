use chrono::{Datelike, Duration, NaiveDate};

use crate::app::weekly_daily_goal_allocation_for_context;
use crate::config::HistoryKpiCardId;
use crate::stats::{
    BreakGlassOverrideExportRow, CSV_EXPORT_FILE_NAME, ComparisonDimension, CsvExportRow,
    DailyExportRow, DailyGoalSnapshot, EXPORT_SCHEMA_VERSION, ExportedStatsFiles,
    FocusRiskForecast, FocusScoreExportRow, FocusStats, HistoryKpiComparisonFilters,
    HistoryKpiExport, HistoryKpiExportContext, HistoryKpiFocusRisk, HistoryKpiFocusScore,
    HistoryKpiGoalPeriodProgress, HistoryKpiGoalStreak, HistoryKpiLastInterruption,
    HistoryKpiRetention, HistoryKpiSessionSummary, HistoryKpiStatsGrowth,
    HistoryKpiWeeklyAllocation, HistoryKpiWeeklyAllocationDay, JSON_EXPORT_FILE_NAME, Path,
    ProductivityComparisonExportRow, ProductivityComparisonFilter, ProfileEffectivenessExportRow,
    SessionExportRow, SessionInterruptionExportRow, StatsExport, TaskTotalsExportRow,
    TaskTrendExportRow, WeeklyConsistencyExportRow, WeeklyExportRow, WeeklyFocusScore,
    average_two_percentages, consistency_score_from_active_days, format_week_label, fs, io,
    weekly_completion_score_pct, write_atomic_bytes,
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
        let csv_path = dir.join(CSV_EXPORT_FILE_NAME);

        let json_content = serde_json::to_vec_pretty(&export)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let csv_content = export.to_csv_bytes()?;

        write_atomic_bytes(&json_path, &json_content)?;
        write_atomic_bytes(&csv_path, &csv_content)?;

        Ok(ExportedStatsFiles {
            json_path,
            csv_path,
        })
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
            overrides: self.export_break_glass_override_rows(),
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
        let weekly_stats = self.weekly_for_day(day);
        let monthly_stats = self.monthly_for_day(day);

        let daily_goal = self.effective_daily_goal_snapshot_for_day(day, context);
        let weekly_goal = self.effective_weekly_goal_snapshot_for_day(day, context);
        let monthly_goal = self.effective_monthly_goal_snapshot_for_day(day, context);

        let daily_progress = goal_period_progress(
            today_stats.focused_minutes(),
            today_stats.pomodoros_completed,
            daily_goal,
        );
        let weekly_progress = goal_period_progress(
            weekly_stats.focused_minutes(),
            weekly_stats.pomodoros_completed,
            weekly_goal,
        );
        let monthly_progress = goal_period_progress(
            monthly_stats.focused_minutes(),
            monthly_stats.pomodoros_completed,
            monthly_goal,
        );

        let goal_streak =
            self.goal_streak_with_day_goal(day, daily_goal, today_stats, |target_day| {
                self.effective_daily_goal_snapshot_for_day(target_day, context)
            });
        let focus_score = self.focus_score_for_day(day, context);
        let focus_risk =
            self.focus_risk_forecast_for_day(day, daily_goal, weekly_goal, monthly_goal);
        let (highest_signal_scope, highest_signal_label, highest_signal_value) =
            focus_risk_highest_signal(&focus_risk);

        let weekly_allocation = weekly_daily_goal_allocation_for_context(
            day,
            weekly_goal,
            weekly_stats,
            &context.recurring_schedule,
        );
        let today_target = weekly_allocation.today_target();
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
                weekly: weekly_progress,
                monthly: monthly_progress,
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
                weekly_risk_level: focus_risk.weekly_goal.risk_level,
                weekly_risk_score_pct: focus_risk.weekly_goal.risk_score_pct,
                monthly_risk_level: focus_risk.monthly_goal.risk_level,
                monthly_risk_score_pct: focus_risk.monthly_goal.risk_score_pct,
                streak_risk_level: focus_risk.streak.risk_level,
                streak_risk_score_pct: focus_risk.streak.risk_score_pct,
            },
            weekly_allocation: HistoryKpiWeeklyAllocation {
                week_target_minutes: weekly_allocation.week_target.minutes,
                week_target_pomodoros: weekly_allocation.week_target.pomodoros,
                completed_minutes: weekly_allocation.completed_minutes,
                completed_pomodoros: weekly_allocation.completed_pomodoros,
                remaining_minutes: weekly_allocation.remaining_minutes,
                remaining_pomodoros: weekly_allocation.remaining_pomodoros,
                remaining_days_in_week: weekly_allocation.remaining_days_in_week,
                allocatable_days: weekly_allocation.allocatable_days,
                uses_schedule_weights: weekly_allocation.uses_schedule_weights,
                today_target_minutes: today_target.minutes,
                today_target_pomodoros: today_target.pomodoros,
                daily_targets: weekly_allocation
                    .daily_targets
                    .into_iter()
                    .map(|entry| HistoryKpiWeeklyAllocationDay {
                        day: entry.day.format("%Y-%m-%d").to_string(),
                        minutes_target: entry.minutes_target,
                        pomodoros_target: entry.pomodoros_target,
                        allocatable: entry.allocatable,
                        weight_minutes: entry.weight_minutes,
                    })
                    .collect(),
            },
            last_interruption: HistoryKpiLastInterruption {
                timestamp_epoch_secs: latest_interruption
                    .as_ref()
                    .map(|event| event.timestamp_epoch_secs),
                reason: latest_interruption.as_ref().map(|event| event.reason),
                task_label: latest_interruption
                    .as_ref()
                    .and_then(|event| event.task_label.clone()),
                task_note: latest_interruption
                    .as_ref()
                    .and_then(|event| event.task_note.clone()),
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
                task_note: session.task_note.clone(),
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
                task_note: event.task_note.clone(),
                remaining_secs: event.remaining_secs,
                profile: event.profile,
            })
            .collect()
    }

    fn export_break_glass_override_rows(&self) -> Vec<BreakGlassOverrideExportRow> {
        self.break_glass_overrides
            .iter()
            .map(|event| BreakGlassOverrideExportRow {
                timestamp_epoch_secs: event.timestamp_epoch_secs,
                date: event.date.clone(),
                task_label: event.task_label.clone(),
                duration_seconds: event.duration_seconds,
                duration_minutes: event.duration_seconds / 60,
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
        let base = self
            .daily_entry(&day_key)
            .and_then(|stats| stats.goal)
            .unwrap_or(context.daily_goal);
        let previous = day.pred_opt().and_then(|previous_day| {
            let previous_day_key = previous_day.format("%Y-%m-%d").to_string();
            self.daily_entry(&previous_day_key).and_then(|stats| {
                stats
                    .goal
                    .map(|goal| (goal, stats.focused_minutes(), stats.pomodoros_completed))
            })
        });
        crate::stats::carry_over_goal_target(base, context.carry_over_daily, previous)
    }

    fn effective_weekly_goal_snapshot_for_day(
        &self,
        day: NaiveDate,
        context: &HistoryKpiExportContext,
    ) -> DailyGoalSnapshot {
        let base = self
            .weekly_goal_snapshot_for_day(day)
            .unwrap_or(context.weekly_goal);
        let previous = day
            .checked_sub_signed(Duration::weeks(1))
            .and_then(|previous_week_day| {
                self.weekly_goal_snapshot_for_day(previous_week_day)
                    .map(|previous_target| {
                        let week = self.weekly_for_day(previous_week_day);
                        (
                            previous_target,
                            week.focused_minutes(),
                            week.pomodoros_completed,
                        )
                    })
            });
        crate::stats::carry_over_goal_target(base, context.carry_over_weekly, previous)
    }

    fn effective_monthly_goal_snapshot_for_day(
        &self,
        day: NaiveDate,
        context: &HistoryKpiExportContext,
    ) -> DailyGoalSnapshot {
        let base = self
            .monthly_goal_snapshot_for_day(day)
            .unwrap_or(context.monthly_goal);
        let previous = previous_month_reference_day(day).and_then(|previous_month_day| {
            self.monthly_goal_snapshot_for_day(previous_month_day)
                .map(|previous_target| {
                    let month = self.monthly_for_day(previous_month_day);
                    (
                        previous_target,
                        month.focused_minutes(),
                        month.pomodoros_completed,
                    )
                })
        });
        crate::stats::carry_over_goal_target(base, context.carry_over_monthly, previous)
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

impl StatsExport {
    fn to_csv_bytes(&self) -> io::Result<Vec<u8>> {
        let mut writer = csv::Writer::from_writer(Vec::new());

        for row in self.csv_rows()? {
            writer
                .serialize(row)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        }

        writer.flush()?;
        writer
            .into_inner()
            .map_err(|e| io::Error::other(format!("csv export finalize failed: {e}")))
    }

    fn csv_row_defaults(record_type: &'static str) -> CsvExportRow {
        CsvExportRow {
            schema_version: EXPORT_SCHEMA_VERSION,
            record_type,
            date: None,
            week_label: None,
            year: None,
            week: None,
            pomodoros_completed: 0,
            focused_seconds: 0,
            focused_minutes: 0,
            goal_minutes: None,
            goal_pomodoros: None,
            goal_met: None,
            task_label: None,
            break_glass_timestamp_epoch_secs: None,
            break_glass_duration_seconds: None,
            interruption_timestamp_epoch_secs: None,
            interruption_reason: None,
            interruption_remaining_secs: None,
            task_note: None,
            recent_window_start: None,
            recent_window_end: None,
            previous_window_start: None,
            previous_window_end: None,
            previous_pomodoros_completed: None,
            previous_focused_seconds: None,
            previous_focused_minutes: None,
            delta_focused_seconds: None,
            delta_focused_minutes: None,
            profile_name: None,
            sessions_completed: None,
            active_days: None,
            consistency_score_pct: None,
            completion_score_pct: None,
            focus_score_pct: None,
            average_focused_minutes_per_session: None,
            focus_share_pct: None,
            comparison_dimension: None,
            comparison_label: None,
            time_of_day_bucket: None,
            kpi_card_id: None,
            kpi_payload_json: None,
        }
    }

    fn csv_rows(&self) -> io::Result<Vec<CsvExportRow>> {
        let mut rows = Vec::with_capacity(
            self.daily.len()
                + self.weekly.len()
                + self.sessions.len()
                + self.interruptions.len()
                + self.overrides.len()
                + self.task_totals.len()
                + self.task_trends.len()
                + self.weekly_consistency.len()
                + self.focus_scores.len()
                + self.profile_effectiveness.len()
                + self.productivity_comparisons.len()
                + 9,
        );

        for daily in &self.daily {
            rows.push(CsvExportRow {
                date: Some(daily.date.clone()),
                pomodoros_completed: daily.pomodoros_completed,
                focused_seconds: daily.focused_seconds,
                focused_minutes: daily.focused_minutes,
                goal_minutes: daily.goal.map(|goal| goal.minutes),
                goal_pomodoros: daily.goal.map(|goal| goal.pomodoros),
                goal_met: daily.goal.map(|_| daily.goal_met),
                ..Self::csv_row_defaults("daily")
            });
        }

        for weekly in &self.weekly {
            rows.push(CsvExportRow {
                week_label: Some(weekly.week_label.clone()),
                year: Some(weekly.year),
                week: Some(weekly.week),
                pomodoros_completed: weekly.pomodoros_completed,
                focused_seconds: weekly.focused_seconds,
                focused_minutes: weekly.focused_minutes,
                ..Self::csv_row_defaults("weekly")
            });
        }

        for session in &self.sessions {
            rows.push(CsvExportRow {
                date: Some(session.date.clone()),
                pomodoros_completed: 1,
                focused_seconds: session.focused_seconds,
                focused_minutes: session.focused_minutes,
                task_label: Some(session.task_label.clone()),
                task_note: Some(session.task_note.clone()),
                profile_name: session.profile.map(|profile| profile.label().to_string()),
                ..Self::csv_row_defaults("focus_session")
            });
        }

        for interruption in &self.interruptions {
            rows.push(CsvExportRow {
                date: Some(interruption.date.clone()),
                task_label: interruption.task_label.clone(),
                interruption_timestamp_epoch_secs: Some(interruption.timestamp_epoch_secs),
                interruption_reason: Some(interruption.reason),
                interruption_remaining_secs: Some(interruption.remaining_secs),
                task_note: interruption.task_note.clone(),
                profile_name: interruption
                    .profile
                    .map(|profile| profile.label().to_string()),
                ..Self::csv_row_defaults("session_interruption")
            });
        }

        for override_event in &self.overrides {
            rows.push(CsvExportRow {
                date: Some(override_event.date.clone()),
                task_label: override_event.task_label.clone(),
                break_glass_timestamp_epoch_secs: Some(override_event.timestamp_epoch_secs),
                break_glass_duration_seconds: Some(override_event.duration_seconds),
                ..Self::csv_row_defaults("break_glass_override")
            });
        }

        for task_total in &self.task_totals {
            rows.push(CsvExportRow {
                pomodoros_completed: task_total.pomodoros_completed,
                focused_seconds: task_total.focused_seconds,
                focused_minutes: task_total.focused_minutes,
                task_label: Some(task_total.task_label.clone()),
                ..Self::csv_row_defaults("task_summary")
            });
        }

        for task_trend in &self.task_trends {
            rows.push(CsvExportRow {
                pomodoros_completed: task_trend.recent_pomodoros_completed,
                focused_seconds: task_trend.recent_focused_seconds,
                focused_minutes: task_trend.recent_focused_minutes,
                task_label: Some(task_trend.task_label.clone()),
                recent_window_start: Some(task_trend.recent_window_start.clone()),
                recent_window_end: Some(task_trend.recent_window_end.clone()),
                previous_window_start: Some(task_trend.previous_window_start.clone()),
                previous_window_end: Some(task_trend.previous_window_end.clone()),
                previous_pomodoros_completed: Some(task_trend.previous_pomodoros_completed),
                previous_focused_seconds: Some(task_trend.previous_focused_seconds),
                previous_focused_minutes: Some(task_trend.previous_focused_minutes),
                delta_focused_seconds: Some(task_trend.delta_focused_seconds),
                delta_focused_minutes: Some(task_trend.delta_focused_minutes),
                ..Self::csv_row_defaults("task_trend")
            });
        }

        for consistency in &self.weekly_consistency {
            rows.push(CsvExportRow {
                week_label: Some(consistency.week_label.clone()),
                year: Some(consistency.year),
                week: Some(consistency.week),
                active_days: Some(u32::from(consistency.active_days)),
                consistency_score_pct: Some(consistency.consistency_score_pct),
                ..Self::csv_row_defaults("weekly_consistency")
            });
        }

        for focus_score in &self.focus_scores {
            rows.push(CsvExportRow {
                week_label: Some(focus_score.week_label.clone()),
                year: Some(focus_score.year),
                week: Some(focus_score.week),
                active_days: Some(u32::from(focus_score.active_days)),
                consistency_score_pct: Some(focus_score.consistency_score_pct),
                completion_score_pct: focus_score.completion_score_pct,
                focus_score_pct: focus_score.focus_score_pct,
                ..Self::csv_row_defaults("focus_score")
            });
        }

        for profile in &self.profile_effectiveness {
            rows.push(CsvExportRow {
                pomodoros_completed: profile.sessions_completed,
                focused_seconds: profile.focused_seconds,
                focused_minutes: profile.focused_minutes,
                profile_name: Some(profile.profile.clone()),
                sessions_completed: Some(profile.sessions_completed),
                active_days: Some(profile.active_days),
                average_focused_minutes_per_session: Some(
                    profile.average_focused_minutes_per_session,
                ),
                focus_share_pct: Some(profile.focus_share_pct),
                ..Self::csv_row_defaults("profile_effectiveness")
            });
        }

        for comparison in &self.productivity_comparisons {
            rows.push(CsvExportRow {
                pomodoros_completed: comparison.sessions_completed,
                focused_seconds: comparison.focused_seconds,
                focused_minutes: comparison.focused_minutes,
                task_label: comparison.task_label.clone(),
                profile_name: comparison.profile.clone(),
                sessions_completed: Some(comparison.sessions_completed),
                average_focused_minutes_per_session: Some(
                    comparison.average_focused_minutes_per_session,
                ),
                focus_share_pct: Some(comparison.focus_share_pct),
                comparison_dimension: Some(comparison.dimension.id().to_string()),
                comparison_label: Some(comparison.label.clone()),
                time_of_day_bucket: comparison.time_of_day.map(|bucket| bucket.id().to_string()),
                ..Self::csv_row_defaults("productivity_comparison")
            });
        }

        rows.extend(self.history_kpi_csv_rows()?);
        Ok(rows)
    }

    fn history_kpi_csv_rows(&self) -> io::Result<Vec<CsvExportRow>> {
        fn payload_json(value: &impl serde::Serialize) -> io::Result<String> {
            serde_json::to_string(value)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
        }
        Ok(vec![
            CsvExportRow {
                kpi_card_id: Some(HistoryKpiCardId::SessionSummary.id().to_string()),
                kpi_payload_json: Some(payload_json(&self.history_kpis.session_summary)?),
                ..Self::csv_row_defaults("history_kpi")
            },
            CsvExportRow {
                kpi_card_id: Some(HistoryKpiCardId::FocusScore.id().to_string()),
                kpi_payload_json: Some(payload_json(&self.history_kpis.focus_score)?),
                ..Self::csv_row_defaults("history_kpi")
            },
            CsvExportRow {
                kpi_card_id: Some(HistoryKpiCardId::GoalStreak.id().to_string()),
                kpi_payload_json: Some(payload_json(&self.history_kpis.goal_streak)?),
                ..Self::csv_row_defaults("history_kpi")
            },
            CsvExportRow {
                kpi_card_id: Some(HistoryKpiCardId::FocusRisk.id().to_string()),
                kpi_payload_json: Some(payload_json(&self.history_kpis.focus_risk)?),
                ..Self::csv_row_defaults("history_kpi")
            },
            CsvExportRow {
                kpi_card_id: Some(HistoryKpiCardId::WeeklyAllocation.id().to_string()),
                kpi_payload_json: Some(payload_json(&self.history_kpis.weekly_allocation)?),
                ..Self::csv_row_defaults("history_kpi")
            },
            CsvExportRow {
                kpi_card_id: Some(HistoryKpiCardId::LastInterruption.id().to_string()),
                kpi_payload_json: Some(payload_json(&self.history_kpis.last_interruption)?),
                ..Self::csv_row_defaults("history_kpi")
            },
            CsvExportRow {
                kpi_card_id: Some(HistoryKpiCardId::StatsGrowth.id().to_string()),
                kpi_payload_json: Some(payload_json(&self.history_kpis.stats_growth)?),
                ..Self::csv_row_defaults("history_kpi")
            },
            CsvExportRow {
                kpi_card_id: Some(HistoryKpiCardId::Retention.id().to_string()),
                kpi_payload_json: Some(payload_json(&self.history_kpis.retention)?),
                ..Self::csv_row_defaults("history_kpi")
            },
            CsvExportRow {
                kpi_card_id: Some(HistoryKpiCardId::ComparisonFilters.id().to_string()),
                kpi_payload_json: Some(payload_json(&self.history_kpis.comparison_filters)?),
                ..Self::csv_row_defaults("history_kpi")
            },
        ])
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

fn previous_month_reference_day(day: NaiveDate) -> Option<NaiveDate> {
    let first_of_month = day.with_day(1)?;
    let previous_month_last_day = first_of_month.checked_sub_signed(Duration::days(1))?;
    let reference_day = day.day().min(previous_month_last_day.day());
    previous_month_last_day.with_day(reference_day)
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
    let mut highest_score = forecast.daily_goal.risk_score_pct;
    let mut highest_signal = forecast.daily_goal.signals.first();
    if forecast.weekly_goal.risk_score_pct > highest_score {
        highest_scope = "weekly";
        highest_score = forecast.weekly_goal.risk_score_pct;
        highest_signal = forecast.weekly_goal.signals.first();
    }
    if forecast.monthly_goal.risk_score_pct > highest_score {
        highest_scope = "monthly";
        highest_score = forecast.monthly_goal.risk_score_pct;
        highest_signal = forecast.monthly_goal.signals.first();
    }
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
