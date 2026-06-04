#[cfg(test)]
use std::cell::RefCell;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::HostsFileDiagnostics;

#[cfg(target_os = "windows")]
pub(super) const HOSTS_FILE: &str = r"C:\Windows\System32\drivers\etc\hosts";
#[cfg(not(target_os = "windows"))]
pub(super) const HOSTS_FILE: &str = "/etc/hosts";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HostsWriteFailStep {
    Snapshot,
    StageWrite,
    AfterReplace,
    RollbackRestore,
}

#[cfg(test)]
thread_local! {
    static TEST_HOSTS_WRITE_FAIL_STEPS: RefCell<Vec<HostsWriteFailStep>> = const { RefCell::new(Vec::new()) };
}

#[cfg(test)]
pub(super) fn set_test_hosts_write_fail_steps(steps: &[HostsWriteFailStep]) {
    TEST_HOSTS_WRITE_FAIL_STEPS.with(|slot| {
        *slot.borrow_mut() = steps.to_vec();
    });
}

pub(super) fn hosts_file_diagnostics_for(path: &Path) -> HostsFileDiagnostics {
    let read_error = fs::File::open(path).err().map(|error| error.to_string());
    let write_error = probe_hosts_write_path(path)
        .err()
        .map(|error| error.to_string());
    HostsFileDiagnostics {
        path: path.display().to_string(),
        read_error,
        write_error,
    }
}

#[cfg(target_os = "windows")]
fn probe_hosts_write_path(path: &Path) -> io::Result<()> {
    OpenOptions::new().append(true).open(path).map(|_| ())
}

#[cfg(not(target_os = "windows"))]
fn probe_hosts_write_path(path: &Path) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let probe = dir.join(format!(
        ".focustime_hosts_probe_{}_{}",
        std::process::id(),
        nanos
    ));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)?;
    fs::remove_file(&probe)
}

enum HostsReplaceError {
    NoMutation(io::Error),
    #[cfg(target_os = "windows")]
    NeedsRollback(io::Error),
}

/// Update the hosts file via staged replacement and rollback, deriving next
/// content from the snapshotted source to avoid TOCTOU races.
pub(super) fn atomic_write_hosts_to_path<F>(hosts_path: &Path, build_content: F) -> io::Result<bool>
where
    F: FnOnce(&str) -> Option<String>,
{
    let staged_path = hosts_transaction_temp_path(hosts_path, "staged");
    let snapshot_path = hosts_transaction_temp_path(hosts_path, "snapshot");

    create_hosts_snapshot(hosts_path, &snapshot_path)?;
    let snapshot_content = match fs::read_to_string(&snapshot_path) {
        Ok(content) => content,
        Err(error) => {
            let _ = remove_file_if_exists(&staged_path);
            let _ = remove_file_if_exists(&snapshot_path);
            return Err(error);
        }
    };
    let Some(content) = build_content(&snapshot_content) else {
        let _ = remove_file_if_exists(&staged_path);
        let _ = remove_file_if_exists(&snapshot_path);
        return Ok(false);
    };

    if let Err(error) = write_staged_hosts_file(hosts_path, &staged_path, &content) {
        let _ = remove_file_if_exists(&staged_path);
        let _ = remove_file_if_exists(&snapshot_path);
        return Err(error);
    }

    match replace_hosts_file(&staged_path, hosts_path) {
        Ok(()) => {}
        Err(HostsReplaceError::NoMutation(error)) => {
            let _ = remove_file_if_exists(&staged_path);
            let _ = remove_file_if_exists(&snapshot_path);
            return Err(error);
        }
        #[cfg(target_os = "windows")]
        Err(HostsReplaceError::NeedsRollback(error)) => {
            return rollback_hosts_write(hosts_path, &snapshot_path, &staged_path, error)
                .map(|()| true);
        }
    }

    if let Err(error) = maybe_fail_hosts_write_step(HostsWriteFailStep::AfterReplace) {
        return rollback_hosts_write(hosts_path, &snapshot_path, &staged_path, error)
            .map(|()| true);
    }

    cleanup_temp_file_best_effort(&staged_path);
    cleanup_temp_file_best_effort(&snapshot_path);
    Ok(true)
}

fn rollback_hosts_write(
    hosts_path: &Path,
    snapshot_path: &Path,
    staged_path: &Path,
    update_error: io::Error,
) -> io::Result<()> {
    let rollback_result = restore_hosts_snapshot(hosts_path, snapshot_path);
    let _ = remove_file_if_exists(staged_path);
    let _ = remove_file_if_exists(snapshot_path);

    match rollback_result {
        Ok(()) => Err(io::Error::new(
            update_error.kind(),
            format!("failed to update hosts file: {update_error}; previous state restored"),
        )),
        Err(rollback_error) => Err(io::Error::other(format!(
            "failed to update hosts file: {update_error}; rollback failed: {rollback_error}"
        ))),
    }
}

