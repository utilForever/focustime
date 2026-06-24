use std::{
    fs, io,
    path::{Path, PathBuf},
};

use super::{
    AppConfig, AppConfigDisk, CURRENT_CONFIG_SCHEMA_VERSION, ConfigDoctorReport,
    ConfigMigrationReport, collect_legacy_profile_rename_advice, config_health_error,
    config_health_warning, detect_legacy_config_deprecation_warnings,
    migrate_config_toml_to_current_detailed, sort_config_health_findings, summarize_config_health,
};

fn next_config_backup_path(path: &Path) -> PathBuf {
    let mut index: usize = 0;
    loop {
        let candidate = if index == 0 {
            path.with_extension("toml.bak")
        } else {
            path.with_extension(format!("toml.bak.{index}"))
        };
        if !candidate.exists() {
            return candidate;
        }
        index += 1;
    }
}

fn write_atomic_text_file(path: &Path, content: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, content)?;
    #[cfg(target_os = "windows")]
    {
        match fs::rename(&tmp, path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                fs::remove_file(path)?;
                fs::rename(&tmp, path)
            }
            Err(error) => Err(error),
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        fs::rename(&tmp, path)
    }
}

fn write_migrated_config_with_backup(
    path: &Path,
    migrated_toml: &toml::Value,
) -> Result<PathBuf, String> {
    let backup_path = next_config_backup_path(path);
    fs::copy(path, &backup_path).map_err(|error| {
        format!(
            "Failed to create config backup at `{}`: {error}",
            backup_path.display()
        )
    })?;
    let content = toml::to_string_pretty(migrated_toml)
        .map_err(|error| format!("Failed to serialize migrated config TOML: {error}"))?;
    write_atomic_text_file(path, &content).map_err(|error| {
        format!(
            "Failed to write migrated config to `{}`: {error}",
            path.display()
        )
    })?;
    Ok(backup_path)
}

pub(crate) fn run_config_doctor() -> ConfigDoctorReport {
    run_config_doctor_with_path(AppConfig::config_path())
}

