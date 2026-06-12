use std::io::{self, Write};

use crate::cli::{
    BlocklistCategoryCommandOutput, BlocklistProfileCommandOutput, HistoryDashboardCommandOutput,
    SessionTemplateCommandOutput, SiteAddCommandOutput, SiteDeleteCommandOutput,
    SiteEditCommandOutput, SiteListCommandOutput, TemporarySiteAddCommandOutput,
    UsageSignalsCommandOutput,
};

use super::{display_input_value, format_duration};

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

pub(in crate::cli) fn print_blocklist_category_command_output(
    payload: &BlocklistCategoryCommandOutput,
) {
    if payload.deprecated {
        println!("Blocklist category workflows are deprecated.");
        println!("Replacement: {}", payload.replacement);
    }
    if payload.updated {
        println!("Blocklist category updated.");
    }
    println!(
        "Selected blocklist profile/category: {} / {}",
        payload.selected_blocklist_profile, payload.selected_blocklist_category
    );
    if payload.categories.is_empty() {
        println!("Categories: none");
        return;
    }
    println!("Categories:");
    for category in &payload.categories {
        let marker = if category.active { "*" } else { " " };
        println!(
            "  {marker} {} (blocklist {}, allowlist {})",
            category.name, category.blocklist_sites_count, category.allowlist_sites_count
        );
    }
}

pub(in crate::cli) fn print_session_template_command_output(
    payload: &SessionTemplateCommandOutput,
) {
    if payload.updated {
        println!("Session template updated.");
    }
    println!(
        "Selected session template: {}",
        payload
            .selected_session_template
            .as_deref()
            .unwrap_or("none")
    );
    if payload.templates.is_empty() {
        println!("Templates: none");
        return;
    }
    println!("Templates:");
    for template in &payload.templates {
        let marker = if template.active { "*" } else { " " };
        println!(
            "  {marker} {} (task `{}`, profile {}, blocklist `{}`, windows {})",
            template.name,
            template.task_label,
            template.profile,
            template.blocklist_profile,
            template.schedule_windows_count
        );
    }
}

pub(in crate::cli) fn print_history_dashboard_command_output(
    payload: &HistoryDashboardCommandOutput,
) {
    if payload.updated {
        println!("History dashboard updated.");
    }
    println!("Card order: {}", payload.card_order.join(", "));
    println!("Pinned cards: {}", payload.pinned_cards.join(", "));
    if payload.cards.is_empty() {
        println!("Cards: none");
        return;
    }
    println!("Cards:");
    for card in &payload.cards {
        let marker = if card.pinned { "*" } else { " " };
        println!("  {marker} {} ({})", card.label, card.id);
    }
}

pub(in crate::cli) fn print_usage_signals_command_output(payload: &UsageSignalsCommandOutput) {
    let mut stdout = io::stdout().lock();
    write_usage_signals_command_output(&mut stdout, payload)
        .expect("failed to write usage-signals command output");
}

fn write_usage_signals_command_output(
    writer: &mut impl Write,
    payload: &UsageSignalsCommandOutput,
) -> io::Result<()> {
    writeln!(writer, "Deprecated command: --usage-signals")?;
    writeln!(writer, "Replacement: {}", payload.replacement)
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

pub(in crate::cli) fn print_temporary_site_add_command_output(
    payload: &TemporarySiteAddCommandOutput,
) {
    if payload.updated {
        println!(
            "Temporary allowlist updated in profile `{}`: added {}, refreshed {}.",
            payload.profile, payload.added, payload.refreshed
        );
    } else {
        println!(
            "No temporary allowlist changes were applied in profile `{}`.",
            payload.profile
        );
    }
    if payload.active.is_empty() {
        println!("Active temporary exceptions: none");
        return;
    }
    println!("Active temporary exceptions ({}):", payload.active.len());
    for entry in &payload.active {
        println!(
            "  - {} (expires in {})",
            entry.site,
            format_duration(entry.remaining_secs)
        );
    }
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

#[cfg(test)]
mod tests {
    use crate::cli::{USAGE_SIGNALS_REPLACEMENT, UsageSignalsCommandOutput};

    use super::write_usage_signals_command_output;

    #[test]
    fn usage_signals_text_output_includes_deprecation_replacement_lines() {
        let payload = UsageSignalsCommandOutput {
            action: "usage-signals",
            deprecated: true,
            replacement: USAGE_SIGNALS_REPLACEMENT,
        };
        let mut output = Vec::new();

        write_usage_signals_command_output(&mut output, &payload).unwrap();

        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("Deprecated command: --usage-signals\n"));
        assert!(text.contains(&format!("Replacement: {}\n", USAGE_SIGNALS_REPLACEMENT)));
    }
}
