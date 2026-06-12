use crate::cli::{
    AppConfig, BLOCKLIST_CATEGORY_REPLACEMENT, BlocklistCategoryCommandKind,
    BlocklistCategoryCommandOutput, BlocklistCategorySummaryOutput, BlocklistProfileCommandKind,
    BlocklistProfileCommandOutput, BlocklistProfileConfig, BlocklistProfileSummaryOutput,
    BlocklistSiteCommandKind, EditSiteResult, InvalidSiteEntryOutput, InvalidSiteInput, OutputMode,
    SiteAddCommandOutput, SiteBlocker, SiteDeleteCommandOutput, SiteEditCommandOutput,
    SiteEditValue, SiteListCommandOutput, SiteListTarget, display_input_value,
    effective_blocked_sites_for_profile, print_blocklist_category_command_output,
    print_blocklist_profile_command_output, print_json, print_site_add_command_output,
    print_site_delete_command_output, print_site_edit_command_output,
    print_site_list_command_output,
};

pub(super) fn execute_blocklist_profile_command(
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

pub(super) fn execute_blocklist_category_command(
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

pub(super) fn execute_blocklist_sites_command(
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

pub(in crate::cli) fn apply_blocklist_profile_command(
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

pub(in crate::cli) fn apply_blocklist_category_command(
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
        categories: Vec::new(),
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
    if current_name.eq_ignore_ascii_case(&name) {
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

pub(in crate::cli) fn apply_site_add_command(
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

pub(in crate::cli) fn apply_site_edit_command(
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

pub(in crate::cli) fn apply_site_delete_command(
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
    let profile = &config.blocklist_profiles[profile_index];
    match target {
        SiteListTarget::Blocklist => &profile.sites,
        SiteListTarget::Allowlist => &profile.allowlist_sites,
    }
}

fn active_profile_sites_mut(
    config: &mut AppConfig,
    profile_index: usize,
    target: SiteListTarget,
) -> &mut Vec<String> {
    let profile = &mut config.blocklist_profiles[profile_index];
    match target {
        SiteListTarget::Blocklist => &mut profile.sites,
        SiteListTarget::Allowlist => &mut profile.allowlist_sites,
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
        deprecated: true,
        replacement: BLOCKLIST_CATEGORY_REPLACEMENT,
        updated,
        selected_blocklist_profile: profile.name.clone(),
        selected_blocklist_category: selected_category,
        categories,
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
