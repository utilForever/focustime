use crate::config::*;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(target_os = "windows")]
const CONFIG_DIR_ENV: &str = "APPDATA";
#[cfg(not(target_os = "windows"))]
const CONFIG_DIR_ENV: &str = "XDG_CONFIG_HOME";

fn unique_temp_base(test_name: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "focustime-config-test-{test_name}-{}-{now}",
        std::process::id()
    ))
}

#[test]
fn default_values_are_canonical_pomodoro() {
    let cfg = AppConfig::default();
    assert_eq!(cfg.focus_secs, 25 * 60);
    assert_eq!(cfg.short_break_secs, 5 * 60);
    assert_eq!(cfg.long_break_secs, 15 * 60);
    assert_eq!(cfg.long_break_interval, 4);
    assert_eq!(cfg.selected_profile, ProfileId::Custom);
    assert!(cfg.custom_profile.is_none());
    assert_eq!(cfg.selected_theme_preset, ThemePreset::Classic);
    assert!(cfg.blocked_sites.is_empty());
    assert_eq!(cfg.selected_blocklist_profile, "Default");
    assert!(cfg.blocklist_profiles.is_empty());
    assert_eq!(cfg.notifications, NotificationConfig::default());
    assert_eq!(cfg.auto_start, AutoStartConfig::default());
    assert_eq!(cfg.recurring_schedule, RecurringScheduleConfig::default());
    assert_eq!(cfg.schedule_runtime, ScheduleRuntimeConfig::default());
    assert!(cfg.profile_automation.is_none());
    assert!(!cfg.strict_mode);
    assert_eq!(cfg.daily_goal, DailyGoalConfig::default());
    assert_eq!(cfg.weekly_goal, WeeklyGoalConfig::default());
    assert_eq!(cfg.monthly_goal, MonthlyGoalConfig::default());
    assert_eq!(cfg.goal_carry_over, GoalCarryOverConfig::default());
    assert_eq!(cfg.stats_retention, StatsRetentionConfig::default());
    assert_eq!(cfg.history_dashboard, HistoryDashboardConfig::default());
    assert_eq!(cfg.feature_flags, FeatureFlagsConfig::default());
    assert_eq!(cfg.shortcuts, ShortcutConfig::default());
}

#[test]
fn stats_retention_defaults_to_balanced_windows() {
    let cfg = AppConfig::default();
    assert_eq!(cfg.stats_retention.preset, StatsRetentionPreset::Balanced);
    let windows = cfg.stats_retention.windows();
    assert_eq!(windows.keep_daily_days, None);
    assert_eq!(windows.keep_focus_sessions_days, Some(365));
    assert_eq!(windows.keep_session_interruptions_days, Some(180));
    assert_eq!(windows.keep_weekly_goal_snapshots_days, Some(365));
    assert_eq!(windows.keep_monthly_goal_snapshots_days, Some(365));
}

#[test]
fn stats_retention_windows_change_by_preset() {
    let keep_all = StatsRetentionConfig {
        preset: StatsRetentionPreset::KeepAll,
    }
    .windows();
    assert_eq!(keep_all.keep_daily_days, None);
    assert_eq!(keep_all.keep_focus_sessions_days, None);
    assert_eq!(keep_all.keep_session_interruptions_days, None);
    assert_eq!(keep_all.keep_weekly_goal_snapshots_days, None);
    assert_eq!(keep_all.keep_monthly_goal_snapshots_days, None);

    let aggressive = StatsRetentionConfig {
        preset: StatsRetentionPreset::Aggressive,
    }
    .windows();
    assert_eq!(aggressive.keep_daily_days, Some(365));
    assert_eq!(aggressive.keep_focus_sessions_days, Some(180));
    assert_eq!(aggressive.keep_session_interruptions_days, Some(90));
    assert_eq!(aggressive.keep_weekly_goal_snapshots_days, Some(180));
    assert_eq!(aggressive.keep_monthly_goal_snapshots_days, Some(180));
}

/// Verifies the full config schema round-trips through serialization.
#[test]
fn round_trip_full_config() {
    let original = AppConfig {
        focus_secs: 50 * 60,
        short_break_secs: 10 * 60,
        long_break_secs: 20 * 60,
        long_break_interval: 3,
        blocked_sites: vec!["example.com".to_string(), "reddit.com".to_string()],
        blocklist_profiles: vec![
            BlocklistProfileConfig {
                name: "Work".to_string(),
                sites: vec!["example.com".to_string(), "reddit.com".to_string()],
                allowlist_sites: vec!["reddit.com".to_string()],
            },
            BlocklistProfileConfig {
                name: "Study".to_string(),
                sites: vec!["x.com".to_string()],
                allowlist_sites: Vec::new(),
            },
        ],
        selected_blocklist_profile: "Study".to_string(),
        selected_profile: ProfileId::DeepWork,
        custom_profile: Some(CustomProfileConfig {
            focus_secs: 30 * 60,
            short_break_secs: 7 * 60,
            long_break_secs: 12 * 60,
            long_break_interval: 5,
        }),
        selected_theme_preset: ThemePreset::HighContrast,
        notifications: NotificationConfig {
            enabled: true,
            sound: true,
        },
        auto_start: AutoStartConfig {
            focus_to_break: true,
            break_to_focus: false,
        },
        recurring_schedule: RecurringScheduleConfig {
            windows: vec![RecurringFocusWindowConfig {
                days: vec!["mon".to_string(), "wed".to_string()],
                start: "09:15".to_string(),
                end: "11:00".to_string(),
            }],
        },
        schedule_runtime: ScheduleRuntimeConfig {
            time_step_minutes: 20,
        },
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
                recurring_schedule: RecurringScheduleConfig::default(),
            }),
            standard: Some(ProfileAutomationConfig {
                notifications: NotificationConfig {
                    enabled: true,
                    sound: true,
                },
                auto_start: AutoStartConfig {
                    focus_to_break: true,
                    break_to_focus: false,
                },
                strict_mode: true,
                recurring_schedule: RecurringScheduleConfig {
                    windows: vec![RecurringFocusWindowConfig {
                        days: vec!["mon".to_string(), "wed".to_string()],
                        start: "09:15".to_string(),
                        end: "11:00".to_string(),
                    }],
                },
            }),
            advanced: Some(ProfileAutomationConfig {
                notifications: NotificationConfig::default(),
                auto_start: AutoStartConfig::default(),
                strict_mode: false,
                recurring_schedule: RecurringScheduleConfig::default(),
            }),
        }),
        strict_mode: true,
        daily_goal: DailyGoalConfig {
            minutes: 180,
            pomodoros: 6,
        },
        weekly_goal: WeeklyGoalConfig {
            minutes: 600,
            pomodoros: 20,
        },
        monthly_goal: MonthlyGoalConfig {
            minutes: 2400,
            pomodoros: 80,
        },
        goal_carry_over: GoalCarryOverConfig {
            daily: true,
            weekly: false,
            monthly: true,
        },
        stats_retention: StatsRetentionConfig {
            preset: StatsRetentionPreset::Aggressive,
        },
        history_dashboard: HistoryDashboardConfig {
            card_order: vec![
                HistoryKpiCardId::FocusScore,
                HistoryKpiCardId::GoalStreak,
                HistoryKpiCardId::SessionSummary,
                HistoryKpiCardId::FocusRisk,
                HistoryKpiCardId::WeeklyAllocation,
                HistoryKpiCardId::LastInterruption,
                HistoryKpiCardId::StatsGrowth,
                HistoryKpiCardId::Retention,
                HistoryKpiCardId::ComparisonFilters,
            ],
            pinned_cards: vec![
                HistoryKpiCardId::FocusScore,
                HistoryKpiCardId::SessionSummary,
                HistoryKpiCardId::Retention,
            ],
        },
        feature_flags: FeatureFlagsConfig::default(),
        shortcuts: ShortcutConfig {
            timer_toggle_pause: "space".to_string(),
            timer_stop_reset: "x".to_string(),
            open_stats_history: "y".to_string(),
            back_stats_history: "y".to_string(),
            quit: "v".to_string(),
            ..ShortcutConfig::default()
        },
    };
    let toml_str = toml::to_string_pretty(&original).unwrap();
    let serialized: toml::Value = toml::from_str(&toml_str).unwrap();
    let root = serialized.as_table().unwrap();
    assert!(!root.contains_key("focus_secs"));
    assert!(!root.contains_key("short_break_secs"));
    assert!(!root.contains_key("long_break_secs"));
    assert!(!root.contains_key("long_break_interval"));
    assert!(!root.contains_key("blocked_sites"));
    assert!(!root.contains_key("notifications"));
    assert!(!root.contains_key("auto_start"));
    assert!(!root.contains_key("strict_mode"));
    assert!(!root.contains_key("recurring_schedule"));
    assert!(!root.contains_key("calendar_sync"));
    assert!(!root.contains_key("wakatime"));
    assert!(!root.contains_key("wakatime_runtime"));
    assert!(!root.contains_key("weekday_profile_rules"));
    assert!(!root.contains_key("history_dashboard"));

    let parsed: AppConfig = toml::from_str(&toml_str).unwrap();
    assert_eq!(parsed.focus_secs, default_focus_secs());
    assert_eq!(parsed.short_break_secs, default_short_break_secs());
    assert_eq!(parsed.long_break_secs, default_long_break_secs());
    assert_eq!(parsed.long_break_interval, default_long_break_interval());
    assert!(parsed.blocked_sites.is_empty());
    assert_eq!(parsed.blocklist_profiles, original.blocklist_profiles);
    assert_eq!(
        parsed.selected_blocklist_profile,
        original.selected_blocklist_profile
    );
    assert_eq!(parsed.selected_profile, original.selected_profile);
    assert_eq!(parsed.custom_profile, original.custom_profile);
    assert_eq!(parsed.selected_theme_preset, original.selected_theme_preset);
    assert_eq!(parsed.notifications, NotificationConfig::default());
    assert_eq!(parsed.auto_start, AutoStartConfig::default());
    assert_eq!(
        parsed.recurring_schedule,
        RecurringScheduleConfig::default()
    );
    assert_eq!(parsed.schedule_runtime, original.schedule_runtime);
    assert_eq!(parsed.profile_automation, original.profile_automation);
    assert!(!parsed.strict_mode);
    assert_eq!(parsed.daily_goal, original.daily_goal);
    assert_eq!(parsed.weekly_goal, WeeklyGoalConfig::default());
    assert_eq!(parsed.monthly_goal, MonthlyGoalConfig::default());
    assert_eq!(parsed.goal_carry_over.daily, original.goal_carry_over.daily);
    assert!(!parsed.goal_carry_over.weekly);
    assert!(!parsed.goal_carry_over.monthly);
    assert_eq!(parsed.stats_retention, original.stats_retention);
    assert_eq!(parsed.history_dashboard, HistoryDashboardConfig::default());
    assert_eq!(parsed.feature_flags, original.feature_flags);
    assert_eq!(parsed.shortcuts, original.shortcuts);
}

