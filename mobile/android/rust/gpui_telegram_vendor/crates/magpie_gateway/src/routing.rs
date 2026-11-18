use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use thiserror::Error;

/// Multi-account / multi-member routing strategy modeled after Magpie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingStrategy {
    #[default]
    Smart,
    Order,
    Rotate,
    Usage,
    Pace,
    Cheapest,
    Drain,
    Fastest,
}

impl RoutingStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Smart => "smart",
            Self::Order => "order",
            Self::Rotate => "rotate",
            Self::Usage => "usage",
            Self::Pace => "pace",
            Self::Cheapest => "cheapest",
            Self::Drain => "drain",
            Self::Fastest => "fastest",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "smart" => Some(Self::Smart),
            "order" | "ordered" => Some(Self::Order),
            "rotate" | "round_robin" => Some(Self::Rotate),
            "usage" | "least_used" => Some(Self::Usage),
            "pace" => Some(Self::Pace),
            "cheapest" | "cheap" => Some(Self::Cheapest),
            "drain" => Some(Self::Drain),
            "fastest" | "lowest_latency" | "latency" => Some(Self::Fastest),
            _ => None,
        }
    }

    pub fn all() -> &'static [RoutingStrategy] {
        &[
            Self::Smart,
            Self::Order,
            Self::Rotate,
            Self::Usage,
            Self::Pace,
            Self::Cheapest,
            Self::Drain,
            Self::Fastest,
        ]
    }
}

fn default_weight() -> u32 {
    1
}

fn default_true() -> bool {
    true
}

fn default_sticky_ttl_ms() -> i64 {
    300_000
}

/// One candidate target inside a `RoutingGroup`.
/// `target` may be `"<provider_id>/<model>"`, `"<provider_id>"`, or `"group/<nested_id>"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutingGroupMember {
    pub target: String,
    #[serde(default = "default_weight")]
    pub weight: u32,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_cost_per_1m: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_cost_per_1m: Option<f64>,
    #[serde(default)]
    pub free: bool,
}

impl RoutingGroupMember {
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            weight: 1,
            enabled: true,
            input_cost_per_1m: None,
            output_cost_per_1m: None,
            free: false,
        }
    }
}

/// Virtual model group addressable via `group/<id>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutingGroup {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub strategy: RoutingStrategy,
    #[serde(default)]
    pub members: Vec<RoutingGroupMember>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_fast_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_deep_model: Option<String>,
    #[serde(default = "default_sticky_ttl_ms")]
    pub sticky_ttl_ms: i64,
}

impl RoutingGroup {
    pub fn new(id: impl Into<String>, name: impl Into<String>, strategy: RoutingStrategy) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            strategy,
            members: Vec::new(),
            intent_fast_model: None,
            intent_deep_model: None,
            sticky_ttl_ms: default_sticky_ttl_ms(),
        }
    }

    pub fn canonical_id(&self) -> String {
        self.id.trim().strip_prefix("group/").unwrap_or(self.id.trim()).to_string()
    }

    pub fn qualified_model_name(&self) -> String {
        format!("group/{}", self.canonical_id())
    }
}

/// Runtime seat accounting with 3600s half-life exponential usage decay and cooldown tracking.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RouteSeatMetrics {
    pub weighted_tokens: f64,
    pub last_decay_ms: i64,
    pub total_requests: u64,
    pub cooldown_until_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_status: Option<u16>,
    pub consecutive_failures: u32,
    pub quota_used_pct: f64,
    pub reset_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_latency_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttft_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tps: Option<f64>,
}

impl RouteSeatMetrics {
    /// Decayed token usage with a 3600-second half-life.
    pub fn decayed_usage(&self, now_ms: i64) -> f64 {
        if self.weighted_tokens <= 0.0 {
            return 0.0;
        }
        let elapsed_secs = ((now_ms - self.last_decay_ms).max(0) as f64) / 1000.0;
        self.weighted_tokens * 0.5_f64.powf(elapsed_secs / 3600.0)
    }

    pub fn is_in_cooldown(&self, now_ms: i64) -> bool {
        now_ms < self.cooldown_until_ms
    }

    pub fn record_usage(&mut self, tokens: u64, latency_ms: Option<u64>, now_ms: i64) {
        let decayed = if self.last_decay_ms > 0 {
            self.decayed_usage(now_ms)
        } else {
            self.weighted_tokens
        };
        self.weighted_tokens = decayed + (tokens as f64);
        self.last_decay_ms = now_ms;
        self.total_requests = self.total_requests.saturating_add(1);
        self.consecutive_failures = 0;
        self.cooldown_until_ms = 0;
        self.last_status = Some(200);
        if let Some(ms) = latency_ms {
            self.last_latency_ms = Some(ms);
        }
    }

