use crate::config::{
    AppConfig, HistoryDashboardConfig, HistoryKpiCardId, ShortcutConfig, ThemePreset,
};
use crate::ui::*;
use chrono::{Datelike, Duration, NaiveDate};
use ratatui::style::Color;
use ratatui::{Terminal, backend::TestBackend};

use crate::stats::current_day_key;
use crate::wakatime::WakatimeTracker;

fn terminal_text(terminal: &Terminal<TestBackend>, width: u16, height: u16) -> String {
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..height {
        for x in 0..width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    text
}

#[test]
fn timer_secondary_hint_includes_setup_shortcut() {
    let app = App::default();
    assert!(timer_secondary_hint(&app).contains("[d] Setup"));
}

#[test]
fn timer_secondary_hint_includes_planner_shortcut() {
    let app = App::default();
    assert!(timer_secondary_hint(&app).contains("[t] Planner"));
}

#[test]
fn timer_secondary_hint_uses_custom_history_shortcut() {
    let app = App::from_config_for_tests(AppConfig {
        shortcuts: ShortcutConfig {
            open_stats_history: "y".to_string(),
            ..ShortcutConfig::default()
        },
        ..AppConfig::default()
    });

    let hint = timer_secondary_hint(&app);
    assert!(hint.contains("[y] History"));
    assert!(!hint.contains("[h] History"));
}

#[test]
fn timer_primary_hint_includes_break_glass_shortcut() {
    let app = App::default();
    assert!(timer_primary_hint(&app).contains("[u] Unblock"));
}

#[test]
fn timer_primary_hint_includes_note_shortcut() {
    let app = App::default();
    assert!(timer_primary_hint(&app).contains("[m] Note"));
}

#[test]
fn app_color_uses_high_contrast_preset_mapping() {
    let app = App::from_config_for_tests(AppConfig {
        selected_theme_preset: ThemePreset::HighContrast,
        ..AppConfig::default()
    });

    assert_eq!(app_color(&app, Color::DarkGray), Color::White);
    assert_eq!(app_color(&app, Color::Cyan), Color::LightCyan);
}

#[test]
fn app_color_uses_deuteranopia_friendly_mapping() {
    let app = App::from_config_for_tests(AppConfig {
        selected_theme_preset: ThemePreset::DeuteranopiaFriendly,
        ..AppConfig::default()
    });

    assert_eq!(app_color(&app, Color::Red), Color::Blue);
    assert_eq!(app_color(&app, Color::Green), Color::Cyan);
}

#[test]
fn timer_primary_hint_marks_note_as_focus_only_when_not_editable() {
    let app = App::default();
    assert!(timer_primary_hint(&app).contains("[m] Note (Focus only)"));
}

#[test]
fn timer_hints_switch_when_note_edit_is_active() {
    let mut app = App::default();
    app.task_labels = vec!["Docs".to_string()];
    app.selected_task_label = Some("Docs".to_string());
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char(' '),
        crossterm::event::KeyModifiers::NONE,
    ));
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('m'),
        crossterm::event::KeyModifiers::NONE,
    ));

    assert_eq!(
        timer_primary_hint(&app),
        "Note: Type text  [Enter] Save  [Esc] Cancel"
    );
    assert_eq!(
        timer_secondary_hint(&app),
        "Views: shortcuts paused while editing note"
    );
    assert_eq!(timer_tertiary_hint(&app), "Note edit: [Esc] Cancel");
}

#[test]
fn timer_hints_allow_note_edit_while_focus_is_paused() {
    let mut app = App::default();
    app.task_labels = vec!["Docs".to_string()];
    app.selected_task_label = Some("Docs".to_string());
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char(' '),
        crossterm::event::KeyModifiers::NONE,
    ));
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char(' '),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(app.timer.status, TimerStatus::Paused);
    assert!(timer_primary_hint(&app).contains("[m] Note"));
    assert!(!timer_primary_hint(&app).contains("Focus only"));

    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('m'),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(
        timer_primary_hint(&app),
        "Note: Type text  [Enter] Save  [Esc] Cancel"
    );
    assert_eq!(
        timer_secondary_hint(&app),
        "Views: shortcuts paused while editing note"
    );
    assert_eq!(timer_tertiary_hint(&app), "Note edit: [Esc] Cancel");
}

