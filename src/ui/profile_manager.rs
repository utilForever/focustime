use crate::ui::{
    Alignment, App, Block, Borders, Color, Constraint, Direction, Frame, Layout, Line, List,
    ListItem, ListState, Modifier, PROFILE_EDIT_GROUPS, PROFILE_IDS, Paragraph, ShortcutAction,
    Span, Style, Wrap, centered_rect, render_centered_error, render_hint_lines,
};

pub(super) fn render_profile_manager(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let outer = centered_rect(56, 72, area);
    let editor_height = profile_editor_height(app);

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
            Constraint::Length(2), // current profile + break template
            Constraint::Length(6), // profile list
            Constraint::Length(editor_height),
            Constraint::Min(0),    // spacer
            Constraint::Length(1), // error line
            Constraint::Length(3), // key hints
        ])
        .split(outer);

    let current = Paragraph::new(format!(
        "Current profile: {} · Break template: {} ({})",
        app.selected_profile_name(),
        app.active_break_template_name(),
        app.active_break_template_summary()
    ))
    .style(
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )
    .wrap(Wrap { trim: true });
    frame.render_widget(current, inner[0]);

    let items = profile_list_items(app);

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
    let selected_index = if PROFILE_IDS.is_empty() {
        None
    } else {
        Some(
            app.profile_selection_index
                .min(PROFILE_IDS.len().saturating_sub(1)),
        )
    };
    list_state.select(selected_index);
    frame.render_stateful_widget(list, inner[1], &mut list_state);

    let editor_block = profile_editor_block(app);
    let lines = profile_editor_lines(app);
    frame.render_widget(Paragraph::new(lines).block(editor_block), inner[2]);

    if let Some(err) = app.config_error.as_ref() {
        render_centered_error(frame, inner[4], format!("⚠  {err}"));
    }

    render_hint_lines(frame, inner[5], profile_manager_hints(app));
}

fn profile_list_items(app: &App) -> Vec<ListItem<'static>> {
    PROFILE_IDS
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
        .collect()
}

fn profile_editor_block(app: &App) -> Block<'static> {
    let editor_title = if app.profile_edit_active {
        " Settings editor ".to_string()
    } else {
        format!(
            " Settings ({} to edit) ",
            app.shortcut_hint(ShortcutAction::ProfileEdit)
        )
    };
    Block::default()
        .borders(Borders::ALL)
        .title(editor_title)
        .style(if app.profile_edit_active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        })
}

