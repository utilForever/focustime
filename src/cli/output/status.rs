use crate::cli::{
    BreakGlassCommandOutput, FocusScoreOutput, GoalOutput, ScheduleDelayCommandOutput,
    SessionMetadataCommandOutput, StatsGrowthSummary, StatsRetentionStatusOutput, StatusOutput,
    TaskGoalOutput, TimerStateOutput,
};

use super::{format_duration, format_expiry_clock_suffix};

pub(in crate::cli) fn print_session_metadata_command_output(
    payload: &SessionMetadataCommandOutput,
) {
    if payload.updated {
        println!("Session metadata updated: {}.", payload.action);
    } else {
        println!("Session metadata: {}.", payload.action);
    }
    println!(
        "Focus intention: {}",
        payload.focus_intention.as_deref().unwrap_or("none")
    );
    println!(
        "Task note: {}",
        payload.task_note.as_deref().unwrap_or("none")
    );
    print_timer_state_output(&payload.timer);
}

pub(in crate::cli) fn print_status_output(payload: &StatusOutput) {
    println!("Date: {}", payload.day);
    println!(
        "Selected profile: {} ({})",
        payload.selected_profile.label, payload.selected_profile.id
    );
    println!(
        "Selected theme preset: {} ({})",
        payload.selected_theme_preset.label, payload.selected_theme_preset.id
    );
    println!(
        "Task label: {}",
        payload.selected_task_label.as_deref().unwrap_or("none")
    );
    println!(
        "Focus intention: {}",
        payload.focus_intention.as_deref().unwrap_or("none")
    );
    println!(
        "Task note: {}",
        payload.task_note.as_deref().unwrap_or("none")
    );
    println!(
        "Blocklist profile: {} ({} sites)",
        payload.selected_blocklist_profile, payload.blocked_sites_count
    );
    print_temporary_overrides_status(payload);
    println!(
        "Strict mode: {}",
        if payload.strict_mode { "on" } else { "off" }
    );
    println!(
        "Today: {} focused minutes, {} pomodoros",
        payload.today.focused_minutes, payload.today.pomodoros_completed
    );
    print_status_goal_line("Daily goal", &payload.goal);
    print_status_goal_line("Weekly goal", &payload.weekly_goal);
    print_status_weekly_allocation_line(&payload.weekly_allocation);
    print_status_goal_line("Monthly goal", &payload.monthly_goal);
    print_status_task_goal_line(payload.selected_task_goal.as_ref());
    println!(
        "Session: {} focused minutes, {} pomodoros",
        payload.session.focused_minutes, payload.session.pomodoros_completed
    );
    if let Some(interruption) = payload.latest_interruption.as_ref() {
        println!(
            "Last interruption: {} ({}, {} remaining, task: {})",
            interruption.reason.label(),
            interruption.date,
            format_duration(interruption.remaining_secs),
            interruption.task_label.as_deref().unwrap_or("none")
        );
    } else {
        println!("Last interruption: none");
    }
    print_status_focus_score_line(&payload.focus_score);
    print_status_focus_risk_line(&payload.focus_risk);
    print_status_growth_line(&payload.stats_growth);
    print_status_retention_line(&payload.stats_retention);
    println!(
        "Live timer: {} {} ({} remaining, source: {})",
        payload.live.phase,
        payload.live.status,
        format_duration(payload.live.remaining_secs),
        payload.live.state_source
    );
    println!(
        "Live focus intention: {}",
        payload.live.focus_intention.as_deref().unwrap_or("none")
    );
    println!(
        "Live task note: {}",
        payload.live.task_note.as_deref().unwrap_or("none")
    );
    if let Some(error) = payload.live.recovery_error.as_deref() {
        println!("Live timer warning: {error}");
    }
}

fn print_temporary_overrides_status(payload: &StatusOutput) {
    if payload.temporary_overrides.is_empty() {
        println!("Temporary overrides: off");
    } else {
        println!(
            "Temporary overrides: {} active",
            payload.temporary_overrides_active_count
        );
        for entry in &payload.temporary_overrides {
            match (
                entry.kind,
                entry.site.as_deref(),
                entry.remaining_secs,
                entry.expires_at_epoch_secs,
                entry.pending_confirmation,
            ) {
                ("break-glass", _, _, _, true) => {
                    println!("  - break-glass (pending confirmation)");
                }
                ("break-glass", _, Some(remaining_secs), Some(epoch_secs), false) => {
                    println!(
                        "  - break-glass (expires in {}{})",
                        format_duration(remaining_secs),
                        format_expiry_clock_suffix(epoch_secs)
                    );
                }
                ("allowlist-site", Some(site), Some(remaining_secs), Some(epoch_secs), false) => {
                    println!(
                        "  - allowlist site {} (expires in {}{})",
                        site,
                        format_duration(remaining_secs),
                        format_expiry_clock_suffix(epoch_secs)
                    );
                }
                _ => {}
            }
        }
    }
}

