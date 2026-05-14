use std::{env, fs, path::Path, thread, time::Duration};

use crate::app::App;

use crate::cli::{
    AppConfig, BackupOutput, BlocklistProfileCommandKind, BlocklistProfileCommandOutput,
    BlocklistProfileConfig, BlocklistProfileSummaryOutput, BlocklistSiteCommandKind,
    BreakGlassCommandOutput, CliCommand, CommandKind, DailyGoalConfig, DailyGoalSnapshot,
    EditSiteResult, ExportOutput, FocusStats, GoalCarryCommandOutput, GoalCommandOutput,
    InvalidSiteEntryOutput, InvalidSiteInput, MigrationCommandOutput, MigrationStepOutput,
    MigrationStepStatus, MonthlyGoalConfig, OutputMode, PathBuf, ProfileId, ProfileOutput,
    ProfileView, RecurringScheduleConfig, RestoreOutput, ScheduleCommandOutput,
    ScheduleDelayCommandOutput, SessionMetadataCommandOutput, SiteAddCommandOutput, SiteBlocker,
    SiteDeleteCommandOutput, SiteEditCommandOutput, SiteEditValue, SiteListCommandOutput,
    SiteListTarget, StatusOutput, StrictCommandOutput, TaskCommandOutput, TaskGoalCommandOutput,
    TaskGoalOutput, ThemeCommandOutput, ThemePreset, TimerCommandOutput, TimerStateOutput,
    WeeklyGoalConfig, available_break_template_views, available_theme_preset_views,
    build_blocking_preview_command_output, build_diagnostics_command_output,
    build_schedule_inspection_output, build_status_output, build_task_goal_output,
    display_input_value, effective_blocked_sites_for_profile, flush_stdout, print_backup_output,
    print_blocking_preview_command_output, print_blocklist_profile_command_output,
    print_break_glass_command_output, print_diagnostics_command_output, print_export_output,
    print_goal_carry_command_output, print_goal_command_output, print_json, print_json_compact,
    print_migration_output, print_profile_output, print_restore_output,
    print_schedule_command_output, print_schedule_delay_command_output,
    print_session_metadata_command_output, print_site_add_command_output,
    print_site_delete_command_output, print_site_edit_command_output,
    print_site_list_command_output, print_status_output, print_strict_command_output,
    print_task_goal_command_output, print_theme_command_output, print_timer_state_output,
    profile_id, profile_view, selected_break_template_view, theme_preset_view, timer_phase_id,
    timer_status_id,
};

const CONFIG_FILE_NAME: &str = "config.toml";
const STATS_FILE_NAME: &str = "stats.toml";

#[derive(Debug, Clone)]
struct MigrationPlan {
    config_path: PathBuf,
    canonical_stats_path: PathBuf,
    warnings: Vec<String>,
}

impl MigrationPlan {
    fn has_changes(&self) -> bool {
        false
    }
}

const MIGRATION_OPERATION_VERIFY_CANONICAL_STATS: &str = "verify_canonical_stats_path";

pub(super) fn execute_cli_command(cli_command: CliCommand) -> Result<(), String> {
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
        CommandKind::FocusIntention { value } => {
            execute_focus_intention_command(value, cli_command.output)
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
        CommandKind::ScheduleDelay => execute_schedule_delay_command(cli_command.output),
        CommandKind::BreakGlassTrigger => execute_break_glass_trigger_command(cli_command.output),
        CommandKind::BreakGlassCancel => execute_break_glass_cancel_command(cli_command.output),
        CommandKind::Diagnostics => execute_diagnostics_command(cli_command.output),
        CommandKind::BlockingPreview => execute_blocking_preview_command(cli_command.output),
        CommandKind::Status {
            watch_interval_secs,
        } => execute_status_command(cli_command.output, watch_interval_secs),
        CommandKind::Backup { dir } => execute_backup_command(dir, cli_command.output),
        CommandKind::Restore { dir } => execute_restore_command(dir, cli_command.output),
        CommandKind::Migrate { dry_run } => execute_migrate_command(dry_run, cli_command.output),
        CommandKind::Export { dir } => execute_export_command(dir, cli_command.output),
        CommandKind::BlocklistProfile { command } => {
            execute_blocklist_profile_command(command, cli_command.output)
        }
        CommandKind::BlocklistSites { target, command } => {
            execute_blocklist_sites_command(target, command, cli_command.output)
        }
    }
}

fn execute_start_command(output: OutputMode) -> Result<(), String> {
    let mut app = App::new();
    app.start_focus_for_cli()?;
    emit_timer_command_output("start", &app, output)
}

fn execute_pause_command(output: OutputMode) -> Result<(), String> {
    let mut app = App::new();
    app.pause_for_cli()?;
    emit_timer_command_output("pause", &app, output)
}

