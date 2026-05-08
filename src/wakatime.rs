use std::collections::VecDeque;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};

const HEARTBEAT_INTERVAL_SECS: u64 = 120;
const HEARTBEAT_RETRY_BACKOFF_SECS: [u64; 2] = [1, 2];
const HEARTBEAT_MAX_ATTEMPTS: u8 = 3;
const HEARTBEAT_QUEUE_CAPACITY: usize = 256;
const HEARTBEAT_QUEUE_SNAPSHOT_FILE_NAME: &str = "wakatime-queue.toml";
const HEARTBEAT_QUEUE_SNAPSHOT_SCHEMA_VERSION: u32 = 1;
#[cfg(not(test))]
const HEARTBEAT_QUEUE_RETRY_DELAY_SECS: u64 = 10;
#[cfg(test)]
const HEARTBEAT_QUEUE_RETRY_DELAY_SECS: u64 = 0;
const DEFAULT_API_URL: &str = "https://wakatime.com";
const DEFAULT_HEARTBEAT_ENTITY: &str = "focustime";
const DEFAULT_HEARTBEAT_PROJECT: &str = "focustime";
const DEFAULT_HEARTBEAT_LANGUAGE: &str = "Pomodoro";

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
struct Heartbeat {
    entity: String,
    #[serde(rename = "type")]
    entity_type: String,
    time: f64,
    project: String,
    language: String,
    is_write: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HeartbeatQueueSnapshot {
    schema_version: u32,
    queued_heartbeats: Vec<Heartbeat>,
    in_flight_heartbeat: Option<Heartbeat>,
    in_flight_from_queue: bool,
    queue_retry_not_before_epoch_secs: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WakatimeRuntimeState {
    NotConfigured,
    Idle,
    Tracking,
    Sending,
    Queued {
        pending: usize,
    },
    Replaying {
        pending: usize,
    },
    Retrying {
        attempt: u8,
        max_attempts: u8,
        next_backoff_secs: u64,
        error: String,
    },
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakatimeConfigStatus {
    Configured,
    MissingConfigFile,
    MissingApiKey,
    UnreadableConfig,
    HomeDirectoryUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WakatimeConfigDiagnostics {
    pub config_path: Option<String>,
    pub status: WakatimeConfigStatus,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WakatimeHeartbeatMetadata {
    pub project: String,
    pub language: String,
}

impl WakatimeHeartbeatMetadata {
    pub fn normalized(&self) -> Self {
        Self {
            project: normalize_nonempty_or_default(&self.project, DEFAULT_HEARTBEAT_PROJECT),
            language: normalize_nonempty_or_default(&self.language, DEFAULT_HEARTBEAT_LANGUAGE),
        }
    }
}

impl Default for WakatimeHeartbeatMetadata {
    fn default() -> Self {
        Self {
            project: DEFAULT_HEARTBEAT_PROJECT.to_string(),
            language: DEFAULT_HEARTBEAT_LANGUAGE.to_string(),
        }
    }
}

fn current_unix_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RetryState {
    attempt: u8,
    max_attempts: u8,
    next_backoff_secs: u64,
    error: String,
}

#[derive(Debug, Clone)]
enum HeartbeatEvent {
    Sent,
    Retrying {
        attempt: u8,
        max_attempts: u8,
        next_backoff_secs: u64,
        error: String,
    },
    Failed {
        error: String,
        retryable: bool,
    },
}

/// Returns the machine hostname for the `X-Machine-Name` header.
fn get_hostname() -> String {
    #[cfg(target_os = "windows")]
    {
        std::env::var("COMPUTERNAME").unwrap_or_else(|_| "unknown".to_string())
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOSTNAME")
            .or_else(|_| std::fs::read_to_string("/etc/hostname").map(|s| s.trim().to_string()))
            .unwrap_or_else(|_| "unknown".to_string())
    }
}

/// Reads WakaTime configuration from `~/.wakatime.cfg`.
struct WakatimeConfig {
    api_key: Option<String>,
    api_url: String,
}

impl WakatimeConfig {
    fn load() -> Self {
        let (api_key, api_url) = Self::parse_config_file().unwrap_or((None, None));
        Self {
            api_key,
            api_url: api_url.unwrap_or_else(|| DEFAULT_API_URL.to_string()),
        }
    }

    fn config_file_path() -> Option<PathBuf> {
        let home = dirs_home()?;
        Some(home.join(".wakatime.cfg"))
    }

    /// Parse `~/.wakatime.cfg` as a simple INI file.
    /// Returns `(api_key, api_url)` from the `[settings]` section.
    fn parse_config_file() -> Option<(Option<String>, Option<String>)> {
        let cfg_path = Self::config_file_path()?;
        let content = fs::read_to_string(cfg_path).ok()?;
        Some(Self::parse_config_str(&content))
    }

    /// Parse an INI config string and return `(api_key, api_url)` from `[settings]`.
    fn parse_config_str(content: &str) -> (Option<String>, Option<String>) {
        let mut api_key: Option<String> = None;
        let mut api_url: Option<String> = None;
        let mut in_settings = false;

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed == "[settings]" {
                in_settings = true;
                continue;
            }
            if trimmed.starts_with('[') {
                in_settings = false;
                continue;
            }
            if !in_settings {
                continue;
            }
            if let Some((key, value)) = parse_setting_line(trimmed) {
                match key {
                    "api_key" => api_key = Some(value.to_string()),
                    "api_url" => api_url = Some(value.to_string()),
                    _ => {}
                };
            }
        }

        (api_key, api_url)
    }
}

/// Returns the user's home directory.
fn dirs_home() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .or_else(|_| {
                std::env::var("HOMEDRIVE").and_then(|d| std::env::var("HOMEPATH").map(|p| d + &p))
            })
            .ok()
            .map(std::path::PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").ok().map(std::path::PathBuf::from)
    }
}

/// Tracks WakaTime heartbeats during Focus sessions.
pub struct WakatimeTracker {
    api_key: Option<String>,
    api_url: String,
    /// Seconds elapsed since the last heartbeat was sent.
    secs_since_last_heartbeat: u64,
    /// Whether a focus session is currently being tracked.
    tracking: bool,
    /// Sender/receiver pair for heartbeat outcomes from background worker threads.
    result_tx: Sender<HeartbeatEvent>,
    result_rx: Receiver<HeartbeatEvent>,
    /// Whether a heartbeat request is currently in flight.
    heartbeat_in_flight: bool,
    /// Retry details for the current in-flight heartbeat, if any.
    retry_state: Option<RetryState>,
    /// Last terminal heartbeat failure message.
    last_error: Option<String>,
    /// Unix timestamp (seconds) of the most recent successful heartbeat.
    last_successful_heartbeat_epoch_secs: Option<u64>,
    /// Backlog of unsent heartbeats, oldest first.
    queued_heartbeats: VecDeque<Heartbeat>,
    /// Heartbeat currently being sent in the worker thread.
    in_flight_heartbeat: Option<Heartbeat>,
    /// Whether the in-flight heartbeat originated from the offline queue.
    in_flight_from_queue: bool,
    /// Earliest time at which queued heartbeats should be replayed again.
    queue_retry_not_before_epoch_secs: Option<u64>,
    /// Durable snapshot path for queued heartbeats and replay state.
    queue_snapshot_path: Option<PathBuf>,
    /// Latches an immediate heartbeat request while another worker is in flight.
    pending_immediate_heartbeat: bool,
    heartbeat_metadata: WakatimeHeartbeatMetadata,
    /// Startup-only warning (for example, invalid persisted queue state).
    startup_warning: Option<String>,
    #[cfg(test)]
    disable_network_io: bool,
}

impl WakatimeTracker {
    pub fn new() -> Self {
        Self::new_with_metadata(WakatimeHeartbeatMetadata::default())
    }

    pub fn new_with_metadata(metadata: WakatimeHeartbeatMetadata) -> Self {
        let config = WakatimeConfig::load();
        let (result_tx, result_rx) = mpsc::channel();
        let mut tracker = Self {
            api_key: config.api_key,
            api_url: config.api_url,
            secs_since_last_heartbeat: 0,
            tracking: false,
            result_tx,
            result_rx,
            heartbeat_in_flight: false,
            retry_state: None,
            last_error: None,
            last_successful_heartbeat_epoch_secs: None,
            queued_heartbeats: VecDeque::new(),
            in_flight_heartbeat: None,
            in_flight_from_queue: false,
            queue_retry_not_before_epoch_secs: None,
            queue_snapshot_path: heartbeat_queue_snapshot_path(),
            pending_immediate_heartbeat: false,
            heartbeat_metadata: metadata.normalized(),
            startup_warning: None,
            #[cfg(test)]
            disable_network_io: false,
        };
        tracker.restore_persisted_queue_state();
        tracker
    }

    /// Returns `true` if actively sending heartbeats for a focus session.
    pub fn is_tracking(&self) -> bool {
        self.tracking
    }

    pub fn runtime_state(&self) -> WakatimeRuntimeState {
        if self.api_key.is_none() {
            return WakatimeRuntimeState::NotConfigured;
        }
        if !self.queued_heartbeats.is_empty() && !self.heartbeat_in_flight {
            return WakatimeRuntimeState::Queued {
                pending: self.pending_heartbeat_count(),
            };
        }
        if let Some(retry) = self.retry_state.as_ref() {
            return WakatimeRuntimeState::Retrying {
                attempt: retry.attempt,
                max_attempts: retry.max_attempts,
                next_backoff_secs: retry.next_backoff_secs,
                error: retry.error.clone(),
            };
        }
        if self.heartbeat_in_flight {
            if self.in_flight_from_queue {
                return WakatimeRuntimeState::Replaying {
                    pending: self.pending_heartbeat_count(),
                };
            }
            return WakatimeRuntimeState::Sending;
        }
        if let Some(error) = self.last_error.as_ref() {
            return WakatimeRuntimeState::Error(error.clone());
        }
        if self.tracking {
            WakatimeRuntimeState::Tracking
        } else {
            WakatimeRuntimeState::Idle
        }
    }

    pub fn config_diagnostics() -> WakatimeConfigDiagnostics {
        let Some(config_path) = WakatimeConfig::config_file_path() else {
            return WakatimeConfigDiagnostics {
                config_path: None,
                status: WakatimeConfigStatus::HomeDirectoryUnavailable,
                detail: "WakaTime config check unavailable: home directory not found".to_string(),
            };
        };
        config_diagnostics_from_read_result(config_path.clone(), fs::read_to_string(config_path))
    }

    pub fn last_successful_heartbeat_epoch_secs(&self) -> Option<u64> {
        self.last_successful_heartbeat_epoch_secs
    }

    pub fn pending_heartbeat_count(&self) -> usize {
        self.queued_heartbeats.len()
            + usize::from(self.heartbeat_in_flight && self.in_flight_from_queue)
    }

    fn restore_persisted_queue_state(&mut self) {
        let Some(path) = self.queue_snapshot_path.as_ref() else {
            return;
        };
        match read_heartbeat_queue_snapshot(path) {
            Ok(Some(snapshot)) => {
                self.queued_heartbeats = snapshot.queued_heartbeats.into_iter().fold(
                    VecDeque::new(),
                    |mut queue, heartbeat| {
                        push_back_with_capacity(&mut queue, heartbeat);
                        queue
                    },
                );
                if let Some(in_flight) = snapshot.in_flight_heartbeat {
                    if snapshot.in_flight_from_queue {
                        push_front_with_capacity(&mut self.queued_heartbeats, in_flight);
                    } else {
                        push_back_with_capacity(&mut self.queued_heartbeats, in_flight);
                    }
                }
                self.heartbeat_in_flight = false;
                self.in_flight_heartbeat = None;
                self.in_flight_from_queue = false;
                self.queue_retry_not_before_epoch_secs = if self.queued_heartbeats.is_empty() {
                    None
                } else {
                    snapshot.queue_retry_not_before_epoch_secs
                };
                self.retry_state = None;
            }
            Ok(None) => {}
            Err(error) => {
                let mut warning = format!(
                    "WakaTime offline queue warning: dropped invalid persisted queue ({error})"
                );
                if let Err(clear_error) = clear_heartbeat_queue_snapshot(path) {
                    warning = format!("{warning}; cleanup failed: {clear_error}");
                }
                self.startup_warning = Some(warning.clone());
                self.last_error = Some(warning);
            }
        }
    }

    fn sync_queue_snapshot(&mut self) {
        let Some(path) = self.queue_snapshot_path.as_ref() else {
            return;
        };
        let result = if let Some(snapshot) = self.queue_snapshot() {
            write_heartbeat_queue_snapshot(path, &snapshot)
        } else {
            clear_heartbeat_queue_snapshot(path)
        };
        if let Err(error) = result {
            eprintln!("wakatime queue persistence failed: {error}");
        }
    }

    fn queue_snapshot(&self) -> Option<HeartbeatQueueSnapshot> {
        let has_replay_backlog =
            !self.queued_heartbeats.is_empty() || self.in_flight_heartbeat.is_some();
        if !has_replay_backlog {
            return None;
        }
        Some(HeartbeatQueueSnapshot {
            schema_version: HEARTBEAT_QUEUE_SNAPSHOT_SCHEMA_VERSION,
            queued_heartbeats: self.queued_heartbeats.iter().cloned().collect(),
            in_flight_heartbeat: self.in_flight_heartbeat.clone(),
            in_flight_from_queue: self.heartbeat_in_flight && self.in_flight_from_queue,
            queue_retry_not_before_epoch_secs: self.queue_retry_not_before_epoch_secs,
        })
    }

    /// Drains heartbeat events from worker threads and updates tracker status.
    pub fn poll_events(&mut self) {
        let mut queue_state_changed = false;
        while let Ok(event) = self.result_rx.try_recv() {
            match event {
                HeartbeatEvent::Sent => {
                    self.heartbeat_in_flight = false;
                    self.in_flight_from_queue = false;
                    self.in_flight_heartbeat = None;
                    self.queue_retry_not_before_epoch_secs = None;
                    self.retry_state = None;
                    self.last_error = None;
                    self.last_successful_heartbeat_epoch_secs = Some(current_unix_epoch_secs());
                    queue_state_changed = true;
                }
                HeartbeatEvent::Retrying {
                    attempt,
                    max_attempts,
                    next_backoff_secs,
                    error,
                } => {
                    self.heartbeat_in_flight = true;
                    self.retry_state = Some(RetryState {
                        attempt,
                        max_attempts,
                        next_backoff_secs,
                        error,
                    });
                }
                HeartbeatEvent::Failed { error, retryable } => {
                    self.heartbeat_in_flight = false;
                    self.in_flight_from_queue = false;
                    self.retry_state = None;
                    self.last_error = Some(error);
                    if retryable {
                        self.requeue_in_flight_heartbeat();
                        self.queue_retry_not_before_epoch_secs = Some(
                            current_unix_epoch_secs()
                                .saturating_add(HEARTBEAT_QUEUE_RETRY_DELAY_SECS),
                        );
                    } else {
                        self.in_flight_heartbeat = None;
                    }
                    queue_state_changed = true;
                }
            }
        }
        self.dispatch_pending_work();
        if queue_state_changed {
            self.sync_queue_snapshot();
        }
    }

    /// Called when a focus session starts (timer transitions to Running in Focus phase).
    /// Sends an immediate heartbeat and resets the interval counter.
    /// Does nothing if no API key is configured.
    pub fn on_focus_start(&mut self) {
        if self.api_key.is_none() {
            return;
        }
        self.poll_events();
        self.set_tracking_state(true);
        self.request_heartbeat(true);
    }

    /// Advances the heartbeat counter by `secs` simulated seconds.
    ///
    /// Sends at most one heartbeat per call regardless of how large `secs` is,
    /// so that a burst of catch-up ticks after a suspend/resume does not
    /// trigger multiple rapid HTTP requests.
    pub fn tick_elapsed(&mut self, secs: u64) {
        self.poll_events();
        if !self.tracking || secs == 0 {
            return;
        }
        // Clamp so that a large elapsed value only triggers one heartbeat.
        self.secs_since_last_heartbeat =
            (self.secs_since_last_heartbeat + secs).min(HEARTBEAT_INTERVAL_SECS);
        if self.secs_since_last_heartbeat >= HEARTBEAT_INTERVAL_SECS {
            self.secs_since_last_heartbeat = 0;
            self.request_heartbeat(false);
        }
    }

    /// Called when the focus session pauses, stops, or moves to a break phase.
    pub fn on_focus_stop(&mut self) {
        self.set_tracking_state(false);
    }

    pub fn set_heartbeat_metadata(&mut self, metadata: WakatimeHeartbeatMetadata) {
        self.heartbeat_metadata = metadata.normalized();
    }

    fn set_tracking_state(&mut self, tracking: bool) {
        self.tracking = tracking;
        self.secs_since_last_heartbeat = 0;
        if !tracking {
            self.pending_immediate_heartbeat = false;
        }
    }

    /// Captures a heartbeat request and either dispatches immediately or queues it.
    fn request_heartbeat(&mut self, immediate: bool) {
        if self.api_key.is_none() {
            return;
        }
        if self.heartbeat_in_flight {
            if immediate {
                self.pending_immediate_heartbeat = true;
            }
            return;
        }

        let heartbeat = self.build_heartbeat_payload_for_now();
        if !self.queued_heartbeats.is_empty() {
            self.enqueue_heartbeat(heartbeat);
            self.dispatch_pending_work();
            return;
        }

        self.dispatch_heartbeat(heartbeat, false);
    }

    fn build_heartbeat_payload_for_now(&self) -> Heartbeat {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);

        build_heartbeat_payload(now, &self.heartbeat_metadata)
    }

    fn enqueue_heartbeat(&mut self, heartbeat: Heartbeat) {
        push_back_with_capacity(&mut self.queued_heartbeats, heartbeat);
        self.sync_queue_snapshot();
    }

    fn requeue_in_flight_heartbeat(&mut self) {
        let Some(in_flight_heartbeat) = self.in_flight_heartbeat.take() else {
            return;
        };
        push_front_with_capacity(&mut self.queued_heartbeats, in_flight_heartbeat);
    }

    fn queue_retry_is_due(&self) -> bool {
        self.queue_retry_not_before_epoch_secs
            .is_none_or(|not_before| current_unix_epoch_secs() >= not_before)
    }

    fn dispatch_pending_work(&mut self) {
        if self.api_key.is_none() || self.heartbeat_in_flight {
            return;
        }
        if self.dispatch_next_queued_heartbeat_if_due() {
            return;
        }
        self.dispatch_pending_immediate_heartbeat();
    }

    fn dispatch_next_queued_heartbeat_if_due(&mut self) -> bool {
        if !self.queue_retry_is_due() {
            return false;
        }
        let Some(heartbeat) = self.queued_heartbeats.pop_front() else {
            return false;
        };
        if self.api_key.is_none() {
            return false;
        }
        self.dispatch_heartbeat(heartbeat, true);
        true
    }

    /// Spawns a background thread to send a heartbeat to the WakaTime API.
    /// Retries transient failures with bounded exponential backoff.
    fn dispatch_heartbeat(&mut self, heartbeat: Heartbeat, from_queue: bool) {
        let Some(api_key) = self.api_key.clone() else {
            return;
        };
        self.heartbeat_in_flight = true;
        self.in_flight_heartbeat = Some(heartbeat.clone());
        self.in_flight_from_queue = from_queue;
        self.queue_retry_not_before_epoch_secs = None;
        self.retry_state = None;
        self.last_error = None;
        self.sync_queue_snapshot();

        #[cfg(test)]
        if self.disable_network_io {
            return;
        }

        let auth = format!("Basic {}", BASE64.encode(api_key.as_bytes()));
        let api_url = self.api_url.trim_end_matches('/');
        let url = format!("{}/api/v1/users/current/heartbeats", api_url);

        let plugin_version = env!("CARGO_PKG_VERSION");
        let os = std::env::consts::OS;
        let user_agent = format!(
            "wakatime/unset ({os}) focustime/{plugin_version} focustime-wakatime/{plugin_version}"
        );
        let hostname = get_hostname();
        let result_tx = self.result_tx.clone();

        std::thread::spawn(move || {
            send_heartbeat_with_retries(result_tx, url, auth, user_agent, hostname, heartbeat);
        });
    }

    fn dispatch_pending_immediate_heartbeat(&mut self) {
        if self.pending_immediate_heartbeat && self.tracking {
            self.pending_immediate_heartbeat = false;
            self.request_heartbeat(true);
        }
    }

    #[cfg(test)]
    pub(crate) fn new_configured_for_tests() -> Self {
        let (result_tx, result_rx) = mpsc::channel();
        Self {
            api_key: Some("test-key".to_string()),
            api_url: DEFAULT_API_URL.to_string(),
            secs_since_last_heartbeat: 0,
            tracking: false,
            result_tx,
            result_rx,
            heartbeat_in_flight: false,
            retry_state: None,
            last_error: None,
            last_successful_heartbeat_epoch_secs: None,
            queued_heartbeats: VecDeque::new(),
            in_flight_heartbeat: None,
            in_flight_from_queue: false,
            queue_retry_not_before_epoch_secs: None,
            queue_snapshot_path: None,
            pending_immediate_heartbeat: false,
            heartbeat_metadata: WakatimeHeartbeatMetadata::default(),
            startup_warning: None,
            disable_network_io: true,
        }
    }

    #[cfg(test)]
    pub(crate) fn new_unconfigured_for_tests() -> Self {
        let (result_tx, result_rx) = mpsc::channel();
        Self {
            api_key: None,
            api_url: DEFAULT_API_URL.to_string(),
            secs_since_last_heartbeat: 0,
            tracking: false,
            result_tx,
            result_rx,
            heartbeat_in_flight: false,
            retry_state: None,
            last_error: None,
            last_successful_heartbeat_epoch_secs: None,
            queued_heartbeats: VecDeque::new(),
            in_flight_heartbeat: None,
            in_flight_from_queue: false,
            queue_retry_not_before_epoch_secs: None,
            queue_snapshot_path: None,
            pending_immediate_heartbeat: false,
            heartbeat_metadata: WakatimeHeartbeatMetadata::default(),
            startup_warning: None,
            disable_network_io: true,
        }
    }

    #[cfg(test)]
    pub(crate) fn push_sent_event_for_tests(&self) {
        let _ = self.result_tx.send(HeartbeatEvent::Sent);
    }

    #[cfg(test)]
    pub(crate) fn push_failed_event_for_tests(&self, error: impl Into<String>) {
        let _ = self.result_tx.send(HeartbeatEvent::Failed {
            error: error.into(),
            retryable: false,
        });
    }

    #[cfg(test)]
    pub(crate) fn heartbeat_metadata_for_tests(&self) -> WakatimeHeartbeatMetadata {
        self.heartbeat_metadata.clone()
    }

    #[cfg(test)]
    pub(crate) fn set_pending_heartbeats_for_tests(&mut self, pending: usize) {
        self.queued_heartbeats.clear();
        for index in 0..pending {
            self.queued_heartbeats.push_back(build_heartbeat_payload(
                index as f64,
                &self.heartbeat_metadata,
            ));
        }
        self.heartbeat_in_flight = false;
        self.in_flight_heartbeat = None;
        self.in_flight_from_queue = false;
        self.queue_retry_not_before_epoch_secs = None;
        self.pending_immediate_heartbeat = false;
        self.retry_state = None;
        self.last_error = None;
    }

    #[cfg(test)]
    pub(crate) fn set_replaying_heartbeats_for_tests(&mut self, pending: usize) {
        self.set_pending_heartbeats_for_tests(pending);
        if pending == 0 {
            return;
        }
        let in_flight = self
            .queued_heartbeats
            .pop_front()
            .unwrap_or_else(|| build_heartbeat_payload(0.0, &self.heartbeat_metadata));
        self.heartbeat_in_flight = true;
        self.in_flight_heartbeat = Some(in_flight);
        self.in_flight_from_queue = true;
    }
}

fn send_heartbeat_with_retries(
    result_tx: Sender<HeartbeatEvent>,
    url: String,
    auth: String,
    user_agent: String,
    hostname: String,
    heartbeat: Heartbeat,
) {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .into();
    let mut attempt: u8 = 1;

    loop {
        let result = agent
            .post(&url)
            .header("Authorization", &auth)
            .header("Content-Type", "application/json")
            .header("User-Agent", &user_agent)
            .header("X-Machine-Name", &hostname)
            .send_json(&heartbeat);

        match result {
            Ok(_) => {
                let _ = result_tx.send(HeartbeatEvent::Sent);
                return;
            }
            Err(error) => {
                let error_message = format_heartbeat_error(&error);
                let retryable = is_retryable_error(&error);
                let backoff_index = attempt.saturating_sub(1) as usize;
                if retryable
                    && let Some(backoff_secs) = HEARTBEAT_RETRY_BACKOFF_SECS.get(backoff_index)
                {
                    let _ = result_tx.send(HeartbeatEvent::Retrying {
                        attempt,
                        max_attempts: HEARTBEAT_MAX_ATTEMPTS,
                        next_backoff_secs: *backoff_secs,
                        error: error_message.clone(),
                    });
                    std::thread::sleep(Duration::from_secs(*backoff_secs));
                    attempt = attempt.saturating_add(1);
                    continue;
                }

                let _ = result_tx.send(HeartbeatEvent::Failed {
                    error: error_message,
                    retryable,
                });
                return;
            }
        }
    }
}

fn is_retryable_error(error: &ureq::Error) -> bool {
    match error {
        ureq::Error::StatusCode(code) => *code == 429 || (500..=599).contains(code),
        ureq::Error::Io(_)
        | ureq::Error::Timeout(_)
        | ureq::Error::HostNotFound
        | ureq::Error::ConnectionFailed => true,
        _ => false,
    }
}

fn format_heartbeat_error(error: &ureq::Error) -> String {
    match error {
        ureq::Error::StatusCode(code) => format!("HTTP {code}"),
        _ => error.to_string(),
    }
}

impl Default for WakatimeTracker {
    fn default() -> Self {
        Self::new()
    }
}

fn heartbeat_queue_snapshot_path() -> Option<PathBuf> {
    crate::config::app_data_path(HEARTBEAT_QUEUE_SNAPSHOT_FILE_NAME)
}

fn read_heartbeat_queue_snapshot(path: &Path) -> io::Result<Option<HeartbeatQueueSnapshot>> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let snapshot: HeartbeatQueueSnapshot = toml::from_str(&content).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid queue snapshot format: {error}"),
        )
    })?;
    if snapshot.schema_version != HEARTBEAT_QUEUE_SNAPSHOT_SCHEMA_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "unsupported queue snapshot schema {}",
                snapshot.schema_version
            ),
        ));
    }
    Ok(Some(snapshot))
}

