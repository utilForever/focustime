use crate::stats::{
    BTreeMap, BTreeSet, ComparisonDimension, DailyGoalSnapshot, Datelike,
    FocusRiskCalibrationMetrics, FocusRiskForecast, FocusRiskLevel, FocusRiskSignal,
    FocusSessionRecord, FocusStats, GoalPeriod, GoalRiskForecast, HeatmapDayStats, MonthlyHeatmap,
    MonthlyStats, ProductivityComparisonFilter, ProductivityComparisonRow, ProfileBucket,
    ProfileEffectiveness, ProfileEffectivenessAccumulator, ProfileTotals, StatsGrowthSection,
    StatsGrowthSummary, StatsRetentionConfig, StatsRetentionPruneResult, StreakRiskForecast,
    TimeOfDayBucket, UsageSignalEntry, UsageSignalSummary, UsageSignalsSummary, WeeklyConsistency,
    WeeklyFocusScore, WeeklyStats, average_two_percentages, backfilled_time_of_day_bucket,
    canonical_task_label, consistency_score_from_active_days, daily_has_activity, days_in_month,
    format_week_label, month_key_for_day, normalize_task_label, parse_week_label,
    percentage_round_nearest, profile_bucket_for, week_key_for_day, weekly_completion_score_pct,
};

