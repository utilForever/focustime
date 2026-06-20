use crate::app::{App, normalize_task_label, task_label_index};
use crate::config::{
    AutomationTriggerActionConfig, ProfileAutomationConfig, SessionTemplateConfig,
};

impl App {
    pub(super) fn apply_selected_session_template_before_start(&mut self) -> Result<(), String> {
        let Some(index) = self.active_session_template else {
            return Ok(());
        };
        self.apply_session_template_by_index(index)?;
        Ok(())
    }

    pub(super) fn select_session_template(&mut self, name: Option<&str>) -> Result<bool, String> {
        let Some(name) = name.map(str::trim).filter(|value| !value.is_empty()) else {
            return Ok(false);
        };
        let Some(index) = self.session_template_index_by_name(name) else {
            return Err(format!("Unknown session template `{name}`."));
        };
        if self.active_session_template == Some(index) {
            return Ok(false);
        }
        self.active_session_template = Some(index);
        self.save_config();
        Ok(true)
    }

    pub(super) fn capture_session_template(&mut self, name: &str) -> Result<bool, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Template name cannot be empty.".to_string());
        }
        if self.session_template_index_by_name(name).is_some() {
            return Err(format!("Template `{name}` already exists."));
        }
        let Some(task_label) = self
            .selected_task_label
            .as_ref()
            .and_then(|value| normalize_task_label(value))
        else {
            return Err("Cannot create template: select a task label first.".to_string());
        };
        let template = SessionTemplateConfig {
            name: name.to_string(),
            task_label,
            profile: self.selected_profile,
            blocklist_profile: self.active_blocklist_profile_name().to_string(),
            schedule: self.selected_profile_automation().recurring_schedule,
        };
        self.session_templates.push(template);
        self.active_session_template = Some(self.session_templates.len().saturating_sub(1));
        self.save_config();
        Ok(true)
    }

    pub(super) fn rename_active_session_template(&mut self, name: &str) -> Result<bool, String> {
        let Some(active_index) = self.active_session_template else {
            return Err("No active session template selected.".to_string());
        };
        self.rename_session_template_at(active_index, name)
    }

    pub(super) fn rename_session_template_at(
        &mut self,
        template_index: usize,
        name: &str,
    ) -> Result<bool, String> {
        if template_index >= self.session_templates.len() {
            return Err("Session template selection is invalid.".to_string());
        }
        let name = name.trim();
        if name.is_empty() {
            return Err("Template name cannot be empty.".to_string());
        }
        let Some(current_name) = self
            .session_templates
            .get(template_index)
            .map(|template| template.name.clone())
        else {
            return Err("Session template selection is invalid.".to_string());
        };
        if current_name == name {
            return Ok(false);
        }
        let duplicate = self
            .session_templates
            .iter()
            .enumerate()
            .any(|(index, template)| {
                index != template_index && template.name.eq_ignore_ascii_case(name)
            });
        if duplicate {
            return Err(format!("Template `{name}` already exists."));
        }
        if let Some(template) = self.session_templates.get_mut(template_index) {
            template.name = name.to_string();
        }
        self.rename_session_template_references(&current_name, name);
        self.save_config();
        Ok(true)
    }

    pub(super) fn delete_active_session_template(&mut self) -> Result<bool, String> {
        let Some(active_index) = self.active_session_template else {
            return Err("No active session template selected.".to_string());
        };
        self.delete_session_template_at(active_index)
    }

    pub(super) fn delete_session_template_at(&mut self, index: usize) -> Result<bool, String> {
        if index >= self.session_templates.len() {
            return Err("Session template selection is invalid.".to_string());
        }
        let removed_name = self
            .session_templates
            .get(index)
            .map(|template| template.name.clone())
            .ok_or_else(|| "Session template selection is invalid.".to_string())?;
        self.session_templates.remove(index);
        if self.session_templates.is_empty() {
            self.active_session_template = None;
        } else {
            self.active_session_template = self.active_session_template.map(|active_index| {
                if active_index == index {
                    index.min(self.session_templates.len().saturating_sub(1))
                } else if active_index > index {
                    active_index.saturating_sub(1)
                } else {
                    active_index.min(self.session_templates.len().saturating_sub(1))
                }
            });
        }
        self.clear_session_template_references(&removed_name);
        self.save_config();
        Ok(true)
    }

    pub(super) fn apply_session_template(&mut self, name: Option<&str>) -> Result<bool, String> {
        let index = if let Some(name) = name.map(str::trim).filter(|value| !value.is_empty()) {
            self.session_template_index_by_name(name)
                .ok_or_else(|| format!("Unknown session template `{name}`."))?
        } else {
            self.active_session_template
                .ok_or_else(|| "No active session template selected.".to_string())?
        };
        self.apply_session_template_by_index(index)
    }

    fn apply_session_template_by_index(&mut self, index: usize) -> Result<bool, String> {
        let Some(template) = self.session_templates.get(index).cloned() else {
            return Err("Session template selection is invalid.".to_string());
        };
        if self.strict_mode_enforced_for_focus() {
            return Err("Cannot apply template while strict focus is active.".to_string());
        }
        let Some(blocklist_index) = self.blocklist_profiles.iter().position(|profile| {
            profile
                .name
                .eq_ignore_ascii_case(&template.blocklist_profile)
        }) else {
            return Err(format!(
                "Template `{}` references missing blocklist profile `{}`.",
                template.name, template.blocklist_profile
            ));
        };
        let task_label = self.resolve_template_task_label(&template.task_label)?;

        let mut automation = self
            .profile_automation
            .for_profile(template.profile, &ProfileAutomationConfig::default());
        automation.recurring_schedule = template.schedule.normalized();
        self.profile_automation
            .set_for_profile(template.profile, automation);

        if !self.apply_profile(template.profile) {
            let reason = self
                .phase_notification
                .clone()
                .unwrap_or_else(|| "Failed to apply profile from template.".to_string());
            return Err(reason);
        }

        self.active_blocklist_profile = blocklist_index;
        self.recompute_blocker_sites_from_active_profile();
        self.selected_task_label = Some(task_label.clone());
        self.sync_planner_selection_to_selected_label();
        self.sync_task_planner_state();
        self.sync_recovery_snapshot();
        self.active_session_template = Some(index);
        self.save_config();
        self.phase_notification = Some(format!("Applied session template `{}`.", template.name));
        Ok(true)
    }

    fn resolve_template_task_label(&mut self, task_label: &str) -> Result<String, String> {
        let Some(normalized_label) = normalize_task_label(task_label) else {
            return Err("Template task label is empty.".to_string());
        };
        if let Some(index) = task_label_index(&self.task_labels, &normalized_label) {
            let Some(existing_label) = self.task_labels.get(index).cloned() else {
                return Err("Template task label lookup failed.".to_string());
            };
            if self.is_task_label_archived(&existing_label) {
                return Err(format!(
                    "Template task label `{existing_label}` is archived and cannot be selected."
                ));
            }
            return Ok(existing_label);
        }
        self.task_labels.push(normalized_label.clone());
        Ok(normalized_label)
    }

    pub(super) fn session_template_index_by_name(&self, name: &str) -> Option<usize> {
        self.session_templates
            .iter()
            .position(|template| template.name.eq_ignore_ascii_case(name.trim()))
    }

    fn rename_session_template_references(&mut self, previous_name: &str, next_name: &str) {
        self.rewrite_session_template_references(previous_name, Some(next_name));
    }

    fn clear_session_template_references(&mut self, removed_name: &str) {
        self.rewrite_session_template_references(removed_name, None);
    }

    fn rewrite_session_template_references(
        &mut self,
        target_name: &str,
        replacement: Option<&str>,
    ) {
        let replacement = replacement.map(str::to_string);

        for trigger in &mut self.automation_triggers {
            if let AutomationTriggerActionConfig::ApplyDefaults {
                session_template, ..
            } = &mut trigger.action
                && session_template
                    .as_deref()
                    .is_some_and(|name| name.eq_ignore_ascii_case(target_name))
            {
                *session_template = replacement.clone();
            }
        }
    }
}
