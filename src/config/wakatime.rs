use super::normalize_nonempty_or_default_string;
use serde::{Deserialize, Serialize};

const WAKATIME_QUEUE_CAPACITY_MIN: usize = 1;
const WAKATIME_QUEUE_CAPACITY_MAX: usize = 4096;
const WAKATIME_RETRY_DELAY_MIN_SECS: u64 = 1;
const WAKATIME_RETRY_DELAY_MAX_SECS: u64 = 60 * 60;
const WAKATIME_RETRY_BACKOFF_MIN_SECS: u64 = 1;
const WAKATIME_RETRY_BACKOFF_MAX_SECS: u64 = 300;
const WAKATIME_RETRY_BACKOFF_MAX_ENTRIES: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct WakatimeRuntimeConfig {
    #[serde(default = "default_wakatime_retry_backoff_secs")]
    pub(crate) retry_backoff_secs: Vec<u64>,
    #[serde(default = "default_wakatime_queue_capacity")]
    pub(crate) queue_capacity: usize,
    #[serde(default = "default_wakatime_queue_retry_delay_secs")]
    pub(crate) queue_retry_delay_secs: u64,
}

impl WakatimeRuntimeConfig {
    pub(crate) fn normalized(&self) -> Self {
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
pub(crate) struct WakatimeMetadataConfig {
    #[serde(default = "default_wakatime_project")]
    pub(crate) project: String,
    #[serde(default = "default_wakatime_language")]
    pub(crate) language: String,
}

impl WakatimeMetadataConfig {
    pub(crate) fn normalized(&self) -> Self {
        Self {
            project: normalize_nonempty_or_default_string(
                &self.project,
                &default_wakatime_project(),
            ),
            language: normalize_nonempty_or_default_string(
                &self.language,
                &default_wakatime_language(),
            ),
        }
    }
}

impl Default for WakatimeMetadataConfig {
    fn default() -> Self {
        Self {
            project: default_wakatime_project(),
            language: default_wakatime_language(),
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
