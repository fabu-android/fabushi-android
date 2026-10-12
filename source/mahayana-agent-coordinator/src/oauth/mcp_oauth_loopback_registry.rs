use fabushi_android_shared::node::mcp::mcp_oauth_loopback::{
    McpOAuthPendingRegistration, McpOAuthPendingStateRegistry,
};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct OAuthLoopbackRegistry {
    pending: McpOAuthPendingStateRegistry,
}

impl OAuthLoopbackRegistry {
    pub fn register(
        &mut self,
        state: impl Into<String>,
        provider: impl Into<String>,
    ) -> Result<(), &'static str> {
        self.pending.register(now_ms(), state, provider)
    }

    pub fn register_bound(
        &mut self,
        state: impl Into<String>,
        provider: impl Into<String>,
        server_id: Option<&str>,
        account_key: Option<&str>,
        generation: Option<u64>,
    ) -> Result<(), &'static str> {
        self.pending.register_bound(
            now_ms(),
            state,
            provider,
            server_id,
            account_key,
            generation,
        )
    }

    pub fn consume(&mut self, state: &str) -> Option<McpOAuthPendingRegistration> {
        self.pending.consume_registration(now_ms(), state)
    }

    pub fn pending_count(&self) -> usize {
        self.pending.pending_count()
    }
}

impl Default for OAuthLoopbackRegistry {
    fn default() -> Self {
        Self {
            pending: McpOAuthPendingStateRegistry::default(),
        }
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
