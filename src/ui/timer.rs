use crate::ui::{
    Alignment, App, Block, Borders, Color, Constraint, Direction, Frame, Gauge, Layout, Line,
    Modifier, Paragraph, Rect, ShortcutAction, Style, TimerPhase, TimerStatus, Wrap, app_color,
    centered_rect, format_timer_goal_streak_line, readable_goal_streak_text, render_hint_lines,
};

pub(super) fn render_timer(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let outer = centered_rect(90, 90, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" focustime ")
        .title_alignment(Alignment::Center)
        .style(Style::default().fg(phase_color(app, app.timer.phase)));
    frame.render_widget(block, outer);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1), // phase + pomodoro count
            Constraint::Min(10),   // body
            Constraint::Length(1), // latest phase notification
            Constraint::Length(3), // key hints
        ])
        .split(outer);

    render_timer_phase_header(frame, app, inner[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(44), Constraint::Percentage(56)])
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
                .fg(phase_color(app, app.timer.phase))
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
                .fg(app_color(app, Color::White))
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
            .style(Style::default().fg(app_color(app, Color::DarkGray))),
        layout[0],
    );

    let elapsed_ratio = 1.0 - app.timer.progress();
    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::NONE))
        .gauge_style(
            Style::default()
                .fg(phase_color(app, app.timer.phase))
                .bg(app_color(app, Color::DarkGray)),
        )
        .ratio(elapsed_ratio);
    frame.render_widget(gauge, layout[2]);
}

fn render_timer_session_panel(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" 🧭 Session ");
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
            Style::default().fg(app_color(app, Color::LightYellow)),
        )
    } else {
        (
            format!(
                "🎯 Task: not selected ({} Tasks)",
                app.shortcut_hint(ShortcutAction::OpenTaskSetup)
            ),
            Style::default().fg(app_color(app, Color::Yellow)),
        )
    };
    let (stats_text, stats_style) = timer_stats_line(app);
    let goal_text = readable_goal_streak_text(&format_timer_goal_streak_line(app));
    let (schedule_next_text, schedule_status_text) = app.recurring_schedule_display_texts();

    let mut lines = vec![
        Line::styled(task_text, task_style),
        Line::styled(
            status_text,
            Style::default().fg(app_color(app, Color::Gray)),
        ),
        Line::styled(
            profile_line,
            Style::default().fg(app_color(app, Color::DarkGray)),
        ),
        Line::styled(
            schedule_next_text,
            Style::default().fg(app_color(app, Color::DarkGray)),
        ),
        Line::styled(
            schedule_status_text,
            Style::default().fg(app_color(app, Color::DarkGray)),
        ),
        Line::styled(format!("📈 {stats_text}"), stats_style),
    ];
    if let Some((goals, streak)) = goal_text.split_once(" | Streak: ") {
        lines.push(Line::styled(
            format!("🔥 {goals}"),
            Style::default().fg(app_color(app, Color::DarkGray)),
        ));
        lines.push(Line::styled(
            format!("🔥 Streak: {streak}"),
            Style::default().fg(app_color(app, Color::DarkGray)),
        ));
    } else {
        lines.push(Line::styled(
            format!("🔥 {goal_text}"),
            Style::default().fg(app_color(app, Color::DarkGray)),
        ));
    }
    for status_line in timer_session_status_lines(strict_status_text) {
        lines.push(Line::styled(
            status_line,
            Style::default().fg(app_color(app, Color::DarkGray)),
        ));
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

pub(super) fn timer_status_text(app: &App) -> (String, String) {
    let status_text = match app.timer.status {
        TimerStatus::Running => "📍 Status: ▶ Running".to_string(),
        TimerStatus::Paused => "📍 Status: ⏸ Paused".to_string(),
        TimerStatus::Idle => "📍 Status: ⏹ Idle".to_string(),
    };
    let strict_text = if app.strict_reset_confirmation_pending() {
        format!(
            "🔒 Strict: confirm reset {}",
            app.shortcut_hint(ShortcutAction::TimerStopReset)
        )
    } else if app.strict_mode_enforced_for_focus() {
        "🔒 Strict: active (skip/quit locked)".to_string()
    } else if app.strict_mode {
        "🔒 Strict: armed".to_string()
    } else {
        "🔓 Strict: off".to_string()
    };
    (status_text, strict_text)
}

fn timer_session_status_lines(strict_status_text: String) -> Vec<String> {
    vec![strict_status_text]
}

#[cfg(test)]
pub(super) fn timer_session_status_lines_for_width(app: &App, width: u16) -> Vec<String> {
    let _ = width;
    let (_, strict_status_text) = timer_status_text(app);
    timer_session_status_lines(strict_status_text)
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
        (
            format!("🔔 {message}"),
            Style::default().fg(app_color(app, Color::Yellow)),
        )
    } else {
        (
            "🔔 Waiting for next completed phase".to_string(),
            Style::default().fg(app_color(app, Color::DarkGray)),
        )
    }
}

