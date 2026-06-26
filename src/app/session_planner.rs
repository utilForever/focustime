use crate::app::{
    App, AppMode, KeyCode, KeyEvent, NavigationAction, PLANNER_RECENT_LABEL_LIMIT,
    PlannerFeedbackLevel, PlannerInputMode, ShortcutAction, normalize_task_label, task_label_index,
    task_label_key, task_label_state_labels,
};

impl App {
    pub(super) fn handle_key_session_planner(&mut self, key: KeyEvent) {
        if self.handle_session_planner_input_key(&key) {
            return;
        }

        if self.handle_quit_key(&key, false) {
            return;
        }

        if self.handle_session_planner_navigation_key(&key) {
            return;
        }
        if self.handle_session_planner_recent_digit_key(&key) {
            return;
        }
        self.handle_session_planner_shortcuts(&key);
    }

    fn handle_session_planner_input_key(&mut self, key: &KeyEvent) -> bool {
        if !self.planner_input_active {
            return false;
        }
        match key.code {
            _ if self.navigation_matches(NavigationAction::Confirm, key) => {
                self.commit_planner_input()
            }
            _ if self.navigation_matches(NavigationAction::Cancel, key) => {
                self.cancel_planner_input()
            }
            _ if self.navigation_matches(NavigationAction::Backspace, key) => {
                self.planner_input.pop();
            }
            KeyCode::Char(c) => {
                self.planner_input.push(c);
            }
            _ => {}
        }
        true
    }

    fn handle_session_planner_navigation_key(&mut self, key: &KeyEvent) -> bool {
        if self.navigation_matches(NavigationAction::Cancel, key) {
            self.set_mode(AppMode::Timer);
            return true;
        }
        if self.handle_session_planner_move_up(key) {
            return true;
        }
        if self.handle_session_planner_move_down(key) {
            return true;
        }
        self.handle_session_planner_confirm_key(key)
    }

    fn handle_session_planner_move_up(&mut self, key: &KeyEvent) -> bool {
        if !self.navigation_matches(NavigationAction::MoveUp, key) {
            return false;
        }
        self.planner_selection_index = self.planner_selection_index.saturating_sub(1);
        true
    }

    fn handle_session_planner_move_down(&mut self, key: &KeyEvent) -> bool {
        if !self.navigation_matches(NavigationAction::MoveDown, key) {
            return false;
        }
        let labels = self.planner_labels_for_display();
        if !labels.is_empty() {
            self.planner_selection_index =
                (self.planner_selection_index + 1).min(labels.len().saturating_sub(1));
        }
        true
    }

    fn handle_session_planner_confirm_key(&mut self, key: &KeyEvent) -> bool {
        if !self.navigation_matches(NavigationAction::Confirm, key) {
            return false;
        }
        self.select_planner_label();
        true
    }

    fn handle_session_planner_recent_digit_key(&mut self, key: &KeyEvent) -> bool {
        if let KeyCode::Char(c @ '1'..='9') = key.code {
            let index = (c as usize).saturating_sub('1' as usize);
            self.select_recent_planner_label(index);
            return true;
        }
        false
    }

    fn handle_session_planner_shortcuts(&mut self, key: &KeyEvent) {
        if self.handle_session_planner_back_shortcut(key) {
            return;
        }
        if self.handle_session_planner_add_shortcut(key) {
            return;
        }
        if self.handle_session_planner_rename_shortcut(key) {
            return;
        }
        if self.handle_session_planner_favorite_shortcut(key) {
            return;
        }
        if self.handle_session_planner_archive_shortcut(key) {
            return;
        }
        if self.handle_session_planner_delete_shortcut(key) {
            return;
        }
        let _ = self.handle_session_planner_select_recent_shortcut(key);
    }

    fn handle_session_planner_back_shortcut(&mut self, key: &KeyEvent) -> bool {
        if !self.shortcut_matches(ShortcutAction::BackTaskSetup, key) {
            return false;
        }
        self.set_mode(AppMode::Timer);
        true
    }

    fn handle_session_planner_add_shortcut(&mut self, key: &KeyEvent) -> bool {
        if !self.shortcut_matches(ShortcutAction::TaskSetupAdd, key) {
            return false;
        }
        self.start_planner_input();
        true
    }

    fn handle_session_planner_rename_shortcut(&mut self, key: &KeyEvent) -> bool {
        if !self.shortcut_matches(ShortcutAction::TaskSetupRename, key) {
            return false;
        }
        self.start_planner_rename_input();
        true
    }