#[test]
fn missing_fields_fall_back_to_defaults() {
    let partial = "focus_secs = 1500\n";
    let cfg: AppConfig = toml::from_str(partial).unwrap();
    assert_eq!(cfg.focus_secs, 1500);
    assert_eq!(cfg.short_break_secs, 5 * 60);
    assert_eq!(cfg.long_break_secs, 15 * 60);
    assert_eq!(cfg.long_break_interval, 4);
    assert_eq!(cfg.selected_profile, ProfileId::Custom);
    assert!(cfg.custom_profile.is_none());
    assert_eq!(cfg.selected_theme_preset, ThemePreset::Classic);
    assert!(cfg.blocked_sites.is_empty());
    assert!(cfg.blocklist_profiles.is_empty());
    assert_eq!(cfg.selected_blocklist_profile, "Default");
    assert_eq!(cfg.notifications, NotificationConfig::default());
    assert_eq!(cfg.auto_start, AutoStartConfig::default());
    assert_eq!(cfg.recurring_schedule, RecurringScheduleConfig::default());
    assert!(cfg.profile_automation.is_none());
    assert!(!cfg.strict_mode);
    assert_eq!(cfg.daily_goal, DailyGoalConfig::default());
    assert_eq!(cfg.weekly_goal, WeeklyGoalConfig::default());
    assert_eq!(cfg.monthly_goal, MonthlyGoalConfig::default());
    assert_eq!(cfg.goal_carry_over, GoalCarryOverConfig::default());
    assert_eq!(cfg.history_dashboard, HistoryDashboardConfig::default());
    assert_eq!(cfg.feature_flags, FeatureFlagsConfig::default());
    assert_eq!(cfg.shortcuts, ShortcutConfig::default());
}

#[test]
fn legacy_break_template_fields_are_ignored_and_not_reserialized() {
    let legacy = r#"
selected_profile = "custom"
selected_break_template = "Deep Work"

[custom_profile]
focus_secs = 1800
short_break_secs = 420
long_break_secs = 900
long_break_interval = 3

[[break_templates]]
name = "Classic"
short_break_secs = 300
long_break_secs = 900
long_break_interval = 4

[[break_templates]]
name = "Deep Work"
short_break_secs = 600
long_break_secs = 1800
long_break_interval = 3
"#;
    let cfg: AppConfig = toml::from_str(legacy).unwrap();
    let normalized = cfg.normalize();
    let custom = normalized.effective_custom_profile();
    assert_eq!(custom.focus_secs, 1800);
    assert_eq!(custom.short_break_secs, 420);
    assert_eq!(custom.long_break_secs, 900);
    assert_eq!(custom.long_break_interval, 3);

    let serialized = toml::to_string_pretty(&normalized).unwrap();
    assert!(!serialized.contains("selected_break_template"));
    assert!(!serialized.contains("[[break_templates]]"));
}

