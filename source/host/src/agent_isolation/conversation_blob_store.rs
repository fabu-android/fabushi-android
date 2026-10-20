use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

use super::agent_worker_pool::{
    AgentBlobWorkerBackend, AgentWorkerFuture, ConversationGarbageCollectionOutcome,
    LegacyBlobRetirementVerdict,
};
use super::conversation_blob_gc::collect_reachable_blob_hex_ids;
use super::conversation_blob_db::{
    open_conversation_blob_db, read_conversation_blob_migration_state,
    set_conversation_blob_migration_state, ConversationBlobDbError,
    ConversationBlobMigrationState,
};
use super::legacy_blob_retirement::verify_legacy_blob_retirement;
use crate::extensions::session::conversation_recovery::{
    find_latest_root_blob_id_in_database, parse_conversation_state_structure,
};

const MAX_STALE_ROOT_SCAN_BYTES: usize = 64 * 1024 * 1024;
const MAX_TRACKED_RECENT_WRITES: usize = 16_384;
const VACUUM_MIN_DELETED_BYTES: u64 = 64 * 1024 * 1024;
const VACUUM_DELETED_SHARE_DENOMINATOR: u64 = 8;

#[derive(Debug, thiserror::Error)]
pub enum ConversationBlobStoreError {
    #[error(transparent)]
    Database(#[from] ConversationBlobDbError),
    #[error("conversation blob sqlite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("conversation blob worker backend mutex poisoned")]
    Poisoned,
}

pub struct ConversationBlobStoreDb {
    connection: Connection,
    recent_write_ms_by_hex_id: HashMap<String, u64>,
}

impl ConversationBlobStoreDb {
    pub fn open(
        blob_db_path: &Path,
        legacy_blob_db_path: Option<&Path>,
        busy_timeout_ms: u64,
    ) -> Result<Self, ConversationBlobStoreError> {
        let connection = open_conversation_blob_db(blob_db_path, busy_timeout_ms)?;
        let mut store = Self {
            connection,
            recent_write_ms_by_hex_id: HashMap::new(),
        };
        if let Some(legacy_blob_db_path) = legacy_blob_db_path {
            let _ = store.adopt_legacy_blobs(legacy_blob_db_path);
        }
        Ok(store)
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    fn adopt_legacy_blobs(
        &mut self,
        legacy_blob_db_path: &Path,
    ) -> Result<bool, ConversationBlobStoreError> {
        let state = read_conversation_blob_migration_state(&self.connection)?;
        if !matches!(
            state,
            ConversationBlobMigrationState::Unstarted
                | ConversationBlobMigrationState::RecoveryRebuilt
        ) || !legacy_blob_db_path.exists()
        {
            return Ok(false);
        }

        let path = legacy_blob_db_path.to_string_lossy().to_string();
        if self
            .connection
            .execute("ATTACH DATABASE ?1 AS legacy", params![path])
            .is_err()
        {
            return Ok(false);
        }
        let adopted = self
            .connection
            .execute_batch(
                "INSERT OR IGNORE INTO blobs (id, data) SELECT id, data FROM legacy.blobs",
            )
            .is_ok();
        let _ = self.connection.execute_batch("DETACH DATABASE legacy");
        if !adopted {
            return Ok(false);
        }
        set_conversation_blob_migration_state(
            &self.connection,
            ConversationBlobMigrationState::AdoptionComplete,
        )?;
        Ok(true)
    }

    pub fn get_blob(&self, id: &[u8]) -> Result<Option<Vec<u8>>, ConversationBlobStoreError> {
        let id = to_hex(id);
        let mut statement = self
            .connection
            .prepare("SELECT data FROM blobs WHERE id = ?1")?;
        let mut rows = statement.query(params![id])?;
        Ok(match rows.next()? {
            Some(row) => Some(row.get::<_, Vec<u8>>(0)?),
            None => None,
        })
    }

    pub fn set_blob(
        &mut self,
        id: &[u8],
        data: &[u8],
    ) -> Result<(), ConversationBlobStoreError> {
        let id = to_hex(id);
        self.connection.execute(
            "INSERT INTO blobs (id, data) VALUES (?1, ?2)
             ON CONFLICT(id) DO UPDATE SET data = excluded.data",
            params![id, data],
        )?;
        self.recent_write_ms_by_hex_id.insert(id, now_ms());
        if self.recent_write_ms_by_hex_id.len() > MAX_TRACKED_RECENT_WRITES {
            if let Some(oldest) = self
                .recent_write_ms_by_hex_id
                .iter()
                .min_by_key(|(_, timestamp)| **timestamp)
                .map(|(id, _)| id.clone())
            {
                self.recent_write_ms_by_hex_id.remove(&oldest);
            }
        }
        Ok(())
    }

    pub fn clear_blobs(&mut self) -> Result<(), ConversationBlobStoreError> {
        self.connection.execute("DELETE FROM blobs", [])?;
        self.recent_write_ms_by_hex_id.clear();
        Ok(())
    }

    pub fn find_latest_root_blob_id(
        &self,
    ) -> Result<Option<Vec<u8>>, ConversationBlobStoreError> {
        Ok(find_latest_root_blob_id_in_database(&self.connection)?)
    }

    pub fn clear_stale_checkpoint_roots(
        &mut self,
        retained_root_id_hex: &str,
    ) -> Result<usize, ConversationBlobStoreError> {
        let mut present_ids = HashSet::new();
        let mut candidates = Vec::new();
        {
            let mut statement = self
                .connection
                .prepare("SELECT id, length(data) FROM blobs")?;
            let rows = statement.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (id, raw_len) = row?;
                present_ids.insert(id.clone());
                let len = raw_len.max(0) as usize;
                if id != retained_root_id_hex && len > 0 && len <= MAX_STALE_ROOT_SCAN_BYTES {
                    candidates.push(id);
                }
            }
        }

        let mut stale = Vec::new();
        for id in candidates {
            let Some(data) = self.get_blob_by_hex_id(&id)? else {
                continue;
            };
            let digest = format!("{:x}", Sha256::digest(&data));
            if digest != id {
                continue;
            }
            let Some(parsed) = parse_conversation_state_structure(&data) else {
                continue;
            };
            if !parsed.turns.is_empty()
                && parsed.turns.iter().all(|turn| {
                    turn.len() == 32 && present_ids.contains(&to_hex(turn))
                })
            {
                stale.push(id);
            }
        }

        if stale.is_empty() {
            return Ok(0);
        }
        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        let mut deleted = 0usize;
        let delete_result = (|| -> Result<(), rusqlite::Error> {
            let mut statement = self
                .connection
                .prepare("DELETE FROM blobs WHERE id = ?1")?;
            for id in &stale {
                deleted += statement.execute(params![id])?;
            }
            Ok(())
        })();
        match delete_result {
            Ok(()) => self.connection.execute_batch("COMMIT")?,
            Err(error) => {
                let _ = self.connection.execute_batch("ROLLBACK");
                return Err(error.into());
            }
        }
        Ok(deleted)
    }

