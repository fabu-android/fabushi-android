use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
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

#[derive(Clone, Debug, PartialEq, Eq)]
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
        })
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
                self.session = None;
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
        let body = self.request_json(
            "/api/auth/browser/start",
            "POST",
            Some(json!({
                "deviceId": self.device_id,
                "platform": "android",
            })),
            None,
        )?;
        let attempt = parse_browser_attempt(&body, &self.base_url)?;
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

    pub fn logout(&mut self) -> Result<(Value, AccountSessionMutation), String> {
        let session = self.session.take();
        self.browser_attempt = None;
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
            return Ok(Some(AccountSessionMutation::Clear));
        };
        let raw = self.request_json(
            "/api/auth/refresh",
            "POST",
            Some(json!({
                "refreshToken":refresh_token,
                "deviceId":current.device_id,
            })),
            None,
        )?;
        let next = normalize_session(&raw, false)
            .ok_or("Fabushi refreshed account session failed validation.")?;
        if next.device_id != current.device_id || next.user_id != current.user_id {
            self.session = None;
            return Err("Fabushi refreshed account session changed identity.".into());
        }
        let verified = self.resolve_identity(next)?;
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
}
