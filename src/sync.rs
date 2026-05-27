use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use getrandom::fill as random_fill;
use pbkdf2::pbkdf2_hmac;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const CONFIG_FILE_NAME: &str = "config.toml";
const STATS_FILE_NAME: &str = "stats.toml";
pub(crate) const SYNC_BUNDLE_FILE_NAME: &str = "focustime-sync.toml";
const SYNC_STATE_FILE_NAME: &str = "sync-state.toml";

const SYNC_BUNDLE_SCHEMA_VERSION: u32 = 1;
const SYNC_STATE_SCHEMA_VERSION: u32 = 1;
const KEY_DERIVATION_ALGORITHM: &str = "pbkdf2-sha256";
const CIPHER_ALGORITHM: &str = "aes-256-gcm";
const KEY_DERIVATION_ITERATIONS: u32 = 210_000;
const KEY_LEN: usize = 32;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const SNAPSHOT_ID_LEN: usize = 16;
const DEVICE_ID_LEN: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SyncBackupResult {
    pub bundle_dir: PathBuf,
    pub bundle_path: PathBuf,
    pub snapshot_id: String,
    pub base_snapshot_id: Option<String>,
    pub device_id: String,
    pub created_at_epoch_secs: i64,
    pub config_hash_sha256: String,
    pub stats_hash_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SyncRestoreResult {
    pub restore_dir: PathBuf,
    pub bundle_path: PathBuf,
    pub snapshot_id: String,
    pub base_snapshot_id: Option<String>,
    pub source_device_id: String,
    pub config_restored_path: PathBuf,
    pub stats_restored_path: PathBuf,
    pub config_hash_sha256: String,
    pub stats_hash_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SyncDiagnostics {
    pub warning: bool,
    pub message: String,
    pub device_id: Option<String>,
    pub last_snapshot_id: Option<String>,
    pub last_success_epoch_secs: Option<i64>,
    pub last_error: Option<String>,
    pub last_error_epoch_secs: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SyncBundleDisk {
    schema_version: u32,
    snapshot_id: String,
    #[serde(default)]
    base_snapshot_id: Option<String>,
    device_id: String,
    created_at_epoch_secs: i64,
    key_derivation: SyncKeyDerivationDisk,
    cipher: SyncCipherDisk,
    payload_hash_sha256: String,
    config_hash_sha256: String,
    stats_hash_sha256: String,
    ciphertext_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SyncKeyDerivationDisk {
    algorithm: String,
    iterations: u32,
    salt_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SyncCipherDisk {
    algorithm: String,
    nonce_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SyncPayloadDisk {
    config_bytes_base64: String,
    stats_bytes_base64: String,
    config_hash_sha256: String,
    stats_hash_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SyncStateDisk {
    #[serde(default = "default_sync_state_schema_version")]
    schema_version: u32,
    #[serde(default)]
    device_id: Option<String>,
    #[serde(default)]
    last_applied_snapshot_id: Option<String>,
    #[serde(default)]
    last_local_config_hash_sha256: Option<String>,
    #[serde(default)]
    last_local_stats_hash_sha256: Option<String>,
    #[serde(default)]
    last_success_epoch_secs: Option<i64>,
    #[serde(default)]
    last_error: Option<String>,
    #[serde(default)]
    last_error_epoch_secs: Option<i64>,
}

impl Default for SyncStateDisk {
    fn default() -> Self {
        Self {
            schema_version: SYNC_STATE_SCHEMA_VERSION,
            device_id: None,
            last_applied_snapshot_id: None,
            last_local_config_hash_sha256: None,
            last_local_stats_hash_sha256: None,
            last_success_epoch_secs: None,
            last_error: None,
            last_error_epoch_secs: None,
        }
    }
}

pub(crate) fn backup_to_dir(dir: &Path, passphrase: &str) -> Result<SyncBackupResult, String> {
    require_nonempty_passphrase(passphrase)?;
    fs::create_dir_all(dir).map_err(|error| {
        format!(
            "Encrypted sync backup failed: could not create `{}`: {error}",
            dir.display()
        )
    })?;

    let source_config = config_file_path()?;
    let source_stats = stats_file_path()?;
    ensure_regular_file(
        &source_config,
        CONFIG_FILE_NAME,
        "Encrypted sync backup failed",
    )?;
    ensure_regular_file(
        &source_stats,
        STATS_FILE_NAME,
        "Encrypted sync backup failed",
    )?;

    let config_bytes = fs::read(&source_config).map_err(|error| {
        format!(
            "Encrypted sync backup failed: could not read `{}`: {error}",
            source_config.display()
        )
    })?;
    let stats_bytes = fs::read(&source_stats).map_err(|error| {
        format!(
            "Encrypted sync backup failed: could not read `{}`: {error}",
            source_stats.display()
        )
    })?;

    let config_hash = sha256_base64(&config_bytes);
    let stats_hash = sha256_base64(&stats_bytes);
    let mut state = load_state()?;
    let device_id = state
        .device_id
        .clone()
        .unwrap_or_else(generate_device_id_for_state);
    let payload = SyncPayloadDisk {
        config_bytes_base64: URL_SAFE_NO_PAD.encode(&config_bytes),
        stats_bytes_base64: URL_SAFE_NO_PAD.encode(&stats_bytes),
        config_hash_sha256: config_hash.clone(),
        stats_hash_sha256: stats_hash.clone(),
    };
    let payload_bytes = serde_json::to_vec(&payload)
        .map_err(|error| format!("Encrypted sync backup failed: payload encode error: {error}"))?;
    let payload_hash = sha256_base64(&payload_bytes);

    let salt = random_bytes::<SALT_LEN>()
        .map_err(|error| format!("Encrypted sync backup failed: {error}"))?;
    let nonce = random_bytes::<NONCE_LEN>()
        .map_err(|error| format!("Encrypted sync backup failed: {error}"))?;
    let key = derive_key(passphrase, &salt);
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|error| format!("Encrypted sync backup failed: cipher setup error: {error}"))?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), payload_bytes.as_ref())
        .map_err(|error| format!("Encrypted sync backup failed: encryption error: {error}"))?;

    let snapshot_id = generate_id(SNAPSHOT_ID_LEN)
        .map_err(|error| format!("Encrypted sync backup failed: {error}"))?;
    let created_at_epoch_secs = current_epoch_secs();
    let bundle = SyncBundleDisk {
        schema_version: SYNC_BUNDLE_SCHEMA_VERSION,
        snapshot_id: snapshot_id.clone(),
        base_snapshot_id: state.last_applied_snapshot_id.clone(),
        device_id: device_id.clone(),
        created_at_epoch_secs,
        key_derivation: SyncKeyDerivationDisk {
            algorithm: KEY_DERIVATION_ALGORITHM.to_string(),
            iterations: KEY_DERIVATION_ITERATIONS,
            salt_base64: URL_SAFE_NO_PAD.encode(salt),
        },
        cipher: SyncCipherDisk {
            algorithm: CIPHER_ALGORITHM.to_string(),
            nonce_base64: URL_SAFE_NO_PAD.encode(nonce),
        },
        payload_hash_sha256: payload_hash,
        config_hash_sha256: config_hash.clone(),
        stats_hash_sha256: stats_hash.clone(),
        ciphertext_base64: URL_SAFE_NO_PAD.encode(ciphertext),
    };
    let bundle_content = toml::to_string_pretty(&bundle)
        .map_err(|error| format!("Encrypted sync backup failed: bundle encode error: {error}"))?;

    let bundle_path = dir.join(SYNC_BUNDLE_FILE_NAME);
    write_atomic_bytes(&bundle_path, bundle_content.as_bytes()).map_err(|error| {
        format!(
            "Encrypted sync backup failed: could not write `{}`: {error}",
            bundle_path.display()
        )
    })?;

    state.device_id = Some(device_id.clone());
    state.last_applied_snapshot_id = Some(snapshot_id.clone());
    state.last_local_config_hash_sha256 = Some(config_hash.clone());
    state.last_local_stats_hash_sha256 = Some(stats_hash.clone());
    state.last_success_epoch_secs = Some(current_epoch_secs());
    state.last_error = None;
    state.last_error_epoch_secs = None;
    save_state(&state)?;

    Ok(SyncBackupResult {
        bundle_dir: dir.to_path_buf(),
        bundle_path,
        snapshot_id,
        base_snapshot_id: bundle.base_snapshot_id,
        device_id,
        created_at_epoch_secs,
        config_hash_sha256: config_hash,
        stats_hash_sha256: stats_hash,
    })
}

pub(crate) fn restore_from_dir(dir: &Path, passphrase: &str) -> Result<SyncRestoreResult, String> {
    require_nonempty_passphrase(passphrase)?;
    let bundle_path = dir.join(SYNC_BUNDLE_FILE_NAME);
    ensure_regular_file(
        &bundle_path,
        SYNC_BUNDLE_FILE_NAME,
        "Encrypted sync restore failed",
    )?;

    let bundle_content = fs::read_to_string(&bundle_path).map_err(|error| {
        format!(
            "Encrypted sync restore failed: could not read `{}`: {error}",
            bundle_path.display()
        )
    })?;
    let bundle: SyncBundleDisk = toml::from_str(&bundle_content)
        .map_err(|error| format!("Encrypted sync restore failed: invalid sync bundle: {error}"))?;
    validate_bundle_schema(&bundle)?;

    let salt = decode_base64_with_context(
        &bundle.key_derivation.salt_base64,
        "Encrypted sync restore failed: invalid key-derivation salt",
    )?;
    let nonce = decode_base64_with_context(
        &bundle.cipher.nonce_base64,
        "Encrypted sync restore failed: invalid cipher nonce",
    )?;
    if nonce.len() != NONCE_LEN {
        return Err(format!(
            "Encrypted sync restore failed: unsupported nonce length {}.",
            nonce.len()
        ));
    }
    let ciphertext = decode_base64_with_context(
        &bundle.ciphertext_base64,
        "Encrypted sync restore failed: invalid encrypted payload",
    )?;
    let key = derive_key_with_iterations(passphrase, &salt, bundle.key_derivation.iterations);
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|error| format!("Encrypted sync restore failed: cipher setup error: {error}"))?;
    let payload_bytes = cipher
        .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
        .map_err(|_| {
            "Encrypted sync restore failed: decryption failed (wrong passphrase or tampered data)."
                .to_string()
        })?;
    let payload_hash = sha256_base64(&payload_bytes);
    if payload_hash != bundle.payload_hash_sha256 {
        return Err(
            "Encrypted sync restore failed: payload integrity mismatch (corrupted bundle)."
                .to_string(),
        );
    }
    let payload: SyncPayloadDisk = serde_json::from_slice(&payload_bytes)
        .map_err(|error| format!("Encrypted sync restore failed: payload decode error: {error}"))?;

    let config_bytes = decode_base64_with_context(
        &payload.config_bytes_base64,
        "Encrypted sync restore failed: invalid config payload",
    )?;
    let stats_bytes = decode_base64_with_context(
        &payload.stats_bytes_base64,
        "Encrypted sync restore failed: invalid stats payload",
    )?;
    verify_payload_hashes(&bundle, &payload, &config_bytes, &stats_bytes)?;

    let config_destination = config_file_path()?;
    let stats_destination = stats_file_path()?;

    let mut state = load_state()?;
    let current_config_hash = read_file_hash_if_exists(&config_destination)?;
    let current_stats_hash = read_file_hash_if_exists(&stats_destination)?;
    if detect_conflict(
        &state,
        &bundle,
        current_config_hash.as_deref(),
        current_stats_hash.as_deref(),
    ) {
        let message = format!(
            "Encrypted sync restore failed: conflict detected because local data changed since snapshot `{}`.",
            state
                .last_applied_snapshot_id
                .as_deref()
                .unwrap_or("unknown")
        );
        record_error_state(&mut state, &message);
        let _ = save_state(&state);
        return Err(message);
    }

    replace_config_and_stats_atomically(
        &config_destination,
        &stats_destination,
        &config_bytes,
        &stats_bytes,
    )?;

    state
        .device_id
        .get_or_insert_with(generate_device_id_for_state);
    state.last_applied_snapshot_id = Some(bundle.snapshot_id.clone());
    state.last_local_config_hash_sha256 = Some(bundle.config_hash_sha256.clone());
    state.last_local_stats_hash_sha256 = Some(bundle.stats_hash_sha256.clone());
    state.last_success_epoch_secs = Some(current_epoch_secs());
    state.last_error = None;
    state.last_error_epoch_secs = None;
    save_state(&state)?;

    Ok(SyncRestoreResult {
        restore_dir: dir.to_path_buf(),
        bundle_path,
        snapshot_id: bundle.snapshot_id,
        base_snapshot_id: bundle.base_snapshot_id,
        source_device_id: bundle.device_id,
        config_restored_path: config_destination,
        stats_restored_path: stats_destination,
        config_hash_sha256: bundle.config_hash_sha256,
        stats_hash_sha256: bundle.stats_hash_sha256,
    })
}

pub(crate) fn diagnostics() -> SyncDiagnostics {
    let state = match load_state() {
        Ok(state) => state,
        Err(error) => {
            return SyncDiagnostics {
                warning: true,
                message: error,
                device_id: None,
                last_snapshot_id: None,
                last_success_epoch_secs: None,
                last_error: None,
                last_error_epoch_secs: None,
            };
        }
    };

    let local_matches_state = match local_hashes_match_state(&state) {
        Ok(matches) => matches,
        Err(error) => {
            return SyncDiagnostics {
                warning: true,
                message: error,
                device_id: state.device_id,
                last_snapshot_id: state.last_applied_snapshot_id,
                last_success_epoch_secs: state.last_success_epoch_secs,
                last_error: state.last_error,
                last_error_epoch_secs: state.last_error_epoch_secs,
            };
        }
    };

    let warning = state.last_error.is_some() || !local_matches_state;
    let message = if let Some(error) = state.last_error.as_deref() {
        format!("Last encrypted sync failed: {error}")
    } else if !local_matches_state {
        "Local config/stats changed since last encrypted sync snapshot.".to_string()
    } else if state.last_applied_snapshot_id.is_some() {
        "Encrypted sync state is healthy.".to_string()
    } else {
        "No encrypted sync snapshot recorded yet.".to_string()
    };

    SyncDiagnostics {
        warning,
        message,
        device_id: state.device_id,
        last_snapshot_id: state.last_applied_snapshot_id,
        last_success_epoch_secs: state.last_success_epoch_secs,
        last_error: state.last_error,
        last_error_epoch_secs: state.last_error_epoch_secs,
    }
}

fn validate_bundle_schema(bundle: &SyncBundleDisk) -> Result<(), String> {
    if bundle.schema_version != SYNC_BUNDLE_SCHEMA_VERSION {
        return Err(format!(
            "Encrypted sync restore failed: unsupported bundle schema version {}.",
            bundle.schema_version
        ));
    }
    if bundle.key_derivation.algorithm != KEY_DERIVATION_ALGORITHM {
        return Err(format!(
            "Encrypted sync restore failed: unsupported key-derivation algorithm `{}`.",
            bundle.key_derivation.algorithm
        ));
    }
    if bundle.cipher.algorithm != CIPHER_ALGORITHM {
        return Err(format!(
            "Encrypted sync restore failed: unsupported cipher algorithm `{}`.",
            bundle.cipher.algorithm
        ));
    }
    if bundle.key_derivation.iterations != KEY_DERIVATION_ITERATIONS {
        return Err(format!(
            "Encrypted sync restore failed: unsupported key-derivation iteration count {}.",
            bundle.key_derivation.iterations
        ));
    }
    Ok(())
}

fn verify_payload_hashes(
    bundle: &SyncBundleDisk,
    payload: &SyncPayloadDisk,
    config_bytes: &[u8],
    stats_bytes: &[u8],
) -> Result<(), String> {
    let config_hash = sha256_base64(config_bytes);
    let stats_hash = sha256_base64(stats_bytes);
    if payload.config_hash_sha256 != config_hash
        || bundle.config_hash_sha256 != config_hash
        || payload.stats_hash_sha256 != stats_hash
        || bundle.stats_hash_sha256 != stats_hash
    {
        return Err(
            "Encrypted sync restore failed: bundle integrity mismatch (corrupted payload)."
                .to_string(),
        );
    }
    Ok(())
}

fn require_nonempty_passphrase(passphrase: &str) -> Result<(), String> {
    if passphrase.trim().is_empty() {
        return Err("Encrypted sync command failed: passphrase cannot be empty.".to_string());
    }
    Ok(())
}

fn config_file_path() -> Result<PathBuf, String> {
    crate::config::app_data_path(CONFIG_FILE_NAME).ok_or_else(|| {
        format!(
            "Encrypted sync command failed: could not determine application data path for `{CONFIG_FILE_NAME}`."
        )
    })
}

fn stats_file_path() -> Result<PathBuf, String> {
    crate::config::stats_data_path(STATS_FILE_NAME).ok_or_else(|| {
        format!(
            "Encrypted sync command failed: could not determine application data path for `{STATS_FILE_NAME}`."
        )
    })
}

fn state_file_path() -> Result<PathBuf, String> {
    crate::config::app_data_path(SYNC_STATE_FILE_NAME).ok_or_else(|| {
        format!(
            "Encrypted sync command failed: could not determine application data path for `{SYNC_STATE_FILE_NAME}`."
        )
    })
}

fn ensure_regular_file(path: &Path, file_name: &str, context: &str) -> Result<(), String> {
    if !path.exists() {
        return Err(format!(
            "{context}: missing `{file_name}` in `{}`.",
            path.parent()
                .map(|parent| parent.display().to_string())
                .unwrap_or_else(|| ".".to_string())
        ));
    }
    if !path.is_file() {
        return Err(format!(
            "{context}: `{}` is not a regular file.",
            path.display()
        ));
    }
    Ok(())
}

fn load_state() -> Result<SyncStateDisk, String> {
    let path = state_file_path()?;
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(SyncStateDisk::default());
        }
        Err(error) => {
            return Err(format!(
                "Encrypted sync diagnostics failed: could not read `{}`: {error}",
                path.display()
            ));
        }
    };
    let state: SyncStateDisk = toml::from_str(&content).map_err(|error| {
        format!(
            "Encrypted sync diagnostics failed: invalid sync state at `{}`: {error}",
            path.display()
        )
    })?;
    if state.schema_version != SYNC_STATE_SCHEMA_VERSION {
        return Err(format!(
            "Encrypted sync diagnostics failed: unsupported sync-state schema version {}.",
            state.schema_version
        ));
    }
    Ok(state)
}

fn save_state(state: &SyncStateDisk) -> Result<(), String> {
    let path = state_file_path()?;
    let content = toml::to_string_pretty(state)
        .map_err(|error| format!("Encrypted sync command failed: state encode error: {error}"))?;
    write_atomic_bytes(&path, content.as_bytes()).map_err(|error| {
        format!(
            "Encrypted sync command failed: could not write `{}`: {error}",
            path.display()
        )
    })
}

fn local_hashes_match_state(state: &SyncStateDisk) -> Result<bool, String> {
    let Some(expected_config_hash) = state.last_local_config_hash_sha256.as_deref() else {
        return Ok(true);
    };
    let Some(expected_stats_hash) = state.last_local_stats_hash_sha256.as_deref() else {
        return Ok(true);
    };
    let config_path = config_file_path()?;
    let stats_path = stats_file_path()?;
    let current_config_hash = read_file_hash_if_exists(&config_path)?;
    let current_stats_hash = read_file_hash_if_exists(&stats_path)?;
    Ok(current_config_hash.as_deref() == Some(expected_config_hash)
        && current_stats_hash.as_deref() == Some(expected_stats_hash))
}

fn read_file_hash_if_exists(path: &Path) -> Result<Option<String>, String> {
    let content = match fs::read(path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!("Failed to read `{}`: {error}", path.display()));
        }
    };
    Ok(Some(sha256_base64(&content)))
}