#[test]
fn timer_note_hints_reflect_custom_confirm_and_cancel_shortcuts() {
    let mut app = App::from_config_for_tests(AppConfig {
        shortcuts: ShortcutConfig {
            confirm: "v".to_string(),
            cancel: "o".to_string(),
            ..ShortcutConfig::default()
        },
        ..AppConfig::default()
    });
    app.task_labels = vec!["Docs".to_string()];
    app.selected_task_label = Some("Docs".to_string());
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char(' '),
        crossterm::event::KeyModifiers::NONE,
    ));
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('m'),
        crossterm::event::KeyModifiers::NONE,
    ));

    assert_eq!(
        timer_primary_hint(&app),
        "Note: Type text  [v] Save  [o] Cancel"
    );
    assert_eq!(
        note_phase_notice_text(&app),
        "📝 Note: Docs   [v] Save   [o] Cancel"
    );
    assert_eq!(timer_tertiary_hint(&app), "Note edit: [o] Cancel");
}

#[test]
fn goal_streak_lines_show_off_when_all_goals_disabled() {
    let app = App::default();

    assert_eq!(
        format_timer_goal_streak_line(&app),
        "Goals: Off (set via [p] -> [e])   Streaks: Off"
    );
    assert_eq!(
        format_history_goal_streak_line(&app),
        "Goals: Off   Streaks: Off"
    );
}

#[test]
fn readable_goal_streak_text_normalizes_off_state() {
    assert_eq!(
        readable_goal_streak_text("Goals: Off   Streaks: Off"),
        "Goals: off | Streak: off"
    );
}

#[test]
fn history_focus_risk_line_shows_low_risk_when_goals_are_off() {
    let app = App::default();
    assert_eq!(
        format_history_focus_risk_line(&app),
        "Risk: D low 0% · W low 0% · M low 0% · S low 0%"
    );
}

#[test]
fn history_focus_risk_line_marks_alert_for_high_risk_forecast() {
    let app = App::from_config_for_tests(AppConfig {
        daily_goal: crate::config::DailyGoalConfig {
            minutes: 120,
            pomodoros: 4,
        },
        weekly_goal: crate::config::WeeklyGoalConfig {
            minutes: 600,
            pomodoros: 24,
        },
        monthly_goal: crate::config::MonthlyGoalConfig {
            minutes: 2400,
            pomodoros: 96,
        },
        ..AppConfig::default()
    });
    let line = format_history_focus_risk_line(&app);
    assert!(line.contains("Risk: D high"));
    assert!(line.contains("ALERT"));
}

