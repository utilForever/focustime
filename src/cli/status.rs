use crate::cli::{
    AppConfig, CustomProfileConfig, DEFAULT_FOCUS_SECS, DEFAULT_LONG_BREAK_INTERVAL,
    DEFAULT_LONG_BREAK_SECS, DEFAULT_SHORT_BREAK_SECS, DailyGoalSnapshot, FocusScoreOutput,
    FocusStats, GoalOutput, LiveStatusOutput, NaiveDate, ProfileId, ProfileSpec, ProfileView,
    SessionOutput, StatsRetentionStatusOutput, StatusOutput, ThemePreset, ThemePresetView,
    TimerPhase, TimerStatus, TodayOutput, carry_over_goal_target, current_day_key,
    effective_blocked_sites_for_profile, session_recovery,
};
use crate::timer::TimerState;

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn build_status_output(config: &AppConfig, stats: &FocusStats) -> StatusOutput {
    let day = current_day_key();
    let day_date = NaiveDate::parse_from_str(&day, "%Y-%m-%d")
        .expect("current_day_key should always be a valid ISO date");
    let today = stats.daily_for(&day);
    let (_, selected_task_label) = stats.task_planner_state();
    let selected_task_label = normalize_optional_task_label(selected_task_label);
    let goal_snapshot = effective_daily_goal_snapshot_for_day(config, stats, day_date);
    let active_sites_count = config
        .blocklist_profiles
        .first()
        .map(|profile| effective_blocked_sites_for_profile(profile).len())
        .unwrap_or_default();
    let selected_automation = config.profile_automation_for(config.selected_profile);
    let live = build_live_status_output(config, selected_task_label.clone());
    let session = build_session_output(&live);
    let latest_interruption = stats.latest_session_interruption();
    let consistency_score_pct = stats
        .weekly_focus_score_for_day(day_date)
        .consistency_score_pct;
    let completion_score_pct = None;
    let focus_score_pct = Some(consistency_score_pct);
    let focus_risk = stats.focus_risk_forecast_for_day(
        day_date,
        goal_snapshot,
        DailyGoalSnapshot::default(),
        DailyGoalSnapshot::default(),
    );
    let stats_growth = stats.growth_summary();
    let retention_windows = config.stats_retention.windows();
    let pending_prune = stats.retention_preview(config.stats_retention, day_date);

    StatusOutput {
        day,
        selected_profile: profile_view(config.selected_profile, &config.effective_custom_profile()),
        selected_theme_preset: theme_preset_view(config.selected_theme_preset),
        selected_task_label,
        selected_blocklist_profile: "Default".to_string(),
        blocked_sites_count: active_sites_count,
        strict_mode: selected_automation.strict_mode,
        goal: GoalOutput {
            configured: goal_snapshot.has_any_target(),
            minutes_target: goal_snapshot.minutes,
            pomodoros_target: goal_snapshot.pomodoros,
            met: goal_snapshot.is_met_by(today),
            carry_over: config.goal_carry_over.daily,
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
            pending_prune,
        },
        live,
    }
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
