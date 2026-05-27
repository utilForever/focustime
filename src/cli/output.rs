use crate::cli::{
    AutomationTriggersCommandOutput, BackupOutput, BlockingPreviewAction,
    BlockingPreviewCommandOutput, BlocklistCategoryCommandOutput, BlocklistProfileCommandOutput,
    BlocklistProfileConfig, BreakGlassCommandOutput, CalendarSyncCommandOutput,
    DaemonStartCommandOutput, DaemonStatusCommandOutput, DaemonStopCommandOutput,
    DiagnosticsCommandOutput, ExportOutput, FocusScoreOutput, GoalCarryCommandOutput,
    GoalCommandOutput, GoalOutput, HistoryDashboardCommandOutput, ProfileOutput,
    RecurringScheduleConfig, RestoreOutput, ScheduleCommandOutput, ScheduleDelayCommandOutput,
    ScheduleInspectionOutput, Serialize, SessionMetadataCommandOutput,
    SessionTemplateCommandOutput, SetupCheck, SetupCheckLevel, SetupCheckOutput, SetupDiagnostics,
    SiteAddCommandOutput, SiteDeleteCommandOutput, SiteEditCommandOutput, SiteListCommandOutput,
    StatsGrowthSummary, StatsRetentionStatusOutput, StatusComparisonOutput, StatusOutput,
    StrictCommandOutput, SyncBackupOutput, SyncRestoreOutput, TaskGoalCommandOutput,
    TaskGoalOutput, TemporarySiteAddCommandOutput, ThemeCommandOutput, TimerStateOutput,
    WeekdayRulesCommandOutput, Write, format_schedule_conflict,
    inspect_schedule_conflicts_from_config, io,
};
use chrono::{Local, TimeZone};

pub(super) fn print_profile_output(payload: &ProfileOutput) {
    if payload.updated {
        println!("Active profile updated.");
    }
    println!(
        "Selected profile: {} ({})",
        payload.selected.label, payload.selected.id
    );
    println!(
        "Durations: focus {}, short break {}, long break {}, cadence every {} focus",
        format_duration(payload.selected.focus_secs),
        format_duration(payload.selected.short_break_secs),
        format_duration(payload.selected.long_break_secs),
        payload.selected.long_break_interval
    );
    println!(
        "Selected break template: {} ({}/{}, every {} focus)",
        payload.selected_break_template.name,
        format_duration(payload.selected_break_template.short_break_secs),
        format_duration(payload.selected_break_template.long_break_secs),
        payload.selected_break_template.long_break_interval
    );
    println!(
        "Selected theme preset: {} ({})",
        payload.selected_theme_preset.label, payload.selected_theme_preset.id
    );
    println!("Available break templates:");
    for template in &payload.available_break_templates {
        println!(
            "  - {}: {}/{}, every {} focus",
            template.name,
            format_duration(template.short_break_secs),
            format_duration(template.long_break_secs),
            template.long_break_interval
        );
    }
    println!("Available profiles:");
    for profile in &payload.available {
        println!(
            "  - {} ({}): {}/{}/{} every {} focus",
            profile.label,
            profile.id,
            format_duration(profile.focus_secs),
            format_duration(profile.short_break_secs),
            format_duration(profile.long_break_secs),
            profile.long_break_interval
        );
    }
    println!("Available theme presets:");
    for preset in &payload.available_theme_presets {
        println!("  - {} ({})", preset.label, preset.id);
    }
}

pub(super) fn print_theme_command_output(payload: &ThemeCommandOutput) {
    if payload.updated {
        println!("Theme preset updated.");
    }
    println!(
        "Selected theme preset: {} ({})",
        payload.selected_theme_preset.label, payload.selected_theme_preset.id
    );
    println!("Available theme presets:");
    for preset in &payload.available_theme_presets {
        println!("  - {} ({})", preset.label, preset.id);
    }
}

