use crate::ui::{
    Alignment, App, Block, Borders, Color, Constraint, Direction, Frame, HistoryFeedbackLevel,
    Layout, Line, List, ListItem, Modifier, Paragraph, Rect, Span, Style, Wrap, centered_rect,
    format_duration_label, format_goal_period_progress, format_wakatime_heartbeat_timestamp,
    render_hint_lines,
};

pub(super) fn render_stats_history(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let outer = centered_rect(72, 78, area);

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
            Constraint::Length(7), // overview
            Constraint::Min(6),    // history panels
            Constraint::Length(1), // status line
            Constraint::Length(2), // hints
        ])
        .split(outer);

    let session_stats = app.session_stats();
    let today_stats = app.today_stats();
    let recent_active_days = app
        .recent_daily_stats(7)
        .into_iter()
        .filter(|(_, stats)| stats.pomodoros_completed > 0 || stats.focused_seconds > 0)
        .count();
    let focus_score_line = readable_focus_score_text(&format_history_focus_score_line(app));
    let goals_line = readable_goal_streak_text(&format_history_goal_streak_line(app));
    let interruption_line = format_history_interruption_line(app);
    let overview = Paragraph::new(vec![
        Line::styled(
            format!(
                "Session 🍅{} · {}m | Today 🍅{} · {}m",
                session_stats.pomodoros_completed,
                session_stats.focused_minutes(),
                today_stats.pomodoros_completed,
                today_stats.focused_minutes()
            ),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Line::styled(focus_score_line, Style::default().fg(Color::DarkGray)),
        Line::styled(goals_line, Style::default().fg(Color::DarkGray)),
        Line::styled(
            format!("Active days (7d): {recent_active_days}"),
            Style::default().fg(Color::DarkGray),
        ),
        Line::styled(interruption_line, Style::default().fg(Color::DarkGray)),
    ])
    .block(Block::default().borders(Borders::ALL).title(" Overview "))
    .wrap(Wrap { trim: true });
    frame.render_widget(overview, inner[0]);

    let content_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(inner[1]);

    let left_sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(content_layout[0]);

    let right_sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(content_layout[1]);

    let task_total_items: Vec<ListItem> = app
        .task_focus_totals(6)
        .into_iter()
        .map(|stats| {
            let goal_summary = app
                .task_goal_progress_for_label(&stats.task_label)
                .map(|progress| format_task_goal_progress_summary(&progress))
                .unwrap_or_else(|| "goal off".to_string());
            ListItem::new(format!(
                "  {} · 🍅{} · {}m · {}",
                stats.task_label,
                stats.pomodoros_completed,
                stats.focused_minutes(),
                goal_summary
            ))
        })
        .collect();
    render_history_panel(
        frame,
        left_sections[0],
        " Task Totals ",
        task_total_items,
        "  No task totals yet.",
    );

    let task_trend_items: Vec<ListItem> = app
        .recent_task_trends(6)
        .into_iter()
        .map(|trend| {
            ListItem::new(format!(
                "  {} · {}m ({})",
                trend.task_label,
                trend.recent_focused_minutes(),
                format_task_trend_delta(&trend)
            ))
        })
        .collect();
    render_history_panel(
        frame,
        left_sections[1],
        " Task Trends (7d vs prev 7d) ",
        task_trend_items,
        "  Not enough task history for trend yet.",
    );

    let profile_items: Vec<ListItem> = app
        .profile_effectiveness()
        .into_iter()
        .map(|entry| {
            ListItem::new(format!(
                "  {:<9} {:>3}% {:>3}m/session",
                entry.profile.label(),
                entry.focus_share_pct,
                entry.average_focused_minutes_per_session()
            ))
        })
        .collect();
    render_history_panel(
        frame,
        right_sections[0],
        " Profile Effect ",
        profile_items,
        "  No profile effectiveness yet.",
    );

    let override_items: Vec<ListItem> = app
        .recent_break_glass_overrides(6)
        .into_iter()
        .map(|event| {
            let task_label = event.task_label.unwrap_or_else(|| "Unlabeled".to_string());
            ListItem::new(format!(
                "  {} {} · {} · {}",
                event.date,
                format_wakatime_heartbeat_timestamp(event.timestamp_epoch_secs),
                task_label,
                format_duration_label(event.duration_seconds)
            ))
        })
        .collect();
    render_history_panel(
        frame,
        left_sections[2],
        " Break-glass Audit ",
        override_items,
        "  No break-glass overrides yet.",
    );

    let monthly_items: Vec<ListItem> = app
        .recent_monthly_stats(4)
        .into_iter()
        .map(|stats| {
            ListItem::new(format!(
                "  {}   🍅{}   {}m",
                format_month_label(stats.year, stats.month),
                stats.pomodoros_completed,
                stats.focused_minutes()
            ))
        })
        .collect();
    render_history_panel(
        frame,
        right_sections[1],
        " Monthly Trend ",
        monthly_items,
        "  No monthly totals yet.",
    );

    render_monthly_heatmap_panel(frame, right_sections[2], app);

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
        frame.render_widget(feedback_widget, inner[2]);
    } else if let Some(err) = app.stats_error.as_ref() {
        let error_widget = Paragraph::new(format!("⚠  {err}"))
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Red))
            .wrap(Wrap { trim: true });
        frame.render_widget(error_widget, inner[2]);
    }

    render_hint_lines(
        frame,
        inner[3],
        vec![
            Line::from("History: [e] Export CSV + JSON"),
            Line::from(if app.strict_mode_enforced_for_focus() {
                "View: [h/Esc] Back  [q/Ctrl-C] Quit (Locked)"
            } else {
                "View: [h/Esc] Back  [q/Ctrl-C] Quit"
            }),
        ],
    );
}

