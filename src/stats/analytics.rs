mod forecast;
mod support;

use crate::stats::{
    BTreeMap, BTreeSet, ComparisonDimension, DailyGoalSnapshot, Datelike,
    FocusRiskCalibrationMetrics, FocusRiskForecast, FocusSessionRecord, FocusStats, GoalPeriod,
    HeatmapDayStats, MonthlyHeatmap, MonthlyStats, ProductivityComparisonFilter,
    ProductivityComparisonRow, ProfileBucket, ProfileEffectiveness,
    ProfileEffectivenessAccumulator, ProfileTotals, StatsGrowthSection, StatsGrowthSummary,
    StatsRetentionConfig, StatsRetentionPruneResult, TimeOfDayBucket, UsageSignalsSummary,
    WeeklyConsistency, WeeklyFocusScore, WeeklyStats, average_two_percentages,
    backfilled_time_of_day_bucket, canonical_task_label, consistency_score_from_active_days,
    daily_has_activity, days_in_month, format_week_label, month_key_for_day, normalize_task_label,
    parse_week_label, percentage_round_nearest, profile_bucket_for, week_key_for_day,
    weekly_completion_score_pct,
};
use forecast::{
    classify_calibration_signal, goal_risk_forecast, observed_goal_miss_for_candidate,
    rolling_cadence_window, streak_risk_forecast,
};
use support::{
    estimated_serialized_bytes, is_day_key_on_or_after, is_month_key_on_or_after,
    is_week_key_on_or_after, retention_cutoff_day, stats_growth_section,
    usage_signal_summary_for_counts,
};

impl FocusStats {
    pub(crate) fn weekly_for_day(&self, day: chrono::NaiveDate) -> WeeklyStats {
        let week = day.iso_week();
        let mut totals = WeeklyStats {
            year: week.year(),
            week: week.week(),
            ..WeeklyStats::default()
        };
        for (day_key, stats) in &self.daily {
            let Ok(candidate) = chrono::NaiveDate::parse_from_str(day_key, "%Y-%m-%d") else {
                continue;
            };
            let candidate_week = candidate.iso_week();
            if candidate_week.year() != totals.year || candidate_week.week() != totals.week {
                continue;
            }
            totals.pomodoros_completed = totals
                .pomodoros_completed
                .saturating_add(stats.pomodoros_completed);
            totals.focused_seconds = totals.focused_seconds.saturating_add(stats.focused_seconds);
        }
        totals
    }

    pub(crate) fn monthly_for_day(&self, day: chrono::NaiveDate) -> MonthlyStats {
        let mut totals = MonthlyStats {
            year: day.year(),
            month: day.month(),
            ..MonthlyStats::default()
        };
        for (day_key, stats) in &self.daily {
            let Ok(candidate) = chrono::NaiveDate::parse_from_str(day_key, "%Y-%m-%d") else {
                continue;
            };
            if candidate.year() != totals.year || candidate.month() != totals.month {
                continue;
            }
            totals.pomodoros_completed = totals
                .pomodoros_completed
                .saturating_add(stats.pomodoros_completed);
            totals.focused_seconds = totals.focused_seconds.saturating_add(stats.focused_seconds);
        }
        totals
    }

    pub(crate) fn weekly_goal_snapshot_for_day(
        &self,
        day: chrono::NaiveDate,
    ) -> Option<DailyGoalSnapshot> {
        let key = week_key_for_day(day);
        self.weekly_goal_snapshots.get(&key).copied()
    }

    pub(crate) fn monthly_goal_snapshot_for_day(
        &self,
        day: chrono::NaiveDate,
    ) -> Option<DailyGoalSnapshot> {
        let key = month_key_for_day(day);
        self.monthly_goal_snapshots.get(&key).copied()
    }

