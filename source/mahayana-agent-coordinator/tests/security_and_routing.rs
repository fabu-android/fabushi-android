use fabushi_mahayana_agent_coordinator::{
    gateway::host_supervisor::{HostState, HostSupervisor},
    local_exec::{daemon_files::DaemonDescriptor, supervisor::LocalExecSupervisor},
    oauth::{
        mcp_oauth_callback_listener::OAuthCallback,
        mcp_oauth_forwarder::OAuthForwarder,
        mcp_oauth_loopback_registry::OAuthLoopbackRegistry,
    },
    routed_mcp_bridge::{
        RoutedMcpBridge, RoutedMcpCall, RoutedMcpExecutor, RoutedMcpFailureCode,
        RoutedMcpOutcome, RoutedMcpProviderError, RoutedMcpTool,
    },
    webauthn::{
        provider::{WebAuthnCeremony, WebAuthnRequest},
        signer::{sign_challenge, ChallengeSigner},
    },
};

#[test]
fn oauth_state_is_single_use_and_fail_closed() {
    let mut registry = OAuthLoopbackRegistry::default();
    let state = "0123456789abcdef0123456789abcdef";
    registry.register(state, "example-provider").unwrap();
    let mut forwarder = OAuthForwarder::new(registry);
    let callback = OAuthCallback { state: state.into(), code: Some("code-1".into()), error: None };
    assert_eq!(
        forwarder.forward(callback.clone()).unwrap().0.provider,
        "example-provider"
    );
    assert!(forwarder.forward(callback).is_err());

    let bound_state = "abcdef0123456789abcdef0123456789";
    forwarder
        .register_bound(
            bound_state,
            "bound-provider",
            Some("17"),
            Some("work"),
            Some(42),
        )
        .unwrap();
    let (bound, _) = forwarder
        .forward(OAuthCallback {
            state: bound_state.into(),
            code: None,
            error: Some("access_denied".into()),
        })
        .unwrap();
    assert_eq!(bound.server_id.as_deref(), Some("17"));
    assert_eq!(bound.account_key.as_deref(), Some("work"));
    assert_eq!(bound.generation, Some(42));
}

#[test]
fn routed_mcp_discovery_rejects_duplicate_names() {
    let mut bridge = RoutedMcpBridge::default();
    let tool = RoutedMcpTool {
        name: "files.read".into(),
        provider: "connector".into(),
        remote_name: "read".into(),
        read_only: true,
    };
    bridge.replace_tools([tool.clone()]).unwrap();
    assert_eq!(bridge.tool("files.read"), Some(&tool));
    assert!(bridge.replace_tools([tool.clone(), tool]).is_err());
}

struct RecordingMcpExecutor {
    mode: u8,
}

impl RoutedMcpExecutor for RecordingMcpExecutor {
    fn execute(
        &mut self,
        provider: &str,
        remote_name: &str,
        arguments_json: &str,
    ) -> Result<String, RoutedMcpProviderError> {
        assert_eq!(provider, "connector");
        assert_eq!(remote_name, "read");
        assert!(!arguments_json.is_empty());
        match self.mode {
            0 => Ok(r#"{"ok":true}"#.into()),
            1 => Err(RoutedMcpProviderError::AuthorizationRequired {
                authorization_url: "https://example.com/oauth".into(),
                state: "0123456789abcdef".into(),
            }),
            _ => Err(RoutedMcpProviderError::Failed),
        }
    }
}

#[test]
fn routed_mcp_call_result_auth_and_error_are_explicit() {
    let mut bridge = RoutedMcpBridge::default();
    bridge.replace_tools([RoutedMcpTool {
        name: "files.read".into(),
        provider: "connector".into(),
        remote_name: "read".into(),
        read_only: true,
    }]).unwrap();

    let call = || RoutedMcpCall {
        call_id: "call-1".into(),
        tool_name: "files.read".into(),
        arguments_json: "{}".into(),
    };

    let result = bridge.execute(&mut RecordingMcpExecutor { mode: 0 }, call()).unwrap();
    assert!(matches!(result, RoutedMcpOutcome::Result { .. }));

    let auth = bridge.execute(&mut RecordingMcpExecutor { mode: 1 }, call()).unwrap();
    assert!(matches!(auth, RoutedMcpOutcome::AuthorizationRequired { .. }));

    let failure = bridge.execute(&mut RecordingMcpExecutor { mode: 2 }, call()).unwrap_err();
    assert_eq!(failure.code, RoutedMcpFailureCode::ExecutionFailed);

    let unknown = bridge.execute(
        &mut RecordingMcpExecutor { mode: 0 },
        RoutedMcpCall { call_id: "call-2".into(), tool_name: "missing".into(), arguments_json: "{}".into() },
    ).unwrap_err();
    assert_eq!(unknown.code, RoutedMcpFailureCode::UnknownTool);
}

struct RecordingSigner {
    calls: usize,
}
impl ChallengeSigner for RecordingSigner {
    fn sign(&mut self, relying_party_id: &str, challenge: &[u8]) -> Result<Vec<u8>, String> {
        self.calls += 1;
        Ok([relying_party_id.as_bytes(), challenge].concat())
    }
}

#[test]
fn webauthn_validates_challenge_before_signing() {
    let request = WebAuthnRequest {
        request_id: "req-1".into(),
        relying_party_id: "example.com".into(),
        challenge: vec![7; 32],
        ceremony: WebAuthnCeremony::Get,
    };
    request.validate().unwrap();
    let mut signer = RecordingSigner { calls: 0 };
    assert!(!sign_challenge(&mut signer, &request.relying_party_id, &request.challenge).unwrap().is_empty());
    assert_eq!(signer.calls, 1);
    assert!(sign_challenge(&mut signer, "example.com", &[1, 2]).is_err());
}

#[test]
fn host_and_local_exec_supervisors_expose_recovery_thresholds() {
    let mut host = HostSupervisor::new(2);
    host.begin_connect();
    let first = host.connected("http://127.0.0.1:4100");
    assert_eq!(first.generation, 1);
    assert!(!host.health_failure("timeout"));
    assert!(host.health_failure("timeout"));
    assert!(matches!(host.state(), HostState::Unhealthy { failures: 2, .. }));

    let descriptor = DaemonDescriptor {
        pid: 77,
        started_at_ms: 1,
        generation_token: "generation-1".into(),
        entry_identity: "android-runner".into(),
        inflight_count: 1,
    };
    let mut local = LocalExecSupervisor::new(2);
    local.adopt(descriptor).unwrap();
    assert!(local.note_exit());
    assert!(local.note_exit());
    assert!(!local.note_exit());
}
