use chrono::{Local, TimeZone};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::app::{
    App, AppMode, DailyGoalProgress, PROFILE_EDIT_FIELD_LABELS, PROFILE_IDS, SetupCheck,
    SetupCheckLevel, SiteFeedbackLevel, SiteInputMode,
};
use crate::timer::{TimerPhase, TimerStatus};
use crate::wakatime::WakatimeRuntimeState;

pub fn render(frame: &mut Frame, app: &App) {
    match app.mode {
        AppMode::Timer => render_timer(frame, app),
        AppMode::SiteManager => render_site_manager(frame, app),
        AppMode::ProfileManager => render_profile_manager(frame, app),
        AppMode::StatsHistory => render_stats_history(frame, app),
        AppMode::SetupDiagnostics => render_setup_diagnostics(frame, app),
    }
}

fn render_timer(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let outer = centered_rect(72, 72, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" focustime ")
        .title_alignment(Alignment::Center)
        .style(Style::default().fg(phase_color(app.timer.phase)));
    frame.render_widget(block, outer);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(2), // phase + pomodoro count
            Constraint::Length(3), // MM:SS
            Constraint::Length(1), // active profile
            Constraint::Length(3), // progress bar
            Constraint::Length(2), // status
            Constraint::Length(1), // latest phase notification
            Constraint::Length(1), // stats summary
            Constraint::Length(1), // daily goal progress
            Constraint::Length(1), // streak summary
            Constraint::Length(1), // wakatime status
            Constraint::Min(0),    // spacer
            Constraint::Length(2), // key hints
        ])
        .split(outer);

    render_timer_phase_header(frame, app, inner[0]);
    render_timer_countdown(frame, app, inner[1]);
    render_timer_profile(frame, app, inner[2]);
    render_timer_progress_bar(frame, app, inner[3]);
    render_timer_status(frame, app, inner[4]);
    render_timer_phase_notice(frame, app, inner[5]);
    render_timer_stats_summary(frame, app, inner[6]);
    render_timer_goal_summary(frame, app, inner[7]);
    render_timer_streak_summary(frame, app, inner[8]);
    render_timer_wakatime_status(frame, app, inner[9]);
    render_timer_hints(frame, app, inner[11]);
}

fn render_timer_phase_header(frame: &mut Frame, app: &App, area: Rect) {
    let phase_text = format!(
        "{}   🍅 ×{}",
        app.timer.phase.label(),
        app.timer.pomodoros_completed
    );
    let phase_widget = Paragraph::new(phase_text)
        .alignment(Alignment::Center)
        .style(
            Style::default()
                .fg(phase_color(app.timer.phase))
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(phase_widget, area);
}

fn render_timer_countdown(frame: &mut Frame, app: &App, area: Rect) {
    let remaining = app.timer.remaining_secs;
    let mins = remaining / 60;
    let secs = remaining % 60;
    let time_text = format!("{:02}:{:02}", mins, secs);
    let time_widget = Paragraph::new(time_text)
        .alignment(Alignment::Center)
        .style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(time_widget, area);
}

fn render_timer_profile(frame: &mut Frame, app: &App, area: Rect) {
    let profile_text = format!(
        "Profile: {} ({})",
        app.selected_profile_name(),
        app.profile_summary(app.selected_profile)
    );
    let profile_widget = Paragraph::new(profile_text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(profile_widget, area);
}

fn render_timer_progress_bar(frame: &mut Frame, app: &App, area: Rect) {
    let elapsed_ratio = 1.0 - app.timer.progress();
    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::NONE))
        .gauge_style(
            Style::default()
                .fg(phase_color(app.timer.phase))
                .bg(Color::DarkGray),
        )
        .ratio(elapsed_ratio);
    frame.render_widget(gauge, area);
}

fn render_timer_status(frame: &mut Frame, app: &App, area: Rect) {
    let (status_text, strict_status_text) = timer_status_text(app);
    let status_widget = Paragraph::new(vec![
        Line::from(status_text),
        Line::from(strict_status_text),
    ])
    .alignment(Alignment::Center)
    .style(Style::default().fg(Color::Gray));
    frame.render_widget(status_widget, area);
}