    pub fn record_metrics(
        &mut self,
        tokens: u64,
        latency_ms: Option<u64>,
        ttft_ms: Option<u64>,
        tps: Option<f64>,
        now_ms: i64,
    ) {
        self.record_usage(tokens, latency_ms, now_ms);
        if let Some(ms) = ttft_ms {
            self.ttft_ms = Some(ms);
        }
        if let Some(rate) = tps {
            self.tps = Some(rate);
        }
    }

    pub fn record_failure(
        &mut self,
        status: Option<u16>,
        retry_after_secs: Option<u64>,
        now_ms: i64,
    ) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.last_status = status;

        let cooldown_ms = if let Some(secs) = retry_after_secs {
            (secs as i64).saturating_mul(1_000).max(1_000)
        } else {
            match status {
                Some(401) | Some(403) => {
                    let shift = self.consecutive_failures.saturating_sub(1).min(6);
                    (5_000i64.saturating_mul(1i64 << shift)).min(300_000)
                }
                Some(402) => 300_000,
                Some(429) => {
                    let shift = self.consecutive_failures.saturating_sub(1).min(7);
                    (5_000i64.saturating_mul(1i64 << shift)).min(900_000)
                }
                Some(code) if code >= 500 => {
                    let shift = self.consecutive_failures.saturating_sub(1).min(6);
                    (3_000i64.saturating_mul(1i64 << shift)).min(300_000)
                }
                _ => 0,
            }
        };
        if cooldown_ms > 0 {
            self.cooldown_until_ms = now_ms.saturating_add(cooldown_ms);
        }
    }
}

/// Category of circuit breaker rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestKind {
    QuotaRest,
    CreditRest,
    RateRest,
    OverloadRest,
}

impl RestKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::QuotaRest => "quota_rest",
            Self::CreditRest => "credit_rest",
            Self::RateRest => "rate_rest",
            Self::OverloadRest => "overload_rest",
        }
    }
}

/// CreditRest ladder in milliseconds: [30m, 1h, 2h, 4h, 8h, 24h, 7d]
pub const CREDIT_REST_LADDER_MS: &[i64] = &[
    30 * 60 * 1000,          // 30 min (1,800,000 ms)
    60 * 60 * 1000,          // 1 hour (3,600,000 ms)
    2 * 60 * 60 * 1000,      // 2 hours (7,200,000 ms)
    4 * 60 * 60 * 1000,      // 4 hours (14,400,000 ms)
    8 * 60 * 60 * 1000,      // 8 hours (28,800,000 ms)
    24 * 60 * 60 * 1000,     // 24 hours (86,400,000 ms)
    7 * 24 * 60 * 60 * 1000, // 7 days (604,800,000 ms)
];

pub const OVERLOAD_REST_MS: i64 = 30_000; // 30 seconds
pub const MAX_RATE_REST_MS: i64 = 30 * 60 * 1000; // 30 minutes
pub const BASE_RATE_REST_MS: i64 = 5_000; // 5 seconds

/// Rest state tracking an active circuit-breaker rest on a seat or account.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RestState {
    pub kind: RestKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    pub until_ms: i64,
    pub failures: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl RestState {
    pub fn is_active(&self, now_ms: i64) -> bool {
        now_ms < self.until_ms
    }

    pub fn remaining_ms(&self, now_ms: i64) -> i64 {
        (self.until_ms - now_ms).max(0)
    }
}

/// Circuit breaker tracking rest ladders and forbiddenAlike disambiguation.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CircuitBreaker {
    #[serde(default)]
    pub rests: HashMap<String, RestState>,
    #[serde(default)]
    pub credit_steps: HashMap<String, usize>,
    #[serde(default)]
    pub recent_error_fingerprints: HashMap<String, Vec<(String, i64)>>,
    #[serde(default)]
    pub known_forbidden_alike: HashSet<String>,
    #[serde(default)]
    pub last_cleared_accounts: Vec<String>,
}

impl CircuitBreaker {
    pub fn error_fingerprint(provider_id: &str, status: u16, error_text: &str) -> String {
        let norm_err = error_text
            .trim()
            .to_ascii_lowercase()
            .chars()
            .take(120)
            .collect::<String>();
        format!("{provider_id}::{status}::{norm_err}")
    }