pub(super) fn print_blocklist_profile_command_output(payload: &BlocklistProfileCommandOutput) {
    if payload.updated {
        println!("Blocklist profile updated.");
    }
    println!(
        "Selected blocklist profile: {}",
        payload.selected_blocklist_profile
    );
    if payload.profiles.is_empty() {
        println!("Profiles: none");
        return;
    }
    println!("Profiles:");
    for profile in &payload.profiles {
        let marker = if profile.active { "*" } else { " " };
        println!(
            "  {marker} {} (blocklist {}, allowlist {}, effective {})",
            profile.name,
            profile.blocklist_sites_count,
            profile.allowlist_sites_count,
            profile.effective_blocked_sites_count
        );
    }
}

pub(super) fn print_blocklist_category_command_output(payload: &BlocklistCategoryCommandOutput) {
    if payload.updated {
        println!("Blocklist category updated.");
    }
    println!(
        "Selected blocklist profile/category: {} / {}",
        payload.selected_blocklist_profile, payload.selected_blocklist_category
    );
    if payload.categories.is_empty() {
        println!("Categories: none");
        return;
    }
    println!("Categories:");
    for category in &payload.categories {
        let marker = if category.active { "*" } else { " " };
        println!(
            "  {marker} {} (blocklist {}, allowlist {})",
            category.name, category.blocklist_sites_count, category.allowlist_sites_count
        );
    }
}

pub(super) fn print_session_template_command_output(payload: &SessionTemplateCommandOutput) {
    if payload.updated {
        println!("Session template updated.");
    }
    println!(
        "Selected session template: {}",
        payload
            .selected_session_template
            .as_deref()
            .unwrap_or("none")
    );
    if payload.templates.is_empty() {
        println!("Templates: none");
        return;
    }
    println!("Templates:");
    for template in &payload.templates {
        let marker = if template.active { "*" } else { " " };
        println!(
            "  {marker} {} (task `{}`, profile {}, blocklist `{}`, windows {})",
            template.name,
            template.task_label,
            template.profile,
            template.blocklist_profile,
            template.schedule_windows_count
        );
    }
}

pub(super) fn print_history_dashboard_command_output(payload: &HistoryDashboardCommandOutput) {
    if payload.updated {
        println!("History dashboard updated.");
    }
    println!("Card order: {}", payload.card_order.join(", "));
    println!("Pinned cards: {}", payload.pinned_cards.join(", "));
    if payload.cards.is_empty() {
        println!("Cards: none");
        return;
    }
    println!("Cards:");
    for card in &payload.cards {
        let marker = if card.pinned { "*" } else { " " };
        println!("  {marker} {} ({})", card.label, card.id);
    }
}

pub(super) fn print_site_list_command_output(payload: &SiteListCommandOutput) {
    println!(
        "Active profile/category `{}` / `{}` {} entries: {}",
        payload.profile,
        payload.category,
        payload.target.id(),
        payload.sites.len()
    );
    for site in &payload.sites {
        println!("  - {site}");
    }
    println!(
        "Effective blocked sites: {}",
        payload.effective_blocked_sites_count
    );
}

pub(super) fn print_site_add_command_output(payload: &SiteAddCommandOutput) {
    if payload.updated {
        println!(
            "Added {} hostname(s) to {} in profile/category `{}` / `{}`.",
            payload.added.len(),
            payload.target.id(),
            payload.profile,
            payload.category
        );
    } else {
        println!(
            "No {} hostnames were added in profile/category `{}` / `{}`.",
            payload.target.id(),
            payload.profile,
            payload.category
        );
    }
    if !payload.duplicates.is_empty() {
        println!("Skipped duplicates: {}", payload.duplicates.join(", "));
    }
    if !payload.invalid.is_empty() {
        println!("Rejected invalid hostnames:");
        for invalid in &payload.invalid {
            println!(
                "  - {} ({})",
                display_input_value(&invalid.input),
                invalid.reason
            );
        }
    }
    println!(
        "{} entries now: {}",
        payload.target.id(),
        payload.sites.join(", ")
    );
    println!(
        "Effective blocked sites: {}",
        payload.effective_blocked_sites_count
    );
}