fn print_status_weekly_allocation_line(allocation: &crate::cli::WeeklyAllocationOutput) {
    if !allocation.available {
        println!("Weekly allocation: off (weekly goal off)");
        return;
    }

    let strategy = if allocation.uses_schedule_weights {
        "schedule-weighted"
    } else {
        "equal-split fallback"
    };
    println!(
        "Weekly allocation: today {} min, {} pomodoros | remaining {} min, {} pomodoros across {}/{} days ({strategy})",
        allocation.today_minutes_target,
        allocation.today_pomodoros_target,
        allocation.remaining_minutes,
        allocation.remaining_pomodoros,
        allocation.allocatable_days,
        allocation.remaining_days_in_week,
    );

    let day_breakdown = allocation
        .days
        .iter()
        .map(|day| {
            let marker = if day.allocatable { "" } else { "*" };
            format!(
                "{}={}m/{}p{}",
                day.date, day.minutes_target, day.pomodoros_target, marker
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    println!("Weekly allocation days: {day_breakdown}");
}

fn print_status_goal_line(label: &str, goal: &GoalOutput) {
    if goal.configured {
        println!(
            "{label}: {} min, {} pomodoros ({}, carry-over: {})",
            goal.minutes_target,
            goal.pomodoros_target,
            if goal.met { "met" } else { "in progress" },
            if goal.carry_over { "on" } else { "off" }
        );
    } else {
        println!(
            "{label}: off (carry-over: {})",
            if goal.carry_over { "on" } else { "off" }
        );
    }
}

fn print_status_task_goal_line(task_goal: Option<&TaskGoalOutput>) {
    let Some(task_goal) = task_goal else {
        println!("Selected task goal: none");
        return;
    };

    if task_goal.configured {
        println!(
            "Selected task goal (`{}`): {} min, {} pomodoros ({})",
            task_goal.task_label,
            task_goal.minutes_target,
            task_goal.pomodoros_target,
            if task_goal.met { "met" } else { "in progress" }
        );
    } else {
        println!("Selected task goal (`{}`): off", task_goal.task_label);
    }
    println!(
        "Selected task progress (`{}`): {} min, {} pomodoros",
        task_goal.task_label, task_goal.focused_minutes, task_goal.pomodoros_completed
    );
}

fn print_status_focus_score_line(focus_score: &FocusScoreOutput) {
    if focus_score.available {
        println!(
            "Focus score: {}% (consistency {}%, completion {}%)",
            focus_score.focus_score_pct.unwrap_or(0),
            focus_score.consistency_score_pct,
            focus_score.completion_score_pct.unwrap_or(0)
        );
    } else {
        println!(
            "Focus score: n/a (weekly goal off; consistency {}%)",
            focus_score.consistency_score_pct
        );
    }
}

fn print_status_focus_risk_line(forecast: &crate::stats::FocusRiskForecast) {
    let alert_active = forecast.alert_active();
    let daily_label = forecast.daily_goal.period.short_label();
    let weekly_label = forecast.weekly_goal.period.short_label();
    let monthly_label = forecast.monthly_goal.period.short_label();
    let alert_suffix = if alert_active { " (alert)" } else { "" };
    println!(
        "Focus risk: {} {} {}% | {} {} {}% | {} {} {}% | Streak {} {}%{}",
        daily_label,
        forecast.daily_goal.risk_level.label(),
        forecast.daily_goal.risk_score_pct,
        weekly_label,
        forecast.weekly_goal.risk_level.label(),
        forecast.weekly_goal.risk_score_pct,
        monthly_label,
        forecast.monthly_goal.risk_level.label(),
        forecast.monthly_goal.risk_score_pct,
        forecast.streak.risk_level.label(),
        forecast.streak.risk_score_pct,
        alert_suffix
    );

    let mut highest_label = daily_label;
    let mut highest_score = forecast.daily_goal.risk_score_pct;
    let mut highest_signal = forecast.daily_goal.signals.first();
    if forecast.weekly_goal.risk_score_pct > highest_score {
        highest_label = weekly_label;
        highest_score = forecast.weekly_goal.risk_score_pct;
        highest_signal = forecast.weekly_goal.signals.first();
    }
    if forecast.monthly_goal.risk_score_pct > highest_score {
        highest_label = monthly_label;
        highest_score = forecast.monthly_goal.risk_score_pct;
        highest_signal = forecast.monthly_goal.signals.first();
    }
    if forecast.streak.risk_score_pct > highest_score {
        highest_label = "Streak";
        highest_signal = forecast.streak.signals.first();
    }
    if alert_active && let Some(signal) = highest_signal {
        println!(
            "Focus risk signal: {highest_label} {} ({})",
            signal.label, signal.value
        );
    }
}

fn print_status_growth_line(growth: &StatsGrowthSummary) {
    println!(
        "Stats growth: {} records, ~{}",
        growth.total_record_count,
        format_bytes(growth.estimated_bytes)
    );
    if growth.high_volume_sections.is_empty() {
        println!("Stats growth top sections: none");
        return;
    }
    let high_volume = growth
        .high_volume_sections
        .iter()
        .map(|section| {
            format!(
                "{} ({}, ~{})",
                section.name,
                section.record_count,
                format_bytes(section.estimated_bytes)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    println!("Stats growth top sections: {high_volume}");
}

fn print_status_retention_line(retention: &StatsRetentionStatusOutput) {
    println!("Stats retention preset: {}", retention.preset);
    println!(
        "Stats retention windows: daily {}, sessions {}, interruptions {}, overrides {}",
        format_retention_window(retention.keep_daily_days),
        format_retention_window(retention.keep_focus_sessions_days),
        format_retention_window(retention.keep_session_interruptions_days),
        format_retention_window(retention.keep_break_glass_overrides_days),
    );
    if retention.pending_prune.any_removed() {
        println!(
            "Stats retention pending prune: {} records (daily {}, sessions {}, interruptions {}, overrides {})",
            retention.pending_prune.total_removed(),
            retention.pending_prune.daily_removed,
            retention.pending_prune.focus_sessions_removed,
            retention.pending_prune.session_interruptions_removed,
            retention.pending_prune.break_glass_overrides_removed
        );
    } else {
        println!("Stats retention pending prune: none");
    }
}

fn format_retention_window(days: Option<u16>) -> String {
    match days {
        Some(days) => format!("{days}d"),
        None => "keep_all".to_string(),
    }
}

fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

pub(in crate::cli) fn print_timer_state_output(timer: &TimerStateOutput) {
    println!(
        "Timer: {} {} ({} remaining)",
        timer.phase,
        timer.status,
        format_duration(timer.remaining_secs)
    );
    println!("Pomodoros completed: {}", timer.pomodoros_completed);
    println!(
        "Task label: {}",
        timer.selected_task_label.as_deref().unwrap_or("none")
    );
    println!(
        "Focus intention: {}",
        timer.focus_intention.as_deref().unwrap_or("none")
    );
    println!(
        "Task note: {}",
        timer.task_note.as_deref().unwrap_or("none")
    );
    println!(
        "Profile: {} ({})",
        timer.selected_profile.label, timer.selected_profile.id
    );
}

pub(in crate::cli) fn print_schedule_delay_command_output(payload: &ScheduleDelayCommandOutput) {
    println!(
        "Schedule delay applied: next scheduled start deferred until {}.",
        payload.delayed_until
    );
    print_timer_state_output(&payload.timer);
}

pub(in crate::cli) fn print_break_glass_command_output(payload: &BreakGlassCommandOutput) {
    if payload.pending_confirmation {
        println!("Break-glass armed. Run `--break-glass-trigger` again to confirm.");
    } else if payload.active {
        if let Some(remaining_secs) = payload.remaining_secs {
            println!(
                "Break-glass active: blocking paused for {} more.",
                format_duration(remaining_secs)
            );
        } else {
            println!("Break-glass active: blocking is currently paused.");
        }
    } else if payload.action == "break-glass-cancel" {
        println!("Break-glass confirmation canceled.");
    } else {
        println!("Break-glass state unchanged.");
    }
    print_timer_state_output(&payload.timer);
}
