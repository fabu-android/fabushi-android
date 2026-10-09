use crate::account_service::{AccountSessionMutation, AndroidAccountService};
use crate::android_agent_roster::AndroidAgentRoster;
use crate::host_secret_store::get_or_create_host_machine_id;
use crate::messaging_service::AndroidMessagingService;
use crate::extensions::transcript::TranscriptStore;
use crate::extensions::webauthn_proxy::{
    WebAuthnBridgeError, WebAuthnProxyExtension, WebAuthnProxyExtensionConfig,
};
use crate::runner::{
    AndroidHostInferenceProvider, AndroidInferenceMode, ProductionTurnAgentOwner,
    ProductionTurnEvent, ProductionTurnInput,
};
use fabushi_constants::composer::text_size_allowed;
use fabushi_android_shared::webauthn_gateway::{
    WebAuthnCeremony, WebAuthnRequestFrame, WebAuthnResponseFrame, WebAuthnStage,
    WebAuthnStageOutcome,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(feature = "ci-account-session-import")]
mod ci_account_session {
    use super::*;
    use std::fs;

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(super) struct CiAccountSessionIdentity {
        pub(super) access_token: String,
        pub(super) session_id: String,
        pub(super) device_id: String,
        pub(super) expires_at_epoch_seconds: u64,
    }

    const CI_SESSION_MAX_BYTES: u64 = 64 * 1024;
    const CI_SESSION_MAX_LIFETIME_SECONDS: u64 = 5 * 60 * 60;

    pub(super) fn parse_ci_account_session_document(
        document: &Value,
        now_epoch_seconds: u64,
    ) -> Option<CiAccountSessionIdentity> {
        let object = document.as_object()?;
        let access_token = object.get("accessToken")?.as_str()?;
        let device_id = object.get("deviceId")?.as_str()?;
        let session_id = object.get("sessionId")?.as_str()?;
        let token_type = object
            .get("tokenType")
            .and_then(Value::as_str)
            .unwrap_or("Bearer");
        let provider = object.get("provider")?.as_str()?;
        let ci_runner = object.get("ciRunner")?.as_bool()?;
        let expiry = object.get("accessTokenExpiresAt")?.as_u64()?;

        if access_token.len() < 24
            || access_token.len() > 16 * 1024
            || access_token.chars().any(char::is_whitespace)
            || token_type != "Bearer"
            || provider != "github-actions"
            || !ci_runner
            || object.contains_key("refreshToken")
            || expiry <= now_epoch_seconds.saturating_add(30)
            || expiry > now_epoch_seconds.saturating_add(CI_SESSION_MAX_LIFETIME_SECONDS)
        {
            return None;
        }

        let device_inner = device_id
            .strip_prefix("gha-")?
            .strip_suffix("-interactive")?;
        let (device_run, device_attempt) = device_inner.split_once('-')?;
        if device_run.is_empty()
            || device_attempt.is_empty()
            || !device_run.chars().all(|value| value.is_ascii_digit())
            || !device_attempt.chars().all(|value| value.is_ascii_digit())
        {
            return None;
        }

        let session_inner = session_id.strip_prefix("ci-runner:")?;
        let (session_run, session_attempt) = session_inner.split_once(':')?;
        if session_run != device_run
            || session_attempt != device_attempt
            || session_run.is_empty()
            || session_attempt.is_empty()
            || !session_run.chars().all(|value| value.is_ascii_digit())
            || !session_attempt.chars().all(|value| value.is_ascii_digit())
        {
            return None;
        }

        Some(CiAccountSessionIdentity {
            access_token: access_token.to_string(),
            session_id: session_id.to_string(),
            device_id: device_id.to_string(),
            expires_at_epoch_seconds: expiry,
        })
    }

    pub(super) fn from_environment(
        now_epoch_seconds: u64,
    ) -> Option<(PathBuf, CiAccountSessionIdentity)> {
        if std::env::var("GITHUB_ACTIONS").ok().as_deref() != Some("true") {
            return None;
        }
        let path = PathBuf::from(std::env::var("FABUSHI_CI_ACCOUNT_SESSION_FILE").ok()?);
        let metadata = fs::metadata(&path).ok()?;
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > CI_SESSION_MAX_BYTES {
            return None;
        }
        let raw = fs::read_to_string(&path).ok()?;
        let document: Value = serde_json::from_str(&raw).ok()?;
        let identity = parse_ci_account_session_document(&document, now_epoch_seconds)?;
        Some((path, identity))
    }


}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AndroidHostMode {
    Production,
    Test,
}

pub struct AndroidJsonHost {
    mode: AndroidHostMode,
    account: AndroidAccountService,
    agents: AndroidAgentRoster,
    transcript: Arc<Mutex<TranscriptStore>>,
    messaging: AndroidMessagingService,
    #[cfg(feature = "ci-account-session-import")]
    ci_session_path: Option<PathBuf>,
    #[cfg(feature = "ci-account-session-import")]
    ci_session_identity: Option<ci_account_session::CiAccountSessionIdentity>,
    logged_in: bool,
    next_attempt: u64,
    next_operation: u64,
    oauth_attempts: BTreeSet<String>,
    browser_attempts: BTreeMap<String, String>,
    events: VecDeque<Value>,
    active_operations: BTreeSet<String>,
    pending_approvals: BTreeMap<String, String>,
    turn_events: Arc<Mutex<VecDeque<Value>>>,
    turn_cancellations: BTreeMap<String, Arc<AtomicBool>>,
    installed_plugins: BTreeSet<String>,
    webauthn: WebAuthnProxyExtension,
    webauthn_provider_queues: BTreeMap<String, VecDeque<WebAuthnRequestFrame>>,
}

impl AndroidJsonHost {
    pub fn new(app_data_dir: impl Into<PathBuf>, mode: AndroidHostMode) -> Self {
        Self::new_with_account_session(app_data_dir, mode, None)
    }

    pub fn new_with_account_session(
        app_data_dir: impl Into<PathBuf>,
        mode: AndroidHostMode,
        initial_account_session_json: Option<&str>,
    ) -> Self {
        let app_data_dir = app_data_dir.into();
        let device_id = get_or_create_host_machine_id(&app_data_dir.join("machine-id"))
            .unwrap_or_else(|error| panic!("failed to open canonical Android machine id: {error}"));
        let account = AndroidAccountService::new(
            device_id,
            (mode == AndroidHostMode::Production)
                .then_some(initial_account_session_json)
                .flatten(),
        )
        .unwrap_or_else(|error| panic!("failed to initialize Android account service: {error}"));
        let agents = AndroidAgentRoster::open(app_data_dir.join("agents.json"))
            .unwrap_or_else(|error| panic!("failed to open canonical Android agent roster: {error}"));
        let transcript = Arc::new(Mutex::new(
            TranscriptStore::open(app_data_dir.join("transcript.json"))
                .unwrap_or_else(|error| panic!("failed to open canonical Android transcript: {error}")),
        ));
        let messaging = AndroidMessagingService::open(&app_data_dir)
            .unwrap_or_else(|error| panic!("failed to open canonical Android messaging repository: {error}"));
        #[cfg(feature = "ci-account-session-import")]
        let ci_session = if mode == AndroidHostMode::Production {
            ci_account_session::from_environment(now_ms() / 1_000)
        } else {
            None
        };
        #[cfg(feature = "ci-account-session-import")]
        let logged_in = ci_session.is_some();
        #[cfg(feature = "ci-account-session-import")]
        let (ci_session_path, ci_session_identity) = ci_session
            .map(|(path, identity)| (Some(path), Some(identity)))
            .unwrap_or((None, None));
        #[cfg(not(feature = "ci-account-session-import"))]
        let logged_in = false;
        Self {
            mode,
            account,
            agents,
            transcript,
            messaging,
            #[cfg(feature = "ci-account-session-import")]
            ci_session_path,
            #[cfg(feature = "ci-account-session-import")]
            ci_session_identity,
            logged_in,
            next_attempt: 0,
            next_operation: 0,
            oauth_attempts: BTreeSet::new(),
            browser_attempts: BTreeMap::new(),
            events: VecDeque::new(),
            active_operations: BTreeSet::new(),
            pending_approvals: BTreeMap::new(),
            turn_events: Arc::new(Mutex::new(VecDeque::new())),
            turn_cancellations: BTreeMap::new(),
            installed_plugins: BTreeSet::new(),
            webauthn: WebAuthnProxyExtension::new(WebAuthnProxyExtensionConfig::default()),
            webauthn_provider_queues: BTreeMap::new(),
        }
    }

