use crate::account_telemetry::{
    AccountTelemetryBuffer, AccountTelemetryProjection, SessionRefreshFailure, SessionReport,
    SessionSignoutCause,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use url::Url;

pub const DEFAULT_FABUSHI_API_BASE_URL: &str = "https://api.ombhrum.com";
const REFRESH_LEEWAY_MS: u64 = 60_000;
const MIN_POLL_MS: u64 = 250;
const MAX_POLL_MS: u64 = 5_000;
const MAX_TOKEN_BYTES: usize = 16 * 1024;
const MAX_SESSION_JSON_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FabushiAccountSession {
    pub access_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    pub access_token_expires_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token_expires_at: Option<u64>,
    pub session_id: String,
    pub device_id: String,
    pub username: String,
    pub user_id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ci_runner: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrowserAttempt {
    attempt_id: String,
    login_url: String,
    poll_secret: String,
    expires_at_ms: u64,
    poll_after_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountSessionMutation {
    Save(String),
    Clear,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AccountRefusalRevocation {
    pub status: Value,
    pub mutation: AccountSessionMutation,
    pub remote_revoke_error: Option<String>,
}

impl AccountSessionMutation {
    pub fn as_private_projection(&self) -> Value {
        match self {
            Self::Save(session_json) => json!({
                "action":"save",
                "sessionJson":session_json,
            }),
            Self::Clear => json!({"action":"clear"}),
        }
    }
}

pub struct AndroidAccountService {
    base_url: Url,
    device_id: String,
    session: Option<FabushiAccountSession>,
    browser_attempt: Option<BrowserAttempt>,
    agent: ureq::Agent,
    browser_attempt_store: Option<PathBuf>,
    session_telemetry: AccountTelemetryBuffer,
    refresh_consecutive_failures: u64,
    refresh_degraded_since_ms: Option<u64>,
}

impl AndroidAccountService {
    pub fn new(
        device_id: impl Into<String>,
        initial_session_json: Option<&str>,
    ) -> Result<Self, String> {
        let device_id = bounded_text(&Value::String(device_id.into()), 200)
            .ok_or("Android account device id is required")?;
        let base_url = normalize_api_base_url(DEFAULT_FABUSHI_API_BASE_URL)?;
        let session = initial_session_json
            .filter(|raw| !raw.trim().is_empty())
            .and_then(|raw| {
                if raw.len() > MAX_SESSION_JSON_BYTES {
                    return None;
                }
                serde_json::from_str::<Value>(raw)
                    .ok()
                    .and_then(|value| normalize_session(&value, false))
            })
            .filter(|session| session.device_id == device_id);
        Ok(Self {
            base_url,
            device_id,
            session,
            browser_attempt: None,
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(20))
                .timeout_read(Duration::from_secs(30))
                .timeout_write(Duration::from_secs(30))
                .redirects(0)
                .build(),
            browser_attempt_store: None,
            session_telemetry: AccountTelemetryBuffer::default(),
            refresh_consecutive_failures: 0,
            refresh_degraded_since_ms: None,
        })
    }

    pub fn with_persistent_browser_attempts(
        device_id: impl Into<String>,
        initial_session_json: Option<&str>,
        store_path: impl Into<PathBuf>,
    ) -> Result<Self, String> {
        let mut service = Self::new(device_id, initial_session_json)?;
        let store_path = store_path.into();
        service.browser_attempt_store = Some(store_path.clone());
        service.browser_attempt = load_browser_attempt(&store_path, &service.base_url)?;
        if service
            .browser_attempt
            .as_ref()
            .is_some_and(|attempt| now_ms() >= attempt.expires_at_ms)
        {
            service.browser_attempt = None;
            clear_browser_attempt(&store_path)?;
        }
        Ok(service)
    }

    #[cfg(test)]
    fn with_base_url(
        device_id: impl Into<String>,
        base_url: &str,
        initial_session_json: Option<&str>,
    ) -> Result<Self, String> {
        let mut service = Self::new(device_id, initial_session_json)?;
        service.base_url = normalize_api_base_url(base_url)?;
        Ok(service)
    }

    pub fn public_status(
        &mut self,
    ) -> Result<(Value, Option<AccountSessionMutation>), String> {
        match self.refresh_if_needed(now_ms()) {
            Ok(mutation) => Ok((self.status_projection(), mutation)),
            Err(error) => {
                // Match Desktop cursor-auth semantics: a refresh transport/protocol failure is
                // not itself proof that the durable credential was revoked. refresh_if_needed()
                // explicitly removes self.session only for terminal identity/revocation outcomes.
                //
                // When the session is still owned, keep the public identity projection logged in
                // and attach the refresh error as diagnostic context. Returning loggedIn=false
                // here would make the Android Coordinator advance the account epoch and tear down
                // unrelated account-scoped state during a transient outage. Bearer-requiring Host
                // operations still call refresh_if_needed()/valid_access_token() and fail closed.
                if self.session.is_some() {
                    let mut status = self.status_projection();
                    if let Some(object) = status.as_object_mut() {
                        object.insert("refreshErrorMessage".into(), json!(error));
                    }
                    return Ok((status, None));
                }
                Ok((
                    json!({
                        "loggedIn":false,
                        "user":Value::Null,
                        "errorMessage":error,
                    }),
                    Some(AccountSessionMutation::Clear),
                ))
            }
        }
    }

    pub fn valid_access_token(
        &mut self,
    ) -> Result<(String, Option<AccountSessionMutation>), String> {
        let mutation = self.refresh_if_needed(now_ms())?;
        let token = self
            .session
            .as_ref()
            .map(|session| session.access_token.clone())
            .ok_or("Sign in to Fabushi to continue.")?;
        Ok((token, mutation))
    }

    pub fn session_fence(&self) -> Option<String> {
        let session = self.session.as_ref()?;
        let material = json!({
            "userId": &session.user_id,
            "sessionId": &session.session_id,
            "deviceId": &session.device_id,
        });
        let encoded = serde_json::to_vec(&material).ok()?;
        Some(format!("session:{}", crate::sha256::sha256_hex(&encoded)))
    }

    pub fn auth_id(&self) -> Option<String> {
        let session = self.session.as_ref()?;
        match &session.user_id {
            Value::String(value) => {
                let value = value.trim();
                (!value.is_empty()).then(|| value.to_string())
            }
            Value::Number(value) => Some(value.to_string()),
            _ => None,
        }
    }

    pub fn account_storage_scope(&self) -> Option<String> {
        let session = self.session.as_ref()?;
        let material = json!({
            "userId": &session.user_id,
            "deviceId": &session.device_id,
        });
        let encoded = serde_json::to_vec(&material).ok()?;
        Some(format!("account-device:{}", crate::sha256::sha256_hex(&encoded)))
    }

    pub fn environment_descriptor_url(&self) -> String {
        self.base_url.to_string()
    }

    pub fn clear_local_session(&mut self) -> AccountSessionMutation {
        self.session = None;
        AccountSessionMutation::Clear
    }

    pub fn session_telemetry_snapshot(&self) -> Vec<AccountTelemetryProjection> {
        self.session_telemetry.snapshot()
    }

    pub fn drain_session_telemetry(&mut self) -> Vec<AccountTelemetryProjection> {
        self.session_telemetry.drain()
    }

    fn record_refresh_failure(&mut self, failure: SessionRefreshFailure, observed_at_ms: u64) {
        self.refresh_consecutive_failures = self.refresh_consecutive_failures.saturating_add(1);
        self.refresh_degraded_since_ms
            .get_or_insert(observed_at_ms);
        self.session_telemetry
            .report_session(SessionReport::RefreshFailed(failure));
    }

    fn record_refresh_recovered(&mut self, observed_at_ms: u64) {
        if self.refresh_consecutive_failures == 0 {
            return;
        }
        let degraded_ms = self
            .refresh_degraded_since_ms
            .map(|started_at| observed_at_ms.saturating_sub(started_at))
            .unwrap_or_default();
        self.session_telemetry
            .report_session(SessionReport::RefreshRecovered {
                consecutive_failures: self.refresh_consecutive_failures,
                degraded_ms,
            });
        self.refresh_consecutive_failures = 0;
        self.refresh_degraded_since_ms = None;
    }

    pub(crate) fn authenticated_api_request(
        &mut self,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> Result<(Value, Option<AccountSessionMutation>), String> {
        let (token, mutation) = self.valid_access_token()?;
        let value = self.request_json(path, method, body, Some(&token))?;
        Ok((value, mutation))
    }

    /// Authenticated request for device-bound Fabushi product APIs such as native messaging.
    ///
    /// The account service remains the only bearer-token owner. Callers cannot inject or cache a
    /// second credential; they only select the server path/body while this owner refreshes the
    /// session and binds the canonical Android device identity on the wire.
    pub(crate) fn authenticated_device_api_request(
        &mut self,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> Result<(Value, Option<AccountSessionMutation>), String> {
        let (token, mutation) = self.valid_access_token()?;
        let value = self.request_json_with_device_header(path, method, body, Some(&token))?;
        Ok((value, mutation))
    }

    /// Upload one bounded Human direct-message resource while keeping bearer ownership inside the
    /// canonical account service. The caller supplies canonical blob bytes but never sees a token.
    pub(crate) fn authenticated_device_upload_resource(
        &mut self,
        path: &str,
        file_name: &str,
        bytes: &[u8],
    ) -> Result<(Value, Option<AccountSessionMutation>), String> {
        const MAX_RESOURCE_BYTES: usize = 32 * 1024 * 1024;
        let file_name = file_name.trim();
        if file_name.is_empty()
            || file_name.len() > 255
            || file_name.chars().any(|ch| ch.is_control() || matches!(ch, '"' | '\\' | '/'))
        {
            return Err("Fabushi Human attachment name is invalid.".into());
        }
        if bytes.is_empty() || bytes.len() > MAX_RESOURCE_BYTES {
            return Err("Fabushi Human attachment size exceeds the backend limit.".into());
        }
        let (token, mutation) = self.valid_access_token()?;
        let url = self
            .base_url
            .join(path.trim_start_matches('/'))
            .map_err(|_| "Fabushi account endpoint is invalid.")?;
        if url.origin() != self.base_url.origin() {
            return Err("Fabushi account request escaped configured origin.".into());
        }
        if !valid_token(&token) {
            return Err("Fabushi account credential is invalid.".into());
        }

        let boundary = format!(
            "fabushi-android-{}",
            crate::sha256::sha256_hex(
                format!("{}:{}:{}", self.device_id, file_name, bytes.len()).as_bytes()
            )
        );
        let mut body = Vec::with_capacity(bytes.len().saturating_add(512));
        write!(
            body,
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
        )
        .map_err(|error| format!("Fabushi Human attachment multipart encoding failed: {error}"))?;
        body.extend_from_slice(bytes);
        write!(body, "\r\n--{boundary}--\r\n")
            .map_err(|error| format!("Fabushi Human attachment multipart encoding failed: {error}"))?;

        let response = self
            .agent
            .post(url.as_str())
            .set("Accept", "application/json")
            .set("Authorization", &format!("Bearer {token}"))
            .set("x-fabushi-device-id", &self.device_id)
            .set("Content-Type", &format!("multipart/form-data; boundary={boundary}"))
            .send_bytes(&body);
        let value = match response {
            Ok(response) => response
                .into_json::<Value>()
                .map_err(|_| "Fabushi Human attachment response is invalid JSON.".to_string())?,
            Err(ureq::Error::Status(status, _)) => {
                return Err(format!("Fabushi Human attachment upload failed ({status})."));
            }
            Err(ureq::Error::Transport(error)) => {
                return Err(format!(
                    "Fabushi Human attachment upload unavailable ({:?}).",
                    error.kind()
                ));
            }
        };
        Ok((value, mutation))
    }

    /// Download one bounded Human direct-message resource with exact-size validation. A short,
    /// oversized, or redirected response fails closed and is never exposed as canonical media.
    pub(crate) fn authenticated_device_download_resource(
        &mut self,
        resource_id: &str,
        expected_size: u64,
    ) -> Result<(Vec<u8>, Option<AccountSessionMutation>), String> {
        const MAX_RESOURCE_BYTES: u64 = 32 * 1024 * 1024;
        let resource_id = resource_id.trim();
        if resource_id.is_empty()
            || resource_id.len() > 128
            || resource_id.contains('/')
            || resource_id.contains('\\')
            || resource_id.chars().any(char::is_control)
        {
            return Err("Fabushi Human attachment resource id is invalid.".into());
        }
        if expected_size == 0 || expected_size > MAX_RESOURCE_BYTES {
            return Err("Fabushi Human attachment size exceeds the backend limit.".into());
        }
        let encoded: String =
            url::form_urlencoded::byte_serialize(resource_id.as_bytes()).collect();
        let path = format!("/api/social/message-resources/{encoded}");
        let (token, mutation) = self.valid_access_token()?;
        let url = self
            .base_url
            .join(path.trim_start_matches('/'))
            .map_err(|_| "Fabushi account endpoint is invalid.")?;
        if url.origin() != self.base_url.origin() {
            return Err("Fabushi account request escaped configured origin.".into());
        }
        if !valid_token(&token) {
            return Err("Fabushi account credential is invalid.".into());
        }
        let response = self
            .agent
            .get(url.as_str())
            .set("Accept", "application/octet-stream")
            .set("Authorization", &format!("Bearer {token}"))
            .set("x-fabushi-device-id", &self.device_id)
            .call();
        let response = match response {
            Ok(response) => response,
            Err(ureq::Error::Status(status, _)) => {
                return Err(format!("Fabushi Human attachment download failed ({status})."));
            }
            Err(ureq::Error::Transport(error)) => {
                return Err(format!(
                    "Fabushi Human attachment download unavailable ({:?}).",
                    error.kind()
                ));
            }
        };
        if let Some(length) = response.header("Content-Length").and_then(|v| v.parse::<u64>().ok()) {
            if length != expected_size || length > MAX_RESOURCE_BYTES {
                return Err(format!(
                    "Fabushi Human attachment download length mismatch: expected {expected_size}, received {length}."
                ));
            }
        }
        let mut bytes = Vec::with_capacity(expected_size as usize);
        let mut reader = response.into_reader();
        let mut remaining = MAX_RESOURCE_BYTES + 1;
        let mut chunk = [0_u8; 16 * 1024];
        while remaining > 0 {
            let capacity = usize::try_from(remaining.min(chunk.len() as u64))
                .unwrap_or(chunk.len());
            let read = reader
                .read(&mut chunk[..capacity])
                .map_err(|error| format!("Fabushi Human attachment body could not be read: {error}"))?;
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..read]);
            remaining = remaining.saturating_sub(read as u64);
        }
        if bytes.len() as u64 != expected_size || bytes.len() as u64 > MAX_RESOURCE_BYTES {
            return Err(format!(
                "Fabushi Human attachment body length mismatch: expected {expected_size}, received {}.",
                bytes.len()
            ));
        }
        Ok((bytes, mutation))
    }

    pub fn update_display_name(
        &mut self,
        display_name: &str,
    ) -> Result<(Value, Option<AccountSessionMutation>), String> {
        let display_name = display_name.trim();
        if display_name.is_empty() || display_name.len() > 200 {
            return Err("Fabushi account display name must contain 1-200 characters.".into());
        }

        let (token, refresh_mutation) = self.valid_access_token()?;
        let response = match self.request_json(
            "/api/auth/update-profile",
            "POST",
            Some(json!({"displayName":display_name})),
            Some(&token),
        ) {
            Ok(response) => response,
            Err(error) => {
                return Ok((
                    json!({"ok":false,"errorMessage":error}),
                    refresh_mutation,
                ));
            }
        };

        let Some(user) = response.get("user").and_then(Value::as_object).cloned() else {
            return Ok((
                json!({"ok":false,"errorMessage":"Fabushi profile update returned an invalid user."}),
                refresh_mutation,
            ));
        };
        let Some(session) = self.session.as_mut() else {
            return Ok((
                json!({"ok":false,"errorMessage":"Fabushi account session disappeared during profile update."}),
                refresh_mutation,
            ));
        };
        let remote_id = user
            .get("id")
            .or_else(|| user.get("userId"))
            .filter(|value| value.is_string() || value.is_number())
            .cloned()
            .unwrap_or_else(|| session.user_id.clone());
        if remote_id != session.user_id {
            return Ok((
                json!({"ok":false,"errorMessage":"Fabushi profile update changed account identity."}),
                refresh_mutation,
            ));
        }
        if let Some(username) = bounded_text(
            user.get("username")
                .or_else(|| user.get("email"))
                .unwrap_or(&Value::Null),
            320,
        ) {
            session.username = username;
        }
        session.user = Some(Value::Object(user));
        let serialized = serde_json::to_string(session)
            .map_err(|_| "failed to serialize updated Fabushi account session")?;
        let mutation = Some(AccountSessionMutation::Save(serialized));
        Ok((
            json!({"ok":true,"auth":self.status_projection()}),
            mutation,
        ))
    }

    pub(crate) fn api_request_with_bearer(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        bearer: &str,
    ) -> Result<Value, String> {
        self.request_json(path, method, body, Some(bearer))
    }

    pub fn browser_start(&mut self) -> Result<Value, String> {
        self.browser_start_for_provider(None)
    }

    pub fn browser_start_for_provider(
        &mut self,
        provider: Option<&str>,
    ) -> Result<Value, String> {
        if self
            .browser_attempt
            .as_ref()
            .is_some_and(|attempt| now_ms() >= attempt.expires_at_ms)
        {
            self.browser_attempt = None;
            self.persist_browser_attempt(None)?;
        }
        if self.browser_attempt.is_some() {
            return Err("Fabushi browser sign-in already has an active attempt.".into());
        }
        let mut request = json!({
            "deviceId": self.device_id,
            "platform": "android",
        });
        if let Some(provider) = provider {
            let provider = validate_oauth_provider(provider)?;
            request["provider"] = Value::String(provider.to_string());
        }
        let body = self.request_json(
            "/api/auth/browser/start",
            "POST",
            Some(request),
            None,
        )?;
        let attempt = parse_browser_attempt(&body, &self.base_url)?;
        self.persist_browser_attempt(Some(&attempt))?;
        let result = json!({
            "attemptId":attempt.attempt_id,
            "loginUrl":attempt.login_url,
            "status":"pending",
            "pollAfterMs":attempt.poll_after_ms,
            "expiresAt":attempt.expires_at_ms,
        });
        self.browser_attempt = Some(attempt);
        Ok(result)
    }

    pub fn browser_reopen(&self, attempt_id: &str) -> Result<Value, String> {
        let attempt = self
            .browser_attempt
            .as_ref()
            .filter(|attempt| attempt.attempt_id == attempt_id)
            .ok_or("browser login attempt is unknown")?;
        if now_ms() >= attempt.expires_at_ms {
            return Err("Fabushi browser sign-in expired.".into());
        }
        Ok(json!({
            "attemptId":attempt.attempt_id,
            "loginUrl":attempt.login_url,
            "status":"pending",
        }))
    }

    pub fn browser_cancel(&mut self, attempt_id: &str) -> Result<Value, String> {
        let attempt = self
            .browser_attempt
            .as_ref()
            .filter(|attempt| attempt.attempt_id == attempt_id)
            .cloned()
            .ok_or("browser login attempt is unknown")?;
        self.browser_attempt = None;
        self.persist_browser_attempt(None)?;
        let path = format!(
            "/api/auth/browser/attempts/{}/cancel",
            encode_path_segment(&attempt.attempt_id),
        );
        let _ = self.request_json(
            &path,
            "POST",
            Some(json!({"pollSecret":attempt.poll_secret})),
            None,
        );
        Ok(json!({"attemptId":attempt.attempt_id,"status":"cancelled"}))
    }

    pub fn browser_poll(
        &mut self,
        attempt_id: &str,
    ) -> Result<(Value, Option<AccountSessionMutation>), String> {
        let attempt = self
            .browser_attempt
            .as_ref()
            .filter(|attempt| attempt.attempt_id == attempt_id)
            .cloned()
            .ok_or("browser login attempt is unknown")?;
        if now_ms() >= attempt.expires_at_ms {
            self.browser_attempt = None;
            self.persist_browser_attempt(None)?;
            return Ok((json!({"attemptId":attempt_id,"status":"expired"}), None));
        }
        let path = format!(
            "/api/auth/browser/attempts/{}",
            encode_path_segment(&attempt.attempt_id),
        );
        let result = self.request_json(
            &path,
            "POST",
            Some(json!({"pollSecret":attempt.poll_secret})),
            None,
        )?;
        let state = bounded_text(
            result.get("status").unwrap_or(&Value::Null),
            80,
        ).ok_or("Fabushi browser sign-in poll response is invalid.")?;
        match state.as_str() {
            "completed" => {
                let raw_session = result
                    .get("session")
                    .ok_or("Fabushi browser sign-in completed without a session.")?;
                let session = normalize_session(raw_session, false)
                    .ok_or("Fabushi browser sign-in returned an invalid durable session.")?;
                let verified = self.resolve_identity(session)?;
                let serialized = serde_json::to_string(&verified)
                    .map_err(|_| "failed to serialize Fabushi account session")?;
                self.session = Some(verified);
                self.browser_attempt = None;
                self.persist_browser_attempt(None)?;
                Ok((
                    json!({
                        "attemptId":attempt_id,
                        "status":"completed",
                        "auth":self.status_projection(),
                    }),
                    Some(AccountSessionMutation::Save(serialized)),
                ))
            }
            "failed" | "expired" | "cancelled" => {
                self.browser_attempt = None;
                self.persist_browser_attempt(None)?;
                Ok((json!({"attemptId":attempt_id,"status":state}), None))
            }
            _ => Ok((
                json!({
                    "attemptId":attempt_id,
                    "status":"pending",
                    "pollAfterMs":attempt.poll_after_ms,
                }),
                None,
            )),
        }
    }



    fn persist_browser_attempt(&self, attempt: Option<&BrowserAttempt>) -> Result<(), String> {
        let Some(path) = self.browser_attempt_store.as_deref() else {
            return Ok(());
        };
        match attempt {
            Some(attempt) => persist_browser_attempt(path, attempt),
            None => clear_browser_attempt(path),
        }
    }

    pub fn logout(&mut self) -> Result<(Value, AccountSessionMutation), String> {
        let session = self.session.take();
        self.browser_attempt = None;
        self.persist_browser_attempt(None)?;
        if let Some(session) = session {
            if let Some(refresh_token) = session.refresh_token {
                let _ = self.request_json(
                    "/api/auth/logout",
                    "POST",
                    Some(json!({
                        "refreshToken":refresh_token,
                        "deviceId":session.device_id,
                    })),
                    Some(&session.access_token),
                );
            }
        }
        Ok((json!({"loggedIn":false,"user":Value::Null}), AccountSessionMutation::Clear))
    }

    pub fn revoke_for_account_refusal(&mut self) -> AccountRefusalRevocation {
        self.revoke_for_account_refusal_with(|service, session| {
            let Some(refresh_token) = session.refresh_token.as_deref() else { return Ok(()); };
            service.request_json(
                "/api/auth/logout",
                "POST",
                Some(json!({"refreshToken":refresh_token,"deviceId":session.device_id})),
                Some(&session.access_token),
            ).map(|_| ())
        })
    }

    fn revoke_for_account_refusal_with<F>(&mut self, revoke: F) -> AccountRefusalRevocation
    where F: FnOnce(&mut Self, &FabushiAccountSession) -> Result<(), String> {
        let rejected = self.session.take();
        self.browser_attempt = None;
        let mut errors = Vec::new();
        if let Err(error) = self.persist_browser_attempt(None) {
            errors.push(format!("failed to clear rejected browser attempt: {error}"));
        }
        if let Some(session) = rejected.as_ref() {
            if let Err(error) = revoke(self, session) {
                errors.push(format!("remote credential revocation was not confirmed: {error}"));
            }
        }
        AccountRefusalRevocation {
            status: json!({"loggedIn":false,"user":Value::Null}),
            mutation: AccountSessionMutation::Clear,
            remote_revoke_error: (!errors.is_empty()).then(|| errors.join("; ")),
        }
    }

    fn refresh_if_needed(
        &mut self,
        now_ms: u64,
    ) -> Result<Option<AccountSessionMutation>, String> {
        let Some(current) = self.session.clone() else {
            return Ok(None);
        };
        if current.access_token_expires_at > now_ms.saturating_add(REFRESH_LEEWAY_MS) {
            return Ok(None);
        }
        let Some(refresh_token) = current.refresh_token.clone() else {
            self.session = None;
            self.session_telemetry
                .report_session(SessionReport::SignedOut {
                    cause: SessionSignoutCause::SessionRevoked,
                    durable: false,
                });
            return Ok(Some(AccountSessionMutation::Clear));
        };
        let raw = match self.request_json(
            "/api/auth/refresh",
            "POST",
            Some(json!({
                "refreshToken":refresh_token,
                "deviceId":current.device_id,
            })),
            None,
        ) {
            Ok(raw) => raw,
            Err(error) => {
                self.record_refresh_failure(classify_refresh_failure(&error), now_ms);
                return Err(error);
            }
        };
        let (next, rotation_rescued) = match merge_refresh_rotation(&current, &raw) {
            Ok(value) => value,
            Err(error) => {
                self.record_refresh_failure(SessionRefreshFailure::BadPayload, now_ms);
                return Err(error);
            }
        };
        if next.device_id != current.device_id || next.user_id != current.user_id {
            self.session = None;
            self.session_telemetry
                .report_session(SessionReport::SignedOut {
                    cause: SessionSignoutCause::Unparseable,
                    durable: false,
                });
            return Err("Fabushi refreshed account session changed identity.".into());
        }
        let verified = match self.resolve_identity(next) {
            Ok(verified) => verified,
            Err(error) => {
                self.record_refresh_failure(classify_refresh_failure(&error), now_ms);
                return Err(error);
            }
        };
        if rotation_rescued {
            self.session_telemetry
                .report_session(SessionReport::RotationRescued);
        }
        self.record_refresh_recovered(now_ms);
        let serialized = serde_json::to_string(&verified)
            .map_err(|_| "failed to serialize refreshed Fabushi account session")?;
        self.session = Some(verified);
        Ok(Some(AccountSessionMutation::Save(serialized)))
    }

    fn resolve_identity(
        &self,
        mut session: FabushiAccountSession,
    ) -> Result<FabushiAccountSession, String> {
        let body = self.request_json(
            "/api/auth/user-info",
            "GET",
            None,
            Some(&session.access_token),
        )?;
        let user = body
            .as_object()
            .ok_or("Fabushi account identity response is invalid.")?;
        let remote_id = user
            .get("id")
            .filter(|value| value.is_string() || value.is_number())
            .cloned()
            .unwrap_or_else(|| session.user_id.clone());
        if remote_id != session.user_id {
            return Err("Fabushi account identity changed while settling the session.".into());
        }
        session.username = bounded_text(
            user.get("username")
                .or_else(|| user.get("email"))
                .unwrap_or(&Value::Null),
            320,
        )
        .unwrap_or(session.username);
        session.user = Some(Value::Object(user.clone()));
        Ok(session)
    }

    fn status_projection(&self) -> Value {
        let Some(session) = self.session.as_ref() else {
            return json!({"loggedIn":false,"user":Value::Null});
        };
        let user = session
            .user
            .as_ref()
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_else(Map::new);
        let email = bounded_text(user.get("email").unwrap_or(&Value::Null), 320)
            .or_else(|| session.username.contains('@').then(|| session.username.clone()));
        let display_name = ["displayName", "name", "username"]
            .iter()
            .find_map(|key| bounded_text(user.get(*key).unwrap_or(&Value::Null), 200))
            .unwrap_or_else(|| session.username.clone());
        json!({
            "loggedIn":true,
            "authId":session.user_id,
            "expiresAt":session.access_token_expires_at,
            "user":{
                "id":session.user_id,
                "nickname":display_name,
                "username":session.username,
                "email":email,
                "profilePictureUrl":user.get("profilePictureUrl")
                    .or_else(|| user.get("avatarUrl"))
                    .or_else(|| user.get("avatar"))
                    .cloned()
                    .unwrap_or(Value::Null),
            }
        })
    }

    fn request_json(
        &self,
        path: &str,
        method: &str,
        body: Option<Value>,
        bearer: Option<&str>,
    ) -> Result<Value, String> {
        self.request_json_with_headers(path, method, body, bearer, false)
    }

    fn request_json_with_device_header(
        &self,
        path: &str,
        method: &str,
        body: Option<Value>,
        bearer: Option<&str>,
    ) -> Result<Value, String> {
        self.request_json_with_headers(path, method, body, bearer, true)
    }

    fn request_json_with_headers(
        &self,
        path: &str,
        method: &str,
        body: Option<Value>,
        bearer: Option<&str>,
        bind_device: bool,
    ) -> Result<Value, String> {
        let url = self
            .base_url
            .join(path.trim_start_matches('/'))
            .map_err(|_| "Fabushi account endpoint is invalid.")?;
        if url.origin() != self.base_url.origin() {
            return Err("Fabushi account request escaped configured origin.".into());
        }
        let mut request = match method {
            "GET" => self.agent.get(url.as_str()),
            "POST" => self.agent.post(url.as_str()),
            _ => return Err("unsupported Fabushi account request method".into()),
        }
        .set("Accept", "application/json");
        if body.is_some() {
            request = request.set("Content-Type", "application/json");
        }
        if let Some(token) = bearer {
            if !valid_token(token) {
                return Err("Fabushi account credential is invalid.".into());
            }
            request = request.set("Authorization", &format!("Bearer {token}"));
        }
        if bind_device {
            request = request.set("x-fabushi-device-id", &self.device_id);
        }

        let response = match body {
            Some(body) => request.send_json(body),
            None => request.call(),
        };
        match response {
            Ok(response) => response
                .into_json::<Value>()
                .map_err(|_| "Fabushi account response is invalid JSON.".into()),
            Err(ureq::Error::Status(status, _)) => {
                Err(format!("Fabushi account request failed ({status})."))
            }
            Err(ureq::Error::Transport(error)) => {
                Err(format!("Fabushi account request unavailable ({:?}).", error.kind()))
            }
        }
    }
}

fn merge_refresh_rotation(
    current: &FabushiAccountSession,
    raw: &Value,
) -> Result<(FabushiAccountSession, bool), String> {
    let mut next = normalize_session(raw, true)
        .ok_or("Fabushi refreshed account session failed validation.")?;
    let rotation_rescued = next.refresh_token.is_none();
    if rotation_rescued {
        next.refresh_token = current.refresh_token.clone();
        next.refresh_token_expires_at = current.refresh_token_expires_at;
    } else if next.refresh_token_expires_at.is_none() {
        return Err("Fabushi refreshed account session omitted refresh expiry.".into());
    }
    if next.refresh_token.is_none() || next.refresh_token_expires_at.is_none() {
        return Err("Fabushi refreshed account session has no durable refresh credential.".into());
    }
    Ok((next, rotation_rescued))
}

fn classify_refresh_failure(error: &str) -> SessionRefreshFailure {
    if let Some(status) = error
        .strip_prefix("Fabushi account request failed (")
        .and_then(|value| value.strip_suffix(")."))
        .and_then(|value| value.parse::<u16>().ok())
    {
        return SessionRefreshFailure::HttpStatus(status);
    }
    if error.starts_with("Fabushi account request unavailable (") {
        return SessionRefreshFailure::Network(
            error
                .trim_start_matches("Fabushi account request unavailable (")
                .trim_end_matches(").")
                .to_string(),
        );
    }
    SessionRefreshFailure::BadPayload
}

pub fn normalize_session(value: &Value, allow_refreshless: bool) -> Option<FabushiAccountSession> {
    let object = value.as_object()?;
    let access_token = bounded_text(object.get("accessToken")?, MAX_TOKEN_BYTES)?;
    if !valid_token(&access_token) {
        return None;
    }
    let refresh_token = object
        .get("refreshToken")
        .and_then(|value| bounded_text(value, MAX_TOKEN_BYTES));
    let access_token_expires_at = normalize_expiry_ms(
        object
            .get("accessTokenExpiresAt")
            .or_else(|| object.get("expiresAtMs"))?,
    )?;
    let refresh_token_expires_at = object
        .get("refreshTokenExpiresAt")
        .and_then(normalize_expiry_ms);
    if !allow_refreshless && (refresh_token.is_none() || refresh_token_expires_at.is_none()) {
        return None;
    }
    let session_id = bounded_text(object.get("sessionId")?, 200)?;
    let device_id = bounded_text(object.get("deviceId")?, 200)?;
    let username = bounded_text(object.get("username")?, 320)?;
    let user_id = object
        .get("userId")
        .filter(|value| {
            value
                .as_str()
                .is_some_and(|value| !value.is_empty() && value.len() <= 200)
                || value.as_f64().is_some_and(f64::is_finite)
        })?
        .clone();
    Some(FabushiAccountSession {
        access_token,
        refresh_token,
        access_token_expires_at,
        refresh_token_expires_at,
        session_id,
        device_id,
        username,
        user_id,
        user: object.get("user").filter(|value| value.is_object()).cloned(),
        provider: object.get("provider").and_then(|value| bounded_text(value, 100)),
        ci_runner: object.get("ciRunner").and_then(Value::as_bool),
    })
}



fn validate_oauth_provider(provider: &str) -> Result<&str, String> {
    match provider.trim() {
        "google" => Ok("google"),
        "github" => Ok("github"),
        _ => Err("Unsupported Fabushi OAuth provider.".into()),
    }
}

fn persist_browser_attempt(path: &Path, attempt: &BrowserAttempt) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("failed to create OAuth attempt store: {error}"))?;
    }
    let payload = serde_json::to_vec(attempt)
        .map_err(|error| format!("failed to serialize OAuth attempt: {error}"))?;
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    let result = (|| -> Result<(), String> {
        let mut file = fs::File::create(&tmp)
            .map_err(|error| format!("failed to create OAuth attempt temporary file: {error}"))?;
        file.write_all(&payload)
            .map_err(|error| format!("failed to write OAuth attempt: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("failed to sync OAuth attempt: {error}"))?;
        drop(file);
        fs::rename(&tmp, path)
            .map_err(|error| format!("failed to commit OAuth attempt: {error}"))?;
        sync_parent_directory(path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn clear_browser_attempt(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => sync_parent_directory(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("failed to clear OAuth attempt: {error}")),
    }
}

fn sync_parent_directory(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("failed to sync OAuth attempt directory: {error}"))?;
    }
    Ok(())
}

fn load_browser_attempt(path: &Path, base_url: &Url) -> Result<Option<BrowserAttempt>, String> {
    let raw = match fs::read(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("failed to read OAuth attempt: {error}")),
    };
    if raw.len() > 64 * 1024 {
        clear_browser_attempt(path)?;
        return Ok(None);
    }
    let attempt: BrowserAttempt = match serde_json::from_slice(&raw) {
        Ok(attempt) => attempt,
        Err(_) => {
            clear_browser_attempt(path)?;
            return Ok(None);
        }
    };
    let trusted_login = Url::parse(&attempt.login_url).ok();
    if attempt.attempt_id.trim().is_empty()
        || attempt.poll_secret.trim().is_empty()
        || trusted_login
            .as_ref()
            .is_none_or(|url| url.origin() != base_url.origin())
    {
        clear_browser_attempt(path)?;
        return Ok(None);
    }
    Ok(Some(attempt))
}

fn parse_browser_attempt(value: &Value, base_url: &Url) -> Result<BrowserAttempt, String> {
    let object = value
        .as_object()
        .ok_or("Fabushi browser sign-in start response is invalid.")?;
    let attempt_id = bounded_text(
        object.get("attemptId").unwrap_or(&Value::Null),
        200,
    )
    .ok_or("Fabushi browser sign-in start response is incomplete.")?;
    let login_url = bounded_text(
        object.get("loginUrl").unwrap_or(&Value::Null),
        4_096,
    )
    .ok_or("Fabushi browser sign-in start response is incomplete.")?;
    let poll_secret = bounded_text(
        object.get("pollSecret").unwrap_or(&Value::Null),
        4_096,
    )
    .ok_or("Fabushi browser sign-in start response is incomplete.")?;
    let expires_at_ms = object
        .get("expiresAt")
        .and_then(normalize_expiry_ms)
        .ok_or("Fabushi browser sign-in start response is incomplete.")?;
    let poll_after_ms = object
        .get("pollAfterMs")
        .and_then(Value::as_u64)
        .unwrap_or(750)
        .clamp(MIN_POLL_MS, MAX_POLL_MS);
    let login = Url::parse(&login_url)
        .map_err(|_| "Fabushi browser sign-in returned an invalid login URL.")?;
    if login.origin() != base_url.origin() {
        return Err("Fabushi browser sign-in returned an untrusted login origin.".into());
    }
    Ok(BrowserAttempt {
        attempt_id,
        login_url,
        poll_secret,
        expires_at_ms,
        poll_after_ms,
    })
}

fn normalize_api_base_url(raw: &str) -> Result<Url, String> {
    let mut url = Url::parse(raw.trim())
        .map_err(|_| "Fabushi account API base URL is invalid.")?;
    let loopback = matches!(
        url.host_str(),
        Some("localhost" | "127.0.0.1" | "::1")
    );
    if url.scheme() != "https" && !(loopback && url.scheme() == "http") {
        return Err("Fabushi account API must use HTTPS outside loopback development.".into());
    }
    if !url.username().is_empty() || url.password().is_some() || url.query().is_some() || url.fragment().is_some() {
        return Err("Fabushi account API base URL must not contain credentials, query, or fragment.".into());
    }
    url.set_path("/");
    Ok(url)
}

fn bounded_text(value: &Value, max: usize) -> Option<String> {
    let value = value.as_str()?.trim();
    (!value.is_empty() && value.len() <= max).then(|| value.to_string())
}

fn normalize_expiry_ms(value: &Value) -> Option<u64> {
    let numeric = value.as_u64()?;
    (numeric > 0).then(|| if numeric < 10_000_000_000 { numeric.saturating_mul(1_000) } else { numeric })
}

fn valid_token(value: &str) -> bool {
    (24..=MAX_TOKEN_BYTES).contains(&value.len())
        && !value.chars().any(char::is_whitespace)
        && !value.contains(['\r', '\n'])
}

fn encode_path_segment(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

fn now_ms() -> u64 {
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

    fn durable_session() -> Value {
        json!({
            "accessToken":"abcdefghijklmnopqrstuvwxyz0123456789",
            "refreshToken":"refresh-abcdefghijklmnopqrstuvwxyz",
            "accessTokenExpiresAt":2_000_000_000_000_u64,
            "refreshTokenExpiresAt":2_100_000_000_000_u64,
            "sessionId":"session-1",
            "deviceId":"device-1",
            "username":"user@example.com",
            "userId":"user-1",
        })
    }



    #[test]
    fn device_bound_authenticated_request_uses_single_account_token_and_machine_identity() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = [0_u8; 4096];
            let read = stream.read(&mut bytes).unwrap();
            let request = String::from_utf8_lossy(&bytes[..read]).to_ascii_lowercase();
            assert!(request.starts_with("get /api/social/friends "));
            assert!(request.contains("authorization: bearer abcdefghijklmnopqrstuvwxyz0123456789"));
            assert!(request.contains("x-fabushi-device-id: device-1"));
            let body = r#"{"ok":true}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body,
            )
            .unwrap();
            stream.flush().unwrap();
        });

        let raw = durable_session().to_string();
        let mut service = AndroidAccountService::with_base_url(
            "device-1",
            &format!("http://{address}"),
            Some(&raw),
        )
        .unwrap();
        let (value, mutation) = service
            .authenticated_device_api_request("GET", "/api/social/friends", None)
            .unwrap();
        assert_eq!(value, json!({"ok":true}));
        assert!(mutation.is_none());
        server.join().unwrap();
    }

    #[test]
    fn device_bound_human_resource_transport_preserves_binary_contract() {
        use std::io::{Read as _, Write as _};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            // Upload.
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                .unwrap();
            let mut raw = Vec::new();
            let mut buffer = [0_u8; 4096];
            while raw.len() < 64 * 1024 {
                let read = stream.read(&mut buffer).unwrap();
                if read == 0 {
                    break;
                }
                raw.extend_from_slice(&buffer[..read]);
                let request = String::from_utf8_lossy(&raw);
                let has_disposition =
                    request.contains("name=\"file\"; filename=\"hello.txt\"");
                let has_body = raw.windows(b"hello".len()).any(|window| window == b"hello");
                if has_disposition && has_body {
                    break;
                }
            }
            let request = String::from_utf8_lossy(&raw).to_string();
            let request_lower = request.to_ascii_lowercase();
            assert!(request.starts_with("POST /api/social/message-resources "));
            assert!(request_lower
                .contains("authorization: bearer abcdefghijklmnopqrstuvwxyz0123456789"));
            assert!(request_lower.contains("x-fabushi-device-id: device-1"));
            assert!(request.contains("name=\"file\"; filename=\"hello.txt\""));
            assert!(raw.windows(b"hello".len()).any(|window| window == b"hello"));
            let body = r#"{"success":true,"resource":{"id":"resource-1","name":"hello.txt","size":5}}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body,
            )
            .unwrap();
            stream.flush().unwrap();

            // Download.
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = [0_u8; 4096];
            let read = stream.read(&mut bytes).unwrap();
            let request = String::from_utf8_lossy(&bytes[..read]).to_string();
            let request_lower = request.to_ascii_lowercase();
            assert!(request.starts_with("GET /api/social/message-resources/resource-1 "));
            assert!(request_lower.contains("accept: application/octet-stream"));
            assert!(request_lower.contains("x-fabushi-device-id: device-1"));
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello",
            )
            .unwrap();
            stream.flush().unwrap();
        });

        let raw = durable_session().to_string();
        let mut service = AndroidAccountService::with_base_url(
            "device-1",
            &format!("http://{address}"),
            Some(&raw),
        )
        .unwrap();
        let (uploaded, mutation) = service
            .authenticated_device_upload_resource(
                "/api/social/message-resources",
                "hello.txt",
                b"hello",
            )
            .unwrap();
        assert!(mutation.is_none());
        assert_eq!(uploaded["resource"]["id"], "resource-1");
        let (downloaded, mutation) = service
            .authenticated_device_download_resource("resource-1", 5)
            .unwrap();
        assert!(mutation.is_none());
        assert_eq!(downloaded, b"hello");
        server.join().unwrap();
    }

    #[test]
    fn human_resource_transport_rejects_unsafe_identifiers_before_network_dispatch() {
        let raw = durable_session().to_string();
        let mut service = AndroidAccountService::new("device-1", Some(&raw)).unwrap();
        assert!(service
            .authenticated_device_upload_resource(
                "/api/social/message-resources",
                "../secret",
                b"hello",
            )
            .is_err());
        assert!(service
            .authenticated_device_upload_resource(
                "/api/social/message-resources",
                "hello.txt",
                &[],
            )
            .is_err());
        assert!(service
            .authenticated_device_download_resource("../secret", 5)
            .is_err());
        assert!(service
            .authenticated_device_download_resource("resource-1", 0)
            .is_err());
    }

    #[test]
    fn canonical_auth_id_projection_accepts_string_and_number_ids() {
        let raw = durable_session().to_string();
        let service = AndroidAccountService::new("device-1", Some(&raw)).unwrap();
        assert_eq!(service.auth_id().as_deref(), Some("user-1"));

        let mut numeric = durable_session();
        numeric["userId"] = json!(42);
        let service = AndroidAccountService::new("device-1", Some(&numeric.to_string())).unwrap();
        assert_eq!(service.auth_id().as_deref(), Some("42"));
    }

    #[test]
    fn profile_display_name_validation_is_bounded_before_network_dispatch() {
        let raw = durable_session().to_string();
        let mut service = AndroidAccountService::new("device-1", Some(&raw)).unwrap();
        assert!(service.update_display_name("").is_err());
        assert!(service.update_display_name(&"x".repeat(201)).is_err());
    }

    #[test]
    fn oauth_provider_allowlist_rejects_untrusted_provider_names() {
        assert_eq!(validate_oauth_provider("google").unwrap(), "google");
        assert_eq!(validate_oauth_provider("github").unwrap(), "github");
        assert!(validate_oauth_provider("oidc").is_err());
        assert!(validate_oauth_provider("../google").is_err());
    }

    #[test]
    fn browser_attempt_store_survives_reopen_and_rejects_foreign_origin() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-account-attempt-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let path = root.join("attempt.json");
        let base = normalize_api_base_url(DEFAULT_FABUSHI_API_BASE_URL).unwrap();
        let attempt = BrowserAttempt {
            attempt_id: "attempt-1".into(),
            login_url: "https://api.ombhrum.com/sign-in?attempt=attempt-1".into(),
            poll_secret: "poll-secret-1".into(),
            expires_at_ms: now_ms().saturating_add(60_000),
            poll_after_ms: 750,
        };
        persist_browser_attempt(&path, &attempt).unwrap();
        assert_eq!(load_browser_attempt(&path, &base).unwrap(), Some(attempt.clone()));

        let foreign = BrowserAttempt {
            login_url: "https://evil.example/sign-in".into(),
            ..attempt
        };
        persist_browser_attempt(&path, &foreign).unwrap();
        assert_eq!(load_browser_attempt(&path, &base).unwrap(), None);
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn clearing_browser_attempt_is_durable_and_leaves_no_temporary_identity() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("attempt.json");
        let attempt = BrowserAttempt {
            attempt_id: "attempt-durable-clear".into(),
            login_url: "https://api.ombhrum.com/sign-in?attempt=durable-clear".into(),
            poll_secret: "poll-secret-durable-clear".into(),
            expires_at_ms: now_ms().saturating_add(60_000),
            poll_after_ms: 750,
        };

        persist_browser_attempt(&path, &attempt).unwrap();
        assert_eq!(
            load_browser_attempt(
                &path,
                &normalize_api_base_url(DEFAULT_FABUSHI_API_BASE_URL).unwrap(),
            )
            .unwrap(),
            Some(attempt),
        );
        assert!(root
            .path()
            .read_dir()
            .unwrap()
            .all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("tmp-")));

        clear_browser_attempt(&path).unwrap();
        assert!(!path.exists());
        assert_eq!(
            load_browser_attempt(
                &path,
                &normalize_api_base_url(DEFAULT_FABUSHI_API_BASE_URL).unwrap(),
            )
            .unwrap(),
            None,
        );
    }

    #[test]
    fn persistent_service_drops_expired_attempt_on_cold_start() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-account-expired-attempt-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let path = root.join("attempt.json");
        let attempt = BrowserAttempt {
            attempt_id: "attempt-expired".into(),
            login_url: "https://api.ombhrum.com/sign-in?attempt=attempt-expired".into(),
            poll_secret: "poll-secret-expired".into(),
            expires_at_ms: now_ms().saturating_sub(1),
            poll_after_ms: 750,
        };
        persist_browser_attempt(&path, &attempt).unwrap();
        let service = AndroidAccountService::with_persistent_browser_attempts(
            "device-1",
            None,
            &path,
        )
        .unwrap();
        assert!(service.browser_attempt.is_none());
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn refresh_rotation_rescue_preserves_existing_refresh_credential() {
        let current = normalize_session(&durable_session(), false).unwrap();
        let mut raw = durable_session();
        raw["accessToken"] = Value::String("refreshed-abcdefghijklmnopqrstuvwxyz0123456789".into());
        raw.as_object_mut().unwrap().remove("refreshToken");
        raw.as_object_mut().unwrap().remove("refreshTokenExpiresAt");

        let (next, rescued) = merge_refresh_rotation(&current, &raw).unwrap();
        assert!(rescued);
        assert_eq!(next.refresh_token, current.refresh_token);
        assert_eq!(next.refresh_token_expires_at, current.refresh_token_expires_at);
    }

    #[test]
    fn transient_refresh_failure_preserves_durable_session_and_avoids_clear_mutation() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = [0_u8; 4096];
            let read = stream.read(&mut bytes).unwrap();
            let request = String::from_utf8_lossy(&bytes[..read]);
            assert!(request.starts_with("POST /api/auth/refresh "));
            let body = r#"{"error":"temporarily unavailable"}"#;
            write!(
                stream,
                "HTTP/1.1 503 Service Unavailable\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body,
            )
            .unwrap();
            stream.flush().unwrap();
        });

        let mut session = durable_session();
        session["accessTokenExpiresAt"] = json!(1_u64);
        let mut service = AndroidAccountService::with_base_url(
            "device-1",
            &format!("http://{address}"),
            Some(&session.to_string()),
        )
        .unwrap();
        let before_fence = service.session_fence();

        let (status, mutation) = service.public_status().unwrap();

        assert_eq!(status["loggedIn"], true);
        assert!(status["refreshErrorMessage"]
            .as_str()
            .is_some_and(|value| value.contains("503")));
        assert_eq!(mutation, None);
        assert_eq!(service.session_fence(), before_fence);
        assert!(service.session.is_some());

        server.join().unwrap();
    }

    #[test]
    fn refresh_telemetry_tracks_failure_recovery_and_missing_refresh_signout() {
        let raw = durable_session().to_string();
        let mut service = AndroidAccountService::new("device-1", Some(&raw)).unwrap();
        service.record_refresh_failure(SessionRefreshFailure::HttpStatus(503), 1_000);
        service.record_refresh_recovered(7_000);
        let snapshot = service.session_telemetry_snapshot();
        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0].metadata["phase"], "refresh_failed");
        assert_eq!(snapshot[0].metadata["http_status"], "503");
        assert_eq!(snapshot[1].metadata["phase"], "refresh_recovered");
        assert_eq!(snapshot[1].metadata["consecutive_failures"], "1");
        assert_eq!(snapshot[1].metadata["degraded_ms"], "30000");

        let session = service.session.as_mut().unwrap();
        session.access_token_expires_at = 1;
        session.refresh_token = None;
        session.refresh_token_expires_at = None;
        let mutation = service.refresh_if_needed(100_000).unwrap();
        assert_eq!(mutation, Some(AccountSessionMutation::Clear));
        let snapshot = service.session_telemetry_snapshot();
        assert_eq!(snapshot.last().unwrap().metadata["phase"], "signed_out");
        assert_eq!(snapshot.last().unwrap().metadata["cause"], "session_revoked");
        assert_eq!(snapshot.last().unwrap().metadata["durable"], "false");
    }

    #[test]
    fn refresh_failure_classifier_preserves_http_network_and_bad_payload_taxonomy() {
        assert_eq!(
            classify_refresh_failure("Fabushi account request failed (429)."),
            SessionRefreshFailure::HttpStatus(429)
        );
        assert!(matches!(
            classify_refresh_failure("Fabushi account request unavailable (ConnectionFailed)."),
            SessionRefreshFailure::Network(_)
        ));
        assert_eq!(
            classify_refresh_failure("Fabushi account response is invalid JSON."),
            SessionRefreshFailure::BadPayload
        );
    }

    #[test]
    fn durable_sessions_require_refresh_credentials_but_ci_shape_can_be_refreshless() {
        let session = durable_session();
        assert!(normalize_session(&session, false).is_some());
        let mut refreshless = session.clone();
        refreshless.as_object_mut().unwrap().remove("refreshToken");
        refreshless.as_object_mut().unwrap().remove("refreshTokenExpiresAt");
        assert!(normalize_session(&refreshless, false).is_none());
        assert!(normalize_session(&refreshless, true).is_some());
    }

    #[test]
    fn browser_attempt_rejects_foreign_origin_and_clamps_poll_interval() {
        let base = normalize_api_base_url(DEFAULT_FABUSHI_API_BASE_URL).unwrap();
        let good = json!({
            "attemptId":"attempt-1",
            "loginUrl":"https://api.ombhrum.com/sign-in?attempt=1",
            "pollSecret":"poll-secret",
            "expiresAt":2_000_000_000_000_u64,
            "pollAfterMs":99_999,
        });
        assert_eq!(parse_browser_attempt(&good, &base).unwrap().poll_after_ms, MAX_POLL_MS);

        let mut bad = good;
        bad["loginUrl"] = Value::String("https://evil.example/sign-in".into());
        assert!(parse_browser_attempt(&bad, &base).is_err());
    }

    #[test]
    fn base_url_fails_closed_outside_https_or_loopback() {
        assert!(normalize_api_base_url("http://api.ombhrum.com").is_err());
        assert!(normalize_api_base_url("http://127.0.0.1:3000").is_ok());
        assert!(normalize_api_base_url("https://user:pass@api.ombhrum.com").is_err());
        assert!(normalize_api_base_url("https://api.ombhrum.com?x=1").is_err());
    }

    #[test]
    fn initial_session_is_bounded_and_validated_before_becoming_runtime_truth() {
        let raw = durable_session().to_string();
        let service = AndroidAccountService::with_base_url("device-1", "https://api.ombhrum.com", Some(&raw)).unwrap();
        assert!(service.session.is_some());
        let invalid = json!({"accessToken":"short"}).to_string();
        let service = AndroidAccountService::new("device-1", Some(&invalid)).unwrap();
        assert!(service.session.is_none());
    }
    #[test]
    fn account_refusal_revocation_clears_local_truth_when_remote_revoke_fails() {
        let raw = durable_session().to_string();
        let mut service = AndroidAccountService::new("device-1", Some(&raw)).unwrap();
        assert!(service.session_fence().is_some());
        let result = service.revoke_for_account_refusal_with(|_, session| {
            assert_eq!(session.device_id, "device-1");
            Err("network unavailable".into())
        });
        assert!(service.session_fence().is_none());
        assert_eq!(result.mutation, AccountSessionMutation::Clear);
        assert_eq!(result.status, json!({"loggedIn":false,"user":Value::Null}));
        assert!(result.remote_revoke_error.as_deref().unwrap().contains("not confirmed"));
    }

    #[test]
    fn account_refusal_revocation_reports_clean_remote_settlement() {
        let raw = durable_session().to_string();
        let mut service = AndroidAccountService::new("device-1", Some(&raw)).unwrap();
        let result = service.revoke_for_account_refusal_with(|_, _| Ok(()));
        assert!(service.session_fence().is_none());
        assert_eq!(result.mutation, AccountSessionMutation::Clear);
        assert!(result.remote_revoke_error.is_none());
    }

}
