#[cfg(feature = "axum-server")]
use axum::http::HeaderMap;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// Well-known session affinity headers recognized across various agent harness clients.
pub const SESSION_HEADERS: &[&str] = &[
    "x-magpie-session",
    "x-conversation-id",
    "x-session-affinity",
    "session_id",
    "session-id",
    "x-codex-window-id",
    "x-deepseek-harness-session-id",
    "x-session-id",
];

/// Extract and normalize session identifier from HTTP request headers.
#[cfg(feature = "axum-server")]
pub fn normalize_session_header(headers: &HeaderMap) -> Option<String> {
    for &hdr_name in SESSION_HEADERS {
        if let Some(val) = headers.get(hdr_name) {
            if let Ok(s) = std::str::from_utf8(val.as_bytes()) {
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    None
}

/// Extract and normalize session identifier from an arbitrary string key-value map.
pub fn normalize_session_from_map(map: &HashMap<String, String>) -> Option<String> {
    for &hdr_name in SESSION_HEADERS {
        if let Some((_, v)) = map.iter().find(|(k, _)| k.eq_ignore_ascii_case(hdr_name)) {
            let trimmed = v.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// Token usage and latency telemetry record for an individual LLM turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageRecord {
    pub id: String,
    pub session_id: String,
    pub timestamp_ms: i64,
    pub provider_id: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub reasoning_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttft_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tps: Option<f64>,
    pub cost_usd: f64,
}

impl UsageRecord {
    pub fn new(
        id: impl Into<String>,
        session_id: impl Into<String>,
        provider_id: impl Into<String>,
        model: impl Into<String>,
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: u64,
        reasoning_tokens: u64,
        ttft_ms: Option<u64>,
        duration_ms: Option<u64>,
        timestamp_ms: i64,
    ) -> Self {
        let model_str = model.into();
        let cost_usd = estimate_cost_usd(
            &model_str,
            input_tokens,
            output_tokens,
            cache_read_tokens,
            reasoning_tokens,
        );
        let tps = duration_ms.and_then(|dur| {
            if dur > 0 && output_tokens > 0 {
                Some((output_tokens as f64) / ((dur as f64) / 1000.0))
            } else {
                None
            }
        });

        Self {
            id: id.into(),
            session_id: session_id.into(),
            timestamp_ms,
            provider_id: provider_id.into(),
            model: model_str,
            input_tokens,
            output_tokens,
            cache_read_tokens,
            reasoning_tokens,
            ttft_ms,
            duration_ms,
            tps,
            cost_usd,
        }
    }

    pub fn total_tokens(&self) -> u64 {
        self.input_tokens.saturating_add(self.output_tokens)
    }
}

/// Baseline USD pricing estimation per million tokens.
pub fn estimate_cost_usd(
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    reasoning_tokens: u64,
) -> f64 {
    let m = model.to_ascii_lowercase();

    let (input_per_1m, output_per_1m) = if m.contains("opus") {
        (15.0, 75.0)
    } else if m.contains("sonnet") || m.contains("gpt-4o") || m.contains("gemini-2.5-pro") {
        (2.50, 10.0)
    } else if m.contains("flash") || m.contains("mini") || m.contains("gpt-4o-mini") {
        (0.15, 0.60)
    } else if m.contains("deepseek") {
        (0.27, 1.10)
    } else if m.contains("qwen") || m.contains("glm") || m.contains("kimi") {
        (0.30, 1.20)
    } else {
        (1.00, 3.00)
    };

    let cache_discount_factor = 0.5; // Cache reads billed at 50% discount
    let effective_input = (input_tokens.saturating_sub(cache_read_tokens) as f64)
        + (cache_read_tokens as f64) * cache_discount_factor;
    let effective_output = (output_tokens.saturating_add(reasoning_tokens)) as f64;

    let cost_input = (effective_input / 1_000_000.0) * input_per_1m;
    let cost_output = (effective_output / 1_000_000.0) * output_per_1m;

    ((cost_input + cost_output) * 10_000.0).round() / 10_000.0
}

/// Aggregated usage metrics rollup.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UsageSummary {
    pub total_records: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub total_reasoning_tokens: u64,
    pub total_tokens: u64,
    pub total_cost_usd: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avg_ttft_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avg_duration_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avg_tps: Option<f64>,
    pub by_session: HashMap<String, u64>,
    pub by_provider: HashMap<String, u64>,
    pub by_model: HashMap<String, u64>,
}

const MAX_RING_BUFFER: usize = 10_000;

/// In-memory ring buffer with persistent JSONL append ledger.
pub struct UsageLedger {
    records: RwLock<VecDeque<UsageRecord>>,
    persistent_path: Option<PathBuf>,
    file_lock: Mutex<Option<std::fs::File>>,
}

impl Default for UsageLedger {
    fn default() -> Self {
        Self::new(None)
    }
}

impl UsageLedger {
    pub fn new(persistent_path: Option<PathBuf>) -> Self {
        let ledger = Self {
            records: RwLock::new(VecDeque::with_capacity(256)),
            persistent_path,
            file_lock: Mutex::new(None),
        };
        if let Some(ref path) = ledger.persistent_path {
            if path.exists() {
                let _ = ledger.load_from_path(path);
            }
        }
        ledger
    }

    fn load_from_path(&self, path: &Path) -> Result<usize, std::io::Error> {
        let file = std::fs::File::open(path)?;
        let reader = BufReader::new(file);
        let mut count = 0;
        let mut loaded = Vec::new();

        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(rec) = serde_json::from_str::<UsageRecord>(trimmed) {
                loaded.push(rec);
                count += 1;
            }
        }

        let mut lock = self.records.write();
        for rec in loaded {
            if lock.len() >= MAX_RING_BUFFER {
                lock.pop_front();
            }
            lock.push_back(rec);
        }

        Ok(count)
    }

    /// Record a completed turn telemetry entry.
    pub fn record(&self, record: UsageRecord) -> Result<(), std::io::Error> {
        if let Some(ref path) = self.persistent_path {
            let mut line = serde_json::to_string(&record)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            line.push('\n');
            let mut file_guard = self.file_lock.lock();
            if file_guard.is_none() {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let f = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)?;
                *file_guard = Some(f);
            }
            if let Some(ref mut file) = *file_guard {
                file.write_all(line.as_bytes())?;
                file.flush()?;
            }
        }

        let mut lock = self.records.write();
        if lock.len() >= MAX_RING_BUFFER {
            lock.pop_front();
        }
        lock.push_back(record);
        Ok(())
    }

    /// Query recent records matching optional session filter.
    pub fn query_records(&self, limit: usize, session_filter: Option<&str>) -> Vec<UsageRecord> {
        let lock = self.records.read();
        let iter = lock.iter().rev();
        if let Some(sess) = session_filter {
            iter.filter(|r| r.session_id == sess)
                .take(limit)
                .cloned()
                .collect()
        } else {
            iter.take(limit).cloned().collect()
        }
    }

    /// Compute usage summary since optional timestamp cutoff.
    pub fn summary(&self, since_ms: Option<i64>) -> UsageSummary {
        let lock = self.records.read();
        let mut sum = UsageSummary::default();
        let mut ttft_total = 0u64;
        let mut ttft_count = 0u64;
        let mut dur_total = 0u64;
        let mut dur_count = 0u64;
        let mut tps_total = 0.0f64;
        let mut tps_count = 0u64;

        for r in lock.iter() {
            if let Some(cutoff) = since_ms {
                if r.timestamp_ms < cutoff {
                    continue;
                }
            }
            sum.total_records += 1;
            sum.total_input_tokens += r.input_tokens;
            sum.total_output_tokens += r.output_tokens;
            sum.total_cache_read_tokens += r.cache_read_tokens;
            sum.total_reasoning_tokens += r.reasoning_tokens;
            sum.total_cost_usd += r.cost_usd;

            *sum.by_session.entry(r.session_id.clone()).or_default() += r.total_tokens();
            *sum.by_provider.entry(r.provider_id.clone()).or_default() += r.total_tokens();
            *sum.by_model.entry(r.model.clone()).or_default() += r.total_tokens();

            if let Some(ms) = r.ttft_ms {
                ttft_total += ms;
                ttft_count += 1;
            }
            if let Some(ms) = r.duration_ms {
                dur_total += ms;
                dur_count += 1;
            }
            if let Some(rate) = r.tps {
                tps_total += rate;
                tps_count += 1;
            }
        }

        sum.total_tokens = sum.total_input_tokens + sum.total_output_tokens;
        if ttft_count > 0 {
            sum.avg_ttft_ms = Some((ttft_total as f64) / (ttft_count as f64));
        }
        if dur_count > 0 {
            sum.avg_duration_ms = Some((dur_total as f64) / (dur_count as f64));
        }
        if tps_count > 0 {
            sum.avg_tps = Some(tps_total / (tps_count as f64));
        }

        sum.total_cost_usd = (sum.total_cost_usd * 10_000.0).round() / 10_000.0;
        sum
    }

    /// Compute usage summary filtered to a specific session ID.
    pub fn summary_for_session(&self, session_id: &str) -> UsageSummary {
        let lock = self.records.read();
        let mut sum = UsageSummary::default();
        let mut ttft_total = 0u64;
        let mut ttft_count = 0u64;
        let mut dur_total = 0u64;
        let mut dur_count = 0u64;
        let mut tps_total = 0.0f64;
        let mut tps_count = 0u64;

        for r in lock.iter().filter(|r| r.session_id == session_id) {
            sum.total_records += 1;
            sum.total_input_tokens += r.input_tokens;
            sum.total_output_tokens += r.output_tokens;
            sum.total_cache_read_tokens += r.cache_read_tokens;
            sum.total_reasoning_tokens += r.reasoning_tokens;
            sum.total_cost_usd += r.cost_usd;

            *sum.by_session.entry(r.session_id.clone()).or_default() += r.total_tokens();
            *sum.by_provider.entry(r.provider_id.clone()).or_default() += r.total_tokens();
            *sum.by_model.entry(r.model.clone()).or_default() += r.total_tokens();

            if let Some(ms) = r.ttft_ms {
                ttft_total += ms;
                ttft_count += 1;
            }
            if let Some(ms) = r.duration_ms {
                dur_total += ms;
                dur_count += 1;
            }
            if let Some(rate) = r.tps {
                tps_total += rate;
                tps_count += 1;
            }
        }

        sum.total_tokens = sum.total_input_tokens + sum.total_output_tokens;
        if ttft_count > 0 {
            sum.avg_ttft_ms = Some((ttft_total as f64) / (ttft_count as f64));
        }
        if dur_count > 0 {
            sum.avg_duration_ms = Some((dur_total as f64) / (dur_count as f64));
        }
        if tps_count > 0 {
            sum.avg_tps = Some(tps_total / (tps_count as f64));
        }

        sum.total_cost_usd = (sum.total_cost_usd * 10_000.0).round() / 10_000.0;
        sum
    }

    pub fn total_entries(&self) -> usize {
        self.records.read().len()
    }
}
