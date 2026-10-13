use std::collections::BTreeMap;
use std::sync::Arc;

use crate::mcp_auth::dashboard_mcp_auth_backend::SandMcpCredentialProvider;

use super::extension::CrossUserSharingExtension;
use super::xuser_relay::{
    SandXuserRelayClient, UreqXuserRelayTransport, XuserRelayDriver, XuserRelayRuntime,
    XuserRelayTransport,
};
use super::xuser_sharing_service::SandXuserSharingService;

/// Build the Android shipping relay without creating a second account/token owner.
///
/// The credential provider is the same dedicated Sand renewal boundary used by the
/// Cursor/Sand backend integration. Fabushi account bearers never enter this path.
pub fn build_xuser_relay_client(
    credentials: Arc<dyn SandMcpCredentialProvider>,
    transport: Arc<dyn XuserRelayTransport>,
) -> Result<Arc<SandXuserRelayClient>, String> {
    let configuration = credentials.configuration();
    let backend_url = configuration.backend_url;
    let token_owner = Arc::clone(&credentials);
    Ok(Arc::new(SandXuserRelayClient::new(
        backend_url,
        Arc::new(move || {
            token_owner
                .credentials()
                .map(|current| current.access_token)
        }),
        transport,
    )))
}

pub fn build_production_xuser_relay_client(
    credentials: Arc<dyn SandMcpCredentialProvider>,
) -> Result<Arc<SandXuserRelayClient>, String> {
    let configuration = credentials.configuration();
    let transport: Arc<dyn XuserRelayTransport> = Arc::new(UreqXuserRelayTransport::new(
        "sand",
        configuration.client_version,
        configuration.box_namespace,
    )?);
    build_xuser_relay_client(credentials, transport)
}


/// Process-owned core composition for Android cross-user sharing.
///
/// This intentionally stops short of Presentation or canonical room installation: those are bound
/// by AndroidJsonHost. Its job is to guarantee that environment policy, sharing state and relay
/// transport share one credential/relay owner instead of being reconstructed by callers.
pub struct ProductionCrossUserCore {
    pub relay: Arc<SandXuserRelayClient>,
    pub service: Arc<SandXuserSharingService>,
    pub relay_driver: Arc<XuserRelayDriver>,
    pub extension: Arc<CrossUserSharingExtension>,
}


impl ProductionCrossUserCore {
    pub fn sharing_state(&self) -> super::xuser_sharing_service::SandSharingState {
        self.extension.get_sharing_state()
    }

    pub fn apply_feature_policy(&self, enabled: bool) -> Result<(), String> {
        self.extension.apply_feature_policy(enabled)
    }

    pub fn request_relay_drain(&self) {
        self.extension.request_relay_drain();
    }

    pub fn prepare_for_upgrade(&self) {
        self.extension.prepare_for_upgrade();
    }
}

pub fn build_xuser_core(
    credentials: Arc<dyn SandMcpCredentialProvider>,
    transport: Arc<dyn XuserRelayTransport>,
    env: &BTreeMap<String, String>,
) -> Result<ProductionCrossUserCore, String> {
    let backend_url = credentials.configuration().backend_url;
    let relay = build_xuser_relay_client(credentials, transport)?;
    let service = Arc::new(SandXuserSharingService::new(Arc::clone(&relay)));
    let weak_service = Arc::downgrade(&service);
    let relay_runtime = Arc::new(XuserRelayRuntime::new(
        Arc::clone(&relay),
        Arc::new(move |event| {
            weak_service
                .upgrade()
                .map_or(Ok(true), |service| service.handle_event(event))
        }),
        Arc::new(system_now_ms),
        // Android has no Desktop NotifyBus owner. While the app Host process is alive, the
        // canonical relay driver uses its bounded safety-poll path; explicit push/foreground
        // hints can still call request_relay_drain without creating another poller.
        Arc::new(|| false),
        Arc::new(|| true),
    ));
    let relay_driver = Arc::new(XuserRelayDriver::new(relay_runtime));
    service.bind_relay_driver(Arc::clone(&relay_driver));
    let extension = Arc::new(CrossUserSharingExtension::new(
        Arc::clone(&service),
        &backend_url,
        env,
    ));
    Ok(ProductionCrossUserCore {
        relay,
        service,
        relay_driver,
        extension,
    })
}