fn detect_conflict(
    state: &SyncStateDisk,
    bundle: &SyncBundleDisk,
    current_config_hash: Option<&str>,
    current_stats_hash: Option<&str>,
) -> bool {
    let Some(last_snapshot_id) = state.last_applied_snapshot_id.as_deref() else {
        return false;
    };
    let Some(expected_config_hash) = state.last_local_config_hash_sha256.as_deref() else {
        return false;
    };
    let Some(expected_stats_hash) = state.last_local_stats_hash_sha256.as_deref() else {
        return false;
    };
    let local_matches_tracked_snapshot = current_config_hash == Some(expected_config_hash)
        && current_stats_hash == Some(expected_stats_hash);
    if !local_matches_tracked_snapshot {
        return true;
    }
    bundle.base_snapshot_id.as_deref() != Some(last_snapshot_id)
}

fn replace_config_and_stats_atomically(
    config_destination: &Path,
    stats_destination: &Path,
    config_bytes: &[u8],
    stats_bytes: &[u8],
) -> Result<(), String> {
    let staged_config_path = temp_path(config_destination, "staged");
    let staged_stats_path = temp_path(stats_destination, "staged");
    write_atomic_bytes(&staged_config_path, config_bytes).map_err(|error| {
        format!(
            "Encrypted sync restore failed: could not stage config at `{}`: {error}",
            staged_config_path.display()
        )
    })?;
    write_atomic_bytes(&staged_stats_path, stats_bytes).map_err(|error| {
        let _ = remove_file_if_exists(&staged_config_path);
        format!(
            "Encrypted sync restore failed: could not stage stats at `{}`: {error}",
            staged_stats_path.display()
        )
    })?;

    let original_config_snapshot = snapshot_existing_file(config_destination, "config snapshot")?;
    let original_stats_snapshot = snapshot_existing_file(stats_destination, "stats snapshot")?;

    replace_file_atomically(
        &staged_config_path,
        config_destination,
        "restore config.toml",
    )?;
    if let Err(error) =
        replace_file_atomically(&staged_stats_path, stats_destination, "restore stats.toml")
    {
        rollback_restored_file(
            original_config_snapshot.as_deref(),
            config_destination,
            "roll back restored config.toml",
        );
        rollback_restored_file(
            original_stats_snapshot.as_deref(),
            stats_destination,
            "roll back restored stats.toml",
        );
        let _ = remove_file_if_exists(&staged_stats_path);
        return Err(error);
    }

    if let Some(snapshot) = original_config_snapshot.as_deref() {
        let _ = remove_file_if_exists(snapshot);
    }
    if let Some(snapshot) = original_stats_snapshot.as_deref() {
        let _ = remove_file_if_exists(snapshot);
    }
    Ok(())
}