    pub fn dispatch(&mut self, method: &str, params: &Value) -> Result<Value, String> {
        match method {
            "host.platform" => Ok(json!({"platform":"android"})),
            "feature.info" => Ok(json!({
                "platform": "android",
                "protocolVersion": "fabushi.feature.v1",
                "runtimeVersion": match self.mode {
                    AndroidHostMode::Production => "android-production",
                    AndroidHostMode::Test => "android-test",
                }
            })),
            "feature.auth.status" => self.account_status(),
            "feature.auth.deviceAgentSession" => Ok(self.device_agent_session()),
            "feature.auth.providers" => Ok(json!([
                {"id":"google","displayName":"Google"},
                {"id":"github","displayName":"GitHub"}
            ])),
            "feature.auth.browserStart" => self.auth_browser_start(),
            "feature.auth.browserReopen" => self.auth_browser_reopen(params),
            "feature.auth.browserCancel" => self.auth_browser_cancel(params),
            "feature.auth.browserPoll" => self.auth_browser_poll(params),
            "feature.auth.oauthStart" => self.oauth_start(params),
            "feature.auth.oauthPoll" => self.oauth_poll(params),
            "feature.mcp.oauthComplete" => self.mcp_oauth_complete(params),
            "feature.auth.logout" => self.account_logout(),
            "listAgents" => Ok(Value::Array(
                self.agents.list().into_iter().map(|agent| agent.as_json()).collect()
            )),
            "countAgents" => Ok(json!(self.agents.count())),
            "createAgent" => {
                let name = required_string(params, "name")?;
                let description = params.get("description").and_then(Value::as_str).unwrap_or("");
                let agent = self.agents.create(name, description).map_err(|error| error.to_string())?;
                Ok(json!({"agent": agent.as_json()}))
            }
            "updateAgent" => {
                let id = required_string(params, "id")?;
                let current = self.agents.get(id).ok_or_else(|| "agent not found".to_string())?;
                let profile = params.get("profile").and_then(Value::as_object).ok_or("profile is required")?;
                let name = profile.get("name").and_then(Value::as_str).unwrap_or(&current.name);
                let description = profile.get("description").and_then(Value::as_str).unwrap_or(&current.description);
                let agent = self.agents.update_profile(id, name, description).map_err(|error| error.to_string())?;
                Ok(agent.as_json())
            }
            "setAgentHiddenFromSidebar" => {
                let id = required_string(params, "id")?;
                let is_hidden = params.get("isHidden").and_then(Value::as_bool).ok_or("isHidden is required")?;
                let agent = self.agents.set_hidden(id, is_hidden).map_err(|error| error.to_string())?;
                Ok(agent.as_json())
            }
            "setAgentUnread" => {
                let id = required_string(params, "id")?;
                let is_unread = params.get("isUnread").and_then(Value::as_bool).ok_or("isUnread is required")?;
                let agent = self.agents.set_unread(id, is_unread).map_err(|error| error.to_string())?;
                Ok(agent.as_json())
            }
            "duplicateAgent" => {
                let id = required_string(params, "id")?;
                let agent = self.agents.duplicate(id).map_err(|error| error.to_string())?;
                Ok(json!({"agent": agent.as_json()}))
            }
            "deleteAgents" => {
                let ids = params.get("ids").and_then(Value::as_array).ok_or("ids array is required")?
                    .iter().filter_map(Value::as_str).map(str::to_string).collect::<Vec<_>>();
                let deleted = self.agents.delete(&ids).map_err(|error| error.to_string())?;
                Ok(json!({"deletedIds": deleted}))
            }
            "getPinnedAgents" => Ok(json!(self.agents.pinned_agent_ids())),
            "setPinnedAgents" => {
                let ids = params.get("ids").and_then(Value::as_array).ok_or("ids array is required")?
                    .iter().filter_map(Value::as_str).map(str::to_string).collect::<Vec<_>>();
                let ids = self.agents.set_pinned_agents(&ids).map_err(|error| error.to_string())?;
                Ok(json!(ids))
            }
            "feature.execute" => self.feature_execute(params),
            "feature.receive" => self.feature_receive(),
            "feature.interrupt" => self.feature_interrupt(params),
            "feature.approval.resolve" => self.feature_approval_resolve(params),
            "feature.marketplace.browse" => self.marketplace_browse(params),
            "feature.marketplace.release" => self.marketplace_release(params),
            "feature.plugin.install" => self.plugin_install(params),
            "feature.plugin.uiDocument" => self.plugin_ui_document(params),
            "plugin.compatibility"
            | "plugin.permission.grant"
            | "plugin.permission.revoke"
            | "runtime.start"
            | "runtime.stop"
            | "runtime.tools"
            | "runtime.call" => Err(format!(
                "{method} is unavailable until the canonical portable Mahayana plugin runtime is migrated; Android refuses placeholder success"
            )),
            "feature.messaging.access.issue" => self.messaging_access_issue(params),
            "feature.messaging.blob.read" => self.messaging_blob_read(params),
            "feature.messaging.execute" => self.messaging_execute(params),
            "feature.transcript.snapshot" => Ok(Value::Array(
                self.transcript
                    .lock()
                    .map_err(|_| "transcript lock poisoned".to_string())?
                    .get_transcript(),
            )),
            "feature.webauthn.registerProvider" => self.webauthn_register_provider(),
            "feature.webauthn.unregisterProvider" => self.webauthn_unregister_provider(params),
            "feature.webauthn.pollRequest" => self.webauthn_poll_request(params),
            "feature.webauthn.submitResponses" => self.webauthn_submit_responses(params),
            "feature.webauthn.requestCeremony" => self.webauthn_request_ceremony(params),
            "platform.request" => self.platform_request(params),
            other => Err(format!("unknown host method {other}")),
        }
    }


    fn messaging_access_issue(&mut self, params: &Value) -> Result<Value, String> {
        let requested_session = required_string(params, "sessionId")?.to_string();
        let requested_device = required_string(params, "deviceId")?.to_string();
        let scopes = params
            .get("scopes")
            .and_then(Value::as_array)
            .ok_or("messaging scopes array is required")?;
        if !scopes.iter().any(|scope| scope.as_str() == Some("messaging")) {
            return Err("messaging scope is required".into());
        }
        let (actor_id, mutation) = self.current_messaging_identity()?;
        Ok(with_account_session_mutation(
            json!({
                "status":"available",
                "actorId":actor_id,
                "sessionId":requested_session,
                "deviceId":requested_device,
                "protocolVersion":2
            }),
            mutation,
        ))
    }

    fn messaging_execute(&mut self, params: &Value) -> Result<Value, String> {
        let (actor_id, mutation) = self.current_messaging_identity()?;
        let result = self
            .messaging
            .execute(params, &actor_id, i64::try_from(now_ms()).unwrap_or(i64::MAX))?;
        Ok(with_account_session_mutation(result, mutation))
    }

    fn messaging_blob_read(&mut self, params: &Value) -> Result<Value, String> {
        let (actor_id, mutation) = self.current_messaging_identity()?;
        let result = self.messaging.read_blob_range(params, &actor_id)?;
        Ok(with_account_session_mutation(result, mutation))
    }

    fn current_messaging_identity(
        &mut self,
    ) -> Result<(String, Option<AccountSessionMutation>), String> {
        if self.mode == AndroidHostMode::Test {
            if !self.logged_in {
                return Err("Sign in to Fabushi to use messaging.".into());
            }
            return Ok(("human:android-test".into(), None));
        }
        #[cfg(feature = "ci-account-session-import")]
        if let Some(identity) = self.ci_session_identity.as_ref() {
            if !self.logged_in {
                return Err("Sign in to Fabushi to use messaging.".into());
            }
            return Ok((
                format!("human:ci:{}", identity.session_id.replace(':', "-")),
                None,
            ));
        }
        let (status, mutation) = self.account.public_status()?;
        if status.get("loggedIn").and_then(Value::as_bool) != Some(true) {
            return Err("Sign in to Fabushi to use messaging.".into());
        }
        let raw_id = status
            .get("user")
            .and_then(|user| user.get("id"))
            .ok_or("Fabushi account identity is missing a user id.")?;
        let raw_id = match raw_id {
            Value::String(value) => value.clone(),
            Value::Number(value) => value.to_string(),
            _ => return Err("Fabushi account user id has an unsupported type.".into()),
        };
        let actor_id = if raw_id.starts_with("human:") {
            raw_id
        } else {
            format!("human:{raw_id}")
        };
        Ok((actor_id, mutation))
    }

