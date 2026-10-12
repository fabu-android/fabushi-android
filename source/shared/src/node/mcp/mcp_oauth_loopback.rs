use std::collections::BTreeMap;
use url::Url;

pub const MCP_OAUTH_LOOPBACK_CALLBACK_URL: &str =
    fabushi_constants::mcp::MCP_OAUTH_LOOPBACK_CALLBACK_URL;
pub const BACKEND_MCP_OAUTH_PENDING_STATE_TTL_MS: u64 = 15 * 60 * 1_000;
pub const MCP_OAUTH_PENDING_TTL_MS: u64 = BACKEND_MCP_OAUTH_PENDING_STATE_TTL_MS + 60_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedMcpOAuthAuthorization {
    pub state: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum McpOAuthCallbackFailureReason {
    ProviderError,
    MissingCode,
    CompletionRejected,
    CompletionTimeout,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpOAuthPendingCallback {
    pub state: String,
    pub code: String,
    pub server_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingAuth {
    server_name: Option<String>,
    expires_at_ms: u64,
    completing: bool,
}

#[derive(Clone, Debug)]
pub struct McpOAuthLoopbackState {
    pending: BTreeMap<String, PendingAuth>,
    ttl_ms: u64,
    disposed: bool,
}

fn is_loopback(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1" | "[::1]")
}

pub fn parse_mcp_oauth_loopback_authorization(
    authorization_url: &str,
    callback_url: &str,
) -> Option<ParsedMcpOAuthAuthorization> {
    let authorization = Url::parse(authorization_url).ok()?;
    let callback = Url::parse(callback_url).ok()?;
    let redirect_uri = authorization
        .query_pairs()
        .find_map(|(key, value)| (key == "redirect_uri").then(|| value.into_owned()))?;
    let state = authorization
        .query_pairs()
        .find_map(|(key, value)| (key == "state").then(|| value.into_owned()))?;
    if state.is_empty() {
        return None;
    }

    let redirect = Url::parse(&redirect_uri).ok()?;
    let redirect_host = redirect.host_str()?;
    let callback_port = callback.port_or_known_default()?;
    let redirect_port = redirect.port_or_known_default()?;
    if redirect.scheme() != "http"
        || !is_loopback(redirect_host)
        || redirect_port != callback_port
        || redirect.path() != callback.path()
    {
        return None;
    }

    Some(ParsedMcpOAuthAuthorization { state })
}

impl McpOAuthLoopbackState {
    pub fn new(ttl_ms: u64) -> Self {
        Self {
            pending: BTreeMap::new(),
            ttl_ms: ttl_ms.max(1),
            disposed: false,
        }
    }

    pub fn register_pending_auth_from_url(
        &mut self,
        now_ms: u64,
        authorization_url: &str,
        callback_url: &str,
        server_name: Option<&str>,
    ) -> bool {
        if self.disposed {
            return false;
        }
        self.expire(now_ms);
        let parsed = match parse_mcp_oauth_loopback_authorization(
            authorization_url,
            callback_url,
        ) {
            Some(parsed) => parsed,
            None => return false,
        };

        let server_name = server_name
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let expires_at_ms = now_ms.saturating_add(self.ttl_ms);
        match self.pending.get_mut(&parsed.state) {
            Some(existing) => {
                existing.expires_at_ms = expires_at_ms;
                if existing.server_name.is_none() {
                    existing.server_name = server_name;
                }
            }
            None => {
                self.pending.insert(
                    parsed.state,
                    PendingAuth {
                        server_name,
                        expires_at_ms,
                        completing: false,
                    },
                );
            }
        }
        true
    }

    pub fn begin_callback(
        &mut self,
        now_ms: u64,
        method: &str,
        raw_callback_url: &str,
        callback_url: &str,
    ) -> Result<McpOAuthPendingCallback, McpOAuthCallbackFailureReason> {
        self.expire(now_ms);
        if self.disposed || method != "GET" {
            return Err(McpOAuthCallbackFailureReason::CompletionRejected);
        }

        let callback = Url::parse(callback_url)
            .map_err(|_| McpOAuthCallbackFailureReason::CompletionRejected)?;
        let request = Url::parse(raw_callback_url)
            .map_err(|_| McpOAuthCallbackFailureReason::CompletionRejected)?;
        if request.path() != callback.path() {
            return Err(McpOAuthCallbackFailureReason::CompletionRejected);
        }

        let state = request
            .query_pairs()
            .find_map(|(key, value)| (key == "state").then(|| value.into_owned()))
            .ok_or(McpOAuthCallbackFailureReason::CompletionRejected)?;
        if !self.pending.contains_key(&state) {
            return Err(McpOAuthCallbackFailureReason::CompletionRejected);
        }

        if request.query_pairs().any(|(key, _)| key == "error") {
            self.pending.remove(&state);
            return Err(McpOAuthCallbackFailureReason::ProviderError);
        }

        let code = match request
            .query_pairs()
            .find_map(|(key, value)| (key == "code").then(|| value.into_owned()))
            .filter(|value| !value.is_empty())
        {
            Some(code) => code,
            None => {
                self.pending.remove(&state);
                return Err(McpOAuthCallbackFailureReason::MissingCode);
            }
        };

        let auth = self
            .pending
            .get_mut(&state)
            .ok_or(McpOAuthCallbackFailureReason::CompletionRejected)?;
        if auth.completing {
            return Err(McpOAuthCallbackFailureReason::CompletionRejected);
        }
        auth.completing = true;
        let server_name = auth.server_name.clone();

        Ok(McpOAuthPendingCallback {
            state,
            code,
            server_name,
        })
    }

    pub fn complete_success(&mut self, state: &str) -> bool {
        self.pending.remove(state).is_some()
    }

    pub fn complete_failure(
        &mut self,
        state: &str,
        transient: bool,
    ) -> bool {
        let Some(auth) = self.pending.get_mut(state) else {
            return false;
        };
        if transient {
            auth.completing = false;
        } else {
            self.pending.remove(state);
        }
        true
    }

    pub fn expire(&mut self, now_ms: u64) {
        self.pending
            .retain(|_, auth| auth.expires_at_ms > now_ms);
    }

    pub fn dispose(&mut self) {
        self.disposed = true;
        self.pending.clear();
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub fn is_disposed(&self) -> bool {
        self.disposed
    }
}

impl Default for McpOAuthLoopbackState {
    fn default() -> Self {
        Self::new(MCP_OAUTH_PENDING_TTL_MS)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpOAuthPendingRegistration {
    pub provider: String,
    pub server_id: Option<String>,
    pub account_key: Option<String>,
    pub generation: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct McpOAuthPendingStateRegistry {
    pending: BTreeMap<String, (McpOAuthPendingRegistration, u64)>,
    ttl_ms: u64,
}

impl McpOAuthPendingStateRegistry {
    pub fn new(ttl_ms: u64) -> Self {
        Self {
            pending: BTreeMap::new(),
            ttl_ms: ttl_ms.max(1),
        }
    }

    pub fn register(
        &mut self,
        now_ms: u64,
        state: impl Into<String>,
        provider: impl Into<String>,
    ) -> Result<(), &'static str> {
        self.register_bound(now_ms, state, provider, None, None, None)
    }

    pub fn register_bound(
        &mut self,
        now_ms: u64,
        state: impl Into<String>,
        provider: impl Into<String>,
        server_id: Option<&str>,
        account_key: Option<&str>,
        generation: Option<u64>,
    ) -> Result<(), &'static str> {
        self.expire(now_ms);
        let state = state.into();
        if state.len() < 16 {
            return Err("OAuth state token is too short");
        }
        if !state.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '~' | '-')) {
            return Err("OAuth state token contains unsupported characters");
        }
        let provider = provider.into();
        if provider.trim().is_empty() {
            return Err("OAuth provider is required");
        }
        let has_identity = server_id.is_some() || account_key.is_some() || generation.is_some();
        if has_identity && (server_id.is_none() || account_key.is_none() || generation.is_none()) {
            return Err("OAuth watch identity must include server, account, and generation");
        }
        let server_id = server_id.map(str::trim).filter(|value| !value.is_empty()).map(str::to_string);
        let account_key = account_key.map(str::trim).filter(|value| !value.is_empty()).map(str::to_string);
        if has_identity && (server_id.is_none() || account_key.is_none() || generation == Some(0)) {
            return Err("OAuth watch identity is invalid");
        }
        let registration = McpOAuthPendingRegistration { provider, server_id, account_key, generation };
        if self.pending.insert(state, (registration, now_ms.saturating_add(self.ttl_ms))).is_some() {
            return Err("OAuth state already registered");
        }
        Ok(())
    }

    pub fn consume(&mut self, now_ms: u64, state: &str) -> Option<String> {
        self.consume_registration(now_ms, state).map(|value| value.provider)
    }

    pub fn consume_registration(
        &mut self,
        now_ms: u64,
        state: &str,
    ) -> Option<McpOAuthPendingRegistration> {
        self.expire(now_ms);
        self.pending.remove(state).map(|(registration, _)| registration)
    }

    pub fn expire(&mut self, now_ms: u64) {
        self.pending.retain(|_, (_, expires_at_ms)| *expires_at_ms > now_ms);
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

impl Default for McpOAuthPendingStateRegistry {
    fn default() -> Self {
        Self::new(MCP_OAUTH_PENDING_TTL_MS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn authorization(state: &str) -> String {
        format!(
            "https://provider.example/authorize?redirect_uri={}&state={state}",
            urlencoding::encode(MCP_OAUTH_LOOPBACK_CALLBACK_URL)
        )
    }

    #[test]
    fn authorization_requires_matching_loopback_redirect_and_nonempty_state() {
        let valid = format!(
            "https://provider.example/authorize?redirect_uri={}&state=0123456789abcdef",
            urlencoding::encode(MCP_OAUTH_LOOPBACK_CALLBACK_URL)
        );
        assert_eq!(
            parse_mcp_oauth_loopback_authorization(
                &valid,
                MCP_OAUTH_LOOPBACK_CALLBACK_URL
            )
            .unwrap()
            .state,
            "0123456789abcdef"
        );
        assert!(parse_mcp_oauth_loopback_authorization(
            "https://provider.example/authorize?redirect_uri=https%3A%2F%2Fevil.example%2Fcallback&state=x",
            MCP_OAUTH_LOOPBACK_CALLBACK_URL,
        )
        .is_none());
    }

    #[test]
    fn register_refreshes_ttl_and_callback_consumes_success_once() {
        let mut state = McpOAuthLoopbackState::new(100);
        assert!(state.register_pending_auth_from_url(
            10,
            &authorization("0123456789abcdef"),
            MCP_OAUTH_LOOPBACK_CALLBACK_URL,
            Some("GitHub"),
        ));
        assert_eq!(state.pending_count(), 1);
        assert!(state.register_pending_auth_from_url(
            50,
            &authorization("0123456789abcdef"),
            MCP_OAUTH_LOOPBACK_CALLBACK_URL,
            None,
        ));

        let callback = state
            .begin_callback(
                120,
                "GET",
                "http://localhost:8787/callback?state=0123456789abcdef&code=code-1",
                MCP_OAUTH_LOOPBACK_CALLBACK_URL,
            )
            .unwrap();
        assert_eq!(callback.server_name.as_deref(), Some("GitHub"));
        assert_eq!(callback.code, "code-1");
        assert!(state.complete_success(&callback.state));
        assert_eq!(state.pending_count(), 0);
        assert!(state
            .begin_callback(
                121,
                "GET",
                "http://localhost:8787/callback?state=0123456789abcdef&code=code-1",
                MCP_OAUTH_LOOPBACK_CALLBACK_URL,
            )
            .is_err());
    }

    #[test]
    fn provider_error_and_missing_code_are_terminal() {
        let mut state = McpOAuthLoopbackState::default();
        state.register_pending_auth_from_url(
            0,
            &authorization("provider-error-state"),
            MCP_OAUTH_LOOPBACK_CALLBACK_URL,
            None,
        );
        assert_eq!(
            state.begin_callback(
                1,
                "GET",
                "http://localhost:8787/callback?state=provider-error-state&error=access_denied",
                MCP_OAUTH_LOOPBACK_CALLBACK_URL,
            ),
            Err(McpOAuthCallbackFailureReason::ProviderError)
        );

        state.register_pending_auth_from_url(
            2,
            &authorization("missing-code-state"),
            MCP_OAUTH_LOOPBACK_CALLBACK_URL,
            None,
        );
        assert_eq!(
            state.begin_callback(
                3,
                "GET",
                "http://localhost:8787/callback?state=missing-code-state",
                MCP_OAUTH_LOOPBACK_CALLBACK_URL,
            ),
            Err(McpOAuthCallbackFailureReason::MissingCode)
        );
        assert_eq!(state.pending_count(), 0);
    }

    #[test]
    fn transient_completion_failure_keeps_pending_for_retry() {
        let mut state = McpOAuthLoopbackState::default();
        state.register_pending_auth_from_url(
            0,
            &authorization("transient-state-01"),
            MCP_OAUTH_LOOPBACK_CALLBACK_URL,
            Some("Drive"),
        );
        let callback = state
            .begin_callback(
                1,
                "GET",
                "http://localhost:8787/callback?state=transient-state-01&code=first",
                MCP_OAUTH_LOOPBACK_CALLBACK_URL,
            )
            .unwrap();
        assert!(state.complete_failure(&callback.state, true));
        let retried = state
            .begin_callback(
                2,
                "GET",
                "http://localhost:8787/callback?state=transient-state-01&code=second",
                MCP_OAUTH_LOOPBACK_CALLBACK_URL,
            )
            .unwrap();
        assert_eq!(retried.code, "second");
    }

    #[test]
    fn pending_state_registry_is_single_use_and_expires() {
        let mut registry = McpOAuthPendingStateRegistry::new(100);
        registry
            .register(10, "0123456789abcdef", "drive")
            .unwrap();
        assert_eq!(registry.pending_count(), 1);
        assert_eq!(registry.consume(20, "0123456789abcdef").as_deref(), Some("drive"));
        assert_eq!(registry.consume(21, "0123456789abcdef"), None);

        registry
            .register(30, "fedcba9876543210", "calendar")
            .unwrap();
        assert_eq!(registry.consume(130, "fedcba9876543210"), None);
        assert_eq!(registry.pending_count(), 0);
    }

    #[test]
    fn expiry_and_dispose_drop_pending_state() {
        let mut state = McpOAuthLoopbackState::new(10);
        state.register_pending_auth_from_url(
            0,
            &authorization("expired-state-0001"),
            MCP_OAUTH_LOOPBACK_CALLBACK_URL,
            None,
        );
        state.expire(10);
        assert_eq!(state.pending_count(), 0);
        state.dispose();
        assert!(state.is_disposed());
        assert!(!state.register_pending_auth_from_url(
            11,
            &authorization("after-dispose-001"),
            MCP_OAUTH_LOOPBACK_CALLBACK_URL,
            None,
        ));
    }
}