#[test]
fn normalize_history_dashboard_ignores_legacy_custom_card_order() {
    let cfg = AppConfig {
        history_dashboard: HistoryDashboardConfig {
            card_order: vec![
                HistoryKpiCardId::Unknown,
                HistoryKpiCardId::GoalStreak,
                HistoryKpiCardId::GoalStreak,
                HistoryKpiCardId::FocusScore,
            ],
            pinned_cards: vec![HistoryKpiCardId::GoalStreak],
        },
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(cfg.history_dashboard, HistoryDashboardConfig::default());
}

#[test]
fn normalize_history_dashboard_ignores_legacy_custom_pins() {
    let cfg = AppConfig {
        history_dashboard: HistoryDashboardConfig {
            card_order: vec![HistoryKpiCardId::FocusScore, HistoryKpiCardId::Retention],
            pinned_cards: vec![
                HistoryKpiCardId::Unknown,
                HistoryKpiCardId::Retention,
                HistoryKpiCardId::Retention,
                HistoryKpiCardId::LastInterruption,
            ],
        },
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(cfg.history_dashboard, HistoryDashboardConfig::default());
}

#[test]
fn shortcut_config_normalizes_invalid_tokens_to_defaults() {
    let cfg = AppConfig {
        shortcuts: ShortcutConfig {
            quit: "qq".to_string(),
            timer_toggle_pause: "    ".to_string(),
            site_add: "Add".to_string(),
            planner_select_recent: "".to_string(),
            navigate_up: "north".to_string(),
            confirm: "accept".to_string(),
            ..ShortcutConfig::default()
        },
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(cfg.shortcuts.quit, "q");
    assert_eq!(cfg.shortcuts.timer_toggle_pause, "space");
    assert_eq!(cfg.shortcuts.site_add, "a");
    assert_eq!(cfg.shortcuts.planner_select_recent, "r");
    assert_eq!(cfg.shortcuts.navigate_up, "up");
    assert_eq!(cfg.shortcuts.confirm, "enter");
}

#[test]
fn shortcut_config_normalizes_space_and_uppercase_tokens() {
    let cfg: AppConfig = toml::from_str(
        r#"
[shortcuts]
timer_toggle_pause = "SPACE"
open_stats_history = "Y"
select_previous_blocklist_profile = "["
navigate_up = "UP"
confirm = "ENTER"
cancel = "ESCAPE"
delete = "DEL"
"#,
    )
    .unwrap();
    let normalized = cfg.normalize();

    assert_eq!(normalized.shortcuts.timer_toggle_pause, "space");
    assert_eq!(normalized.shortcuts.open_stats_history, "y");
    assert_eq!(normalized.shortcuts.select_previous_blocklist_profile, "[");
    assert_eq!(normalized.shortcuts.navigate_up, "up");
    assert_eq!(normalized.shortcuts.confirm, "enter");
    assert_eq!(normalized.shortcuts.cancel, "esc");
    assert_eq!(normalized.shortcuts.delete, "delete");
}

#[test]
fn theme_preset_deserializes_aliases_and_falls_back_to_classic() {
    let alias_cfg: AppConfig = toml::from_str("selected_theme_preset = \"colorblind-friendly\"")
        .expect("theme alias should parse");
    assert_eq!(
        alias_cfg.selected_theme_preset,
        ThemePreset::DeuteranopiaFriendly
    );

    let unknown_cfg: AppConfig =
        toml::from_str("selected_theme_preset = \"unknown\"").expect("config should parse");
    assert_eq!(unknown_cfg.selected_theme_preset, ThemePreset::Classic);
}

#[test]
fn partial_auto_start_block_uses_defaults_for_missing_fields() {
    let partial = r#"
[auto_start]
focus_to_break = true
"#;
    let cfg: AppConfig = toml::from_str(partial).unwrap();

    assert!(cfg.auto_start.focus_to_break);
    assert!(!cfg.auto_start.break_to_focus);
}

#[test]
fn partial_recurring_schedule_window_uses_defaults_for_missing_fields() {
    let partial = r#"
[recurring_schedule]
[[recurring_schedule.windows]]
start = "08:30"
"#;
    let cfg: AppConfig = toml::from_str(partial).unwrap();

    assert_eq!(cfg.recurring_schedule.windows.len(), 1);
    let window = &cfg.recurring_schedule.windows[0];
    assert_eq!(window.start, "08:30");
    assert_eq!(window.end, default_schedule_window_end());
    assert_eq!(window.days, default_schedule_window_days());
}

#[test]
fn recurring_schedule_rejects_one_time_windows_config() {
    let partial = r#"
[recurring_schedule]
[[recurring_schedule.one_time_windows]]
date = "2026-04-27"
start = "09:00"
end = "10:00"
"#;

    let error = toml::from_str::<AppConfig>(partial).unwrap_err();

    assert!(error.to_string().contains("one_time_windows"));
}

#[test]
fn normalize_drops_recurring_windows_with_invalid_time_ranges() {
    let cfg = AppConfig {
        recurring_schedule: RecurringScheduleConfig {
            windows: vec![
                RecurringFocusWindowConfig {
                    days: vec!["mon".to_string()],
                    start: "09:00".to_string(),
                    end: "11:00".to_string(),
                },
                RecurringFocusWindowConfig {
                    days: vec!["tue".to_string()],
                    start: "18:00".to_string(),
                    end: "09:00".to_string(),
                },
            ],
        },
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(cfg.recurring_schedule.windows.len(), 1);
    assert_eq!(cfg.recurring_schedule.windows[0].start, "09:00");
    assert_eq!(cfg.recurring_schedule.windows[0].end, "11:00");
}

#[test]
fn normalize_clamps_schedule_runtime_time_step_to_safe_bounds() {
    let cfg = AppConfig {
        schedule_runtime: ScheduleRuntimeConfig {
            time_step_minutes: 0,
        },
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(
        cfg.schedule_runtime,
        ScheduleRuntimeConfig {
            time_step_minutes: 1,
        }
    );

    let cfg = AppConfig {
        schedule_runtime: ScheduleRuntimeConfig {
            time_step_minutes: 200,
        },
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(
        cfg.schedule_runtime,
        ScheduleRuntimeConfig {
            time_step_minutes: 60,
        }
    );
}

#[test]
fn unknown_selected_profile_falls_back_to_custom_without_dropping_config() {
    let config = r#"
focus_secs = 1500
short_break_secs = 360
long_break_secs = 900
long_break_interval = 3
selected_profile = "unknown"
blocked_sites = ["reddit.com", "youtube.com"]
"#;
    let parsed: AppConfig = toml::from_str(config).unwrap();
    assert_eq!(parsed.selected_profile, ProfileId::Custom);
    assert_eq!(parsed.focus_secs, 1500);
    assert_eq!(parsed.short_break_secs, 360);
    assert_eq!(parsed.long_break_secs, 900);
    assert_eq!(parsed.long_break_interval, 3);
    assert_eq!(parsed.blocked_sites, vec!["reddit.com", "youtube.com"]);
    assert_eq!(parsed.auto_start, AutoStartConfig::default());
    assert_eq!(
        parsed.recurring_schedule,
        RecurringScheduleConfig::default()
    );
    assert_eq!(parsed.daily_goal, DailyGoalConfig::default());
    assert_eq!(parsed.weekly_goal, WeeklyGoalConfig::default());
    assert_eq!(parsed.monthly_goal, MonthlyGoalConfig::default());
    assert_eq!(parsed.goal_carry_over, GoalCarryOverConfig::default());
}

#[test]
fn legacy_duration_fields_seed_custom_profile() {
    let legacy = r#"
focus_secs = 1800
short_break_secs = 360
long_break_secs = 900
long_break_interval = 3
"#;
    let cfg: AppConfig = toml::from_str(legacy).unwrap();
    let custom = cfg.effective_custom_profile();
    assert_eq!(custom.focus_secs, 1800);
    assert_eq!(custom.short_break_secs, 360);
    assert_eq!(custom.long_break_secs, 900);
    assert_eq!(custom.long_break_interval, 3);
}

/// Verifies explicit custom profile settings override built-in defaults.
#[test]
fn effective_custom_profile_uses_explicit_profile_when_present() {
    let cfg = AppConfig {
        focus_secs: 25 * 60,
        short_break_secs: 5 * 60,
        long_break_secs: 15 * 60,
        long_break_interval: 4,
        blocked_sites: Vec::new(),
        blocklist_profiles: vec![BlocklistProfileConfig::default()],
        selected_blocklist_profile: "Default".to_string(),
        selected_profile: ProfileId::Custom,
        custom_profile: Some(CustomProfileConfig {
            focus_secs: 40 * 60,
            short_break_secs: 8 * 60,
            long_break_secs: 16 * 60,
            long_break_interval: 2,
        }),
        selected_theme_preset: ThemePreset::Classic,
        notifications: NotificationConfig::default(),
        auto_start: AutoStartConfig::default(),
        recurring_schedule: RecurringScheduleConfig::default(),
        schedule_runtime: ScheduleRuntimeConfig::default(),
        profile_automation: None,
        strict_mode: false,
        daily_goal: DailyGoalConfig::default(),
        weekly_goal: WeeklyGoalConfig::default(),
        monthly_goal: MonthlyGoalConfig::default(),
        goal_carry_over: GoalCarryOverConfig::default(),
        stats_retention: StatsRetentionConfig::default(),
        history_dashboard: HistoryDashboardConfig::default(),
        feature_flags: FeatureFlagsConfig::default(),
        shortcuts: ShortcutConfig::default(),
    };
    let custom = cfg.effective_custom_profile();
    assert_eq!(custom.focus_secs, 40 * 60);
    assert_eq!(custom.short_break_secs, 8 * 60);
    assert_eq!(custom.long_break_secs, 16 * 60);
    assert_eq!(custom.long_break_interval, 2);
}

#[test]
fn load_returns_default_when_config_file_is_corrupt() {
    let temp_base = unique_temp_base("corrupt");
    let app_dir = temp_base.join("focustime");
    fs::create_dir_all(&app_dir).unwrap();
    fs::write(app_dir.join("config.toml"), "this is not valid toml !!!").unwrap();

    let cfg = AppConfig::load_with_env(|key| {
        if key == CONFIG_DIR_ENV {
            Some(temp_base.clone().into_os_string())
        } else {
            None
        }
    });
    let _ = fs::remove_dir_all(&temp_base);

    assert_eq!(cfg.focus_secs, crate::timer::DEFAULT_FOCUS_SECS);
    assert_eq!(cfg.short_break_secs, crate::timer::DEFAULT_SHORT_BREAK_SECS);
    assert_eq!(cfg.long_break_secs, crate::timer::DEFAULT_LONG_BREAK_SECS);
    assert_eq!(
        cfg.long_break_interval,
        crate::timer::DEFAULT_LONG_BREAK_INTERVAL
    );
    assert_eq!(cfg.selected_profile, ProfileId::Custom);
    assert!(cfg.custom_profile.is_none());
    assert!(cfg.blocked_sites.is_empty());
    assert_eq!(cfg.selected_blocklist_profile, "Default");
    assert!(cfg.blocklist_profiles.is_empty());
    assert_eq!(cfg.auto_start, AutoStartConfig::default());
    assert_eq!(cfg.recurring_schedule, RecurringScheduleConfig::default());
    assert!(cfg.profile_automation.is_none());
    assert!(!cfg.strict_mode);
    assert_eq!(cfg.daily_goal, DailyGoalConfig::default());
    assert_eq!(cfg.weekly_goal, WeeklyGoalConfig::default());
    assert_eq!(cfg.monthly_goal, MonthlyGoalConfig::default());
    assert_eq!(cfg.goal_carry_over, GoalCarryOverConfig::default());
    assert_eq!(cfg.history_dashboard, HistoryDashboardConfig::default());
}

#[test]
fn load_with_env_defaults_legacy_one_time_schedule_windows_config() {
    let temp_base = unique_temp_base("legacy-one-time-schedule");
    let app_dir = temp_base.join("focustime");
    fs::create_dir_all(&app_dir).unwrap();
    fs::write(
        app_dir.join("config.toml"),
        r#"
focus_secs = 1800

[recurring_schedule]
[[recurring_schedule.one_time_windows]]
date = "2026-05-02"
start = "14:00"
end = "16:00"
"#,
    )
    .unwrap();

    let cfg = AppConfig::load_with_env(|key| {
        if key == CONFIG_DIR_ENV {
            Some(temp_base.clone().into_os_string())
        } else {
            None
        }
    });
    let _ = fs::remove_dir_all(&temp_base);

    assert_eq!(cfg.focus_secs, crate::timer::DEFAULT_FOCUS_SECS);
    assert_eq!(cfg.recurring_schedule, RecurringScheduleConfig::default());
}

#[test]
fn load_with_env_migrates_legacy_config_without_schema_version() {
    let temp_base = unique_temp_base("legacy-no-version");
    let app_dir = temp_base.join("focustime");
    fs::create_dir_all(&app_dir).unwrap();
    fs::write(
        app_dir.join("config.toml"),
        "focus_secs = 1800\nshort_break_secs = 360\n",
    )
    .unwrap();

    let cfg = AppConfig::load_with_env(|key| {
        if key == CONFIG_DIR_ENV {
            Some(temp_base.clone().into_os_string())
        } else {
            None
        }
    });
    let _ = fs::remove_dir_all(&temp_base);

    assert_eq!(cfg.focus_secs, 1800);
    assert_eq!(cfg.short_break_secs, 360);
}

#[test]
fn load_with_env_reports_deprecation_warnings_for_legacy_fields() {
    let temp_base = unique_temp_base("legacy-deprecation-warnings");
    let app_dir = temp_base.join("focustime");
    fs::create_dir_all(&app_dir).unwrap();
    fs::write(
        app_dir.join("config.toml"),
        r#"
focus_secs = 1800
short_break_secs = 360
long_break_secs = 900
long_break_interval = 3
strict_mode = true
blocked_sites = ["youtube.com", "reddit.com"]
"#,
    )
    .unwrap();

    let (_, warnings) = AppConfig::load_with_env_and_deprecation_warnings(|key| {
        if key == CONFIG_DIR_ENV {
            Some(temp_base.clone().into_os_string())
        } else {
            None
        }
    });
    let _ = fs::remove_dir_all(&temp_base);

    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("Deprecated top-level timer fields"))
    );
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("Deprecated top-level automation fields"))
    );
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("Deprecated `blocked_sites` is in use"))
    );
}