    fn handle_session_planner_favorite_shortcut(&mut self, key: &KeyEvent) -> bool {
        if !self.shortcut_matches(ShortcutAction::TaskSetupFavorite, key) {
            return false;
        }
        self.toggle_planner_favorite();
        true
    }

    fn handle_session_planner_archive_shortcut(&mut self, key: &KeyEvent) -> bool {
        if !self.shortcut_matches(ShortcutAction::TaskSetupArchive, key) {
            return false;
        }
        self.toggle_planner_archive();
        true
    }

    fn handle_session_planner_delete_shortcut(&mut self, key: &KeyEvent) -> bool {
        if !(self.navigation_matches(NavigationAction::Delete, key)
            || self.shortcut_matches(ShortcutAction::TaskSetupDelete, key))
        {
            return false;
        }
        self.remove_planner_label();
        true
    }

    fn handle_session_planner_select_recent_shortcut(&mut self, key: &KeyEvent) -> bool {
        if !self.shortcut_matches(ShortcutAction::TaskSetupSelectRecent, key) {
            return false;
        }
        self.select_recent_planner_label(0);
        true
    }

    fn start_planner_input(&mut self) {
        self.planner_input.clear();
        self.planner_input_active = true;
        self.planner_input_mode = Some(PlannerInputMode::Add);
        self.planner_feedback = None;
    }

    fn start_planner_rename_input(&mut self) {
        if self.task_labels.is_empty() {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        }

        self.clamp_planner_selection();
        let Some(label) = self.planner_selected_label() else {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        };

        self.planner_input = label;
        self.planner_input_active = true;
        self.planner_input_mode = Some(PlannerInputMode::Rename);
        self.planner_feedback = None;
    }

    fn cancel_planner_input(&mut self) {
        self.planner_input.clear();
        self.planner_input_active = false;
        self.planner_input_mode = None;
    }

