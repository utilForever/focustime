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
struct StatsPersistencePaths {
    canonical: PathBuf,
    legacy: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct MigrationPlan {
    config_path: PathBuf,
    canonical_stats_path: PathBuf,
    legacy_stats_path: Option<PathBuf>,
    copy_legacy_stats_to_canonical: bool,
    disable_stats_legacy_path_read_fallback: bool,
    disable_stats_legacy_path_dual_write: bool,
    stats_copy_skip_reason: String,
    warnings: Vec<String>,
}

impl MigrationPlan {
    fn has_changes(&self) -> bool {
        self.copy_legacy_stats_to_canonical
            || self.disable_stats_legacy_path_read_fallback
            || self.disable_stats_legacy_path_dual_write
    }

    fn requires_config_update(&self) -> bool {
        self.disable_stats_legacy_path_read_fallback || self.disable_stats_legacy_path_dual_write
    }
}

const MIGRATION_OPERATION_COPY_LEGACY_STATS: &str = "copy_legacy_stats_to_canonical";
const MIGRATION_OPERATION_DISABLE_READ_FALLBACK: &str = "disable_stats_legacy_path_read_fallback";
const MIGRATION_OPERATION_DISABLE_DUAL_WRITE: &str = "disable_stats_legacy_path_dual_write";

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
        sync_legacy_blocked_sites(config);
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
        sync_legacy_blocked_sites(config);
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
            sync_legacy_blocked_sites(config);
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
    sync_legacy_blocked_sites(config);

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

fn sync_legacy_blocked_sites(config: &mut AppConfig) {
    ensure_blocklist_profiles(config);
    let index = selected_blocklist_profile_index(config);
    config.selected_blocklist_profile = config.blocklist_profiles[index].name.clone();
    if config.feature_flags.legacy_blocked_sites_mirror {
        config.blocked_sites =
            effective_blocked_sites_for_profile(&config.blocklist_profiles[index]);
    }
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
        config.align_legacy_automation_with_selected_profile();
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
    let config = AppConfig::load().normalized();
    let backup_dir = match dir {
        Some(path) => path,
        None => env::current_dir().map_err(|error| {
            format!("Backup failed: could not determine current directory: {error}")
        })?,
    };
    fs::create_dir_all(&backup_dir)
        .map_err(|error| format!("Backup failed: could not create backup directory: {error}"))?;

    let source_config = config_file_path()?;
    let stats_paths = stats_persistence_paths()?;
    let source_stats = resolve_stats_backup_source_path(
        &stats_paths,
        config.feature_flags.stats_legacy_path_read_fallback,
    );
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
    let config = AppConfig::load().normalized();
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
    let stats_paths = stats_persistence_paths()?;
    let stats_restored_path = stats_paths.canonical.clone();
    let stats_legacy_restored_path = if config.feature_flags.stats_legacy_path_dual_write {
        stats_paths.legacy.clone()
    } else {
        None
    };
    let staged_config_path = temp_restore_path(&config_restored_path, "staged");
    let staged_stats_path = temp_restore_path(&stats_restored_path, "staged");
    let staged_legacy_stats_path = stats_legacy_restored_path
        .as_ref()
        .map(|path| temp_restore_path(path, "staged"));
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
    if let Some(staged_legacy) = staged_legacy_stats_path.as_deref() {
        copy_file_with_context(
            &source_stats,
            staged_legacy,
            "stage restore legacy stats.toml mirror",
        )?;
    }

    let original_config_snapshot = snapshot_existing_file(
        &config_restored_path,
        "snapshot existing config.toml for rollback",
    )?;
    let original_stats_snapshot = snapshot_existing_file(
        &stats_restored_path,
        "snapshot existing stats.toml for rollback",
    )?;
    let original_legacy_stats_snapshot =
        if let Some(legacy_stats_path) = stats_legacy_restored_path.as_deref() {
            snapshot_existing_file(
                legacy_stats_path,
                "snapshot existing legacy stats.toml for rollback",
            )?
        } else {
            None
        };

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
        if let Some(legacy_stats_path) = stats_legacy_restored_path.as_deref() {
            rollback_restored_file(
                original_legacy_stats_snapshot.as_deref(),
                legacy_stats_path,
                "roll back restored legacy stats.toml",
            );
        }
        let _ = remove_file_if_exists(&staged_stats_path);
        if let Some(staged_legacy) = staged_legacy_stats_path.as_deref() {
            let _ = remove_file_if_exists(staged_legacy);
        }
        return Err(error);
    }
    restore_legacy_stats_mirror_if_enabled(
        stats_legacy_restored_path.as_deref(),
        staged_legacy_stats_path.as_deref(),
        original_legacy_stats_snapshot.as_deref(),
        original_config_snapshot.as_deref(),
        &config_restored_path,
        original_stats_snapshot.as_deref(),
        &stats_restored_path,
    )?;
    if let Some(snapshot) = original_config_snapshot.as_deref() {
        let _ = remove_file_if_exists(snapshot);
    }
    if let Some(snapshot) = original_stats_snapshot.as_deref() {
        let _ = remove_file_if_exists(snapshot);
    }
    if let Some(snapshot) = original_legacy_stats_snapshot.as_deref() {
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
    let mut steps = migration_steps_for_plan(&plan);
    if !dry_run && plan.has_changes() {
        apply_migration_plan(&config, &plan, &mut steps)?;
    }
    let payload = MigrationCommandOutput {
        action: "migrate",
        dry_run,
        changed: plan.has_changes(),
        config_path: plan.config_path.clone(),
        canonical_stats_path: plan.canonical_stats_path.clone(),
        legacy_stats_path: plan.legacy_stats_path.clone(),
        steps,
        warnings: plan.warnings.clone(),
    };
    match output {
        OutputMode::Text => print_migration_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn build_migration_plan(config: &AppConfig) -> Result<MigrationPlan, String> {
    let config_path = config_file_path()?;
    ensure_regular_file_if_exists(&config_path, "config path")?;
    let stats_paths = stats_persistence_paths()?;
    ensure_regular_file_if_exists(&stats_paths.canonical, "canonical stats path")?;
    if let Some(legacy) = stats_paths.legacy.as_deref() {
        ensure_regular_file_if_exists(legacy, "legacy stats path")?;
    }

    let mut copy_legacy_stats_to_canonical = false;
    let mut stats_copy_skip_reason = "No legacy stats file was found to migrate.".to_string();
    let mut warnings = Vec::new();

    let canonical_exists = stats_paths.canonical.exists();
    let legacy_exists = stats_paths
        .legacy
        .as_ref()
        .is_some_and(|path| path.exists());
    if let Some(legacy) = stats_paths.legacy.as_ref() {
        if legacy_exists && !canonical_exists {
            copy_legacy_stats_to_canonical = true;
            stats_copy_skip_reason.clear();
        } else if legacy_exists && canonical_exists {
            let canonical_bytes = fs::read(&stats_paths.canonical).map_err(|error| {
                format!(
                    "Migration planning failed: could not read canonical stats file `{}`: {error}",
                    stats_paths.canonical.display()
                )
            })?;
            let legacy_bytes = fs::read(legacy).map_err(|error| {
                format!(
                    "Migration planning failed: could not read legacy stats file `{}`: {error}",
                    legacy.display()
                )
            })?;
            if canonical_bytes != legacy_bytes {
                return Err(format!(
                    "Migration failed: canonical stats `{}` and legacy stats `{}` differ. Run `focustime --backup`, reconcile the two files, and rerun `focustime --migrate`.",
                    stats_paths.canonical.display(),
                    legacy.display()
                ));
            }
            stats_copy_skip_reason =
                "Canonical and legacy stats files already match; copy is not required.".to_string();
        } else if !legacy_exists && !canonical_exists {
            stats_copy_skip_reason = format!(
                "Neither canonical (`{}`) nor legacy (`{}`) stats files exist.",
                stats_paths.canonical.display(),
                legacy.display()
            );
        } else {
            stats_copy_skip_reason =
                "Canonical stats file already exists; legacy copy is not required.".to_string();
        }
    } else {
        stats_copy_skip_reason =
            "No distinct legacy stats path exists for this environment.".to_string();
    }

    if !copy_legacy_stats_to_canonical {
        warnings.push(stats_copy_skip_reason.clone());
    }

    Ok(MigrationPlan {
        config_path,
        canonical_stats_path: stats_paths.canonical,
        legacy_stats_path: stats_paths.legacy,
        copy_legacy_stats_to_canonical,
        disable_stats_legacy_path_read_fallback: config
            .feature_flags
            .stats_legacy_path_read_fallback,
        disable_stats_legacy_path_dual_write: config.feature_flags.stats_legacy_path_dual_write,
        stats_copy_skip_reason,
        warnings,
    })
}

fn migration_steps_for_plan(plan: &MigrationPlan) -> Vec<MigrationStepOutput> {
    let mut steps = Vec::new();
    let status_for_apply = MigrationStepStatus::Planned;

    if plan.copy_legacy_stats_to_canonical {
        let legacy = plan
            .legacy_stats_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<unavailable>".to_string());
        steps.push(MigrationStepOutput {
            operation: MIGRATION_OPERATION_COPY_LEGACY_STATS,
            status: status_for_apply,
            detail: format!(
                "Copy legacy stats `{legacy}` to canonical path `{}`.",
                plan.canonical_stats_path.display()
            ),
        });
    } else {
        steps.push(MigrationStepOutput {
            operation: MIGRATION_OPERATION_COPY_LEGACY_STATS,
            status: MigrationStepStatus::Skipped,
            detail: plan.stats_copy_skip_reason.clone(),
        });
    }

    if plan.disable_stats_legacy_path_read_fallback {
        steps.push(MigrationStepOutput {
            operation: MIGRATION_OPERATION_DISABLE_READ_FALLBACK,
            status: status_for_apply,
            detail: "Set `feature_flags.stats_legacy_path_read_fallback = false` in config.toml."
                .to_string(),
        });
    } else {
        steps.push(MigrationStepOutput {
            operation: MIGRATION_OPERATION_DISABLE_READ_FALLBACK,
            status: MigrationStepStatus::Skipped,
            detail: "`feature_flags.stats_legacy_path_read_fallback` is already disabled."
                .to_string(),
        });
    }

    if plan.disable_stats_legacy_path_dual_write {
        steps.push(MigrationStepOutput {
            operation: MIGRATION_OPERATION_DISABLE_DUAL_WRITE,
            status: status_for_apply,
            detail: "Set `feature_flags.stats_legacy_path_dual_write = false` in config.toml."
                .to_string(),
        });
    } else {
        steps.push(MigrationStepOutput {
            operation: MIGRATION_OPERATION_DISABLE_DUAL_WRITE,
            status: MigrationStepStatus::Skipped,
            detail: "`feature_flags.stats_legacy_path_dual_write` is already disabled.".to_string(),
        });
    }

    steps
}

fn apply_migration_plan(
    config: &AppConfig,
    plan: &MigrationPlan,
    steps: &mut [MigrationStepOutput],
) -> Result<(), String> {
    let mut config_snapshot = None;
    let mut canonical_stats_snapshot = None;
    let mut staged_canonical_stats_path = None;
    let mut applied_canonical_stats = false;

    if plan.copy_legacy_stats_to_canonical {
        canonical_stats_snapshot = match snapshot_existing_file(
            &plan.canonical_stats_path,
            "snapshot existing canonical stats.toml for migration rollback",
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return Err(migration_setup_failure(
                    format!("could not prepare canonical stats snapshot: {error}"),
                    config_snapshot.as_deref(),
                    canonical_stats_snapshot.as_deref(),
                    staged_canonical_stats_path.as_deref(),
                ));
            }
        };
        let staged_path = temp_restore_path(&plan.canonical_stats_path, "staged");
        staged_canonical_stats_path = Some(staged_path.clone());
        let legacy_stats_path = match plan.legacy_stats_path.as_ref() {
            Some(path) => path,
            None => {
                return Err(migration_setup_failure(
                    "missing legacy stats path for planned copy step".to_string(),
                    config_snapshot.as_deref(),
                    canonical_stats_snapshot.as_deref(),
                    staged_canonical_stats_path.as_deref(),
                ));
            }
        };
        if let Err(error) = copy_file_with_context(
            legacy_stats_path,
            &staged_path,
            "stage migration copy of stats.toml",
        ) {
            return Err(migration_setup_failure(
                format!("could not stage canonical stats update: {error}"),
                config_snapshot.as_deref(),
                canonical_stats_snapshot.as_deref(),
                staged_canonical_stats_path.as_deref(),
            ));
        }
    }

    if plan.requires_config_update() {
        config_snapshot = match snapshot_existing_file(
            &plan.config_path,
            "snapshot existing config.toml for migration rollback",
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return Err(migration_setup_failure(
                    format!("could not prepare config snapshot: {error}"),
                    config_snapshot.as_deref(),
                    canonical_stats_snapshot.as_deref(),
                    staged_canonical_stats_path.as_deref(),
                ));
            }
        };
    }

    if let Some(staged_path) = staged_canonical_stats_path.as_deref() {
        if let Err(error) = replace_file_atomically(
            staged_path,
            &plan.canonical_stats_path,
            "apply migration copy of stats.toml",
        ) {
            return Err(migration_failure_with_rollback(
                format!("could not update canonical stats.toml: {error}"),
                rollback_migration_files(
                    config_snapshot.as_deref(),
                    &plan.config_path,
                    false,
                    canonical_stats_snapshot.as_deref(),
                    &plan.canonical_stats_path,
                    true,
                ),
            ));
        }
        applied_canonical_stats = true;
    }

    if plan.requires_config_update() {
        let mut migrated = config.clone();
        if plan.disable_stats_legacy_path_read_fallback {
            migrated.feature_flags.stats_legacy_path_read_fallback = false;
        }
        if plan.disable_stats_legacy_path_dual_write {
            migrated.feature_flags.stats_legacy_path_dual_write = false;
        }
        if let Err(error) = migrated.save() {
            return Err(migration_failure_with_rollback(
                format!("could not save migrated config.toml: {error}"),
                rollback_migration_files(
                    config_snapshot.as_deref(),
                    &plan.config_path,
                    true,
                    canonical_stats_snapshot.as_deref(),
                    &plan.canonical_stats_path,
                    applied_canonical_stats,
                ),
            ));
        }
    }

    if let Some(snapshot) = config_snapshot.as_deref() {
        let _ = remove_file_if_exists(snapshot);
    }
    if let Some(snapshot) = canonical_stats_snapshot.as_deref() {
        let _ = remove_file_if_exists(snapshot);
    }
    if let Some(staged_path) = staged_canonical_stats_path.as_deref() {
        let _ = remove_file_if_exists(staged_path);
    }

    for step in steps {
        if step.status == MigrationStepStatus::Planned {
            step.status = MigrationStepStatus::Applied;
        }
    }
    Ok(())
}

fn rollback_migration_files(
    config_snapshot: Option<&Path>,
    config_path: &Path,
    config_was_updated: bool,
    canonical_stats_snapshot: Option<&Path>,
    canonical_stats_path: &Path,
    canonical_stats_was_updated: bool,
) -> Vec<String> {
    let mut rollback_errors = Vec::new();
    if config_was_updated
        && let Err(error) = rollback_file(
            config_snapshot,
            config_path,
            "roll back migrated config.toml",
        )
    {
        rollback_errors.push(error);
    }
    if canonical_stats_was_updated
        && let Err(error) = rollback_file(
            canonical_stats_snapshot,
            canonical_stats_path,
            "roll back migrated canonical stats.toml",
        )
    {
        rollback_errors.push(error);
    }
    rollback_errors
}

fn rollback_file(snapshot: Option<&Path>, destination: &Path, context: &str) -> Result<(), String> {
    if let Some(snapshot) = snapshot {
        replace_file_atomically(snapshot, destination, context)
    } else {
        remove_file_if_exists(destination)
    }
}

fn migration_failure_with_rollback(error: String, rollback_errors: Vec<String>) -> String {
    let mut message = format!("Migration failed: {error}");
    if !rollback_errors.is_empty() {
        message.push_str("; rollback failed: ");
        message.push_str(&rollback_errors.join("; "));
    }
    message.push_str(
        ". Next steps: run `focustime --backup`, inspect `config.toml` and `stats.toml`, then retry with `focustime --migrate --dry-run`.",
    );
    message
}

fn migration_setup_failure(
    error: String,
    config_snapshot: Option<&Path>,
    canonical_stats_snapshot: Option<&Path>,
    staged_canonical_stats_path: Option<&Path>,
) -> String {
    migration_failure_with_rollback(
        error,
        cleanup_migration_temp_files(
            config_snapshot,
            canonical_stats_snapshot,
            staged_canonical_stats_path,
        ),
    )
}

fn cleanup_migration_temp_files(
    config_snapshot: Option<&Path>,
    canonical_stats_snapshot: Option<&Path>,
    staged_canonical_stats_path: Option<&Path>,
) -> Vec<String> {
    let mut cleanup_errors = Vec::new();
    let cleanup_targets = [
        ("config snapshot", config_snapshot),
        ("canonical stats snapshot", canonical_stats_snapshot),
        ("staged canonical stats", staged_canonical_stats_path),
    ];
    for (label, path) in cleanup_targets {
        if let Some(path) = path
            && let Err(error) = remove_file_if_exists(path)
        {
            cleanup_errors.push(format!(
                "cleanup failed for {label} `{}`: {error}",
                path.display()
            ));
        }
    }
    cleanup_errors
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

fn restore_legacy_stats_mirror_if_enabled(
    legacy_stats_path: Option<&Path>,
    staged_legacy_stats_path: Option<&Path>,
    original_legacy_stats_snapshot: Option<&Path>,
    original_config_snapshot: Option<&Path>,
    config_restored_path: &Path,
    original_stats_snapshot: Option<&Path>,
    stats_restored_path: &Path,
) -> Result<(), String> {
    let (Some(legacy_stats_path), Some(staged_legacy_stats_path)) =
        (legacy_stats_path, staged_legacy_stats_path)
    else {
        return Ok(());
    };
    if let Err(error) = replace_file_atomically(
        staged_legacy_stats_path,
        legacy_stats_path,
        "restore legacy stats.toml mirror",
    ) {
        rollback_restored_file(
            original_config_snapshot,
            config_restored_path,
            "roll back restored config.toml",
        );
        rollback_restored_file(
            original_stats_snapshot,
            stats_restored_path,
            "roll back restored stats.toml",
        );
        rollback_restored_file(
            original_legacy_stats_snapshot,
            legacy_stats_path,
            "roll back restored legacy stats.toml",
        );
        return Err(error);
    }
    Ok(())
}

fn config_file_path() -> Result<PathBuf, String> {
    crate::config::app_data_path(CONFIG_FILE_NAME).ok_or_else(|| {
        format!(
            "could not determine application data path for `{CONFIG_FILE_NAME}` (environment is not configured)"
        )
    })
}

fn stats_persistence_paths() -> Result<StatsPersistencePaths, String> {
    let canonical = crate::config::stats_data_path(STATS_FILE_NAME).ok_or_else(|| {
        format!(
            "could not determine application data path for `{STATS_FILE_NAME}` (environment is not configured)"
        )
    })?;
    let legacy = crate::config::app_data_path(STATS_FILE_NAME).filter(|path| path != &canonical);
    Ok(StatsPersistencePaths { canonical, legacy })
}

fn resolve_stats_backup_source_path(
    paths: &StatsPersistencePaths,
    legacy_read_fallback: bool,
) -> PathBuf {
    if paths.canonical.exists() {
        return paths.canonical.clone();
    }
    if legacy_read_fallback
        && let Some(legacy) = paths.legacy.as_ref()
        && legacy.exists()
    {
        return legacy.clone();
    }
    paths.canonical.clone()
}

fn stats_path_compatibility(config: &AppConfig) -> crate::stats::StatsPathCompatibilityOptions {
    crate::stats::StatsPathCompatibilityOptions {
        legacy_path_read_fallback: config.feature_flags.stats_legacy_path_read_fallback,
        legacy_path_dual_write: config.feature_flags.stats_legacy_path_dual_write,
    }
}

fn stats_load_options(config: &AppConfig) -> crate::stats::StatsLoadOptions {
    crate::stats::StatsLoadOptions {
        metadata_task_label_fallback: config.feature_flags.metadata_task_label_fallback,
        path_compatibility: stats_path_compatibility(config),
    }
}

fn stats_save_options(config: &AppConfig) -> crate::stats::StatsSaveOptions {
    crate::stats::StatsSaveOptions {
        path_compatibility: stats_path_compatibility(config),
    }
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
    fn apply_migration_plan_copies_legacy_stats_to_canonical() {
        let temp_dir = create_temp_dir("migrate-copy");
        let config_path = temp_dir.join("config.toml");
        let legacy_stats_path = temp_dir.join("legacy-stats.toml");
        let canonical_stats_path = temp_dir.join("state").join("stats.toml");
        fs::write(&legacy_stats_path, "daily = {}\n")
            .expect("legacy stats file should be writable");

        let plan = MigrationPlan {
            config_path,
            canonical_stats_path: canonical_stats_path.clone(),
            legacy_stats_path: Some(legacy_stats_path),
            copy_legacy_stats_to_canonical: true,
            disable_stats_legacy_path_read_fallback: false,
            disable_stats_legacy_path_dual_write: false,
            stats_copy_skip_reason: String::new(),
            warnings: Vec::new(),
        };
        let mut steps = migration_steps_for_plan(&plan);
        apply_migration_plan(&AppConfig::default(), &plan, &mut steps)
            .expect("migration copy should succeed");

        let canonical_bytes =
            fs::read(&canonical_stats_path).expect("canonical stats file should be readable");
        assert_eq!(canonical_bytes, b"daily = {}\n");

        let copy_step = steps
            .iter()
            .find(|step| step.operation == MIGRATION_OPERATION_COPY_LEGACY_STATS)
            .expect("copy step should exist");
        assert_eq!(copy_step.status, MigrationStepStatus::Applied);

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn rollback_migration_files_restores_snapshots() {
        let temp_dir = create_temp_dir("migrate-rollback");
        let config_path = temp_dir.join("config.toml");
        let config_snapshot = temp_dir.join("config.snapshot.toml");
        let canonical_stats_path = temp_dir.join("stats.toml");
        let canonical_snapshot = temp_dir.join("stats.snapshot.toml");
        fs::write(&config_path, "schema_version = 1\n").expect("config file should be writable");
        fs::write(&canonical_stats_path, "daily = {}\n")
            .expect("canonical stats file should be writable");
        fs::write(&config_snapshot, "schema_version = 0\n")
            .expect("config snapshot should be writable");
        fs::write(&canonical_snapshot, "daily = { old = true }\n")
            .expect("stats snapshot should be writable");

        let rollback_errors = rollback_migration_files(
            Some(config_snapshot.as_path()),
            &config_path,
            true,
            Some(canonical_snapshot.as_path()),
            &canonical_stats_path,
            true,
        );

        assert!(rollback_errors.is_empty());
        let restored_config =
            fs::read_to_string(&config_path).expect("restored config should exist");
        let restored_stats =
            fs::read_to_string(&canonical_stats_path).expect("restored stats should exist");
        assert_eq!(restored_config, "schema_version = 0\n");
        assert_eq!(restored_stats, "daily = { old = true }\n");

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn cleanup_migration_temp_files_removes_snapshot_artifacts() {
        let temp_dir = create_temp_dir("migrate-cleanup");
        let config_snapshot = temp_dir.join("config.original.tmp");
        let canonical_snapshot = temp_dir.join("stats.original.tmp");
        let staged_stats = temp_dir.join("stats.staged.tmp");
        fs::write(&config_snapshot, "config").expect("config snapshot should be writable");
        fs::write(&canonical_snapshot, "stats").expect("stats snapshot should be writable");
        fs::write(&staged_stats, "staged").expect("staged stats should be writable");

        let cleanup_errors = cleanup_migration_temp_files(
            Some(config_snapshot.as_path()),
            Some(canonical_snapshot.as_path()),
            Some(staged_stats.as_path()),
        );

        assert!(cleanup_errors.is_empty());
        assert!(!config_snapshot.exists());
        assert!(!canonical_snapshot.exists());
        assert!(!staged_stats.exists());

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }
}
