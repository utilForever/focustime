use crate::app::App;
use crate::cli::{
    AppConfig, FocusStats, OutputMode, UsageSignalsCommandOutput,
    build_blocking_preview_command_output, build_diagnostics_command_output,
    print_blocking_preview_command_output, print_config_doctor_output,
    print_config_migration_output, print_diagnostics_command_output, print_json,
    print_usage_signals_command_output,
};
use crate::config::{run_config_doctor, run_config_migration_assistant};

use super::data::stats_load_options;

const USAGE_SIGNAL_SUMMARY_LIMIT: usize = 5;
pub(super) fn execute_config_doctor_command(output: OutputMode) -> Result<(), String> {
    let payload = run_config_doctor();
    match output {
        OutputMode::Text => print_config_doctor_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

pub(super) fn execute_config_migrate_command(
    apply: bool,
    output: OutputMode,
) -> Result<(), String> {
    let payload = run_config_migration_assistant(apply);
    match output {
        OutputMode::Text => print_config_migration_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

pub(super) fn execute_diagnostics_command(output: OutputMode) -> Result<(), String> {
    let mut app = App::new();
    app.record_command_usage_for_cli("diagnostics");
    let payload = build_diagnostics_command_output(&app.setup_diagnostics);

    match output {
        OutputMode::Text => print_diagnostics_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

pub(super) fn execute_blocking_preview_command(output: OutputMode) -> Result<(), String> {
    let mut app = App::new();
    app.record_command_usage_for_cli("blocking-preview");
    let preview = app.blocking_preview_for_cli()?;
    let payload = build_blocking_preview_command_output(&preview);

    match output {
        OutputMode::Text => print_blocking_preview_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

pub(super) fn execute_usage_signals_command(output: OutputMode) -> Result<(), String> {
    let config = AppConfig::load().normalized();
    let stats = FocusStats::load_with_options(stats_load_options(&config))
        .map_err(|error| format!("Failed to load stats: {error}"))?;
    let payload = UsageSignalsCommandOutput {
        action: "usage-signals",
        summary: stats.usage_signal_summary(USAGE_SIGNAL_SUMMARY_LIMIT),
    };
    match output {
        OutputMode::Text => print_usage_signals_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}
