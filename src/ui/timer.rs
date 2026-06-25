use crate::ui::{
    Alignment, App, Block, Borders, Color, Constraint, Direction, Frame, Gauge, Layout, Line,
    Local, Modifier, NavigationAction, Paragraph, Rect, ShortcutAction, Style, TimeZone,
    TimerPhase, TimerStatus, WakatimeRuntimeState, Wrap, app_color, centered_rect,
    format_timer_goal_streak_line, readable_goal_streak_text, render_hint_lines,
};

pub(super) fn render_timer(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let outer = centered_rect(90, 90, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" focustime ")
        .title_alignment(Alignment::Center)
        .style(Style::default().fg(phase_color(app, app.timer.phase)));
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
                .fg(phase_color(app, app.timer.phase))
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
                .fg(app_color(app, Color::White))
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
            .style(Style::default().fg(app_color(app, Color::DarkGray))),
        layout[0],
    );

    let elapsed_ratio = 1.0 - app.timer.progress();
    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::NONE))
        .gauge_style(
            Style::default()
                .fg(phase_color(app, app.timer.phase))
                .bg(app_color(app, Color::DarkGray)),
        )
        .ratio(elapsed_ratio);
    frame.render_widget(gauge, layout[2]);
}

fn render_timer_session_panel(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" 🧭 Session ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let (status_text, strict_status_text, break_glass_status_text, temporary_allowlist_status_text) =
        timer_status_text(app);
    let profile_line = format!(
        "🗂  Profile: {} ({})",
        app.selected_profile_name(),
        app.profile_summary(app.selected_profile)
    );
    let (task_text, task_style) = if let Some(label) = app.current_task_label() {
        (
            format!("🎯 Task: {label}"),
            Style::default().fg(app_color(app, Color::LightYellow)),
        )
    } else {
        (
            format!(
                "🎯 Task: not selected ({} Planner)",
                app.shortcut_hint(ShortcutAction::OpenSessionPlanner)
            ),
            Style::default().fg(app_color(app, Color::Yellow)),
        )
    };
    let can_edit_session_note = app.can_edit_session_note();
    let (note_text, note_style) = if let Some(note) = app.current_task_note() {
        (
            format!("📝 Note: {note}"),
            Style::default().fg(app_color(app, Color::Cyan)),
        )
    } else if can_edit_session_note {
        (
            format!(
                "📝 Note: none yet ({} Edit note)",
                app.shortcut_hint(ShortcutAction::TimerEditNote)
            ),
            Style::default().fg(app_color(app, Color::DarkGray)),
        )
    } else {
        (
            "📝 Note: unavailable (start or resume focus to edit)".to_string(),
            Style::default().fg(app_color(app, Color::DarkGray)),
        )
    };
    let (stats_text, stats_style) = timer_stats_line(app);
    let goal_text = readable_goal_streak_text(&format_timer_goal_streak_line(app));
    let (waka_text, waka_color) = wakatime_status_line(app);
    let (schedule_next_text, schedule_status_text) = app.recurring_schedule_display_texts();

    let mut lines = vec![
        Line::styled(task_text, task_style),
        Line::styled(note_text, note_style),
        Line::styled(
            status_text,
            Style::default().fg(app_color(app, Color::Gray)),
        ),
        Line::styled(
            profile_line,
            Style::default().fg(app_color(app, Color::DarkGray)),
        ),
        Line::styled(
            schedule_next_text,
            Style::default().fg(app_color(app, Color::DarkGray)),
        ),
        Line::styled(
            schedule_status_text,
            Style::default().fg(app_color(app, Color::DarkGray)),
        ),
        Line::styled(format!("📈 {stats_text}"), stats_style),
    ];
    if let Some((goals, streak)) = goal_text.split_once(" | Streak: ") {
        lines.push(Line::styled(
            format!("🔥 {goals}"),
            Style::default().fg(app_color(app, Color::DarkGray)),
        ));
        lines.push(Line::styled(
            format!("🔥 Streak: {streak}"),
            Style::default().fg(app_color(app, Color::DarkGray)),
        ));
    } else {
        lines.push(Line::styled(
            format!("🔥 {goal_text}"),
            Style::default().fg(app_color(app, Color::DarkGray)),
        ));
    }
    lines.push(Line::styled(waka_text, Style::default().fg(waka_color)));
    for status_line in timer_session_status_lines(
        strict_status_text,
        break_glass_status_text,
        temporary_allowlist_status_text,
        app,
        inner.width,
    ) {
        lines.push(Line::styled(
            status_line,
            Style::default().fg(app_color(app, Color::DarkGray)),
        ));
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

pub(super) fn timer_status_text(app: &App) -> (String, String, String, String) {
    let status_text = match app.timer.status {
        TimerStatus::Running => "📍 Status: ▶ Running".to_string(),
        TimerStatus::Paused => "📍 Status: ⏸ Paused".to_string(),
        TimerStatus::Idle => "📍 Status: ⏹ Idle".to_string(),
    };
    let strict_text = if app.strict_reset_confirmation_pending() {
        format!(
            "🔒 Strict: confirm reset {}",
            app.shortcut_hint(ShortcutAction::TimerStopReset)
        )
    } else if app.strict_mode_enforced_for_focus() {
        "🔒 Strict: active (skip/quit locked)".to_string()
    } else if app.strict_mode {
        "🔒 Strict: armed".to_string()
    } else {
        "🔓 Strict: off".to_string()
    };
    let break_glass_text = if app.break_glass_confirmation_pending() {
        format!(
            "🚨 Break-glass: confirm {}",
            app.shortcut_hint(ShortcutAction::BreakGlassOverride)
        )
    } else if let Some(remaining_secs) = app.break_glass_override_remaining_secs() {
        format!(
            "🚨 Break-glass: active ({})",
            format_duration_label(remaining_secs)
        )
    } else {
        "🚨 Break-glass: off".to_string()
    };
    let temporary_allowlist_text =
        if let Some(next_expiry_secs) = app.next_temporary_allowlist_expiry_remaining_secs() {
            format!(
                "⏳ Temp allowlist: {} active (next {})",
                app.active_temporary_allowlist_count(),
                format_duration_label(next_expiry_secs)
            )
        } else {
            "⏳ Temp allowlist: off".to_string()
        };
    (
        status_text,
        strict_text,
        break_glass_text,
        temporary_allowlist_text,
    )
}

fn timer_session_status_lines(
    strict_status_text: String,
    break_glass_status_text: String,
    temporary_allowlist_status_text: String,
    app: &App,
    width: u16,
) -> Vec<String> {
    let narrow = is_narrow_timer_status(width);
    let mut lines = vec![strict_status_text, break_glass_status_text];
    lines.push(timer_temporary_allowlist_summary_line(
        app,
        narrow,
        temporary_allowlist_status_text,
    ));

    let active_entries = app.active_temporary_allowlist_entries();
    if active_entries.is_empty() {
        return lines;
    }

    let max_visible_entries = max_visible_temporary_allowlist_entries(width);

    for entry in active_entries.iter().take(max_visible_entries) {
        lines.push(timer_temporary_allowlist_entry_line(
            &entry.site,
            entry.remaining_secs,
            narrow,
        ));
    }

    if let Some(more_entries_line) = timer_temporary_allowlist_more_entries_line(
        active_entries.len(),
        max_visible_entries,
        narrow,
    ) {
        lines.push(more_entries_line);
    }

    lines
}

fn is_narrow_timer_status(width: u16) -> bool {
    width < 56
}

fn max_visible_temporary_allowlist_entries(width: u16) -> usize {
    if width < 56 {
        1
    } else if width < 76 {
        2
    } else {
        3
    }
}

fn timer_temporary_allowlist_summary_line(
    app: &App,
    narrow: bool,
    temporary_allowlist_status_text: String,
) -> String {
    if !narrow {
        return temporary_allowlist_status_text;
    }

    match app.next_temporary_allowlist_expiry_remaining_secs() {
        Some(next_expiry_secs) => format!(
            "⏳ Temp: {} active (next {})",
            app.active_temporary_allowlist_count(),
            format_duration_label(next_expiry_secs)
        ),
        None => "⏳ Temp: off".to_string(),
    }
}

fn timer_temporary_allowlist_entry_line(site: &str, remaining_secs: u64, narrow: bool) -> String {
    if narrow {
        format!("  • {} ({})", site, format_duration_label(remaining_secs))
    } else {
        format!(
            "  • {} (expires in {})",
            site,
            format_duration_label(remaining_secs)
        )
    }
}

fn timer_temporary_allowlist_more_entries_line(
    total_entries: usize,
    max_visible_entries: usize,
    narrow: bool,
) -> Option<String> {
    let hidden_entries = total_entries.saturating_sub(max_visible_entries);
    if hidden_entries == 0 {
        return None;
    }
    if narrow {
        return Some(format!("  • +{hidden_entries} more"));
    }
    if hidden_entries == 1 {
        Some("  • +1 more active exception".to_string())
    } else {
        Some(format!("  • +{hidden_entries} more active exceptions"))
    }
}

#[cfg(test)]
pub(super) fn timer_session_status_lines_for_width(app: &App, width: u16) -> Vec<String> {
    let (_, strict_status_text, break_glass_status_text, temporary_allowlist_status_text) =
        timer_status_text(app);
    timer_session_status_lines(
        strict_status_text,
        break_glass_status_text,
        temporary_allowlist_status_text,
        app,
        width,
    )
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
            format!(
                "📝 Note: {draft}   {} Save   {} Cancel",
                app.navigation_hint(NavigationAction::Confirm),
                app.navigation_hint(NavigationAction::Cancel)
            ),
            Style::default().fg(app_color(app, Color::Cyan)),
        );
    }

    if let Some(message) = app.phase_notification.as_ref() {
        (
            format!("🔔 {message}"),
            Style::default().fg(app_color(app, Color::Yellow)),
        )
    } else {
        (
            "🔔 Waiting for next completed phase".to_string(),
            Style::default().fg(app_color(app, Color::DarkGray)),
        )
    }
}

