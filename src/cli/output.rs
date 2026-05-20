use crate::blocker::{domain_rule_matches_host, normalize_domain_rule};
use crate::cli::{
    AutomationTriggersCommandOutput, BackupOutput, BlockingPreviewAction,
    BlockingPreviewCommandOutput, BlocklistCategoryCommandOutput, BlocklistProfileCommandOutput,
    BlocklistProfileConfig, BreakGlassCommandOutput, DiagnosticsCommandOutput, ExportOutput,
    FocusScoreOutput, GoalCarryCommandOutput, GoalCommandOutput, GoalOutput, ProfileOutput,
    RecurringScheduleConfig, RestoreOutput, ScheduleCommandOutput, ScheduleDelayCommandOutput,
    ScheduleInspectionOutput, Serialize, SessionMetadataCommandOutput,
    SessionTemplateCommandOutput, SetupCheck, SetupCheckLevel, SetupCheckOutput, SetupDiagnostics,
    SiteAddCommandOutput, SiteDeleteCommandOutput, SiteEditCommandOutput, SiteListCommandOutput,
    StatsGrowthSummary, StatsRetentionStatusOutput, StatusOutput, StrictCommandOutput,
    TaskGoalCommandOutput, TaskGoalOutput, TemporarySiteAddCommandOutput, ThemeCommandOutput,
    TimerStateOutput, WeekdayRulesCommandOutput, Write, format_schedule_conflict,
    inspect_schedule_conflicts_from_config, io,
};

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
    println!(
        "Temporary allowlist active: {}",
        payload.temporary_allowlist_active_count
    );
    for entry in &payload.temporary_allowlist_active {
        println!(
            "  - {} (expires in {})",
            entry.site,
            format_duration(entry.remaining_secs)
        );
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

pub(super) fn display_input_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "<empty>".to_string()
    } else {
        trimmed.to_string()
    }
}

pub(super) fn effective_blocked_sites_for_profile(profile: &BlocklistProfileConfig) -> Vec<String> {
    let allowlist_rules: Vec<String> = all_allowlist_rules_for_profile(profile)
        .iter()
        .filter_map(|rule| normalize_domain_rule(rule).ok())
        .collect();

    let mut seen = std::collections::HashSet::new();
    all_blocklist_rules_for_profile(profile)
        .into_iter()
        .filter_map(|site| normalize_domain_rule(&site).ok())
        .filter(|site| !site.starts_with("*."))
        .filter(|site| {
            !allowlist_rules
                .iter()
                .any(|allow_rule| domain_rule_matches_host(allow_rule, site))
        })
        .filter(|site| seen.insert(site.to_ascii_lowercase()))
        .collect()
}

fn all_blocklist_rules_for_profile(profile: &BlocklistProfileConfig) -> Vec<String> {
    if profile.categories.is_empty() {
        return dedup_case_insensitive(profile.sites.iter().cloned());
    }
    dedup_case_insensitive(
        profile
            .categories
            .iter()
            .flat_map(|category| category.sites.iter().cloned()),
    )
}

fn all_allowlist_rules_for_profile(profile: &BlocklistProfileConfig) -> Vec<String> {
    if profile.categories.is_empty() {
        return dedup_case_insensitive(profile.allowlist_sites.iter().cloned());
    }
    dedup_case_insensitive(
        profile
            .categories
            .iter()
            .flat_map(|category| category.allowlist_sites.iter().cloned()),
    )
}

fn dedup_case_insensitive<I>(values: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    let mut seen = std::collections::HashSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.to_ascii_lowercase()))
        .collect()
}