    pub(crate) fn weekly_focus_score_for_day(&self, day: chrono::NaiveDate) -> WeeklyFocusScore {
        let iso_week = day.iso_week();
        let year = iso_week.year();
        let week = iso_week.week();
        let week_label = format_week_label(year, week);
        let active_days = self
            .weekly_active_days()
            .get(&(year, week))
            .copied()
            .unwrap_or(0);
        let consistency_score_pct = consistency_score_from_active_days(active_days);
        let totals = self.weekly_for_day(day);
        let completion_score_pct = self
            .weekly_goal_snapshot_for_day(day)
            .and_then(|goal| weekly_completion_score_pct(goal, totals));
        let focus_score_pct = completion_score_pct
            .map(|completion| average_two_percentages(consistency_score_pct, completion));

        WeeklyFocusScore {
            year,
            week,
            week_label,
            active_days,
            consistency_score_pct,
            completion_score_pct,
            focus_score_pct,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn recent_weekly(&self, limit: usize) -> Vec<WeeklyStats> {
        let mut weekly = self.weekly_stats();
        weekly.reverse();
        weekly.truncate(limit);
        weekly
    }

    pub(crate) fn recent_weekly_consistency(&self, limit: usize) -> Vec<WeeklyConsistency> {
        let mut weekly = self.weekly_consistency_stats();
        weekly.reverse();
        weekly.truncate(limit);
        weekly
    }

    pub(crate) fn recent_weekly_focus_scores(&self, limit: usize) -> Vec<WeeklyFocusScore> {
        let mut weekly = self.weekly_focus_score_stats();
        weekly.reverse();
        weekly.truncate(limit);
        weekly
    }

    pub(crate) fn latest_weekly_focus_score(&self) -> Option<WeeklyFocusScore> {
        self.recent_weekly_focus_scores(1).into_iter().next()
    }

    pub(crate) fn focus_risk_forecast_for_day(
        &self,
        day: chrono::NaiveDate,
        daily_goal: DailyGoalSnapshot,
        _weekly_goal: DailyGoalSnapshot,
        _monthly_goal: DailyGoalSnapshot,
    ) -> FocusRiskForecast {
        let day_key = day.format("%Y-%m-%d").to_string();
        let daily_stats = self.daily_for(&day_key);
        let cadence = rolling_cadence_window(self, day, 7);
        let daily_goal_forecast = goal_risk_forecast(
            GoalPeriod::Daily,
            daily_goal,
            daily_stats.focused_minutes(),
            daily_stats.pomodoros_completed,
            1,
            cadence,
        );

        let streak = self.goal_streak_with_day_goal(day, daily_goal, daily_stats, |_| daily_goal);
        let streak_forecast =
            streak_risk_forecast(self, day, daily_goal, daily_stats, cadence, streak);

        FocusRiskForecast {
            daily_goal: daily_goal_forecast,
            streak: streak_forecast,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn focus_risk_calibration_metrics_for_day(
        &self,
        day: chrono::NaiveDate,
        daily_goal: DailyGoalSnapshot,
        weekly_goal: DailyGoalSnapshot,
        monthly_goal: DailyGoalSnapshot,
        window_days: u16,
    ) -> FocusRiskCalibrationMetrics {
        let mut sample_count = 0_u32;
        let mut alert_count = 0_u32;
        let mut true_positive_alerts = 0_u32;
        let mut false_positive_alerts = 0_u32;
        let mut missed_warning_count = 0_u32;

        for offset in 0..window_days.max(1) {
            let candidate = day
                .checked_sub_signed(chrono::Duration::days(i64::from(offset)))
                .unwrap_or(day);
            let day_key = candidate.format("%Y-%m-%d").to_string();
            let day_stats = self.daily_for(&day_key);
            let candidate_daily_goal = day_stats.goal.unwrap_or(daily_goal);
            let candidate_weekly_goal = self
                .weekly_goal_snapshot_for_day(candidate)
                .unwrap_or(weekly_goal);
            let candidate_monthly_goal = self
                .monthly_goal_snapshot_for_day(candidate)
                .unwrap_or(monthly_goal);
            let forecast = self.focus_risk_forecast_for_day(
                candidate,
                candidate_daily_goal,
                candidate_weekly_goal,
                candidate_monthly_goal,
            );

            let Some(observed_miss) = observed_goal_miss_for_candidate(
                self,
                candidate,
                day_stats,
                candidate_daily_goal,
                candidate_weekly_goal,
                candidate_monthly_goal,
            ) else {
                continue;
            };

            sample_count = sample_count.saturating_add(1);
            classify_calibration_signal(
                forecast.alert_active(),
                observed_miss,
                &mut alert_count,
                &mut true_positive_alerts,
                &mut false_positive_alerts,
                &mut missed_warning_count,
            );
        }

        let precision_pct = if alert_count == 0 {
            0
        } else {
            percentage_round_nearest(u64::from(true_positive_alerts), u64::from(alert_count))
        };
        let missed_warning_rate_pct = if sample_count == 0 {
            0
        } else {
            percentage_round_nearest(u64::from(missed_warning_count), u64::from(sample_count))
        };

        FocusRiskCalibrationMetrics {
            sample_count,
            alert_count,
            true_positive_alerts,
            false_positive_alerts,
            precision_pct,
            missed_warning_count,
            missed_warning_rate_pct,
        }
    }

    pub(crate) fn recent_monthly(&self, limit: usize) -> Vec<MonthlyStats> {
        let mut monthly = self.monthly_stats();
        monthly.reverse();
        monthly.truncate(limit);
        monthly
    }

    pub(crate) fn latest_monthly_heatmap(&self) -> MonthlyHeatmap {
        let (year, month) = self.latest_recorded_month_key().unwrap_or_else(|| {
            let now = chrono::Local::now().date_naive();
            (now.year(), now.month())
        });
        self.monthly_heatmap(year, month)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn profile_totals(&self) -> Vec<ProfileTotals> {
        let mut by_profile: BTreeMap<ProfileBucket, ProfileTotals> = BTreeMap::new();
        for session in &self.focus_sessions {
            let profile = profile_bucket_for(session.profile);
            let entry = by_profile.entry(profile).or_insert(ProfileTotals {
                profile,
                pomodoros_completed: 0,
                focused_seconds: 0,
            });
            entry.pomodoros_completed = entry.pomodoros_completed.saturating_add(1);
            entry.focused_seconds = entry
                .focused_seconds
                .saturating_add(session.focused_seconds);
        }

        let mut totals: Vec<ProfileTotals> = by_profile.into_values().collect();
        totals.sort_by(|left, right| {
            right
                .focused_seconds
                .cmp(&left.focused_seconds)
                .then_with(|| right.pomodoros_completed.cmp(&left.pomodoros_completed))
                .then_with(|| left.profile.cmp(&right.profile))
        });
        totals
    }

    pub(crate) fn profile_effectiveness(&self) -> Vec<ProfileEffectiveness> {
        let mut by_profile: BTreeMap<ProfileBucket, ProfileEffectivenessAccumulator> =
            BTreeMap::new();
        let mut total_focused_seconds: u64 = 0;
        for session in &self.focus_sessions {
            let profile = profile_bucket_for(session.profile);
            let entry = by_profile.entry(profile).or_default();
            entry.sessions_completed = entry.sessions_completed.saturating_add(1);
            entry.focused_seconds = entry
                .focused_seconds
                .saturating_add(session.focused_seconds);
            entry.active_days.insert(session.date.clone());
            total_focused_seconds = total_focused_seconds.saturating_add(session.focused_seconds);
        }

        let mut effectiveness: Vec<ProfileEffectiveness> = by_profile
            .into_iter()
            .map(|(profile, totals)| ProfileEffectiveness {
                profile,
                sessions_completed: totals.sessions_completed,
                focused_seconds: totals.focused_seconds,
                active_days: totals.active_days.len() as u32,
                focus_share_pct: percentage_round_nearest(
                    totals.focused_seconds,
                    total_focused_seconds,
                ),
            })
            .collect();
        effectiveness.sort_by(|left, right| {
            right
                .focus_share_pct
                .cmp(&left.focus_share_pct)
                .then_with(|| {
                    right
                        .average_focused_minutes_per_session()
                        .cmp(&left.average_focused_minutes_per_session())
                })
                .then_with(|| right.sessions_completed.cmp(&left.sessions_completed))
                .then_with(|| left.profile.cmp(&right.profile))
        });
        effectiveness
    }

    pub(crate) fn productivity_comparison(
        &self,
        dimension: ComparisonDimension,
        filter: &ProductivityComparisonFilter,
        limit: usize,
    ) -> Vec<ProductivityComparisonRow> {
        if limit == 0 {
            return Vec::new();
        }

        let normalized_task_filter = filter
            .task_label
            .as_deref()
            .and_then(normalize_task_label)
            .map(|label| {
                canonical_task_label(&self.task_labels, &label)
                    .unwrap_or(label)
                    .to_ascii_lowercase()
            });
        let mut grouped: BTreeMap<String, ProductivityComparisonRow> = BTreeMap::new();
        let mut total_focused_seconds: u64 = 0;

        for session in &self.focus_sessions {
            let Some(task_label) = normalize_task_label(&session.task_label) else {
                continue;
            };
            let task_label =
                canonical_task_label(&self.task_labels, &task_label).unwrap_or(task_label);
            let task_key = task_label.to_ascii_lowercase();
            if normalized_task_filter
                .as_ref()
                .is_some_and(|expected| expected != &task_key)
            {
                continue;
            }

            let profile = profile_bucket_for(session.profile);
            if filter.profile.is_some_and(|expected| expected != profile) {
                continue;
            }

            let time_of_day = focus_session_time_of_day(session);
            if filter
                .time_of_day
                .is_some_and(|expected| expected != time_of_day)
            {
                continue;
            }

            total_focused_seconds = total_focused_seconds.saturating_add(session.focused_seconds);
            let (group_key, label, row_task_label, row_profile, row_time_of_day) = match dimension {
                ComparisonDimension::TaskLabel => (
                    task_key.clone(),
                    task_label.clone(),
                    Some(task_label),
                    None,
                    None,
                ),
                ComparisonDimension::Profile => (
                    profile.id().to_string(),
                    profile.label().to_string(),
                    None,
                    Some(profile),
                    None,
                ),
                ComparisonDimension::TimeOfDay => (
                    time_of_day.id().to_string(),
                    time_of_day.label().to_string(),
                    None,
                    None,
                    Some(time_of_day),
                ),
            };

            let entry = grouped
                .entry(group_key)
                .or_insert_with(|| ProductivityComparisonRow {
                    dimension,
                    label,
                    task_label: row_task_label,
                    profile: row_profile,
                    time_of_day: row_time_of_day,
                    sessions_completed: 0,
                    focused_seconds: 0,
                    focus_share_pct: 0,
                });
            entry.sessions_completed = entry.sessions_completed.saturating_add(1);
            entry.focused_seconds = entry
                .focused_seconds
                .saturating_add(session.focused_seconds);
        }

        let mut comparisons: Vec<ProductivityComparisonRow> = grouped.into_values().collect();
        for entry in &mut comparisons {
            entry.focus_share_pct =
                percentage_round_nearest(entry.focused_seconds, total_focused_seconds);
        }
        comparisons.sort_by(|left, right| {
            right
                .focused_seconds
                .cmp(&left.focused_seconds)
                .then_with(|| right.sessions_completed.cmp(&left.sessions_completed))
                .then_with(|| left.label.cmp(&right.label))
        });
        comparisons.truncate(limit);
        comparisons
    }

    pub(super) fn weekly_consistency_stats(&self) -> Vec<WeeklyConsistency> {
        self.weekly_active_days()
            .into_iter()
            .map(|((year, week), active_days)| WeeklyConsistency {
                year,
                week,
                week_label: format_week_label(year, week),
                active_days,
                consistency_score_pct: consistency_score_from_active_days(active_days),
            })
            .collect()
    }

    pub(super) fn weekly_focus_score_stats(&self) -> Vec<WeeklyFocusScore> {
        let consistency_by_key: BTreeMap<(i32, u32), WeeklyConsistency> = self
            .weekly_consistency_stats()
            .into_iter()
            .map(|consistency| ((consistency.year, consistency.week), consistency))
            .collect();
        let weekly_totals_by_key: BTreeMap<(i32, u32), WeeklyStats> = self
            .weekly_stats()
            .into_iter()
            .map(|stats| ((stats.year, stats.week), stats))
            .collect();
        let mut all_week_keys: BTreeSet<(i32, u32)> = consistency_by_key.keys().copied().collect();
        for week_label in self.weekly_goal_snapshots.keys() {
            if let Some(week_key) = parse_week_label(week_label) {
                all_week_keys.insert(week_key);
            }
        }

        all_week_keys
            .into_iter()
            .map(|(year, week)| {
                let consistency = consistency_by_key.get(&(year, week));
                let week_label = format_week_label(year, week);
                let active_days = consistency.map_or(0, |entry| entry.active_days);
                let consistency_score_pct =
                    consistency.map_or(0, |entry| entry.consistency_score_pct);
                let totals =
                    weekly_totals_by_key
                        .get(&(year, week))
                        .copied()
                        .unwrap_or(WeeklyStats {
                            year,
                            week,
                            ..WeeklyStats::default()
                        });
                let completion_score_pct = self
                    .weekly_goal_snapshots
                    .get(&week_label)
                    .copied()
                    .and_then(|goal| weekly_completion_score_pct(goal, totals));
                let focus_score_pct = completion_score_pct
                    .map(|completion| average_two_percentages(consistency_score_pct, completion));
                WeeklyFocusScore {
                    year,
                    week,
                    week_label,
                    active_days,
                    consistency_score_pct,
                    completion_score_pct,
                    focus_score_pct,
                }
            })
            .collect()
    }

    fn weekly_active_days(&self) -> BTreeMap<(i32, u32), u8> {
        let mut weekly = BTreeMap::new();
        for (day_key, stats) in &self.daily {
            if !daily_has_activity(*stats) {
                continue;
            }
            let Ok(day) = chrono::NaiveDate::parse_from_str(day_key, "%Y-%m-%d") else {
                continue;
            };
            let iso_week = day.iso_week();
            let active_days = weekly
                .entry((iso_week.year(), iso_week.week()))
                .or_insert(0_u8);
            *active_days = active_days.saturating_add(1).min(7);
        }
        weekly
    }

    pub(super) fn weekly_stats(&self) -> Vec<WeeklyStats> {
        let mut weekly = BTreeMap::new();

        for (day_key, stats) in &self.daily {
            let Ok(day) = chrono::NaiveDate::parse_from_str(day_key, "%Y-%m-%d") else {
                continue;
            };
            let iso_week = day.iso_week();
            let entry = weekly
                .entry((iso_week.year(), iso_week.week()))
                .or_insert_with(|| WeeklyStats {
                    year: iso_week.year(),
                    week: iso_week.week(),
                    ..WeeklyStats::default()
                });
            entry.pomodoros_completed = entry
                .pomodoros_completed
                .saturating_add(stats.pomodoros_completed);
            entry.focused_seconds = entry.focused_seconds.saturating_add(stats.focused_seconds);
        }

        weekly.into_values().collect()
    }

    pub(super) fn monthly_stats(&self) -> Vec<MonthlyStats> {
        let mut monthly = BTreeMap::new();

        for (day_key, stats) in &self.daily {
            let Ok(day) = chrono::NaiveDate::parse_from_str(day_key, "%Y-%m-%d") else {
                continue;
            };
            let entry = monthly
                .entry((day.year(), day.month()))
                .or_insert_with(|| MonthlyStats {
                    year: day.year(),
                    month: day.month(),
                    ..MonthlyStats::default()
                });
            entry.pomodoros_completed = entry
                .pomodoros_completed
                .saturating_add(stats.pomodoros_completed);
            entry.focused_seconds = entry.focused_seconds.saturating_add(stats.focused_seconds);
        }

        monthly.into_values().collect()
    }

    fn latest_recorded_month_key(&self) -> Option<(i32, u32)> {
        self.daily
            .keys()
            .rev()
            .find_map(|day_key| chrono::NaiveDate::parse_from_str(day_key, "%Y-%m-%d").ok())
            .map(|day| (day.year(), day.month()))
    }

    fn monthly_heatmap(&self, year: i32, month: u32) -> MonthlyHeatmap {
        let (year, month) = if chrono::NaiveDate::from_ymd_opt(year, month, 1).is_some() {
            (year, month)
        } else {
            let now = chrono::Local::now().date_naive();
            (now.year(), now.month())
        };

        let month_start = chrono::NaiveDate::from_ymd_opt(year, month, 1)
            .expect("validated month/year should produce valid first day");
        let days_in_month = days_in_month(year, month);
        let mut max_focused_minutes = 0;
        let mut days = Vec::with_capacity(days_in_month as usize);
        for day in 1..=days_in_month {
            let day_key = format!("{year:04}-{month:02}-{day:02}");
            let stats = self.daily_for(&day_key);
            let focused_minutes = stats.focused_minutes();
            max_focused_minutes = max_focused_minutes.max(focused_minutes);
            days.push(HeatmapDayStats {
                day,
                pomodoros_completed: stats.pomodoros_completed,
                focused_seconds: stats.focused_seconds,
            });
        }

        MonthlyHeatmap {
            year,
            month,
            first_weekday_monday0: month_start.weekday().num_days_from_monday(),
            days_in_month,
            max_focused_minutes,
            days,
        }
    }

    pub(crate) fn growth_summary(&self) -> StatsGrowthSummary {
        let mut sections = vec![
            stats_growth_section("daily", self.daily.len(), &self.daily),
            stats_growth_section(
                "weekly_goal_snapshots",
                self.weekly_goal_snapshots.len(),
                &self.weekly_goal_snapshots,
            ),
            stats_growth_section(
                "monthly_goal_snapshots",
                self.monthly_goal_snapshots.len(),
                &self.monthly_goal_snapshots,
            ),
            stats_growth_section("task_labels", self.task_labels.len(), &self.task_labels),
            stats_growth_section(
                "selected_task_label",
                usize::from(self.selected_task_label.is_some()),
                &self.selected_task_label,
            ),
            stats_growth_section(
                "task_label_favorites",
                self.task_label_favorites.len(),
                &self.task_label_favorites,
            ),
            stats_growth_section(
                "task_label_archived",
                self.task_label_archived.len(),
                &self.task_label_archived,
            ),
            stats_growth_section(
                "focus_sessions",
                self.focus_sessions.len(),
                &self.focus_sessions,
            ),
            stats_growth_section(
                "session_interruptions",
                self.session_interruptions.len(),
                &self.session_interruptions,
            ),
            stats_growth_section(
                "command_usage_counts",
                self.command_usage_counts.len(),
                &self.command_usage_counts,
            ),
            stats_growth_section(
                "screen_usage_counts",
                self.screen_usage_counts.len(),
                &self.screen_usage_counts,
            ),
        ];
        sections.sort_by(|left, right| left.name.cmp(&right.name));
        let total_record_count = sections.iter().fold(0_usize, |total, section| {
            total.saturating_add(section.record_count)
        });
        let mut high_volume_sections: Vec<StatsGrowthSection> = sections
            .iter()
            .filter(|section| section.record_count > 0)
            .cloned()
            .collect();
        high_volume_sections.sort_by(|left, right| {
            right
                .record_count
                .cmp(&left.record_count)
                .then_with(|| right.estimated_bytes.cmp(&left.estimated_bytes))
                .then_with(|| left.name.cmp(&right.name))
        });
        high_volume_sections.truncate(3);

        StatsGrowthSummary {
            total_record_count,
            estimated_bytes: estimated_serialized_bytes(&self.to_persisted()),
            sections,
            high_volume_sections,
        }
    }

    pub(crate) fn apply_retention_policy(
        &mut self,
        retention: StatsRetentionConfig,
        reference_day: chrono::NaiveDate,
    ) -> StatsRetentionPruneResult {
        let windows = retention.windows();
        let mut result = StatsRetentionPruneResult::default();

        if let Some(keep_days) = windows.keep_daily_days {
            let cutoff_day = retention_cutoff_day(reference_day, keep_days);
            let before = self.daily.len();
            self.daily
                .retain(|day_key, _| is_day_key_on_or_after(day_key, cutoff_day));
            result.daily_removed = before.saturating_sub(self.daily.len());
        }

        if let Some(keep_days) = windows.keep_focus_sessions_days {
            let cutoff_day = retention_cutoff_day(reference_day, keep_days);
            let before = self.focus_sessions.len();
            self.focus_sessions
                .retain(|session| is_day_key_on_or_after(&session.date, cutoff_day));
            result.focus_sessions_removed = before.saturating_sub(self.focus_sessions.len());
        }

        if let Some(keep_days) = windows.keep_session_interruptions_days {
            let cutoff_day = retention_cutoff_day(reference_day, keep_days);
            let before = self.session_interruptions.len();
            self.session_interruptions
                .retain(|event| is_day_key_on_or_after(&event.date, cutoff_day));
            result.session_interruptions_removed =
                before.saturating_sub(self.session_interruptions.len());
        }

        if let Some(keep_days) = windows.keep_weekly_goal_snapshots_days {
            let cutoff_day = retention_cutoff_day(reference_day, keep_days);
            let before = self.weekly_goal_snapshots.len();
            self.weekly_goal_snapshots
                .retain(|week_key, _| is_week_key_on_or_after(week_key, cutoff_day));
            result.weekly_goal_snapshots_removed =
                before.saturating_sub(self.weekly_goal_snapshots.len());
        }

        if let Some(keep_days) = windows.keep_monthly_goal_snapshots_days {
            let cutoff_day = retention_cutoff_day(reference_day, keep_days);
            let before = self.monthly_goal_snapshots.len();
            self.monthly_goal_snapshots
                .retain(|month_key, _| is_month_key_on_or_after(month_key, cutoff_day));
            result.monthly_goal_snapshots_removed =
                before.saturating_sub(self.monthly_goal_snapshots.len());
        }

        result
    }

    pub(crate) fn retention_preview(
        &self,
        retention: StatsRetentionConfig,
        reference_day: chrono::NaiveDate,
    ) -> StatsRetentionPruneResult {
        let mut cloned = self.clone();
        cloned.apply_retention_policy(retention, reference_day)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn usage_signal_summary(&self, limit: usize) -> UsageSignalsSummary {
        let limit = limit.max(1);
        UsageSignalsSummary {
            commands: usage_signal_summary_for_counts(&self.command_usage_counts, limit),
            screens: usage_signal_summary_for_counts(&self.screen_usage_counts, limit),
        }
    }
}

fn focus_session_time_of_day(session: &FocusSessionRecord) -> TimeOfDayBucket {
    backfilled_time_of_day_bucket(
        session.completion_time_of_day_bucket,
        session.completion_timestamp_epoch_secs,
    )
}
