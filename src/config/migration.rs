use super::{
    AppConfig, AutoStartConfig, CURRENT_CONFIG_SCHEMA_VERSION, ConfigHealthFinding,
    ConfigHealthSeverity, ConfigHealthStatus, ConfigMigrationStepReport,
    LEGACY_CONFIG_SCHEMA_VERSION, NotificationConfig, RecurringScheduleConfig,
    canonical_profile_id_token, default_focus_secs, default_long_break_interval,
    default_long_break_secs, default_short_break_secs,
};

/// Migrates raw TOML config data to the current schema without step details.
pub(super) fn migrate_config_toml_to_current(config_toml: toml::Value) -> Option<toml::Value> {
    migrate_config_toml_to_current_detailed(config_toml)
        .ok()
        .map(|(migrated, _, _)| migrated)
}

/// Migrates raw TOML config data to the current schema and reports each applied step.
pub(super) fn migrate_config_toml_to_current_detailed(
    mut config_toml: toml::Value,
) -> Result<(toml::Value, u32, Vec<ConfigMigrationStepReport>), String> {
    let schema_version = detect_config_schema_version(&config_toml)
        .ok_or_else(|| "Missing or invalid `schema_version` in config.toml.".to_string())?;
    let mut steps = Vec::new();
    if schema_version > CURRENT_CONFIG_SCHEMA_VERSION {
        // Forward-compatibility mode: keep unknown/newer schema content untouched.
        return Ok((config_toml, schema_version, steps));
    }

    let mut from_schema_version = schema_version;
    while from_schema_version < CURRENT_CONFIG_SCHEMA_VERSION {
        let to_schema_version = from_schema_version + 1;
        config_toml = migrate_config_toml_step(config_toml, from_schema_version).ok_or_else(|| {
            format!(
                "Unsupported migration step from schema v{from_schema_version} to v{to_schema_version}."
            )
        })?;
        steps.push(ConfigMigrationStepReport {
            from_schema_version,
            to_schema_version,
            summary: migration_step_summary(from_schema_version, to_schema_version),
        });
        from_schema_version = to_schema_version;
    }
    let canonicalization_input = config_toml.clone();
    canonicalize_legacy_profile_aliases(&mut config_toml);
    if canonicalization_input != config_toml {
        steps.push(ConfigMigrationStepReport {
            from_schema_version,
            to_schema_version: from_schema_version,
            summary: "Canonicalize legacy profile aliases in config values.".to_string(),
        });
    }
    let weekday_rules_input = config_toml.clone();
    remove_weekday_profile_rules(&mut config_toml);
    if weekday_rules_input != config_toml {
        steps.push(ConfigMigrationStepReport {
            from_schema_version,
            to_schema_version: from_schema_version,
            summary: "Remove deprecated weekday profile rules; use profile schedules directly."
                .to_string(),
        });
    }
    let automation_triggers_input = config_toml.clone();
    remove_automation_triggers(&mut config_toml);
    if automation_triggers_input != config_toml {
        steps.push(ConfigMigrationStepReport {
            from_schema_version,
            to_schema_version: from_schema_version,
            summary: "Remove deprecated automation triggers; use profile schedules directly."
                .to_string(),
        });
    }
    let schedule_exception_dates_input = config_toml.clone();
    remove_schedule_exception_dates(&mut config_toml);
    if schedule_exception_dates_input != config_toml {
        steps.push(ConfigMigrationStepReport {
            from_schema_version,
            to_schema_version: from_schema_version,
            summary: "Remove deprecated schedule exception dates; schedules now use recurring windows only."
                .to_string(),
        });
    }
    let session_templates_input = config_toml.clone();
    remove_session_templates(&mut config_toml);
    if session_templates_input != config_toml {
        steps.push(ConfigMigrationStepReport {
            from_schema_version,
            to_schema_version: from_schema_version,
            summary: "Remove retired session template persistence; use task, profile, schedule, and blocklist settings directly."
                .to_string(),
        });
    }
    let blocklist_category_input = config_toml.clone();
    migrate_blocklist_categories_to_profile_rules(&mut config_toml);
    if blocklist_category_input != config_toml {
        steps.push(ConfigMigrationStepReport {
            from_schema_version,
            to_schema_version: from_schema_version,
            summary: "Flatten deprecated blocklist category rules into profile-level site lists."
                .to_string(),
        });
    }
    Ok((config_toml, schema_version, steps))
}