fn timer_status_text(app: &App) -> (&'static str, &'static str) {
    let status_text = match app.timer.status {
        TimerStatus::Running => "▶  Running",
        TimerStatus::Paused => "⏸  Paused",
        TimerStatus::Idle => "⏹  Idle",
    };
    let strict_text = if app.strict_reset_confirmation_pending() {
        "🔒 Strict mode: press [s] again to confirm stop/reset"
    } else if app.strict_mode_enforced_for_focus() {
        "🔒 Strict mode active: skip locked, stop requires confirmation"
    } else if app.strict_mode {
        "🔒 Strict mode armed: enforced during active focus only"
    } else {
        "🔓 Strict mode off"
    };
    (status_text, strict_text)
}

fn render_timer_phase_notice(frame: &mut Frame, app: &App, area: Rect) {
    let (text, style) = phase_notice_line(app);
    let widget = Paragraph::new(text)
        .alignment(Alignment::Center)
        .style(style);
    frame.render_widget(widget, area);
}

fn phase_notice_line(app: &App) -> (String, Style) {
    if let Some(message) = app.phase_notification.as_ref() {
        (format!("🔔 {message}"), Style::default().fg(Color::Yellow))
    } else {
        (
            "🔔 Waiting for next completed phase".to_string(),
            Style::default().fg(Color::DarkGray),
        )
    }
}

fn render_timer_stats_summary(frame: &mut Frame, app: &App, area: Rect) {
    let (text, style) = timer_stats_line(app);
    let stats_widget = Paragraph::new(text)
        .alignment(Alignment::Center)
        .style(style);
    frame.render_widget(stats_widget, area);
}

fn timer_stats_line(app: &App) -> (String, Style) {
    if let Some(err) = app.stats_error.as_ref() {
        (
            format!("⚠ Stats persistence warning: {err}"),
            Style::default().fg(Color::Yellow),
        )
    } else {
        let session_stats = app.session_stats();
        let today_stats = app.today_stats();
        (
            format!(
                "Session: 🍅{} · {}m   Today: 🍅{} · {}m",
                session_stats.pomodoros_completed,
                session_stats.focused_minutes(),
                today_stats.pomodoros_completed,
                today_stats.focused_minutes()
            ),
            Style::default().fg(Color::DarkGray),
        )
    }
}

fn render_timer_goal_summary(frame: &mut Frame, app: &App, area: Rect) {
    let goal_progress = app.today_goal_progress();
    let goal_line = if goal_progress.has_any_target() {
        format_goal_progress_line(goal_progress)
    } else {
        "Goal: Off ([p] Profiles -> [e] Edit)".to_string()
    };
    let goal_widget = Paragraph::new(goal_line)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(goal_widget, area);
}

fn render_timer_streak_summary(frame: &mut Frame, app: &App, area: Rect) {
    let streak_widget = Paragraph::new(format_streak_summary_line(app))
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(streak_widget, area);
}

fn render_timer_wakatime_status(frame: &mut Frame, app: &App, area: Rect) {
    let (waka_text, waka_color) = wakatime_status_line(app);
    let waka_widget = Paragraph::new(waka_text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(waka_color));
    frame.render_widget(waka_widget, area);
}

fn wakatime_status_line(app: &App) -> (String, Color) {
    let runtime_state = app.wakatime.runtime_state();
    let (status_text, status_color) = match &runtime_state {
        WakatimeRuntimeState::NotConfigured => {
            ("⏱ WakaTime: not configured".to_string(), Color::DarkGray)
        }
        WakatimeRuntimeState::Idle => ("⏱ WakaTime: idle".to_string(), Color::DarkGray),
        WakatimeRuntimeState::Tracking => ("⏱ WakaTime: tracking".to_string(), Color::Green),
        WakatimeRuntimeState::Sending => {
            ("⏱ WakaTime: sending heartbeat...".to_string(), Color::Cyan)
        }
        WakatimeRuntimeState::Retrying {
            attempt,
            max_attempts,
            next_backoff_secs,
            error,
        } => (
            format!(
                "⏱ WakaTime: retrying ({attempt}/{max_attempts}) in {next_backoff_secs}s ({error})"
            ),
            Color::Yellow,
        ),
        WakatimeRuntimeState::Error(error) => (format!("⏱ WakaTime: error ({error})"), Color::Red),
    };

    if matches!(runtime_state, WakatimeRuntimeState::NotConfigured) {
        return (status_text, status_color);
    }

    (
        format!("{status_text} · {}", wakatime_last_success_text(app)),
        status_color,
    )
}