fn execute_resume_command(output: OutputMode) -> Result<(), String> {
    let mut app = App::new();
    app.resume_for_cli()?;
    emit_timer_command_output("resume", &app, output)
}

fn execute_stop_command(output: OutputMode) -> Result<(), String> {
    let mut app = App::new();
    app.stop_for_cli()?;
    emit_timer_command_output("stop", &app, output)
}

fn execute_next_command(output: OutputMode) -> Result<(), String> {
    let mut app = App::new();
    app.next_phase_for_cli()?;
    emit_timer_command_output("next", &app, output)
}

fn execute_task_command(label: String, output: OutputMode) -> Result<(), String> {
    let mut app = App::new();
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
) -> Result<(), String> {
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

fn execute_focus_intention_command(
    value: Option<String>,
    output: OutputMode,
) -> Result<(), String> {
    let mut app = App::new();
    let mut updated = false;
    if let Some(value) = value {
        app.set_focus_intention_for_cli(&value)?;
        updated = true;
    }
    emit_session_metadata_command_output("focus-intention", updated, &app, output)
}

fn execute_task_note_command(value: Option<String>, output: OutputMode) -> Result<(), String> {
    let mut app = App::new();
    let mut updated = false;
    if let Some(value) = value {
        app.set_task_note_for_cli(&value)?;
        updated = true;
    }
    emit_session_metadata_command_output("task-note", updated, &app, output)
}

fn execute_blocklist_profile_command(
    command: BlocklistProfileCommandKind,
    output: OutputMode,
) -> Result<(), String> {
    let mut config = AppConfig::load().normalized();
    let payload = apply_blocklist_profile_command(&mut config, command)?;
    if payload.updated {
        config
            .save()
            .map_err(|error| format!("Failed to save blocklist profile settings: {error}"))?;
    }

    match output {
        OutputMode::Text => print_blocklist_profile_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_blocklist_sites_command(
    target: SiteListTarget,
    command: BlocklistSiteCommandKind,
    output: OutputMode,
) -> Result<(), String> {
    let mut config = AppConfig::load().normalized();
    match command {
        BlocklistSiteCommandKind::List => {
            let payload = build_site_list_command_output(&config, target, "site-list");
            match output {
                OutputMode::Text => print_site_list_command_output(&payload),
                OutputMode::Json => print_json(&payload)?,
            }
        }
        BlocklistSiteCommandKind::Add { input } => {
            let payload = apply_site_add_command(&mut config, target, &input)?;
            if payload.updated {
                config
                    .save()
                    .map_err(|error| format!("Failed to save site changes: {error}"))?;
            }
            match output {
                OutputMode::Text => print_site_add_command_output(&payload),
                OutputMode::Json => print_json(&payload)?,
            }
        }
        BlocklistSiteCommandKind::Edit { value } => {
            let payload = apply_site_edit_command(&mut config, target, &value)?;
            if payload.updated {
                config
                    .save()
                    .map_err(|error| format!("Failed to save site changes: {error}"))?;
            }
            match output {
                OutputMode::Text => print_site_edit_command_output(&payload),
                OutputMode::Json => print_json(&payload)?,
            }
        }
        BlocklistSiteCommandKind::Delete { site } => {
            let payload = apply_site_delete_command(&mut config, target, &site)?;
            if payload.updated {
                config
                    .save()
                    .map_err(|error| format!("Failed to save site changes: {error}"))?;
            }
            match output {
                OutputMode::Text => print_site_delete_command_output(&payload),
                OutputMode::Json => print_json(&payload)?,
            }
        }
    }
    Ok(())
}

pub(super) fn apply_blocklist_profile_command(
    config: &mut AppConfig,
    command: BlocklistProfileCommandKind,
) -> Result<BlocklistProfileCommandOutput, String> {
    ensure_blocklist_profiles(config);

    let (action, updated) = match command {
        BlocklistProfileCommandKind::Select { profile } => {
            handle_select_blocklist_profile(config, profile)?
        }
        BlocklistProfileCommandKind::Create { name } => {
            handle_create_blocklist_profile(config, name)?
        }
        BlocklistProfileCommandKind::Rename { name } => {
            handle_rename_blocklist_profile(config, name)?
        }
        BlocklistProfileCommandKind::Delete => handle_delete_blocklist_profile(config)?,
    };

    if updated {
        sync_selected_blocklist_profile(config);
    }
    Ok(build_blocklist_profile_command_output(
        config, action, updated,
    ))
}

fn handle_select_blocklist_profile(
    config: &mut AppConfig,
    profile: Option<String>,
) -> Result<(&'static str, bool), String> {
    let mut updated = false;
    if let Some(profile) = profile {
        let index = blocklist_profile_index_by_name(&config.blocklist_profiles, &profile)
            .ok_or_else(|| format!("Unknown blocklist profile `{profile}`."))?;
        let selected = config.blocklist_profiles[index].name.clone();
        if !config
            .selected_blocklist_profile
            .eq_ignore_ascii_case(&selected)
        {
            config.selected_blocklist_profile = selected;
            updated = true;
        }
    }
    Ok(("blocklist-profile", updated))
}

fn handle_create_blocklist_profile(
    config: &mut AppConfig,
    name: String,
) -> Result<(&'static str, bool), String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Profile name cannot be empty.".to_string());
    }
    if blocklist_profile_index_by_name(&config.blocklist_profiles, &name).is_some() {
        return Err(format!("Profile `{name}` already exists."));
    }
    config.blocklist_profiles.push(BlocklistProfileConfig {
        name: name.clone(),
        sites: Vec::new(),
        allowlist_sites: Vec::new(),
    });
    config.selected_blocklist_profile = name;
    Ok(("blocklist-profile-create", true))
}

fn handle_rename_blocklist_profile(
    config: &mut AppConfig,
    name: String,
) -> Result<(&'static str, bool), String> {
    let index = selected_blocklist_profile_index(config);
    let current_name = config.blocklist_profiles[index].name.clone();
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Profile name cannot be empty.".to_string());
    }
    if current_name == name {
        return Ok(("blocklist-profile-rename", false));
    }

    let duplicate =
        config
            .blocklist_profiles
            .iter()
            .enumerate()
            .any(|(candidate_index, profile)| {
                candidate_index != index && profile.name.eq_ignore_ascii_case(&name)
            });
    if duplicate {
        return Err(format!("Profile `{name}` already exists."));
    }

    config.blocklist_profiles[index].name = name.clone();
    config.selected_blocklist_profile = name;
    Ok(("blocklist-profile-rename", true))
}

fn handle_delete_blocklist_profile(config: &mut AppConfig) -> Result<(&'static str, bool), String> {
    if config.blocklist_profiles.len() <= 1 {
        return Err("At least one blocklist profile is required.".to_string());
    }
    let index = selected_blocklist_profile_index(config);
    config.blocklist_profiles.remove(index);
    let next_index = index.min(config.blocklist_profiles.len().saturating_sub(1));
    config.selected_blocklist_profile = config.blocklist_profiles[next_index].name.clone();
    Ok(("blocklist-profile-delete", true))
}

pub(super) fn apply_site_add_command(
    config: &mut AppConfig,
    target: SiteListTarget,
    input: &str,
) -> Result<SiteAddCommandOutput, String> {
    ensure_blocklist_profiles(config);
    let index = selected_blocklist_profile_index(config);
    let mut working = SiteBlocker::new();
    let existing_sites = active_profile_sites(config, index, target).to_vec();
    for site in existing_sites {
        working.add_site(site);
    }
    let result = working.add_sites_from_input(input);
    let updated = !result.added.is_empty();
    *active_profile_sites_mut(config, index, target) = working.sites.clone();
    if updated {
        sync_selected_blocklist_profile(config);
    }

    let active_profile = &config.blocklist_profiles[index];
    Ok(SiteAddCommandOutput {
        action: "site-add",
        updated,
        profile: active_profile.name.clone(),
        target,
        added: result.added,
        duplicates: result.duplicates,
        invalid: invalid_site_entries_output(&result.invalid),
        sites: active_profile_sites(config, index, target).to_vec(),
        effective_blocked_sites_count: effective_blocked_sites_for_profile(active_profile).len(),
    })
}

pub(super) fn apply_site_edit_command(
    config: &mut AppConfig,
    target: SiteListTarget,
    value: &SiteEditValue,
) -> Result<SiteEditCommandOutput, String> {
    ensure_blocklist_profiles(config);
    let index = selected_blocklist_profile_index(config);
    let previous = value.previous.trim();

    let mut working = SiteBlocker::new();
    for site in active_profile_sites(config, index, target) {
        working.add_site(site.clone());
    }
    let edit_index = working
        .sites
        .iter()
        .position(|site| site.eq_ignore_ascii_case(previous))
        .ok_or_else(|| {
            format!(
                "Site `{}` was not found in {}.",
                value.previous,
                target.id()
            )
        })?;
    let result = working.edit_site_from_input(edit_index, &value.next);
    match result {
        EditSiteResult::Updated { old, new } => {
            *active_profile_sites_mut(config, index, target) = working.sites.clone();
            sync_selected_blocklist_profile(config);
            let active_profile = &config.blocklist_profiles[index];
            Ok(SiteEditCommandOutput {
                action: "site-edit",
                updated: true,
                profile: active_profile.name.clone(),
                target,
                previous: old,
                current: new,
                sites: active_profile_sites(config, index, target).to_vec(),
                effective_blocked_sites_count: effective_blocked_sites_for_profile(active_profile)
                    .len(),
            })
        }
        EditSiteResult::Unchanged { hostname } => {
            let active_profile = &config.blocklist_profiles[index];
            Ok(SiteEditCommandOutput {
                action: "site-edit",
                updated: false,
                profile: active_profile.name.clone(),
                target,
                previous: hostname.clone(),
                current: hostname,
                sites: active_profile_sites(config, index, target).to_vec(),
                effective_blocked_sites_count: effective_blocked_sites_for_profile(active_profile)
                    .len(),
            })
        }
        EditSiteResult::Duplicate { hostname } => {
            Err(format!("`{hostname}` already exists in {}.", target.id()))
        }
        EditSiteResult::Invalid(invalid) => Err(format!(
            "Invalid hostname `{}` ({})",
            display_input_value(&invalid.input),
            invalid.reason.message()
        )),
        EditSiteResult::MissingSelection => Err("No site selected to edit.".to_string()),
    }
}

pub(super) fn apply_site_delete_command(
    config: &mut AppConfig,
    target: SiteListTarget,
    site: &str,
) -> Result<SiteDeleteCommandOutput, String> {
    ensure_blocklist_profiles(config);
    let index = selected_blocklist_profile_index(config);
    let site = site.trim();

    let mut working = SiteBlocker::new();
    for current in active_profile_sites(config, index, target) {
        working.add_site(current.clone());
    }
    let delete_index = working
        .sites
        .iter()
        .position(|value| value.eq_ignore_ascii_case(site))
        .ok_or_else(|| format!("Site `{site}` was not found in {}.", target.id()))?;
    let removed = working
        .remove_site(delete_index)
        .ok_or_else(|| format!("Site `{site}` was not found in {}.", target.id()))?;
    *active_profile_sites_mut(config, index, target) = working.sites.clone();
    sync_selected_blocklist_profile(config);

    let active_profile = &config.blocklist_profiles[index];
    Ok(SiteDeleteCommandOutput {
        action: "site-delete",
        updated: true,
        profile: active_profile.name.clone(),
        target,
        removed,
        sites: active_profile_sites(config, index, target).to_vec(),
        effective_blocked_sites_count: effective_blocked_sites_for_profile(active_profile).len(),
    })
}

fn ensure_blocklist_profiles(config: &mut AppConfig) {
    if config.blocklist_profiles.is_empty() {
        config
            .blocklist_profiles
            .push(BlocklistProfileConfig::default());
        config.selected_blocklist_profile = config.blocklist_profiles[0].name.clone();
    }
}

fn blocklist_profile_index_by_name(
    profiles: &[BlocklistProfileConfig],
    name: &str,
) -> Option<usize> {
    profiles
        .iter()
        .position(|profile| profile.name.eq_ignore_ascii_case(name.trim()))
}

fn selected_blocklist_profile_index(config: &AppConfig) -> usize {
    blocklist_profile_index_by_name(
        &config.blocklist_profiles,
        &config.selected_blocklist_profile,
    )
    .unwrap_or(0)
}

fn active_profile_sites(
    config: &AppConfig,
    profile_index: usize,
    target: SiteListTarget,
) -> &[String] {
    match target {
        SiteListTarget::Blocklist => &config.blocklist_profiles[profile_index].sites,
        SiteListTarget::Allowlist => &config.blocklist_profiles[profile_index].allowlist_sites,
    }
}

fn active_profile_sites_mut(
    config: &mut AppConfig,
    profile_index: usize,
    target: SiteListTarget,
) -> &mut Vec<String> {
    match target {
        SiteListTarget::Blocklist => &mut config.blocklist_profiles[profile_index].sites,
        SiteListTarget::Allowlist => &mut config.blocklist_profiles[profile_index].allowlist_sites,
    }
}

fn sync_selected_blocklist_profile(config: &mut AppConfig) {
    ensure_blocklist_profiles(config);
    let index = selected_blocklist_profile_index(config);
    config.selected_blocklist_profile = config.blocklist_profiles[index].name.clone();
}

fn build_blocklist_profile_command_output(
    config: &AppConfig,
    action: &'static str,
    updated: bool,
) -> BlocklistProfileCommandOutput {
    let selected_name = config
        .blocklist_profiles
        .get(selected_blocklist_profile_index(config))
        .map(|profile| profile.name.clone())
        .unwrap_or_else(|| config.selected_blocklist_profile.clone());
    let profiles = config
        .blocklist_profiles
        .iter()
        .map(|profile| BlocklistProfileSummaryOutput {
            name: profile.name.clone(),
            active: profile.name.eq_ignore_ascii_case(&selected_name),
            blocklist_sites_count: profile.sites.len(),
            allowlist_sites_count: profile.allowlist_sites.len(),
            effective_blocked_sites_count: effective_blocked_sites_for_profile(profile).len(),
        })
        .collect();

    BlocklistProfileCommandOutput {
        action,
        updated,
        selected_blocklist_profile: selected_name,
        profiles,
    }
}

fn build_site_list_command_output(
    config: &AppConfig,
    target: SiteListTarget,
    action: &'static str,
) -> SiteListCommandOutput {
    if config.blocklist_profiles.is_empty() {
        let fallback = if config.selected_blocklist_profile.trim().is_empty() {
            "Default".to_string()
        } else {
            config.selected_blocklist_profile.clone()
        };
        return SiteListCommandOutput {
            action,
            profile: fallback,
            target,
            sites: Vec::new(),
            effective_blocked_sites_count: 0,
        };
    }
    let index = selected_blocklist_profile_index(config);
    let profile = &config.blocklist_profiles[index];
    SiteListCommandOutput {
        action,
        profile: profile.name.clone(),
        target,
        sites: active_profile_sites(config, index, target).to_vec(),
        effective_blocked_sites_count: effective_blocked_sites_for_profile(profile).len(),
    }
}

fn invalid_site_entries_output(values: &[InvalidSiteInput]) -> Vec<InvalidSiteEntryOutput> {
    values
        .iter()
        .map(|invalid| InvalidSiteEntryOutput {
            input: invalid.input.clone(),
            reason: invalid.reason.message().to_string(),
        })
        .collect()
}

fn execute_profile_command(profile: Option<ProfileId>, output: OutputMode) -> Result<(), String> {
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
    let selected_break_template = selected_break_template_view(&config);
    let available_break_templates = available_break_template_views(&config);
    let selected_theme_preset = theme_preset_view(config.selected_theme_preset);
    let available_theme_presets = available_theme_preset_views();
    let payload = ProfileOutput {
        updated,
        selected,
        available,
        selected_break_template,
        available_break_templates,
        selected_theme_preset,
        available_theme_presets,
    };

    match output {
        OutputMode::Text => print_profile_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_theme_command(preset: Option<ThemePreset>, output: OutputMode) -> Result<(), String> {
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

fn execute_goal_command(goal: Option<DailyGoalConfig>, output: OutputMode) -> Result<(), String> {
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
) -> Result<(), String> {
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
) -> Result<(), String> {
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

fn execute_goal_carry_command(enabled: Option<bool>, output: OutputMode) -> Result<(), String> {
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
) -> Result<(), String> {
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
) -> Result<(), String> {
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

fn execute_strict_command(enabled: Option<bool>, output: OutputMode) -> Result<(), String> {
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
) -> Result<(), String> {
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

fn execute_schedule_delay_command(output: OutputMode) -> Result<(), String> {
    let mut app = App::new();
    let delayed_until = app.schedule_delay_for_cli()?;
    emit_schedule_delay_command_output("schedule-delay", delayed_until, &app, output)
}

fn execute_break_glass_trigger_command(output: OutputMode) -> Result<(), String> {
    let mut app = App::new();
    app.trigger_break_glass_for_cli()?;
    emit_break_glass_command_output("break-glass-trigger", &app, output)
}

fn execute_break_glass_cancel_command(output: OutputMode) -> Result<(), String> {
    let mut app = App::new();
    app.cancel_break_glass_for_cli()?;
    emit_break_glass_command_output("break-glass-cancel", &app, output)
}

fn execute_diagnostics_command(output: OutputMode) -> Result<(), String> {
    let app = App::new();
    let payload = build_diagnostics_command_output(&app.setup_diagnostics);

    match output {
        OutputMode::Text => print_diagnostics_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_blocking_preview_command(output: OutputMode) -> Result<(), String> {
    let app = App::new();
    let preview = app.blocking_preview_for_cli()?;
    let payload = build_blocking_preview_command_output(&preview);

    match output {
        OutputMode::Text => print_blocking_preview_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_status_command(
    output: OutputMode,
    watch_interval_secs: Option<u64>,
) -> Result<(), String> {
    if let Some(interval_secs) = watch_interval_secs {
        return execute_status_watch_command(output, interval_secs);
    }

    let payload = load_status_output()?;
    emit_status_output(&payload, output, false)
}

fn execute_status_watch_command(output: OutputMode, interval_secs: u64) -> Result<(), String> {
    if interval_secs == 0 {
        return Err("`--watch` interval must be greater than 0 seconds.".to_string());
    }

    loop {
        let payload = load_status_output()?;
        emit_status_output(&payload, output, true)?;
        flush_stdout()?;
        thread::sleep(Duration::from_secs(interval_secs));
    }
}

fn load_status_output() -> Result<StatusOutput, String> {
    let config = AppConfig::load().normalized();
    let stats = FocusStats::load_with_options(stats_load_options(&config))
        .map_err(|error| format!("Failed to load stats: {error}"))?;
    Ok(build_status_output(&config, &stats))
}

fn emit_status_output(
    payload: &StatusOutput,
    output: OutputMode,
    watch_mode: bool,
) -> Result<(), String> {
    match output {
        OutputMode::Text => {
            print_status_output(payload);
            if watch_mode {
                println!();
            }
        }
        OutputMode::Json => {
            if watch_mode {
                print_json_compact(payload)?;
            } else {
                print_json(payload)?;
            }
        }
    }
    Ok(())
}

fn execute_export_command(dir: Option<PathBuf>, output: OutputMode) -> Result<(), String> {
    let config = AppConfig::load().normalized();
    let stats = FocusStats::load_with_options(stats_load_options(&config))
        .map_err(|error| format!("Failed to load stats: {error}"))?;
    let target_dir = match dir {
        Some(path) => path,
        None => env::current_dir()
            .map_err(|error| format!("Failed to determine current directory: {error}"))?,
    };
    let exported = stats
        .export_to_dir(&target_dir)
        .map_err(|error| format!("Export failed: {error}"))?;

    let payload = ExportOutput {
        export_dir: target_dir,
        json_path: exported.json_path,
        csv_path: exported.csv_path,
    };
    match output {
        OutputMode::Text => print_export_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_backup_command(dir: Option<PathBuf>, output: OutputMode) -> Result<(), String> {
    let _config = AppConfig::load().normalized();
    let backup_dir = match dir {
        Some(path) => path,
        None => env::current_dir().map_err(|error| {
            format!("Backup failed: could not determine current directory: {error}")
        })?,
    };
    fs::create_dir_all(&backup_dir)
        .map_err(|error| format!("Backup failed: could not create backup directory: {error}"))?;

    let source_config = config_file_path()?;
    let source_stats = stats_persistence_path()?;
    ensure_backup_source_file(&source_config, CONFIG_FILE_NAME)?;
    ensure_backup_source_file(&source_stats, STATS_FILE_NAME)?;
    let config_backup_path = backup_dir.join(CONFIG_FILE_NAME);
    let stats_backup_path = backup_dir.join(STATS_FILE_NAME);

    copy_file_with_context(&source_config, &config_backup_path, "backup config.toml")?;
    copy_file_with_context(&source_stats, &stats_backup_path, "backup stats.toml")?;

    let payload = BackupOutput {
        backup_dir,
        config_backup_path,
        stats_backup_path,
    };
    match output {
        OutputMode::Text => print_backup_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_restore_command(dir: Option<PathBuf>, output: OutputMode) -> Result<(), String> {
    let _config = AppConfig::load().normalized();
    let restore_dir = match dir {
        Some(path) => path,
        None => env::current_dir().map_err(|error| {
            format!("Restore failed: could not determine current directory: {error}")
        })?,
    };
    let source_config = restore_dir.join(CONFIG_FILE_NAME);
    let source_stats = restore_dir.join(STATS_FILE_NAME);
    ensure_restore_source_file(&source_config, CONFIG_FILE_NAME)?;
    ensure_restore_source_file(&source_stats, STATS_FILE_NAME)?;

    let config_restored_path = config_file_path()?;
    let stats_restored_path = stats_persistence_path()?;
    let staged_config_path = temp_restore_path(&config_restored_path, "staged");
    let staged_stats_path = temp_restore_path(&stats_restored_path, "staged");
    copy_file_with_context(
        &source_config,
        &staged_config_path,
        "stage restore config.toml",
    )?;
    copy_file_with_context(
        &source_stats,
        &staged_stats_path,
        "stage restore stats.toml",
    )?;

    let original_config_snapshot = snapshot_existing_file(
        &config_restored_path,
        "snapshot existing config.toml for rollback",
    )?;
    let original_stats_snapshot = snapshot_existing_file(
        &stats_restored_path,
        "snapshot existing stats.toml for rollback",
    )?;

    replace_file_atomically(
        &staged_config_path,
        &config_restored_path,
        "restore config.toml",
    )?;
    if let Err(error) = replace_file_atomically(
        &staged_stats_path,
        &stats_restored_path,
        "restore stats.toml",
    ) {
        rollback_restored_file(
            original_config_snapshot.as_deref(),
            &config_restored_path,
            "roll back restored config.toml",
        );
        rollback_restored_file(
            original_stats_snapshot.as_deref(),
            &stats_restored_path,
            "roll back restored stats.toml",
        );
        let _ = remove_file_if_exists(&staged_stats_path);
        return Err(error);
    }
    if let Some(snapshot) = original_config_snapshot.as_deref() {
        let _ = remove_file_if_exists(snapshot);
    }
    if let Some(snapshot) = original_stats_snapshot.as_deref() {
        let _ = remove_file_if_exists(snapshot);
    }

    let payload = RestoreOutput {
        restore_dir,
        config_restored_path,
        stats_restored_path,
    };
    match output {
        OutputMode::Text => print_restore_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_migrate_command(dry_run: bool, output: OutputMode) -> Result<(), String> {
    let config = AppConfig::load().normalized();
    let plan = build_migration_plan(&config)?;
    let steps = migration_steps_for_plan(&plan);
    let payload = MigrationCommandOutput {
        action: "migrate",
        dry_run,
        changed: plan.has_changes(),
        config_path: plan.config_path.clone(),
        canonical_stats_path: plan.canonical_stats_path.clone(),
        steps,
        warnings: plan.warnings.clone(),
    };
    match output {
        OutputMode::Text => print_migration_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn build_migration_plan(_config: &AppConfig) -> Result<MigrationPlan, String> {
    let config_path = config_file_path()?;
    ensure_regular_file_if_exists(&config_path, "config path")?;
    let canonical_stats_path = stats_persistence_path()?;
    ensure_regular_file_if_exists(&canonical_stats_path, "canonical stats path")?;
    let warnings = vec![
        "Legacy stats-path migration has been retired. Runtime persistence is canonical-path only."
            .to_string(),
    ];
    Ok(MigrationPlan {
        config_path,
        canonical_stats_path,
        warnings,
    })
}

fn migration_steps_for_plan(plan: &MigrationPlan) -> Vec<MigrationStepOutput> {
    let detail = if plan.canonical_stats_path.exists() {
        format!(
            "Canonical stats path `{}` is active; no migration is required.",
            plan.canonical_stats_path.display()
        )
    } else {
        format!(
            "Canonical stats path `{}` is not present yet; it will be created on next save.",
            plan.canonical_stats_path.display()
        )
    };
    vec![MigrationStepOutput {
        operation: MIGRATION_OPERATION_VERIFY_CANONICAL_STATS,
        status: MigrationStepStatus::Skipped,
        detail,
    }]
}

fn ensure_regular_file_if_exists(path: &Path, context: &str) -> Result<(), String> {
    if path.exists() && !path.is_file() {
        return Err(format!(
            "Migration failed: {context} `{}` is not a regular file.",
            path.display()
        ));
    }
    Ok(())
}

fn ensure_restore_source_file(path: &Path, file_name: &str) -> Result<(), String> {
    if !path.exists() {
        return Err(format!(
            "Restore failed: missing `{file_name}` in `{}`.",
            path.parent()
                .map(|parent| parent.display().to_string())
                .unwrap_or_else(|| ".".to_string())
        ));
    }
    if !path.is_file() {
        return Err(format!(
            "Restore failed: `{}` is not a regular file.",
            path.display()
        ));
    }
    Ok(())
}

fn ensure_backup_source_file(path: &Path, file_name: &str) -> Result<(), String> {
    if !path.exists() {
        return Err(format!(
            "Backup failed: missing `{file_name}` in `{}`.",
            path.parent()
                .map(|parent| parent.display().to_string())
                .unwrap_or_else(|| ".".to_string())
        ));
    }
    if !path.is_file() {
        return Err(format!(
            "Backup failed: `{}` is not a regular file.",
            path.display()
        ));
    }
    Ok(())
}

fn snapshot_existing_file(path: &Path, context: &str) -> Result<Option<PathBuf>, String> {
    if !path.exists() {
        return Ok(None);
    }
    if !path.is_file() {
        return Err(format!(
            "Failed to {context}: `{}` is not a regular file.",
            path.display()
        ));
    }
    let snapshot = temp_restore_path(path, "original");
    fs::copy(path, &snapshot).map_err(|error| {
        format!(
            "Failed to {context}: `{}` -> `{}`: {error}",
            path.display(),
            snapshot.display()
        )
    })?;
    Ok(Some(snapshot))
}

fn temp_restore_path(path: &Path, marker: &str) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let target_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("focustime-restore");
    let pid = std::process::id();
    parent.join(format!(".{target_name}.{pid}.{marker}.tmp"))
}

fn replace_file_atomically(
    staged_path: &Path,
    destination: &Path,
    context: &str,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        match fs::rename(staged_path, destination) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                fs::remove_file(destination).map_err(|remove_error| {
                    format!(
                        "Failed to {context}: could not replace `{}`: {remove_error}",
                        destination.display()
                    )
                })?;
                fs::rename(staged_path, destination).map_err(|rename_error| {
                    format!(
                        "Failed to {context}: `{}` -> `{}`: {rename_error}",
                        staged_path.display(),
                        destination.display()
                    )
                })
            }
            Err(error) => {
                let _ = remove_file_if_exists(staged_path);
                Err(format!(
                    "Failed to {context}: `{}` -> `{}`: {error}",
                    staged_path.display(),
                    destination.display()
                ))
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        fs::rename(staged_path, destination).map_err(|error| {
            let _ = remove_file_if_exists(staged_path);
            format!(
                "Failed to {context}: `{}` -> `{}`: {error}",
                staged_path.display(),
                destination.display()
            )
        })
    }
}

fn remove_file_if_exists(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    fs::remove_file(path).map_err(|error| format!("Failed to remove `{}`: {error}", path.display()))
}

fn rollback_restored_file(snapshot: Option<&Path>, destination: &Path, rollback_context: &str) {
    if let Some(snapshot) = snapshot {
        let _ = replace_file_atomically(snapshot, destination, rollback_context);
    } else {
        let _ = remove_file_if_exists(destination);
    }
}

fn config_file_path() -> Result<PathBuf, String> {
    crate::config::app_data_path(CONFIG_FILE_NAME).ok_or_else(|| {
        format!(
            "could not determine application data path for `{CONFIG_FILE_NAME}` (environment is not configured)"
        )
    })
}

fn stats_persistence_path() -> Result<PathBuf, String> {
    crate::config::stats_data_path(STATS_FILE_NAME).ok_or_else(|| {
        format!(
            "could not determine application data path for `{STATS_FILE_NAME}` (environment is not configured)"
        )
    })
}

fn stats_load_options(_config: &AppConfig) -> crate::stats::StatsLoadOptions {
    crate::stats::StatsLoadOptions::default()
}

fn stats_save_options(_config: &AppConfig) -> crate::stats::StatsSaveOptions {
    crate::stats::StatsSaveOptions::default()
}

fn copy_file_with_context(source: &Path, destination: &Path, context: &str) -> Result<(), String> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Failed to {context}: could not create `{}`: {error}",
                parent.display()
            )
        })?;
    }
    fs::copy(source, destination).map_err(|error| {
        format!(
            "Failed to {context}: `{}` -> `{}`: {error}",
            source.display(),
            destination.display()
        )
    })?;
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
        focus_intention: app.focus_intention_for_cli(),
        task_note: app.task_note_for_cli(),
        timer: build_timer_state_output(app),
    };
    match output {
        OutputMode::Text => print_session_metadata_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn emit_schedule_delay_command_output(
    action: &'static str,
    delayed_until: String,
    app: &App,
    output: OutputMode,
) -> Result<(), String> {
    let payload = ScheduleDelayCommandOutput {
        action,
        delayed_until,
        timer: build_timer_state_output(app),
    };
    match output {
        OutputMode::Text => print_schedule_delay_command_output(&payload),
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
    let focus_intention = app.focus_intention_for_cli();
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
        focus_intention,
        task_note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn create_temp_dir(test_name: &str) -> PathBuf {
        let unique = format!(
            "focustime-{test_name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("current time should be after unix epoch")
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        fs::create_dir_all(&path).expect("temp dir should be created");
        path
    }

    #[test]
    fn migration_plan_reports_canonical_only_behavior() {
        let temp_dir = create_temp_dir("migrate-canonical-only");
        let config_path = temp_dir.join("config.toml");
        let canonical_stats_path = temp_dir.join("state").join("stats.toml");
        fs::write(&config_path, "schema_version = 1\n").expect("config file should be writable");
        fs::create_dir_all(
            canonical_stats_path
                .parent()
                .expect("canonical path should have parent"),
        )
        .expect("canonical stats parent should be writable");
        fs::write(&canonical_stats_path, "daily = {}\n")
            .expect("canonical stats file should be writable");

        let plan = MigrationPlan {
            config_path,
            canonical_stats_path: canonical_stats_path.clone(),
            warnings: vec![
                "Legacy stats-path migration has been retired. Runtime persistence is canonical-path only."
                    .to_string(),
            ],
        };
        let steps = migration_steps_for_plan(&plan);

        assert!(!plan.has_changes());
        assert!(
            plan.warnings
                .iter()
                .any(|warning| warning.contains("Legacy stats-path migration has been retired"))
        );
        let verify_step = steps
            .iter()
            .find(|step| step.operation == MIGRATION_OPERATION_VERIFY_CANONICAL_STATS)
            .expect("verify step should exist");
        assert_eq!(verify_step.status, MigrationStepStatus::Skipped);
        assert!(verify_step.detail.contains("Canonical stats path"));

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }
}
