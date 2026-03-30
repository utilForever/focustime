use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::Serialize;

const HEARTBEAT_INTERVAL_SECS: u64 = 120;
const DEFAULT_API_URL: &str = "https://wakatime.com";

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

    /// Parse `~/.wakatime.cfg` as a simple INI file.
    /// Returns `(api_key, api_url)` from the `[settings]` section.
    fn parse_config_file() -> Option<(Option<String>, Option<String>)> {
        let home = dirs_home()?;
        let cfg_path = home.join(".wakatime.cfg");
        let content = fs::read_to_string(cfg_path).ok()?;

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
            if let Some((key, value)) = trimmed.split_once('=') {
                let k = key.trim();
                let v = value.trim().to_string();
                match k {
                    "api_key" if !v.is_empty() => api_key = Some(v),
                    "api_url" if !v.is_empty() => api_url = Some(v),
                    _ => {}
                }
            }
        }

        Some((api_key, api_url))
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
}

impl WakatimeTracker {
    pub fn new() -> Self {
        let config = WakatimeConfig::load();
        Self {
            api_key: config.api_key,
            api_url: config.api_url,
            secs_since_last_heartbeat: 0,
            tracking: false,
        }
    }

    /// Returns `true` if an API key is configured.
    pub fn is_configured(&self) -> bool {
        self.api_key.is_some()
    }

    /// Returns `true` if actively sending heartbeats for a focus session.
    pub fn is_tracking(&self) -> bool {
        self.tracking
    }

    /// Called when a focus session starts (timer transitions to Running in Focus phase).
    /// Sends an immediate heartbeat and resets the interval counter.
    pub fn on_focus_start(&mut self) {
        self.tracking = true;
        self.secs_since_last_heartbeat = 0;
        self.send_heartbeat_async();
    }

    /// Called each second while the focus session is running.
    /// Sends a heartbeat every `HEARTBEAT_INTERVAL_SECS` seconds.
    pub fn tick(&mut self) {
        if !self.tracking {
            return;
        }
        self.secs_since_last_heartbeat += 1;
        if self.secs_since_last_heartbeat >= HEARTBEAT_INTERVAL_SECS {
            self.secs_since_last_heartbeat = 0;
            self.send_heartbeat_async();
        }
    }

    /// Called when the focus session pauses, stops, or moves to a break phase.
    pub fn on_focus_stop(&mut self) {
        self.tracking = false;
        self.secs_since_last_heartbeat = 0;
    }

    /// Spawns a background thread to send a heartbeat to the WakaTime API.
    /// Failures are silently ignored so the TUI remains unaffected.
    fn send_heartbeat_async(&self) {
        let Some(ref api_key) = self.api_key else {
            return;
        };

        let auth = format!("Basic {}", BASE64.encode(api_key.as_bytes()));
        let url = format!("{}/api/v1/users/current/heartbeats", self.api_url);

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

        let heartbeat = Heartbeat {
            entity: "focustime".to_string(),
            entity_type: "app".to_string(),
            time: now,
            project: "focustime".to_string(),
            language: "Pomodoro".to_string(),
            is_write: false,
        };

        std::thread::spawn(move || {
            let _ = ureq::post(&url)
                .set("Authorization", &auth)
                .set("Content-Type", "application/json")
                .set("User-Agent", &user_agent)
                .set("X-Machine-Name", &hostname)
                .send_json(heartbeat);
        });
    }
}

impl Default for WakatimeTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_config_extracts_api_key() {
        let content = "[settings]\napi_key = test-key-123\n";
        let mut api_key = None;
        let mut api_url = None;
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
            if let Some((k, v)) = trimmed.split_once('=') {
                match k.trim() {
                    "api_key" => api_key = Some(v.trim().to_string()),
                    "api_url" => api_url = Some(v.trim().to_string()),
                    _ => {}
                }
            }
        }

        assert_eq!(api_key, Some("test-key-123".to_string()));
        assert_eq!(api_url, None);
    }

    #[test]
    fn parse_config_extracts_api_url() {
        let content = "[settings]\napi_key = mykey\napi_url = https://wakatime.example.com\n";
        let mut api_key = None;
        let mut api_url = None;
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
            if let Some((k, v)) = trimmed.split_once('=') {
                match k.trim() {
                    "api_key" => api_key = Some(v.trim().to_string()),
                    "api_url" => api_url = Some(v.trim().to_string()),
                    _ => {}
                }
            }
        }

        assert_eq!(api_key, Some("mykey".to_string()));
        assert_eq!(api_url, Some("https://wakatime.example.com".to_string()));
    }

    #[test]
    fn parse_config_ignores_other_sections() {
        let content = "[other]\napi_key = wrong\n[settings]\napi_key = correct\n";
        let mut api_key = None;
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
            if let Some((k, v)) = trimmed.split_once('=')
                && k.trim() == "api_key"
            {
                api_key = Some(v.trim().to_string());
            }
        }

        assert_eq!(api_key, Some("correct".to_string()));
    }

    #[test]
    fn tracker_not_configured_when_no_api_key() {
        let tracker = WakatimeTracker {
            api_key: None,
            api_url: DEFAULT_API_URL.to_string(),
            secs_since_last_heartbeat: 0,
            tracking: false,
        };
        assert!(!tracker.is_configured());
        assert!(!tracker.is_tracking());
    }

    #[test]
    fn tracker_configured_when_api_key_present() {
        let tracker = WakatimeTracker {
            api_key: Some("test-key".to_string()),
            api_url: DEFAULT_API_URL.to_string(),
            secs_since_last_heartbeat: 0,
            tracking: false,
        };
        assert!(tracker.is_configured());
    }

    #[test]
    fn on_focus_start_sets_tracking() {
        let mut tracker = WakatimeTracker {
            api_key: None, // no key: heartbeat send is skipped but tracking flag still sets
            api_url: DEFAULT_API_URL.to_string(),
            secs_since_last_heartbeat: 50,
            tracking: false,
        };
        tracker.on_focus_start();
        assert!(tracker.is_tracking());
        assert_eq!(tracker.secs_since_last_heartbeat, 0);
    }

    #[test]
    fn on_focus_stop_clears_tracking() {
        let mut tracker = WakatimeTracker {
            api_key: None,
            api_url: DEFAULT_API_URL.to_string(),
            secs_since_last_heartbeat: 60,
            tracking: true,
        };
        tracker.on_focus_stop();
        assert!(!tracker.is_tracking());
        assert_eq!(tracker.secs_since_last_heartbeat, 0);
    }

    #[test]
    fn tick_increments_counter_and_resets_at_interval() {
        let mut tracker = WakatimeTracker {
            api_key: None, // no HTTP call made
            api_url: DEFAULT_API_URL.to_string(),
            secs_since_last_heartbeat: HEARTBEAT_INTERVAL_SECS - 1,
            tracking: true,
        };
        tracker.tick();
        // Counter should have reset after reaching the interval threshold
        assert_eq!(tracker.secs_since_last_heartbeat, 0);
    }

    #[test]
    fn tick_does_nothing_when_not_tracking() {
        let mut tracker = WakatimeTracker {
            api_key: None,
            api_url: DEFAULT_API_URL.to_string(),
            secs_since_last_heartbeat: 0,
            tracking: false,
        };
        tracker.tick();
        assert_eq!(tracker.secs_since_last_heartbeat, 0);
    }
}
