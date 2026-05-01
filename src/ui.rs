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
    PLANNER_RECENT_LABEL_LIMIT, PROFILE_IDS, PlannerFeedbackLevel, PlannerInputMode, SetupCheck,
    SetupCheckLevel, SiteFeedbackLevel, SiteInputMode, SiteListMode,
};
use crate::blocker::BlockingPreviewAction;
use crate::timer::{TimerPhase, TimerStatus};
use crate::wakatime::WakatimeRuntimeState;

mod history;
use history::*;
mod profile_manager;
use profile_manager::*;
mod site_manager;
use site_manager::*;
mod setup;
use setup::*;

const PROFILE_EDIT_GROUP_TIMER: [usize; 4] = [0, 1, 2, 3];
const PROFILE_EDIT_GROUP_AUTOMATION: [usize; 5] = [4, 5, 6, 7, 8];
const PROFILE_EDIT_GROUP_GOALS: [usize; 9] = [9, 10, 11, 12, 13, 14, 15, 16, 17];
const PROFILE_EDIT_GROUP_WAKATIME: [usize; 2] = [18, 19];
const PROFILE_EDIT_GROUP_SCHEDULE: [usize; 15] =
    [20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34];
const PROFILE_EDIT_GROUPS: [(&str, &[usize]); 5] = [
    ("Timer", &PROFILE_EDIT_GROUP_TIMER),
    ("Automation", &PROFILE_EDIT_GROUP_AUTOMATION),
    ("Goals", &PROFILE_EDIT_GROUP_GOALS),
    ("WakaTime", &PROFILE_EDIT_GROUP_WAKATIME),
    ("Schedule", &PROFILE_EDIT_GROUP_SCHEDULE),
];

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

    let outer = centered_rect(90, 90, area);
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
    let block = Block::default().borders(Borders::ALL).title(" 🧭 Session ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let (status_text, strict_status_text, break_glass_status_text) = timer_status_text(app);
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
    let can_edit_session_note = app.can_edit_session_note();
    let (note_text, note_style) = if let Some(note) = app.current_task_note() {
        (format!("📝 Note: {note}"), Style::default().fg(Color::Cyan))
    } else if can_edit_session_note {
        (
            "📝 Note: none yet ([m] Edit note)".to_string(),
            Style::default().fg(Color::DarkGray),
        )
    } else {
        (
            "📝 Note: unavailable (start or resume focus to edit)".to_string(),
            Style::default().fg(Color::DarkGray),
        )
    };
    let (stats_text, stats_style) = timer_stats_line(app);
    let goal_text = readable_goal_streak_text(&format_timer_goal_streak_line(app));
    let (waka_text, waka_color) = wakatime_status_line(app);
    let (schedule_next_text, schedule_status_text) = app.recurring_schedule_display_texts();
    let strict_and_break_glass = format!("{strict_status_text} · {break_glass_status_text}");

    let mut lines = vec![
        Line::styled(task_text, task_style),
        Line::styled(note_text, note_style),
        Line::styled(status_text, Style::default().fg(Color::Gray)),
        Line::styled(profile_line, Style::default().fg(Color::DarkGray)),
        Line::styled(schedule_next_text, Style::default().fg(Color::DarkGray)),
        Line::styled(schedule_status_text, Style::default().fg(Color::DarkGray)),
        Line::styled(format!("📈 {stats_text}"), stats_style),
    ];
    if let Some((goals, streak)) = goal_text.split_once(" | Streak: ") {
        lines.push(Line::styled(
            format!("🔥 {goals}"),
            Style::default().fg(Color::DarkGray),
        ));
        lines.push(Line::styled(
            format!("🔥 Streak: {streak}"),
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        lines.push(Line::styled(
            format!("🔥 {goal_text}"),
            Style::default().fg(Color::DarkGray),
        ));
    }
    lines.push(Line::styled(waka_text, Style::default().fg(waka_color)));
    lines.push(Line::styled(
        strict_and_break_glass,
        Style::default().fg(Color::DarkGray),
    ));

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn timer_status_text(app: &App) -> (String, String, String) {
    let status_text = match app.timer.status {
        TimerStatus::Running => "📍 Status: ▶ Running".to_string(),
        TimerStatus::Paused => "📍 Status: ⏸ Paused".to_string(),
        TimerStatus::Idle => "📍 Status: ⏹ Idle".to_string(),
    };
    let strict_text = if app.strict_reset_confirmation_pending() {
        "🔒 Strict: confirm reset [s]".to_string()
    } else if app.strict_mode_enforced_for_focus() {
        "🔒 Strict: active (skip/quit locked)".to_string()
    } else if app.strict_mode {
        "🔒 Strict: armed".to_string()
    } else {
        "🔓 Strict: off".to_string()
    };
    let break_glass_text = if app.break_glass_confirmation_pending() {
        "🚨 Break-glass: confirm [u]".to_string()
    } else if let Some(remaining_secs) = app.break_glass_override_remaining_secs() {
        format!(
            "🚨 Break-glass: active ({})",
            format_duration_label(remaining_secs)
        )
    } else {
        "🚨 Break-glass: off".to_string()
    };
    (status_text, strict_text, break_glass_text)
}

fn render_timer_phase_notice(frame: &mut Frame, app: &App, area: Rect) {
    let (text, style) = phase_notice_line(app);
    let widget = Paragraph::new(text)
        .alignment(Alignment::Center)
        .style(style);
    frame.render_widget(widget, area);
}

fn phase_notice_line(app: &App) -> (String, Style) {
    if app.timer_note_input_active() {
        let draft = app.timer_note_input_value().trim();
        let draft = if draft.is_empty() { "<empty>" } else { draft };
        return (
            format!("📝 Note: {draft}   [Enter] Save   [Esc] Cancel"),
            Style::default().fg(Color::Cyan),
        );
    }

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

fn format_duration_label(duration_secs: u64) -> String {
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
        area,
        vec![
            Line::from(timer_primary_hint(app)),
            Line::from(timer_secondary_hint(app)),
            Line::from(timer_tertiary_hint(app)),
        ],
    );
}

fn render_hint_lines(frame: &mut Frame, area: Rect, lines: Vec<Line<'static>>) {
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn timer_primary_hint(app: &App) -> &'static str {
    if app.timer_note_input_active() {
        "Note: Type text  [Enter] Save  [Esc] Cancel"
    } else if app.break_glass_confirmation_pending() {
        "Timer: [Space] Run/Pause  [s] Stop/Reset  [n] Next  [m] Note  [u] Confirm unblock  [z] Delay 10m"
    } else if app.strict_reset_confirmation_pending() {
        "Timer: [Space] Run/Pause  [s] Confirm reset  [n] Next (Locked)  [m] Note  [u] Unblock  [z] Delay 10m"
    } else if app.strict_mode_enforced_for_focus() {
        "Timer: [Space] Run/Pause  [s] Stop/Reset (Confirm)  [n] Next (Locked)  [m] Note  [u] Unblock  [z] Delay 10m"
    } else if app.can_edit_session_note() {
        "Timer: [Space] Run/Pause  [s] Stop/Reset  [n] Next  [m] Note  [u] Unblock  [z] Delay 10m"
    } else {
        "Timer: [Space] Run/Pause  [s] Stop/Reset  [n] Next  [m] Note (Focus only)  [u] Unblock  [z] Delay 10m"
    }
}

fn timer_secondary_hint(app: &App) -> &'static str {
    if app.timer_note_input_active() {
        "Views: shortcuts paused while editing note"
    } else if app.strict_mode_enforced_for_focus() {
        "Views: [t] Planner  [h] History  [b] Sites  [p] Profiles (Locked)  [d] Setup"
    } else {
        "Views: [t] Planner  [h] History  [b] Sites  [p] Profiles  [d] Setup"
    }
}

fn timer_tertiary_hint(app: &App) -> &'static str {
    if app.timer_note_input_active() {
        "Note edit: [Esc] Cancel"
    } else if app.strict_mode_enforced_for_focus() {
        "Navigate: [q/Esc] Quit (Locked during active focus)"
    } else {
        "Navigate: [q/Esc] Quit"
    }
}

fn render_session_planner(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let outer = centered_rect(52, 70, area);

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
            Constraint::Length(1), // current task
            Constraint::Min(4),    // task labels list
            Constraint::Length(3), // task label input
            Constraint::Length(1), // feedback
            Constraint::Length(3), // hints
        ])
        .split(outer);

    render_session_planner_selected_task(frame, app, inner[0]);
    render_session_planner_labels(frame, app, inner[1]);
    render_session_planner_input(frame, app, inner[2]);
    render_session_planner_feedback(frame, app, inner[3]);
    render_session_planner_hints(frame, app, inner[4]);
}

