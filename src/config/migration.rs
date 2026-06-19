use super::{
    AppConfig, AutoStartConfig, CURRENT_CONFIG_SCHEMA_VERSION, ConfigHealthFinding,
    ConfigHealthSeverity, ConfigHealthStatus, ConfigMigrationStepReport,
    LEGACY_CONFIG_SCHEMA_VERSION, NotificationConfig, RecurringScheduleConfig,
    WEEKDAY_PROFILE_RULE_REPLACEMENT_AT, canonical_profile_id_token, default_focus_secs,
    default_long_break_interval, default_long_break_secs, default_short_break_secs,
};

pub(super) fn migrate_config_toml_to_current(config_toml: toml::Value) -> Option<toml::Value> {
    migrate_config_toml_to_current_detailed(config_toml)
        .ok()
        .map(|(migrated, _, _)| migrated)
}

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
    migrate_weekday_profile_rules_to_automation_triggers(&mut config_toml);
    if weekday_rules_input != config_toml {
        steps.push(ConfigMigrationStepReport {
            from_schema_version,
            to_schema_version: from_schema_version,
            summary: "Move deprecated weekday profile rules into automation time triggers."
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

pub(super) fn canonicalize_legacy_profile_aliases(config_toml: &mut toml::Value) {
    let Some(table) = config_toml.as_table_mut() else {
        return;
    };
    migrate_profile_value_in_table(table, "selected_profile");
    migrate_profile_automation_preset_keys(table);
    migrate_profile_value_in_array_table(table, "session_templates", "profile");
    migrate_profile_value_in_array_table(table, "weekday_profile_rules", "profile");
    migrate_automation_trigger_action_profiles(table);
}

pub(super) fn migrate_weekday_profile_rules_to_automation_triggers(config_toml: &mut toml::Value) {
    let Some(table) = config_toml.as_table_mut() else {
        return;
    };
    let Some(weekday_rules) = table
        .get("weekday_profile_rules")
        .and_then(|value| value.as_array().cloned())
    else {
        return;
    };
    if table
        .get("automation_triggers")
        .is_some_and(|value| !value.is_array())
    {
        return;
    }
    table.remove("weekday_profile_rules");
    if weekday_rules.is_empty() {
        return;
    }

    let triggers = table
        .entry("automation_triggers")
        .or_insert_with(|| toml::Value::Array(Vec::new()));
    let Some(triggers) = triggers.as_array_mut() else {
        return;
    };
    triggers.retain(|trigger| !is_weekday_profile_replacement_trigger_value(trigger));
    for rule in weekday_rules {
        let Some(trigger) = weekday_rule_value_to_automation_trigger(rule) else {
            continue;
        };
        triggers.push(trigger);
    }
}

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

fn weekday_rule_value_to_automation_trigger(rule: toml::Value) -> Option<toml::Value> {
    let rule = rule.as_table()?;
    let day = rule
        .get("day")
        .and_then(toml::Value::as_str)
        .unwrap_or("mon");
    let profile = rule
        .get("profile")
        .and_then(toml::Value::as_str)
        .and_then(canonical_profile_id_token)
        .unwrap_or("advanced");
    let blocklist_profile = rule
        .get("blocklist_profile")
        .and_then(toml::Value::as_str)
        .unwrap_or("Default");

    let mut trigger = toml::map::Map::new();
    trigger.insert("type".to_string(), toml::Value::String("time".to_string()));
    trigger.insert(
        "days".to_string(),
        toml::Value::Array(vec![toml::Value::String(day.to_string())]),
    );
    trigger.insert(
        "at".to_string(),
        toml::Value::String(WEEKDAY_PROFILE_RULE_REPLACEMENT_AT.to_string()),
    );

    let mut action = toml::map::Map::new();
    action.insert(
        "type".to_string(),
        toml::Value::String("apply_defaults".to_string()),
    );
    action.insert(
        "profile".to_string(),
        toml::Value::String(profile.to_string()),
    );
    action.insert(
        "blocklist_profile".to_string(),
        toml::Value::String(blocklist_profile.to_string()),
    );
    if let Some(template) = rule.get("session_template").and_then(toml::Value::as_str)
        && !template.trim().is_empty()
    {
        action.insert(
            "session_template".to_string(),
            toml::Value::String(template.to_string()),
        );
    }

    let mut entry = toml::map::Map::new();
    entry.insert("trigger".to_string(), toml::Value::Table(trigger));
    entry.insert("action".to_string(), toml::Value::Table(action));
    Some(toml::Value::Table(entry))
}

fn is_weekday_profile_replacement_trigger_value(trigger: &toml::Value) -> bool {
    let Some(trigger) = trigger.as_table() else {
        return false;
    };
    let Some(condition) = trigger.get("trigger").and_then(toml::Value::as_table) else {
        return false;
    };
    let Some(action) = trigger.get("action").and_then(toml::Value::as_table) else {
        return false;
    };
    condition
        .get("type")
        .and_then(toml::Value::as_str)
        .is_some_and(|value| value == "time")
        && condition
            .get("at")
            .and_then(toml::Value::as_str)
            .is_some_and(|value| value == WEEKDAY_PROFILE_RULE_REPLACEMENT_AT)
        && action
            .get("type")
            .and_then(toml::Value::as_str)
            .is_some_and(|value| value == "apply_defaults")
}

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

pub(super) fn sort_config_health_findings(findings: &mut [ConfigHealthFinding]) {
    findings.sort_by(|left, right| {
        left.code
            .cmp(&right.code)
            .then_with(|| left.message.cmp(&right.message))
    });
}

pub(super) fn legacy_profile_token_migration_target(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "classic" => Some("basic"),
        "deep-work" | "deep_work" | "deepwork" => Some("standard"),
        "custom" => Some("advanced"),
        _ => None,
    }
}

pub(super) fn collect_legacy_profile_rename_advice(config_toml: &toml::Value) -> Vec<String> {
    let Some(table) = config_toml.as_table() else {
        return Vec::new();
    };

    let mut advice = Vec::new();
    if let Some(value) = table.get("selected_profile").and_then(toml::Value::as_str) {
        push_legacy_profile_value_advice(&mut advice, "selected_profile", value);
    }

    push_legacy_profile_value_array_advice(
        &mut advice,
        table,
        "session_templates",
        "profile",
        "session_templates",
    );
    push_legacy_profile_value_array_advice(
        &mut advice,
        table,
        "weekday_profile_rules",
        "profile",
        "weekday_profile_rules",
    );
    push_legacy_automation_trigger_profile_advice(&mut advice, table);
    push_legacy_profile_automation_key_advice(&mut advice, table);

    advice.sort_unstable();
    advice.dedup();
    advice
}

pub(super) fn collect_blocklist_category_migration_advice(
    config_toml: &toml::Value,
) -> Vec<String> {
    let Some(table) = config_toml.as_table() else {
        return Vec::new();
    };
    let Some(profiles) = table
        .get("blocklist_profiles")
        .and_then(toml::Value::as_array)
    else {
        return Vec::new();
    };

    let mut advice = Vec::new();
    for (index, profile) in profiles.iter().enumerate() {
        let Some(profile) = profile.as_table() else {
            continue;
        };
        if profile.contains_key("categories") || profile.contains_key("selected_category") {
            advice.push(format!(
                "blocklist_profiles[{index}] uses deprecated category config; run migration to fold category `sites` and `allowlist_sites` into profile-level lists."
            ));
        }
    }
    advice
}

pub(super) fn push_legacy_profile_value_array_advice(
    advice: &mut Vec<String>,
    table: &toml::map::Map<String, toml::Value>,
    array_key: &str,
    field_key: &str,
    location_prefix: &str,
) {
    let Some(array) = table.get(array_key).and_then(toml::Value::as_array) else {
        return;
    };
    for (index, entry) in array.iter().enumerate() {
        let Some(entry_table) = entry.as_table() else {
            continue;
        };
        let Some(value) = entry_table.get(field_key).and_then(toml::Value::as_str) else {
            continue;
        };
        push_legacy_profile_value_advice(
            advice,
            &format!("{location_prefix}[{index}].{field_key}"),
            value,
        );
    }
}

pub(super) fn push_legacy_automation_trigger_profile_advice(
    advice: &mut Vec<String>,
    table: &toml::map::Map<String, toml::Value>,
) {
    let Some(array) = table
        .get("automation_triggers")
        .and_then(toml::Value::as_array)
    else {
        return;
    };
    for (index, entry) in array.iter().enumerate() {
        let Some(entry_table) = entry.as_table() else {
            continue;
        };
        let Some(action_table) = entry_table.get("action").and_then(toml::Value::as_table) else {
            continue;
        };
        let Some(value) = action_table.get("profile").and_then(toml::Value::as_str) else {
            continue;
        };
        push_legacy_profile_value_advice(
            advice,
            &format!("automation_triggers[{index}].action.profile"),
            value,
        );
    }
}

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

    if !config.weekday_profile_rules.is_empty() {
        warnings.push(
            "Deprecated `weekday_profile_rules` is in use. Move weekday defaults to profile schedules and session templates.".to_string(),
        );
    }

    if !config.automation_triggers.is_empty() {
        warnings.push(
            "Deprecated `automation_triggers` is in use. Use profile schedules for automatic focus starts, `--schedule-delay` for postponing active schedule windows, and session templates for task/profile/blocklist defaults.".to_string(),
        );
    }

    if config.calendar_sync.enabled {
        warnings.push(
            "Deprecated standalone calendar sync behavior is enabled. Calendar data is now supported only as an opt-in schedule annotation cache; keep `[calendar_sync]` disabled or absent for deterministic schedule behavior without calendar data.".to_string(),
        );
    }

    warnings
}

pub(super) fn detect_config_schema_version(config_toml: &toml::Value) -> Option<u32> {
    let table = config_toml.as_table()?;
    table
        .get("schema_version")
        .map(|value| value.as_integer().and_then(|raw| u32::try_from(raw).ok()))
        .unwrap_or(Some(LEGACY_CONFIG_SCHEMA_VERSION))
}

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

pub(super) fn migrate_config_toml_legacy_to_v1(
    mut config_toml: toml::Value,
) -> Option<toml::Value> {
    let table = config_toml.as_table_mut()?;
    table.insert("schema_version".to_string(), toml::Value::Integer(1));
    Some(config_toml)
}

pub(super) fn migrate_config_toml_v1_to_v2(mut config_toml: toml::Value) -> Option<toml::Value> {
    let table = config_toml.as_table_mut()?;
    migrate_profile_value_in_table(table, "selected_profile");
    migrate_profile_automation_preset_keys(table);
    migrate_profile_value_in_array_table(table, "session_templates", "profile");
    migrate_profile_value_in_array_table(table, "weekday_profile_rules", "profile");
    migrate_automation_trigger_action_profiles(table);
    table.insert(
        "schema_version".to_string(),
        toml::Value::Integer(i64::from(CURRENT_CONFIG_SCHEMA_VERSION)),
    );
    Some(config_toml)
}

pub(super) fn migrate_profile_value_in_array_table(
    table: &mut toml::map::Map<String, toml::Value>,
    array_key: &str,
    field_key: &str,
) {
    let Some(array) = table
        .get_mut(array_key)
        .and_then(|value| value.as_array_mut())
    else {
        return;
    };
    for entry in array {
        let Some(entry_table) = entry.as_table_mut() else {
            continue;
        };
        migrate_profile_value_in_table(entry_table, field_key);
    }
}

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

pub(super) fn migrate_automation_trigger_action_profiles(
    table: &mut toml::map::Map<String, toml::Value>,
) {
    let Some(triggers) = table
        .get_mut("automation_triggers")
        .and_then(|value| value.as_array_mut())
    else {
        return;
    };
    for entry in triggers {
        let Some(entry_table) = entry.as_table_mut() else {
            continue;
        };
        let Some(action_table) = entry_table
            .get_mut("action")
            .and_then(|action| action.as_table_mut())
        else {
            continue;
        };
        migrate_profile_value_in_table(action_table, "profile");
    }
}