    pub fn is_forbidden_alike(&self, provider_id: &str, status: u16, error_text: &str) -> bool {
        let fp = Self::error_fingerprint(provider_id, status, error_text);
        if self.known_forbidden_alike.contains(&fp) {
            return true;
        }
        let err_lower = error_text.to_ascii_lowercase();
        if status == 400 || status == 403 {
            if err_lower.contains("safety")
                || err_lower.contains("policy")
                || err_lower.contains("prompt")
                || err_lower.contains("content_filter")
                || err_lower.contains("invalid_request")
                || err_lower.contains("unsupported_country")
                || err_lower.contains("context_length")
                || err_lower.contains("maximum context")
                || err_lower.contains("model not found")
                || err_lower.contains("not supported")
            {
                return true;
            }
        }
        false
    }

    pub fn record_upstream_error(
        &mut self,
        seat_or_account: &str,
        provider_id: &str,
        status: u16,
        error_text: &str,
        retry_after_secs: Option<u64>,
        reset_at_ms: Option<i64>,
        now_ms: i64,
    ) -> Result<Option<RestState>, String> {
        let fp = Self::error_fingerprint(provider_id, status, error_text);

        if self.known_forbidden_alike.contains(&fp) {
            self.last_cleared_accounts = vec![seat_or_account.to_string()];
            return Ok(None);
        }

        let err_lower = error_text.to_ascii_lowercase();
        let inherent_rejection = (status == 400 || status == 403)
            && (err_lower.contains("safety")
                || err_lower.contains("policy")
                || err_lower.contains("prompt")
                || err_lower.contains("content_filter")
                || err_lower.contains("invalid_request")
                || err_lower.contains("unsupported_country")
                || err_lower.contains("context_length")
                || err_lower.contains("maximum context")
                || err_lower.contains("model not found"));

        if inherent_rejection {
            self.known_forbidden_alike.insert(fp);
            self.last_cleared_accounts = vec![seat_or_account.to_string()];
            return Ok(None);
        }

        // Multi-account error disambiguation across identical 403 / 5xx
        if status == 403 || status >= 500 {
            let list = self.recent_error_fingerprints.entry(fp.clone()).or_default();
            list.retain(|(_, ts)| now_ms.saturating_sub(*ts) <= 60_000);
            list.push((seat_or_account.to_string(), now_ms));

            let distinct_accounts: HashSet<String> = list.iter().map(|(a, _)| a.clone()).collect();
            if distinct_accounts.len() >= 2 {
                self.known_forbidden_alike.insert(fp);
                let mut cleared = Vec::new();
                for acct in &distinct_accounts {
                    if let Some(r) = self.rests.get(acct) {
                        if r.status == Some(status) {
                            self.rests.remove(acct);
                            cleared.push(acct.clone());
                        }
                    }
                    cleared.push(acct.clone());
                }
                cleared.sort();
                cleared.dedup();
                self.last_cleared_accounts = cleared;
                return Ok(None);
            }
        }

        let (kind, duration_ms) = if status == 402
            || err_lower.contains("credit")
            || err_lower.contains("insufficient_quota")
            || err_lower.contains("billing")
            || err_lower.contains("balance")
        {
            let step = self.credit_steps.entry(seat_or_account.to_string()).or_insert(0);
            let idx = (*step).min(CREDIT_REST_LADDER_MS.len().saturating_sub(1));
            let dur = CREDIT_REST_LADDER_MS[idx];
            *step = (*step + 1).min(CREDIT_REST_LADDER_MS.len().saturating_sub(1));
            (RestKind::CreditRest, dur)
        } else if (status == 429 && (err_lower.contains("overload") || err_lower.contains("capacity")))
            || status == 503
            || status == 529
        {
            (RestKind::OverloadRest, OVERLOAD_REST_MS)
        } else if reset_at_ms.is_some() || err_lower.contains("quota") {
            let dur = reset_at_ms
                .map(|r| (r - now_ms).max(1000))
                .or_else(|| retry_after_secs.map(|s| (s as i64).saturating_mul(1000).max(1000)))
                .unwrap_or(3600 * 1000);
            (RestKind::QuotaRest, dur)
        } else if status == 429 || err_lower.contains("rate_limit") || err_lower.contains("rate limit") {
            let current_failures = self
                .rests
                .get(seat_or_account)
                .map(|r| r.failures)
                .unwrap_or(0);
            let shift = current_failures.min(9);
            let exp_dur = (BASE_RATE_REST_MS.saturating_mul(1i64 << shift)).min(MAX_RATE_REST_MS);
            let dur = if let Some(secs) = retry_after_secs {
                ((secs as i64) * 1000).max(exp_dur).min(MAX_RATE_REST_MS)
            } else {
                exp_dur
            };
            (RestKind::RateRest, dur)
        } else {
            let current_failures = self
                .rests
                .get(seat_or_account)
                .map(|r| r.failures)
                .unwrap_or(0);
            let dur = (5_000i64.saturating_mul(1i64 << current_failures.min(6))).min(300_000);
            (RestKind::RateRest, dur)
        };

        let failures = self
            .rests
            .get(seat_or_account)
            .map(|r| r.failures + 1)
            .unwrap_or(1);

        let rest_state = RestState {
            kind,
            status: Some(status),
            until_ms: now_ms.saturating_add(duration_ms),
            failures,
            error: Some(error_text.to_string()),
        };

        self.rests.insert(seat_or_account.to_string(), rest_state.clone());
        self.last_cleared_accounts.clear();
        Ok(Some(rest_state))
    }

