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
    assert_eq!(cfg.selected_break_template, "Classic");
    assert_eq!(cfg.selected_theme_preset, ThemePreset::Classic);
    assert_eq!(cfg.break_templates.len(), 2);
    assert!(cfg.blocked_sites.is_empty());
    assert_eq!(cfg.selected_blocklist_profile, "Default");
    assert!(cfg.blocklist_profiles.is_empty());
    assert_eq!(cfg.notifications, NotificationConfig::default());
    assert_eq!(cfg.auto_start, AutoStartConfig::default());
    assert_eq!(cfg.recurring_schedule, RecurringScheduleConfig::default());
    assert_eq!(cfg.schedule_runtime, ScheduleRuntimeConfig::default());
    assert!(cfg.profile_automation.is_none());
    assert!(!cfg.strict_mode);
    assert_eq!(
        cfg.break_glass_duration_secs,
        default_break_glass_duration_secs()
    );
    assert_eq!(cfg.daily_goal, DailyGoalConfig::default());
    assert_eq!(cfg.weekly_goal, WeeklyGoalConfig::default());
    assert_eq!(cfg.monthly_goal, MonthlyGoalConfig::default());
    assert_eq!(cfg.goal_carry_over, GoalCarryOverConfig::default());
    assert_eq!(cfg.stats_retention, StatsRetentionConfig::default());
    assert_eq!(cfg.wakatime, WakatimeMetadataConfig::default());
    assert_eq!(cfg.wakatime_runtime, WakatimeRuntimeConfig::default());
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
    assert_eq!(windows.keep_break_glass_overrides_days, Some(180));
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
    assert_eq!(keep_all.keep_break_glass_overrides_days, None);
    assert_eq!(keep_all.keep_weekly_goal_snapshots_days, None);
    assert_eq!(keep_all.keep_monthly_goal_snapshots_days, None);

    let aggressive = StatsRetentionConfig {
        preset: StatsRetentionPreset::Aggressive,
    }
    .windows();
    assert_eq!(aggressive.keep_daily_days, Some(365));
    assert_eq!(aggressive.keep_focus_sessions_days, Some(180));
    assert_eq!(aggressive.keep_session_interruptions_days, Some(90));
    assert_eq!(aggressive.keep_break_glass_overrides_days, Some(90));
    assert_eq!(aggressive.keep_weekly_goal_snapshots_days, Some(180));
    assert_eq!(aggressive.keep_monthly_goal_snapshots_days, Some(180));
}

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
        break_templates: vec![
            BreakTemplateConfig {
                name: "Quick".to_string(),
                short_break_secs: 3 * 60,
                long_break_secs: 10 * 60,
                long_break_interval: 4,
            },
            BreakTemplateConfig {
                name: "Recovery".to_string(),
                short_break_secs: 8 * 60,
                long_break_secs: 20 * 60,
                long_break_interval: 3,
            },
        ],
        selected_break_template: "Recovery".to_string(),
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
            exception_dates: vec!["2026-04-27".to_string(), "2026-05-05".to_string()],
            one_time_windows: vec![OneTimeFocusWindowConfig {
                date: "2026-05-10".to_string(),
                start: "14:00".to_string(),
                end: "15:00".to_string(),
            }],
        },
        schedule_runtime: ScheduleRuntimeConfig {
            time_step_minutes: 20,
            delay_secs: 15 * 60,
        },
        profile_automation: Some(ProfileAutomationSettingsConfig {
            classic: Some(ProfileAutomationConfig {
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
            deep_work: Some(ProfileAutomationConfig {
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
                    exception_dates: vec!["2026-04-27".to_string(), "2026-05-05".to_string()],
                    one_time_windows: vec![OneTimeFocusWindowConfig {
                        date: "2026-05-10".to_string(),
                        start: "14:00".to_string(),
                        end: "15:00".to_string(),
                    }],
                },
            }),
            custom: Some(ProfileAutomationConfig {
                notifications: NotificationConfig::default(),
                auto_start: AutoStartConfig::default(),
                strict_mode: false,
                recurring_schedule: RecurringScheduleConfig::default(),
            }),
        }),
        strict_mode: true,
        break_glass_duration_secs: 7 * 60,
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
        wakatime: WakatimeMetadataConfig {
            project: "Team Focus".to_string(),
            language: "Focus Session".to_string(),
            task_mappings: vec![
                WakatimeTaskMappingConfig {
                    task_label: "Docs".to_string(),
                    project: Some("Documentation".to_string()),
                    language: Some("Markdown".to_string()),
                },
                WakatimeTaskMappingConfig {
                    task_label: "Code".to_string(),
                    project: Some("Engineering".to_string()),
                    language: None,
                },
            ],
        },
        wakatime_runtime: WakatimeRuntimeConfig {
            retry_backoff_secs: vec![2, 5, 10],
            queue_capacity: 512,
            queue_retry_delay_secs: 30,
        },
        feature_flags: FeatureFlagsConfig {
            legacy_automation_mirror: true,
            legacy_blocked_sites_mirror: true,
            metadata_task_label_fallback: true,
        },
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
    let parsed: AppConfig = toml::from_str(&toml_str).unwrap();
    assert_eq!(parsed.focus_secs, original.focus_secs);
    assert_eq!(parsed.short_break_secs, original.short_break_secs);
    assert_eq!(parsed.long_break_secs, original.long_break_secs);
    assert_eq!(parsed.long_break_interval, original.long_break_interval);
    assert_eq!(parsed.blocked_sites, original.blocked_sites);
    assert_eq!(parsed.blocklist_profiles, original.blocklist_profiles);
    assert_eq!(
        parsed.selected_blocklist_profile,
        original.selected_blocklist_profile
    );
    assert_eq!(parsed.selected_profile, original.selected_profile);
    assert_eq!(parsed.custom_profile, original.custom_profile);
    assert_eq!(parsed.break_templates, original.break_templates);
    assert_eq!(
        parsed.selected_break_template,
        original.selected_break_template
    );
    assert_eq!(parsed.selected_theme_preset, original.selected_theme_preset);
    assert_eq!(parsed.notifications, original.notifications);
    assert_eq!(parsed.auto_start, original.auto_start);
    assert_eq!(parsed.recurring_schedule, original.recurring_schedule);
    assert_eq!(parsed.schedule_runtime, original.schedule_runtime);
    assert_eq!(parsed.profile_automation, original.profile_automation);
    assert_eq!(parsed.strict_mode, original.strict_mode);
    assert_eq!(
        parsed.break_glass_duration_secs,
        original.break_glass_duration_secs
    );
    assert_eq!(parsed.daily_goal, original.daily_goal);
    assert_eq!(parsed.weekly_goal, original.weekly_goal);
    assert_eq!(parsed.monthly_goal, original.monthly_goal);
    assert_eq!(parsed.goal_carry_over, original.goal_carry_over);
    assert_eq!(parsed.stats_retention, original.stats_retention);
    assert_eq!(parsed.wakatime, original.wakatime);
    assert_eq!(parsed.wakatime_runtime, original.wakatime_runtime);
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
    assert_eq!(cfg.selected_break_template, "");
    assert_eq!(cfg.selected_theme_preset, ThemePreset::Classic);
    assert_eq!(cfg.break_templates.len(), 2);
    assert!(cfg.blocked_sites.is_empty());
    assert!(cfg.blocklist_profiles.is_empty());
    assert_eq!(cfg.selected_blocklist_profile, "Default");
    assert_eq!(cfg.notifications, NotificationConfig::default());
    assert_eq!(cfg.auto_start, AutoStartConfig::default());
    assert_eq!(cfg.recurring_schedule, RecurringScheduleConfig::default());
    assert!(cfg.profile_automation.is_none());
    assert!(!cfg.strict_mode);
    assert_eq!(
        cfg.break_glass_duration_secs,
        default_break_glass_duration_secs()
    );
    assert_eq!(cfg.daily_goal, DailyGoalConfig::default());
    assert_eq!(cfg.weekly_goal, WeeklyGoalConfig::default());
    assert_eq!(cfg.monthly_goal, MonthlyGoalConfig::default());
    assert_eq!(cfg.goal_carry_over, GoalCarryOverConfig::default());
    assert_eq!(cfg.wakatime, WakatimeMetadataConfig::default());
    assert_eq!(cfg.feature_flags, FeatureFlagsConfig::default());
    assert_eq!(cfg.shortcuts, ShortcutConfig::default());
}

