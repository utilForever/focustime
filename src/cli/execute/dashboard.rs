use crate::cli::{
    AppConfig, HistoryDashboardCardOutput, HistoryDashboardCommandKind,
    HistoryDashboardCommandOutput, OutputMode, print_history_dashboard_command_output, print_json,
};

pub(super) fn execute_history_dashboard_command(
    command: HistoryDashboardCommandKind,
    output: OutputMode,
) -> Result<(), String> {
    let mut config = AppConfig::load().normalized();
    let payload = apply_history_dashboard_command(&mut config, command)?;
    match output {
        OutputMode::Text => print_history_dashboard_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

pub(in crate::cli) fn apply_history_dashboard_command(
    config: &mut AppConfig,
    _command: HistoryDashboardCommandKind,
) -> Result<HistoryDashboardCommandOutput, String> {
    config.history_dashboard = config.history_dashboard.normalized();
    Ok(build_history_dashboard_command_output(config))
}

fn build_history_dashboard_command_output(config: &AppConfig) -> HistoryDashboardCommandOutput {
    HistoryDashboardCommandOutput {
        action: "history-dashboard",
        cards: config
            .history_dashboard
            .card_order
            .iter()
            .copied()
            .map(|card| HistoryDashboardCardOutput {
                id: card.id(),
                label: card.label(),
            })
            .collect(),
    }
}
