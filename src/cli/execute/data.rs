use std::{env, fs, path::Path};

use crate::cli::{
    AppConfig, BackupOutput, DailyGoalSnapshot, ExportOutput, FocusStats, OutputMode, PathBuf,
    RestoreOutput, print_backup_output, print_export_output, print_json, print_restore_output,
};

const CONFIG_FILE_NAME: &str = "config.toml";
const STATS_FILE_NAME: &str = "stats.toml";

#[derive(Clone, Copy)]
struct ArtifactDirectoryWorkflow {
    name: &'static str,
    role: &'static str,
    create: bool,
}

const EXPORT_ARTIFACTS: ArtifactDirectoryWorkflow = ArtifactDirectoryWorkflow {
    name: "Export",
    role: "target",
    create: true,
};
const BACKUP_ARTIFACTS: ArtifactDirectoryWorkflow = ArtifactDirectoryWorkflow {
    name: "Backup",
    role: "target",
    create: true,
};
const RESTORE_ARTIFACTS: ArtifactDirectoryWorkflow = ArtifactDirectoryWorkflow {
    name: "Restore",
    role: "source",
    create: false,
};

pub(super) fn execute_export_command(
    dir: Option<PathBuf>,
    output: OutputMode,
) -> Result<(), String> {
    let config = AppConfig::load().normalized();
    let stats = FocusStats::load_with_options(stats_load_options(&config))
        .map_err(|error| format!("Failed to load stats: {error}"))?;
    let history_kpi_context = build_history_kpi_export_context(&config);
    let target_dir = resolve_artifact_directory(dir, EXPORT_ARTIFACTS)?;
    let exported = stats
        .export_to_dir_with_context(&target_dir, &history_kpi_context)
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

fn build_history_kpi_export_context(config: &AppConfig) -> crate::stats::HistoryKpiExportContext {
    let selected_automation = config.profile_automation_for(config.selected_profile);
    crate::stats::HistoryKpiExportContext {
        reference_day: chrono::Local::now().date_naive(),
        daily_goal: DailyGoalSnapshot {
            minutes: config.daily_goal.minutes,
            pomodoros: config.daily_goal.pomodoros,
        },
        weekly_goal: DailyGoalSnapshot {
            minutes: config.weekly_goal.minutes,
            pomodoros: config.weekly_goal.pomodoros,
        },
        monthly_goal: DailyGoalSnapshot {
            minutes: config.monthly_goal.minutes,
            pomodoros: config.monthly_goal.pomodoros,
        },
        recurring_schedule: selected_automation.recurring_schedule,
        stats_retention: config.stats_retention,
        comparison_dimension: crate::stats::ComparisonDimension::TaskLabel,
        comparison_task_filter: None,
        comparison_profile_filter: None,
        comparison_time_of_day_filter: None,
    }
}

pub(super) fn execute_backup_command(
    dir: Option<PathBuf>,
    output: OutputMode,
) -> Result<(), String> {
    let _config = AppConfig::load().normalized();
    let backup_dir = resolve_artifact_directory(dir, BACKUP_ARTIFACTS)?;

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

pub(super) fn execute_restore_command(
    dir: Option<PathBuf>,
    output: OutputMode,
) -> Result<(), String> {
    let _config = AppConfig::load().normalized();
    let restore_dir = resolve_artifact_directory(dir, RESTORE_ARTIFACTS)?;
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

fn resolve_artifact_directory(
    dir: Option<PathBuf>,
    workflow: ArtifactDirectoryWorkflow,
) -> Result<PathBuf, String> {
    let directory = match dir {
        Some(path) => path,
        None => env::current_dir().map_err(|error| {
            format!(
                "{} failed: could not determine current directory: {error}",
                workflow.name
            )
        })?,
    };

    if workflow.create {
        fs::create_dir_all(&directory).map_err(|error| {
            format!(
                "{} failed: could not create {} directory `{}`: {error}",
                workflow.name,
                workflow.role,
                directory.display()
            )
        })?;
    }

    Ok(directory)
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

pub(super) fn stats_load_options(_config: &AppConfig) -> crate::stats::StatsLoadOptions {
    crate::stats::StatsLoadOptions::default()
}

pub(super) fn stats_save_options(_config: &AppConfig) -> crate::stats::StatsSaveOptions {
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

#[cfg(test)]
mod tests {
    use super::{BACKUP_ARTIFACTS, RESTORE_ARTIFACTS, resolve_artifact_directory};
    use crate::cli::PathBuf;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn artifact_producer_creates_target_directory() {
        let root = unique_temp_dir("artifact-create");
        let target = root.join("nested").join("reports");

        let resolved = resolve_artifact_directory(Some(target.clone()), BACKUP_ARTIFACTS)
            .expect("target artifact directory should be created");

        assert_eq!(resolved, target);
        assert!(resolved.is_dir());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn artifact_consumer_does_not_create_source_directory() {
        let root = unique_temp_dir("artifact-source");
        let source = root.join("missing-backup");

        let resolved = resolve_artifact_directory(Some(source.clone()), RESTORE_ARTIFACTS)
            .expect("source artifact directory should be passed through");

        assert_eq!(resolved, source);
        assert!(!resolved.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn artifact_producer_reports_consistent_directory_creation_error() {
        let root = unique_temp_dir("artifact-file-target");
        let target = root.join("occupied");
        fs::create_dir_all(&root).expect("failed to create temp root");
        fs::write(&target, "not a directory").expect("failed to write occupied file");

        let error = resolve_artifact_directory(Some(target), BACKUP_ARTIFACTS).unwrap_err();

        assert!(error.contains("Backup failed: could not create target directory"));
        let _ = fs::remove_dir_all(root);
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        let unique = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "focustime-{label}-{}-{now}-{unique}",
            std::process::id()
        ))
    }
}