pub(super) fn readable_goal_streak_text(text: &str) -> String {
    text.replace("   Streaks: ", " | Streak: ")
        .replace(" current · ", " current, ")
        .replace("Off (set via [p] -> [e])", "off (set in Profiles)")
        .replace("Goals: Off", "Goals: off")
        .replace("Streaks: Off", "Streak: off")
        .replace("Streak: Off", "Streak: off")
}

pub(super) fn readable_focus_score_text(text: &str) -> String {
    if text.contains("weekly goal off; consistency ") {
        text.replace(
            "Focus score: n/a (weekly goal off; consistency ",
            "Focus score: n/a · Consistency: ",
        )
        .replace(" days))", " days, weekly goal off)")
    } else {
        text.replace(" (consistency ", " · Consistency: ")
            .replace(" · completion ", " · Goal progress: ")
            .replace("%)", "%")
    }
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

fn render_monthly_heatmap_panel(frame: &mut Frame, area: Rect, app: &App) {
    let heatmap = app.latest_monthly_heatmap();
    let title = format!(
        " Heatmap {} ",
        format_month_label(heatmap.year, heatmap.month)
    );
    let mut lines: Vec<Line> = vec![Line::styled(
        "  Mo Tu We Th Fr Sa Su",
        Style::default().fg(Color::DarkGray),
    )];

    let mut weekday = heatmap.first_weekday_monday0 as usize;
    let mut week_spans = vec![Span::raw("  ")];
    for _ in 0..weekday {
        week_spans.push(Span::raw("   "));
    }
    for day in heatmap.days {
        let (symbol, color) =
            heatmap_cell_symbol(day.focused_minutes(), heatmap.max_focused_minutes);
        week_spans.push(Span::styled(
            format!("{symbol}  "),
            Style::default().fg(color),
        ));
        if weekday == 6 {
            lines.push(Line::from(week_spans));
            week_spans = vec![Span::raw("  ")];
            weekday = 0;
        } else {
            weekday += 1;
        }
    }
    if weekday != 0 {
        for _ in weekday..7 {
            week_spans.push(Span::raw("   "));
        }
        lines.push(Line::from(week_spans));
    }

    lines.push(Line::styled(
        "  None  .   Low  :  *  O  #  High",
        Style::default().fg(Color::DarkGray),
    ));

    let widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title))
        .style(Style::default().fg(Color::Gray))
        .wrap(Wrap { trim: false });
    frame.render_widget(widget, area);
}

pub(super) fn format_history_goal_streak_line(app: &App) -> String {
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
        "Goals: Off   Streaks: Off".to_string()
    }
}

pub(super) fn format_history_focus_score_line(app: &App) -> String {
    match app.latest_weekly_focus_score() {
        Some(score) => match (score.focus_score_pct, score.completion_score_pct) {
            (Some(focus_score), Some(completion_score)) => format!(
                "Focus score: {focus_score}% (consistency {}% · completion {completion_score}%)",
                score.consistency_score_pct
            ),
            _ => format!(
                "Focus score: n/a (weekly goal off; consistency {}% ({}/7 days))",
                score.consistency_score_pct, score.active_days
            ),
        },
        None => "Focus score: n/a".to_string(),
    }
}

pub(super) fn format_history_interruption_line(app: &App) -> String {
    match app.latest_session_interruption() {
        Some(event) => format!(
            "Last interruption: {} · {} remaining · {}",
            event.reason.label(),
            format_duration_label(event.remaining_secs),
            event.task_label.unwrap_or_else(|| "Unlabeled".to_string())
        ),
        None => "Last interruption: none".to_string(),
    }
}

pub(super) fn format_task_trend_delta(trend: &crate::stats::TaskTrend) -> String {
    let delta_minutes = trend.delta_focused_minutes();
    if delta_minutes > 0 {
        format!("+{delta_minutes}m")
    } else if delta_minutes < 0 {
        format!("{delta_minutes}m")
    } else {
        "0m".to_string()
    }
}

pub(super) fn format_task_goal_progress_summary(
    progress: &crate::stats::TaskGoalProgress,
) -> String {
    if !progress.target.has_any_target() {
        return "g:off".to_string();
    }
    if progress.met {
        "g:met".to_string()
    } else {
        "g:in".to_string()
    }
}

fn heatmap_cell_symbol(focused_minutes: u64, max_focused_minutes: u64) -> (char, Color) {
    if focused_minutes == 0 || max_focused_minutes == 0 {
        return ('.', Color::DarkGray);
    }

    let scaled = (focused_minutes.saturating_mul(4))
        .saturating_add(max_focused_minutes.saturating_sub(1))
        / max_focused_minutes;
    match scaled.clamp(1, 4) {
        1 => (':', Color::Green),
        2 => ('*', Color::Yellow),
        3 => ('O', Color::LightRed),
        _ => ('#', Color::Red),
    }
}

pub(super) fn format_month_label(year: i32, month: u32) -> String {
    format!("{year:04}-{month:02}")
}