fn wakatime_last_success_text(app: &App) -> String {
    match app.wakatime.last_successful_heartbeat_epoch_secs() {
        Some(epoch_secs) => format!(
            "last success {}",
            format_wakatime_heartbeat_timestamp(epoch_secs)
        ),
        None => "last success not yet sent".to_string(),
    }
}

fn format_wakatime_heartbeat_timestamp(epoch_secs: u64) -> String {
    i64::try_from(epoch_secs)
        .ok()
        .and_then(|secs| Local.timestamp_opt(secs, 0).single())
        .map(|datetime| datetime.format("%H:%M:%S").to_string())
        .unwrap_or_else(|| epoch_secs.to_string())
}

fn render_timer_hints(frame: &mut Frame, app: &App, area: Rect) {
    let hints_widget = Paragraph::new(vec![
        Line::from(timer_primary_hint(app)),
        Line::from(timer_secondary_hint(app)),
    ])
    .alignment(Alignment::Center)
    .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hints_widget, area);
}

fn timer_primary_hint(app: &App) -> &'static str {
    if app.strict_reset_confirmation_pending() {
        "Timer: [Space] Run/Pause  [s] Confirm Stop/Reset  [n] Next (Locked)"
    } else if app.strict_mode_enforced_for_focus() {
        "Timer: [Space] Run/Pause  [s] Stop/Reset (Confirm)  [n] Next (Locked)"
    } else {
        "Timer: [Space] Run/Pause  [s] Stop  [n] Next"
    }
}

fn timer_secondary_hint(app: &App) -> &'static str {
    if app.strict_mode_enforced_for_focus() {
        "Views: [h] History  [p] Profiles (Locked)  [b] Sites  [d] Setup  [q/Esc] Quit (Locked)"
    } else {
        "Views: [h] History  [p] Profiles  [b] Sites  [d] Setup  [q/Esc] Quit"
    }
}

