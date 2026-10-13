use base64::{
    engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD},
    Engine as _,
};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::mcp_auth::dashboard_mcp_auth_backend::SandMcpCredentialProvider;

use super::android_sharing_manager::AndroidXuserSharingManager;
use super::extension::CrossUserSharingExtension;
use super::xuser_departure_obligations::{SandXuserDepartureObligations, XuserDepartureRelay};
use super::xuser_pending_departure_store::SandXuserPendingDepartureStore;
use super::xuser_room_tombstone_store::SandXuserRoomTombstoneStore;
use super::xuser_relay::{
    SandXuserRelayClient, UreqXuserRelayTransport, XuserRelayDriver, XuserRelayRuntime,
    XuserRelayTransport,
};
use super::xuser_sharing_service::{SandXuserSharingService, XuserSharingManager};

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


pub fn android_xuser_environment(
    packaged: bool,
    process_env: impl IntoIterator<Item = (String, String)>,
) -> BTreeMap<String, String> {
    let mut env = process_env.into_iter().collect::<BTreeMap<_, _>>();
    if packaged {
        // Desktop derives this from the packaged Host process environment. Android APK processes
        // do not inherit that desktop variable, so the production composition must state the
        // packaging fact explicitly. Test/dev composition never gets this implicit elevation.
        env.insert("SAND_PACKAGED".into(), "1".into());
        env.remove("SAND_HOST_DEV_ERROR_DETAIL");
    }
    env
}

/// Process-owned core composition for Android cross-user sharing.
///
/// This intentionally stops short of Presentation or canonical room installation: those are bound
/// by AndroidJsonHost. Its job is to guarantee that environment policy, sharing state and relay
/// transport share one credential/relay owner instead of being reconstructed by callers.
pub struct ProductionCrossUserCore {
    credentials: Arc<dyn SandMcpCredentialProvider>,
    pub relay: Arc<SandXuserRelayClient>,
    pub service: Arc<SandXuserSharingService>,
    pub relay_driver: Arc<XuserRelayDriver>,
    pub extension: Arc<CrossUserSharingExtension>,
    pub departures: SandXuserDepartureObligations,
    manager: Mutex<Option<Arc<AndroidXuserSharingManager>>>,
}


impl ProductionCrossUserCore {
    /// Refresh the canonical shared-room human identity from the same dedicated
    /// Sand credential owner used by the relay. Fabushi account bearers never
    /// enter this boundary.
    pub fn refresh_self_auth_id(&self) -> Result<Option<String>, String> {
        let token = self.credentials.credentials()?.access_token;
        let subject = jwt_subject_from_access_token(&token);
        self.service.set_self_auth_id(subject.clone());
        Ok(subject)
    }

    pub fn bind_manager(&self, manager: Arc<AndroidXuserSharingManager>) {
        let service_manager: Arc<dyn XuserSharingManager> = manager.clone();
        self.service.bind_manager(service_manager);
        *self.manager.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(manager);
    }

    pub fn canonical_room_manager_bound(&self) -> bool {
        self.manager
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some()
    }

    pub fn sharing_state(&self) -> super::xuser_sharing_service::SandSharingState {
        self.extension.get_sharing_state()
    }

    pub fn apply_feature_policy(&self, enabled: bool) -> Result<(), String> {
        self.extension.apply_feature_policy(enabled)
    }

    pub fn request_relay_drain(&self) {
        self.extension.request_relay_drain();
    }

    /// Fence all account-scoped sharing state before the canonical account owner advances epoch.
    ///
    /// Installation-local experiment overrides intentionally survive account switches, so clearing
    /// experiment network state alone is insufficient. The sharing service must be disabled and
    /// its previous self identity cleared before a new account can ever re-enable the relay.
    pub fn fence_account_transition(&self) -> Result<(), String> {
        if let Some(manager) = self
            .manager
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .cloned()
        {
            manager.fence_account_transition()?;
        }
        self.extension.fence_account_transition();
        Ok(())
    }

    pub fn prepare_for_upgrade(&self) {
        self.extension.prepare_for_upgrade();
    }
}

pub fn build_xuser_core(
    credentials: Arc<dyn SandMcpCredentialProvider>,
    transport: Arc<dyn XuserRelayTransport>,
    env: &BTreeMap<String, String>,
    app_data_dir: &Path,
) -> Result<ProductionCrossUserCore, String> {
    let backend_url = credentials.configuration().backend_url;
    let identity_credentials = Arc::clone(&credentials);
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

    let durable_root = app_data_dir.join("xuser-sharing");
    let departures = SandXuserDepartureObligations::new(
        Arc::new(SandXuserPendingDepartureStore::new(&durable_root)),
        Arc::new(SandXuserRoomTombstoneStore::new(&durable_root)),
    );
    let departure_relay: Arc<dyn XuserDepartureRelay> = relay.clone();
    service.bind_departure_obligations(departures.clone(), departure_relay);

    let extension = Arc::new(CrossUserSharingExtension::new(
        Arc::clone(&service),
        &backend_url,
        env,
    ));
    let core = ProductionCrossUserCore {
        credentials: identity_credentials,
        relay,
        service,
        relay_driver,
        extension,
        departures,
        manager: Mutex::new(None),
    };
    core.refresh_self_auth_id()?;
    Ok(core)
}

pub fn build_production_xuser_core(
    credentials: Arc<dyn SandMcpCredentialProvider>,
    env: &BTreeMap<String, String>,
    app_data_dir: &Path,
) -> Result<ProductionCrossUserCore, String> {
    let configuration = credentials.configuration();
    let transport: Arc<dyn XuserRelayTransport> = Arc::new(UreqXuserRelayTransport::new(
        "sand",
        configuration.client_version,
        configuration.box_namespace,
    )?);
    build_xuser_core(credentials, transport, env, app_data_dir)
}

