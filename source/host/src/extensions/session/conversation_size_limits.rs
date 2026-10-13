use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::agent_isolation::{
    AgentWorkerPool, ConversationGarbageCollectionOutcome, ProductionAgentStoreWorkerBackend,
};

use super::session_diagnostics::{SessionDiagnostic, report_session_diagnostic};

pub const SOFT_LIMIT_DEFAULT_BYTES: u64 = 256 * 1024 * 1024;
pub const HARD_LIMIT_DEFAULT_BYTES: u64 = 1024 * 1024 * 1024;
pub const GC_PENDING_WRITE_RETENTION_MS: u64 = 60_000;
pub const SOFT_GC_MIN_INTERVAL_MS: u64 = 30 * 60_000;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ConversationSizeLimits {
    pub soft_limit_mb: Option<f64>,
    pub hard_limit_mb: Option<f64>,
}

pub type ConversationSizeLimitsReader =
    Arc<dyn Fn() -> ConversationSizeLimits + Send + Sync + 'static>;
pub type ConversationGcReporter =
    Arc<dyn Fn(&ConversationGcReport) + Send + Sync + 'static>;

fn limits_reader_slot() -> &'static RwLock<Option<ConversationSizeLimitsReader>> {
    static SLOT: OnceLock<RwLock<Option<ConversationSizeLimitsReader>>> = OnceLock::new();
    SLOT.get_or_init(|| RwLock::new(None))
}

fn gc_reporter_slot() -> &'static RwLock<Option<ConversationGcReporter>> {
    static SLOT: OnceLock<RwLock<Option<ConversationGcReporter>>> = OnceLock::new();
    SLOT.get_or_init(|| RwLock::new(None))
}

static PINNED_CONVERSATION_GC_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn pin_conversation_size_limits_reader(reader: Option<ConversationSizeLimitsReader>) {
    if let Ok(mut slot) = limits_reader_slot().write() {
        *slot = reader;
    }
}

pub fn pin_conversation_gc(enabled: bool) {
    PINNED_CONVERSATION_GC_ENABLED.store(enabled, Ordering::Release);
}

pub fn pin_conversation_gc_reporter(reporter: Option<ConversationGcReporter>) {
    if let Ok(mut slot) = gc_reporter_slot().write() {
        *slot = reporter;
    }
}

pub fn current_conversation_size_limits() -> ConversationSizeLimits {
    let reader = limits_reader_slot()
        .read()
        .ok()
        .and_then(|slot| slot.as_ref().map(Arc::clone));
    reader.map(|reader| reader()).unwrap_or_default()
}

