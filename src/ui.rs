use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph},
};

use crate::app::{ActiveTab, App, InputMode};
use crate::timer::{TimerPhase, TimerStatus};

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let outer = centered_rect(60, 80, area);

    // Split outer into tab bar + content
    let root_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(outer);

    render_tab_bar(frame, app, root_chunks[0]);

    match app.active_tab {
        ActiveTab::Timer => render_timer(frame, app, root_chunks[1]),
        ActiveTab::SiteList => render_site_list(frame, app, root_chunks[1]),
    }
}

fn render_tab_bar(frame: &mut Frame, app: &App, area: Rect) {
    let timer_style = if app.active_tab == ActiveTab::Timer {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let sites_style = if app.active_tab == ActiveTab::SiteList {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let tab_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    frame.render_widget(
        Paragraph::new(" [Timer]")
            .alignment(Alignment::Center)
            .style(timer_style),
        tab_chunks[0],
    );
    frame.render_widget(
        Paragraph::new("[Sites]")
            .alignment(Alignment::Center)
            .style(sites_style),
        tab_chunks[1],
    );
}

fn render_timer(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" focustime ")
        .title_alignment(Alignment::Center)
        .style(Style::default().fg(phase_color(app.timer.phase)));
    frame.render_widget(block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(2), // phase + pomodoro count
            Constraint::Length(3), // MM:SS
            Constraint::Length(3), // progress bar
            Constraint::Length(2), // status
            Constraint::Min(0),    // spacer
            Constraint::Length(1), // key hints
        ])
        .split(area);

    let phase_text = format!(
        "{}   🍅 ×{}",
        app.timer.phase.label(),
        app.timer.pomodoros_completed
    );
    frame.render_widget(
        Paragraph::new(phase_text)
            .alignment(Alignment::Center)
            .style(
                Style::default()
                    .fg(phase_color(app.timer.phase))
                    .add_modifier(Modifier::BOLD),
            ),
        inner[0],
    );

    let remaining = app.timer.remaining_secs;
    let time_text = format!("{:02}:{:02}", remaining / 60, remaining % 60);
    frame.render_widget(
        Paragraph::new(time_text)
            .alignment(Alignment::Center)
            .style(
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        inner[1],
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
    frame.render_widget(gauge, inner[2]);

    let status_text = match app.timer.status {
        TimerStatus::Running => "▶  Running",
        TimerStatus::Paused => "⏸  Paused",
        TimerStatus::Idle => "⏹  Idle",
    };
    frame.render_widget(
        Paragraph::new(status_text)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Gray)),
        inner[3],
    );

    let hints = "[Space] Start/Pause  [s] Stop  [n] Next  [Tab] Sites  [q] Quit";
    frame.render_widget(
        Paragraph::new(hints)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray)),
        inner[5],
    );
}

fn render_site_list(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Site Blocking ")
        .title_alignment(Alignment::Center)
        .style(Style::default().fg(Color::Yellow));
    frame.render_widget(block, area);

    // Layout: list | error banner (optional) | input (optional) | hints
    let error_height: u16 = if app.blocker_error.is_some() { 1 } else { 0 };
    let input_height: u16 = if app.input_mode == InputMode::AddingSite {
        3
    } else {
        0
    };

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Min(1),               // list
            Constraint::Length(error_height), // error
            Constraint::Length(input_height), // input box
            Constraint::Length(1),            // hints
        ])
        .split(area);

    // Site list
    let items: Vec<ListItem> = if app.block_list.is_empty() {
        vec![ListItem::new("  (no sites blocked)").style(Style::default().fg(Color::DarkGray))]
    } else {
        app.block_list
            .iter()
            .map(|s| ListItem::new(format!("  {s}")))
            .collect()
    };
    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    let mut list_state = ListState::default();
    if !app.block_list.is_empty() {
        list_state.select(Some(app.selected_site));
    }
    frame.render_stateful_widget(list, inner[0], &mut list_state);

    // Error banner
    if let Some(err) = &app.blocker_error {
        frame.render_widget(
            Paragraph::new(err.as_str())
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Red)),
            inner[1],
        );
    }

    // Input box
    if app.input_mode == InputMode::AddingSite {
        let input_block = Block::default().borders(Borders::ALL).title(" Add site ");
        let input_text = format!("{}_", app.input_buffer);
        frame.render_widget(Paragraph::new(input_text).block(input_block), inner[2]);
    }

    // Key hints
    let hints = if app.input_mode == InputMode::AddingSite {
        "[Enter] Confirm  [Esc] Cancel"
    } else {
        "[a] Add  [d] Delete  [↑↓] Navigate  [Tab] Timer  [q] Quit"
    };
    frame.render_widget(
        Paragraph::new(hints)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray)),
        inner[3],
    );
}

fn phase_color(phase: TimerPhase) -> Color {
    match phase {
        TimerPhase::Focus => Color::Red,
        TimerPhase::ShortBreak => Color::Green,
        TimerPhase::LongBreak => Color::Cyan,
    }
}

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