impl FocusStats {
    pub fn weekly_for_day(&self, day: chrono::NaiveDate) -> WeeklyStats {
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

    pub fn monthly_for_day(&self, day: chrono::NaiveDate) -> MonthlyStats {
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

    pub fn weekly_goal_snapshot_for_day(
        &self,
        day: chrono::NaiveDate,
    ) -> Option<DailyGoalSnapshot> {
        let key = week_key_for_day(day);
        self.weekly_goal_snapshots.get(&key).copied()
    }

    pub fn monthly_goal_snapshot_for_day(
        &self,
        day: chrono::NaiveDate,
    ) -> Option<DailyGoalSnapshot> {
        let key = month_key_for_day(day);
        self.monthly_goal_snapshots.get(&key).copied()
    }

    pub fn weekly_focus_score_for_day(&self, day: chrono::NaiveDate) -> WeeklyFocusScore {
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
    pub fn recent_weekly(&self, limit: usize) -> Vec<WeeklyStats> {
        let mut weekly = self.weekly_stats();
        weekly.reverse();
        weekly.truncate(limit);
        weekly
    }

    pub fn recent_weekly_consistency(&self, limit: usize) -> Vec<WeeklyConsistency> {
        let mut weekly = self.weekly_consistency_stats();
        weekly.reverse();
        weekly.truncate(limit);
        weekly
    }

    pub fn recent_weekly_focus_scores(&self, limit: usize) -> Vec<WeeklyFocusScore> {
        let mut weekly = self.weekly_focus_score_stats();
        weekly.reverse();
        weekly.truncate(limit);
        weekly
    }

    pub fn latest_weekly_focus_score(&self) -> Option<WeeklyFocusScore> {
        self.recent_weekly_focus_scores(1).into_iter().next()
    }

    pub fn focus_risk_forecast_for_day(
        &self,
        day: chrono::NaiveDate,
        daily_goal: DailyGoalSnapshot,
        weekly_goal: DailyGoalSnapshot,
        monthly_goal: DailyGoalSnapshot,
    ) -> FocusRiskForecast {
        let day_key = day.format("%Y-%m-%d").to_string();
        let daily_stats = self.daily_for(&day_key);
        let weekly_stats = self.weekly_for_day(day);
        let monthly_stats = self.monthly_for_day(day);
        let cadence = rolling_cadence_window(self, day, 7);
        let daily_goal_forecast = goal_risk_forecast(
            GoalPeriod::Daily,
            daily_goal,
            daily_stats.focused_minutes(),
            daily_stats.pomodoros_completed,
            1,
            cadence,
        );
        let weekly_goal_forecast = goal_risk_forecast(
            GoalPeriod::Weekly,
            weekly_goal,
            weekly_stats.focused_minutes(),
            weekly_stats.pomodoros_completed,
            remaining_days_in_week(day),
            cadence,
        );
        let monthly_goal_forecast = goal_risk_forecast(
            GoalPeriod::Monthly,
            monthly_goal,
            monthly_stats.focused_minutes(),
            monthly_stats.pomodoros_completed,
            remaining_days_in_month(day),
            cadence,
        );

        let streak = self.goal_streak_with_day_goal(day, daily_goal, daily_stats, |_| daily_goal);
        let streak_forecast =
            streak_risk_forecast(self, day, daily_goal, daily_stats, cadence, streak);

        FocusRiskForecast {
            daily_goal: daily_goal_forecast,
            weekly_goal: weekly_goal_forecast,
            monthly_goal: monthly_goal_forecast,
            streak: streak_forecast,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn focus_risk_calibration_metrics_for_day(
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

    pub fn recent_monthly(&self, limit: usize) -> Vec<MonthlyStats> {
        let mut monthly = self.monthly_stats();
        monthly.reverse();
        monthly.truncate(limit);
        monthly
    }

    pub fn latest_monthly_heatmap(&self) -> MonthlyHeatmap {
        let (year, month) = self.latest_recorded_month_key().unwrap_or_else(|| {
            let now = chrono::Local::now().date_naive();
            (now.year(), now.month())
        });
        self.monthly_heatmap(year, month)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn profile_totals(&self) -> Vec<ProfileTotals> {
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

    pub fn profile_effectiveness(&self) -> Vec<ProfileEffectiveness> {
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

    pub fn productivity_comparison(
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

    pub fn growth_summary(&self) -> StatsGrowthSummary {
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
                "break_glass_overrides",
                self.break_glass_overrides.len(),
                &self.break_glass_overrides,
            ),
            stats_growth_section(
                "task_goal_targets",
                self.task_goal_targets.len(),
                &self.task_goal_targets,
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

    pub fn apply_retention_policy(
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

        if let Some(keep_days) = windows.keep_break_glass_overrides_days {
            let cutoff_day = retention_cutoff_day(reference_day, keep_days);
            let before = self.break_glass_overrides.len();
            self.break_glass_overrides
                .retain(|event| is_day_key_on_or_after(&event.date, cutoff_day));
            result.break_glass_overrides_removed =
                before.saturating_sub(self.break_glass_overrides.len());
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

    pub fn retention_preview(
        &self,
        retention: StatsRetentionConfig,
        reference_day: chrono::NaiveDate,
    ) -> StatsRetentionPruneResult {
        let mut cloned = self.clone();
        cloned.apply_retention_policy(retention, reference_day)
    }

    pub fn usage_signal_summary(&self, limit: usize) -> UsageSignalsSummary {
        let limit = limit.max(1);
        UsageSignalsSummary {
            commands: usage_signal_summary_for_counts(&self.command_usage_counts, limit),
            screens: usage_signal_summary_for_counts(&self.screen_usage_counts, limit),
        }
    }
}

fn observed_goal_miss_for_candidate(
    stats: &FocusStats,
    candidate: chrono::NaiveDate,
    day_stats: crate::stats::DailyStats,
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

fn classify_calibration_signal(
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

fn focus_session_time_of_day(session: &FocusSessionRecord) -> TimeOfDayBucket {
    backfilled_time_of_day_bucket(
        session.completion_time_of_day_bucket,
        session.completion_timestamp_epoch_secs,
    )
}

#[derive(Debug, Clone, Copy)]
struct CadenceWindow {
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

fn rolling_cadence_window(
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

fn goal_risk_forecast(
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

fn streak_risk_forecast(
    stats: &FocusStats,
    day: chrono::NaiveDate,
    daily_goal: DailyGoalSnapshot,
    today_stats: crate::stats::DailyStats,
    cadence: CadenceWindow,
    streak: crate::stats::GoalStreak,
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

fn rolling_goal_reliability_pct(
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

fn completion_pct_for_totals(
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

fn pace_gap_pct(
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

fn gap_pct(recent_rate: u64, required_rate: u64) -> u8 {
    if required_rate == 0 || recent_rate >= required_rate {
        return 0;
    }
    percentage_round_nearest(required_rate.saturating_sub(recent_rate), required_rate)
}

fn weighted_pct(parts: &[(u8, u16)]) -> u8 {
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

fn risk_signal(label: &str, value: &str) -> FocusRiskSignal {
    FocusRiskSignal {
        label: label.to_string(),
        value: value.to_string(),
    }
}

fn remaining_days_in_week(day: chrono::NaiveDate) -> u32 {
    7_u32.saturating_sub(day.weekday().num_days_from_monday())
}

fn remaining_days_in_month(day: chrono::NaiveDate) -> u32 {
    let total_days = days_in_month(day.year(), day.month());
    total_days.saturating_sub(day.day()).saturating_add(1)
}

fn div_ceil_u64(value: u64, divisor: u64) -> u64 {
    if divisor == 0 {
        return value;
    }
    value.div_ceil(divisor)
}

fn usage_signal_summary_for_counts(
    counts: &BTreeMap<String, u64>,
    limit: usize,
) -> UsageSignalSummary {
    let total_events = counts
        .values()
        .copied()
        .fold(0_u64, |total, value| total.saturating_add(value));
    let unique_surfaces = counts.len();
    let mut entries: Vec<(String, u64)> = counts.iter().map(|(k, v)| (k.clone(), *v)).collect();

    let mut top_entries = entries.clone();
    top_entries.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    top_entries.truncate(limit);

    entries.sort_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)));
    entries.truncate(limit);

    UsageSignalSummary {
        total_events,
        unique_surfaces,
        top: usage_signal_rows(top_entries, total_events),
        rare: usage_signal_rows(entries, total_events),
    }
}

fn usage_signal_rows(entries: Vec<(String, u64)>, total_events: u64) -> Vec<UsageSignalEntry> {
    entries
        .into_iter()
        .map(|(surface, count)| UsageSignalEntry {
            surface,
            count,
            share_pct: percentage_round_nearest(count, total_events),
        })
        .collect()
}

fn stats_growth_section(
    name: &str,
    record_count: usize,
    value: &impl serde::Serialize,
) -> StatsGrowthSection {
    StatsGrowthSection {
        name: name.to_string(),
        record_count,
        estimated_bytes: estimated_serialized_bytes(value),
    }
}

fn estimated_serialized_bytes(value: &impl serde::Serialize) -> u64 {
    #[derive(serde::Serialize)]
    struct SizeProbe<'a, T: ?Sized + serde::Serialize> {
        value: &'a T,
    }

    toml::to_string(&SizeProbe { value })
        .expect("stats growth section should be serializable")
        .len() as u64
}

fn retention_cutoff_day(reference_day: chrono::NaiveDate, keep_days: u16) -> chrono::NaiveDate {
    let days_to_keep = i64::from(keep_days.max(1));
    reference_day
        .checked_sub_signed(chrono::Duration::days(days_to_keep.saturating_sub(1)))
        .unwrap_or(reference_day)
}

fn is_day_key_on_or_after(day_key: &str, cutoff_day: chrono::NaiveDate) -> bool {
    chrono::NaiveDate::parse_from_str(day_key, "%Y-%m-%d")
        .map(|day| day >= cutoff_day)
        .unwrap_or(true)
}

fn is_week_key_on_or_after(week_key: &str, cutoff_day: chrono::NaiveDate) -> bool {
    let Some((year, week)) = parse_week_label(week_key) else {
        return true;
    };
    let Some(week_start) = chrono::NaiveDate::from_isoywd_opt(year, week, chrono::Weekday::Mon)
    else {
        return true;
    };
    let week_end = week_start
        .checked_add_signed(chrono::Duration::days(6))
        .unwrap_or(week_start);
    week_end >= cutoff_day
}

fn is_month_key_on_or_after(month_key: &str, cutoff_day: chrono::NaiveDate) -> bool {
    let Some((year_token, month_token)) = month_key.split_once('-') else {
        return true;
    };
    let Ok(year) = year_token.parse::<i32>() else {
        return true;
    };
    let Ok(month) = month_token.parse::<u32>() else {
        return true;
    };
    let Some(month_start) = chrono::NaiveDate::from_ymd_opt(year, month, 1) else {
        return true;
    };
    let month_end_day = days_in_month(year, month);
    let month_end =
        chrono::NaiveDate::from_ymd_opt(year, month, month_end_day).unwrap_or(month_start);
    month_end >= cutoff_day
}
