use crate::ui::{
    Alignment, App, Block, BlocklistProfileInputMode, Borders, Color, Constraint, Direction, Frame,
    Layout, Line, List, ListItem, ListState, Modifier, NavigationAction, Paragraph, Rect,
    ShortcutAction, SiteFeedbackLevel, SiteInputMode, SiteListMode, Span, Style, TimerPhase,
    TimerStatus, app_color, centered_rect, format_duration_label, render_centered_error,
    render_hint_lines,
};

pub(super) fn render_site_manager(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let outer = centered_rect(70, 82, area);

    let block_color = if app.blocker.is_blocking {
        app_color(app, Color::Red)
    } else {
        app_color(app, Color::Green)
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
            Constraint::Min(4),    // site list
            Constraint::Length(3), // site input area
            Constraint::Length(3), // profile input area
            Constraint::Length(1), // error/feedback line
            Constraint::Length(3), // key hints
        ])
        .split(outer);

    let status_text = site_manager_status_span(app);
    frame.render_widget(
        Paragraph::new(Line::from(status_text)).alignment(Alignment::Center),
        inner[0],
    );

    let site_list_mode = app.site_list_mode();
    let profile_text = format!(
        "Profile: {} ({}/{}) · Category: {} ({}/{}) · List: {} · Effective blocks: {}",
        app.active_blocklist_profile_name(),
        app.active_blocklist_profile_position(),
        app.blocklist_profile_count(),
        app.active_blocklist_category_name(),
        app.active_blocklist_category_position(),
        app.blocklist_category_count(),
        site_list_mode.label(),
        app.effective_blocked_site_count()
    );
    frame.render_widget(
        Paragraph::new(profile_text)
            .alignment(Alignment::Center)
            .style(Style::default().fg(app_color(app, Color::Cyan))),
        inner[1],
    );

    let input_mode = app.site_input_mode();
    let profile_input_mode = app.blocklist_profile_input_mode();
    let (list_title_label, empty_text, idle_input_text) =
        site_manager_copy_for_mode(app, site_list_mode);
    render_site_manager_site_list(frame, app, inner[2], &list_title_label, &empty_text);
    render_site_manager_input(
        frame,
        app,
        inner[3],
        input_mode,
        site_list_mode,
        &idle_input_text,
    );
    render_site_manager_profile_input(frame, app, inner[4], profile_input_mode);
    render_site_manager_feedback_line(frame, app, inner[5]);
    render_hint_lines(
        frame,
        app,
        inner[6],
        site_manager_hint_lines(app, site_list_mode, input_mode),
    );
}

fn site_manager_status_span(app: &App) -> Span<'static> {
    let focus_session_active =
        app.timer.phase == TimerPhase::Focus && app.timer.status != TimerStatus::Idle;
    if app.blocker.is_blocking {
        return Span::styled(
            "Blocking is ACTIVE during this focus session",
            Style::default()
                .fg(app_color(app, Color::Red))
                .add_modifier(Modifier::BOLD),
        );
    }

    if app.break_glass_override_active() {
        let remaining_secs = app.break_glass_override_remaining_secs().unwrap_or(0);
        return Span::styled(
            format!(
                "Break-glass override active — blocking paused ({} left)",
                format_duration_label(remaining_secs)
            ),
            Style::default().fg(app_color(app, Color::Yellow)),
        );
    }

    if focus_session_active {
        if app.block_error.is_some() {
            return Span::styled(
                format!(
                    "Focus session active — blocking unavailable (permission/setup issue; open {} Setup)",
                    app.shortcut_hint(ShortcutAction::OpenSetupDiagnostics)
                ),
                Style::default().fg(app_color(app, Color::Yellow)),
            );
        }
        if app.effective_blocked_site_count() == 0 {
            return Span::styled(
                "Focus session active — blocking inactive (effective blocked set is empty)",
                Style::default().fg(app_color(app, Color::Yellow)),
            );
        }
        return Span::styled(
            format!(
                "Focus session active — blocking unavailable (open {} Setup)",
                app.shortcut_hint(ShortcutAction::OpenSetupDiagnostics)
            ),
            Style::default().fg(app_color(app, Color::Yellow)),
        );
    }

    Span::styled(
        "Blocking will activate when a focus session starts",
        Style::default().fg(app_color(app, Color::Gray)),
    )
}

