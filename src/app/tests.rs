use crate::app::*;
use crate::blocker;
use crate::config::{
    FeatureFlagsConfig, HistoryDashboardConfig, HistoryKpiCardId, ShortcutConfig,
    StatsRetentionConfig,
};
use crate::error::UserFacingError;
use crate::session_recovery::{
    self, InProgressSessionSnapshot, RecoveryTimerPhase, RecoveryTimerStatus,
};
use chrono::{Datelike, Duration as ChronoDuration, Local, LocalResult, TimeZone, Weekday};
use std::{
    fs,
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl_key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::CONTROL)
}

fn local_datetime_today(hour: u32, minute: u32) -> DateTime<Local> {
    let today = Local::now().date_naive();
    match Local.with_ymd_and_hms(today.year(), today.month(), today.day(), hour, minute, 0) {
        LocalResult::Single(dt) => dt,
        LocalResult::Ambiguous(earliest, _) => earliest,
        LocalResult::None => panic!("local datetime should be representable in tests"),
    }
}

fn weekday_token(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "mon",
        Weekday::Tue => "tue",
        Weekday::Wed => "wed",
        Weekday::Thu => "thu",
        Weekday::Fri => "fri",
        Weekday::Sat => "sat",
        Weekday::Sun => "sun",
    }
}

fn seed_large_history(app: &mut App, day_count: i64, sessions_per_day: u32) {
    let day_count = day_count.max(1);
    let start = Local::now().date_naive() - ChronoDuration::days(day_count.saturating_sub(1));
    let goal = DailyGoalSnapshot {
        minutes: 25,
        pomodoros: 1,
    };
    for day_offset in 0..day_count {
        let day = start + ChronoDuration::days(day_offset);
        let day_key = day.format("%Y-%m-%d").to_string();
        for session_idx in 0..sessions_per_day.max(1) {
            let profile = if (session_idx + u32::try_from(day_offset).unwrap_or(0)) % 2 == 0 {
                Some(ProfileId::DeepWork)
            } else {
                Some(ProfileId::Classic)
            };
            let task_label = if session_idx % 2 == 0 {
                Some("Project A")
            } else {
                Some("Project B")
            };
            app.stats.record_completed_pomodoro_with_task(
                &day_key,
                goal,
                task_label,
                25 * 60,
                profile,
            );
        }
    }
    app.task_labels = vec![
        "Project A".to_string(),
        "Project B".to_string(),
        "Project C".to_string(),
    ];
    app.mark_stats_dirty();
}

fn snapshot_for_tests(
    phase: TimerPhase,
    status: TimerStatus,
    remaining_secs: u64,
    task_label: Option<&str>,
    selected_profile: ProfileId,
) -> InProgressSessionSnapshot {
    InProgressSessionSnapshot {
        phase: RecoveryTimerPhase::from_timer_phase(phase),
        status: RecoveryTimerStatus::from_timer_status(status),
        remaining_secs,
        pomodoros_completed: 0,
        selected_task_label: task_label.map(str::to_string),
        selected_profile,
        captured_at_epoch_secs: None,
    }
}

#[test]
fn app_default_uses_canonical_config_in_tests() {
    let app = App::default();

    assert!(app.blocker.sites.is_empty());
    assert_eq!(app.blocklist_profile_count(), 1);
    assert_eq!(app.active_blocklist_profile_name(), "Default");
    assert_eq!(app.timer.focus_secs, DEFAULT_FOCUS_SECS);
    assert_eq!(app.timer.short_break_secs, DEFAULT_SHORT_BREAK_SECS);
    assert_eq!(app.timer.long_break_secs, DEFAULT_LONG_BREAK_SECS);
    assert_eq!(app.timer.long_break_interval, DEFAULT_LONG_BREAK_INTERVAL);
    assert_eq!(app.auto_start, AutoStartConfig::default());
    assert_eq!(app.recurring_schedule, RecurringScheduleConfig::default());
    assert!(!app.strict_mode);
    assert_eq!(app.daily_goal, DailyGoalConfig::default());
    assert_eq!(app.weekly_goal, WeeklyGoalConfig::default());
    assert_eq!(app.monthly_goal, MonthlyGoalConfig::default());
}

#[test]
fn legacy_stats_path_migration_warning_includes_actionable_guidance() {
    let warning = format_legacy_stats_path_migration_warning(
        Path::new("state/stats.toml"),
        Path::new("config/stats.toml"),
    );

    assert!(warning.contains("Legacy stats path"));
    assert!(warning.contains("canonical stats"));
    assert!(warning.contains("focustime --backup"));
    assert!(warning.contains("focustime --restore"));
}

/// Verifies startup applies the selected built-in profile settings.
#[test]
fn selected_builtin_profile_is_applied_on_startup() {
    let config = AppConfig {
        focus_secs: 10 * 60,
        short_break_secs: 2 * 60,
        long_break_secs: 8 * 60,
        long_break_interval: 2,
        blocked_sites: Vec::new(),
        blocklist_profiles: vec![BlocklistProfileConfig::default()],
        selected_blocklist_profile: "Default".to_string(),
        blocking_backend: BlockingBackendConfig::default(),
        selected_profile: ProfileId::Classic,
        custom_profile: Some(CustomProfileConfig {
            focus_secs: 40 * 60,
            short_break_secs: 8 * 60,
            long_break_secs: 16 * 60,
            long_break_interval: 2,
        }),
        notifications: NotificationConfig::default(),
        auto_start: AutoStartConfig::default(),
        recurring_schedule: RecurringScheduleConfig::default(),
        schedule_runtime: ScheduleRuntimeConfig::default(),
        profile_automation: None,
        strict_mode: false,
        break_glass_duration_secs: 5 * 60,
        daily_goal: DailyGoalConfig::default(),
        weekly_goal: WeeklyGoalConfig::default(),
        monthly_goal: MonthlyGoalConfig::default(),
        goal_carry_over: GoalCarryOverConfig::default(),
        stats_retention: StatsRetentionConfig::default(),
        history_dashboard: HistoryDashboardConfig::default(),
        selected_theme_preset: ThemePreset::Classic,
        wakatime: WakatimeMetadataConfig::default(),
        wakatime_runtime: WakatimeRuntimeConfig::default(),
        feature_flags: FeatureFlagsConfig::default(),
        shortcuts: ShortcutConfig::default(),
    };
    let app = App::from_config(config);
    assert_eq!(app.selected_profile, ProfileId::Classic);
    assert_eq!(app.timer.focus_secs, DEFAULT_FOCUS_SECS);
    assert_eq!(app.timer.short_break_secs, DEFAULT_SHORT_BREAK_SECS);
    assert_eq!(app.timer.long_break_secs, DEFAULT_LONG_BREAK_SECS);
    assert_eq!(app.timer.long_break_interval, DEFAULT_LONG_BREAK_INTERVAL);
}

#[test]
fn app_from_config_applies_wakatime_runtime_knobs() {
    let app = App::from_config(AppConfig {
        wakatime_runtime: WakatimeRuntimeConfig {
            retry_backoff_secs: vec![2, 4, 8],
            queue_capacity: 512,
            queue_retry_delay_secs: 25,
        },
        ..AppConfig::default()
    });

    assert_eq!(
        app.wakatime_runtime_options_for_tests(),
        crate::wakatime::WakatimeRuntimeOptions {
            retry_backoff_secs: vec![2, 4, 8],
            queue_capacity: 512,
            queue_retry_delay_secs: 25,
        }
    );
}

#[test]
fn applying_profile_loads_profile_scoped_automation_rules() {
    let classic_schedule = RecurringScheduleConfig::default();
    let deep_work_schedule = RecurringScheduleConfig {
        windows: vec![RecurringFocusWindowConfig {
            days: vec!["mon".to_string()],
            start: "09:00".to_string(),
            end: "11:00".to_string(),
        }],
    };
    let config = AppConfig {
        selected_profile: ProfileId::Classic,
        profile_automation: Some(ProfileAutomationSettingsConfig {
            basic: Some(ProfileAutomationConfig {
                notifications: NotificationConfig {
                    enabled: true,
                    sound: false,
                },
                auto_start: AutoStartConfig {
                    focus_to_break: false,
                    break_to_focus: false,
                },
                strict_mode: false,
                recurring_schedule: classic_schedule.clone(),
            }),
            standard: Some(ProfileAutomationConfig {
                notifications: NotificationConfig {
                    enabled: false,
                    sound: false,
                },
                auto_start: AutoStartConfig {
                    focus_to_break: true,
                    break_to_focus: true,
                },
                strict_mode: true,
                recurring_schedule: deep_work_schedule.clone(),
            }),
            advanced: None,
        }),
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);

    assert_eq!(app.recurring_schedule, classic_schedule);
    assert!(!app.strict_mode);
    assert!(!app.auto_start.focus_to_break);
    assert!(app.apply_profile(ProfileId::DeepWork));
    assert_eq!(app.selected_profile, ProfileId::DeepWork);
    assert_eq!(app.recurring_schedule, deep_work_schedule);
    assert!(app.strict_mode);
    assert!(app.auto_start.focus_to_break);
    assert!(app.auto_start.break_to_focus);
    assert!(!app.notification_settings.enabled);
}

#[test]
fn applying_profile_with_missing_automation_uses_neutral_defaults() {
    let mut app = App::default();
    app.notification_settings = NotificationConfig {
        enabled: false,
        sound: true,
    };
    app.auto_start = AutoStartConfig {
        focus_to_break: true,
        break_to_focus: true,
    };
    app.strict_mode = true;
    app.recurring_schedule = RecurringScheduleConfig {
        windows: vec![RecurringFocusWindowConfig {
            days: vec!["mon".to_string()],
            start: "09:00".to_string(),
            end: "10:00".to_string(),
        }],
    };
    app.profile_automation.standard = None;

    assert!(app.apply_profile(ProfileId::DeepWork));
    assert_eq!(app.selected_profile, ProfileId::DeepWork);
    assert_eq!(app.notification_settings, NotificationConfig::default());
    assert_eq!(app.auto_start, AutoStartConfig::default());
    assert!(!app.strict_mode);
    assert_eq!(app.recurring_schedule, RecurringScheduleConfig::default());
}

#[test]
fn deep_work_profile_reaches_long_break_on_third_focus() {
    let config = AppConfig {
        selected_profile: ProfileId::DeepWork,
        custom_profile: Some(CustomProfileConfig::default()),
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    assert_eq!(app.timer.long_break_interval, 3);

    for _ in 0..2 {
        app.timer.status = TimerStatus::Running;
        app.timer.remaining_secs = 1;
        app.on_tick(false); // focus -> short break
        assert_eq!(app.timer.phase, TimerPhase::ShortBreak);

        app.timer.status = TimerStatus::Running;
        app.timer.remaining_secs = 1;
        app.on_tick(false); // short break -> focus
        assert_eq!(app.timer.phase, TimerPhase::Focus);
    }

    app.timer.status = TimerStatus::Running;
    app.timer.remaining_secs = 1;
    app.on_tick(false); // third focus completion -> long break
    assert_eq!(app.timer.phase, TimerPhase::LongBreak);
}

#[test]
fn export_stats_to_dir_writes_files_and_reports_success() {
    let mut app = App::default();
    app.insert_daily_stats_for_tests(
        "2026-04-06",
        DailyStats {
            pomodoros_completed: 2,
            focused_seconds: 50 * 60,
            goal: Some(DailyGoalSnapshot {
                minutes: 25,
                pomodoros: 1,
            }),
        },
    );

    let export_dir = unique_temp_dir("app-export");
    app.export_stats_to_dir(&export_dir);

    let feedback = app.history_feedback.as_ref().unwrap();
    assert_eq!(feedback.level, HistoryFeedbackLevel::Success);
    assert!(feedback.message.contains("Exported to "));
    assert!(!feedback.message.contains('\n'));
    assert!(feedback.message.contains("JSON focustime-stats.json"));
    assert!(feedback.message.contains("CSV focustime-stats.csv"));
    assert!(feedback.message.contains("focustime-stats.json"));
    assert!(feedback.message.contains("focustime-stats.csv"));
    assert!(export_dir.join("focustime-stats.json").exists());
    assert!(export_dir.join("focustime-stats.csv").exists());

    fs::remove_dir_all(export_dir).unwrap();
}

#[test]
fn export_stats_to_dir_reports_warning_for_invalid_target() {
    let mut app = App::default();
    let export_root = unique_temp_dir("app-export-error");
    let not_a_directory = export_root.join("occupied-file");
    fs::write(&not_a_directory, "occupied").unwrap();

    app.export_stats_to_dir(&not_a_directory);

    let feedback = app.history_feedback.as_ref().unwrap();
    assert_eq!(feedback.level, HistoryFeedbackLevel::Warning);
    assert!(feedback.message.starts_with("Export failed:"));

    fs::remove_dir_all(export_root).unwrap();
}

fn unique_temp_dir(label: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("focustime-{label}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn profile_manager_enter_applies_selection() {
    let config = AppConfig {
        selected_profile: ProfileId::Custom,
        custom_profile: Some(CustomProfileConfig::default()),
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);

    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Up)); // Custom -> Deep Work
    app.handle_key(key(KeyCode::Enter));

    assert_eq!(app.mode, AppMode::Timer);
    assert_eq!(app.selected_profile, ProfileId::DeepWork);
    let (focus, short_break, long_break, cadence) = app.profile_values(ProfileId::DeepWork);
    assert_eq!(app.timer.focus_secs, focus);
    assert_eq!(app.timer.short_break_secs, short_break);
    assert_eq!(app.timer.long_break_secs, long_break);
    assert_eq!(app.timer.long_break_interval, cadence);
}

#[test]
fn editing_custom_profile_updates_timer_when_custom_is_selected() {
    let custom = CustomProfileConfig {
        focus_secs: 30 * 60,
        short_break_secs: 6 * 60,
        long_break_secs: 12 * 60,
        long_break_interval: 3,
    };
    let config = AppConfig {
        selected_profile: ProfileId::Custom,
        custom_profile: Some(custom.clone()),
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    let original_focus = app.timer.focus_secs;

    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Char('e')));
    app.handle_key(key(KeyCode::Right)); // +1 minute focus
    app.handle_key(key(KeyCode::Enter)); // save edit

    assert_eq!(app.selected_profile, ProfileId::Custom);
    assert_eq!(
        app.timer.focus_secs,
        original_focus + CUSTOM_DURATION_STEP_SECS
    );
    assert_eq!(
        app.custom_profile.focus_secs,
        original_focus + CUSTOM_DURATION_STEP_SECS
    );
}

#[test]
fn cancelling_custom_profile_edit_restores_original_values() {
    let custom = CustomProfileConfig {
        focus_secs: 30 * 60,
        short_break_secs: 6 * 60,
        long_break_secs: 12 * 60,
        long_break_interval: 3,
    };
    let config = AppConfig {
        selected_profile: ProfileId::Custom,
        custom_profile: Some(custom),
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    let original = app.custom_profile.clone();

    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Char('e')));
    app.handle_key(key(KeyCode::Right));
    assert_ne!(app.custom_profile.focus_secs, original.focus_secs);

    app.handle_key(key(KeyCode::Esc));

    assert!(!app.profile_edit_active);
    assert_eq!(app.custom_profile, original);
    assert_eq!(app.timer.focus_secs, original.focus_secs);
}

#[test]
fn persisted_config_keeps_legacy_fields_from_custom_profile() {
    let custom = CustomProfileConfig {
        focus_secs: 35 * 60,
        short_break_secs: 7 * 60,
        long_break_secs: 14 * 60,
        long_break_interval: 5,
    };
    let config = AppConfig {
        selected_profile: ProfileId::DeepWork,
        custom_profile: Some(custom.clone()),
        strict_mode: true,
        ..AppConfig::default()
    };
    let app = App::from_config(config);

    let persisted = app.persisted_config();
    assert_eq!(persisted.selected_profile, ProfileId::DeepWork);
    assert_eq!(persisted.focus_secs, custom.focus_secs);
    assert_eq!(persisted.short_break_secs, custom.short_break_secs);
    assert_eq!(persisted.long_break_secs, custom.long_break_secs);
    assert_eq!(persisted.long_break_interval, custom.long_break_interval);
    assert_eq!(persisted.custom_profile, Some(custom));
    assert_eq!(persisted.notifications, NotificationConfig::default());
    assert_eq!(persisted.auto_start, AutoStartConfig::default());
    assert!(persisted.strict_mode);
    assert_eq!(persisted.break_glass_duration_secs, 5 * 60);
    assert_eq!(persisted.daily_goal, DailyGoalConfig::default());
    assert_eq!(persisted.wakatime, WakatimeMetadataConfig::default());
    assert_eq!(persisted.selected_blocklist_profile, "Default");
    assert_eq!(persisted.blocklist_profiles.len(), 1);
    assert_eq!(persisted.blocklist_profiles[0].name, "Default");
}

#[test]
fn persisted_config_preserves_wakatime_metadata() {
    let config = AppConfig {
        wakatime: WakatimeMetadataConfig {
            project: "Team Focus".to_string(),
            language: "Deep Work".to_string(),
        },
        ..AppConfig::default()
    };
    let app = App::from_config(config);

    let persisted = app.persisted_config();
    assert_eq!(
        persisted.wakatime,
        WakatimeMetadataConfig {
            project: "Team Focus".to_string(),
            language: "Deep Work".to_string(),
        }
    );
}

#[test]
fn persisted_config_seeds_fallback_profile_with_active_sites() {
    let mut app = App::default();
    app.blocklist_profiles.clear();
    app.blocker.sites = vec!["example.com".to_string(), "github.com".to_string()];

    let persisted = app.persisted_config();

    assert_eq!(persisted.selected_blocklist_profile, "Default");
    assert_eq!(persisted.blocklist_profiles.len(), 1);
    assert_eq!(persisted.blocklist_profiles[0].name, "Default");
    assert_eq!(
        persisted.blocklist_profiles[0].sites,
        vec!["example.com".to_string(), "github.com".to_string()]
    );
    assert!(persisted.blocked_sites.is_empty());
}

#[test]
fn profile_summary_displays_seconds_when_not_minute_aligned() {
    let config = AppConfig {
        selected_profile: ProfileId::Custom,
        custom_profile: Some(CustomProfileConfig {
            focus_secs: 25 * 60 + 1,
            short_break_secs: 5 * 60 + 2,
            long_break_secs: 15 * 60 + 3,
            long_break_interval: 4,
        }),
        ..AppConfig::default()
    };
    let app = App::from_config(config);
    assert_eq!(
        app.profile_summary(ProfileId::Custom),
        "25:01/5:02/15:03 · every 4 focus"
    );
}

#[test]
fn profile_edit_field_value_displays_second_precision() {
    let config = AppConfig {
        selected_profile: ProfileId::Custom,
        custom_profile: Some(CustomProfileConfig {
            focus_secs: 10 * 60 + 7,
            short_break_secs: 2 * 60,
            long_break_secs: 8 * 60 + 9,
            long_break_interval: 3,
        }),
        ..AppConfig::default()
    };
    let app = App::from_config(config);
    assert_eq!(app.profile_edit_field_value(0), "10:07");
    assert_eq!(app.profile_edit_field_value(1), "2m");
    assert_eq!(app.profile_edit_field_value(2), "8:09");
    assert_eq!(app.profile_edit_field_value(4), "On");
    assert_eq!(app.profile_edit_field_value(5), "Off");
    assert_eq!(app.profile_edit_field_value(6), "Off");
    assert_eq!(app.profile_edit_field_value(7), "Off");
    assert_eq!(app.profile_edit_field_value(8), "Off");
    assert_eq!(app.profile_edit_field_value(9), "Off");
    assert_eq!(app.profile_edit_field_value(10), "Off");
    assert_eq!(app.profile_edit_field_value(11), "Off");
    assert_eq!(app.profile_edit_field_value(12), "Off");
    assert_eq!(app.profile_edit_field_value(13), "Off");
    assert_eq!(app.profile_edit_field_value(14), "Off");
    assert_eq!(app.profile_edit_field_value(15), "Off");
    assert_eq!(app.profile_edit_field_value(16), "Off");
    assert_eq!(app.profile_edit_field_value(17), "Off");
    assert_eq!(app.profile_edit_field_value(18), "focustime");
    assert_eq!(app.profile_edit_field_value(19), "Pomodoro");
}