    pub fn collect_garbage(
        &mut self,
        retained_root_id_hex: &str,
        pending_write_retention_ms: u64,
    ) -> Result<ConversationGarbageCollectionOutcome, ConversationBlobStoreError> {
        let Some(root_bytes) = self.get_blob_by_hex_id(retained_root_id_hex)? else {
            return Ok(ConversationGarbageCollectionOutcome::Skipped {
                reason: "no-root".into(),
                unresolved_proto_refs: 0,
            });
        };

        let mut lookup_error = None;
        let walk = collect_reachable_blob_hex_ids(&root_bytes, |id| {
            match self.get_blob_by_hex_id(id) {
                Ok(value) => value,
                Err(error) => {
                    if lookup_error.is_none() {
                        lookup_error = Some(error);
                    }
                    None
                }
            }
        });
        if let Some(error) = lookup_error {
            return Err(error);
        }
        let walk = match walk {
            Ok(walk) => walk,
            Err(()) => {
                return Ok(ConversationGarbageCollectionOutcome::Skipped {
                    reason: "root-undecodable".into(),
                    unresolved_proto_refs: 0,
                });
            }
        };
        if walk.unresolved_proto_refs > 0 {
            return Ok(ConversationGarbageCollectionOutcome::Skipped {
                reason: "unresolved-refs".into(),
                unresolved_proto_refs: walk.unresolved_proto_refs,
            });
        }

        let floor = now_ms().saturating_sub(pending_write_retention_ms);
        let mut deletable = Vec::new();
        let mut deleted_bytes = 0u64;
        let mut live_rows = 0usize;
        let mut live_bytes = 0u64;
        let mut retained_pending_rows = 0usize;
        {
            let mut statement = self
                .connection
                .prepare("SELECT id, length(data) FROM blobs")?;
            let rows = statement.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (id, raw_len) = row?;
                let len = raw_len.max(0) as u64;
                if id == retained_root_id_hex || walk.reachable_hex_ids.contains(&id) {
                    live_rows += 1;
                    live_bytes = live_bytes.saturating_add(len);
                    continue;
                }
                if self
                    .recent_write_ms_by_hex_id
                    .get(&id)
                    .is_some_and(|last_write_ms| *last_write_ms > floor)
                {
                    retained_pending_rows += 1;
                    live_rows += 1;
                    live_bytes = live_bytes.saturating_add(len);
                    continue;
                }
                deleted_bytes = deleted_bytes.saturating_add(len);
                deletable.push(id);
            }
        }

