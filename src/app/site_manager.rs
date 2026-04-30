use super::*;

impl App {
    pub(super) fn handle_key_site_manager(&mut self, key: KeyEvent) {
        if self.blocklist_profile_input_active {
            match key.code {
                KeyCode::Enter => {
                    self.commit_blocklist_profile_input();
                }
                KeyCode::Esc => {
                    self.cancel_blocklist_profile_input();
                }
                KeyCode::Backspace => {
                    self.blocklist_profile_input.pop();
                }
                KeyCode::Char(c) => {
                    self.blocklist_profile_input.push(c);
                }
                _ => {}
            }
            return;
        }

        if self.site_input_active {
            match key.code {
                KeyCode::Enter => {
                    self.commit_site_input();
                }
                KeyCode::Esc => {
                    self.cancel_site_input();
                }
                KeyCode::Backspace => {
                    self.site_input.pop();
                }
                KeyCode::Char(c) => {
                    self.site_input.push(c);
                }
                _ => {}
            }
            return;
        }

        if self.handle_quit_key(&key, false) {
            return;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('b') => {
                self.mode = AppMode::Timer;
            }
            KeyCode::Down | KeyCode::Char('j') if !self.active_policy_sites().is_empty() => {
                self.selected_site =
                    (self.selected_site + 1).min(self.active_policy_sites().len() - 1);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected_site = self.selected_site.saturating_sub(1);
            }
            KeyCode::Char('m') => {
                self.toggle_site_list_mode();
            }
            KeyCode::Char('a') => {
                self.start_site_input(SiteInputMode::Add);
            }
            KeyCode::Char('e') => {
                self.start_site_input(SiteInputMode::Edit);
            }
            KeyCode::Char('d') | KeyCode::Delete => {
                self.remove_selected_site();
            }
            KeyCode::Char('[') => {
                self.select_previous_blocklist_profile();
            }
            KeyCode::Char(']') => {
                self.select_next_blocklist_profile();
            }
            KeyCode::Char('n') => {
                self.start_blocklist_profile_input(BlocklistProfileInputMode::Create);
            }
            KeyCode::Char('r') => {
                self.start_blocklist_profile_input(BlocklistProfileInputMode::Rename);
            }
            KeyCode::Char('x') => {
                self.delete_active_blocklist_profile();
            }
            _ => {}
        }
    }

    fn toggle_site_list_mode(&mut self) {
        self.site_list_mode = self.site_list_mode.toggle();
        self.clamp_selection();
        self.set_site_feedback(
            SiteFeedbackLevel::Success,
            format!("Switched to {} entries", self.site_list_mode.label()),
        );
    }

    pub(super) fn start_site_input(&mut self, mode: SiteInputMode) {
        self.cancel_blocklist_profile_input();
        self.site_input_active = true;
        self.site_feedback = None;
        match mode {
            SiteInputMode::Add => {
                self.site_edit_index = None;
                self.site_input.clear();
            }
            SiteInputMode::Edit => {
                if self.active_policy_sites().is_empty() {
                    self.site_input_active = false;
                    self.set_site_feedback(SiteFeedbackLevel::Warning, "No site selected to edit");
                    return;
                }
                self.clamp_selection();
                self.site_edit_index = Some(self.selected_site);
                self.site_input = self
                    .active_policy_sites()
                    .get(self.selected_site)
                    .cloned()
                    .unwrap_or_default();
            }
        }
    }

    pub(super) fn cancel_site_input(&mut self) {
        self.site_input.clear();
        self.site_input_active = false;
        self.site_edit_index = None;
    }

    pub(super) fn start_blocklist_profile_input(&mut self, mode: BlocklistProfileInputMode) {
        self.cancel_site_input();
        self.blocklist_profile_input_active = true;
        self.blocklist_profile_input_mode = Some(mode);
        self.site_feedback = None;
        match mode {
            BlocklistProfileInputMode::Create => {
                self.blocklist_profile_input.clear();
            }
            BlocklistProfileInputMode::Rename => {
                self.blocklist_profile_input = self.active_blocklist_profile_name().to_string();
            }
        }
    }

