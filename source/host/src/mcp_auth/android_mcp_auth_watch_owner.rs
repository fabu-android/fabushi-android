use super::{AndroidMcpAuthWatchManager, McpAuthBackendPort};
use fabushi_android_shared::node::mcp::mcp_auth_watch_lifecycle::{
    classify_backend_auth_status, gate_auth_server, McpAuthPollOutcome, McpAuthPollSettlement,
    McpAuthPollTick, McpAuthServerGate, McpAuthServerSnapshot, McpAuthStatusDecision,
    McpAuthWatchCompletion, PendingMcpAuthWatch,
};
use fabushi_android_shared::node::mcp::mcp_server_id::parse_i32_mcp_server_id;
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const OWNER_TICK_MS: u64 = 250;

pub trait McpAuthAdminPolicyPort: Send + Sync {
    fn fresh_server_snapshot(
        &self,
        server_id: &str,
    ) -> Result<Option<McpAuthServerSnapshot>, String>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpAuthOwnerEvent {
    Completed(McpAuthWatchCompletion),
    Cancelled(McpAuthWatchCompletion),
    Expired(McpAuthWatchCompletion),
    BackendUnavailable {
        generation: u64,
        server_id: String,
        account_key: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpAuthenticateResult {
    AlreadyAuthenticated,
    AuthorizationRequired {
        authorization_url: String,
        watch: PendingMcpAuthWatch,
        replaced: Option<PendingMcpAuthWatch>,
    },
    NotConfigured,
    AdminBlocked,
    UnsupportedTransport,
    Unreachable(String),
    NotSupported(String),
}

pub struct AndroidMcpAuthWatchOwner {
    manager: Arc<Mutex<AndroidMcpAuthWatchManager>>,
    events: Arc<Mutex<VecDeque<McpAuthOwnerEvent>>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl AndroidMcpAuthWatchOwner {
    pub fn start(
        manager: Arc<Mutex<AndroidMcpAuthWatchManager>>,
        backend: Arc<dyn McpAuthBackendPort>,
        policy: Arc<dyn McpAuthAdminPolicyPort>,
    ) -> Result<Self, String> {
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let manager_for_worker = Arc::clone(&manager);
        let events_for_worker = Arc::clone(&events);
        let stop_for_worker = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("fabushi-mcp-auth-watch-owner".into())
            .spawn(move || {
                while !stop_for_worker.load(Ordering::Acquire) {
                    advance_all(
                        &manager_for_worker,
                        backend.as_ref(),
                        policy.as_ref(),
                        &events_for_worker,
                        system_now_ms(),
                    );
                    let mut remaining = OWNER_TICK_MS;
                    while remaining > 0 && !stop_for_worker.load(Ordering::Acquire) {
                        let slice = remaining.min(50);
                        thread::sleep(Duration::from_millis(slice));
                        remaining -= slice;
                    }
                }
            })
            .map_err(|error| format!("failed to start MCP auth watch owner: {error}"))?;
        Ok(Self {
            manager,
            events,
            stop,
            worker: Some(worker),
        })
    }

    pub fn manager(&self) -> Arc<Mutex<AndroidMcpAuthWatchManager>> {
        Arc::clone(&self.manager)
    }

    pub fn drain_events(&self) -> Result<Vec<McpAuthOwnerEvent>, String> {
        let mut events = self
            .events
            .lock()
            .map_err(|_| "MCP auth owner event queue lock poisoned".to_string())?;
        Ok(events.drain(..).collect())
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for AndroidMcpAuthWatchOwner {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn authenticate_and_register(
    manager: &mut AndroidMcpAuthWatchManager,
    backend: &dyn McpAuthBackendPort,
    policy: &dyn McpAuthAdminPolicyPort,
    now_ms: u64,
    server_id: &str,
    account_key: &str,
    oauth_redirect_uri: &str,
    requesting_agent_id: Option<&str>,
    force_reauth: bool,
) -> Result<McpAuthenticateResult, String> {
    let initial = policy.fresh_server_snapshot(server_id)?;
    let fresh = if initial
        .as_ref()
        .is_some_and(|snapshot| snapshot.disabled_by_team_admin_policy)
    {
        policy.fresh_server_snapshot(server_id)?
    } else {
        None
    };
    let eligible = match gate_auth_server(initial.as_ref(), fresh.as_ref()) {
        McpAuthServerGate::Eligible(snapshot) => snapshot,
        McpAuthServerGate::NotConfigured => return Ok(McpAuthenticateResult::NotConfigured),
        McpAuthServerGate::AdminBlocked => return Ok(McpAuthenticateResult::AdminBlocked),
        McpAuthServerGate::UnsupportedTransport => {
            return Ok(McpAuthenticateResult::UnsupportedTransport)
        }
    };
    let server_number = parse_i32_mcp_server_id(&eligible.server_id)
        .map_err(|error| error.to_string())?;
    let status = backend.check_auth_status(
        server_number,
        account_key,
        oauth_redirect_uri,
        force_reauth,
    )?;
    match classify_backend_auth_status(
        eligible.server_url.as_deref(),
        &status,
        force_reauth,
    ) {
        McpAuthStatusDecision::AlreadyAuthenticated => {
            Ok(McpAuthenticateResult::AlreadyAuthenticated)
        }
        McpAuthStatusDecision::Start { authorization_url } => {
            let server_url = eligible
                .server_url
                .as_deref()
                .ok_or("MCP auth server URL is required")?;
            let (watch, replaced) = manager.begin_watch(
                now_ms,
                &eligible.server_id,
                &eligible.server_name,
                server_url,
                account_key,
                requesting_agent_id,
                force_reauth,
            )?;
            Ok(McpAuthenticateResult::AuthorizationRequired {
                authorization_url,
                watch,
                replaced,
            })
        }
        McpAuthStatusDecision::Unreachable { detail } => {
            Ok(McpAuthenticateResult::Unreachable(detail))
        }
        McpAuthStatusDecision::NotSupported { detail } => {
            Ok(McpAuthenticateResult::NotSupported(detail))
        }
    }
}

fn advance_all(
    manager: &Arc<Mutex<AndroidMcpAuthWatchManager>>,
    backend: &dyn McpAuthBackendPort,
    policy: &dyn McpAuthAdminPolicyPort,
    events: &Arc<Mutex<VecDeque<McpAuthOwnerEvent>>>,
    now_ms: u64,
) {
    let watches = match manager.lock() {
        Ok(manager) => manager.watches(),
        Err(_) => return,
    };
    for watch in watches {
        let tick = match manager.lock() {
            Ok(mut manager) => manager.poll_tick(
                now_ms,
                &watch.server_id,
                &watch.account_key,
            ),
            Err(_) => return,
        };
        let Ok(tick) = tick else {
            continue;
        };
        match tick {
            McpAuthPollTick::Idle | McpAuthPollTick::Suppressed => {}
            McpAuthPollTick::Expired(completion) => {
                push_event(events, McpAuthOwnerEvent::Expired(completion));
            }
            McpAuthPollTick::Request(request) => {
                let outcome = match policy.fresh_server_snapshot(&request.server_id) {
                    Ok(Some(snapshot)) if snapshot.disabled_by_team_admin_policy => {
                        McpAuthPollOutcome::AdminBlocked
                    }
                    Ok(Some(snapshot)) if snapshot.server_id == request.server_id => {
                        match backend.validate_token(&request.server_url, &request.account_key) {
                            Ok(true) => {
                                match policy.fresh_server_snapshot(&request.server_id) {
                                    Ok(Some(fresh))
                                        if fresh.server_id == request.server_id
                                            && !fresh.disabled_by_team_admin_policy =>
                                    {
                                        McpAuthPollOutcome::TokenValid
                                    }
                                    Ok(_) => McpAuthPollOutcome::AdminBlocked,
                                    Err(_) => McpAuthPollOutcome::Unreachable,
                                }
                            }
                            Ok(false) => McpAuthPollOutcome::TokenInvalid,
                            Err(_) => {
                                push_event(
                                    events,
                                    McpAuthOwnerEvent::BackendUnavailable {
                                        generation: request.generation,
                                        server_id: request.server_id.clone(),
                                        account_key: request.account_key.clone(),
                                    },
                                );
                                McpAuthPollOutcome::Unreachable
                            }
                        }
                    }
                    Ok(_) => McpAuthPollOutcome::AdminBlocked,
                    Err(_) => McpAuthPollOutcome::Unreachable,
                };
                let settled_at = system_now_ms();
                let settlement = match manager.lock() {
                    Ok(mut manager) => {
                        if settled_at > request.deadline_ms {
                            manager.poll_failed(settled_at, &request)
                        } else {
                            manager.settle_poll(settled_at, &request, outcome)
                        }
                    }
                    Err(_) => return,
                };
                if let Ok(settlement) = settlement {
                    match settlement {
                        McpAuthPollSettlement::Completed(completion) => {
                            push_event(events, McpAuthOwnerEvent::Completed(completion))
                        }
                        McpAuthPollSettlement::Cancelled(completion) => {
                            push_event(events, McpAuthOwnerEvent::Cancelled(completion))
                        }
                        McpAuthPollSettlement::Pending | McpAuthPollSettlement::Stale => {}
                    }
                }
            }
        }
    }
}

fn push_event(
    events: &Arc<Mutex<VecDeque<McpAuthOwnerEvent>>>,
    event: McpAuthOwnerEvent,
) {
    if let Ok(mut events) = events.lock() {
        events.push_back(event);
    }
}

fn system_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fabushi_android_shared::node::mcp::mcp_auth_watch::{
        AUTH_WATCH_POLL_INTERVAL_MS, AUTH_WATCH_POLL_TIMEOUT_MS,
    };
    use fabushi_android_shared::node::mcp::mcp_auth_watch_lifecycle::{
        McpAuthServerSnapshot, McpAuthTransport, McpBackendAuthStatus,
    };
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeBackend {
        validations: AtomicUsize,
        valid: bool,
        check: McpBackendAuthStatus,
    }

    impl McpAuthBackendPort for FakeBackend {
        fn check_auth_status(
            &self,
            _server_id: i32,
            _account_key: &str,
            _oauth_redirect_uri: &str,
            _force_reauth: bool,
        ) -> Result<McpBackendAuthStatus, String> {
            Ok(self.check.clone())
        }

        fn validate_token(&self, _server_url: &str, _account_key: &str) -> Result<bool, String> {
            self.validations.fetch_add(1, Ordering::SeqCst);
            Ok(self.valid)
        }
    }

    #[derive(Clone)]
    struct FakePolicy {
        blocked: bool,
    }

    impl McpAuthAdminPolicyPort for FakePolicy {
        fn fresh_server_snapshot(
            &self,
            server_id: &str,
        ) -> Result<Option<McpAuthServerSnapshot>, String> {
            Ok(Some(McpAuthServerSnapshot {
                server_id: server_id.to_string(),
                server_name: "Calendar".into(),
                server_url: Some("https://mcp.example.test".into()),
                transport: McpAuthTransport::Http,
                disabled_by_team_admin_policy: self.blocked,
            }))
        }
    }

    fn temp_store(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-mcp-owner-{name}-{}-{}.json",
            std::process::id(),
            system_now_ms()
        ))
    }

    fn backend(valid: bool) -> FakeBackend {
        FakeBackend {
            validations: AtomicUsize::new(0),
            valid,
            check: McpBackendAuthStatus {
                is_available: false,
                requires_auth: true,
                has_valid_token: false,
                auth_url: "https://auth.example.test".into(),
                error: String::new(),
            },
        }
    }

    #[test]
    fn authenticate_checks_status_then_registers_durable_watch() {
        let path = temp_store("authenticate");
        let mut manager = AndroidMcpAuthWatchManager::open(&path, 0).unwrap();
        let result = authenticate_and_register(
            &mut manager,
            &backend(false),
            &FakePolicy { blocked: false },
            0,
            "17",
            "default",
            "http://127.0.0.1:18080/oauth/callback",
            Some("agent-a"),
            false,
        )
        .unwrap();
        assert!(matches!(
            result,
            McpAuthenticateResult::AuthorizationRequired { ref watch, .. }
                if watch.requesting_agent_id.as_deref() == Some("agent-a")
        ));
        assert_eq!(manager.len(), 1);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn admin_block_fails_closed_before_watch_registration() {
        let path = temp_store("admin");
        let mut manager = AndroidMcpAuthWatchManager::open(&path, 0).unwrap();
        let result = authenticate_and_register(
            &mut manager,
            &backend(false),
            &FakePolicy { blocked: true },
            0,
            "17",
            "default",
            "http://127.0.0.1:18080/oauth/callback",
            None,
            false,
        )
        .unwrap();
        assert_eq!(result, McpAuthenticateResult::AdminBlocked);
        assert!(manager.is_empty());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn valid_token_completes_current_generation_after_fresh_policy_recheck() {
        let path = temp_store("valid");
        let manager = Arc::new(Mutex::new(
            AndroidMcpAuthWatchManager::open(&path, 0).unwrap(),
        ));
        manager
            .lock()
            .unwrap()
            .begin_watch(
                0,
                "17",
                "Calendar",
                "https://mcp.example.test",
                "default",
                Some("agent-a"),
                false,
            )
            .unwrap();
        let events = Arc::new(Mutex::new(VecDeque::new()));
        advance_all(
            &manager,
            &backend(true),
            &FakePolicy { blocked: false },
            &events,
            AUTH_WATCH_POLL_INTERVAL_MS,
        );
        assert!(matches!(
            events.lock().unwrap().front(),
            Some(McpAuthOwnerEvent::Completed(completion))
                if completion.requesting_agent_id.as_deref() == Some("agent-a")
        ));
        assert!(manager.lock().unwrap().is_empty());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn invalid_token_stays_pending_and_stale_generation_cannot_complete() {
        let path = temp_store("invalid");
        let manager = Arc::new(Mutex::new(
            AndroidMcpAuthWatchManager::open(&path, 0).unwrap(),
        ));
        let first = manager
            .lock()
            .unwrap()
            .begin_watch(
                0,
                "17",
                "Calendar",
                "https://mcp.example.test",
                "default",
                Some("agent-a"),
                false,
            )
            .unwrap()
            .0;
        let events = Arc::new(Mutex::new(VecDeque::new()));
        advance_all(
            &manager,
            &backend(false),
            &FakePolicy { blocked: false },
            &events,
            AUTH_WATCH_POLL_INTERVAL_MS,
        );
        assert!(events.lock().unwrap().is_empty());
        let replacement = manager
            .lock()
            .unwrap()
            .begin_watch(
                AUTH_WATCH_POLL_INTERVAL_MS + 1,
                "17",
                "Calendar",
                "https://mcp.example.test",
                "default",
                Some("agent-b"),
                false,
            )
            .unwrap()
            .0;
        assert!(replacement.generation > first.generation);
        assert_eq!(
            manager.lock().unwrap().watch("17", "default").unwrap().generation,
            replacement.generation
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn deadline_is_bounded_and_process_death_restores_then_advances_watch() {
        let path = temp_store("restore");
        {
            let mut manager = AndroidMcpAuthWatchManager::open(&path, 0).unwrap();
            manager
                .begin_watch(
                    0,
                    "17",
                    "Calendar",
                    "https://mcp.example.test",
                    "default",
                    Some("agent-a"),
                    false,
                )
                .unwrap();
        }
        let manager = Arc::new(Mutex::new(
            AndroidMcpAuthWatchManager::open(&path, AUTH_WATCH_POLL_INTERVAL_MS).unwrap(),
        ));
        let restored = manager
            .lock()
            .unwrap()
            .watch("17", "default")
            .cloned()
            .unwrap();
        assert!(!restored.is_polling);
        let events = Arc::new(Mutex::new(VecDeque::new()));
        advance_all(
            &manager,
            &backend(true),
            &FakePolicy { blocked: false },
            &events,
            AUTH_WATCH_POLL_INTERVAL_MS,
        );
        assert!(matches!(
            events.lock().unwrap().front(),
            Some(McpAuthOwnerEvent::Completed(_))
        ));
        assert!(AUTH_WATCH_POLL_TIMEOUT_MS <= 30_000);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn cancel_and_admin_recheck_are_terminal_and_late_callbacks_stay_stale() {
        let path = temp_store("cancel");
        let mut manager = AndroidMcpAuthWatchManager::open(&path, 0).unwrap();
        manager
            .begin_watch(
                0,
                "17",
                "Calendar",
                "https://mcp.example.test",
                "work",
                None,
                false,
            )
            .unwrap();
        let cancelled = manager.cancel_watch("17", "work").unwrap().unwrap();
        assert_eq!(cancelled.outcome, "cancelled");
        assert!(manager.note_auth_completed_elsewhere("17", "work").unwrap().is_none());

        manager
            .begin_watch(
                1,
                "18",
                "Drive",
                "https://mcp.example.test",
                "work",
                None,
                false,
            )
            .unwrap();
        let manager = Arc::new(Mutex::new(manager));
        let events = Arc::new(Mutex::new(VecDeque::new()));
        advance_all(
            &manager,
            &backend(true),
            &FakePolicy { blocked: true },
            &events,
            1 + AUTH_WATCH_POLL_INTERVAL_MS,
        );
        assert!(matches!(
            events.lock().unwrap().front(),
            Some(McpAuthOwnerEvent::Cancelled(completion))
                if completion.server_id == "18"
        ));
        let _ = fs::remove_file(path);
    }
}
