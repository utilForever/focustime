use crate::app::weekly_daily_goal_allocation_for_context;
use crate::cli::{
    AppConfig, CustomProfileConfig, DEFAULT_FOCUS_SECS, DEFAULT_LONG_BREAK_INTERVAL,
    DEFAULT_LONG_BREAK_SECS, DEFAULT_SHORT_BREAK_SECS, DailyGoalSnapshot, Datelike,
    FocusScoreOutput, FocusStats, GoalOutput, LiveStatusOutput, NaiveDate, ProfileId, ProfileSpec,
    ProfileView, SessionOutput, StatsRetentionStatusOutput, StatusOutput,
    TemporaryOverrideStatusOutput, ThemePreset, ThemePresetView, TimerPhase, TimerStatus,
    TodayOutput, WeeklyAllocationDayOutput, WeeklyAllocationOutput, carry_over_goal_target,
    current_day_key, effective_blocked_sites_for_profile, session_recovery,
};
use crate::session_recovery::{
    WorkflowStateSnapshot, WorkflowTemporaryOverrideKind, WorkflowTemporaryOverrideSnapshot,
};
use crate::timer::TimerState;

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn build_status_output(config: &AppConfig, stats: &FocusStats) -> StatusOutput {
    let day = current_day_key();
    let day_date = NaiveDate::parse_from_str(&day, "%Y-%m-%d")
        .expect("current_day_key should always be a valid ISO date");
    let today = stats.daily_for(&day);
    let week = stats.weekly_for_day(day_date);
    let month = stats.monthly_for_day(day_date);
    let (_, selected_task_label) = stats.task_planner_state();
    let selected_task_label = normalize_optional_task_label(selected_task_label);
    let goal_snapshot = effective_daily_goal_snapshot_for_day(config, stats, day_date);
    let weekly_goal_snapshot = effective_weekly_goal_snapshot(config, stats, day_date);
    let monthly_goal_snapshot = effective_monthly_goal_snapshot(config, stats, day_date);
    let active_sites_count = config
        .blocklist_profiles
        .iter()
        .find(|profile| {
            profile
                .name
                .eq_ignore_ascii_case(&config.selected_blocklist_profile)
        })
        .map(|profile| effective_blocked_sites_for_profile(profile).len())
        .unwrap_or_default();
    let selected_automation = config.profile_automation_for(config.selected_profile);
    let workflow_state = session_recovery::load_workflow_state().ok().flatten();
    let temporary_overrides = active_temporary_override_status(config, workflow_state.as_ref());
    let temporary_overrides_active_count = temporary_overrides
        .iter()
        .filter(|entry| !entry.pending_confirmation)
        .count();
    let live = build_live_status_output(config, selected_task_label.clone());
    let session = build_session_output(&live);
    let latest_interruption = stats.latest_session_interruption();
    let consistency_score_pct = stats
        .weekly_focus_score_for_day(day_date)
        .consistency_score_pct;
    let completion_score_pct = if weekly_goal_snapshot.has_any_target() {
        weekly_goal_completion_score_pct(
            weekly_goal_snapshot,
            week.focused_minutes(),
            week.pomodoros_completed,
        )
    } else {
        None
    };
    let focus_score_pct = completion_score_pct.map(|completion| {
        (u16::from(consistency_score_pct) + u16::from(completion)).div_ceil(2) as u8
    });
    let focus_risk = stats.focus_risk_forecast_for_day(
        day_date,
        goal_snapshot,
        weekly_goal_snapshot,
        monthly_goal_snapshot,
    );
    let weekly_allocation = build_weekly_allocation_output(
        day_date,
        weekly_goal_snapshot,
        week,
        &selected_automation.recurring_schedule,
    );
    let stats_growth = stats.growth_summary();
    let retention_windows = config.stats_retention.windows();
    let pending_prune = stats.retention_preview(config.stats_retention, day_date);

    StatusOutput {
        day,
        selected_profile: profile_view(config.selected_profile, &config.effective_custom_profile()),
        selected_theme_preset: theme_preset_view(config.selected_theme_preset),
        selected_task_label,
        selected_blocklist_profile: config.selected_blocklist_profile.clone(),
        blocked_sites_count: active_sites_count,
        temporary_overrides_active_count,
        temporary_overrides,
        strict_mode: selected_automation.strict_mode,
        goal: GoalOutput {
            configured: goal_snapshot.has_any_target(),
            minutes_target: goal_snapshot.minutes,
            pomodoros_target: goal_snapshot.pomodoros,
            met: goal_snapshot.is_met_by(today),
            carry_over: config.goal_carry_over.daily,
        },
        weekly_goal: GoalOutput {
            configured: weekly_goal_snapshot.has_any_target(),
            minutes_target: weekly_goal_snapshot.minutes,
            pomodoros_target: weekly_goal_snapshot.pomodoros,
            met: weekly_goal_snapshot
                .is_met_by_totals(week.focused_minutes(), week.pomodoros_completed),
            carry_over: config.goal_carry_over.weekly,
        },
        weekly_allocation,
        monthly_goal: GoalOutput {
            configured: monthly_goal_snapshot.has_any_target(),
            minutes_target: monthly_goal_snapshot.minutes,
            pomodoros_target: monthly_goal_snapshot.pomodoros,
            met: monthly_goal_snapshot
                .is_met_by_totals(month.focused_minutes(), month.pomodoros_completed),
            carry_over: config.goal_carry_over.monthly,
        },
        session: SessionOutput {
            focused_minutes: session.focused_minutes,
            pomodoros_completed: session.pomodoros_completed,
        },
        today: TodayOutput {
            focused_minutes: today.focused_minutes(),
            pomodoros_completed: today.pomodoros_completed,
        },
        latest_interruption,
        focus_score: FocusScoreOutput {
            available: focus_score_pct.is_some(),
            focus_score_pct,
            consistency_score_pct,
            completion_score_pct,
        },
        focus_risk,
        stats_growth,
        stats_retention: StatsRetentionStatusOutput {
            preset: config.stats_retention.preset.id(),
            keep_daily_days: retention_windows.keep_daily_days,
            keep_focus_sessions_days: retention_windows.keep_focus_sessions_days,
            keep_session_interruptions_days: retention_windows.keep_session_interruptions_days,
            keep_break_glass_overrides_days: retention_windows.keep_break_glass_overrides_days,
            pending_prune,
        },
        live,
    }
}