#[test]
fn editing_strict_mode_field_updates_and_persists_setting() {
    let mut app = App::default();

    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Char('e')));
    for _ in 0..8 {
        app.handle_key(key(KeyCode::Down));
    }
    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Enter));

    assert!(app.strict_mode);
    let persisted = app.persisted_config();
    assert!(persisted.strict_mode);
}

#[test]
fn profile_manager_edit_mode_available_for_non_custom_profile() {
    let config = AppConfig {
        selected_profile: ProfileId::Classic,
        notifications: NotificationConfig {
            enabled: false,
            sound: false,
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);

    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Char('e')));

    assert!(app.profile_edit_active);
}

#[test]
fn editing_notification_fields_updates_and_persists_settings() {
    let config = AppConfig {
        selected_profile: ProfileId::DeepWork,
        notifications: NotificationConfig {
            enabled: false,
            sound: false,
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);

    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Char('e')));
    for _ in 0..4 {
        app.handle_key(key(KeyCode::Down));
    }
    app.handle_key(key(KeyCode::Right)); // notifications -> On
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Right)); // sound -> On
    app.handle_key(key(KeyCode::Enter));

    let persisted = app.persisted_config();
    assert!(persisted.notifications.enabled);
    assert!(persisted.notifications.sound);
}

#[test]
fn editing_auto_start_fields_updates_and_persists_settings() {
    let mut app = App::default();

    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Char('e')));
    for _ in 0..6 {
        app.handle_key(key(KeyCode::Down));
    }
    app.handle_key(key(KeyCode::Right)); // auto-start break -> On
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Right)); // auto-start focus -> On
    app.handle_key(key(KeyCode::Enter));

    assert!(app.auto_start.focus_to_break);
    assert!(app.auto_start.break_to_focus);

    let persisted = app.persisted_config();
    assert!(persisted.auto_start.focus_to_break);
    assert!(persisted.auto_start.break_to_focus);
}

#[test]
fn editing_recurring_schedule_fields_updates_and_persists_settings() {
    let mut app = App::default();

    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Char('e')));
    app.profile_edit_field = PROFILE_EDIT_SCHEDULE_ADD_REMOVE_INDEX;
    app.handle_key(key(KeyCode::Right)); // add default window
    app.profile_edit_schedule_day = 6; // Sun
    app.profile_edit_field = PROFILE_EDIT_SCHEDULE_DAY_ENABLED_INDEX;
    app.handle_key(key(KeyCode::Right)); // enable Sunday
    app.profile_edit_field = PROFILE_EDIT_SCHEDULE_START_INDEX;
    app.handle_key(key(KeyCode::Right)); // 09:00 -> 09:15
    app.profile_edit_field = PROFILE_EDIT_SCHEDULE_END_INDEX;
    app.handle_key(key(KeyCode::Left)); // 10:00 -> 09:45
    app.handle_key(key(KeyCode::Enter));

    assert_eq!(app.recurring_schedule.windows.len(), 1);
    let window = &app.recurring_schedule.windows[0];
    assert_eq!(window.start, "09:15");
    assert_eq!(window.end, "09:45");
    assert!(
        window
            .days
            .iter()
            .any(|day| day.eq_ignore_ascii_case("sun"))
    );

    let persisted = app.persisted_config();
    assert_eq!(persisted.recurring_schedule, app.recurring_schedule);
}

/// Verifies daily goal editor changes are persisted.
#[test]
fn editing_daily_goal_fields_updates_and_persists_settings() {
    let mut app = App::default();

    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Char('e')));
    for _ in 0..9 {
        app.handle_key(key(KeyCode::Down));
    }
    app.handle_key(key(KeyCode::Right)); // minutes goal -> 5m
    app.handle_key(key(KeyCode::Right)); // minutes goal -> 10m
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Right)); // pomodoros goal -> 1
    app.handle_key(key(KeyCode::Enter));

    assert_eq!(app.daily_goal.minutes, 10);
    assert_eq!(app.daily_goal.pomodoros, 1);

    let persisted = app.persisted_config();
    assert_eq!(persisted.daily_goal.minutes, 10);
    assert_eq!(persisted.daily_goal.pomodoros, 1);
}

#[test]
fn editing_weekly_and_monthly_goal_fields_updates_and_persists_settings() {
    let mut app = App::default();

    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Char('e')));
    app.profile_edit_field = PROFILE_EDIT_WEEKLY_GOAL_MINUTES_INDEX;
    app.handle_key(key(KeyCode::Right)); // weekly minutes -> 5m
    app.handle_key(key(KeyCode::Right)); // weekly minutes -> 10m
    app.profile_edit_field = PROFILE_EDIT_WEEKLY_GOAL_POMODOROS_INDEX;
    app.handle_key(key(KeyCode::Right)); // weekly pomodoros -> 1
    app.profile_edit_field = PROFILE_EDIT_MONTHLY_GOAL_MINUTES_INDEX;
    app.handle_key(key(KeyCode::Right)); // monthly minutes -> 5m
    app.handle_key(key(KeyCode::Right)); // monthly minutes -> 10m
    app.handle_key(key(KeyCode::Right)); // monthly minutes -> 15m
    app.profile_edit_field = PROFILE_EDIT_MONTHLY_GOAL_POMODOROS_INDEX;
    app.handle_key(key(KeyCode::Right)); // monthly pomodoros -> 1
    app.handle_key(key(KeyCode::Right)); // monthly pomodoros -> 2
    app.handle_key(key(KeyCode::Enter));

    assert_eq!(app.weekly_goal.minutes, 10);
    assert_eq!(app.weekly_goal.pomodoros, 1);
    assert_eq!(app.monthly_goal.minutes, 15);
    assert_eq!(app.monthly_goal.pomodoros, 2);

    let persisted = app.persisted_config();
    assert_eq!(persisted.weekly_goal.minutes, 10);
    assert_eq!(persisted.weekly_goal.pomodoros, 1);
    assert_eq!(persisted.monthly_goal.minutes, 15);
    assert_eq!(persisted.monthly_goal.pomodoros, 2);
}

#[test]
fn editing_goal_carry_over_fields_updates_and_persists_settings() {
    let mut app = App::default();

    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Char('e')));
    app.profile_edit_field = PROFILE_EDIT_DAILY_GOAL_CARRY_OVER_INDEX;
    app.handle_key(key(KeyCode::Right));
    app.profile_edit_field = PROFILE_EDIT_WEEKLY_GOAL_CARRY_OVER_INDEX;
    app.handle_key(key(KeyCode::Right));
    app.profile_edit_field = PROFILE_EDIT_MONTHLY_GOAL_CARRY_OVER_INDEX;
    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Enter));

    assert!(app.goal_carry_over.daily);
    assert!(app.goal_carry_over.weekly);
    assert!(app.goal_carry_over.monthly);

    let persisted = app.persisted_config();
    assert!(persisted.goal_carry_over.daily);
    assert!(persisted.goal_carry_over.weekly);
    assert!(persisted.goal_carry_over.monthly);
}

#[test]
fn cancelling_profile_edit_restores_recurring_schedule_settings() {
    let original_schedule = RecurringScheduleConfig {
        windows: vec![RecurringFocusWindowConfig {
            days: vec!["tue".to_string(), "thu".to_string()],
            start: "13:00".to_string(),
            end: "14:30".to_string(),
        }],
    };
    let config = AppConfig {
        recurring_schedule: original_schedule.clone(),
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);

    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Char('e')));
    app.profile_edit_field = PROFILE_EDIT_SCHEDULE_ADD_REMOVE_INDEX;
    app.handle_key(key(KeyCode::Right)); // add extra window
    app.profile_edit_field = PROFILE_EDIT_SCHEDULE_START_INDEX;
    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Esc)); // cancel edit

    assert_eq!(app.recurring_schedule, original_schedule);
    assert_eq!(app.recurring_windows.len(), 1);
}

#[test]
fn editing_wakatime_metadata_fields_updates_and_persists_settings() {
    let config = AppConfig {
        wakatime: WakatimeMetadataConfig {
            project: "A".to_string(),
            language: "B".to_string(),
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);

    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Char('e')));
    for _ in 0..PROFILE_EDIT_WAKATIME_PROJECT_INDEX {
        app.handle_key(key(KeyCode::Down));
    }
    app.handle_key(key(KeyCode::Backspace));
    for c in "Team Focus".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Backspace));
    for c in "Deep Work".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    let persisted = app.persisted_config();
    assert_eq!(
        persisted.wakatime,
        WakatimeMetadataConfig {
            project: "Team Focus".to_string(),
            language: "Deep Work".to_string(),
        }
    );
    assert_eq!(
        app.wakatime_heartbeat_metadata_for_tests(),
        WakatimeHeartbeatMetadata {
            project: "Team Focus".to_string(),
            language: "Deep Work".to_string(),
        }
    );
}

#[test]
fn editing_wakatime_metadata_blank_values_fall_back_to_defaults() {
    let config = AppConfig {
        wakatime: WakatimeMetadataConfig {
            project: "A".to_string(),
            language: "B".to_string(),
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);

    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Char('e')));
    for _ in 0..PROFILE_EDIT_WAKATIME_PROJECT_INDEX {
        app.handle_key(key(KeyCode::Down));
    }
    app.handle_key(key(KeyCode::Backspace));
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Backspace));
    app.handle_key(key(KeyCode::Enter));

    let defaults = WakatimeMetadataConfig::default();
    assert_eq!(app.wakatime_metadata, defaults);
    assert_eq!(app.persisted_config().wakatime, defaults);
    assert_eq!(
        app.wakatime_heartbeat_metadata_for_tests(),
        WakatimeHeartbeatMetadata::default()
    );
}

#[test]
fn selecting_task_label_keeps_global_wakatime_metadata() {
    let config = AppConfig {
        wakatime: WakatimeMetadataConfig {
            project: "Global Project".to_string(),
            language: "Global Language".to_string(),
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);

    app.select_task_label_for_cli("Docs").unwrap();

    assert_eq!(
        app.wakatime_heartbeat_metadata_for_tests(),
        WakatimeHeartbeatMetadata {
            project: "Global Project".to_string(),
            language: "Global Language".to_string(),
        }
    );
}

#[test]
fn cancelling_profile_edit_restores_daily_goal_settings() {
    let config = AppConfig {
        daily_goal: DailyGoalConfig {
            minutes: 25,
            pomodoros: 3,
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);

    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Char('e')));
    for _ in 0..9 {
        app.handle_key(key(KeyCode::Down));
    }
    app.handle_key(key(KeyCode::Right)); // minutes goal -> 30m
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Right)); // pomodoros goal -> 4
    app.handle_key(key(KeyCode::Esc)); // cancel

    assert_eq!(app.daily_goal.minutes, 25);
    assert_eq!(app.daily_goal.pomodoros, 3);
}

#[test]
fn cancelling_profile_edit_restores_weekly_and_monthly_goal_settings() {
    let config = AppConfig {
        weekly_goal: WeeklyGoalConfig {
            minutes: 120,
            pomodoros: 4,
        },
        monthly_goal: MonthlyGoalConfig {
            minutes: 600,
            pomodoros: 20,
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);

    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Char('e')));
    app.profile_edit_field = PROFILE_EDIT_WEEKLY_GOAL_MINUTES_INDEX;
    app.handle_key(key(KeyCode::Right)); // weekly minutes -> 125m
    app.profile_edit_field = PROFILE_EDIT_WEEKLY_GOAL_POMODOROS_INDEX;
    app.handle_key(key(KeyCode::Right)); // weekly pomodoros -> 5
    app.profile_edit_field = PROFILE_EDIT_MONTHLY_GOAL_MINUTES_INDEX;
    app.handle_key(key(KeyCode::Left)); // monthly minutes -> 595m
    app.profile_edit_field = PROFILE_EDIT_MONTHLY_GOAL_POMODOROS_INDEX;
    app.handle_key(key(KeyCode::Left)); // monthly pomodoros -> 19
    app.handle_key(key(KeyCode::Esc)); // cancel

    assert_eq!(app.weekly_goal.minutes, 120);
    assert_eq!(app.weekly_goal.pomodoros, 4);
    assert_eq!(app.monthly_goal.minutes, 600);
    assert_eq!(app.monthly_goal.pomodoros, 20);
}

#[test]
fn cancelling_profile_edit_restores_goal_carry_over_settings() {
    let config = AppConfig {
        goal_carry_over: GoalCarryOverConfig {
            daily: true,
            weekly: false,
            monthly: true,
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);

    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Char('e')));
    app.profile_edit_field = PROFILE_EDIT_DAILY_GOAL_CARRY_OVER_INDEX;
    app.handle_key(key(KeyCode::Left));
    app.profile_edit_field = PROFILE_EDIT_WEEKLY_GOAL_CARRY_OVER_INDEX;
    app.handle_key(key(KeyCode::Right));
    app.profile_edit_field = PROFILE_EDIT_MONTHLY_GOAL_CARRY_OVER_INDEX;
    app.handle_key(key(KeyCode::Left));
    app.handle_key(key(KeyCode::Esc));

    assert!(app.goal_carry_over.daily);
    assert!(!app.goal_carry_over.weekly);
    assert!(app.goal_carry_over.monthly);
}

#[test]
fn cancelling_profile_edit_restores_auto_start_settings() {
    let config = AppConfig {
        auto_start: AutoStartConfig {
            focus_to_break: true,
            break_to_focus: true,
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);

    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Char('e')));
    for _ in 0..6 {
        app.handle_key(key(KeyCode::Down));
    }
    app.handle_key(key(KeyCode::Left)); // auto-start break -> Off
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Left)); // auto-start focus -> Off
    app.handle_key(key(KeyCode::Esc)); // cancel

    assert!(!app.profile_edit_active);
    assert!(app.auto_start.focus_to_break);
    assert!(app.auto_start.break_to_focus);
}

