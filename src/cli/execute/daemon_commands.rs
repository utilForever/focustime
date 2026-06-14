use crate::cli::{
    DAEMON_API_REPLACEMENT, DaemonConnectionOutput, DaemonStartCommandOutput,
    DaemonStatusCommandOutput, DaemonStopCommandOutput, OutputMode,
    print_daemon_start_command_output, print_daemon_status_command_output,
    print_daemon_stop_command_output, print_json,
};
use crate::daemon;

pub(super) fn execute_daemon_start_command(
    port: Option<u16>,
    output: OutputMode,
) -> Result<(), String> {
    if daemon::is_daemon_child_process() {
        return daemon::run_foreground(port);
    }

    let started = daemon::start_background(port)?;
    let payload = DaemonStartCommandOutput {
        action: "daemon-start",
        deprecated: true,
        replacement: DAEMON_API_REPLACEMENT,
        already_running: started.already_running,
        daemon: daemon_connection_output(&started.info),
    };
    match output {
        OutputMode::Text => print_daemon_start_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

pub(super) fn execute_daemon_status_command(output: OutputMode) -> Result<(), String> {
    let status = daemon::status()?;
    let payload = DaemonStatusCommandOutput {
        action: "daemon-status",
        deprecated: true,
        replacement: DAEMON_API_REPLACEMENT,
        running: status.running,
        daemon: status.info.as_ref().map(daemon_connection_output),
    };
    match output {
        OutputMode::Text => print_daemon_status_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

pub(super) fn execute_daemon_stop_command(output: OutputMode) -> Result<(), String> {
    let stopped = daemon::stop()?;
    let payload = DaemonStopCommandOutput {
        action: "daemon-stop",
        deprecated: true,
        replacement: DAEMON_API_REPLACEMENT,
        was_running: stopped.was_running,
        stopped: stopped.stopped,
        daemon: stopped.info.as_ref().map(daemon_connection_output),
    };
    match output {
        OutputMode::Text => print_daemon_stop_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

fn daemon_connection_output(connection: &daemon::DaemonConnectionInfo) -> DaemonConnectionOutput {
    DaemonConnectionOutput {
        pid: connection.pid,
        host: connection.host.clone(),
        port: connection.port,
        started_at_epoch_secs: connection.started_at_epoch_secs,
    }
}
