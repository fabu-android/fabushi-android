use fabushi_android_shared::node::mcp::mcp_oauth_loopback::McpOAuthPendingRegistration;

use super::mcp_oauth_callback_listener::OAuthCallback;
use super::mcp_oauth_loopback_registry::OAuthLoopbackRegistry;

pub struct OAuthForwarder {
    registry: OAuthLoopbackRegistry,
}

impl OAuthForwarder {
    pub fn new(registry: OAuthLoopbackRegistry) -> Self { Self { registry } }

    pub fn register(
        &mut self,
        state: impl Into<String>,
        provider: impl Into<String>,
    ) -> Result<(), &'static str> {
        self.registry.register(state, provider)
    }

    pub fn register_bound(
        &mut self,
        state: impl Into<String>,
        provider: impl Into<String>,
        server_id: Option<&str>,
        account_key: Option<&str>,
        generation: Option<u64>,
    ) -> Result<(), &'static str> {
        self.registry.register_bound(state, provider, server_id, account_key, generation)
    }

    pub fn pending_count(&self) -> usize { self.registry.pending_count() }

    pub fn forward(
        &mut self,
        callback: OAuthCallback,
    ) -> Result<(McpOAuthPendingRegistration, OAuthCallback), &'static str> {
        callback.validate()?;
        let registration = self
            .registry
            .consume(&callback.state)
            .ok_or("OAuth callback state is unknown or already consumed")?;
        Ok((registration, callback))
    }
}

impl Default for OAuthForwarder {
    fn default() -> Self { Self::new(OAuthLoopbackRegistry::default()) }
}
