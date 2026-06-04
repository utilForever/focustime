use super::{
    AppConfig, AutoStartConfig, CURRENT_CONFIG_SCHEMA_VERSION, ConfigHealthFinding,
    ConfigHealthSeverity, ConfigHealthStatus, ConfigMigrationStepReport,
    LEGACY_CONFIG_SCHEMA_VERSION, NotificationConfig, RecurringScheduleConfig,
    canonical_profile_id_token, default_focus_secs, default_long_break_interval,
    default_long_break_secs, default_short_break_secs,
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
