use std::collections::VecDeque;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::Heartbeat;

const HEARTBEAT_QUEUE_SNAPSHOT_FILE_NAME: &str = "wakatime-queue.toml";
pub(super) const HEARTBEAT_QUEUE_SNAPSHOT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct HeartbeatQueueSnapshot {
    pub(super) schema_version: u32,
    pub(super) queued_heartbeats: Vec<Heartbeat>,
    pub(super) in_flight_heartbeat: Option<Heartbeat>,
    pub(super) in_flight_from_queue: bool,
    pub(super) queue_retry_not_before_epoch_secs: Option<u64>,
}

pub(super) fn heartbeat_queue_snapshot_path() -> Option<PathBuf> {
    crate::config::app_data_path(HEARTBEAT_QUEUE_SNAPSHOT_FILE_NAME)
}

pub(super) fn read_heartbeat_queue_snapshot(
    path: &Path,
) -> io::Result<Option<HeartbeatQueueSnapshot>> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let snapshot: HeartbeatQueueSnapshot = toml::from_str(&content).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid queue snapshot format: {error}"),
        )
    })?;
    if snapshot.schema_version != HEARTBEAT_QUEUE_SNAPSHOT_SCHEMA_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "unsupported queue snapshot schema {}",
                snapshot.schema_version
            ),
        ));
    }
    Ok(Some(snapshot))
}

pub(super) fn write_heartbeat_queue_snapshot(
    path: &Path,
    snapshot: &HeartbeatQueueSnapshot,
) -> io::Result<()> {
    let content = toml::to_string_pretty(snapshot)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    write_atomic_text(path, &content)
}

pub(super) fn clear_heartbeat_queue_snapshot(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn write_atomic_text(path: &Path, content: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp_path = path.with_extension("toml.tmp");
    fs::write(&tmp_path, content)?;
    if let Err(error) = sync_file_to_disk(&tmp_path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(error);
    }
    #[cfg(target_os = "windows")]
    {
        match fs::rename(&tmp_path, path) {
            Ok(()) => sync_parent_dir_to_disk(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                fs::remove_file(path)?;
                match fs::rename(&tmp_path, path) {
                    Ok(()) => sync_parent_dir_to_disk(path),
                    Err(error) => {
                        let _ = fs::remove_file(&tmp_path);
                        Err(error)
                    }
                }
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
            Ok(()) => sync_parent_dir_to_disk(path),
            Err(error) => {
                let _ = fs::remove_file(&tmp_path);
                Err(error)
            }
        }
    }
}

fn sync_file_to_disk(path: &Path) -> io::Result<()> {
    let file = fs::OpenOptions::new().write(true).open(path)?;
    file.sync_all()
}

fn sync_parent_dir_to_disk(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        if let Some(parent) = path.parent() {
            let dir = fs::File::open(parent)?;
            dir.sync_all()?;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

pub(super) fn push_back_with_capacity(
    queue: &mut VecDeque<Heartbeat>,
    heartbeat: Heartbeat,
    capacity: usize,
) {
    if queue.len() >= capacity {
        let _ = queue.pop_front();
    }
    queue.push_back(heartbeat);
}

pub(super) fn push_front_with_capacity(
    queue: &mut VecDeque<Heartbeat>,
    heartbeat: Heartbeat,
    capacity: usize,
) {
    if queue.len() >= capacity {
        let _ = queue.pop_front();
    }
    queue.push_front(heartbeat);
}
