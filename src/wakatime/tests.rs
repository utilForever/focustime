use super::*;
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

fn unique_temp_queue_dir(test_name: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time must be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "focustime-wakatime-queue-dir-{test_name}-{}-{now}",
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
fn runtime_options_normalization_clamps_queue_retry_delay_to_positive_bounds() {
    let normalized = WakatimeRuntimeOptions {
        queue_retry_delay_secs: 0,
        ..WakatimeRuntimeOptions::default()
    }
    .normalized();
    assert_eq!(
        normalized.queue_retry_delay_secs,
        MIN_HEARTBEAT_QUEUE_RETRY_DELAY_SECS
    );

    let normalized = WakatimeRuntimeOptions {
        queue_retry_delay_secs: u64::MAX,
        ..WakatimeRuntimeOptions::default()
    }
    .normalized();
    assert_eq!(
        normalized.queue_retry_delay_secs,
        MAX_HEARTBEAT_QUEUE_RETRY_DELAY_SECS
    );

    let normalized = WakatimeRuntimeOptions {
        retry_backoff_secs: vec![0, 0, 0, 0, 0, 0, 0, 0, 5],
        ..WakatimeRuntimeOptions::default()
    }
    .normalized();
    assert_eq!(normalized.retry_backoff_secs, vec![5]);
}

#[test]
fn scaled_queue_retry_delay_grows_exponentially_and_is_capped() {
    let mut tracker = tracker_with(Some("test-key"), true, 0);
    tracker.runtime.queue_retry_delay_secs = 30;

    assert_eq!(tracker.scaled_queue_retry_delay_secs(1), 30);
    assert_eq!(tracker.scaled_queue_retry_delay_secs(2), 60);
    assert_eq!(tracker.scaled_queue_retry_delay_secs(3), 120);
    assert_eq!(
        tracker.scaled_queue_retry_delay_secs(8),
        MAX_HEARTBEAT_QUEUE_RETRY_DELAY_SECS
    );
}

#[test]
fn retryable_queue_failures_increase_replay_delay_until_success() {
    let mut tracker = tracker_with(Some("test-key"), true, 0);
    tracker.runtime.queue_retry_delay_secs = 2;
    tracker.request_heartbeat(true);

    tracker
        .result_tx
        .send(HeartbeatEvent::Failed {
            error: "network unavailable".to_string(),
            retryable: true,
        })
        .expect("test event send must succeed");
    tracker.poll_events();

    let first_not_before = tracker
        .queue_retry_not_before_epoch_secs
        .expect("first retryable failure should set queue replay delay");
    assert_eq!(tracker.queue_retry_failure_streak, 1);

    tracker.queue_retry_not_before_epoch_secs = Some(current_unix_epoch_secs().saturating_sub(1));
    tracker.poll_events();
    assert!(tracker.heartbeat_in_flight);

    tracker
        .result_tx
        .send(HeartbeatEvent::Failed {
            error: "still offline".to_string(),
            retryable: true,
        })
        .expect("test event send must succeed");
    tracker.poll_events();

    let second_not_before = tracker
        .queue_retry_not_before_epoch_secs
        .expect("second retryable failure should set queue replay delay");
    assert_eq!(tracker.queue_retry_failure_streak, 2);
    assert!(second_not_before > first_not_before);

    tracker
        .result_tx
        .send(HeartbeatEvent::Sent)
        .expect("test event send must succeed");
    tracker.poll_events();
    assert_eq!(tracker.queue_retry_failure_streak, 0);
    assert!(tracker.queue_retry_not_before_epoch_secs.is_none());
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
        max_attempts: tracker.runtime.max_attempts(),
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
    let queue_capacity = tracker.runtime.queue_capacity;
    for index in 0..(queue_capacity + 5) {
        tracker.enqueue_heartbeat(build_heartbeat_payload(
            index as f64,
            &tracker.heartbeat_metadata,
        ));
    }
    assert_eq!(tracker.queued_heartbeats.len(), queue_capacity);
    let oldest = tracker
        .queued_heartbeats
        .front()
        .expect("queue should have oldest heartbeat after capping");
    assert_eq!(oldest.time, 5.0);
}

#[test]
fn requeue_overflow_drops_oldest_queued_heartbeat() {
    let mut tracker = tracker_with(Some("test-key"), true, 0);
    let queue_capacity = tracker.runtime.queue_capacity;
    tracker.queued_heartbeats = (0..queue_capacity)
        .map(|index| build_heartbeat_payload(index as f64, &tracker.heartbeat_metadata))
        .collect();
    tracker.in_flight_heartbeat = Some(build_heartbeat_payload(999.0, &tracker.heartbeat_metadata));

    tracker.requeue_in_flight_heartbeat();

    assert_eq!(tracker.queued_heartbeats.len(), queue_capacity);
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
        (queue_capacity - 1) as f64
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
            max_attempts: tracker.runtime.max_attempts(),
            next_backoff_secs: 1,
            error: "HTTP 503".to_string(),
        })
        .expect("test event send must succeed");

    tracker.poll_events();

    assert_eq!(
        tracker.runtime_state(),
        WakatimeRuntimeState::Retrying {
            attempt: 1,
            max_attempts: tracker.runtime.max_attempts(),
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
    let queue_capacity = WakatimeRuntimeOptions::default().queue_capacity;
    let oversized = HeartbeatQueueSnapshot {
        schema_version: HEARTBEAT_QUEUE_SNAPSHOT_SCHEMA_VERSION,
        queued_heartbeats: (0..(queue_capacity + 5))
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

    assert_eq!(restored.queued_heartbeats.len(), queue_capacity);
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

#[test]
fn snapshot_persistence_failure_is_exposed_via_runtime_state() {
    let snapshot_dir = unique_temp_queue_dir("persist-failure-runtime");
    fs::create_dir_all(&snapshot_dir).expect("test snapshot directory should be created");

    let mut tracker = WakatimeTracker::new_configured_for_tests();
    tracker.queue_snapshot_path = Some(snapshot_dir.clone());
    tracker.sync_queue_snapshot();

    assert!(
        tracker
            .queue_persistence_error
            .as_deref()
            .is_some_and(|message| message.contains("offline queue persistence error"))
    );
    assert!(matches!(
        tracker.runtime_state(),
        WakatimeRuntimeState::Error(message)
            if message.contains("offline queue persistence error")
    ));

    let _ = fs::remove_dir_all(snapshot_dir);
}

#[test]
fn successful_snapshot_sync_clears_persistence_error() {
    let snapshot_path = unique_temp_queue_snapshot_path("persist-error-cleared");

    let mut tracker = WakatimeTracker::new_configured_for_tests();
    tracker.queue_snapshot_path = Some(snapshot_path.clone());
    tracker.queue_persistence_error = Some("previous failure".to_string());
    tracker.set_pending_heartbeats_for_tests(1);
    tracker.sync_queue_snapshot();

    assert!(tracker.queue_persistence_error.is_none());

    let _ = fs::remove_file(snapshot_path);
}