#[cfg(test)]
pub(super) fn note_phase_notice_text(app: &App) -> String {
    phase_notice_line(app).0
}

fn timer_stats_line(app: &App) -> (String, Style) {
    if let Some(err) = app.stats_error.as_ref() {
        (
            format!("⚠ Stats persistence warning: {err}"),
            Style::default().fg(app_color(app, Color::Yellow)),
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
            Style::default().fg(app_color(app, Color::DarkGray)),
        )
    }
}

pub(super) fn wakatime_status_line(app: &App) -> (String, Color) {
    let runtime_state = app.wakatime_runtime_state();
    let (status_text, status_color) = match &runtime_state {
        WakatimeRuntimeState::NotConfigured => (
            "⏱  WakaTime: not configured".to_string(),
            app_color(app, Color::DarkGray),
        ),
        WakatimeRuntimeState::Idle => (
            "⏱  WakaTime: idle".to_string(),
            app_color(app, Color::DarkGray),
        ),
        WakatimeRuntimeState::Tracking => (
            "⏱  WakaTime: tracking".to_string(),
            app_color(app, Color::Green),
        ),
        WakatimeRuntimeState::Sending => (
            "⏱  WakaTime: sending heartbeat...".to_string(),
            app_color(app, Color::Cyan),
        ),
        WakatimeRuntimeState::Queued { pending } => (
            format!("⏱  WakaTime: queued offline ({pending} pending)"),
            app_color(app, Color::Yellow),
        ),
        WakatimeRuntimeState::Replaying { pending } => (
            format!("⏱  WakaTime: resending queued heartbeats ({pending} pending)"),
            app_color(app, Color::Cyan),
        ),
        WakatimeRuntimeState::Retrying {
            attempt,
            max_attempts,
            next_backoff_secs,
            error,
        } => (
            format!(
                "⏱  WakaTime: retrying ({attempt}/{max_attempts}) in {next_backoff_secs}s ({error})"
            ),
            app_color(app, Color::Yellow),
        ),
        WakatimeRuntimeState::Error(error) => (
            format!("⏱  WakaTime: error ({error})"),
            app_color(app, Color::Red),
        ),
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
    match app.wakatime_last_successful_heartbeat_epoch_secs() {
        Some(epoch_secs) => format!(
            "last success {}",
            format_wakatime_heartbeat_timestamp(epoch_secs)
        ),
        None => "last success not yet sent".to_string(),
    }
}

pub(super) fn format_wakatime_heartbeat_timestamp(epoch_secs: u64) -> String {
    i64::try_from(epoch_secs)
        .ok()
        .and_then(|secs| Local.timestamp_opt(secs, 0).single())
        .map(|datetime| datetime.format("%H:%M:%S").to_string())
        .unwrap_or_else(|| epoch_secs.to_string())
}

pub(super) fn format_duration_label(duration_secs: u64) -> String {
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
        app,
        area,
        vec![
            Line::from(timer_primary_hint(app)),
            Line::from(timer_secondary_hint(app)),
            Line::from(timer_tertiary_hint(app)),
        ],
    );
}