    fn commit_planner_input(&mut self) {
        let Some(mode) = self.planner_input_mode else {
            self.set_planner_feedback(
                PlannerFeedbackLevel::Warning,
                "Planner input mode unavailable",
            );
            return;
        };
        let Some(label) = normalize_task_label(&self.planner_input) else {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "Task label cannot be empty");
            return;
        };
        match mode {
            PlannerInputMode::Add => self.commit_planner_add_input(label),
            PlannerInputMode::Rename => self.commit_planner_rename_input(label),
        }
    }

    fn commit_planner_add_input(&mut self, label: String) {
        if let Some(existing_index) = task_label_index(&self.task_labels, &label) {
            let Some(existing_label) = self.task_labels.get(existing_index).cloned() else {
                self.set_planner_feedback(
                    PlannerFeedbackLevel::Warning,
                    "No task labels available",
                );
                return;
            };
            if let Some(display_index) = self.planner_display_index_for_label(&existing_label) {
                self.planner_selection_index = display_index;
            }
            if self.is_task_label_archived(&existing_label) {
                self.cancel_planner_input();
                self.set_planner_feedback(
                    PlannerFeedbackLevel::Warning,
                    format!(
                        "`{existing_label}` is archived; unarchive it with {} before selecting",
                        self.shortcut_hint(ShortcutAction::TaskSetupArchive)
                    ),
                );
                return;
            }
            self.selected_task_label = Some(existing_label);
            self.sync_task_planner_state();
            self.sync_recovery_snapshot();
            self.cancel_planner_input();
            self.set_planner_feedback(
                PlannerFeedbackLevel::Warning,
                format!("`{label}` already exists, selected existing label"),
            );
            return;
        }

        self.task_labels.push(label.clone());
        self.clamp_planner_selection();
        self.selected_task_label = Some(label.clone());
        self.sync_planner_selection_to_selected_label();
        self.sync_task_planner_state();
        self.sync_recovery_snapshot();
        self.cancel_planner_input();
        self.set_planner_feedback(
            PlannerFeedbackLevel::Success,
            format!("Added and selected `{label}`"),
        );
    }

    pub(super) fn commit_planner_rename_input(&mut self, label: String) {
        if self.task_labels.is_empty() {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        }

        self.clamp_planner_selection();
        let Some(current_label) = self.planner_selected_label() else {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        };
        let Some(current_index) = task_label_index(&self.task_labels, &current_label) else {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        };

        if current_label == label {
            self.set_planner_feedback(
                PlannerFeedbackLevel::Warning,
                format!("No change for `{current_label}`"),
            );
            return;
        }

        if let Some(existing_index) = task_label_index(&self.task_labels, &label)
            && existing_index != current_index
        {
            self.set_planner_feedback(
                PlannerFeedbackLevel::Warning,
                format!("`{label}` already exists"),
            );
            return;
        }

        let source_goal_target = self
            .stats
            .task_goal_progress_for_label(&current_label)
            .map(|progress| progress.target)
            .unwrap_or_default();
        let destination_goal_target = self
            .stats
            .task_goal_progress_for_label(&label)
            .map(|progress| progress.target)
            .unwrap_or_default();
        if !current_label.eq_ignore_ascii_case(&label)
            && source_goal_target.has_any_target()
            && destination_goal_target.has_any_target()
        {
            self.set_planner_feedback(
                PlannerFeedbackLevel::Warning,
                format!(
                    "Cannot rename `{current_label}` -> `{label}`: destination task goal already exists"
                ),
            );
            return;
        }

        if let Some(target) = self.task_labels.get_mut(current_index) {
            *target = label.clone();
        }
        let current_key = task_label_key(&current_label);
        let next_key = task_label_key(&label);
        if self.task_label_favorites.remove(&current_key) {
            self.task_label_favorites.insert(next_key.clone());
        }
        if self.task_label_archived.remove(&current_key) {
            self.task_label_archived.insert(next_key.clone());
        }
        if self
            .selected_task_label
            .as_ref()
            .is_some_and(|selected| selected.eq_ignore_ascii_case(&current_label))
        {
            self.selected_task_label = Some(label.clone());
        }
        if self
            .active_focus_task_label
            .as_ref()
            .is_some_and(|active| active.eq_ignore_ascii_case(&current_label))
        {
            self.active_focus_task_label = Some(label.clone());
        }
        self.stats.rename_task_goal_target(&current_label, &label);
        if let Some(index) = self.planner_display_index_for_label(&label) {
            self.planner_selection_index = index;
        }

        self.sync_task_planner_state();
        self.sync_recovery_snapshot();
        self.cancel_planner_input();
        self.set_planner_feedback(
            PlannerFeedbackLevel::Success,
            format!("Renamed `{current_label}` -> `{label}`"),
        );
    }

    fn remove_planner_label(&mut self) {
        if self.task_labels.is_empty() {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        }

        self.clamp_planner_selection();
        let Some(removed_label) = self.planner_selected_label() else {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        };
        let Some(removed_index) = task_label_index(&self.task_labels, &removed_label) else {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        };
        let previous_selection = self.planner_selection_index;
        let removed = self.task_labels.remove(removed_index);
        self.task_label_favorites.remove(&task_label_key(&removed));
        self.task_label_archived.remove(&task_label_key(&removed));
        self.stats.remove_task_goal_target(&removed);

        let removed_was_selected = self
            .selected_task_label
            .as_ref()
            .is_some_and(|selected| selected.eq_ignore_ascii_case(&removed));
        let selected_label_missing = self
            .selected_task_label
            .as_ref()
            .is_some_and(|selected| task_label_index(&self.task_labels, selected).is_none());
        if removed_was_selected || selected_label_missing {
            self.selected_task_label = self.nearest_selectable_task_label(previous_selection);
        }
        let display_labels = self.planner_labels_for_display();
        if display_labels.is_empty() {
            self.planner_selection_index = 0;
        } else if removed_was_selected || selected_label_missing {
            if let Some(selected_label) = self.selected_task_label.as_ref()
                && let Some(index) = self.planner_display_index_for_label(selected_label)
            {
                self.planner_selection_index = index;
            } else {
                self.planner_selection_index = previous_selection.min(display_labels.len() - 1);
            }
        } else {
            self.planner_selection_index = previous_selection.min(display_labels.len() - 1);
        }

        self.sync_task_planner_state();
        self.sync_recovery_snapshot();
        let feedback = if removed_was_selected {
            if let Some(selected) = self.selected_task_label.as_ref() {
                format!("Deleted `{removed}` (selected `{selected}`)")
            } else {
                format!("Deleted `{removed}` (no selected label)")
            }
        } else {
            format!("Deleted `{removed}`")
        };
        self.set_planner_feedback(PlannerFeedbackLevel::Success, feedback);
    }

    fn select_recent_planner_label(&mut self, index: usize) {
        let recent = self.planner_recent_labels(PLANNER_RECENT_LABEL_LIMIT);
        if recent.is_empty() {
            self.set_planner_feedback(
                PlannerFeedbackLevel::Warning,
                "No recent task labels available",
            );
            return;
        }
        if index >= recent.len() {
            self.set_planner_feedback(
                PlannerFeedbackLevel::Warning,
                format!("Recent quick-pick {} is unavailable", index + 1),
            );
            return;
        }

        let label = recent[index].clone();
        let Some(existing_index) = task_label_index(&self.task_labels, &label) else {
            self.set_planner_feedback(
                PlannerFeedbackLevel::Warning,
                format!("Recent label `{label}` is no longer available"),
            );
            return;
        };
        let Some(existing_label) = self.task_labels.get(existing_index).cloned() else {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        };
        if self.is_task_label_archived(&existing_label) {
            self.set_planner_feedback(
                PlannerFeedbackLevel::Warning,
                format!(
                    "`{existing_label}` is archived; unarchive it with {} before selecting",
                    self.shortcut_hint(ShortcutAction::TaskSetupArchive)
                ),
            );
            return;
        }
        if let Some(display_index) = self.planner_display_index_for_label(&existing_label) {
            self.planner_selection_index = display_index;
        }
        self.selected_task_label = Some(existing_label);
        self.sync_task_planner_state();
        self.sync_recovery_snapshot();
        self.set_planner_feedback(
            PlannerFeedbackLevel::Success,
            format!("Selected recent `{label}`"),
        );
    }

    pub(super) fn select_planner_label(&mut self) {
        if self.task_labels.is_empty() {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        }

        self.clamp_planner_selection();
        let Some(label) = self.planner_selected_label() else {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        };
        if self.is_task_label_archived(&label) {
            self.set_planner_feedback(
                PlannerFeedbackLevel::Warning,
                format!("Cannot select archived label `{label}`"),
            );
            return;
        }
        self.selected_task_label = Some(label.clone());
        self.sync_planner_selection_to_selected_label();
        self.sync_task_planner_state();
        self.sync_recovery_snapshot();
        self.set_planner_feedback(PlannerFeedbackLevel::Success, format!("Selected `{label}`"));
    }

    fn toggle_planner_favorite(&mut self) {
        if self.task_labels.is_empty() {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        }

        self.clamp_planner_selection();
        let Some(label) = self.planner_selected_label() else {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        };
        let key = task_label_key(&label);
        let message = if self.task_label_favorites.remove(&key) {
            format!("Removed favorite `{label}`")
        } else {
            self.task_label_favorites.insert(key);
            format!("Favorited `{label}`")
        };
        if let Some(index) = self.planner_display_index_for_label(&label) {
            self.planner_selection_index = index;
        }
        self.sync_task_planner_state();
        self.sync_recovery_snapshot();
        self.set_planner_feedback(PlannerFeedbackLevel::Success, message);
    }

    fn toggle_planner_archive(&mut self) {
        if self.task_labels.is_empty() {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        }

        self.clamp_planner_selection();
        let Some(label) = self.planner_selected_label() else {
            self.set_planner_feedback(PlannerFeedbackLevel::Warning, "No task labels available");
            return;
        };
        let key = task_label_key(&label);
        let message = if self.task_label_archived.remove(&key) {
            format!("Unarchived `{label}`")
        } else {
            self.task_label_archived.insert(key);
            if self
                .selected_task_label
                .as_ref()
                .is_some_and(|selected| selected.eq_ignore_ascii_case(&label))
            {
                self.selected_task_label =
                    self.nearest_selectable_task_label(self.planner_selection_index);
            }
            format!("Archived `{label}`")
        };
        if let Some(index) = self.planner_display_index_for_label(&label) {
            self.planner_selection_index = index;
        } else {
            self.clamp_planner_selection();
        }
        self.sync_task_planner_state();
        self.sync_recovery_snapshot();
        self.set_planner_feedback(PlannerFeedbackLevel::Success, message);
    }

    pub(super) fn clamp_planner_selection(&mut self) {
        let display_labels = self.planner_labels_for_display();
        if display_labels.is_empty() {
            self.planner_selection_index = 0;
        } else {
            self.planner_selection_index = self
                .planner_selection_index
                .min(display_labels.len().saturating_sub(1));
        }
    }

    pub(super) fn sync_task_planner_state(&mut self) {
        self.sync_wakatime_metadata_to_tracker();
        if self.stats.update_task_planner_state_with_label_states(
            self.task_labels.clone(),
            self.selected_task_label.clone(),
            task_label_state_labels(&self.task_label_favorites, &self.task_labels),
            task_label_state_labels(&self.task_label_archived, &self.task_labels),
        ) {
            self.mark_stats_dirty();
            self.flush_stats_if_dirty(false);
        }
    }

    pub(super) fn open_session_planner(&mut self) {
        self.set_mode(AppMode::TaskSetup);
        self.planner_feedback = None;
        self.planner_input.clear();
        self.planner_input_active = false;
        self.planner_input_mode = None;
        self.sync_planner_selection_to_selected_label();
    }
}
