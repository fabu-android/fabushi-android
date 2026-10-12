use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

pub const ANDROID_STRUCTURED_LOG_SPILL_MAX_BYTES: usize = 1024 * 1024;
pub const ANDROID_STRUCTURED_LOG_SPILL_MAX_AGE_MS: u64 = 18 * 60 * 60 * 1_000;
pub const ANDROID_STRUCTURED_LOG_SPILL_MAX_ENTRIES: usize = 1_000;

const ALLOWED_EVENTS: &[&str] = &[
    "sand.access_blocked",
    "sand.agent_load",
    "sand.agents_unreachable",
    "sand.attachment.edge_failed",
    "sand.box_dns_diagnostic",
    "sand.box_reachability",
    "sand.box_migration_watch",
    "sand.box_rebuild_escalation",
    "sand.box_rebuild_pending_stall",
    "sand.box_rebuild_stage",
    "sand.box_recreate_visible",
    "sand.box_secrets.push",
    "sand.box_setup_visible",
    "sand.connector_auth",
    "sand.desktop.coordinator_lifecycle",
    "sand.desktop.edge_failed",
    "sand.desktop.event_loop",
    "sand.desktop.startup",
    "sand.desktop.process_crash",
    "sand.desktop.renderer_lifecycle",
    "sand.desktop.coordinator_handoff",
    "sand.desktop.local_exec_lifecycle",
    "sand.desktop.session",
    "sand.desktop.unclean_exit",
    "sand.image.edge_failed",
    "sand.mcp.desktop_edge_failed",
    "sand.reaction.ack",
    "sand.recovery_action",
    "sand.render_stream",
    "sand.render_ttfr",
    "sand.replica.resync",
    "sand.resync.completed",
    "sand.send_ack",
    "sand.send",
    "sand.send_journal.restore",
    "sand.telemetry.sink_edge_failed",
    "sand.transport.stream_down",
    "sand.update.apply",
    "sand.update.check",
    "sand.update.outcome",
    "sand.update.prompt",
    "sand.vnc_liveness",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryDropReason {
    ShipFailed,
    OverflowEvicted,
    ReplayExpired,
    BackendDropped,
    AccountRotated,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DropCounter {
    pub observed: u64,
    pub acknowledged_through: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredLogRecord {
    pub level: String,
    pub message: String,
    pub metadata: BTreeMap<String, String>,
    pub timestamp: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredLogCheckpoint {
    pub counter_id: String,
    pub counters: BTreeMap<TelemetryDropReason, DropCounter>,
    pub records: Vec<StructuredLogRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidStructuredLogSpillState {
    pub version: u8,
    pub account_slot: String,
    pub checkpoint: StructuredLogCheckpoint,
}

impl AndroidStructuredLogSpillState {
    pub fn empty(account_slot: impl Into<String>, counter_id: impl Into<String>) -> Self {
        let mut counters = BTreeMap::new();
        for reason in [
            TelemetryDropReason::ShipFailed,
            TelemetryDropReason::OverflowEvicted,
            TelemetryDropReason::ReplayExpired,
            TelemetryDropReason::BackendDropped,
            TelemetryDropReason::AccountRotated,
        ] {
            counters.insert(reason, DropCounter::default());
        }
        Self {
            version: 1,
            account_slot: account_slot.into(),
            checkpoint: StructuredLogCheckpoint {
                counter_id: counter_id.into(),
                counters,
                records: Vec::new(),
            },
        }
    }

    pub fn normalize(mut self, now_ms: u64) -> Self {
        let mut eligible = Vec::with_capacity(self.checkpoint.records.len());
        for record in self.checkpoint.records.drain(..) {
            if !ALLOWED_EVENTS.contains(&record.message.as_str()) {
                continue;
            }
            if now_ms.saturating_sub(record.timestamp) > ANDROID_STRUCTURED_LOG_SPILL_MAX_AGE_MS {
                self.increment(TelemetryDropReason::ReplayExpired, 1);
                continue;
            }
            eligible.push(record);
        }

        if eligible.len() > ANDROID_STRUCTURED_LOG_SPILL_MAX_ENTRIES {
            let evicted = eligible.len() - ANDROID_STRUCTURED_LOG_SPILL_MAX_ENTRIES;
            self.increment(TelemetryDropReason::OverflowEvicted, evicted as u64);
            eligible.drain(..evicted);
        }
        self.checkpoint.records = eligible;

        while !self.checkpoint.records.is_empty()
            && encoded_len(&self) > ANDROID_STRUCTURED_LOG_SPILL_MAX_BYTES
        {
            self.checkpoint.records.remove(0);
            self.increment(TelemetryDropReason::OverflowEvicted, 1);
        }
        self
    }

    pub fn has_pending_drops(&self) -> bool {
        self.checkpoint
            .counters
            .values()
            .any(|counter| counter.observed > counter.acknowledged_through)
    }

    fn increment(&mut self, reason: TelemetryDropReason, count: u64) {
        let counter = self.checkpoint.counters.entry(reason).or_default();
        counter.observed = counter.observed.saturating_add(count);
    }
}

fn encoded_len(state: &AndroidStructuredLogSpillState) -> usize {
    serde_json::to_vec(state).map(|bytes| bytes.len()).unwrap_or(usize::MAX)
}

fn valid_state(state: &AndroidStructuredLogSpillState) -> bool {
    if state.version != 1
        || state.account_slot.is_empty()
        || state.account_slot.chars().count() > 128
        || state.checkpoint.counter_id.is_empty()
        || state.checkpoint.counter_id.chars().count() > 128
        || state.checkpoint.records.len() > ANDROID_STRUCTURED_LOG_SPILL_MAX_ENTRIES
    {
        return false;
    }
    if ![
        TelemetryDropReason::ShipFailed,
        TelemetryDropReason::OverflowEvicted,
        TelemetryDropReason::ReplayExpired,
        TelemetryDropReason::BackendDropped,
        TelemetryDropReason::AccountRotated,
    ]
    .iter()
    .all(|reason| {
        state
            .checkpoint
            .counters
            .get(reason)
            .is_some_and(|counter| counter.acknowledged_through <= counter.observed)
    }) {
        return false;
    }

    state.checkpoint.records.iter().all(|record| {
        matches!(record.level.as_str(), "debug" | "info" | "warn" | "error")
            && !record.message.is_empty()
            && record.message.chars().count() <= 128
            && record.metadata.values().all(|value| value.chars().count() <= 4096)
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpillLoad {
    Empty,
    Loaded(AndroidStructuredLogSpillState),
    AccountRotated,
    InvalidDiscarded,
    OversizeDiscarded,
}

#[derive(Clone, Debug)]
pub struct FileAndroidStructuredLogSpill {
    path: PathBuf,
}

impl FileAndroidStructuredLogSpill {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn load(&self, account_slot: &str, now_ms: u64) -> io::Result<SpillLoad> {
        let metadata = match fs::metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(SpillLoad::Empty),
            Err(error) => return Err(error),
        };
        if metadata.len() as usize > ANDROID_STRUCTURED_LOG_SPILL_MAX_BYTES {
            self.clear()?;
            return Ok(SpillLoad::OversizeDiscarded);
        }

        let raw = fs::read(&self.path)?;
        let state: AndroidStructuredLogSpillState = match serde_json::from_slice(&raw) {
            Ok(state) if valid_state(&state) => state,
            _ => {
                self.clear()?;
                return Ok(SpillLoad::InvalidDiscarded);
            }
        };

        if state.account_slot != account_slot {
            self.clear()?;
            return Ok(SpillLoad::AccountRotated);
        }

        let normalized = state.normalize(now_ms);
        if normalized.checkpoint.records.is_empty() && !normalized.has_pending_drops() {
            self.clear()?;
            return Ok(SpillLoad::Empty);
        }
        if encoded_len(&normalized) != raw.len() {
            self.replace(&normalized, now_ms)?;
        }
        Ok(SpillLoad::Loaded(normalized))
    }

    pub fn replace(
        &self,
        state: &AndroidStructuredLogSpillState,
        now_ms: u64,
    ) -> io::Result<Option<AndroidStructuredLogSpillState>> {
        if !valid_state(state) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid structured log spill state",
            ));
        }
        let normalized = state.clone().normalize(now_ms);
        if normalized.checkpoint.records.is_empty() && !normalized.has_pending_drops() {
            self.clear()?;
            return Ok(None);
        }
        let encoded = serde_json::to_vec(&normalized)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if encoded.len() > ANDROID_STRUCTURED_LOG_SPILL_MAX_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "structured log spill exceeds byte bound after normalization",
            ));
        }
        atomic_replace(&self.path, &encoded)?;
        Ok(Some(normalized))
    }

    pub fn clear(&self) -> io::Result<()> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

fn atomic_replace(path: &Path, encoded: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "spill path has no parent"))?;
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let result = (|| -> io::Result<()> {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(encoded)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-structured-log-spill-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    fn record(timestamp: u64, message: &str) -> StructuredLogRecord {
        StructuredLogRecord {
            level: "warn".into(),
            message: message.into(),
            metadata: BTreeMap::from([("kind".into(), "bounded".into())]),
            timestamp,
        }
    }

    #[test]
    fn spill_replays_only_current_account_and_discards_foreign_slot() {
        let root = root("account");
        let path = root.join("spill.json");
        let spill = FileAndroidStructuredLogSpill::new(&path);
        let mut state = AndroidStructuredLogSpillState::empty("account-a", "counter-a");
        state.checkpoint.records.push(record(1_000, "sand.send"));
        spill.replace(&state, 1_000).unwrap();

        assert_eq!(spill.load("account-b", 1_001).unwrap(), SpillLoad::AccountRotated);
        assert_eq!(spill.load("account-b", 1_002).unwrap(), SpillLoad::Empty);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn spill_expires_old_records_and_keeps_drop_checkpoint() {
        let root = root("expiry");
        let path = root.join("spill.json");
        let spill = FileAndroidStructuredLogSpill::new(&path);
        let now = ANDROID_STRUCTURED_LOG_SPILL_MAX_AGE_MS + 2_000;
        let mut state = AndroidStructuredLogSpillState::empty("account-a", "counter-a");
        state.checkpoint.records.push(record(1, "sand.send"));
        spill.replace(&state, now).unwrap();
        let loaded = match spill.load("account-a", now).unwrap() {
            SpillLoad::Loaded(state) => state,
            other => panic!("expected loaded checkpoint, got {other:?}"),
        };
        assert!(loaded.checkpoint.records.is_empty());
        assert_eq!(
            loaded.checkpoint.counters[&TelemetryDropReason::ReplayExpired].observed,
            1
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn spill_bounds_entry_count_and_uses_atomic_file_replacement() {
        let root = root("bound");
        let path = root.join("spill.json");
        let spill = FileAndroidStructuredLogSpill::new(&path);
        let mut state = AndroidStructuredLogSpillState::empty("account-a", "counter-a");
        for index in 0..(ANDROID_STRUCTURED_LOG_SPILL_MAX_ENTRIES + 10) {
            state
                .checkpoint
                .records
                .push(record(1_000 + index as u64, "sand.send_ack"));
        }
        let stored = spill.replace(&state, 2_000).unwrap().unwrap();
        assert_eq!(
            stored.checkpoint.records.len(),
            ANDROID_STRUCTURED_LOG_SPILL_MAX_ENTRIES
        );
        assert_eq!(
            stored.checkpoint.counters[&TelemetryDropReason::OverflowEvicted].observed,
            10
        );
        assert!(root
            .read_dir()
            .unwrap()
            .all(|entry| !entry.unwrap().file_name().to_string_lossy().contains("tmp-")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_or_oversize_spill_is_deleted_fail_closed() {
        let root = root("invalid");
        let path = root.join("spill.json");
        fs::create_dir_all(&root).unwrap();
        fs::write(&path, b"{bad").unwrap();
        let spill = FileAndroidStructuredLogSpill::new(&path);
        assert_eq!(spill.load("account-a", 1).unwrap(), SpillLoad::InvalidDiscarded);
        fs::write(&path, vec![b'x'; ANDROID_STRUCTURED_LOG_SPILL_MAX_BYTES + 1]).unwrap();
        assert_eq!(spill.load("account-a", 2).unwrap(), SpillLoad::OversizeDiscarded);
        assert!(!path.exists());
        let _ = fs::remove_dir_all(root);
    }
}