    pub fn is_in_rest(&self, seat_or_account: &str, now_ms: i64) -> bool {
        self.rests
            .get(seat_or_account)
            .is_some_and(|r| r.is_active(now_ms))
    }

    pub fn get_rest_state(&self, seat_or_account: &str, now_ms: i64) -> Option<&RestState> {
        let state = self.rests.get(seat_or_account)?;
        if state.is_active(now_ms) {
            Some(state)
        } else {
            None
        }
    }

    pub fn clear_rest(&mut self, seat_or_account: &str) {
        self.rests.remove(seat_or_account);
        self.last_cleared_accounts = vec![seat_or_account.to_string()];
    }

    pub fn record_success(&mut self, seat_or_account: &str) {
        self.rests.remove(seat_or_account);
        self.credit_steps.remove(seat_or_account);
        self.last_cleared_accounts = vec![seat_or_account.to_string()];
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionAffinityEntry {
    pub target: String,
    pub expires_at_ms: i64,
}

/// Trace record explaining why the router chose a particular target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteDecisionTrace {
    pub timestamp_ms: i64,
    pub requested_model: String,
    pub resolved_provider_id: String,
    pub resolved_model: String,
    pub strategy: RoutingStrategy,
    pub reason: String,
    pub candidates_considered: usize,
    #[serde(default)]
    pub masking_hits: usize,
}

/// Mutable routing state holding seat decay counters, rotation cursors, affinity, and trace ring buffer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RouteRuntimeMetrics {
    #[serde(default)]
    pub seats: HashMap<String, RouteSeatMetrics>,
    #[serde(default)]
    pub rotate_cursor: HashMap<String, usize>,
    #[serde(default)]
    pub drain_pinned: HashMap<String, String>,
    #[serde(default)]
    pub session_affinity: HashMap<String, SessionAffinityEntry>,
    #[serde(default)]
    pub traces: VecDeque<RouteDecisionTrace>,
    #[serde(default)]
    pub circuit_breaker: CircuitBreaker,
}

const MAX_TRACE_RING: usize = 64;

impl RouteRuntimeMetrics {
    pub fn record_usage(&mut self, seat_key: &str, tokens: u64, latency_ms: Option<u64>, now_ms: i64) {
        self.seats
            .entry(seat_key.to_string())
            .or_default()
            .record_usage(tokens, latency_ms, now_ms);
        self.circuit_breaker.record_success(seat_key);
    }

    pub fn record_metrics(
        &mut self,
        seat_key: &str,
        tokens: u64,
        latency_ms: Option<u64>,
        ttft_ms: Option<u64>,
        tps: Option<f64>,
        now_ms: i64,
    ) {
        self.seats
            .entry(seat_key.to_string())
            .or_default()
            .record_metrics(tokens, latency_ms, ttft_ms, tps, now_ms);
        self.circuit_breaker.record_success(seat_key);
    }

    pub fn record_failure(
        &mut self,
        seat_key: &str,
        status: Option<u16>,
        retry_after_secs: Option<u64>,
        now_ms: i64,
    ) {
        self.seats
            .entry(seat_key.to_string())
            .or_default()
            .record_failure(status, retry_after_secs, now_ms);
    }

