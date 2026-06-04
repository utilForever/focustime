use std::sync::mpsc::Sender;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};

use super::{
    DEFAULT_HEARTBEAT_ENTITY, HeartbeatEvent, WakatimeHeartbeatMetadata, WakatimeRuntimeOptions,
};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub(in crate::wakatime) struct Heartbeat {
    pub(super) entity: String,
    #[serde(rename = "type")]
    pub(super) entity_type: String,
    pub(super) time: f64,
    pub(super) project: String,
    pub(super) language: String,
    pub(super) is_write: bool,
}

pub(super) fn build_heartbeat_payload(now: f64, metadata: &WakatimeHeartbeatMetadata) -> Heartbeat {
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

pub(super) fn spawn_heartbeat_worker(
    result_tx: Sender<HeartbeatEvent>,
    api_key: String,
    api_url: String,
    heartbeat: Heartbeat,
    runtime: WakatimeRuntimeOptions,
) {
    let auth = format!("Basic {}", BASE64.encode(api_key.as_bytes()));
    let api_url = api_url.trim_end_matches('/');
    let url = format!("{}/api/v1/users/current/heartbeats", api_url);

    let plugin_version = env!("CARGO_PKG_VERSION");
    let os = std::env::consts::OS;
    let user_agent = format!(
        "wakatime/unset ({os}) focustime/{plugin_version} focustime-wakatime/{plugin_version}"
    );
    let hostname = get_hostname();

    std::thread::spawn(move || {
        send_heartbeat_with_retries(
            result_tx, url, auth, user_agent, hostname, heartbeat, runtime,
        );
    });
}

fn send_heartbeat_with_retries(
    result_tx: Sender<HeartbeatEvent>,
    url: String,
    auth: String,
    user_agent: String,
    hostname: String,
    heartbeat: Heartbeat,
    runtime: WakatimeRuntimeOptions,
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
                    && let Some(backoff_secs) = runtime.retry_backoff_secs.get(backoff_index)
                {
                    let _ = result_tx.send(HeartbeatEvent::Retrying {
                        attempt,
                        max_attempts: runtime.max_attempts(),
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

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn is_retryable_error(error: &ureq::Error) -> bool {
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
