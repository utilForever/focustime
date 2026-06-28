use crate::cli::{
    HistoryDashboardCommandOutput, SiteAddCommandOutput, SiteDeleteCommandOutput,
    SiteEditCommandOutput, SiteListCommandOutput,
};

use super::display_input_value;

pub(in crate::cli) fn print_history_dashboard_command_output(
    payload: &HistoryDashboardCommandOutput,
) {
    let card_order = payload
        .cards
        .iter()
        .map(|card| card.id)
        .collect::<Vec<_>>()
        .join(", ");
    println!("Default KPI layout: {card_order}");
    if payload.cards.is_empty() {
        println!("Cards: none");
        return;
    }
    println!("Cards:");
    for card in &payload.cards {
        println!("  {} ({})", card.label, card.id);
    }
}

pub(in crate::cli) fn print_site_list_command_output(payload: &SiteListCommandOutput) {
    println!(
        "Canonical {} entries: {}",
        payload.target.id(),
        payload.sites.len()
    );
    for site in &payload.sites {
        println!("  - {site}");
    }
    println!(
        "Effective blocked sites: {}",
        payload.effective_blocked_sites_count
    );
}

pub(in crate::cli) fn print_site_add_command_output(payload: &SiteAddCommandOutput) {
    if payload.updated {
        println!(
            "Added {} hostname(s) to canonical {}.",
            payload.added.len(),
            payload.target.id(),
        );
    } else {
        println!("No canonical {} hostnames were added.", payload.target.id(),);
    }
    if !payload.duplicates.is_empty() {
        println!("Skipped duplicates: {}", payload.duplicates.join(", "));
    }
    if !payload.invalid.is_empty() {
        println!("Rejected invalid hostnames:");
        for invalid in &payload.invalid {
            println!(
                "  - {} ({})",
                display_input_value(&invalid.input),
                invalid.reason
            );
        }
    }
    println!(
        "{} entries now: {}",
        payload.target.id(),
        payload.sites.join(", ")
    );
    println!(
        "Effective blocked sites: {}",
        payload.effective_blocked_sites_count
    );
}

pub(in crate::cli) fn print_site_edit_command_output(payload: &SiteEditCommandOutput) {
    if payload.updated {
        println!(
            "Updated canonical {} hostname: {} -> {}",
            payload.target.id(),
            payload.previous,
            payload.current
        );
    } else {
        println!(
            "No change for canonical {} hostname `{}`.",
            payload.target.id(),
            payload.current,
        );
    }
    println!(
        "{} entries now: {}",
        payload.target.id(),
        payload.sites.join(", ")
    );
    println!(
        "Effective blocked sites: {}",
        payload.effective_blocked_sites_count
    );
}

pub(in crate::cli) fn print_site_delete_command_output(payload: &SiteDeleteCommandOutput) {
    println!(
        "Deleted canonical {} hostname `{}`.",
        payload.target.id(),
        payload.removed,
    );
    println!(
        "{} entries now: {}",
        payload.target.id(),
        payload.sites.join(", ")
    );
    println!(
        "Effective blocked sites: {}",
        payload.effective_blocked_sites_count
    );
}