fn jwt_subject_from_access_token(token: &str) -> Option<String> {
    let encoded = token.split('.').nth(1)?;
    let payload = URL_SAFE_NO_PAD
        .decode(encoded)
        .or_else(|_| URL_SAFE.decode(encoded))
        .ok()?;
    serde_json::from_slice::<serde_json::Value>(&payload)
        .ok()?
        .get("sub")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
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
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_root(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-xuser-production-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

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
    fn android_packaged_environment_marks_only_production_as_packaged() {
        let packaged = android_xuser_environment(
            true,
            vec![
                ("SAND_HOST_DEV_ERROR_DETAIL".into(), "1".into()),
                ("OTHER".into(), "value".into()),
            ],
        );
        assert_eq!(packaged.get("SAND_PACKAGED").map(String::as_str), Some("1"));
        assert!(!packaged.contains_key("SAND_HOST_DEV_ERROR_DETAIL"));
        assert_eq!(packaged.get("OTHER").map(String::as_str), Some("value"));

        let dev = android_xuser_environment(
            false,
            vec![("SAND_HOST_DEV_ERROR_DETAIL".into(), "1".into())],
        );
        assert!(!dev.contains_key("SAND_PACKAGED"));
        assert_eq!(
            dev.get("SAND_HOST_DEV_ERROR_DETAIL").map(String::as_str),
            Some("1")
        );
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
        let root = test_root("denied");
        let core = build_xuser_core(
            credentials,
            transport,
            &BTreeMap::new(),
            &root,
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
        let _ = std::fs::remove_dir_all(root);
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
        let root = test_root("allowed");
        let core = build_xuser_core(credentials, transport, &env, &root).unwrap();
        core.extension.apply_feature_policy(true).unwrap();
        assert!(core.service.is_started());
        assert!(core.relay_driver.is_started());
        core.extension.stop();
        assert!(!core.service.is_started());
        assert!(!core.relay_driver.is_started());
        assert_eq!(
            core.departures.pending.file_path.parent(),
            Some(root.join("xuser-sharing").as_path())
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn account_transition_fence_stops_relay_and_clears_previous_identity() {
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
        let root = test_root("account-fence");
        let core = build_xuser_core(credentials, transport, &env, &root).unwrap();
        core.service.set_self_auth_id(Some("old-account".into()));
        core.apply_feature_policy(true).unwrap();
        assert!(core.service.is_started());

        core.fence_account_transition();

        assert!(!core.service.is_started());
        assert!(!core.extension.is_enabled());
        // Re-enable the local state without starting/reconciling the relay to prove the old
        // account identity was actually erased rather than merely hidden by disabled projection.
        core.service.set_enabled(true);
        assert_eq!(core.service.get_state().self_auth_id, None);
        core.service.set_enabled(false);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn prepare_for_upgrade_stops_started_core_without_restarting() {
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
        let root = test_root("upgrade");
        let core = build_xuser_core(credentials, transport, &env, &root).unwrap();
        core.apply_feature_policy(true).unwrap();
        assert!(core.service.is_started());
        core.prepare_for_upgrade();
        assert!(!core.service.is_started());
        assert!(
            !core.extension.is_enabled() || !core.service.is_started(),
            "upgrade preparation must not restart sharing work"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn core_refreshes_self_auth_id_from_dedicated_sand_jwt() {
        struct JwtCredentials {
            subject: Mutex<String>,
        }
        impl SandMcpCredentialProvider for JwtCredentials {
            fn configuration(&self) -> crate::mcp_auth::dashboard_mcp_auth_backend::SandMcpBackendConfiguration {
                crate::mcp_auth::dashboard_mcp_auth_backend::SandMcpBackendConfiguration {
                    backend_url: "https://sand.example".into(),
                    client_version: "1.0.0".into(),
                    box_namespace: "prod".into(),
                }
            }

            fn credentials(&self) -> Result<SandMcpBackendCredentials, String> {
                let subject = self.subject.lock().unwrap().clone();
                let payload = URL_SAFE_NO_PAD.encode(
                    serde_json::to_vec(&json!({"sub":subject})).unwrap()
                );
                Ok(SandMcpBackendCredentials {
                    backend_url: "https://sand.example".into(),
                    access_token: format!("header.{payload}.signature"),
                    machine_id: "machine-1".into(),
                    client_version: "1.0.0".into(),
                    box_namespace: "prod".into(),
                })
            }
        }

        let credentials = Arc::new(JwtCredentials {
            subject: Mutex::new("auth-a".into()),
        });
        let provider: Arc<dyn SandMcpCredentialProvider> = credentials.clone();
        let transport: Arc<dyn XuserRelayTransport> = Arc::new(CapturingTransport {
            tokens: Mutex::new(Vec::new()),
        });
        let root = test_root("identity");
        let core = build_xuser_core(provider, transport, &BTreeMap::new(), &root).unwrap();
        assert_eq!(core.sharing_state().self_auth_id.as_deref(), Some("auth-a"));

        *credentials.subject.lock().unwrap() = "auth-b".into();
        assert_eq!(core.refresh_self_auth_id().unwrap().as_deref(), Some("auth-b"));
        assert_eq!(core.sharing_state().self_auth_id.as_deref(), Some("auth-b"));
        let _ = std::fs::remove_dir_all(root);
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
