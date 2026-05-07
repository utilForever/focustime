use crate::stats::{
    BTreeMap, BTreeSet, DailyGoalSnapshot, Datelike, FocusStats, HeatmapDayStats, MonthlyHeatmap,
    MonthlyStats, ProfileBucket, ProfileEffectiveness, ProfileEffectivenessAccumulator,
    ProfileTotals, StatsGrowthSection, StatsGrowthSummary, StatsRetentionConfig,
    StatsRetentionPruneResult, WeeklyConsistency, WeeklyFocusScore, WeeklyStats,
    average_two_percentages, consistency_score_from_active_days, daily_has_activity, days_in_month,
    format_week_label, month_key_for_day, parse_week_label, percentage_round_nearest,
    profile_bucket_for, week_key_for_day, weekly_completion_score_pct,
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
    toml::to_string(value)
        .map(|serialized| serialized.len() as u64)
        .unwrap_or(0)
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