    pub fn record_failure_disambiguated(
        &mut self,
        seat_key: &str,
        provider_id: &str,
        status: u16,
        error_text: &str,
        retry_after_secs: Option<u64>,
        reset_at_ms: Option<i64>,
        now_ms: i64,
    ) -> bool {
        match self.circuit_breaker.record_upstream_error(
            seat_key,
            provider_id,
            status,
            error_text,
            retry_after_secs,
            reset_at_ms,
            now_ms,
        ) {
            Ok(Some(rest)) => {
                let seat = self.seats.entry(seat_key.to_string()).or_default();
                seat.consecutive_failures = rest.failures;
                seat.last_status = rest.status;
                seat.cooldown_until_ms = rest.until_ms;
                true
            }
            Ok(None) => {
                // When record_upstream_error returns Ok(None) (request-level rejection or
                // multi-account identical error unwinding) or unwinds rests, synchronize self.seats:
                // For any seat that is no longer in self.circuit_breaker.rests (or for all accounts
                // whose rest was cleared), reset seat.cooldown_until_ms = 0 and seat.consecutive_failures = 0.
                for acct in &self.circuit_breaker.last_cleared_accounts {
                    if let Some(seat) = self.seats.get_mut(acct) {
                        seat.cooldown_until_ms = 0;
                        seat.consecutive_failures = 0;
                    }
                }
                if let Some(seat) = self.seats.get_mut(seat_key) {
                    seat.cooldown_until_ms = 0;
                    seat.consecutive_failures = 0;
                }
                false
            }
            Err(_) => false,
        }
    }

    pub fn push_trace(&mut self, trace: RouteDecisionTrace) {
        if self.traces.len() >= MAX_TRACE_RING {
            self.traces.pop_front();
        }
        self.traces.push_back(trace);
    }
}

/// Classified intent tier used for optional fast/deep split routing inside a `RoutingGroup`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentTier {
    Fast,
    Standard,
    Deep,
}

/// Classify a turn into `Fast`, `Standard`, or `Deep` without an external classifier model.
pub fn classify_intent(prompt_preview: &str, messages_count: usize, tools_count: usize) -> IntentTier {
    let trimmed = prompt_preview.trim();
    let lower = trimmed.to_ascii_lowercase();
    let chars = trimmed.chars().count();

    let deep_keywords = [
        "architecture",
        "refactor",
        "debug",
        "analyze",
        "proof",
        "theorem",
        "implement",
        "design",
        "security",
        "架构",
        "重构",
        "深入",
        "推理",
        "证明",
        "调试",
        "设计",
        "分析",
    ];
    if chars > 600
        || (tools_count >= 5 && chars > 120)
        || deep_keywords
            .iter()
            .any(|kw| trimmed.contains(kw) || lower.contains(kw))
    {
        return IntentTier::Deep;
    }

    let fast_keywords = [
        "translate",
        "summarize",
        "hello",
        "hi",
        "ping",
        "翻译",
        "总结",
        "你好",
        "简述",
    ];
    if chars <= 160
        && tools_count == 0
        && messages_count <= 4
        && (chars <= 48
            || fast_keywords
                .iter()
                .any(|kw| trimmed.contains(kw) || lower.contains(kw)))
    {
        return IntentTier::Fast;
    }

    IntentTier::Standard
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RoutingError {
    #[error("Routing group '{0}' not found")]
    GroupNotFound(String),
    #[error("Routing group cycle detected involving '{0}'")]
    CycleDetected(String),
    #[error("Routing group nesting exceeded max depth {0}")]
    MaxDepthExceeded(usize),
    #[error("Routing group '{0}' has no enabled members")]
    NoEnabledMembers(String),
}

/// Concrete resolved `(provider_id, model)` candidate produced by resolving a group or target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRouteTarget {
    pub provider_id: String,
    pub model: String,
    pub strategy: RoutingStrategy,
    pub reason: String,
}

pub fn split_target_spec(target: &str, default_provider: &str) -> (String, String) {
    let trimmed = target.trim();
    if let Some((prov, model)) = trimmed.split_once('/') {
        let p = prov.trim();
        let m = model.trim();
        if !p.is_empty() && !m.is_empty() {
            return (p.to_string(), m.to_string());
        }
    }
    (default_provider.to_string(), trimmed.to_string())
}

const MAX_GROUP_DEPTH: usize = 4;

