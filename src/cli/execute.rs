#[cfg(test)]
use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

use crate::app::App;
use crate::error::UserMessage;

use crate::cli::{
    AppConfig, BreakGlassCommandOutput, CliCommand, CommandKind, DailyGoalConfig,
    DailyGoalSnapshot, FocusStats, GoalCarryCommandOutput, GoalCommandOutput, MonthlyGoalConfig,
    OutputMode, ProfileId, ProfileOutput, ProfileView, RecurringScheduleConfig,
    ScheduleCommandOutput, SessionMetadataCommandOutput, StrictCommandOutput, TaskCommandOutput,
    TaskGoalCommandOutput, TaskGoalOutput, TemporaryAllowlistStatusOutput,
    TemporarySiteAddCommandOutput, ThemeCommandOutput, ThemePreset, TimerCommandOutput,
    TimerStateOutput, WeeklyGoalConfig, available_theme_preset_views,
    build_schedule_inspection_output, build_task_goal_output, print_break_glass_command_output,
    print_goal_carry_command_output, print_goal_command_output, print_json, print_profile_output,
    print_schedule_command_output, print_session_metadata_command_output,
    print_strict_command_output, print_task_goal_command_output,
    print_temporary_site_add_command_output, print_theme_command_output, print_timer_state_output,
    profile_id, profile_view, theme_preset_view, timer_phase_id, timer_status_id,
};

mod blocklists;
mod dashboard;
mod data;
mod diagnostics;
mod status;

#[cfg(test)]
pub(super) use blocklists::{
    apply_blocklist_profile_command, apply_site_add_command, apply_site_delete_command,
    apply_site_edit_command,
};
use blocklists::{execute_blocklist_profile_command, execute_blocklist_sites_command};
#[cfg(test)]
pub(super) use dashboard::apply_history_dashboard_command;
use dashboard::execute_history_dashboard_command;
use data::{
    execute_backup_command, execute_export_command, execute_restore_command, stats_load_options,
    stats_save_options,
};
use diagnostics::execute_diagnostics_command;
use status::execute_status_command;
#[cfg(test)]
use status::{WATCH_INTERRUPTED, next_watch_deadline, wait_for_next_watch_tick};

type CliExecuteResult<T> = Result<T, UserMessage>;

