use chrono::{Local, TimeZone};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::app::{
    App, AppMode, BlocklistProfileInputMode, DailyGoalProgress, HistoryFeedbackLevel,
    PROFILE_EDIT_FIELD_LABELS, PROFILE_IDS, PlannerFeedbackLevel, SetupCheck, SetupCheckLevel,
    SiteFeedbackLevel, SiteInputMode,
};
use crate::timer::{TimerPhase, TimerStatus};
use crate::wakatime::WakatimeRuntimeState;

pub fn render(frame: &mut Frame, app: &App) {
    match app.mode {
        AppMode::Timer => render_timer(frame, app),
        AppMode::SiteManager => render_site_manager(frame, app),
        AppMode::ProfileManager => render_profile_manager(frame, app),
        AppMode::SessionPlanner => render_session_planner(frame, app),
        AppMode::StatsHistory => render_stats_history(frame, app),
        AppMode::SetupDiagnostics => render_setup_diagnostics(frame, app),
    }
}

fn render_timer(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let outer = centered_rect(88, 90, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" focustime ")
        .title_alignment(Alignment::Center)
        .style(Style::default().fg(phase_color(app.timer.phase)));
    frame.render_widget(block, outer);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(2), // phase + pomodoro count
            Constraint::Min(10),   // body
            Constraint::Length(1), // latest phase notification
            Constraint::Length(4), // key hints
        ])
        .split(outer);

    render_timer_phase_header(frame, app, inner[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(inner[1]);
    render_timer_focus_panel(frame, app, body[0]);
    render_timer_session_panel(frame, app, body[1]);

    render_timer_phase_notice(frame, app, inner[2]);
    render_timer_hints(frame, app, inner[3]);
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

fn render_timer_focus_panel(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" ⏱  Timer ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // countdown
            Constraint::Length(1), // spacer
            Constraint::Length(5), // progress
            Constraint::Min(0),    // spacer
        ])
        .split(inner);

    render_timer_countdown(frame, app, layout[0]);
    render_timer_progress_bar(frame, app, layout[2]);
}

fn render_timer_progress_bar(frame: &mut Frame, app: &App, area: Rect) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // progress label
            Constraint::Length(1), // empty line
            Constraint::Length(3), // progress bar
        ])
        .split(area);
    frame.render_widget(
        Paragraph::new("⏳ Progress")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray)),
        layout[0],
    );

    let elapsed_ratio = 1.0 - app.timer.progress();
    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::NONE))
        .gauge_style(
            Style::default()
                .fg(phase_color(app.timer.phase))
                .bg(Color::DarkGray),
        )
        .ratio(elapsed_ratio);
    frame.render_widget(gauge, layout[2]);
}