fn active_temporary_override_status(
    config: &AppConfig,
    workflow_state: Option<&WorkflowStateSnapshot>,
) -> Vec<TemporaryOverrideStatusOutput> {
    let now_epoch_secs = chrono::Local::now().timestamp();
    let selected_profile = config.selected_blocklist_profile.trim();
    let Some(workflow_state) = workflow_state else {
        return Vec::new();
    };

    let mut active = workflow_state
        .temporary_overrides
        .clone()
        .into_iter()
        .filter_map(|entry| {
            temporary_override_status_output(entry, selected_profile, now_epoch_secs)
        })
        .collect::<Vec<_>>();
    active.sort_by(|left, right| {
        left.pending_confirmation
            .cmp(&right.pending_confirmation)
            .then_with(|| {
                left.remaining_secs
                    .unwrap_or(u64::MAX)
                    .cmp(&right.remaining_secs.unwrap_or(u64::MAX))
            })
            .then_with(|| left.kind.cmp(right.kind))
            .then_with(|| left.site.cmp(&right.site))
    });
    active
}

fn temporary_override_status_output(
    entry: WorkflowTemporaryOverrideSnapshot,
    selected_profile: &str,
    now_epoch_secs: i64,
) -> Option<TemporaryOverrideStatusOutput> {
    match entry.kind {
        WorkflowTemporaryOverrideKind::AllowlistSite => {
            temporary_allowlist_override_status(entry, selected_profile, now_epoch_secs)
        }
        WorkflowTemporaryOverrideKind::BreakGlass => {
            break_glass_override_status(entry, now_epoch_secs)
        }
    }
}

