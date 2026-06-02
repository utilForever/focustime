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
    assert!(cfg.session_templates.is_empty());
    assert!(cfg.selected_session_template.is_empty());
    assert!(cfg.automation_triggers.is_empty());
    assert_eq!(cfg.selected_theme_preset, ThemePreset::Classic);
    assert!(cfg.blocked_sites.is_empty());
    assert_eq!(cfg.selected_blocklist_profile, "Default");
    assert!(cfg.blocklist_profiles.is_empty());
    assert_eq!(cfg.blocking_backend, BlockingBackendConfig::default());
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
    assert_eq!(cfg.history_dashboard, HistoryDashboardConfig::default());
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
                ..BlocklistProfileConfig::default()
            },
            BlocklistProfileConfig {
                name: "Study".to_string(),
                sites: vec!["x.com".to_string()],
                allowlist_sites: Vec::new(),
                ..BlocklistProfileConfig::default()
            },
        ],
        selected_blocklist_profile: "Study".to_string(),
        blocking_backend: BlockingBackendConfig::default(),
        selected_profile: ProfileId::DeepWork,
        custom_profile: Some(CustomProfileConfig {
            focus_secs: 30 * 60,
            short_break_secs: 7 * 60,
            long_break_secs: 12 * 60,
            long_break_interval: 5,
        }),
        session_templates: vec![SessionTemplateConfig {
            name: "Morning deep work".to_string(),
            task_label: "Docs".to_string(),
            profile: ProfileId::DeepWork,
            blocklist_profile: "Work".to_string(),
            schedule: RecurringScheduleConfig {
                windows: vec![RecurringFocusWindowConfig {
                    days: vec!["mon".to_string(), "wed".to_string()],
                    start: "09:15".to_string(),
                    end: "11:00".to_string(),
                }],
                exception_dates: vec!["2026-04-27".to_string()],
                one_time_windows: vec![OneTimeFocusWindowConfig {
                    date: "2026-05-10".to_string(),
                    start: "14:00".to_string(),
                    end: "15:00".to_string(),
                }],
            },
        }],
        selected_session_template: "Morning deep work".to_string(),
        automation_triggers: vec![
            AutomationTriggerRuleConfig {
                trigger: AutomationTriggerConditionConfig::Time {
                    days: vec!["mon".to_string(), "wed".to_string()],
                    at: "08:55".to_string(),
                },
                action: AutomationTriggerActionConfig::ApplyDefaults {
                    profile: ProfileId::DeepWork,
                    blocklist_profile: "Work".to_string(),
                    session_template: Some("Morning deep work".to_string()),
                },
            },
            AutomationTriggerRuleConfig {
                trigger: AutomationTriggerConditionConfig::FocusCompleted,
                action: AutomationTriggerActionConfig::DelayScheduleStart { delay_secs: 5 * 60 },
            },
        ],
        weekday_profile_rules: Vec::new(),
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
        calendar_sync: CalendarSyncConfig::default(),
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
                    exception_dates: vec!["2026-04-27".to_string(), "2026-05-05".to_string()],
                    one_time_windows: vec![OneTimeFocusWindowConfig {
                        date: "2026-05-10".to_string(),
                        start: "14:00".to_string(),
                        end: "15:00".to_string(),
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
    assert_eq!(parsed.blocking_backend, original.blocking_backend);
    assert_eq!(parsed.selected_profile, original.selected_profile);
    assert_eq!(parsed.custom_profile, original.custom_profile);
    assert_eq!(parsed.session_templates, original.session_templates);
    assert_eq!(
        parsed.selected_session_template,
        original.selected_session_template
    );
    assert_eq!(parsed.automation_triggers, original.automation_triggers);
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
    assert_eq!(
        parsed.break_glass_duration_secs,
        original.break_glass_duration_secs
    );
    assert_eq!(parsed.daily_goal, original.daily_goal);
    assert_eq!(parsed.weekly_goal, original.weekly_goal);
    assert_eq!(parsed.monthly_goal, original.monthly_goal);
    assert_eq!(parsed.goal_carry_over, original.goal_carry_over);
    assert_eq!(parsed.stats_retention, original.stats_retention);
    assert_eq!(parsed.history_dashboard, original.history_dashboard);
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
    assert!(cfg.session_templates.is_empty());
    assert_eq!(cfg.selected_session_template, "");
    assert_eq!(cfg.selected_theme_preset, ThemePreset::Classic);
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
    assert_eq!(cfg.history_dashboard, HistoryDashboardConfig::default());
    assert_eq!(cfg.wakatime, WakatimeMetadataConfig::default());
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
fn normalize_history_dashboard_filters_unknown_and_appends_missing_cards() {
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

    assert_eq!(
        cfg.history_dashboard.card_order[0],
        HistoryKpiCardId::GoalStreak
    );
    assert_eq!(
        cfg.history_dashboard.card_order[1],
        HistoryKpiCardId::FocusScore
    );
    assert_eq!(
        cfg.history_dashboard.card_order.len(),
        HistoryKpiCardId::all().len()
    );
    for card in HistoryKpiCardId::all() {
        assert!(cfg.history_dashboard.card_order.contains(&card));
    }
}

#[test]
fn normalize_history_dashboard_filters_unknown_and_duplicate_pins() {
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

    assert_eq!(
        cfg.history_dashboard.pinned_cards,
        vec![
            HistoryKpiCardId::Retention,
            HistoryKpiCardId::LastInterruption
        ]
    );
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
fn normalize_session_templates_deduplicates_and_filters_invalid_entries() {
    let cfg = AppConfig {
        blocklist_profiles: vec![
            BlocklistProfileConfig {
                name: "Work".to_string(),
                sites: vec!["youtube.com".to_string()],
                allowlist_sites: Vec::new(),
                ..BlocklistProfileConfig::default()
            },
            BlocklistProfileConfig {
                name: "Study".to_string(),
                sites: vec!["reddit.com".to_string()],
                allowlist_sites: Vec::new(),
                ..BlocklistProfileConfig::default()
            },
        ],
        session_templates: vec![
            SessionTemplateConfig {
                name: "Morning".to_string(),
                task_label: "  Docs  ".to_string(),
                profile: ProfileId::DeepWork,
                blocklist_profile: "work".to_string(),
                schedule: RecurringScheduleConfig::default(),
            },
            SessionTemplateConfig {
                name: "morning".to_string(),
                task_label: "Code".to_string(),
                profile: ProfileId::Classic,
                blocklist_profile: "missing".to_string(),
                schedule: RecurringScheduleConfig::default(),
            },
            SessionTemplateConfig {
                name: "No Task".to_string(),
                task_label: "   ".to_string(),
                profile: ProfileId::Custom,
                blocklist_profile: "Study".to_string(),
                schedule: RecurringScheduleConfig::default(),
            },
        ],
        selected_session_template: "MORNING".to_string(),
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(cfg.session_templates.len(), 2);
    assert_eq!(cfg.session_templates[0].name, "Morning");
    assert_eq!(cfg.session_templates[0].task_label, "Docs");
    assert_eq!(cfg.session_templates[0].blocklist_profile, "Work");
    assert_eq!(cfg.session_templates[1].name, "morning (2)");
    assert_eq!(cfg.session_templates[1].task_label, "Code");
    assert_eq!(cfg.session_templates[1].blocklist_profile, "Work");
    assert_eq!(cfg.selected_session_template, "Morning");
}

#[test]
fn normalize_selected_session_template_clears_unknown_value() {
    let cfg = AppConfig {
        session_templates: vec![SessionTemplateConfig {
            name: "Morning".to_string(),
            task_label: "Docs".to_string(),
            profile: ProfileId::DeepWork,
            blocklist_profile: "Default".to_string(),
            schedule: RecurringScheduleConfig::default(),
        }],
        selected_session_template: "Missing".to_string(),
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(cfg.selected_session_template, "");
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
fn normalize_filters_invalid_time_triggers_and_clamps_delay_action() {
    let cfg = AppConfig {
        automation_triggers: vec![
            AutomationTriggerRuleConfig {
                trigger: AutomationTriggerConditionConfig::Time {
                    days: vec!["MONDAY".to_string(), "fri".to_string(), "mon".to_string()],
                    at: "08:05".to_string(),
                },
                action: AutomationTriggerActionConfig::DelayScheduleStart { delay_secs: 0 },
            },
            AutomationTriggerRuleConfig {
                trigger: AutomationTriggerConditionConfig::Time {
                    days: vec!["nonday".to_string()],
                    at: "09:00".to_string(),
                },
                action: AutomationTriggerActionConfig::StartFocus,
            },
        ],
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(cfg.automation_triggers.len(), 1);
    assert_eq!(
        cfg.automation_triggers[0],
        AutomationTriggerRuleConfig {
            trigger: AutomationTriggerConditionConfig::Time {
                days: vec!["mon".to_string(), "fri".to_string()],
                at: "08:05".to_string(),
            },
            action: AutomationTriggerActionConfig::DelayScheduleStart { delay_secs: 60 },
        }
    );
}

#[test]
fn normalize_automation_trigger_apply_defaults_resolves_references() {
    let cfg = AppConfig {
        blocklist_profiles: vec![
            BlocklistProfileConfig {
                name: "Default".to_string(),
                sites: Vec::new(),
                allowlist_sites: Vec::new(),
                ..BlocklistProfileConfig::default()
            },
            BlocklistProfileConfig {
                name: "Work".to_string(),
                sites: Vec::new(),
                allowlist_sites: Vec::new(),
                ..BlocklistProfileConfig::default()
            },
        ],
        session_templates: vec![SessionTemplateConfig {
            name: "Deep Flow".to_string(),
            task_label: "Docs".to_string(),
            profile: ProfileId::DeepWork,
            blocklist_profile: "Work".to_string(),
            schedule: RecurringScheduleConfig::default(),
        }],
        automation_triggers: vec![AutomationTriggerRuleConfig {
            trigger: AutomationTriggerConditionConfig::FocusStarted,
            action: AutomationTriggerActionConfig::ApplyDefaults {
                profile: ProfileId::DeepWork,
                blocklist_profile: "work".to_string(),
                session_template: Some("deep flow".to_string()),
            },
        }],
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(cfg.automation_triggers.len(), 1);
    assert_eq!(
        cfg.automation_triggers[0],
        AutomationTriggerRuleConfig {
            trigger: AutomationTriggerConditionConfig::FocusStarted,
            action: AutomationTriggerActionConfig::ApplyDefaults {
                profile: ProfileId::DeepWork,
                blocklist_profile: "Work".to_string(),
                session_template: Some("Deep Flow".to_string()),
            },
        }
    );
}

#[test]
fn automation_trigger_rules_require_trigger_and_action_fields() {
    let missing_action = r#"
[[automation_triggers]]
trigger = { type = "focus_started" }
"#;
    let missing_trigger = r#"
[[automation_triggers]]
action = { type = "start_focus" }
"#;

    let missing_action_error = toml::from_str::<AppConfig>(missing_action)
        .expect_err("rule missing action should fail deserialization")
        .to_string();
    let missing_trigger_error = toml::from_str::<AppConfig>(missing_trigger)
        .expect_err("rule missing trigger should fail deserialization")
        .to_string();

    assert!(missing_action_error.contains("missing field `action`"));
    assert!(missing_trigger_error.contains("missing field `trigger`"));
}

#[test]
fn validate_automation_trigger_rules_rejects_conflicting_rules() {
    let rules = vec![
        AutomationTriggerRuleConfig {
            trigger: AutomationTriggerConditionConfig::Time {
                days: vec!["mon".to_string()],
                at: "09:00".to_string(),
            },
            action: AutomationTriggerActionConfig::StartFocus,
        },
        AutomationTriggerRuleConfig {
            trigger: AutomationTriggerConditionConfig::Time {
                days: vec!["monday".to_string()],
                at: "09:00".to_string(),
            },
            action: AutomationTriggerActionConfig::DelayScheduleStart { delay_secs: 5 * 60 },
        },
    ];

    let error =
        validate_automation_trigger_rules(&rules, &[BlocklistProfileConfig::default()], &[])
            .unwrap_err();

    assert!(error.contains("Conflicting automation trigger rules"));
    assert!(error.contains("rule #1"));
    assert!(error.contains("rule #2"));
    assert!(error.contains("start_focus"));
    assert!(error.contains("delay_schedule_start"));
    assert!(error.contains("time trigger"));
    assert!(error.contains("do not overlap"));
}

#[test]
fn validate_automation_trigger_rules_rejects_event_conflicts_with_action_diagnostics() {
    let rules = vec![
        AutomationTriggerRuleConfig {
            trigger: AutomationTriggerConditionConfig::FocusStarted,
            action: AutomationTriggerActionConfig::StartFocus,
        },
        AutomationTriggerRuleConfig {
            trigger: AutomationTriggerConditionConfig::FocusStarted,
            action: AutomationTriggerActionConfig::ApplyDefaults {
                profile: ProfileId::DeepWork,
                blocklist_profile: "Default".to_string(),
                session_template: None,
            },
        },
    ];

    let error =
        validate_automation_trigger_rules(&rules, &[BlocklistProfileConfig::default()], &[])
            .unwrap_err();

    assert!(error.contains("rule #1"));
    assert!(error.contains("rule #2"));
    assert!(error.contains("start_focus"));
    assert!(error.contains("apply_defaults"));
    assert!(error.contains("event trigger `focus_started`"));
}

#[test]
fn validate_automation_trigger_rules_rejects_multi_day_time_overlap() {
    let rules = vec![
        AutomationTriggerRuleConfig {
            trigger: AutomationTriggerConditionConfig::Time {
                days: vec!["mon".to_string(), "tue".to_string()],
                at: "09:00".to_string(),
            },
            action: AutomationTriggerActionConfig::StartFocus,
        },
        AutomationTriggerRuleConfig {
            trigger: AutomationTriggerConditionConfig::Time {
                days: vec!["tuesday".to_string()],
                at: "09:00".to_string(),
            },
            action: AutomationTriggerActionConfig::DelayScheduleStart { delay_secs: 60 },
        },
    ];

    let error =
        validate_automation_trigger_rules(&rules, &[BlocklistProfileConfig::default()], &[])
            .unwrap_err();

    assert!(error.contains("rule #1"));
    assert!(error.contains("rule #2"));
    assert!(error.contains("time trigger `tue@09:00`"));
}

#[test]
fn validate_automation_trigger_rules_rejects_missing_references() {
    let rules = vec![AutomationTriggerRuleConfig {
        trigger: AutomationTriggerConditionConfig::FocusStarted,
        action: AutomationTriggerActionConfig::ApplyDefaults {
            profile: ProfileId::DeepWork,
            blocklist_profile: "Work".to_string(),
            session_template: Some("Deep Flow".to_string()),
        },
    }];

    let error =
        validate_automation_trigger_rules(&rules, &[BlocklistProfileConfig::default()], &[])
            .unwrap_err();

    assert!(error.contains("blocklist profile `Work` does not exist"));
}

#[test]
fn validate_automation_trigger_rules_accepts_distinct_valid_rules() {
    let blocklists = vec![
        BlocklistProfileConfig::default(),
        BlocklistProfileConfig {
            name: "Work".to_string(),
            sites: Vec::new(),
            allowlist_sites: Vec::new(),
            ..BlocklistProfileConfig::default()
        },
    ];
    let templates = vec![SessionTemplateConfig {
        name: "Deep Flow".to_string(),
        task_label: "Docs".to_string(),
        profile: ProfileId::DeepWork,
        blocklist_profile: "Work".to_string(),
        schedule: RecurringScheduleConfig::default(),
    }];
    let rules = vec![
        AutomationTriggerRuleConfig {
            trigger: AutomationTriggerConditionConfig::Time {
                days: vec!["mon".to_string()],
                at: "09:00".to_string(),
            },
            action: AutomationTriggerActionConfig::ApplyDefaults {
                profile: ProfileId::DeepWork,
                blocklist_profile: "Work".to_string(),
                session_template: Some("Deep Flow".to_string()),
            },
        },
        AutomationTriggerRuleConfig {
            trigger: AutomationTriggerConditionConfig::ScheduleWindowStart,
            action: AutomationTriggerActionConfig::StartFocus,
        },
    ];

    validate_automation_trigger_rules(&rules, &blocklists, &templates).unwrap();
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
            queue_retry_delay_secs: 0,
            ..WakatimeRuntimeConfig::default()
        },
        ..AppConfig::default()
    }
    .normalize();
    assert_eq!(cfg.wakatime_runtime.retry_backoff_secs, vec![2, 5, 10]);
    assert_eq!(cfg.wakatime_runtime.queue_retry_delay_secs, 1);

    let cfg = AppConfig {
        wakatime_runtime: WakatimeRuntimeConfig {
            retry_backoff_secs: vec![0, 0, 0, 0, 0, 0, 0, 0, 5],
            ..WakatimeRuntimeConfig::default()
        },
        ..AppConfig::default()
    }
    .normalize();
    assert_eq!(cfg.wakatime_runtime.retry_backoff_secs, vec![5]);
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
        blocking_backend: BlockingBackendConfig::default(),
        selected_profile: ProfileId::Custom,
        custom_profile: Some(CustomProfileConfig {
            focus_secs: 40 * 60,
            short_break_secs: 8 * 60,
            long_break_secs: 16 * 60,
            long_break_interval: 2,
        }),
        session_templates: Vec::new(),
        selected_session_template: String::new(),
        automation_triggers: Vec::new(),
        weekday_profile_rules: Vec::new(),
        selected_theme_preset: ThemePreset::Classic,
        notifications: NotificationConfig::default(),
        auto_start: AutoStartConfig::default(),
        recurring_schedule: RecurringScheduleConfig::default(),
        schedule_runtime: ScheduleRuntimeConfig::default(),
        calendar_sync: CalendarSyncConfig::default(),
        profile_automation: None,
        strict_mode: false,
        break_glass_duration_secs: default_break_glass_duration_secs(),
        daily_goal: DailyGoalConfig::default(),
        weekly_goal: WeeklyGoalConfig::default(),
        monthly_goal: MonthlyGoalConfig::default(),
        goal_carry_over: GoalCarryOverConfig::default(),
        stats_retention: StatsRetentionConfig::default(),
        history_dashboard: HistoryDashboardConfig::default(),
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
    assert_eq!(cfg.history_dashboard, HistoryDashboardConfig::default());
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

    let session_template_profile = root
        .get("session_templates")
        .and_then(toml::Value::as_array)
        .and_then(|array| array.first())
        .and_then(toml::Value::as_table)
        .and_then(|template| template.get("profile"))
        .and_then(toml::Value::as_str);
    assert_eq!(session_template_profile, Some("advanced"));

    let weekday_rule_profile = root
        .get("weekday_profile_rules")
        .and_then(toml::Value::as_array)
        .and_then(|array| array.first())
        .and_then(toml::Value::as_table)
        .and_then(|rule| rule.get("profile"))
        .and_then(toml::Value::as_str);
    assert_eq!(weekday_rule_profile, Some("basic"));

    let automation_trigger_profile = root
        .get("automation_triggers")
        .and_then(toml::Value::as_array)
        .and_then(|array| array.first())
        .and_then(toml::Value::as_table)
        .and_then(|trigger| trigger.get("action"))
        .and_then(toml::Value::as_table)
        .and_then(|action| action.get("profile"))
        .and_then(toml::Value::as_str);
    assert_eq!(automation_trigger_profile, Some("standard"));
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

    assert_eq!(report.action, "config-doctor");
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
fn config_migration_assistant_preview_reports_changes_without_writing() {
    let temp_base = unique_temp_base("migration-preview");
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

    let report = run_config_migration_assistant_with_path(false, Some(config_path.clone()));
    let persisted = fs::read_to_string(&config_path).unwrap();
    let _ = fs::remove_dir_all(&temp_base);

    assert_eq!(report.action, "config-migrate");
    assert!(!report.applied);
    assert!(report.changed);
    assert!(report.backup_path.is_none());
    assert!(persisted.contains("schema_version = 1"));
}

#[test]
fn config_migration_assistant_apply_writes_migrated_config_and_backup() {
    let temp_base = unique_temp_base("migration-apply");
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

    let report = run_config_migration_assistant_with_path(true, Some(config_path.clone()));
    let migrated = fs::read_to_string(&config_path).unwrap();
    let backup_path = report
        .backup_path
        .clone()
        .expect("backup path should be recorded when apply succeeds");
    let backup = fs::read_to_string(&backup_path).unwrap();
    let _ = fs::remove_dir_all(&temp_base);

    assert_eq!(report.action, "config-migrate-apply");
    assert!(report.applied);
    assert!(report.changed);
    assert!(migrated.contains("schema_version = 2"));
    assert!(migrated.contains("selected_profile = \"standard\""));
    assert!(backup.contains("selected_profile = \"deep_work\""));
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
fn calendar_sync_normalization_clamps_runtime_bounds() {
    let min_bounded = CalendarSyncConfig {
        enabled: true,
        refresh_secs: 0,
        lookahead_days: 0,
        sources: Vec::new(),
    }
    .normalized();
    assert_eq!(min_bounded.refresh_secs, 300);
    assert_eq!(min_bounded.lookahead_days, 1);

    let max_bounded = CalendarSyncConfig {
        enabled: true,
        refresh_secs: u64::MAX,
        lookahead_days: u16::MAX,
        sources: Vec::new(),
    }
    .normalized();
    assert_eq!(max_bounded.refresh_secs, 86_400);
    assert_eq!(max_bounded.lookahead_days, 90);
}

#[test]
fn calendar_sync_normalization_deduplicates_sources_and_autonames_blanks() {
    let normalized = CalendarSyncConfig {
        enabled: true,
        refresh_secs: 1800,
        lookahead_days: 14,
        sources: vec![
            CalendarSourceConfig {
                name: "   ".to_string(),
                provider: CalendarProviderConfig::Ics,
                url: " https://example.com/A.ics ".to_string(),
                enabled: true,
            },
            CalendarSourceConfig {
                name: "Work".to_string(),
                provider: CalendarProviderConfig::Ics,
                url: "https://example.com/a.ics".to_string(),
                enabled: false,
            },
            CalendarSourceConfig {
                name: "".to_string(),
                provider: CalendarProviderConfig::Google,
                url: "https://example.com/a.ics".to_string(),
                enabled: true,
            },
            CalendarSourceConfig {
                name: "Ignored".to_string(),
                provider: CalendarProviderConfig::Outlook,
                url: "   ".to_string(),
                enabled: true,
            },
        ],
    }
    .normalized();

    assert_eq!(normalized.sources.len(), 2);
    assert_eq!(normalized.sources[0].name, "calendar-source-1");
    assert_eq!(normalized.sources[0].provider, CalendarProviderConfig::Ics);
    assert_eq!(normalized.sources[0].url, "https://example.com/A.ics");
    assert!(normalized.sources[0].enabled);

    assert_eq!(normalized.sources[1].name, "calendar-source-2");
    assert_eq!(
        normalized.sources[1].provider,
        CalendarProviderConfig::Google
    );
    assert_eq!(normalized.sources[1].url, "https://example.com/a.ics");
    assert!(normalized.sources[1].enabled);
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
fn normalize_deduplicates_profile_names_and_fixes_selection() {
    let cfg = AppConfig {
        blocklist_profiles: vec![
            BlocklistProfileConfig {
                name: "Work".to_string(),
                sites: vec!["a.com".to_string()],
                allowlist_sites: vec!["b.com".to_string()],
                ..BlocklistProfileConfig::default()
            },
            BlocklistProfileConfig {
                name: "work".to_string(),
                sites: vec!["b.com".to_string()],
                allowlist_sites: Vec::new(),
                ..BlocklistProfileConfig::default()
            },
        ],
        selected_blocklist_profile: "missing".to_string(),
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(cfg.blocklist_profiles[0].name, "Work");
    assert_eq!(cfg.blocklist_profiles[1].name, "work (2)");
    assert_eq!(cfg.selected_blocklist_profile, "Work");
    assert!(cfg.blocked_sites.is_empty());
}

#[test]
fn normalize_keeps_legacy_blocked_sites_empty_for_profile_only_config() {
    let cfg = AppConfig {
        blocklist_profiles: vec![BlocklistProfileConfig {
            name: "Work".to_string(),
            sites: vec!["a.com".to_string(), "b.com".to_string()],
            allowlist_sites: vec!["b.com".to_string()],
            ..BlocklistProfileConfig::default()
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
fn normalize_keeps_legacy_blocked_sites_when_profiles_exist() {
    let cfg = AppConfig {
        blocked_sites: vec!["legacy-only.com".to_string()],
        blocklist_profiles: vec![BlocklistProfileConfig {
            name: "Work".to_string(),
            sites: vec!["a.com".to_string(), "b.com".to_string()],
            allowlist_sites: vec!["b.com".to_string()],
            ..BlocklistProfileConfig::default()
        }],
        selected_blocklist_profile: "Work".to_string(),
        ..AppConfig::default()
    }
    .normalize();

    assert_eq!(cfg.selected_blocklist_profile, "Work");
    assert_eq!(cfg.blocked_sites, vec!["legacy-only.com".to_string()]);
}

#[test]
fn normalize_merges_legacy_profile_lists_when_categories_exist() {
    let cfg = AppConfig {
        blocklist_profiles: vec![BlocklistProfileConfig {
            name: "Work".to_string(),
            sites: vec!["legacy.com".to_string()],
            allowlist_sites: vec!["legacy-allow.com".to_string()],
            categories: vec![BlocklistCategoryConfig {
                name: "Social".to_string(),
                sites: vec!["youtube.com".to_string()],
                allowlist_sites: Vec::new(),
            }],
            selected_category: "Social".to_string(),
        }],
        selected_blocklist_profile: "Work".to_string(),
        ..AppConfig::default()
    }
    .normalize();

    let profile = &cfg.blocklist_profiles[0];
    assert!(profile.sites.contains(&"youtube.com".to_string()));
    assert!(profile.sites.contains(&"legacy.com".to_string()));
    assert!(
        profile
            .allowlist_sites
            .contains(&"legacy-allow.com".to_string())
    );
    assert!(profile.categories.iter().any(|category| {
        category.name.eq_ignore_ascii_case("Social")
            && category
                .sites
                .iter()
                .any(|site| site.eq_ignore_ascii_case("youtube.com"))
    }));
    assert!(profile.categories.iter().any(|category| {
        category.name.eq_ignore_ascii_case("General")
            && category
                .sites
                .iter()
                .any(|site| site.eq_ignore_ascii_case("legacy.com"))
            && category
                .allowlist_sites
                .iter()
                .any(|site| site.eq_ignore_ascii_case("legacy-allow.com"))
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
fn feature_flags_default_enables_wakatime_integration() {
    let flags = FeatureFlagsConfig::default();
    assert!(flags.integrations.is_enabled("wakatime"));
    assert_eq!(flags.integrations.enabled, vec!["wakatime".to_string()]);
}

#[test]
fn feature_flags_normalization_deduplicates_and_trims_integration_names() {
    let normalized = FeatureFlagsConfig {
        integrations: IntegrationFeatureFlagsConfig {
            enabled: vec![
                "WakaTime".to_string(),
                "  wakatime ".to_string(),
                "custom".to_string(),
                "".to_string(),
            ],
        },
    }
    .normalized();

    assert_eq!(
        normalized.integrations.enabled,
        vec!["wakatime".to_string(), "custom".to_string()]
    );
}