pub fn build_production_xuser_core(
    credentials: Arc<dyn SandMcpCredentialProvider>,
    env: &BTreeMap<String, String>,
) -> Result<ProductionCrossUserCore, String> {
    let configuration = credentials.configuration();
    let transport: Arc<dyn XuserRelayTransport> = Arc::new(UreqXuserRelayTransport::new(
        "sand",
        configuration.client_version,
        configuration.box_namespace,
    )?);
    build_xuser_core(credentials, transport, env)
}

fn system_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_auth::dashboard_mcp_auth_backend::SandMcpBackendCredentials;
    use crate::extensions::cross_user_sharing::xuser_relay::XuserRelayHttpError;
    use serde_json::{json, Value};
    use std::sync::Mutex;

    struct RotatingSandCredentials {
        calls: Mutex<u64>,
    }

    impl SandMcpCredentialProvider for RotatingSandCredentials {
        fn configuration(&self) -> crate::mcp_auth::dashboard_mcp_auth_backend::SandMcpBackendConfiguration {
            crate::mcp_auth::dashboard_mcp_auth_backend::SandMcpBackendConfiguration {
                backend_url: "https://sand.example".into(),
                client_version: "1.0.0".into(),
                box_namespace: "prod".into(),
            }
        }

        fn credentials(&self) -> Result<SandMcpBackendCredentials, String> {
            let mut calls = self.calls.lock().unwrap();
            *calls += 1;
            Ok(SandMcpBackendCredentials {
                backend_url: "https://sand.example".into(),
                access_token: format!("sand-token-{}", *calls),
                machine_id: "machine-1".into(),
                client_version: "1.0.0".into(),
                box_namespace: "prod".into(),
            })
        }
    }

    struct CapturingTransport {
        tokens: Mutex<Vec<String>>,
    }

    impl XuserRelayTransport for CapturingTransport {
        fn request(
            &self,
            _method: &str,
            _url: &str,
            token: &str,
            _body: Option<&Value>,
        ) -> Result<Value, XuserRelayHttpError> {
            self.tokens.lock().unwrap().push(token.to_string());
            Ok(json!({}))
        }
    }

    #[test]
    fn core_composition_uses_one_relay_and_fails_closed_when_environment_denies_sharing() {
        let credentials: Arc<dyn SandMcpCredentialProvider> = Arc::new(
            RotatingSandCredentials {
                calls: Mutex::new(0),
            },
        );
        let transport: Arc<dyn XuserRelayTransport> = Arc::new(CapturingTransport {
            tokens: Mutex::new(Vec::new()),
        });
        let core = build_xuser_core(
            credentials,
            transport,
            &BTreeMap::new(),
        )
        .unwrap();

        assert!(!core.extension.is_enabled());
        core.extension.apply_feature_policy(true).unwrap();
        assert!(!core.extension.is_enabled());
        assert!(
            !core.service.is_started(),
            "environment denial must keep the process-owned sharing service stopped"
        );
        assert!(!core.relay_driver.is_started());
    }

    #[test]
    fn allowed_core_binds_one_relay_driver_and_stops_cleanly() {
        let credentials: Arc<dyn SandMcpCredentialProvider> = Arc::new(
            RotatingSandCredentials {
                calls: Mutex::new(0),
            },
        );
        let transport: Arc<dyn XuserRelayTransport> = Arc::new(CapturingTransport {
            tokens: Mutex::new(Vec::new()),
        });
        let env = BTreeMap::from([
            ("SAND_DEV_XUSER_SHARING".to_string(), "1".to_string()),
            ("SAND_PACKAGED".to_string(), "0".to_string()),
        ]);
        let core = build_xuser_core(credentials, transport, &env).unwrap();
        core.extension.apply_feature_policy(true).unwrap();
        assert!(core.service.is_started());
        assert!(core.relay_driver.is_started());
        core.extension.stop();
        assert!(!core.service.is_started());
        assert!(!core.relay_driver.is_started());
    }

    #[test]
    fn relay_fetches_a_fresh_dedicated_sand_token_for_each_request() {
        let credentials: Arc<dyn SandMcpCredentialProvider> = Arc::new(
            RotatingSandCredentials {
                calls: Mutex::new(0),
            },
        );
        let transport = Arc::new(CapturingTransport {
            tokens: Mutex::new(Vec::new()),
        });
        let relay = build_xuser_relay_client(
            Arc::clone(&credentials),
            transport.clone(),
        )
        .unwrap();

        relay.fetch_share_state().unwrap();
        relay.fetch_share_state().unwrap();

        assert_eq!(
            transport.tokens.lock().unwrap().as_slice(),
            ["sand-token-1", "sand-token-2"]
        );
    }
}