        if !deletable.is_empty() {
            self.connection.execute_batch("BEGIN IMMEDIATE")?;
            let delete_result = (|| -> Result<(), rusqlite::Error> {
                let mut statement = self
                    .connection
                    .prepare("DELETE FROM blobs WHERE id = ?1")?;
                for id in &deletable {
                    let _ = statement.execute(params![id])?;
                }
                Ok(())
            })();
            match delete_result {
                Ok(()) => self.connection.execute_batch("COMMIT")?,
                Err(error) => {
                    let _ = self.connection.execute_batch("ROLLBACK");
                    return Err(error.into());
                }
            }
            for id in &deletable {
                self.recent_write_ms_by_hex_id.remove(id);
            }
        }

        let total_scanned_bytes = deleted_bytes.saturating_add(live_bytes);
        let vacuumed = deleted_bytes >= VACUUM_MIN_DELETED_BYTES
            || (deleted_bytes > 0
                && deleted_bytes.saturating_mul(VACUUM_DELETED_SHARE_DENOMINATOR)
                    >= total_scanned_bytes);
        if vacuumed {
            self.connection.execute_batch("VACUUM")?;
        }
        self.connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;

        Ok(ConversationGarbageCollectionOutcome::Collected {
            deleted_rows: deletable.len(),
            deleted_bytes,
            live_rows,
            live_bytes,
            retained_pending_rows,
            vacuumed,
        })
    }

    pub fn verify_legacy_blob_retirement(
        &self,
        retained_root_id_hex: &str,
        legacy_blob_db_path: &Path,
    ) -> LegacyBlobRetirementVerdict {
        verify_legacy_blob_retirement(
            &self.connection,
            legacy_blob_db_path,
            retained_root_id_hex,
        )
    }

    fn get_blob_by_hex_id(
        &self,
        id: &str,
    ) -> Result<Option<Vec<u8>>, ConversationBlobStoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT data FROM blobs WHERE id = ?1")?;
        let mut rows = statement.query(params![id])?;
        Ok(match rows.next()? {
            Some(row) => Some(row.get::<_, Vec<u8>>(0)?),
            None => None,
        })
    }
}

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}


fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

pub struct ConversationBlobWorkerBackend {
    busy_timeout_ms: u64,
    stores: Mutex<HashMap<PathBuf, ConversationBlobStoreDb>>,
}

impl ConversationBlobWorkerBackend {
    pub fn new(busy_timeout_ms: u64) -> Self {
        Self {
            busy_timeout_ms,
            stores: Mutex::new(HashMap::new()),
        }
    }

    pub fn active_store_count(&self) -> usize {
        self.stores
            .lock()
            .map(|stores| stores.len())
            .unwrap_or_default()
    }

    pub fn close_store(&self, blob_db_path: &Path) -> Result<(), ConversationBlobStoreError> {
        self.stores
            .lock()
            .map_err(|_| ConversationBlobStoreError::Poisoned)?
            .remove(blob_db_path);
        Ok(())
    }

    fn with_store<T>(
        &self,
        blob_db_path: &Path,
        legacy_blob_db_path: Option<&Path>,
        operation: impl FnOnce(&mut ConversationBlobStoreDb) -> Result<T, ConversationBlobStoreError>,
    ) -> Result<T, ConversationBlobStoreError> {
        let mut stores = self
            .stores
            .lock()
            .map_err(|_| ConversationBlobStoreError::Poisoned)?;
        if !stores.contains_key(blob_db_path) {
            let store = ConversationBlobStoreDb::open(
                blob_db_path,
                legacy_blob_db_path,
                self.busy_timeout_ms,
            )?;
            stores.insert(blob_db_path.to_path_buf(), store);
        }
        operation(
            stores
                .get_mut(blob_db_path)
                .expect("store inserted before operation"),
        )
    }
}