#[test]
fn today_goal_progress_reports_ratios_for_minutes_and_pomodoros() {
    let config = AppConfig {
        daily_goal: DailyGoalConfig {
            minutes: 60,
            pomodoros: 4,
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    let day_key = current_day_key();
    app.stats
        .record_focus_elapsed(&day_key, 30 * 60, app.current_goal_snapshot());
    app.stats
        .record_completed_pomodoro(&day_key, app.current_goal_snapshot());

    let progress = app.today_goal_progress();
    assert_eq!(progress.minutes.completed, 30);
    assert_eq!(progress.minutes.target, 60);
    assert!((progress.minutes.ratio - 0.5).abs() < f64::EPSILON);
    assert_eq!(progress.pomodoros.completed, 1);
    assert_eq!(progress.pomodoros.target, 4);
    assert!((progress.pomodoros.ratio - 0.25).abs() < f64::EPSILON);
}

#[test]
fn weekly_and_monthly_goal_progress_use_current_period_totals() {
    let config = AppConfig {
        weekly_goal: WeeklyGoalConfig {
            minutes: 100,
            pomodoros: 3,
        },
        monthly_goal: MonthlyGoalConfig {
            minutes: 90,
            pomodoros: 3,
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    let day_key = current_day_key();
    app.stats
        .record_focus_elapsed(&day_key, 120 * 60, app.current_goal_snapshot());
    app.stats
        .record_completed_pomodoro(&day_key, app.current_goal_snapshot());

    let weekly = app.current_week_goal_progress();
    let monthly = app.current_month_goal_progress();

    assert_eq!(weekly.minutes.completed, 120);
    assert_eq!(weekly.minutes.target, 100);
    assert!((weekly.minutes.ratio - 1.0).abs() < f64::EPSILON);
    assert_eq!(weekly.pomodoros.completed, 1);
    assert_eq!(weekly.pomodoros.target, 3);
    assert!((weekly.pomodoros.ratio - (1.0 / 3.0)).abs() < f64::EPSILON);

    assert_eq!(monthly.minutes.completed, 120);
    assert_eq!(monthly.minutes.target, 90);
    assert!((monthly.minutes.ratio - 1.0).abs() < f64::EPSILON);
    assert_eq!(monthly.pomodoros.completed, 1);
    assert_eq!(monthly.pomodoros.target, 3);
    assert!((monthly.pomodoros.ratio - (1.0 / 3.0)).abs() < f64::EPSILON);
}

#[test]
fn daily_goal_progress_applies_previous_day_deficit_when_carry_over_is_enabled() {
    let config = AppConfig {
        daily_goal: DailyGoalConfig {
            minutes: 60,
            pomodoros: 2,
        },
        goal_carry_over: GoalCarryOverConfig {
            daily: true,
            ..GoalCarryOverConfig::default()
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    let today = current_day_key();
    let today_date = chrono::NaiveDate::parse_from_str(&today, "%Y-%m-%d")
        .expect("current day key should parse");
    let yesterday = today_date.pred_opt().expect("yesterday should exist");
    let yesterday_key = yesterday.format("%Y-%m-%d").to_string();
    let previous_target = DailyGoalSnapshot {
        minutes: 50,
        pomodoros: 3,
    };

    app.stats
        .record_focus_elapsed(&yesterday_key, 30 * 60, previous_target);
    app.stats
        .record_completed_pomodoro(&yesterday_key, previous_target);
    app.stats
        .record_focus_elapsed(&today, 40 * 60, app.current_goal_snapshot());
    app.stats
        .record_completed_pomodoro(&today, app.current_goal_snapshot());

    let progress = app.today_goal_progress();
    assert_eq!(progress.minutes.completed, 40);
    assert_eq!(progress.minutes.target, 80);
    assert_eq!(progress.pomodoros.completed, 1);
    assert_eq!(progress.pomodoros.target, 4);
}

#[test]
fn weekly_goal_progress_applies_previous_week_deficit_when_enabled() {
    let config = AppConfig {
        weekly_goal: WeeklyGoalConfig {
            minutes: 100,
            pomodoros: 3,
        },
        goal_carry_over: GoalCarryOverConfig {
            weekly: true,
            ..GoalCarryOverConfig::default()
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    let today = current_day_key();
    let today_date = chrono::NaiveDate::parse_from_str(&today, "%Y-%m-%d")
        .expect("current day key should parse");
    let previous_week_day = today_date - chrono::Duration::days(7);
    let previous_week_key = previous_week_day.format("%Y-%m-%d").to_string();
    let goal = app.current_goal_snapshot();
    app.stats.sync_weekly_goal_snapshot(
        previous_week_day,
        DailyGoalSnapshot {
            minutes: 50,
            pomodoros: 2,
        },
    );

    app.stats
        .record_focus_elapsed(&previous_week_key, 20 * 60, goal);
    app.stats
        .record_completed_pomodoro(&previous_week_key, goal);
    app.stats.record_focus_elapsed(&today, 20 * 60, goal);
    app.stats.record_completed_pomodoro(&today, goal);

    let weekly = app.current_week_goal_progress();
    assert_eq!(weekly.minutes.target, 130);
    assert_eq!(weekly.pomodoros.target, 4);
    assert_eq!(weekly.minutes.completed, 20);
    assert_eq!(weekly.pomodoros.completed, 1);
}

#[test]
fn monthly_goal_progress_applies_previous_month_deficit_when_enabled() {
    let config = AppConfig {
        monthly_goal: MonthlyGoalConfig {
            minutes: 300,
            pomodoros: 10,
        },
        goal_carry_over: GoalCarryOverConfig {
            monthly: true,
            ..GoalCarryOverConfig::default()
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    let today = current_day_key();
    let today_date = chrono::NaiveDate::parse_from_str(&today, "%Y-%m-%d")
        .expect("current day key should parse");
    let month_start = chrono::NaiveDate::from_ymd_opt(today_date.year(), today_date.month(), 1)
        .expect("current month start should be representable");
    let previous_month_day = month_start
        .pred_opt()
        .expect("previous month day should be representable");
    let previous_month_key = previous_month_day.format("%Y-%m-%d").to_string();
    let goal = app.current_goal_snapshot();
    app.stats.sync_monthly_goal_snapshot(
        previous_month_day,
        DailyGoalSnapshot {
            minutes: 200,
            pomodoros: 6,
        },
    );

    app.stats
        .record_focus_elapsed(&previous_month_key, 120 * 60, goal);
    for _ in 0..4 {
        app.stats
            .record_completed_pomodoro(&previous_month_key, goal);
    }
    app.stats.record_focus_elapsed(&today, 20 * 60, goal);
    app.stats.record_completed_pomodoro(&today, goal);

    let monthly = app.current_month_goal_progress();
    assert_eq!(monthly.minutes.target, 380);
    assert_eq!(monthly.pomodoros.target, 12);
    assert_eq!(monthly.minutes.completed, 20);
    assert_eq!(monthly.pomodoros.completed, 1);
}

#[test]
fn weekly_goal_progress_carries_full_previous_week_when_snapshot_exists_without_activity() {
    let config = AppConfig {
        weekly_goal: WeeklyGoalConfig {
            minutes: 100,
            pomodoros: 3,
        },
        goal_carry_over: GoalCarryOverConfig {
            weekly: true,
            ..GoalCarryOverConfig::default()
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    let today = current_day_key();
    let today_date = chrono::NaiveDate::parse_from_str(&today, "%Y-%m-%d")
        .expect("current day key should parse");
    let previous_week_day = today_date - chrono::Duration::days(7);
    let goal = app.current_goal_snapshot();
    app.stats.sync_weekly_goal_snapshot(
        previous_week_day,
        DailyGoalSnapshot {
            minutes: 40,
            pomodoros: 2,
        },
    );
    app.stats.record_focus_elapsed(&today, 20 * 60, goal);
    app.stats.record_completed_pomodoro(&today, goal);

    let weekly = app.current_week_goal_progress();
    assert_eq!(weekly.minutes.target, 140);
    assert_eq!(weekly.pomodoros.target, 5);
    assert_eq!(weekly.minutes.completed, 20);
    assert_eq!(weekly.pomodoros.completed, 1);
}

#[test]
fn weekly_daily_goal_allocation_uses_schedule_weights_for_remaining_days() {
    let config = AppConfig {
        weekly_goal: WeeklyGoalConfig {
            minutes: 300,
            pomodoros: 10,
        },
        recurring_schedule: RecurringScheduleConfig {
            windows: vec![RecurringFocusWindowConfig {
                days: vec!["wed".to_string(), "thu".to_string(), "fri".to_string()],
                start: "09:00".to_string(),
                end: "12:00".to_string(),
            }],
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    let day = chrono::NaiveDate::from_ymd_opt(2026, 4, 22).expect("test day should be valid");
    assert_eq!(day.weekday(), chrono::Weekday::Wed);

    app.insert_daily_stats_for_tests(
        "2026-04-21",
        crate::stats::DailyStats {
            pomodoros_completed: 2,
            focused_seconds: 60 * 60,
            goal: None,
        },
    );

    let allocation = app.weekly_daily_goal_allocation_for_day(day);
    assert!(allocation.has_any_target());
    assert_eq!(allocation.remaining_minutes, 240);
    assert_eq!(allocation.remaining_pomodoros, 8);
    assert_eq!(allocation.remaining_days_in_week, 5);
    assert_eq!(allocation.allocatable_days, 3);
    assert!(allocation.uses_schedule_weights);
    assert_eq!(allocation.daily_targets.len(), 5);

    assert_eq!(allocation.daily_targets[0].minutes_target, 80);
    assert_eq!(allocation.daily_targets[0].pomodoros_target, 3);
    assert_eq!(allocation.daily_targets[1].minutes_target, 80);
    assert_eq!(allocation.daily_targets[1].pomodoros_target, 3);
    assert_eq!(allocation.daily_targets[2].minutes_target, 80);
    assert_eq!(allocation.daily_targets[2].pomodoros_target, 2);
    assert_eq!(allocation.daily_targets[3].minutes_target, 0);
    assert_eq!(allocation.daily_targets[3].pomodoros_target, 0);
    assert_eq!(allocation.daily_targets[4].minutes_target, 0);
    assert_eq!(allocation.daily_targets[4].pomodoros_target, 0);
}

#[test]
fn weekly_daily_goal_allocation_falls_back_to_equal_split_without_schedule_windows() {
    let config = AppConfig {
        weekly_goal: WeeklyGoalConfig {
            minutes: 100,
            pomodoros: 5,
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    let day = chrono::NaiveDate::from_ymd_opt(2026, 4, 24).expect("test day should be valid");
    assert_eq!(day.weekday(), chrono::Weekday::Fri);

    app.insert_daily_stats_for_tests(
        "2026-04-23",
        crate::stats::DailyStats {
            pomodoros_completed: 2,
            focused_seconds: 10 * 60,
            goal: None,
        },
    );

    let allocation = app.weekly_daily_goal_allocation_for_day(day);
    assert!(allocation.has_any_target());
    assert_eq!(allocation.remaining_minutes, 90);
    assert_eq!(allocation.remaining_pomodoros, 3);
    assert_eq!(allocation.remaining_days_in_week, 3);
    assert_eq!(allocation.allocatable_days, 3);
    assert!(!allocation.uses_schedule_weights);
    assert_eq!(allocation.daily_targets.len(), 3);

    for target in allocation.daily_targets {
        assert_eq!(target.minutes_target, 30);
        assert_eq!(target.pomodoros_target, 1);
        assert!(target.allocatable);
    }
}

#[test]
fn sync_goal_snapshot_for_day_keeps_weekly_and_monthly_carry_across_idle_boundaries() {
    let config = AppConfig {
        weekly_goal: WeeklyGoalConfig {
            minutes: 100,
            pomodoros: 0,
        },
        monthly_goal: MonthlyGoalConfig {
            minutes: 200,
            pomodoros: 0,
        },
        goal_carry_over: GoalCarryOverConfig {
            weekly: true,
            monthly: true,
            ..GoalCarryOverConfig::default()
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);

    let week1 = chrono::NaiveDate::from_ymd_opt(2026, 4, 6).expect("week1 date should be valid");
    let week2 = chrono::NaiveDate::from_ymd_opt(2026, 4, 13).expect("week2 date should be valid");
    let week3 = chrono::NaiveDate::from_ymd_opt(2026, 4, 20).expect("week3 date should be valid");
    app.sync_goal_snapshot_for_day(week1);
    app.sync_goal_snapshot_for_day(week2);
    let weekly_target = app.effective_weekly_goal_snapshot_for_day(week3);
    assert_eq!(weekly_target.minutes, 200);
    assert_eq!(weekly_target.pomodoros, 0);

    let month1 = chrono::NaiveDate::from_ymd_opt(2026, 1, 15).expect("month1 date should be valid");
    let month2 = chrono::NaiveDate::from_ymd_opt(2026, 2, 15).expect("month2 date should be valid");
    let month3 = chrono::NaiveDate::from_ymd_opt(2026, 3, 15).expect("month3 date should be valid");
    app.sync_goal_snapshot_for_day(month1);
    app.sync_goal_snapshot_for_day(month2);
    let monthly_target = app.effective_monthly_goal_snapshot_for_day(month3);
    assert_eq!(monthly_target.minutes, 400);
    assert_eq!(monthly_target.pomodoros, 0);
}

#[test]
fn sync_goal_snapshot_for_day_uses_persisted_weekly_and_monthly_base_for_historical_days() {
    let config = AppConfig {
        weekly_goal: WeeklyGoalConfig {
            minutes: 100,
            pomodoros: 2,
        },
        monthly_goal: MonthlyGoalConfig {
            minutes: 300,
            pomodoros: 6,
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    let historical_day =
        chrono::NaiveDate::from_ymd_opt(2026, 4, 15).expect("historical day should be valid");

    app.sync_goal_snapshot_for_day(historical_day);

    app.weekly_goal.minutes = 240;
    app.weekly_goal.pomodoros = 8;
    app.monthly_goal.minutes = 900;
    app.monthly_goal.pomodoros = 30;

    assert_eq!(
        app.effective_weekly_goal_snapshot_for_day(historical_day),
        DailyGoalSnapshot {
            minutes: 100,
            pomodoros: 2,
        }
    );
    assert_eq!(
        app.effective_monthly_goal_snapshot_for_day(historical_day),
        DailyGoalSnapshot {
            minutes: 300,
            pomodoros: 6,
        }
    );
}

#[test]
fn sync_goal_snapshot_for_day_persists_idle_daily_snapshots_for_next_day_carry_over() {
    let config = AppConfig {
        daily_goal: DailyGoalConfig {
            minutes: 60,
            pomodoros: 0,
        },
        goal_carry_over: GoalCarryOverConfig {
            daily: true,
            ..GoalCarryOverConfig::default()
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    let day1 = chrono::NaiveDate::from_ymd_opt(2026, 4, 8).expect("day1 date should be valid");
    let day2 = chrono::NaiveDate::from_ymd_opt(2026, 4, 9).expect("day2 date should be valid");
    let day3 = chrono::NaiveDate::from_ymd_opt(2026, 4, 10).expect("day3 date should be valid");

    app.sync_goal_snapshot_for_day(day1);
    app.sync_goal_snapshot_for_day(day2);

    let day2_key = day2.format("%Y-%m-%d").to_string();
    assert_eq!(
        app.stats
            .daily_entry(&day2_key)
            .and_then(|stats| stats.goal),
        Some(DailyGoalSnapshot {
            minutes: 60,
            pomodoros: 0,
        })
    );

    let day3_target = app.effective_daily_goal_snapshot_for_day(day3);
    assert_eq!(day3_target.minutes, 120);
    assert_eq!(day3_target.pomodoros, 0);
}

#[test]
fn poll_wakatime_status_and_on_tick_sync_today_goal_snapshot() {
    let config = AppConfig {
        daily_goal: DailyGoalConfig {
            minutes: 60,
            pomodoros: 2,
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    let day_key = current_day_key();

    app.stats.insert_daily_for_tests(
        &day_key,
        DailyStats {
            pomodoros_completed: 0,
            focused_seconds: 0,
            goal: None,
        },
    );
    app.poll_wakatime_status();
    assert_eq!(app.today_stats().goal, Some(app.current_goal_snapshot()));

    app.stats.insert_daily_for_tests(
        &day_key,
        DailyStats {
            pomodoros_completed: 0,
            focused_seconds: 0,
            goal: None,
        },
    );
    app.on_tick(true);
    assert_eq!(app.today_stats().goal, Some(app.current_goal_snapshot()));
}

#[test]
fn record_focus_elapsed_persists_base_daily_goal_snapshot_when_carry_over_is_enabled() {
    let config = AppConfig {
        daily_goal: DailyGoalConfig {
            minutes: 60,
            pomodoros: 2,
        },
        goal_carry_over: GoalCarryOverConfig {
            daily: true,
            ..GoalCarryOverConfig::default()
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    let today = chrono::Local::now().date_naive();
    let yesterday = today.pred_opt().expect("yesterday should exist");
    let yesterday_key = yesterday.format("%Y-%m-%d").to_string();
    let previous_target = DailyGoalSnapshot {
        minutes: 50,
        pomodoros: 3,
    };
    app.stats
        .record_focus_elapsed(&yesterday_key, 30 * 60, previous_target);
    app.stats
        .record_completed_pomodoro(&yesterday_key, previous_target);

    app.record_focus_elapsed(60);

    assert_eq!(
        app.today_stats().goal,
        Some(DailyGoalSnapshot {
            minutes: 60,
            pomodoros: 2,
        })
    );
}

#[test]
fn goal_streak_counts_yesterday_until_today_is_missed() {
    let config = AppConfig {
        daily_goal: DailyGoalConfig {
            minutes: 60,
            pomodoros: 1,
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    let goal = app.current_goal_snapshot();
    let yesterday = chrono::Local::now().date_naive().pred_opt().unwrap();
    let day_before = yesterday.pred_opt().unwrap();

    for day in [day_before, yesterday] {
        let day_key = day.format("%Y-%m-%d").to_string();
        app.stats.record_focus_elapsed(&day_key, 60 * 60, goal);
        app.stats.record_completed_pomodoro(&day_key, goal);
    }

    let streak = app.goal_streak();
    assert_eq!(streak.current, 2);
    assert_eq!(streak.best, 2);
}

#[test]
fn goal_streak_uses_per_day_goal_fallback_for_legacy_history_entries() {
    let config = AppConfig {
        daily_goal: DailyGoalConfig {
            minutes: 60,
            pomodoros: 0,
        },
        goal_carry_over: GoalCarryOverConfig {
            daily: true,
            ..GoalCarryOverConfig::default()
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);

    let today = chrono::Local::now().date_naive();
    let legacy_day = today
        .checked_sub_signed(chrono::Duration::days(3))
        .expect("legacy day should be representable");
    let yesterday = today.pred_opt().expect("yesterday should be representable");
    app.stats.insert_daily_for_tests(
        &legacy_day.format("%Y-%m-%d").to_string(),
        DailyStats {
            pomodoros_completed: 1,
            focused_seconds: 70 * 60,
            goal: None,
        },
    );
    app.stats.insert_daily_for_tests(
        &yesterday.format("%Y-%m-%d").to_string(),
        DailyStats {
            pomodoros_completed: 0,
            focused_seconds: 0,
            goal: Some(DailyGoalSnapshot {
                minutes: 120,
                pomodoros: 0,
            }),
        },
    );

    let streak = app.goal_streak();
    assert_eq!(streak.best, 1);
}

#[test]
fn committing_goal_edit_updates_today_goal_snapshot() {
    let config = AppConfig {
        daily_goal: DailyGoalConfig {
            minutes: 60,
            pomodoros: 1,
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    let day_key = current_day_key();

    app.stats
        .record_focus_elapsed(&day_key, 30 * 60, app.current_goal_snapshot());

    app.begin_profile_edit();
    app.daily_goal.minutes = 90;
    app.commit_profile_edit();

    assert_eq!(
        app.today_stats().goal,
        Some(DailyGoalSnapshot {
            minutes: 90,
            pomodoros: 1,
        })
    );
}

#[test]
fn goal_streak_for_day_key_uses_the_same_day_for_date_and_stats_lookup() {
    let config = AppConfig {
        daily_goal: DailyGoalConfig {
            minutes: 60,
            pomodoros: 1,
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    let goal = app.current_goal_snapshot();

    for day_key in ["2026-04-08", "2026-04-09"] {
        app.stats.record_focus_elapsed(day_key, 60 * 60, goal);
        app.stats.record_completed_pomodoro(day_key, goal);
    }

    let streak = app.goal_streak_for_day_key("2026-04-09");
    assert_eq!(streak.current, 2);
    assert_eq!(streak.best, 2);
}

#[test]
fn goal_streak_for_day_key_fails_closed_for_invalid_day_keys() {
    let app = App::from_config(AppConfig {
        daily_goal: DailyGoalConfig {
            minutes: 60,
            pomodoros: 1,
        },
        ..AppConfig::default()
    });

    let streak = app.goal_streak_for_day_key("not-a-day");
    assert_eq!(streak, GoalStreak::default());
}

#[test]
fn goal_streak_for_day_key_handles_legacy_entries_without_goal_snapshots() {
    let config = AppConfig {
        daily_goal: DailyGoalConfig {
            minutes: 60,
            pomodoros: 1,
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);

    app.stats.insert_daily_for_tests(
        "2026-04-08",
        DailyStats {
            pomodoros_completed: 1,
            focused_seconds: 60 * 60,
            goal: None,
        },
    );
    app.stats.insert_daily_for_tests(
        "2026-04-09",
        DailyStats {
            pomodoros_completed: 1,
            focused_seconds: 60 * 60,
            goal: None,
        },
    );

    let streak = app.goal_streak_for_day_key("2026-04-09");
    assert_eq!(streak.current, 2);
    assert_eq!(streak.best, 2);
}

#[test]
fn goal_streak_for_day_key_applies_daily_carry_over_to_historical_day_targets() {
    let config = AppConfig {
        daily_goal: DailyGoalConfig {
            minutes: 60,
            pomodoros: 0,
        },
        goal_carry_over: GoalCarryOverConfig {
            daily: true,
            ..GoalCarryOverConfig::default()
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);

    app.stats.insert_daily_for_tests(
        "2026-04-08",
        DailyStats {
            pomodoros_completed: 0,
            focused_seconds: 0,
            goal: Some(DailyGoalSnapshot {
                minutes: 30,
                pomodoros: 0,
            }),
        },
    );
    app.stats.insert_daily_for_tests(
        "2026-04-09",
        DailyStats {
            pomodoros_completed: 0,
            focused_seconds: 60 * 60,
            goal: Some(DailyGoalSnapshot {
                minutes: 60,
                pomodoros: 0,
            }),
        },
    );

    let streak = app.goal_streak_for_day_key("2026-04-09");
    assert_eq!(streak.current, 0);
    assert_eq!(streak.best, 0);
}

#[test]
fn goal_streak_for_day_key_uses_historical_day_base_goal_when_config_changed() {
    let config = AppConfig {
        daily_goal: DailyGoalConfig {
            minutes: 120,
            pomodoros: 0,
        },
        goal_carry_over: GoalCarryOverConfig {
            daily: true,
            ..GoalCarryOverConfig::default()
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);

    app.stats.insert_daily_for_tests(
        "2026-04-08",
        DailyStats {
            pomodoros_completed: 0,
            focused_seconds: 0,
            goal: Some(DailyGoalSnapshot {
                minutes: 40,
                pomodoros: 0,
            }),
        },
    );
    app.stats.insert_daily_for_tests(
        "2026-04-09",
        DailyStats {
            pomodoros_completed: 0,
            focused_seconds: 130 * 60,
            goal: Some(DailyGoalSnapshot {
                minutes: 60,
                pomodoros: 0,
            }),
        },
    );

    let streak = app.goal_streak_for_day_key("2026-04-09");
    assert_eq!(streak.current, 1);
    assert_eq!(streak.best, 1);
}

#[test]
fn cancelling_profile_edit_restores_notification_settings() {
    let config = AppConfig {
        selected_profile: ProfileId::Classic,
        notifications: NotificationConfig {
            enabled: true,
            sound: true,
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);

    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Char('e')));
    for _ in 0..4 {
        app.handle_key(key(KeyCode::Down));
    }
    app.handle_key(key(KeyCode::Left)); // notifications -> Off
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Left)); // sound -> Off
    app.handle_key(key(KeyCode::Esc)); // cancel

    assert!(!app.profile_edit_active);
    assert!(app.notification_settings.enabled);
    assert!(app.notification_settings.sound);
}

#[test]
fn cancelling_profile_edit_restores_wakatime_metadata() {
    let config = AppConfig {
        wakatime: WakatimeMetadataConfig {
            project: "Team Focus".to_string(),
            language: "Deep Work".to_string(),
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);

    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Char('e')));
    app.profile_edit_field = PROFILE_EDIT_WAKATIME_PROJECT_INDEX;
    app.handle_key(key(KeyCode::Char('X')));
    app.profile_edit_field = PROFILE_EDIT_WAKATIME_LANGUAGE_INDEX;
    app.handle_key(key(KeyCode::Char('Y')));
    app.handle_key(key(KeyCode::Esc));

    assert_eq!(
        app.wakatime_metadata,
        WakatimeMetadataConfig {
            project: "Team Focus".to_string(),
            language: "Deep Work".to_string(),
        }
    );
    assert_eq!(
        app.wakatime_heartbeat_metadata_for_tests(),
        WakatimeHeartbeatMetadata {
            project: "Team Focus".to_string(),
            language: "Deep Work".to_string(),
        }
    );
}

#[test]
fn timer_escape_quits_application() {
    let mut app = App::default();
    app.handle_key(key(KeyCode::Esc));
    assert!(app.should_quit);
}

#[test]
fn site_manager_add_site_runs_unified_mutation_flow() {
    let mut app = App::default();
    app.handle_key(key(KeyCode::Char('b')));
    app.handle_key(key(KeyCode::Char('a')));
    for c in "Example.com".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert_eq!(app.mode, AppMode::SiteManager);
    assert!(!app.site_input_active);
    assert_eq!(app.blocker.sites, vec!["example.com"]);
    assert_eq!(app.selected_site, 0);
    assert_eq!(
        app.site_feedback,
        Some(SiteFeedback {
            level: SiteFeedbackLevel::Success,
            message: "Added 1 site".to_string(),
        })
    );
    assert!(app.config_error.is_none());
}

#[test]
fn site_manager_delete_site_clamps_selection() {
    let config = AppConfig {
        blocked_sites: vec![
            "a.com".to_string(),
            "b.com".to_string(),
            "c.com".to_string(),
        ],
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.mode = AppMode::SiteManager;
    app.selected_site = 2;

    app.handle_key(key(KeyCode::Char('d')));

    assert_eq!(
        app.blocker.sites,
        vec!["a.com".to_string(), "b.com".to_string()]
    );
    assert_eq!(app.selected_site, 1);
    assert_eq!(
        app.site_feedback,
        Some(SiteFeedback {
            level: SiteFeedbackLevel::Success,
            message: "Removed `c.com` from blocklist".to_string(),
        })
    );
    assert!(app.config_error.is_none());
}

#[test]
fn site_manager_bulk_add_via_paste_supports_comma_and_newline() {
    let mut app = App::default();
    app.handle_key(key(KeyCode::Char('b')));

    app.handle_paste("Example.com,\ngithub.com\nexam_ple.com".to_string());
    app.handle_key(key(KeyCode::Enter));

    assert_eq!(
        app.blocker.sites,
        vec!["example.com".to_string(), "github.com".to_string()]
    );
    let feedback = app
        .site_feedback
        .as_ref()
        .expect("bulk add should provide feedback");
    assert_eq!(feedback.level, SiteFeedbackLevel::Warning);
    assert!(feedback.message.contains("Added 2 sites"));
    assert!(feedback.message.contains("invalid hostname"));
}

#[test]
fn site_manager_paste_targets_blocklist_profile_input_when_active() {
    let mut app = App::default();
    app.handle_key(key(KeyCode::Char('b')));
    app.handle_key(key(KeyCode::Char('n')));

    app.handle_paste("Work".to_string());

    assert!(app.blocklist_profile_input_active);
    assert_eq!(app.blocklist_profile_input, "Work");
    assert!(!app.site_input_active);
    assert!(app.site_input.is_empty());
}

#[test]
fn site_manager_ctrl_c_quits_during_text_input_modes() {
    let mut app = App::default();
    app.handle_key(key(KeyCode::Char('b')));
    app.handle_key(key(KeyCode::Char('a')));
    assert!(app.site_input_active);

    app.handle_key(ctrl_key(KeyCode::Char('c')));

    assert!(app.should_quit);

    let mut app = App::default();
    app.handle_key(key(KeyCode::Char('b')));
    app.handle_key(key(KeyCode::Char('n')));
    assert!(app.blocklist_profile_input_active);

    app.handle_key(ctrl_key(KeyCode::Char('c')));

    assert!(app.should_quit);
}

#[test]
fn site_manager_input_modes_are_mutually_exclusive() {
    let mut app = App::default();
    app.mode = AppMode::SiteManager;

    app.start_blocklist_profile_input(BlocklistProfileInputMode::Create);
    assert!(app.blocklist_profile_input_active);
    assert!(!app.site_input_active);

    app.start_site_input(SiteInputMode::Add);
    assert!(app.site_input_active);
    assert!(!app.blocklist_profile_input_active);
    assert!(app.blocklist_profile_input.is_empty());

    app.start_blocklist_profile_input(BlocklistProfileInputMode::Rename);
    assert!(app.blocklist_profile_input_active);
    assert!(!app.site_input_active);
    assert!(app.site_input.is_empty());
}

#[test]
fn site_manager_invalid_add_keeps_draft_open() {
    let mut app = App::default();
    app.handle_key(key(KeyCode::Char('b')));
    app.handle_key(key(KeyCode::Char('a')));
    for c in "exam_ple.com".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }

    app.handle_key(key(KeyCode::Enter));

    assert!(app.site_input_active);
    assert!(app.site_edit_index.is_none());
    assert_eq!(app.site_input, "exam_ple.com");
    assert!(app.blocker.sites.is_empty());
}

#[test]
fn site_manager_edit_selected_site() {
    let config = AppConfig {
        blocked_sites: vec!["a.com".to_string(), "b.com".to_string()],
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);

    app.handle_key(key(KeyCode::Char('b')));
    app.handle_key(key(KeyCode::Char('e')));
    for _ in 0.."a.com".len() {
        app.handle_key(key(KeyCode::Backspace));
    }
    for c in "news.ycombinator.com".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert_eq!(
        app.blocker.sites,
        vec!["news.ycombinator.com".to_string(), "b.com".to_string()]
    );
    assert_eq!(
        app.site_feedback,
        Some(SiteFeedback {
            level: SiteFeedbackLevel::Success,
            message: "Updated `a.com` -> `news.ycombinator.com`".to_string(),
        })
    );
}

#[test]
fn site_manager_invalid_edit_keeps_draft_open() {
    let config = AppConfig {
        blocked_sites: vec!["a.com".to_string(), "b.com".to_string()],
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.handle_key(key(KeyCode::Char('b')));
    app.handle_key(key(KeyCode::Char('e')));
    for _ in 0.."a.com".len() {
        app.handle_key(key(KeyCode::Backspace));
    }
    for c in "b.com".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }

    app.handle_key(key(KeyCode::Enter));

    assert!(app.site_input_active);
    assert_eq!(app.site_edit_index, Some(0));
    assert_eq!(app.site_input, "b.com");
    assert_eq!(
        app.blocker.sites,
        vec!["a.com".to_string(), "b.com".to_string()]
    );
}

#[test]
fn site_manager_reapply_decision_uses_focus_state() {
    let mut app = App::default();
    assert!(!app.should_resync_blocking_after_site_mutation());

    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;
    assert!(app.should_resync_blocking_after_site_mutation());
}

#[test]
fn site_manager_switches_between_blocklist_profiles() {
    let config = AppConfig {
        blocklist_profiles: vec![
            BlocklistProfileConfig {
                name: "Work".to_string(),
                sites: vec!["a.com".to_string()],
                allowlist_sites: Vec::new(),
            },
            BlocklistProfileConfig {
                name: "Study".to_string(),
                sites: vec!["b.com".to_string(), "c.com".to_string()],
                allowlist_sites: Vec::new(),
            },
        ],
        selected_blocklist_profile: "Work".to_string(),
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.handle_key(key(KeyCode::Char('b')));

    assert_eq!(app.active_blocklist_profile_name(), "Work");
    assert_eq!(app.blocker.sites, vec!["a.com".to_string()]);

    app.handle_key(key(KeyCode::Char(']')));

    assert_eq!(app.active_blocklist_profile_name(), "Study");
    assert_eq!(
        app.blocker.sites,
        vec!["b.com".to_string(), "c.com".to_string()]
    );
}

#[test]
fn site_manager_uses_profile_sites_not_selected_category() {
    let config: AppConfig = toml::from_str(
        r#"
selected_blocklist_profile = "Work"

[[blocklist_profiles]]
name = "Work"
sites = []
allowlist_sites = []
selected_category = "Social"

[[blocklist_profiles.categories]]
name = "General"
sites = ["general.com"]
allowlist_sites = []

[[blocklist_profiles.categories]]
name = "Social"
sites = ["social.com"]
allowlist_sites = []
"#,
    )
    .unwrap();
    let mut app = App::from_config(config);
    app.handle_key(key(KeyCode::Char('b')));

    assert_eq!(
        app.active_policy_sites(),
        vec!["general.com".to_string(), "social.com".to_string()]
    );

    app.handle_key(key(KeyCode::Char('a')));
    for c in "new.com".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    let persisted = app.persisted_config();
    assert_eq!(
        persisted.blocklist_profiles[0].sites,
        vec![
            "general.com".to_string(),
            "social.com".to_string(),
            "new.com".to_string()
        ]
    );
}

#[test]
fn site_manager_allowlist_mode_clamps_selection_on_profile_switch() {
    let config = AppConfig {
        blocklist_profiles: vec![
            BlocklistProfileConfig {
                name: "Study".to_string(),
                sites: vec!["study.com".to_string()],
                allowlist_sites: vec!["allow-a.com".to_string(), "allow-b.com".to_string()],
            },
            BlocklistProfileConfig {
                name: "Work".to_string(),
                sites: vec!["work.com".to_string(), "news.com".to_string()],
                allowlist_sites: vec!["news.com".to_string()],
            },
        ],
        selected_blocklist_profile: "Study".to_string(),
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.handle_key(key(KeyCode::Char('b')));
    app.handle_key(key(KeyCode::Char('m')));
    app.handle_key(key(KeyCode::Down));

    assert_eq!(app.site_list_mode(), SiteListMode::Allowlist);
    assert_eq!(app.selected_site, 1);

    app.handle_key(key(KeyCode::Char(']')));

    assert_eq!(app.active_blocklist_profile_name(), "Work");
    assert_eq!(app.site_list_mode(), SiteListMode::Allowlist);
    assert_eq!(app.active_policy_sites(), vec!["news.com".to_string()]);
    assert_eq!(app.selected_site, 0);
    assert_eq!(app.blocker.sites, vec!["work.com".to_string()]);
}

#[test]
fn allowlist_excludes_sites_from_effective_blocking() {
    let config = AppConfig {
        blocklist_profiles: vec![BlocklistProfileConfig {
            name: "Work".to_string(),
            sites: vec!["a.com".to_string(), "b.com".to_string()],
            allowlist_sites: vec!["b.com".to_string()],
        }],
        selected_blocklist_profile: "Work".to_string(),
        ..AppConfig::default()
    };
    let app = App::from_config(config);

    assert_eq!(app.blocker.sites, vec!["a.com".to_string()]);
}

#[test]
fn wildcard_rules_are_kept_in_effective_blocking() {
    let config = AppConfig {
        blocklist_profiles: vec![BlocklistProfileConfig {
            name: "Work".to_string(),
            sites: vec!["*.example.com".to_string()],
            allowlist_sites: Vec::new(),
        }],
        selected_blocklist_profile: "Work".to_string(),
        ..AppConfig::default()
    };
    let app = App::from_config(config);

    assert_eq!(app.blocker.sites, vec!["*.example.com".to_string()]);
}

#[test]
fn runtime_effective_blocking_canonicalizes_dotted_and_punycode_rules() {
    let config = AppConfig {
        blocklist_profiles: vec![BlocklistProfileConfig {
            name: "Work".to_string(),
            sites: vec![
                ".Example.com".to_string(),
                "*.xn--bcher-kva.example.".to_string(),
                "api.example.com.".to_string(),
            ],
            allowlist_sites: vec![".example.com".to_string()],
        }],
        selected_blocklist_profile: "Work".to_string(),
        ..AppConfig::default()
    };
    let app = App::from_config(config);

    assert_eq!(
        app.blocker.sites,
        vec!["*.xn--bcher-kva.example".to_string()]
    );
}

#[test]
fn site_manager_allowlist_mode_updates_effective_blocked_sites() {
    let config = AppConfig {
        blocklist_profiles: vec![BlocklistProfileConfig {
            name: "Default".to_string(),
            sites: vec!["a.com".to_string(), "b.com".to_string()],
            allowlist_sites: vec!["b.com".to_string()],
        }],
        selected_blocklist_profile: "Default".to_string(),
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.handle_key(key(KeyCode::Char('b')));
    app.handle_key(key(KeyCode::Char('m'))); // switch to allowlist view
    app.handle_key(key(KeyCode::Char('a')));
    for c in "a.com".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert_eq!(
        app.active_policy_sites(),
        vec!["b.com".to_string(), "a.com".to_string()]
    );
    assert!(app.blocker.sites.is_empty());
    let persisted = app.persisted_config();
    assert_eq!(persisted.blocked_sites, Vec::<String>::new());
    assert_eq!(
        persisted.blocklist_profiles[0].allowlist_sites,
        vec!["b.com".to_string(), "a.com".to_string()]
    );
}

#[test]
fn site_manager_create_rename_and_delete_blocklist_profile() {
    let mut app = App::default();
    app.handle_key(key(KeyCode::Char('b')));

    app.handle_key(key(KeyCode::Char('n')));
    for c in "Work".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert_eq!(app.blocklist_profile_count(), 2);
    assert_eq!(app.active_blocklist_profile_name(), "Work");
    assert!(app.blocker.sites.is_empty());

    app.handle_key(key(KeyCode::Char('r')));
    for _ in 0.."Work".len() {
        app.handle_key(key(KeyCode::Backspace));
    }
    for c in "Deep Work".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert_eq!(app.active_blocklist_profile_name(), "Deep Work");

    app.handle_key(key(KeyCode::Char('x')));

    assert_eq!(app.blocklist_profile_count(), 1);
    assert_eq!(app.active_blocklist_profile_name(), "Default");
}

#[test]
fn persisted_config_does_not_write_legacy_blocked_sites_mirror() {
    let mut app = App::default();
    app.handle_key(key(KeyCode::Char('b')));
    app.handle_key(key(KeyCode::Char('a')));
    for c in "example.com".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    let persisted = app.persisted_config();
    assert_eq!(persisted.selected_blocklist_profile, "Default");
    assert!(persisted.blocked_sites.is_empty());
    assert_eq!(persisted.blocklist_profiles.len(), 1);
    assert_eq!(
        persisted.blocklist_profiles[0].sites,
        vec!["example.com".to_string()]
    );
}

#[test]
fn persisted_config_keeps_blocklist_profiles_for_blocked_sites() {
    let mut app = App::from_config(AppConfig {
        blocked_sites: vec!["legacy-only.com".to_string()],
        ..AppConfig::default()
    });
    app.handle_key(key(KeyCode::Char('b')));
    app.handle_key(key(KeyCode::Char('a')));
    for c in "example.com".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    let persisted = app.persisted_config();
    assert!(persisted.blocked_sites.is_empty());
    assert!(
        persisted.blocklist_profiles[0]
            .sites
            .contains(&"example.com".to_string())
    );
}

#[test]
fn completed_focus_tick_increments_session_pomodoros() {
    let mut app = App::default();
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;
    app.timer.remaining_secs = 1;

    app.on_tick(false);

    assert_eq!(app.session_stats().pomodoros_completed, 1);
    assert_eq!(app.today_stats().pomodoros_completed, 1);
    assert_eq!(
        app.phase_notification.as_deref(),
        Some("Focus complete. Next up: Short Break.")
    );
    assert_eq!(app.timer.status, TimerStatus::Idle);
}

#[test]
fn skipping_focus_does_not_increment_session_pomodoros() {
    let mut app = App::default();
    assert_eq!(app.session_stats().pomodoros_completed, 0);

    app.handle_key(key(KeyCode::Char('n')));

    assert_eq!(app.session_stats().pomodoros_completed, 0);
    assert!(app.phase_notification.is_none());
}

#[test]
fn manual_skip_records_session_interruption_reason() {
    let mut app = App::default();
    app.task_labels = vec!["Project A".to_string()];
    app.selected_task_label = Some("Project A".to_string());
    app.handle_key(key(KeyCode::Char(' ')));
    app.timer.remaining_secs = 1200;

    app.handle_key(key(KeyCode::Char('n')));

    let interruptions = app.recent_session_interruptions(1);
    assert_eq!(interruptions.len(), 1);
    assert_eq!(
        interruptions[0].reason,
        SessionInterruptionReason::ManualSkip
    );
    assert_eq!(interruptions[0].task_label.as_deref(), Some("Project A"));
    assert_eq!(interruptions[0].remaining_secs, 1200);
}

#[test]
fn manual_stop_records_session_interruption_reason() {
    let mut app = App::default();
    app.task_labels = vec!["Project A".to_string()];
    app.selected_task_label = Some("Project A".to_string());
    app.handle_key(key(KeyCode::Char(' ')));
    app.timer.remaining_secs = 900;

    app.handle_key(key(KeyCode::Char('s')));

    let interruptions = app.recent_session_interruptions(1);
    assert_eq!(interruptions.len(), 1);
    assert_eq!(
        interruptions[0].reason,
        SessionInterruptionReason::ManualStop
    );
    assert_eq!(interruptions[0].task_label.as_deref(), Some("Project A"));
    assert_eq!(interruptions[0].remaining_secs, 900);
}

#[test]
fn natural_focus_completion_does_not_record_session_interruption() {
    let mut app = App::default();
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;
    app.timer.remaining_secs = 1;

    app.on_tick(false);

    assert!(app.recent_session_interruptions(1).is_empty());
}

#[test]
fn natural_focus_completion_auto_starts_break_when_enabled() {
    let config = AppConfig {
        auto_start: AutoStartConfig {
            focus_to_break: true,
            break_to_focus: false,
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;
    app.timer.remaining_secs = 1;

    app.on_tick(false);

    assert_eq!(app.timer.phase, TimerPhase::ShortBreak);
    assert_eq!(app.timer.status, TimerStatus::Running);
}

#[test]
fn natural_break_completion_auto_starts_focus_when_enabled() {
    let config = AppConfig {
        auto_start: AutoStartConfig {
            focus_to_break: false,
            break_to_focus: true,
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.task_labels = vec!["Auto Task".to_string()];
    app.selected_task_label = Some("Auto Task".to_string());
    app.timer.phase = TimerPhase::ShortBreak;
    app.timer.status = TimerStatus::Running;
    app.timer.remaining_secs = 1;

    app.on_tick(false);

    assert_eq!(app.timer.phase, TimerPhase::Focus);
    assert_eq!(app.timer.status, TimerStatus::Running);
}

#[test]
fn manual_skip_keeps_next_phase_idle_when_auto_start_is_enabled() {
    let config = AppConfig {
        auto_start: AutoStartConfig {
            focus_to_break: true,
            break_to_focus: true,
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);

    app.handle_key(key(KeyCode::Char('n')));

    assert_eq!(app.timer.phase, TimerPhase::ShortBreak);
    assert_eq!(app.timer.status, TimerStatus::Idle);
    assert!(app.phase_notification.is_none());
}

#[test]
fn recurring_schedule_next_window_text_shows_upcoming_window_for_today() {
    let now = local_datetime_today(10, 15);
    let config = AppConfig {
        recurring_schedule: RecurringScheduleConfig {
            windows: vec![crate::config::RecurringFocusWindowConfig {
                days: vec![weekday_token(now.weekday()).to_string()],
                start: "11:00".to_string(),
                end: "12:00".to_string(),
            }],
        },
        ..AppConfig::default()
    };
    let app = App::from_config(config);

    assert_eq!(
        app.recurring_schedule_texts_at(now).0,
        "🗓  Next schedule: today 11:00-12:00"
    );
}

#[test]
fn recurring_schedule_next_window_text_shows_active_window_then_next_window() {
    let now = local_datetime_today(10, 15);
    let config = AppConfig {
        recurring_schedule: RecurringScheduleConfig {
            windows: vec![
                crate::config::RecurringFocusWindowConfig {
                    days: vec![weekday_token(now.weekday()).to_string()],
                    start: "10:00".to_string(),
                    end: "11:00".to_string(),
                },
                crate::config::RecurringFocusWindowConfig {
                    days: vec![weekday_token(now.weekday()).to_string()],
                    start: "14:00".to_string(),
                    end: "15:00".to_string(),
                },
            ],
        },
        ..AppConfig::default()
    };
    let app = App::from_config(config);

    assert_eq!(
        app.recurring_schedule_texts_at(now).0,
        "🗓  Next schedule: in progress until 11:00 · then today 14:00-15:00"
    );
}

#[test]
fn recurring_schedule_display_texts_use_current_frame_timestamp() {
    let simulated_now = local_datetime_today(10, 15) + ChronoDuration::days(1);
    let config = AppConfig {
        recurring_schedule: RecurringScheduleConfig {
            windows: vec![crate::config::RecurringFocusWindowConfig {
                days: vec![weekday_token(simulated_now.weekday()).to_string()],
                start: "10:00".to_string(),
                end: "11:00".to_string(),
            }],
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.current_frame_now = simulated_now;
    app.task_labels.clear();
    app.selected_task_label = None;
    app.schedule_armed_occurrence_key = None;

    let (next_text, status_text) = app.recurring_schedule_display_texts();
    assert!(next_text.starts_with("🗓  Next schedule: in progress until 11:00"));
    assert_eq!(
        status_text,
        "⚙  Schedule status: window active; select a task with [t], then press [Space]"
    );
}

#[test]
fn recurring_schedule_status_text_guides_task_selection_when_active_and_armed() {
    let now = local_datetime_today(10, 15);
    let config = AppConfig {
        recurring_schedule: RecurringScheduleConfig {
            windows: vec![crate::config::RecurringFocusWindowConfig {
                days: vec![weekday_token(now.weekday()).to_string()],
                start: "10:00".to_string(),
                end: "11:00".to_string(),
            }],
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.sync_recurring_schedule(now);

    assert_eq!(
        app.recurring_schedule_texts_at(now).1,
        "⚙  Schedule status: window active; select a task with [t], then press [Space]"
    );
}

#[test]
fn recurring_schedule_status_text_guides_start_when_active_and_idle() {
    let now = local_datetime_today(10, 15);
    let config = AppConfig {
        recurring_schedule: RecurringScheduleConfig {
            windows: vec![crate::config::RecurringFocusWindowConfig {
                days: vec![weekday_token(now.weekday()).to_string()],
                start: "10:00".to_string(),
                end: "11:00".to_string(),
            }],
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.task_labels = vec!["Coding".to_string()];
    app.selected_task_label = Some("Coding".to_string());

    assert_eq!(
        app.recurring_schedule_texts_at(now).1,
        "⚙  Schedule status: window active; press [Space] to start focus"
    );
}

#[test]
fn recurring_schedule_status_text_guides_resume_when_active_and_paused() {
    let now = local_datetime_today(10, 15);
    let config = AppConfig {
        recurring_schedule: RecurringScheduleConfig {
            windows: vec![crate::config::RecurringFocusWindowConfig {
                days: vec![weekday_token(now.weekday()).to_string()],
                start: "10:00".to_string(),
                end: "11:00".to_string(),
            }],
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.task_labels = vec!["Coding".to_string()];
    app.selected_task_label = Some("Coding".to_string());
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Paused;

    assert_eq!(
        app.recurring_schedule_texts_at(now).1,
        "⚙  Schedule status: window active; press [Space] to resume focus"
    );
}

#[test]
fn recurring_schedule_status_text_guides_switch_to_focus_when_active_in_break() {
    let now = local_datetime_today(10, 15);
    let config = AppConfig {
        recurring_schedule: RecurringScheduleConfig {
            windows: vec![crate::config::RecurringFocusWindowConfig {
                days: vec![weekday_token(now.weekday()).to_string()],
                start: "10:00".to_string(),
                end: "11:00".to_string(),
            }],
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.timer.phase = TimerPhase::ShortBreak;
    app.timer.status = TimerStatus::Running;

    assert_eq!(
        app.recurring_schedule_texts_at(now).1,
        "⚙  Schedule status: window active; press [n] to switch to focus"
    );
}

#[test]
fn recurring_schedule_status_text_shows_ready_for_upcoming_window() {
    let now = local_datetime_today(10, 15);
    let config = AppConfig {
        recurring_schedule: RecurringScheduleConfig {
            windows: vec![crate::config::RecurringFocusWindowConfig {
                days: vec![weekday_token(now.weekday()).to_string()],
                start: "11:00".to_string(),
                end: "12:00".to_string(),
            }],
        },
        ..AppConfig::default()
    };
    let app = App::from_config(config);

    assert_eq!(
        app.recurring_schedule_texts_at(now).1,
        "⚙  Schedule status: ready for next window"
    );
}

#[test]
fn recurring_schedule_text_has_no_calendar_annotations() {
    let now = local_datetime_today(10, 15);
    let config = AppConfig {
        recurring_schedule: RecurringScheduleConfig {
            windows: vec![crate::config::RecurringFocusWindowConfig {
                days: vec![weekday_token(now.weekday()).to_string()],
                start: "11:00".to_string(),
                end: "12:00".to_string(),
            }],
        },
        ..AppConfig::default()
    };
    let app = App::from_config(config);

    let (next_text, status_text) = app.recurring_schedule_texts_at(now);

    assert!(!next_text.contains("calendar"));
    assert!(!status_text.contains("calendar"));
}

#[test]
fn recurring_schedule_auto_starts_focus_when_window_begins_and_task_is_selected() {
    let now = local_datetime_today(10, 15);
    let config = AppConfig {
        recurring_schedule: RecurringScheduleConfig {
            windows: vec![crate::config::RecurringFocusWindowConfig {
                days: vec![weekday_token(now.weekday()).to_string()],
                start: "10:00".to_string(),
                end: "11:00".to_string(),
            }],
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.task_labels = vec!["Coding".to_string()];
    app.selected_task_label = Some("Coding".to_string());

    app.sync_recurring_schedule(now);

    assert_eq!(app.timer.phase, TimerPhase::Focus);
    assert_eq!(app.timer.status, TimerStatus::Running);
    assert_eq!(
        app.phase_notification.as_deref(),
        Some("Scheduled window started. Focus auto-started.")
    );
    assert!(app.schedule_armed_occurrence_key.is_none());
}

#[test]
fn recurring_schedule_arms_when_window_begins_without_task_label() {
    let now = local_datetime_today(10, 15);
    let config = AppConfig {
        recurring_schedule: RecurringScheduleConfig {
            windows: vec![crate::config::RecurringFocusWindowConfig {
                days: vec![weekday_token(now.weekday()).to_string()],
                start: "10:00".to_string(),
                end: "11:00".to_string(),
            }],
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);

    app.sync_recurring_schedule(now);

    assert_eq!(app.timer.phase, TimerPhase::Focus);
    assert_eq!(app.timer.status, TimerStatus::Idle);
    assert!(app.schedule_armed_occurrence_key.is_some());
    assert_eq!(
        app.phase_notification.as_deref(),
        Some(
            "Scheduled window started. Select a task with [t], then press [Space] to start focus."
        )
    );
}

#[test]
fn recurring_schedule_arms_when_selected_task_label_is_archived() {
    let now = local_datetime_today(10, 15);
    let config = AppConfig {
        recurring_schedule: RecurringScheduleConfig {
            windows: vec![crate::config::RecurringFocusWindowConfig {
                days: vec![weekday_token(now.weekday()).to_string()],
                start: "10:00".to_string(),
                end: "11:00".to_string(),
            }],
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.task_labels = vec!["Coding".to_string()];
    app.selected_task_label = Some("Coding".to_string());
    app.task_label_archived.insert(task_label_key("Coding"));

    app.sync_recurring_schedule(now);

    assert_eq!(app.timer.phase, TimerPhase::Focus);
    assert_eq!(app.timer.status, TimerStatus::Idle);
    assert!(app.schedule_armed_occurrence_key.is_some());
    assert_eq!(
        app.phase_notification.as_deref(),
        Some(
            "Scheduled window started. Select a task with [t], then press [Space] to start focus."
        )
    );
}

#[test]
fn schedule_editor_uses_configured_runtime_step_minutes() {
    let config = AppConfig {
        recurring_schedule: RecurringScheduleConfig {
            windows: vec![crate::config::RecurringFocusWindowConfig {
                days: vec!["mon".to_string()],
                start: "09:00".to_string(),
                end: "11:00".to_string(),
            }],
        },
        schedule_runtime: ScheduleRuntimeConfig {
            time_step_minutes: 30,
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);

    app.adjust_selected_schedule_time(true, true);

    assert_eq!(app.recurring_schedule.windows[0].start, "09:30");
}

#[test]
fn recurring_schedule_auto_start_switches_idle_break_to_focus() {
    let now = local_datetime_today(10, 15);
    let config = AppConfig {
        recurring_schedule: RecurringScheduleConfig {
            windows: vec![crate::config::RecurringFocusWindowConfig {
                days: vec![weekday_token(now.weekday()).to_string()],
                start: "10:00".to_string(),
                end: "11:00".to_string(),
            }],
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.task_labels = vec!["Coding".to_string()];
    app.selected_task_label = Some("Coding".to_string());
    app.timer.phase = TimerPhase::ShortBreak;
    app.timer.status = TimerStatus::Idle;

    app.sync_recurring_schedule(now);

    assert_eq!(app.timer.phase, TimerPhase::Focus);
    assert_eq!(app.timer.status, TimerStatus::Running);
    assert_eq!(
        app.phase_notification.as_deref(),
        Some("Scheduled window started. Focus auto-started.")
    );
    assert!(app.schedule_armed_occurrence_key.is_none());
}

#[test]
fn recurring_schedule_does_not_retrigger_within_same_window_occurrence() {
    let first_tick = local_datetime_today(10, 15);
    let second_tick = local_datetime_today(10, 16);
    let config = AppConfig {
        recurring_schedule: RecurringScheduleConfig {
            windows: vec![crate::config::RecurringFocusWindowConfig {
                days: vec![weekday_token(first_tick.weekday()).to_string()],
                start: "10:00".to_string(),
                end: "11:00".to_string(),
            }],
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.task_labels = vec!["Coding".to_string()];
    app.selected_task_label = Some("Coding".to_string());

    app.sync_recurring_schedule(first_tick);
    assert_eq!(app.timer.status, TimerStatus::Running);

    app.timer.status = TimerStatus::Idle;
    app.sync_recurring_schedule(second_tick);

    assert_eq!(app.timer.status, TimerStatus::Idle);
}

#[test]
fn recurring_schedule_triggers_overlapping_window_when_new_window_starts() {
    let first_tick = local_datetime_today(10, 15);
    let overlap_window_tick = local_datetime_today(10, 35);
    let config = AppConfig {
        recurring_schedule: RecurringScheduleConfig {
            windows: vec![
                crate::config::RecurringFocusWindowConfig {
                    days: vec![weekday_token(first_tick.weekday()).to_string()],
                    start: "10:00".to_string(),
                    end: "11:00".to_string(),
                },
                crate::config::RecurringFocusWindowConfig {
                    days: vec![weekday_token(first_tick.weekday()).to_string()],
                    start: "10:30".to_string(),
                    end: "11:30".to_string(),
                },
            ],
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.task_labels = vec!["Coding".to_string()];
    app.selected_task_label = Some("Coding".to_string());

    app.sync_recurring_schedule(first_tick);
    assert_eq!(app.timer.status, TimerStatus::Running);

    app.timer.status = TimerStatus::Idle;
    app.sync_recurring_schedule(overlap_window_tick);

    assert_eq!(app.timer.phase, TimerPhase::Focus);
    assert_eq!(app.timer.status, TimerStatus::Running);
    assert_eq!(
        app.phase_notification.as_deref(),
        Some("Scheduled window started. Focus auto-started.")
    );
}

#[test]
fn recurring_schedule_does_not_retrigger_within_same_overlapping_occurrence() {
    let first_tick = local_datetime_today(10, 15);
    let overlap_window_tick = local_datetime_today(10, 35);
    let same_overlap_occurrence_tick = local_datetime_today(10, 40);
    let config = AppConfig {
        recurring_schedule: RecurringScheduleConfig {
            windows: vec![
                crate::config::RecurringFocusWindowConfig {
                    days: vec![weekday_token(first_tick.weekday()).to_string()],
                    start: "10:00".to_string(),
                    end: "11:00".to_string(),
                },
                crate::config::RecurringFocusWindowConfig {
                    days: vec![weekday_token(first_tick.weekday()).to_string()],
                    start: "10:30".to_string(),
                    end: "11:30".to_string(),
                },
            ],
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.task_labels = vec!["Coding".to_string()];
    app.selected_task_label = Some("Coding".to_string());

    app.sync_recurring_schedule(first_tick);
    app.timer.status = TimerStatus::Idle;
    app.sync_recurring_schedule(overlap_window_tick);
    assert_eq!(app.timer.status, TimerStatus::Running);

    app.timer.status = TimerStatus::Idle;
    app.sync_recurring_schedule(same_overlap_occurrence_tick);

    assert_eq!(app.timer.phase, TimerPhase::Focus);
    assert_eq!(app.timer.status, TimerStatus::Idle);
}

#[test]
fn schedule_window_transition_auto_starts_next_active_window() {
    let first_tick = local_datetime_today(10, 15);
    let overlap_transition_tick = local_datetime_today(10, 20);
    let config = AppConfig {
        recurring_schedule: RecurringScheduleConfig {
            windows: vec![
                crate::config::RecurringFocusWindowConfig {
                    days: vec![weekday_token(first_tick.weekday()).to_string()],
                    start: "10:00".to_string(),
                    end: "10:20".to_string(),
                },
                crate::config::RecurringFocusWindowConfig {
                    days: vec![weekday_token(first_tick.weekday()).to_string()],
                    start: "10:20".to_string(),
                    end: "10:40".to_string(),
                },
            ],
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.task_labels = vec!["Coding".to_string()];
    app.selected_task_label = Some("Coding".to_string());

    app.sync_recurring_schedule(first_tick);
    assert_eq!(app.timer.status, TimerStatus::Running);

    app.timer.status = TimerStatus::Idle;
    app.phase_notification = None;
    app.sync_recurring_schedule(overlap_transition_tick);

    assert_eq!(app.timer.status, TimerStatus::Running);
    assert!(app.phase_notification.as_deref().is_some_and(|message| {
        message.contains("Scheduled window started. Focus auto-started.")
    }));
}

#[test]
fn focus_completed_does_not_apply_profile_defaults_without_session_template() {
    let mut app = App::default();
    app.task_labels = vec!["Docs".to_string()];
    app.selected_task_label = Some("Docs".to_string());
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;
    app.timer.remaining_secs = 1;

    app.on_tick(false);

    assert_ne!(app.selected_profile, ProfileId::DeepWork);
}

#[test]
fn strict_mode_blocks_skip_during_active_focus() {
    let config = AppConfig {
        strict_mode: true,
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;
    app.timer.remaining_secs = app.timer.focus_secs;

    app.handle_key(key(KeyCode::Char('n')));

    assert_eq!(app.timer.phase, TimerPhase::Focus);
    assert_eq!(app.timer.status, TimerStatus::Running);
    assert_eq!(app.timer.remaining_secs, app.timer.focus_secs);
}

#[test]
fn strict_mode_requires_second_stop_press_during_active_focus() {
    let config = AppConfig {
        strict_mode: true,
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;
    app.timer.remaining_secs = app.timer.focus_secs.saturating_sub(10);

    app.handle_key(key(KeyCode::Char('s')));

    assert_eq!(app.timer.status, TimerStatus::Running);
    assert_eq!(
        app.timer.remaining_secs,
        app.timer.focus_secs.saturating_sub(10)
    );
    assert!(app.strict_reset_confirmation_pending());

    app.handle_key(key(KeyCode::Char('s')));

    assert_eq!(app.timer.status, TimerStatus::Idle);
    assert_eq!(app.timer.remaining_secs, app.timer.focus_secs);
    assert!(!app.strict_reset_confirmation_pending());
}

#[test]
fn pending_strict_reset_confirmation_clears_when_opening_site_manager() {
    let config = AppConfig {
        strict_mode: true,
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;

    app.handle_key(key(KeyCode::Char('s')));
    assert!(app.strict_reset_confirmation_pending());

    app.handle_key(key(KeyCode::Char('b')));

    assert_eq!(app.mode, AppMode::SiteManager);
    assert!(!app.strict_reset_confirmation_pending());
}

#[test]
fn pending_strict_reset_confirmation_clears_when_opening_history() {
    let config = AppConfig {
        strict_mode: true,
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;

    app.handle_key(key(KeyCode::Char('s')));
    assert!(app.strict_reset_confirmation_pending());

    app.handle_key(key(KeyCode::Char('h')));

    assert_eq!(app.mode, AppMode::StatsHistory);
    assert!(!app.strict_reset_confirmation_pending());
}

#[test]
fn stop_resets_immediately_when_strict_mode_is_disabled() {
    let mut app = App::default();
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;
    app.timer.remaining_secs = app.timer.focus_secs.saturating_sub(5);

    app.handle_key(key(KeyCode::Char('s')));

    assert_eq!(app.timer.status, TimerStatus::Idle);
    assert_eq!(app.timer.remaining_secs, app.timer.focus_secs);
}

#[test]
fn strict_mode_blocks_profile_manager_access_during_active_focus() {
    let config = AppConfig {
        strict_mode: true,
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;

    app.handle_key(key(KeyCode::Char('p')));

    assert_eq!(app.mode, AppMode::Timer);
}

#[test]
fn strict_mode_blocks_profile_apply_during_active_focus() {
    let config = AppConfig {
        strict_mode: true,
        selected_profile: ProfileId::Custom,
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;
    app.timer.remaining_secs = app.timer.focus_secs.saturating_sub(20);
    app.mode = AppMode::ProfileManager;
    app.profile_selection_index = profile_index(ProfileId::Classic);

    app.handle_key(key(KeyCode::Enter));

    assert_eq!(app.selected_profile, ProfileId::Custom);
    assert_eq!(app.mode, AppMode::ProfileManager);
    assert_eq!(app.timer.phase, TimerPhase::Focus);
    assert_eq!(app.timer.status, TimerStatus::Running);
    assert_eq!(
        app.timer.remaining_secs,
        app.timer.focus_secs.saturating_sub(20)
    );
    assert!(
        app.config_error
            .as_deref()
            .is_some_and(|err| err.contains("strict focus active"))
    );
}

#[test]
fn strict_mode_cannot_be_disabled_during_active_focus_profile_edit() {
    let config = AppConfig {
        strict_mode: true,
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;
    app.mode = AppMode::ProfileManager;
    app.profile_edit_active = true;
    app.profile_edit_field = 8;

    app.handle_key(key(KeyCode::Left));

    assert!(app.strict_mode);
}

/// Verifies strict mode blocks custom profile changes during active focus.
#[test]
fn strict_mode_blocks_custom_profile_commit_during_active_focus() {
    let config = AppConfig {
        strict_mode: true,
        selected_profile: ProfileId::Custom,
        custom_profile: Some(CustomProfileConfig::default()),
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;
    app.mode = AppMode::ProfileManager;
    app.profile_edit_active = true;
    app.profile_edit_field = 0;
    app.profile_edit_snapshot = Some(ProfileEditSnapshot {
        custom_profile: app.custom_profile.clone(),
        notification_settings: app.notification_settings,
        auto_start: app.auto_start,
        recurring_schedule: app.recurring_schedule.clone(),
        strict_mode: app.strict_mode,
        daily_goal: app.daily_goal,
        weekly_goal: app.weekly_goal,
        monthly_goal: app.monthly_goal,
        goal_carry_over: app.goal_carry_over,
        selected_theme_preset: app.selected_theme_preset,
        wakatime_metadata: app.wakatime_metadata.clone(),
    });
    let original_profile_automation = app
        .profile_automation
        .for_profile(ProfileId::Custom, &ProfileAutomationConfig::default());
    app.custom_profile.focus_secs = app.custom_profile.focus_secs.saturating_add(60);
    app.notification_settings.enabled = false;

    app.handle_key(key(KeyCode::Enter));

    assert!(app.profile_edit_active);
    assert!(app.profile_edit_snapshot.is_some());
    assert!(
        app.config_error
            .as_deref()
            .is_some_and(|err| err.contains("strict focus active"))
    );
    assert_eq!(
        app.profile_automation
            .for_profile(ProfileId::Custom, &ProfileAutomationConfig::default()),
        original_profile_automation
    );

    app.timer.remaining_secs = 1;
    app.on_tick(false);
    assert!(app.phase_notification.is_some());
}

/// Verifies enabling strict mode during active focus persists without resetting the timer.
#[test]
fn enabling_strict_mode_saves_during_active_focus_for_custom_profile_without_reset() {
    let config = AppConfig {
        strict_mode: false,
        selected_profile: ProfileId::Custom,
        custom_profile: Some(CustomProfileConfig::default()),
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;
    app.timer.remaining_secs = app.timer.focus_secs.saturating_sub(30);
    app.mode = AppMode::ProfileManager;
    app.profile_edit_active = true;
    app.profile_edit_field = 8;
    app.profile_edit_snapshot = Some(ProfileEditSnapshot {
        custom_profile: app.custom_profile.clone(),
        notification_settings: app.notification_settings,
        auto_start: app.auto_start,
        recurring_schedule: app.recurring_schedule.clone(),
        strict_mode: app.strict_mode,
        daily_goal: app.daily_goal,
        weekly_goal: app.weekly_goal,
        monthly_goal: app.monthly_goal,
        goal_carry_over: app.goal_carry_over,
        selected_theme_preset: app.selected_theme_preset,
        wakatime_metadata: app.wakatime_metadata.clone(),
    });

    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Enter));

    assert!(app.strict_mode);
    assert!(!app.profile_edit_active);
    assert!(app.profile_edit_snapshot.is_none());
    assert!(app.config_error.is_none());
    assert_eq!(app.timer.phase, TimerPhase::Focus);
    assert_eq!(app.timer.status, TimerStatus::Running);
    assert_eq!(
        app.timer.remaining_secs,
        app.timer.focus_secs.saturating_sub(30)
    );
    assert!(app.persisted_config().strict_mode);
}

#[test]
fn strict_mode_blocks_quit_keys_during_active_focus() {
    let config = AppConfig {
        strict_mode: true,
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;

    app.handle_key(key(KeyCode::Char('q')));
    assert!(!app.should_quit);

    app.handle_key(key(KeyCode::Esc));
    assert!(!app.should_quit);

    app.handle_key(ctrl_key(KeyCode::Char('c')));
    assert!(!app.should_quit);
    assert!(
        app.phase_notification
            .as_deref()
            .is_some_and(|msg| msg.contains("Strict mode active"))
    );
}

#[test]
fn strict_mode_blocks_custom_c_quit_key_during_active_focus() {
    let config = AppConfig {
        strict_mode: true,
        shortcuts: ShortcutConfig {
            quit: "c".to_string(),
            ..ShortcutConfig::default()
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;

    app.handle_key(key(KeyCode::Char('c')));

    assert!(!app.should_quit);
    assert!(
        app.phase_notification
            .as_deref()
            .is_some_and(|msg| msg.contains("Strict mode active"))
    );
}

#[test]
fn strict_mode_allows_quit_when_focus_not_active() {
    let config = AppConfig {
        strict_mode: true,
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Idle;

    app.handle_key(key(KeyCode::Char('q')));

    assert!(app.should_quit);
}

#[test]
fn custom_c_quit_key_allows_quit_when_focus_not_active() {
    let config = AppConfig {
        strict_mode: true,
        shortcuts: ShortcutConfig {
            quit: "c".to_string(),
            ..ShortcutConfig::default()
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Idle;

    app.handle_key(key(KeyCode::Char('c')));

    assert!(app.should_quit);
}

#[test]
fn strict_mode_stays_enforced_during_break_glass_override() {
    let config = AppConfig {
        strict_mode: true,
        blocked_sites: vec!["example.com".to_string()],
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;
    app.break_glass_expires_at = Some(Instant::now() + Duration::from_secs(120));

    assert!(app.strict_mode_enforced_for_focus());
}

#[test]
fn break_glass_requires_confirmation_before_unblocking() {
    let config = AppConfig {
        blocked_sites: vec!["example.com".to_string()],
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;
    app.apply_blocking_for_phase();
    assert!(app.should_block_for_current_state());

    app.handle_key(key(KeyCode::Char('u')));
    assert!(app.break_glass_confirmation_pending());
    assert!(app.should_block_for_current_state());

    app.handle_key(key(KeyCode::Char('u')));
    assert!(!app.break_glass_confirmation_pending());
    if app.break_glass_override_active() {
        assert!(!app.should_block_for_current_state());
    } else {
        assert!(app.should_block_for_current_state());
        assert!(
            app.phase_notification
                .as_deref()
                .is_some_and(|message| message.contains("failed"))
        );
    }
}

#[test]
fn break_glass_expiry_reapplies_blocking_and_logs_notification() {
    let mut app = App::default();
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;
    app.break_glass_expires_at = Some(Instant::now() - Duration::from_secs(1));
    app.poll_wakatime_status();

    assert!(!app.break_glass_override_active());
    assert!(app.should_block_for_current_state());
    assert!(
        app.phase_notification
            .as_deref()
            .is_some_and(|message| message.contains("expired"))
    );
}

#[test]
fn break_glass_is_rejected_when_focus_is_not_active() {
    let config = AppConfig {
        blocked_sites: vec!["example.com".to_string()],
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Idle;

    app.handle_key(key(KeyCode::Char('u')));

    assert!(!app.break_glass_confirmation_pending());
    assert!(
        app.phase_notification
            .as_deref()
            .is_some_and(|message| message.contains("only during active focus"))
    );
}

#[test]
fn break_glass_records_audit_event_in_stats() {
    let config = AppConfig {
        blocked_sites: vec!["example.com".to_string()],
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.task_labels = vec!["Project A".to_string()];
    app.selected_task_label = Some("Project A".to_string());
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;
    app.apply_blocking_for_phase();

    app.handle_key(key(KeyCode::Char('u')));
    app.handle_key(key(KeyCode::Char('u')));

    let overrides = app.recent_break_glass_overrides(1);
    if app.break_glass_override_active() {
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].date.len(), 10);
        assert_eq!(overrides[0].task_label.as_deref(), Some("Project A"));
        assert_eq!(overrides[0].duration_seconds, 5 * 60);
    } else {
        assert!(overrides.is_empty());
    }
}

#[test]
fn break_glass_without_sites_does_not_record_audit_event() {
    let mut app = App::default();
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;

    app.handle_key(key(KeyCode::Char('u')));

    assert!(!app.break_glass_confirmation_pending());
    assert!(app.recent_break_glass_overrides(1).is_empty());
}

#[test]
fn focus_elapsed_accumulates_session_and_today_minutes() {
    let mut app = App::default();
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;
    app.timer.remaining_secs = app.timer.focus_secs;

    for _ in 0..120 {
        app.on_tick(false);
    }

    assert_eq!(app.session_stats().focused_minutes(), 2);
    assert_eq!(app.today_stats().focused_minutes(), 2);
}

#[test]
fn on_tick_without_phase_change_does_not_sync_workflow_state() {
    let mut app = App::default();
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;
    app.timer.remaining_secs = app.timer.focus_secs;
    app.break_glass_expires_at = Some(Instant::now() + Duration::from_secs(120));
    session_recovery::clear_workflow_state().unwrap();

    app.on_tick(false);

    assert!(session_recovery::test_saved_workflow_snapshot().is_none());
}

#[test]
fn completed_focus_session_tracks_active_profile_for_history_totals() {
    let config = AppConfig {
        selected_profile: ProfileId::DeepWork,
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.task_labels = vec!["Project A".to_string()];
    app.selected_task_label = Some("Project A".to_string());

    app.handle_key(key(KeyCode::Char(' ')));
    app.timer.remaining_secs = 1;
    app.on_tick(false);

    let totals = app.profile_focus_totals();
    assert_eq!(totals.len(), 1);
    assert_eq!(totals[0].profile, crate::stats::ProfileBucket::DeepWork);
    assert_eq!(totals[0].pomodoros_completed, 1);
    assert_eq!(totals[0].focused_minutes(), 50);
}

#[test]
fn completed_focus_session_updates_task_totals_and_trends() {
    let mut app = App::default();
    app.task_labels = vec!["Project A".to_string()];
    app.selected_task_label = Some("Project A".to_string());

    app.handle_key(key(KeyCode::Char(' ')));
    app.timer.remaining_secs = 1;
    app.on_tick(false);

    let task_totals = app.task_focus_totals(5);
    assert_eq!(task_totals.len(), 1);
    assert_eq!(task_totals[0].task_label, "Project A");
    assert_eq!(task_totals[0].pomodoros_completed, 1);

    let trends = app.recent_task_trends(5);
    assert_eq!(trends.len(), 1);
    assert_eq!(trends[0].task_label, "Project A");
    assert!(trends[0].delta_focused_minutes() > 0);
}

#[test]
fn history_view_toggles_from_timer_mode() {
    let mut app = App::default();

    app.handle_key(key(KeyCode::Char('h')));
    assert_eq!(app.mode, AppMode::StatsHistory);

    app.handle_key(key(KeyCode::Esc));
    assert_eq!(app.mode, AppMode::Timer);
}

#[test]
fn history_view_cycles_comparison_dimensions_with_arrow_keys() {
    let mut app = App::default();
    app.handle_key(key(KeyCode::Char('h')));
    assert_eq!(app.mode, AppMode::StatsHistory);
    assert_eq!(
        app.history_comparison_dimension(),
        crate::stats::ComparisonDimension::TaskLabel
    );

    app.handle_key(key(KeyCode::Right));
    assert_eq!(
        app.history_comparison_dimension(),
        crate::stats::ComparisonDimension::Profile
    );

    app.handle_key(key(KeyCode::Right));
    assert_eq!(
        app.history_comparison_dimension(),
        crate::stats::ComparisonDimension::TimeOfDay
    );

    app.handle_key(key(KeyCode::Left));
    assert_eq!(
        app.history_comparison_dimension(),
        crate::stats::ComparisonDimension::Profile
    );
}

#[test]
fn history_view_comparison_dimension_wraps_in_both_directions() {
    let mut app = App::default();
    app.handle_key(key(KeyCode::Char('h')));
    assert_eq!(
        app.history_comparison_dimension(),
        crate::stats::ComparisonDimension::TaskLabel
    );

    app.handle_key(key(KeyCode::Left));
    assert_eq!(
        app.history_comparison_dimension(),
        crate::stats::ComparisonDimension::TimeOfDay
    );

    app.handle_key(key(KeyCode::Right));
    assert_eq!(
        app.history_comparison_dimension(),
        crate::stats::ComparisonDimension::TaskLabel
    );

    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Right));
    app.handle_key(key(KeyCode::Right));
    assert_eq!(
        app.history_comparison_dimension(),
        crate::stats::ComparisonDimension::TaskLabel
    );
}

#[test]
fn history_view_comparison_filters_cycle_with_wrap_and_stale_task_selection() {
    let mut app = App::default();
    app.task_labels = vec!["Alpha".to_string(), "Beta".to_string()];
    app.handle_key(key(KeyCode::Char('h')));

    assert_eq!(app.history_task_filter, None);
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.history_task_filter.as_deref(), Some("Alpha"));
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.history_task_filter.as_deref(), Some("Beta"));
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.history_task_filter, None);

    app.history_task_filter = Some("Ghost".to_string());
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.history_task_filter.as_deref(), Some("Alpha"));
    app.history_task_filter = Some("Ghost".to_string());
    app.handle_key(key(KeyCode::Up));
    assert_eq!(app.history_task_filter.as_deref(), Some("Beta"));

    assert_eq!(app.history_profile_filter, None);
    app.handle_key(key(KeyCode::Char(']')));
    assert_eq!(
        app.history_profile_filter,
        Some(crate::stats::ProfileBucket::Classic)
    );
    app.handle_key(key(KeyCode::Char('[')));
    assert_eq!(app.history_profile_filter, None);

    assert_eq!(app.history_time_of_day_filter, None);
    app.handle_key(key(KeyCode::Char('.')));
    assert_eq!(
        app.history_time_of_day_filter,
        Some(crate::stats::TimeOfDayBucket::Morning)
    );
    app.handle_key(key(KeyCode::Char(',')));
    assert_eq!(app.history_time_of_day_filter, None);
    app.handle_key(key(KeyCode::Char(',')));
    assert_eq!(
        app.history_time_of_day_filter,
        Some(crate::stats::TimeOfDayBucket::Unknown)
    );
}

#[test]
fn history_dashboard_view_data_reuses_cache_across_repeated_reads() {
    let mut app = App::default();
    seed_large_history(&mut app, 180, 4);
    app.handle_key(key(KeyCode::Char('h')));

    let _ = app.history_dashboard_view_data();
    let _ = app.history_dashboard_view_data();
    let cache_stats = app.history_dashboard_cache_stats();
    assert_eq!(cache_stats.static_rebuilds, 1);
    assert_eq!(cache_stats.comparison_rebuilds, 1);
}

#[test]
fn history_dashboard_filter_changes_rebuild_only_comparison_snapshot() {
    let mut app = App::default();
    seed_large_history(&mut app, 180, 4);
    app.handle_key(key(KeyCode::Char('h')));

    let _ = app.history_dashboard_view_data();
    let before = app.history_dashboard_cache_stats();

    app.handle_key(key(KeyCode::Down));
    let _ = app.history_dashboard_view_data();
    let after = app.history_dashboard_cache_stats();

    assert_eq!(after.static_rebuilds, before.static_rebuilds);
    assert_eq!(after.comparison_rebuilds, before.comparison_rebuilds + 1);
}

#[test]
fn history_dashboard_stats_change_rebuilds_static_and_comparison_snapshots() {
    let mut app = App::default();
    seed_large_history(&mut app, 180, 4);
    app.handle_key(key(KeyCode::Char('h')));

    let _ = app.history_dashboard_view_data();
    let before = app.history_dashboard_cache_stats();

    let goal = DailyGoalSnapshot {
        minutes: 25,
        pomodoros: 1,
    };
    let day_key = current_day_key();
    app.stats.record_completed_pomodoro_with_task(
        &day_key,
        goal,
        Some("Project C"),
        30 * 60,
        Some(ProfileId::DeepWork),
    );
    app.mark_stats_dirty();

    let _ = app.history_dashboard_view_data();
    let after = app.history_dashboard_cache_stats();
    assert_eq!(after.static_rebuilds, before.static_rebuilds + 1);
    assert_eq!(after.comparison_rebuilds, before.comparison_rebuilds + 1);
}

#[test]
fn history_dashboard_goal_config_change_rebuilds_static_snapshot() {
    let mut app = App::default();
    seed_large_history(&mut app, 180, 4);
    app.handle_key(key(KeyCode::Char('h')));

    let _ = app.history_dashboard_view_data();
    let before = app.history_dashboard_cache_stats();

    app.weekly_goal.minutes = app.weekly_goal.minutes.saturating_add(30);

    let _ = app.history_dashboard_view_data();
    let after = app.history_dashboard_cache_stats();
    assert_eq!(after.static_rebuilds, before.static_rebuilds + 1);
    assert_eq!(after.comparison_rebuilds, before.comparison_rebuilds);
}

#[test]
fn history_dashboard_uses_stable_default_layout_despite_legacy_customization() {
    let mut app = App::from_config(AppConfig {
        history_dashboard: HistoryDashboardConfig {
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
    });
    app.handle_key(key(KeyCode::Char('h')));

    assert_eq!(
        app.history_dashboard_cards(),
        vec![
            HistoryKpiCardId::SessionSummary,
            HistoryKpiCardId::FocusScore,
            HistoryKpiCardId::GoalStreak,
            HistoryKpiCardId::FocusRisk,
            HistoryKpiCardId::WeeklyAllocation,
            HistoryKpiCardId::LastInterruption,
            HistoryKpiCardId::StatsGrowth,
            HistoryKpiCardId::Retention,
            HistoryKpiCardId::ComparisonFilters
        ]
    );

    app.handle_key(key(KeyCode::Char('j')));
    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Char('<')));

    let persisted = app.persisted_config();
    assert_eq!(
        persisted.history_dashboard,
        HistoryDashboardConfig::default()
    );
    assert!(app.history_feedback.is_none());
}

#[test]
fn history_dashboard_deprecated_shortcuts_do_not_change_feedback() {
    let mut app = App::from_config(AppConfig {
        history_dashboard: HistoryDashboardConfig {
            card_order: vec![
                HistoryKpiCardId::SessionSummary,
                HistoryKpiCardId::FocusScore,
                HistoryKpiCardId::GoalStreak,
                HistoryKpiCardId::FocusRisk,
                HistoryKpiCardId::WeeklyAllocation,
                HistoryKpiCardId::LastInterruption,
                HistoryKpiCardId::StatsGrowth,
                HistoryKpiCardId::Retention,
                HistoryKpiCardId::ComparisonFilters,
            ],
            pinned_cards: vec![HistoryKpiCardId::SessionSummary],
        },
        ..AppConfig::default()
    });
    app.handle_key(key(KeyCode::Char('h')));
    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Char('>')));

    assert_eq!(
        app.history_dashboard_cards(),
        HistoryKpiCardId::all().to_vec()
    );
    assert!(app.history_feedback.is_none());
}

#[test]
fn custom_cancel_shortcut_controls_history_back_navigation() {
    let config = AppConfig {
        shortcuts: ShortcutConfig {
            cancel: "v".to_string(),
            ..ShortcutConfig::default()
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);

    app.handle_key(key(KeyCode::Char('h')));
    assert_eq!(app.mode, AppMode::StatsHistory);

    app.handle_key(key(KeyCode::Esc));
    assert_eq!(app.mode, AppMode::StatsHistory);

    app.handle_key(key(KeyCode::Char('v')));
    assert_eq!(app.mode, AppMode::Timer);
}

#[test]
fn custom_history_shortcut_opens_history_view() {
    let config = AppConfig {
        shortcuts: ShortcutConfig {
            open_stats_history: "y".to_string(),
            ..ShortcutConfig::default()
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);

    app.handle_key(key(KeyCode::Char('h')));
    assert_eq!(app.mode, AppMode::Timer);

    app.handle_key(key(KeyCode::Char('y')));
    assert_eq!(app.mode, AppMode::StatsHistory);
}

#[test]
fn custom_quit_shortcut_replaces_default_quit_char() {
    let config = AppConfig {
        shortcuts: ShortcutConfig {
            quit: "x".to_string(),
            ..ShortcutConfig::default()
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);

    app.handle_key(key(KeyCode::Char('q')));
    assert!(!app.should_quit);

    app.handle_key(key(KeyCode::Char('x')));
    assert!(app.should_quit);
}

#[test]
fn diagnostics_view_toggles_from_timer_mode() {
    let mut app = App::default();

    app.handle_key(key(KeyCode::Char('d')));
    assert_eq!(app.mode, AppMode::SetupDiagnostics);

    app.handle_key(key(KeyCode::Esc));
    assert_eq!(app.mode, AppMode::Timer);
}

#[test]
fn record_current_screen_usage_tracks_initial_timer_surface() {
    let mut app = App::default();
    app.record_current_screen_usage();
    let summary = app.stats.usage_signal_summary(6);
    assert_eq!(summary.screens.total_events, 1);
    assert_eq!(summary.screens.unique_surfaces, 1);
    assert_eq!(summary.screens.top[0].surface, "timer");
    assert_eq!(summary.screens.top[0].count, 1);
}

#[test]
fn screen_mode_transitions_record_usage_counts() {
    let mut app = App::default();
    app.record_current_screen_usage();
    app.open_site_manager();
    app.open_stats_history();
    app.open_profile_manager();
    app.open_session_planner();
    app.open_setup_diagnostics();
    app.handle_key(key(KeyCode::Esc));
    let summary = app.stats.usage_signal_summary(6);
    assert_eq!(summary.screens.total_events, 7);
    assert_eq!(summary.screens.unique_surfaces, 6);
    assert_eq!(summary.screens.top[0].surface, "timer");
    assert_eq!(summary.screens.top[0].count, 2);
    assert!(
        summary
            .screens
            .top
            .iter()
            .any(|entry| entry.surface == "site-manager")
    );
    assert!(
        summary
            .screens
            .top
            .iter()
            .any(|entry| entry.surface == "stats-history")
    );
    assert!(
        summary
            .screens
            .top
            .iter()
            .any(|entry| entry.surface == "profile-manager")
    );
    assert!(
        summary
            .screens
            .top
            .iter()
            .any(|entry| entry.surface == "task-setup")
    );
    assert!(
        summary
            .screens
            .top
            .iter()
            .any(|entry| entry.surface == "setup-diagnostics")
    );
}

#[test]
fn pending_strict_reset_confirmation_clears_when_opening_setup_diagnostics() {
    let config = AppConfig {
        strict_mode: true,
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;

    app.handle_key(key(KeyCode::Char('s')));
    assert!(app.strict_reset_confirmation_pending());

    app.handle_key(key(KeyCode::Char('d')));

    assert_eq!(app.mode, AppMode::SetupDiagnostics);
    assert!(!app.strict_reset_confirmation_pending());
}

#[test]
fn catchup_tick_does_not_increment_focus_stats() {
    let config = AppConfig {
        auto_start: AutoStartConfig {
            focus_to_break: true,
            break_to_focus: true,
        },
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;
    app.timer.remaining_secs = 1;

    app.on_tick(true);

    assert_eq!(app.timer.phase, TimerPhase::ShortBreak);
    assert_eq!(app.timer.status, TimerStatus::Idle);
    assert_eq!(app.session_stats().pomodoros_completed, 0);
    assert_eq!(app.session_stats().focused_seconds, 0);
    assert!(app.phase_notification.is_none());
}

#[test]
fn partial_focus_elapsed_marks_unsaved_flag_for_drop_flush() {
    let mut app = App::default();
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;
    app.timer.remaining_secs = app.timer.focus_secs;

    app.on_tick(false);

    assert!(app.stats_has_unsaved_elapsed);
    assert_eq!(app.session_stats().focused_seconds, 1);
}

#[test]
fn ctrl_c_quits_during_profile_edit() {
    let mut app = App::default();
    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Char('e')));
    app.handle_key(ctrl_key(KeyCode::Char('c')));
    assert!(app.should_quit);
}

#[test]
fn q_quits_during_wakatime_metadata_edit() {
    let mut app = App::default();
    app.handle_key(key(KeyCode::Char('p')));
    app.handle_key(key(KeyCode::Char('e')));
    for _ in 0..PROFILE_EDIT_WAKATIME_PROJECT_INDEX {
        app.handle_key(key(KeyCode::Down));
    }

    app.handle_key(key(KeyCode::Char('q')));

    assert!(app.should_quit);
    assert_eq!(app.wakatime_metadata.project, "focustime");
}

#[test]
fn poll_wakatime_status_applies_async_failure_event() {
    let mut app = App::default();
    app.replace_wakatime_tracker_for_tests(WakatimeTracker::new_configured_for_tests());
    app.wakatime_tracker_mut_for_tests()
        .expect("wakatime tracker should be available")
        .push_failed_event_for_tests("HTTP 503");

    app.poll_wakatime_status();

    assert_eq!(
        app.wakatime_runtime_state(),
        crate::wakatime::WakatimeRuntimeState::Error("HTTP 503".to_string())
    );
}

#[test]
fn poll_wakatime_status_transitions_queued_backlog_to_replaying() {
    let mut app = App::default();
    app.replace_wakatime_tracker_for_tests(WakatimeTracker::new_configured_for_tests());
    app.wakatime_tracker_mut_for_tests()
        .expect("wakatime tracker should be available")
        .set_pending_heartbeats_for_tests(2);
    assert!(matches!(
        app.wakatime_runtime_state(),
        crate::wakatime::WakatimeRuntimeState::Queued { pending: 2 }
    ));

    app.poll_wakatime_status();

    assert!(matches!(
        app.wakatime_runtime_state(),
        crate::wakatime::WakatimeRuntimeState::Replaying { pending: 2 }
    ));
}

#[test]
/// Verifies setup diagnostics expose queued WakaTime heartbeats as a warning.
fn setup_diagnostics_reports_wakatime_offline_queue_status() {
    let mut app = App::default();
    app.replace_wakatime_tracker_for_tests(WakatimeTracker::new_configured_for_tests());
    app.wakatime_tracker_mut_for_tests()
        .expect("wakatime tracker should be available")
        .set_pending_heartbeats_for_tests(3);

    app.refresh_setup_diagnostics();

    assert_eq!(
        app.setup_diagnostics.wakatime_runtime.level,
        SetupCheckLevel::Warning
    );
    assert!(
        app.setup_diagnostics
            .wakatime_runtime
            .message
            .contains("Queued: 3 WakaTime heartbeats pending replay")
    );
}

#[test]
fn focus_does_not_start_without_selected_task_label() {
    let mut app = App::default();

    app.handle_key(key(KeyCode::Char(' ')));

    assert_eq!(app.timer.status, TimerStatus::Idle);
    assert_eq!(
        app.phase_notification.as_deref(),
        Some("Select a task with [t] before starting focus.")
    );
}

#[test]
fn planner_rename_updates_active_focus_task_label() {
    let mut app = App::default();
    app.task_labels = vec!["Docs".to_string()];
    app.selected_task_label = Some("Docs".to_string());
    app.handle_key(key(KeyCode::Char(' ')));

    app.open_session_planner();
    app.planner_selection_index = 0;
    app.commit_planner_rename_input("Writing".to_string());

    assert_eq!(app.active_focus_task_label.as_deref(), Some("Writing"));
    let snapshot = session_recovery::test_saved_snapshot().expect("snapshot should be saved");
    assert_eq!(snapshot.selected_task_label.as_deref(), Some("Writing"));
}

#[test]
fn cli_start_fails_without_selected_task_label() {
    let mut app = App::default();

    let result = app.start_focus_for_cli();

    assert!(result.is_err());
    assert_eq!(app.timer.status, TimerStatus::Idle);
}

#[test]
fn cli_start_fails_when_selected_task_label_is_archived() {
    let mut app = App::default();
    app.task_labels = vec!["Docs".to_string()];
    app.selected_task_label = Some("Docs".to_string());
    app.task_label_archived.insert(task_label_key("Docs"));

    let result = app.start_focus_for_cli();

    assert!(result.is_err());
    assert_eq!(app.timer.status, TimerStatus::Idle);
}

#[test]
fn cli_start_begins_focus_when_task_label_exists() {
    let mut app = App::default();
    app.selected_task_label = Some("Docs".to_string());

    let result = app.start_focus_for_cli();

    assert!(result.is_ok());
    assert_eq!(app.timer.status, TimerStatus::Running);
    assert_eq!(app.active_focus_task_label, Some("Docs".to_string()));
}

#[test]
fn cli_pause_and_resume_transitions_timer_state() {
    let mut app = App::default();
    app.selected_task_label = Some("Docs".to_string());
    app.start_focus_for_cli().unwrap();

    app.pause_for_cli().unwrap();
    assert_eq!(app.timer.status, TimerStatus::Paused);

    app.resume_for_cli().unwrap();
    assert_eq!(app.timer.status, TimerStatus::Running);
}

#[test]
fn cli_pause_requires_running_timer() {
    let mut app = App::default();

    let error = app.pause_for_cli().unwrap_err();

    assert_eq!(error, "Cannot pause: timer is not running.");
    let message = error.user_message();
    assert_eq!(message.code, "app.timer.not_running");
    assert_eq!(
        message.hint.as_deref(),
        Some("Start a focus session first with `focustime --start`.")
    );
}

#[test]
fn cli_stop_respects_strict_mode_during_focus() {
    let mut app = App::default();
    app.strict_mode = true;
    app.selected_task_label = Some("Docs".to_string());
    app.start_focus_for_cli().unwrap();

    let error = app.stop_for_cli().unwrap_err();

    assert_eq!(error, "Cannot stop: strict mode is active during focus.");
    assert_eq!(app.timer.status, TimerStatus::Running);
}

#[test]
fn cli_next_respects_strict_mode_during_focus() {
    let mut app = App::default();
    app.strict_mode = true;
    app.selected_task_label = Some("Docs".to_string());
    app.start_focus_for_cli().unwrap();

    let error = app.next_phase_for_cli().unwrap_err();

    assert_eq!(
        error,
        "Cannot skip to next phase: strict mode is active during focus."
    );
    assert_eq!(app.timer.phase, TimerPhase::Focus);
    assert_eq!(app.timer.status, TimerStatus::Running);
}

#[test]
fn cli_stop_records_session_interruption_reason() {
    let mut app = App::default();
    app.selected_task_label = Some("Docs".to_string());
    app.start_focus_for_cli().unwrap();
    app.timer.remaining_secs = 1000;

    app.stop_for_cli().unwrap();

    let interruptions = app.recent_session_interruptions(1);
    assert_eq!(interruptions.len(), 1);
    assert_eq!(
        interruptions[0].reason,
        SessionInterruptionReason::ManualStop
    );
    assert_eq!(interruptions[0].task_label.as_deref(), Some("Docs"));
    assert_eq!(interruptions[0].remaining_secs, 1000);
}

#[test]
fn cli_next_records_session_interruption_reason() {
    let mut app = App::default();
    app.selected_task_label = Some("Docs".to_string());
    app.start_focus_for_cli().unwrap();
    app.timer.remaining_secs = 800;

    app.next_phase_for_cli().unwrap();

    let interruptions = app.recent_session_interruptions(1);
    assert_eq!(interruptions.len(), 1);
    assert_eq!(
        interruptions[0].reason,
        SessionInterruptionReason::ManualSkip
    );
    assert_eq!(interruptions[0].task_label.as_deref(), Some("Docs"));
    assert_eq!(interruptions[0].remaining_secs, 800);
}

#[test]
fn cli_break_glass_trigger_requires_active_focus() {
    let config = AppConfig {
        blocked_sites: vec!["example.com".to_string()],
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Idle;

    let error = app.trigger_break_glass_for_cli().unwrap_err();

    assert_eq!(
        error,
        "Break-glass override is available only during active focus."
    );
    assert!(!app.break_glass_confirmation_pending());
}

#[test]
fn cli_break_glass_trigger_persists_pending_confirmation() {
    let config = AppConfig {
        blocked_sites: vec!["example.com".to_string()],
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;

    app.trigger_break_glass_for_cli().unwrap();

    let snapshot = session_recovery::test_saved_workflow_snapshot()
        .expect("workflow snapshot should be saved");
    assert!(snapshot.temporary_overrides.contains(
        &session_recovery::WorkflowTemporaryOverrideSnapshot::break_glass_pending_confirmation()
    ));
}

#[test]
fn cli_break_glass_trigger_arms_confirmation_when_valid() {
    let config = AppConfig {
        blocked_sites: vec!["example.com".to_string()],
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;

    app.trigger_break_glass_for_cli().unwrap();

    assert!(app.break_glass_confirmation_pending());
}

#[test]
fn cli_break_glass_cancel_clears_persisted_workflow_state() {
    let config = AppConfig {
        blocked_sites: vec!["example.com".to_string()],
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;
    app.trigger_break_glass_for_cli().unwrap();
    assert!(
        session_recovery::test_saved_workflow_snapshot()
            .is_some_and(|snapshot| snapshot.temporary_overrides.contains(
                &session_recovery::WorkflowTemporaryOverrideSnapshot::break_glass_pending_confirmation()
            ))
    );

    app.cancel_break_glass_for_cli().unwrap();

    assert!(session_recovery::test_saved_workflow_snapshot().is_none());
}

#[test]
fn cli_break_glass_cancel_requires_pending_confirmation() {
    let mut app = App::default();

    let error = app.cancel_break_glass_for_cli().unwrap_err();

    assert_eq!(
        error,
        "Cannot cancel break-glass: no confirmation is pending."
    );
}

#[test]
fn app_restores_cli_workflow_state_from_snapshot() {
    session_recovery::set_test_load_snapshot(Some(snapshot_for_tests(
        TimerPhase::Focus,
        TimerStatus::Running,
        300,
        Some("Docs"),
        ProfileId::Classic,
    )));
    session_recovery::set_test_load_workflow_state(Some(session_recovery::WorkflowStateSnapshot {
        schedule_armed_occurrence_key: None,
        last_schedule_occurrence_key: None,
        strict_reset_confirmation_pending: false,
        temporary_overrides: vec![
            session_recovery::WorkflowTemporaryOverrideSnapshot::break_glass_pending_confirmation(),
        ],
    }));

    let app = App::default();

    assert!(app.break_glass_confirmation_pending());
}

#[test]
fn app_restores_temporary_overrides_from_canonical_snapshot() {
    let now = Local::now();
    session_recovery::set_test_load_snapshot(Some(snapshot_for_tests(
        TimerPhase::Focus,
        TimerStatus::Running,
        300,
        Some("Docs"),
        ProfileId::Classic,
    )));
    session_recovery::set_test_load_workflow_state(Some(session_recovery::WorkflowStateSnapshot {
        schedule_armed_occurrence_key: None,
        last_schedule_occurrence_key: None,
        strict_reset_confirmation_pending: false,
        temporary_overrides: vec![
            session_recovery::WorkflowTemporaryOverrideSnapshot::break_glass_active(
                (now + ChronoDuration::seconds(90)).timestamp(),
            ),
            session_recovery::WorkflowTemporaryOverrideSnapshot::temporary_allowlist(
                "Default",
                "reddit.com",
                (now + ChronoDuration::seconds(120)).timestamp(),
            ),
        ],
    }));

    let app = App::default();

    assert!(app.break_glass_override_remaining_secs().is_some());
    assert_eq!(app.active_temporary_allowlist_count(), 1);
    assert_eq!(
        app.active_temporary_allowlist_entries()[0].site,
        "reddit.com"
    );
}

#[test]
fn app_restores_schedule_arming_continuity_from_workflow_snapshot() {
    let now = local_datetime_today(10, 15);
    let config = AppConfig {
        recurring_schedule: RecurringScheduleConfig {
            windows: vec![RecurringFocusWindowConfig {
                days: vec![weekday_token(now.weekday()).to_string()],
                start: "00:00".to_string(),
                end: "23:59".to_string(),
            }],
        },
        ..AppConfig::default()
    };
    let probe_app = App::from_config(config.clone());
    let active_occurrence = probe_app
        .active_schedule_occurrence_at(now)
        .expect("schedule window should be active");
    let active_occurrence_key = occurrence_key(&active_occurrence);

    session_recovery::set_test_load_workflow_state(Some(session_recovery::WorkflowStateSnapshot {
        schedule_armed_occurrence_key: Some(active_occurrence_key.clone()),
        last_schedule_occurrence_key: Some(active_occurrence_key.clone()),
        strict_reset_confirmation_pending: false,
        temporary_overrides: Vec::new(),
    }));

    let app = App::from_config(config);

    assert_eq!(
        app.schedule_armed_occurrence_key.as_deref(),
        Some(active_occurrence_key.as_str())
    );
    assert_eq!(
        app.last_schedule_occurrence_key.as_deref(),
        Some(active_occurrence_key.as_str())
    );
}

#[test]
fn app_restores_strict_reset_confirmation_from_workflow_snapshot() {
    session_recovery::set_test_load_snapshot(Some(snapshot_for_tests(
        TimerPhase::Focus,
        TimerStatus::Running,
        300,
        Some("Docs"),
        ProfileId::Classic,
    )));
    session_recovery::set_test_load_workflow_state(Some(session_recovery::WorkflowStateSnapshot {
        schedule_armed_occurrence_key: None,
        last_schedule_occurrence_key: None,
        strict_reset_confirmation_pending: true,
        temporary_overrides: Vec::new(),
    }));

    let app = App::from_config(AppConfig {
        strict_mode: true,
        ..AppConfig::default()
    });

    assert!(app.strict_reset_confirmation_pending());
}

#[test]
fn app_reports_partial_runtime_recovery_notice_for_ignored_workflow_artifacts() {
    session_recovery::set_test_load_workflow_state(Some(session_recovery::WorkflowStateSnapshot {
        schedule_armed_occurrence_key: Some("recurring:stale".to_string()),
        last_schedule_occurrence_key: Some("recurring:stale".to_string()),
        strict_reset_confirmation_pending: true,
        temporary_overrides: vec![
            session_recovery::WorkflowTemporaryOverrideSnapshot::break_glass_pending_confirmation(),
        ],
    }));

    let app = App::default();

    let notice = app
        .phase_notification
        .as_deref()
        .expect("partial recovery should emit a startup notice");
    assert!(notice.contains("Ignored saved runtime artifacts"));
    assert!(notice.contains("schedule arm state"));
    assert!(notice.contains("schedule trigger continuity"));
    assert!(notice.contains("break-glass confirmation"));
    assert!(notice.contains("strict reset confirmation"));
}

#[test]
fn cli_break_glass_cancel_clears_pending_confirmation() {
    let config = AppConfig {
        blocked_sites: vec!["example.com".to_string()],
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;
    app.trigger_break_glass_for_cli().unwrap();
    assert!(app.break_glass_confirmation_pending());

    app.cancel_break_glass_for_cli().unwrap();

    assert!(!app.break_glass_confirmation_pending());
}

#[test]
fn cli_break_glass_trigger_reports_active_override() {
    let config = AppConfig {
        blocked_sites: vec!["example.com".to_string()],
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;
    app.break_glass_expires_at = Some(Instant::now() + Duration::from_secs(90));

    let error = app.trigger_break_glass_for_cli().unwrap_err();

    assert!(error.contains("Break-glass override already active"));
    assert!(!app.break_glass_confirmation_pending());
}

#[test]
fn cli_break_glass_trigger_pending_confirm_rechecks_focus_state() {
    let config = AppConfig {
        blocked_sites: vec!["example.com".to_string()],
        ..AppConfig::default()
    };
    let mut app = App::from_config(config);
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;
    app.trigger_break_glass_for_cli().unwrap();
    assert!(app.break_glass_confirmation_pending());
    app.timer.status = TimerStatus::Idle;

    let error = app.trigger_break_glass_for_cli().unwrap_err();

    assert_eq!(
        error,
        "Break-glass override is available only during active focus."
    );
    assert!(!app.break_glass_confirmation_pending());
}

#[test]
fn cli_task_selection_auto_creates_and_reuses_labels() {
    let mut app = App::default();

    let created = app.select_task_label_for_cli("  Docs  ").unwrap();
    assert!(created);
    assert_eq!(app.task_labels, vec!["Docs".to_string()]);
    assert_eq!(app.selected_task_label.as_deref(), Some("Docs"));

    let created = app.select_task_label_for_cli("docs").unwrap();
    assert!(!created);
    assert_eq!(app.task_labels, vec!["Docs".to_string()]);
    assert_eq!(app.selected_task_label.as_deref(), Some("Docs"));
}

#[test]
fn cli_task_selection_rejects_archived_labels() {
    let mut app = App::default();
    app.task_labels = vec!["Docs".to_string()];
    app.task_label_archived.insert(task_label_key("Docs"));

    let error = app.select_task_label_for_cli("Docs").unwrap_err();

    assert!(error.contains("Cannot select archived task label"));
    assert!(app.selected_task_label.is_none());
}

#[test]
fn session_planner_adds_label_and_allows_focus_start() {
    let mut app = App::default();

    app.handle_key(key(KeyCode::Char('t')));
    assert_eq!(app.mode, AppMode::TaskSetup);

    app.handle_key(key(KeyCode::Char('a')));
    for c in "Docs".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    app.handle_key(key(KeyCode::Enter));

    assert_eq!(app.selected_task_label.as_deref(), Some("Docs"));
    assert_eq!(app.current_task_label(), Some("Docs"));

    app.handle_key(key(KeyCode::Char('t')));
    assert_eq!(app.mode, AppMode::Timer);

    app.handle_key(key(KeyCode::Char(' ')));
    assert_eq!(app.timer.status, TimerStatus::Running);
}

#[test]
fn session_planner_renames_highlighted_label_and_updates_selection() {
    let mut app = App::default();
    app.task_labels = vec!["Docs".to_string(), "Review".to_string()];
    app.selected_task_label = Some("Docs".to_string());

    app.handle_key(key(KeyCode::Char('t')));
    app.handle_key(key(KeyCode::Char('e')));
    assert!(app.planner_input_active);
    assert_eq!(app.planner_input_mode, Some(PlannerInputMode::Rename));

    app.planner_input = "Writing".to_string();
    app.handle_key(key(KeyCode::Enter));

    assert_eq!(
        app.task_labels,
        vec!["Writing".to_string(), "Review".to_string()]
    );
    assert_eq!(app.selected_task_label.as_deref(), Some("Writing"));
    assert!(!app.planner_input_active);
    assert_eq!(app.planner_input_mode, None);
    assert!(
        app.planner_feedback
            .as_ref()
            .is_some_and(|feedback| feedback.message.contains("Renamed"))
    );
}

#[test]
fn session_planner_delete_selected_label_selects_nearest_remaining() {
    let mut app = App::default();
    app.task_labels = vec![
        "Docs".to_string(),
        "Review".to_string(),
        "Planning".to_string(),
    ];
    app.selected_task_label = Some("Review".to_string());

    app.handle_key(key(KeyCode::Char('t')));
    app.planner_selection_index = 1;
    app.handle_key(key(KeyCode::Char('d')));

    assert_eq!(
        app.task_labels,
        vec!["Docs".to_string(), "Planning".to_string()]
    );
    assert_eq!(app.selected_task_label.as_deref(), Some("Planning"));
    assert_eq!(app.planner_selection_index, 1);
}

#[test]
fn session_planner_delete_last_label_clears_selection() {
    let mut app = App::default();
    app.task_labels = vec!["Docs".to_string()];
    app.selected_task_label = Some("Docs".to_string());

    app.handle_key(key(KeyCode::Char('t')));
    app.handle_key(key(KeyCode::Delete));

    assert!(app.task_labels.is_empty());
    assert!(app.selected_task_label.is_none());
    assert_eq!(app.planner_selection_index, 0);
}

#[test]
fn session_planner_toggle_favorite_updates_display_order() {
    let mut app = App::default();
    app.task_labels = vec!["Docs".to_string(), "Review".to_string()];
    app.selected_task_label = Some("Docs".to_string());

    app.open_session_planner();
    app.planner_selection_index = 1;
    app.handle_key(key(KeyCode::Char('f')));

    assert!(app.is_task_label_favorite("Review"));
    assert_eq!(
        app.planner_labels_for_display(),
        vec!["Review".to_string(), "Docs".to_string()]
    );
}

#[test]
fn session_planner_archive_blocks_selecting_archived_label() {
    let mut app = App::default();
    app.task_labels = vec!["Docs".to_string(), "Review".to_string()];
    app.selected_task_label = Some("Review".to_string());

    app.open_session_planner();
    app.planner_selection_index = 0;
    app.handle_key(key(KeyCode::Char('x')));
    assert!(app.is_task_label_archived("Docs"));

    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.selected_task_label.as_deref(), Some("Review"));
    assert!(
        app.planner_feedback
            .as_ref()
            .is_some_and(|feedback| { feedback.message.contains("Cannot select archived") })
    );
}

#[test]
fn session_planner_archiving_selected_label_switches_to_selectable_label() {
    let mut app = App::default();
    app.task_labels = vec!["Docs".to_string(), "Review".to_string()];
    app.selected_task_label = Some("Docs".to_string());

    app.open_session_planner();
    app.handle_key(key(KeyCode::Char('x')));

    assert!(app.is_task_label_archived("Docs"));
    assert_eq!(app.selected_task_label.as_deref(), Some("Review"));
}

#[test]
fn session_planner_archiving_selected_label_prefers_nearest_selectable_label() {
    let mut app = App::default();
    app.task_labels = vec![
        "Design".to_string(),
        "Build".to_string(),
        "Docs".to_string(),
    ];
    app.selected_task_label = Some("Docs".to_string());

    app.open_session_planner();
    app.planner_selection_index = 2;
    app.handle_key(key(KeyCode::Char('x')));

    assert!(app.is_task_label_archived("Docs"));
    assert_eq!(app.selected_task_label.as_deref(), Some("Build"));
}

#[test]
fn session_planner_archiving_selected_label_prefers_left_when_closer_than_right() {
    let mut app = App::default();
    app.task_labels = vec![
        "Design".to_string(),
        "Build".to_string(),
        "Docs".to_string(),
        "Review".to_string(),
    ];
    app.selected_task_label = Some("Build".to_string());
    app.task_label_archived.insert(task_label_key("Docs"));

    app.open_session_planner();
    app.planner_selection_index = 1;
    app.handle_key(key(KeyCode::Char('x')));

    assert!(app.is_task_label_archived("Build"));
    assert_eq!(app.selected_task_label.as_deref(), Some("Design"));
}

#[test]
fn session_planner_recent_quick_pick_selects_recent_labels() {
    let mut app = App::default();
    app.task_labels = vec![
        "Docs".to_string(),
        "Review".to_string(),
        "Bugfix".to_string(),
    ];
    app.selected_task_label = Some("Docs".to_string());
    let goal = DailyGoalSnapshot {
        minutes: 25,
        pomodoros: 1,
    };
    app.stats
        .record_completed_pomodoro_with_task("2026-04-07", goal, Some("Docs"), 25 * 60, None);
    app.stats.record_completed_pomodoro_with_task(
        "2026-04-08",
        goal,
        Some("Review"),
        25 * 60,
        None,
    );
    app.stats.record_completed_pomodoro_with_task(
        "2026-04-09",
        goal,
        Some("Bugfix"),
        25 * 60,
        None,
    );

    app.handle_key(key(KeyCode::Char('t')));
    app.handle_key(key(KeyCode::Char('r')));
    assert_eq!(app.selected_task_label.as_deref(), Some("Bugfix"));

    app.handle_key(key(KeyCode::Char('2')));
    assert_eq!(app.selected_task_label.as_deref(), Some("Review"));
}

#[test]
fn planner_recent_quick_pick_skips_archived_labels() {
    let mut app = App::default();
    app.task_labels = vec!["Docs".to_string(), "Review".to_string()];
    app.selected_task_label = Some("Docs".to_string());
    app.task_label_archived.insert(task_label_key("Review"));
    let goal = DailyGoalSnapshot {
        minutes: 25,
        pomodoros: 1,
    };
    app.stats.record_completed_pomodoro_with_task(
        "2026-04-08",
        goal,
        Some("Review"),
        25 * 60,
        None,
    );
    app.stats
        .record_completed_pomodoro_with_task("2026-04-09", goal, Some("Docs"), 25 * 60, None);

    app.open_session_planner();
    app.handle_key(key(KeyCode::Char('r')));
    assert_eq!(app.selected_task_label.as_deref(), Some("Docs"));
}

#[test]
fn planner_recent_labels_finds_older_valid_labels_after_stale_entries() {
    let mut app = App::default();
    app.task_labels = vec!["Keep A".to_string(), "Keep B".to_string()];
    let goal = DailyGoalSnapshot {
        minutes: 25,
        pomodoros: 1,
    };

    app.stats.record_completed_pomodoro_with_task(
        "2026-04-01",
        goal,
        Some("Keep A"),
        25 * 60,
        None,
    );
    app.stats.record_completed_pomodoro_with_task(
        "2026-04-02",
        goal,
        Some("Keep B"),
        25 * 60,
        None,
    );
    for i in 0..20 {
        let label = format!("Stale {i}");
        app.stats.record_completed_pomodoro_with_task(
            "2026-04-10",
            goal,
            Some(&label),
            25 * 60,
            None,
        );
    }

    let recent = app.planner_recent_labels(2);
    assert_eq!(recent, vec!["Keep B".to_string(), "Keep A".to_string()]);
}

#[test]
fn startup_restores_valid_in_progress_snapshot() {
    let _ = blocker::take_test_blocking_action();
    session_recovery::set_test_load_snapshot(Some(snapshot_for_tests(
        TimerPhase::Focus,
        TimerStatus::Running,
        42,
        Some("Docs"),
        ProfileId::DeepWork,
    )));

    let app = App::from_config(AppConfig::default());

    assert_eq!(app.timer.phase, TimerPhase::Focus);
    assert_eq!(app.timer.status, TimerStatus::Running);
    assert_eq!(app.timer.remaining_secs, 42);
    assert_eq!(app.selected_profile, ProfileId::DeepWork);
    assert_eq!(app.selected_task_label.as_deref(), Some("Docs"));
    assert_eq!(blocker::take_test_blocking_action(), Some("block"));
    assert!(
        app.phase_notification
            .as_deref()
            .is_some_and(|message| message.contains("Recovered in-progress Focus session"))
    );
}

#[test]
fn startup_reconciles_elapsed_recovery_time_while_session_is_running() {
    let now_epoch_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("current time should be after unix epoch")
        .as_secs() as i64;
    session_recovery::set_test_load_snapshot(Some(InProgressSessionSnapshot {
        phase: RecoveryTimerPhase::Focus,
        status: RecoveryTimerStatus::Running,
        remaining_secs: 120,
        pomodoros_completed: 0,
        selected_task_label: Some("Docs".to_string()),
        selected_profile: ProfileId::Classic,
        captured_at_epoch_secs: Some(now_epoch_secs - 10),
    }));

    let app = App::from_config(AppConfig::default());

    assert_eq!(app.timer.phase, TimerPhase::Focus);
    assert_eq!(app.timer.status, TimerStatus::Running);
    assert!((109..=111).contains(&app.timer.remaining_secs));
    assert_eq!(app.selected_task_label.as_deref(), Some("Docs"));
}

#[test]
fn startup_reconciles_elapsed_recovery_time_when_phase_completed_offline() {
    session_recovery::set_test_load_snapshot(Some(InProgressSessionSnapshot {
        phase: RecoveryTimerPhase::Focus,
        status: RecoveryTimerStatus::Running,
        remaining_secs: 1,
        pomodoros_completed: 0,
        selected_task_label: Some("Docs".to_string()),
        selected_profile: ProfileId::Classic,
        captured_at_epoch_secs: Some(0),
    }));

    let app = App::from_config(AppConfig::default());

    assert_eq!(app.timer.phase, TimerPhase::ShortBreak);
    assert_eq!(app.timer.status, TimerStatus::Idle);
    assert_eq!(app.timer.remaining_secs, app.timer.short_break_secs);
    assert_eq!(app.timer.pomodoros_completed, 1);
    assert_eq!(app.selected_task_label.as_deref(), Some("Docs"));
    assert!(app.phase_notification.as_deref().is_some_and(|message| {
        message.contains("Recovered elapsed timer state into Short Break phase")
    }));
    assert!(session_recovery::test_saved_snapshot().is_none());
}

#[test]
fn startup_recovery_rehydrates_profile_automation_runtime_for_snapshot_profile() {
    let deep_work_schedule = RecurringScheduleConfig {
        windows: vec![RecurringFocusWindowConfig {
            days: vec!["mon".to_string()],
            start: "09:00".to_string(),
            end: "11:00".to_string(),
        }],
    };
    let config = AppConfig {
        selected_profile: ProfileId::Custom,
        strict_mode: false,
        profile_automation: Some(ProfileAutomationSettingsConfig {
            basic: Some(ProfileAutomationConfig::default()),
            standard: Some(ProfileAutomationConfig {
                notifications: NotificationConfig {
                    enabled: false,
                    sound: true,
                },
                auto_start: AutoStartConfig {
                    focus_to_break: true,
                    break_to_focus: true,
                },
                strict_mode: true,
                recurring_schedule: deep_work_schedule.clone(),
            }),
            advanced: Some(ProfileAutomationConfig::default()),
        }),
        ..AppConfig::default()
    };
    session_recovery::set_test_load_snapshot(Some(snapshot_for_tests(
        TimerPhase::Focus,
        TimerStatus::Running,
        42,
        Some("Docs"),
        ProfileId::DeepWork,
    )));

    let app = App::from_config(config);

    assert_eq!(app.selected_profile, ProfileId::DeepWork);
    assert_eq!(app.timer.phase, TimerPhase::Focus);
    assert_eq!(app.timer.status, TimerStatus::Running);
    assert!(!app.notification_settings.enabled);
    assert!(app.notification_settings.sound);
    assert!(app.auto_start.focus_to_break);
    assert!(app.auto_start.break_to_focus);
    assert!(app.strict_mode);
    assert_eq!(app.recurring_schedule, deep_work_schedule);
}

#[test]
fn startup_restores_pomodoro_count_for_phase_cadence() {
    session_recovery::set_test_load_snapshot(Some(InProgressSessionSnapshot {
        phase: RecoveryTimerPhase::Focus,
        status: RecoveryTimerStatus::Running,
        remaining_secs: 1,
        pomodoros_completed: 3,
        selected_task_label: Some("Docs".to_string()),
        selected_profile: ProfileId::Classic,
        captured_at_epoch_secs: None,
    }));

    let mut app = App::from_config(AppConfig::default());
    assert_eq!(app.timer.pomodoros_completed, 3);

    app.on_tick(false);

    assert_eq!(app.timer.phase, TimerPhase::LongBreak);
}

#[test]
fn startup_ignores_invalid_snapshot_and_starts_fresh() {
    session_recovery::set_test_load_snapshot(Some(snapshot_for_tests(
        TimerPhase::Focus,
        TimerStatus::Idle,
        60,
        Some("Docs"),
        ProfileId::Classic,
    )));

    let app = App::from_config(AppConfig::default());

    assert_eq!(app.timer.phase, TimerPhase::Focus);
    assert_eq!(app.timer.status, TimerStatus::Idle);
    assert_eq!(app.timer.remaining_secs, DEFAULT_FOCUS_SECS);
    assert!(
        app.phase_notification
            .as_deref()
            .is_some_and(|message| message.contains("Ignored saved in-progress session"))
    );
    assert!(session_recovery::test_saved_snapshot().is_none());
}

#[test]
fn startup_reports_recovery_load_error() {
    let _ = blocker::take_test_blocking_action();
    session_recovery::set_test_load_error("simulated read failure");

    let app = App::from_config(AppConfig::default());

    assert_eq!(blocker::take_test_blocking_action(), Some("unblock"));
    assert!(
        app.phase_notification
            .as_deref()
            .is_some_and(|message| message.contains("Ignored saved in-progress session"))
    );
}

#[test]
fn reset_clears_saved_recovery_snapshot() {
    let mut app = App::default();
    app.selected_task_label = Some("Docs".to_string());

    app.handle_key(key(KeyCode::Char(' ')));
    assert_eq!(app.timer.status, TimerStatus::Running);
    assert!(session_recovery::test_saved_snapshot().is_some());

    app.handle_key(key(KeyCode::Char('s')));
    assert_eq!(app.timer.status, TimerStatus::Idle);
    assert!(session_recovery::test_saved_snapshot().is_none());
}

#[test]
fn recovery_snapshot_prefers_active_focus_label_over_selected_label() {
    let mut app = App::default();
    app.task_labels = vec!["Task A".to_string(), "Task B".to_string()];
    app.selected_task_label = Some("Task A".to_string());

    app.handle_key(key(KeyCode::Char(' ')));
    assert_eq!(app.active_focus_task_label.as_deref(), Some("Task A"));

    app.selected_task_label = Some("Task B".to_string());
    app.sync_recovery_snapshot();

    let snapshot = session_recovery::test_saved_snapshot().expect("snapshot should be saved");
    assert_eq!(snapshot.selected_task_label.as_deref(), Some("Task A"));
}

#[test]
fn planner_label_change_during_running_break_updates_recovery_snapshot() {
    let mut app = App::default();
    app.task_labels = vec!["Task A".to_string(), "Task B".to_string()];
    app.selected_task_label = Some("Task A".to_string());
    app.sync_task_planner_state();

    app.handle_key(key(KeyCode::Char(' '))); // focus running
    app.handle_key(key(KeyCode::Char('n'))); // short break idle
    app.handle_key(key(KeyCode::Char(' '))); // short break running
    assert_eq!(app.timer.phase, TimerPhase::ShortBreak);
    assert_eq!(app.timer.status, TimerStatus::Running);

    app.open_session_planner();
    app.planner_selection_index = 1;
    app.select_planner_label();

    let snapshot = session_recovery::test_saved_snapshot().expect("snapshot should be saved");
    assert_eq!(snapshot.selected_task_label.as_deref(), Some("Task B"));
    assert_eq!(snapshot.phase, RecoveryTimerPhase::ShortBreak);
    assert_eq!(snapshot.status, RecoveryTimerStatus::Running);
}

#[test]
fn prevent_double_input_on_windows() {
    let mut press_event = KeyEvent::from(KeyCode::Char('a'));
    press_event.kind = KeyEventKind::Press;

    let mut release_event = KeyEvent::from(KeyCode::Char('a'));
    release_event.kind = KeyEventKind::Release;

    assert!(
        should_handle_key(&press_event),
        "Press event should be handled"
    );
    assert!(
        !should_handle_key(&release_event),
        "Release event should NOT be handled"
    );
}