/// Describes a schema-version migration step for user-facing reports.
pub(super) fn migration_step_summary(from_schema_version: u32, to_schema_version: u32) -> String {
    match (from_schema_version, to_schema_version) {
        (0, 1) => "Add explicit config schema version marker.".to_string(),
        (1, 2) => {
            "Rename legacy profile tokens and profile_automation preset keys to canonical values."
                .to_string()
        }
        _ => format!("Migrate config schema from v{from_schema_version} to v{to_schema_version}."),
    }
}

/// Rewrites legacy profile aliases inside raw config tables before deserialization.
pub(super) fn canonicalize_legacy_profile_aliases(config_toml: &mut toml::Value) {
    let Some(table) = config_toml.as_table_mut() else {
        return;
    };
    migrate_profile_value_in_table(table, "selected_profile");
    migrate_profile_automation_preset_keys(table);
}

/// Removes retired weekday profile rules without migrating them to replacement triggers.
pub(super) fn remove_weekday_profile_rules(config_toml: &mut toml::Value) {
    let Some(table) = config_toml.as_table_mut() else {
        return;
    };
    if !table.contains_key("weekday_profile_rules") {
        return;
    }
    table.remove("weekday_profile_rules");
}

/// Removes retired standalone automation trigger rules.
pub(super) fn remove_automation_triggers(config_toml: &mut toml::Value) {
    let Some(table) = config_toml.as_table_mut() else {
        return;
    };
    if !table.contains_key("automation_triggers") {
        return;
    }
    table.remove("automation_triggers");
}

/// Removes retired schedule exception dates from all persisted schedule surfaces.
pub(super) fn remove_schedule_exception_dates(config_toml: &mut toml::Value) {
    let Some(table) = config_toml.as_table_mut() else {
        return;
    };
    remove_exception_dates_from_named_schedule(table, "recurring_schedule");
    remove_profile_automation_schedule_exception_dates(table);
}

/// Removes retired session template persistence fields.
pub(super) fn remove_session_templates(config_toml: &mut toml::Value) {
    let Some(table) = config_toml.as_table_mut() else {
        return;
    };
    table.remove("session_templates");
    table.remove("selected_session_template");
}

fn remove_exception_dates_from_named_schedule(
    table: &mut toml::map::Map<String, toml::Value>,
    schedule_key: &str,
) {
    let Some(schedule) = table
        .get_mut(schedule_key)
        .and_then(toml::Value::as_table_mut)
    else {
        return;
    };
    schedule.remove("exception_dates");
}

fn remove_profile_automation_schedule_exception_dates(
    table: &mut toml::map::Map<String, toml::Value>,
) {
    let Some(profile_automation) = table
        .get_mut("profile_automation")
        .and_then(toml::Value::as_table_mut)
    else {
        return;
    };
    for (_, preset) in profile_automation.iter_mut() {
        let Some(preset) = preset.as_table_mut() else {
            continue;
        };
        remove_exception_dates_from_named_schedule(preset, "recurring_schedule");
    }
}

/// Moves deprecated blocklist category rules into profile-level blocked-site entries.
pub(super) fn migrate_blocklist_categories_to_profile_rules(config_toml: &mut toml::Value) {
    let Some(table) = config_toml.as_table_mut() else {
        return;
    };
    let Some(profiles) = table
        .get_mut("blocklist_profiles")
        .and_then(toml::Value::as_array_mut)
    else {
        return;
    };

    for profile in profiles {
        let Some(profile) = profile.as_table_mut() else {
            continue;
        };
        let existing_sites = string_array(profile.get("sites"));
        let existing_allowlist_sites = string_array(profile.get("allowlist_sites"));
        let categories = profile.remove("categories");
        profile.remove("selected_category");
        let Some(toml::Value::Array(categories)) = categories else {
            continue;
        };
        if categories.is_empty() {
            continue;
        }

        let (sites, allowlist_sites) = flatten_blocklist_category_values(
            &categories,
            &existing_sites,
            &existing_allowlist_sites,
        );
        profile.insert("sites".to_string(), string_values(sites));
        profile.insert(
            "allowlist_sites".to_string(),
            string_values(allowlist_sites),
        );
    }
}