pub(super) fn print_temporary_site_add_command_output(payload: &TemporarySiteAddCommandOutput) {
    if payload.updated {
        println!(
            "Temporary allowlist updated in profile `{}`: added {}, refreshed {}.",
            payload.profile, payload.added, payload.refreshed
        );
    } else {
        println!(
            "No temporary allowlist changes were applied in profile `{}`.",
            payload.profile
        );
    }
    if payload.active.is_empty() {
        println!("Active temporary exceptions: none");
        return;
    }
    println!("Active temporary exceptions ({}):", payload.active.len());
    for entry in &payload.active {
        println!(
            "  - {} (expires in {})",
            entry.site,
            format_duration(entry.remaining_secs)
        );
    }
}

pub(super) fn print_site_edit_command_output(payload: &SiteEditCommandOutput) {
    if payload.updated {
        println!(
            "Updated {} hostname in profile/category `{}` / `{}`: {} -> {}",
            payload.target.id(),
            payload.profile,
            payload.category,
            payload.previous,
            payload.current
        );
    } else {
        println!(
            "No change for {} hostname `{}` in profile/category `{}` / `{}`.",
            payload.target.id(),
            payload.current,
            payload.profile,
            payload.category
        );
    }
    println!(
        "{} entries now: {}",
        payload.target.id(),
        payload.sites.join(", ")
    );
    println!(
        "Effective blocked sites: {}",
        payload.effective_blocked_sites_count
    );
}

pub(super) fn print_site_delete_command_output(payload: &SiteDeleteCommandOutput) {
    println!(
        "Deleted {} hostname `{}` from profile/category `{}` / `{}`.",
        payload.target.id(),
        payload.removed,
        payload.profile,
        payload.category
    );
    println!(
        "{} entries now: {}",
        payload.target.id(),
        payload.sites.join(", ")
    );
    println!(
        "Effective blocked sites: {}",
        payload.effective_blocked_sites_count
    );
}