fn render_timer_session_panel(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" 🧭 Session Overview ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let (status_text, strict_status_text) = timer_status_text(app);
    let profile_line = format!(
        "🗂  Profile: {} ({})",
        app.selected_profile_name(),
        app.profile_summary(app.selected_profile)
    );
    let (task_text, task_style) = if let Some(label) = app.current_task_label() {
        (
            format!("🎯 Task: {label}"),
            Style::default().fg(Color::LightYellow),
        )
    } else {
        (
            "🎯 Task: not selected ([t] Planner)".to_string(),
            Style::default().fg(Color::Yellow),
        )
    };
    let (stats_text, stats_style) = timer_stats_line(app);
    let goal_line = format!("🔥 {}", format_timer_goal_streak_line(app));
    let (waka_text, waka_color) = wakatime_status_line(app);

    let lines = vec![
        Line::from(""),
        Line::styled(status_text, Style::default().fg(Color::Gray)),
        Line::styled(strict_status_text, Style::default().fg(Color::DarkGray)),
        Line::from(""),
        Line::styled(profile_line, Style::default().fg(Color::DarkGray)),
        Line::styled(task_text, task_style),
        Line::from(""),
        Line::styled(format!("📈 {stats_text}"), stats_style),
        Line::styled(goal_line, Style::default().fg(Color::DarkGray)),
        Line::styled(waka_text, Style::default().fg(waka_color)),
    ];

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn timer_status_text(app: &App) -> (&'static str, &'static str) {
    let status_text = match app.timer.status {
        TimerStatus::Running => "📍 Status: ▶ Running",
        TimerStatus::Paused => "📍 Status: ⏸ Paused",
        TimerStatus::Idle => "📍 Status: ⏹ Idle",
    };
    let strict_text = if app.strict_reset_confirmation_pending() {
        "🔒 Strict mode: confirm reset with [s]"
    } else if app.strict_mode_enforced_for_focus() {
        "🔒 Strict mode: active (skip/quit locked, reset confirm)"
    } else if app.strict_mode {
        "🔒 Strict mode: armed (enforced during active focus)"
    } else {
        "🔓 Strict mode: off"
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

fn wakatime_status_line(app: &App) -> (String, Color) {
    let runtime_state = app.wakatime.runtime_state();
    let (status_text, status_color) = match &runtime_state {
        WakatimeRuntimeState::NotConfigured => {
            ("⏱  WakaTime: not configured".to_string(), Color::DarkGray)
        }
        WakatimeRuntimeState::Idle => ("⏱  WakaTime: idle".to_string(), Color::DarkGray),
        WakatimeRuntimeState::Tracking => ("⏱  WakaTime: tracking".to_string(), Color::Green),
        WakatimeRuntimeState::Sending => {
            ("⏱  WakaTime: sending heartbeat...".to_string(), Color::Cyan)
        }
        WakatimeRuntimeState::Retrying {
            attempt,
            max_attempts,
            next_backoff_secs,
            error,
        } => (
            format!(
                "⏱  WakaTime: retrying ({attempt}/{max_attempts}) in {next_backoff_secs}s ({error})"
            ),
            Color::Yellow,
        ),
        WakatimeRuntimeState::Error(error) => (format!("⏱  WakaTime: error ({error})"), Color::Red),
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
        Line::from(timer_tertiary_hint(app)),
    ])
    .alignment(Alignment::Center)
    .style(Style::default().fg(Color::DarkGray))
    .wrap(Wrap { trim: true });
    frame.render_widget(hints_widget, area);
}

fn timer_primary_hint(app: &App) -> &'static str {
    if app.strict_reset_confirmation_pending() {
        "⌨  Timer: [Space] Run/Pause  [s] Confirm reset  [n] Next (Locked)"
    } else if app.strict_mode_enforced_for_focus() {
        "⌨  Timer: [Space] Run/Pause  [s] Stop/Reset (Confirm)  [n] Next (Locked)"
    } else {
        "⌨  Timer: [Space] Run/Pause  [s] Stop/Reset  [n] Next"
    }
}

fn timer_secondary_hint(app: &App) -> &'static str {
    if app.strict_mode_enforced_for_focus() {
        "🧭 Views: [t] Planner  [h] History  [p] Profiles (Locked)  [b] Sites  [d] Setup"
    } else {
        "🧭 Views: [t] Planner  [h] History  [p] Profiles  [b] Sites  [d] Setup"
    }
}

