use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Deserializer, Serialize};

/// Persistent application configuration stored as TOML.
///
/// File locations:
/// - Unix / macOS: `$XDG_CONFIG_HOME/focustime/config.toml` if set,
///   otherwise `~/.config/focustime/config.toml`
/// - Windows:      `%APPDATA%\focustime\config.toml`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Duration of a focus session in seconds (legacy compatibility field).
    #[serde(default = "default_focus_secs")]
    pub focus_secs: u64,
    /// Duration of a short-break session in seconds (legacy compatibility field).
    #[serde(default = "default_short_break_secs")]
    pub short_break_secs: u64,
    /// Duration of a long-break session in seconds (legacy compatibility field).
    #[serde(default = "default_long_break_secs")]
    pub long_break_secs: u64,
    /// Number of completed focus sessions before a long break.
    #[serde(default = "default_long_break_interval")]
    pub long_break_interval: u32,
    /// Sites that should be blocked during focus sessions.
    #[serde(default)]
    pub blocked_sites: Vec<String>,
    /// Selected profile identifier.
    #[serde(default)]
    pub selected_profile: ProfileId,
    /// Editable custom profile persisted by the app.
    ///
    /// When this is absent, the app derives it from the legacy duration fields.
    #[serde(default)]
    pub custom_profile: Option<CustomProfileConfig>,
    /// Notification preferences for phase transitions.
    #[serde(default)]
    pub notifications: NotificationConfig,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotificationConfig {
    #[serde(default = "default_notification_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub sound: bool,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enabled: default_notification_enabled(),
            sound: false,
        }
    }
}

fn default_notification_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ProfileId {
    Classic,
    DeepWork,
    #[default]
    Custom,
}

impl ProfileId {
    fn from_config_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "classic" => Self::Classic,
            "deep-work" | "deep_work" | "deepwork" => Self::DeepWork,
            "custom" => Self::Custom,
            _ => Self::Custom,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ProfileId::Classic => "Classic",
            ProfileId::DeepWork => "Deep Work",
            ProfileId::Custom => "Custom",
        }
    }
}

impl<'de> Deserialize<'de> for ProfileId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(Self::from_config_value(&value))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomProfileConfig {
    #[serde(default = "default_focus_secs")]
    pub focus_secs: u64,
    #[serde(default = "default_short_break_secs")]
    pub short_break_secs: u64,
    #[serde(default = "default_long_break_secs")]
    pub long_break_secs: u64,
    #[serde(default = "default_long_break_interval")]
    pub long_break_interval: u32,
}

impl CustomProfileConfig {
    pub fn normalized(&self) -> Self {
        Self {
            focus_secs: nonzero_or_default_u64(self.focus_secs, default_focus_secs()),
            short_break_secs: nonzero_or_default_u64(
                self.short_break_secs,
                default_short_break_secs(),
            ),
            long_break_secs: nonzero_or_default_u64(
                self.long_break_secs,
                default_long_break_secs(),
            ),
            long_break_interval: nonzero_or_default_u32(
                self.long_break_interval,
                default_long_break_interval(),
            ),
        }
    }
}

impl Default for CustomProfileConfig {
    fn default() -> Self {
        Self {
            focus_secs: default_focus_secs(),
            short_break_secs: default_short_break_secs(),
            long_break_secs: default_long_break_secs(),
            long_break_interval: default_long_break_interval(),
        }
    }
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
fn default_long_break_interval() -> u32 {
    crate::timer::DEFAULT_LONG_BREAK_INTERVAL
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            focus_secs: default_focus_secs(),
            short_break_secs: default_short_break_secs(),
            long_break_secs: default_long_break_secs(),
            long_break_interval: default_long_break_interval(),
            blocked_sites: Vec::new(),
            selected_profile: ProfileId::default(),
            custom_profile: None,
            notifications: NotificationConfig::default(),
        }
    }
}

impl AppConfig {
    /// Load the config from disk, falling back to [`AppConfig::default`] on any
    /// error (missing file, parse error, corrupt data, etc.).
    pub fn load() -> Self {
        Self::try_load().unwrap_or_default()
    }