    pub(super) fn cancel_blocklist_profile_input(&mut self) {
        self.blocklist_profile_input.clear();
        self.blocklist_profile_input_active = false;
        self.blocklist_profile_input_mode = None;
    }

    fn commit_site_input(&mut self) {
        let input = self.site_input.clone();
        let mode = self.site_list_mode;
        let mut working = SiteBlocker::new();
        for site in self.active_profile_sites_for_mode(mode).iter().cloned() {
            working.add_site(site);
        }

        let committed = if let Some(index) = self.site_edit_index {
            let edit_result = working.edit_site_from_input(index, &input);
            if let Some(target_sites) = self.active_profile_sites_for_mode_mut(mode) {
                *target_sites = working.sites.clone();
            }
            self.apply_edit_site_result(edit_result)
        } else {
            let add_result = working.add_sites_from_input(&input);
            if let Some(target_sites) = self.active_profile_sites_for_mode_mut(mode) {
                *target_sites = working.sites.clone();
            }
            self.apply_bulk_add_result(add_result)
        };

        if committed {
            self.cancel_site_input();
        }
    }

    fn commit_blocklist_profile_input(&mut self) {
        let Some(mode) = self.blocklist_profile_input_mode else {
            return;
        };

        let name = self.blocklist_profile_input.trim().to_string();
        if name.is_empty() {
            self.set_site_feedback(SiteFeedbackLevel::Warning, "Profile name cannot be empty");
            return;
        }

        let has_duplicate = self
            .blocklist_profiles
            .iter()
            .enumerate()
            .any(|(index, profile)| {
                let is_current = mode == BlocklistProfileInputMode::Rename
                    && index == self.active_blocklist_profile;
                !is_current && profile.name.eq_ignore_ascii_case(&name)
            });
        if has_duplicate {
            self.set_site_feedback(
                SiteFeedbackLevel::Warning,
                format!("Profile `{name}` already exists"),
            );
            return;
        }

        match mode {
            BlocklistProfileInputMode::Create => {
                self.blocklist_profiles.push(BlocklistProfileConfig {
                    name: name.clone(),
                    sites: Vec::new(),
                    allowlist_sites: Vec::new(),
                });
                self.active_blocklist_profile = self.blocklist_profiles.len().saturating_sub(1);
                self.recompute_blocker_sites_from_active_profile();
                self.clamp_selection();
                self.cancel_blocklist_profile_input();
                self.save_config();
                self.sync_blocking_after_site_mutation();
                self.set_site_feedback(
                    SiteFeedbackLevel::Success,
                    format!("Created profile `{name}`"),
                );
            }
            BlocklistProfileInputMode::Rename => {
                let old_name = self.active_blocklist_profile_name().to_string();
                if old_name == name {
                    self.set_site_feedback(
                        SiteFeedbackLevel::Warning,
                        format!("No change for profile `{name}`"),
                    );
                    return;
                }
                if let Some(profile) = self
                    .blocklist_profiles
                    .get_mut(self.active_blocklist_profile)
                {
                    profile.name = name.clone();
                }
                self.cancel_blocklist_profile_input();
                self.save_config();
                self.set_site_feedback(
                    SiteFeedbackLevel::Success,
                    format!("Renamed profile `{old_name}` -> `{name}`"),
                );
            }
        }
    }

    fn apply_bulk_add_result(&mut self, result: BulkAddResult) -> bool {
        let committed = !result.added.is_empty();
        if committed {
            self.selected_site = self.active_policy_site_count().saturating_sub(1);
            self.finalize_site_mutation();
        }

        let mut parts = Vec::new();
        if !result.added.is_empty() {
            parts.push(format!(
                "Added {}",
                format_count(result.added.len(), "site", "sites")
            ));
        }
        if !result.duplicates.is_empty() {
            parts.push(format!(
                "Skipped {}",
                format_count(result.duplicates.len(), "duplicate", "duplicates")
            ));
        }
        if !result.invalid.is_empty() {
            parts.push(format!(
                "Rejected {} ({})",
                format_count(
                    result.invalid.len(),
                    "invalid hostname",
                    "invalid hostnames"
                ),
                summarize_invalid_inputs(&result.invalid)
            ));
        }

        let level = if result.invalid.is_empty() && result.duplicates.is_empty() {
            SiteFeedbackLevel::Success
        } else {
            SiteFeedbackLevel::Warning
        };
        let message = if parts.is_empty() {
            "No hostnames submitted".to_string()
        } else {
            parts.join(" • ")
        };
        self.set_site_feedback(level, message);
        committed
    }

