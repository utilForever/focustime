use crate::cli::{
    FocusScoreOutput, GoalOutput, StatsGrowthSummary, StatsRetentionStatusOutput, StatusOutput,
    TimerStateOutput,
};

use super::format_duration;

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
        "Blocklist profile: {} ({} sites)",
        payload.selected_blocklist_profile, payload.blocked_sites_count
    );
    println!(
        "Strict mode: {}",
        if payload.strict_mode { "on" } else { "off" }
    );
    println!(
        "Today: {} focused minutes, {} pomodoros",
        payload.today.focused_minutes, payload.today.pomodoros_completed
    );
    print_status_goal_line("Daily goal", &payload.goal);
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
    if let Some(error) = payload.live.recovery_error.as_deref() {
        println!("Live timer warning: {error}");
    }
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

fn print_status_focus_score_line(focus_score: &FocusScoreOutput) {
    if focus_score.available {
        println!(
            "Focus score: {}% (consistency)",
            focus_score.focus_score_pct.unwrap_or(0),
        );
    } else {
        println!("Focus score: n/a");
    }
}

fn print_status_focus_risk_line(forecast: &crate::stats::FocusRiskForecast) {
    let alert_active = forecast.alert_active();
    let daily_label = forecast.daily_goal.period.short_label();
    let alert_suffix = if alert_active { " (alert)" } else { "" };
    println!(
        "Focus risk: {} {} {}% | Streak {} {}%{}",
        daily_label,
        forecast.daily_goal.risk_level.label(),
        forecast.daily_goal.risk_score_pct,
        forecast.streak.risk_level.label(),
        forecast.streak.risk_score_pct,
        alert_suffix
    );

    let mut highest_label = daily_label;
    let highest_score = forecast.daily_goal.risk_score_pct;
    let mut highest_signal = forecast.daily_goal.signals.first();
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
        "Stats retention windows: daily {}, sessions {}, interruptions {}",
        format_retention_window(retention.keep_daily_days),
        format_retention_window(retention.keep_focus_sessions_days),
        format_retention_window(retention.keep_session_interruptions_days),
    );
    if retention.pending_prune.any_removed() {
        println!(
            "Stats retention pending prune: {} records (daily {}, sessions {}, interruptions {})",
            retention.pending_prune.total_removed(),
            retention.pending_prune.daily_removed,
            retention.pending_prune.focus_sessions_removed,
            retention.pending_prune.session_interruptions_removed,
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
        "Profile: {} ({})",
        timer.selected_profile.label, timer.selected_profile.id
    );
}