impl Default for ConversationBlobWorkerBackend {
    fn default() -> Self {
        Self::new(5_000)
    }
}

impl AgentBlobWorkerBackend for ConversationBlobWorkerBackend {
    type Error = ConversationBlobStoreError;

    fn get_blob<'a>(
        &'a self,
        _agent_id: &'a str,
        blob_db_path: &'a Path,
        blob_id: &'a [u8],
        legacy_blob_db_path: Option<&'a Path>,
    ) -> AgentWorkerFuture<'a, Result<Option<Vec<u8>>, Self::Error>> {
        let result = self.with_store(blob_db_path, legacy_blob_db_path, |store| {
            store.get_blob(blob_id)
        });
        Box::pin(async move { result })
    }

    fn set_blob<'a>(
        &'a self,
        _agent_id: &'a str,
        blob_db_path: &'a Path,
        blob_id: &'a [u8],
        blob_data: &'a [u8],
        legacy_blob_db_path: Option<&'a Path>,
    ) -> AgentWorkerFuture<'a, Result<(), Self::Error>> {
        let result = self.with_store(blob_db_path, legacy_blob_db_path, |store| {
            store.set_blob(blob_id, blob_data)
        });
        Box::pin(async move { result })
    }

    fn find_latest_root_blob_id<'a>(
        &'a self,
        _agent_id: &'a str,
        blob_db_path: &'a Path,
        legacy_blob_db_path: Option<&'a Path>,
    ) -> AgentWorkerFuture<'a, Result<Option<Vec<u8>>, Self::Error>> {
        let result = self.with_store(blob_db_path, legacy_blob_db_path, |store| {
            store.find_latest_root_blob_id()
        });
        Box::pin(async move { result })
    }

    fn clear_blobs<'a>(
        &'a self,
        _agent_id: &'a str,
        blob_db_path: &'a Path,
        legacy_blob_db_path: Option<&'a Path>,
    ) -> AgentWorkerFuture<'a, Result<(), Self::Error>> {
        let result = self.with_store(blob_db_path, legacy_blob_db_path, |store| {
            store.clear_blobs()
        });
        Box::pin(async move { result })
    }

    fn clear_stale_checkpoint_roots<'a>(
        &'a self,
        _agent_id: &'a str,
        blob_db_path: &'a Path,
        retained_root_id_hex: &'a str,
        legacy_blob_db_path: Option<&'a Path>,
    ) -> AgentWorkerFuture<'a, Result<usize, Self::Error>> {
        let result = self.with_store(blob_db_path, legacy_blob_db_path, |store| {
            store.clear_stale_checkpoint_roots(retained_root_id_hex)
        });
        Box::pin(async move { result })
    }

    fn collect_conversation_garbage<'a>(
        &'a self,
        _agent_id: &'a str,
        blob_db_path: &'a Path,
        retained_root_id_hex: &'a str,
        pending_write_retention_ms: u64,
        legacy_blob_db_path: Option<&'a Path>,
    ) -> AgentWorkerFuture<'a, Result<ConversationGarbageCollectionOutcome, Self::Error>> {
        let result = self.with_store(blob_db_path, legacy_blob_db_path, |store| {
            store.collect_garbage(retained_root_id_hex, pending_write_retention_ms)
        });
        Box::pin(async move { result })
    }

    fn verify_legacy_blob_retirement<'a>(
        &'a self,
        _agent_id: &'a str,
        blob_db_path: &'a Path,
        retained_root_id_hex: &'a str,
        legacy_blob_db_path: &'a Path,
    ) -> AgentWorkerFuture<'a, Result<LegacyBlobRetirementVerdict, Self::Error>> {
        let result = self.with_store(blob_db_path, Some(legacy_blob_db_path), |store| {
            Ok(store.verify_legacy_blob_retirement(
                retained_root_id_hex,
                legacy_blob_db_path,
            ))
        });
        Box::pin(async move { result })
    }

    fn close_store<'a>(
        &'a self,
        _agent_id: &'a str,
        blob_db_path: &'a Path,
    ) -> AgentWorkerFuture<'a, Result<(), Self::Error>> {
        let result = ConversationBlobWorkerBackend::close_store(self, blob_db_path);
        Box::pin(async move { result })
    }
}