    fn apply_edit_site_result(&mut self, result: EditSiteResult) -> bool {
        match result {
            EditSiteResult::Updated { old, new } => {
                self.finalize_site_mutation();
                self.set_site_feedback(
                    SiteFeedbackLevel::Success,
                    format!("Updated `{old}` -> `{new}`"),
                );
                true
            }
            EditSiteResult::Unchanged { hostname } => {
                self.set_site_feedback(
                    SiteFeedbackLevel::Warning,
                    format!("No change for `{hostname}`"),
                );
                false
            }
            EditSiteResult::Duplicate { hostname } => {
                self.set_site_feedback(
                    SiteFeedbackLevel::Warning,
                    format!(
                        "`{hostname}` is already in the {}",
                        self.site_list_mode.label().to_ascii_lowercase()
                    ),
                );
                false
            }
            EditSiteResult::Invalid(invalid) => {
                self.set_site_feedback(
                    SiteFeedbackLevel::Warning,
                    format!(
                        "Invalid hostname `{}` ({})",
                        display_input_value(&invalid.input),
                        invalid.reason.message()
                    ),
                );
                false
            }
            EditSiteResult::MissingSelection => {
                self.set_site_feedback(SiteFeedbackLevel::Warning, "No site selected to edit");
                false
            }
        }
    }

    fn remove_selected_site(&mut self) {
        if self.active_policy_sites().is_empty() {
            self.set_site_feedback(SiteFeedbackLevel::Warning, "No site selected to delete");
            return;
        }

        let mode = self.site_list_mode;
        let selected_site = self.selected_site;
        let list_name = mode.label().to_ascii_lowercase();
        let mut working = SiteBlocker::new();
        for site in self.active_profile_sites_for_mode(mode).iter().cloned() {
            working.add_site(site);
        }
        let removed = working.remove_site(selected_site);
        if let Some(target_sites) = self.active_profile_sites_for_mode_mut(mode) {
            *target_sites = working.sites.clone();
        }

        if let Some(removed) = removed {
            self.finalize_site_mutation();
            self.set_site_feedback(
                SiteFeedbackLevel::Success,
                format!("Removed `{removed}` from {list_name}"),
            );
        } else {
            self.set_site_feedback(SiteFeedbackLevel::Warning, "No site selected to delete");
        }
    }

    fn select_previous_blocklist_profile(&mut self) {
        if self.blocklist_profiles.len() <= 1 {
            return;
        }
        let next = if self.active_blocklist_profile == 0 {
            self.blocklist_profiles.len().saturating_sub(1)
        } else {
            self.active_blocklist_profile.saturating_sub(1)
        };
        self.switch_blocklist_profile(next);
    }

    fn select_next_blocklist_profile(&mut self) {
        if self.blocklist_profiles.len() <= 1 {
            return;
        }
        let next = (self.active_blocklist_profile + 1) % self.blocklist_profiles.len();
        self.switch_blocklist_profile(next);
    }

    fn switch_blocklist_profile(&mut self, next_index: usize) {
        if next_index >= self.blocklist_profiles.len()
            || next_index == self.active_blocklist_profile
        {
            return;
        }

        self.active_blocklist_profile = next_index;
        self.recompute_blocker_sites_from_active_profile();
        self.clamp_selection();
        self.save_config();
        self.sync_blocking_after_site_mutation();
        self.set_site_feedback(
            SiteFeedbackLevel::Success,
            format!(
                "Switched to profile `{}`",
                self.active_blocklist_profile_name()
            ),
        );
    }