#[test]
fn load_with_env_avoids_duration_deprecation_warning_when_custom_profile_exists() {
    let temp_base = unique_temp_base("custom-profile-no-duration-warning");
    let app_dir = temp_base.join("focustime");
    fs::create_dir_all(&app_dir).unwrap();
    fs::write(
        app_dir.join("config.toml"),
        r#"
focus_secs = 1800
short_break_secs = 360
long_break_secs = 900
long_break_interval = 3

[custom_profile]
focus_secs = 1800
short_break_secs = 360
long_break_secs = 900
long_break_interval = 3
"#,
    )
    .unwrap();

    let (_, warnings) = AppConfig::load_with_env_and_deprecation_warnings(|key| {
        if key == CONFIG_DIR_ENV {
            Some(temp_base.clone().into_os_string())
        } else {
            None
        }
    });
    let _ = fs::remove_dir_all(&temp_base);

    assert!(
        !warnings
            .iter()
            .any(|warning| warning.contains("Deprecated top-level timer fields"))
    );
}

#[test]
fn load_with_env_defaults_do_not_emit_deprecation_warnings() {
    let temp_base = unique_temp_base("default-no-deprecation-warning");
    let app_dir = temp_base.join("focustime");
    fs::create_dir_all(&app_dir).unwrap();
    fs::write(app_dir.join("config.toml"), "").unwrap();

    let (_, warnings) = AppConfig::load_with_env_and_deprecation_warnings(|key| {
        if key == CONFIG_DIR_ENV {
            Some(temp_base.clone().into_os_string())
        } else {
            None
        }
    });
    let _ = fs::remove_dir_all(&temp_base);

    assert!(warnings.is_empty());
}

#[test]
fn load_with_env_ignores_legacy_calendar_sync_without_warning() {
    let temp_base = unique_temp_base("legacy-calendar-sync-ignored");
    let app_dir = temp_base.join("focustime");
    fs::create_dir_all(&app_dir).unwrap();
    fs::write(
        app_dir.join("config.toml"),
        r#"
[calendar_sync]
enabled = true

[[calendar_sync.sources]]
name = "Work"
provider = "ics"
url = "https://example.com/work.ics"
"#,
    )
    .unwrap();

    let (_, warnings) = AppConfig::load_with_env_and_deprecation_warnings(|key| {
        if key == CONFIG_DIR_ENV {
            Some(temp_base.clone().into_os_string())
        } else {
            None
        }
    });
    let _ = fs::remove_dir_all(&temp_base);

    assert!(warnings.is_empty());
}

#[test]
fn load_with_env_reports_automation_deprecation_when_profile_automation_is_partial() {
    let temp_base = unique_temp_base("legacy-automation-partial-profile");
    let app_dir = temp_base.join("focustime");
    fs::create_dir_all(&app_dir).unwrap();
    fs::write(
        app_dir.join("config.toml"),
        r#"
[notifications]
enabled = true
sound = true

[profile_automation.deep_work]
strict_mode = true
"#,
    )
    .unwrap();

    let (_, warnings) = AppConfig::load_with_env_and_deprecation_warnings(|key| {
        if key == CONFIG_DIR_ENV {
            Some(temp_base.clone().into_os_string())
        } else {
            None
        }
    });
    let _ = fs::remove_dir_all(&temp_base);

    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("Deprecated top-level automation fields"))
    );
}

#[test]
fn load_with_env_skips_automation_deprecation_when_all_profiles_are_configured() {
    let temp_base = unique_temp_base("legacy-automation-all-profiles");
    let app_dir = temp_base.join("focustime");
    fs::create_dir_all(&app_dir).unwrap();
    fs::write(
        app_dir.join("config.toml"),
        r#"
[notifications]
enabled = true
sound = true

[profile_automation.classic]
strict_mode = false

[profile_automation.deep_work]
strict_mode = true

[profile_automation.custom]
strict_mode = false
"#,
    )
    .unwrap();

    let (_, warnings) = AppConfig::load_with_env_and_deprecation_warnings(|key| {
        if key == CONFIG_DIR_ENV {
            Some(temp_base.clone().into_os_string())
        } else {
            None
        }
    });
    let _ = fs::remove_dir_all(&temp_base);

    assert!(
        !warnings
            .iter()
            .any(|warning| warning.contains("Deprecated top-level automation fields"))
    );
}

#[test]
fn load_with_env_migrates_explicit_legacy_schema_version() {
    let temp_base = unique_temp_base("legacy-version-zero");
    let app_dir = temp_base.join("focustime");
    fs::create_dir_all(&app_dir).unwrap();
    fs::write(
        app_dir.join("config.toml"),
        "schema_version = 0\nfocus_secs = 1950\n",
    )
    .unwrap();

    let cfg = AppConfig::load_with_env(|key| {
        if key == CONFIG_DIR_ENV {
            Some(temp_base.clone().into_os_string())
        } else {
            None
        }
    });
    let _ = fs::remove_dir_all(&temp_base);

    assert_eq!(cfg.focus_secs, 1950);
}

#[test]
fn migrate_config_toml_v1_to_v2_maps_profile_ids_and_profile_automation_keys() {
    let v1: toml::Value = toml::from_str(
        r#"
schema_version = 1
selected_profile = "deep-work"
selected_session_template = "Template"

[[session_templates]]
name = "Template"
task_label = "Task"
profile = "custom"
blocklist_profile = "Default"

[[weekday_profile_rules]]
day = "mon"
profile = "classic"
blocklist_profile = "Default"

[[automation_triggers]]
trigger = { type = "focus_completed" }
action = { type = "apply_defaults", profile = "deep_work", blocklist_profile = "Default" }

[profile_automation.classic]
strict_mode = false

[profile_automation.deep_work]
strict_mode = true

[profile_automation.custom]
strict_mode = false
"#,
    )
    .unwrap();
    let migrated = migrate_config_toml_to_current(v1).expect("v1 payload should migrate to v2");
    let root = migrated.as_table().expect("root should be a table");

    assert_eq!(
        root.get("schema_version").and_then(toml::Value::as_integer),
        Some(i64::from(CURRENT_CONFIG_SCHEMA_VERSION))
    );
    assert_eq!(
        root.get("selected_profile").and_then(toml::Value::as_str),
        Some("standard")
    );

    let profile_automation = root
        .get("profile_automation")
        .and_then(toml::Value::as_table)
        .expect("profile_automation should be a table");
    assert!(profile_automation.get("basic").is_some());
    assert!(profile_automation.get("standard").is_some());
    assert!(profile_automation.get("advanced").is_some());
    assert!(profile_automation.get("classic").is_none());
    assert!(profile_automation.get("deep_work").is_none());
    assert!(profile_automation.get("custom").is_none());

    assert!(root.get("session_templates").is_none());
    assert!(root.get("selected_session_template").is_none());
    assert!(root.get("weekday_profile_rules").is_none());

    assert!(root.get("automation_triggers").is_none());
}

#[test]
fn migrate_config_toml_removes_session_template_persistence() {
    let original: toml::Value = toml::from_str(
        r#"
schema_version = 2
selected_session_template = "Morning"

[[session_templates]]
name = "Morning"
task_label = "Docs"
profile = "basic"
blocklist_profile = "Default"
"#,
    )
    .unwrap();

    let (migrated, _, steps) = migrate_config_toml_to_current_detailed(original).unwrap();
    let root = migrated.as_table().expect("root should be a table");
    assert!(root.get("session_templates").is_none());
    assert!(root.get("selected_session_template").is_none());
    assert!(steps.iter().any(|step| {
        step.summary
            == "Remove retired session template persistence; use task, profile, schedule, and blocklist settings directly."
    }));
}

#[test]
fn migrate_deprecated_schedule_shims_removes_weekday_rules_and_automation_triggers() {
    let v2: toml::Value = toml::from_str(
        r#"
schema_version = 2
weekday_profile_rules = []

[[automation_triggers]]
trigger = { type = "time", days = ["mon"], at = "00:00" }
action = { type = "apply_defaults", profile = "standard", blocklist_profile = "Default" }
"#,
    )
    .unwrap();
    let migrated = migrate_config_toml_to_current(v2).expect("v2 payload should migrate");
    let root = migrated.as_table().expect("root should be a table");

    assert!(root.get("weekday_profile_rules").is_none());
    assert!(root.get("automation_triggers").is_none());
}