#[test]
fn goal_streak_lines_render_daily_weekly_monthly_period_progress() {
    let mut app = App::default();
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('p'),
        crossterm::event::KeyModifiers::NONE,
    ));
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('e'),
        crossterm::event::KeyModifiers::NONE,
    ));
    app.profile_edit_field = PROFILE_EDIT_GROUP_GOALS[3];
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Right,
        crossterm::event::KeyModifiers::NONE,
    ));
    app.profile_edit_field = PROFILE_EDIT_GROUP_GOALS[4];
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Right,
        crossterm::event::KeyModifiers::NONE,
    ));
    app.profile_edit_field = PROFILE_EDIT_GROUP_GOALS[6];
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Right,
        crossterm::event::KeyModifiers::NONE,
    ));
    app.profile_edit_field = PROFILE_EDIT_GROUP_GOALS[7];
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Right,
        crossterm::event::KeyModifiers::NONE,
    ));
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    ));

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

    app.insert_daily_stats_for_tests(
        &today,
        crate::stats::DailyStats {
            pomodoros_completed: 1,
            focused_seconds: 5 * 60,
            goal: None,
        },
    );
    app.insert_daily_stats_for_tests(
        &weekly_day.format("%Y-%m-%d").to_string(),
        crate::stats::DailyStats {
            pomodoros_completed: 1,
            focused_seconds: 5 * 60,
            goal: None,
        },
    );
    app.insert_daily_stats_for_tests(
        &monthly_day.format("%Y-%m-%d").to_string(),
        crate::stats::DailyStats {
            pomodoros_completed: 1,
            focused_seconds: 5 * 60,
            goal: None,
        },
    );

    let streak = app.goal_streak();
    let expected = format!(
        "Goals: {} · {} · {}   Streaks: {}d current · {}d best",
        format_goal_period_progress("D", app.today_goal_progress()),
        format_goal_period_progress("W", app.current_week_goal_progress()),
        format_goal_period_progress("M", app.current_month_goal_progress()),
        streak.current,
        streak.best
    );

    assert_eq!(format_timer_goal_streak_line(&app), expected);
    assert_eq!(format_history_goal_streak_line(&app), expected);
}

#[test]
fn weekly_allocation_lines_show_off_when_weekly_goal_is_disabled() {
    let app = App::default();
    assert_eq!(
        format_history_weekly_allocation_line(&app),
        "Weekly allocation: off"
    );
    assert_eq!(
        planner_weekly_allocation_summary(&app),
        "Weekly allocation: off"
    );
}

#[test]
fn weekly_allocation_lines_show_today_targets_when_weekly_goal_is_configured() {
    let app = App::from_config_for_tests(AppConfig {
        weekly_goal: crate::config::WeeklyGoalConfig {
            minutes: 120,
            pomodoros: 4,
        },
        ..AppConfig::default()
    });
    let history_line = format_history_weekly_allocation_line(&app);
    let planner_line = planner_weekly_allocation_summary(&app);

    assert!(history_line.contains("Weekly allocation: today"));
    assert!(planner_line.contains("Weekly allocation: today"));
}

#[test]
fn task_goal_progress_summary_formats_state_and_metrics() {
    let configured = crate::stats::TaskGoalProgress {
        task_label: "Docs".to_string(),
        target: crate::stats::DailyGoalSnapshot {
            minutes: 120,
            pomodoros: 4,
        },
        pomodoros_completed: 2,
        focused_seconds: 60 * 60,
        met: false,
    };
    let configured_text = format_task_goal_progress_summary(&configured);
    assert_eq!(configured_text, "g:in");

    let off = crate::stats::TaskGoalProgress {
        task_label: "Docs".to_string(),
        target: crate::stats::DailyGoalSnapshot::default(),
        pomodoros_completed: 0,
        focused_seconds: 0,
        met: false,
    };
    assert_eq!(format_task_goal_progress_summary(&off), "g:off");

    let met = crate::stats::TaskGoalProgress {
        task_label: "Docs".to_string(),
        target: crate::stats::DailyGoalSnapshot {
            minutes: 60,
            pomodoros: 2,
        },
        pomodoros_completed: 2,
        focused_seconds: 60 * 60,
        met: true,
    };
    assert_eq!(format_task_goal_progress_summary(&met), "g:met");
}

#[test]
fn timer_primary_hint_omits_schedule_delay_shortcut() {
    let app = App::default();
    assert!(!timer_primary_hint(&app).contains("Delay 10m"));
}

#[test]
fn timer_primary_hint_shows_break_glass_confirmation_prompt() {
    let mut app = App::default();
    app.blocker.add_site("example.com".to_string());
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;

    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('u'),
        crossterm::event::KeyModifiers::NONE,
    ));

    assert!(timer_primary_hint(&app).contains("Confirm unblock"));
}