fn temporary_allowlist_override_status(
    entry: WorkflowTemporaryOverrideSnapshot,
    selected_profile: &str,
    now_epoch_secs: i64,
) -> Option<TemporaryOverrideStatusOutput> {
    let profile = entry.profile.unwrap_or_default();
    if selected_profile.is_empty() || !profile.eq_ignore_ascii_case(selected_profile) {
        return None;
    }
    let expires_at_epoch_secs = entry.expires_at_epoch_secs?;
    if expires_at_epoch_secs <= now_epoch_secs {
        return None;
    }
    let site = entry.site.unwrap_or_default().trim().to_string();
    if site.is_empty() {
        return None;
    }
    Some(TemporaryOverrideStatusOutput {
        kind: "allowlist-site",
        profile: Some(profile),
        site: Some(site),
        remaining_secs: Some((expires_at_epoch_secs - now_epoch_secs) as u64),
        expires_at_epoch_secs: Some(expires_at_epoch_secs),
        pending_confirmation: false,
    })
}

fn break_glass_override_status(
    entry: WorkflowTemporaryOverrideSnapshot,
    now_epoch_secs: i64,
) -> Option<TemporaryOverrideStatusOutput> {
    let active_expiry = entry
        .expires_at_epoch_secs
        .filter(|expires_at_epoch_secs| *expires_at_epoch_secs > now_epoch_secs);
    if active_expiry.is_none() && !entry.confirmation_pending {
        return None;
    }
    Some(TemporaryOverrideStatusOutput {
        kind: "break-glass",
        profile: None,
        site: None,
        remaining_secs: active_expiry
            .map(|expires_at_epoch_secs| (expires_at_epoch_secs - now_epoch_secs) as u64),
        expires_at_epoch_secs: active_expiry,
        pending_confirmation: entry.confirmation_pending,
    })
}

fn build_session_output(live: &LiveStatusOutput) -> SessionOutput {
    if !live.in_progress {
        return SessionOutput {
            focused_minutes: 0,
            pomodoros_completed: 0,
        };
    }

    let mut focused_seconds =
        u64::from(live.pomodoros_completed).saturating_mul(live.selected_profile.focus_secs);
    if live.phase == "focus" && (live.status == "running" || live.status == "paused") {
        focused_seconds = focused_seconds.saturating_add(
            live.selected_profile
                .focus_secs
                .saturating_sub(live.remaining_secs),
        );
    }

    SessionOutput {
        focused_minutes: focused_seconds / 60,
        pomodoros_completed: live.pomodoros_completed,
    }
}

fn weekly_goal_completion_score_pct(
    goal: DailyGoalSnapshot,
    focused_minutes: u64,
    pomodoros_completed: u32,
) -> Option<u8> {
    let minute_score = if goal.minutes > 0 {
        Some(percentage_round_nearest(
            focused_minutes.min(goal.minutes),
            goal.minutes,
        ))
    } else {
        None
    };
    let pomodoro_score = if goal.pomodoros > 0 {
        Some(percentage_round_nearest(
            u64::from(pomodoros_completed.min(goal.pomodoros)),
            u64::from(goal.pomodoros),
        ))
    } else {
        None
    };
    match (minute_score, pomodoro_score) {
        (None, None) => None,
        (Some(score), None) | (None, Some(score)) => Some(score),
        (Some(left), Some(right)) => Some((u16::from(left) + u16::from(right)).div_ceil(2) as u8),
    }
}

fn build_weekly_allocation_output(
    day: NaiveDate,
    weekly_goal_snapshot: DailyGoalSnapshot,
    week: crate::stats::WeeklyStats,
    schedule: &crate::config::RecurringScheduleConfig,
) -> WeeklyAllocationOutput {
    let allocation =
        weekly_daily_goal_allocation_for_context(day, weekly_goal_snapshot, week, schedule);
    let today_target = allocation.today_target();
    WeeklyAllocationOutput {
        available: allocation.has_any_target(),
        uses_schedule_weights: allocation.uses_schedule_weights,
        remaining_days_in_week: allocation.remaining_days_in_week,
        allocatable_days: allocation.allocatable_days,
        completed_minutes: allocation.completed_minutes,
        completed_pomodoros: allocation.completed_pomodoros,
        remaining_minutes: allocation.remaining_minutes,
        remaining_pomodoros: allocation.remaining_pomodoros,
        today_minutes_target: today_target.minutes,
        today_pomodoros_target: today_target.pomodoros,
        days: allocation
            .daily_targets
            .into_iter()
            .map(|target| WeeklyAllocationDayOutput {
                date: target.day.format("%Y-%m-%d").to_string(),
                minutes_target: target.minutes_target,
                pomodoros_target: target.pomodoros_target,
                allocatable: target.allocatable,
            })
            .collect(),
    }
}

