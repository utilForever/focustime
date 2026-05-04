use crate::cli::{
    AppConfig, BreakTemplateConfig, BreakTemplateView, CustomProfileConfig, DEFAULT_FOCUS_SECS,
    DEFAULT_LONG_BREAK_INTERVAL, DEFAULT_LONG_BREAK_SECS, DEFAULT_SHORT_BREAK_SECS,
    DailyGoalSnapshot, Datelike, FocusScoreOutput, FocusStats, GoalOutput, LiveStatusOutput,
    NaiveDate, ProfileId, ProfileSpec, ProfileView, SessionOutput, StatusOutput, TaskGoalOutput,
    ThemePreset, ThemePresetView, TimerPhase, TimerStatus, TodayOutput, carry_over_goal_target,
    current_day_key, effective_blocked_sites_for_profile, session_recovery,
};

pub(super) fn build_status_output(config: &AppConfig, stats: &FocusStats) -> StatusOutput {
    let day = current_day_key();
    let day_date = NaiveDate::parse_from_str(&day, "%Y-%m-%d")
        .expect("current_day_key should always be a valid ISO date");
    let today = stats.daily_for(&day);
    let week = stats.weekly_for_day(day_date);
    let month = stats.monthly_for_day(day_date);
    let (_, selected_task_label) = stats.task_planner_state();
    let (selected_task_label, focus_intention, task_note) =
        mirror_metadata_from_task_label(selected_task_label);
    let goal_snapshot = effective_daily_goal_snapshot_for_day(config, stats, day_date);
    let weekly_goal_snapshot = effective_weekly_goal_snapshot(config, stats, day_date);
    let monthly_goal_snapshot = effective_monthly_goal_snapshot(config, stats, day_date);
    let selected_task_goal = selected_task_label
        .as_ref()
        .map(|label| build_task_goal_output(stats, label));
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

    StatusOutput {
        day,
        selected_profile: profile_view(config.selected_profile, &config.effective_custom_profile()),
        selected_break_template: selected_break_template_view(config),
        available_break_templates: available_break_template_views(config),
        selected_theme_preset: theme_preset_view(config.selected_theme_preset),
        selected_task_label,
        focus_intention,
        task_note,
        selected_blocklist_profile: config.selected_blocklist_profile.clone(),
        blocked_sites_count: active_sites_count,
        strict_mode: config.strict_mode,
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
        monthly_goal: GoalOutput {
            configured: monthly_goal_snapshot.has_any_target(),
            minutes_target: monthly_goal_snapshot.minutes,
            pomodoros_target: monthly_goal_snapshot.pomodoros,
            met: monthly_goal_snapshot
                .is_met_by_totals(month.focused_minutes(), month.pomodoros_completed),
            carry_over: config.goal_carry_over.monthly,
        },
        selected_task_goal,
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

pub(super) fn build_task_goal_output(stats: &FocusStats, label: &str) -> TaskGoalOutput {
    match stats.task_goal_progress_for_label(label) {
        Some(progress) => {
            let focused_minutes = progress.focused_minutes();
            TaskGoalOutput {
                task_label: progress.task_label,
                configured: progress.target.has_any_target(),
                minutes_target: progress.target.minutes,
                pomodoros_target: progress.target.pomodoros,
                focused_minutes,
                pomodoros_completed: progress.pomodoros_completed,
                met: progress.met,
            }
        }
        None => TaskGoalOutput {
            task_label: label.to_string(),
            configured: false,
            minutes_target: 0,
            pomodoros_target: 0,
            focused_minutes: 0,
            pomodoros_completed: 0,
            met: false,
        },
    }
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
    let (fallback_task_label, fallback_focus_intention, fallback_task_note) =
        mirror_metadata_from_task_label(fallback_task_label);
    match session_recovery::load() {
        Ok(Some(snapshot)) => {
            let phase = snapshot.phase();
            let status = snapshot.status();
            LiveStatusOutput {
                state_source: "recovery",
                recovery_error: None,
                in_progress: true,
                phase: timer_phase_id(phase),
                status: timer_status_id(status),
                remaining_secs: snapshot.remaining_secs,
                pomodoros_completed: snapshot.pomodoros_completed,
                selected_profile: profile_view(snapshot.selected_profile, &custom),
                selected_task_label: snapshot.normalized_task_label(),
                focus_intention: snapshot.normalized_focus_intention(),
                task_note: snapshot.normalized_task_note(),
                strict_mode_enforced: config.strict_mode
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
                focus_intention: fallback_focus_intention.clone(),
                task_note: fallback_task_note.clone(),
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
                focus_intention: fallback_focus_intention,
                task_note: fallback_task_note,
                strict_mode_enforced: false,
            }
        }
    }
}

pub(super) fn mirror_metadata_from_task_label(
    task_label: Option<String>,
) -> (Option<String>, Option<String>, Option<String>) {
    let task_label = task_label
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let focus_intention = task_label.clone();
    let task_note = task_label.clone();
    (task_label, focus_intention, task_note)
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
        ProfileId::Classic => "classic",
        ProfileId::DeepWork => "deep-work",
        ProfileId::Custom => "custom",
    }
}

fn break_template_view(template: &BreakTemplateConfig) -> BreakTemplateView {
    let template = template.normalized();
    BreakTemplateView {
        name: template.name,
        short_break_secs: template.short_break_secs,
        long_break_secs: template.long_break_secs,
        long_break_interval: template.long_break_interval,
    }
}

fn break_template_matches_custom_profile(
    template: &BreakTemplateConfig,
    custom_profile: &CustomProfileConfig,
) -> bool {
    let template = template.normalized();
    let custom_profile = custom_profile.normalized();
    template.short_break_secs == custom_profile.short_break_secs
        && template.long_break_secs == custom_profile.long_break_secs
        && template.long_break_interval == custom_profile.long_break_interval
}

fn selected_break_template_index(config: &AppConfig) -> Option<usize> {
    let custom_profile = config.effective_custom_profile();
    let selected_name = config.selected_break_template.trim();
    let selected_index = config
        .break_templates
        .iter()
        .position(|template| template.name.eq_ignore_ascii_case(selected_name));

    if let Some(index) = selected_index
        && config.break_templates.get(index).is_some_and(|template| {
            break_template_matches_custom_profile(template, &custom_profile)
        })
    {
        return Some(index);
    }

    config
        .break_templates
        .iter()
        .position(|template| break_template_matches_custom_profile(template, &custom_profile))
}

pub(super) fn selected_break_template_view(config: &AppConfig) -> BreakTemplateView {
    if let Some(index) = selected_break_template_index(config) {
        return config
            .break_templates
            .get(index)
            .map(break_template_view)
            .unwrap_or_else(|| break_template_view(&BreakTemplateConfig::default()));
    }

    let custom = config.effective_custom_profile();
    BreakTemplateView {
        name: "Custom".to_string(),
        short_break_secs: custom.short_break_secs,
        long_break_secs: custom.long_break_secs,
        long_break_interval: custom.long_break_interval,
    }
}

pub(super) fn available_break_template_views(config: &AppConfig) -> Vec<BreakTemplateView> {
    config
        .break_templates
        .iter()
        .map(break_template_view)
        .collect()
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
