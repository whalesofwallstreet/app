use bincode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::fs::{self, File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;
use tracing::{info, warn};

pub const DEFAULT_MAX_WAL_FILE_SIZE: u64 = 50 * 1024 * 1024; // 50MB
pub const DEFAULT_MAX_ARCHIVES: usize = 5;

#[derive(Error, Debug)]
pub enum WalError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] bincode::Error),
    #[error("Corrupted WAL entry at offset {0}: {1}")]
    CorruptedEntry(u64, String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WalEntry {
    RouteCreated {
        id: String,
        user_id: String,
        source_chain: String,
        dest_chain: String,
        source_asset: String,
        dest_asset: String,
        amount_in: u64,
        amount_out: u64,
        provider: String,
        path: String,
        timestamp: u64,
    },
    RouteStatusUpdated {
        id: String,
        status: String,
        timestamp: u64,
    },
    RouteCompleted {
        id: String,
        amount_out: Option<u64>,
        timestamp: u64,
    },
    RouteFailed {
        id: String,
        reason: String,
        timestamp: u64,
    },
}

impl WalEntry {
    pub fn timestamp(&self) -> u64 {
        match self {
            WalEntry::RouteCreated { timestamp, .. } => *timestamp,
            WalEntry::RouteStatusUpdated { timestamp, .. } => *timestamp,
            WalEntry::RouteCompleted { timestamp, .. } => *timestamp,
            WalEntry::RouteFailed { timestamp, .. } => *timestamp,
        }
    }

    pub fn now_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InFlightRouteState {
    pub id: String,
    pub user_id: String,
    pub source_chain: String,
    pub dest_chain: String,
    pub source_asset: String,
    pub dest_asset: String,
    pub amount_in: u64,
    pub amount_out: u64,
    pub provider: String,
    pub path: String,
    pub status: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub error_reason: Option<String>,
}

#[derive(Clone)]
pub struct WalManager {
    wal_dir: PathBuf,
    max_file_size: u64,
    max_archives: usize,
    active_file: Arc<Mutex<Option<File>>>,
}

impl WalManager {
    pub fn new<P: AsRef<Path>>(wal_dir: P) -> Self {
        Self {
            wal_dir: wal_dir.as_ref().to_path_buf(),
            max_file_size: DEFAULT_MAX_WAL_FILE_SIZE,
            max_archives: DEFAULT_MAX_ARCHIVES,
            active_file: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_max_file_size(mut self, bytes: u64) -> Self {
        self.max_file_size = bytes;
        self
    }

    pub fn with_max_archives(mut self, count: usize) -> Self {
        self.max_archives = count;
        self
    }

    pub fn active_file_path(&self) -> PathBuf {
        self.wal_dir.join("active.wal")
    }

    async fn ensure_dir_and_file(&self) -> Result<File, WalError> {
        fs::create_dir_all(&self.wal_dir).await?;
        let active_path = self.active_file_path();
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&active_path)
            .await?;
        Ok(file)
    }

    pub async fn append(&self, entry: &WalEntry) -> Result<(), WalError> {
        let payload = bincode::serialize(entry)?;
        let len = payload.len() as u32;
        let len_bytes = len.to_le_bytes();

        let mut lock = self.active_file.lock().await;
        if lock.is_none() {
            let file = self.ensure_dir_and_file().await?;
            *lock = Some(file);
        }

        let file = lock.as_mut().expect("file initialized above");
        file.write_all(&len_bytes).await?;
        file.write_all(&payload).await?;
        file.flush().await?;

        // Check file size for rotation
        let metadata = file.metadata().await?;
        if metadata.len() >= self.max_file_size {
            // Drop file handle before rotating
            *lock = None;
            self.rotate_active_log_internal().await?;
        }

        Ok(())
    }

    async fn rotate_active_log_internal(&self) -> Result<(), WalError> {
        let active_path = self.active_file_path();
        if !active_path.exists() {
            return Ok(());
        }

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let archive_filename = format!(
            "wal_{}_{}.archive",
            timestamp,
            uuid::Uuid::new_v4().simple()
        );
        let archive_path = self.wal_dir.join(archive_filename);

        fs::rename(&active_path, &archive_path).await?;
        info!("Rotated active WAL log to {:?}", archive_path);

        self.prune_old_archives().await?;
        Ok(())
    }

    pub async fn rotate_active_log(&self) -> Result<(), WalError> {
        let mut lock = self.active_file.lock().await;
        *lock = None;
        self.rotate_active_log_internal().await
    }

    async fn prune_old_archives(&self) -> Result<(), WalError> {
        let mut entries = fs::read_dir(&self.wal_dir).await?;
        let mut archive_paths = Vec::new();

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                if filename.starts_with("wal_") && filename.ends_with(".archive") {
                    archive_paths.push(path);
                }
            }
        }

        // Sort ascending by path name (which has timestamp)
        archive_paths.sort();

        if archive_paths.len() > self.max_archives {
            let to_remove_count = archive_paths.len() - self.max_archives;
            for path in archive_paths.iter().take(to_remove_count) {
                if let Err(err) = fs::remove_file(path).await {
                    warn!("Failed to prune old WAL archive {:?}: {}", path, err);
                } else {
                    info!("Pruned old WAL archive {:?}", path);
                }
            }
        }

        Ok(())
    }

    pub async fn replay(&self) -> Result<HashMap<String, InFlightRouteState>, WalError> {
        let mut in_flight_map: HashMap<String, InFlightRouteState> = HashMap::new();

        if !self.wal_dir.exists() {
            return Ok(in_flight_map);
        }

        // 1. Gather all WAL log files in order (archives first, then active.wal)
        let mut files_to_read = Vec::new();

        let mut read_dir = fs::read_dir(&self.wal_dir).await?;
        let mut archive_files = Vec::new();

        while let Ok(Some(entry)) = read_dir.next_entry().await {
            let path = entry.path();
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                if filename.starts_with("wal_") && filename.ends_with(".archive") {
                    archive_files.push(path);
                }
            }
        }

        archive_files.sort();
        files_to_read.extend(archive_files);

        let active_path = self.active_file_path();
        if active_path.exists() {
            files_to_read.push(active_path);
        }

        // 2. Read each file and replay entries
        for file_path in files_to_read {
            if let Ok(mut file) = File::open(&file_path).await {
                let mut buf = Vec::new();
                if file.read_to_end(&mut buf).await.is_ok() {
                    let mut cursor = 0;
                    while cursor + 4 <= buf.len() {
                        let len_bytes: [u8; 4] = buf[cursor..cursor + 4]
                            .try_into()
                            .expect("slice checked length");
                        let entry_len = u32::from_le_bytes(len_bytes) as usize;

                        if cursor + 4 + entry_len > buf.len() {
                            // Truncated trailing entry (e.g. SIGKILL during write)
                            warn!(
                                "Reached truncated trailing entry in WAL {:?} at offset {}. Ignoring incomplete entry.",
                                file_path, cursor
                            );
                            break;
                        }

                        let payload = &buf[cursor + 4..cursor + 4 + entry_len];
                        cursor += 4 + entry_len;

                        match bincode::deserialize::<WalEntry>(payload) {
                            Ok(entry) => {
                                apply_entry_to_state(&mut in_flight_map, entry);
                            }
                            Err(err) => {
                                warn!(
                                    "Corrupted WAL entry payload in {:?} at offset {}: {}. Skipping.",
                                    file_path, cursor, err
                                );
                            }
                        }
                    }
                }
            }
        }

        Ok(in_flight_map)
    }
}