fn flatten_group_members(
    group_id: &str,
    groups: &HashMap<String, RoutingGroup>,
    depth: usize,
    visiting: &mut HashSet<String>,
    out: &mut Vec<RoutingGroupMember>,
) -> Result<(), RoutingError> {
    if depth >= MAX_GROUP_DEPTH {
        return Err(RoutingError::MaxDepthExceeded(MAX_GROUP_DEPTH));
    }
    let clean_id = group_id.trim().strip_prefix("group/").unwrap_or(group_id.trim());
    if !visiting.insert(clean_id.to_string()) {
        return Err(RoutingError::CycleDetected(clean_id.to_string()));
    }
    let group = groups
        .get(clean_id)
        .ok_or_else(|| RoutingError::GroupNotFound(clean_id.to_string()))?;

    for member in &group.members {
        if !member.enabled || member.target.trim().is_empty() {
            continue;
        }
        let target = member.target.trim();
        if let Some(nested) = target.strip_prefix("group/") {
            flatten_group_members(nested, groups, depth + 1, visiting, out)?;
        } else {
            out.push(member.clone());
        }
    }

    visiting.remove(clean_id);
    Ok(())
}

/// Resolve an ordered list of candidate `(provider_id, model)` targets for a `RoutingGroup`.
#[allow(clippy::too_many_arguments)]
pub fn resolve_group_targets(
    group_id: &str,
    groups: &HashMap<String, RoutingGroup>,
    metrics: &mut RouteRuntimeMetrics,
    default_provider_id: &str,
    session_key: Option<&str>,
    prompt_preview: &str,
    messages_count: usize,
    tools_count: usize,
    now_ms: i64,
) -> Result<Vec<ResolvedRouteTarget>, RoutingError> {
    let clean_id = group_id.trim().strip_prefix("group/").unwrap_or(group_id.trim());
    let group = groups
        .get(clean_id)
        .ok_or_else(|| RoutingError::GroupNotFound(clean_id.to_string()))?;

    let mut visited = HashSet::new();
    let mut flat = Vec::new();
    flatten_group_members(clean_id, groups, 0, &mut visited, &mut flat)?;
    if flat.is_empty() {
        return Err(RoutingError::NoEnabledMembers(clean_id.to_string()));
    }

    let pool_key = format!("group/{clean_id}");
    let intent = classify_intent(prompt_preview, messages_count, tools_count);
    let intent_target = match intent {
        IntentTier::Fast => group
            .intent_fast_model
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|m| (m, "intent_fast")),
        IntentTier::Deep => group
            .intent_deep_model
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|m| (m, "intent_deep")),
        IntentTier::Standard => None,
    };

    let is_cooled = |target: &str, m: &RouteRuntimeMetrics| -> bool {
        m.seats.get(target).is_some_and(|s| s.is_in_cooldown(now_ms))
            || m.circuit_breaker.is_in_rest(target, now_ms)
    };

    // Check session affinity first (only if target still exists and is not cooled down)
    let mut pinned_first: Option<(String, String)> = None;
    if intent_target.is_none() && group.sticky_ttl_ms > 0 {
        if let Some(sess) = session_key.map(str::trim).filter(|s| !s.is_empty()) {
            let aff_key = format!("{sess}::{pool_key}");
            if let Some(entry) = metrics.session_affinity.get(&aff_key) {
                if now_ms < entry.expires_at_ms {
                    let exists = flat.iter().any(|m| m.target == entry.target);
                    let in_cooldown = is_cooled(&entry.target, metrics);
                    if exists && !in_cooldown {
                        pinned_first = Some((entry.target.clone(), "session_affinity".to_string()));
                    }
                }
            }
        }
    }

    let mut ordered_indices: Vec<usize> = (0..flat.len()).collect();
    match group.strategy {
        RoutingStrategy::Order => {}
        RoutingStrategy::Rotate => {
            let cursor = metrics.rotate_cursor.get(&pool_key).copied().unwrap_or(0) % flat.len();
            ordered_indices.rotate_left(cursor);
            if intent_target.is_none() && pinned_first.is_none() {
                metrics
                    .rotate_cursor
                    .insert(pool_key.clone(), (cursor + 1) % flat.len());
            }
        }
        RoutingStrategy::Usage => {
            ordered_indices.sort_by(|&a, &b| {
                let sa = metrics.seats.get(&flat[a].target);
                let sb = metrics.seats.get(&flat[b].target);
                let ca = is_cooled(&flat[a].target, metrics);
                let cb = is_cooled(&flat[b].target, metrics);
                let ua = sa.map(|s| s.decayed_usage(now_ms)).unwrap_or(0.0);
                let ub = sb.map(|s| s.decayed_usage(now_ms)).unwrap_or(0.0);
                ca.cmp(&cb)
                    .then_with(|| ua.total_cmp(&ub))
                    .then_with(|| a.cmp(&b))
            });
        }
        RoutingStrategy::Pace => {
            ordered_indices.sort_by(|&a, &b| {
                let sa = metrics.seats.get(&flat[a].target);
                let sb = metrics.seats.get(&flat[b].target);
                let ca = is_cooled(&flat[a].target, metrics);
                let cb = is_cooled(&flat[b].target, metrics);
                let score = |m: Option<&RouteSeatMetrics>| -> f64 {
                    let Some(seat) = m else { return 0.0 };
                    let urgency = if seat.reset_at_ms > now_ms {
                        let rem = ((seat.reset_at_ms - now_ms) as f64) / 3_600_000.0;
                        seat.quota_used_pct + rem.min(24.0) * 0.5
                    } else {
                        seat.quota_used_pct
                    };
                    urgency + seat.decayed_usage(now_ms) * 0.001
                };
                ca.cmp(&cb)
                    .then_with(|| score(sa).total_cmp(&score(sb)))
                    .then_with(|| a.cmp(&b))
            });
        }
        RoutingStrategy::Cheapest => {
            ordered_indices.sort_by(|&a, &b| {
                let ma = &flat[a];
                let mb = &flat[b];
                let ca = is_cooled(&ma.target, metrics);
                let cb = is_cooled(&mb.target, metrics);
                let price = |m: &RoutingGroupMember| -> f64 {
                    if m.free {
                        0.0
                    } else {
                        m.input_cost_per_1m.unwrap_or(5.0) + m.output_cost_per_1m.unwrap_or(15.0)
                    }
                };
                ca.cmp(&cb)
                    .then_with(|| price(ma).total_cmp(&price(mb)))
                    .then_with(|| a.cmp(&b))
            });
        }
        RoutingStrategy::Fastest => {
            ordered_indices.sort_by(|&a, &b| {
                let ma = &flat[a];
                let mb = &flat[b];
                let sa = metrics.seats.get(&ma.target);
                let sb = metrics.seats.get(&mb.target);
                let ca = is_cooled(&ma.target, metrics);
                let cb = is_cooled(&mb.target, metrics);

                let latency_score = |s: Option<&RouteSeatMetrics>| -> f64 {
                    let Some(seat) = s else { return 500.0 };
                    let base_latency = seat
                        .ttft_ms
                        .or(seat.last_latency_ms)
                        .unwrap_or(500) as f64;
                    let tps_bonus = seat.tps.map(|rate| (rate / 50.0).min(5.0) * 20.0).unwrap_or(0.0);
                    (base_latency - tps_bonus).max(1.0)
                };

                let la = latency_score(sa);
                let lb = latency_score(sb);

                ca.cmp(&cb)
                    .then_with(|| la.total_cmp(&lb))
                    .then_with(|| a.cmp(&b))
            });
        }
        RoutingStrategy::Drain => {
            let pinned_target = metrics.drain_pinned.get(&pool_key).cloned();
            let valid_pinned = pinned_target.and_then(|pt| {
                let pos = flat.iter().position(|m| m.target == pt)?;
                let cooled = is_cooled(&pt, metrics)
                    || metrics.seats.get(&pt).is_some_and(|s| s.quota_used_pct >= 99.5);
                (!cooled).then_some(pos)
            });
            let chosen_pos = valid_pinned.unwrap_or_else(|| {
                flat.iter()
                    .position(|m| {
                        !is_cooled(&m.target, metrics)
                            && !metrics.seats.get(&m.target).is_some_and(|s| s.quota_used_pct >= 99.5)
                    })
                    .unwrap_or(0)
            });
            metrics
                .drain_pinned
                .insert(pool_key.clone(), flat[chosen_pos].target.clone());
            ordered_indices.sort_by_key(|&idx| usize::from(idx != chosen_pos));
        }
        RoutingStrategy::Smart => {
            ordered_indices.sort_by(|&a, &b| {
                let ma = &flat[a];
                let mb = &flat[b];
                let sa = metrics.seats.get(&ma.target);
                let sb = metrics.seats.get(&mb.target);
                let smart_cost = |m: &RoutingGroupMember, seat: Option<&RouteSeatMetrics>| -> (u8, f64) {
                    let cooled = is_cooled(&m.target, metrics);
                    let tier = if cooled { 2 } else if seat.is_some_and(|s| s.quota_used_pct >= 90.0) { 1 } else { 0 };
                    let weight = m.weight.max(1) as f64;
                    let load = seat.map(|s| s.decayed_usage(now_ms)).unwrap_or(0.0) / weight;
                    let free_bonus = if m.free { -1000.0 } else { 0.0 };
                    (tier, load + free_bonus)
                };
                let (ta, la) = smart_cost(ma, sa);
                let (tb, lb) = smart_cost(mb, sb);
                ta.cmp(&tb).then_with(|| la.total_cmp(&lb)).then_with(|| a.cmp(&b))
            });
        }
    }

    let strategy_reason = match group.strategy {
        RoutingStrategy::Smart => "smart_score",
        RoutingStrategy::Order => "order",
        RoutingStrategy::Rotate => "rotate",
        RoutingStrategy::Usage => "least_usage",
        RoutingStrategy::Pace => "pace",
        RoutingStrategy::Cheapest => "cheapest",
        RoutingStrategy::Drain => "drain",
        RoutingStrategy::Fastest => "fastest_latency",
    };

    let mut out = Vec::new();
    let mut push_unique = |target_str: &str, reason: &str| {
        let (provider_id, model) = split_target_spec(target_str, default_provider_id);
        if !model.is_empty()
            && !out.iter().any(|r: &ResolvedRouteTarget| {
                r.provider_id == provider_id && r.model == model
            })
        {
            out.push(ResolvedRouteTarget {
                provider_id,
                model,
                strategy: group.strategy,
                reason: reason.to_string(),
            });
        }
    };

    // Intent fast/deep override has highest priority if configured on the group
    if let Some((override_m, override_reason)) = intent_target {
        push_unique(override_m, override_reason);
    }

    if let Some((ref affinity_target, ref aff_reason)) = pinned_first {
        push_unique(affinity_target, aff_reason);
    }

    for idx in ordered_indices {
        push_unique(&flat[idx].target, strategy_reason);
    }

    if let (Some(sess), Some(primary)) = (
        session_key.map(str::trim).filter(|s| !s.is_empty()),
        out.first(),
    ) {
        if group.sticky_ttl_ms > 0 {
            let aff_key = format!("{sess}::{pool_key}");
            metrics.session_affinity.insert(
                aff_key,
                SessionAffinityEntry {
                    target: format!("{}/{}", primary.provider_id, primary.model),
                    expires_at_ms: now_ms.saturating_add(group.sticky_ttl_ms),
                },
            );
        }
    }

    Ok(out)
}