pub(super) fn run_config_doctor_with_path(config_path: Option<PathBuf>) -> ConfigDoctorReport {
    let action = "config-health";
    let mut detected_schema_version = None;
    let mut migration_steps = Vec::new();
    let mut findings = Vec::new();

    let Some(path) = config_path.clone() else {
        findings.push(config_health_error(
            "config.path_unavailable",
            "Could not determine the config.toml path from the current environment.",
            "Set the platform config directory environment variables (for example APPDATA on Windows, XDG_CONFIG_HOME/HOME on Unix), then retry.",
        ));
        return ConfigDoctorReport {
            action,
            config_path,
            detected_schema_version,
            current_schema_version: CURRENT_CONFIG_SCHEMA_VERSION,
            status: summarize_config_health(&findings),
            migration_steps,
            findings,
        };
    };

    if !path.exists() {
        findings.push(config_health_warning(
            "config.file_missing",
            format!("Config file `{}` does not exist.", path.display()),
            "Run focustime once (or create config.toml), then rerun the doctor.",
        ));
        return ConfigDoctorReport {
            action,
            config_path: Some(path),
            detected_schema_version,
            current_schema_version: CURRENT_CONFIG_SCHEMA_VERSION,
            status: summarize_config_health(&findings),
            migration_steps,
            findings,
        };
    }

    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) => {
            findings.push(config_health_error(
                "config.read_failed",
                format!("Failed to read `{}`: {error}", path.display()),
                "Check filesystem permissions and path accessibility, then rerun the doctor.",
            ));
            return ConfigDoctorReport {
                action,
                config_path: Some(path),
                detected_schema_version,
                current_schema_version: CURRENT_CONFIG_SCHEMA_VERSION,
                status: summarize_config_health(&findings),
                migration_steps,
                findings,
            };
        }
    };

    let original_toml: toml::Value = match toml::from_str(&content) {
        Ok(value) => value,
        Err(error) => {
            findings.push(config_health_error(
                "config.toml_parse_error",
                format!("config.toml is not valid TOML: {error}"),
                "Fix the TOML syntax error and rerun the doctor.",
            ));
            return ConfigDoctorReport {
                action,
                config_path: Some(path),
                detected_schema_version,
                current_schema_version: CURRENT_CONFIG_SCHEMA_VERSION,
                status: summarize_config_health(&findings),
                migration_steps,
                findings,
            };
        }
    };

    let rename_advice = collect_legacy_profile_rename_advice(&original_toml);
    let (migrated_toml, schema_version, steps) =
        match migrate_config_toml_to_current_detailed(original_toml.clone()) {
            Ok(result) => result,
            Err(error) => {
                findings.push(config_health_error(
                    "config.migration_failed",
                    format!("Config migration analysis failed: {error}"),
                    "Ensure schema_version is valid and the file is structurally correct TOML.",
                ));
                return ConfigDoctorReport {
                    action,
                    config_path: Some(path),
                    detected_schema_version,
                    current_schema_version: CURRENT_CONFIG_SCHEMA_VERSION,
                    status: summarize_config_health(&findings),
                    migration_steps,
                    findings,
                };
            }
        };
    detected_schema_version = Some(schema_version);
    migration_steps = steps;

    if schema_version > CURRENT_CONFIG_SCHEMA_VERSION {
        findings.push(config_health_warning(
            "config.schema_newer_than_supported",
            format!(
                "Config schema version {schema_version} is newer than this build's supported schema version {}.",
                CURRENT_CONFIG_SCHEMA_VERSION
            ),
            "Use a focustime build that supports this schema before writing config changes.",
        ));
        sort_config_health_findings(&mut findings);
        return ConfigDoctorReport {
            action,
            config_path: Some(path),
            detected_schema_version,
            current_schema_version: CURRENT_CONFIG_SCHEMA_VERSION,
            status: summarize_config_health(&findings),
            migration_steps,
            findings,
        };
    } else if schema_version < CURRENT_CONFIG_SCHEMA_VERSION {
        findings.push(config_health_warning(
            "config.schema_outdated",
            format!(
                "Config schema version {schema_version} is older than current schema version {}.",
                CURRENT_CONFIG_SCHEMA_VERSION
            ),
            "Run `focustime --diagnostics` to review migration guidance, then update config.toml to the current schema.",
        ));
    }

    for advice in rename_advice {
        findings.push(config_health_warning(
            "config.legacy_profile_token",
            advice,
            "Run `focustime --diagnostics` to review canonical profile-key migration guidance.",
        ));
    }
    let disk: AppConfigDisk = match migrated_toml.try_into() {
        Ok(disk) => disk,
        Err(error) => {
            findings.push(config_health_error(
                "config.deserialize_failed",
                format!("Config could not be deserialized after migration steps: {error}"),
                "Fix incompatible field types/structures in config.toml, then rerun the doctor.",
            ));
            return ConfigDoctorReport {
                action,
                config_path: Some(path),
                detected_schema_version,
                current_schema_version: CURRENT_CONFIG_SCHEMA_VERSION,
                status: summarize_config_health(&findings),
                migration_steps,
                findings,
            };
        }
    };

    let config = disk.config;
    for warning in detect_legacy_config_deprecation_warnings(&config) {
        findings.push(config_health_warning(
            "config.deprecated_field_in_use",
            warning,
            "Update config.toml to canonical fields and rerun the doctor.",
        ));
    }
    let _normalized = config.normalize();

    sort_config_health_findings(&mut findings);
    ConfigDoctorReport {
        action,
        config_path: Some(path),
        detected_schema_version,
        current_schema_version: CURRENT_CONFIG_SCHEMA_VERSION,
        status: summarize_config_health(&findings),
        migration_steps,
        findings,
    }
}

pub(crate) fn run_config_migration_assistant(apply: bool) -> ConfigMigrationReport {
    run_config_migration_assistant_with_path(apply, AppConfig::config_path())
}