fn write_heartbeat_queue_snapshot(
    path: &Path,
    snapshot: &HeartbeatQueueSnapshot,
) -> io::Result<()> {
    let content = toml::to_string_pretty(snapshot)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    write_atomic_text(path, &content)
}

fn clear_heartbeat_queue_snapshot(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn write_atomic_text(path: &Path, content: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp_path = path.with_extension("toml.tmp");
    fs::write(&tmp_path, content)?;
    #[cfg(target_os = "windows")]
    {
        match fs::rename(&tmp_path, path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                fs::remove_file(path)?;
                fs::rename(&tmp_path, path)
            }
            Err(error) => {
                let _ = fs::remove_file(&tmp_path);
                Err(error)
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        match fs::rename(&tmp_path, path) {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = fs::remove_file(&tmp_path);
                Err(error)
            }
        }
    }
}

fn push_back_with_capacity(queue: &mut VecDeque<Heartbeat>, heartbeat: Heartbeat) {
    if queue.len() >= HEARTBEAT_QUEUE_CAPACITY {
        let _ = queue.pop_front();
    }
    queue.push_back(heartbeat);
}

fn push_front_with_capacity(queue: &mut VecDeque<Heartbeat>, heartbeat: Heartbeat) {
    if queue.len() >= HEARTBEAT_QUEUE_CAPACITY {
        let _ = queue.pop_front();
    }
    queue.push_front(heartbeat);
}

fn build_heartbeat_payload(now: f64, metadata: &WakatimeHeartbeatMetadata) -> Heartbeat {
    let metadata = metadata.normalized();
    Heartbeat {
        entity: DEFAULT_HEARTBEAT_ENTITY.to_string(),
        entity_type: "app".to_string(),
        time: now,
        project: metadata.project,
        language: metadata.language,
        is_write: false,
    }
}

fn parse_setting_line(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once('=')?;
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    Some((key.trim(), value))
}

fn normalize_nonempty_or_default(value: &str, default: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

fn config_diagnostics_from_read_result(
    config_path: PathBuf,
    read_result: io::Result<String>,
) -> WakatimeConfigDiagnostics {
    let config_path_text = config_path.display().to_string();
    match read_result {
        Ok(content) => {
            let (api_key, _) = WakatimeConfig::parse_config_str(&content);
            if api_key.is_some() {
                WakatimeConfigDiagnostics {
                    config_path: Some(config_path_text.clone()),
                    status: WakatimeConfigStatus::Configured,
                    detail: format!("Configured ({config_path_text})"),
                }
            } else {
                WakatimeConfigDiagnostics {
                    config_path: Some(config_path_text.clone()),
                    status: WakatimeConfigStatus::MissingApiKey,
                    detail: format!(
                        "Config found at {config_path_text}, but [settings].api_key is missing"
                    ),
                }
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => WakatimeConfigDiagnostics {
            config_path: Some(config_path_text.clone()),
            status: WakatimeConfigStatus::MissingConfigFile,
            detail: format!("Config file not found ({config_path_text})"),
        },
        Err(error) => WakatimeConfigDiagnostics {
            config_path: Some(config_path_text),
            status: WakatimeConfigStatus::UnreadableConfig,
            detail: format!("Unable to read WakaTime config: {error}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use crate::wakatime::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_queue_snapshot_path(test_name: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time must be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "focustime-wakatime-queue-{test_name}-{}-{now}.toml",
            std::process::id()
        ))
    }

    fn tracker_with(
        api_key: Option<&str>,
        tracking: bool,
        secs_since_last_heartbeat: u64,
    ) -> WakatimeTracker {
        let (result_tx, result_rx) = mpsc::channel();
        WakatimeTracker {
            api_key: api_key.map(str::to_string),
            api_url: DEFAULT_API_URL.to_string(),
            secs_since_last_heartbeat,
            tracking,
            result_tx,
            result_rx,
            heartbeat_in_flight: false,
            retry_state: None,
            last_error: None,
            last_successful_heartbeat_epoch_secs: None,
            queued_heartbeats: VecDeque::new(),
            in_flight_heartbeat: None,
            in_flight_from_queue: false,
            queue_retry_not_before_epoch_secs: None,
            queue_snapshot_path: None,
            pending_immediate_heartbeat: false,
            heartbeat_metadata: WakatimeHeartbeatMetadata::default(),
            startup_warning: None,
            disable_network_io: true,
        }
    }

    #[test]
    fn heartbeat_payload_uses_configured_metadata() {
        let metadata = WakatimeHeartbeatMetadata {
            project: "Team Focus".to_string(),
            language: "Deep Work".to_string(),
        };

        let payload = build_heartbeat_payload(123.0, &metadata);

        assert_eq!(payload.entity, DEFAULT_HEARTBEAT_ENTITY.to_string());
        assert_eq!(payload.entity_type, "app");
        assert_eq!(payload.time, 123.0);
        assert_eq!(payload.project, "Team Focus");
        assert_eq!(payload.language, "Deep Work");
        assert!(!payload.is_write);
    }

    #[test]
    fn heartbeat_payload_normalizes_blank_metadata_to_defaults() {
        let metadata = WakatimeHeartbeatMetadata {
            project: "   ".to_string(),
            language: "".to_string(),
        };

        let payload = build_heartbeat_payload(123.0, &metadata);

        assert_eq!(payload.project, DEFAULT_HEARTBEAT_PROJECT);
        assert_eq!(payload.language, DEFAULT_HEARTBEAT_LANGUAGE);
    }

    #[test]
    fn parse_config_extracts_api_key() {
        let (api_key, api_url) =
            WakatimeConfig::parse_config_str("[settings]\napi_key = test-key-123\n");
        assert_eq!(api_key, Some("test-key-123".to_string()));
        assert_eq!(api_url, None);
    }

    #[test]
    fn parse_config_extracts_api_url() {
        let (api_key, api_url) = WakatimeConfig::parse_config_str(
            "[settings]\napi_key = mykey\napi_url = https://wakatime.example.com\n",
        );
        assert_eq!(api_key, Some("mykey".to_string()));
        assert_eq!(api_url, Some("https://wakatime.example.com".to_string()));
    }

    #[test]
    fn parse_config_ignores_other_sections() {
        let (api_key, _) = WakatimeConfig::parse_config_str(
            "[other]\napi_key = wrong\n[settings]\napi_key = correct\n",
        );
        assert_eq!(api_key, Some("correct".to_string()));
    }

    #[test]
    fn config_diagnostics_reports_configured_state() {
        let diagnostics = config_diagnostics_from_read_result(
            PathBuf::from(".wakatime.cfg"),
            Ok("[settings]\napi_key = test-key\n".to_string()),
        );
        assert_eq!(diagnostics.status, WakatimeConfigStatus::Configured);
        assert!(diagnostics.detail.contains("Configured"));
    }

    #[test]
    fn config_diagnostics_reports_missing_api_key() {
        let diagnostics = config_diagnostics_from_read_result(
            PathBuf::from(".wakatime.cfg"),
            Ok("[settings]\napi_url = https://wakatime.example.com\n".to_string()),
        );
        assert_eq!(diagnostics.status, WakatimeConfigStatus::MissingApiKey);
        assert!(diagnostics.detail.contains("api_key is missing"));
    }

    #[test]
    fn config_diagnostics_reports_missing_file() {
        let diagnostics = config_diagnostics_from_read_result(
            PathBuf::from(".wakatime.cfg"),
            Err(io::Error::new(io::ErrorKind::NotFound, "not found")),
        );
        assert_eq!(diagnostics.status, WakatimeConfigStatus::MissingConfigFile);
        assert!(diagnostics.detail.contains("not found"));
    }

    #[test]
    fn tracker_not_configured_when_no_api_key() {
        let tracker = tracker_with(None, false, 0);
        assert!(!tracker.is_tracking());
        assert_eq!(tracker.runtime_state(), WakatimeRuntimeState::NotConfigured);
    }

    #[test]
    fn tracker_configured_when_api_key_present() {
        let tracker = tracker_with(Some("test-key"), false, 0);
        assert_eq!(tracker.runtime_state(), WakatimeRuntimeState::Idle);
    }

    #[test]
    fn on_focus_start_does_not_track_without_api_key() {
        let mut tracker = tracker_with(None, false, 50);
        tracker.on_focus_start();
        assert!(!tracker.is_tracking());
        assert_eq!(tracker.secs_since_last_heartbeat, 50);
    }

    #[test]
    fn on_focus_start_sets_tracking_when_configured() {
        let mut tracker = tracker_with(Some("test-key"), false, 50);
        tracker.on_focus_start();
        assert!(tracker.is_tracking());
        assert_eq!(tracker.secs_since_last_heartbeat, 0);
    }

    #[test]
    fn on_focus_stop_clears_tracking() {
        let mut tracker = tracker_with(None, true, 60);
        tracker.pending_immediate_heartbeat = true;
        tracker.on_focus_stop();
        assert!(!tracker.is_tracking());
        assert_eq!(tracker.secs_since_last_heartbeat, 0);
        assert!(!tracker.pending_immediate_heartbeat);
    }

    #[test]
    fn set_heartbeat_metadata_normalizes_and_updates_values() {
        let mut tracker = tracker_with(None, false, 0);
        tracker.set_heartbeat_metadata(WakatimeHeartbeatMetadata {
            project: " Team Focus ".to_string(),
            language: " ".to_string(),
        });
        assert_eq!(
            tracker.heartbeat_metadata,
            WakatimeHeartbeatMetadata {
                project: "Team Focus".to_string(),
                language: DEFAULT_HEARTBEAT_LANGUAGE.to_string(),
            }
        );
    }

    #[test]
    fn tick_increments_counter_and_resets_at_interval() {
        let mut tracker = tracker_with(None, true, HEARTBEAT_INTERVAL_SECS - 1);
        tracker.tick_elapsed(1);
        // Counter should have reset after reaching the interval threshold
        assert_eq!(tracker.secs_since_last_heartbeat, 0);
    }

    #[test]
    fn tick_does_nothing_when_not_tracking() {
        let mut tracker = tracker_with(None, false, 0);
        tracker.tick_elapsed(1);
        assert_eq!(tracker.secs_since_last_heartbeat, 0);
    }

    #[test]
    fn tick_elapsed_clamps_to_single_heartbeat_on_burst() {
        let mut tracker = tracker_with(None, true, 0);
        // Simulate 10 minutes of catch-up at once; should only fire one heartbeat
        tracker.tick_elapsed(600);
        assert_eq!(tracker.secs_since_last_heartbeat, 0);
    }

    #[test]
    fn on_focus_start_latches_immediate_heartbeat_when_inflight() {
        let mut tracker = tracker_with(Some("test-key"), false, 0);
        tracker.heartbeat_in_flight = true;
        tracker.retry_state = Some(RetryState {
            attempt: 1,
            max_attempts: HEARTBEAT_MAX_ATTEMPTS,
            next_backoff_secs: 1,
            error: "HTTP 503".to_string(),
        });

        tracker.on_focus_start();

        assert!(tracker.tracking);
        assert!(tracker.heartbeat_in_flight);
        assert!(tracker.pending_immediate_heartbeat);
    }

    #[test]
    fn pending_immediate_heartbeat_dispatches_after_inflight_send_completes() {
        let mut tracker = tracker_with(Some("test-key"), true, 0);
        tracker.heartbeat_in_flight = true;
        tracker.pending_immediate_heartbeat = true;

        tracker
            .result_tx
            .send(HeartbeatEvent::Sent)
            .expect("test event send must succeed");

        tracker.poll_events();

        assert!(tracker.heartbeat_in_flight);
        assert!(!tracker.pending_immediate_heartbeat);
    }

    #[test]
    fn pending_immediate_heartbeat_dispatches_after_inflight_failure() {
        let mut tracker = tracker_with(Some("test-key"), true, 0);
        tracker.heartbeat_in_flight = true;
        tracker.pending_immediate_heartbeat = true;

        tracker
            .result_tx
            .send(HeartbeatEvent::Failed {
                error: "HTTP 503".to_string(),
                retryable: false,
            })
            .expect("test event send must succeed");

        tracker.poll_events();

        assert!(tracker.heartbeat_in_flight);
        assert!(!tracker.pending_immediate_heartbeat);
    }

    #[test]
    fn retryable_failure_requeues_heartbeat_for_replay() {
        let mut tracker = tracker_with(Some("test-key"), true, 0);
        tracker.request_heartbeat(true);
        assert!(tracker.heartbeat_in_flight);

        tracker
            .result_tx
            .send(HeartbeatEvent::Failed {
                error: "network unavailable".to_string(),
                retryable: true,
            })
            .expect("test event send must succeed");

        tracker.poll_events();

        assert_eq!(tracker.pending_heartbeat_count(), 1);
        assert!(matches!(
            tracker.runtime_state(),
            WakatimeRuntimeState::Replaying { pending: 1 }
        ));
    }

    #[test]
    fn queued_heartbeats_take_priority_over_new_requests() {
        let mut tracker = tracker_with(Some("test-key"), true, 0);
        tracker.set_pending_heartbeats_for_tests(2);

        tracker.on_focus_start();

        assert!(matches!(
            tracker.runtime_state(),
            WakatimeRuntimeState::Replaying { pending: 2 }
        ));
        assert_eq!(tracker.pending_heartbeat_count(), 2);
    }

    #[test]
    fn queue_capacity_drops_oldest_entries_when_full() {
        let mut tracker = tracker_with(Some("test-key"), true, 0);
        for index in 0..(HEARTBEAT_QUEUE_CAPACITY + 5) {
            tracker.enqueue_heartbeat(build_heartbeat_payload(
                index as f64,
                &tracker.heartbeat_metadata,
            ));
        }
        assert_eq!(tracker.queued_heartbeats.len(), HEARTBEAT_QUEUE_CAPACITY);
        let oldest = tracker
            .queued_heartbeats
            .front()
            .expect("queue should have oldest heartbeat after capping");
        assert_eq!(oldest.time, 5.0);
    }

    #[test]
    fn requeue_overflow_drops_oldest_queued_heartbeat() {
        let mut tracker = tracker_with(Some("test-key"), true, 0);
        tracker.queued_heartbeats = (0..HEARTBEAT_QUEUE_CAPACITY)
            .map(|index| build_heartbeat_payload(index as f64, &tracker.heartbeat_metadata))
            .collect();
        tracker.in_flight_heartbeat =
            Some(build_heartbeat_payload(999.0, &tracker.heartbeat_metadata));

        tracker.requeue_in_flight_heartbeat();

        assert_eq!(tracker.queued_heartbeats.len(), HEARTBEAT_QUEUE_CAPACITY);
        assert_eq!(
            tracker
                .queued_heartbeats
                .front()
                .expect("in-flight heartbeat should be first")
                .time,
            999.0
        );
        assert_eq!(
            tracker
                .queued_heartbeats
                .get(1)
                .expect("oldest queued heartbeat should be evicted")
                .time,
            1.0
        );
        assert_eq!(
            tracker
                .queued_heartbeats
                .back()
                .expect("latest queued heartbeat should remain")
                .time,
            (HEARTBEAT_QUEUE_CAPACITY - 1) as f64
        );
    }

    #[test]
    fn runtime_state_tracking_when_configured_and_running() {
        let mut tracker = tracker_with(Some("test-key"), true, 0);
        tracker.poll_events();
        assert_eq!(tracker.runtime_state(), WakatimeRuntimeState::Tracking);
    }

    #[test]
    fn runtime_state_updates_to_retrying_from_worker_event() {
        let mut tracker = tracker_with(Some("test-key"), true, 0);
        tracker
            .result_tx
            .send(HeartbeatEvent::Retrying {
                attempt: 1,
                max_attempts: HEARTBEAT_MAX_ATTEMPTS,
                next_backoff_secs: 1,
                error: "HTTP 503".to_string(),
            })
            .expect("test event send must succeed");

        tracker.poll_events();

        assert_eq!(
            tracker.runtime_state(),
            WakatimeRuntimeState::Retrying {
                attempt: 1,
                max_attempts: HEARTBEAT_MAX_ATTEMPTS,
                next_backoff_secs: 1,
                error: "HTTP 503".to_string(),
            }
        );
    }

    #[test]
    fn runtime_state_updates_to_error_after_failure_event() {
        let mut tracker = tracker_with(Some("test-key"), true, 0);
        tracker
            .result_tx
            .send(HeartbeatEvent::Failed {
                error: "HTTP 500".to_string(),
                retryable: false,
            })
            .expect("test event send must succeed");

        tracker.poll_events();

        assert_eq!(
            tracker.runtime_state(),
            WakatimeRuntimeState::Error("HTTP 500".to_string())
        );
    }

    #[test]
    fn success_event_clears_previous_error_state() {
        let mut tracker = tracker_with(Some("test-key"), true, 0);
        tracker
            .result_tx
            .send(HeartbeatEvent::Failed {
                error: "io: network unreachable".to_string(),
                retryable: false,
            })
            .expect("test event send must succeed");
        tracker.poll_events();
        assert!(matches!(
            tracker.runtime_state(),
            WakatimeRuntimeState::Error(_)
        ));

        tracker
            .result_tx
            .send(HeartbeatEvent::Sent)
            .expect("test event send must succeed");
        tracker.poll_events();

        assert_eq!(tracker.runtime_state(), WakatimeRuntimeState::Tracking);
        assert!(tracker.last_successful_heartbeat_epoch_secs.is_some());
    }

    #[test]
    fn success_event_records_last_success_timestamp() {
        let mut tracker = tracker_with(Some("test-key"), true, 0);
        assert!(tracker.last_successful_heartbeat_epoch_secs.is_none());

        tracker
            .result_tx
            .send(HeartbeatEvent::Sent)
            .expect("test event send must succeed");
        tracker.poll_events();

        assert!(tracker.last_successful_heartbeat_epoch_secs.is_some());
    }

    #[test]
    fn failure_event_preserves_last_success_timestamp() {
        let mut tracker = tracker_with(Some("test-key"), true, 0);
        tracker.last_successful_heartbeat_epoch_secs = Some(123);

        tracker
            .result_tx
            .send(HeartbeatEvent::Failed {
                error: "HTTP 500".to_string(),
                retryable: false,
            })
            .expect("test event send must succeed");
        tracker.poll_events();

        assert_eq!(tracker.last_successful_heartbeat_epoch_secs, Some(123));
    }

    #[test]
    fn retryable_error_classification_matches_policy() {
        assert!(is_retryable_error(&ureq::Error::StatusCode(429)));
        assert!(is_retryable_error(&ureq::Error::StatusCode(503)));
        assert!(!is_retryable_error(&ureq::Error::StatusCode(400)));
        assert!(is_retryable_error(&ureq::Error::ConnectionFailed));
        assert!(!is_retryable_error(&ureq::Error::BadUri(
            "missing-host".to_string()
        )));
    }

    #[test]
    fn queue_snapshot_restores_pending_backlog_after_restart() {
        let snapshot_path = unique_temp_queue_snapshot_path("restore-pending");
        let mut tracker = WakatimeTracker::new_configured_for_tests();
        tracker.queue_snapshot_path = Some(snapshot_path.clone());
        tracker.set_pending_heartbeats_for_tests(3);
        tracker.sync_queue_snapshot();

        let mut restored = WakatimeTracker::new_configured_for_tests();
        restored.queue_snapshot_path = Some(snapshot_path.clone());
        restored.restore_persisted_queue_state();

        assert_eq!(restored.pending_heartbeat_count(), 3);
        assert!(matches!(
            restored.runtime_state(),
            WakatimeRuntimeState::Queued { pending: 3 }
        ));

        let _ = fs::remove_file(snapshot_path);
    }

    #[test]
    fn queue_snapshot_restores_in_flight_replay_heartbeat_at_front() {
        let snapshot_path = unique_temp_queue_snapshot_path("restore-in-flight");
        let mut tracker = WakatimeTracker::new_configured_for_tests();
        tracker.queue_snapshot_path = Some(snapshot_path.clone());
        tracker.set_replaying_heartbeats_for_tests(3);
        tracker.sync_queue_snapshot();

        let mut restored = WakatimeTracker::new_configured_for_tests();
        restored.queue_snapshot_path = Some(snapshot_path.clone());
        restored.restore_persisted_queue_state();

        assert_eq!(restored.pending_heartbeat_count(), 3);
        assert_eq!(
            restored
                .queued_heartbeats
                .front()
                .expect("restored queue should have first heartbeat")
                .time,
            0.0
        );
        assert_eq!(
            restored
                .queued_heartbeats
                .get(1)
                .expect("restored queue should have second heartbeat")
                .time,
            1.0
        );
        assert_eq!(
            restored
                .queued_heartbeats
                .get(2)
                .expect("restored queue should have third heartbeat")
                .time,
            2.0
        );

        let _ = fs::remove_file(snapshot_path);
    }

    #[test]
    fn queue_snapshot_restore_is_bounded_by_queue_capacity() {
        let snapshot_path = unique_temp_queue_snapshot_path("restore-capacity");
        let oversized = HeartbeatQueueSnapshot {
            schema_version: HEARTBEAT_QUEUE_SNAPSHOT_SCHEMA_VERSION,
            queued_heartbeats: (0..(HEARTBEAT_QUEUE_CAPACITY + 5))
                .map(|index| {
                    build_heartbeat_payload(index as f64, &WakatimeHeartbeatMetadata::default())
                })
                .collect(),
            in_flight_heartbeat: None,
            in_flight_from_queue: false,
            queue_retry_not_before_epoch_secs: None,
        };
        write_heartbeat_queue_snapshot(&snapshot_path, &oversized)
            .expect("oversized queue snapshot should be written");

        let mut restored = WakatimeTracker::new_configured_for_tests();
        restored.queue_snapshot_path = Some(snapshot_path.clone());
        restored.restore_persisted_queue_state();

        assert_eq!(restored.queued_heartbeats.len(), HEARTBEAT_QUEUE_CAPACITY);
        assert_eq!(
            restored
                .queued_heartbeats
                .front()
                .expect("restored bounded queue should keep newest capacity window")
                .time,
            5.0
        );

        let _ = fs::remove_file(snapshot_path);
    }

    #[test]
    fn invalid_queue_snapshot_is_dropped_and_warning_is_exposed() {
        let snapshot_path = unique_temp_queue_snapshot_path("restore-invalid");
        fs::write(&snapshot_path, "not-valid = [this is invalid toml")
            .expect("invalid snapshot fixture should be written");

        let mut tracker = WakatimeTracker::new_configured_for_tests();
        tracker.queue_snapshot_path = Some(snapshot_path.clone());
        tracker.restore_persisted_queue_state();

        assert!(
            tracker
                .last_error
                .as_deref()
                .is_some_and(|message| message.contains("dropped invalid persisted queue"))
        );
        assert!(
            tracker
                .startup_warning
                .as_deref()
                .is_some_and(|message| message.contains("dropped invalid persisted queue"))
        );
        assert!(!snapshot_path.exists());
    }

    #[test]
    fn sent_event_clears_queue_snapshot_when_backlog_is_drained() {
        let snapshot_path = unique_temp_queue_snapshot_path("sent-clears-snapshot");
        let mut tracker = WakatimeTracker::new_configured_for_tests();
        tracker.queue_snapshot_path = Some(snapshot_path.clone());
        tracker.set_replaying_heartbeats_for_tests(1);
        tracker.sync_queue_snapshot();
        assert!(snapshot_path.exists());

        tracker.push_sent_event_for_tests();
        tracker.poll_events();

        assert!(!snapshot_path.exists());
    }
}
