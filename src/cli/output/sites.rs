use crate::cli::{
    BlocklistProfileCommandOutput, HistoryDashboardCommandOutput, SiteAddCommandOutput,
    SiteDeleteCommandOutput, SiteEditCommandOutput, SiteListCommandOutput,
};

use super::display_input_value;

pub(in crate::cli) fn print_blocklist_profile_command_output(
    payload: &BlocklistProfileCommandOutput,
) {
    if payload.updated {
        println!("Blocklist profile updated.");
    }
    println!(
        "Selected blocklist profile: {}",
        payload.selected_blocklist_profile
    );
    if payload.profiles.is_empty() {
        println!("Profiles: none");
        return;
    }
    println!("Profiles:");
    for profile in &payload.profiles {
        let marker = if profile.active { "*" } else { " " };
        println!(
            "  {marker} {} (blocklist {}, allowlist {}, effective {})",
            profile.name,
            profile.blocklist_sites_count,
            profile.allowlist_sites_count,
            profile.effective_blocked_sites_count
        );
    }
}

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
        "Active profile `{}` {} entries: {}",
        payload.profile,
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
            "Added {} hostname(s) to {} in profile `{}`.",
            payload.added.len(),
            payload.target.id(),
            payload.profile
        );
    } else {
        println!(
            "No {} hostnames were added in profile `{}`.",
            payload.target.id(),
            payload.profile
        );
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
            "Updated {} hostname in profile `{}`: {} -> {}",
            payload.target.id(),
            payload.profile,
            payload.previous,
            payload.current
        );
    } else {
        println!(
            "No change for {} hostname `{}` in profile `{}`.",
            payload.target.id(),
            payload.current,
            payload.profile
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
        "Deleted {} hostname `{}` from profile `{}`.",
        payload.target.id(),
        payload.removed,
        payload.profile
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