#[test]
fn timer_status_text_shows_active_break_glass_state() {
    let mut app = App::default();
    app.blocker.add_site("example.com".to_string());
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;

    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('u'),
        crossterm::event::KeyModifiers::NONE,
    ));
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('u'),
        crossterm::event::KeyModifiers::NONE,
    ));

    let (_, _, break_glass_status, _) = timer_status_text(&app);
    assert!(break_glass_status.contains("Break-glass: active"));
}

#[test]
fn timer_session_status_lines_include_active_temporary_allowlist_entries() {
    let mut app = App::default();
    let (added, refreshed) = app
        .add_temporary_allowlist_for_cli("reddit.com=120s,news.ycombinator.com=180s")
        .expect("temporary allowlist entries should be accepted");
    assert_eq!(added, 2);
    assert_eq!(refreshed, 0);

    let lines = timer_session_status_lines_for_width(&app, 80);
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Temp allowlist: 2 active"))
    );
    assert!(lines.iter().any(|line| line.contains("reddit.com")));
    assert!(
        lines
            .iter()
            .any(|line| line.contains("news.ycombinator.com"))
    );
}

#[test]
fn timer_session_status_lines_compact_temporary_allowlist_in_narrow_layouts() {
    let mut app = App::default();
    let (added, refreshed) = app
        .add_temporary_allowlist_for_cli("reddit.com=120s,news.ycombinator.com=180s")
        .expect("temporary allowlist entries should be accepted");
    assert_eq!(added, 2);
    assert_eq!(refreshed, 0);

    let lines = timer_session_status_lines_for_width(&app, 50);
    assert!(lines.iter().any(|line| line.contains("⏳ Temp: 2 active")));
    assert!(lines.iter().any(|line| line.contains("reddit.com")));
    assert!(lines.iter().any(|line| line.contains("+1 more")));
}

#[test]
fn timer_secondary_hint_includes_setup_shortcut_in_strict_mode() {
    let mut app = App::default();
    app.strict_mode = true;
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;

    assert!(timer_secondary_hint(&app).contains("[d] Setup"));
}

#[test]
fn timer_secondary_hint_includes_planner_shortcut_in_strict_mode() {
    let mut app = App::default();
    app.strict_mode = true;
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;

    assert!(timer_secondary_hint(&app).contains("[t] Planner"));
}

#[test]
fn render_setup_check_wraps_long_warning_message() {
    let width = 38;
    let height = 3;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let check = SetupCheck {
        level: SetupCheckLevel::Warning,
        message: "wrapped output should include TAIL-END".to_string(),
    };
    let app = App::default();

    terminal
        .draw(|frame| render_setup_check(frame, &app, frame.area(), "Check", &check))
        .expect("render should succeed");

    let text = terminal_text(&terminal, width, height);
    assert!(text.contains("TAIL-END"));
}

#[test]
fn setup_diagnostics_view_wraps_long_status_messages() {
    let width = 80;
    let height = 30;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let mut app = App::default();
    app.mode = AppMode::SetupDiagnostics;
    app.setup_diagnostics.blocking_permissions = SetupCheck {
        level: SetupCheckLevel::Warning,
        message: "permission denied while probing parent directory WRAP-END".to_string(),
    };

    terminal
        .draw(|frame| render(frame, &app))
        .expect("render should succeed");

    let text = terminal_text(&terminal, width, height);
    assert!(
        text.contains("WRAP-END"),
        "rendered diagnostics text:\n{text}"
    );
}

#[test]
fn setup_diagnostics_view_renders_deprecation_warnings() {
    let width = 100;
    let height = 30;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let mut app = App::default();
    app.mode = AppMode::SetupDiagnostics;
    app.setup_diagnostics.deprecation_warnings = vec![
        "Deprecated top-level timer fields are in use.".to_string(),
        "Deprecated top-level automation fields are in use.".to_string(),
    ];

    terminal
        .draw(|frame| render(frame, &app))
        .expect("render should succeed");

    let text = terminal_text(&terminal, width, height);
    assert!(text.contains("Deprecation warnings:"));
}