fn timer_stats_line(app: &App) -> (String, Style) {
    if let Some(err) = app.stats_error.as_ref() {
        (
            format!("⚠ Stats persistence warning: {err}"),
            Style::default().fg(app_color(app, Color::Yellow)),
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
            Style::default().fg(app_color(app, Color::DarkGray)),
        )
    }
}

pub(super) fn format_duration_label(duration_secs: u64) -> String {
    let minutes = duration_secs / 60;
    let remaining_seconds = duration_secs % 60;
    if remaining_seconds == 0 {
        format!("{minutes}m")
    } else {
        format!("{minutes}:{remaining_seconds:02}")
    }
}

fn render_timer_hints(frame: &mut Frame, app: &App, area: Rect) {
    render_hint_lines(
        frame,
        app,
        area,
        vec![
            Line::from(timer_primary_hint(app)),
            Line::from(timer_secondary_hint(app)),
            Line::from(timer_tertiary_hint(app)),
        ],
    );
}

pub(super) fn timer_primary_hint(app: &App) -> String {
    let timer_toggle = app.shortcut_hint(ShortcutAction::TimerTogglePause);
    let timer_stop = app.shortcut_hint(ShortcutAction::TimerStopReset);
    let timer_next = app.shortcut_hint(ShortcutAction::TimerNextPhase);

    if app.strict_reset_confirmation_pending() {
        format!(
            "Timer: {timer_toggle} Run/Pause  {timer_stop} Confirm reset  {timer_next} Next (Locked)"
        )
    } else if app.strict_mode_enforced_for_focus() {
        format!(
            "Timer: {timer_toggle} Run/Pause  {timer_stop} Stop/Reset (Confirm)  {timer_next} Next (Locked)"
        )
    } else {
        format!("Timer: {timer_toggle} Run/Pause  {timer_stop} Stop/Reset  {timer_next} Next")
    }
}

pub(super) fn timer_secondary_hint(app: &App) -> String {
    let tasks = app.shortcut_hint(ShortcutAction::OpenTaskSetup);
    let history = app.shortcut_hint(ShortcutAction::OpenStatsHistory);
    let sites = app.shortcut_hint(ShortcutAction::OpenSiteManager);
    let profiles = app.shortcut_hint(ShortcutAction::OpenProfileManager);
    let setup = app.shortcut_hint(ShortcutAction::OpenSetupDiagnostics);

    if app.strict_mode_enforced_for_focus() {
        format!(
            "Views: {tasks} Tasks  {history} History  {sites} Sites  {profiles} Profiles (Locked)  {setup} Setup"
        )
    } else {
        format!(
            "Views: {tasks} Tasks  {history} History  {sites} Sites  {profiles} Profiles  {setup} Setup"
        )
    }
}

pub(super) fn timer_tertiary_hint(app: &App) -> String {
    let quit = app.shortcut_label(ShortcutAction::Quit);
    if app.strict_mode_enforced_for_focus() {
        format!("Navigate: [{quit}/Esc] Quit (Locked during active focus)")
    } else {
        format!("Navigate: [{quit}/Esc] Quit")
    }
}

fn phase_color(app: &App, phase: TimerPhase) -> Color {
    match phase {
        TimerPhase::Focus => app_color(app, Color::Red),
        TimerPhase::ShortBreak => app_color(app, Color::Green),
        TimerPhase::LongBreak => app_color(app, Color::Cyan),
    }
}