pub(super) fn execute_cli_command(cli_command: CliCommand) -> CliExecuteResult<()> {
    if let Some(surface_id) = command_usage_surface_id(&cli_command.kind)
        && !command_usage_records_via_app(&cli_command.kind)
    {
        if let Err(error) = record_command_usage_direct(surface_id) {
            eprintln!("Warning: failed to record command usage signal for `{surface_id}`: {error}");
        }
    }
    match cli_command.kind {
        CommandKind::Start => execute_start_command(cli_command.output),
        CommandKind::Pause => execute_pause_command(cli_command.output),
        CommandKind::Resume => execute_resume_command(cli_command.output),
        CommandKind::Stop => execute_stop_command(cli_command.output),
        CommandKind::Next => execute_next_command(cli_command.output),
        CommandKind::Task { label } => execute_task_command(label, cli_command.output),
        CommandKind::TaskGoal { label, goal } => {
            execute_task_goal_command(label, goal, cli_command.output)
        }
        CommandKind::TaskNote { value } => execute_task_note_command(value, cli_command.output),
        CommandKind::Profile { profile } => execute_profile_command(profile, cli_command.output),
        CommandKind::Theme { preset } => execute_theme_command(preset, cli_command.output),
        CommandKind::Goal { goal } => execute_goal_command(goal, cli_command.output),
        CommandKind::GoalWeekly { goal } => execute_weekly_goal_command(goal, cli_command.output),
        CommandKind::GoalMonthly { goal } => execute_monthly_goal_command(goal, cli_command.output),
        CommandKind::GoalCarry { enabled } => {
            execute_goal_carry_command(enabled, cli_command.output)
        }
        CommandKind::GoalCarryWeekly { enabled } => {
            execute_weekly_goal_carry_command(enabled, cli_command.output)
        }
        CommandKind::GoalCarryMonthly { enabled } => {
            execute_monthly_goal_carry_command(enabled, cli_command.output)
        }
        CommandKind::Strict { enabled } => execute_strict_command(enabled, cli_command.output),
        CommandKind::Schedule { schedule } => {
            execute_schedule_command(schedule, cli_command.output)
        }
        CommandKind::BreakGlassTrigger => execute_break_glass_trigger_command(cli_command.output),
        CommandKind::BreakGlassCancel => execute_break_glass_cancel_command(cli_command.output),
        CommandKind::Diagnostics => {
            execute_diagnostics_command(cli_command.output).map_err(UserMessage::from)
        }
        CommandKind::Status {
            watch_interval_secs,
        } => execute_status_command(cli_command.output, watch_interval_secs)
            .map_err(UserMessage::from),
        CommandKind::Backup { dir } => {
            execute_backup_command(dir, cli_command.output).map_err(UserMessage::from)
        }
        CommandKind::Restore { dir } => {
            execute_restore_command(dir, cli_command.output).map_err(UserMessage::from)
        }
        CommandKind::Export { dir } => {
            execute_export_command(dir, cli_command.output).map_err(UserMessage::from)
        }
        CommandKind::BlocklistProfile { command } => {
            execute_blocklist_profile_command(command, cli_command.output)
                .map_err(UserMessage::from)
        }
        CommandKind::BlocklistSites { target, command } => {
            execute_blocklist_sites_command(target, command, cli_command.output)
                .map_err(UserMessage::from)
        }
        CommandKind::AllowlistSiteAddTemporary { input } => {
            execute_allowlist_site_add_temporary_command(input, cli_command.output)
        }
        CommandKind::HistoryDashboard { command } => {
            execute_history_dashboard_command(command, cli_command.output)
                .map_err(UserMessage::from)
        }
    }
}
/// Maps command variants to their help-surface identifiers.
fn command_usage_surface_id(command: &CommandKind) -> Option<&'static str> {
    match command {
        CommandKind::Start => Some("start"),
        CommandKind::Pause => Some("pause"),
        CommandKind::Resume => Some("resume"),
        CommandKind::Stop => Some("stop"),
        CommandKind::Next => Some("next"),
        CommandKind::Task { .. } => Some("task"),
        CommandKind::TaskGoal { .. } => Some("task-goal"),
        CommandKind::TaskNote { .. } => Some("task-note"),
        CommandKind::Profile { .. } => Some("profile"),
        CommandKind::Theme { .. } => Some("theme"),
        CommandKind::Goal { .. } => Some("goal"),
        CommandKind::GoalWeekly { .. } => Some("goal-weekly"),
        CommandKind::GoalMonthly { .. } => Some("goal-monthly"),
        CommandKind::GoalCarry { .. } => Some("goal-carry"),
        CommandKind::GoalCarryWeekly { .. } => Some("goal-carry-weekly"),
        CommandKind::GoalCarryMonthly { .. } => Some("goal-carry-monthly"),
        CommandKind::Strict { .. } => Some("strict"),
        CommandKind::Schedule { .. } => Some("schedule"),
        CommandKind::BreakGlassTrigger => Some("break-glass-trigger"),
        CommandKind::BreakGlassCancel => Some("break-glass-cancel"),
        CommandKind::Diagnostics => Some("diagnostics"),
        CommandKind::Status { .. } => Some("status"),
        CommandKind::Backup { .. } => Some("backup"),
        CommandKind::Restore { .. } => Some("restore"),
        CommandKind::Export { .. } => Some("export"),
        CommandKind::BlocklistProfile { .. } => Some("blocklist-profile"),
        CommandKind::BlocklistSites { .. } => Some("blocklist-sites"),
        CommandKind::AllowlistSiteAddTemporary { .. } => Some("allowlist-site-add-temporary"),
        CommandKind::HistoryDashboard { .. } => Some("history-dashboard"),
    }
}

