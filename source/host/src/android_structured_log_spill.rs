use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

pub const ANDROID_STRUCTURED_LOG_SPILL_MAX_BYTES: usize = 1024 * 1024;
// Preserve the Desktop structured-log replay/skew contract through the shared constants owner.
pub const ANDROID_STRUCTURED_LOG_SPILL_MAX_AGE_MS: u64 =
    fabushi_constants::structured_log::STRUCTURED_LOG_REPLAY_MAX_AGE_MS;
pub const ANDROID_STRUCTURED_LOG_FUTURE_TIMESTAMP_MAX_SKEW_MS: u64 =
    fabushi_constants::structured_log::STRUCTURED_LOG_FUTURE_TIMESTAMP_MAX_SKEW_MS;
pub const ANDROID_STRUCTURED_LOG_SPILL_MAX_ENTRIES: usize = 1_000;
pub const ANDROID_STRUCTURED_LOG_BATCH_SIZE: usize = 128;
pub const ANDROID_STRUCTURED_LOG_RECOVERY_ID_CAP: usize = 256;

const ALLOWED_EVENTS: &[&str] = &[
    "sand.access_blocked",
    "sand.ack.obligation",
    "sand.agent_load",
    "sand.agent.error",
    "sand.agent.error.detail",
    "sand.agent.open",
    "sand.agents_unreachable",
    "sand.attachment.edge_failed",
    "sand.automation.lifecycle",
    "sand.bot_block",
    "sand.bot_block_detail",
    "sand.box_copy_in",
    "sand.box_dns_diagnostic",
    "sand.box.log_ship",
    "sand.box_reachability",
    "sand.box_migration_watch",
    "sand.box_rebuild_escalation",
    "sand.box_rebuild_pending_stall",
    "sand.box_rebuild_stage",
    "sand.box_recreate_visible",
    "sand.box.recreate_decided",
    "sand.box_secrets.push",
    "sand.box_setup_visible",
    "sand.connector_auth",
    "sand.client_resource",
    "sand.renderer_unhandled",
    "sand.renderer.boundary_caught",
    "sand.entrypoint_error",
    "sand.send.queued_flush",
    "sand.client_persistence",
    "sand.desktop.coordinator_lifecycle",
    "sand.desktop.edge_failed",
    "sand.desktop.event_loop",
    "sand.host.event_loop",
    "sand.host.invariant_violation",
    "sand.gateway_command_error",
    "sand.gateway_command_timing",
    "sand.desktop.startup",
    "sand.desktop.process_crash",
    "sand.desktop.renderer_lifecycle",
    "sand.desktop.coordinator_handoff",
    "sand.desktop.local_exec_lifecycle",
    "sand.desktop.session",
    "sand.desktop.signin",
    "sand.desktop.unclean_exit",
    "sand.image.edge_failed",
    "sand.local_exec.refused",
    "sand.local_exec.exec_failed",
    "sand.local_exec.provider",
    "sand.mcp.desktop_edge_failed",
    "sand.pending_wake",
    "sand.queue.watchdog",
    "sand.queue.dequeued",
    "sand.queue.accepted",
    "sand.reaction.ack",
    "sand.recovery_action",
    "sand.render_stream",
    "sand.render_ttfr",
    "sand.replica.resync",
    "sand.resync.completed",
    "sand.send_ack",
    "sand.send",
    "sand.send_journal.restore",
    "sand.session.summary_build",
    "sand.session.materialize",
    "sand.session.maintenance",
    "sand.session.store_db",
    "sand.memory.synthesis",
    "sand.telemetry.sink_edge_failed",
    "sand.turn.interrupt",
    "sand.turn.await",
    "sand.turn.retry",
    "sand.turn.closing_send_nudge",
    "sand.user_message.received",
    "sand.computer_use.usage",
    "sand.ttft",
    "sand.turn.usage",
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
    #[serde(default)]
    pub recovery_ids: Vec<String>,
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
                recovery_ids: Vec::new(),
            },
        }
    }

    pub fn normalize(mut self, now_ms: u64) -> Self {
        let records = std::mem::take(&mut self.checkpoint.records);
        let mut eligible = Vec::with_capacity(records.len());
        for record in records {
            if !ALLOWED_EVENTS.contains(&record.message.as_str()) {
                continue;
            }
            if !fabushi_constants::structured_log::structured_log_timestamp_allowed(
                now_ms,
                record.timestamp,
            ) {
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
        if self.checkpoint.recovery_ids.len() > ANDROID_STRUCTURED_LOG_RECOVERY_ID_CAP {
            let drain = self.checkpoint.recovery_ids.len() - ANDROID_STRUCTURED_LOG_RECOVERY_ID_CAP;
            self.checkpoint.recovery_ids.drain(..drain);
        }

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

    pub fn observe_drop(&mut self, reason: TelemetryDropReason, count: u64) {
        self.increment(reason, count);
    }

    pub fn acknowledge_drop_counters(&mut self) {
        for counter in self.checkpoint.counters.values_mut() {
            counter.acknowledged_through = counter.observed;
        }
    }

    fn increment(&mut self, reason: TelemetryDropReason, count: u64) {
        let counter = self.checkpoint.counters.entry(reason).or_default();
        counter.observed = counter.observed.saturating_add(count);
    }
}

fn encoded_len(state: &AndroidStructuredLogSpillState) -> usize {
    serde_json::to_vec(state).map(|bytes| bytes.len()).unwrap_or(usize::MAX)
}

fn valid_state(state: &AndroidStructuredLogSpillState, enforce_record_cap: bool) -> bool {
    if state.version != 1
        || state.account_slot.is_empty()
        || state.account_slot.chars().count() > 128
        || state.checkpoint.counter_id.is_empty()
        || state.checkpoint.counter_id.chars().count() > 128
        || (enforce_record_cap
            && state.checkpoint.records.len() > ANDROID_STRUCTURED_LOG_SPILL_MAX_ENTRIES)
        || state.checkpoint.recovery_ids.len() > ANDROID_STRUCTURED_LOG_RECOVERY_ID_CAP
        || state.checkpoint.recovery_ids.iter().any(|value| {
            value.is_empty()
                || value.chars().count() > 128
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'-' | b'_' | b'.'))
        })
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
            Ok(state) if valid_state(&state, true) => state,
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
        if normalized.checkpoint.records.is_empty()
            && !normalized.has_pending_drops()
            && normalized.checkpoint.recovery_ids.is_empty() {
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
        if !valid_state(state, false) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid structured log spill state",
            ));
        }
        let normalized = state.clone().normalize(now_ms);
        if normalized.checkpoint.records.is_empty()
            && !normalized.has_pending_drops()
            && normalized.checkpoint.recovery_ids.is_empty() {
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

fn validate_recovery_id(recovery_id: &str) -> io::Result<()> {
    if recovery_id.is_empty()
        || recovery_id.chars().count() > 128
        || !recovery_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'-' | b'_' | b'.'))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "structured telemetry recovery id is invalid",
        ));
    }
    Ok(())
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