fn timer_tertiary_hint(app: &App) -> &'static str {
    if app.strict_mode_enforced_for_focus() {
        "🚪 Quit: [q/Esc] Locked during active focus"
    } else {
        "🚪 Quit: [q/Esc]"
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
            Constraint::Length(1), // profile line
            Constraint::Length(1), // DoH warning
            Constraint::Length(1), // spacer
            Constraint::Min(3),    // site list
            Constraint::Length(1), // spacer
            Constraint::Length(3), // site input area
            Constraint::Length(3), // profile input area
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

    let profile_text = format!(
        "Profile: {} ({}/{})",
        app.active_blocklist_profile_name(),
        app.active_blocklist_profile_position(),
        app.blocklist_profile_count()
    );
    frame.render_widget(
        Paragraph::new(profile_text)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Cyan)),
        inner[1],
    );

    // DoH warning
    let doh_warning =
        Paragraph::new("⚠ Disable DNS-over-HTTPS in your browser for blocking to work")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Yellow));
    frame.render_widget(doh_warning, inner[2]);

    let input_mode = app.site_input_mode();
    let profile_input_mode = app.blocklist_profile_input_mode();

    // Site list
    let list_title = format!(
        " Blocked Sites · {} ({}) ",
        app.active_blocklist_profile_name(),
        app.blocker.sites.len()
    );
    let list_block = Block::default()
        .borders(Borders::ALL)
        .title(list_title)
        .style(Style::default().fg(Color::Gray));

    if app.blocker.sites.is_empty() {
        let empty = Paragraph::new("  No sites blocked yet. Press [a] to add one.")
            .style(Style::default().fg(Color::DarkGray))
            .block(list_block);
        frame.render_widget(empty, inner[4]);
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
        frame.render_stateful_widget(list, inner[4], &mut list_state);
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
    frame.render_widget(input_widget, inner[6]);

    let profile_input_title = match profile_input_mode {
        Some(BlocklistProfileInputMode::Create) => " New Blocklist Profile ",
        Some(BlocklistProfileInputMode::Rename) => " Rename Blocklist Profile ",
        None => " Blocklist Profiles ",
    };
    let profile_input_block = Block::default()
        .borders(Borders::ALL)
        .title(profile_input_title)
        .style(if app.blocklist_profile_input_active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        });
    let profile_input_text = if app.blocklist_profile_input_active {
        format!("{}_", app.blocklist_profile_input)
    } else {
        "Use [n] to create, [r] to rename, [x] to delete, [[ ] to switch".to_string()
    };
    frame.render_widget(
        Paragraph::new(profile_input_text)
            .block(profile_input_block)
            .style(if app.blocklist_profile_input_active {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            }),
        inner[7],
    );

    // Error line
    if let Some(err) = app.block_error.as_ref() {
        let privilege_hint = if cfg!(target_os = "windows") {
            " (try running in an elevated command prompt)"
        } else {
            " (try running with elevated privileges)"
        };
        render_centered_error(frame, inner[8], format!("⚠  {err}{privilege_hint}"));
    } else if let Some(err) = app.config_error.as_ref() {
        render_centered_error(frame, inner[8], format!("⚠  {err}"));
    } else if let Some(feedback) = app.site_feedback.as_ref() {
        let (prefix, color) = match feedback.level {
            SiteFeedbackLevel::Success => ("✓", Color::Green),
            SiteFeedbackLevel::Warning => ("⚠", Color::Yellow),
        };
        let feedback_widget = Paragraph::new(format!("{prefix}  {}", feedback.message))
            .alignment(Alignment::Center)
            .style(Style::default().fg(color));
        frame.render_widget(feedback_widget, inner[8]);
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
    } else if app.blocklist_profile_input_active {
        vec![
            Line::from("Profile: [Enter] Save  [Esc] Cancel"),
            Line::from("Tip: use descriptive names like Work, Study, or Deep Work"),
        ]
    } else if app.strict_mode_enforced_for_focus() {
        vec![
            Line::from("Sites: [a] Add  [e] Edit  [d/Del] Remove  [↑/↓] Move"),
            Line::from(
                "Profiles: [[ ] Switch  [n] New  [r] Rename  [x] Delete  [b/Esc] Back  [q] Quit (Locked)",
            ),
        ]
    } else {
        vec![
            Line::from("Sites: [a] Add  [e] Edit  [d/Del] Remove  [↑/↓] Move"),
            Line::from("Profiles: [[ ] Switch  [n] New  [r] Rename  [x] Delete  [b/Esc] Back"),
        ]
    };
    let hints_widget = Paragraph::new(hint_lines)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hints_widget, inner[10]);
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

fn render_session_planner(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let outer = centered_rect(66, 82, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Session Planner ")
        .title_alignment(Alignment::Center)
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(block, outer);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(2), // current task
            Constraint::Length(1), // spacer
            Constraint::Min(5),    // task labels list
            Constraint::Length(3), // add input
            Constraint::Length(2), // feedback
            Constraint::Length(3), // hints
        ])
        .split(outer);

    render_session_planner_selected_task(frame, app, inner[0]);
    render_session_planner_labels(frame, app, inner[2]);
    render_session_planner_input(frame, app, inner[3]);
    render_session_planner_feedback(frame, app, inner[4]);
    render_session_planner_hints(frame, app, inner[5]);
}

