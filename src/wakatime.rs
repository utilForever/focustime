use std::collections::VecDeque;
use std::fs;
#[cfg(test)]
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{SystemTime, UNIX_EPOCH};

mod config;
mod queue;
mod transport;

use config::{WakatimeConfig, config_diagnostics_from_read_result};
use queue::{
    HEARTBEAT_QUEUE_SNAPSHOT_SCHEMA_VERSION, HeartbeatQueueSnapshot,
    clear_heartbeat_queue_snapshot, heartbeat_queue_snapshot_path, push_back_with_capacity,
    push_front_with_capacity, read_heartbeat_queue_snapshot, write_heartbeat_queue_snapshot,
};
pub(in crate::wakatime) use transport::Heartbeat;
#[cfg(test)]
use transport::is_retryable_error;
use transport::{build_heartbeat_payload, spawn_heartbeat_worker};

const HEARTBEAT_INTERVAL_SECS: u64 = 120;
const DEFAULT_HEARTBEAT_RETRY_BACKOFF_SECS: [u64; 3] = [2, 5, 10];
const DEFAULT_HEARTBEAT_QUEUE_CAPACITY: usize = 512;
const DEFAULT_HEARTBEAT_QUEUE_RETRY_DELAY_SECS: u64 = 30;
const MAX_HEARTBEAT_RETRY_BACKOFF_ENTRIES: usize = 8;
const MAX_HEARTBEAT_RETRY_BACKOFF_SECS: u64 = 300;
const MAX_HEARTBEAT_QUEUE_CAPACITY: usize = 4096;
const MIN_HEARTBEAT_QUEUE_RETRY_DELAY_SECS: u64 = 1;
const MAX_HEARTBEAT_QUEUE_RETRY_DELAY_SECS: u64 = 3600;
pub(super) const DEFAULT_API_URL: &str = "https://wakatime.com";
const DEFAULT_HEARTBEAT_ENTITY: &str = "focustime";
const DEFAULT_HEARTBEAT_PROJECT: &str = "focustime";
const DEFAULT_HEARTBEAT_LANGUAGE: &str = "Pomodoro";

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WakatimeRuntimeOptions {
    pub retry_backoff_secs: Vec<u64>,
    pub queue_capacity: usize,
    pub queue_retry_delay_secs: u64,
}

impl WakatimeRuntimeOptions {
    pub fn normalized(&self) -> Self {
        let retry_backoff_secs = self
            .retry_backoff_secs
            .iter()
            .filter_map(|secs| {
                if *secs == 0 {
                    None
                } else {
                    Some((*secs).clamp(1, MAX_HEARTBEAT_RETRY_BACKOFF_SECS))
                }
            })
            .take(MAX_HEARTBEAT_RETRY_BACKOFF_ENTRIES)
            .collect::<Vec<_>>();
        Self {
            retry_backoff_secs: if retry_backoff_secs.is_empty() {
                DEFAULT_HEARTBEAT_RETRY_BACKOFF_SECS.to_vec()
            } else {
                retry_backoff_secs
            },
            queue_capacity: self.queue_capacity.clamp(1, MAX_HEARTBEAT_QUEUE_CAPACITY),
            queue_retry_delay_secs: self.queue_retry_delay_secs.clamp(
                MIN_HEARTBEAT_QUEUE_RETRY_DELAY_SECS,
                MAX_HEARTBEAT_QUEUE_RETRY_DELAY_SECS,
            ),
        }
    }

    fn max_attempts(&self) -> u8 {
        let attempts = self.retry_backoff_secs.len().saturating_add(1);
        attempts.min(u8::MAX as usize) as u8
    }
}

