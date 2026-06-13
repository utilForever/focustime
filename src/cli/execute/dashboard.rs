use crate::cli::{
    AppConfig, HistoryDashboardCardOutput, HistoryDashboardCommandKind,
    HistoryDashboardCommandOutput, OutputMode, print_history_dashboard_command_output, print_json,
};

const HISTORY_DASHBOARD_REPLACEMENT: &str = "Focus History now uses a stable default KPI layout; dashboard pin, unpin, and order customization commands are deprecated and no longer change configuration.";

pub(super) fn execute_history_dashboard_command(
    command: HistoryDashboardCommandKind,
    output: OutputMode,
) -> Result<(), String> {
    let mut config = AppConfig::load().normalized();
    let payload = apply_history_dashboard_command(&mut config, command)?;
    if payload.updated {
        config
            .save()
            .map_err(|error| format!("Failed to save history dashboard settings: {error}"))?;
    }
    match output {
        OutputMode::Text => print_history_dashboard_command_output(&payload),
        OutputMode::Json => print_json(&payload)?,
    }
    Ok(())
}

pub(in crate::cli) fn apply_history_dashboard_command(
    config: &mut AppConfig,
    command: HistoryDashboardCommandKind,
) -> Result<HistoryDashboardCommandOutput, String> {
    config.history_dashboard = config.history_dashboard.normalized();
    let (action, deprecated) = match command {
        HistoryDashboardCommandKind::Show => ("history-dashboard", false),
        HistoryDashboardCommandKind::Pin { .. } => ("history-dashboard-pin", true),
        HistoryDashboardCommandKind::Unpin { .. } => ("history-dashboard-unpin", true),
        HistoryDashboardCommandKind::SetOrder { .. } => ("history-dashboard-order", true),
    };

    Ok(build_history_dashboard_command_output(
        config, action, deprecated,
    ))
}

fn build_history_dashboard_command_output(
    config: &AppConfig,
    action: &'static str,
    deprecated: bool,
) -> HistoryDashboardCommandOutput {
    HistoryDashboardCommandOutput {
        action,
        deprecated,
        replacement: HISTORY_DASHBOARD_REPLACEMENT,
        updated: false,
        card_order: config
            .history_dashboard
            .card_order
            .iter()
            .map(|card| card.id())
            .collect(),
        pinned_cards: config
            .history_dashboard
            .pinned_cards
            .iter()
            .map(|card| card.id())
            .collect(),
        cards: config
            .history_dashboard
            .card_order
            .iter()
            .copied()
            .map(|card| HistoryDashboardCardOutput {
                id: card.id(),
                label: card.label(),
                pinned: true,
            })
            .collect(),
    }
}
