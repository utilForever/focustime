use std::collections::BTreeSet;

use crate::app::{App, task_label_index, task_label_key};

impl App {
    pub(crate) fn is_task_label_favorite(&self, label: &str) -> bool {
        self.task_label_favorites.contains(&task_label_key(label))
    }

    pub(crate) fn is_task_label_archived(&self, label: &str) -> bool {
        self.task_label_archived.contains(&task_label_key(label))
    }

    pub(crate) fn planner_labels_for_display(&self) -> Vec<String> {
        let mut favorites = Vec::new();
        let mut others = Vec::new();
        for label in &self.task_labels {
            if self.is_task_label_favorite(label) {
                favorites.push(label.clone());
            } else {
                others.push(label.clone());
            }
        }
        favorites.extend(others);
        favorites
    }

    pub(super) fn planner_display_index_for_label(&self, label: &str) -> Option<usize> {
        self.planner_labels_for_display()
            .iter()
            .position(|entry| entry.eq_ignore_ascii_case(label))
    }

    pub(super) fn planner_selected_label(&self) -> Option<String> {
        self.planner_labels_for_display()
            .get(self.planner_selection_index)
            .cloned()
    }

    pub(super) fn nearest_selectable_task_label(&self, preferred_index: usize) -> Option<String> {
        let labels = self.planner_labels_for_display();
        if labels.is_empty() {
            return None;
        }

        let start = preferred_index.min(labels.len().saturating_sub(1));
        for distance in 0..labels.len() {
            if let Some(index) = start.checked_sub(distance)
                && let Some(label) = labels.get(index)
                && !self.is_task_label_archived(label)
            {
                return Some(label.clone());
            }
            if distance > 0
                && let Some(index) = start.checked_add(distance)
                && let Some(label) = labels.get(index)
                && !self.is_task_label_archived(label)
            {
                return Some(label.clone());
            }
        }
        None
    }

    pub(super) fn has_selectable_task_label_for_focus(&self) -> bool {
        self.selected_task_label
            .as_ref()
            .is_some_and(|label| !self.is_task_label_archived(label))
    }

    pub(super) fn sync_planner_selection_to_selected_label(&mut self) {
        if let Some(selected_label) = self.selected_task_label.as_ref()
            && let Some(index) = self.planner_display_index_for_label(selected_label)
        {
            self.planner_selection_index = index;
            return;
        }
        self.clamp_planner_selection();
    }

    pub(crate) fn planner_recent_labels(&self, limit: usize) -> Vec<String> {
        if limit == 0 || self.task_labels.is_empty() {
            return Vec::new();
        }

        let mut source_limit = limit;
        loop {
            let source = self.stats.recent_task_labels(source_limit);
            let exhausted = source.len() < source_limit;
            let recent = self.collect_recent_selectable_labels(source, limit);

            if recent.len() >= limit || exhausted {
                return recent;
            }

            let Some(next_limit) = source_limit.checked_mul(2) else {
                return recent;
            };
            source_limit = next_limit;
        }
    }

    fn collect_recent_selectable_labels(&self, source: Vec<String>, limit: usize) -> Vec<String> {
        let mut recent = Vec::new();
        let mut seen = BTreeSet::new();

        for label in source {
            let Some(canonical) = self.canonical_selectable_task_label(&label) else {
                continue;
            };
            let key = canonical.to_ascii_lowercase();
            if !seen.insert(key) {
                continue;
            }
            recent.push(canonical);
            if recent.len() >= limit {
                break;
            }
        }

        recent
    }

    fn canonical_selectable_task_label(&self, label: &str) -> Option<String> {
        let existing_index = task_label_index(&self.task_labels, label)?;
        let canonical = self.task_labels.get(existing_index)?.clone();
        (!self.is_task_label_archived(&canonical)).then_some(canonical)
    }
}