fn site_manager_copy_for_mode(app: &App, site_list_mode: SiteListMode) -> (String, String, String) {
    match site_list_mode {
        SiteListMode::Blocklist => (
            "Blocklist Sites".to_string(),
            format!(
                "  No blocked sites yet. Press {} to add one.",
                app.shortcut_hint(ShortcutAction::SiteAdd)
            ),
            format!(
                "Press {} to add/import blocked sites or {} to edit selected",
                app.shortcut_hint(ShortcutAction::SiteAdd),
                app.shortcut_hint(ShortcutAction::SiteEdit)
            ),
        ),
        SiteListMode::Allowlist => (
            "Allowlist Exceptions".to_string(),
            format!(
                "  No allowlist exceptions yet. Press {} to add one.",
                app.shortcut_hint(ShortcutAction::SiteAdd)
            ),
            format!(
                "Press {} to add allowlist exceptions (HOST=30m for temporary) or {} to edit selected",
                app.shortcut_hint(ShortcutAction::SiteAdd),
                app.shortcut_hint(ShortcutAction::SiteEdit)
            ),
        ),
    }
}

fn render_site_manager_site_list(
    frame: &mut Frame,
    app: &App,
    area: Rect,
    list_title_label: &str,
    empty_text: &str,
) {
    let list_title = format!(
        " {list_title_label} · {} / {} ({}) ",
        app.active_blocklist_profile_name(),
        app.active_blocklist_category_name(),
        app.active_policy_site_count()
    );
    let list_block = Block::default()
        .borders(Borders::ALL)
        .title(list_title)
        .style(Style::default().fg(app_color(app, Color::Gray)));

    if app.active_policy_sites().is_empty() {
        let empty = Paragraph::new(empty_text)
            .style(Style::default().fg(app_color(app, Color::DarkGray)))
            .block(list_block);
        frame.render_widget(empty, area);
        return;
    }

    let items: Vec<ListItem> = app
        .active_policy_sites()
        .iter()
        .map(|s| ListItem::new(format!("  {s}")))
        .collect();
    let list = List::new(items)
        .block(list_block)
        .highlight_style(
            Style::default()
                .fg(app_color(app, Color::Black))
                .bg(app_color(app, Color::White))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");
    let mut list_state = ListState::default();
    list_state.select(Some(app.selected_site));
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_site_manager_input(
    frame: &mut Frame,
    app: &App,
    area: Rect,
    input_mode: SiteInputMode,
    site_list_mode: SiteListMode,
    idle_input_text: &str,
) {
    let input_title = match input_mode {
        SiteInputMode::Add => match site_list_mode {
            SiteListMode::Blocklist => " Add / Import Blocklist Sites ",
            SiteListMode::Allowlist => " Add Allowlist Sites / Temporary Exceptions ",
        },
        SiteInputMode::Edit => match site_list_mode {
            SiteListMode::Blocklist => " Edit Blocklist Site ",
            SiteListMode::Allowlist => " Edit Allowlist Site ",
        },
    };
    let active_style = if app.site_input_active {
        Style::default().fg(app_color(app, Color::Yellow))
    } else {
        Style::default().fg(app_color(app, Color::DarkGray))
    };
    let input_text = if app.site_input_active {
        format!("{}_", app.site_input)
    } else {
        idle_input_text.to_string()
    };

    frame.render_widget(
        Paragraph::new(input_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(input_title)
                    .style(active_style),
            )
            .style(active_style),
        area,
    );
}

fn render_site_manager_profile_input(
    frame: &mut Frame,
    app: &App,
    area: Rect,
    profile_input_mode: Option<BlocklistProfileInputMode>,
) {
    let profile_input_title = match profile_input_mode {
        Some(BlocklistProfileInputMode::Create) => " New Blocklist Profile ",
        Some(BlocklistProfileInputMode::Rename) => " Rename Blocklist Profile ",
        Some(BlocklistProfileInputMode::CreateCategory) => " Deprecated Blocklist Category ",
        Some(BlocklistProfileInputMode::RenameCategory) => " Deprecated Blocklist Category ",
        None => " Blocklist Profiles ",
    };
    let active_style = if app.blocklist_profile_input_active {
        Style::default().fg(app_color(app, Color::Yellow))
    } else {
        Style::default().fg(app_color(app, Color::DarkGray))
    };
    let profile_input_text = if app.blocklist_profile_input_active {
        format!("{}_", app.blocklist_profile_input)
    } else {
        format!(
            "Profiles [{} {}] switch · {} create/{} rename/{} delete · Categories deprecated [{} {}] switch",
            app.shortcut_label(ShortcutAction::SelectPreviousBlocklistProfile),
            app.shortcut_label(ShortcutAction::SelectNextBlocklistProfile),
            app.shortcut_hint(ShortcutAction::CreateBlocklistProfile),
            app.shortcut_hint(ShortcutAction::RenameBlocklistProfile),
            app.shortcut_hint(ShortcutAction::DeleteBlocklistProfile),
            app.navigation_label(NavigationAction::MoveLeft),
            app.navigation_label(NavigationAction::MoveRight),
        )
    };
    frame.render_widget(
        Paragraph::new(profile_input_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(profile_input_title)
                    .style(active_style),
            )
            .style(active_style),
        area,
    );
}

fn render_site_manager_feedback_line(frame: &mut Frame, app: &App, area: Rect) {
    if let Some(err) = app.block_error.as_ref() {
        let privilege_hint = if cfg!(target_os = "windows") {
            " (try running in an elevated command prompt)"
        } else {
            " (try running with elevated privileges)"
        };
        render_centered_error(
            frame,
            app,
            area,
            format!(
                "⚠  {err}{privilege_hint} · open {} Setup for remediation",
                app.shortcut_hint(ShortcutAction::OpenSetupDiagnostics)
            ),
        );
        return;
    }

    if let Some(err) = app.config_error.as_ref() {
        render_centered_error(frame, app, area, format!("⚠  {err}"));
        return;
    }

    if let Some(feedback) = app.site_feedback.as_ref() {
        let (prefix, color) = match feedback.level {
            SiteFeedbackLevel::Success => ("✓", app_color(app, Color::Green)),
            SiteFeedbackLevel::Warning => ("⚠", app_color(app, Color::Yellow)),
        };
        let feedback_widget = Paragraph::new(format!("{prefix}  {}", feedback.message))
            .alignment(Alignment::Center)
            .style(Style::default().fg(color));
        frame.render_widget(feedback_widget, area);
    }
}

fn site_manager_hint_lines(
    app: &App,
    site_list_mode: SiteListMode,
    input_mode: SiteInputMode,
) -> Vec<Line<'static>> {
    if app.site_input_active {
        return vec![
            Line::from(match input_mode {
                SiteInputMode::Add => format!(
                    "Input: {} Add/Import  {} Cancel",
                    app.navigation_hint(NavigationAction::Confirm),
                    app.navigation_hint(NavigationAction::Cancel)
                ),
                SiteInputMode::Edit => format!(
                    "Input: {} Save  {} Cancel",
                    app.navigation_hint(NavigationAction::Confirm),
                    app.navigation_hint(NavigationAction::Cancel)
                ),
            }),
            Line::from(match input_mode {
                SiteInputMode::Add => format!(
                    "Tip: comma/newline entries. In Allowlist mode, use HOST=30m for temporary exceptions, then press {}",
                    app.navigation_hint(NavigationAction::Confirm)
                ),
                SiteInputMode::Edit => format!(
                    "Tip: enter one hostname, then press {}",
                    app.navigation_hint(NavigationAction::Confirm)
                ),
            }),
            Line::from("Tip: disable DNS-over-HTTPS in your browser so blocking can apply"),
        ];
    }

    if app.blocklist_profile_input_active {
        let target = match app.blocklist_profile_input_mode() {
            Some(
                BlocklistProfileInputMode::CreateCategory
                | BlocklistProfileInputMode::RenameCategory,
            ) => "Category",
            _ => "Profile",
        };
        return vec![
            Line::from(format!(
                "{target}: {} Save  {} Cancel",
                app.navigation_hint(NavigationAction::Confirm),
                app.navigation_hint(NavigationAction::Cancel)
            )),
            Line::from(
                "Tip: categories are deprecated; manage blocklist/allowlist sites on profiles",
            ),
            Line::from("Tip: disable DNS-over-HTTPS in your browser so blocking can apply"),
        ];
    }

    if app.strict_mode_enforced_for_focus() {
        return vec![
            Line::from(format!(
                "Mode: {} Toggle ({})  Sites: {} Add  {} Edit  {}/{} Remove  {}/{} Move",
                app.shortcut_hint(ShortcutAction::ToggleSiteListMode),
                site_list_mode.label(),
                app.shortcut_hint(ShortcutAction::SiteAdd),
                app.shortcut_hint(ShortcutAction::SiteEdit),
                app.shortcut_hint(ShortcutAction::SiteDelete),
                app.navigation_hint(NavigationAction::Delete),
                app.navigation_hint(NavigationAction::MoveUp),
                app.navigation_hint(NavigationAction::MoveDown),
            )),
            Line::from(format!(
                "Profiles: [{} {}] Switch  {} New  {} Rename  {} Delete  Categories (deprecated): [{}/{}] Switch  [{}/{}] Back  [{}] Quit (Locked)",
                app.shortcut_label(ShortcutAction::SelectPreviousBlocklistProfile),
                app.shortcut_label(ShortcutAction::SelectNextBlocklistProfile),
                app.shortcut_hint(ShortcutAction::CreateBlocklistProfile),
                app.shortcut_hint(ShortcutAction::RenameBlocklistProfile),
                app.shortcut_hint(ShortcutAction::DeleteBlocklistProfile),
                app.navigation_label(NavigationAction::MoveLeft),
                app.navigation_label(NavigationAction::MoveRight),
                app.shortcut_label(ShortcutAction::BackSiteManager),
                app.navigation_label(NavigationAction::Cancel),
                app.shortcut_label(ShortcutAction::Quit),
            )),
            Line::from("Tip: disable DNS-over-HTTPS in your browser so blocking can apply"),
        ];
    }

    vec![
        Line::from(format!(
            "Mode: {} Toggle ({})  Sites: {} Add  {} Edit  {}/{} Remove  {}/{} Move",
            app.shortcut_hint(ShortcutAction::ToggleSiteListMode),
            site_list_mode.label(),
            app.shortcut_hint(ShortcutAction::SiteAdd),
            app.shortcut_hint(ShortcutAction::SiteEdit),
            app.shortcut_hint(ShortcutAction::SiteDelete),
            app.navigation_hint(NavigationAction::Delete),
            app.navigation_hint(NavigationAction::MoveUp),
            app.navigation_hint(NavigationAction::MoveDown),
        )),
        Line::from(format!(
            "Profiles: [{} {}] Switch  {} New  {} Rename  {} Delete  Categories (deprecated): [{}/{}] Switch  [{}/{}] Back",
            app.shortcut_label(ShortcutAction::SelectPreviousBlocklistProfile),
            app.shortcut_label(ShortcutAction::SelectNextBlocklistProfile),
            app.shortcut_hint(ShortcutAction::CreateBlocklistProfile),
            app.shortcut_hint(ShortcutAction::RenameBlocklistProfile),
            app.shortcut_hint(ShortcutAction::DeleteBlocklistProfile),
            app.navigation_label(NavigationAction::MoveLeft),
            app.navigation_label(NavigationAction::MoveRight),
            app.shortcut_label(ShortcutAction::BackSiteManager),
            app.navigation_label(NavigationAction::Cancel),
        )),
        Line::from("Tip: disable DNS-over-HTTPS in your browser so blocking can apply"),
    ]
}
