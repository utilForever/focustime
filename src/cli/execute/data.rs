use std::{env, fs};

use crate::cli::{
    AppConfig, DailyGoalSnapshot, ExportOutput, FocusStats, OutputMode, PathBuf,
    print_export_output, print_json,
};

pub(super) fn execute_export_command(
    dir: Option<PathBuf>,
    output: OutputMode,
) -> Result<(), String> {
    let config = AppConfig::load().normalized();
    let stats = FocusStats::load_with_options(stats_load_options(&config))
        .map_err(|error| format!("Failed to load stats: {error}"))?;
    let history_kpi_context = build_history_kpi_export_context(&config);
    let target_dir = resolve_export_directory(dir)?;
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

fn resolve_export_directory(dir: Option<PathBuf>) -> Result<PathBuf, String> {
    let directory = match dir {
        Some(path) => path,
        None => env::current_dir().map_err(|error| {
            format!("Export failed: could not determine current directory: {error}")
        })?,
    };

    fs::create_dir_all(&directory).map_err(|error| {
        format!(
            "Export failed: could not create target directory `{}`: {error}",
            directory.display()
        )
    })?;

    Ok(directory)
}

pub(super) fn stats_load_options(_config: &AppConfig) -> crate::stats::StatsLoadOptions {
    crate::stats::StatsLoadOptions::default()
}

pub(super) fn stats_save_options(_config: &AppConfig) -> crate::stats::StatsSaveOptions {
    crate::stats::StatsSaveOptions::default()
}

#[cfg(test)]
mod tests {
    use super::resolve_export_directory;
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

        let resolved = resolve_export_directory(Some(target.clone()))
            .expect("export target directory should be created");

        assert_eq!(resolved, target);
        assert!(resolved.is_dir());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn artifact_producer_reports_consistent_directory_creation_error() {
        let root = unique_temp_dir("artifact-file-target");
        let target = root.join("occupied");
        fs::create_dir_all(&root).expect("failed to create temp root");
        fs::write(&target, "not a directory").expect("failed to write occupied file");

        let error = resolve_export_directory(Some(target)).unwrap_err();

        assert!(error.contains("Export failed: could not create target directory"));
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
