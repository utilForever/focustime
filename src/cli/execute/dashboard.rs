use crate::cli::{
    AppConfig, HistoryDashboardCardOutput, HistoryDashboardCommandKind,
    HistoryDashboardCommandOutput, HistoryKpiCardId, OutputMode,
    print_history_dashboard_command_output, print_json,
};

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
    let (action, updated) = match command {
        HistoryDashboardCommandKind::Show => ("history-dashboard", false),
        HistoryDashboardCommandKind::Pin { card } => {
            if config.history_dashboard.pinned_cards.contains(&card) {
                ("history-dashboard-pin", false)
            } else {
                let insert_at = config
                    .history_dashboard
                    .pinned_cards
                    .iter()
                    .position(|candidate| {
                        card_order_index(&config.history_dashboard.card_order, *candidate)
                            > card_order_index(&config.history_dashboard.card_order, card)
                    })
                    .unwrap_or(config.history_dashboard.pinned_cards.len());
                config
                    .history_dashboard
                    .pinned_cards
                    .insert(insert_at, card);
                ("history-dashboard-pin", true)
            }
        }
        HistoryDashboardCommandKind::Unpin { card } => {
            if let Some(index) = config
                .history_dashboard
                .pinned_cards
                .iter()
                .position(|candidate| *candidate == card)
            {
                if config.history_dashboard.pinned_cards.len() <= 1 {
                    return Err(
                        "At least one history dashboard card must remain pinned.".to_string()
                    );
                }
                config.history_dashboard.pinned_cards.remove(index);
                ("history-dashboard-unpin", true)
            } else {
                ("history-dashboard-unpin", false)
            }
        }
        HistoryDashboardCommandKind::SetOrder { order } => {
            if config.history_dashboard.card_order == order {
                ("history-dashboard-order", false)
            } else {
                config.history_dashboard.card_order = order;
                ("history-dashboard-order", true)
            }
        }
    };

    config
        .history_dashboard
        .pinned_cards
        .sort_by_key(|card| card_order_index(&config.history_dashboard.card_order, *card));
    Ok(build_history_dashboard_command_output(
        config, action, updated,
    ))
}

fn build_history_dashboard_command_output(
    config: &AppConfig,
    action: &'static str,
    updated: bool,
) -> HistoryDashboardCommandOutput {
    let mut cards = config.history_dashboard.pinned_cards.clone();
    for card in &config.history_dashboard.card_order {
        if !cards.contains(card) {
            cards.push(*card);
        }
    }
    HistoryDashboardCommandOutput {
        action,
        updated,
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
        cards: cards
            .into_iter()
            .map(|card| HistoryDashboardCardOutput {
                id: card.id(),
                label: card.label(),
                pinned: config.history_dashboard.pinned_cards.contains(&card),
            })
            .collect(),
    }
}

fn card_order_index(order: &[HistoryKpiCardId], card: HistoryKpiCardId) -> usize {
    order
        .iter()
        .position(|candidate| *candidate == card)
        .unwrap_or(usize::MAX)
}