fn write_atomic_bytes(path: &Path, content: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp_path = temp_path(path, "tmp");
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

fn snapshot_existing_file(path: &Path, context: &str) -> Result<Option<PathBuf>, String> {
    if !path.exists() {
        return Ok(None);
    }
    if !path.is_file() {
        return Err(format!(
            "Encrypted sync restore failed: `{}` is not a regular file during {context}.",
            path.display()
        ));
    }
    let snapshot = temp_path(path, "original");
    fs::copy(path, &snapshot).map_err(|error| {
        format!(
            "Encrypted sync restore failed: could not snapshot `{}` to `{}`: {error}",
            path.display(),
            snapshot.display()
        )
    })?;
    Ok(Some(snapshot))
}

fn replace_file_atomically(
    staged_path: &Path,
    destination: &Path,
    context: &str,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        match fs::rename(staged_path, destination) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                fs::remove_file(destination).map_err(|remove_error| {
                    format!(
                        "Encrypted sync restore failed: could not replace `{}` while {context}: {remove_error}",
                        destination.display()
                    )
                })?;
                fs::rename(staged_path, destination).map_err(|rename_error| {
                    format!(
                        "Encrypted sync restore failed: `{}` -> `{}` while {context}: {rename_error}",
                        staged_path.display(),
                        destination.display()
                    )
                })
            }
            Err(error) => {
                let _ = remove_file_if_exists(staged_path);
                Err(format!(
                    "Encrypted sync restore failed: `{}` -> `{}` while {context}: {error}",
                    staged_path.display(),
                    destination.display()
                ))
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        fs::rename(staged_path, destination).map_err(|error| {
            let _ = remove_file_if_exists(staged_path);
            format!(
                "Encrypted sync restore failed: `{}` -> `{}` while {context}: {error}",
                staged_path.display(),
                destination.display()
            )
        })
    }
}

