use crate::stats::{
    BreakGlassOverrideExportRow, CSV_EXPORT_FILE_NAME, CsvExportRow, DailyExportRow,
    EXPORT_SCHEMA_VERSION, ExportedStatsFiles, FocusScoreExportRow, FocusStats,
    JSON_EXPORT_FILE_NAME, Path, ProfileEffectivenessExportRow, SessionExportRow,
    SessionInterruptionExportRow, StatsExport, TaskTotalsExportRow, TaskTrendExportRow,
    WeeklyConsistencyExportRow, WeeklyExportRow, format_week_label, fs, io, write_atomic_bytes,
};

impl FocusStats {
    pub fn export_to_dir(&self, dir: &Path) -> io::Result<ExportedStatsFiles> {
        fs::create_dir_all(dir)?;
        let export = self.export_data();
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

    pub(super) fn export_data(&self) -> StatsExport {
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
                focus_intention: session.focus_intention.clone(),
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
                focus_intention: event.focus_intention.clone(),
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
}

impl StatsExport {
    fn to_csv_bytes(&self) -> io::Result<Vec<u8>> {
        let mut writer = csv::Writer::from_writer(Vec::new());

        for row in self.csv_rows() {
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
            focus_intention: None,
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
        }
    }

    fn csv_rows(&self) -> Vec<CsvExportRow> {
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
                + self.profile_effectiveness.len(),
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
                focus_intention: Some(session.focus_intention.clone()),
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
                focus_intention: interruption.focus_intention.clone(),
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

        rows
    }
}
