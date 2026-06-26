use crate::ui::{
    Alignment, App, Block, Borders, Color, Constraint, Direction, Frame, Layout, Line, List,
    ListItem, ListState, Modifier, NavigationAction, PLANNER_RECENT_LABEL_LIMIT, Paragraph,
    PlannerFeedbackLevel, PlannerInputMode, Rect, ShortcutAction, Style, Wrap, app_color,
    centered_rect, render_hint_lines,
};

pub(super) fn render_session_planner(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let outer = centered_rect(52, 70, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Task Setup ")
        .title_alignment(Alignment::Center)
        .style(Style::default().fg(app_color(app, Color::Cyan)));
    frame.render_widget(block, outer);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(2), // current task + weekly allocation
            Constraint::Min(4),    // task label list
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
    let selected_task = app.selected_task_label.as_ref().map_or_else(
        || "Selected task: none (required before focus starts)".to_string(),
        |label| format!("Selected task: {label}"),
    );
    let weekly_allocation = planner_weekly_allocation_summary(app);
    let selected_text = vec![Line::from(selected_task), Line::from(weekly_allocation)];
    frame.render_widget(
        Paragraph::new(selected_text)
            .style(Style::default().fg(app_color(app, Color::White)))
            .wrap(Wrap { trim: true }),
        area,
    );
}

pub(super) fn planner_weekly_allocation_summary(app: &App) -> String {
    let allocation = app.weekly_daily_goal_allocation();
    if !allocation.has_any_target() {
        return "Weekly allocation: off".to_string();
    }
    if allocation.remaining_minutes == 0 && allocation.remaining_pomodoros == 0 {
        return format!(
            "Weekly allocation: met ({} day(s) left)",
            allocation.remaining_days_in_week
        );
    }
    let today_target = allocation.today_target();
    format!(
        "Weekly allocation: today {}m/{}p, left {}m/{}p ({}/{} day(s))",
        today_target.minutes,
        today_target.pomodoros,
        allocation.remaining_minutes,
        allocation.remaining_pomodoros,
        allocation.allocatable_days,
        allocation.remaining_days_in_week
    )
}

fn render_session_planner_labels(frame: &mut Frame, app: &App, area: Rect) {
    let title = " Task Labels ";
    if app.task_labels.is_empty() {
        frame.render_widget(
            Paragraph::new(format!(
                "No task labels yet. Press {} to add one.",
                app.shortcut_hint(ShortcutAction::TaskSetupAdd)
            ))
            .style(Style::default().fg(app_color(app, Color::DarkGray)))
            .block(Block::default().borders(Borders::ALL).title(title)),
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
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .fg(app_color(app, Color::Black))
                .bg(app_color(app, Color::White))
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
            Some(PlannerInputMode::Rename) => " Rename task label ".to_string(),
            _ => " Add task label ".to_string(),
        }
    } else {
        format!(
            " Task labels ({} add / {} rename / {} favorite / {} archive) ",
            app.shortcut_hint(ShortcutAction::TaskSetupAdd),
            app.shortcut_hint(ShortcutAction::TaskSetupRename),
            app.shortcut_hint(ShortcutAction::TaskSetupFavorite),
            app.shortcut_hint(ShortcutAction::TaskSetupArchive),
        )
    };
    let input_text = if app.planner_input_active {
        format!("{}|", app.planner_input)
    } else {
        format!(
            "Use {} add, {} rename, {} favorite, {} archive, {}/{} delete highlighted",
            app.shortcut_hint(ShortcutAction::TaskSetupAdd),
            app.shortcut_hint(ShortcutAction::TaskSetupRename),
            app.shortcut_hint(ShortcutAction::TaskSetupFavorite),
            app.shortcut_hint(ShortcutAction::TaskSetupArchive),
            app.shortcut_hint(ShortcutAction::TaskSetupDelete),
            app.navigation_hint(NavigationAction::Delete),
        )
    };
    frame.render_widget(
        Paragraph::new(input_text)
            .style(if app.planner_input_active {
                Style::default().fg(app_color(app, Color::White))
            } else {
                Style::default().fg(app_color(app, Color::DarkGray))
            })
            .block(Block::default().borders(Borders::ALL).title(input_title)),
        area,
    );
}

fn render_session_planner_feedback(frame: &mut Frame, app: &App, area: Rect) {
    if let Some(feedback) = app.planner_feedback.as_ref() {
        let (prefix, color) = match feedback.level {
            PlannerFeedbackLevel::Success => ("✓", app_color(app, Color::Green)),
            PlannerFeedbackLevel::Warning => ("⚠", app_color(app, Color::Yellow)),
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
            parts.push(format!(
                "[{}/1] {label}",
                app.shortcut_label(ShortcutAction::TaskSetupSelectRecent)
            ));
        } else {
            parts.push(format!("[{}] {label}", index + 1));
        }
    }
    format!("Recent: {}", parts.join("  "))
}

fn render_session_planner_hints(frame: &mut Frame, app: &App, area: Rect) {
    let hints = if app.planner_input_active {
        let commit_line = match app.planner_input_mode {
            Some(PlannerInputMode::Rename) => format!(
                "Input: rename label, then {}",
                app.navigation_hint(NavigationAction::Confirm)
            ),
            _ => format!(
                "Input: type task label, then {}",
                app.navigation_hint(NavigationAction::Confirm)
            ),
        };
        vec![
            Line::from(commit_line),
            Line::from(format!(
                "Input: {} Cancel",
                app.navigation_hint(NavigationAction::Cancel)
            )),
            Line::from(if app.strict_mode_enforced_for_focus() {
                format!(
                    "View: [{}/Ctrl-C] Quit (Locked)",
                    app.shortcut_label(ShortcutAction::Quit)
                )
            } else {
                format!(
                    "View: [{}/Ctrl-C] Quit",
                    app.shortcut_label(ShortcutAction::Quit)
                )
            }),
        ]
    } else {
        let planner_line = format!(
            "Tasks: {}/{} Move  {} Select  {} Add  {} Rename  {} Favorite  {} Archive  {}/{} Delete",
            app.navigation_hint(NavigationAction::MoveUp),
            app.navigation_hint(NavigationAction::MoveDown),
            app.navigation_hint(NavigationAction::Confirm),
            app.shortcut_hint(ShortcutAction::TaskSetupAdd),
            app.shortcut_hint(ShortcutAction::TaskSetupRename),
            app.shortcut_hint(ShortcutAction::TaskSetupFavorite),
            app.shortcut_hint(ShortcutAction::TaskSetupArchive),
            app.shortcut_hint(ShortcutAction::TaskSetupDelete),
            app.navigation_hint(NavigationAction::Delete),
        );
        let detail_line = format!(
            "{}  |  Archived labels stay visible and cannot be selected",
            planner_recent_quick_pick_text(app)
        );
        vec![
            Line::from(planner_line),
            Line::from(detail_line),
            Line::from(if app.strict_mode_enforced_for_focus() {
                format!(
                    "View: [{}/{}] Back  [{}/Ctrl-C] Quit (Locked)",
                    app.shortcut_label(ShortcutAction::BackTaskSetup),
                    app.navigation_label(NavigationAction::Cancel),
                    app.shortcut_label(ShortcutAction::Quit),
                )
            } else {
                format!(
                    "View: [{}/{}] Back  [{}/Ctrl-C] Quit",
                    app.shortcut_label(ShortcutAction::BackTaskSetup),
                    app.navigation_label(NavigationAction::Cancel),
                    app.shortcut_label(ShortcutAction::Quit),
                )
            }),
        ]
    };
    render_hint_lines(frame, app, area, hints);
}