fn create_hosts_snapshot(hosts_path: &Path, snapshot_path: &Path) -> io::Result<()> {
    maybe_fail_hosts_write_step(HostsWriteFailStep::Snapshot)?;
    fs::copy(hosts_path, snapshot_path).map(|_| ())
}

fn write_staged_hosts_file(hosts_path: &Path, staged_path: &Path, content: &str) -> io::Result<()> {
    maybe_fail_hosts_write_step(HostsWriteFailStep::StageWrite)?;
    fs::write(staged_path, content)?;
    // Copy the original file's permissions onto the staged file so replacement
    // does not silently change the hosts file mode bits.
    if let Ok(meta) = fs::metadata(hosts_path) {
        let _ = fs::set_permissions(staged_path, meta.permissions());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn replace_hosts_file(staged_path: &Path, hosts_path: &Path) -> Result<(), HostsReplaceError> {
    match fs::rename(staged_path, hosts_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            fs::remove_file(hosts_path).map_err(HostsReplaceError::NoMutation)?;
            fs::rename(staged_path, hosts_path).map_err(HostsReplaceError::NeedsRollback)
        }
        Err(error) => Err(HostsReplaceError::NoMutation(error)),
    }
}

#[cfg(not(target_os = "windows"))]
fn replace_hosts_file(staged_path: &Path, hosts_path: &Path) -> Result<(), HostsReplaceError> {
    fs::rename(staged_path, hosts_path).map_err(HostsReplaceError::NoMutation)
}

fn restore_hosts_snapshot(hosts_path: &Path, snapshot_path: &Path) -> io::Result<()> {
    maybe_fail_hosts_write_step(HostsWriteFailStep::RollbackRestore)?;
    restore_snapshot_file(snapshot_path, hosts_path)
}

#[cfg(target_os = "windows")]
fn restore_snapshot_file(snapshot_path: &Path, hosts_path: &Path) -> io::Result<()> {
    match fs::rename(snapshot_path, hosts_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            fs::remove_file(hosts_path)?;
            fs::rename(snapshot_path, hosts_path)
        }
        Err(error) => Err(error),
    }
}

#[cfg(not(target_os = "windows"))]
fn restore_snapshot_file(snapshot_path: &Path, hosts_path: &Path) -> io::Result<()> {
    fs::rename(snapshot_path, hosts_path)
}

fn hosts_transaction_temp_path(hosts_path: &Path, marker: &str) -> PathBuf {
    let dir = hosts_path.parent().unwrap_or(Path::new("."));
    let target_name = hosts_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("hosts");
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    dir.join(format!(
        ".{target_name}.focustime.{pid}.{nanos}.{marker}.tmp"
    ))
}

fn remove_file_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn cleanup_temp_file_best_effort(path: &Path) {
    if let Err(error) = remove_file_if_exists(path) {
        eprintln!(
            "Warning: failed to remove hosts temp file `{}`: {error}",
            path.display()
        );
    }
}

fn maybe_fail_hosts_write_step(step: HostsWriteFailStep) -> io::Result<()> {
    #[cfg(test)]
    {
        let should_fail = TEST_HOSTS_WRITE_FAIL_STEPS.with(|slot| {
            let mut configured = slot.borrow_mut();
            if configured.first().copied() == Some(step) {
                configured.remove(0);
                true
            } else {
                false
            }
        });
        if should_fail {
            return Err(io::Error::other(format!(
                "simulated hosts write failure at {step:?}"
            )));
        }
    }
    #[cfg(not(test))]
    {
        let _ = step;
    }

    Ok(())
}

/// Flush the OS DNS cache so /etc/hosts changes take effect immediately.
/// Best-effort: failures are silently ignored.
pub(super) fn flush_dns_cache() {
    #[cfg(target_os = "macos")]
    {
        // Flush mDNSResponder cache (macOS 10.10.4+)
        let _ = Command::new("dscacheutil").arg("-flushcache").status();
        let _ = Command::new("killall")
            .args(["-HUP", "mDNSResponder"])
            .status();
    }
    #[cfg(target_os = "linux")]
    {
        // systemd-resolved
        let _ = Command::new("systemd-resolve")
            .arg("--flush-caches")
            .status();
        // nscd (older systems)
        let _ = Command::new("nscd").args(["-i", "hosts"]).status();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("ipconfig").arg("/flushdns").status();
    }
}
