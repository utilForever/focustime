use crate::app::PlannerPane;
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
        .title(" Session Planner ")
        .title_alignment(Alignment::Center)
        .style(Style::default().fg(app_color(app, Color::Cyan)));
    frame.render_widget(block, outer);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(1), // current task
            Constraint::Min(4),    // task/template lists
            Constraint::Length(3), // task label input
            Constraint::Length(1), // feedback
            Constraint::Length(3), // hints
        ])
        .split(outer);
    let lists = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(inner[1]);

    render_session_planner_selected_task(frame, app, inner[0]);
    render_session_planner_labels(frame, app, lists[0]);
    render_session_planner_templates(frame, app, lists[1]);
    render_session_planner_input(frame, app, inner[2]);
    render_session_planner_feedback(frame, app, inner[3]);
    render_session_planner_hints(frame, app, inner[4]);
}

fn render_session_planner_selected_task(frame: &mut Frame, app: &App, area: Rect) {
    let selected_task = app.selected_task_label.as_ref().map_or_else(
        || "Selected task: none (required before focus starts)".to_string(),
        |label| format!("Selected task: {label}"),
    );
    let selected_template = app
        .active_session_template_name()
        .map_or_else(|| "none".to_string(), str::to_string);
    let selected_text = format!("{selected_task}   |   Active template: {selected_template}");
    frame.render_widget(
        Paragraph::new(Line::from(selected_text))
            .style(Style::default().fg(app_color(app, Color::White)))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_session_planner_labels(frame: &mut Frame, app: &App, area: Rect) {
    let title = if app.planner_pane == PlannerPane::Tasks {
        " Task Labels * "
    } else {
        " Task Labels "
    };
    if app.task_labels.is_empty() {
        frame.render_widget(
            Paragraph::new(format!(
                "No task labels yet. Press {} to add one.",
                app.shortcut_hint(ShortcutAction::PlannerAdd)
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

fn render_session_planner_templates(frame: &mut Frame, app: &App, area: Rect) {
    let title = if app.planner_pane == PlannerPane::Templates {
        " Session Templates * "
    } else {
        " Session Templates "
    };
    if app.session_templates.is_empty() {
        frame.render_widget(
            Paragraph::new(format!(
                "No templates yet. Press {} to capture current task/profile/blocklist/schedule.",
                app.shortcut_hint(ShortcutAction::PlannerAdd)
            ))
            .style(Style::default().fg(app_color(app, Color::DarkGray)))
            .wrap(Wrap { trim: true })
            .block(Block::default().borders(Borders::ALL).title(title)),
            area,
        );
        return;
    }

    let items: Vec<ListItem> = app
        .session_templates
        .iter()
        .map(|template| {
            let marker = if app
                .active_session_template_name()
                .is_some_and(|active| active.eq_ignore_ascii_case(&template.name))
            {
                "✓"
            } else {
                " "
            };
            let schedule_windows =
                template.schedule.windows.len() + template.schedule.one_time_windows.len();
            ListItem::new(format!(
                " {marker} {} ({}, {}, {}, {}w)",
                template.name,
                template.profile.label(),
                template.task_label,
                template.blocklist_profile,
                schedule_windows
            ))
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
        app.planner_template_selection_index
            .min(app.session_templates.len().saturating_sub(1)),
    ));
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_session_planner_input(frame: &mut Frame, app: &App, area: Rect) {
    let input_title = if app.planner_input_active {
        match app.planner_input_mode {
            Some(PlannerInputMode::Rename) => " Rename task label ".to_string(),
            Some(PlannerInputMode::CreateTemplate) => " Create session template ".to_string(),
            Some(PlannerInputMode::RenameTemplate) => " Rename session template ".to_string(),
            _ => " Add task label ".to_string(),
        }
    } else {
        match app.planner_pane {
            PlannerPane::Tasks => format!(
                " Task input ({} add / {} rename / {} favorite / {} archive) ",
                app.shortcut_hint(ShortcutAction::PlannerAdd),
                app.shortcut_hint(ShortcutAction::PlannerRename),
                app.shortcut_hint(ShortcutAction::PlannerFavorite),
                app.shortcut_hint(ShortcutAction::PlannerArchive),
            ),
            PlannerPane::Templates => format!(
                " Template input ({} create / {} rename) ",
                app.shortcut_hint(ShortcutAction::PlannerAdd),
                app.shortcut_hint(ShortcutAction::PlannerRename),
            ),
        }
    };
    let input_text = if app.planner_input_active {
        format!("{}|", app.planner_input)
    } else {
        match app.planner_pane {
            PlannerPane::Tasks => format!(
                "Use {} add, {} rename, {} favorite, {} archive, {}/{} delete highlighted",
                app.shortcut_hint(ShortcutAction::PlannerAdd),
                app.shortcut_hint(ShortcutAction::PlannerRename),
                app.shortcut_hint(ShortcutAction::PlannerFavorite),
                app.shortcut_hint(ShortcutAction::PlannerArchive),
                app.shortcut_hint(ShortcutAction::PlannerDelete),
                app.navigation_hint(NavigationAction::Delete),
            ),
            PlannerPane::Templates => format!(
                "Use {} apply, {} create, {} rename, {}/{} delete highlighted",
                app.navigation_hint(NavigationAction::Confirm),
                app.shortcut_hint(ShortcutAction::PlannerAdd),
                app.shortcut_hint(ShortcutAction::PlannerRename),
                app.shortcut_hint(ShortcutAction::PlannerDelete),
                app.navigation_hint(NavigationAction::Delete),
            ),
        }
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
                app.shortcut_label(ShortcutAction::PlannerSelectRecent)
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
            Some(PlannerInputMode::CreateTemplate) => format!(
                "Input: type template name, then {}",
                app.navigation_hint(NavigationAction::Confirm)
            ),
            Some(PlannerInputMode::RenameTemplate) => format!(
                "Input: rename template, then {}",
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
        let planner_line = match app.planner_pane {
            PlannerPane::Tasks => format!(
                "Planner: {}/{} Move  {}/{} Pane  {} Select  {} Add  {} Rename  {} Favorite  {} Archive  {}/{} Delete",
                app.navigation_hint(NavigationAction::MoveUp),
                app.navigation_hint(NavigationAction::MoveDown),
                app.navigation_hint(NavigationAction::MoveLeft),
                app.navigation_hint(NavigationAction::MoveRight),
                app.navigation_hint(NavigationAction::Confirm),
                app.shortcut_hint(ShortcutAction::PlannerAdd),
                app.shortcut_hint(ShortcutAction::PlannerRename),
                app.shortcut_hint(ShortcutAction::PlannerFavorite),
                app.shortcut_hint(ShortcutAction::PlannerArchive),
                app.shortcut_hint(ShortcutAction::PlannerDelete),
                app.navigation_hint(NavigationAction::Delete),
            ),
            PlannerPane::Templates => format!(
                "Planner: {}/{} Move  {}/{} Pane  {} Apply  {} Create  {} Rename  {}/{} Delete",
                app.navigation_hint(NavigationAction::MoveUp),
                app.navigation_hint(NavigationAction::MoveDown),
                app.navigation_hint(NavigationAction::MoveLeft),
                app.navigation_hint(NavigationAction::MoveRight),
                app.navigation_hint(NavigationAction::Confirm),
                app.shortcut_hint(ShortcutAction::PlannerAdd),
                app.shortcut_hint(ShortcutAction::PlannerRename),
                app.shortcut_hint(ShortcutAction::PlannerDelete),
                app.navigation_hint(NavigationAction::Delete),
            ),
        };
        let detail_line = match app.planner_pane {
            PlannerPane::Tasks => format!(
                "{}  |  Archived labels stay visible and cannot be selected",
                planner_recent_quick_pick_text(app)
            ),
            PlannerPane::Templates => {
                "Templates apply task + profile + blocklist + schedule together".to_string()
            }
        };
        vec![
            Line::from(planner_line),
            Line::from(detail_line),
            Line::from(if app.strict_mode_enforced_for_focus() {
                format!(
                    "View: [{}/{}] Back  [{}/Ctrl-C] Quit (Locked)",
                    app.shortcut_label(ShortcutAction::BackSessionPlanner),
                    app.navigation_label(NavigationAction::Cancel),
                    app.shortcut_label(ShortcutAction::Quit),
                )
            } else {
                format!(
                    "View: [{}/{}] Back  [{}/Ctrl-C] Quit",
                    app.shortcut_label(ShortcutAction::BackSessionPlanner),
                    app.navigation_label(NavigationAction::Cancel),
                    app.shortcut_label(ShortcutAction::Quit),
                )
            }),
        ]
    };
    render_hint_lines(frame, app, area, hints);
}