fn apply_entry_to_state(map: &mut HashMap<String, InFlightRouteState>, entry: WalEntry) {
    match entry {
        WalEntry::RouteCreated {
            id,
            user_id,
            source_chain,
            dest_chain,
            source_asset,
            dest_asset,
            amount_in,
            amount_out,
            provider,
            path,
            timestamp,
        } => {
            let state = InFlightRouteState {
                id: id.clone(),
                user_id,
                source_chain,
                dest_chain,
                source_asset,
                dest_asset,
                amount_in,
                amount_out,
                provider,
                path,
                status: "CREATED".to_string(),
                created_at: timestamp,
                updated_at: timestamp,
                error_reason: None,
            };
            map.insert(id, state);
        }
        WalEntry::RouteStatusUpdated {
            id,
            status,
            timestamp,
        } => {
            if let Some(state) = map.get_mut(&id) {
                state.status = status;
                state.updated_at = timestamp;
            }
        }
        WalEntry::RouteCompleted {
            id,
            amount_out,
            timestamp,
        } => {
            if let Some(state) = map.get_mut(&id) {
                state.status = "COMPLETED".to_string();
                if let Some(out) = amount_out {
                    state.amount_out = out;
                }
                state.updated_at = timestamp;
            }
        }
        WalEntry::RouteFailed {
            id,
            reason,
            timestamp,
        } => {
            if let Some(state) = map.get_mut(&id) {
                state.status = "FAILED".to_string();
                state.error_reason = Some(reason);
                state.updated_at = timestamp;
            }
        }
    }
}
