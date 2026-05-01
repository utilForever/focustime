use super::*;

pub(super) fn render_site_manager(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let outer = centered_rect(70, 82, area);

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
        "Profile: {} ({}/{}) · List: {} · Effective blocks: {}",
        app.active_blocklist_profile_name(),
        app.active_blocklist_profile_position(),
        app.blocklist_profile_count(),
        site_list_mode.label(),
        app.effective_blocked_site_count()
    );
    frame.render_widget(
        Paragraph::new(profile_text)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Cyan)),
        inner[1],
    );

    let input_mode = app.site_input_mode();
    let profile_input_mode = app.blocklist_profile_input_mode();
    let (list_title_label, empty_text, idle_input_text) =
        site_manager_copy_for_mode(site_list_mode);
    render_site_manager_site_list(frame, app, inner[2], list_title_label, empty_text);
    render_site_manager_input(
        frame,
        app,
        inner[3],
        input_mode,
        site_list_mode,
        idle_input_text,
    );
    render_site_manager_profile_input(frame, app, inner[4], profile_input_mode);
    render_site_manager_feedback_line(frame, app, inner[5]);
    render_hint_lines(
        frame,
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
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        );
    }

    if app.break_glass_override_active() {
        let remaining_secs = app.break_glass_override_remaining_secs().unwrap_or(0);
        return Span::styled(
            format!(
                "Break-glass override active — blocking paused ({} left)",
                format_duration_label(remaining_secs)
            ),
            Style::default().fg(Color::Yellow),
        );
    }

    if focus_session_active {
        if app.block_error.is_some() {
            return Span::styled(
                "Focus session active — blocking unavailable (permission/setup issue; open [d] Setup)",
                Style::default().fg(Color::Yellow),
            );
        }
        if app.effective_blocked_site_count() == 0 {
            return Span::styled(
                "Focus session active — blocking inactive (effective blocked set is empty)",
                Style::default().fg(Color::Yellow),
            );
        }
        return Span::styled(
            "Focus session active — blocking unavailable (open [d] Setup)",
            Style::default().fg(Color::Yellow),
        );
    }

    Span::styled(
        "Blocking will activate when a focus session starts",
        Style::default().fg(Color::Gray),
    )
}

fn site_manager_copy_for_mode(
    site_list_mode: SiteListMode,
) -> (&'static str, &'static str, &'static str) {
    match site_list_mode {
        SiteListMode::Blocklist => (
            "Blocklist Sites",
            "  No blocked sites yet. Press [a] to add one.",
            "Press [a] to add/import blocked sites or [e] to edit selected",
        ),
        SiteListMode::Allowlist => (
            "Allowlist Exceptions",
            "  No allowlist exceptions yet. Press [a] to add one.",
            "Press [a] to add/import allowlist exceptions or [e] to edit selected",
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
        " {list_title_label} · {} ({}) ",
        app.active_blocklist_profile_name(),
        app.active_policy_site_count()
    );
    let list_block = Block::default()
        .borders(Borders::ALL)
        .title(list_title)
        .style(Style::default().fg(Color::Gray));

    if app.active_policy_sites().is_empty() {
        let empty = Paragraph::new(empty_text)
            .style(Style::default().fg(Color::DarkGray))
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
                .fg(Color::Black)
                .bg(Color::White)
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
            SiteListMode::Allowlist => " Add / Import Allowlist Sites ",
        },
        SiteInputMode::Edit => match site_list_mode {
            SiteListMode::Blocklist => " Edit Blocklist Site ",
            SiteListMode::Allowlist => " Edit Allowlist Site ",
        },
    };
    let active_style = if app.site_input_active {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
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
        None => " Blocklist Profiles ",
    };
    let active_style = if app.blocklist_profile_input_active {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let profile_input_text = if app.blocklist_profile_input_active {
        format!("{}_", app.blocklist_profile_input)
    } else {
        "Use [m] to toggle blocklist/allowlist, [n] create, [r] rename, [x] delete, [[ ] switch"
            .to_string()
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
            area,
            format!("⚠  {err}{privilege_hint} · open [d] Setup for remediation"),
        );
        return;
    }

    if let Some(err) = app.config_error.as_ref() {
        render_centered_error(frame, area, format!("⚠  {err}"));
        return;
    }

    if let Some(feedback) = app.site_feedback.as_ref() {
        let (prefix, color) = match feedback.level {
            SiteFeedbackLevel::Success => ("✓", Color::Green),
            SiteFeedbackLevel::Warning => ("⚠", Color::Yellow),
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
                SiteInputMode::Add => "Input: [Enter] Add/Import  [Esc] Cancel",
                SiteInputMode::Edit => "Input: [Enter] Save  [Esc] Cancel",
            }),
            Line::from(match input_mode {
                SiteInputMode::Add => "Tip: paste comma/newline hostnames, then press [Enter]",
                SiteInputMode::Edit => "Tip: enter one hostname, then press [Enter]",
            }),
            Line::from("Tip: disable DNS-over-HTTPS in your browser so blocking can apply"),
        ];
    }

    if app.blocklist_profile_input_active {
        return vec![
            Line::from("Profile: [Enter] Save  [Esc] Cancel"),
            Line::from("Tip: use descriptive names like Work, Study, or Deep Work"),
            Line::from("Tip: disable DNS-over-HTTPS in your browser so blocking can apply"),
        ];
    }

    if app.strict_mode_enforced_for_focus() {
        return vec![
            Line::from(format!(
                "Mode: [m] Toggle ({})  Sites: [a] Add  [e] Edit  [d/Del] Remove  [↑/↓] Move",
                site_list_mode.label()
            )),
            Line::from(
                "Profiles: [[ ] Switch  [n] New  [r] Rename  [x] Delete  [b/Esc] Back  [q] Quit (Locked)",
            ),
            Line::from("Tip: disable DNS-over-HTTPS in your browser so blocking can apply"),
        ];
    }

    vec![
        Line::from(format!(
            "Mode: [m] Toggle ({})  Sites: [a] Add  [e] Edit  [d/Del] Remove  [↑/↓] Move",
            site_list_mode.label()
        )),
        Line::from("Profiles: [[ ] Switch  [n] New  [r] Rename  [x] Delete  [b/Esc] Back"),
        Line::from("Tip: disable DNS-over-HTTPS in your browser so blocking can apply"),
    ]
}
