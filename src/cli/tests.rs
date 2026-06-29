use crate::cli::*;
use crate::config::{ConfigDoctorReport, ConfigHealthStatus, ConfigMigrationReport};
use crate::session_recovery::{
    self, InProgressSessionSnapshot, RecoveryTimerPhase, RecoveryTimerStatus,
};
use chrono::Duration;
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;

fn parse(values: &[&str]) -> Result<CliAction, String> {
    parse_args(values.iter().map(OsString::from))
}

fn parse_with_contract(values: &[&str]) -> Result<CliAction, CliError> {
    parse_args_with_contract(values.iter().map(OsString::from))
}

fn test_config_doctor_report() -> ConfigDoctorReport {
    ConfigDoctorReport {
        action: "config-health",
        config_path: None,
        detected_schema_version: None,
        current_schema_version: 2,
        status: ConfigHealthStatus::Ok,
        migration_steps: Vec::new(),
        findings: Vec::new(),
    }
}

fn test_config_migration_report() -> ConfigMigrationReport {
    ConfigMigrationReport {
        action: "config-migration-guidance",
        applied: false,
        config_path: None,
        backup_path: None,
        detected_schema_version: None,
        target_schema_version: 2,
        changed: false,
        status: ConfigHealthStatus::Ok,
        steps: Vec::new(),
        findings: Vec::new(),
    }
}

fn test_diagnostics_blocking_preview_output() -> DiagnosticsBlockingPreviewOutput {
    DiagnosticsBlockingPreviewOutput {
        status: "ok",
        error: None,
        preview: Some(BlockingPreviewOutput {
            backend: "hosts",
            backend_target: "hosts".to_string(),
            hosts_file_path: "hosts".to_string(),
            action: "block",
            would_change: true,
            effective_blocked_sites_count: 1,
            effective_blocked_sites: vec!["example.com".to_string()],
            section: Some("127.0.0.1 example.com\n".to_string()),
        }),
    }
}

#[test]
fn parse_without_arguments_runs_default_tui() {
    let parsed = parse(&[]).unwrap();
    assert_eq!(parsed, CliAction::RunTui);
}

#[test]
fn parse_start_runs_as_noninteractive_command() {
    let parsed = parse(&["--start"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Start,
            output: OutputMode::Text
        })
    );
}

/// Verifies that status output can be requested in JSON mode.
#[test]
fn parse_status_supports_json_mode() {
    let parsed = parse(&["--status", "--json"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Status {
                watch_interval_secs: None,
            },
            output: OutputMode::Json
        })
    );
}