fn percentage_round_nearest(part: u64, total: u64) -> u8 {
    if total == 0 {
        return 0;
    }
    let rounded = (u128::from(part) * 100 + (u128::from(total) / 2)) / u128::from(total);
    rounded.min(u128::from(u8::MAX)) as u8
}

fn effective_daily_goal_snapshot_for_day(
    config: &AppConfig,
    stats: &FocusStats,
    day: NaiveDate,
) -> DailyGoalSnapshot {
    let day_key = day.format("%Y-%m-%d").to_string();
    let base = stats
        .daily_entry(&day_key)
        .and_then(|daily| daily.goal)
        .unwrap_or(DailyGoalSnapshot {
            minutes: config.daily_goal.minutes,
            pomodoros: config.daily_goal.pomodoros,
        });
    let previous = day.pred_opt().and_then(|previous_day| {
        let day_key = previous_day.format("%Y-%m-%d").to_string();
        stats.daily_entry(&day_key).and_then(|daily| {
            daily
                .goal
                .map(|goal| (goal, daily.focused_minutes(), daily.pomodoros_completed))
        })
    });
    carry_over_goal_target(base, config.goal_carry_over.daily, previous)
}

fn effective_weekly_goal_snapshot(
    config: &AppConfig,
    stats: &FocusStats,
    day: NaiveDate,
) -> DailyGoalSnapshot {
    let base = DailyGoalSnapshot {
        minutes: config.weekly_goal.minutes,
        pomodoros: config.weekly_goal.pomodoros,
    };
    let previous =
        day.checked_sub_signed(chrono::Duration::weeks(1))
            .and_then(|previous_week_day| {
                stats
                    .weekly_goal_snapshot_for_day(previous_week_day)
                    .map(|previous_target| {
                        let week = stats.weekly_for_day(previous_week_day);
                        (
                            previous_target,
                            week.focused_minutes(),
                            week.pomodoros_completed,
                        )
                    })
            });
    carry_over_goal_target(base, config.goal_carry_over.weekly, previous)
}

fn effective_monthly_goal_snapshot(
    config: &AppConfig,
    stats: &FocusStats,
    day: NaiveDate,
) -> DailyGoalSnapshot {
    let base = DailyGoalSnapshot {
        minutes: config.monthly_goal.minutes,
        pomodoros: config.monthly_goal.pomodoros,
    };
    let previous = previous_month_reference_day(day).and_then(|previous_month_day| {
        stats
            .monthly_goal_snapshot_for_day(previous_month_day)
            .map(|previous_target| {
                let month = stats.monthly_for_day(previous_month_day);
                (
                    previous_target,
                    month.focused_minutes(),
                    month.pomodoros_completed,
                )
            })
    });
    carry_over_goal_target(base, config.goal_carry_over.monthly, previous)
}

fn previous_month_reference_day(day: NaiveDate) -> Option<NaiveDate> {
    let month_start = NaiveDate::from_ymd_opt(day.year(), day.month(), 1)?;
    month_start.pred_opt()
}