fn render_session_planner_selected_task(frame: &mut Frame, app: &App, area: Rect) {
    let selected_text = app.selected_task_label.as_ref().map_or_else(
        || "Selected task: none (required before focus starts)".to_string(),
        |label| format!("Selected task: {label}"),
    );
    frame.render_widget(
        Paragraph::new(Line::from(selected_text))
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

    let display_labels = app.planner_labels_for_display();
    let items: Vec<ListItem> = display_labels
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
            let mut badges = Vec::new();
            if app.is_task_label_favorite(label) {
                badges.push("★");
            }
            if app.is_task_label_archived(label) {
                badges.push("archived");
            }
            let suffix = if badges.is_empty() {
                String::new()
            } else {
                format!(" ({})", badges.join(", "))
            };
            ListItem::new(format!(" {marker} {label}{suffix}"))
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
            .min(display_labels.len().saturating_sub(1)),
    ));
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_session_planner_input(frame: &mut Frame, app: &App, area: Rect) {
    let input_title = if app.planner_input_active {
        match app.planner_input_mode {
            Some(PlannerInputMode::Rename) => " Rename task label ",
            _ => " Add task label ",
        }
    } else {
        " Task label input ([a] add / [e] rename / [f] favorite / [x] archive) "
    };
    let input_text = if app.planner_input_active {
        format!("{}|", app.planner_input)
    } else {
        "Use [a] add, [e] rename, [f] favorite, [x] archive, [d/Del] delete highlighted".to_string()
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

fn planner_recent_quick_pick_text(app: &App) -> String {
    let recent = app.planner_recent_labels(PLANNER_RECENT_LABEL_LIMIT);
    if recent.is_empty() {
        return "Recent: none yet".to_string();
    }

    let mut parts = Vec::new();
    for (index, label) in recent.iter().enumerate() {
        if index == 0 {
            parts.push(format!("[r/1] {label}"));
        } else {
            parts.push(format!("[{}] {label}", index + 1));
        }
    }
    format!("Recent: {}", parts.join("  "))
}

fn render_session_planner_hints(frame: &mut Frame, app: &App, area: Rect) {
    let hints = if app.planner_input_active {
        let commit_line = match app.planner_input_mode {
            Some(PlannerInputMode::Rename) => "Input: rename label, then [Enter]",
            _ => "Input: type task label, then [Enter]",
        };
        vec![
            Line::from(commit_line),
            Line::from("Input: [Esc] Cancel"),
            Line::from(if app.strict_mode_enforced_for_focus() {
                "View: [q/Ctrl-C] Quit (Locked)"
            } else {
                "View: [q/Ctrl-C] Quit"
            }),
        ]
    } else {
        vec![
            Line::from(
                "Planner: [↑/↓] Move  [Enter] Select  [a] Add  [e] Rename  [f] Favorite  [x] Archive  [d/Del] Delete",
            ),
            Line::from(format!(
                "{}  |  Archived labels stay visible and cannot be selected",
                planner_recent_quick_pick_text(app)
            )),
            Line::from(if app.strict_mode_enforced_for_focus() {
                "View: [t/Esc] Back  [q/Ctrl-C] Quit (Locked)"
            } else {
                "View: [t/Esc] Back  [q/Ctrl-C] Quit"
            }),
        ]
    };
    render_hint_lines(frame, area, hints);
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
    format!("{pomodoros}   {minutes}")
}

fn format_goal_period_progress(period: &str, progress: DailyGoalProgress) -> String {
    format!("{period} {}", format_goal_progress_line(progress))
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
    let daily_goal_progress = app.today_goal_progress();
    let weekly_goal_progress = app.current_week_goal_progress();
    let monthly_goal_progress = app.current_month_goal_progress();
    let streak = app.goal_streak();
    if daily_goal_progress.has_any_target()
        || weekly_goal_progress.has_any_target()
        || monthly_goal_progress.has_any_target()
    {
        format!(
            "Goals: {} · {} · {}   Streaks: {}d current · {}d best",
            format_goal_period_progress("D", daily_goal_progress),
            format_goal_period_progress("W", weekly_goal_progress),
            format_goal_period_progress("M", monthly_goal_progress),
            streak.current,
            streak.best
        )
    } else {
        "Goals: Off (set via [p] -> [e])   Streaks: Off".to_string()
    }
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
mod tests;
