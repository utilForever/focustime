mod diagnostics;
mod sites;
mod status;

use std::path::Path;

pub(super) use diagnostics::{
    build_blocking_preview_command_output, build_diagnostics_blocking_preview_error,
    build_diagnostics_blocking_preview_output, build_diagnostics_command_output,
    build_schedule_inspection_output, print_blocking_preview_command_output,
    print_config_doctor_output, print_config_migration_output, print_diagnostics_command_output,
};
pub(super) use sites::{
    print_blocklist_category_command_output, print_blocklist_profile_command_output,
    print_history_dashboard_command_output, print_session_template_command_output,
    print_site_add_command_output, print_site_delete_command_output,
    print_site_edit_command_output, print_site_list_command_output,
    print_temporary_site_add_command_output, print_usage_signals_command_output,
};
pub(super) use status::{
    print_break_glass_command_output, print_daemon_start_command_output,
    print_daemon_status_command_output, print_daemon_stop_command_output,
    print_schedule_delay_command_output, print_session_metadata_command_output,
    print_status_output, print_timer_state_output,
};

use crate::cli::{
    AutomationTriggersCommandOutput, BackupOutput, BlocklistProfileConfig,
    CalendarSyncCommandOutput, ExportOutput, FeatureInventoryOutput, GoalCarryCommandOutput,
    GoalCommandOutput, ProfileOutput, RestoreOutput, ScheduleCommandOutput, Serialize,
    StrictCommandOutput, TaskGoalCommandOutput, ThemeCommandOutput, WeekdayRulesCommandOutput,
    Write, io,
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
        "Selected theme preset: {} ({})",
        payload.selected_theme_preset.label, payload.selected_theme_preset.id
    );
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

pub(super) fn print_export_output(payload: &ExportOutput) {
    println!("Exported stats to {}", payload.export_dir.display());
    print_artifact_paths(&[("JSON", &payload.json_path), ("CSV", &payload.csv_path)]);
}

pub(super) fn print_feature_inventory_output(payload: &FeatureInventoryOutput) {
    println!(
        "Exported feature inventory report to {}",
        payload.export_dir.display()
    );
    print_artifact_paths(&[
        ("JSON", &payload.json_path),
        ("Markdown", &payload.markdown_path),
    ]);
    println!(
        "Features: {} (keep {}, merge {}, remove {})",
        payload.total_features, payload.keep_count, payload.merge_count, payload.remove_count
    );
}

pub(super) fn print_backup_output(payload: &BackupOutput) {
    println!("Backed up app data to {}", payload.backup_dir.display());
    print_artifact_paths(&[
        ("Config", &payload.config_backup_path),
        ("Stats", &payload.stats_backup_path),
    ]);
}

pub(super) fn print_restore_output(payload: &RestoreOutput) {
    println!("Restored app data from {}", payload.restore_dir.display());
    print_artifact_paths(&[
        ("Config", &payload.config_restored_path),
        ("Stats", &payload.stats_restored_path),
    ]);
}

fn print_artifact_paths(paths: &[(&str, &Path)]) {
    for (label, path) in paths {
        println!("{label}: {}", path.display());
    }
}

pub(super) fn print_calendar_sync_command_output(payload: &CalendarSyncCommandOutput) {
    if payload.deprecated {
        println!("Deprecated command: --calendar-sync");
        println!("Replacement: {}", payload.replacement);
    }
    println!("Calendar cache refreshed for schedule annotations.");
    println!("Behavior model: {}", payload.behavior_model);
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
    if payload.deprecated {
        println!("Weekday rules are deprecated. {}", payload.replacement);
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
    print!("{}", format_automation_triggers_command_output(payload));
}

pub(super) fn format_automation_triggers_command_output(
    payload: &AutomationTriggersCommandOutput,
) -> String {
    let mut output = String::new();
    if payload.updated {
        output.push_str("Automation triggers updated.\n");
    }
    if payload.deprecated {
        output.push_str(&format!(
            "Automation triggers are deprecated. {}",
            payload.replacement
        ));
        output.push('\n');
    }
    if payload.rules.is_empty() {
        output.push_str("Automation triggers: none\n");
        return output;
    }
    output.push_str("Automation triggers:\n");
    for rule in &payload.rules {
        let formatted = serde_json::to_string(rule)
            .unwrap_or_else(|_| "<failed to serialize automation trigger>".to_string());
        output.push_str(&format!("  - {formatted}\n"));
    }
    output
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
    use crate::config::{AppConfig, BlocklistCategoryConfig};

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
    fn effective_blocked_sites_prefers_profile_level_rules_when_present() {
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
            vec!["legacy-top-level.com".to_string()]
        );
    }

    #[test]
    fn effective_blocked_sites_uses_normalized_profile_rules_for_category_compatibility() {
        let config = AppConfig {
            blocklist_profiles: vec![crate::config::BlocklistProfileConfig {
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
            }],
            ..AppConfig::default()
        }
        .normalized();
        let profile = &config.blocklist_profiles[0];

        assert_eq!(
            effective_blocked_sites_for_profile(profile),
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