    fn delete_active_blocklist_profile(&mut self) {
        if self.blocklist_profiles.len() <= 1 {
            self.set_site_feedback(
                SiteFeedbackLevel::Warning,
                "At least one blocklist profile is required",
            );
            return;
        }

        let removed = self
            .blocklist_profiles
            .remove(self.active_blocklist_profile);
        if self.active_blocklist_profile >= self.blocklist_profiles.len() {
            self.active_blocklist_profile = self.blocklist_profiles.len().saturating_sub(1);
        }
        self.recompute_blocker_sites_from_active_profile();
        self.clamp_selection();
        self.save_config();
        self.sync_blocking_after_site_mutation();
        self.set_site_feedback(
            SiteFeedbackLevel::Success,
            format!(
                "Deleted profile `{}` (active: `{}`)",
                removed.name,
                self.active_blocklist_profile_name()
            ),
        );
    }

    pub(super) fn clamp_selection(&mut self) {
        if self.active_policy_sites().is_empty() {
            self.selected_site = 0;
        } else {
            self.selected_site = self.selected_site.min(self.active_policy_sites().len() - 1);
        }
    }

    pub(super) fn clamp_blocklist_profile_selection(&mut self) {
        if self.blocklist_profiles.is_empty() {
            self.blocklist_profiles
                .push(BlocklistProfileConfig::default());
            self.active_blocklist_profile = 0;
            return;
        }
        self.active_blocklist_profile = self
            .active_blocklist_profile
            .min(self.blocklist_profiles.len().saturating_sub(1));
    }

    pub(super) fn active_profile_sites_for_mode(&self, mode: SiteListMode) -> &[String] {
        self.blocklist_profiles
            .get(self.active_blocklist_profile)
            .map(|profile| match mode {
                SiteListMode::Blocklist => &profile.sites,
                SiteListMode::Allowlist => &profile.allowlist_sites,
            })
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    fn active_profile_sites_for_mode_mut(
        &mut self,
        mode: SiteListMode,
    ) -> Option<&mut Vec<String>> {
        self.clamp_blocklist_profile_selection();
        self.blocklist_profiles
            .get_mut(self.active_blocklist_profile)
            .map(|profile| match mode {
                SiteListMode::Blocklist => &mut profile.sites,
                SiteListMode::Allowlist => &mut profile.allowlist_sites,
            })
    }

    pub(super) fn recompute_blocker_sites_from_active_profile(&mut self) {
        self.clamp_blocklist_profile_selection();
        self.blocker.sites.clear();
        if let Some(active_profile) = self.blocklist_profiles.get(self.active_blocklist_profile) {
            for site in effective_blocked_sites_for_profile(active_profile) {
                self.blocker.add_site(site);
            }
        }
    }

    pub(super) fn open_site_manager(&mut self) {
        self.pending_timer_action = None;
        self.mode = AppMode::SiteManager;
        self.site_list_mode = SiteListMode::Blocklist;
        self.cancel_site_input();
        self.cancel_blocklist_profile_input();
        self.clamp_blocklist_profile_selection();
        self.clamp_selection();
    }

    fn finalize_site_mutation(&mut self) {
        self.recompute_blocker_sites_from_active_profile();
        self.clamp_selection();
        self.save_config();
        self.sync_blocking_after_site_mutation();
    }

    fn sync_blocking_after_site_mutation(&mut self) {
        if !self.should_resync_blocking_after_site_mutation() {
            return;
        }

        let should_block = self.should_block_for_current_state();
        let block_result = if should_block {
            if self.blocker.sites.is_empty() {
                self.blocker.unblock()
            } else {
                self.blocker.block()
            }
        } else {
            self.blocker.unblock()
        };
        self.set_block_error_from_result(block_result);
    }

    pub(super) fn should_resync_blocking_after_site_mutation(&self) -> bool {
        self.should_block_for_current_state() || self.blocker.is_blocking
    }
}
