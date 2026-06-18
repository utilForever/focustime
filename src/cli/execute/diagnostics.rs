use crate::app::App;
use crate::cli::{
    OutputMode, build_blocking_preview_command_output, build_diagnostics_blocking_preview_error,
    build_diagnostics_blocking_preview_output, build_diagnostics_command_output,
    print_blocking_preview_command_output, print_config_doctor_output,
    print_config_migration_output, print_diagnostics_command_output, print_json,
};
use crate::config::{run_config_doctor, run_config_migration_assistant};
use crate::error::UserMessage;

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
    let config_doctor = run_config_doctor();
    let config_migration = run_config_migration_assistant(false);
    let blocking_preview = match app.blocking_preview_for_cli() {
        Ok(preview) => build_diagnostics_blocking_preview_output(&preview),
        Err(error) => build_diagnostics_blocking_preview_error(error.to_string()),
    };
    let payload = build_diagnostics_command_output(
        &app.setup_diagnostics,
        config_doctor,
        config_migration,
        blocking_preview,
    );

    match output {
        OutputMode::Text => print_diagnostics_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

pub(super) fn execute_blocking_preview_command(output: OutputMode) -> Result<(), UserMessage> {
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
