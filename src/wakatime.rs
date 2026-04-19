use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::Serialize;

const HEARTBEAT_INTERVAL_SECS: u64 = 120;
const HEARTBEAT_RETRY_BACKOFF_SECS: [u64; 2] = [1, 2];
const HEARTBEAT_MAX_ATTEMPTS: u8 = 3;
const DEFAULT_API_URL: &str = "https://wakatime.com";
const DEFAULT_HEARTBEAT_ENTITY: &str = "focustime";
const DEFAULT_HEARTBEAT_PROJECT: &str = "focustime";
const DEFAULT_HEARTBEAT_LANGUAGE: &str = "Pomodoro";

#[derive(Debug, Serialize)]
struct Heartbeat {
    entity: String,
    #[serde(rename = "type")]
    entity_type: String,
    time: f64,
    project: String,
    language: String,
    is_write: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WakatimeRuntimeState {
    NotConfigured,
    Idle,
    Tracking,
    Sending,
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
    /// Latches an immediate heartbeat request while another worker is in flight.
    pending_immediate_heartbeat: bool,
    heartbeat_metadata: WakatimeHeartbeatMetadata,
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
        Self {
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
            pending_immediate_heartbeat: false,
            heartbeat_metadata: metadata.normalized(),
            #[cfg(test)]
            disable_network_io: false,
        }
    }

    /// Returns `true` if actively sending heartbeats for a focus session.
    pub fn is_tracking(&self) -> bool {
        self.tracking
    }

    pub fn runtime_state(&self) -> WakatimeRuntimeState {
        if self.api_key.is_none() {
            return WakatimeRuntimeState::NotConfigured;
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

    /// Drains heartbeat events from worker threads and updates tracker status.
    pub fn poll_events(&mut self) {
        while let Ok(event) = self.result_rx.try_recv() {
            match event {
                HeartbeatEvent::Sent => {
                    self.heartbeat_in_flight = false;
                    self.retry_state = None;
                    self.last_error = None;
                    self.last_successful_heartbeat_epoch_secs = Some(current_unix_epoch_secs());
                    self.dispatch_pending_immediate_heartbeat();
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
                HeartbeatEvent::Failed { error } => {
                    self.heartbeat_in_flight = false;
                    self.retry_state = None;
                    self.last_error = Some(error);
                    self.dispatch_pending_immediate_heartbeat();
                }
            }
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
        self.queue_heartbeat_async(true);
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
            self.queue_heartbeat_async(false);
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
    }

    /// Spawns a background thread to send a heartbeat to the WakaTime API.
    /// Retries transient failures with bounded exponential backoff.
    fn queue_heartbeat_async(&mut self, immediate: bool) {
        let Some(ref api_key) = self.api_key else {
            return;
        };
        if self.heartbeat_in_flight {
            if immediate {
                self.pending_immediate_heartbeat = true;
            }
            return;
        }
        self.heartbeat_in_flight = true;
        self.retry_state = None;
        self.last_error = None;

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

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);

        let heartbeat = build_heartbeat_payload(now, &self.heartbeat_metadata);
        let result_tx = self.result_tx.clone();

        std::thread::spawn(move || {
            send_heartbeat_with_retries(result_tx, url, auth, user_agent, hostname, heartbeat);
        });
    }

    fn dispatch_pending_immediate_heartbeat(&mut self) {
        if self.pending_immediate_heartbeat && self.tracking {
            self.pending_immediate_heartbeat = false;
            self.queue_heartbeat_async(true);
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
            pending_immediate_heartbeat: false,
            heartbeat_metadata: WakatimeHeartbeatMetadata::default(),
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
            pending_immediate_heartbeat: false,
            heartbeat_metadata: WakatimeHeartbeatMetadata::default(),
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
        });
    }

    #[cfg(test)]
    pub(crate) fn heartbeat_metadata_for_tests(&self) -> WakatimeHeartbeatMetadata {
        self.heartbeat_metadata.clone()
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
                let backoff_index = attempt.saturating_sub(1) as usize;
                if is_retryable_error(&error)
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
    use super::*;

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
            pending_immediate_heartbeat: false,
            heartbeat_metadata: WakatimeHeartbeatMetadata::default(),
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
        tracker.on_focus_stop();
        assert!(!tracker.is_tracking());
        assert_eq!(tracker.secs_since_last_heartbeat, 0);
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
            })
            .expect("test event send must succeed");

        tracker.poll_events();

        assert!(tracker.heartbeat_in_flight);
        assert!(!tracker.pending_immediate_heartbeat);
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
}
