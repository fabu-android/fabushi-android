use fabushi_android_shared::node::mcp::mcp_server_id::validate_mcp_server_id;
use super::mcp_auth_wait_registry::{
    McpAuthCompletionIdentity, McpAuthWaitRegistration, McpAuthWaitRegistry,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostMcpAuthCompletionEvent {
    pub server_id: String,
    pub server_name: String,
    pub account_key: String,
    pub outcome: String,
    pub requesting_agent_id: Option<String>,
}

pub trait McpAuthCompletionRuntime {
    type Error;

    fn note_auth_completed_elsewhere(
        &mut self,
        server_id: &str,
        account_key: &str,
    ) -> Option<String>;

    fn restart_mcp_management(&mut self) -> Result<(), Self::Error>;

    fn resume_after_mcp_auth(
        &mut self,
        agent_id: &str,
        server_name: &str,
        account_key: &str,
    ) -> Result<(), Self::Error>;
}

pub struct HostMcpAuthCompletion {
    waits: McpAuthWaitRegistry,
}

impl HostMcpAuthCompletion {
    pub fn new(waits: McpAuthWaitRegistry) -> Self {
        Self { waits }
    }

    pub fn register_connect_card(&mut self, now_ms: u64, registration: McpAuthWaitRegistration) {
        self.waits.register(now_ms, registration);
    }

    pub fn resolve<R: McpAuthCompletionRuntime>(
        &mut self,
        now_ms: u64,
        runtime: &mut R,
        completion: HostMcpAuthCompletionEvent,
    ) -> Result<Option<String>, R::Error> {
        let Ok(server_id) = validate_mcp_server_id(&completion.server_id) else {
            return Ok(None);
        };
        let waiting_agent = self.waits.take(
            now_ms,
            &McpAuthCompletionIdentity {
                server_id: server_id.clone(),
                server_name: completion.server_name.clone(),
            },
        );
        let watching_agent = runtime.note_auth_completed_elsewhere(
            &server_id,
            &completion.account_key,
        );

        if completion.outcome.eq_ignore_ascii_case("cancelled") {
            return Ok(None);
        }

        let agent_id = completion
            .requesting_agent_id
            .filter(|value| !value.trim().is_empty())
            .or(watching_agent)
            .or(waiting_agent);

        if let Some(agent_id) = agent_id {
            runtime.resume_after_mcp_auth(
                &agent_id,
                &completion.server_name,
                &completion.account_key,
            )?;
            Ok(Some(agent_id))
        } else {
            Ok(None)
        }
    }

    pub fn resolve_desktop<R: McpAuthCompletionRuntime>(
        &mut self,
        now_ms: u64,
        runtime: &mut R,
        completion: HostMcpAuthCompletionEvent,
    ) -> Result<Option<String>, R::Error> {
        let _ = runtime.restart_mcp_management();
        self.resolve(now_ms, runtime, completion)
    }
}

impl Default for HostMcpAuthCompletion {
    fn default() -> Self {
        Self::new(McpAuthWaitRegistry::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeRuntime {
        watcher: Option<String>,
        restarts: usize,
        resumes: Vec<(String, String, String)>,
        notes: usize,
    }

    impl McpAuthCompletionRuntime for FakeRuntime {
        type Error = ();

        fn note_auth_completed_elsewhere(
            &mut self,
            _server_id: &str,
            _account_key: &str,
        ) -> Option<String> {
            self.notes += 1;
            self.watcher.clone()
        }

        fn restart_mcp_management(&mut self) -> Result<(), Self::Error> {
            self.restarts += 1;
            Ok(())
        }

        fn resume_after_mcp_auth(
            &mut self,
            agent_id: &str,
            server_name: &str,
            account_key: &str,
        ) -> Result<(), Self::Error> {
            self.resumes.push((
                agent_id.to_string(),
                server_name.to_string(),
                account_key.to_string(),
            ));
            Ok(())
        }
    }

    fn completion(outcome: &str) -> HostMcpAuthCompletionEvent {
        HostMcpAuthCompletionEvent {
            server_id: "1".into(),
            server_name: "GitHub".into(),
            account_key: "account".into(),
            outcome: outcome.into(),
            requesting_agent_id: None,
        }
    }

    #[test]
    fn requesting_agent_then_watcher_then_waiter_controls_resume_identity() {
        let mut service = HostMcpAuthCompletion::default();
        service.register_connect_card(
            0,
            McpAuthWaitRegistration {
                agent_id: "waiter".into(),
                connector: "github".into(),
                server_id: None,
            },
        );
        let mut runtime = FakeRuntime {
            watcher: Some("watcher".into()),
            ..Default::default()
        };
        let mut event = completion("completed");
        event.requesting_agent_id = Some("requester".into());
        let selected = service.resolve(1, &mut runtime, event).unwrap();
        assert_eq!(selected.as_deref(), Some("requester"));
        assert_eq!(runtime.resumes[0].0, "requester");
    }

    #[test]
    fn invalid_server_id_completion_fails_closed_before_watch_or_resume() {
        let mut service = HostMcpAuthCompletion::default();
        let mut runtime = FakeRuntime {
            watcher: Some("watcher".into()),
            ..Default::default()
        };
        let mut event = completion("completed");
        event.server_id = "01".into();
        event.requesting_agent_id = Some("requester".into());
        assert_eq!(service.resolve(1, &mut runtime, event).unwrap(), None);
        assert_eq!(runtime.notes, 0);
        assert!(runtime.resumes.is_empty());
    }

    #[test]
    fn cancelled_completion_is_not_resumed_but_still_updates_watch_state() {
        let mut service = HostMcpAuthCompletion::default();
        let mut runtime = FakeRuntime::default();
        assert_eq!(
            service.resolve(1, &mut runtime, completion("cancelled")).unwrap(),
            None
        );
        assert_eq!(runtime.notes, 1);
        assert!(runtime.resumes.is_empty());
    }

    #[test]
    fn desktop_completion_restarts_management_before_resolution() {
        let mut service = HostMcpAuthCompletion::default();
        let mut runtime = FakeRuntime {
            watcher: Some("watcher".into()),
            ..Default::default()
        };
        let selected = service
            .resolve_desktop(1, &mut runtime, completion("completed"))
            .unwrap();
        assert_eq!(selected.as_deref(), Some("watcher"));
        assert_eq!(runtime.restarts, 1);
        assert_eq!(runtime.resumes.len(), 1);
    }
}