fn render_site_manager(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let outer = centered_rect(60, 80, area);

    let block_color = if app.blocker.is_blocking {
        Color::Red
    } else {
        Color::Green
    };

    let title = if app.blocker.is_blocking {
        " 🚫 Site Blocking – ACTIVE "
    } else {
        " 🌐 Site Blocking – Inactive "
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_alignment(Alignment::Center)
        .style(Style::default().fg(block_color));
    frame.render_widget(block, outer);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(1), // status line
            Constraint::Length(1), // DoH warning
            Constraint::Length(1), // spacer
            Constraint::Min(3),    // site list
            Constraint::Length(1), // spacer
            Constraint::Length(3), // input area
            Constraint::Length(1), // error line
            Constraint::Length(1), // spacer
            Constraint::Length(2), // key hints
        ])
        .split(outer);

    // Blocking status — derive the message from both the blocker flag and the
    // current timer phase/status so the copy is accurate in all states.
    let focus_session_active =
        app.timer.phase == TimerPhase::Focus && app.timer.status != TimerStatus::Idle;
    let status_text = if app.blocker.is_blocking {
        Span::styled(
            "Blocking is ACTIVE during this focus session",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )
    } else if focus_session_active {
        // Focus session is running/paused but blocking is not active
        // (empty site list or a permission error prevented it).
        Span::styled(
            "Focus session active — blocking inactive (no sites or permission error)",
            Style::default().fg(Color::Yellow),
        )
    } else {
        Span::styled(
            "Blocking will activate when a focus session starts",
            Style::default().fg(Color::Gray),
        )
    };
    frame.render_widget(
        Paragraph::new(Line::from(status_text)).alignment(Alignment::Center),
        inner[0],
    );

    // DoH warning
    let doh_warning =
        Paragraph::new("⚠ Disable DNS-over-HTTPS in your browser for blocking to work")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Yellow));
    frame.render_widget(doh_warning, inner[1]);

    let input_mode = app.site_input_mode();

    // Site list
    let list_title = format!(" Blocked Sites ({}) ", app.blocker.sites.len());
    let list_block = Block::default()
        .borders(Borders::ALL)
        .title(list_title)
        .style(Style::default().fg(Color::Gray));

    if app.blocker.sites.is_empty() {
        let empty = Paragraph::new("  No sites blocked yet. Press [a] to add one.")
            .style(Style::default().fg(Color::DarkGray))
            .block(list_block);
        frame.render_widget(empty, inner[3]);
    } else {
        let items: Vec<ListItem> = app
            .blocker
            .sites
            .iter()
            .map(|s| ListItem::new(format!("  {s}")))
            .collect();

        let list = List::new(items)
            .block(list_block)
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        let mut list_state = ListState::default();
        list_state.select(Some(app.selected_site));
        frame.render_stateful_widget(list, inner[3], &mut list_state);
    }

    // Input area
    let input_title = match input_mode {
        SiteInputMode::Add => " Add / Import Sites ",
        SiteInputMode::Edit => " Edit Site ",
    };
    let input_block = Block::default()
        .borders(Borders::ALL)
        .title(input_title)
        .style(if app.site_input_active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        });

    let input_text = if app.site_input_active {
        format!("{}_", app.site_input)
    } else {
        "Press [a] to add/import (comma/newline) or [e] to edit selected".to_string()
    };
    let input_widget =
        Paragraph::new(input_text)
            .block(input_block)
            .style(if app.site_input_active {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            });
    frame.render_widget(input_widget, inner[5]);

    // Error line
    if let Some(err) = app.block_error.as_ref() {
        let privilege_hint = if cfg!(target_os = "windows") {
            " (try running in an elevated command prompt)"
        } else {
            " (try running with elevated privileges)"
        };
        render_centered_error(frame, inner[6], format!("⚠  {err}{privilege_hint}"));
    } else if let Some(err) = app.config_error.as_ref() {
        render_centered_error(frame, inner[6], format!("⚠  {err}"));
    } else if let Some(feedback) = app.site_feedback.as_ref() {
        let (prefix, color) = match feedback.level {
            SiteFeedbackLevel::Success => ("✓", Color::Green),
            SiteFeedbackLevel::Warning => ("⚠", Color::Yellow),
        };
        let feedback_widget = Paragraph::new(format!("{prefix}  {}", feedback.message))
            .alignment(Alignment::Center)
            .style(Style::default().fg(color));
        frame.render_widget(feedback_widget, inner[6]);
    }

    // Key hints
    let hint_lines = if app.site_input_active {
        vec![
            Line::from(match input_mode {
                SiteInputMode::Add => "Input: [Enter] Add/Import  [Esc] Cancel",
                SiteInputMode::Edit => "Input: [Enter] Save  [Esc] Cancel",
            }),
            Line::from(match input_mode {
                SiteInputMode::Add => "Tip: paste comma/newline hostnames, then press [Enter]",
                SiteInputMode::Edit => "Tip: enter one hostname, then press [Enter]",
            }),
        ]
    } else if app.strict_mode_enforced_for_focus() {
        vec![
            Line::from("Sites: [a] Add  [e] Edit  [d/Del] Remove  [↑/↓] Move"),
            Line::from("View: [b/Esc] Back  [q] Quit (Locked)"),
        ]
    } else {
        vec![
            Line::from("Sites: [a] Add  [e] Edit  [d/Del] Remove  [↑/↓] Move"),
            Line::from("View: [b/Esc] Back  [q] Quit"),
        ]
    };
    let hints_widget = Paragraph::new(hint_lines)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hints_widget, inner[8]);
}

