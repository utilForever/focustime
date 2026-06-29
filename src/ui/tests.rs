use crate::app::{
    App, AppMode, PROFILE_EDIT_DAILY_GOAL_MINUTES_INDEX, PROFILE_EDIT_DAILY_GOAL_POMODOROS_INDEX,
    PROFILE_EDIT_SCHEDULE_WINDOW_INDEX, PlannerInputMode, SetupCheck, SetupCheckLevel,
};
use crate::blocker::BlockingPreviewAction;
use crate::config::{
    AppConfig, HistoryDashboardConfig, HistoryKpiCardId, ShortcutConfig, ThemePreset,
};
use crate::timer::{TimerPhase, TimerStatus};
use crate::ui::{
    app_color, format_duration_label, format_goal_period_progress, format_history_focus_risk_line,
    format_history_goal_streak_line, format_history_interruption_line,
    format_history_weekly_allocation_line, format_month_label, format_timer_goal_streak_line,
    readable_goal_streak_text, render, render_setup_check, timer_primary_hint,
    timer_secondary_hint, timer_session_status_lines_for_width,
};
use chrono::{Datelike, NaiveDate};
use ratatui::style::Color;
use ratatui::{Terminal, backend::TestBackend};

use crate::stats::current_day_key;

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
    assert!(timer_secondary_hint(&app).contains("[t] Tasks"));
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
fn timer_primary_hint_omits_note_shortcut() {
    let app = App::default();
    assert!(!timer_primary_hint(&app).contains("Note"));
    assert!(!timer_primary_hint(&app).contains("Unblock"));
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
        "Risk: D low 0% · S low 0%"
    );
}

#[test]
fn history_focus_risk_line_marks_alert_for_high_risk_forecast() {
    let app = App::from_config_for_tests(AppConfig {
        daily_goal: crate::config::DailyGoalConfig {
            minutes: 120,
            pomodoros: 4,
        },
        ..AppConfig::default()
    });
    let line = format_history_focus_risk_line(&app);
    assert!(line.contains("Risk: D high"));
    assert!(line.contains("ALERT"));
}

#[test]
fn goal_streak_lines_render_daily_period_progress() {
    let mut app = App::default();
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('p'),
        crossterm::event::KeyModifiers::NONE,
    ));
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('e'),
        crossterm::event::KeyModifiers::NONE,
    ));
    app.profile_edit_field = PROFILE_EDIT_DAILY_GOAL_MINUTES_INDEX;
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Right,
        crossterm::event::KeyModifiers::NONE,
    ));
    app.profile_edit_field = PROFILE_EDIT_DAILY_GOAL_POMODOROS_INDEX;
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Right,
        crossterm::event::KeyModifiers::NONE,
    ));
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    ));

    let streak = app.goal_streak();
    let expected = format!(
        "Goal: {}   Streaks: {}d current · {}d best",
        format_goal_period_progress("D", app.today_goal_progress()),
        streak.current,
        streak.best
    );

    assert_eq!(format_timer_goal_streak_line(&app), expected);
    assert_eq!(format_history_goal_streak_line(&app), expected);
}

#[test]
fn weekly_allocation_line_shows_retired_message_when_goal_is_disabled() {
    let app = App::default();
    assert_eq!(
        format_history_weekly_allocation_line(&app),
        "Weekly allocation: retired"
    );
}

#[test]
fn weekly_allocation_line_stays_retired_when_legacy_goal_is_configured() {
    let app = App::from_config_for_tests(AppConfig {
        weekly_goal: crate::config::WeeklyGoalConfig {
            minutes: 120,
            pomodoros: 4,
        },
        ..AppConfig::default()
    });
    assert_eq!(
        format_history_weekly_allocation_line(&app),
        "Weekly allocation: retired"
    );
}

#[test]
fn timer_primary_hint_omits_schedule_delay_shortcut() {
    let app = App::default();
    assert!(!timer_primary_hint(&app).contains("Delay 10m"));
}

#[test]
fn timer_session_status_lines_include_strict_only() {
    let app = App::default();

    let lines = timer_session_status_lines_for_width(&app, 80);

    assert_eq!(lines.len(), 1);
    assert!(lines.iter().any(|line| line.contains("Strict:")));
    assert!(!lines.iter().any(|line| line.contains("allowlist")));
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

    assert!(timer_secondary_hint(&app).contains("[t] Tasks"));
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
    assert!(text.contains("Hosts-file preview"));
    assert!(!text.contains("Preview backend"));
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
    assert!(text.contains("Add Allowlist Sites"));
    assert!(!text.contains("Temporary Exceptions"));
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
    app.profile_edit_field = PROFILE_EDIT_SCHEDULE_WINDOW_INDEX;

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
    app.mode = AppMode::TaskSetup;

    terminal
        .draw(|frame| render(frame, &app))
        .expect("render should succeed");

    let text = terminal_text(&terminal, width, height);
    assert!(text.contains("Task Setup"));
}

#[test]
fn session_planner_view_renders_label_management_hints() {
    let width = 120;
    let height = 28;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let mut app = App::default();
    app.mode = AppMode::TaskSetup;

    terminal
        .draw(|frame| render(frame, &app))
        .expect("render should succeed");

    let text = terminal_text(&terminal, width, height);
    assert!(text.contains("Rename"));
    assert!(text.contains("Delete"));
    assert!(text.contains("Tasks:"));
}

#[test]
fn session_planner_view_renders_rename_input_title() {
    let width = 120;
    let height = 28;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let mut app = App::default();
    app.mode = AppMode::TaskSetup;
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
