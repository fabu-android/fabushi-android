use super::xuser_sharing_environment::resolve_xuser_sharing_environment;
use super::xuser_sharing_service::{SandSharingState, SandXuserSharingService};
use std::collections::BTreeMap;
use std::sync::Arc;

pub const SHARING_DISABLED_MESSAGE: &str = "Sharing isn't enabled for your account.";

/// Android-adapted owner for the Desktop CrossUserSharing extension contract.
///
/// Desktop subscribes directly to HostExperiments + NotifyBus. Android keeps those platform
/// concerns outside this state owner: the process-owned Host/Coordinator explicitly applies the
/// current feature policy and forwards reconnect/notify drain signals. The service remains the
/// single sharing-state/relay owner.
pub struct CrossUserSharingExtension {
    service: Arc<SandXuserSharingService>,
    allowed: bool,
    reason: Option<String>,
}

impl CrossUserSharingExtension {
    pub fn new(
        service: Arc<SandXuserSharingService>,
        backend_url: &str,
        env: &BTreeMap<String, String>,
    ) -> Self {
        let gate = resolve_xuser_sharing_environment(backend_url, env);
        if !gate.is_allowed {
            service.set_enabled(false);
        }
        Self {
            service,
            allowed: gate.is_allowed,
            reason: gate.reason,
        }
    }

    pub fn apply_feature_policy(&self, enabled: bool) -> Result<(), String> {
        let enabled = enabled && self.allowed;
        self.service.set_enabled(enabled);
        if enabled {
            self.service.start().map(|_| ())
        } else {
            Ok(())
        }
    }

    pub fn request_relay_drain(&self) {
        if self.allowed {
            self.service.request_relay_drain();
        }
    }

    pub fn stop(&self) {
        self.service.stop();
    }

    pub fn is_enabled(&self) -> bool {
        self.service.get_state().is_enabled
    }

    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    pub fn get_sharing_state(&self) -> SandSharingState {
        self.service.get_state()
    }

    pub fn prepare_for_upgrade(&self) {
        self.service.prepare_for_upgrade();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::cross_user_sharing::xuser_relay::{
        XuserRelayClient, XuserRelayTransport, XuserRelayHttpError,
    };
    use serde_json::{json, Value};

    struct NoopTransport;
    impl XuserRelayTransport for NoopTransport {
        fn request(
            &self,
            _method: &str,
            _url: &str,
            _token: &str,
            _body: Option<&Value>,
        ) -> Result<Value, XuserRelayHttpError> {
            Ok(json!({"rooms":[],"pendingJoinRequests":[]}))
        }
    }

    fn service() -> Arc<SandXuserSharingService> {
        let relay = Arc::new(XuserRelayClient::new(
            "https://api.example.test".into(),
            Arc::new(|| Ok("test-token".into())),
            Arc::new(NoopTransport),
        ));
        Arc::new(SandXuserSharingService::new(relay))
    }

    #[test]
    fn platform_policy_cannot_override_environment_denial() {
        let service = service();
        let extension = CrossUserSharingExtension::new(
            Arc::clone(&service),
            "https://api2.cursor.sh",
            &BTreeMap::new(),
        );
        extension.apply_feature_policy(true).unwrap();
        assert!(!extension.is_enabled());
        assert!(extension.reason().is_some());
    }

    #[test]
    fn allowed_policy_starts_and_stop_disables_background_owner() {
        let service = service();
        let env = BTreeMap::from([
            ("SAND_DEV_XUSER_SHARING".into(), "1".into()),
            ("SAND_PACKAGED".into(), "0".into()),
        ]);
        let extension = CrossUserSharingExtension::new(
            Arc::clone(&service),
            "https://dev.example.test",
            &env,
        );
        extension.apply_feature_policy(true).unwrap();
        assert!(extension.is_enabled());
        assert!(service.is_started());
        extension.stop();
        assert!(!service.is_started());
    }
}
