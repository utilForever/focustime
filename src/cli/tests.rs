use crate::cli::*;
use crate::config::FeatureFlagsConfig;
use crate::session_recovery::{
    self, InProgressSessionSnapshot, RecoveryTimerPhase, RecoveryTimerStatus,
};
use chrono::{Datelike, Duration};
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;

fn parse(values: &[&str]) -> Result<CliAction, String> {
    parse_args(values.iter().map(OsString::from))
}

fn parse_with_contract(values: &[&str]) -> Result<CliAction, CliError> {
    parse_args_with_contract(values.iter().map(OsString::from))
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

#[test]
fn parse_status_supports_json_mode() {
    let parsed = parse(&["--status", "--json"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Status {
                watch_interval_secs: None
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
                watch_interval_secs: Some(DEFAULT_WATCH_INTERVAL_SECS)
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
                watch_interval_secs: Some(3)
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
                watch_interval_secs: Some(2)
            },
            output: OutputMode::Text
        })
    );
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
fn parse_task_goal_without_value_reads_selected_label_goal() {
    let parsed = parse(&["--task-goal"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::TaskGoal {
                label: None,
                goal: None
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_task_goal_with_label_reads_specific_goal() {
    let parsed = parse(&["--task-goal", "Docs"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::TaskGoal {
                label: Some("Docs".to_string()),
                goal: None
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_task_goal_with_label_and_target_sets_goal() {
    let parsed = parse(&["--task-goal", "Docs:120,4"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::TaskGoal {
                label: Some("Docs".to_string()),
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
fn parse_task_goal_with_colon_in_label_reads_specific_goal() {
    let parsed = parse(&["--task-goal", "Docs:API"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::TaskGoal {
                label: Some("Docs:API".to_string()),
                goal: None
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_focus_intention_without_value_reads_current_metadata() {
    let parsed = parse(&["--focus-intention"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::FocusIntention { value: None },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_focus_intention_with_equals_sets_metadata() {
    let parsed = parse(&["--focus-intention=Write docs"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::FocusIntention {
                value: Some("Write docs".to_string())
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_task_note_without_value_reads_current_metadata() {
    let parsed = parse(&["--task-note"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::TaskNote { value: None },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_task_note_with_value_sets_metadata() {
    let parsed = parse(&["--task-note", "Capture blockers"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::TaskNote {
                value: Some("Capture blockers".to_string())
            },
            output: OutputMode::Text
        })
    );
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
fn parse_weekly_goal_without_value_reads_current_goal() {
    let parsed = parse(&["--goal-weekly"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::GoalWeekly { goal: None },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_weekly_goal_with_equals_sets_goal() {
    let parsed = parse(&["--goal-weekly=420,14"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::GoalWeekly {
                goal: Some(WeeklyGoalConfig {
                    minutes: 420,
                    pomodoros: 14
                })
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_monthly_goal_with_value_sets_goal() {
    let parsed = parse(&["--goal-monthly", "1800,60"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::GoalMonthly {
                goal: Some(MonthlyGoalConfig {
                    minutes: 1800,
                    pomodoros: 60
                })
            },
            output: OutputMode::Text
        })
    );
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
fn parse_goal_carry_weekly_with_equals_sets_state() {
    let parsed = parse(&["--goal-carry-weekly=on"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::GoalCarryWeekly {
                enabled: Some(true)
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_goal_carry_monthly_with_value_sets_state() {
    let parsed = parse(&["--goal-carry-monthly", "off"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::GoalCarryMonthly {
                enabled: Some(false)
            },
            output: OutputMode::Text
        })
    );
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
fn parse_schedule_delay_supports_json_mode() {
    let parsed = parse(&["--schedule-delay", "--json"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::ScheduleDelay,
            output: OutputMode::Json
        })
    );
}

#[test]
fn parse_break_glass_trigger_supports_json_mode() {
    let parsed = parse(&["--break-glass-trigger", "--json"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::BreakGlassTrigger,
            output: OutputMode::Json
        })
    );
}

#[test]
fn parse_break_glass_cancel_defaults_to_text_mode() {
    let parsed = parse(&["--break-glass-cancel"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::BreakGlassCancel,
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_schedule_set_accepts_json_payload() {
    let payload = r#"{"windows":[{"days":["mon","wed"],"start":"09:00","end":"11:00"}],"exception_dates":["2026-12-25"]}"#;
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
                    exception_dates: vec!["2026-12-25".to_string()],
                    one_time_windows: Vec::new(),
                }),
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_schedule_set_accepts_one_time_windows_payload() {
    let payload = r#"{"windows":[],"exception_dates":[],"one_time_windows":[{"date":"2026-05-02","start":"14:00","end":"15:30"}]}"#;
    let parsed = parse_args([OsString::from("--schedule-set"), OsString::from(payload)]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Schedule {
                schedule: Some(RecurringScheduleConfig {
                    windows: Vec::new(),
                    exception_dates: Vec::new(),
                    one_time_windows: vec![OneTimeFocusWindowConfig {
                        date: "2026-05-02".to_string(),
                        start: "14:00".to_string(),
                        end: "15:30".to_string(),
                    }],
                }),
            },
            output: OutputMode::Text
        })
    );
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
        ..RecurringScheduleConfig::default()
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
        ..RecurringScheduleConfig::default()
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
fn diagnostics_output_includes_effective_feature_flags() {
    let app = App::default();
    let payload = build_diagnostics_command_output(&app.setup_diagnostics);

    assert!(payload.feature_flags.metadata_task_label_fallback);
    assert!(payload.deprecation_warnings.is_empty());
}

#[test]
fn diagnostics_output_includes_deprecation_warnings() {
    let mut app = App::default();
    app.setup_diagnostics.deprecation_warnings = vec![
        "Deprecated top-level timer fields are in use.".to_string(),
        "Deprecated legacy stats path detected.".to_string(),
    ];

    let payload = build_diagnostics_command_output(&app.setup_diagnostics);

    assert_eq!(
        payload.deprecation_warnings,
        vec![
            "Deprecated top-level timer fields are in use.".to_string(),
            "Deprecated legacy stats path detected.".to_string()
        ]
    );
}

#[test]
fn parse_blocking_preview_supports_json_mode() {
    let parsed = parse(&["--blocking-preview", "--json"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::BlockingPreview,
            output: OutputMode::Json
        })
    );
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
fn parse_migrate_runs_command() {
    let parsed = parse(&["--migrate"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Migrate { dry_run: false },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_migrate_supports_dry_run_and_json_mode() {
    let parsed = parse(&["--migrate", "--dry-run", "--json"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::Migrate { dry_run: true },
            output: OutputMode::Json
        })
    );
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
fn parse_blocklist_profile_without_value_reads_current_profile() {
    let parsed = parse(&["--blocklist-profile"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::BlocklistProfile {
                command: BlocklistProfileCommandKind::Select { profile: None }
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_blocklist_profile_with_value_selects_profile() {
    let parsed = parse(&["--blocklist-profile", "Work"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::BlocklistProfile {
                command: BlocklistProfileCommandKind::Select {
                    profile: Some("Work".to_string())
                }
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_blocklist_profile_rename_with_equals() {
    let parsed = parse(&["--blocklist-profile-rename=Deep Work"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::BlocklistProfile {
                command: BlocklistProfileCommandKind::Rename {
                    name: "Deep Work".to_string()
                }
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_blocklist_profile_delete_runs_command() {
    let parsed = parse(&["--blocklist-profile-delete"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::BlocklistProfile {
                command: BlocklistProfileCommandKind::Delete
            },
            output: OutputMode::Text
        })
    );
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
fn parse_allowlist_site_delete_with_equals() {
    let parsed = parse(&["--allowlist-site-delete=reddit.com"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::BlocklistSites {
                target: SiteListTarget::Allowlist,
                command: BlocklistSiteCommandKind::Delete {
                    site: "reddit.com".to_string()
                }
            },
            output: OutputMode::Text
        })
    );
}

#[test]
fn parse_allowlist_site_edit_with_equals() {
    let parsed = parse(&["--allowlist-site-edit=old.com=new.com"]).unwrap();
    assert_eq!(
        parsed,
        CliAction::RunCommand(CliCommand {
            kind: CommandKind::BlocklistSites {
                target: SiteListTarget::Allowlist,
                command: BlocklistSiteCommandKind::Edit {
                    value: SiteEditValue {
                        previous: "old.com".to_string(),
                        next: "new.com".to_string()
                    }
                }
            },
            output: OutputMode::Text
        })
    );
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
fn classify_key_value_arg_accepts_task_goal_equals_label() {
    let parsed = classify_key_value_arg("--task-goal=Docs").unwrap();
    assert_eq!(
        parsed,
        Some(ParsedToken::TaskGoal {
            label: Some("Docs".to_string()),
            goal: None
        })
    );
}

#[test]
fn classify_key_value_arg_accepts_task_goal_equals_label_and_target() {
    let parsed = classify_key_value_arg("--task-goal=Docs:90,3").unwrap();
    assert_eq!(
        parsed,
        Some(ParsedToken::TaskGoal {
            label: Some("Docs".to_string()),
            goal: Some(DailyGoalConfig {
                minutes: 90,
                pomodoros: 3
            })
        })
    );
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
fn classify_key_value_arg_accepts_weekly_goal_equals_value() {
    let parsed = classify_key_value_arg("--goal-weekly=420,14").unwrap();
    assert_eq!(
        parsed,
        Some(ParsedToken::GoalWeekly(Some(WeeklyGoalConfig {
            minutes: 420,
            pomodoros: 14
        })))
    );
}

#[test]
fn classify_key_value_arg_accepts_monthly_goal_equals_value() {
    let parsed = classify_key_value_arg("--goal-monthly=1800,60").unwrap();
    assert_eq!(
        parsed,
        Some(ParsedToken::GoalMonthly(Some(MonthlyGoalConfig {
            minutes: 1800,
            pomodoros: 60
        })))
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
fn classify_key_value_arg_accepts_goal_carry_weekly_equals_value() {
    let parsed = classify_key_value_arg("--goal-carry-weekly=off").unwrap();
    assert_eq!(parsed, Some(ParsedToken::GoalCarryWeekly(Some(false))));
}

#[test]
fn classify_key_value_arg_accepts_goal_carry_monthly_equals_value() {
    let parsed = classify_key_value_arg("--goal-carry-monthly=on").unwrap();
    assert_eq!(parsed, Some(ParsedToken::GoalCarryMonthly(Some(true))));
}

#[test]
fn classify_key_value_arg_accepts_schedule_set_equals_value() {
    let payload = "--schedule-set={\"windows\":[{\"days\":[\"fri\"],\"start\":\"10:00\",\"end\":\"11:00\"}],\"exception_dates\":[]}";
    let parsed = classify_key_value_arg(payload).unwrap();
    assert_eq!(
        parsed,
        Some(ParsedToken::ScheduleSet(RecurringScheduleConfig {
            windows: vec![RecurringFocusWindowConfig {
                days: vec!["fri".to_string()],
                start: "10:00".to_string(),
                end: "11:00".to_string(),
            }],
            exception_dates: Vec::new(),
            one_time_windows: Vec::new(),
        }))
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
fn classify_key_value_arg_rejects_empty_weekly_goal_equals_value() {
    let error = classify_key_value_arg("--goal-weekly=").unwrap_err();
    assert!(error.contains("`--goal-weekly=` requires values"));
}

#[test]
fn classify_key_value_arg_rejects_empty_monthly_goal_equals_value() {
    let error = classify_key_value_arg("--goal-monthly=").unwrap_err();
    assert!(error.contains("`--goal-monthly=` requires values"));
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
fn classify_key_value_arg_rejects_empty_goal_carry_weekly_equals_value() {
    let error = classify_key_value_arg("--goal-carry-weekly=").unwrap_err();
    assert!(error.contains("`--goal-carry-weekly=` requires `on` or `off`"));
}

#[test]
fn classify_key_value_arg_rejects_empty_goal_carry_monthly_equals_value() {
    let error = classify_key_value_arg("--goal-carry-monthly=").unwrap_err();
    assert!(error.contains("`--goal-carry-monthly=` requires `on` or `off`"));
}

#[test]
fn classify_key_value_arg_rejects_empty_schedule_set_equals_value() {
    let error = classify_key_value_arg("--schedule-set=").unwrap_err();
    assert!(error.contains("`--schedule-set=` requires a JSON payload."));
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
fn parse_rejects_focus_intention_with_blank_value() {
    let error = parse(&["--focus-intention", "   "]).unwrap_err();
    assert!(error.contains("`--focus-intention` requires a value"));
}

#[test]
fn parse_rejects_task_note_with_blank_equals_value() {
    let error = parse(&["--task-note="]).unwrap_err();
    assert!(error.contains("`--task-note=` requires a non-empty value."));
}

#[test]
fn parse_rejects_task_goal_with_blank_label() {
    let error = parse(&["--task-goal=:120,4"]).unwrap_err();
    assert!(error.contains("Task goal requires a task label before `:`."));
}

#[test]
fn parse_rejects_task_goal_with_non_numeric_pomodoros_suffix() {
    let error = parse(&["--task-goal=Docs:120,abc"]).unwrap_err();
    assert!(error.contains("Invalid goal pomodoros"));
}

#[test]
fn parse_rejects_theme_with_unknown_value() {
    let error = parse(&["--theme=solarized"]).unwrap_err();
    assert!(error.contains("Invalid theme preset"));
}

#[test]
fn parse_rejects_task_goal_with_extra_goal_components() {
    let error = parse(&["--task-goal=Docs:120,4,5"]).unwrap_err();
    assert!(error.contains("Invalid goal pomodoros"));
}

#[test]
fn parse_rejects_goal_without_two_numbers() {
    let error = parse(&["--goal=120"]).unwrap_err();
    assert!(error.contains("Invalid goal"));
}

#[test]
fn parse_rejects_weekly_goal_without_two_numbers() {
    let error = parse(&["--goal-weekly=120"]).unwrap_err();
    assert!(error.contains("Invalid goal"));
}

#[test]
fn parse_rejects_monthly_goal_without_two_numbers() {
    let error = parse(&["--goal-monthly=120"]).unwrap_err();
    assert!(error.contains("Invalid goal"));
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
fn parse_rejects_blocklist_profile_create_without_value() {
    let error = parse(&["--blocklist-profile-create"]).unwrap_err();
    assert!(error.contains("`--blocklist-profile-create` requires a profile name"));
}

#[test]
fn parse_rejects_blocklist_site_edit_without_old_new_separator() {
    let error = parse(&["--blocklist-site-edit=example.com"]).unwrap_err();
    assert!(error.contains("Invalid site edit"));
}

#[test]
fn parse_rejects_schedule_set_with_invalid_weekday() {
    let payload =
        r#"{"windows":[{"days":["nonday"],"start":"09:00","end":"10:00"}],"exception_dates":[]}"#;
    let error =
        parse_args([OsString::from("--schedule-set"), OsString::from(payload)]).unwrap_err();
    assert!(error.contains("unknown weekday"));
}

#[test]
fn parse_rejects_schedule_set_with_invalid_exception_date() {
    let payload = r#"{"windows":[{"days":["mon"],"start":"09:00","end":"10:00"}],"exception_dates":["2026-99-99"]}"#;
    let error =
        parse_args([OsString::from("--schedule-set"), OsString::from(payload)]).unwrap_err();
    assert!(error.contains("must be YYYY-MM-DD"));
}

#[test]
fn parse_rejects_schedule_set_with_invalid_one_time_date() {
    let payload = r#"{"windows":[],"exception_dates":[],"one_time_windows":[{"date":"2026-99-99","start":"09:00","end":"10:00"}]}"#;
    let error =
        parse_args([OsString::from("--schedule-set"), OsString::from(payload)]).unwrap_err();
    assert!(error.contains("Invalid one-time window"));
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
fn parse_rejects_multiple_primary_commands_for_migrate_and_backup() {
    let error = parse(&["--migrate", "--backup"]).unwrap_err();
    assert!(error.contains("Multiple primary commands"));
}

#[test]
fn parse_rejects_multiple_primary_commands_for_schedule_delay_and_break_glass() {
    let error = parse(&["--schedule-delay", "--break-glass-trigger"]).unwrap_err();
    assert!(error.contains("Multiple primary commands"));
}

#[test]
fn parse_rejects_unknown_option() {
    let error = parse(&["--unknown"]).unwrap_err();
    assert!(error.contains("Unknown option"));
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
fn parse_rejects_dry_run_without_migrate() {
    let error = parse(&["--dry-run"]).unwrap_err();
    assert!(error.contains("`--dry-run` is only valid with `--migrate`"));
}

#[test]
fn parse_rejects_duplicate_dry_run_flags() {
    let error = parse(&["--migrate", "--dry-run", "--dry-run"]).unwrap_err();
    assert!(error.contains("`--dry-run` can only be specified once"));
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
    assert!(error.message.contains("Unknown option"));
}

#[test]
fn parse_with_contract_detects_json_on_early_parse_failures() {
    let error = parse_with_contract(&["--schedule-set", "--json"]).unwrap_err();
    assert_eq!(error.kind, CliErrorKind::Usage);
    assert_eq!(error.output, OutputMode::Json);
    assert_eq!(error.exit_code(), EXIT_CODE_USAGE_ERROR);
    assert!(
        error
            .message
            .contains("`--schedule-set` requires a JSON payload")
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
fn apply_blocklist_profile_select_updates_selection_case_insensitively() {
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
    }
    .normalized();

    let payload = apply_blocklist_profile_command(
        &mut config,
        BlocklistProfileCommandKind::Select {
            profile: Some("STUDY".to_string()),
        },
    )
    .unwrap();

    assert!(payload.updated);
    assert_eq!(payload.selected_blocklist_profile, "Study");
    assert_eq!(config.selected_blocklist_profile, "Study");
    assert!(config.blocked_sites.is_empty());
}

#[test]
fn apply_blocklist_profile_rename_updates_selection_and_name() {
    let mut config = AppConfig {
        blocklist_profiles: vec![
            crate::config::BlocklistProfileConfig {
                name: "Work".to_string(),
                sites: vec!["a.com".to_string()],
                allowlist_sites: Vec::new(),
            },
            crate::config::BlocklistProfileConfig {
                name: "Study".to_string(),
                sites: vec!["study.com".to_string()],
                allowlist_sites: Vec::new(),
            },
        ],
        selected_blocklist_profile: "Work".to_string(),
        ..AppConfig::default()
    }
    .normalized();

    let payload = apply_blocklist_profile_command(
        &mut config,
        BlocklistProfileCommandKind::Rename {
            name: "Deep Work".to_string(),
        },
    )
    .unwrap();

    assert!(payload.updated);
    assert_eq!(payload.selected_blocklist_profile, "Deep Work");
    assert_eq!(config.selected_blocklist_profile, "Deep Work");
    assert_eq!(config.blocklist_profiles[0].name, "Deep Work");
    assert!(config.blocked_sites.is_empty());
}

#[test]
fn apply_blocklist_profile_delete_switches_selection() {
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
        selected_blocklist_profile: "Work".to_string(),
        ..AppConfig::default()
    }
    .normalized();

    let payload =
        apply_blocklist_profile_command(&mut config, BlocklistProfileCommandKind::Delete).unwrap();

    assert!(payload.updated);
    assert_eq!(payload.selected_blocklist_profile, "Study");
    assert_eq!(config.selected_blocklist_profile, "Study");
    assert_eq!(config.blocklist_profiles.len(), 1);
    assert!(config.blocked_sites.is_empty());
}

#[test]
fn apply_allowlist_site_add_updates_effective_blocking() {
    let mut config = AppConfig {
        blocklist_profiles: vec![crate::config::BlocklistProfileConfig {
            name: "Default".to_string(),
            sites: vec!["a.com".to_string(), "b.com".to_string()],
            allowlist_sites: vec!["b.com".to_string()],
        }],
        selected_blocklist_profile: "Default".to_string(),
        ..AppConfig::default()
    }
    .normalized();

    let payload = apply_site_add_command(&mut config, SiteListTarget::Allowlist, "a.com").unwrap();

    assert!(payload.updated);
    assert_eq!(payload.target, SiteListTarget::Allowlist);
    assert_eq!(
        config.blocklist_profiles[0].allowlist_sites,
        vec!["b.com".to_string(), "a.com".to_string()]
    );
    assert!(config.blocked_sites.is_empty());
    assert_eq!(payload.effective_blocked_sites_count, 0);
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
fn apply_site_delete_command_updates_allowlist_and_effective_blocking() {
    let mut config = AppConfig {
        blocklist_profiles: vec![crate::config::BlocklistProfileConfig {
            name: "Default".to_string(),
            sites: vec!["a.com".to_string(), "b.com".to_string()],
            allowlist_sites: vec!["b.com".to_string()],
        }],
        selected_blocklist_profile: "Default".to_string(),
        ..AppConfig::default()
    }
    .normalized();

    let payload =
        apply_site_delete_command(&mut config, SiteListTarget::Allowlist, "b.com").unwrap();

    assert!(payload.updated);
    assert_eq!(payload.removed, "b.com");
    assert!(config.blocklist_profiles[0].allowlist_sites.is_empty());
    assert!(config.blocked_sites.is_empty());
    assert_eq!(payload.effective_blocked_sites_count, 2);
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
fn build_status_output_trims_selected_break_template_name() {
    let config = AppConfig {
        selected_break_template: "  deep work  ".to_string(),
        custom_profile: Some(CustomProfileConfig {
            focus_secs: DEFAULT_FOCUS_SECS,
            short_break_secs: 10 * 60,
            long_break_secs: 30 * 60,
            long_break_interval: 3,
        }),
        ..AppConfig::default()
    };
    let stats = FocusStats::default();

    let output = build_status_output(&config, &stats);

    assert_eq!(output.selected_break_template.name, "Deep Work");
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
fn build_status_output_uses_custom_template_sentinel_when_unmatched() {
    let config = AppConfig {
        selected_break_template: String::new(),
        custom_profile: Some(CustomProfileConfig {
            focus_secs: DEFAULT_FOCUS_SECS,
            short_break_secs: 7 * 60,
            long_break_secs: 21 * 60,
            long_break_interval: 5,
        }),
        ..AppConfig::default()
    };
    let stats = FocusStats::default();

    let output = build_status_output(&config, &stats);

    assert_eq!(output.selected_break_template.name, "Custom");
    assert_eq!(output.selected_break_template.short_break_secs, 7 * 60);
    assert_eq!(output.selected_break_template.long_break_secs, 21 * 60);
    assert_eq!(output.selected_break_template.long_break_interval, 5);
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
        output.stats_retention.keep_break_glass_overrides_days,
        Some(180)
    );
    assert_eq!(
        output.stats_retention.pending_prune.focus_sessions_removed,
        1
    );
}

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
fn build_status_output_includes_unconfigured_selected_task_goal() {
    let mut stats = FocusStats::default();
    let changed =
        stats.update_task_planner_state(vec!["Docs".to_string()], Some("Docs".to_string()));
    assert!(changed);
    let config = AppConfig::default();

    let output = build_status_output(&config, &stats);
    let selected_task_goal = output
        .selected_task_goal
        .expect("selected task goal should exist when a task is selected");

    assert_eq!(selected_task_goal.task_label, "Docs");
    assert!(!selected_task_goal.configured);
    assert_eq!(selected_task_goal.minutes_target, 0);
    assert_eq!(selected_task_goal.pomodoros_target, 0);
    assert_eq!(selected_task_goal.focused_minutes, 0);
    assert_eq!(selected_task_goal.pomodoros_completed, 0);
    assert!(!selected_task_goal.met);
    assert!(!output.focus_score.available);
    assert!(output.focus_score.focus_score_pct.is_none());
}

#[test]
fn build_status_output_disables_task_label_metadata_mirror_when_flag_disabled() {
    let mut stats = FocusStats::default();
    let changed =
        stats.update_task_planner_state(vec!["Docs".to_string()], Some("Docs".to_string()));
    assert!(changed);
    let config = AppConfig {
        feature_flags: FeatureFlagsConfig {
            metadata_task_label_fallback: false,
        },
        ..AppConfig::default()
    };

    let output = build_status_output(&config, &stats);

    assert_eq!(output.selected_task_label.as_deref(), Some("Docs"));
    assert!(output.focus_intention.is_none());
    assert!(output.task_note.is_none());
    assert!(output.live.focus_intention.is_none());
    assert!(output.live.task_note.is_none());
}

#[test]
fn build_status_output_applies_carry_over_to_goal_targets_when_enabled() {
    let mut stats = FocusStats::default();
    let today = current_day_key();
    let today_date = NaiveDate::parse_from_str(&today, "%Y-%m-%d")
        .expect("current day key should parse as a date");
    let yesterday = today_date.pred_opt().expect("yesterday should exist");
    let yesterday_key = yesterday.format("%Y-%m-%d").to_string();
    let month_start = NaiveDate::from_ymd_opt(today_date.year(), today_date.month(), 1)
        .expect("month start should be representable");
    let previous_month_day = month_start
        .pred_opt()
        .expect("previous month day should be representable");
    let previous_month_key = previous_month_day.format("%Y-%m-%d").to_string();
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
        weekly_goal: WeeklyGoalConfig {
            minutes: 100,
            pomodoros: 3,
        },
        monthly_goal: MonthlyGoalConfig {
            minutes: 300,
            pomodoros: 10,
        },
        goal_carry_over: crate::config::GoalCarryOverConfig {
            daily: true,
            weekly: true,
            monthly: true,
        },
        ..AppConfig::default()
    };

    let output = build_status_output(&config, &stats);
    assert_eq!(output.goal.minutes_target, 80);
    assert_eq!(output.goal.pomodoros_target, 4);
    assert!(output.goal.carry_over);

    let mut monthly_stats = FocusStats::default();
    monthly_stats.sync_monthly_goal_snapshot(
        previous_month_day,
        DailyGoalSnapshot {
            minutes: 200,
            pomodoros: 6,
        },
    );
    monthly_stats.record_focus_elapsed(&previous_month_key, 120 * 60, base_daily_goal);
    for _ in 0..4 {
        monthly_stats.record_completed_pomodoro(&previous_month_key, base_daily_goal);
    }
    let output = build_status_output(&config, &monthly_stats);
    assert_eq!(output.monthly_goal.minutes_target, 380);
    assert_eq!(output.monthly_goal.pomodoros_target, 12);
    assert!(output.monthly_goal.carry_over);
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
        focus_intention: Some("Write docs".to_string()),
        task_note: Some("API section".to_string()),
        selected_profile: ProfileId::DeepWork,
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
    assert_eq!(output.live.focus_intention.as_deref(), Some("Write docs"));
    assert_eq!(output.live.task_note.as_deref(), Some("API section"));
    assert_eq!(output.live.selected_profile.id, "deep-work");
    assert!(output.live.recovery_error.is_none());
    assert_eq!(output.session.pomodoros_completed, 3);
    assert_eq!(output.session.focused_minutes, 199);
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
            focus_intention: Some("Write API docs"),
            task_note: Some("Skipped due urgent review"),
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