fn build_live_status_output(
    config: &AppConfig,
    fallback_task_label: Option<String>,
) -> LiveStatusOutput {
    let custom = config.effective_custom_profile();
    let strict_mode_enabled = config
        .profile_automation_for(config.selected_profile)
        .strict_mode;
    let fallback_task_label = normalize_optional_task_label(fallback_task_label);
    match session_recovery::load() {
        Ok(Some(snapshot)) => {
            let pre_reconcile_in_progress = snapshot.status() != TimerStatus::Idle;
            let selected_profile = profile_view(snapshot.selected_profile, &custom);
            let timer_for_reconciliation = TimerState::with_profile(
                selected_profile.focus_secs,
                selected_profile.short_break_secs,
                selected_profile.long_break_secs,
                selected_profile.long_break_interval,
            );
            let snapshot = snapshot.reconcile_elapsed_for_timer(&timer_for_reconciliation);
            let phase = snapshot.phase();
            let status = snapshot.status();
            let in_progress = pre_reconcile_in_progress || status != TimerStatus::Idle;
            LiveStatusOutput {
                state_source: "recovery",
                recovery_error: None,
                in_progress,
                phase: timer_phase_id(phase),
                status: timer_status_id(status),
                remaining_secs: snapshot.remaining_secs,
                pomodoros_completed: snapshot.pomodoros_completed,
                selected_profile,
                selected_task_label: snapshot.normalized_task_label(),
                strict_mode_enforced: strict_mode_enabled
                    && phase == TimerPhase::Focus
                    && status != TimerStatus::Idle,
            }
        }
        Ok(None) => {
            let selected_profile = profile_view(config.selected_profile, &custom);
            LiveStatusOutput {
                state_source: "default",
                recovery_error: None,
                in_progress: false,
                phase: timer_phase_id(TimerPhase::Focus),
                status: timer_status_id(TimerStatus::Idle),
                remaining_secs: selected_profile.focus_secs,
                pomodoros_completed: 0,
                selected_profile,
                selected_task_label: fallback_task_label.clone(),
                strict_mode_enforced: false,
            }
        }
        Err(error) => {
            let selected_profile = profile_view(config.selected_profile, &custom);
            LiveStatusOutput {
                state_source: "default",
                recovery_error: Some(error),
                in_progress: false,
                phase: timer_phase_id(TimerPhase::Focus),
                status: timer_status_id(TimerStatus::Idle),
                remaining_secs: selected_profile.focus_secs,
                pomodoros_completed: 0,
                selected_profile,
                selected_task_label: fallback_task_label,
                strict_mode_enforced: false,
            }
        }
    }
}

pub(super) fn normalize_optional_task_label(task_label: Option<String>) -> Option<String> {
    task_label
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(super) fn profile_view(profile: ProfileId, custom: &CustomProfileConfig) -> ProfileView {
    let spec = resolve_profile_spec(profile, custom);
    ProfileView {
        id: profile_id(profile),
        label: profile.label(),
        focus_secs: spec.focus_secs,
        short_break_secs: spec.short_break_secs,
        long_break_secs: spec.long_break_secs,
        long_break_interval: spec.long_break_interval,
    }
}

fn resolve_profile_spec(profile: ProfileId, custom: &CustomProfileConfig) -> ProfileSpec {
    match profile {
        ProfileId::Classic => ProfileSpec {
            focus_secs: DEFAULT_FOCUS_SECS,
            short_break_secs: DEFAULT_SHORT_BREAK_SECS,
            long_break_secs: DEFAULT_LONG_BREAK_SECS,
            long_break_interval: DEFAULT_LONG_BREAK_INTERVAL,
        },
        ProfileId::DeepWork => ProfileSpec {
            focus_secs: 50 * 60,
            short_break_secs: 10 * 60,
            long_break_secs: 30 * 60,
            long_break_interval: 3,
        },
        ProfileId::Custom => {
            let custom = custom.normalized();
            ProfileSpec {
                focus_secs: custom.focus_secs,
                short_break_secs: custom.short_break_secs,
                long_break_secs: custom.long_break_secs,
                long_break_interval: custom.long_break_interval,
            }
        }
    }
}

pub(super) fn profile_id(profile: ProfileId) -> &'static str {
    match profile {
        ProfileId::Classic => "basic",
        ProfileId::DeepWork => "standard",
        ProfileId::Custom => "advanced",
    }
}

pub(super) fn theme_preset_view(preset: ThemePreset) -> ThemePresetView {
    ThemePresetView {
        id: preset.id(),
        label: preset.label(),
    }
}

pub(super) fn available_theme_preset_views() -> Vec<ThemePresetView> {
    [
        ThemePreset::Classic,
        ThemePreset::HighContrast,
        ThemePreset::DeuteranopiaFriendly,
    ]
    .into_iter()
    .map(theme_preset_view)
    .collect()
}

pub(super) fn timer_phase_id(phase: TimerPhase) -> &'static str {
    match phase {
        TimerPhase::Focus => "focus",
        TimerPhase::ShortBreak => "short-break",
        TimerPhase::LongBreak => "long-break",
    }
}

pub(super) fn timer_status_id(status: TimerStatus) -> &'static str {
    match status {
        TimerStatus::Idle => "idle",
        TimerStatus::Running => "running",
        TimerStatus::Paused => "paused",
    }
}
