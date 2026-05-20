use std::{
    env, fs,
    path::Path,
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use crate::app::App;
use crate::config::validate_automation_trigger_rules;

use crate::cli::{
    AppConfig, AutomationTriggerRuleConfig, AutomationTriggersCommandOutput, BackupOutput,
    BlocklistCategoryCommandKind, BlocklistCategoryCommandOutput, BlocklistCategorySummaryOutput,
    BlocklistProfileCommandKind, BlocklistProfileCommandOutput, BlocklistProfileConfig,
    BlocklistProfileSummaryOutput, BlocklistSiteCommandKind, BreakGlassCommandOutput, CliCommand,
    CommandKind, DailyGoalConfig, DailyGoalSnapshot, EditSiteResult, ExportOutput, FocusStats,
    GoalCarryCommandOutput, GoalCommandOutput, InvalidSiteEntryOutput, InvalidSiteInput,
    MonthlyGoalConfig, OutputMode, PathBuf, ProfileId, ProfileOutput, ProfileView,
    RecurringScheduleConfig, RestoreOutput, ScheduleCommandOutput, ScheduleDelayCommandOutput,
    SessionMetadataCommandOutput, SessionTemplateCommandKind, SessionTemplateCommandOutput,
    SessionTemplateSummaryOutput, SiteAddCommandOutput, SiteBlocker, SiteDeleteCommandOutput,
    SiteEditCommandOutput, SiteEditValue, SiteListCommandOutput, SiteListTarget, StatusOutput,
    StrictCommandOutput, TaskCommandOutput, TaskGoalCommandOutput, TaskGoalOutput,
    TemporaryAllowlistStatusOutput, TemporarySiteAddCommandOutput, ThemeCommandOutput, ThemePreset,
    TimerCommandOutput, TimerStateOutput, WeekdayProfileRuleConfig, WeekdayRulesCommandOutput,
    WeeklyGoalConfig, available_break_template_views, available_theme_preset_views,
    build_blocking_preview_command_output, build_diagnostics_command_output,
    build_schedule_inspection_output, build_status_output, build_task_goal_output,
    display_input_value, effective_blocked_sites_for_profile, flush_stdout,
    print_automation_triggers_command_output, print_backup_output,
    print_blocking_preview_command_output, print_blocklist_category_command_output,
    print_blocklist_profile_command_output, print_break_glass_command_output,
    print_diagnostics_command_output, print_export_output, print_goal_carry_command_output,
    print_goal_command_output, print_json, print_json_compact, print_profile_output,
    print_restore_output, print_schedule_command_output, print_schedule_delay_command_output,
    print_session_metadata_command_output, print_session_template_command_output,
    print_site_add_command_output, print_site_delete_command_output,
    print_site_edit_command_output, print_site_list_command_output, print_status_output,
    print_strict_command_output, print_task_goal_command_output,
    print_temporary_site_add_command_output, print_theme_command_output, print_timer_state_output,
    print_weekday_rules_command_output, profile_id, profile_view, selected_break_template_view,
    theme_preset_view, timer_phase_id, timer_status_id,
};

const CONFIG_FILE_NAME: &str = "config.toml";
const STATS_FILE_NAME: &str = "stats.toml";
const WATCH_INTERRUPT_POLL_INTERVAL: Duration = Duration::from_millis(200);

static WATCH_INTERRUPTED: AtomicBool = AtomicBool::new(false);
static WATCH_INTERRUPT_HANDLER: OnceLock<Result<(), String>> = OnceLock::new();

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
        CommandKind::WeekdayRules { rules } => {
            execute_weekday_rules_command(rules, cli_command.output)
        }
        CommandKind::AutomationTriggers { rules } => {
            execute_automation_triggers_command(rules, cli_command.output)
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
        CommandKind::Export { dir } => execute_export_command(dir, cli_command.output),
        CommandKind::BlocklistProfile { command } => {
            execute_blocklist_profile_command(command, cli_command.output)
        }
        CommandKind::BlocklistCategory { command } => {
            execute_blocklist_category_command(command, cli_command.output)
        }
        CommandKind::BlocklistSites { target, command } => {
            execute_blocklist_sites_command(target, command, cli_command.output)
        }
        CommandKind::AllowlistSiteAddTemporary { input } => {
            execute_allowlist_site_add_temporary_command(input, cli_command.output)
        }
        CommandKind::SessionTemplate { command } => {
            execute_session_template_command(command, cli_command.output)
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

fn execute_blocklist_category_command(
    command: BlocklistCategoryCommandKind,
    output: OutputMode,
) -> Result<(), String> {
    let mut config = AppConfig::load().normalized();
    let payload = apply_blocklist_category_command(&mut config, command)?;
    if payload.updated {
        config
            .save()
            .map_err(|error| format!("Failed to save blocklist category settings: {error}"))?;
    }
    match output {
        OutputMode::Text => print_blocklist_category_command_output(&payload),
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

fn execute_allowlist_site_add_temporary_command(
    input: String,
    output: OutputMode,
) -> Result<(), String> {
    let mut app = App::new();
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

fn execute_session_template_command(
    command: SessionTemplateCommandKind,
    output: OutputMode,
) -> Result<(), String> {
    let mut app = App::new();
    let (action, updated) = match command {
        SessionTemplateCommandKind::Select { name } => (
            "session-template",
            app.select_session_template_for_cli(name.as_deref())?,
        ),
        SessionTemplateCommandKind::Apply { name } => (
            "session-template-apply",
            app.apply_session_template_for_cli(name.as_deref())?,
        ),
        SessionTemplateCommandKind::Create { name } => (
            "session-template-create",
            app.create_session_template_for_cli(&name)?,
        ),
        SessionTemplateCommandKind::Rename { name } => (
            "session-template-rename",
            app.rename_active_session_template_for_cli(&name)?,
        ),
        SessionTemplateCommandKind::Delete => (
            "session-template-delete",
            app.delete_active_session_template_for_cli()?,
        ),
    };
    let payload = build_session_template_command_output(&app, action, updated);
    match output {
        OutputMode::Text => print_session_template_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
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

pub(super) fn apply_blocklist_category_command(
    config: &mut AppConfig,
    command: BlocklistCategoryCommandKind,
) -> Result<BlocklistCategoryCommandOutput, String> {
    ensure_blocklist_profiles(config);
    let profile_index = selected_blocklist_profile_index(config);
    ensure_blocklist_categories(&mut config.blocklist_profiles[profile_index]);
    let profile = &mut config.blocklist_profiles[profile_index];

    let (action, updated) = match command {
        BlocklistCategoryCommandKind::Select { category } => (
            "blocklist-category",
            handle_select_blocklist_category(profile, category)?,
        ),
        BlocklistCategoryCommandKind::Create { name } => (
            "blocklist-category-create",
            handle_create_blocklist_category(profile, name)?,
        ),
        BlocklistCategoryCommandKind::Rename { name } => (
            "blocklist-category-rename",
            handle_rename_blocklist_category(profile, name)?,
        ),
        BlocklistCategoryCommandKind::Delete => (
            "blocklist-category-delete",
            handle_delete_blocklist_category(profile)?,
        ),
    };

    sync_profile_site_mirrors(profile);
    sync_selected_blocklist_profile(config);
    Ok(build_blocklist_category_command_output(
        config, action, updated,
    ))
}

fn handle_select_blocklist_category(
    profile: &mut BlocklistProfileConfig,
    category: Option<String>,
) -> Result<bool, String> {
    let Some(category) = category else {
        return Ok(false);
    };
    let index = profile
        .categories
        .iter()
        .position(|candidate| candidate.name.eq_ignore_ascii_case(category.trim()))
        .ok_or_else(|| format!("Unknown blocklist category `{category}`."))?;
    let selected = profile.categories[index].name.clone();
    if profile.selected_category.eq_ignore_ascii_case(&selected) {
        return Ok(false);
    }
    profile.selected_category = selected;
    Ok(true)
}

fn handle_create_blocklist_category(
    profile: &mut BlocklistProfileConfig,
    name: String,
) -> Result<bool, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Category name cannot be empty.".to_string());
    }
    if profile
        .categories
        .iter()
        .any(|category| category.name.eq_ignore_ascii_case(&name))
    {
        return Err(format!("Category `{name}` already exists."));
    }
    profile
        .categories
        .push(crate::config::BlocklistCategoryConfig {
            name: name.clone(),
            sites: Vec::new(),
            allowlist_sites: Vec::new(),
        });
    profile.selected_category = name;
    Ok(true)
}

fn handle_rename_blocklist_category(
    profile: &mut BlocklistProfileConfig,
    name: String,
) -> Result<bool, String> {
    let index = selected_blocklist_category_index(profile);
    let current = profile.categories[index].name.clone();
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Category name cannot be empty.".to_string());
    }
    if current.eq_ignore_ascii_case(&name) {
        return Ok(false);
    }
    let duplicate = profile
        .categories
        .iter()
        .enumerate()
        .any(|(candidate_index, category)| {
            candidate_index != index && category.name.eq_ignore_ascii_case(&name)
        });
    if duplicate {
        return Err(format!("Category `{name}` already exists."));
    }
    profile.categories[index].name = name.clone();
    profile.selected_category = name;
    Ok(true)
}

fn handle_delete_blocklist_category(profile: &mut BlocklistProfileConfig) -> Result<bool, String> {
    if profile.categories.len() <= 1 {
        return Err("At least one blocklist category is required.".to_string());
    }
    let index = selected_blocklist_category_index(profile);
    profile.categories.remove(index);
    let next_index = index.min(profile.categories.len().saturating_sub(1));
    profile.selected_category = profile.categories[next_index].name.clone();
    Ok(true)
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
        categories: vec![crate::config::BlocklistCategoryConfig {
            name: "General".to_string(),
            sites: Vec::new(),
            allowlist_sites: Vec::new(),
        }],
        selected_category: "General".to_string(),
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
    sync_profile_site_mirrors(&mut config.blocklist_profiles[index]);
    if updated {
        sync_selected_blocklist_profile(config);
    }

    let active_profile = &config.blocklist_profiles[index];
    Ok(SiteAddCommandOutput {
        action: "site-add",
        updated,
        profile: active_profile.name.clone(),
        category: active_profile.selected_category.clone(),
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
            sync_profile_site_mirrors(&mut config.blocklist_profiles[index]);
            sync_selected_blocklist_profile(config);
            let active_profile = &config.blocklist_profiles[index];
            Ok(SiteEditCommandOutput {
                action: "site-edit",
                updated: true,
                profile: active_profile.name.clone(),
                category: active_profile.selected_category.clone(),
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
                category: active_profile.selected_category.clone(),
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
    sync_profile_site_mirrors(&mut config.blocklist_profiles[index]);
    sync_selected_blocklist_profile(config);

    let active_profile = &config.blocklist_profiles[index];
    Ok(SiteDeleteCommandOutput {
        action: "site-delete",
        updated: true,
        profile: active_profile.name.clone(),
        category: active_profile.selected_category.clone(),
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
    let index = selected_blocklist_profile_index(config);
    ensure_blocklist_categories(&mut config.blocklist_profiles[index]);
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
    let profile = &config.blocklist_profiles[profile_index];
    if profile.categories.is_empty() {
        return match target {
            SiteListTarget::Blocklist => &profile.sites,
            SiteListTarget::Allowlist => &profile.allowlist_sites,
        };
    }
    let category_index = selected_blocklist_category_index(profile);
    let category = &profile.categories[category_index];
    match target {
        SiteListTarget::Blocklist => &category.sites,
        SiteListTarget::Allowlist => &category.allowlist_sites,
    }
}

fn active_profile_sites_mut(
    config: &mut AppConfig,
    profile_index: usize,
    target: SiteListTarget,
) -> &mut Vec<String> {
    ensure_blocklist_categories(&mut config.blocklist_profiles[profile_index]);
    let category_index =
        selected_blocklist_category_index(&config.blocklist_profiles[profile_index]);
    let category = &mut config.blocklist_profiles[profile_index].categories[category_index];
    match target {
        SiteListTarget::Blocklist => &mut category.sites,
        SiteListTarget::Allowlist => &mut category.allowlist_sites,
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

fn build_blocklist_category_command_output(
    config: &AppConfig,
    action: &'static str,
    updated: bool,
) -> BlocklistCategoryCommandOutput {
    let profile_index = selected_blocklist_profile_index(config);
    let profile = &config.blocklist_profiles[profile_index];
    let selected_category = if profile.categories.is_empty() {
        "General".to_string()
    } else {
        let index = selected_blocklist_category_index(profile);
        profile.categories[index].name.clone()
    };
    let categories = profile
        .categories
        .iter()
        .map(|category| BlocklistCategorySummaryOutput {
            name: category.name.clone(),
            active: category.name.eq_ignore_ascii_case(&selected_category),
            blocklist_sites_count: category.sites.len(),
            allowlist_sites_count: category.allowlist_sites.len(),
        })
        .collect();
    BlocklistCategoryCommandOutput {
        action,
        updated,
        selected_blocklist_profile: profile.name.clone(),
        selected_blocklist_category: selected_category,
        categories,
    }
}

fn build_session_template_command_output(
    app: &App,
    action: &'static str,
    updated: bool,
) -> SessionTemplateCommandOutput {
    let selected_session_template = app.active_session_template_name().map(str::to_string);
    let mut templates = Vec::with_capacity(app.session_template_count());
    templates.extend(app.session_templates.iter().map(|template| {
        SessionTemplateSummaryOutput {
            name: template.name.clone(),
            active: selected_session_template
                .as_deref()
                .is_some_and(|selected| selected.eq_ignore_ascii_case(&template.name)),
            task_label: template.task_label.clone(),
            profile: profile_id(template.profile),
            blocklist_profile: template.blocklist_profile.clone(),
            schedule_windows_count: template.schedule.windows.len()
                + template.schedule.one_time_windows.len(),
        }
    }));
    SessionTemplateCommandOutput {
        action,
        updated,
        selected_session_template,
        templates,
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
            category: "General".to_string(),
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
        category: profile.selected_category.clone(),
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

fn ensure_blocklist_categories(profile: &mut BlocklistProfileConfig) {
    if profile.categories.is_empty() {
        profile
            .categories
            .push(crate::config::BlocklistCategoryConfig {
                name: "General".to_string(),
                sites: profile.sites.clone(),
                allowlist_sites: profile.allowlist_sites.clone(),
            });
    }
    let selected = profile.selected_category.trim().to_string();
    if selected.is_empty() {
        profile.selected_category = profile
            .categories
            .first()
            .map(|category| category.name.clone())
            .unwrap_or_else(|| "General".to_string());
    } else if let Some(category) = profile
        .categories
        .iter()
        .find(|category| category.name.eq_ignore_ascii_case(&selected))
    {
        profile.selected_category = category.name.clone();
    } else {
        profile.selected_category = profile
            .categories
            .first()
            .map(|category| category.name.clone())
            .unwrap_or_else(|| "General".to_string());
    }
}

fn selected_blocklist_category_index(profile: &BlocklistProfileConfig) -> usize {
    profile
        .categories
        .iter()
        .position(|category| {
            category
                .name
                .eq_ignore_ascii_case(&profile.selected_category)
        })
        .unwrap_or(0)
}

fn sync_profile_site_mirrors(profile: &mut BlocklistProfileConfig) {
    let mut sites: Vec<String> = Vec::new();
    let mut allowlist_sites: Vec<String> = Vec::new();
    for category in &profile.categories {
        for site in &category.sites {
            if !sites
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(site))
            {
                sites.push(site.clone());
            }
        }
        for site in &category.allowlist_sites {
            if !allowlist_sites
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(site))
            {
                allowlist_sites.push(site.clone());
            }
        }
    }
    profile.sites = sites;
    profile.allowlist_sites = allowlist_sites;
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

fn execute_weekday_rules_command(
    rules: Option<Vec<WeekdayProfileRuleConfig>>,
    output: OutputMode,
) -> Result<(), String> {
    let mut config = AppConfig::load().normalized();
    let mut updated = false;
    if let Some(rules) = rules {
        config.weekday_profile_rules = rules;
        config = config.normalized();
        config
            .save()
            .map_err(|error| format!("Failed to save weekday rules: {error}"))?;
        updated = true;
    }

    let payload = WeekdayRulesCommandOutput {
        updated,
        rules: config.weekday_profile_rules.clone(),
    };

    match output {
        OutputMode::Text => print_weekday_rules_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn execute_automation_triggers_command(
    rules: Option<Vec<AutomationTriggerRuleConfig>>,
    output: OutputMode,
) -> Result<(), String> {
    let mut config = AppConfig::load().normalized();
    let mut updated = false;
    if let Some(rules) = rules {
        config.automation_triggers = validate_and_normalize_automation_triggers(rules, &config)?;
        config
            .save()
            .map_err(|error| format!("Failed to save automation trigger rules: {error}"))?;
        updated = true;
    }

    let payload = AutomationTriggersCommandOutput {
        updated,
        rules: config.automation_triggers.clone(),
    };

    match output {
        OutputMode::Text => print_automation_triggers_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn validate_and_normalize_automation_triggers(
    rules: Vec<AutomationTriggerRuleConfig>,
    config: &AppConfig,
) -> Result<Vec<AutomationTriggerRuleConfig>, String> {
    validate_automation_trigger_rules(
        &rules,
        &config.blocklist_profiles,
        &config.session_templates,
    )
    .map_err(|error| format!("Invalid automation trigger rules: {error}"))?;

    let mut normalized = config.clone();
    normalized.automation_triggers = rules;
    Ok(normalized.normalized().automation_triggers)
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

    install_watch_interrupt_handler()?;
    WATCH_INTERRUPTED.store(false, Ordering::SeqCst);
    let interval = Duration::from_secs(interval_secs);
    let mut next_deadline = Instant::now();

    loop {
        if WATCH_INTERRUPTED.load(Ordering::SeqCst) {
            break;
        }

        let payload = load_status_output()?;
        emit_status_output(&payload, output, true)?;
        flush_stdout()?;

        next_deadline = next_watch_deadline(next_deadline, interval, Instant::now());
        if wait_for_next_watch_tick(next_deadline) {
            break;
        }
    }

    Ok(())
}

fn install_watch_interrupt_handler() -> Result<(), String> {
    WATCH_INTERRUPT_HANDLER
        .get_or_init(|| unsafe { install_platform_watch_interrupt_handler() })
        .clone()
}

fn next_watch_deadline(previous_deadline: Instant, interval: Duration, now: Instant) -> Instant {
    let mut deadline = previous_deadline + interval;
    while deadline <= now {
        deadline += interval;
    }
    deadline
}

fn wait_for_next_watch_tick(deadline: Instant) -> bool {
    loop {
        if WATCH_INTERRUPTED.load(Ordering::SeqCst) {
            return true;
        }

        let now = Instant::now();
        if now >= deadline {
            return false;
        }

        let sleep_for = deadline
            .saturating_duration_since(now)
            .min(WATCH_INTERRUPT_POLL_INTERVAL);
        thread::sleep(sleep_for);
    }
}

#[cfg(unix)]
unsafe fn install_platform_watch_interrupt_handler() -> Result<(), String> {
    unsafe extern "C" fn handle_sigint(_signal: i32) {
        WATCH_INTERRUPTED.store(true, Ordering::SeqCst);
    }

    unsafe extern "C" {
        fn signal(signum: i32, handler: unsafe extern "C" fn(i32)) -> unsafe extern "C" fn(i32);
    }

    const SIGINT: i32 = 2;

    let _previous = unsafe { signal(SIGINT, handle_sigint) };
    Ok(())
}

#[cfg(target_os = "windows")]
unsafe fn install_platform_watch_interrupt_handler() -> Result<(), String> {
    unsafe extern "system" fn handle_console_ctrl(ctrl_type: u32) -> i32 {
        const CTRL_C_EVENT: u32 = 0;
        const CTRL_BREAK_EVENT: u32 = 1;
        if ctrl_type == CTRL_C_EVENT || ctrl_type == CTRL_BREAK_EVENT {
            WATCH_INTERRUPTED.store(true, Ordering::SeqCst);
            return 1;
        }
        0
    }

    unsafe extern "system" {
        fn SetConsoleCtrlHandler(
            handler_routine: Option<unsafe extern "system" fn(u32) -> i32>,
            add: i32,
        ) -> i32;
    }

    let installed = unsafe { SetConsoleCtrlHandler(Some(handle_console_ctrl), 1) };
    if installed == 0 {
        Err(
            "Failed to install watch interrupt handler: SetConsoleCtrlHandler returned 0."
                .to_string(),
        )
    } else {
        Ok(())
    }
}

#[cfg(not(any(unix, target_os = "windows")))]
unsafe fn install_platform_watch_interrupt_handler() -> Result<(), String> {
    Ok(())
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
    use std::sync::{Mutex, OnceLock};

    static WATCH_TEST_GUARD: OnceLock<Mutex<()>> = OnceLock::new();

    fn guard_watch_state() -> std::sync::MutexGuard<'static, ()> {
        WATCH_TEST_GUARD
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("watch test guard should lock")
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

    #[test]
    fn validate_and_normalize_automation_triggers_rejects_invalid_delay_without_clamping() {
        let config = AppConfig::default().normalized();
        let rules = vec![AutomationTriggerRuleConfig {
            trigger: crate::config::AutomationTriggerConditionConfig::ScheduleWindowEnd,
            action: crate::config::AutomationTriggerActionConfig::DelayScheduleStart {
                delay_secs: 0,
            },
        }];

        let error = validate_and_normalize_automation_triggers(rules, &config).unwrap_err();

        assert!(error.contains("Invalid automation trigger rules"));
        assert!(error.contains("delay_secs"));
    }

    #[test]
    fn validate_and_normalize_automation_triggers_normalizes_valid_day_aliases() {
        let config = AppConfig::default().normalized();
        let rules = vec![AutomationTriggerRuleConfig {
            trigger: crate::config::AutomationTriggerConditionConfig::Time {
                days: vec!["MONDAY".to_string(), "monday".to_string()],
                at: "09:00".to_string(),
            },
            action: crate::config::AutomationTriggerActionConfig::StartFocus,
        }];

        let normalized = validate_and_normalize_automation_triggers(rules, &config)
            .expect("valid rules should normalize");

        assert_eq!(
            normalized[0].trigger,
            crate::config::AutomationTriggerConditionConfig::Time {
                days: vec!["mon".to_string()],
                at: "09:00".to_string(),
            }
        );
    }
}