#[test]
fn setup_diagnostics_view_shows_truncation_indicator_for_many_warnings() {
    let width = 120;
    let height = 50;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let mut app = App::default();
    app.mode = AppMode::SetupDiagnostics;
    app.setup_diagnostics.deprecation_warnings = vec![
        "warning one".to_string(),
        "warning two".to_string(),
        "warning three".to_string(),
        "warning four".to_string(),
    ];

    terminal
        .draw(|frame| render(frame, &app))
        .expect("render should succeed");

    let text = terminal_text(&terminal, width, height);
    assert!(text.contains("warning one"));
    assert!(text.contains("+2 more"));
}

#[test]
fn setup_diagnostics_view_renders_blocking_preview_section() {
    let width = 100;
    let height = 40;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let mut app = App::default();
    app.mode = AppMode::SetupDiagnostics;
    app.blocking_preview.action = BlockingPreviewAction::Block;
    app.blocking_preview.would_change = true;
    app.blocking_preview.effective_blocked_sites_count = 2;
    app.blocking_preview.section =
        Some("# focustime-block-start\n127.0.0.1 example.com\n# focustime-block-end\n".to_string());

    terminal
        .draw(|frame| render(frame, &app))
        .expect("render should succeed");

    let text = terminal_text(&terminal, width, height);
    assert!(text.contains("action: block"));
    assert!(
        text.contains("# focustime-block-start"),
        "rendered diagnostics text:\n{text}"
    );
}

#[test]
fn site_manager_status_surfaces_permission_remediation_context() {
    let width = 120;
    let height = 30;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let mut app = App::default();
    app.mode = AppMode::SiteManager;
    app.timer.phase = TimerPhase::Focus;
    app.timer.status = TimerStatus::Running;
    app.block_error = Some("permission denied".to_string());

    terminal
        .draw(|frame| render(frame, &app))
        .expect("render should succeed");

    let text = terminal_text(&terminal, width, height);
    assert!(text.contains("blocking unavailable (permission/setup issue"));
}

#[test]
fn site_manager_renders_allowlist_mode_labels() {
    let width = 120;
    let height = 30;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let mut app = App::default();
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('b'),
        crossterm::event::KeyModifiers::NONE,
    ));
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('m'),
        crossterm::event::KeyModifiers::NONE,
    ));

    terminal
        .draw(|frame| render(frame, &app))
        .expect("render should succeed");

    let text = terminal_text(&terminal, width, height);
    assert!(text.contains("Allowlist Exceptions"));
    assert!(text.contains("Mode: [m] Toggle (Allowlist)"));
    assert!(!text.contains("Category"));
}

#[test]
fn history_view_renders_monthly_heatmap_profile_and_task_panels() {
    let width = 100;
    let height = 24;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let mut app = App::default();
    app.mode = AppMode::StatsHistory;
    let today = NaiveDate::parse_from_str(&current_day_key(), "%Y-%m-%d")
        .expect("current day key should parse as a date");
    let month_label = format_month_label(today.year(), today.month());
    let first_entry = NaiveDate::from_ymd_opt(today.year(), today.month(), 6)
        .expect("date should be valid for any month");
    let second_entry = NaiveDate::from_ymd_opt(today.year(), today.month(), 8)
        .expect("date should be valid for any month");
    app.insert_daily_stats_for_tests(
        &first_entry.format("%Y-%m-%d").to_string(),
        crate::stats::DailyStats {
            pomodoros_completed: 1,
            focused_seconds: 30 * 60,
            goal: None,
        },
    );
    app.insert_daily_stats_for_tests(
        &second_entry.format("%Y-%m-%d").to_string(),
        crate::stats::DailyStats {
            pomodoros_completed: 2,
            focused_seconds: 45 * 60,
            goal: None,
        },
    );

    terminal
        .draw(|frame| render(frame, &app))
        .expect("render should succeed");

    let text = terminal_text(&terminal, width, height);
    let text_lower = text.to_ascii_lowercase();
    assert!(text.contains("Monthly Trend"));
    assert!(text.contains(&format!("Heatmap {month_label}")));
    assert!(text_lower.contains("profile effect"));
    assert!(text.contains("Comparison: Task"));
    assert!(text.contains("Task Trends"));
    assert!(text.contains("Break-glass Audit"));
    assert!(text_lower.contains("focus score"));
    assert!(text.contains(&month_label));
}