pub(super) fn timer_primary_hint(app: &App) -> String {
    let timer_toggle = app.shortcut_hint(ShortcutAction::TimerTogglePause);
    let timer_stop = app.shortcut_hint(ShortcutAction::TimerStopReset);
    let timer_next = app.shortcut_hint(ShortcutAction::TimerNextPhase);
    let timer_note = app.shortcut_hint(ShortcutAction::TimerEditNote);
    let timer_unblock = app.shortcut_hint(ShortcutAction::BreakGlassOverride);

    if app.timer_note_input_active() {
        format!(
            "Note: Type text  {} Save  {} Cancel",
            app.navigation_hint(NavigationAction::Confirm),
            app.navigation_hint(NavigationAction::Cancel)
        )
    } else if app.break_glass_confirmation_pending() {
        format!(
            "Timer: {timer_toggle} Run/Pause  {timer_stop} Stop/Reset  {timer_next} Next  {timer_note} Note  {timer_unblock} Confirm unblock"
        )
    } else if app.strict_reset_confirmation_pending() {
        format!(
            "Timer: {timer_toggle} Run/Pause  {timer_stop} Confirm reset  {timer_next} Next (Locked)  {timer_note} Note  {timer_unblock} Unblock"
        )
    } else if app.strict_mode_enforced_for_focus() {
        format!(
            "Timer: {timer_toggle} Run/Pause  {timer_stop} Stop/Reset (Confirm)  {timer_next} Next (Locked)  {timer_note} Note  {timer_unblock} Unblock"
        )
    } else if app.can_edit_session_note() {
        format!(
            "Timer: {timer_toggle} Run/Pause  {timer_stop} Stop/Reset  {timer_next} Next  {timer_note} Note  {timer_unblock} Unblock"
        )
    } else {
        format!(
            "Timer: {timer_toggle} Run/Pause  {timer_stop} Stop/Reset  {timer_next} Next  {timer_note} Note (Focus only)  {timer_unblock} Unblock"
        )
    }
}