fn render_session_planner_selected_task(frame: &mut Frame, app: &App, area: Rect) {
    let selected_text = app.selected_task_label.as_ref().map_or_else(
        || {
            vec![
                Line::from("Selected task:"),
                Line::from("  none (required before focus starts)"),
            ]
        },
        |label| {
            vec![
                Line::from("Selected task:"),
                Line::from(format!("  {label}")),
            ]
        },
    );
    frame.render_widget(
        Paragraph::new(selected_text)
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_session_planner_labels(frame: &mut Frame, app: &App, area: Rect) {
    if app.task_labels.is_empty() {
        frame.render_widget(
            Paragraph::new("No task labels yet. Press [a] to add one.")
                .style(Style::default().fg(Color::DarkGray))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Task Labels "),
                ),
            area,
        );
        return;
    }

    let items: Vec<ListItem> = app
        .task_labels
        .iter()
        .map(|label| {
            let marker = if app
                .selected_task_label
                .as_ref()
                .is_some_and(|selected| selected.eq_ignore_ascii_case(label))
            {
                "✓"
            } else {
                " "
            };
            ListItem::new(format!(" {marker} {label}"))
        })
        .collect();
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Task Labels "),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");
    let mut state = ListState::default();
    state.select(Some(
        app.planner_selection_index
            .min(app.task_labels.len().saturating_sub(1)),
    ));
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_session_planner_input(frame: &mut Frame, app: &App, area: Rect) {
    let input_title = if app.planner_input_active {
        " Add task label "
    } else {
        " Add task label ([a] to type) "
    };
    let input_text = if app.planner_input_active {
        format!("{}|", app.planner_input)
    } else {
        "Type a label, then press [Enter] to add and select".to_string()
    };
    frame.render_widget(
        Paragraph::new(input_text)
            .style(if app.planner_input_active {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(Color::DarkGray)
            })
            .block(Block::default().borders(Borders::ALL).title(input_title)),
        area,
    );
}

fn render_session_planner_feedback(frame: &mut Frame, app: &App, area: Rect) {
    if let Some(feedback) = app.planner_feedback.as_ref() {
        let (prefix, color) = match feedback.level {
            PlannerFeedbackLevel::Success => ("✓", Color::Green),
            PlannerFeedbackLevel::Warning => ("⚠", Color::Yellow),
        };
        frame.render_widget(
            Paragraph::new(format!("{prefix}  {}", feedback.message))
                .style(Style::default().fg(color))
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true }),
            area,
        );
    }
}

fn render_session_planner_hints(frame: &mut Frame, app: &App, area: Rect) {
    let hints = if app.planner_input_active {
        vec![
            Line::from("Input: type task label, then [Enter]"),
            Line::from("Input: [Esc] Cancel"),
            Line::from(if app.strict_mode_enforced_for_focus() {
                "View: [q/Ctrl-C] Quit (Locked)"
            } else {
                "View: [q/Ctrl-C] Quit"
            }),
        ]
    } else {
        vec![
            Line::from("Planner: [↑/↓] Move"),
            Line::from("Planner: [Enter] Select  [a] Add label"),
            Line::from(if app.strict_mode_enforced_for_focus() {
                "View: [t/Esc] Back  [q/Ctrl-C] Quit (Locked)"
            } else {
                "View: [t/Esc] Back  [q/Ctrl-C] Quit"
            }),
        ]
    };
    frame.render_widget(
        Paragraph::new(hints)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray)),
        area,
    );
}

fn render_stats_history(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let outer = centered_rect(65, 80, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Focus History ")
        .title_alignment(Alignment::Center)
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(block, outer);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(1), // session + today summary
            Constraint::Length(1), // goal + streak summary
            Constraint::Length(1), // spacer
            Constraint::Min(3),    // history list
            Constraint::Length(2), // status line
            Constraint::Length(2), // hints
        ])
        .split(outer);

    let session_stats = app.session_stats();
    let today_stats = app.today_stats();
    let session_summary = Paragraph::new(format!(
        "This session: 🍅{} · {}m   Today: 🍅{} · {}m",
        session_stats.pomodoros_completed,
        session_stats.focused_minutes(),
        today_stats.pomodoros_completed,
        today_stats.focused_minutes()
    ))
    .style(
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_widget(session_summary, inner[0]);

    let goal_summary = Paragraph::new(format_history_goal_streak_line(app))
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(goal_summary, inner[1]);

    let history_sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(inner[3]);

    let weekly_items: Vec<ListItem> = app
        .recent_weekly_stats(6)
        .into_iter()
        .map(|stats| {
            ListItem::new(format!(
                "  {}   🍅{}   {}m",
                format_week_label(stats.year, stats.week),
                stats.pomodoros_completed,
                stats.focused_minutes()
            ))
        })
        .collect();
    render_history_panel(
        frame,
        history_sections[0],
        " Weekly Totals ",
        weekly_items,
        "  No weekly totals yet.",
    );

    let history_items: Vec<ListItem> = app
        .recent_daily_stats(10)
        .into_iter()
        .map(|(day, stats)| {
            ListItem::new(format!(
                "  {day}   🍅{}   {}m",
                stats.pomodoros_completed,
                stats.focused_minutes()
            ))
        })
        .collect();
    render_history_panel(
        frame,
        history_sections[1],
        " Recent Days ",
        history_items,
        "  No completed focus history yet.",
    );

    if let Some(feedback) = app.history_feedback.as_ref() {
        let (prefix, color) = match feedback.level {
            HistoryFeedbackLevel::Success => ("✓", Color::Green),
            HistoryFeedbackLevel::Warning => ("⚠", Color::Yellow),
        };
        let message = format!("{prefix}  {}", feedback.message);
        let feedback_widget = Paragraph::new(message)
            .alignment(Alignment::Center)
            .style(Style::default().fg(color))
            .wrap(Wrap { trim: true });
        frame.render_widget(feedback_widget, inner[4]);
    } else if let Some(err) = app.stats_error.as_ref() {
        let error_widget = Paragraph::new(format!("⚠  {err}"))
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Red))
            .wrap(Wrap { trim: true });
        frame.render_widget(error_widget, inner[4]);
    }

    let hints = Paragraph::new(vec![
        Line::from("History: [e] Export CSV + JSON"),
        Line::from(if app.strict_mode_enforced_for_focus() {
            "View: [h/Esc] Back  [q/Ctrl-C] Quit (Locked)"
        } else {
            "View: [h/Esc] Back  [q/Ctrl-C] Quit"
        }),
    ])
    .alignment(Alignment::Center)
    .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hints, inner[5]);
}