fn command_usage_records_via_app(command: &CommandKind) -> bool {
    matches!(
        command,
        CommandKind::Start
            | CommandKind::Pause
            | CommandKind::Resume
            | CommandKind::Stop
            | CommandKind::Next
            | CommandKind::Task { .. }
            | CommandKind::TaskNote { .. }
            | CommandKind::AllowlistSiteAddTemporary { .. }
            | CommandKind::BreakGlassTrigger
            | CommandKind::BreakGlassCancel
            | CommandKind::Diagnostics
    )
}

fn record_command_usage_direct(surface_id: &str) -> Result<(), String> {
    let config = AppConfig::load().normalized();
    let mut stats = FocusStats::load_with_options(stats_load_options(&config))
        .map_err(|error| format!("Failed to load stats: {error}"))?;
    if !stats.record_command_usage(surface_id) {
        return Ok(());
    }
    stats
        .save_with_options(stats_save_options(&config))
        .map_err(|error| format!("Failed to save usage signals: {error}"))
}

fn execute_start_command(output: OutputMode) -> CliExecuteResult<()> {
    let mut app = App::new();
    app.record_command_usage_for_cli("start");
    app.start_focus_for_cli()?;
    emit_timer_command_output("start", &app, output)?;
    Ok(())
}

fn execute_pause_command(output: OutputMode) -> CliExecuteResult<()> {
    let mut app = App::new();
    app.record_command_usage_for_cli("pause");
    app.pause_for_cli()?;
    emit_timer_command_output("pause", &app, output)?;
    Ok(())
}

fn execute_resume_command(output: OutputMode) -> CliExecuteResult<()> {
    let mut app = App::new();
    app.record_command_usage_for_cli("resume");
    app.resume_for_cli()?;
    emit_timer_command_output("resume", &app, output)?;
    Ok(())
}

fn execute_stop_command(output: OutputMode) -> CliExecuteResult<()> {
    let mut app = App::new();
    app.record_command_usage_for_cli("stop");
    app.stop_for_cli()?;
    emit_timer_command_output("stop", &app, output)?;
    Ok(())
}

fn execute_next_command(output: OutputMode) -> CliExecuteResult<()> {
    let mut app = App::new();
    app.record_command_usage_for_cli("next");
    app.next_phase_for_cli()?;
    emit_timer_command_output("next", &app, output)?;
    Ok(())
}

