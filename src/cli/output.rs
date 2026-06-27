mod diagnostics;
mod sites;
mod status;

use std::path::Path;

pub(super) use diagnostics::{
    build_diagnostics_blocking_preview_error, build_diagnostics_blocking_preview_output,
    build_diagnostics_command_output, build_schedule_inspection_output,
    print_diagnostics_command_output,
};
pub(super) use sites::{
    print_blocklist_profile_command_output, print_history_dashboard_command_output,
    print_site_add_command_output, print_site_delete_command_output,
    print_site_edit_command_output, print_site_list_command_output,
};
pub(super) use status::{
    print_break_glass_command_output, print_status_output, print_timer_state_output,
};

use crate::cli::{
    BackupOutput, BlocklistProfileConfig, ExportOutput, GoalCarryCommandOutput, GoalCommandOutput,
    ProfileOutput, RestoreOutput, ScheduleCommandOutput, Serialize, StrictCommandOutput,
    ThemeCommandOutput, Write, io,
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
    use crate::config::AppConfig;

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
    fn effective_blocked_sites_uses_profile_level_rules() {
        let profile = crate::config::BlocklistProfileConfig {
            sites: vec!["legacy-top-level.com".to_string()],
            ..crate::config::BlocklistProfileConfig::default()
        };

        assert_eq!(
            effective_blocked_sites_for_profile(&profile),
            vec!["legacy-top-level.com".to_string()]
        );
    }

    #[test]
    fn effective_blocked_sites_uses_normalized_profile_rules_for_category_compatibility() {
        let config: AppConfig = toml::from_str(
            r#"
[[blocklist_profiles]]
name = "Default"

[[blocklist_profiles.categories]]
name = "Social"
sites = ["News.com", "*.example.com"]
allowlist_sites = ["news.com"]

[[blocklist_profiles.categories]]
name = "Work"
sites = ["*.EXAMPLE.com", "forum.example.com"]
allowlist_sites = []
"#,
        )
        .unwrap();
        let config = config.normalized();
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