fn render_history_panel(
    frame: &mut Frame,
    area: Rect,
    title: &'static str,
    items: Vec<ListItem>,
    empty_message: &'static str,
) {
    if items.is_empty() {
        let empty = Paragraph::new(empty_message)
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL).title(title));
        frame.render_widget(empty, area);
    } else {
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(title))
            .style(Style::default().fg(Color::Gray));
        frame.render_widget(list, area);
    }
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

fn format_timer_goal_streak_line(app: &App) -> String {
    let goal_progress = app.today_goal_progress();
    let streak = app.goal_streak();
    if goal_progress.has_any_target() {
        format!(
            "{}   Streaks: {}d current · {}d best",
            format_goal_progress_line(goal_progress),
            streak.current,
            streak.best
        )
    } else {
        "Goal: Off (set via [p] -> [e])   Streaks: Off".to_string()
    }
}

fn format_history_goal_streak_line(app: &App) -> String {
    let goal_progress = app.today_goal_progress();
    let streak = app.goal_streak();
    if goal_progress.has_any_target() {
        format!(
            "{}   Streaks: {}d current · {}d best",
            format_goal_progress_line(goal_progress),
            streak.current,
            streak.best
        )
    } else {
        "Daily goal: Off   Streaks: Off".to_string()
    }
}

fn format_week_label(year: i32, week: u32) -> String {
    format!("{year:04}-W{week:02}")
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
    fn timer_secondary_hint_includes_planner_shortcut() {
        let app = App::default();
        assert!(timer_secondary_hint(&app).contains("[t] Planner"));
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
    fn history_view_renders_weekly_summary_panel() {
        let width = 100;
        let height = 24;
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        let mut app = App::default();
        app.mode = AppMode::StatsHistory;
        app.insert_daily_stats_for_tests(
            "2026-04-06",
            crate::stats::DailyStats {
                pomodoros_completed: 1,
                focused_seconds: 30 * 60,
                goal: None,
            },
        );
        app.insert_daily_stats_for_tests(
            "2026-04-08",
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
        assert!(text.contains("Weekly Totals"));
        assert!(text.contains(&format_week_label(2026, 15)));
        assert!(text.contains("75m"));
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
    fn week_label_uses_zero_padded_iso_format() {
        assert_eq!(format_week_label(2026, 5), "2026-W05");
    }

    #[test]
    fn wakatime_status_line_shows_not_yet_sent_when_no_success_exists() {
        let mut app = App::default();
        app.wakatime = WakatimeTracker::new_configured_for_tests();

        let (text, color) = wakatime_status_line(&app);

        assert_eq!(text, "⏱  WakaTime: idle · last success not yet sent");
        assert_eq!(color, Color::DarkGray);
    }

    #[test]
    fn wakatime_status_line_shows_last_success_time_after_success_event() {
        let mut app = App::default();
        app.wakatime = WakatimeTracker::new_configured_for_tests();
        app.wakatime.push_sent_event_for_tests();
        app.wakatime.poll_events();

        let (text, color) = wakatime_status_line(&app);

        assert!(text.starts_with("⏱  WakaTime: idle · last success "));
        assert!(!text.contains("not yet sent"));
        assert_eq!(color, Color::DarkGray);
    }

    #[test]
    fn wakatime_status_line_for_not_configured_omits_last_success_suffix() {
        let mut app = App::default();
        app.wakatime = WakatimeTracker::new_unconfigured_for_tests();

        let (text, color) = wakatime_status_line(&app);

        assert_eq!(text, "⏱  WakaTime: not configured");
        assert_eq!(color, Color::DarkGray);
    }
}