fn render_profile_manager(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let outer = centered_rect(70, 80, area);
    let profile_editor_height = PROFILE_EDIT_FIELD_LABELS.len() as u16 + 2;

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Pomodoro Profiles ")
        .title_alignment(Alignment::Center)
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(block, outer);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(1), // current profile
            Constraint::Length(1), // spacer
            Constraint::Length(7), // profile list
            Constraint::Length(1), // spacer
            Constraint::Length(profile_editor_height),
            Constraint::Min(0),    // spacer
            Constraint::Length(1), // error line
            Constraint::Length(2), // key hints
        ])
        .split(outer);

    let current = Paragraph::new(format!("Current profile: {}", app.selected_profile_name()))
        .style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(current, inner[0]);

    let items: Vec<ListItem> = PROFILE_IDS
        .iter()
        .map(|profile| {
            let marker = if *profile == app.selected_profile {
                "✓"
            } else {
                " "
            };
            let summary = app.profile_summary(*profile);
            ListItem::new(format!(" {} {}  {}", marker, profile.label(), summary))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Profiles "))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");
    let mut list_state = ListState::default();
    list_state.select(Some(app.profile_selection_index.min(PROFILE_IDS.len() - 1)));
    frame.render_stateful_widget(list, inner[2], &mut list_state);

    let editor_title = if app.profile_edit_active {
        " Custom + notification + auto-start settings editor "
    } else {
        " Custom + notification + auto-start settings ([e] to edit) "
    };
    let editor_block = Block::default()
        .borders(Borders::ALL)
        .title(editor_title)
        .style(if app.profile_edit_active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        });

    let mut lines = Vec::with_capacity(PROFILE_EDIT_FIELD_LABELS.len());
    for (index, label) in PROFILE_EDIT_FIELD_LABELS.iter().enumerate() {
        let value = app.profile_edit_field_value(index);
        let mut line = Line::from(format!("{label:<18} {value}"));
        if app.profile_edit_active && index == app.profile_edit_field {
            line = Line::from(vec![
                Span::styled("> ", Style::default().fg(Color::Yellow)),
                Span::styled(
                    format!("{label:<18} {value}"),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            ]);
        }
        lines.push(line);
    }
    frame.render_widget(Paragraph::new(lines).block(editor_block), inner[4]);

    if let Some(err) = app.config_error.as_ref() {
        render_centered_error(frame, inner[6], format!("⚠  {err}"));
    }

    let hints = if app.profile_edit_active {
        vec![
            Line::from("Edit: [↑/↓] Field  [←/→] Change value"),
            Line::from(if app.strict_mode_enforced_for_focus() {
                "[Enter] Save  [Esc] Cancel  [q/Ctrl-C] Quit (Locked)"
            } else {
                "[Enter] Save  [Esc] Cancel  [q/Ctrl-C] Quit"
            }),
        ]
    } else {
        vec![
            Line::from(if app.strict_mode_enforced_for_focus() {
                "Profiles: [↑/↓] Move  [Enter] Apply (Locked)  [e] Edit"
            } else {
                "Profiles: [↑/↓] Move  [Enter] Apply  [e] Edit"
            }),
            Line::from(if app.strict_mode_enforced_for_focus() {
                "View: [p/Esc] Back  [q] Quit (Locked)"
            } else {
                "View: [p/Esc] Back  [q] Quit"
            }),
        ]
    };
    let hints_widget = Paragraph::new(hints)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hints_widget, inner[7]);
}