#[test]
fn migrate_config_toml_v1_to_v2_merges_legacy_profile_automation_into_existing_preset_key() {
    let v1: toml::Value = toml::from_str(
        r#"
schema_version = 1

[profile_automation.standard]
strict_mode = true
[profile_automation.standard.notifications]
enabled = false

[profile_automation.deep_work]
strict_mode = false
[profile_automation.deep_work.notifications]
enabled = true
sound = true
[profile_automation.deep_work.auto_start]
focus_to_break = true
break_to_focus = true
"#,
    )
    .unwrap();

    let migrated = migrate_config_toml_to_current(v1).expect("v1 payload should migrate to v2");
    let profile_automation = migrated
        .as_table()
        .and_then(|root| root.get("profile_automation"))
        .and_then(toml::Value::as_table)
        .expect("profile_automation should be a table");

    let standard = profile_automation
        .get("standard")
        .and_then(toml::Value::as_table)
        .expect("standard profile automation should exist");
    let notifications = standard
        .get("notifications")
        .and_then(toml::Value::as_table)
        .expect("notifications should exist");
    let auto_start = standard
        .get("auto_start")
        .and_then(toml::Value::as_table)
        .expect("auto_start should exist");

    assert_eq!(
        standard.get("strict_mode").and_then(toml::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        notifications.get("enabled").and_then(toml::Value::as_bool),
        Some(false)
    );
    assert_eq!(
        notifications.get("sound").and_then(toml::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        auto_start
            .get("focus_to_break")
            .and_then(toml::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        auto_start
            .get("break_to_focus")
            .and_then(toml::Value::as_bool),
        Some(true)
    );
    assert!(profile_automation.get("deep_work").is_none());
}

#[test]
fn migrate_config_toml_current_schema_canonicalizes_legacy_profile_aliases() {
    let v2_with_legacy_alias: toml::Value = toml::from_str(
        r#"
schema_version = 2
selected_profile = "deep_work"
"#,
    )
    .unwrap();

    let migrated = migrate_config_toml_to_current(v2_with_legacy_alias)
        .expect("current-schema payload should still canonicalize aliases");
    let root = migrated.as_table().expect("root should be a table");
    assert_eq!(
        root.get("selected_profile").and_then(toml::Value::as_str),
        Some("standard")
    );
}

#[test]
fn migrate_config_toml_collapses_profile_collisions_without_losing_blocks() {
    let original: toml::Value = toml::from_str(
        r#"
schema_version = 2
selected_blocklist_profile = "Work"

[[blocklist_profiles]]
name = "Work"
sites = ["a.com"]
allowlist_sites = ["b.com"]

[[blocklist_profiles]]
name = "Personal"
sites = ["b.com"]
allowlist_sites = []
"#,
    )
    .unwrap();

    let migrated = migrate_config_toml_to_current(original)
        .expect("current-schema payload should still collapse blocklist profiles");
    let root = migrated.as_table().expect("root should be a table");
    let profiles = root
        .get("blocklist_profiles")
        .and_then(toml::Value::as_array)
        .unwrap();
    let profile = profiles[0].as_table().unwrap();

    assert_eq!(
        profile.get("sites").unwrap().as_array().unwrap(),
        &vec![
            toml::Value::String("a.com".to_string()),
            toml::Value::String("b.com".to_string()),
        ]
    );
    assert_eq!(
        profile.get("allowlist_sites").unwrap().as_array().unwrap(),
        &Vec::<toml::Value>::new()
    );
}

#[test]
fn migrate_config_toml_preserves_same_profile_allowlist_overrides() {
    let original: toml::Value = toml::from_str(
        r#"
schema_version = 2
selected_blocklist_profile = "Work"

[[blocklist_profiles]]
name = "Work"
sites = ["a.com"]
allowlist_sites = ["b.com"]

[[blocklist_profiles]]
name = "Personal"
sites = ["b.com"]
allowlist_sites = ["b.com"]
"#,
    )
    .unwrap();

    let migrated = migrate_config_toml_to_current(original)
        .expect("current-schema payload should still collapse blocklist profiles");
    let root = migrated.as_table().expect("root should be a table");
    let profiles = root
        .get("blocklist_profiles")
        .and_then(toml::Value::as_array)
        .unwrap();
    let profile = profiles[0].as_table().unwrap();

    assert_eq!(
        profile.get("sites").unwrap().as_array().unwrap(),
        &vec![
            toml::Value::String("a.com".to_string()),
            toml::Value::String("b.com".to_string()),
        ]
    );
    assert_eq!(
        profile.get("allowlist_sites").unwrap().as_array().unwrap(),
        &vec![toml::Value::String("b.com".to_string())]
    );
}

#[test]
fn migrate_config_toml_legacy_to_v1_sets_intermediate_schema_version_to_one() {
    let legacy: toml::Value = toml::from_str("focus_secs = 1500").unwrap();
    let migrated = migrate_config_toml_legacy_to_v1(legacy).expect("legacy migration should work");
    let root = migrated.as_table().expect("root should be a table");
    assert_eq!(
        root.get("schema_version").and_then(toml::Value::as_integer),
        Some(1)
    );
}

#[test]
fn config_doctor_reports_parse_errors() {
    let temp_base = unique_temp_base("doctor-parse-error");
    let app_dir = temp_base.join("focustime");
    fs::create_dir_all(&app_dir).unwrap();
    let config_path = app_dir.join("config.toml");
    fs::write(&config_path, "this is not valid toml !!!").unwrap();

    let report = run_config_doctor_with_path(Some(config_path.clone()));
    let _ = fs::remove_dir_all(&temp_base);

    assert_eq!(report.action, "config-health");
    assert_eq!(report.status, ConfigHealthStatus::Error);
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "config.toml_parse_error")
    );
}

#[test]
fn config_doctor_reports_schema_and_legacy_profile_findings() {
    let temp_base = unique_temp_base("doctor-schema-legacy-profile");
    let app_dir = temp_base.join("focustime");
    fs::create_dir_all(&app_dir).unwrap();
    let config_path = app_dir.join("config.toml");
    fs::write(
        &config_path,
        r#"
schema_version = 1
selected_profile = "deep_work"
"#,
    )
    .unwrap();

    let report = run_config_doctor_with_path(Some(config_path.clone()));
    let _ = fs::remove_dir_all(&temp_base);

    assert_eq!(report.status, ConfigHealthStatus::Warning);
    assert!(!report.migration_steps.is_empty());
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "config.schema_outdated")
    );
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "config.legacy_profile_token")
    );
}

#[test]
fn config_doctor_stops_after_newer_schema_warning() {
    let temp_base = unique_temp_base("doctor-newer-schema");
    let app_dir = temp_base.join("focustime");
    fs::create_dir_all(&app_dir).unwrap();
    let config_path = app_dir.join("config.toml");
    fs::write(
        &config_path,
        r#"
schema_version = 99
selected_profile = "standard"
future_only = "ignored"
"#,
    )
    .unwrap();

    let report = run_config_doctor_with_path(Some(config_path.clone()));
    let _ = fs::remove_dir_all(&temp_base);

    assert_eq!(report.status, ConfigHealthStatus::Warning);
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "config.schema_newer_than_supported")
    );
    assert!(
        report
            .findings
            .iter()
            .all(|finding| finding.code != "config.deserialize_failed")
    );
}

#[test]
fn config_doctor_detects_legacy_automation_fields_before_normalization() {
    let temp_base = unique_temp_base("doctor-legacy-automation-pre-normalize");
    let app_dir = temp_base.join("focustime");
    fs::create_dir_all(&app_dir).unwrap();
    let config_path = app_dir.join("config.toml");
    fs::write(
        &config_path,
        r#"
[notifications]
enabled = true
sound = true
"#,
    )
    .unwrap();

    let report = run_config_doctor_with_path(Some(config_path.clone()));
    let _ = fs::remove_dir_all(&temp_base);

    assert!(
        report.findings.iter().any(|finding| {
            finding.code == "config.deprecated_field_in_use"
                && finding
                    .message
                    .contains("Deprecated top-level automation fields")
        }),
        "expected deprecation warning for top-level automation fields"
    );
}

#[test]
fn config_doctor_omits_retired_blocklist_category_warnings() {
    let temp_base = unique_temp_base("doctor-blocklist-categories");
    let app_dir = temp_base.join("focustime");
    fs::create_dir_all(&app_dir).unwrap();
    let config_path = app_dir.join("config.toml");
    fs::write(
        &config_path,
        r#"
schema_version = 2
selected_blocklist_profile = "Work"

[[blocklist_profiles]]
name = "Work"
selected_category = "Social"

[[blocklist_profiles.categories]]
name = "General"
sites = ["docs.rs"]
allowlist_sites = []

[[blocklist_profiles.categories]]
name = "Social"
sites = ["youtube.com"]
allowlist_sites = ["reddit.com"]
"#,
    )
    .unwrap();

    let report = run_config_doctor_with_path(Some(config_path.clone()));
    let _ = fs::remove_dir_all(&temp_base);

    assert!(
        report
            .findings
            .iter()
            .all(|finding| finding.code != "config.blocklist_category_migration"),
        "retired blocklist category paths should not produce doctor warnings: {:#?}",
        report.findings
    );
    assert!(report.migration_steps.iter().any(|step| {
        step.summary == "Flatten deprecated blocklist category rules into profile-level site lists."
    }));
}