    /// Returns the effective custom profile configuration.
    ///
    /// If `custom_profile` is not present in the config file, this derives
    /// values from legacy duration fields to preserve user settings.
    pub fn effective_custom_profile(&self) -> CustomProfileConfig {
        self.custom_profile
            .clone()
            .unwrap_or(CustomProfileConfig {
                focus_secs: self.focus_secs,
                short_break_secs: self.short_break_secs,
                long_break_secs: self.long_break_secs,
                long_break_interval: self.long_break_interval,
            })
            .normalized()
    }

    fn try_load() -> Option<Self> {
        Self::try_load_with_env(|key| std::env::var_os(key))
    }

    #[cfg(test)]
    fn load_with_env(get_var: impl FnMut(&str) -> Option<OsString>) -> Self {
        Self::try_load_with_env(get_var).unwrap_or_default()
    }

    fn try_load_with_env(get_var: impl FnMut(&str) -> Option<OsString>) -> Option<Self> {
        let path = Self::config_path_with_env(get_var)?;
        let content = fs::read_to_string(path).ok()?;
        toml::from_str(&content).ok().map(Self::normalize)
    }

    /// Persist the current config to disk.
    /// Creates parent directories as needed.
    #[cfg_attr(test, allow(dead_code))]
    pub fn save(&self) -> io::Result<()> {
        let path = Self::config_path().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "cannot determine config directory")
        })?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        // Best-effort atomic write: temp file + rename.
        // On Windows, rename cannot replace an existing file; we fall back to
        // remove+rename when the destination already exists.
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

    fn normalize(mut self) -> Self {
        self.focus_secs = nonzero_or_default_u64(self.focus_secs, default_focus_secs());
        self.short_break_secs =
            nonzero_or_default_u64(self.short_break_secs, default_short_break_secs());
        self.long_break_secs =
            nonzero_or_default_u64(self.long_break_secs, default_long_break_secs());
        self.long_break_interval =
            nonzero_or_default_u32(self.long_break_interval, default_long_break_interval());
        self.custom_profile = self.custom_profile.map(|profile| profile.normalized());
        self
    }

    #[cfg_attr(test, allow(dead_code))]
    fn config_path() -> Option<PathBuf> {
        app_data_path("config.toml")
    }

    fn config_path_with_env(get_var: impl FnMut(&str) -> Option<OsString>) -> Option<PathBuf> {
        let app_dir = app_dir_with_env(get_var)?;
        Some(app_dir.join("config.toml"))
    }
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn app_data_path(file_name: &str) -> Option<PathBuf> {
    let app_dir = app_dir()?;
    Some(app_dir.join(file_name))
}

fn app_dir() -> Option<PathBuf> {
    app_dir_with_env(|key| std::env::var_os(key))
}

fn app_dir_with_env(get_var: impl FnMut(&str) -> Option<OsString>) -> Option<PathBuf> {
    let config_dir = config_dir_from_env(get_var)?;
    Some(config_dir.join("focustime"))
}

fn config_dir_from_env(mut get_var: impl FnMut(&str) -> Option<OsString>) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        env_path_from_value(get_var("APPDATA")?)
    }
    #[cfg(not(target_os = "windows"))]
    {
        // Honour XDG_CONFIG_HOME if set, otherwise fall back to ~/.config.
        if let Some(xdg) = get_var("XDG_CONFIG_HOME").and_then(env_path_from_value) {
            return Some(xdg);
        }
        let home = get_var("HOME").and_then(env_path_from_value)?;
        Some(home.join(".config"))
    }
}

fn env_path_from_value(value: OsString) -> Option<PathBuf> {
    if value.is_empty() {
        return None;
    }
    if let Some(value_utf8) = value.to_str()
        && value_utf8.trim().is_empty()
    {
        return None;
    }
    Some(PathBuf::from(value))
}

fn nonzero_or_default_u64(value: u64, default: u64) -> u64 {
    if value == 0 { default } else { value }
}