fn rollback_restored_file(snapshot: Option<&Path>, destination: &Path, context: &str) {
    if let Some(snapshot) = snapshot {
        let _ = replace_file_atomically(snapshot, destination, context);
    } else {
        let _ = remove_file_if_exists(destination);
    }
}

fn remove_file_if_exists(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    fs::remove_file(path).map_err(|error| format!("Failed to remove `{}`: {error}", path.display()))
}

fn temp_path(path: &Path, marker: &str) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let target_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("focustime-sync");
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    parent.join(format!(".{target_name}.{pid}.{nanos}.{marker}.tmp"))
}

fn record_error_state(state: &mut SyncStateDisk, message: &str) {
    state.last_error = Some(message.to_string());
    state.last_error_epoch_secs = Some(current_epoch_secs());
}

fn derive_key(passphrase: &str, salt: &[u8]) -> [u8; KEY_LEN] {
    derive_key_with_iterations(passphrase, salt, KEY_DERIVATION_ITERATIONS)
}

fn derive_key_with_iterations(passphrase: &str, salt: &[u8], iterations: u32) -> [u8; KEY_LEN] {
    let mut key = [0u8; KEY_LEN];
    pbkdf2_hmac::<Sha256>(passphrase.as_bytes(), salt, iterations, &mut key);
    key
}