#[test]
fn config_migration_assistant_preview_reports_changes_without_writing() {
    let temp_base = unique_temp_base("migration-preview");
    let app_dir = temp_base.join("focustime");
    fs::create_dir_all(&app_dir).unwrap();
    let config_path = app_dir.join("config.toml");
    let original = r#"
schema_version = 1
selected_profile = "deep_work"
"#;
    fs::write(&config_path, original).unwrap();

    let report = run_config_migration_assistant_with_path(false, Some(config_path.clone()));
    let persisted = fs::read_to_string(&config_path).unwrap();
    let _ = fs::remove_dir_all(&temp_base);

    assert_eq!(report.action, "config-migration-guidance");
    assert!(!report.applied);
    assert!(report.changed);
    assert!(report.backup_path.is_none());
    assert_eq!(persisted, original);
}

#[test]
fn config_migration_assistant_apply_writes_migrated_config_and_backup() {
    let temp_base = unique_temp_base("migration-apply");
    let app_dir = temp_base.join("focustime");
    fs::create_dir_all(&app_dir).unwrap();
    let config_path = app_dir.join("config.toml");
    let original = r#"
schema_version = 1
selected_profile = "deep_work"
"#;
    fs::write(&config_path, original).unwrap();

    let report = run_config_migration_assistant_with_path(true, Some(config_path.clone()));
    let migrated = fs::read_to_string(&config_path).unwrap();
    let backup_path = report
        .backup_path
        .clone()
        .expect("backup path should be recorded when apply succeeds");
    let backup = fs::read_to_string(&backup_path).unwrap();
    let _ = fs::remove_dir_all(&temp_base);

    assert_eq!(report.action, "config-migration-apply");
    assert!(report.applied);
    assert!(report.changed);
    assert!(migrated.contains("schema_version = 2"));
    assert!(migrated.contains("selected_profile = \"standard\""));
    assert_eq!(backup, original);
}

#[test]
fn load_with_env_leniently_parses_newer_schema_version() {
    let temp_base = unique_temp_base("future-version");
    let app_dir = temp_base.join("focustime");
    fs::create_dir_all(&app_dir).unwrap();
    fs::write(
        app_dir.join("config.toml"),
        "schema_version = 99\nfocus_secs = 2100\nfuture_only = \"ignored\"\n",
    )
    .unwrap();

    let cfg = AppConfig::load_with_env(|key| {
        if key == CONFIG_DIR_ENV {
            Some(temp_base.clone().into_os_string())
        } else {
            None
        }
    });
    let _ = fs::remove_dir_all(&temp_base);

    assert_eq!(cfg.focus_secs, 2100);
}

#[test]
fn save_with_env_writes_current_schema_version() {
    let temp_base = unique_temp_base("save-schema-version");
    let cfg = AppConfig {
        focus_secs: 2100,
        ..AppConfig::default()
    };
    cfg.save_with_env(|key| {
        if key == CONFIG_DIR_ENV {
            Some(temp_base.clone().into_os_string())
        } else {
            None
        }
    })
    .unwrap();

    let app_dir = temp_base.join("focustime");
    let saved = fs::read_to_string(app_dir.join("config.toml")).unwrap();
    let saved_toml: toml::Value = toml::from_str(&saved).unwrap();
    let _ = fs::remove_dir_all(&temp_base);

    assert_eq!(
        saved_toml
            .get("schema_version")
            .and_then(toml::Value::as_integer),
        Some(i64::from(CURRENT_CONFIG_SCHEMA_VERSION))
    );
    assert_eq!(
        saved_toml
            .get("focus_secs")
            .and_then(toml::Value::as_integer),
        None
    );
}

#[test]
fn save_with_env_omits_legacy_calendar_sync_section() {
    let temp_base = unique_temp_base("save-omits-legacy-calendar-sync");
    let app_dir = temp_base.join("focustime");
    fs::create_dir_all(&app_dir).unwrap();
    fs::write(
        app_dir.join("config.toml"),
        r#"
[calendar_sync]
enabled = true
lookahead_days = 30

[[calendar_sync.sources]]
name = "Work"
provider = "google"
url = "https://example.com/work.ics"
"#,
    )
    .unwrap();

    let cfg = AppConfig::load_with_env(|key| {
        if key == CONFIG_DIR_ENV {
            Some(temp_base.clone().into_os_string())
        } else {
            None
        }
    });
    cfg.save_with_env(|key| {
        if key == CONFIG_DIR_ENV {
            Some(temp_base.clone().into_os_string())
        } else {
            None
        }
    })
    .unwrap();

    let saved = fs::read_to_string(app_dir.join("config.toml")).unwrap();
    let saved_toml: toml::Value = toml::from_str(&saved).unwrap();
    let _ = fs::remove_dir_all(&temp_base);

    assert!(saved_toml.get("calendar_sync").is_none());
    assert!(!saved.contains("[calendar_sync]"));
}

#[test]
fn save_with_env_omits_retired_wakatime_sections() {
    let temp_base = unique_temp_base("save-omits-retired-wakatime");
    let app_dir = temp_base.join("focustime");
    fs::create_dir_all(&app_dir).unwrap();
    fs::write(
        app_dir.join("config.toml"),
        r#"
[wakatime]
api_key = "waka_key"
project = "FocusTime"
language = "Rust"

[wakatime_runtime]
queue_path = "queued-heartbeats.json"
max_queue_len = 256
retry_initial_backoff_secs = 5
retry_max_backoff_secs = 300
"#,
    )
    .unwrap();

    let cfg = AppConfig::load_with_env(|key| {
        if key == CONFIG_DIR_ENV {
            Some(temp_base.clone().into_os_string())
        } else {
            None
        }
    });
    cfg.save_with_env(|key| {
        if key == CONFIG_DIR_ENV {
            Some(temp_base.clone().into_os_string())
        } else {
            None
        }
    })
    .unwrap();

    let saved = fs::read_to_string(app_dir.join("config.toml")).unwrap();
    let saved_toml: toml::Value = toml::from_str(&saved).unwrap();
    let _ = fs::remove_dir_all(&temp_base);

    assert!(saved_toml.get("wakatime").is_none());
    assert!(saved_toml.get("wakatime_runtime").is_none());
    assert!(!saved.contains("[wakatime]"));
    assert!(!saved.contains("[wakatime_runtime]"));
}

#[cfg(not(target_os = "windows"))]
#[test]
fn config_dir_returns_none_when_home_is_blank_and_xdg_is_unset() {
    let dir = config_dir_from_env(|key| match key {
        "XDG_CONFIG_HOME" => None,
        "HOME" => Some(OsString::from("   ")),
        _ => None,
    });
    assert!(dir.is_none());
}

#[cfg(not(target_os = "windows"))]
#[test]
fn config_dir_ignores_relative_xdg_config_home_and_falls_back_to_home() {
    let dir = config_dir_from_env(|key| match key {
        "XDG_CONFIG_HOME" => Some(OsString::from("relative-config")),
        "HOME" => Some(OsString::from("/tmp/home")),
        _ => None,
    });
    assert_eq!(dir, Some(PathBuf::from("/tmp/home/.config")));
}

#[cfg(not(target_os = "windows"))]
#[test]
fn config_dir_uses_absolute_xdg_config_home_when_set() {
    let dir = config_dir_from_env(|key| match key {
        "XDG_CONFIG_HOME" => Some(OsString::from("/tmp/xdg")),
        _ => None,
    });
    assert_eq!(dir, Some(PathBuf::from("/tmp/xdg")));
}

#[cfg(target_os = "windows")]
#[test]
fn config_dir_returns_none_when_appdata_is_blank() {
    let dir = config_dir_from_env(|key| match key {
        "APPDATA" => Some(OsString::from("   ")),
        _ => None,
    });
    assert!(dir.is_none());
}

#[cfg(target_os = "windows")]
#[test]
fn config_dir_returns_none_when_appdata_is_relative() {
    let dir = config_dir_from_env(|key| match key {
        "APPDATA" => Some(OsString::from("AppData\\Roaming")),
        _ => None,
    });
    assert!(dir.is_none());
}

#[cfg(target_os = "windows")]
#[test]
fn config_dir_uses_absolute_appdata_when_set() {
    let dir = config_dir_from_env(|key| match key {
        "APPDATA" => Some(OsString::from(r"C:\Users\test\AppData\Roaming")),
        _ => None,
    });
    assert_eq!(dir, Some(PathBuf::from(r"C:\Users\test\AppData\Roaming")));
}

#[cfg(not(target_os = "windows"))]
#[test]
fn stats_state_dir_uses_absolute_xdg_state_home_when_set() {
    let dir = stats_state_dir_from_env(|key| match key {
        "XDG_STATE_HOME" => Some(OsString::from("/tmp/state")),
        _ => None,
    });
    assert_eq!(dir, Some(PathBuf::from("/tmp/state")));
}

#[cfg(not(target_os = "windows"))]
#[test]
fn stats_state_dir_falls_back_to_absolute_xdg_data_home() {
    let dir = stats_state_dir_from_env(|key| match key {
        "XDG_STATE_HOME" => Some(OsString::from("relative-state")),
        "XDG_DATA_HOME" => Some(OsString::from("/tmp/data")),
        _ => None,
    });
    assert_eq!(dir, Some(PathBuf::from("/tmp/data")));
}