#[test]
fn history_view_hints_include_export_shortcut() {
    let width = 100;
    let height = 24;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let mut app = App::default();
    app.mode = AppMode::StatsHistory;

    terminal
        .draw(|frame| render(frame, &app))
        .expect("render should succeed");

    let text = terminal_text(&terminal, width, height);
    assert!(text.contains("[e] Export CSV + JSON"));
}

#[test]
fn history_view_hints_describe_stable_dashboard_layout() {
    let width = 140;
    let height = 40;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let mut app = App::default();
    app.mode = AppMode::StatsHistory;

    terminal
        .draw(|frame| render(frame, &app))
        .expect("render should succeed");

    let text = terminal_text(&terminal, width, height);
    assert!(
        text.contains("Dashboard: Stable default KPI layout"),
        "{text}"
    );
    assert!(!text.contains("Toggle pin"), "{text}");
    assert!(!text.contains("Move [<]/[>]"), "{text}");
}

#[test]
fn history_view_dashboard_shows_stable_default_kpis_without_pin_markers() {
    let width = 120;
    let height = 24;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let mut app = App::from_config_for_tests(AppConfig {
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
    app.mode = AppMode::StatsHistory;

    terminal
        .draw(|frame| render(frame, &app))
        .expect("render should succeed");

    let text = terminal_text(&terminal, width, height);
    assert!(text.contains("Session"));
    assert!(text.contains("Focus score"));
    assert!(!text.contains(">*"));
    assert!(!text.contains("* Focus"));
}

#[test]
fn history_view_shows_last_interruption_summary() {
    let mut app = App::default();
    app.task_labels = vec!["Docs".to_string()];
    app.selected_task_label = Some("Docs".to_string());
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char(' '),
        crossterm::event::KeyModifiers::NONE,
    ));
    app.timer.remaining_secs = 900;
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('n'),
        crossterm::event::KeyModifiers::NONE,
    ));

    let line = format_history_interruption_line(&app);
    let event = app
        .latest_session_interruption()
        .expect("skip should record an interruption event");
    assert!(line.contains("Last interruption"));
    assert!(line.contains(event.reason.label()));
    assert!(line.contains(&format_duration_label(event.remaining_secs)));
    assert!(line.contains(event.task_label.as_deref().unwrap_or("Unlabeled")));
}

#[test]
fn timer_view_renders_selected_task_label() {
    let width = 100;
    let height = 24;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let mut app = App::default();
    app.task_labels = vec!["Docs".to_string()];
    app.selected_task_label = Some("Docs".to_string());

    terminal
        .draw(|frame| render(frame, &app))
        .expect("render should succeed");

    let text = terminal_text(&terminal, width, height);
    assert!(text.contains("Task: Docs"));
}

#[test]
fn timer_view_shows_note_unavailable_outside_active_focus() {
    let width = 100;
    let height = 24;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let app = App::default();

    terminal
        .draw(|frame| render(frame, &app))
        .expect("render should succeed");

    let text = terminal_text(&terminal, width, height);
    assert!(text.contains("Note: unavailable"));
}

#[test]
fn timer_view_renders_active_session_note() {
    let width = 100;
    let height = 24;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let mut app = App::default();
    app.task_labels = vec!["Docs".to_string()];
    app.selected_task_label = Some("Docs".to_string());
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char(' '),
        crossterm::event::KeyModifiers::NONE,
    ));

    terminal
        .draw(|frame| render(frame, &app))
        .expect("render should succeed");

    let text = terminal_text(&terminal, width, height);
    assert!(text.contains("Note: Docs"));
}