    fn webauthn_register_provider(&mut self) -> Result<Value, String> {
        let now = now_ms();
        let (provider_id, welcome) = self.webauthn.register_provider(now);
        self.webauthn_provider_queues
            .entry(provider_id.clone())
            .or_default()
            .push_back(welcome);
        Ok(json!({"providerId": provider_id}))
    }

    fn webauthn_unregister_provider(&mut self, params: &Value) -> Result<Value, String> {
        let provider_id = required_string(params, "providerId")?.to_string();
        self.webauthn.bridge_mut().unregister_provider(&provider_id);
        self.webauthn_provider_queues.remove(&provider_id);
        Ok(json!({"providerId":provider_id,"status":"unregistered"}))
    }

    fn webauthn_poll_request(&mut self, params: &Value) -> Result<Value, String> {
        let provider_id = required_string(params, "providerId")?.to_string();
        for (target_provider, _, frame) in self.webauthn.expire(now_ms()) {
            self.webauthn_provider_queues
                .entry(target_provider)
                .or_default()
                .push_back(frame);
        }
        let frame = self
            .webauthn_provider_queues
            .entry(provider_id.clone())
            .or_default()
            .pop_front();
        Ok(json!({
            "providerId": provider_id,
            "frame": frame.map(webauthn_request_frame_json).unwrap_or(Value::Null),
        }))
    }