#[cfg(not(target_os = "windows"))]
#[test]
fn stats_state_dir_falls_back_to_home_local_state() {
    let dir = stats_state_dir_from_env(|key| match key {
        "XDG_STATE_HOME" => None,
        "XDG_DATA_HOME" => None,
        "HOME" => Some(OsString::from("/tmp/home")),
        _ => None,
    });
    assert_eq!(dir, Some(PathBuf::from("/tmp/home/.local/state")));
}

#[cfg(target_os = "windows")]
#[test]
fn stats_state_dir_uses_absolute_localappdata_when_set() {
    let dir = stats_state_dir_from_env(|key| match key {
        "LOCALAPPDATA" => Some(OsString::from(r"C:\Users\test\AppData\Local")),
        _ => None,
    });
    assert_eq!(dir, Some(PathBuf::from(r"C:\Users\test\AppData\Local")));
}

#[cfg(target_os = "windows")]
#[test]
fn stats_state_dir_falls_back_to_absolute_appdata_when_localappdata_missing() {
    let dir = stats_state_dir_from_env(|key| match key {
        "LOCALAPPDATA" => None,
        "APPDATA" => Some(OsString::from(r"C:\Users\test\AppData\Roaming")),
        _ => None,
    });
    assert_eq!(dir, Some(PathBuf::from(r"C:\Users\test\AppData\Roaming")));
}

#[cfg(target_os = "windows")]
#[test]
fn stats_state_dir_returns_none_when_all_candidates_are_relative() {
    let dir = stats_state_dir_from_env(|key| match key {
        "LOCALAPPDATA" => Some(OsString::from("AppData\\Local")),
        "APPDATA" => Some(OsString::from("AppData\\Roaming")),
        _ => None,
    });
    assert!(dir.is_none());
}

#[cfg(unix)]
#[test]
fn env_path_from_value_accepts_non_utf8_value() {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let non_utf8 = OsString::from_vec(vec![b'/', b't', b'm', b'p', b'/', 0x80, b'x']);
    let parsed =
        env_path_from_value(non_utf8.clone()).expect("non-UTF-8 env var should be accepted");
    assert_eq!(
        parsed.as_os_str().as_bytes(),
        non_utf8.as_os_str().as_bytes()
    );
}

#[cfg(unix)]
#[test]
fn config_dir_uses_non_utf8_xdg_config_home() {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let non_utf8 = OsString::from_vec(vec![b'/', b't', b'm', b'p', b'/', 0x81, b'y']);
    let dir = config_dir_from_env(|key| match key {
        "XDG_CONFIG_HOME" => Some(non_utf8.clone()),
        _ => None,
    })
    .expect("non-UTF-8 XDG_CONFIG_HOME should be accepted");
    assert_eq!(dir.as_os_str().as_bytes(), non_utf8.as_os_str().as_bytes());
}

#[test]
fn round_trip_empty_blocked_sites() {
    let original = AppConfig::default();
    let toml_str = toml::to_string_pretty(&original).unwrap();
    let parsed: AppConfig = toml::from_str(&toml_str).unwrap();
    assert!(parsed.blocked_sites.is_empty());
    assert_eq!(parsed.selected_blocklist_profile, "Default");
    assert!(parsed.blocklist_profiles.is_empty());
}