impl Default for WakatimeRuntimeOptions {
    fn default() -> Self {
        Self {
            retry_backoff_secs: DEFAULT_HEARTBEAT_RETRY_BACKOFF_SECS.to_vec(),
            queue_capacity: DEFAULT_HEARTBEAT_QUEUE_CAPACITY,
            queue_retry_delay_secs: DEFAULT_HEARTBEAT_QUEUE_RETRY_DELAY_SECS,
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
    /// Consecutive retryable replay failures used to scale queue replay delay.
    queue_retry_failure_streak: u32,
    /// Durable snapshot path for queued heartbeats and replay state.
    queue_snapshot_path: Option<PathBuf>,
    /// Latches an immediate heartbeat request while another worker is in flight.
    pending_immediate_heartbeat: bool,
    heartbeat_metadata: WakatimeHeartbeatMetadata,
    runtime: WakatimeRuntimeOptions,
    /// Startup-only warning (for example, invalid persisted queue state).
    startup_warning: Option<String>,
    /// Last queue snapshot persistence failure message.
    queue_persistence_error: Option<String>,
    #[cfg(test)]
    disable_network_io: bool,
}

impl WakatimeTracker {
    pub fn new() -> Self {
        Self::new_with_settings(
            WakatimeHeartbeatMetadata::default(),
            WakatimeRuntimeOptions::default(),
        )
    }

    pub fn new_with_settings(
        metadata: WakatimeHeartbeatMetadata,
        runtime: WakatimeRuntimeOptions,
    ) -> Self {
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
            queue_retry_failure_streak: 0,
            queue_snapshot_path: heartbeat_queue_snapshot_path(),
            pending_immediate_heartbeat: false,
            heartbeat_metadata: metadata.normalized(),
            runtime: runtime.normalized(),
            startup_warning: None,
            queue_persistence_error: None,
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
        if let Some(error) = self.queue_persistence_error.as_ref() {
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
                let queue_capacity = self.runtime.queue_capacity;
                self.queued_heartbeats = snapshot.queued_heartbeats.into_iter().fold(
                    VecDeque::new(),
                    |mut queue, heartbeat| {
                        push_back_with_capacity(&mut queue, heartbeat, queue_capacity);
                        queue
                    },
                );
                if let Some(in_flight) = snapshot.in_flight_heartbeat {
                    if snapshot.in_flight_from_queue {
                        push_front_with_capacity(
                            &mut self.queued_heartbeats,
                            in_flight,
                            queue_capacity,
                        );
                    } else {
                        push_back_with_capacity(
                            &mut self.queued_heartbeats,
                            in_flight,
                            queue_capacity,
                        );
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
                self.queue_retry_failure_streak = 0;
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
        match result {
            Ok(()) => self.queue_persistence_error = None,
            Err(error) => {
                self.queue_persistence_error =
                    Some(format!("WakaTime offline queue persistence error: {error}"));
            }
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
                    self.queue_retry_failure_streak = 0;
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
                        self.queue_retry_failure_streak =
                            self.queue_retry_failure_streak.saturating_add(1);
                        let retry_delay_secs =
                            self.scaled_queue_retry_delay_secs(self.queue_retry_failure_streak);
                        self.queue_retry_not_before_epoch_secs =
                            Some(current_unix_epoch_secs().saturating_add(retry_delay_secs));
                    } else {
                        self.in_flight_heartbeat = None;
                        self.queue_retry_not_before_epoch_secs = None;
                        self.queue_retry_failure_streak = 0;
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
        push_back_with_capacity(
            &mut self.queued_heartbeats,
            heartbeat,
            self.runtime.queue_capacity,
        );
        self.sync_queue_snapshot();
    }

    fn requeue_in_flight_heartbeat(&mut self) {
        let Some(in_flight_heartbeat) = self.in_flight_heartbeat.take() else {
            return;
        };
        push_front_with_capacity(
            &mut self.queued_heartbeats,
            in_flight_heartbeat,
            self.runtime.queue_capacity,
        );
    }

    fn queue_retry_is_due(&self) -> bool {
        self.queue_retry_not_before_epoch_secs
            .is_none_or(|not_before| current_unix_epoch_secs() >= not_before)
    }

    fn scaled_queue_retry_delay_secs(&self, failure_streak: u32) -> u64 {
        let exponent = failure_streak.saturating_sub(1);
        let multiplier = 2u64.checked_pow(exponent).unwrap_or(u64::MAX);
        self.runtime
            .queue_retry_delay_secs
            .saturating_mul(multiplier)
            .min(MAX_HEARTBEAT_QUEUE_RETRY_DELAY_SECS)
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

        spawn_heartbeat_worker(
            self.result_tx.clone(),
            api_key,
            self.api_url.clone(),
            heartbeat,
            self.runtime.clone(),
        );
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
            queue_retry_failure_streak: 0,
            queue_snapshot_path: None,
            pending_immediate_heartbeat: false,
            heartbeat_metadata: WakatimeHeartbeatMetadata::default(),
            runtime: WakatimeRuntimeOptions {
                queue_retry_delay_secs: 0,
                ..WakatimeRuntimeOptions::default()
            },
            startup_warning: None,
            queue_persistence_error: None,
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
            queue_retry_failure_streak: 0,
            queue_snapshot_path: None,
            pending_immediate_heartbeat: false,
            heartbeat_metadata: WakatimeHeartbeatMetadata::default(),
            runtime: WakatimeRuntimeOptions {
                queue_retry_delay_secs: 0,
                ..WakatimeRuntimeOptions::default()
            },
            startup_warning: None,
            queue_persistence_error: None,
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
    pub(crate) fn push_retrying_event_for_tests(
        &self,
        attempt: u8,
        max_attempts: u8,
        next_backoff_secs: u64,
        error: impl Into<String>,
    ) {
        let _ = self.result_tx.send(HeartbeatEvent::Retrying {
            attempt,
            max_attempts,
            next_backoff_secs,
            error: error.into(),
        });
    }

    #[cfg(test)]
    pub(crate) fn heartbeat_metadata_for_tests(&self) -> WakatimeHeartbeatMetadata {
        self.heartbeat_metadata.clone()
    }

    #[cfg(test)]
    pub(crate) fn runtime_options_for_tests(&self) -> WakatimeRuntimeOptions {
        self.runtime.clone()
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
        self.queue_retry_failure_streak = 0;
        self.pending_immediate_heartbeat = false;
        self.retry_state = None;
        self.last_error = None;
        self.queue_persistence_error = None;
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

impl Default for WakatimeTracker {
    fn default() -> Self {
        Self::new()
    }
}

fn normalize_nonempty_or_default(value: &str, default: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests;