fn render_stats_history(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let outer = centered_rect(65, 80, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Daily Focus History ")
        .title_alignment(Alignment::Center)
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(block, outer);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(1), // session summary
            Constraint::Length(1), // today summary
            Constraint::Length(1), // daily goal progress
            Constraint::Length(1), // streak summary
            Constraint::Length(1), // spacer
            Constraint::Min(3),    // history list
            Constraint::Length(1), // error line
            Constraint::Length(1), // hints
        ])
        .split(outer);

    let session_stats = app.session_stats();
    let today_stats = app.today_stats();
    let session_summary = Paragraph::new(format!(
        "This session: 🍅{} · {}m",
        session_stats.pomodoros_completed,
        session_stats.focused_minutes()
    ))
    .style(
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_widget(session_summary, inner[0]);

    let today_summary = Paragraph::new(format!(
        "Today: 🍅{} · {}m",
        today_stats.pomodoros_completed,
        today_stats.focused_minutes()
    ))
    .style(Style::default().fg(Color::Gray));
    frame.render_widget(today_summary, inner[1]);

    let goal_progress = app.today_goal_progress();
    let goal_line = if goal_progress.has_any_target() {
        format_goal_progress_line(goal_progress)
    } else {
        "Daily goal: Off".to_string()
    };
    let goal_summary = Paragraph::new(goal_line).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(goal_summary, inner[2]);

    let streak_summary =
        Paragraph::new(format_streak_summary_line(app)).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(streak_summary, inner[3]);

    let history_items: Vec<ListItem> = app
        .recent_daily_stats(14)
        .into_iter()
        .map(|(day, stats)| {
            ListItem::new(format!(
                "  {day}   🍅{}   {}m",
                stats.pomodoros_completed,
                stats.focused_minutes()
            ))
        })
        .collect();

    if history_items.is_empty() {
        let empty = Paragraph::new("  No completed focus history yet.")
            .style(Style::default().fg(Color::DarkGray))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Recent Days "),
            );
        frame.render_widget(empty, inner[5]);
    } else {
        let list = List::new(history_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Recent Days "),
            )
            .style(Style::default().fg(Color::Gray));
        frame.render_widget(list, inner[5]);
    }

    if let Some(err) = app.stats_error.as_ref() {
        render_centered_error(frame, inner[6], format!("⚠  {err}"));
    }

    let hints = Paragraph::new(if app.strict_mode_enforced_for_focus() {
        "[h/Esc] Back  [q/Ctrl-C] Quit (Locked)"
    } else {
        "[h/Esc] Back  [q/Ctrl-C] Quit"
    })
    .alignment(Alignment::Center)
    .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hints, inner[7]);
}

fn render_setup_diagnostics(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let outer = centered_rect(72, 68, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Setup Diagnostics ")
        .title_alignment(Alignment::Center)
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(block, outer);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(1), // hosts path
            Constraint::Length(1), // spacer
            Constraint::Length(2), // blocking permissions
            Constraint::Length(2), // hosts write capability
            Constraint::Length(2), // wakatime config status
            Constraint::Min(0),    // spacer
            Constraint::Length(2), // key hints
        ])
        .split(outer);

    let hosts_path = Paragraph::new(format!(
        "Hosts file: {}",
        app.setup_diagnostics.hosts_file_path
    ))
    .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hosts_path, inner[0]);

    render_setup_check(
        frame,
        inner[2],
        "Blocking permissions",
        &app.setup_diagnostics.blocking_permissions,
    );
    render_setup_check(
        frame,
        inner[3],
        "Hosts write capability",
        &app.setup_diagnostics.hosts_write_capability,
    );
    render_setup_check(
        frame,
        inner[4],
        "WakaTime config status",
        &app.setup_diagnostics.wakatime_config,
    );

    let hints = Paragraph::new(vec![
        Line::from("Diagnostics: [r] Refresh"),
        Line::from(if app.strict_mode_enforced_for_focus() {
            "View: [d/Esc] Back  [q/Ctrl-C] Quit (Locked)"
        } else {
            "View: [d/Esc] Back  [q/Ctrl-C] Quit"
        }),
    ])
    .alignment(Alignment::Center)
    .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hints, inner[6]);
}