#[derive(Clone, Debug)]
pub struct AndroidStructuredLogSpool {
    spill: FileAndroidStructuredLogSpill,
    state: AndroidStructuredLogSpillState,
}

impl AndroidStructuredLogSpool {
    pub fn open(
        path: impl Into<PathBuf>,
        account_slot: &str,
        counter_id: &str,
        now_ms: u64,
    ) -> io::Result<Self> {
        let spill = FileAndroidStructuredLogSpill::new(path);
        let state = match spill.load(account_slot, now_ms)? {
            SpillLoad::Loaded(state) => state,
            SpillLoad::Empty
            | SpillLoad::AccountRotated
            | SpillLoad::InvalidDiscarded
            | SpillLoad::OversizeDiscarded => {
                AndroidStructuredLogSpillState::empty(account_slot, counter_id)
            }
        };
        Ok(Self { spill, state })
    }

    pub fn account_slot(&self) -> &str {
        &self.state.account_slot
    }

    pub fn rotate_account(
        &mut self,
        account_slot: &str,
        counter_id: &str,
    ) -> io::Result<bool> {
        if self.state.account_slot == account_slot {
            return Ok(false);
        }
        self.spill.clear()?;
        self.state = AndroidStructuredLogSpillState::empty(account_slot, counter_id);
        Ok(true)
    }

