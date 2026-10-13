use std::sync::Arc;

use crate::mcp_auth::dashboard_mcp_auth_backend::SandMcpCredentialProvider;

use super::xuser_relay::{
    SandXuserRelayClient, UreqXuserRelayTransport, XuserRelayTransport,
};

/// Build the Android shipping relay without creating a second account/token owner.
///
/// The credential provider is the same dedicated Sand renewal boundary used by the
/// Cursor/Sand backend integration. Fabushi account bearers never enter this path.
pub fn build_xuser_relay_client(
    credentials: Arc<dyn SandMcpCredentialProvider>,
    transport: Arc<dyn XuserRelayTransport>,
) -> Result<Arc<SandXuserRelayClient>, String> {
    let initial = credentials.credentials()?;
    let backend_url = initial.backend_url.clone();
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
    let initial = credentials.credentials()?;
    let transport: Arc<dyn XuserRelayTransport> = Arc::new(UreqXuserRelayTransport::new(
        "sand",
        initial.client_version,
        initial.box_namespace,
    )?);
    build_xuser_relay_client(credentials, transport)
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
            ["sand-token-2", "sand-token-3"]
        );
    }
}