fn flatten_blocklist_category_values(
    categories: &[toml::Value],
    legacy_sites: &[String],
    legacy_allowlist_sites: &[String],
) -> (Vec<String>, Vec<String>) {
    let mut normalized = Vec::new();
    let mut seen_names = std::collections::HashSet::new();
    for category in categories {
        let Some(category) = category.as_table() else {
            continue;
        };
        let name = category
            .get("name")
            .and_then(toml::Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or("General")
            .to_string();
        let name = unique_name(&name, &mut seen_names);
        normalized.push((
            name,
            string_array(category.get("sites")),
            string_array(category.get("allowlist_sites")),
        ));
    }

    if normalized.is_empty() {
        return (
            dedup_case_insensitive_strings(legacy_sites.iter().cloned()),
            dedup_case_insensitive_strings(legacy_allowlist_sites.iter().cloned()),
        );
    }

    if !legacy_sites.is_empty() || !legacy_allowlist_sites.is_empty() {
        let general_index = normalized
            .iter()
            .position(|(name, _, _)| name.eq_ignore_ascii_case("General"))
            .unwrap_or_else(|| {
                normalized.push(("General".to_string(), Vec::new(), Vec::new()));
                normalized.len().saturating_sub(1)
            });
        merge_unique_case_insensitive(&mut normalized[general_index].1, legacy_sites);
        merge_unique_case_insensitive(&mut normalized[general_index].2, legacy_allowlist_sites);
    }

    let mut sites = Vec::new();
    let mut allowlist_sites = Vec::new();
    for (_, category_sites, category_allowlist_sites) in normalized {
        merge_unique_case_insensitive(&mut sites, &category_sites);
        merge_unique_case_insensitive(&mut allowlist_sites, &category_allowlist_sites);
    }
    (sites, allowlist_sites)
}

fn string_array(value: Option<&toml::Value>) -> Vec<String> {
    value
        .and_then(toml::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(toml::Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn string_values(values: Vec<String>) -> toml::Value {
    toml::Value::Array(values.into_iter().map(toml::Value::String).collect())
}

fn unique_name(base_name: &str, seen_names: &mut std::collections::HashSet<String>) -> String {
    if seen_names.insert(base_name.to_ascii_lowercase()) {
        return base_name.to_string();
    }

    let mut suffix = 2usize;
    loop {
        let candidate = format!("{base_name} ({suffix})");
        if seen_names.insert(candidate.to_ascii_lowercase()) {
            return candidate;
        }
        suffix += 1;
    }
}

fn dedup_case_insensitive_strings<I>(values: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    let mut seen = std::collections::HashSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.to_ascii_lowercase()))
        .collect()
}

fn merge_unique_case_insensitive(target: &mut Vec<String>, source: &[String]) {
    let mut seen: std::collections::HashSet<String> = target
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect();
    for value in source {
        if seen.insert(value.to_ascii_lowercase()) {
            target.push(value.clone());
        }
    }
}

/// Builds a warning-level config health finding with sorted advice messages.
pub(super) fn config_health_warning(
    code: impl Into<String>,
    message: impl Into<String>,
    remediation: impl Into<String>,
) -> ConfigHealthFinding {
    ConfigHealthFinding {
        code: code.into(),
        severity: ConfigHealthSeverity::Warning,
        message: message.into(),
        remediation: remediation.into(),
    }
}

/// Builds an error-level config health finding with sorted advice messages.
pub(super) fn config_health_error(
    code: impl Into<String>,
    message: impl Into<String>,
    remediation: impl Into<String>,
) -> ConfigHealthFinding {
    ConfigHealthFinding {
        code: code.into(),
        severity: ConfigHealthSeverity::Error,
        message: message.into(),
        remediation: remediation.into(),
    }
}

/// Collapses config health findings into the highest-severity status.
pub(super) fn summarize_config_health(findings: &[ConfigHealthFinding]) -> ConfigHealthStatus {
    if findings
        .iter()
        .any(|finding| finding.severity == ConfigHealthSeverity::Error)
    {
        return ConfigHealthStatus::Error;
    }
    if findings.is_empty() {
        ConfigHealthStatus::Ok
    } else {
        ConfigHealthStatus::Warning
    }
}

/// Sorts config health findings into a deterministic display order.
pub(super) fn sort_config_health_findings(findings: &mut [ConfigHealthFinding]) {
    findings.sort_by(|left, right| {
        left.code
            .cmp(&right.code)
            .then_with(|| left.message.cmp(&right.message))
    });
}

/// Maps a legacy profile token to its canonical replacement, if one exists.
pub(super) fn legacy_profile_token_migration_target(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "classic" => Some("basic"),
        "deep-work" | "deep_work" | "deepwork" => Some("standard"),
        "custom" => Some("advanced"),
        _ => None,
    }
}

/// Collects guidance for legacy profile names that can be canonicalized automatically.
pub(super) fn collect_legacy_profile_rename_advice(config_toml: &toml::Value) -> Vec<String> {
    let Some(table) = config_toml.as_table() else {
        return Vec::new();
    };

    let mut advice = Vec::new();
    if let Some(value) = table.get("selected_profile").and_then(toml::Value::as_str) {
        push_legacy_profile_value_advice(&mut advice, "selected_profile", value);
    }

    push_legacy_profile_automation_key_advice(&mut advice, table);

    advice.sort_unstable();
    advice.dedup();
    advice
}

/// Adds profile-rename advice for legacy profile automation preset keys.
pub(super) fn push_legacy_profile_automation_key_advice(
    advice: &mut Vec<String>,
    table: &toml::map::Map<String, toml::Value>,
) {
    let Some(profile_automation) = table
        .get("profile_automation")
        .and_then(toml::Value::as_table)
    else {
        return;
    };

    for (legacy_key, canonical_key) in [
        ("classic", "basic"),
        ("deep_work", "standard"),
        ("deep-work", "standard"),
        ("custom", "advanced"),
    ] {
        if profile_automation.contains_key(legacy_key) {
            advice.push(format!(
                "[profile_automation.{legacy_key}] should be renamed to [profile_automation.{canonical_key}]."
            ));
        }
    }
}

/// Adds one profile-rename advice message when a legacy token is recognized.
pub(super) fn push_legacy_profile_value_advice(
    advice: &mut Vec<String>,
    location: &str,
    value: &str,
) {
    let Some(target) = legacy_profile_token_migration_target(value) else {
        return;
    };
    advice.push(format!(
        "{location} uses legacy value \"{value}\"; migrate it to \"{target}\"."
    ));
}

/// Detects deprecated config surfaces that should be replaced by current workflows.
pub(super) fn detect_legacy_config_deprecation_warnings(config: &AppConfig) -> Vec<String> {
    let mut warnings = Vec::new();
    let duration_override_without_custom_profile = config.custom_profile.is_none()
        && (config.focus_secs != default_focus_secs()
            || config.short_break_secs != default_short_break_secs()
            || config.long_break_secs != default_long_break_secs()
            || config.long_break_interval != default_long_break_interval());
    if duration_override_without_custom_profile {
        warnings.push(
            "Deprecated top-level timer fields (`focus_secs`, `short_break_secs`, `long_break_secs`, `long_break_interval`) are in use. Move these values into `[custom_profile]`.".to_string(),
        );
    }

    let legacy_automation_values_configured = config.notifications != NotificationConfig::default()
        || config.auto_start != AutoStartConfig::default()
        || config.strict_mode
        || config.recurring_schedule != RecurringScheduleConfig::default();
    let profile_automation_incomplete = config
        .profile_automation
        .as_ref()
        .map(|settings| {
            settings.basic.is_none() || settings.standard.is_none() || settings.advanced.is_none()
        })
        .unwrap_or(true);
    let legacy_automation_in_use =
        legacy_automation_values_configured && profile_automation_incomplete;
    if legacy_automation_in_use {
        warnings.push(
            "Deprecated top-level automation fields (`notifications`, `auto_start`, `strict_mode`, `recurring_schedule`) are in use. Move them under `[profile_automation.<preset>]`.".to_string(),
        );
    }

    if config.blocklist_profiles.is_empty() && !config.blocked_sites.is_empty() {
        warnings.push(
            "Deprecated `blocked_sites` is in use without `[[blocklist_profiles]]`. Move entries into a blocklist profile (for example `Default`).".to_string(),
        );
    }

    warnings
}

/// Reads the raw config schema version, defaulting absent legacy configs to v0.
pub(super) fn detect_config_schema_version(config_toml: &toml::Value) -> Option<u32> {
    let table = config_toml.as_table()?;
    table
        .get("schema_version")
        .map(|value| value.as_integer().and_then(|raw| u32::try_from(raw).ok()))
        .unwrap_or(Some(LEGACY_CONFIG_SCHEMA_VERSION))
}

/// Applies one schema-version migration step to raw TOML config data.
pub(super) fn migrate_config_toml_step(
    config_toml: toml::Value,
    from_schema_version: u32,
) -> Option<toml::Value> {
    match from_schema_version {
        LEGACY_CONFIG_SCHEMA_VERSION => migrate_config_toml_legacy_to_v1(config_toml),
        1 => migrate_config_toml_v1_to_v2(config_toml),
        _ => None,
    }
}

/// Adds the first explicit schema marker to legacy TOML config data.
pub(super) fn migrate_config_toml_legacy_to_v1(
    mut config_toml: toml::Value,
) -> Option<toml::Value> {
    let table = config_toml.as_table_mut()?;
    table.insert("schema_version".to_string(), toml::Value::Integer(1));
    Some(config_toml)
}

/// Canonicalizes profile references and advances v1 TOML config data to v2.
pub(super) fn migrate_config_toml_v1_to_v2(mut config_toml: toml::Value) -> Option<toml::Value> {
    let table = config_toml.as_table_mut()?;
    migrate_profile_value_in_table(table, "selected_profile");
    migrate_profile_automation_preset_keys(table);
    table.insert(
        "schema_version".to_string(),
        toml::Value::Integer(i64::from(CURRENT_CONFIG_SCHEMA_VERSION)),
    );
    Some(config_toml)
}

/// Canonicalizes one profile value stored directly in a TOML table.
pub(super) fn migrate_profile_value_in_table(
    table: &mut toml::map::Map<String, toml::Value>,
    field_key: &str,
) {
    let Some(value) = table.get_mut(field_key) else {
        return;
    };
    let Some(raw) = value.as_str() else {
        return;
    };
    let Some(mapped) = canonical_profile_id_token(raw) else {
        return;
    };
    *value = toml::Value::String(mapped.to_string());
}

/// Canonicalizes legacy keys under the profile automation preset table.
pub(super) fn migrate_profile_automation_preset_keys(
    table: &mut toml::map::Map<String, toml::Value>,
) {
    let Some(profile_automation) = table
        .get_mut("profile_automation")
        .and_then(|value| value.as_table_mut())
    else {
        return;
    };
    migrate_table_key(profile_automation, "classic", "basic");
    migrate_table_key(profile_automation, "deep_work", "standard");
    migrate_table_key(profile_automation, "deep-work", "standard");
    migrate_table_key(profile_automation, "custom", "advanced");
}

/// Renames a TOML table key while preserving an existing canonical destination.
pub(super) fn migrate_table_key(
    table: &mut toml::map::Map<String, toml::Value>,
    old_key: &str,
    new_key: &str,
) {
    let Some(value) = table.remove(old_key) else {
        return;
    };
    if let Some(existing) = table.get_mut(new_key) {
        merge_toml_value_prefer_existing(existing, value);
        return;
    }
    table.insert(new_key.to_string(), value);
}

/// Merges incoming TOML data without overwriting existing destination values.
pub(super) fn merge_toml_value_prefer_existing(existing: &mut toml::Value, incoming: toml::Value) {
    let (toml::Value::Table(existing_table), toml::Value::Table(incoming_table)) =
        (existing, incoming)
    else {
        return;
    };

    for (key, incoming_value) in incoming_table {
        if let Some(existing_value) = existing_table.get_mut(&key) {
            merge_toml_value_prefer_existing(existing_value, incoming_value);
        } else {
            existing_table.insert(key, incoming_value);
        }
    }
}
