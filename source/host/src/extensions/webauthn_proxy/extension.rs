use super::{
    webauthn_proxy_bridge::{WebAuthnBridge, WebAuthnBridgeError, WebAuthnBridgeRequest, WebAuthnBridgeSettlement},
    webauthn_proxy_marker::{apply_webauthn_proxy_marker, WebAuthnProxyMarkerOutcome},
};
use fabushi_android_shared::webauthn_gateway::{WebAuthnCeremony, WebAuthnResponseFrame};
use std::{io, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WebAuthnProxyExtensionConfig {
    pub ceremony_timeout_ms: u64,
    pub liveness_window_ms: u64,
}

impl Default for WebAuthnProxyExtensionConfig {
    fn default() -> Self {
        Self {
            ceremony_timeout_ms: fabushi_android_shared::webauthn_gateway::SAND_WEBAUTHN_CEREMONY_TIMEOUT_MS,
            liveness_window_ms: fabushi_android_shared::webauthn_gateway::SAND_WEBAUTHN_LIVENESS_WINDOW_MS,
        }
    }
}

pub struct WebAuthnProxyExtension {
    bridge: WebAuthnBridge,
}

impl WebAuthnProxyExtension {
    pub fn new(config: WebAuthnProxyExtensionConfig) -> Self {
        Self {
            bridge: WebAuthnBridge::new(config.ceremony_timeout_ms, config.liveness_window_ms),
        }
    }

    pub fn register_provider(
        &mut self,
        now_ms: u64,
    ) -> (String, fabushi_android_shared::webauthn_gateway::WebAuthnRequestFrame) {
        self.bridge.register_provider(now_ms)
    }

    pub fn request_ceremony(
        &mut self,
        now_ms: u64,
        ceremony: WebAuthnCeremony,
    ) -> Result<WebAuthnBridgeRequest, WebAuthnBridgeError> {
        self.bridge.request_ceremony(now_ms, ceremony)
    }

    pub fn submit_responses(
        &mut self,
        now_ms: u64,
        provider_id: Option<&str>,
        frames: &[WebAuthnResponseFrame],
    ) -> Vec<(String, WebAuthnBridgeSettlement)> {
        self.bridge.submit_responses(now_ms, provider_id, frames)
    }

    pub fn expire(
        &mut self,
        now_ms: u64,
    ) -> Vec<(String, String, fabushi_android_shared::webauthn_gateway::WebAuthnRequestFrame)> {
        self.bridge.expire(now_ms)
    }

    pub fn apply_enablement(
        enabled: bool,
        marker_path: &Path,
        policy_command: &Path,
    ) -> io::Result<WebAuthnProxyMarkerOutcome> {
        apply_webauthn_proxy_marker(enabled, marker_path, policy_command)
    }

    pub fn bridge_mut(&mut self) -> &mut WebAuthnBridge {
        &mut self.bridge
    }
}

impl Default for WebAuthnProxyExtension {
    fn default() -> Self {
        Self::new(WebAuthnProxyExtensionConfig::default())
    }
}