fn sha256_base64(input: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    URL_SAFE_NO_PAD.encode(hasher.finalize())
}

fn decode_base64_with_context(value: &str, context: &str) -> Result<Vec<u8>, String> {
    URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|error| format!("{context}: {error}"))
}

fn random_bytes<const N: usize>() -> Result<[u8; N], String> {
    let mut bytes = [0u8; N];
    random_fill(&mut bytes).map_err(|error| format!("failed to generate random bytes: {error}"))?;
    Ok(bytes)
}

fn generate_id(len: usize) -> Result<String, String> {
    let mut bytes = vec![0u8; len];
    random_fill(&mut bytes).map_err(|error| format!("failed to generate random id: {error}"))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn generate_device_id_for_state() -> String {
    generate_id(DEVICE_ID_LEN).unwrap_or_else(|_| "device-id-unavailable".to_string())
}

fn current_epoch_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or(0)
}

fn default_sync_state_schema_version() -> u32 {
    SYNC_STATE_SCHEMA_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_state() -> SyncStateDisk {
        SyncStateDisk {
            schema_version: SYNC_STATE_SCHEMA_VERSION,
            device_id: Some("dev".to_string()),
            last_applied_snapshot_id: Some("snap-current".to_string()),
            last_local_config_hash_sha256: Some("cfg-hash".to_string()),
            last_local_stats_hash_sha256: Some("stats-hash".to_string()),
            last_success_epoch_secs: Some(10),
            last_error: None,
            last_error_epoch_secs: None,
        }
    }

    fn sample_bundle(base_snapshot_id: Option<&str>) -> SyncBundleDisk {
        SyncBundleDisk {
            schema_version: SYNC_BUNDLE_SCHEMA_VERSION,
            snapshot_id: "snap-next".to_string(),
            base_snapshot_id: base_snapshot_id.map(ToString::to_string),
            device_id: "remote".to_string(),
            created_at_epoch_secs: 20,
            key_derivation: SyncKeyDerivationDisk {
                algorithm: KEY_DERIVATION_ALGORITHM.to_string(),
                iterations: KEY_DERIVATION_ITERATIONS,
                salt_base64: "c2FsdA".to_string(),
            },
            cipher: SyncCipherDisk {
                algorithm: CIPHER_ALGORITHM.to_string(),
                nonce_base64: "bm9uY2U".to_string(),
            },
            payload_hash_sha256: "payload".to_string(),
            config_hash_sha256: "cfg".to_string(),
            stats_hash_sha256: "stats".to_string(),
            ciphertext_base64: "ciphertext".to_string(),
        }
    }

    #[test]
    fn validate_bundle_schema_rejects_zero_kdf_iterations() {
        let mut bundle = sample_bundle(None);
        bundle.key_derivation.iterations = 0;
        let error = validate_bundle_schema(&bundle).expect_err("zero iterations should fail");
        assert!(error.contains("unsupported key-derivation iteration count 0"));
    }

    #[test]
    fn validate_bundle_schema_rejects_unexpected_kdf_iterations() {
        let mut bundle = sample_bundle(None);
        bundle.key_derivation.iterations = KEY_DERIVATION_ITERATIONS + 1;
        let error = validate_bundle_schema(&bundle).expect_err("unexpected iterations should fail");
        assert!(error.contains("unsupported key-derivation iteration count"));
    }

    #[test]
    fn conflict_detection_is_false_when_local_matches_state() {
        let state = sample_state();
        let bundle = sample_bundle(Some("snap-current"));
        assert!(!detect_conflict(
            &state,
            &bundle,
            Some("cfg-hash"),
            Some("stats-hash")
        ));
    }

    #[test]
    fn conflict_detection_is_true_when_local_diverged_even_if_lineage_matches() {
        let state = sample_state();
        let bundle = sample_bundle(Some("snap-current"));
        assert!(detect_conflict(
            &state,
            &bundle,
            Some("cfg-modified"),
            Some("stats-modified")
        ));
    }

    #[test]
    fn conflict_detection_is_true_when_local_diverged_and_base_mismatch() {
        let state = sample_state();
        let bundle = sample_bundle(Some("other-snapshot"));
        assert!(detect_conflict(
            &state,
            &bundle,
            Some("cfg-modified"),
            Some("stats-modified")
        ));
    }
}
