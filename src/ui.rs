use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph},
};

use crate::app::{App, AppMode, PROFILE_EDIT_FIELD_LABELS, PROFILE_IDS};
use crate::timer::{TimerPhase, TimerStatus};
use crate::wakatime::WakatimeRuntimeState;

pub fn render(frame: &mut Frame, app: &App) {
    match app.mode {
        AppMode::Timer => render_timer(frame, app),
        AppMode::SiteManager => render_site_manager(frame, app),
        AppMode::ProfileManager => render_profile_manager(frame, app),
        AppMode::StatsHistory => render_stats_history(frame, app),
    }
}

fn render_timer(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Outer centered block
    let outer = centered_rect(72, 72, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" focustime ")
        .title_alignment(Alignment::Center)
        .style(Style::default().fg(phase_color(app.timer.phase)));
    frame.render_widget(block, outer);

    // Inner layout: title | time | profile | progress | status | phase notice | stats | wakatime | spacer | hints
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
            Constraint::Length(1), // wakatime status
            Constraint::Min(0),    // spacer
            Constraint::Length(2), // key hints
        ])
        .split(outer);

    // Phase label + pomodoro count
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
    frame.render_widget(phase_widget, inner[0]);

    // MM:SS countdown
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
    frame.render_widget(time_widget, inner[1]);

    // Active profile
    let profile_text = format!(
        "Profile: {} ({})",
        app.selected_profile_name(),
        app.profile_summary(app.selected_profile)
    );
    let profile_widget = Paragraph::new(profile_text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(profile_widget, inner[2]);

    // Progress bar (fills as time counts down, so invert: elapsed / total)
    let elapsed_ratio = 1.0 - app.timer.progress();
    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::NONE))
        .gauge_style(
            Style::default()
                .fg(phase_color(app.timer.phase))
                .bg(Color::DarkGray),
        )
        .ratio(elapsed_ratio);
    frame.render_widget(gauge, inner[3]);

    // Status indicator
    let status_text = match app.timer.status {
        TimerStatus::Running => "▶  Running",
        TimerStatus::Paused => "⏸  Paused",
        TimerStatus::Idle => "⏹  Idle",
    };
    let status_widget = Paragraph::new(status_text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Gray));
    frame.render_widget(status_widget, inner[4]);

    // Phase transition notification
    let (phase_notification_text, phase_notification_style) =
        if let Some(message) = app.phase_notification.as_ref() {
            (format!("🔔 {message}"), Style::default().fg(Color::Yellow))
        } else {
            (
                "🔔 Waiting for next completed phase".to_string(),
                Style::default().fg(Color::DarkGray),
            )
        };
    let phase_notification_widget = Paragraph::new(phase_notification_text)
        .alignment(Alignment::Center)
        .style(phase_notification_style);
    frame.render_widget(phase_notification_widget, inner[5]);

    // Session + today stats summary
    let stats_line = if let Some(err) = app.stats_error.as_ref() {
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
    };
    let stats_widget = Paragraph::new(stats_line.0)
        .alignment(Alignment::Center)
        .style(stats_line.1);
    frame.render_widget(stats_widget, inner[6]);

    // WakaTime status
    let (waka_text, waka_color) = match app.wakatime.runtime_state() {
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
    let waka_widget = Paragraph::new(waka_text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(waka_color));
    frame.render_widget(waka_widget, inner[7]);

    // Key hints
    let hints_widget = Paragraph::new(vec![
        Line::from("[Space] Start/Pause  [s] Stop  [n] Next"),
        Line::from("[h] History  [p] Profiles  [b] Block Sites  [q/Esc] Quit"),
    ])
    .alignment(Alignment::Center)
    .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hints_widget, inner[9]);
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
            Constraint::Length(1), // key hints
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
    let input_block = Block::default()
        .borders(Borders::ALL)
        .title(" Add Site ")
        .style(if app.site_input_active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        });

    let input_text = if app.site_input_active {
        format!("{}_", app.site_input)
    } else {
        "Press [a] to add a site (e.g. youtube.com)".to_string()
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
    }

    // Key hints
    let hints = if app.site_input_active {
        "[Enter] Confirm  [Esc] Cancel"
    } else {
        "[a] Add  [d] Delete  [↑/↓] Navigate  [b/Esc] Back  [q] Quit"
    };
    let hints_widget = Paragraph::new(hints)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hints_widget, inner[8]);
}

fn render_profile_manager(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let outer = centered_rect(70, 80, area);

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
            Constraint::Length(9), // custom + notification editor
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
        " Custom + notification settings editor "
    } else {
        " Custom + notification settings ([e] to edit) "
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
            Line::from("[↑/↓] Field  [←/→] Adjust"),
            Line::from("[Enter] Save  [Esc] Cancel  [q/Ctrl-C] Quit"),
        ]
    } else {
        vec![
            Line::from("[↑/↓] Select  [Enter] Apply  [e] Edit Settings"),
            Line::from("[p/Esc] Back  [q] Quit"),
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
        frame.render_widget(empty, inner[3]);
    } else {
        let list = List::new(history_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Recent Days "),
            )
            .style(Style::default().fg(Color::Gray));
        frame.render_widget(list, inner[3]);
    }

    if let Some(err) = app.stats_error.as_ref() {
        render_centered_error(frame, inner[4], format!("⚠  {err}"));
    }

    let hints = Paragraph::new("[h/Esc] Back  [q/Ctrl-C] Quit")
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hints, inner[5]);
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
