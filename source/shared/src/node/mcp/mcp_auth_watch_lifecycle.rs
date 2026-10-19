use std::collections::BTreeMap;
use url::Url;

use super::mcp_auth_watch::{
    auth_watch_key, AUTH_WATCH_POLL_INTERVAL_MS, AUTH_WATCH_POLL_TIMEOUT_MS,
    AUTH_WATCH_TIMEOUT_MS,
};
use super::mcp_server_id::{parse_i32_mcp_server_id, validate_mcp_server_id};

pub const DEFAULT_MCP_ACCOUNT_KEY: &str = "default";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpAuthTransport {
    Http,
    Sse,
    Stdio,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpAuthServerSnapshot {
    pub server_id: String,
    pub server_name: String,
    pub server_url: Option<String>,
    pub transport: McpAuthTransport,
    pub disabled_by_team_admin_policy: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpBackendAuthStatus {
    pub is_available: bool,
    pub requires_auth: bool,
    pub has_valid_token: bool,
    pub auth_url: String,
    pub error: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpAuthServerGate {
    Eligible(McpAuthServerSnapshot),
    NotConfigured,
    AdminBlocked,
    UnsupportedTransport,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpAuthStatusDecision {
    AlreadyAuthenticated,
    Start { authorization_url: String },
    Unreachable { detail: String },
    NotSupported { detail: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingMcpAuthWatch {
    pub generation: u64,
    pub server_id: String,
    pub server_name: String,
    pub server_url: String,
    pub account_key: String,
    pub requesting_agent_id: Option<String>,
    pub force_reauth: bool,
    pub suppress_first_poll: bool,
    pub started_at_ms: u64,
    pub expires_at_ms: u64,
    pub next_poll_at_ms: u64,
    pub is_polling: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpAuthPollRequest {
    pub generation: u64,
    pub server_id: String,
    pub server_name: String,
    pub server_url: String,
    pub account_key: String,
    pub requesting_agent_id: Option<String>,
    pub deadline_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpAuthPollOutcome {
    TokenValid,
    TokenInvalid,
    AdminBlocked,
    Unreachable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpAuthWatchCompletion {
    pub generation: u64,
    pub server_id: String,
    pub server_name: String,
    pub account_key: String,
    pub requesting_agent_id: Option<String>,
    pub outcome: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpAuthPollTick {
    Idle,
    Suppressed,
    Expired(McpAuthWatchCompletion),
    Request(McpAuthPollRequest),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpAuthPollSettlement {
    Pending,
    Completed(McpAuthWatchCompletion),
    Cancelled(McpAuthWatchCompletion),
    Stale,
}

#[derive(Clone, Debug, Default)]
pub struct McpAuthWatchLifecycle {
    watches: BTreeMap<String, PendingMcpAuthWatch>,
    next_generation: u64,
}

pub fn normalize_mcp_account_key(raw: &str) -> Result<String, &'static str> {
    let key = raw.trim().to_ascii_lowercase();
    if key.is_empty() {
        return Err("MCP account label is required.");
    }
    if key.len() > 320 || key.chars().any(char::is_control) {
        return Err("MCP account label is invalid.");
    }
    Ok(key)
}

fn is_loopback_hostname(hostname: &str) -> bool {
    matches!(hostname, "localhost" | "127.0.0.1")
}

pub fn validate_authorization_url(auth_url: &str, server_url: Option<&str>) -> Option<String> {
    let parsed = Url::parse(auth_url.trim()).ok()?;
    if parsed.scheme() == "https" {
        return Some(parsed.to_string());
    }
    if parsed.scheme() != "http" || !parsed.host_str().is_some_and(is_loopback_hostname) {
        return None;
    }
    let server = Url::parse(server_url?).ok()?;
    if server.host_str().is_some_and(is_loopback_hostname) {
        Some(parsed.to_string())
    } else {
        None
    }
}

pub fn gate_auth_server(
    initial: Option<&McpAuthServerSnapshot>,
    fresh_if_initially_admin_blocked: Option<&McpAuthServerSnapshot>,
) -> McpAuthServerGate {
    let Some(initial) = initial else {
        return McpAuthServerGate::NotConfigured;
    };
    if validate_mcp_server_id(&initial.server_id).is_err()
        || parse_i32_mcp_server_id(&initial.server_id).is_err()
    {
        return McpAuthServerGate::NotConfigured;
    }

    let selected = if initial.disabled_by_team_admin_policy {
        let Some(fresh) = fresh_if_initially_admin_blocked else {
            return McpAuthServerGate::AdminBlocked;
        };
        if fresh.server_id != initial.server_id || fresh.disabled_by_team_admin_policy {
            return McpAuthServerGate::AdminBlocked;
        }
        fresh
    } else {
        initial
    };

    if matches!(selected.transport, McpAuthTransport::Stdio) {
        return McpAuthServerGate::UnsupportedTransport;
    }
    McpAuthServerGate::Eligible(selected.clone())
}

pub fn classify_backend_auth_status(
    server_url: Option<&str>,
    status: &McpBackendAuthStatus,
    force_reauth: bool,
) -> McpAuthStatusDecision {
    if !force_reauth && (status.has_valid_token || (status.is_available && !status.requires_auth)) {
        return McpAuthStatusDecision::AlreadyAuthenticated;
    }
    if status.requires_auth && !status.auth_url.trim().is_empty() {
        return match validate_authorization_url(&status.auth_url, server_url) {
            Some(authorization_url) => McpAuthStatusDecision::Start { authorization_url },
            None => McpAuthStatusDecision::NotSupported {
                detail: "The connector returned an invalid authorization URL.".into(),
            },
        };
    }
    if !status.is_available && !status.requires_auth {
        return McpAuthStatusDecision::Unreachable {
            detail: bounded_status_detail(&status.error, "Connector unavailable"),
        };
    }
    McpAuthStatusDecision::NotSupported {
        detail: "Connector authentication is not available.".into(),
    }
}

fn bounded_status_detail(value: &str, fallback: &str) -> String {
    let cleaned: String = value
        .chars()
        .filter(|ch| !ch.is_control())
        .take(512)
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        fallback.to_string()
    } else {
        cleaned.to_string()
    }
}

impl McpAuthWatchLifecycle {
    pub fn new() -> Self {
        Self {
            watches: BTreeMap::new(),
            next_generation: 1,
        }
    }

    pub fn from_restored(
        now_ms: u64,
        next_generation: u64,
        watches: impl IntoIterator<Item = PendingMcpAuthWatch>,
    ) -> Self {
        let mut lifecycle = Self {
            watches: BTreeMap::new(),
            next_generation: next_generation.max(1),
        };
        for mut watch in watches {
            if validate_mcp_server_id(&watch.server_id).is_err()
                || parse_i32_mcp_server_id(&watch.server_id).is_err()
                || normalize_mcp_account_key(&watch.account_key).is_err()
                || watch.server_url.trim().is_empty()
                || watch.server_name.trim().is_empty()
                || watch.expires_at_ms <= now_ms
            {
                continue;
            }
            watch.is_polling = false;
            watch.next_poll_at_ms = watch.next_poll_at_ms.max(now_ms);
            lifecycle.next_generation = lifecycle
                .next_generation
                .max(watch.generation.saturating_add(1));
            lifecycle.watches.insert(
                auth_watch_key(&watch.server_id, &watch.account_key),
                watch,
            );
        }
        lifecycle
    }

    pub fn next_generation(&self) -> u64 {
        self.next_generation
    }

    pub fn watches(&self) -> impl Iterator<Item = &PendingMcpAuthWatch> {
        self.watches.values()
    }

    pub fn watch(&self, server_id: &str, account_key: &str) -> Option<&PendingMcpAuthWatch> {
        let server_id = validate_mcp_server_id(server_id).ok()?;
        parse_i32_mcp_server_id(&server_id).ok()?;
        let account_key = normalize_mcp_account_key(account_key).ok()?;
        self.watches.get(&auth_watch_key(&server_id, &account_key))
    }

    pub fn begin_watch(
        &mut self,
        now_ms: u64,
        server_id: &str,
        server_name: &str,
        server_url: &str,
        account_key: &str,
        requesting_agent_id: Option<&str>,
        force_reauth: bool,
    ) -> Result<(PendingMcpAuthWatch, Option<PendingMcpAuthWatch>), &'static str> {
        let server_id = validate_mcp_server_id(server_id)?;
        parse_i32_mcp_server_id(&server_id)?;
        let account_key = normalize_mcp_account_key(account_key)?;
        let server_name = server_name.trim();
        let server_url = server_url.trim();
        if server_name.is_empty() || server_name.len() > 320 {
            return Err("MCP server name is invalid.");
        }
        let parsed_server = Url::parse(server_url).map_err(|_| "MCP server URL is invalid.")?;
        if !matches!(parsed_server.scheme(), "http" | "https") {
            return Err("MCP server URL is invalid.");
        }
        let key = auth_watch_key(&server_id, &account_key);
        let previous = self.watches.remove(&key);
        let preserved_agent = requesting_agent_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| previous.as_ref().and_then(|watch| watch.requesting_agent_id.clone()));
        let generation = self.next_generation.max(1);
        self.next_generation = generation.saturating_add(1).max(1);
        let watch = PendingMcpAuthWatch {
            generation,
            server_id,
            server_name: server_name.to_string(),
            server_url: parsed_server.to_string(),
            account_key,
            requesting_agent_id: preserved_agent,
            force_reauth,
            suppress_first_poll: force_reauth,
            started_at_ms: now_ms,
            expires_at_ms: now_ms.saturating_add(AUTH_WATCH_TIMEOUT_MS),
            next_poll_at_ms: now_ms.saturating_add(AUTH_WATCH_POLL_INTERVAL_MS),
            is_polling: false,
        };
        self.watches.insert(key, watch.clone());
        Ok((watch, previous))
    }

    pub fn poll_tick(
        &mut self,
        now_ms: u64,
        server_id: &str,
        account_key: &str,
    ) -> McpAuthPollTick {
        let Ok(server_id) = validate_mcp_server_id(server_id) else {
            return McpAuthPollTick::Idle;
        };
        if parse_i32_mcp_server_id(&server_id).is_err() {
            return McpAuthPollTick::Idle;
        }
        let Ok(account_key) = normalize_mcp_account_key(account_key) else {
            return McpAuthPollTick::Idle;
        };
        let key = auth_watch_key(&server_id, &account_key);
        let Some(watch) = self.watches.get_mut(&key) else {
            return McpAuthPollTick::Idle;
        };
        if now_ms >= watch.expires_at_ms {
            let watch = self.watches.remove(&key).expect("watch exists");
            return McpAuthPollTick::Expired(completion_from_watch(watch, "timeout"));
        }
        if watch.is_polling || now_ms < watch.next_poll_at_ms {
            return McpAuthPollTick::Idle;
        }
        if watch.suppress_first_poll {
            watch.suppress_first_poll = false;
            watch.next_poll_at_ms = now_ms.saturating_add(AUTH_WATCH_POLL_INTERVAL_MS);
            return McpAuthPollTick::Suppressed;
        }
        watch.is_polling = true;
        McpAuthPollTick::Request(McpAuthPollRequest {
            generation: watch.generation,
            server_id: watch.server_id.clone(),
            server_name: watch.server_name.clone(),
            server_url: watch.server_url.clone(),
            account_key: watch.account_key.clone(),
            requesting_agent_id: watch.requesting_agent_id.clone(),
            deadline_ms: now_ms.saturating_add(AUTH_WATCH_POLL_TIMEOUT_MS),
        })
    }

    pub fn settle_poll(
        &mut self,
        now_ms: u64,
        request: &McpAuthPollRequest,
        outcome: McpAuthPollOutcome,
    ) -> McpAuthPollSettlement {
        let key = auth_watch_key(&request.server_id, &request.account_key);
        let Some(current) = self.watches.get(&key) else {
            return McpAuthPollSettlement::Stale;
        };
        if current.generation != request.generation
            || current.server_url != request.server_url
            || current.server_name != request.server_name
        {
            return McpAuthPollSettlement::Stale;
        }
        if now_ms >= current.expires_at_ms {
            self.watches.remove(&key);
            return McpAuthPollSettlement::Stale;
        }
        match outcome {
            McpAuthPollOutcome::TokenValid => {
                let watch = self.watches.remove(&key).expect("watch exists");
                McpAuthPollSettlement::Completed(completion_from_watch(watch, "completed"))
            }
            McpAuthPollOutcome::AdminBlocked => {
                let watch = self.watches.remove(&key).expect("watch exists");
                McpAuthPollSettlement::Cancelled(completion_from_watch(watch, "cancelled"))
            }
            McpAuthPollOutcome::TokenInvalid | McpAuthPollOutcome::Unreachable => {
                let watch = self.watches.get_mut(&key).expect("watch exists");
                watch.is_polling = false;
                watch.next_poll_at_ms = now_ms.saturating_add(AUTH_WATCH_POLL_INTERVAL_MS);
                McpAuthPollSettlement::Pending
            }
        }
    }

    pub fn poll_failed(
        &mut self,
        now_ms: u64,
        request: &McpAuthPollRequest,
    ) -> McpAuthPollSettlement {
        self.settle_poll(now_ms, request, McpAuthPollOutcome::Unreachable)
    }

    pub fn note_auth_completed_elsewhere(
        &mut self,
        server_id: &str,
        account_key: &str,
    ) -> Option<PendingMcpAuthWatch> {
        let server_id = validate_mcp_server_id(server_id).ok()?;
        parse_i32_mcp_server_id(&server_id).ok()?;
        let account_key = normalize_mcp_account_key(account_key).ok()?;
        self.watches.remove(&auth_watch_key(&server_id, &account_key))
    }

    pub fn cancel_watch(
        &mut self,
        server_id: &str,
        account_key: &str,
    ) -> Option<McpAuthWatchCompletion> {
        self.note_auth_completed_elsewhere(server_id, account_key)
            .map(|watch| completion_from_watch(watch, "cancelled"))
    }

    pub fn cancel_server(&mut self, server_id: &str) -> Vec<McpAuthWatchCompletion> {
        let Ok(server_id) = validate_mcp_server_id(server_id) else {
            return Vec::new();
        };
        if parse_i32_mcp_server_id(&server_id).is_err() {
            return Vec::new();
        }
        let keys: Vec<String> = self
            .watches
            .iter()
            .filter_map(|(key, watch)| (watch.server_id == server_id).then_some(key.clone()))
            .collect();
        keys.into_iter()
            .filter_map(|key| self.watches.remove(&key))
            .map(|watch| completion_from_watch(watch, "cancelled"))
            .collect()
    }

    pub fn cancel_all(&mut self) -> Vec<McpAuthWatchCompletion> {
        let watches = std::mem::take(&mut self.watches);
        watches
            .into_values()
            .map(|watch| completion_from_watch(watch, "cancelled"))
            .collect()
    }

    pub fn prune_expired(&mut self, now_ms: u64) -> usize {
        let before = self.watches.len();
        self.watches.retain(|_, watch| watch.expires_at_ms > now_ms);
        before.saturating_sub(self.watches.len())
    }

    pub fn len(&self) -> usize {
        self.watches.len()
    }

    pub fn is_empty(&self) -> bool {
        self.watches.is_empty()
    }
}

fn completion_from_watch(
    watch: PendingMcpAuthWatch,
    outcome: &'static str,
) -> McpAuthWatchCompletion {
    McpAuthWatchCompletion {
        generation: watch.generation,
        server_id: watch.server_id,
        server_name: watch.server_name,
        account_key: watch.account_key,
        requesting_agent_id: watch.requesting_agent_id,
        outcome,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn http_server(disabled: bool) -> McpAuthServerSnapshot {
        McpAuthServerSnapshot {
            server_id: "17".into(),
            server_name: "Calendar".into(),
            server_url: Some("https://mcp.example.test/api".into()),
            transport: McpAuthTransport::Http,
            disabled_by_team_admin_policy: disabled,
        }
    }

    #[test]
    fn fresh_admin_policy_recheck_is_required_before_auth() {
        let initial = http_server(true);
        assert_eq!(
            gate_auth_server(Some(&initial), None),
            McpAuthServerGate::AdminBlocked
        );
        assert_eq!(
            gate_auth_server(Some(&initial), Some(&http_server(true))),
            McpAuthServerGate::AdminBlocked
        );
        assert!(matches!(
            gate_auth_server(Some(&initial), Some(&http_server(false))),
            McpAuthServerGate::Eligible(_)
        ));
    }

    #[test]
    fn status_classification_preserves_already_auth_force_reauth_and_url_fence() {
        let valid = McpBackendAuthStatus {
            is_available: true,
            requires_auth: false,
            has_valid_token: true,
            auth_url: String::new(),
            error: String::new(),
        };
        assert_eq!(
            classify_backend_auth_status(Some("https://mcp.example.test"), &valid, false),
            McpAuthStatusDecision::AlreadyAuthenticated
        );

        let needs_auth = McpBackendAuthStatus {
            is_available: false,
            requires_auth: true,
            has_valid_token: true,
            auth_url: "https://auth.example.test/authorize".into(),
            error: String::new(),
        };
        assert!(matches!(
            classify_backend_auth_status(Some("https://mcp.example.test"), &needs_auth, true),
            McpAuthStatusDecision::Start { .. }
        ));

        let invalid = McpBackendAuthStatus {
            auth_url: "javascript:alert(1)".into(),
            ..needs_auth
        };
        assert!(matches!(
            classify_backend_auth_status(Some("https://mcp.example.test"), &invalid, true),
            McpAuthStatusDecision::NotSupported { .. }
        ));
    }

    #[test]
    fn loopback_http_auth_is_allowed_only_for_loopback_server() {
        assert!(validate_authorization_url(
            "http://127.0.0.1:43121/callback",
            Some("http://localhost:8080/mcp")
        )
        .is_some());
        assert!(validate_authorization_url(
            "http://127.0.0.1:43121/callback",
            Some("https://remote.example/mcp")
        )
        .is_none());
    }

    #[test]
    fn replacement_preserves_requester_and_fences_stale_poll() {
        let mut lifecycle = McpAuthWatchLifecycle::new();
        let (first, _) = lifecycle
            .begin_watch(
                0,
                "17",
                "Calendar",
                "https://mcp.example.test",
                "Default",
                Some("agent-a"),
                false,
            )
            .unwrap();
        let request = match lifecycle.poll_tick(AUTH_WATCH_POLL_INTERVAL_MS, "17", "default") {
            McpAuthPollTick::Request(request) => request,
            other => panic!("expected poll request, got {other:?}"),
        };
        assert_eq!(request.generation, first.generation);

        let (second, replaced) = lifecycle
            .begin_watch(
                AUTH_WATCH_POLL_INTERVAL_MS + 1,
                "17",
                "Calendar",
                "https://mcp.example.test",
                "default",
                None,
                false,
            )
            .unwrap();
        assert_eq!(replaced.unwrap().generation, first.generation);
        assert_eq!(second.requesting_agent_id.as_deref(), Some("agent-a"));
        assert_eq!(
            lifecycle.settle_poll(
                AUTH_WATCH_POLL_INTERVAL_MS + 2,
                &request,
                McpAuthPollOutcome::TokenValid,
            ),
            McpAuthPollSettlement::Stale
        );
        assert_eq!(lifecycle.watch("17", "default").unwrap().generation, second.generation);
    }

    #[test]
    fn force_reauth_suppresses_first_poll_then_uses_30_second_deadline() {
        let mut lifecycle = McpAuthWatchLifecycle::new();
        lifecycle
            .begin_watch(
                100,
                "17",
                "Calendar",
                "https://mcp.example.test",
                "default",
                None,
                true,
            )
            .unwrap();
        assert_eq!(
            lifecycle.poll_tick(100 + AUTH_WATCH_POLL_INTERVAL_MS, "17", "default"),
            McpAuthPollTick::Suppressed
        );
        let at = 100 + AUTH_WATCH_POLL_INTERVAL_MS * 2;
        let request = match lifecycle.poll_tick(at, "17", "default") {
            McpAuthPollTick::Request(request) => request,
            other => panic!("expected request, got {other:?}"),
        };
        assert_eq!(request.deadline_ms, at + AUTH_WATCH_POLL_TIMEOUT_MS);
    }

    #[test]
    fn success_timeout_cancel_and_account_switch_are_fenced() {
        let mut lifecycle = McpAuthWatchLifecycle::new();
        lifecycle
            .begin_watch(
                0,
                "17",
                "Calendar",
                "https://mcp.example.test",
                "account-a",
                Some("agent-a"),
                false,
            )
            .unwrap();
        assert!(lifecycle.note_auth_completed_elsewhere("17", "account-b").is_none());
        assert_eq!(lifecycle.len(), 1);

        let request = match lifecycle.poll_tick(AUTH_WATCH_POLL_INTERVAL_MS, "17", "account-a") {
            McpAuthPollTick::Request(request) => request,
            other => panic!("expected request, got {other:?}"),
        };
        assert!(matches!(
            lifecycle.settle_poll(
                AUTH_WATCH_POLL_INTERVAL_MS + 1,
                &request,
                McpAuthPollOutcome::TokenValid,
            ),
            McpAuthPollSettlement::Completed(_)
        ));
        assert!(lifecycle.is_empty());

        lifecycle
            .begin_watch(
                0,
                "18",
                "Drive",
                "https://mcp.example.test",
                "default",
                None,
                false,
            )
            .unwrap();
        let expired = lifecycle.poll_tick(AUTH_WATCH_TIMEOUT_MS, "18", "default");
        let McpAuthPollTick::Expired(completion) = expired else {
            panic!("expected timeout completion, got {expired:?}");
        };
        assert_eq!(completion.server_id, "18");
        assert_eq!(completion.account_key, "default");
        assert_eq!(completion.outcome, "timeout");
        assert!(lifecycle.is_empty());

        lifecycle
            .begin_watch(
                0,
                "19",
                "Docs",
                "https://mcp.example.test",
                "default",
                None,
                false,
            )
            .unwrap();
        let cancelled = lifecycle.cancel_watch("19", "default").unwrap();
        assert_eq!(cancelled.outcome, "cancelled");
        assert!(lifecycle.is_empty());
    }

    #[test]
    fn restored_watch_recovers_after_process_death_without_reusing_inflight_state() {
        let mut lifecycle = McpAuthWatchLifecycle::new();
        lifecycle
            .begin_watch(
                100,
                "17",
                "Calendar",
                "https://mcp.example.test",
                "default",
                Some("agent-a"),
                false,
            )
            .unwrap();
        let _ = lifecycle.poll_tick(100 + AUTH_WATCH_POLL_INTERVAL_MS, "17", "default");
        let restored = McpAuthWatchLifecycle::from_restored(
            200,
            lifecycle.next_generation(),
            lifecycle.watches().cloned().collect::<Vec<_>>(),
        );
        let watch = restored.watch("17", "default").unwrap();
        assert!(!watch.is_polling);
        assert_eq!(watch.requesting_agent_id.as_deref(), Some("agent-a"));
    }
}