fn profile_editor_lines(app: &App) -> Vec<Line<'static>> {
    if !app.profile_edit_active {
        return vec![
            Line::from("Press [e] to edit the selected profile."),
            Line::from("Sections: Timer · Automation · Goals · WakaTime · Schedule"),
            Line::from("Editor is compact for smaller terminals."),
        ];
    }

    let (group_name, fields) = profile_edit_group(app.profile_edit_field);
    let selected_pos = fields
        .iter()
        .position(|index| *index == app.profile_edit_field)
        .unwrap_or(0);
    const MAX_VISIBLE_FIELDS: usize = 8;
    let max_start = fields.len().saturating_sub(MAX_VISIBLE_FIELDS);
    let mut start = selected_pos.saturating_sub(MAX_VISIBLE_FIELDS / 2);
    if start > max_start {
        start = max_start;
    }
    let end = (start + MAX_VISIBLE_FIELDS).min(fields.len());

    let mut lines = Vec::new();
    lines.push(Line::styled(
        format!(
            " {group_name} settings ({}/{}) ",
            selected_pos + 1,
            fields.len()
        ),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ));
    if start > 0 {
        lines.push(Line::styled(
            "… ↑ more fields",
            Style::default().fg(Color::DarkGray),
        ));
    }
    for index in &fields[start..end] {
        let value = app.profile_edit_field_value(*index);
        let label = profile_edit_field_display_label(*index);
        let mut line = Line::from(format!("{label:<18} {value}"));
        if *index == app.profile_edit_field {
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
    if end < fields.len() {
        lines.push(Line::styled(
            "… ↓ more fields",
            Style::default().fg(Color::DarkGray),
        ));
    }
    lines
}

fn profile_manager_hints(app: &App) -> Vec<Line<'static>> {
    if app.profile_edit_active {
        vec![
            Line::from("Sections: Timer · Automation · Goals · WakaTime · Schedule"),
            Line::from("Edit: [↑/↓] Field  [←/→] Change value  [Type/Backspace] WakaTime text"),
            Line::from(if app.strict_mode_enforced_for_focus() {
                format!(
                    "[Enter] Save  [Esc] Cancel  [{}/Ctrl-C] Quit (Locked)",
                    app.shortcut_label(ShortcutAction::Quit)
                )
            } else {
                format!(
                    "[Enter] Save  [Esc] Cancel  [{}/Ctrl-C] Quit",
                    app.shortcut_label(ShortcutAction::Quit)
                )
            }),
        ]
    } else {
        vec![
            Line::from(if app.strict_mode_enforced_for_focus() {
                format!(
                    "Profiles: [↑/↓] Move  [Enter] Apply (Locked)  {} Edit",
                    app.shortcut_hint(ShortcutAction::ProfileEdit)
                )
            } else {
                format!(
                    "Profiles: [↑/↓] Move  [Enter] Apply  {} Edit",
                    app.shortcut_hint(ShortcutAction::ProfileEdit)
                )
            }),
            Line::from(format!(
                "Templates: '{}' previous  '{}' next break template",
                app.shortcut_label(ShortcutAction::SelectPreviousBreakTemplate),
                app.shortcut_label(ShortcutAction::SelectNextBreakTemplate),
            )),
            Line::from(if app.strict_mode_enforced_for_focus() {
                format!(
                    "View: [{}/Esc] Back  [{}] Quit (Locked)",
                    app.shortcut_label(ShortcutAction::BackProfileManager),
                    app.shortcut_label(ShortcutAction::Quit),
                )
            } else {
                format!(
                    "View: [{}/Esc] Back  [{}] Quit",
                    app.shortcut_label(ShortcutAction::BackProfileManager),
                    app.shortcut_label(ShortcutAction::Quit),
                )
            }),
        ]
    }
}

fn profile_edit_field_display_label(field_index: usize) -> &'static str {
    match field_index {
        0 => "Focus",
        1 => "Short break",
        2 => "Long break",
        3 => "Long-break cadence",
        4 => "Phase notifications",
        5 => "Sound alert",
        6 => "Auto-start break",
        7 => "Auto-start focus",
        8 => "Strict focus mode",
        9 => "Daily goal minutes",
        10 => "Daily goal pomodoros",
        11 => "Daily goal carry-over",
        12 => "Weekly goal minutes",
        13 => "Weekly goal pomodoros",
        14 => "Weekly goal carry-over",
        15 => "Monthly goal minutes",
        16 => "Monthly goal pomodoros",
        17 => "Monthly goal carry-over",
        18 => "WakaTime project",
        19 => "WakaTime language",
        20 => "Window selector",
        21 => "Day selector",
        22 => "Day enabled",
        23 => "Start time",
        24 => "End time",
        25 => "Add/remove",
        26 => "Exception selector",
        27 => "Exception date",
        28 => "Exception add/remove",
        29 => "One-time selector",
        30 => "One-time date",
        31 => "One-time start",
        32 => "One-time end",
        33 => "One-time add/remove",
        34 => "Conflict inspector",
        _ => "",
    }
}

fn profile_edit_group(field_index: usize) -> (&'static str, &'static [usize]) {
    for (group_name, fields) in PROFILE_EDIT_GROUPS {
        if fields.contains(&field_index) {
            return (group_name, fields);
        }
    }
    PROFILE_EDIT_GROUPS[0]
}

fn profile_editor_height(app: &App) -> u16 {
    if !app.profile_edit_active {
        return 7;
    }

    let (_, fields) = profile_edit_group(app.profile_edit_field);
    let visible_rows = fields.len().min(8) as u16;
    let overflow_rows = if fields.len() > 8 { 2 } else { 0 };
    visible_rows + overflow_rows + 3
}