pub(super) fn print_session_metadata_command_output(payload: &SessionMetadataCommandOutput) {
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

pub(super) fn print_daemon_start_command_output(payload: &DaemonStartCommandOutput) {
    if payload.already_running {
        println!("Daemon already running.");
    } else {
        println!("Daemon started.");
    }
    println!("PID: {}", payload.daemon.pid);
    println!("Address: {}:{}", payload.daemon.host, payload.daemon.port);
    println!(
        "Started at epoch seconds: {}",
        payload.daemon.started_at_epoch_secs
    );
}

pub(super) fn print_daemon_status_command_output(payload: &DaemonStatusCommandOutput) {
    println!("Daemon running: {}", payload.running);
    if let Some(daemon) = &payload.daemon {
        println!("PID: {}", daemon.pid);
        println!("Address: {}:{}", daemon.host, daemon.port);
        println!("Started at epoch seconds: {}", daemon.started_at_epoch_secs);
    } else {
        println!("Daemon metadata: unavailable");
    }
}

pub(super) fn print_daemon_stop_command_output(payload: &DaemonStopCommandOutput) {
    if !payload.was_running {
        println!("No running daemon found.");
        return;
    }
    if payload.stopped {
        println!("Daemon stopped.");
    } else {
        println!("Stop signal sent, but daemon shutdown was not confirmed.");
    }
    if let Some(daemon) = &payload.daemon {
        println!("PID: {}", daemon.pid);
        println!("Address: {}:{}", daemon.host, daemon.port);
    }
}

pub(super) fn print_status_output(payload: &StatusOutput) {
    println!("Date: {}", payload.day);
    println!(
        "Selected profile: {} ({})",
        payload.selected_profile.label, payload.selected_profile.id
    );
    println!(
        "Selected break template: {} ({}/{}, every {} focus)",
        payload.selected_break_template.name,
        format_duration(payload.selected_break_template.short_break_secs),
        format_duration(payload.selected_break_template.long_break_secs),
        payload.selected_break_template.long_break_interval
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
    if payload.temporary_allowlist_active.is_empty() {
        println!("Temporary allowlist: off");
    } else {
        let next_expiry_text = match (
            payload.temporary_allowlist_next_expiry_remaining_secs,
            payload.temporary_allowlist_next_expiry_epoch_secs,
        ) {
            (Some(remaining_secs), Some(epoch_secs)) => format!(
                " (next expiry in {}{})",
                format_duration(remaining_secs),
                format_expiry_clock_suffix(epoch_secs)
            ),
            (Some(remaining_secs), None) => {
                format!(" (next expiry in {})", format_duration(remaining_secs))
            }
            (None, Some(epoch_secs)) => {
                format!(" (next expiry{})", format_expiry_clock_suffix(epoch_secs))
            }
            (None, None) => String::new(),
        };
        println!(
            "Temporary allowlist: {} active{}",
            payload.temporary_allowlist_active_count, next_expiry_text
        );
        println!("Active temporary exceptions:");
        for entry in &payload.temporary_allowlist_active {
            println!(
                "  - {} (expires in {}{})",
                entry.site,
                format_duration(entry.remaining_secs),
                format_expiry_clock_suffix(entry.expires_at_epoch_secs)
            );
        }
    }
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
    print_status_comparison_line(&payload.comparison);
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

fn print_status_comparison_line(comparison: &StatusComparisonOutput) {
    let task_filter = comparison.task_filter.as_deref().unwrap_or("all");
    let profile_filter = comparison
        .profile_filter
        .map(|profile| profile.label().to_string())
        .unwrap_or_else(|| "All".to_string());
    let time_filter = comparison
        .time_of_day_filter
        .map(|bucket| bucket.label().to_string())
        .unwrap_or_else(|| "All".to_string());
    println!(
        "Comparison: {} | task {} | profile {} | time {} | limit {}",
        comparison.dimension.id(),
        task_filter,
        profile_filter,
        time_filter,
        comparison.limit
    );
    if comparison.rows.is_empty() {
        println!("Comparison rows: none");
        return;
    }
    for row in &comparison.rows {
        println!(
            "  - {}: {} min, {} sessions, {}% share",
            row.label,
            row.focused_minutes(),
            row.sessions_completed,
            row.focus_share_pct
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
    if alert_active {
        if let Some(signal) = highest_signal {
            println!(
                "Focus risk signal: {highest_label} {} ({})",
                signal.label, signal.value
            );
        }
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

pub(super) fn print_timer_state_output(timer: &TimerStateOutput) {
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

pub(super) fn print_schedule_delay_command_output(payload: &ScheduleDelayCommandOutput) {
    println!(
        "Schedule delay applied: next scheduled start deferred until {}.",
        payload.delayed_until
    );
    print_timer_state_output(&payload.timer);
}

pub(super) fn print_break_glass_command_output(payload: &BreakGlassCommandOutput) {
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

pub(super) fn print_export_output(payload: &ExportOutput) {
    println!("Exported stats to {}", payload.export_dir.display());
    println!("JSON: {}", payload.json_path.display());
    println!("CSV: {}", payload.csv_path.display());
}

pub(super) fn print_backup_output(payload: &BackupOutput) {
    println!("Backed up app data to {}", payload.backup_dir.display());
    println!("Config: {}", payload.config_backup_path.display());
    println!("Stats: {}", payload.stats_backup_path.display());
}

pub(super) fn print_restore_output(payload: &RestoreOutput) {
    println!("Restored app data from {}", payload.restore_dir.display());
    println!("Config: {}", payload.config_restored_path.display());
    println!("Stats: {}", payload.stats_restored_path.display());
}

pub(super) fn print_sync_backup_output(payload: &SyncBackupOutput) {
    println!(
        "Created encrypted sync snapshot in {}",
        payload.bundle_dir.display()
    );
    println!("Bundle: {}", payload.bundle_path.display());
    println!("Snapshot: {}", payload.snapshot_id);
    if let Some(base_snapshot_id) = payload.base_snapshot_id.as_deref() {
        println!("Base snapshot: {base_snapshot_id}");
    } else {
        println!("Base snapshot: none");
    }
    println!("Device ID: {}", payload.device_id);
}

pub(super) fn print_sync_restore_output(payload: &SyncRestoreOutput) {
    println!(
        "Restored encrypted sync snapshot from {}",
        payload.restore_dir.display()
    );
    println!("Bundle: {}", payload.bundle_path.display());
    println!("Snapshot: {}", payload.snapshot_id);
    if let Some(base_snapshot_id) = payload.base_snapshot_id.as_deref() {
        println!("Base snapshot: {base_snapshot_id}");
    } else {
        println!("Base snapshot: none");
    }
    println!("Source device ID: {}", payload.source_device_id);
    println!("Config: {}", payload.config_restored_path.display());
    println!("Stats: {}", payload.stats_restored_path.display());
}

pub(super) fn print_calendar_sync_command_output(payload: &CalendarSyncCommandOutput) {
    println!("Calendar sync refreshed.");
    println!("Sources: {}", payload.source_count);
    println!("Busy windows: {}", payload.windows_count);
    println!("Source errors: {}", payload.error_count);
    if !payload.errors.is_empty() {
        println!("Errors:");
        for error in &payload.errors {
            println!("  - {error}");
        }
    }
}

pub(super) fn print_goal_command_output(label: &str, payload: &GoalCommandOutput) {
    if payload.updated {
        println!("{label} goal updated.");
    }
    if payload.configured {
        println!(
            "{label} goal: {} min, {} pomodoros",
            payload.minutes_target, payload.pomodoros_target
        );
    } else {
        println!("{label} goal: off");
    }
}

pub(super) fn print_task_goal_command_output(payload: &TaskGoalCommandOutput) {
    if payload.updated {
        println!("Task goal updated for `{}`.", payload.task_label);
    }
    if payload.configured {
        println!(
            "Task goal (`{}`): {} min, {} pomodoros ({})",
            payload.task_label,
            payload.minutes_target,
            payload.pomodoros_target,
            if payload.met { "met" } else { "in progress" }
        );
    } else {
        println!("Task goal (`{}`): off", payload.task_label);
    }
    println!(
        "Task progress (`{}`): {} min, {} pomodoros",
        payload.task_label, payload.focused_minutes, payload.pomodoros_completed
    );
}

pub(super) fn print_goal_carry_command_output(label: &str, payload: &GoalCarryCommandOutput) {
    if payload.updated {
        println!("{label} goal carry-over updated.");
    }
    println!(
        "{label} goal carry-over: {}",
        if payload.carry_over { "on" } else { "off" }
    );
}

pub(super) fn print_strict_command_output(payload: &StrictCommandOutput) {
    if payload.updated {
        println!("Strict mode updated.");
    }
    println!(
        "Strict mode: {}",
        if payload.strict_mode { "on" } else { "off" }
    );
}

pub(super) fn print_schedule_command_output(payload: &ScheduleCommandOutput) {
    if payload.updated {
        println!("Schedule updated.");
    }
    if payload.schedule.windows.is_empty() {
        println!("Recurring windows: none");
    } else {
        println!("Recurring windows:");
        for window in &payload.schedule.windows {
            println!(
                "  - [{}] {}-{}",
                window.days.join(","),
                window.start,
                window.end
            );
        }
    }
    if payload.schedule.exception_dates.is_empty() {
        println!("Exception dates: none");
    } else {
        println!(
            "Exception dates: {}",
            payload.schedule.exception_dates.join(", ")
        );
    }
    if payload.schedule.one_time_windows.is_empty() {
        println!("One-time windows: none");
    } else {
        println!("One-time windows:");
        for window in &payload.schedule.one_time_windows {
            println!("  - {} {}-{}", window.date, window.start, window.end);
        }
    }
    if payload.inspection.conflicts.is_empty() {
        println!("Schedule conflicts: none");
    } else {
        println!(
            "Schedule conflicts: {} detected",
            payload.inspection.conflict_count
        );
        for conflict in &payload.inspection.conflicts {
            println!("  - {conflict}");
        }
    }
}

pub(super) fn print_weekday_rules_command_output(payload: &WeekdayRulesCommandOutput) {
    if payload.updated {
        println!("Weekday rules updated.");
    }
    if payload.rules.is_empty() {
        println!("Weekday rules: none");
        return;
    }
    println!("Weekday rules:");
    for rule in &payload.rules {
        let template = rule.session_template.as_deref().unwrap_or("none");
        println!(
            "  - {} -> profile {}, blocklist {}, template {}",
            rule.day,
            rule.profile.label(),
            rule.blocklist_profile,
            template
        );
    }
}

pub(super) fn print_automation_triggers_command_output(payload: &AutomationTriggersCommandOutput) {
    if payload.updated {
        println!("Automation triggers updated.");
    }
    if payload.rules.is_empty() {
        println!("Automation triggers: none");
        return;
    }
    println!("Automation triggers:");
    for rule in &payload.rules {
        let formatted = serde_json::to_string(rule)
            .unwrap_or_else(|_| "<failed to serialize automation trigger>".to_string());
        println!("  - {formatted}");
    }
}

pub(super) fn build_schedule_inspection_output(
    schedule: &RecurringScheduleConfig,
) -> ScheduleInspectionOutput {
    let conflicts = inspect_schedule_conflicts_from_config(schedule)
        .into_iter()
        .map(|conflict| format_schedule_conflict(&conflict))
        .collect::<Vec<_>>();
    ScheduleInspectionOutput {
        conflict_count: conflicts.len(),
        conflicts,
    }
}

pub(super) fn print_diagnostics_command_output(payload: &DiagnosticsCommandOutput) {
    println!("Hosts file: {}", payload.hosts_file_path);
    println!(
        "Backend policy: {} (order: {})",
        payload.backend_policy, payload.backend_order
    );
    print_diagnostics_check("Backend selection", &payload.backend_selection);
    print_diagnostics_check("Command backend", &payload.command_backend);
    print_diagnostics_check("Blocking permissions", &payload.blocking_permissions);
    print_diagnostics_check("Hosts write capability", &payload.hosts_write_capability);
    print_diagnostics_check("WakaTime config", &payload.wakatime_config);
    print_diagnostics_check("Encrypted sync", &payload.sync_status);
    println!(
        "Encrypted sync device ID: {}",
        payload.sync_device_id.as_deref().unwrap_or("none")
    );
    println!(
        "Encrypted sync last snapshot: {}",
        payload.sync_last_snapshot_id.as_deref().unwrap_or("none")
    );
    if let Some(last_success_epoch_secs) = payload.sync_last_success_epoch_secs {
        println!("Encrypted sync last success (epoch): {last_success_epoch_secs}");
    } else {
        println!("Encrypted sync last success (epoch): none");
    }
    if let Some(last_error) = payload.sync_last_error.as_deref() {
        println!("Encrypted sync last error: {last_error}");
    } else {
        println!("Encrypted sync last error: none");
    }
    if let Some(last_error_epoch_secs) = payload.sync_last_error_epoch_secs {
        println!("Encrypted sync last error (epoch): {last_error_epoch_secs}");
    } else {
        println!("Encrypted sync last error (epoch): none");
    }
    if payload.deprecation_warnings.is_empty() {
        println!("Deprecation warnings: none");
    } else {
        println!("Deprecation warnings:");
        for warning in &payload.deprecation_warnings {
            println!("  - {warning}");
        }
    }
}

fn print_diagnostics_check(label: &str, check: &SetupCheckOutput) {
    println!("{label}: {} ({})", check.message, check.level);
}

pub(super) fn build_diagnostics_command_output(
    diagnostics: &SetupDiagnostics,
) -> DiagnosticsCommandOutput {
    DiagnosticsCommandOutput {
        hosts_file_path: diagnostics.hosts_file_path.clone(),
        backend_policy: diagnostics.backend_policy.clone(),
        backend_order: diagnostics.backend_order.clone(),
        backend_selection: setup_check_output(&diagnostics.backend_selection),
        command_backend: setup_check_output(&diagnostics.command_backend),
        blocking_permissions: setup_check_output(&diagnostics.blocking_permissions),
        hosts_write_capability: setup_check_output(&diagnostics.hosts_write_capability),
        wakatime_config: setup_check_output(&diagnostics.wakatime_config),
        sync_status: setup_check_output(&diagnostics.sync_status),
        sync_device_id: diagnostics.sync_device_id.clone(),
        sync_last_snapshot_id: diagnostics.sync_last_snapshot_id.clone(),
        sync_last_success_epoch_secs: diagnostics.sync_last_success_epoch_secs,
        sync_last_error: diagnostics.sync_last_error.clone(),
        sync_last_error_epoch_secs: diagnostics.sync_last_error_epoch_secs,
        deprecation_warnings: diagnostics.deprecation_warnings.clone(),
    }
}

pub(super) fn print_blocking_preview_command_output(payload: &BlockingPreviewCommandOutput) {
    println!(
        "Backend: {} (target: {})",
        payload.backend, payload.backend_target
    );
    if !payload.attempted_backends.is_empty() {
        println!(
            "Attempted backends: {}",
            payload.attempted_backends.join(" -> ")
        );
    }
    println!(
        "Fallback used: {}",
        if payload.fallback_used { "yes" } else { "no" }
    );
    println!("Hosts file: {}", payload.hosts_file_path);
    println!(
        "Preview action: {} (changes: {})",
        payload.action,
        if payload.would_change { "yes" } else { "no" }
    );
    println!(
        "Effective blocked sites: {}",
        payload.effective_blocked_sites_count
    );
    if !payload.effective_blocked_sites.is_empty() {
        println!("Sites: {}", payload.effective_blocked_sites.join(", "));
    }
    if let Some(section) = payload.section.as_deref() {
        println!("Section preview:");
        print!("{section}");
    } else {
        println!("Section preview: none");
    }
}

pub(super) fn build_blocking_preview_command_output(
    preview: &crate::blocker::BlockingPreview,
) -> BlockingPreviewCommandOutput {
    let action = match preview.action {
        BlockingPreviewAction::Block => "block",
        BlockingPreviewAction::Unblock => "unblock",
        BlockingPreviewAction::NoChange => "no_change",
    };
    BlockingPreviewCommandOutput {
        backend: preview.backend.id(),
        backend_target: preview.backend_target.clone(),
        attempted_backends: preview
            .attempted_backends
            .iter()
            .map(|backend| backend.id())
            .collect(),
        fallback_used: preview.fallback_used,
        hosts_file_path: preview.hosts_file_path.clone(),
        action,
        would_change: preview.would_change,
        effective_blocked_sites_count: preview.effective_blocked_sites.len(),
        effective_blocked_sites: preview.effective_blocked_sites.clone(),
        section: preview.section_for_display().map(ToString::to_string),
    }
}

fn setup_check_output(check: &SetupCheck) -> SetupCheckOutput {
    SetupCheckOutput {
        level: setup_check_level_id(check.level),
        message: check.message.clone(),
    }
}

fn setup_check_level_id(level: SetupCheckLevel) -> &'static str {
    match level {
        SetupCheckLevel::Ok => "ok",
        SetupCheckLevel::Warning => "warning",
    }
}

pub(super) fn print_json<T: Serialize>(payload: &T) -> Result<(), String> {
    let json = serde_json::to_string_pretty(payload)
        .map_err(|error| format!("Failed to encode JSON output: {error}"))?;
    println!("{json}");
    Ok(())
}

pub(super) fn print_json_compact<T: Serialize>(payload: &T) -> Result<(), String> {
    let json = serde_json::to_string(payload)
        .map_err(|error| format!("Failed to encode JSON output: {error}"))?;
    println!("{json}");
    Ok(())
}

pub(super) fn flush_stdout() -> Result<(), String> {
    io::stdout()
        .flush()
        .map_err(|error| format!("Failed to flush stdout: {error}"))
}

pub(super) fn format_duration(secs: u64) -> String {
    let minutes = secs / 60;
    let seconds = secs % 60;
    match (minutes, seconds) {
        (0, s) => format!("{s}s"),
        (m, 0) => format!("{m}m"),
        (m, s) => format!("{m}m {s}s"),
    }
}

fn format_expiry_clock_suffix(epoch_secs: i64) -> String {
    Local
        .timestamp_opt(epoch_secs, 0)
        .single()
        .map(|datetime| format!(" at {}", datetime.format("%H:%M:%S")))
        .unwrap_or_default()
}

pub(super) fn display_input_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "<empty>".to_string()
    } else {
        trimmed.to_string()
    }
}

pub(super) fn effective_blocked_sites_for_profile(profile: &BlocklistProfileConfig) -> Vec<String> {
    crate::config::effective_blocked_sites_for_profile(profile)
}

#[cfg(test)]
mod tests {
    use super::effective_blocked_sites_for_profile;
    use crate::config::BlocklistCategoryConfig;

    #[test]
    fn effective_blocked_sites_keeps_wildcard_only_rules() {
        let profile = crate::config::BlocklistProfileConfig {
            sites: vec!["*.Example.com".to_string()],
            ..crate::config::BlocklistProfileConfig::default()
        };

        assert_eq!(
            effective_blocked_sites_for_profile(&profile),
            vec!["*.example.com".to_string()]
        );
    }

    #[test]
    fn effective_blocked_sites_keeps_wildcard_when_allowlist_is_exact_host() {
        let profile = crate::config::BlocklistProfileConfig {
            sites: vec!["*.example.com".to_string()],
            allowlist_sites: vec!["ads.example.com".to_string()],
            ..crate::config::BlocklistProfileConfig::default()
        };

        assert_eq!(
            effective_blocked_sites_for_profile(&profile),
            vec!["*.example.com".to_string()]
        );
    }

    #[test]
    fn effective_blocked_sites_aggregates_categories_and_dedups_case_insensitively() {
        let profile = crate::config::BlocklistProfileConfig {
            sites: vec!["legacy-top-level.com".to_string()],
            categories: vec![
                BlocklistCategoryConfig {
                    name: "Social".to_string(),
                    sites: vec!["News.com".to_string(), "*.example.com".to_string()],
                    allowlist_sites: vec!["news.com".to_string()],
                },
                BlocklistCategoryConfig {
                    name: "Work".to_string(),
                    sites: vec!["*.EXAMPLE.com".to_string(), "forum.example.com".to_string()],
                    allowlist_sites: Vec::new(),
                },
            ],
            ..crate::config::BlocklistProfileConfig::default()
        };

        assert_eq!(
            effective_blocked_sites_for_profile(&profile),
            vec!["*.example.com".to_string(), "forum.example.com".to_string()]
        );
    }

    #[test]
    fn effective_blocked_sites_canonicalizes_dotted_and_punycode_rules() {
        let profile = crate::config::BlocklistProfileConfig {
            sites: vec![
                ".Example.com".to_string(),
                "*.xn--bcher-kva.example.".to_string(),
                "api.example.com.".to_string(),
            ],
            allowlist_sites: vec![".example.com".to_string()],
            ..crate::config::BlocklistProfileConfig::default()
        };

        assert_eq!(
            effective_blocked_sites_for_profile(&profile),
            vec!["*.xn--bcher-kva.example".to_string()]
        );
    }
}