#[test]
fn normalize_migrates_legacy_blocked_sites_into_default_profile() {
    let cfg = AppConfig {
        blocked_sites: vec!["reddit.com".to_string(), "youtube.com".to_string()],
        blocklist_profiles: Vec::new(),
        selected_blocklist_profile: String::new(),
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(cfg.blocklist_profiles.len(), 1);
    assert_eq!(cfg.blocklist_profiles[0].name, "Default");
    assert_eq!(
        cfg.blocklist_profiles[0].sites,
        vec!["reddit.com".to_string(), "youtube.com".to_string()]
    );
    assert_eq!(cfg.selected_blocklist_profile, "Default");
    assert_eq!(cfg.blocked_sites, cfg.blocklist_profiles[0].sites);
    assert!(cfg.blocklist_profiles[0].allowlist_sites.is_empty());
}

#[test]
fn normalize_collapses_profiles_and_fixes_selection() {
    let cfg = AppConfig {
        blocklist_profiles: vec![
            BlocklistProfileConfig {
                name: "Work".to_string(),
                sites: vec!["a.com".to_string()],
                allowlist_sites: vec!["b.com".to_string()],
            },
            BlocklistProfileConfig {
                name: "Personal".to_string(),
                sites: vec!["b.com".to_string()],
                allowlist_sites: Vec::new(),
            },
        ],
        selected_blocklist_profile: "missing".to_string(),
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(cfg.blocklist_profiles.len(), 1);
    assert_eq!(cfg.blocklist_profiles[0].name, "Default");
    assert_eq!(
        cfg.blocklist_profiles[0].sites,
        vec!["a.com".to_string(), "b.com".to_string()]
    );
    assert_eq!(
        cfg.blocklist_profiles[0].allowlist_sites,
        Vec::<String>::new()
    );
    assert_eq!(
        effective_blocked_sites_for_profile(&cfg.blocklist_profiles[0]),
        vec!["a.com".to_string(), "b.com".to_string()]
    );
    assert_eq!(cfg.selected_blocklist_profile, "Default");
    assert!(cfg.blocked_sites.is_empty());
}

#[test]
fn normalize_preserves_same_profile_allowlist_overrides() {
    let cfg = AppConfig {
        blocklist_profiles: vec![
            BlocklistProfileConfig {
                name: "Work".to_string(),
                sites: vec!["a.com".to_string()],
                allowlist_sites: vec!["b.com".to_string()],
            },
            BlocklistProfileConfig {
                name: "Personal".to_string(),
                sites: vec!["b.com".to_string()],
                allowlist_sites: vec!["b.com".to_string()],
            },
        ],
        selected_blocklist_profile: "Personal".to_string(),
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(
        cfg.blocklist_profiles[0].sites,
        vec!["a.com".to_string(), "b.com".to_string()]
    );
    assert_eq!(
        cfg.blocklist_profiles[0].allowlist_sites,
        vec!["b.com".to_string()]
    );
    assert_eq!(
        effective_blocked_sites_for_profile(&cfg.blocklist_profiles[0]),
        vec!["a.com".to_string()]
    );
}

#[test]
fn normalize_keeps_legacy_blocked_sites_empty_for_profile_only_config() {
    let cfg = AppConfig {
        blocklist_profiles: vec![BlocklistProfileConfig {
            name: "Work".to_string(),
            sites: vec!["a.com".to_string(), "b.com".to_string()],
            allowlist_sites: vec!["b.com".to_string()],
        }],
        selected_blocklist_profile: "Work".to_string(),
        ..AppConfig::default()
    }
    .normalize();

    assert!(cfg.blocked_sites.is_empty());
}

#[test]
fn normalize_migrates_legacy_automation_into_per_profile_settings() {
    let legacy_schedule = RecurringScheduleConfig {
        windows: vec![RecurringFocusWindowConfig {
            days: vec!["mon".to_string(), "tue".to_string()],
            start: "09:00".to_string(),
            end: "10:30".to_string(),
        }],
    };
    let cfg = AppConfig {
        selected_profile: ProfileId::DeepWork,
        notifications: NotificationConfig {
            enabled: true,
            sound: true,
        },
        auto_start: AutoStartConfig {
            focus_to_break: true,
            break_to_focus: true,
        },
        recurring_schedule: legacy_schedule.clone(),
        profile_automation: None,
        strict_mode: true,
        ..AppConfig::default()
    }
    .normalize();

    let expected = ProfileAutomationConfig::from_legacy(
        NotificationConfig {
            enabled: true,
            sound: true,
        },
        AutoStartConfig {
            focus_to_break: true,
            break_to_focus: true,
        },
        true,
        legacy_schedule,
    );
    assert_eq!(cfg.profile_automation_for(ProfileId::Classic), expected);
    assert_eq!(cfg.profile_automation_for(ProfileId::DeepWork), expected);
    assert_eq!(cfg.profile_automation_for(ProfileId::Custom), expected);
    assert_eq!(cfg.notifications, expected.notifications);
    assert_eq!(cfg.auto_start, expected.auto_start);
    assert_eq!(cfg.strict_mode, expected.strict_mode);
    assert_eq!(cfg.recurring_schedule, expected.recurring_schedule);
}

#[test]
fn normalize_selected_profile_automation_keeps_top_level_legacy_fields() {
    let classic = ProfileAutomationConfig {
        notifications: NotificationConfig {
            enabled: true,
            sound: false,
        },
        auto_start: AutoStartConfig {
            focus_to_break: true,
            break_to_focus: false,
        },
        strict_mode: false,
        recurring_schedule: RecurringScheduleConfig::default(),
    };
    let deep_work = ProfileAutomationConfig {
        notifications: NotificationConfig {
            enabled: false,
            sound: false,
        },
        auto_start: AutoStartConfig {
            focus_to_break: false,
            break_to_focus: true,
        },
        strict_mode: true,
        recurring_schedule: RecurringScheduleConfig {
            windows: vec![RecurringFocusWindowConfig {
                days: vec!["fri".to_string()],
                start: "13:00".to_string(),
                end: "15:00".to_string(),
            }],
        },
    };

    let cfg = AppConfig {
        selected_profile: ProfileId::DeepWork,
        notifications: NotificationConfig::default(),
        auto_start: AutoStartConfig::default(),
        recurring_schedule: RecurringScheduleConfig::default(),
        strict_mode: false,
        profile_automation: Some(ProfileAutomationSettingsConfig {
            basic: Some(classic),
            standard: Some(deep_work.clone()),
            advanced: None,
        }),
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(cfg.notifications, NotificationConfig::default());
    assert_eq!(cfg.auto_start, AutoStartConfig::default());
    assert!(!cfg.strict_mode);
    assert_eq!(cfg.recurring_schedule, RecurringScheduleConfig::default());
}

#[test]
fn normalize_legacy_automation_fields_do_not_override_profile_automation() {
    let deep_work = ProfileAutomationConfig {
        notifications: NotificationConfig {
            enabled: true,
            sound: true,
        },
        auto_start: AutoStartConfig {
            focus_to_break: true,
            break_to_focus: true,
        },
        strict_mode: true,
        recurring_schedule: RecurringScheduleConfig {
            windows: vec![RecurringFocusWindowConfig {
                days: vec!["fri".to_string()],
                start: "13:00".to_string(),
                end: "15:00".to_string(),
            }],
        },
    };
    let cfg = AppConfig {
        selected_profile: ProfileId::DeepWork,
        notifications: NotificationConfig {
            enabled: false,
            sound: false,
        },
        auto_start: AutoStartConfig {
            focus_to_break: false,
            break_to_focus: false,
        },
        strict_mode: false,
        recurring_schedule: RecurringScheduleConfig::default(),
        profile_automation: Some(ProfileAutomationSettingsConfig {
            basic: None,
            standard: Some(deep_work.clone()),
            advanced: None,
        }),
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(
        cfg.notifications,
        NotificationConfig {
            enabled: false,
            sound: false,
        }
    );
    assert_eq!(
        cfg.auto_start,
        AutoStartConfig {
            focus_to_break: false,
            break_to_focus: false,
        }
    );
    assert!(!cfg.strict_mode);
    assert_eq!(cfg.recurring_schedule, RecurringScheduleConfig::default());
    assert_eq!(cfg.profile_automation_for(ProfileId::DeepWork), deep_work);
}

#[test]
fn normalize_merges_legacy_blocked_sites_when_profiles_exist() {
    let cfg = AppConfig {
        blocked_sites: vec!["legacy-only.com".to_string()],
        blocklist_profiles: vec![BlocklistProfileConfig {
            name: "Work".to_string(),
            sites: vec!["a.com".to_string(), "b.com".to_string()],
            allowlist_sites: vec!["b.com".to_string()],
        }],
        selected_blocklist_profile: "Work".to_string(),
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(cfg.selected_blocklist_profile, "Default");
    assert_eq!(cfg.blocked_sites, vec!["legacy-only.com".to_string()]);
    assert_eq!(cfg.blocklist_profiles.len(), 1);
    assert_eq!(
        cfg.blocklist_profiles[0].sites,
        vec![
            "legacy-only.com".to_string(),
            "a.com".to_string(),
            "b.com".to_string()
        ]
    );
    assert_eq!(
        cfg.blocklist_profiles[0].allowlist_sites,
        vec!["b.com".to_string()]
    );
}

#[test]
fn normalize_merges_legacy_profile_lists_when_categories_exist() {
    let cfg: AppConfig = toml::from_str(
        r#"
selected_blocklist_profile = "Work"

[[blocklist_profiles]]
name = "Work"
sites = ["legacy.com"]
allowlist_sites = ["legacy-allow.com"]
selected_category = "Social"

[[blocklist_profiles.categories]]
name = "Social"
sites = ["youtube.com"]
allowlist_sites = []
"#,
    )
    .unwrap();
    let cfg = cfg.normalize();

    let profile = &cfg.blocklist_profiles[0];
    assert!(profile.sites.contains(&"youtube.com".to_string()));
    assert!(profile.sites.contains(&"legacy.com".to_string()));
    assert!(
        profile
            .allowlist_sites
            .contains(&"legacy-allow.com".to_string())
    );
}

#[test]
fn config_migration_flattens_blocklist_categories_into_profile_rules() {
    let original: toml::Value = toml::from_str(
        r#"
schema_version = 2
selected_blocklist_profile = "Work"

[[blocklist_profiles]]
name = "Work"
sites = ["legacy.com"]
allowlist_sites = ["legacy-allow.com"]
selected_category = "Social"

[[blocklist_profiles.categories]]
name = "Social"
sites = ["youtube.com", "legacy.com"]
allowlist_sites = ["news.com"]

[[blocklist_profiles.categories]]
name = "General"
sites = ["docs.example.com"]
allowlist_sites = []
"#,
    )
    .unwrap();

    let (migrated, _, steps) = migrate_config_toml_to_current_detailed(original).unwrap();
    let profiles = migrated
        .get("blocklist_profiles")
        .and_then(toml::Value::as_array)
        .unwrap();
    let profile = profiles[0].as_table().unwrap();

    assert!(!profile.contains_key("categories"));
    assert!(!profile.contains_key("selected_category"));
    assert_eq!(
        profile.get("sites").unwrap().as_array().unwrap(),
        &vec![
            toml::Value::String("youtube.com".to_string()),
            toml::Value::String("legacy.com".to_string()),
            toml::Value::String("docs.example.com".to_string()),
        ]
    );
    assert_eq!(
        profile.get("allowlist_sites").unwrap().as_array().unwrap(),
        &vec![
            toml::Value::String("news.com".to_string()),
            toml::Value::String("legacy-allow.com".to_string()),
        ]
    );
    assert!(steps.iter().any(|step| {
        step.summary == "Flatten deprecated blocklist category rules into profile-level site lists."
    }));
}

#[test]
fn config_migration_removes_schedule_exception_dates() {
    let original: toml::Value = toml::from_str(
        r#"
schema_version = 2

[recurring_schedule]
exception_dates = ["2026-12-25"]

[profile_automation.basic.recurring_schedule]
exception_dates = ["2026-12-25"]

[[session_templates]]
name = "Morning"
task_label = "Docs"
profile = "basic"
blocklist_profile = "Default"

[session_templates.schedule]
exception_dates = ["2026-12-25"]
"#,
    )
    .unwrap();

    let (migrated, _, steps) = migrate_config_toml_to_current_detailed(original).unwrap();
    assert!(
        !migrated.to_string().contains("exception_dates"),
        "migrated config should not retain schedule exception date keys"
    );
    let root = migrated.as_table().expect("root should be a table");
    assert!(root.get("session_templates").is_none());
    assert!(steps.iter().any(|step| {
        step.summary
            == "Remove deprecated schedule exception dates; schedules now use recurring windows only."
    }));
}

#[test]
fn config_migration_removes_retired_wakatime_sections() {
    let original: toml::Value = toml::from_str(
        r#"
schema_version = 2

[wakatime]
api_key = "waka_key"
project = "FocusTime"
language = "Rust"

[wakatime_runtime]
queue_path = "queued-heartbeats.json"
max_queue_len = 256
retry_initial_backoff_secs = 5
retry_max_backoff_secs = 300
"#,
    )
    .unwrap();

    let (migrated, _, steps) = migrate_config_toml_to_current_detailed(original).unwrap();
    let root = migrated.as_table().expect("root should be a table");

    assert!(!root.contains_key("wakatime"));
    assert!(!root.contains_key("wakatime_runtime"));
    assert!(steps.iter().any(|step| {
        step.summary == "Remove retired WakaTime heartbeat config and runtime tuning sections."
    }));
}

#[test]
fn effective_blocked_sites_wildcard_allowlist_covers_narrower_wildcard_blocks() {
    let profile = BlocklistProfileConfig {
        sites: vec![
            "*.api.example.com".to_string(),
            "*.deep.api.example.com".to_string(),
            "*.other.com".to_string(),
        ],
        allowlist_sites: vec!["*.example.com".to_string()],
        ..BlocklistProfileConfig::default()
    };

    assert_eq!(
        effective_blocked_sites_for_profile(&profile),
        vec!["*.other.com".to_string()]
    );
}

#[test]
fn effective_blocked_sites_exact_allowlist_does_not_cancel_wildcard_block() {
    let profile = BlocklistProfileConfig {
        sites: vec!["*.api.example.com".to_string()],
        allowlist_sites: vec!["api.example.com".to_string()],
        ..BlocklistProfileConfig::default()
    };

    assert_eq!(
        effective_blocked_sites_for_profile(&profile),
        vec!["*.api.example.com".to_string()]
    );
}

#[test]
fn feature_flags_default_has_no_integrations() {
    let flags = FeatureFlagsConfig::default();
    assert!(flags.integrations.enabled.is_empty());
}

#[test]
fn feature_flags_normalization_deduplicates_and_trims_integration_names() {
    let normalized = FeatureFlagsConfig {
        integrations: IntegrationFeatureFlagsConfig {
            enabled: vec![
                "Calendar".to_string(),
                "  calendar ".to_string(),
                "custom".to_string(),
                "".to_string(),
            ],
        },
    }
    .normalized();

    assert_eq!(
        normalized.integrations.enabled,
        vec!["calendar".to_string(), "custom".to_string()]
    );
}
