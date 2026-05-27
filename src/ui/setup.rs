use crate::ui::{
    Alignment, App, Block, BlockingPreviewAction, Borders, Color, Constraint, Direction, Frame,
    Layout, Line, Modifier, NavigationAction, Paragraph, Rect, SetupCheck, SetupCheckLevel,
    ShortcutAction, Span, Style, Wrap, app_color, centered_rect, render_hint_lines,
};

const DEPRECATION_WARNING_PANEL_LINES: usize = 4;
const MAX_VISIBLE_DEPRECATION_WARNINGS: usize = DEPRECATION_WARNING_PANEL_LINES - 2;

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
            Constraint::Length(1),                                      // hosts path
            Constraint::Length(1),                                      // backend policy/order
            Constraint::Length(0),                                      // spacer
            Constraint::Length(2),                                      // backend selection
            Constraint::Length(2),                                      // command backend
            Constraint::Length(2),                                      // blocking permissions
            Constraint::Length(2),                                      // hosts write capability
            Constraint::Length(2),                                      // wakatime config status
            Constraint::Length(2),                                      // encrypted sync status
            Constraint::Length(DEPRECATION_WARNING_PANEL_LINES as u16), // deprecation warnings
            Constraint::Length(1),                                      // preview summary
            Constraint::Min(0),                                         // preview section
            Constraint::Length(2),                                      // key hints
        ])
        .split(outer);

    let hosts_path = Paragraph::new(format!(
        "Hosts file: {}",
        app.setup_diagnostics.hosts_file_path
    ))
    .style(Style::default().fg(app_color(app, Color::DarkGray)));
    frame.render_widget(hosts_path, inner[0]);
    frame.render_widget(
        Paragraph::new(format!(
            "Backend policy: {} (order: {})",
            app.setup_diagnostics.backend_policy, app.setup_diagnostics.backend_order
        ))
        .style(Style::default().fg(app_color(app, Color::DarkGray))),
        inner[1],
    );

    render_setup_check(
        frame,
        app,
        inner[3],
        "Backend selection",
        &app.setup_diagnostics.backend_selection,
    );
    render_setup_check(
        frame,
        app,
        inner[4],
        "Command backend readiness",
        &app.setup_diagnostics.command_backend,
    );

    render_setup_check(
        frame,
        app,
        inner[5],
        "Blocking permissions",
        &app.setup_diagnostics.blocking_permissions,
    );
    render_setup_check(
        frame,
        app,
        inner[6],
        "Hosts write capability",
        &app.setup_diagnostics.hosts_write_capability,
    );
    render_setup_check(
        frame,
        app,
        inner[7],
        "WakaTime config status",
        &app.setup_diagnostics.wakatime_config,
    );
    render_setup_check(
        frame,
        app,
        inner[8],
        "Encrypted sync status",
        &app.setup_diagnostics.sync_status,
    );
    let deprecation_lines = if app.setup_diagnostics.deprecation_warnings.is_empty() {
        vec![Line::from("Deprecation warnings: none")]
    } else {
        let mut lines = vec![Line::from("Deprecation warnings:")];
        let warnings = &app.setup_diagnostics.deprecation_warnings;
        let hidden = warnings
            .len()
            .saturating_sub(MAX_VISIBLE_DEPRECATION_WARNINGS);
        lines.extend(
            warnings
                .iter()
                .take(MAX_VISIBLE_DEPRECATION_WARNINGS)
                .map(|warning| Line::from(format!("- {warning}"))),
        );
        if hidden > 0 {
            lines.push(Line::from(format!(
                "- +{hidden} more (run --diagnostics for full list)"
            )));
        }
        lines
    };
    frame.render_widget(
        Paragraph::new(deprecation_lines)
            .style(Style::default().fg(app_color(app, Color::Yellow)))
            .wrap(Wrap { trim: true }),
        inner[9],
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
        let backend = app
            .blocking_preview
            .backend
            .map(|backend| backend.id())
            .unwrap_or("unknown");
        (
            format!(
                "Preview backend: {backend} · action: {action} · fallback: {} · changes: {} · effective blocked sites: {}",
                if app.blocking_preview.fallback_used {
                    "yes"
                } else {
                    "no"
                },
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
        inner[10],
    );

    let preview_section_text = if app.blocking_preview.error.is_some() {
        "Preview section unavailable due to backend-access error.".to_string()
    } else if let Some(section) = app.blocking_preview.section.as_ref() {
        section.clone()
    } else {
        app.blocking_preview
            .backend_target
            .as_ref()
            .map(|target| format!("No hosts section preview. Backend target: {target}"))
            .unwrap_or_else(|| "No blocking preview details are available.".to_string())
    };
    frame.render_widget(
        Paragraph::new(preview_section_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Blocking preview details "),
            )
            .style(Style::default().fg(app_color(app, Color::Gray)))
            .wrap(Wrap { trim: false }),
        inner[11],
    );

    render_hint_lines(
        frame,
        app,
        inner[12],
        vec![
            Line::from(format!(
                "Diagnostics: {} Refresh checks + preview",
                app.shortcut_hint(ShortcutAction::RefreshSetupDiagnostics)
            )),
            Line::from(if app.strict_mode_enforced_for_focus() {
                format!(
                    "View: [{}/{}] Back  [{}/Ctrl-C] Quit (Locked)",
                    app.shortcut_label(ShortcutAction::BackSetupDiagnostics),
                    app.navigation_label(NavigationAction::Cancel),
                    app.shortcut_label(ShortcutAction::Quit),
                )
            } else {
                format!(
                    "View: [{}/{}] Back  [{}/Ctrl-C] Quit",
                    app.shortcut_label(ShortcutAction::BackSetupDiagnostics),
                    app.navigation_label(NavigationAction::Cancel),
                    app.shortcut_label(ShortcutAction::Quit),
                )
            }),
        ],
    );
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
