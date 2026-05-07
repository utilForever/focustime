use crate::ui::{
    Alignment, App, Block, BlockingPreviewAction, Borders, Color, Constraint, Direction, Frame,
    Layout, Line, Modifier, Paragraph, Rect, SetupCheck, SetupCheckLevel, ShortcutAction, Span,
    Style, Wrap, app_color, centered_rect, render_hint_lines,
};

pub(super) fn render_setup_diagnostics(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let outer = centered_rect(72, 68, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Setup Diagnostics ")
        .title_alignment(Alignment::Center)
        .style(Style::default().fg(app_color(app, Color::Cyan)));
    frame.render_widget(block, outer);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(1), // hosts path
            Constraint::Length(1), // spacer
            Constraint::Length(2), // blocking permissions
            Constraint::Length(2), // hosts write capability
            Constraint::Length(2), // wakatime config status
            Constraint::Length(2), // feature flags
            Constraint::Length(1), // preview summary
            Constraint::Min(0),    // preview section
            Constraint::Length(2), // key hints
        ])
        .split(outer);

    let hosts_path = Paragraph::new(format!(
        "Hosts file: {}",
        app.setup_diagnostics.hosts_file_path
    ))
    .style(Style::default().fg(app_color(app, Color::DarkGray)));
    frame.render_widget(hosts_path, inner[0]);

    render_setup_check(
        frame,
        app,
        inner[2],
        "Blocking permissions",
        &app.setup_diagnostics.blocking_permissions,
    );
    render_setup_check(
        frame,
        app,
        inner[3],
        "Hosts write capability",
        &app.setup_diagnostics.hosts_write_capability,
    );
    render_setup_check(
        frame,
        app,
        inner[4],
        "WakaTime config status",
        &app.setup_diagnostics.wakatime_config,
    );
    frame.render_widget(
        Paragraph::new(format!(
            "Feature flags: automation-mirror={} · blocked-sites-mirror={} · metadata-fallback={}",
            bool_label(app.setup_diagnostics.feature_flags.legacy_automation_mirror),
            bool_label(
                app.setup_diagnostics
                    .feature_flags
                    .legacy_blocked_sites_mirror
            ),
            bool_label(
                app.setup_diagnostics
                    .feature_flags
                    .metadata_task_label_fallback
            )
        ))
        .style(Style::default().fg(app_color(app, Color::DarkGray)))
        .wrap(Wrap { trim: true }),
        inner[5],
    );

    let (preview_summary, preview_style) = if let Some(error) = app.blocking_preview.error.as_ref()
    {
        (
            format!("Preview unavailable: {error}"),
            Style::default().fg(app_color(app, Color::Yellow)),
        )
    } else {
        let action = match app.blocking_preview.action {
            BlockingPreviewAction::Block => "block",
            BlockingPreviewAction::Unblock => "unblock",
            BlockingPreviewAction::NoChange => "no-change",
        };
        (
            format!(
                "Preview action: {action} · changes: {} · effective blocked sites: {}",
                if app.blocking_preview.would_change {
                    "yes"
                } else {
                    "no"
                },
                app.blocking_preview.effective_blocked_sites_count
            ),
            Style::default().fg(app_color(app, Color::DarkGray)),
        )
    };
    frame.render_widget(
        Paragraph::new(preview_summary)
            .alignment(Alignment::Left)
            .style(preview_style),
        inner[6],
    );

    let preview_section_text = if app.blocking_preview.error.is_some() {
        "Preview section unavailable due to hosts-file access error.".to_string()
    } else if let Some(section) = app.blocking_preview.section.as_ref() {
        section.clone()
    } else {
        "No focustime block section changes are required for the current state.".to_string()
    };
    frame.render_widget(
        Paragraph::new(preview_section_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Blocking preview (focustime section) "),
            )
            .style(Style::default().fg(app_color(app, Color::Gray)))
            .wrap(Wrap { trim: false }),
        inner[7],
    );

    render_hint_lines(
        frame,
        app,
        inner[8],
        vec![
            Line::from(format!(
                "Diagnostics: {} Refresh checks + preview",
                app.shortcut_hint(ShortcutAction::RefreshSetupDiagnostics)
            )),
            Line::from(if app.strict_mode_enforced_for_focus() {
                format!(
                    "View: [{}/Esc] Back  [{}/Ctrl-C] Quit (Locked)",
                    app.shortcut_label(ShortcutAction::BackSetupDiagnostics),
                    app.shortcut_label(ShortcutAction::Quit),
                )
            } else {
                format!(
                    "View: [{}/Esc] Back  [{}/Ctrl-C] Quit",
                    app.shortcut_label(ShortcutAction::BackSetupDiagnostics),
                    app.shortcut_label(ShortcutAction::Quit),
                )
            }),
        ],
    );
}

fn bool_label(enabled: bool) -> &'static str {
    if enabled { "on" } else { "off" }
}

pub(super) fn render_setup_check(
    frame: &mut Frame,
    app: &App,
    area: Rect,
    label: &str,
    check: &SetupCheck,
) {
    let (icon, status_color) = match check.level {
        SetupCheckLevel::Ok => ("✓", app_color(app, Color::Green)),
        SetupCheckLevel::Warning => ("⚠", app_color(app, Color::Yellow)),
    };
    let line = Line::from(vec![
        Span::styled(
            format!("{icon} {label}: "),
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            check.message.as_str(),
            Style::default().fg(app_color(app, Color::Gray)),
        ),
    ]);
    frame.render_widget(Paragraph::new(line).wrap(Wrap { trim: true }), area);
}