#[test]
fn normalize_clamps_zero_break_glass_duration_to_default() {
    let cfg = AppConfig {
        break_glass_duration_secs: 0,
        ..AppConfig::default()
    }
    .normalize();
    assert_eq!(
        cfg.break_glass_duration_secs,
        default_break_glass_duration_secs()
    );
}

#[test]
fn normalize_break_templates_preserves_empty_selection() {
    let cfg = AppConfig {
        break_templates: Vec::new(),
        selected_break_template: String::new(),
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(cfg.break_templates.len(), 2);
    assert_eq!(cfg.selected_break_template, "");
}

#[test]
fn normalize_break_templates_deduplicates_names_and_clamps_values() {
    let cfg = AppConfig {
        break_templates: vec![
            BreakTemplateConfig {
                name: "Recovery".to_string(),
                short_break_secs: 0,
                long_break_secs: 0,
                long_break_interval: 0,
            },
            BreakTemplateConfig {
                name: "recovery".to_string(),
                short_break_secs: 2 * 60,
                long_break_secs: 12 * 60,
                long_break_interval: 2,
            },
        ],
        selected_break_template: "missing".to_string(),
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(cfg.break_templates[0].name, "Recovery");
    assert_eq!(
        cfg.break_templates[0].short_break_secs,
        default_short_break_secs()
    );
    assert_eq!(
        cfg.break_templates[0].long_break_secs,
        default_long_break_secs()
    );
    assert_eq!(
        cfg.break_templates[0].long_break_interval,
        default_long_break_interval()
    );
    assert_eq!(cfg.break_templates[1].name, "recovery (2)");
    assert_eq!(cfg.selected_break_template, "Recovery");
}

#[test]
fn shortcut_config_normalizes_invalid_tokens_to_defaults() {
    let cfg = AppConfig {
        shortcuts: ShortcutConfig {
            quit: "qq".to_string(),
            timer_toggle_pause: "    ".to_string(),
            site_add: "Add".to_string(),
            planner_select_recent: "".to_string(),
            ..ShortcutConfig::default()
        },
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(cfg.shortcuts.quit, "q");
    assert_eq!(cfg.shortcuts.timer_toggle_pause, "space");
    assert_eq!(cfg.shortcuts.site_add, "a");
    assert_eq!(cfg.shortcuts.planner_select_recent, "r");
}

#[test]
fn shortcut_config_normalizes_space_and_uppercase_tokens() {
    let cfg: AppConfig = toml::from_str(
        r#"
[shortcuts]
timer_toggle_pause = "SPACE"
open_stats_history = "Y"
select_previous_blocklist_profile = "["
"#,
    )
    .unwrap();
    let normalized = cfg.normalize();

    assert_eq!(normalized.shortcuts.timer_toggle_pause, "space");
    assert_eq!(normalized.shortcuts.open_stats_history, "y");
    assert_eq!(normalized.shortcuts.select_previous_blocklist_profile, "[");
}

#[test]
fn normalize_selected_break_template_uses_template_matching_custom_values() {
    let cfg = AppConfig {
        selected_break_template: "Classic".to_string(),
        custom_profile: Some(CustomProfileConfig {
            focus_secs: default_focus_secs(),
            short_break_secs: 10 * 60,
            long_break_secs: 30 * 60,
            long_break_interval: 3,
        }),
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(cfg.selected_break_template, "Deep Work");
}

#[test]
fn normalize_selected_break_template_clears_unknown_when_no_template_matches_custom_values() {
    let cfg = AppConfig {
        selected_break_template: "Classic".to_string(),
        custom_profile: Some(CustomProfileConfig {
            focus_secs: default_focus_secs(),
            short_break_secs: 7 * 60,
            long_break_secs: 21 * 60,
            long_break_interval: 5,
        }),
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(cfg.selected_break_template, "");
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
    assert!(cfg.recurring_schedule.exception_dates.is_empty());
    assert!(cfg.recurring_schedule.one_time_windows.is_empty());
}

#[test]
fn partial_one_time_schedule_window_uses_defaults_for_missing_fields() {
    let partial = r#"
[recurring_schedule]
[[recurring_schedule.one_time_windows]]
date = "2026-04-27"
"#;
    let cfg: AppConfig = toml::from_str(partial).unwrap();

    assert_eq!(cfg.recurring_schedule.one_time_windows.len(), 1);
    let window = &cfg.recurring_schedule.one_time_windows[0];
    assert_eq!(window.date, "2026-04-27");
    assert_eq!(window.start, default_schedule_window_start());
    assert_eq!(window.end, default_schedule_window_end());
}

#[test]
fn normalize_drops_one_time_window_without_date_in_config() {
    let partial = r#"
[recurring_schedule]
[[recurring_schedule.one_time_windows]]
start = "09:00"
end = "10:00"
"#;
    let cfg: AppConfig = toml::from_str(partial).unwrap();
    let normalized = cfg.normalize();

    assert!(normalized.recurring_schedule.one_time_windows.is_empty());
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
            exception_dates: Vec::new(),
            one_time_windows: Vec::new(),
        },
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(cfg.recurring_schedule.windows.len(), 1);
    assert_eq!(cfg.recurring_schedule.windows[0].start, "09:00");
    assert_eq!(cfg.recurring_schedule.windows[0].end, "11:00");
}

#[test]
fn normalize_recurring_schedule_exception_dates_dedupes_and_drops_invalid_entries() {
    let cfg = AppConfig {
        recurring_schedule: RecurringScheduleConfig {
            windows: Vec::new(),
            exception_dates: vec![
                " 2026-12-25 ".to_string(),
                "2026-02-30".to_string(),
                "2026-01-01".to_string(),
                "2026-12-25".to_string(),
                "not-a-date".to_string(),
            ],
            one_time_windows: Vec::new(),
        },
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(
        cfg.recurring_schedule.exception_dates,
        vec!["2026-01-01".to_string(), "2026-12-25".to_string()]
    );
}

#[test]
fn normalize_drops_one_time_windows_with_invalid_entries() {
    let cfg = AppConfig {
        recurring_schedule: RecurringScheduleConfig {
            windows: Vec::new(),
            exception_dates: Vec::new(),
            one_time_windows: vec![
                OneTimeFocusWindowConfig {
                    date: "2026-04-27".to_string(),
                    start: "10:00".to_string(),
                    end: "11:00".to_string(),
                },
                OneTimeFocusWindowConfig {
                    date: "not-a-date".to_string(),
                    start: "10:00".to_string(),
                    end: "11:00".to_string(),
                },
                OneTimeFocusWindowConfig {
                    date: "2026-04-28".to_string(),
                    start: "12:00".to_string(),
                    end: "11:00".to_string(),
                },
                OneTimeFocusWindowConfig {
                    date: "2026-04-29".to_string(),
                    start: "25:00".to_string(),
                    end: "26:00".to_string(),
                },
                OneTimeFocusWindowConfig {
                    date: String::new(),
                    start: "09:00".to_string(),
                    end: "10:00".to_string(),
                },
            ],
        },
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(cfg.recurring_schedule.one_time_windows.len(), 1);
    assert_eq!(
        cfg.recurring_schedule.one_time_windows[0].date,
        "2026-04-27"
    );
    assert_eq!(cfg.recurring_schedule.one_time_windows[0].start, "10:00");
    assert_eq!(cfg.recurring_schedule.one_time_windows[0].end, "11:00");
}

#[test]
fn normalize_clamps_schedule_runtime_knobs_to_safe_bounds() {
    let cfg = AppConfig {
        schedule_runtime: ScheduleRuntimeConfig {
            time_step_minutes: 0,
            delay_secs: 0,
        },
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(
        cfg.schedule_runtime,
        ScheduleRuntimeConfig {
            time_step_minutes: 1,
            delay_secs: 60,
        }
    );

    let cfg = AppConfig {
        schedule_runtime: ScheduleRuntimeConfig {
            time_step_minutes: 200,
            delay_secs: 24 * 60 * 60,
        },
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(
        cfg.schedule_runtime,
        ScheduleRuntimeConfig {
            time_step_minutes: 60,
            delay_secs: 12 * 60 * 60,
        }
    );
}

#[test]
fn normalize_clamps_wakatime_runtime_knobs_and_falls_back_for_invalid_backoff() {
    let cfg = AppConfig {
        wakatime_runtime: WakatimeRuntimeConfig {
            retry_backoff_secs: vec![0, 900, 2, 0],
            queue_capacity: 0,
            queue_retry_delay_secs: 24 * 60 * 60,
        },
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(cfg.wakatime_runtime.retry_backoff_secs, vec![300, 2]);
    assert_eq!(cfg.wakatime_runtime.queue_capacity, 1);
    assert_eq!(cfg.wakatime_runtime.queue_retry_delay_secs, 60 * 60);

    let cfg = AppConfig {
        wakatime_runtime: WakatimeRuntimeConfig {
            retry_backoff_secs: vec![0, 0],
            ..WakatimeRuntimeConfig::default()
        },
        ..AppConfig::default()
    }
    .normalize();
    assert_eq!(cfg.wakatime_runtime.retry_backoff_secs, vec![1, 2]);
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
    assert_eq!(parsed.wakatime, WakatimeMetadataConfig::default());
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
        break_templates: default_break_templates(),
        selected_break_template: default_break_template_name(),
        selected_theme_preset: ThemePreset::Classic,
        notifications: NotificationConfig::default(),
        auto_start: AutoStartConfig::default(),
        recurring_schedule: RecurringScheduleConfig::default(),
        schedule_runtime: ScheduleRuntimeConfig::default(),
        profile_automation: None,
        strict_mode: false,
        break_glass_duration_secs: default_break_glass_duration_secs(),
        daily_goal: DailyGoalConfig::default(),
        weekly_goal: WeeklyGoalConfig::default(),
        monthly_goal: MonthlyGoalConfig::default(),
        goal_carry_over: GoalCarryOverConfig::default(),
        stats_retention: StatsRetentionConfig::default(),
        wakatime: WakatimeMetadataConfig::default(),
        wakatime_runtime: WakatimeRuntimeConfig::default(),
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
    assert_eq!(cfg.selected_break_template, "Classic");
    assert_eq!(cfg.break_templates.len(), 2);
    assert!(cfg.blocked_sites.is_empty());
    assert_eq!(cfg.selected_blocklist_profile, "Default");
    assert!(cfg.blocklist_profiles.is_empty());
    assert_eq!(cfg.auto_start, AutoStartConfig::default());
    assert_eq!(cfg.recurring_schedule, RecurringScheduleConfig::default());
    assert!(cfg.profile_automation.is_none());
    assert!(!cfg.strict_mode);
    assert_eq!(
        cfg.break_glass_duration_secs,
        default_break_glass_duration_secs()
    );
    assert_eq!(cfg.daily_goal, DailyGoalConfig::default());
    assert_eq!(cfg.weekly_goal, WeeklyGoalConfig::default());
    assert_eq!(cfg.monthly_goal, MonthlyGoalConfig::default());
    assert_eq!(cfg.goal_carry_over, GoalCarryOverConfig::default());
    assert_eq!(cfg.wakatime, WakatimeMetadataConfig::default());
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
        Some(2100)
    );
}

#[test]
fn wakatime_metadata_normalizes_blank_fields_to_defaults() {
    let cfg: AppConfig = toml::from_str(
        r#"
[wakatime]
project = "   "
language = ""
"#,
    )
    .unwrap();
    let normalized = cfg.normalize();
    assert_eq!(normalized.wakatime, WakatimeMetadataConfig::default());
}

#[test]
fn wakatime_task_mappings_normalize_filter_and_deduplicate() {
    let cfg: AppConfig = toml::from_str(
        r#"
[wakatime]
project = " Team Focus "
language = " Focus Session "

[[wakatime.task_mappings]]
task_label = " Docs "
project = " Documentation "

[[wakatime.task_mappings]]
task_label = "docs"
language = "Rust"

[[wakatime.task_mappings]]
task_label = "Planning"
project = "   "
language = ""

[[wakatime.task_mappings]]
task_label = "   "
project = "Ignored"
"#,
    )
    .unwrap();

    let normalized = cfg.normalize();
    assert_eq!(normalized.wakatime.project, "Team Focus");
    assert_eq!(normalized.wakatime.language, "Focus Session");
    assert_eq!(
        normalized.wakatime.task_mappings,
        vec![WakatimeTaskMappingConfig {
            task_label: "Docs".to_string(),
            project: Some("Documentation".to_string()),
            language: None,
        }]
    );
}

#[test]
fn wakatime_task_mappings_resolve_with_per_field_fallback() {
    let metadata = WakatimeMetadataConfig {
        project: "Global Project".to_string(),
        language: "Global Language".to_string(),
        task_mappings: vec![
            WakatimeTaskMappingConfig {
                task_label: "Docs".to_string(),
                project: Some("Documentation".to_string()),
                language: None,
            },
            WakatimeTaskMappingConfig {
                task_label: "Review".to_string(),
                project: None,
                language: Some("PR Review".to_string()),
            },
        ],
    };

    let (docs_project, docs_language) =
        metadata.resolved_project_language_for_task_label(Some(" docs "));
    assert_eq!(docs_project, "Documentation");
    assert_eq!(docs_language, "Global Language");

    let (review_project, review_language) =
        metadata.resolved_project_language_for_task_label(Some("REVIEW"));
    assert_eq!(review_project, "Global Project");
    assert_eq!(review_language, "PR Review");

    let (fallback_project, fallback_language) =
        metadata.resolved_project_language_for_task_label(Some("Unknown"));
    assert_eq!(fallback_project, "Global Project");
    assert_eq!(fallback_language, "Global Language");

    let (none_project, none_language) = metadata.resolved_project_language_for_task_label(None);
    assert_eq!(none_project, "Global Project");
    assert_eq!(none_language, "Global Language");
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
fn normalize_deduplicates_profile_names_and_fixes_selection() {
    let cfg = AppConfig {
        blocklist_profiles: vec![
            BlocklistProfileConfig {
                name: "Work".to_string(),
                sites: vec!["a.com".to_string()],
                allowlist_sites: vec!["b.com".to_string()],
            },
            BlocklistProfileConfig {
                name: "work".to_string(),
                sites: vec!["b.com".to_string()],
                allowlist_sites: Vec::new(),
            },
        ],
        selected_blocklist_profile: "missing".to_string(),
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(cfg.blocklist_profiles[0].name, "Work");
    assert_eq!(cfg.blocklist_profiles[1].name, "work (2)");
    assert_eq!(cfg.selected_blocklist_profile, "Work");
    assert_eq!(cfg.blocked_sites, vec!["a.com".to_string()]);
}

#[test]
fn normalize_derives_legacy_blocked_sites_from_effective_blocked_set() {
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

    assert_eq!(cfg.blocked_sites, vec!["a.com".to_string()]);
}

#[test]
fn normalize_migrates_legacy_automation_into_per_profile_settings() {
    let legacy_schedule = RecurringScheduleConfig {
        windows: vec![RecurringFocusWindowConfig {
            days: vec!["mon".to_string(), "tue".to_string()],
            start: "09:00".to_string(),
            end: "10:30".to_string(),
        }],
        exception_dates: vec!["2026-12-25".to_string()],
        one_time_windows: vec![OneTimeFocusWindowConfig {
            date: "2026-05-02".to_string(),
            start: "14:00".to_string(),
            end: "16:00".to_string(),
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
fn normalize_selected_profile_automation_updates_legacy_view() {
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
            exception_dates: Vec::new(),
            one_time_windows: Vec::new(),
        },
    };

    let cfg = AppConfig {
        selected_profile: ProfileId::DeepWork,
        notifications: NotificationConfig::default(),
        auto_start: AutoStartConfig::default(),
        recurring_schedule: RecurringScheduleConfig::default(),
        strict_mode: false,
        profile_automation: Some(ProfileAutomationSettingsConfig {
            classic: Some(classic),
            deep_work: Some(deep_work.clone()),
            custom: None,
        }),
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(cfg.notifications, deep_work.notifications);
    assert_eq!(cfg.auto_start, deep_work.auto_start);
    assert_eq!(cfg.strict_mode, deep_work.strict_mode);
    assert_eq!(cfg.recurring_schedule, deep_work.recurring_schedule);
}

#[test]
fn normalize_keeps_legacy_automation_fields_when_mirror_flag_is_disabled() {
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
            exception_dates: Vec::new(),
            one_time_windows: Vec::new(),
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
            classic: None,
            deep_work: Some(deep_work.clone()),
            custom: None,
        }),
        feature_flags: FeatureFlagsConfig {
            legacy_automation_mirror: false,
            ..FeatureFlagsConfig::default()
        },
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
fn normalize_keeps_legacy_blocked_sites_when_mirror_flag_is_disabled() {
    let cfg = AppConfig {
        blocked_sites: vec!["legacy-only.com".to_string()],
        blocklist_profiles: vec![BlocklistProfileConfig {
            name: "Work".to_string(),
            sites: vec!["a.com".to_string(), "b.com".to_string()],
            allowlist_sites: vec!["b.com".to_string()],
        }],
        selected_blocklist_profile: "Work".to_string(),
        feature_flags: FeatureFlagsConfig {
            legacy_blocked_sites_mirror: false,
            ..FeatureFlagsConfig::default()
        },
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(cfg.selected_blocklist_profile, "Work");
    assert_eq!(cfg.blocked_sites, vec!["legacy-only.com".to_string()]);
}