#[test]
fn parse_status_watch_without_interval_uses_default_cadence() {
    let parsed = parse(&["--status", "--watch"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Status {
                watch_interval_secs: Some(DEFAULT_WATCH_INTERVAL_SECS),
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_status_watch_with_equals_interval() {
    let parsed = parse(&["--status", "--watch=3"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Status {
                watch_interval_secs: Some(3),
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_status_watch_with_space_interval() {
    let parsed = parse(&["--status", "--watch", "2"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Status {
                watch_interval_secs: Some(2),
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_rejects_retired_daemon_lifecycle_options() {
    for flag in [
        "--daemon-start",
        "--daemon-status",
        "--daemon-stop",
        "--daemon-port=43123",
    ] {
        let error = parse_with_contract(&[flag, "--json"]).unwrap_err();
        assert_eq!(error.output, OutputMode::Json);
        assert!(error.message.contains(&format!("Unknown option `{flag}`")));
        assert!(error.hint.is_none());
    }
}

#[test]
fn usage_text_omits_retired_daemon_lifecycle_options() {
    let usage = usage_text();
    for retired in [
        "--daemon-start",
        "--daemon-status",
        "--daemon-stop",
        "--daemon-port",
    ] {
        assert!(!usage.contains(retired));
    }
}

#[test]
fn usage_text_omits_retired_config_diagnostics_commands() {
    let usage = usage_text();
    for retired in [
        "--config-doctor",
        "--config-migrate",
        "--config-migrate-apply",
    ] {
        assert!(!usage.contains(retired));
    }
    assert!(usage.contains("--diagnostics"));
}

#[test]
fn usage_text_keeps_supported_cli_automation_replacements() {
    let usage = usage_text();
    for supported in [
        "--start", "--pause", "--resume", "--stop", "--next", "--task",
    ] {
        assert!(usage.contains(supported));
    }
}

#[test]
fn usage_text_omits_break_glass_commands() {
    let usage = usage_text();
    assert!(!usage.contains("--break-glass-trigger"));
    assert!(!usage.contains("--break-glass-cancel"));
}

#[test]
fn parse_rejects_status_comparison_options_as_unknown_options() {
    let error = parse_with_contract(&[
        "--status",
        "--compare-by",
        "time-of-day",
        "--compare-task",
        "Docs",
        "--compare-profile",
        "classic",
        "--compare-time",
        "night",
        "--compare-limit",
        "3",
    ])
    .unwrap_err();

    assert_eq!(error.exit_code(), EXIT_CODE_USAGE_ERROR);
    assert!(error.message.contains("Unknown option `--compare-by`."));
    assert!(error.hint.is_none());
}

#[test]
fn parse_rejects_equals_status_comparison_options_as_unknown_options() {
    let error = parse_with_contract(&["--status", "--compare-time=night"]).unwrap_err();

    assert!(
        error
            .message
            .contains("Unknown option `--compare-time=night`.")
    );
    assert!(error.hint.is_none());
}

#[test]
fn parse_rejects_comparison_options_without_status_as_unknown_options() {
    let error = parse_with_contract(&["--compare-by", "task"]).unwrap_err();

    assert!(error.message.contains("Unknown option `--compare-by`."));
    assert!(error.hint.is_none());
}

#[test]
fn parse_pause_supports_json_mode() {
    let parsed = parse(&["--pause", "--json"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Pause,
            output: OutputMode::Json
        })
    );
}

#[test]
fn parse_task_with_equals_sets_label() {
    let parsed = parse(&["--task=Docs"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Task {
                label: "Docs".to_string()
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_task_with_value_sets_label() {
    let parsed = parse(&["--task", "Docs"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Task {
                label: "Docs".to_string()
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_task_note_without_value_is_removed() {
    let error = parse_with_contract(&["--task-note"]).unwrap_err();

    assert_eq!(error.exit_code(), EXIT_CODE_USAGE_ERROR);
    assert!(error.message.contains("Unknown option `--task-note`."));
    assert_eq!(
        error.hint.as_deref(),
        Some(
            "Use `--task` to select the active task label; task labels are the supported session context."
        )
    );
}

#[test]
fn parse_task_goal_without_value_is_removed() {
    let error = parse_with_contract(&["--task-goal"]).unwrap_err();

    assert_eq!(error.exit_code(), EXIT_CODE_USAGE_ERROR);
    assert!(error.message.contains("Unknown option `--task-goal`."));
    assert_eq!(
        error.hint.as_deref(),
        Some("Use `--goal` for the daily goal; task labels remain available through `--task`.")
    );
}

#[test]
fn parse_task_goal_with_equals_value_is_removed() {
    let error = parse_with_contract(&["--task-goal=Docs:120,4"]).unwrap_err();

    assert_eq!(error.exit_code(), EXIT_CODE_USAGE_ERROR);
    assert!(
        error
            .message
            .contains("Unknown option `--task-goal=Docs:120,4`.")
    );
    assert_eq!(
        error.hint.as_deref(),
        Some("Use `--goal` for the daily goal; task labels remain available through `--task`.")
    );
}

#[test]
fn parse_task_note_with_value_is_removed() {
    let error = parse_with_contract(&["--task-note", "Capture blockers"]).unwrap_err();

    assert_eq!(error.exit_code(), EXIT_CODE_USAGE_ERROR);
    assert!(error.message.contains("Unknown option `--task-note`."));
}

#[test]
fn parse_profile_supports_json_mode() {
    let parsed = parse(&["--profile", "--json"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Profile { profile: None },
            output: OutputMode::Json
        })
    );
}

#[test]
fn parse_profile_without_value_reads_current_profile() {
    let parsed = parse(&["--profile"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Profile { profile: None },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_profile_with_value_sets_profile() {
    let parsed = parse(&["--profile", "deep-work"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Profile {
                profile: Some(ProfileId::DeepWork)
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_profile_with_equals_sets_profile() {
    let parsed = parse(&["--profile=classic"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Profile {
                profile: Some(ProfileId::Classic)
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_theme_without_value_reads_current_theme() {
    let parsed = parse(&["--theme"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Theme { preset: None },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_theme_with_value_sets_theme() {
    let parsed = parse(&["--theme", "high-contrast"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Theme {
                preset: Some(ThemePreset::HighContrast)
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_theme_with_equals_sets_theme() {
    let parsed = parse(&["--theme=deuteranopia-friendly"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Theme {
                preset: Some(ThemePreset::DeuteranopiaFriendly)
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_goal_without_value_reads_current_goal() {
    let parsed = parse(&["--goal"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Goal { goal: None },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_goal_with_equals_sets_goal() {
    let parsed = parse(&["--goal=120,4"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Goal {
                goal: Some(DailyGoalConfig {
                    minutes: 120,
                    pomodoros: 4
                })
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_goal_with_value_sets_goal() {
    let parsed = parse(&["--goal", "45,2"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Goal {
                goal: Some(DailyGoalConfig {
                    minutes: 45,
                    pomodoros: 2
                })
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_rejects_weekly_goal_without_value() {
    let error = parse(&["--goal-weekly"]).unwrap_err();
    assert!(error.contains("Unknown option `--goal-weekly`"));
    assert!(error.contains("Weekly and monthly goal commands were removed."));
}

#[test]
fn parse_rejects_weekly_goal_with_equals_value() {
    let error = parse(&["--goal-weekly=420,14"]).unwrap_err();
    assert!(error.contains("Unknown option `--goal-weekly=420,14`"));
    assert!(error.contains("Weekly and monthly goal commands were removed."));
}

#[test]
fn parse_rejects_monthly_goal_with_value() {
    let error = parse(&["--goal-monthly", "1800,60"]).unwrap_err();
    assert!(error.contains("Unknown option `--goal-monthly`"));
    assert!(error.contains("Weekly and monthly goal commands were removed."));
}

#[test]
fn parse_strict_without_value_reads_current_state() {
    let parsed = parse(&["--strict"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Strict { enabled: None },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_strict_with_equals_sets_state() {
    let parsed = parse(&["--strict=on"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Strict {
                enabled: Some(true)
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_goal_carry_without_value_reads_current_state() {
    let parsed = parse(&["--goal-carry"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::GoalCarry { enabled: None },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_rejects_goal_carry_weekly_with_equals_value() {
    let error = parse(&["--goal-carry-weekly=on"]).unwrap_err();
    assert!(error.contains("Unknown option `--goal-carry-weekly=on`"));
    assert!(error.contains("Weekly and monthly goal carry-over commands were removed."));
}

#[test]
fn parse_rejects_goal_carry_monthly_with_value() {
    let error = parse(&["--goal-carry-monthly", "off"]).unwrap_err();
    assert!(error.contains("Unknown option `--goal-carry-monthly`"));
    assert!(error.contains("Weekly and monthly goal carry-over commands were removed."));
}

#[test]
fn parse_schedule_reads_current_schedule() {
    let parsed = parse(&["--schedule"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Schedule { schedule: None },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_weekday_rules_is_retired() {
    let error = parse(&["--weekday-rules"]).unwrap_err();
    assert!(error.contains("Unknown option `--weekday-rules`"));
}

#[test]
fn parse_automation_triggers_is_retired() {
    let error = parse(&["--automation-triggers"]).unwrap_err();
    assert!(error.contains("Unknown option `--automation-triggers`"));
    assert!(error.contains("Standalone automation trigger commands were removed."));
}

#[test]
fn parse_break_glass_commands_are_removed_with_guidance() {
    for flag in ["--break-glass-trigger", "--break-glass-cancel"] {
        let error = parse_with_contract(&[flag]).unwrap_err();
        assert_eq!(error.exit_code(), EXIT_CODE_USAGE_ERROR);
        assert!(error.message.contains(&format!("Unknown option `{flag}`.")));
        assert_eq!(
            error.hint.as_deref(),
            Some(
                "Use normal timer controls (`--pause`, `--resume`, `--stop`, `--next`) or manage blocked sites with blocklist commands."
            )
        );
    }
}

#[test]
fn parse_schedule_set_accepts_json_payload() {
    let payload = r#"{"windows":[{"days":["mon","wed"],"start":"09:00","end":"11:00"}]}"#;
    let parsed = parse_args([OsString::from("--schedule-set"), OsString::from(payload)]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Schedule {
                schedule: Some(RecurringScheduleConfig {
                    windows: vec![RecurringFocusWindowConfig {
                        days: vec!["mon".to_string(), "wed".to_string()],
                        start: "09:00".to_string(),
                        end: "11:00".to_string(),
                    }],
                }),
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_schedule_set_rejects_one_time_windows_payload() {
    let payload = r#"{"windows":[],"one_time_windows":[{"date":"2026-05-02","start":"14:00","end":"15:30"}]}"#;
    let error = parse_args([OsString::from("--schedule-set"), OsString::from(payload)])
        .expect_err("one-time schedule windows should be rejected");

    assert!(error.contains("one_time_windows"));
}

#[test]
fn parse_automation_triggers_set_is_retired() {
    let payload = r#"[{"trigger":{"type":"focus_completed"},"action":{"type":"start_focus"}}]"#;
    let error = parse_args([
        OsString::from("--automation-triggers-set"),
        OsString::from(payload),
    ])
    .unwrap_err();
    assert!(error.contains("Unknown option `--automation-triggers-set`"));
}

#[test]
fn schedule_inspection_output_reports_detected_conflicts() {
    let schedule = RecurringScheduleConfig {
        windows: vec![
            RecurringFocusWindowConfig {
                days: vec!["mon".to_string()],
                start: "09:00".to_string(),
                end: "11:00".to_string(),
            },
            RecurringFocusWindowConfig {
                days: vec!["mon".to_string()],
                start: "10:30".to_string(),
                end: "12:00".to_string(),
            },
        ],
    };

    let output = build_schedule_inspection_output(&schedule);

    assert_eq!(output.conflict_count, 1);
    assert_eq!(output.conflicts.len(), 1);
    assert!(output.conflicts[0].contains("recurring #1 overlaps recurring #2"));
}

#[test]
fn schedule_inspection_output_reports_no_conflicts() {
    let schedule = RecurringScheduleConfig {
        windows: vec![RecurringFocusWindowConfig {
            days: vec!["mon".to_string()],
            start: "09:00".to_string(),
            end: "10:00".to_string(),
        }],
    };

    let output = build_schedule_inspection_output(&schedule);

    assert_eq!(output.conflict_count, 0);
    assert!(output.conflicts.is_empty());
}

#[test]
fn parse_diagnostics_supports_json_mode() {
    let parsed = parse(&["--diagnostics", "--json"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Diagnostics,
            output: OutputMode::Json
        })
    );
}

#[test]
fn parse_rejects_retired_config_diagnostics_commands_with_guidance() {
    for flag in [
        "--config-doctor",
        "--config-migrate",
        "--config-migrate-apply",
    ] {
        let error = parse_with_contract(&[flag, "--json"]).unwrap_err();
        assert_eq!(error.output, OutputMode::Json);
        assert!(error.message.contains(&format!("Unknown option `{flag}`")));
        assert!(
            error
                .message
                .contains("Dedicated config diagnostics commands were removed.")
        );
        assert_eq!(
            error.hint.as_deref(),
            Some(
                "Use `focustime --diagnostics` for setup checks, config health, and migration guidance."
            )
        );
    }
}

#[test]
fn diagnostics_output_includes_config_health_and_migration_guidance() {
    let app = App::default();
    let payload = build_diagnostics_command_output(
        &app.setup_diagnostics,
        test_config_doctor_report(),
        test_config_migration_report(),
        test_diagnostics_blocking_preview_output(),
    );

    assert_eq!(payload.action, "diagnostics");
    assert!(payload.setup.deprecation_warnings.is_empty());
    assert_eq!(payload.blocking_preview.status, "ok");
    let preview = payload
        .blocking_preview
        .preview
        .as_ref()
        .expect("diagnostics should include blocking preview details");
    assert_eq!(preview.backend, "hosts");
    assert_eq!(preview.action, "block");
    assert!(preview.would_change);
    assert_eq!(preview.effective_blocked_sites, vec!["example.com"]);
    assert_eq!(payload.config_doctor.action, "config-health");
    assert_eq!(payload.config_doctor.status, ConfigHealthStatus::Ok);
    assert_eq!(payload.config_migration.action, "config-migration-guidance");
    assert!(!payload.config_migration.applied);
}

#[test]
fn diagnostics_output_includes_deprecation_warnings() {
    let mut app = App::default();
    app.setup_diagnostics.deprecation_warnings = vec![
        "Deprecated top-level timer fields are in use.".to_string(),
        "Deprecated top-level automation fields are in use.".to_string(),
    ];

    let payload = build_diagnostics_command_output(
        &app.setup_diagnostics,
        test_config_doctor_report(),
        test_config_migration_report(),
        test_diagnostics_blocking_preview_output(),
    );

    assert_eq!(
        payload.setup.deprecation_warnings,
        vec![
            "Deprecated top-level timer fields are in use.".to_string(),
            "Deprecated top-level automation fields are in use.".to_string()
        ]
    );
}

#[test]
fn parse_blocking_preview_reports_unknown_option() {
    let error = parse(&["--blocking-preview", "--json"]).unwrap_err();
    assert!(error.contains("Unknown option `--blocking-preview`"));
}

#[test]
fn parse_export_accepts_optional_directory() {
    let parsed = parse(&["--export", "reports"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Export {
                dir: Some(PathBuf::from("reports"))
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_backup_accepts_optional_directory() {
    let parsed = parse(&["--backup", "reports"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Backup {
                dir: Some(PathBuf::from("reports"))
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_backup_without_value_uses_default_directory() {
    let parsed = parse(&["--backup"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Backup { dir: None },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_backup_with_equals_accepts_directory() {
    let parsed = parse(&["--backup=reports"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Backup {
                dir: Some(PathBuf::from("reports"))
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_restore_accepts_optional_directory() {
    let parsed = parse(&["--restore", "reports"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Restore {
                dir: Some(PathBuf::from("reports"))
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_restore_without_value_uses_default_directory() {
    let parsed = parse(&["--restore"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Restore { dir: None },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_restore_with_equals_accepts_directory() {
    let parsed = parse(&["--restore=reports"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Restore {
                dir: Some(PathBuf::from("reports"))
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_calendar_sync_is_retired() {
    let error = parse_with_contract(&["--calendar-sync", "--json"]).unwrap_err();

    assert_eq!(error.exit_code(), EXIT_CODE_USAGE_ERROR);
    assert!(error.message.contains("Unknown option `--calendar-sync`."));
    assert!(error.hint.is_none());
}

#[test]
fn parse_export_with_equals_accepts_directory() {
    let parsed = parse(&["--export=reports"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Export {
                dir: Some(PathBuf::from("reports"))
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_feature_inventory_is_retired() {
    let error = parse_with_contract(&["--feature-inventory=reports", "--json"]).unwrap_err();

    assert_eq!(error.exit_code(), EXIT_CODE_USAGE_ERROR);
    assert_eq!(error.output, OutputMode::Json);
    assert!(
        error
            .message
            .contains("Unknown option `--feature-inventory=reports`.")
    );
    assert!(error.hint.is_none());
}

#[test]
fn parse_blocklist_profile_commands_are_retired() {
    for args in [
        &["--blocklist-profile"][..],
        &["--blocklist-profile", "Work"][..],
        &["--blocklist-profile-create", "Work"][..],
        &["--blocklist-profile-create=Work"][..],
        &["--blocklist-profile-rename", "Deep Work"][..],
        &["--blocklist-profile-rename=Deep Work"][..],
        &["--blocklist-profile-delete"][..],
    ] {
        let error = parse(args).unwrap_err();
        assert!(error.contains("Unknown option `--blocklist-profile"));
        assert!(error.contains("Blocklist profile commands were removed."));
    }
}

#[test]
fn parse_history_dashboard_without_value_reads_dashboard_state() {
    let parsed = parse(&["--history-dashboard"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::HistoryDashboard {
                command: HistoryDashboardCommandKind::Show
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_rejects_retired_history_dashboard_customization_flags() {
    for values in [
        &["--history-dashboard-pin", "focus_score"][..],
        &["--history-dashboard-unpin=goal_streak"][..],
        &["--history-dashboard-order=focus_score,goal_streak"][..],
    ] {
        let error = parse(values).unwrap_err();
        assert!(error.contains("Unknown option `"));
        assert!(error.contains("--history-dashboard"));
    }
}

#[test]
fn usage_text_omits_retired_history_dashboard_customization_flags() {
    assert!(!USAGE_TEXT.contains("--history-dashboard-pin"));
    assert!(!USAGE_TEXT.contains("--history-dashboard-unpin"));
    assert!(!USAGE_TEXT.contains("--history-dashboard-order"));
    assert!(USAGE_TEXT.contains("--history-dashboard [--json]"));
}

#[test]
fn parse_rejects_removed_blocklist_category_flags() {
    for values in [
        &["--blocklist-category"][..],
        &["--blocklist-category=Social"][..],
        &["--blocklist-category-create", "Social"][..],
        &["--blocklist-category-create=Social"][..],
        &["--blocklist-category-rename", "Focus"][..],
        &["--blocklist-category-rename=Focus"][..],
        &["--blocklist-category-delete"][..],
    ] {
        let error = parse(values).unwrap_err();
        assert!(error.contains("Unknown option `--blocklist-category"));
    }
}

#[test]
fn usage_text_omits_removed_blocklist_category_flags() {
    assert!(!USAGE_TEXT.contains("--blocklist-category"));
    assert!(USAGE_TEXT.contains("--blocklist-sites [--json]"));
    assert!(!USAGE_TEXT.contains("--allowlist-sites"));
}

#[test]
fn parse_blocklist_site_add_with_equals() {
    let parsed = parse(&["--blocklist-site-add=github.com,news.ycombinator.com"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::BlocklistSites {
                target: SiteListTarget::Blocklist,
                command: BlocklistSiteCommandKind::Add {
                    input: "github.com,news.ycombinator.com".to_string()
                }
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_allowlist_site_add_temporary_is_removed() {
    let error =
        parse_with_contract(&["--allowlist-site-add-temporary=reddit.com=30m"]).unwrap_err();

    assert!(
        error
            .message
            .contains("Temporary allowlist commands were removed.")
    );
    assert_eq!(
        error.hint.as_deref(),
        Some(
            "Use blocklist site commands for CLI site-rule management; keep persistent exceptions in `allowlist_sites` config when needed."
        )
    );
}

#[test]
fn parse_allowlist_site_management_commands_are_removed_with_guidance() {
    for args in [
        &["--allowlist-sites"][..],
        &["--allowlist-site-add", "reddit.com"][..],
        &["--allowlist-site-add=reddit.com"][..],
        &["--allowlist-site-edit", "old.com=new.com"][..],
        &["--allowlist-site-edit=old.com=new.com"][..],
        &["--allowlist-site-delete", "reddit.com"][..],
        &["--allowlist-site-delete=reddit.com"][..],
    ] {
        let error = parse_with_contract(args).unwrap_err();
        assert_eq!(error.exit_code(), EXIT_CODE_USAGE_ERROR);
        assert!(error.message.contains("Unknown option `--allowlist-"));
        assert!(
            error
                .message
                .contains("Allowlist site management commands were removed.")
        );
        assert_eq!(
            error.hint.as_deref(),
            Some(
                "Manage blocked hostnames with `--blocklist-sites`, `--blocklist-site-add`, `--blocklist-site-edit`, and `--blocklist-site-delete`; keep exceptions in `allowlist_sites` config when needed."
            )
        );
    }
}

#[test]
fn classify_key_value_arg_accepts_profile_equals_value() {
    let parsed = classify_key_value_arg("--profile=deep-work").unwrap();
    assert_eq!(
        parsed,
        Some(ParsedToken::Profile(Some(ProfileId::DeepWork)))
    );
}

#[test]
fn classify_key_value_arg_accepts_theme_equals_value() {
    let parsed = classify_key_value_arg("--theme=high_contrast").unwrap();
    assert_eq!(
        parsed,
        Some(ParsedToken::Theme(Some(ThemePreset::HighContrast)))
    );
}

#[test]
fn classify_key_value_arg_accepts_task_equals_value() {
    let parsed = classify_key_value_arg("--task=Docs").unwrap();
    assert_eq!(parsed, Some(ParsedToken::Task("Docs".to_string())));
}

#[test]
fn classify_key_value_arg_rejects_empty_task_equals_value() {
    let error = classify_key_value_arg("--task=").unwrap_err();
    assert!(error.contains("`--task=` requires a task label."));
}

#[test]
fn classify_key_value_arg_rejects_empty_profile_equals_value() {
    let error = classify_key_value_arg("--profile=").unwrap_err();
    assert!(error.contains("`--profile=` requires a profile value."));
}

#[test]
fn classify_key_value_arg_rejects_empty_theme_equals_value() {
    let error = classify_key_value_arg("--theme=").unwrap_err();
    assert!(error.contains("`--theme=` requires a theme value."));
}

#[test]
fn classify_key_value_arg_accepts_export_equals_value() {
    let parsed = classify_key_value_arg("--export=reports").unwrap();
    assert_eq!(
        parsed,
        Some(ParsedToken::Export(Some(PathBuf::from("reports"))))
    );
}

#[test]
fn classify_key_value_arg_accepts_backup_equals_value() {
    let parsed = classify_key_value_arg("--backup=reports").unwrap();
    assert_eq!(
        parsed,
        Some(ParsedToken::Backup(Some(PathBuf::from("reports"))))
    );
}

#[test]
fn classify_key_value_arg_accepts_restore_equals_value() {
    let parsed = classify_key_value_arg("--restore=reports").unwrap();
    assert_eq!(
        parsed,
        Some(ParsedToken::Restore(Some(PathBuf::from("reports"))))
    );
}

#[test]
fn classify_key_value_arg_accepts_goal_equals_value() {
    let parsed = classify_key_value_arg("--goal=90,3").unwrap();
    assert_eq!(
        parsed,
        Some(ParsedToken::Goal(Some(DailyGoalConfig {
            minutes: 90,
            pomodoros: 3
        })))
    );
}

#[test]
fn classify_key_value_arg_ignores_weekly_goal_equals_value() {
    assert_eq!(
        classify_key_value_arg("--goal-weekly=420,14").unwrap(),
        None
    );
}

#[test]
fn classify_key_value_arg_ignores_monthly_goal_equals_value() {
    assert_eq!(
        classify_key_value_arg("--goal-monthly=1800,60").unwrap(),
        None
    );
}

#[test]
fn classify_key_value_arg_accepts_strict_equals_value() {
    let parsed = classify_key_value_arg("--strict=off").unwrap();
    assert_eq!(parsed, Some(ParsedToken::Strict(Some(false))));
}

#[test]
fn classify_key_value_arg_accepts_goal_carry_equals_value() {
    let parsed = classify_key_value_arg("--goal-carry=on").unwrap();
    assert_eq!(parsed, Some(ParsedToken::GoalCarry(Some(true))));
}

#[test]
fn classify_key_value_arg_ignores_goal_carry_weekly_equals_value() {
    assert_eq!(
        classify_key_value_arg("--goal-carry-weekly=off").unwrap(),
        None
    );
}

#[test]
fn classify_key_value_arg_ignores_goal_carry_monthly_equals_value() {
    assert_eq!(
        classify_key_value_arg("--goal-carry-monthly=on").unwrap(),
        None
    );
}

/// Verifies key-value parsing accepts inline schedule JSON payloads.
#[test]
fn classify_key_value_arg_accepts_schedule_set_equals_value() {
    let payload =
        "--schedule-set={\"windows\":[{\"days\":[\"fri\"],\"start\":\"10:00\",\"end\":\"11:00\"}]}";
    let parsed = classify_key_value_arg(payload).unwrap();
    assert_eq!(
        parsed,
        Some(ParsedToken::ScheduleSet(RecurringScheduleConfig {
            windows: vec![RecurringFocusWindowConfig {
                days: vec!["fri".to_string()],
                start: "10:00".to_string(),
                end: "11:00".to_string(),
            }],
        }))
    );
}

#[test]
fn classify_key_value_arg_ignores_automation_triggers_set_equals_value() {
    let payload = "--automation-triggers-set=[{\"trigger\":{\"type\":\"schedule_window_start\"},\"action\":{\"type\":\"start_focus\"}}]";
    assert_eq!(classify_key_value_arg(payload).unwrap(), None);
}

#[test]
fn classify_key_value_arg_ignores_retired_history_dashboard_customization() {
    assert_eq!(
        classify_key_value_arg("--history-dashboard-pin=focus_score").unwrap(),
        None
    );
    assert_eq!(
        classify_key_value_arg("--history-dashboard-order=focus_score,goal_streak").unwrap(),
        None
    );
}

#[test]
fn classify_key_value_arg_rejects_empty_export_equals_value() {
    let error = classify_key_value_arg("--export=").unwrap_err();
    assert!(error.contains("`--export=` requires a target directory."));
}

#[test]
fn classify_key_value_arg_rejects_empty_backup_equals_value() {
    let error = classify_key_value_arg("--backup=").unwrap_err();
    assert!(error.contains("`--backup=` requires a target directory."));
}

#[test]
fn classify_key_value_arg_rejects_empty_restore_equals_value() {
    let error = classify_key_value_arg("--restore=").unwrap_err();
    assert!(error.contains("`--restore=` requires a source directory."));
}

#[test]
fn classify_key_value_arg_rejects_empty_goal_equals_value() {
    let error = classify_key_value_arg("--goal=").unwrap_err();
    assert!(error.contains("`--goal=` requires values"));
}

#[test]
fn classify_key_value_arg_ignores_empty_weekly_goal_equals_value() {
    assert_eq!(classify_key_value_arg("--goal-weekly=").unwrap(), None);
}

#[test]
fn classify_key_value_arg_ignores_empty_monthly_goal_equals_value() {
    assert_eq!(classify_key_value_arg("--goal-monthly=").unwrap(), None);
}

#[test]
fn classify_key_value_arg_rejects_empty_strict_equals_value() {
    let error = classify_key_value_arg("--strict=").unwrap_err();
    assert!(error.contains("`--strict=` requires `on` or `off`"));
}

#[test]
fn classify_key_value_arg_rejects_empty_goal_carry_equals_value() {
    let error = classify_key_value_arg("--goal-carry=").unwrap_err();
    assert!(error.contains("`--goal-carry=` requires `on` or `off`"));
}

#[test]
fn classify_key_value_arg_ignores_empty_goal_carry_weekly_equals_value() {
    assert_eq!(
        classify_key_value_arg("--goal-carry-weekly=").unwrap(),
        None
    );
}

#[test]
fn classify_key_value_arg_ignores_empty_goal_carry_monthly_equals_value() {
    assert_eq!(
        classify_key_value_arg("--goal-carry-monthly=").unwrap(),
        None
    );
}

#[test]
fn classify_key_value_arg_rejects_empty_schedule_set_equals_value() {
    let error = classify_key_value_arg("--schedule-set=").unwrap_err();
    assert!(error.contains("`--schedule-set=` requires a JSON payload."));
}

#[test]
fn parse_weekday_rules_set_equals_is_retired() {
    let error = parse(&["--weekday-rules-set=[]"]).unwrap_err();
    assert!(error.contains("Unknown option `--weekday-rules-set=[]`"));
}

#[test]
fn parse_automation_triggers_set_equals_is_retired() {
    let error = parse(&["--automation-triggers-set=[]"]).unwrap_err();
    assert!(error.contains("Unknown option `--automation-triggers-set=[]`"));
}

#[test]
fn parse_session_template_commands_are_retired() {
    for option in [
        "--session-template",
        "--session-template=Deep Flow",
        "--session-template-apply",
        "--session-template-apply=Deep Flow",
        "--session-template-create",
        "--session-template-create=Deep Flow",
        "--session-template-rename",
        "--session-template-rename=Deep Flow",
        "--session-template-delete",
        "--session-template-delete=Deep Flow",
    ] {
        let error = parse(&[option]).unwrap_err();
        assert!(error.contains("Session template commands were removed."));
    }
}

#[test]
fn parse_help_returns_show_help_action() {
    let parsed = parse(&["--help"]).unwrap();
    assert_eq!(parsed, CliAction::ShowHelp);
}

#[test]
fn parse_help_short_circuits_unknown_arguments() {
    let parsed = parse(&["--help", "--unknown"]).unwrap();
    assert_eq!(parsed, CliAction::ShowHelp);
}

#[test]
fn parse_rejects_task_without_value() {
    let error = parse(&["--task"]).unwrap_err();
    assert!(error.contains("`--task` requires a task label"));
}

#[test]
fn parse_rejects_task_with_blank_value() {
    let error = parse(&["--task", "   "]).unwrap_err();
    assert!(error.contains("`--task` requires a task label"));
}

#[test]
fn parse_rejects_task_note_with_blank_equals_value_as_removed_option() {
    let error = parse(&["--task-note="]).unwrap_err();
    assert!(error.contains("Unknown option `--task-note=`."));
}

#[test]
fn parse_rejects_theme_with_unknown_value() {
    let error = parse(&["--theme=solarized"]).unwrap_err();
    assert!(error.contains("Invalid theme preset"));
}

#[test]
fn parse_rejects_goal_without_two_numbers() {
    let error = parse(&["--goal=120"]).unwrap_err();
    assert!(error.contains("Invalid goal"));
}

#[test]
fn parse_rejects_weekly_goal_as_removed_even_with_invalid_shape() {
    let error = parse(&["--goal-weekly=120"]).unwrap_err();
    assert!(error.contains("Unknown option `--goal-weekly=120`"));
}

#[test]
fn parse_rejects_monthly_goal_as_removed_even_with_invalid_shape() {
    let error = parse(&["--goal-monthly=120"]).unwrap_err();
    assert!(error.contains("Unknown option `--goal-monthly=120`"));
}

#[test]
fn parse_rejects_strict_with_unknown_value() {
    let error = parse(&["--strict=enabled"]).unwrap_err();
    assert!(error.contains("Invalid strict mode"));
}

#[test]
fn parse_rejects_goal_carry_with_unknown_value() {
    let error = parse(&["--goal-carry=enabled"]).unwrap_err();
    assert!(error.contains("Invalid goal carry-over"));
}

#[test]
fn parse_rejects_schedule_set_without_payload() {
    let error = parse(&["--schedule-set"]).unwrap_err();
    assert!(error.contains("`--schedule-set` requires a JSON payload"));
}

#[test]
fn parse_rejects_weekday_rules_set_without_payload_as_retired() {
    let error = parse(&["--weekday-rules-set"]).unwrap_err();
    assert!(error.contains("Unknown option `--weekday-rules-set`"));
}

#[test]
fn parse_rejects_automation_triggers_set_without_payload_as_retired() {
    let error = parse(&["--automation-triggers-set"]).unwrap_err();
    assert!(error.contains("Unknown option `--automation-triggers-set`"));
}

#[test]
fn parse_rejects_blocklist_profile_create_without_value_as_retired() {
    let error = parse(&["--blocklist-profile-create"]).unwrap_err();
    assert!(error.contains("Unknown option `--blocklist-profile-create`"));
    assert!(error.contains("Blocklist profile commands were removed."));
}

#[test]
fn parse_rejects_blocklist_site_edit_without_old_new_separator() {
    let error = parse(&["--blocklist-site-edit=example.com"]).unwrap_err();
    assert!(error.contains("Invalid site edit"));
}

/// Verifies invalid schedule weekday tokens are rejected.
#[test]
fn parse_rejects_schedule_set_with_invalid_weekday() {
    let payload = r#"{"windows":[{"days":["nonday"],"start":"09:00","end":"10:00"}]}"#;
    let error =
        parse_args([OsString::from("--schedule-set"), OsString::from(payload)]).unwrap_err();
    assert!(error.contains("unknown weekday"));
}

#[test]
fn parse_rejects_schedule_set_with_deprecated_exception_dates() {
    let payload = r#"{"windows":[{"days":["mon"],"start":"09:00","end":"10:00"}],"exception_dates":["2026-99-99"]}"#;
    let error =
        parse_args([OsString::from("--schedule-set"), OsString::from(payload)]).unwrap_err();
    assert!(error.contains("exception_dates"));
}

#[test]
fn parse_rejects_schedule_set_with_deprecated_one_time_windows() {
    let payload = r#"{"windows":[],"one_time_windows":[]}"#;
    let error =
        parse_args([OsString::from("--schedule-set"), OsString::from(payload)]).unwrap_err();
    assert!(error.contains("one_time_windows"));
}

#[test]
fn parse_rejects_multiple_primary_commands() {
    let error = parse(&["--status", "--export"]).unwrap_err();
    assert!(error.contains("Multiple primary commands"));
}

#[test]
fn parse_rejects_multiple_primary_commands_for_backup_and_restore() {
    let error = parse(&["--backup", "--restore"]).unwrap_err();
    assert!(error.contains("Multiple primary commands"));
}

#[test]
fn parse_rejects_multiple_primary_commands_for_backup_and_export() {
    let error = parse(&["--backup", "--export"]).unwrap_err();
    assert!(error.contains("Multiple primary commands"));
}

#[test]
fn parse_rejects_unknown_option() {
    let error = parse(&["--unknown"]).unwrap_err();
    assert!(error.contains("Unknown option"));
}

#[test]
fn parse_schedule_delay_is_retired() {
    let error = parse(&["--schedule-delay"]).unwrap_err();
    assert!(error.contains("Unknown option `--schedule-delay`"));
}

#[test]
fn parse_rejects_json_without_noninteractive_command() {
    let error = parse(&["--json"]).unwrap_err();
    assert!(error.contains("`--json` is only valid"));
}

#[test]
fn parse_start_supports_json_mode() {
    let parsed = parse(&["--start", "--json"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Start,
            output: OutputMode::Json
        })
    );
}

#[test]
fn parse_rejects_watch_without_status() {
    let error = parse(&["--watch"]).unwrap_err();
    assert!(error.contains("`--watch` is only valid with `--status`"));
}

#[test]
fn parse_rejects_watch_with_non_status_command() {
    let error = parse(&["--export", "--watch"]).unwrap_err();
    assert!(error.contains("`--watch` is only valid with `--status`"));
}

#[test]
fn parse_rejects_removed_migrate_option() {
    let error = parse(&["--migrate"]).unwrap_err();
    assert!(error.contains("Unknown option `--migrate`"));
}

#[test]
fn parse_rejects_removed_dry_run_option() {
    let error = parse(&["--dry-run"]).unwrap_err();
    assert!(error.contains("Unknown option `--dry-run`"));
}

#[test]
fn parse_rejects_removed_usage_signals_option() {
    let error = parse(&["--usage-signals"]).unwrap_err();
    assert!(error.contains("Unknown option `--usage-signals`"));
}

#[test]
fn parse_rejects_removed_sync_backup_option() {
    let error = parse(&["--sync-backup"]).unwrap_err();
    assert!(error.contains("Unknown option `--sync-backup`"));
}

#[test]
fn parse_rejects_removed_sync_passphrase_option() {
    let error = parse(&["--sync-passphrase=secret"]).unwrap_err();
    assert!(error.contains("Unknown option `--sync-passphrase=secret`"));
}

#[test]
fn parse_rejects_backup_with_blank_positional_value() {
    let error = parse(&["--backup", "   "]).unwrap_err();
    assert!(error.contains("`--backup` requires a target directory."));
}

#[test]
fn parse_rejects_restore_with_blank_positional_value() {
    let error = parse(&["--restore", "   "]).unwrap_err();
    assert!(error.contains("`--restore` requires a source directory."));
}

#[test]
fn parse_rejects_export_with_blank_positional_value() {
    let error = parse(&["--export", "   "]).unwrap_err();
    assert!(error.contains("`--export` requires a target directory."));
}

#[test]
fn parse_rejects_watch_with_zero_seconds() {
    let error = parse(&["--status", "--watch=0"]).unwrap_err();
    assert!(error.contains("positive whole number of seconds"));
}

#[test]
fn parse_rejects_watch_with_non_numeric_seconds() {
    let error = parse(&["--status", "--watch=abc"]).unwrap_err();
    assert!(error.contains("positive whole number of seconds"));
}

#[test]
fn parse_rejects_duplicate_watch_flags() {
    let error = parse(&["--status", "--watch", "--watch=2"]).unwrap_err();
    assert!(error.contains("can only be specified once"));
}

#[test]
fn parse_with_contract_marks_json_usage_errors() {
    let error = parse_with_contract(&["--status", "--unknown", "--json"]).unwrap_err();
    assert_eq!(error.kind, CliErrorKind::Usage);
    assert_eq!(error.output, OutputMode::Json);
    assert_eq!(error.exit_code(), EXIT_CODE_USAGE_ERROR);
    assert_eq!(error.code, "cli.usage");
    assert!(error.message.contains("Unknown option"));
}

#[test]
fn parse_with_contract_treats_long_retired_sync_flags_as_plain_unknown_options() {
    let error = parse_with_contract(&["--sync-restore", "--json"]).unwrap_err();
    assert_eq!(error.kind, CliErrorKind::Usage);
    assert_eq!(error.output, OutputMode::Json);
    assert_eq!(error.exit_code(), EXIT_CODE_USAGE_ERROR);
    assert_eq!(error.code, "cli.usage");
    assert!(error.message.contains("Unknown option `--sync-restore`"));
    assert!(error.hint.is_none());
}

#[test]
fn parse_with_contract_treats_removed_usage_signals_as_plain_unknown_option() {
    let error = parse_with_contract(&["--usage-signals", "--json"]).unwrap_err();
    assert_eq!(error.kind, CliErrorKind::Usage);
    assert_eq!(error.output, OutputMode::Json);
    assert_eq!(error.exit_code(), EXIT_CODE_USAGE_ERROR);
    assert_eq!(error.code, "cli.usage");
    assert!(error.message.contains("Unknown option `--usage-signals`"));
    assert!(error.hint.is_none());
}

#[test]
fn parse_with_contract_scopes_replacement_hint_to_first_unknown_option() {
    let error = parse_with_contract(&["--unknown-flag", "--sync-restore", "--json"]).unwrap_err();
    assert_eq!(error.kind, CliErrorKind::Usage);
    assert_eq!(error.output, OutputMode::Json);
    assert_eq!(error.exit_code(), EXIT_CODE_USAGE_ERROR);
    assert_eq!(error.code, "cli.usage");
    assert!(error.message.contains("Unknown option `--unknown-flag`"));
    assert!(error.hint.is_none());
}

#[test]
fn parse_with_contract_detects_json_on_early_parse_failures() {
    let error = parse_with_contract(&["--schedule-set", "--json"]).unwrap_err();
    assert_eq!(error.kind, CliErrorKind::Usage);
    assert_eq!(error.output, OutputMode::Json);
    assert_eq!(error.exit_code(), EXIT_CODE_USAGE_ERROR);
    assert_eq!(error.code, "cli.usage");
    assert!(
        error
            .message
            .contains("`--schedule-set` requires a JSON payload")
    );
}

#[test]
fn parse_with_contract_marks_json_usage_errors_from_key_value_parsing() {
    let error = parse_with_contract(&["--goal=abc", "--json"]).unwrap_err();
    assert_eq!(error.kind, CliErrorKind::Usage);
    assert_eq!(error.output, OutputMode::Json);
    assert_eq!(error.exit_code(), EXIT_CODE_USAGE_ERROR);
    assert_eq!(error.code, "cli.usage");
    assert!(error.message.contains("Invalid goal"));
}

#[test]
fn parse_with_contract_marks_json_usage_errors_from_finalize_step() {
    let error = parse_with_contract(&["--watch", "--json"]).unwrap_err();
    assert_eq!(error.kind, CliErrorKind::Usage);
    assert_eq!(error.output, OutputMode::Json);
    assert_eq!(error.exit_code(), EXIT_CODE_USAGE_ERROR);
    assert_eq!(error.code, "cli.usage");
    assert!(
        error
            .message
            .contains("`--watch` is only valid with `--status`.")
    );
}

#[test]
fn runtime_error_preserves_user_message_code_and_hint() {
    let error = runtime_error(
        OutputMode::Json,
        crate::error::UserMessage::with_hint(
            "app.timer.not_running",
            "Cannot pause: timer is not running.",
            "Start a focus session first with `focustime --start`.",
        ),
    );

    assert_eq!(error.kind, CliErrorKind::Runtime);
    assert_eq!(error.output, OutputMode::Json);
    assert_eq!(error.exit_code(), EXIT_CODE_RUNTIME_ERROR);
    assert_eq!(error.code, "app.timer.not_running");
    assert_eq!(error.message, "Cannot pause: timer is not running.");
    assert_eq!(
        error.hint.as_deref(),
        Some("Start a focus session first with `focustime --start`.")
    );
}

#[cfg(unix)]
#[test]
fn parse_rejects_non_utf8_arguments() {
    let invalid = OsString::from_vec(vec![0x66, 0x6f, 0x80]);
    let error = parse_args(vec![invalid]).unwrap_err();
    assert!(error.contains("Arguments must be valid UTF-8."));
}

#[test]
fn blocklist_site_commands_collapse_existing_profiles_to_canonical_rules() {
    let mut config = AppConfig {
        blocklist_profiles: vec![
            crate::config::BlocklistProfileConfig {
                name: "Work".to_string(),
                sites: vec!["a.com".to_string()],
                allowlist_sites: Vec::new(),
            },
            crate::config::BlocklistProfileConfig {
                name: "Study".to_string(),
                sites: vec!["study.com".to_string(), "news.com".to_string()],
                allowlist_sites: vec!["news.com".to_string()],
            },
        ],
        selected_blocklist_profile: "work".to_string(),
        ..AppConfig::default()
    };

    let payload =
        apply_site_add_command(&mut config, SiteListTarget::Blocklist, "docs.example.com").unwrap();

    assert!(payload.updated);
    assert_eq!(config.selected_blocklist_profile, "Default");
    assert_eq!(config.blocklist_profiles.len(), 1);
    assert_eq!(config.blocklist_profiles[0].name, "Default");
    assert_eq!(
        config.blocklist_profiles[0].sites,
        vec![
            "a.com".to_string(),
            "study.com".to_string(),
            "news.com".to_string(),
            "docs.example.com".to_string()
        ]
    );
    assert_eq!(
        config.blocklist_profiles[0].allowlist_sites,
        vec!["news.com".to_string()]
    );
}

#[test]
fn apply_history_dashboard_show_uses_stable_default_layout() {
    let mut config = AppConfig {
        history_dashboard: crate::config::HistoryDashboardConfig {
            card_order: vec![
                HistoryKpiCardId::GoalStreak,
                HistoryKpiCardId::FocusScore,
                HistoryKpiCardId::SessionSummary,
                HistoryKpiCardId::FocusRisk,
                HistoryKpiCardId::WeeklyAllocation,
                HistoryKpiCardId::LastInterruption,
                HistoryKpiCardId::StatsGrowth,
                HistoryKpiCardId::Retention,
                HistoryKpiCardId::ComparisonFilters,
            ],
            pinned_cards: vec![HistoryKpiCardId::GoalStreak],
        },
        ..AppConfig::default()
    }
    .normalized();

    let payload =
        apply_history_dashboard_command(&mut config, HistoryDashboardCommandKind::Show).unwrap();

    assert_eq!(payload.action, "history-dashboard");
    assert_eq!(
        config.history_dashboard,
        crate::config::HistoryDashboardConfig::default()
    );
    assert_eq!(
        payload.cards.iter().map(|card| card.id).collect::<Vec<_>>(),
        HistoryKpiCardId::supported_dashboard_cards()
            .iter()
            .map(|card| card.id())
            .collect::<Vec<_>>()
    );
}

#[test]
fn apply_site_add_command_uses_profile_sites_not_selected_category() {
    let mut config: AppConfig = toml::from_str(
        r#"
selected_blocklist_profile = "Default"

[[blocklist_profiles]]
name = "Default"
sites = ["profile.com"]
allowlist_sites = []
selected_category = "Social"

[[blocklist_profiles.categories]]
name = "General"
sites = ["category-general.com"]
allowlist_sites = []

[[blocklist_profiles.categories]]
name = "Social"
sites = ["category-social.com"]
allowlist_sites = []
"#,
    )
    .unwrap();
    config = config.normalized();

    let payload =
        apply_site_add_command(&mut config, SiteListTarget::Blocklist, "new.com").unwrap();

    assert!(payload.updated);
    assert_eq!(
        config.blocklist_profiles[0].sites,
        vec![
            "category-general.com".to_string(),
            "profile.com".to_string(),
            "category-social.com".to_string(),
            "new.com".to_string()
        ]
    );
    assert_eq!(
        payload.sites,
        vec![
            "category-general.com".to_string(),
            "profile.com".to_string(),
            "category-social.com".to_string(),
            "new.com".to_string()
        ]
    );
}

#[test]
fn apply_site_edit_command_updates_blocklist_sites() {
    let mut config = AppConfig {
        blocklist_profiles: vec![crate::config::BlocklistProfileConfig {
            name: "Default".to_string(),
            sites: vec!["a.com".to_string(), "b.com".to_string()],
            allowlist_sites: Vec::new(),
        }],
        selected_blocklist_profile: "Default".to_string(),
        ..AppConfig::default()
    }
    .normalized();

    let payload = apply_site_edit_command(
        &mut config,
        SiteListTarget::Blocklist,
        &SiteEditValue {
            previous: "a.com".to_string(),
            next: "news.ycombinator.com".to_string(),
        },
    )
    .unwrap();

    assert!(payload.updated);
    assert_eq!(payload.previous, "a.com");
    assert_eq!(payload.current, "news.ycombinator.com");
    assert_eq!(
        config.blocklist_profiles[0].sites,
        vec!["news.ycombinator.com".to_string(), "b.com".to_string()]
    );
    assert!(config.blocked_sites.is_empty());
}

#[test]
fn apply_site_edit_command_handles_duplicate_case_entries() {
    let mut config = AppConfig {
        blocklist_profiles: vec![crate::config::BlocklistProfileConfig {
            name: "Default".to_string(),
            sites: vec![
                "A.com".to_string(),
                "a.com".to_string(),
                "b.com".to_string(),
            ],
            allowlist_sites: Vec::new(),
        }],
        selected_blocklist_profile: "Default".to_string(),
        ..AppConfig::default()
    };

    let payload = apply_site_edit_command(
        &mut config,
        SiteListTarget::Blocklist,
        &SiteEditValue {
            previous: "a.com".to_string(),
            next: "news.com".to_string(),
        },
    )
    .unwrap();

    assert!(payload.updated);
    assert_eq!(payload.previous, "a.com");
    assert_eq!(payload.current, "news.com");
    assert_eq!(
        config.blocklist_profiles[0].sites,
        vec!["news.com".to_string(), "b.com".to_string()]
    );
    assert!(config.blocked_sites.is_empty());
}

#[test]
fn apply_site_delete_command_does_not_restore_legacy_blocked_sites() {
    let mut config = AppConfig {
        blocked_sites: vec!["legacy.com".to_string()],
        blocklist_profiles: vec![crate::config::BlocklistProfileConfig {
            name: "Default".to_string(),
            sites: vec!["a.com".to_string()],
            allowlist_sites: Vec::new(),
        }],
        selected_blocklist_profile: "Default".to_string(),
        ..AppConfig::default()
    }
    .normalized();

    let payload =
        apply_site_delete_command(&mut config, SiteListTarget::Blocklist, "legacy.com").unwrap();

    assert!(payload.updated);
    assert_eq!(payload.removed, "legacy.com");
    assert!(config.blocked_sites.is_empty());
    assert_eq!(
        config.blocklist_profiles[0].sites,
        vec!["a.com".to_string()]
    );
    assert_eq!(payload.effective_blocked_sites_count, 1);
}

#[test]
fn apply_site_delete_command_handles_duplicate_case_entries() {
    let mut config = AppConfig {
        blocklist_profiles: vec![crate::config::BlocklistProfileConfig {
            name: "Default".to_string(),
            sites: vec![
                "example.com".to_string(),
                "Example.com".to_string(),
                "other.com".to_string(),
            ],
            allowlist_sites: Vec::new(),
        }],
        selected_blocklist_profile: "Default".to_string(),
        ..AppConfig::default()
    };

    let payload =
        apply_site_delete_command(&mut config, SiteListTarget::Blocklist, " EXAMPLE.COM ").unwrap();

    assert!(payload.updated);
    assert_eq!(payload.removed, "example.com");
    assert_eq!(
        config.blocklist_profiles[0].sites,
        vec!["other.com".to_string()]
    );
    assert!(config.blocked_sites.is_empty());
    assert_eq!(payload.effective_blocked_sites_count, 1);
}

#[test]
fn build_status_output_matches_blocklist_profile_case_insensitively() {
    let config = AppConfig {
        blocklist_profiles: vec![crate::config::BlocklistProfileConfig {
            name: "Work".to_string(),
            sites: vec!["youtube.com".to_string(), "reddit.com".to_string()],
            allowlist_sites: Vec::new(),
        }],
        selected_blocklist_profile: "work".to_string(),
        ..AppConfig::default()
    };
    let stats = FocusStats::default();

    let output = build_status_output(&config, &stats);

    assert_eq!(output.blocked_sites_count, 2);
}

#[test]
fn build_status_output_includes_selected_theme_preset() {
    let config = AppConfig {
        selected_theme_preset: ThemePreset::DeuteranopiaFriendly,
        ..AppConfig::default()
    };
    let stats = FocusStats::default();

    let output = build_status_output(&config, &stats);

    assert_eq!(output.selected_theme_preset.id, "deuteranopia-friendly");
    assert_eq!(output.selected_theme_preset.label, "Deuteranopia Friendly");
}

#[test]
fn build_status_output_excludes_allowlist_from_blocked_sites_count() {
    let config = AppConfig {
        blocklist_profiles: vec![crate::config::BlocklistProfileConfig {
            name: "Work".to_string(),
            sites: vec!["youtube.com".to_string(), "reddit.com".to_string()],
            allowlist_sites: vec!["reddit.com".to_string()],
        }],
        selected_blocklist_profile: "Work".to_string(),
        ..AppConfig::default()
    };
    let stats = FocusStats::default();

    let output = build_status_output(&config, &stats);

    assert_eq!(output.blocked_sites_count, 1);
}

#[test]
fn build_status_output_includes_growth_and_retention_signals() {
    let mut stats = FocusStats::default();
    let goal = DailyGoalSnapshot {
        minutes: 25,
        pomodoros: 1,
    };
    let today = NaiveDate::parse_from_str(&current_day_key(), "%Y-%m-%d")
        .expect("current day key should parse as a date");
    let old_day = today
        .checked_sub_signed(Duration::days(500))
        .unwrap()
        .format("%Y-%m-%d")
        .to_string();
    let recent_day = today.format("%Y-%m-%d").to_string();
    stats.record_completed_pomodoro_with_task(&old_day, goal, Some("Docs"), 25 * 60, None);
    stats.record_completed_pomodoro_with_task(&recent_day, goal, Some("Docs"), 25 * 60, None);

    let config = AppConfig::default();
    let output = build_status_output(&config, &stats);

    assert!(output.stats_growth.total_record_count > 0);
    assert!(output.stats_growth.estimated_bytes > 0);
    assert_eq!(output.stats_retention.preset, "balanced");
    assert_eq!(output.stats_retention.keep_daily_days, None);
    assert_eq!(output.stats_retention.keep_focus_sessions_days, Some(365));
    assert_eq!(
        output.stats_retention.keep_session_interruptions_days,
        Some(180)
    );
    assert_eq!(
        output.stats_retention.pending_prune.focus_sessions_removed,
        1
    );
}

#[cfg(any())]
#[test]
fn build_status_output_reports_daily_weekly_monthly_goal_state() {
    let mut stats = FocusStats::default();
    let today = current_day_key();
    let today_date = NaiveDate::parse_from_str(&today, "%Y-%m-%d")
        .expect("current day key should parse as a date");
    let weekly_day = (-6..=6)
        .filter(|offset| *offset != 0)
        .map(|offset| today_date + Duration::days(i64::from(offset)))
        .find(|candidate| candidate.iso_week() == today_date.iso_week())
        .expect("there should be at least one nearby day in the current ISO week");
    let monthly_day = (-31..=31)
        .filter(|offset| *offset != 0)
        .map(|offset| today_date + Duration::days(i64::from(offset)))
        .find(|candidate| {
            candidate.year() == today_date.year()
                && candidate.month() == today_date.month()
                && candidate.iso_week() != today_date.iso_week()
        })
        .expect("there should be at least one nearby day in the current month");
    let outside_period_day = today_date - Duration::days(40);

    let daily_snapshot = DailyGoalSnapshot {
        minutes: 30,
        pomodoros: 1,
    };
    stats.record_focus_elapsed(&today, 30 * 60, daily_snapshot);
    stats.record_completed_pomodoro(&today, daily_snapshot);
    let weekly_day_key = weekly_day.format("%Y-%m-%d").to_string();
    stats.record_focus_elapsed(&weekly_day_key, 20 * 60, daily_snapshot);
    stats.record_completed_pomodoro(&weekly_day_key, daily_snapshot);
    let monthly_day_key = monthly_day.format("%Y-%m-%d").to_string();
    stats.record_focus_elapsed(&monthly_day_key, 25 * 60, daily_snapshot);
    stats.record_completed_pomodoro(&monthly_day_key, daily_snapshot);
    let outside_period_day_key = outside_period_day.format("%Y-%m-%d").to_string();
    stats.record_focus_elapsed(&outside_period_day_key, 200 * 60, daily_snapshot);
    stats.record_completed_pomodoro(&outside_period_day_key, daily_snapshot);

    let in_period_config = AppConfig {
        daily_goal: DailyGoalConfig {
            minutes: 30,
            pomodoros: 1,
        },
        weekly_goal: WeeklyGoalConfig {
            minutes: 50,
            pomodoros: 2,
        },
        monthly_goal: MonthlyGoalConfig {
            minutes: 55,
            pomodoros: 2,
        },
        ..AppConfig::default()
    };

    let in_period_output = build_status_output(&in_period_config, &stats);

    assert!(in_period_output.goal.configured);
    assert!(in_period_output.goal.met);
    assert!(in_period_output.weekly_goal.configured);
    assert!(in_period_output.weekly_goal.met);
    assert!(in_period_output.monthly_goal.configured);
    assert!(in_period_output.monthly_goal.met);
    assert!(in_period_output.focus_score.available);
    assert_eq!(in_period_output.focus_score.completion_score_pct, Some(100));
    assert!(in_period_output.focus_score.focus_score_pct.is_some());
    assert_eq!(in_period_output.focus_risk.daily_goal.risk_score_pct, 0);
    assert!(!in_period_output.focus_risk.daily_goal.signals.is_empty());

    let boundary_config = AppConfig {
        daily_goal: DailyGoalConfig {
            minutes: 30,
            pomodoros: 1,
        },
        weekly_goal: WeeklyGoalConfig {
            minutes: 120,
            pomodoros: 3,
        },
        monthly_goal: MonthlyGoalConfig {
            minutes: 120,
            pomodoros: 3,
        },
        ..AppConfig::default()
    };

    let boundary_output = build_status_output(&boundary_config, &stats);

    assert!(boundary_output.goal.met);
    assert!(!boundary_output.weekly_goal.met);
    assert!(!boundary_output.monthly_goal.met);
    assert!(boundary_output.focus_score.available);
    assert!(boundary_output.focus_risk.alert_active());
}

#[test]
fn build_status_output_daily_goal_uses_persisted_same_day_snapshot() {
    let mut stats = FocusStats::default();
    let today = current_day_key();
    let persisted_snapshot = DailyGoalSnapshot {
        minutes: 30,
        pomodoros: 1,
    };
    stats.record_focus_elapsed(&today, 30 * 60, persisted_snapshot);
    stats.record_completed_pomodoro(&today, persisted_snapshot);

    let config = AppConfig {
        daily_goal: DailyGoalConfig {
            minutes: 120,
            pomodoros: 4,
        },
        ..AppConfig::default()
    };

    let output = build_status_output(&config, &stats);

    assert_eq!(output.goal.minutes_target, 30);
    assert_eq!(output.goal.pomodoros_target, 1);
    assert!(output.goal.met);
}

#[test]
fn build_status_output_keeps_selected_task_label_without_task_goal() {
    let mut stats = FocusStats::default();
    let changed =
        stats.update_task_planner_state(vec!["Docs".to_string()], Some("Docs".to_string()));
    assert!(changed);
    let config = AppConfig::default();

    let output = build_status_output(&config, &stats);

    assert_eq!(output.selected_task_label.as_deref(), Some("Docs"));
    assert!(output.focus_score.available);
    assert!(output.focus_score.focus_score_pct.is_some());
    assert!(!output.focus_risk.daily_goal.configured);
    assert!(!output.focus_risk.streak.configured);
}

#[test]
fn build_status_output_does_not_mirror_task_label_metadata() {
    let mut stats = FocusStats::default();
    let changed =
        stats.update_task_planner_state(vec!["Docs".to_string()], Some("Docs".to_string()));
    assert!(changed);
    let config = AppConfig::default();

    let output = build_status_output(&config, &stats);

    assert_eq!(output.selected_task_label.as_deref(), Some("Docs"));
    assert_eq!(output.live.selected_task_label.as_deref(), Some("Docs"));
}

#[test]
fn build_status_output_applies_carry_over_to_goal_targets_when_enabled() {
    let mut stats = FocusStats::default();
    let today = current_day_key();
    let today_date = NaiveDate::parse_from_str(&today, "%Y-%m-%d")
        .expect("current day key should parse as a date");
    let yesterday = today_date.pred_opt().expect("yesterday should exist");
    let yesterday_key = yesterday.format("%Y-%m-%d").to_string();
    let previous_daily_goal = DailyGoalSnapshot {
        minutes: 50,
        pomodoros: 3,
    };

    stats.record_focus_elapsed(&yesterday_key, 30 * 60, previous_daily_goal);
    stats.record_completed_pomodoro(&yesterday_key, previous_daily_goal);

    let base_daily_goal = DailyGoalSnapshot {
        minutes: 60,
        pomodoros: 2,
    };
    stats.record_focus_elapsed(&today, 40 * 60, base_daily_goal);
    stats.record_completed_pomodoro(&today, base_daily_goal);

    let config = AppConfig {
        daily_goal: DailyGoalConfig {
            minutes: 60,
            pomodoros: 2,
        },
        goal_carry_over: crate::config::GoalCarryOverConfig {
            daily: true,
            weekly: false,
            monthly: false,
        },
        ..AppConfig::default()
    };

    let output = build_status_output(&config, &stats);
    assert_eq!(output.goal.minutes_target, 80);
    assert_eq!(output.goal.pomodoros_target, 4);
    assert!(output.goal.carry_over);
}

#[test]
fn build_status_output_daily_carry_over_does_not_reapply_older_day_debt() {
    let mut stats = FocusStats::default();
    let today = current_day_key();
    let today_date = NaiveDate::parse_from_str(&today, "%Y-%m-%d")
        .expect("current day key should parse as a date");
    let yesterday = today_date.pred_opt().expect("yesterday should exist");
    let yesterday_key = yesterday.format("%Y-%m-%d").to_string();
    let day_before = yesterday.pred_opt().expect("day before should exist");
    let day_before_key = day_before.format("%Y-%m-%d").to_string();

    stats.record_focus_elapsed(
        &day_before_key,
        30 * 60,
        DailyGoalSnapshot {
            minutes: 50,
            pomodoros: 2,
        },
    );
    stats.record_completed_pomodoro(
        &day_before_key,
        DailyGoalSnapshot {
            minutes: 50,
            pomodoros: 2,
        },
    );
    stats.insert_daily_for_tests(
        &yesterday_key,
        crate::stats::DailyStats {
            pomodoros_completed: 0,
            focused_seconds: 0,
            goal: Some(DailyGoalSnapshot {
                minutes: 60,
                pomodoros: 2,
            }),
        },
    );

    let config = AppConfig {
        daily_goal: DailyGoalConfig {
            minutes: 60,
            pomodoros: 2,
        },
        goal_carry_over: crate::config::GoalCarryOverConfig {
            daily: true,
            ..crate::config::GoalCarryOverConfig::default()
        },
        ..AppConfig::default()
    };

    let output = build_status_output(&config, &stats);
    assert_eq!(output.goal.minutes_target, 120);
    assert_eq!(output.goal.pomodoros_target, 4);
}

#[test]
fn build_status_output_daily_carry_over_skips_when_previous_day_goal_is_absent() {
    let mut stats = FocusStats::default();
    let today = current_day_key();
    let today_date = NaiveDate::parse_from_str(&today, "%Y-%m-%d")
        .expect("current day key should parse as a date");
    let yesterday = today_date.pred_opt().expect("yesterday should exist");
    let yesterday_key = yesterday.format("%Y-%m-%d").to_string();
    stats.insert_daily_for_tests(
        &yesterday_key,
        crate::stats::DailyStats {
            pomodoros_completed: 4,
            focused_seconds: 120 * 60,
            goal: None,
        },
    );

    let config = AppConfig {
        daily_goal: DailyGoalConfig {
            minutes: 60,
            pomodoros: 2,
        },
        goal_carry_over: crate::config::GoalCarryOverConfig {
            daily: true,
            ..crate::config::GoalCarryOverConfig::default()
        },
        ..AppConfig::default()
    };

    let output = build_status_output(&config, &stats);
    assert_eq!(output.goal.minutes_target, 60);
    assert_eq!(output.goal.pomodoros_target, 2);
}

#[cfg(any())]
#[test]
fn build_status_output_applies_weekly_carry_over_to_goal_targets_when_enabled() {
    let mut stats = FocusStats::default();
    let today = current_day_key();
    let today_date = NaiveDate::parse_from_str(&today, "%Y-%m-%d")
        .expect("current day key should parse as a date");
    let previous_week_day = today_date - Duration::days(7);
    let previous_week_key = previous_week_day.format("%Y-%m-%d").to_string();
    let goal = DailyGoalSnapshot {
        minutes: 60,
        pomodoros: 2,
    };
    stats.sync_weekly_goal_snapshot(
        previous_week_day,
        DailyGoalSnapshot {
            minutes: 50,
            pomodoros: 2,
        },
    );
    stats.record_focus_elapsed(&previous_week_key, 20 * 60, goal);
    stats.record_completed_pomodoro(&previous_week_key, goal);
    stats.record_focus_elapsed(&today, 40 * 60, goal);
    stats.record_completed_pomodoro(&today, goal);

    let config = AppConfig {
        weekly_goal: WeeklyGoalConfig {
            minutes: 100,
            pomodoros: 3,
        },
        goal_carry_over: crate::config::GoalCarryOverConfig {
            weekly: true,
            ..crate::config::GoalCarryOverConfig::default()
        },
        ..AppConfig::default()
    };

    let output = build_status_output(&config, &stats);
    assert_eq!(output.weekly_goal.minutes_target, 130);
    assert_eq!(output.weekly_goal.pomodoros_target, 4);
    assert!(output.weekly_goal.carry_over);
}

#[cfg(any())]
#[test]
fn build_status_output_weekly_carry_over_skips_when_previous_period_has_no_snapshot() {
    let mut stats = FocusStats::default();
    let today = current_day_key();
    let today_date = NaiveDate::parse_from_str(&today, "%Y-%m-%d")
        .expect("current day key should parse as a date");
    let previous_week_day = today_date - Duration::days(7);
    let previous_week_key = previous_week_day.format("%Y-%m-%d").to_string();
    let goal = DailyGoalSnapshot {
        minutes: 60,
        pomodoros: 2,
    };
    stats.record_focus_elapsed(&previous_week_key, 70 * 60, goal);
    stats.record_completed_pomodoro(&previous_week_key, goal);

    let config = AppConfig {
        weekly_goal: WeeklyGoalConfig {
            minutes: 100,
            pomodoros: 3,
        },
        goal_carry_over: crate::config::GoalCarryOverConfig {
            weekly: true,
            ..crate::config::GoalCarryOverConfig::default()
        },
        ..AppConfig::default()
    };

    let output = build_status_output(&config, &stats);
    assert_eq!(output.weekly_goal.minutes_target, 100);
    assert_eq!(output.weekly_goal.pomodoros_target, 3);
}

#[cfg(any())]
#[test]
fn build_status_output_includes_weekly_allocation_breakdown() {
    let mut stats = FocusStats::default();
    let today = current_day_key();
    let baseline_goal = DailyGoalSnapshot::default();
    stats.record_focus_elapsed(&today, 30 * 60, baseline_goal);
    stats.record_completed_pomodoro(&today, baseline_goal);

    let config = AppConfig {
        weekly_goal: WeeklyGoalConfig {
            minutes: 210,
            pomodoros: 7,
        },
        recurring_schedule: RecurringScheduleConfig {
            windows: vec![RecurringFocusWindowConfig {
                days: vec![
                    "mon".to_string(),
                    "tue".to_string(),
                    "wed".to_string(),
                    "thu".to_string(),
                    "fri".to_string(),
                    "sat".to_string(),
                    "sun".to_string(),
                ],
                start: "09:00".to_string(),
                end: "10:00".to_string(),
            }],
        },
        ..AppConfig::default()
    };

    let output = build_status_output(&config, &stats);
    assert!(output.weekly_allocation.available);
    assert_eq!(
        output.weekly_allocation.allocatable_days,
        output.weekly_allocation.remaining_days_in_week
    );
    assert!(!output.weekly_allocation.days.is_empty());
    assert_eq!(
        output.weekly_allocation.today_minutes_target,
        output.weekly_allocation.days[0].minutes_target
    );
    assert_eq!(
        output.weekly_allocation.today_pomodoros_target,
        output.weekly_allocation.days[0].pomodoros_target
    );
    assert_eq!(
        output
            .weekly_allocation
            .days
            .iter()
            .map(|day| day.minutes_target)
            .sum::<u64>(),
        output.weekly_allocation.remaining_minutes
    );
    assert_eq!(
        output
            .weekly_allocation
            .days
            .iter()
            .map(|day| day.pomodoros_target)
            .sum::<u32>(),
        output.weekly_allocation.remaining_pomodoros
    );
}

#[cfg(any())]
#[test]
fn build_status_output_weekly_allocation_uses_equal_split_fallback_without_schedule() {
    let config = AppConfig {
        weekly_goal: WeeklyGoalConfig {
            minutes: 90,
            pomodoros: 3,
        },
        ..AppConfig::default()
    };
    let output = build_status_output(&config, &FocusStats::default());

    assert!(output.weekly_allocation.available);
    assert!(!output.weekly_allocation.uses_schedule_weights);
    assert_eq!(
        output.weekly_allocation.allocatable_days,
        output.weekly_allocation.remaining_days_in_week
    );
}

#[cfg(any())]
#[test]
fn build_status_output_monthly_carry_over_uses_previous_snapshot_after_goal_change() {
    let mut stats = FocusStats::default();
    let today = current_day_key();
    let today_date = NaiveDate::parse_from_str(&today, "%Y-%m-%d")
        .expect("current day key should parse as a date");
    let month_start = NaiveDate::from_ymd_opt(today_date.year(), today_date.month(), 1)
        .expect("month start should be representable");
    let previous_month_day = month_start
        .pred_opt()
        .expect("previous month day should be representable");
    let previous_month_key = previous_month_day.format("%Y-%m-%d").to_string();
    let goal = DailyGoalSnapshot {
        minutes: 60,
        pomodoros: 2,
    };
    stats.sync_monthly_goal_snapshot(
        previous_month_day,
        DailyGoalSnapshot {
            minutes: 200,
            pomodoros: 6,
        },
    );
    stats.record_focus_elapsed(&previous_month_key, 120 * 60, goal);
    for _ in 0..4 {
        stats.record_completed_pomodoro(&previous_month_key, goal);
    }

    let config = AppConfig {
        monthly_goal: MonthlyGoalConfig {
            minutes: 300,
            pomodoros: 10,
        },
        goal_carry_over: crate::config::GoalCarryOverConfig {
            monthly: true,
            ..crate::config::GoalCarryOverConfig::default()
        },
        ..AppConfig::default()
    };

    let output = build_status_output(&config, &stats);
    assert_eq!(output.monthly_goal.minutes_target, 380);
    assert_eq!(output.monthly_goal.pomodoros_target, 12);
}

#[test]
fn build_status_output_uses_recovery_snapshot_for_live_state() {
    session_recovery::set_test_load_snapshot(Some(InProgressSessionSnapshot {
        phase: RecoveryTimerPhase::Focus,
        status: RecoveryTimerStatus::Running,
        remaining_secs: 42,
        pomodoros_completed: 3,
        selected_task_label: Some("Docs".to_string()),
        selected_profile: ProfileId::DeepWork,
        captured_at_epoch_secs: None,
    }));
    let config = AppConfig::default();
    let stats = FocusStats::default();

    let output = build_status_output(&config, &stats);

    assert!(output.live.in_progress);
    assert_eq!(output.live.state_source, "recovery");
    assert_eq!(output.live.phase, "focus");
    assert_eq!(output.live.status, "running");
    assert_eq!(output.live.remaining_secs, 42);
    assert_eq!(output.live.pomodoros_completed, 3);
    assert_eq!(output.live.selected_task_label.as_deref(), Some("Docs"));
    assert_eq!(output.live.selected_profile.id, "standard");
    assert!(output.live.recovery_error.is_none());
    assert_eq!(output.session.pomodoros_completed, 3);
    assert_eq!(output.session.focused_minutes, 199);
}

#[test]
fn build_status_output_reconciles_elapsed_running_recovery_snapshot() {
    session_recovery::set_test_load_snapshot(Some(InProgressSessionSnapshot {
        phase: RecoveryTimerPhase::Focus,
        status: RecoveryTimerStatus::Running,
        remaining_secs: 1,
        pomodoros_completed: 0,
        selected_task_label: Some("Docs".to_string()),
        selected_profile: ProfileId::Classic,
        captured_at_epoch_secs: Some(0),
    }));
    let config = AppConfig::default();
    let stats = FocusStats::default();

    let output = build_status_output(&config, &stats);

    assert!(output.live.in_progress);
    assert_eq!(output.live.state_source, "recovery");
    assert_eq!(output.live.phase, "short-break");
    assert_eq!(output.live.status, "idle");
    assert_eq!(output.live.pomodoros_completed, 1);
    assert_eq!(output.live.remaining_secs, DEFAULT_SHORT_BREAK_SECS);
    assert_eq!(output.session.pomodoros_completed, 1);
    assert_eq!(output.session.focused_minutes, DEFAULT_FOCUS_SECS / 60);
    assert_eq!(output.live.selected_task_label.as_deref(), Some("Docs"));
    assert!(!output.live.strict_mode_enforced);
}

#[test]
fn build_status_output_includes_latest_session_interruption() {
    let mut stats = FocusStats::default();
    stats.record_session_interruption_event(
        "2026-04-09",
        1_711_000_123,
        crate::stats::SessionInterruptionReason::ManualSkip,
        crate::stats::FocusSessionMetadata {
            task_label: Some("Docs"),
        },
        600,
        Some(ProfileId::Classic),
    );

    let output = build_status_output(&AppConfig::default(), &stats);
    let interruption = output
        .latest_interruption
        .expect("latest interruption should be present");
    assert_eq!(
        interruption.reason,
        crate::stats::SessionInterruptionReason::ManualSkip
    );
    assert_eq!(interruption.task_label.as_deref(), Some("Docs"));
    assert_eq!(interruption.remaining_secs, 600);
}

#[test]
fn build_status_output_reports_recovery_error_without_failing() {
    session_recovery::set_test_load_error("simulated load failure");
    let config = AppConfig::default();
    let stats = FocusStats::default();

    let output = build_status_output(&config, &stats);

    assert!(!output.live.in_progress);
    assert_eq!(output.live.state_source, "default");
    assert_eq!(
        output.live.recovery_error.as_deref(),
        Some("simulated load failure")
    );
    assert_eq!(output.live.status, "idle");
}

#[test]
fn cli_resume_requires_paused_timer() {
    let mut app = App::default();
    let error = app.resume_for_cli().unwrap_err();
    assert_eq!(error, "Cannot resume: timer is not paused.");
}

#[test]
fn cli_stop_requires_non_idle_timer() {
    let mut app = App::default();
    let error = app.stop_for_cli().unwrap_err();
    assert_eq!(error, "Cannot stop: timer is already idle.");
}