/// Parse an HTTP `Retry-After` header value (integer seconds or RFC 7231 HTTP-date).
pub fn parse_retry_after_header(raw: &str) -> Option<u64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(secs) = trimmed.parse::<u64>() {
        return Some(secs);
    }

    let now_ts = chrono::Utc::now().timestamp();
    if let Ok(dt) = chrono::DateTime::parse_from_rfc2822(trimmed) {
        let diff = dt.timestamp().saturating_sub(now_ts);
        return Some(diff.max(0) as u64);
    }

    for fmt in [
        "%a, %d %b %Y %H:%M:%S GMT",
        "%a, %d %b %Y %H:%M:%S UTC",
        "%A, %d-%b-%y %H:%M:%S GMT",
        "%a %b %e %H:%M:%S %Y",
    ] {
        if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(trimmed, fmt) {
            let diff = ndt.and_utc().timestamp().saturating_sub(now_ts);
            return Some(diff.max(0) as u64);
        }
    }

    // Fallback for HTTP-dates where weekday prefix is present (even if weekday mismatches calendar):
    // strip leading "<day>, " and parse "%d %b %Y %H:%M:%S [GMT|UTC]".
    if let Some((_wkday, rest)) = trimmed.split_once(',') {
        let rest_trim = rest.trim();
        for fmt in [
            "%d %b %Y %H:%M:%S GMT",
            "%d %b %Y %H:%M:%S UTC",
            "%d-%b-%y %H:%M:%S GMT",
            "%d-%b-%y %H:%M:%S UTC",
        ] {
            if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(rest_trim, fmt) {
                let diff = ndt.and_utc().timestamp().saturating_sub(now_ts);
                return Some(diff.max(0) as u64);
            }
        }
    }

    None
}