#[test]
fn timer_view_renders_note_edit_notice() {
    let width = 100;
    let height = 24;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let mut app = App::default();
    app.task_labels = vec!["Docs".to_string()];
    app.selected_task_label = Some("Docs".to_string());
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char(' '),
        crossterm::event::KeyModifiers::NONE,
    ));
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('m'),
        crossterm::event::KeyModifiers::NONE,
    ));

    terminal
        .draw(|frame| render(frame, &app))
        .expect("render should succeed");

    let text = terminal_text(&terminal, width, height);
    assert!(text.contains("[Enter] Save"));
    assert!(text.contains("Cancel"));
}

#[test]
fn timer_view_renders_schedule_status_lines() {
    let width = 100;
    let height = 24;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let app = App::default();

    terminal
        .draw(|frame| render(frame, &app))
        .expect("render should succeed");

    let text = terminal_text(&terminal, width, height);
    assert!(text.contains("Next schedule: none configured"));
    assert!(text.contains("Schedule status: off"));
}

#[test]
fn profile_editor_renders_schedule_fields() {
    let width = 120;
    let height = 80;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let mut app = App::default();
    app.mode = AppMode::ProfileManager;
    app.profile_edit_active = true;
    app.profile_edit_field = PROFILE_EDIT_GROUP_SCHEDULE[0];

    terminal
        .draw(|frame| render(frame, &app))
        .expect("render should succeed");

    let text = terminal_text(&terminal, width, height);
    assert!(text.contains("Schedule settings"));
    assert!(text.contains("Window selector"));
    assert!(text.contains("Day selector"));
    assert!(text.contains("Start time"));
    assert!(text.contains("Add/remove"));
    assert!(!text.contains("Conflict inspector"));
}

#[test]
fn session_planner_view_renders_title() {
    let width = 100;
    let height = 24;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let mut app = App::default();
    app.mode = AppMode::SessionPlanner;

    terminal
        .draw(|frame| render(frame, &app))
        .expect("render should succeed");

    let text = terminal_text(&terminal, width, height);
    assert!(text.contains("Session Planner"));
}

#[test]
fn session_planner_view_renders_label_management_hints() {
    let width = 120;
    let height = 28;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let mut app = App::default();
    app.mode = AppMode::SessionPlanner;

    terminal
        .draw(|frame| render(frame, &app))
        .expect("render should succeed");

    let text = terminal_text(&terminal, width, height);
    assert!(text.contains("Rename"));
    assert!(text.contains("Delete"));
    assert!(text.contains("Planner:"));
}

#[test]
fn session_planner_view_renders_rename_input_title() {
    let width = 120;
    let height = 28;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let mut app = App::default();
    app.mode = AppMode::SessionPlanner;
    app.planner_input_active = true;
    app.planner_input_mode = Some(PlannerInputMode::Rename);
    app.planner_input = "Docs".to_string();

    terminal
        .draw(|frame| render(frame, &app))
        .expect("render should succeed");

    let text = terminal_text(&terminal, width, height);
    assert!(text.contains("Rename task label"));
}

#[test]
fn session_planner_view_renders_favorite_and_archived_badges() {
    let width = 120;
    let height = 28;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let mut app = App::default();
    app.task_labels = vec!["Docs".to_string(), "Review".to_string()];
    app.selected_task_label = Some("Docs".to_string());

    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('t'),
        crossterm::event::KeyModifiers::NONE,
    ));
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('f'),
        crossterm::event::KeyModifiers::NONE,
    ));
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('j'),
        crossterm::event::KeyModifiers::NONE,
    ));
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('x'),
        crossterm::event::KeyModifiers::NONE,
    ));

    terminal
        .draw(|frame| render(frame, &app))
        .expect("render should succeed");

    let text = terminal_text(&terminal, width, height);
    assert!(text.contains("★"));
    assert!(text.contains("archived"));
}

#[test]
fn month_label_uses_zero_padded_iso_format() {
    assert_eq!(format_month_label(2026, 5), "2026-05");
}

