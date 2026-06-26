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
    NavigationAction, PLANNER_RECENT_LABEL_LIMIT, PROFILE_IDS, PlannerFeedbackLevel,
    PlannerInputMode, SetupCheck, SetupCheckLevel, ShortcutAction, SiteFeedbackLevel,
    SiteInputMode, SiteListMode,
};
use crate::blocker::BlockingPreviewAction;
use crate::config::ThemePreset;
use crate::timer::{TimerPhase, TimerStatus};
use crate::wakatime::WakatimeRuntimeState;

mod history;
#[cfg(test)]
use history::{
    format_history_focus_risk_line, format_history_goal_streak_line,
    format_history_interruption_line, format_history_weekly_allocation_line, format_month_label,
};
use history::{readable_goal_streak_text, render_stats_history};
mod profile_manager;
use profile_manager::render_profile_manager;
mod session_planner;
#[cfg(test)]
use session_planner::planner_weekly_allocation_summary;
use session_planner::render_session_planner;
mod site_manager;
use site_manager::render_site_manager;
mod setup;
#[cfg(test)]
use setup::render_setup_check;
use setup::render_setup_diagnostics;
mod timer;
use timer::{format_duration_label, format_wakatime_heartbeat_timestamp, render_timer};
#[cfg(test)]
use timer::{
    timer_primary_hint, timer_secondary_hint, timer_session_status_lines_for_width,
    timer_status_text, wakatime_status_line,
};

const PROFILE_EDIT_GROUP_TIMER: [usize; 4] = [0, 1, 2, 3];
const PROFILE_EDIT_GROUP_AUTOMATION: [usize; 5] = [4, 5, 6, 7, 8];
const PROFILE_EDIT_GROUP_GOALS: [usize; 9] = [9, 10, 11, 12, 13, 14, 15, 16, 17];
const PROFILE_EDIT_GROUP_WAKATIME: [usize; 2] = [18, 19];
const PROFILE_EDIT_GROUP_SCHEDULE: [usize; 6] = [20, 21, 22, 23, 24, 25];
const PROFILE_EDIT_GROUP_APPEARANCE: [usize; 1] = [26];
const PROFILE_EDIT_GROUPS: [(&str, &[usize]); 6] = [
    ("Timer", &PROFILE_EDIT_GROUP_TIMER),
    ("Automation", &PROFILE_EDIT_GROUP_AUTOMATION),
    ("Goals", &PROFILE_EDIT_GROUP_GOALS),
    ("WakaTime", &PROFILE_EDIT_GROUP_WAKATIME),
    ("Schedule", &PROFILE_EDIT_GROUP_SCHEDULE),
    ("Appearance", &PROFILE_EDIT_GROUP_APPEARANCE),
];

pub(crate) fn render(frame: &mut Frame, app: &App) {
    match app.mode {
        AppMode::Timer => render_timer(frame, app),
        AppMode::SiteManager => render_site_manager(frame, app),
        AppMode::ProfileManager => render_profile_manager(frame, app),
        AppMode::TaskSetup => render_session_planner(frame, app),
        AppMode::StatsHistory => render_stats_history(frame, app),
        AppMode::SetupDiagnostics => render_setup_diagnostics(frame, app),
    }
}

fn render_hint_lines(frame: &mut Frame, app: &App, area: Rect, lines: Vec<Line<'static>>) {
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .style(Style::default().fg(app_color(app, Color::DarkGray)))
            .wrap(Wrap { trim: true }),
        area,
    );
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

fn render_centered_error(frame: &mut Frame, app: &App, area: Rect, message: String) {
    let err_widget = Paragraph::new(message)
        .alignment(Alignment::Center)
        .style(Style::default().fg(app_color(app, Color::Red)));
    frame.render_widget(err_widget, area);
}

pub(super) fn app_color(app: &App, color: Color) -> Color {
    themed_color(color, Some(app.selected_theme_preset()))
}

fn themed_color(color: Color, preset: Option<ThemePreset>) -> Color {
    let preset = preset.unwrap_or(ThemePreset::Classic);
    match preset {
        ThemePreset::Classic => color,
        ThemePreset::HighContrast => match color {
            Color::DarkGray | Color::Gray => Color::White,
            Color::Yellow => Color::LightYellow,
            Color::Cyan => Color::LightCyan,
            Color::Green => Color::LightGreen,
            Color::Red => Color::LightRed,
            Color::Black => Color::White,
            _ => color,
        },
        ThemePreset::DeuteranopiaFriendly => match color {
            Color::Red => Color::Blue,
            Color::LightRed => Color::LightBlue,
            Color::Green => Color::Cyan,
            Color::LightGreen => Color::LightCyan,
            Color::Cyan => Color::Magenta,
            Color::LightYellow => Color::Yellow,
            _ => color,
        },
    }
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