    fn webauthn_submit_responses(&mut self, params: &Value) -> Result<Value, String> {
        let provider_id = params
            .get("providerId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let raw_frames = params
            .get("frames")
            .and_then(Value::as_array)
            .ok_or("frames array is required")?;
        let frames = raw_frames
            .iter()
            .map(parse_webauthn_response_frame)
            .collect::<Result<Vec<_>, _>>()?;
        let settlements = self
            .webauthn
            .submit_responses(now_ms(), provider_id, &frames)
            .into_iter()
            .map(|(request_id, settlement)| match settlement {
                crate::extensions::webauthn_proxy::WebAuthnBridgeSettlement::CredentialJson(value) => {
                    json!({"requestId":request_id,"kind":"result","credentialJson":value})
                }
                crate::extensions::webauthn_proxy::WebAuthnBridgeSettlement::Error { name, message, code } => {
                    json!({"requestId":request_id,"kind":"error","name":name,"message":message,"code":code})
                }
            })
            .collect::<Vec<_>>();
        Ok(json!({"accepted":true,"settlements":settlements}))
    }

    fn webauthn_request_ceremony(&mut self, params: &Value) -> Result<Value, String> {
        let kind = required_string(params, "kind")?;
        if kind != "create" && kind != "get" {
            return Err("kind must be create or get".into());
        }
        let origin = required_string(params, "origin")?;
        let payload_json = params
            .get("payloadJson")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or("payloadJson is required")?;
        serde_json::from_str::<Value>(payload_json)
            .map_err(|error| format!("payloadJson must be valid JSON: {error}"))?;

        let request = self
            .webauthn
            .request_ceremony(
                now_ms(),
                WebAuthnCeremony {
                    kind: kind.to_string(),
                    origin: origin.to_string(),
                    payload_json: payload_json.to_string(),
                },
            )
            .map_err(webauthn_bridge_error_message)?;
        self.webauthn_provider_queues
            .entry(request.provider_id.clone())
            .or_default()
            .push_back(request.frame);
        Ok(json!({
            "providerId":request.provider_id,
            "requestId":request.request_id,
            "deadlineAtMs":request.deadline_at_ms,
        }))
    }

    pub fn cancel_operation(&mut self, operation_id: &str, reason: Option<&str>) -> Result<(), String> {
        if operation_id.trim().is_empty() {
            return Err("operation id is required".into());
        }
        if let Some(cancelled) = self.turn_cancellations.remove(operation_id) {
            cancelled.store(true, Ordering::Release);
        }
        self.active_operations.remove(operation_id);
        self.pending_approvals.retain(|_, pending_operation| pending_operation != operation_id);
        self.turn_events
            .lock()
            .map_err(|_| "turn event queue lock poisoned".to_string())?
            .retain(|event| {
                event.get("operationId").and_then(Value::as_str) != Some(operation_id)
            });
        self.events.push_back(json!({
            "type":"operation.interrupted",
            "operationId":operation_id,
            "reason":reason.unwrap_or("cancelled")
        }));
        Ok(())
    }

    fn account_status(&mut self) -> Result<Value, String> {
        if self.mode == AndroidHostMode::Test {
            return Ok(self.auth_status());
        }
        #[cfg(feature = "ci-account-session-import")]
        if self.ci_session_identity.is_some() {
            return Ok(self.auth_status());
        }
        let (status, mutation) = self.account.public_status()?;
        Ok(with_account_session_mutation(status, mutation))
    }

    fn account_logout(&mut self) -> Result<Value, String> {
        if self.mode == AndroidHostMode::Test {
            self.logged_in = false;
            return Ok(self.auth_status());
        }
        #[cfg(feature = "ci-account-session-import")]
        if self.ci_session_identity.is_some() {
            self.logged_in = false;
            self.ci_session_identity = None;
            if let Some(path) = self.ci_session_path.take() {
                let _ = std::fs::remove_file(path);
            }
            return Ok(self.auth_status());
        }
        let (status, mutation) = self.account.logout()?;
        Ok(with_account_session_mutation(status, Some(mutation)))
    }

    fn auth_browser_start(&mut self) -> Result<Value, String> {
        if self.mode == AndroidHostMode::Test {
            return self.browser_start();
        }
        self.account.browser_start()
    }

    fn auth_browser_reopen(&mut self, params: &Value) -> Result<Value, String> {
        if self.mode == AndroidHostMode::Test {
            return self.browser_reopen(params);
        }
        self.account.browser_reopen(required_string(params, "attemptId")?)
    }

    fn auth_browser_cancel(&mut self, params: &Value) -> Result<Value, String> {
        if self.mode == AndroidHostMode::Test {
            return self.browser_cancel(params);
        }
        self.account.browser_cancel(required_string(params, "attemptId")?)
    }

    fn auth_browser_poll(&mut self, params: &Value) -> Result<Value, String> {
        if self.mode == AndroidHostMode::Test {
            return self.browser_poll(params);
        }
        let (result, mutation) = self
            .account
            .browser_poll(required_string(params, "attemptId")?)?;
        Ok(with_account_session_mutation(result, mutation))
    }

    fn auth_status(&self) -> Value {
        if self.logged_in {
            json!({
                "loggedIn":true,
                "user":{
                    "nickname":"Fabushi",
                    "username":"fabushi",
                    "email":"fabushi@example.invalid"
                }
            })
        } else {
            json!({"loggedIn":false,"user":Value::Null})
        }
    }

    fn device_agent_session(&self) -> Value {
        if !self.logged_in {
            return json!({"loggedIn": false});
        }
        #[cfg(feature = "ci-account-session-import")]
        {
            let Some(identity) = self.ci_session_identity.as_ref() else {
                return json!({
                    "loggedIn": true,
                    "available": false,
                    "reason": "device_agent_session_unavailable"
                });
            };
            return json!({
                "loggedIn": true,
                "available": true,
                "accessToken": identity.access_token,
                "deviceId": identity.device_id,
                "sessionId": identity.session_id,
                "accessTokenExpiresAt": identity.expires_at_epoch_seconds,
            });
        }
        #[cfg(not(feature = "ci-account-session-import"))]
        {
            json!({
                "loggedIn": true,
                "available": false,
                "reason": "device_agent_session_unavailable"
            })
        }
    }

    fn next_attempt_id(&mut self, prefix: &str) -> String {
        self.next_attempt = self.next_attempt.saturating_add(1);
        format!("{prefix}_{:08}", self.next_attempt)
    }

    fn next_operation_id(&mut self, request_id: &str) -> String {
        self.next_operation = self.next_operation.saturating_add(1);
        if request_id.trim().is_empty() {
            format!("android-operation-{:08}", self.next_operation)
        } else {
            request_id.to_string()
        }
    }

    fn browser_start(&mut self) -> Result<Value, String> {
        let attempt_id = self.next_attempt_id("browser");
        let url = format!("https://auth.fabushi.invalid/android?attemptId={attempt_id}");
        self.browser_attempts.insert(attempt_id.clone(), "pending".into());
        Ok(json!({"attemptId":attempt_id,"url":url,"status":"pending"}))
    }

    fn browser_reopen(&mut self, params: &Value) -> Result<Value, String> {
        let attempt_id = required_string(params, "attemptId")?;
        if !self.browser_attempts.contains_key(attempt_id) {
            return Err("browser login attempt is unknown".into());
        }
        Ok(json!({
            "attemptId":attempt_id,
            "url":format!("https://auth.fabushi.invalid/android?attemptId={attempt_id}")
        }))
    }

    fn browser_cancel(&mut self, params: &Value) -> Result<Value, String> {
        let attempt_id = required_string(params, "attemptId")?.to_string();
        self.browser_attempts.insert(attempt_id.clone(), "cancelled".into());
        Ok(json!({"attemptId":attempt_id,"status":"cancelled"}))
    }

    fn browser_poll(&mut self, params: &Value) -> Result<Value, String> {
        let attempt_id = required_string(params, "attemptId")?.to_string();
        let status = self.browser_attempts.get(&attempt_id).cloned().unwrap_or_else(|| "unknown".into());
        if self.mode == AndroidHostMode::Test && status == "pending" {
            self.logged_in = true;
            self.browser_attempts.insert(attempt_id.clone(), "completed".into());
            return Ok(json!({"attemptId":attempt_id,"status":"completed","auth":self.auth_status()}));
        }
        Ok(json!({"attemptId":attempt_id,"status":status}))
    }

    fn mcp_oauth_complete(&mut self, params: &Value) -> Result<Value, String> {
        let provider = required_string(params, "provider")?.to_string();
        let state = required_string(params, "state")?.to_string();
        let code = params
            .get("code")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let error = params
            .get("error")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        if code.is_some() == error.is_some() {
            return Err("MCP OAuth completion requires exactly one of code or error".into());
        }

        let outcome = if error.is_some() { "failed" } else { "completed" };
        self.events.push_back(json!({
            "type": if outcome == "completed" {
                "mcp.auth.completed"
            } else {
                "mcp.auth.failed"
            },
            "provider": provider.clone(),
            "state": state.clone(),
            "outcome": outcome,
        }));
        Ok(json!({
            "provider": provider,
            "state": state,
            "outcome": outcome,
        }))
    }

    fn oauth_start(&mut self, params: &Value) -> Result<Value, String> {
        let provider = required_string(params, "provider")?;
        let attempt_id = self.next_attempt_id("oauth");
        self.oauth_attempts.insert(attempt_id.clone());
        Ok(json!({
            "attemptId":attempt_id,
            "provider":provider,
            "url":format!("https://auth.fabushi.invalid/oauth/{provider}?attemptId={attempt_id}")
        }))
    }

    fn oauth_poll(&mut self, params: &Value) -> Result<Value, String> {
        let attempt_id = required_string(params, "attemptId")?;
        if !self.oauth_attempts.remove(attempt_id) {
            return Err("OAuth attempt is unknown or already consumed".into());
        }
        if self.mode == AndroidHostMode::Test {
            self.logged_in = true;
            Ok(json!({"attemptId":attempt_id,"status":"completed","auth":self.auth_status()}))
        } else {
            Ok(json!({"attemptId":attempt_id,"status":"pending"}))
        }
    }

    fn feature_execute(&mut self, params: &Value) -> Result<Value, String> {
        let command = params.get("command").and_then(Value::as_object).ok_or("feature.execute requires command")?;
        let command = Value::Object(command.clone());
        let kind = required_string(&command, "type")?;
        let request_id = command.get("requestId").and_then(Value::as_str).unwrap_or("");
        let operation_id = self.next_operation_id(request_id);
        self.active_operations.insert(operation_id.clone());
        self.events.push_back(json!({
            "type":"operation.started",
            "operationId":operation_id,
            "requestId":request_id
        }));
        let mut private_session_mutation = None;

        match kind {
            "bot.list" => {
                let bots = self.agents.list().into_iter().map(|agent| agent.as_json()).collect::<Vec<_>>();
                self.events.push_back(json!({
                    "type":"bot.listed",
                    "operationId":operation_id,
                    "requestId":request_id,
                    "bots":bots,
                }));
                self.finish_operation(&operation_id);
            }
            "bot.create" => {
                let name = required_string(&command, "name")?;
                let description = command.get("description").and_then(Value::as_str).unwrap_or("");
                let agent = self.agents.create(name, description).map_err(|error| error.to_string())?;
                self.events.push_back(json!({
                    "type":"bot.created",
                    "operationId":operation_id,
                    "requestId":request_id,
                    "bot":agent.as_json(),
                }));
                self.finish_operation(&operation_id);
            }
            "chat.send" => {
                let (bearer_token, session_mutation) = self.bearer_token_for_turn()?;
                self.queue_chat_turn(
                    &operation_id,
                    request_id,
                    &command,
                    bearer_token,
                )?;
                private_session_mutation = session_mutation;
            }
            "marketplace.install" => {
                if let Some(id) = command.get("miniAppId").and_then(Value::as_str) {
                    self.installed_plugins.insert(id.to_string());
                }
                self.finish_operation(&operation_id);
            }
            "miniapp.open" | "session.clear" => {
                self.finish_operation(&operation_id);
            }
            "capability.request" => {
                let capability = required_string(&command, "capability")?;
                let approval_id = format!("approval-{operation_id}");
                if self.pending_approvals.contains_key(&approval_id) {
                    return Err("approval identity collision".into());
                }
                self.pending_approvals
                    .insert(approval_id.clone(), operation_id.clone());
                self.events.push_back(json!({
                    "type":"approval.requested",
                    "operationId":operation_id,
                    "approvalId":approval_id,
                    "capability":capability,
                    "reason":command.get("reason").cloned().unwrap_or(Value::Null)
                }));
            }
            "runtime.longTask" => {}
            _ => {
                self.finish_operation(&operation_id);
            }
        }

        Ok(with_account_session_mutation(
            json!({"requestId":request_id,"operationId":operation_id,"accepted":true}),
            private_session_mutation,
        ))
    }

    fn bearer_token_for_turn(
        &mut self,
    ) -> Result<(Option<String>, Option<AccountSessionMutation>), String> {
        if self.mode == AndroidHostMode::Test {
            return Ok((None, None));
        }
        #[cfg(feature = "ci-account-session-import")]
        if let Some(identity) = self.ci_session_identity.as_ref() {
            return Ok((Some(identity.access_token.clone()), None));
        }
        let (token, mutation) = self.account.valid_access_token()?;
        Ok((Some(token), mutation))
    }

    fn queue_chat_turn(
        &mut self,
        operation_id: &str,
        request_id: &str,
        command: &Value,
        bearer_token: Option<String>,
    ) -> Result<(), String> {
        let prompt = command
            .get("text")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or("chat.send text is required")?
            .to_string();
        if !text_size_allowed(&prompt) {
            return Err("chat.send text exceeds maximum composer size".into());
        }

        let assistant_entry_id = format!("assistant:{operation_id}");
        {
            let mut transcript = self
                .transcript
                .lock()
                .map_err(|_| "transcript lock poisoned".to_string())?;
            let user_was_new = transcript
                .append_entry_if_absent(json!({
                    "id":request_id,
                    "kind":"message",
                    "role":"user",
                    "content":prompt.clone(),
                    "operationId":operation_id,
                    "timestampMs":now_ms(),
                }))
                .map_err(|error| format!("failed to persist user transcript entry: {error}"))?;

            if !user_was_new {
                if let Some(existing) = transcript.entry(&assistant_entry_id) {
                    let text = existing
                        .get("content")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let mut events = self
                        .turn_events
                        .lock()
                        .map_err(|_| "turn event queue lock poisoned".to_string())?;
                    if !text.is_empty() {
                        events.push_back(json!({
                            "type":"chat.message",
                            "operationId":operation_id,
                            "requestId":request_id,
                            "role":"assistant",
                            "text":text,
                            "recovered":true,
                        }));
                    }
                    events.push_back(json!({
                        "type":"operation.completed",
                        "operationId":operation_id,
                        "requestId":request_id,
                        "finishReason":"recovered",
                        "attempts":0,
                        "deduped":true,
                    }));
                    return Ok(());
                }
            }
        }

        let agent_id = command
            .get("agentId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("mahayana-assistant")
            .to_string();
        let model = command
            .get("model")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("default")
            .to_string();

        let cancelled = Arc::new(AtomicBool::new(false));
        self.turn_cancellations
            .insert(operation_id.to_string(), cancelled.clone());

        let mode = self.mode;
        let turn_events = self.turn_events.clone();
        let transcript = self.transcript.clone();
        let operation_id_owned = operation_id.to_string();
        let request_id_owned = request_id.to_string();
        let assistant_entry_id_owned = assistant_entry_id.clone();

        let spawn = thread::Builder::new()
            .name(format!("fabushi-turn-{}", operation_id.chars().take(32).collect::<String>()))
            .spawn(move || {
                let provider = match mode {
                    AndroidHostMode::Test => {
                        AndroidHostInferenceProvider::new(AndroidInferenceMode::Test)
                    }
                    AndroidHostMode::Production => {
                        let Some(token) = bearer_token else {
                            push_turn_event(
                                &turn_events,
                                json!({
                                    "type":"operation.failed",
                                    "operationId":operation_id_owned,
                                    "requestId":request_id_owned,
                                    "message":"provider_credentials_unavailable",
                                }),
                            );
                            return;
                        };
                        match AndroidHostInferenceProvider::production(token, cancelled.clone()) {
                            Ok(provider) => provider,
                            Err(error) => {
                                push_turn_event(
                                    &turn_events,
                                    json!({
                                        "type":"operation.failed",
                                        "operationId":operation_id_owned,
                                        "requestId":request_id_owned,
                                        "message":error.message,
                                    }),
                                );
                                return;
                            }
                        }
                    }
                };

                let mut owner = ProductionTurnAgentOwner::new(provider);
                let mut final_text = String::new();
                let mut terminal_emitted = false;
                let mut sink = |event: ProductionTurnEvent| -> Result<(), String> {
                    if cancelled.load(Ordering::Acquire) {
                        return Err("cancelled".into());
                    }
                    match event {
                        ProductionTurnEvent::Retrying {
                            attempt,
                            delay_ms,
                            reason,
                        } => push_turn_event(
                            &turn_events,
                            json!({
                                "type":"turn.retrying",
                                "operationId":operation_id_owned,
                                "requestId":request_id_owned,
                                "attempt":attempt,
                                "delayMs":delay_ms,
                                "reason":reason,
                            }),
                        ),
                        ProductionTurnEvent::Delta(delta) => {
                            final_text.push_str(&delta);
                            push_turn_event(
                                &turn_events,
                                json!({
                                    "type":"chat.delta",
                                    "operationId":operation_id_owned,
                                    "requestId":request_id_owned,
                                    "delta":delta,
                                }),
                            );
                        }
                        ProductionTurnEvent::Completed {
                            finish_reason,
                            attempts,
                        } => {
                            if !final_text.is_empty() {
                                let persist = transcript
                                    .lock()
                                    .map_err(|_| "transcript lock poisoned".to_string())?
                                    .append_entry_if_absent(json!({
                                        "id":assistant_entry_id_owned,
                                        "kind":"message",
                                        "role":"assistant",
                                        "content":final_text.clone(),
                                        "operationId":operation_id_owned,
                                        "timestampMs":now_ms(),
                                    }))
                                    .map_err(|error| {
                                        format!("failed to persist assistant transcript entry: {error}")
                                    })?;
                                let _ = persist;
                                push_turn_event(
                                    &turn_events,
                                    json!({
                                        "type":"chat.message",
                                        "operationId":operation_id_owned,
                                        "requestId":request_id_owned,
                                        "role":"assistant",
                                        "text":final_text.clone(),
                                    }),
                                );
                            }
                            terminal_emitted = true;
                            push_turn_event(
                                &turn_events,
                                json!({
                                    "type":"operation.completed",
                                    "operationId":operation_id_owned,
                                    "requestId":request_id_owned,
                                    "finishReason":finish_reason,
                                    "attempts":attempts,
                                }),
                            );
                        }
                        ProductionTurnEvent::Failed { message } => {
                            terminal_emitted = true;
                            push_turn_event(
                                &turn_events,
                                json!({
                                    "type":"operation.failed",
                                    "operationId":operation_id_owned,
                                    "requestId":request_id_owned,
                                    "message":message,
                                }),
                            );
                        }
                        ProductionTurnEvent::Cancelled => {
                            terminal_emitted = true;
                            push_turn_event(
                                &turn_events,
                                json!({
                                    "type":"operation.interrupted",
                                    "operationId":operation_id_owned,
                                    "requestId":request_id_owned,
                                    "reason":"cancelled",
                                }),
                            );
                        }
                    }
                    Ok(())
                };

                let result = owner.run_with_event_sink(
                    ProductionTurnInput {
                        operation_id: operation_id_owned.clone(),
                        request_id: request_id_owned.clone(),
                        agent_id,
                        model,
                        prompt,
                        resume_checkpoint_available: false,
                    },
                    &mut sink,
                );

                if let Err(error) = result {
                    if !cancelled.load(Ordering::Acquire) && !terminal_emitted {
                        push_turn_event(
                            &turn_events,
                            json!({
                                "type":"operation.failed",
                                "operationId":operation_id_owned,
                                "requestId":request_id_owned,
                                "message":error.message,
                            }),
                        );
                    }
                }
            });

        if let Err(error) = spawn {
            self.turn_cancellations.remove(operation_id);
            self.active_operations.remove(operation_id);
            return Err(format!("failed to start turn worker: {error}"));
        }

        Ok(())
    }

    fn feature_receive(&mut self) -> Result<Value, String> {
        if let Some(event) = self.events.pop_front() {
            return Ok(event);
        }

        for wait in 0..=10 {
            let event = self
                .turn_events
                .lock()
                .map_err(|_| "turn event queue lock poisoned".to_string())?
                .pop_front();
            if let Some(event) = event {
                if matches!(
                    event.get("type").and_then(Value::as_str),
                    Some("operation.completed")
                        | Some("operation.failed")
                        | Some("operation.interrupted")
                ) {
                    if let Some(operation_id) =
                        event.get("operationId").and_then(Value::as_str)
                    {
                        self.active_operations.remove(operation_id);
                        self.turn_cancellations.remove(operation_id);
                    }
                }
                return Ok(event);
            }
            if wait < 10 && !self.active_operations.is_empty() {
                thread::sleep(Duration::from_millis(5));
            } else {
                break;
            }
        }

        Ok(json!({}))
    }

    fn finish_operation(&mut self, operation_id: &str) {
        self.active_operations.remove(operation_id);
        self.events.push_back(json!({
            "type":"operation.completed",
            "operationId":operation_id
        }));
    }

    fn feature_interrupt(&mut self, params: &Value) -> Result<Value, String> {
        let operation_id = required_string(params, "operationId")?.to_string();
        self.cancel_operation(&operation_id, Some("user"))?;
        Ok(json!({"operationId":operation_id,"status":"interrupted"}))
    }

    fn feature_approval_resolve(&mut self, params: &Value) -> Result<Value, String> {
        let approval_id = required_string(params, "approvalId")?.to_string();
        let approved = params
            .get("approved")
            .and_then(Value::as_bool)
            .ok_or("approved is required")?;
        let operation_id = self
            .pending_approvals
            .remove(&approval_id)
            .ok_or("approval is unknown, stale, cancelled, or already consumed")?;
        if !self.active_operations.contains(&operation_id) {
            return Err("approval operation is no longer active".into());
        }

        self.events.push_back(json!({
            "type":"approval.resolved",
            "approvalId":approval_id,
            "operationId":operation_id,
            "approved":approved,
        }));
        if approved {
            self.active_operations.remove(&operation_id);
            self.events.push_back(json!({
                "type":"operation.failed",
                "operationId":operation_id,
                "message":"capability_broker_execution_not_migrated",
                "outcome":"not-executed"
            }));
            Ok(json!({
                "status":"resolved",
                "approved":true,
                "operationId":operation_id,
                "execution":"not-executed",
                "reason":"capability_broker_execution_not_migrated"
            }))
        } else {
            self.active_operations.remove(&operation_id);
            self.events.push_back(json!({
                "type":"operation.interrupted",
                "operationId":operation_id,
                "reason":"approval-denied"
            }));
            Ok(json!({
                "status":"resolved",
                "approved":false,
                "operationId":operation_id,
                "execution":"not-executed"
            }))
        }
    }

    fn marketplace_browse(&mut self, _params: &Value) -> Result<Value, String> {
        if self.mode == AndroidHostMode::Test {
            return Ok(json!({"plugins":[{
                "pluginId":"global-dharma",
                "displayName":"全球法布施",
                "description":"Android deterministic Mini App",
                "latestVersion":"1.0.0",
                "commands":[]
            }]}));
        }
        let (result, mutation) = self.authenticated_platform_api(
            "GET",
            "/v1/marketplace/plugins?platform=android",
            None,
        )?;
        Ok(with_account_session_mutation(result, mutation))
    }

    fn marketplace_release(&mut self, params: &Value) -> Result<Value, String> {
        let plugin_id = required_string(params, "pluginId")?;
        let version = required_string(params, "version")?;
        let plugin_id = encode_api_path_segment(plugin_id)?;
        let version = encode_api_path_segment(version)?;
        if self.mode == AndroidHostMode::Test {
            let install = json!({
                "protocol":"fabushi.marketplace.install.v1",
                "strategy":"github-immutable",
                "source":{
                    "sourceRef":"test-fixture-only",
                    "marketplaceHostsPackage":false
                }
            });
            return Ok(json!({
                "pluginId":plugin_id,
                "version":version,
                "install":install,
                "releaseManifest":{
                    "pluginId":plugin_id,
                    "version":version,
                    "install":install
                }
            }));
        }
        let path = format!("/v1/marketplace/plugins/{plugin_id}/releases/{version}");
        let (result, mutation) = self.authenticated_platform_api("GET", &path, None)?;
        Ok(with_account_session_mutation(result, mutation))
    }

    fn platform_request(&mut self, params: &Value) -> Result<Value, String> {
        if params.get("authenticated").and_then(Value::as_bool) != Some(true) {
            return Err("platform.request requires the authenticated Fabushi account boundary".into());
        }
        let method = required_string(params, "method")?.to_ascii_uppercase();
        if !matches!(method.as_str(), "GET" | "POST") {
            return Err("platform.request supports only GET and POST".into());
        }
        let path = required_string(params, "path")?;
        validate_platform_api_path(path)?;
        let body = match params.get("body") {
            None | Some(Value::Null) => None,
            Some(value) if value.is_object() => Some(value.clone()),
            Some(_) => return Err("platform.request body must be a JSON object".into()),
        };
        if method == "GET" && body.is_some() {
            return Err("platform.request GET must not carry a request body".into());
        }
        let (result, mutation) = self.authenticated_platform_api(&method, path, body)?;
        Ok(with_account_session_mutation(
            json!({"ok":true,"data":result}),
            mutation,
        ))
    }

    fn authenticated_platform_api(
        &mut self,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> Result<(Value, Option<AccountSessionMutation>), String> {
        validate_platform_api_path(path)?;
        if self.mode == AndroidHostMode::Test {
            return Err("production platform API access is disabled in deterministic Host test mode".into());
        }
        #[cfg(feature = "ci-account-session-import")]
        if let Some(identity) = self.ci_session_identity.as_ref() {
            let result = self.account.api_request_with_bearer(
                method,
                path,
                body,
                &identity.access_token,
            )?;
            return Ok((result, None));
        }
        self.account.authenticated_api_request(method, path, body)
    }

    fn plugin_install(&mut self, params: &Value) -> Result<Value, String> {
        let release = params.get("release").ok_or("feature.plugin.install requires release")?;
        let plugin_id = release
            .get("pluginId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or("plugin release is missing pluginId")?
            .to_string();
        if self.mode != AndroidHostMode::Test {
            return Err(
                "feature.plugin.install is unavailable until the verified immutable package installer and canonical portable runtime are migrated; Android refuses in-memory placeholder installation"
                    .into(),
            );
        }
        self.installed_plugins.insert(plugin_id.clone());
        Ok(json!({
            "pluginId":plugin_id,
            "runtime":"deepseek-js",
            "requestedPermissions":[],
            "fixture":true
        }))
    }

    fn plugin_ui_document(&self, params: &Value) -> Result<Value, String> {
        let plugin_id = required_string(params, "pluginId")?;
        if self.mode != AndroidHostMode::Test {
            return Err(
                "feature.plugin.uiDocument is unavailable until the installed package UI document is verified and loaded from the canonical plugin store; Android refuses placeholder HTML"
                    .into(),
            );
        }
        if !self.installed_plugins.contains(plugin_id) {
            return Err("test plugin is not installed".into());
        }
        Ok(json!({
            "pluginId":plugin_id,
            "html":"<!doctype html><html><body><main id=\"app\">Fabushi Mini App test fixture</main></body></html>",
            "fixture":true
        }))
    }
}


fn with_account_session_mutation(
    mut result: Value,
    mutation: Option<AccountSessionMutation>,
) -> Value {
    if let Some(mutation) = mutation {
        if let Some(object) = result.as_object_mut() {
            object.insert(
                "_accountSessionMutation".into(),
                mutation.as_private_projection(),
            );
        }
    }
    result
}

fn push_turn_event(events: &Arc<Mutex<VecDeque<Value>>>, event: Value) {
    if let Ok(mut queue) = events.lock() {
        queue.push_back(event);
    }
}

impl Drop for AndroidJsonHost {
    fn drop(&mut self) {
        for cancelled in self.turn_cancellations.values() {
            cancelled.store(true, Ordering::Release);
        }
        self.turn_cancellations.clear();
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

fn webauthn_bridge_error_message(error: WebAuthnBridgeError) -> String {
    match error {
        WebAuthnBridgeError::NoProvider { message }
        | WebAuthnBridgeError::ProviderStale { message } => message.to_string(),
        WebAuthnBridgeError::DispatchFailed(message) => message,
        WebAuthnBridgeError::UnknownRequest => "unknown WebAuthn request".into(),
        WebAuthnBridgeError::TimedOut => "WebAuthn request timed out".into(),
    }
}

fn webauthn_request_frame_json(frame: WebAuthnRequestFrame) -> Value {
    match frame {
        WebAuthnRequestFrame::Welcome { provider_id } => {
            json!({"kind":"welcome","providerId":provider_id})
        }
        WebAuthnRequestFrame::Ceremony { request_id, ceremony } => json!({
            "kind":"ceremony",
            "requestId":request_id,
            "ceremony":{
                "kind":ceremony.kind,
                "origin":ceremony.origin,
                "payloadJson":ceremony.payload_json,
            }
        }),
        WebAuthnRequestFrame::Cancel { request_id } => {
            json!({"kind":"cancel","requestId":request_id})
        }
    }
}

fn parse_webauthn_response_frame(value: &Value) -> Result<WebAuthnResponseFrame, String> {
    let kind = required_string(value, "kind")?;
    match kind {
        "hello" => Ok(WebAuthnResponseFrame::Hello {
            computer_id: value.get("computerId").and_then(Value::as_str).map(str::to_string),
            label: value.get("label").and_then(Value::as_str).map(str::to_string),
        }),
        "ping" => Ok(WebAuthnResponseFrame::Ping),
        "stage" => {
            let request_id = required_string(value, "requestId")?.to_string();
            let stage = match required_string(value, "stage")? {
                "grant" => WebAuthnStage::Grant,
                "sign" => WebAuthnStage::Sign,
                _ => return Err("stage must be grant or sign".into()),
            };
            let outcome = match required_string(value, "outcome")? {
                "ok" => WebAuthnStageOutcome::Ok,
                "declined" => WebAuthnStageOutcome::Declined,
                "failed" => WebAuthnStageOutcome::Failed,
                _ => return Err("outcome must be ok, declined, or failed".into()),
            };
            Ok(WebAuthnResponseFrame::Stage {
                request_id,
                stage,
                outcome,
            })
        }
        "result" => Ok(WebAuthnResponseFrame::Result {
            request_id: required_string(value, "requestId")?.to_string(),
            credential_json: required_string(value, "credentialJson")?.to_string(),
        }),
        "error" => Ok(WebAuthnResponseFrame::Error {
            request_id: required_string(value, "requestId")?.to_string(),
            name: required_string(value, "name")?.to_string(),
            message: required_string(value, "message")?.to_string(),
            code: value.get("code").and_then(Value::as_str).map(str::to_string),
        }),
        _ => Err(format!("unsupported WebAuthn response frame kind {kind}")),
    }
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{key} is required"))
}

fn validate_platform_api_path(path: &str) -> Result<(), String> {
    if path.len() > 4_096
        || !path.starts_with("/v1/")
        || path.contains(['\r', '\n', '#', '\\'])
    {
        return Err("platform.request path must be a bounded /v1/* Fabushi API path".into());
    }
    let path_only = path.split('?').next().unwrap_or(path);
    if path_only
        .split('/')
        .any(|segment| matches!(segment, "." | ".."))
    {
        return Err("platform.request path traversal is forbidden".into());
    }
    let lower = path_only.to_ascii_lowercase();
    if lower.contains("%2e") || lower.contains("%2f") || lower.contains("%5c") {
        return Err("platform.request encoded path traversal is forbidden".into());
    }
    Ok(())
}

fn encode_api_path_segment(value: &str) -> Result<String, String> {
    if value.is_empty()
        || value.len() > 200
        || value.chars().any(|character| character.is_control())
    {
        return Err("Fabushi API path identifier is invalid".into());
    }
    Ok(url::form_urlencoded::byte_serialize(value.as_bytes()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_test_journey_covers_auth_stream_approval_and_interrupt() {
        let mut host = AndroidJsonHost::new("/tmp/fabushi-host-test", AndroidHostMode::Test);
        assert_eq!(host.dispatch("feature.info", &json!({})).unwrap()["platform"], "android");
        assert!(host.dispatch("feature.auth.providers", &json!({})).unwrap().as_array().unwrap().iter().any(|p| p["id"] == "google"));

        let oauth = host.dispatch("feature.auth.oauthStart", &json!({"provider":"google"})).unwrap();
        let completed = host.dispatch("feature.auth.oauthPoll", &json!({"attemptId":oauth["attemptId"]})).unwrap();
        assert_eq!(completed["status"], "completed");

        let accepted = host.dispatch("feature.execute", &json!({"command":{
            "type":"capability.request",
            "requestId":"capability-1",
            "capability":"camera"
        }})).unwrap();
        assert_eq!(accepted["requestId"], "capability-1");
        let mut saw_approval = false;
        for _ in 0..8 {
            let event = host.dispatch("feature.receive", &json!({})).unwrap();
            if event["type"] == "approval.requested" {
                saw_approval = true;
                break;
            }
        }
        assert!(saw_approval);
        let approval = host
            .dispatch(
                "feature.approval.resolve",
                &json!({"approvalId":format!("approval-{}", accepted["operationId"].as_str().unwrap()),"approved":true}),
            )
            .unwrap();
        assert_eq!(approval["execution"], "not-executed");
        assert!(host
            .dispatch(
                "feature.approval.resolve",
                &json!({"approvalId":format!("approval-{}", accepted["operationId"].as_str().unwrap()),"approved":true}),
            )
            .is_err());

        let long_task = host.dispatch("feature.execute", &json!({"command":{
            "type":"runtime.longTask",
            "requestId":"long-1"
        }})).unwrap();
        let operation_id = long_task["operationId"].as_str().unwrap();
        let interrupted = host.dispatch("feature.interrupt", &json!({"operationId":operation_id})).unwrap();
        assert_eq!(interrupted["status"], "interrupted");
    }

    #[test]
    fn webauthn_provider_transport_queues_ceremony_and_settles_result_once() {
        let mut host = AndroidJsonHost::new("/tmp/fabushi-host-webauthn", AndroidHostMode::Test);
        let registered = host
            .dispatch("feature.webauthn.registerProvider", &json!({}))
            .unwrap();
        let provider_id = registered["providerId"].as_str().unwrap().to_string();

        let welcome = host
            .dispatch(
                "feature.webauthn.pollRequest",
                &json!({"providerId":provider_id}),
            )
            .unwrap();
        assert_eq!(welcome["frame"]["kind"], "welcome");

        host.dispatch(
            "feature.webauthn.submitResponses",
            &json!({"providerId":provider_id,"frames":[{"kind":"ping"}]}),
        )
        .unwrap();

        let requested = host
            .dispatch(
                "feature.webauthn.requestCeremony",
                &json!({
                    "kind":"get",
                    "origin":"https://cursor.com",
                    "payloadJson":"{\"challenge\":\"abc\"}"
                }),
            )
            .unwrap();
        let request_id = requested["requestId"].as_str().unwrap().to_string();

        let ceremony = host
            .dispatch(
                "feature.webauthn.pollRequest",
                &json!({"providerId":provider_id}),
            )
            .unwrap();
        assert_eq!(ceremony["frame"]["kind"], "ceremony");
        assert_eq!(ceremony["frame"]["requestId"], request_id);

        let settled = host
            .dispatch(
                "feature.webauthn.submitResponses",
                &json!({
                    "providerId":provider_id,
                    "frames":[{
                        "kind":"result",
                        "requestId":request_id,
                        "credentialJson":"{\"id\":\"cred-1\"}"
                    }]
                }),
            )
            .unwrap();
        assert_eq!(settled["settlements"].as_array().unwrap().len(), 1);

        let duplicate = host
            .dispatch(
                "feature.webauthn.submitResponses",
                &json!({
                    "providerId":provider_id,
                    "frames":[{
                        "kind":"result",
                        "requestId":request_id,
                        "credentialJson":"{}"
                    }]
                }),
            )
            .unwrap();
        assert!(duplicate["settlements"].as_array().unwrap().is_empty());
    }

    #[test]
    fn production_unknown_methods_fail_closed() {
        let mut host = AndroidJsonHost::new("/tmp/fabushi-host-prod", AndroidHostMode::Production);
        assert!(host.dispatch("arbitrary.renderer.method", &json!({})).is_err());
    }
    #[test]
    fn canonical_agent_roster_drives_direct_and_bot_surfaces() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-json-host-roster-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let mut host = AndroidJsonHost::new(&root, AndroidHostMode::Test);

        let created = host.dispatch("createAgent", &json!({
            "name":"First Agent",
            "description":"one",
            "origin":"user"
        })).unwrap();
        let id = created["agent"]["id"].as_str().unwrap().to_string();

        host.dispatch("updateAgent", &json!({
            "id":id,
            "profile":{"name":"Renamed Agent","description":"two"}
        })).unwrap();
        host.dispatch("setAgentHiddenFromSidebar", &json!({"id":id,"isHidden":true})).unwrap();
        host.dispatch("setAgentUnread", &json!({"id":id,"isUnread":true})).unwrap();
        host.dispatch("setPinnedAgents", &json!({"ids":[id]})).unwrap();

        let list = host.dispatch("listAgents", &json!({})).unwrap();
        assert_eq!(list.as_array().unwrap().len(), 1);
        assert_eq!(list[0]["name"], "Renamed Agent");
        assert_eq!(list[0]["isHiddenFromSidebar"], true);
        assert_eq!(list[0]["hasUnread"], true);
        assert_eq!(list[0]["isPinned"], true);

        host.dispatch("feature.execute", &json!({"command":{
            "type":"bot.list",
            "requestId":"bot-list-1"
        }})).unwrap();
        let mut listed = None;
        for _ in 0..4 {
            let event = host.dispatch("feature.receive", &json!({})).unwrap();
            if event["type"] == "bot.listed" {
                listed = Some(event);
                break;
            }
        }
        let listed = listed.expect("bot.listed event");
        assert_eq!(listed["bots"][0]["id"], id);

        let duplicate = host.dispatch("duplicateAgent", &json!({"id":id})).unwrap();
        let duplicate_id = duplicate["agent"]["id"].as_str().unwrap().to_string();
        assert_ne!(duplicate_id, id);
        assert_eq!(host.dispatch("countAgents", &json!({})).unwrap(), 2);

        host.dispatch("deleteAgents", &json!({"ids":[id]})).unwrap();
        assert_eq!(host.dispatch("countAgents", &json!({})).unwrap(), 1);

        drop(host);
        let reopened = AndroidJsonHost::new(&root, AndroidHostMode::Test);
        let reopened_list = reopened.agents.list();
        assert_eq!(reopened_list.len(), 1);
        assert_eq!(reopened_list[0].id, duplicate_id);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn transcript_survives_host_reopen_and_dedupes_settled_send() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-json-host-transcript-{}-{}",
            std::process::id(),
            now_ms()
        ));

        {
            let mut host = AndroidJsonHost::new(&root, AndroidHostMode::Test);
            let accepted = host
                .dispatch(
                    "feature.execute",
                    &json!({"command":{
                        "type":"chat.send",
                        "requestId":"recover-request-1",
                        "text":"hello after restart",
                        "agentId":"mahayana-assistant"
                    }}),
                )
                .unwrap();
            assert_eq!(accepted["operationId"], "recover-request-1");

            let mut saw_delta = false;
            let mut completed = false;
            for _ in 0..16 {
                let event = host.dispatch("feature.receive", &json!({})).unwrap();
                if event["type"] == "chat.delta" {
                    saw_delta = true;
                }
                if event["type"] == "operation.completed" {
                    completed = true;
                    break;
                }
            }
            assert!(saw_delta);
            assert!(completed);
            let snapshot = host
                .dispatch("feature.transcript.snapshot", &json!({}))
                .unwrap();
            assert_eq!(snapshot.as_array().unwrap().len(), 2);
        }

        {
            let mut reopened = AndroidJsonHost::new(&root, AndroidHostMode::Test);
            let snapshot = reopened
                .dispatch("feature.transcript.snapshot", &json!({}))
                .unwrap();
            assert_eq!(snapshot.as_array().unwrap().len(), 2);

            reopened
                .dispatch(
                    "feature.execute",
                    &json!({"command":{
                        "type":"chat.send",
                        "requestId":"recover-request-1",
                        "text":"hello after restart",
                        "agentId":"mahayana-assistant"
                    }}),
                )
                .unwrap();

            let mut recovered = false;
            let mut deduped = false;
            for _ in 0..16 {
                let event = reopened.dispatch("feature.receive", &json!({})).unwrap();
                if event["type"] == "chat.message" && event["recovered"] == true {
                    recovered = true;
                }
                if event["type"] == "operation.completed" && event["deduped"] == true {
                    deduped = true;
                    break;
                }
            }
            assert!(recovered);
            assert!(deduped);
            let snapshot = reopened
                .dispatch("feature.transcript.snapshot", &json!({}))
                .unwrap();
            assert_eq!(snapshot.as_array().unwrap().len(), 2);
        }

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(feature = "ci-account-session-import")]
    #[test]
    fn ci_session_validation_is_bounded_and_refresh_token_free() {
        let now = 1_000_000_u64;
        let valid = json!({
            "accessToken":"abcdefghijklmnopqrstuvwxyz0123456789",
            "deviceId":"gha-12345-7-interactive",
            "sessionId":"ci-runner:12345:7",
            "tokenType":"Bearer",
            "provider":"github-actions",
            "ciRunner":true,
            "accessTokenExpiresAt":now + 3_600,
        });
        let parsed = ci_account_session::parse_ci_account_session_document(&valid, now).unwrap();
        assert_eq!(parsed.access_token, "abcdefghijklmnopqrstuvwxyz0123456789");
        assert_eq!(parsed.session_id, "ci-runner:12345:7");
        assert_eq!(parsed.device_id, "gha-12345-7-interactive");

        let mut mismatched = valid.clone();
        mismatched["sessionId"] = Value::String("ci-runner:12345:8".into());
        assert!(ci_account_session::parse_ci_account_session_document(&mismatched, now).is_none());

        let mut refresh = valid.clone();
        refresh["refreshToken"] = Value::String("forbidden".into());
        assert!(ci_account_session::parse_ci_account_session_document(&refresh, now).is_none());

        let mut expired = valid.clone();
        expired["accessTokenExpiresAt"] = json!(now + 10);
        assert!(ci_account_session::parse_ci_account_session_document(&expired, now).is_none());
    }

    #[cfg(feature = "ci-account-session-import")]
    #[test]
    fn device_agent_session_exposes_only_the_validated_short_lived_ci_session() {
        let now = 1_000_000_u64;
        let identity = ci_account_session::parse_ci_account_session_document(
            &json!({
                "accessToken":"abcdefghijklmnopqrstuvwxyz0123456789",
                "deviceId":"gha-12345-7-interactive",
                "sessionId":"ci-runner:12345:7",
                "tokenType":"Bearer",
                "provider":"github-actions",
                "ciRunner":true,
                "accessTokenExpiresAt":now + 3_600,
            }),
            now,
        )
        .unwrap();
        let root = std::env::temp_dir().join(format!(
            "fabushi-device-session-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let mut host = AndroidJsonHost::new(&root, AndroidHostMode::Test);
        host.logged_in = true;
        host.ci_session_identity = Some(identity);
        let session = host.device_agent_session();
        assert_eq!(session["loggedIn"], true);
        assert_eq!(session["available"], true);
        assert_eq!(session["deviceId"], "gha-12345-7-interactive");
        assert_eq!(session["sessionId"], "ci-runner:12345:7");
        assert_eq!(
            session["accessToken"],
            "abcdefghijklmnopqrstuvwxyz0123456789"
        );
        host.logged_in = false;
        assert_eq!(host.device_agent_session(), json!({"loggedIn":false}));
        let _ = std::fs::remove_dir_all(root);
    }


    #[test]
    fn production_platform_request_contract_rejects_auth_escape_and_fake_plugin_success() {
        assert!(validate_platform_api_path("/v1/marketplace/plugins?platform=android").is_ok());
        assert!(validate_platform_api_path("/api/auth/logout").is_err());
        assert!(validate_platform_api_path("/v1/../api/auth/logout").is_err());
        assert!(validate_platform_api_path("/v1/%2e%2e/api/auth/logout").is_err());

        let root = std::env::temp_dir().join(format!("fabushi-platform-contract-{}", now_ms()));
        let mut host = AndroidJsonHost::new(&root, AndroidHostMode::Test);
        for method in [
            "plugin.compatibility",
            "plugin.permission.grant",
            "plugin.permission.revoke",
            "runtime.start",
            "runtime.stop",
            "runtime.tools",
            "runtime.call",
        ] {
            let error = host.dispatch(method, &json!({"pluginId":"test"})).unwrap_err();
            assert!(error.contains("refuses placeholder success"));
        }
        let production_root = std::env::temp_dir().join(format!("fabushi-plugin-production-{}", now_ms()));
        let mut production_host = AndroidJsonHost::new(&production_root, AndroidHostMode::Production);
        assert!(production_host
            .dispatch(
                "feature.plugin.install",
                &json!({"release":{"pluginId":"global-dharma"}}),
            )
            .unwrap_err()
            .contains("refuses in-memory placeholder installation"));
        assert!(production_host
            .dispatch("feature.plugin.uiDocument", &json!({"pluginId":"global-dharma"}))
            .unwrap_err()
            .contains("refuses placeholder HTML"));
        let _ = std::fs::remove_dir_all(production_root);

        assert!(host
            .dispatch(
                "platform.request",
                &json!({
                    "authenticated":true,
                    "method":"POST",
                    "path":"/api/auth/logout",
                    "body":{}
                }),
            )
            .is_err());
        let _ = std::fs::remove_dir_all(root);
    }


}