#[test]
fn wakatime_status_line_shows_not_yet_sent_when_no_success_exists() {
    let mut app = App::default();
    app.replace_wakatime_tracker_for_tests(WakatimeTracker::new_configured_for_tests());

    let (text, color) = wakatime_status_line(&app);

    assert_eq!(text, "⏱  WakaTime: idle · last success not yet sent");
    assert_eq!(color, Color::DarkGray);
}

#[test]
fn wakatime_status_line_shows_last_success_time_after_success_event() {
    let mut app = App::default();
    app.replace_wakatime_tracker_for_tests(WakatimeTracker::new_configured_for_tests());
    let tracker = app
        .wakatime_tracker_mut_for_tests()
        .expect("wakatime tracker should be available");
    tracker.push_sent_event_for_tests();
    tracker.poll_events();

    let (text, color) = wakatime_status_line(&app);

    assert!(text.starts_with("⏱  WakaTime: idle · last success "));
    assert!(!text.contains("not yet sent"));
    assert_eq!(color, Color::DarkGray);
}

#[test]
fn wakatime_status_line_shows_offline_queue_backlog() {
    let mut app = App::default();
    app.replace_wakatime_tracker_for_tests(WakatimeTracker::new_configured_for_tests());
    app.wakatime_tracker_mut_for_tests()
        .expect("wakatime tracker should be available")
        .set_pending_heartbeats_for_tests(3);

    let (text, color) = wakatime_status_line(&app);

    assert_eq!(
        text,
        "⏱  WakaTime: queued offline (3 pending) · last success not yet sent"
    );
    assert_eq!(color, Color::Yellow);
}

#[test]
fn wakatime_status_line_shows_replaying_queue_backlog() {
    let mut app = App::default();
    app.replace_wakatime_tracker_for_tests(WakatimeTracker::new_configured_for_tests());
    app.wakatime_tracker_mut_for_tests()
        .expect("wakatime tracker should be available")
        .set_replaying_heartbeats_for_tests(2);

    let (text, color) = wakatime_status_line(&app);

    assert_eq!(
        text,
        "⏱  WakaTime: resending queued heartbeats (2 pending) · last success not yet sent"
    );
    assert_eq!(color, Color::Cyan);
}

#[test]
fn wakatime_status_line_shows_retrying_state() {
    let mut app = App::default();
    app.replace_wakatime_tracker_for_tests(WakatimeTracker::new_configured_for_tests());
    let tracker = app
        .wakatime_tracker_mut_for_tests()
        .expect("wakatime tracker should be available");
    tracker.push_retrying_event_for_tests(2, 4, 5, "HTTP 503");
    tracker.poll_events();

    let (text, color) = wakatime_status_line(&app);

    assert_eq!(
        text,
        "⏱  WakaTime: retrying (2/4) in 5s (HTTP 503) · last success not yet sent"
    );
    assert_eq!(color, Color::Yellow);
}

#[test]
fn wakatime_status_line_shows_error_state() {
    let mut app = App::default();
    app.replace_wakatime_tracker_for_tests(WakatimeTracker::new_configured_for_tests());
    let tracker = app
        .wakatime_tracker_mut_for_tests()
        .expect("wakatime tracker should be available");
    tracker.push_failed_event_for_tests("HTTP 500");
    tracker.poll_events();

    let (text, color) = wakatime_status_line(&app);

    assert_eq!(
        text,
        "⏱  WakaTime: error (HTTP 500) · last success not yet sent"
    );
    assert_eq!(color, Color::Red);
}

#[test]
fn wakatime_status_line_for_not_configured_omits_last_success_suffix() {
    let mut app = App::default();
    app.replace_wakatime_tracker_for_tests(WakatimeTracker::new_unconfigured_for_tests());

    let (text, color) = wakatime_status_line(&app);

    assert_eq!(text, "⏱  WakaTime: not configured");
    assert_eq!(color, Color::DarkGray);
}
