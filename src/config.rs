use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Persistent application configuration stored as TOML.
///
/// File locations:
/// - Unix / macOS: `$XDG_CONFIG_HOME/focustime/config.toml` if set,
///   otherwise `~/.config/focustime/config.toml`
/// - Windows:      `%APPDATA%\focustime\config.toml`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Duration of a focus session in seconds.
    #[serde(default = "default_focus_secs")]
    pub focus_secs: u64,
    /// Duration of a short-break session in seconds.
    #[serde(default = "default_short_break_secs")]
    pub short_break_secs: u64,
    /// Duration of a long-break session in seconds.
    #[serde(default = "default_long_break_secs")]
    pub long_break_secs: u64,
    /// Sites that should be blocked during focus sessions.
    #[serde(default)]
    pub blocked_sites: Vec<String>,
}

fn default_focus_secs() -> u64 {
    crate::timer::DEFAULT_FOCUS_SECS
}
fn default_short_break_secs() -> u64 {
    crate::timer::DEFAULT_SHORT_BREAK_SECS
}
fn default_long_break_secs() -> u64 {
    crate::timer::DEFAULT_LONG_BREAK_SECS
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            focus_secs: default_focus_secs(),
            short_break_secs: default_short_break_secs(),
            long_break_secs: default_long_break_secs(),
            blocked_sites: Vec::new(),
        }
    }
}

impl AppConfig {
    /// Load the config from disk, falling back to [`AppConfig::default`] on any
    /// error (missing file, parse error, corrupt data, etc.).
    pub fn load() -> Self {
        Self::try_load().unwrap_or_default()
    }

    fn try_load() -> Option<Self> {
        let path = Self::config_path()?;
        let content = fs::read_to_string(path).ok()?;
        toml::from_str(&content).ok()
    }

    /// Persist the current config to disk.
    /// Creates parent directories as needed.
    pub fn save(&self) -> io::Result<()> {
        let path = Self::config_path().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "cannot determine config directory")
        })?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        // Atomic write: write to a temp file first, then rename.
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, &content)?;
        #[cfg(target_os = "windows")]
        {
            match fs::rename(&tmp, &path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    fs::remove_file(&path)?;
                    fs::rename(&tmp, &path)
                }
                Err(e) => Err(e),
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            fs::rename(&tmp, &path)
        }
    }

    fn config_path() -> Option<PathBuf> {
        let config_dir = config_dir()?;
        Some(config_dir.join("focustime").join("config.toml"))
    }
}

/// Returns the platform-appropriate configuration base directory.
fn config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA").ok().map(PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        // Honour XDG_CONFIG_HOME if set, otherwise fall back to ~/.config.
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
            && !xdg.trim().is_empty()
        {
            return Some(PathBuf::from(xdg));
        }
        let home = std::env::var("HOME").ok()?;
        Some(PathBuf::from(home).join(".config"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values_are_canonical_pomodoro() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.focus_secs, 25 * 60);
        assert_eq!(cfg.short_break_secs, 5 * 60);
        assert_eq!(cfg.long_break_secs, 15 * 60);
        assert!(cfg.blocked_sites.is_empty());
    }

    #[test]
    fn round_trip_full_config() {
        let original = AppConfig {
            focus_secs: 50 * 60,
            short_break_secs: 10 * 60,
            long_break_secs: 20 * 60,
            blocked_sites: vec!["example.com".to_string(), "reddit.com".to_string()],
        };
        let toml_str = toml::to_string_pretty(&original).unwrap();
        let parsed: AppConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.focus_secs, original.focus_secs);
        assert_eq!(parsed.short_break_secs, original.short_break_secs);
        assert_eq!(parsed.long_break_secs, original.long_break_secs);
        assert_eq!(parsed.blocked_sites, original.blocked_sites);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        // A config file that only specifies one field; the rest should default.
        let partial = "focus_secs = 1500\n";
        let cfg: AppConfig = toml::from_str(partial).unwrap();
        assert_eq!(cfg.focus_secs, 1500);
        assert_eq!(cfg.short_break_secs, 5 * 60);
        assert_eq!(cfg.long_break_secs, 15 * 60);
        assert!(cfg.blocked_sites.is_empty());
    }

    #[test]
    fn corrupt_toml_parse_failure_can_fall_back_to_default() {
        let content = "this is not valid toml !!!";
        let result: Result<AppConfig, _> = toml::from_str(content);
        assert!(result.is_err(), "invalid TOML should fail to parse");
        // Applying a default fallback after parse failure yields defaults.
        let cfg = result.ok().unwrap_or_default();
        assert_eq!(cfg.focus_secs, crate::timer::DEFAULT_FOCUS_SECS);
    }

    #[test]
    fn round_trip_empty_blocked_sites() {
        let original = AppConfig::default();
        let toml_str = toml::to_string_pretty(&original).unwrap();
        let parsed: AppConfig = toml::from_str(&toml_str).unwrap();
        assert!(parsed.blocked_sites.is_empty());
    }
}