    pub fn record(
        &mut self,
        level: &str,
        message: &str,
        metadata: BTreeMap<String, String>,
        timestamp_ms: u64,
    ) -> io::Result<()> {
        if !matches!(level, "debug" | "info" | "warn" | "error")
            || !ALLOWED_EVENTS.contains(&message)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "structured telemetry record is outside the allowed schema",
            ));
        }
        self.state.checkpoint.records.push(StructuredLogRecord {
            level: level.into(),
            message: message.into(),
            metadata,
            timestamp: timestamp_ms,
        });
        self.state = self
            .spill
            .replace(&self.state, timestamp_ms)?
            .unwrap_or_else(|| {
                AndroidStructuredLogSpillState::empty(
                    self.state.account_slot.clone(),
                    self.state.checkpoint.counter_id.clone(),
                )
            });
        Ok(())
    }

    pub fn claim_recovery_id(
        &mut self,
        recovery_id: &str,
        timestamp_ms: u64,
    ) -> io::Result<bool> {
        validate_recovery_id(recovery_id)?;
        if self
            .state
            .checkpoint
            .recovery_ids
            .iter()
            .any(|value| value == recovery_id)
        {
            return Ok(false);
        }
        let mut candidate = self.state.clone();
        candidate.checkpoint.recovery_ids.push(recovery_id.to_string());
        if candidate.checkpoint.recovery_ids.len() > ANDROID_STRUCTURED_LOG_RECOVERY_ID_CAP {
            let drain =
                candidate.checkpoint.recovery_ids.len() - ANDROID_STRUCTURED_LOG_RECOVERY_ID_CAP;
            candidate.checkpoint.recovery_ids.drain(..drain);
        }
        self.state = self
            .spill
            .replace(&candidate, timestamp_ms)?
            .unwrap_or_else(|| {
                AndroidStructuredLogSpillState::empty(
                    candidate.account_slot.clone(),
                    candidate.checkpoint.counter_id.clone(),
                )
            });
        Ok(true)
    }

    pub fn record_once(
        &mut self,
        recovery_id: &str,
        level: &str,
        message: &str,
        metadata: BTreeMap<String, String>,
        timestamp_ms: u64,
    ) -> io::Result<bool> {
        validate_recovery_id(recovery_id)?;
        if self
            .state
            .checkpoint
            .recovery_ids
            .iter()
            .any(|value| value == recovery_id)
        {
            return Ok(false);
        }
        if !matches!(level, "debug" | "info" | "warn" | "error")
            || !ALLOWED_EVENTS.contains(&message)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "structured telemetry record is outside the allowed schema",
            ));
        }

        let mut candidate = self.state.clone();
        candidate.checkpoint.records.push(StructuredLogRecord {
            level: level.into(),
            message: message.into(),
            metadata,
            timestamp: timestamp_ms,
        });
        candidate
            .checkpoint
            .recovery_ids
            .push(recovery_id.to_string());
        if candidate.checkpoint.recovery_ids.len() > ANDROID_STRUCTURED_LOG_RECOVERY_ID_CAP {
            let drain =
                candidate.checkpoint.recovery_ids.len() - ANDROID_STRUCTURED_LOG_RECOVERY_ID_CAP;
            candidate.checkpoint.recovery_ids.drain(..drain);
        }
        self.state = self
            .spill
            .replace(&candidate, timestamp_ms)?
            .unwrap_or_else(|| {
                AndroidStructuredLogSpillState::empty(
                    candidate.account_slot.clone(),
                    candidate.checkpoint.counter_id.clone(),
                )
            });
        Ok(true)
    }

    pub fn batch(&self) -> Vec<StructuredLogRecord> {
        self.state
            .checkpoint
            .records
            .iter()
            .take(ANDROID_STRUCTURED_LOG_BATCH_SIZE)
            .cloned()
            .collect()
    }

    pub fn mark_ship_failed(&mut self, now_ms: u64) -> io::Result<()> {
        self.state.observe_drop(TelemetryDropReason::ShipFailed, 1);
        if let Some(state) = self.spill.replace(&self.state, now_ms)? {
            self.state = state;
        }
        Ok(())
    }

    pub fn settle_success(
        &mut self,
        processed: u64,
        backend_dropped: u64,
        now_ms: u64,
    ) -> io::Result<()> {
        let acknowledged = processed
            .saturating_add(backend_dropped)
            .min(self.state.checkpoint.records.len() as u64) as usize;
        self.state.checkpoint.records.drain(..acknowledged);
        if backend_dropped > 0 {
            self.state
                .observe_drop(TelemetryDropReason::BackendDropped, backend_dropped);
        }
        self.state.acknowledge_drop_counters();
        match self.spill.replace(&self.state, now_ms)? {
            Some(state) => self.state = state,
            None => {
                self.state = AndroidStructuredLogSpillState::empty(
                    self.state.account_slot.clone(),
                    self.state.checkpoint.counter_id.clone(),
                );
            }
        }
        Ok(())
    }

    pub fn snapshot(&self) -> AndroidStructuredLogSpillState {
        self.state.clone()
    }
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
    fn recovery_id_is_durable_across_shipping_and_prevents_duplicate_replay() {
        let root = root("recovery-id");
        let path = root.join("spill.json");
        let mut spool =
            AndroidStructuredLogSpool::open(&path, "account-a", "counter-a", 1_000).unwrap();
        let metadata = BTreeMap::from([("kind".into(), "unclean_exit".into())]);
        assert!(spool
            .record_once(
                "unclean:100:150",
                "warn",
                "sand.desktop.unclean_exit",
                metadata.clone(),
                1_001,
            )
            .unwrap());
        assert!(!spool
            .record_once(
                "unclean:100:150",
                "warn",
                "sand.desktop.unclean_exit",
                metadata.clone(),
                1_002,
            )
            .unwrap());
        assert_eq!(spool.batch().len(), 1);

        spool.settle_success(1, 0, 1_003).unwrap();
        assert!(spool.batch().is_empty());
        drop(spool);

        let mut reopened =
            AndroidStructuredLogSpool::open(&path, "account-a", "counter-a", 1_004).unwrap();
        assert!(!reopened
            .record_once(
                "unclean:100:150",
                "warn",
                "sand.desktop.unclean_exit",
                metadata,
                1_005,
            )
            .unwrap());
        assert!(reopened.batch().is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn spill_accepts_desktop_turn_telemetry_contracts() {
        let root = root("turn-telemetry");
        let path = root.join("spill.json");
        let mut spool =
            AndroidStructuredLogSpool::open(&path, "account-a", "counter-a", 1_000).unwrap();
        for (index, event) in [
            "sand.turn.interrupt",
            "sand.turn.await",
            "sand.turn.retry",
            "sand.turn.closing_send_nudge",
            "sand.user_message.received",
            "sand.computer_use.usage",
            "sand.ttft",
            "sand.turn.usage",
        ]
        .into_iter()
        .enumerate()
        {
            spool
                .record(
                    "info",
                    event,
                    BTreeMap::from([("conversation_id".into(), "agent-a".into())]),
                    1_001 + index as u64,
                )
                .unwrap();
        }
        assert_eq!(spool.batch().len(), 8);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn spill_accepts_agent_error_event() {
        let root = root("agent-error");
        let path = root.join("spill.json");
        let mut spool = AndroidStructuredLogSpool::open(&path, "account-a", "counter-a", 1_000)
            .unwrap();
        spool
            .record(
                "error",
                "sand.agent.error",
                BTreeMap::from([
                    ("source".into(), "turn".into()),
                    ("conversation_id".into(), "agent-1".into()),
                    ("error_code".into(), "SAND-E0001".into()),
                ]),
                1_001,
            )
            .unwrap();
        let snapshot = spool.snapshot();
        assert_eq!(snapshot.checkpoint.records.len(), 1);
        assert_eq!(snapshot.checkpoint.records[0].message, "sand.agent.error");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn spill_accepts_restored_automation_and_box_telemetry_contracts() {
        let root = root("restored-telemetry");
        let path = root.join("spill.json");
        let mut spool =
            AndroidStructuredLogSpool::open(&path, "account-a", "counter-a", 1_000).unwrap();
        for (index, event) in [
            "sand.automation.lifecycle",
            "sand.box.log_ship",
            "sand.box.recreate_decided",
        ]
        .into_iter()
        .enumerate()
        {
            spool
                .record(
                    "info",
                    event,
                    BTreeMap::from([("kind".into(), "bounded".into())]),
                    1_001 + index as u64,
                )
                .unwrap();
        }
        assert_eq!(
            spool
                .batch()
                .iter()
                .map(|record| record.message.as_str())
                .collect::<Vec<_>>(),
            vec![
                "sand.automation.lifecycle",
                "sand.box.log_ship",
                "sand.box.recreate_decided",
            ]
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn spill_accepts_invariant_and_local_exec_telemetry_contracts() {
        let root = root("host-local-exec-telemetry");
        let path = root.join("spill.json");
        let mut spool =
            AndroidStructuredLogSpool::open(&path, "account-a", "counter-a", 1_000).unwrap();
        for (index, event) in [
            "sand.host.invariant_violation",
            "sand.local_exec.refused",
            "sand.local_exec.exec_failed",
            "sand.local_exec.provider",
        ]
        .into_iter()
        .enumerate()
        {
            spool
                .record(
                    if event == "sand.host.invariant_violation" { "error" } else { "warn" },
                    event,
                    BTreeMap::from([("kind".into(), "bounded".into())]),
                    1_001 + index as u64,
                )
                .unwrap();
        }
        assert_eq!(spool.batch().len(), 4);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn spill_accepts_canonical_turn_queue_contracts() {
        let root = root("turn-queue");
        let path = root.join("spill.json");
        let mut spool =
            AndroidStructuredLogSpool::open(&path, "account-a", "counter-a", 1_000).unwrap();
        for event in ["sand.queue.accepted", "sand.queue.dequeued", "sand.queue.watchdog"] {
            spool
                .record(
                    "info",
                    event,
                    BTreeMap::from([("conversation_id".into(), "agent-a".into())]),
                    1_001,
                )
                .unwrap();
        }
        assert_eq!(
            spool.batch().iter().map(|record| record.message.as_str()).collect::<Vec<_>>(),
            vec!["sand.queue.accepted", "sand.queue.dequeued", "sand.queue.watchdog"],
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn spill_accepts_pending_wake_queue_contract() {
        let root = root("pending-wake");
        let path = root.join("spill.json");
        let mut spool =
            AndroidStructuredLogSpool::open(&path, "account-a", "counter-a", 1_000).unwrap();
        spool
            .record(
                "warn",
                "sand.pending_wake",
                BTreeMap::from([
                    ("conversation_id".into(), "agent-a".into()),
                    ("outcome".into(), "pruned".into()),
                    ("reason".into(), "stale".into()),
                ]),
                1_001,
            )
            .unwrap();
        assert_eq!(spool.batch()[0].message, "sand.pending_wake");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn spill_accepts_memory_and_session_diagnostic_contracts() {
        let root = root("memory-session-telemetry");
        let path = root.join("spill.json");
        let mut spool =
            AndroidStructuredLogSpool::open(&path, "account-a", "counter-a", 1_000).unwrap();
        for (index, event) in [
            "sand.memory.synthesis",
            "sand.session.store_db",
            "sand.session.maintenance",
            "sand.session.materialize",
            "sand.session.summary_build",
        ]
        .into_iter()
        .enumerate()
        {
            spool
                .record(
                    "info",
                    event,
                    BTreeMap::from([("kind".into(), "bounded".into())]),
                    1_001 + index as u64,
                )
                .unwrap();
        }
        assert_eq!(spool.batch().len(), 5);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn spill_accepts_account_signin_and_tracks_rotation_drop() {
        let mut state = AndroidStructuredLogSpillState::empty("account-a", "counter-a");
        state.checkpoint.records.push(record(1_000, "sand.desktop.signin"));
        state.observe_drop(TelemetryDropReason::AccountRotated, 1);
        let normalized = state.normalize(1_001);
        assert_eq!(normalized.checkpoint.records.len(), 1);
        assert_eq!(
            normalized.checkpoint.counters[&TelemetryDropReason::AccountRotated].observed,
            1
        );
        assert!(normalized.has_pending_drops());
        let mut acknowledged = normalized;
        acknowledged.acknowledge_drop_counters();
        assert!(!acknowledged.has_pending_drops());
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
    fn replay_age_matches_desktop_structured_log_contract() {
        assert_eq!(ANDROID_STRUCTURED_LOG_SPILL_MAX_AGE_MS, 17 * 60 * 60 * 1_000);
        assert_eq!(
            ANDROID_STRUCTURED_LOG_FUTURE_TIMESTAMP_MAX_SKEW_MS,
            2 * 60 * 60 * 1_000
        );
    }

    #[test]
    fn spill_rejects_far_future_records_under_desktop_skew_contract() {
        let now = 24 * 60 * 60 * 1_000;
        let mut state = AndroidStructuredLogSpillState::empty("account-a", "counter-a");
        state.checkpoint.records.push(record(
            now + ANDROID_STRUCTURED_LOG_FUTURE_TIMESTAMP_MAX_SKEW_MS + 1,
            "sand.send",
        ));
        let normalized = state.normalize(now);
        assert!(normalized.checkpoint.records.is_empty());
        assert_eq!(
            normalized.checkpoint.counters[&TelemetryDropReason::ReplayExpired].observed,
            1
        );
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
    fn spool_accepts_canonical_agent_error_and_open_events() {
        let root = root("agent-events");
        let path = root.join("spill.json");
        let mut spool =
            AndroidStructuredLogSpool::open(&path, "account-a", "counter-a", 1_000).unwrap();
        for (index, event) in [
            "sand.agent.error",
            "sand.agent.error.detail",
            "sand.agent.open",
        ]
        .into_iter()
        .enumerate()
        {
            spool
                .record(
                    if event.contains("error") { "error" } else { "info" },
                    event,
                    BTreeMap::from([("conversation_id".into(), "conversation-a".into())]),
                    1_001 + index as u64,
                )
                .unwrap();
        }
        let reopened =
            AndroidStructuredLogSpool::open(&path, "account-a", "ignored", 2_000).unwrap();
        assert_eq!(reopened.snapshot().checkpoint.records.len(), 3);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn spool_rotates_account_and_reopens_durable_record() {
        let root = root("spool");
        let path = root.join("spill.json");
        let mut spool =
            AndroidStructuredLogSpool::open(&path, "account-a", "counter-a", 1_000).unwrap();
        spool
            .record(
                "error",
                "sand.desktop.edge_failed",
                BTreeMap::from([("error_class".into(), "HostError".into())]),
                1_001,
            )
            .unwrap();
        let reopened =
            AndroidStructuredLogSpool::open(&path, "account-a", "ignored", 1_002).unwrap();
        assert_eq!(reopened.snapshot().checkpoint.records.len(), 1);

        let mut reopened = reopened;
        assert!(reopened.rotate_account("account-b", "counter-b").unwrap());
        assert!(reopened.snapshot().checkpoint.records.is_empty());
        assert_eq!(reopened.account_slot(), "account-b");
        assert_eq!(
            FileAndroidStructuredLogSpill::new(&path)
                .load("account-b", 1_003)
                .unwrap(),
            SpillLoad::Empty
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn spool_batches_and_settles_backend_receipts_durably() {
        let root = root("ship");
        let path = root.join("spill.json");
        let mut spool =
            AndroidStructuredLogSpool::open(&path, "account-a", "counter-a", 1_000).unwrap();
        for index in 0..(ANDROID_STRUCTURED_LOG_BATCH_SIZE + 3) {
            spool
                .record(
                    "info",
                    "sand.send",
                    BTreeMap::from([("index".into(), index.to_string())]),
                    1_000 + index as u64,
                )
                .unwrap();
        }
        assert_eq!(spool.batch().len(), ANDROID_STRUCTURED_LOG_BATCH_SIZE);
        spool
            .settle_success(
                (ANDROID_STRUCTURED_LOG_BATCH_SIZE - 1) as u64,
                1,
                2_000,
            )
            .unwrap();
        assert_eq!(spool.batch().len(), 3);
        assert_eq!(
            spool.snapshot().checkpoint.counters[&TelemetryDropReason::BackendDropped].observed,
            1
        );

        let reopened =
            AndroidStructuredLogSpool::open(&path, "account-a", "ignored", 2_001).unwrap();
        assert_eq!(reopened.batch().len(), 3);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn failed_ship_retains_batch_and_persists_drop_counter() {
        let root = root("ship-failed");
        let path = root.join("spill.json");
        let mut spool =
            AndroidStructuredLogSpool::open(&path, "account-a", "counter-a", 1_000).unwrap();
        spool
            .record(
                "warn",
                "sand.send_ack",
                BTreeMap::from([("outcome".into(), "timeout".into())]),
                1_001,
            )
            .unwrap();
        let before = spool.batch();
        spool.mark_ship_failed(1_002).unwrap();
        assert_eq!(spool.batch(), before);

        let reopened =
            AndroidStructuredLogSpool::open(&path, "account-a", "ignored", 1_003).unwrap();
        assert_eq!(reopened.batch(), before);
        assert_eq!(
            reopened.snapshot().checkpoint.counters[&TelemetryDropReason::ShipFailed].observed,
            1
        );
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