fn nonzero_or_default_u32(value: u32, default: u32) -> u32 {
    if value == 0 { default } else { value }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[cfg(target_os = "windows")]
    const CONFIG_DIR_ENV: &str = "APPDATA";
    #[cfg(not(target_os = "windows"))]
    const CONFIG_DIR_ENV: &str = "XDG_CONFIG_HOME";

    #[test]
    fn default_values_are_canonical_pomodoro() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.focus_secs, 25 * 60);
        assert_eq!(cfg.short_break_secs, 5 * 60);
        assert_eq!(cfg.long_break_secs, 15 * 60);
        assert_eq!(cfg.long_break_interval, 4);
        assert_eq!(cfg.selected_profile, ProfileId::Custom);
        assert!(cfg.custom_profile.is_none());
        assert!(cfg.blocked_sites.is_empty());
        assert_eq!(cfg.notifications, NotificationConfig::default());
    }

    #[test]
    fn round_trip_full_config() {
        let original = AppConfig {
            focus_secs: 50 * 60,
            short_break_secs: 10 * 60,
            long_break_secs: 20 * 60,
            long_break_interval: 3,
            blocked_sites: vec!["example.com".to_string(), "reddit.com".to_string()],
            selected_profile: ProfileId::DeepWork,
            custom_profile: Some(CustomProfileConfig {
                focus_secs: 30 * 60,
                short_break_secs: 7 * 60,
                long_break_secs: 12 * 60,
                long_break_interval: 5,
            }),
            notifications: NotificationConfig {
                enabled: true,
                sound: true,
            },
        };
        let toml_str = toml::to_string_pretty(&original).unwrap();
        let parsed: AppConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.focus_secs, original.focus_secs);
        assert_eq!(parsed.short_break_secs, original.short_break_secs);
        assert_eq!(parsed.long_break_secs, original.long_break_secs);
        assert_eq!(parsed.long_break_interval, original.long_break_interval);
        assert_eq!(parsed.blocked_sites, original.blocked_sites);
        assert_eq!(parsed.selected_profile, original.selected_profile);
        assert_eq!(parsed.custom_profile, original.custom_profile);
        assert_eq!(parsed.notifications, original.notifications);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let partial = "focus_secs = 1500\n";
        let cfg: AppConfig = toml::from_str(partial).unwrap();
        assert_eq!(cfg.focus_secs, 1500);
        assert_eq!(cfg.short_break_secs, 5 * 60);
        assert_eq!(cfg.long_break_secs, 15 * 60);
        assert_eq!(cfg.long_break_interval, 4);
        assert_eq!(cfg.selected_profile, ProfileId::Custom);
        assert!(cfg.custom_profile.is_none());
        assert!(cfg.blocked_sites.is_empty());
        assert_eq!(cfg.notifications, NotificationConfig::default());
    }

    #[test]
    fn unknown_selected_profile_falls_back_to_custom_without_dropping_config() {
        let config = r#"
focus_secs = 1500
short_break_secs = 360
long_break_secs = 900
long_break_interval = 3
selected_profile = "unknown"
blocked_sites = ["reddit.com", "youtube.com"]
"#;
        let parsed: AppConfig = toml::from_str(config).unwrap();
        assert_eq!(parsed.selected_profile, ProfileId::Custom);
        assert_eq!(parsed.focus_secs, 1500);
        assert_eq!(parsed.short_break_secs, 360);
        assert_eq!(parsed.long_break_secs, 900);
        assert_eq!(parsed.long_break_interval, 3);
        assert_eq!(parsed.blocked_sites, vec!["reddit.com", "youtube.com"]);
    }

    #[test]
    fn legacy_duration_fields_seed_custom_profile() {
        let legacy = r#"
focus_secs = 1800
short_break_secs = 360
long_break_secs = 900
long_break_interval = 3
"#;
        let cfg: AppConfig = toml::from_str(legacy).unwrap();
        let custom = cfg.effective_custom_profile();
        assert_eq!(custom.focus_secs, 1800);
        assert_eq!(custom.short_break_secs, 360);
        assert_eq!(custom.long_break_secs, 900);
        assert_eq!(custom.long_break_interval, 3);
    }

    #[test]
    fn effective_custom_profile_uses_explicit_profile_when_present() {
        let cfg = AppConfig {
            focus_secs: 25 * 60,
            short_break_secs: 5 * 60,
            long_break_secs: 15 * 60,
            long_break_interval: 4,
            blocked_sites: Vec::new(),
            selected_profile: ProfileId::Custom,
            custom_profile: Some(CustomProfileConfig {
                focus_secs: 40 * 60,
                short_break_secs: 8 * 60,
                long_break_secs: 16 * 60,
                long_break_interval: 2,
            }),
            notifications: NotificationConfig::default(),
        };
        let custom = cfg.effective_custom_profile();
        assert_eq!(custom.focus_secs, 40 * 60);
        assert_eq!(custom.short_break_secs, 8 * 60);
        assert_eq!(custom.long_break_secs, 16 * 60);
        assert_eq!(custom.long_break_interval, 2);
    }

    #[test]
    fn load_returns_default_when_config_file_is_corrupt() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_base = std::env::temp_dir().join(format!(
            "focustime-config-test-{}-{now}",
            std::process::id()
        ));
        let app_dir = temp_base.join("focustime");
        fs::create_dir_all(&app_dir).unwrap();
        fs::write(app_dir.join("config.toml"), "this is not valid toml !!!").unwrap();

        let cfg = AppConfig::load_with_env(|key| {
            if key == CONFIG_DIR_ENV {
                Some(temp_base.clone().into_os_string())
            } else {
                None
            }
        });
        let _ = fs::remove_dir_all(&temp_base);

        assert_eq!(cfg.focus_secs, crate::timer::DEFAULT_FOCUS_SECS);
        assert_eq!(cfg.short_break_secs, crate::timer::DEFAULT_SHORT_BREAK_SECS);
        assert_eq!(cfg.long_break_secs, crate::timer::DEFAULT_LONG_BREAK_SECS);
        assert_eq!(
            cfg.long_break_interval,
            crate::timer::DEFAULT_LONG_BREAK_INTERVAL
        );
        assert_eq!(cfg.selected_profile, ProfileId::Custom);
        assert!(cfg.custom_profile.is_none());
        assert!(cfg.blocked_sites.is_empty());
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn config_dir_returns_none_when_home_is_blank_and_xdg_is_unset() {
        let dir = config_dir_from_env(|key| match key {
            "XDG_CONFIG_HOME" => None,
            "HOME" => Some(OsString::from("   ")),
            _ => None,
        });
        assert!(dir.is_none());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn config_dir_returns_none_when_appdata_is_blank() {
        let dir = config_dir_from_env(|key| match key {
            "APPDATA" => Some(OsString::from("   ")),
            _ => None,
        });
        assert!(dir.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn env_path_from_value_accepts_non_utf8_value() {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};

        let non_utf8 = OsString::from_vec(vec![b'/', b't', b'm', b'p', b'/', 0x80, b'x']);
        let parsed =
            env_path_from_value(non_utf8.clone()).expect("non-UTF-8 env var should be accepted");
        assert_eq!(
            parsed.as_os_str().as_bytes(),
            non_utf8.as_os_str().as_bytes()
        );
    }

    #[cfg(unix)]
    #[test]
    fn config_dir_uses_non_utf8_xdg_config_home() {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};

        let non_utf8 = OsString::from_vec(vec![b'/', b't', b'm', b'p', b'/', 0x81, b'y']);
        let dir = config_dir_from_env(|key| match key {
            "XDG_CONFIG_HOME" => Some(non_utf8.clone()),
            _ => None,
        })
        .expect("non-UTF-8 XDG_CONFIG_HOME should be accepted");
        assert_eq!(dir.as_os_str().as_bytes(), non_utf8.as_os_str().as_bytes());
    }

    #[test]
    fn round_trip_empty_blocked_sites() {
        let original = AppConfig::default();
        let toml_str = toml::to_string_pretty(&original).unwrap();
        let parsed: AppConfig = toml::from_str(&toml_str).unwrap();
        assert!(parsed.blocked_sites.is_empty());
    }
}