pub(super) fn timer_secondary_hint(app: &App) -> String {
    let planner = app.shortcut_hint(ShortcutAction::OpenSessionPlanner);
    let history = app.shortcut_hint(ShortcutAction::OpenStatsHistory);
    let sites = app.shortcut_hint(ShortcutAction::OpenSiteManager);
    let profiles = app.shortcut_hint(ShortcutAction::OpenProfileManager);
    let setup = app.shortcut_hint(ShortcutAction::OpenSetupDiagnostics);

    if app.timer_note_input_active() {
        "Views: shortcuts paused while editing note".to_string()
    } else if app.strict_mode_enforced_for_focus() {
        format!(
            "Views: {planner} Planner  {history} History  {sites} Sites  {profiles} Profiles (Locked)  {setup} Setup"
        )
    } else {
        format!(
            "Views: {planner} Planner  {history} History  {sites} Sites  {profiles} Profiles  {setup} Setup"
        )
    }
}

pub(super) fn timer_tertiary_hint(app: &App) -> String {
    let quit = app.shortcut_label(ShortcutAction::Quit);
    if app.timer_note_input_active() {
        format!(
            "Note edit: {} Cancel",
            app.navigation_hint(NavigationAction::Cancel)
        )
    } else if app.strict_mode_enforced_for_focus() {
        format!("Navigate: [{quit}/Esc] Quit (Locked during active focus)")
    } else {
        format!("Navigate: [{quit}/Esc] Quit")
    }
}

fn phase_color(app: &App, phase: TimerPhase) -> Color {
    match phase {
        TimerPhase::Focus => app_color(app, Color::Red),
        TimerPhase::ShortBreak => app_color(app, Color::Green),
        TimerPhase::LongBreak => app_color(app, Color::Cyan),
    }
}
