use std::fs;
use std::io;
use std::path::PathBuf;

use super::{DEFAULT_API_URL, WakatimeConfigDiagnostics, WakatimeConfigStatus};

/// Reads WakaTime configuration from `~/.wakatime.cfg`.
pub(super) struct WakatimeConfig {
    pub(super) api_key: Option<String>,
    pub(super) api_url: String,
}

impl WakatimeConfig {
    pub(super) fn load() -> Self {
        let (api_key, api_url) = Self::parse_config_file().unwrap_or((None, None));
        Self {
            api_key,
            api_url: api_url.unwrap_or_else(|| DEFAULT_API_URL.to_string()),
        }
    }

    pub(super) fn config_file_path() -> Option<PathBuf> {
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
    pub(super) fn parse_config_str(content: &str) -> (Option<String>, Option<String>) {
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

fn parse_setting_line(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once('=')?;
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    Some((key.trim(), value))
}

pub(super) fn config_diagnostics_from_read_result(
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