fn execute_task_command(label: String, output: OutputMode) -> CliExecuteResult<()> {
    let mut app = App::new();
    app.record_command_usage_for_cli("task");
    let created = app.select_task_label_for_cli(&label)?;
    let selected_task_label = app
        .selected_task_label_for_cli()
        .ok_or_else(|| "Task selection failed: no selected task label after update.".to_string())?;
    let payload = TaskCommandOutput {
        action: "task",
        created,
        selected_task_label,
        timer: build_timer_state_output(&app),
    };

    match output {
        OutputMode::Text => {
            if payload.created {
                println!(
                    "Task label added and selected: {}",
                    payload.selected_task_label
                );
            } else {
                println!("Task label selected: {}", payload.selected_task_label);
            }
            print_timer_state_output(&payload.timer);
        }
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_task_goal_command(
    label: Option<String>,
    goal: Option<DailyGoalConfig>,
    output: OutputMode,
) -> CliExecuteResult<()> {
    let config = AppConfig::load().normalized();
    let mut stats = FocusStats::load_with_options(stats_load_options(&config))?;
    let selected_task_label = stats.task_planner_state().1;
    let requested_label = label.or(selected_task_label).ok_or_else(|| {
        "No task label selected. Use `--task LABEL` first or pass `--task-goal LABEL`.".to_string()
    })?;

    let mut updated = false;
    let task_label = if let Some(goal) = goal {
        let target = DailyGoalSnapshot {
            minutes: goal.minutes,
            pomodoros: goal.pomodoros,
        };
        let canonical = stats.set_task_goal_target(&requested_label, target)?;
        stats
            .save_with_options(stats_save_options(&config))
            .map_err(|error| format!("Failed to save task goals: {error}"))?;
        updated = true;
        canonical
    } else {
        requested_label
    };

    let TaskGoalOutput {
        task_label,
        configured,
        minutes_target,
        pomodoros_target,
        focused_minutes,
        pomodoros_completed,
        met,
    } = build_task_goal_output(&stats, &task_label);
    let payload = TaskGoalCommandOutput {
        updated,
        task_label,
        configured,
        minutes_target,
        pomodoros_target,
        focused_minutes,
        pomodoros_completed,
        met,
    };

    match output {
        OutputMode::Text => print_task_goal_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_task_note_command(value: Option<String>, output: OutputMode) -> CliExecuteResult<()> {
    let mut app = App::new();
    app.record_command_usage_for_cli("task-note");
    let mut updated = false;
    if let Some(value) = value {
        app.set_task_note_for_cli(&value)?;
        updated = true;
    }
    emit_session_metadata_command_output("task-note", updated, &app, output)?;
    Ok(())
}

fn execute_allowlist_site_add_temporary_command(
    input: String,
    output: OutputMode,
) -> CliExecuteResult<()> {
    let mut app = App::new();
    app.record_command_usage_for_cli("allowlist-site-add-temporary");
    let (added, refreshed) = app.add_temporary_allowlist_for_cli(&input)?;
    let payload = TemporarySiteAddCommandOutput {
        action: "allowlist-site-add-temporary",
        updated: added > 0 || refreshed > 0,
        profile: app.selected_blocklist_profile_name_for_cli(),
        added,
        refreshed,
        active: app
            .active_temporary_allowlist_entries()
            .into_iter()
            .map(|entry| TemporaryAllowlistStatusOutput {
                site: entry.site,
                remaining_secs: entry.remaining_secs,
                expires_at_epoch_secs: entry.expires_at_epoch_secs,
            })
            .collect(),
    };
    match output {
        OutputMode::Text => print_temporary_site_add_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_profile_command(profile: Option<ProfileId>, output: OutputMode) -> CliExecuteResult<()> {
    let mut config = AppConfig::load().normalized();
    let mut updated = false;
    if let Some(profile) = profile {
        config.selected_profile = profile;
        config
            .save()
            .map_err(|error| format!("Failed to save selected profile: {error}"))?;
        updated = true;
    }

    let custom = config.effective_custom_profile();
    let selected = profile_view(config.selected_profile, &custom);
    let available = [ProfileId::Classic, ProfileId::DeepWork, ProfileId::Custom]
        .into_iter()
        .map(|candidate| profile_view(candidate, &custom))
        .collect();
    let selected_theme_preset = theme_preset_view(config.selected_theme_preset);
    let available_theme_presets = available_theme_preset_views();
    let payload = ProfileOutput {
        updated,
        selected,
        available,
        selected_theme_preset,
        available_theme_presets,
    };

    match output {
        OutputMode::Text => print_profile_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_theme_command(preset: Option<ThemePreset>, output: OutputMode) -> CliExecuteResult<()> {
    let mut config = AppConfig::load().normalized();
    let mut updated = false;
    if let Some(preset) = preset {
        config.selected_theme_preset = preset;
        config
            .save()
            .map_err(|error| format!("Failed to save theme preset: {error}"))?;
        updated = true;
    }

    let payload = ThemeCommandOutput {
        updated,
        selected_theme_preset: theme_preset_view(config.selected_theme_preset),
        available_theme_presets: available_theme_preset_views(),
    };

    match output {
        OutputMode::Text => print_theme_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_goal_command(goal: Option<DailyGoalConfig>, output: OutputMode) -> CliExecuteResult<()> {
    let mut config = AppConfig::load().normalized();
    let mut updated = false;
    if let Some(goal) = goal {
        config.daily_goal = goal;
        config
            .save()
            .map_err(|error| format!("Failed to save daily goal: {error}"))?;
        updated = true;
    }

    let payload = GoalCommandOutput {
        updated,
        configured: config.daily_goal.minutes > 0 || config.daily_goal.pomodoros > 0,
        minutes_target: config.daily_goal.minutes,
        pomodoros_target: config.daily_goal.pomodoros,
    };

    match output {
        OutputMode::Text => print_goal_command_output("Daily", &payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_weekly_goal_command(
    goal: Option<WeeklyGoalConfig>,
    output: OutputMode,
) -> CliExecuteResult<()> {
    let mut config = AppConfig::load().normalized();
    let mut updated = false;
    if let Some(goal) = goal {
        config.weekly_goal = goal;
        config
            .save()
            .map_err(|error| format!("Failed to save weekly goal: {error}"))?;
        updated = true;
    }

    let payload = GoalCommandOutput {
        updated,
        configured: config.weekly_goal.minutes > 0 || config.weekly_goal.pomodoros > 0,
        minutes_target: config.weekly_goal.minutes,
        pomodoros_target: config.weekly_goal.pomodoros,
    };

    match output {
        OutputMode::Text => print_goal_command_output("Weekly", &payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_monthly_goal_command(
    goal: Option<MonthlyGoalConfig>,
    output: OutputMode,
) -> CliExecuteResult<()> {
    let mut config = AppConfig::load().normalized();
    let mut updated = false;
    if let Some(goal) = goal {
        config.monthly_goal = goal;
        config
            .save()
            .map_err(|error| format!("Failed to save monthly goal: {error}"))?;
        updated = true;
    }

    let payload = GoalCommandOutput {
        updated,
        configured: config.monthly_goal.minutes > 0 || config.monthly_goal.pomodoros > 0,
        minutes_target: config.monthly_goal.minutes,
        pomodoros_target: config.monthly_goal.pomodoros,
    };

    match output {
        OutputMode::Text => print_goal_command_output("Monthly", &payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_goal_carry_command(enabled: Option<bool>, output: OutputMode) -> CliExecuteResult<()> {
    let mut config = AppConfig::load().normalized();
    let mut updated = false;
    if let Some(enabled) = enabled {
        config.goal_carry_over.daily = enabled;
        config
            .save()
            .map_err(|error| format!("Failed to save daily goal carry-over: {error}"))?;
        updated = true;
    }

    let payload = GoalCarryCommandOutput {
        updated,
        carry_over: config.goal_carry_over.daily,
    };
    match output {
        OutputMode::Text => print_goal_carry_command_output("Daily", &payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_weekly_goal_carry_command(
    enabled: Option<bool>,
    output: OutputMode,
) -> CliExecuteResult<()> {
    let mut config = AppConfig::load().normalized();
    let mut updated = false;
    if let Some(enabled) = enabled {
        config.goal_carry_over.weekly = enabled;
        config
            .save()
            .map_err(|error| format!("Failed to save weekly goal carry-over: {error}"))?;
        updated = true;
    }

    let payload = GoalCarryCommandOutput {
        updated,
        carry_over: config.goal_carry_over.weekly,
    };
    match output {
        OutputMode::Text => print_goal_carry_command_output("Weekly", &payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_monthly_goal_carry_command(
    enabled: Option<bool>,
    output: OutputMode,
) -> CliExecuteResult<()> {
    let mut config = AppConfig::load().normalized();
    let mut updated = false;
    if let Some(enabled) = enabled {
        config.goal_carry_over.monthly = enabled;
        config
            .save()
            .map_err(|error| format!("Failed to save monthly goal carry-over: {error}"))?;
        updated = true;
    }

    let payload = GoalCarryCommandOutput {
        updated,
        carry_over: config.goal_carry_over.monthly,
    };
    match output {
        OutputMode::Text => print_goal_carry_command_output("Monthly", &payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_strict_command(enabled: Option<bool>, output: OutputMode) -> CliExecuteResult<()> {
    let mut config = AppConfig::load().normalized();
    let mut updated = false;
    if let Some(enabled) = enabled {
        let mut automation = config.profile_automation_for(config.selected_profile);
        automation.strict_mode = enabled;
        config.set_profile_automation_for(config.selected_profile, automation);
        config
            .save()
            .map_err(|error| format!("Failed to save strict mode: {error}"))?;
        updated = true;
    }

    let payload = StrictCommandOutput {
        updated,
        strict_mode: config
            .profile_automation_for(config.selected_profile)
            .strict_mode,
    };

    match output {
        OutputMode::Text => print_strict_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_schedule_command(
    schedule: Option<RecurringScheduleConfig>,
    output: OutputMode,
) -> CliExecuteResult<()> {
    let mut config = AppConfig::load().normalized();
    let mut updated = false;
    if let Some(schedule) = schedule {
        let mut automation = config.profile_automation_for(config.selected_profile);
        automation.recurring_schedule = schedule.normalized();
        config.set_profile_automation_for(config.selected_profile, automation);
        config
            .save()
            .map_err(|error| format!("Failed to save recurring schedule: {error}"))?;
        updated = true;
    }
    let selected_automation = config.profile_automation_for(config.selected_profile);
    let inspection = build_schedule_inspection_output(&selected_automation.recurring_schedule);
    let payload = ScheduleCommandOutput {
        updated,
        schedule: selected_automation.recurring_schedule,
        inspection,
    };

    match output {
        OutputMode::Text => print_schedule_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_break_glass_trigger_command(output: OutputMode) -> CliExecuteResult<()> {
    let mut app = App::new();
    app.record_command_usage_for_cli("break-glass-trigger");
    app.trigger_break_glass_for_cli()?;
    emit_break_glass_command_output("break-glass-trigger", &app, output)?;
    Ok(())
}

fn execute_break_glass_cancel_command(output: OutputMode) -> CliExecuteResult<()> {
    let mut app = App::new();
    app.record_command_usage_for_cli("break-glass-cancel");
    app.cancel_break_glass_for_cli()?;
    emit_break_glass_command_output("break-glass-cancel", &app, output)?;
    Ok(())
}

fn emit_timer_command_output(
    action: &'static str,
    app: &App,
    output: OutputMode,
) -> Result<(), String> {
    let payload = TimerCommandOutput {
        action,
        timer: build_timer_state_output(app),
    };

    match output {
        OutputMode::Text => {
            println!("Timer action applied: {}", payload.action);
            print_timer_state_output(&payload.timer);
        }
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn emit_session_metadata_command_output(
    action: &'static str,
    updated: bool,
    app: &App,
    output: OutputMode,
) -> Result<(), String> {
    let payload = SessionMetadataCommandOutput {
        action,
        updated,
        task_note: app.task_note_for_cli(),
        timer: build_timer_state_output(app),
    };
    match output {
        OutputMode::Text => print_session_metadata_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn emit_break_glass_command_output(
    action: &'static str,
    app: &App,
    output: OutputMode,
) -> Result<(), String> {
    let payload = BreakGlassCommandOutput {
        action,
        pending_confirmation: app.break_glass_confirmation_pending(),
        active: app.break_glass_override_active(),
        remaining_secs: app.break_glass_override_remaining_secs(),
        timer: build_timer_state_output(app),
    };
    match output {
        OutputMode::Text => print_break_glass_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn build_timer_state_output(app: &App) -> TimerStateOutput {
    let (phase, status, remaining_secs, pomodoros_completed) = app.timer_state_for_cli();
    let profile = app.selected_profile_id();
    let (focus_secs, short_break_secs, long_break_secs, long_break_interval) =
        app.profile_values(profile);
    let selected_task_label = app.selected_task_label_for_cli();
    let task_note = app.task_note_for_cli();

    TimerStateOutput {
        phase: timer_phase_id(phase),
        status: timer_status_id(status),
        remaining_secs,
        pomodoros_completed,
        selected_profile: ProfileView {
            id: profile_id(profile),
            label: profile.label(),
            focus_secs,
            short_break_secs,
            long_break_secs,
            long_break_interval,
        },
        selected_task_label,
        task_note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    static WATCH_TEST_GUARD: OnceLock<Mutex<()>> = OnceLock::new();

    fn guard_watch_state() -> std::sync::MutexGuard<'static, ()> {
        WATCH_TEST_GUARD
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("watch test guard should lock")
    }

    #[test]
    fn command_usage_surface_id_maps_expected_surfaces() {
        assert_eq!(command_usage_surface_id(&CommandKind::Start), Some("start"));
        assert_eq!(
            command_usage_surface_id(&CommandKind::Task {
                label: "docs".to_string()
            }),
            Some("task")
        );
        assert_eq!(
            command_usage_surface_id(&CommandKind::Status {
                watch_interval_secs: None,
            }),
            Some("status")
        );
        assert_eq!(
            command_usage_surface_id(&CommandKind::Export { dir: None }),
            Some("export")
        );
    }

    #[test]
    fn command_usage_records_via_app_matches_expected_commands() {
        assert!(command_usage_records_via_app(&CommandKind::Start));
        assert!(command_usage_records_via_app(&CommandKind::Diagnostics));
        assert!(!command_usage_records_via_app(&CommandKind::Backup {
            dir: None
        }));
        assert!(!command_usage_records_via_app(&CommandKind::Status {
            watch_interval_secs: None,
        }));
    }

    #[test]
    fn next_watch_deadline_advances_when_on_schedule() {
        let _guard = guard_watch_state();
        let base = Instant::now();
        let interval = Duration::from_secs(2);
        let now = base + Duration::from_millis(500);

        let deadline = next_watch_deadline(base, interval, now);

        assert_eq!(deadline, base + interval);
    }

    #[test]
    fn next_watch_deadline_skips_missed_intervals() {
        let _guard = guard_watch_state();
        let base = Instant::now();
        let interval = Duration::from_secs(1);
        let now = base + Duration::from_millis(3300);

        let deadline = next_watch_deadline(base, interval, now);

        assert_eq!(deadline, base + Duration::from_secs(4));
    }

    #[test]
    fn wait_for_next_watch_tick_returns_interrupt_state_immediately() {
        let _guard = guard_watch_state();
        WATCH_INTERRUPTED.store(true, Ordering::SeqCst);

        let interrupted = wait_for_next_watch_tick(Instant::now() + Duration::from_secs(1));

        WATCH_INTERRUPTED.store(false, Ordering::SeqCst);
        assert!(interrupted);
    }

    #[test]
    fn wait_for_next_watch_tick_returns_false_after_deadline() {
        let _guard = guard_watch_state();
        WATCH_INTERRUPTED.store(false, Ordering::SeqCst);

        let interrupted = wait_for_next_watch_tick(Instant::now());

        WATCH_INTERRUPTED.store(false, Ordering::SeqCst);
        assert!(!interrupted);
    }

    #[test]
    fn wait_for_next_watch_tick_observes_interrupt_request() {
        let _guard = guard_watch_state();
        WATCH_INTERRUPTED.store(false, Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_secs(2);

        let wait_thread = std::thread::spawn(move || wait_for_next_watch_tick(deadline));
        std::thread::sleep(Duration::from_millis(30));
        WATCH_INTERRUPTED.store(true, Ordering::SeqCst);

        let interrupted = wait_thread.join().expect("watch wait thread should join");
        WATCH_INTERRUPTED.store(false, Ordering::SeqCst);
        assert!(interrupted);
    }
}
