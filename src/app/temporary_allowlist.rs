use std::collections::HashSet;

use crate::app::{App, Local};
use crate::temporary_allowlist::{
    active_temporary_allowlist_sites_for_profile, parse_temporary_allowlist_specs,
    prune_expired_temporary_allowlist_entries, upsert_temporary_allowlist_entries,
};

impl App {
    pub(super) fn sync_temporary_allowlist_entries(&mut self, now: chrono::DateTime<Local>) {
        let expired_count = prune_expired_temporary_allowlist_entries(
            &mut self.temporary_allowlist_entries,
            now.timestamp(),
        );
        if expired_count == 0 {
            return;
        }

        self.recompute_blocker_sites_from_active_profile();
        self.apply_blocking_for_phase();
        self.append_temporary_allowlist_expiry_notice(expired_count);
        if let Err(error) = self.sync_cli_workflow_state() {
            self.config_error = Some(error);
        }
    }

    pub(super) fn active_temporary_allowlist_site_set_for_profile(
        &self,
        profile_name: &str,
    ) -> HashSet<String> {
        active_temporary_allowlist_sites_for_profile(
            &self.temporary_allowlist_entries,
            profile_name,
            self.current_frame_now.timestamp(),
        )
        .into_iter()
        .map(|site| site.to_ascii_lowercase())
        .collect()
    }

    pub(super) fn add_temporary_allowlist_entries_for_active_profile_from_input(
        &mut self,
        input: &str,
    ) -> Result<(usize, usize), String> {
        let specs = parse_temporary_allowlist_specs(input)?;
        self.current_frame_now = Local::now();
        prune_expired_temporary_allowlist_entries(
            &mut self.temporary_allowlist_entries,
            self.current_frame_now.timestamp(),
        );

        let profile_name = self.active_blocklist_profile_name().to_string();
        let (added, refreshed) = upsert_temporary_allowlist_entries(
            &mut self.temporary_allowlist_entries,
            &profile_name,
            &specs,
            self.current_frame_now.timestamp(),
        );
        if added == 0 && refreshed == 0 {
            return Ok((0, 0));
        }

        self.recompute_blocker_sites_from_active_profile();
        self.apply_blocking_for_phase();
        if let Err(error) = self.sync_cli_workflow_state() {
            self.config_error = Some(error);
        }
        Ok((added, refreshed))
    }

    fn append_temporary_allowlist_expiry_notice(&mut self, expired_count: usize) {
        let notice = if expired_count == 1 {
            "1 temporary allowlist entry expired.".to_string()
        } else {
            format!("{expired_count} temporary allowlist entries expired.")
        };
        if let Some(existing) = self.phase_notification.as_mut() {
            existing.push(' ');
            existing.push_str(&notice);
        } else {
            self.phase_notification = Some(notice);
        }
    }
}
