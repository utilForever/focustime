use std::collections::HashSet;

use super::{normalize_nonempty_or_default_string, normalize_optional_nonempty_string};
use serde::{Deserialize, Serialize};

const WAKATIME_QUEUE_CAPACITY_MIN: usize = 1;
const WAKATIME_QUEUE_CAPACITY_MAX: usize = 4096;
const WAKATIME_RETRY_DELAY_MIN_SECS: u64 = 1;
const WAKATIME_RETRY_DELAY_MAX_SECS: u64 = 60 * 60;
const WAKATIME_RETRY_BACKOFF_MIN_SECS: u64 = 1;
const WAKATIME_RETRY_BACKOFF_MAX_SECS: u64 = 300;
const WAKATIME_RETRY_BACKOFF_MAX_ENTRIES: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WakatimeTaskMappingConfig {
    #[serde(default)]
    pub task_label: String,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WakatimeRuntimeConfig {
    #[serde(default = "default_wakatime_retry_backoff_secs")]
    pub retry_backoff_secs: Vec<u64>,
    #[serde(default = "default_wakatime_queue_capacity")]
    pub queue_capacity: usize,
    #[serde(default = "default_wakatime_queue_retry_delay_secs")]
    pub queue_retry_delay_secs: u64,
}

impl WakatimeRuntimeConfig {
    pub fn normalized(&self) -> Self {
        Self {
            retry_backoff_secs: normalize_wakatime_retry_backoff_secs(&self.retry_backoff_secs),
            queue_capacity: self
                .queue_capacity
                .clamp(WAKATIME_QUEUE_CAPACITY_MIN, WAKATIME_QUEUE_CAPACITY_MAX),
            queue_retry_delay_secs: self
                .queue_retry_delay_secs
                .clamp(WAKATIME_RETRY_DELAY_MIN_SECS, WAKATIME_RETRY_DELAY_MAX_SECS),
        }
    }
}

impl Default for WakatimeRuntimeConfig {
    fn default() -> Self {
        Self {
            retry_backoff_secs: default_wakatime_retry_backoff_secs(),
            queue_capacity: default_wakatime_queue_capacity(),
            queue_retry_delay_secs: default_wakatime_queue_retry_delay_secs(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WakatimeMetadataConfig {
    #[serde(default = "default_wakatime_project")]
    pub project: String,
    #[serde(default = "default_wakatime_language")]
    pub language: String,
    #[serde(default)]
    pub task_mappings: Vec<WakatimeTaskMappingConfig>,
}

impl WakatimeMetadataConfig {
    pub fn normalized(&self) -> Self {
        Self {
            project: normalize_nonempty_or_default_string(
                &self.project,
                &default_wakatime_project(),
            ),
            language: normalize_nonempty_or_default_string(
                &self.language,
                &default_wakatime_language(),
            ),
            task_mappings: normalize_wakatime_task_mappings(&self.task_mappings),
        }
    }

    pub fn task_mapping_for_label(&self, task_label: &str) -> Option<&WakatimeTaskMappingConfig> {
        let task_label = task_label.trim();
        if task_label.is_empty() {
            return None;
        }
        self.task_mappings
            .iter()
            .find(|mapping| mapping.task_label.eq_ignore_ascii_case(task_label))
    }

    pub fn resolved_project_language_for_task_label(
        &self,
        task_label: Option<&str>,
    ) -> (String, String) {
        let Some(task_label) = task_label.map(str::trim).filter(|label| !label.is_empty()) else {
            return (self.project.clone(), self.language.clone());
        };
        let Some(mapping) = self.task_mapping_for_label(task_label) else {
            return (self.project.clone(), self.language.clone());
        };
        (
            mapping
                .project
                .clone()
                .unwrap_or_else(|| self.project.clone()),
            mapping
                .language
                .clone()
                .unwrap_or_else(|| self.language.clone()),
        )
    }
}

impl Default for WakatimeMetadataConfig {
    fn default() -> Self {
        Self {
            project: default_wakatime_project(),
            language: default_wakatime_language(),
            task_mappings: Vec::new(),
        }
    }
}

fn default_wakatime_project() -> String {
    "focustime".to_string()
}

fn default_wakatime_language() -> String {
    "Pomodoro".to_string()
}

fn default_wakatime_retry_backoff_secs() -> Vec<u64> {
    vec![2, 5, 10]
}

fn default_wakatime_queue_capacity() -> usize {
    512
}

fn default_wakatime_queue_retry_delay_secs() -> u64 {
    30
}

fn normalize_wakatime_task_mappings(
    mappings: &[WakatimeTaskMappingConfig],
) -> Vec<WakatimeTaskMappingConfig> {
    let mut normalized = Vec::new();
    let mut seen_labels = HashSet::new();
    for mapping in mappings {
        let Some(task_label) = normalize_optional_nonempty_string(Some(&mapping.task_label)) else {
            continue;
        };
        let project = normalize_optional_nonempty_string(mapping.project.as_deref());
        let language = normalize_optional_nonempty_string(mapping.language.as_deref());
        if project.is_none() && language.is_none() {
            continue;
        }
        let key = task_label.to_ascii_lowercase();
        if seen_labels.insert(key) {
            normalized.push(WakatimeTaskMappingConfig {
                task_label,
                project,
                language,
            });
        }
    }
    normalized
}

fn normalize_wakatime_retry_backoff_secs(backoff_secs: &[u64]) -> Vec<u64> {
    let normalized = backoff_secs
        .iter()
        .filter_map(|secs| {
            if *secs == 0 {
                None
            } else {
                Some((*secs).clamp(
                    WAKATIME_RETRY_BACKOFF_MIN_SECS,
                    WAKATIME_RETRY_BACKOFF_MAX_SECS,
                ))
            }
        })
        .take(WAKATIME_RETRY_BACKOFF_MAX_ENTRIES)
        .collect::<Vec<_>>();
    if normalized.is_empty() {
        default_wakatime_retry_backoff_secs()
    } else {
        normalized
    }
}