fn positive_floor(raw: Option<String>) -> Option<u64> {
    raw.and_then(|raw| raw.trim().parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
        .map(|value| value.floor() as u64)
        .filter(|value| *value > 0)
}

fn configured_limit_bytes(value_mb: Option<f64>) -> Option<u64> {
    value_mb
        .filter(|value| value.is_finite() && *value > 0.0)
        .map(|value| (value * 1024.0 * 1024.0).floor() as u64)
        .filter(|value| *value > 0)
}

fn parse_enabled(raw: Option<String>, pinned: bool) -> bool {
    match raw.map(|value| value.trim().to_ascii_lowercase()).as_deref() {
        Some("1" | "true" | "on") => true,
        Some("0" | "false" | "off") => false,
        _ => pinned,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConversationSizePolicy {
    pub enabled: bool,
    pub soft_limit_bytes: u64,
    pub hard_limit_bytes: u64,
}

impl ConversationSizePolicy {
    pub fn from_environment() -> Self {
        let configured = current_conversation_size_limits();
        let pinned = PINNED_CONVERSATION_GC_ENABLED.load(Ordering::Acquire);
        Self::from_lookup(|name| std::env::var(name).ok(), configured, pinned)
    }

    pub fn from_lookup(
        mut lookup: impl FnMut(&str) -> Option<String>,
        configured: ConversationSizeLimits,
        pinned_gc_enabled: bool,
    ) -> Self {
        let soft_limit_bytes = positive_floor(lookup("SAND_CONVERSATION_SOFT_LIMIT_BYTES"))
            .or_else(|| configured_limit_bytes(configured.soft_limit_mb))
            .unwrap_or(SOFT_LIMIT_DEFAULT_BYTES);
        let hard_limit_bytes = positive_floor(lookup("SAND_CONVERSATION_HARD_LIMIT_BYTES"))
            .or_else(|| configured_limit_bytes(configured.hard_limit_mb))
            .unwrap_or(HARD_LIMIT_DEFAULT_BYTES);
        Self {
            enabled: parse_enabled(lookup("SAND_CONVERSATION_GC"), pinned_gc_enabled),
            soft_limit_bytes,
            hard_limit_bytes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationGcTarget {
    pub agent_id: String,
    pub blob_db_path: PathBuf,
    pub legacy_blob_db_path: PathBuf,
    pub retained_root_id_hex: String,
}

impl ConversationGcTarget {
    pub fn from_root(
        agent_id: impl Into<String>,
        blob_db_path: impl Into<PathBuf>,
        legacy_blob_db_path: impl Into<PathBuf>,
        retained_root_id: &[u8],
    ) -> Self {
        Self {
            agent_id: agent_id.into(),
            blob_db_path: blob_db_path.into(),
            legacy_blob_db_path: legacy_blob_db_path.into(),
            retained_root_id_hex: to_hex(retained_root_id),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationGcReport {
    pub trigger: String,
    pub agent_id: String,
    pub outcome: String,
    pub skip_reason: Option<String>,
    pub unresolved_proto_refs: Option<usize>,
    pub deleted_rows: Option<usize>,
    pub deleted_bytes: Option<u64>,
    pub live_rows: Option<usize>,
    pub live_bytes: Option<u64>,
    pub vacuumed: Option<bool>,
    pub still_over_cap: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandConversationTooLargeError {
    pub size_bytes: u64,
    pub limit_bytes: u64,
}

impl fmt::Display for SandConversationTooLargeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mib = 1024 * 1024;
        let size_mb = self.size_bytes.saturating_add(mib / 2) / mib;
        let limit_mb = self.limit_bytes.saturating_add(mib / 2) / mib;
        write!(
            formatter,
            "This conversation's stored state is {size_mb} MB, over the {limit_mb} MB limit, and compaction could not shrink it. Start a new conversation with this agent to continue."
        )
    }
}

impl Error for SandConversationTooLargeError {}

#[derive(Debug, Clone, Copy, Default)]
struct SoftGcState {
    in_flight: bool,
    last_run_ms: u64,
}

#[derive(Clone, Default)]
pub struct ConversationSizeMaintenance {
    soft_gc_state_by_blob_db_path: Arc<Mutex<HashMap<PathBuf, SoftGcState>>>,
}

impl ConversationSizeMaintenance {
    pub fn schedule_conversation_size_maintenance(
        &self,
        pool: Arc<AgentWorkerPool<ProductionAgentStoreWorkerBackend>>,
        target: ConversationGcTarget,
        policy: ConversationSizePolicy,
    ) -> bool {
        if !policy.enabled
            || measure_conversation_blob_bytes(&target.blob_db_path) < policy.soft_limit_bytes
        {
            return false;
        }
        let observed_now_ms = now_ms();
        if !self.reserve_soft_gc(&target.blob_db_path, observed_now_ms) {
            return false;
        }

        let maintenance = self.clone();
        let state_key = target.blob_db_path.clone();
        let state_key_for_thread = state_key.clone();
        let agent_id = target.agent_id.clone();
        let spawn = thread::Builder::new()
            .name(format!("mahayana-session-gc-{agent_id}"))
            .spawn(move || {
                match run_conversation_gc(pool.as_ref(), &target) {
                    Ok(verdict) => report_conversation_gc_verdict(
                        "soft_schedule",
                        &target,
                        verdict.as_ref(),
                        false,
                    ),
                    Err(error) => {
                        report_conversation_gc_failure("soft_schedule", &target);
                        report_gc_error(&target.agent_id, &error);
                    }
                }
                maintenance.finish_soft_gc(&state_key_for_thread);
            });

        if let Err(error) = spawn {
            self.finish_soft_gc(&state_key);
            report_gc_error(&agent_id, &error.to_string());
            return false;
        }
        true
    }

    pub fn ensure_conversation_capacity_for_turn(
        &self,
        pool: Arc<AgentWorkerPool<ProductionAgentStoreWorkerBackend>>,
        target: ConversationGcTarget,
        policy: ConversationSizePolicy,
    ) -> Result<(), SandConversationTooLargeError> {
        if !policy.enabled {
            return Ok(());
        }

        let size = measure_conversation_blob_bytes(&target.blob_db_path);
        if size < policy.hard_limit_bytes {
            if size >= policy.soft_limit_bytes {
                let _ = self.schedule_conversation_size_maintenance(pool, target, policy);
            }
            return Ok(());
        }

        let verdict = match run_conversation_gc(pool.as_ref(), &target) {
            Ok(verdict) => verdict,
            Err(error) => {
                report_conversation_gc_failure("turn_gate", &target);
                report_gc_error(&target.agent_id, &error);
                return Ok(());
            }
        };

        match &verdict {
            Some(ConversationGarbageCollectionOutcome::Collected { .. }) => {
                let after = measure_conversation_blob_bytes(&target.blob_db_path);
                let still_over_cap = after >= policy.hard_limit_bytes;
                report_conversation_gc_verdict(
                    "turn_gate",
                    &target,
                    verdict.as_ref(),
                    still_over_cap,
                );
                if still_over_cap {
                    return Err(SandConversationTooLargeError {
                        size_bytes: after,
                        limit_bytes: policy.hard_limit_bytes,
                    });
                }
            }
            Some(ConversationGarbageCollectionOutcome::Skipped { .. }) | None => {
                report_conversation_gc_verdict(
                    "turn_gate",
                    &target,
                    verdict.as_ref(),
                    false,
                );
            }
        }
        Ok(())
    }

    fn reserve_soft_gc(&self, blob_db_path: &Path, observed_now_ms: u64) -> bool {
        let mut states = self
            .soft_gc_state_by_blob_db_path
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let state = states.entry(blob_db_path.to_path_buf()).or_default();
        if state.in_flight
            || observed_now_ms.saturating_sub(state.last_run_ms) < SOFT_GC_MIN_INTERVAL_MS
        {
            return false;
        }
        state.in_flight = true;
        state.last_run_ms = observed_now_ms;
        true
    }

    fn finish_soft_gc(&self, blob_db_path: &Path) {
        let mut states = self
            .soft_gc_state_by_blob_db_path
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(state) = states.get_mut(blob_db_path) {
            state.in_flight = false;
        }
    }
}

pub fn measure_conversation_blob_bytes(blob_db_path: &Path) -> u64 {
    let wal_path = PathBuf::from(format!("{}-wal", blob_db_path.display()));
    [blob_db_path.to_path_buf(), wal_path]
        .into_iter()
        .filter_map(|path| fs::metadata(path).ok())
        .fold(0u64, |total, metadata| total.saturating_add(metadata.len()))
}

pub fn run_conversation_gc(
    pool: &AgentWorkerPool<ProductionAgentStoreWorkerBackend>,
    target: &ConversationGcTarget,
) -> Result<Option<ConversationGarbageCollectionOutcome>, String> {
    if target.retained_root_id_hex.is_empty() {
        return Ok(None);
    }
    futures::executor::block_on(pool.collect_conversation_garbage(
        &target.agent_id,
        &target.blob_db_path,
        &target.retained_root_id_hex,
        GC_PENDING_WRITE_RETENTION_MS,
        Some(&target.legacy_blob_db_path),
    ))
    .map(Some)
    .map_err(|error| error.to_string())
}

pub fn report_conversation_gc_verdict(
    trigger: &str,
    target: &ConversationGcTarget,
    verdict: Option<&ConversationGarbageCollectionOutcome>,
    still_over_cap: bool,
) {
    let report = match verdict {
        None => ConversationGcReport {
            trigger: trigger.to_string(),
            agent_id: target.agent_id.clone(),
            outcome: "skipped".into(),
            skip_reason: Some("no-root".into()),
            unresolved_proto_refs: None,
            deleted_rows: None,
            deleted_bytes: None,
            live_rows: None,
            live_bytes: None,
            vacuumed: None,
            still_over_cap,
        },
        Some(ConversationGarbageCollectionOutcome::Skipped {
            reason,
            unresolved_proto_refs,
        }) => ConversationGcReport {
            trigger: trigger.to_string(),
            agent_id: target.agent_id.clone(),
            outcome: "skipped".into(),
            skip_reason: Some(reason.clone()),
            unresolved_proto_refs: Some(*unresolved_proto_refs),
            deleted_rows: None,
            deleted_bytes: None,
            live_rows: None,
            live_bytes: None,
            vacuumed: None,
            still_over_cap,
        },
        Some(ConversationGarbageCollectionOutcome::Collected {
            deleted_rows,
            deleted_bytes,
            live_rows,
            live_bytes,
            retained_pending_rows: _,
            vacuumed,
        }) => ConversationGcReport {
            trigger: trigger.to_string(),
            agent_id: target.agent_id.clone(),
            outcome: "collected".into(),
            skip_reason: None,
            unresolved_proto_refs: None,
            deleted_rows: Some(*deleted_rows),
            deleted_bytes: Some(*deleted_bytes),
            live_rows: Some(*live_rows),
            live_bytes: Some(*live_bytes),
            vacuumed: Some(*vacuumed),
            still_over_cap,
        },
    };
    let reporter = gc_reporter_slot()
        .read()
        .ok()
        .and_then(|slot| slot.as_ref().map(Arc::clone));
    if let Some(reporter) = reporter {
        reporter(&report);
    }
}

fn report_conversation_gc_failure(trigger: &str, target: &ConversationGcTarget) {
    let report = ConversationGcReport {
        trigger: trigger.to_string(),
        agent_id: target.agent_id.clone(),
        outcome: "failed".into(),
        skip_reason: None,
        unresolved_proto_refs: None,
        deleted_rows: None,
        deleted_bytes: None,
        live_rows: None,
        live_bytes: None,
        vacuumed: None,
        still_over_cap: false,
    };
    let reporter = gc_reporter_slot()
        .read()
        .ok()
        .and_then(|slot| slot.as_ref().map(Arc::clone));
    if let Some(reporter) = reporter {
        reporter(&report);
    }
}

fn report_gc_error(agent_id: &str, error: &str) {
    report_session_diagnostic(&SessionDiagnostic {
        family: "maintenance".into(),
        kind: "conversation_gc_failed".into(),
        metadata: BTreeMap::from([
            ("agentId".into(), Value::String(agent_id.to_string())),
            ("errorClass".into(), Value::String(error.to_string())),
        ]),
    });
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

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or_default()
}
