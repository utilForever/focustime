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

    fn config_path() -> Option<PathBuf> {
        let config_dir = config_dir()?;
        Some(config_dir.join("focustime").join("config.toml"))
    }
}

/// Returns the platform-appropriate configuration base directory.
fn config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        env_path_var("APPDATA")
    }
    #[cfg(not(target_os = "windows"))]
    {
        // Honour XDG_CONFIG_HOME if set, otherwise fall back to ~/.config.
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
            && !xdg.trim().is_empty()
        {
            return Some(PathBuf::from(xdg));
        }
        let home = env_path_var("HOME")?;
        Some(home.join(".config"))
    }
}

fn env_path_var(key: &str) -> Option<PathBuf> {
    let value = std::env::var_os(key)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[cfg(target_os = "windows")]
    const CONFIG_DIR_ENV: &str = "APPDATA";
    #[cfg(not(target_os = "windows"))]
    const CONFIG_DIR_ENV: &str = "XDG_CONFIG_HOME";

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct EnvVarGuard {
        key: &'static str,
        original: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
            let original = std::env::var_os(key);
            // SAFETY: test-only mutation of process environment under a module-
            // local lock to avoid concurrent updates from these tests.
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, original }
        }

        #[cfg(not(target_os = "windows"))]
        fn unset(key: &'static str) -> Self {
            let original = std::env::var_os(key);
            // SAFETY: test-only mutation of process environment under a module-
            // local lock to avoid concurrent updates from these tests.
            unsafe {
                std::env::remove_var(key);
            }
            Self { key, original }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            // SAFETY: restores the original value captured before mutation.
            unsafe {
                if let Some(value) = self.original.as_ref() {
                    std::env::set_var(self.key, value);
                } else {
                    std::env::remove_var(self.key);
                }
            }
        }
    }

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
    fn load_returns_default_when_config_file_is_corrupt() {
        let _lock = env_lock().lock().unwrap();

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

        let _env_guard = EnvVarGuard::set(CONFIG_DIR_ENV, &temp_base);
        let cfg = AppConfig::load();
        let _ = fs::remove_dir_all(&temp_base);

        assert_eq!(cfg.focus_secs, crate::timer::DEFAULT_FOCUS_SECS);
        assert_eq!(cfg.short_break_secs, crate::timer::DEFAULT_SHORT_BREAK_SECS);
        assert_eq!(cfg.long_break_secs, crate::timer::DEFAULT_LONG_BREAK_SECS);
        assert!(cfg.blocked_sites.is_empty());
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn config_dir_returns_none_when_home_is_blank_and_xdg_is_unset() {
        let _lock = env_lock().lock().unwrap();
        let _xdg_guard = EnvVarGuard::unset("XDG_CONFIG_HOME");
        let _home_guard = EnvVarGuard::set("HOME", "   ");
        assert!(config_dir().is_none());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn config_dir_returns_none_when_appdata_is_blank() {
        let _lock = env_lock().lock().unwrap();
        let _appdata_guard = EnvVarGuard::set("APPDATA", "   ");
        assert!(config_dir().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn env_path_var_accepts_non_utf8_value() {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};

        let _lock = env_lock().lock().unwrap();
        let non_utf8 = OsString::from_vec(vec![b'/', b't', b'm', b'p', b'/', 0x80, b'x']);
        let _home_guard = EnvVarGuard::set("HOME", &non_utf8);
        let parsed = env_path_var("HOME").expect("non-UTF-8 env var should be accepted");
        assert_eq!(
            parsed.as_os_str().as_bytes(),
            non_utf8.as_os_str().as_bytes()
        );
    }

    #[test]
    fn round_trip_empty_blocked_sites() {
        let original = AppConfig::default();
        let toml_str = toml::to_string_pretty(&original).unwrap();
        let parsed: AppConfig = toml::from_str(&toml_str).unwrap();
        assert!(parsed.blocked_sites.is_empty());
    }
}