fn render_setup_check(frame: &mut Frame, area: Rect, label: &str, check: &SetupCheck) {
    let (icon, status_color) = match check.level {
        SetupCheckLevel::Ok => ("✓", Color::Green),
        SetupCheckLevel::Warning => ("⚠", Color::Yellow),
    };
    let line = Line::from(vec![
        Span::styled(
            format!("{icon} {label}: "),
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(check.message.as_str(), Style::default().fg(Color::Gray)),
    ]);
    frame.render_widget(Paragraph::new(line).wrap(Wrap { trim: true }), area);
}

fn format_goal_progress_line(progress: DailyGoalProgress) -> String {
    let pomodoros = format_goal_metric_progress(
        "🍅",
        progress.pomodoros.completed,
        progress.pomodoros.target,
        "",
    );
    let minutes = format_goal_metric_progress(
        "⏱",
        progress.minutes.completed,
        progress.minutes.target,
        "m",
    );
    format!("Goal: {pomodoros}   {minutes}")
}

fn format_goal_metric_progress(
    label: &str,
    completed: u64,
    target: u64,
    unit_suffix: &str,
) -> String {
    if target == 0 {
        format!("{label} Off")
    } else {
        format!("{label} {completed}/{target}{unit_suffix}")
    }
}

fn format_streak_summary_line(app: &App) -> String {
    let streak = app.goal_streak();
    let suffix = if app.today_goal_progress().has_any_target() {
        ""
    } else {
        " (goal off)"
    };
    format!(
        "Streaks: current {}d · best {}d{}",
        streak.current, streak.best, suffix
    )
}

fn phase_color(phase: TimerPhase) -> Color {
    match phase {
        TimerPhase::Focus => Color::Red,
        TimerPhase::ShortBreak => Color::Green,
        TimerPhase::LongBreak => Color::Cyan,
    }
}

fn render_centered_error(frame: &mut Frame, area: Rect, message: String) {
    let err_widget = Paragraph::new(message)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Red));
    frame.render_widget(err_widget, area);
}

/// Returns a centered rectangle of given percentage of the parent rect.
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let v_outer_top = (100 - percent_y) / 2;
    let v_outer_bottom = 100 - percent_y - v_outer_top;

    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(v_outer_top),
            Constraint::Percentage(percent_y),
            Constraint::Percentage(v_outer_bottom),
        ])
        .split(r);

    let h_outer_left = (100 - percent_x) / 2;
    let h_outer_right = 100 - percent_x - h_outer_left;

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(h_outer_left),
            Constraint::Percentage(percent_x),
            Constraint::Percentage(h_outer_right),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

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
    fn timer_secondary_hint_includes_setup_shortcut_in_strict_mode() {
        let mut app = App::default();
        app.strict_mode = true;
        app.timer.phase = TimerPhase::Focus;
        app.timer.status = TimerStatus::Running;

        assert!(timer_secondary_hint(&app).contains("[d] Setup"));
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

        terminal
            .draw(|frame| render_setup_check(frame, frame.area(), "Check", &check))
            .expect("render should succeed");

        let text = terminal_text(&terminal, width, height);
        assert!(text.contains("TAIL-END"));
    }

    #[test]
    fn setup_diagnostics_view_wraps_long_status_messages() {
        let width = 80;
        let height = 24;
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
        assert!(text.contains("WRAP-END"));
    }

    #[test]
    fn wakatime_status_line_shows_not_yet_sent_when_no_success_exists() {
        let mut app = App::default();
        app.wakatime = WakatimeTracker::new_configured_for_tests();

        let (text, color) = wakatime_status_line(&app);

        assert_eq!(text, "⏱ WakaTime: idle · last success not yet sent");
        assert_eq!(color, Color::DarkGray);
    }

    #[test]
    fn wakatime_status_line_shows_last_success_time_after_success_event() {
        let mut app = App::default();
        app.wakatime = WakatimeTracker::new_configured_for_tests();
        app.wakatime.push_sent_event_for_tests();
        app.wakatime.poll_events();

        let (text, color) = wakatime_status_line(&app);

        assert!(text.starts_with("⏱ WakaTime: idle · last success "));
        assert!(!text.contains("not yet sent"));
        assert_eq!(color, Color::DarkGray);
    }

    #[test]
    fn wakatime_status_line_for_not_configured_omits_last_success_suffix() {
        let mut app = App::default();
        app.wakatime = WakatimeTracker::new_unconfigured_for_tests();

        let (text, color) = wakatime_status_line(&app);

        assert_eq!(text, "⏱ WakaTime: not configured");
        assert_eq!(color, Color::DarkGray);
    }
}