pub(super) fn run_config_migration_assistant_with_path(
    apply: bool,
    config_path: Option<PathBuf>,
) -> ConfigMigrationReport {
    let action = if apply {
        "config-migration-apply"
    } else {
        "config-migration-guidance"
    };
    let mut detected_schema_version = None;
    let mut backup_path = None;
    let mut steps = Vec::new();
    let mut changed = false;
    let mut applied = false;
    let mut findings = Vec::new();

    let Some(path) = config_path.clone() else {
        findings.push(config_health_error(
            "config.path_unavailable",
            "Could not determine the config.toml path from the current environment.",
            "Set the platform config directory environment variables (for example APPDATA on Windows, XDG_CONFIG_HOME/HOME on Unix), then retry.",
        ));
        return ConfigMigrationReport {
            action,
            applied,
            config_path,
            backup_path,
            detected_schema_version,
            target_schema_version: CURRENT_CONFIG_SCHEMA_VERSION,
            changed,
            status: summarize_config_health(&findings),
            steps,
            findings,
        };
    };

    if !path.exists() {
        findings.push(config_health_warning(
            "config.file_missing",
            format!("Config file `{}` does not exist.", path.display()),
            "Run focustime once (or create config.toml), then rerun diagnostics.",
        ));
        return ConfigMigrationReport {
            action,
            applied,
            config_path: Some(path),
            backup_path,
            detected_schema_version,
            target_schema_version: CURRENT_CONFIG_SCHEMA_VERSION,
            changed,
            status: summarize_config_health(&findings),
            steps,
            findings,
        };
    }

    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) => {
            findings.push(config_health_error(
                "config.read_failed",
                format!("Failed to read `{}`: {error}", path.display()),
                "Check filesystem permissions and path accessibility, then rerun diagnostics.",
            ));
            return ConfigMigrationReport {
                action,
                applied,
                config_path: Some(path),
                backup_path,
                detected_schema_version,
                target_schema_version: CURRENT_CONFIG_SCHEMA_VERSION,
                changed,
                status: summarize_config_health(&findings),
                steps,
                findings,
            };
        }
    };

    let original_toml: toml::Value = match toml::from_str(&content) {
        Ok(value) => value,
        Err(error) => {
            findings.push(config_health_error(
                "config.toml_parse_error",
                format!("config.toml is not valid TOML: {error}"),
                "Fix the TOML syntax error and rerun diagnostics.",
            ));
            return ConfigMigrationReport {
                action,
                applied,
                config_path: Some(path),
                backup_path,
                detected_schema_version,
                target_schema_version: CURRENT_CONFIG_SCHEMA_VERSION,
                changed,
                status: summarize_config_health(&findings),
                steps,
                findings,
            };
        }
    };

    for advice in collect_legacy_profile_rename_advice(&original_toml) {
        findings.push(config_health_warning(
            "config.legacy_profile_token",
            advice,
            "Use canonical profile tokens (`basic`, `standard`, `advanced`).",
        ));
    }
    let (migrated_toml, schema_version, migration_steps) =
        match migrate_config_toml_to_current_detailed(original_toml.clone()) {
            Ok(result) => result,
            Err(error) => {
                findings.push(config_health_error(
                    "config.migration_failed",
                    format!("Migration analysis failed: {error}"),
                    "Ensure schema_version is valid and the file is structurally correct TOML.",
                ));
                return ConfigMigrationReport {
                    action,
                    applied,
                    config_path: Some(path),
                    backup_path,
                    detected_schema_version,
                    target_schema_version: CURRENT_CONFIG_SCHEMA_VERSION,
                    changed,
                    status: summarize_config_health(&findings),
                    steps,
                    findings,
                };
            }
        };
    detected_schema_version = Some(schema_version);
    steps = migration_steps;
    changed = original_toml != migrated_toml;

    if schema_version > CURRENT_CONFIG_SCHEMA_VERSION {
        findings.push(config_health_warning(
            "config.schema_newer_than_supported",
            format!(
                "Config schema version {schema_version} is newer than this build's supported schema version {}.",
                CURRENT_CONFIG_SCHEMA_VERSION
            ),
            "Use a focustime build that supports this schema before applying migration.",
        ));
    } else if !changed {
        findings.push(config_health_warning(
            "config.already_current",
            "Config already matches the current migration schema and canonical key mapping."
                .to_string(),
            "No migration apply step is required.",
        ));
    }

    if apply && changed && schema_version <= CURRENT_CONFIG_SCHEMA_VERSION {
        match write_migrated_config_with_backup(&path, &migrated_toml) {
            Ok(created_backup) => {
                backup_path = Some(created_backup);
                applied = true;
            }
            Err(error) => {
                findings.push(config_health_error(
                    "config.apply_failed",
                    error,
                    "Fix the file system error, restore from backup if needed, and retry migration apply.",
                ));
            }
        }
    }

    sort_config_health_findings(&mut findings);
    ConfigMigrationReport {
        action,
        applied,
        config_path: Some(path),
        backup_path,
        detected_schema_version,
        target_schema_version: CURRENT_CONFIG_SCHEMA_VERSION,
        changed,
        status: summarize_config_health(&findings),
        steps,
        findings,
    }
}
