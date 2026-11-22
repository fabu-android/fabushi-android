use crate::presets::ProtocolKind;
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine as _;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Local AI CLI / desktop subscription families supported by Magpie discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionKind {
    GoogleVertex,
    CodexChatGpt,
    ClaudeCode,
    GitHubCopilot,
    GeminiCodeAssist,
    Antigravity,
    GrokSuperGrok,
}

impl SubscriptionKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GoogleVertex => "google_vertex",
            Self::CodexChatGpt => "codex_chatgpt",
            Self::ClaudeCode => "claude_code",
            Self::GitHubCopilot => "github_copilot",
            Self::GeminiCodeAssist => "gemini_code_assist",
            Self::Antigravity => "antigravity",
            Self::GrokSuperGrok => "grok_supergrok",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::GoogleVertex => "Google Vertex AI ADC",
            Self::CodexChatGpt => "Codex / ChatGPT Plus & Pro",
            Self::ClaudeCode => "Claude Code Subscription",
            Self::GitHubCopilot => "GitHub Copilot",
            Self::GeminiCodeAssist => "Gemini Code Assist",
            Self::Antigravity => "Antigravity OAuth",
            Self::GrokSuperGrok => "Grok / SuperGrok",
        }
    }
}

pub fn is_sensitive_header_name(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase();
    lower == "authorization"
        || lower == "proxy-authorization"
        || lower == "x-api-key"
        || lower == "x-goog-api-key"
        || lower == "api-key"
        || lower == "x-access-key"
        || lower == "x-auth"
        || lower == "x-authorization"
        || lower.starts_with("x-auth-")
        || lower.starts_with("x-auth_")
        || lower.contains("api-key")
        || lower.contains("api_key")
        || lower.contains("access-key")
        || lower.contains("access_key")
        || lower.contains("token")
        || lower.contains("secret")
        || lower.contains("password")
        || lower.contains("credential")
        || lower.contains("cookie")
}

pub fn redact_header_value(name: &str, value: &str) -> String {
    if is_sensitive_header_name(name) {
        let trimmed = value.trim();
        if trimmed
            .get(..7)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("bearer "))
        {
            "Bearer ***".to_string()
        } else {
            "***".to_string()
        }
    } else {
        value.to_string()
    }
}

fn serialize_redacted_headers<S>(
    headers: &BTreeMap<String, String>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let redacted: BTreeMap<&String, String> = headers
        .iter()
        .map(|(k, v)| (k, redact_header_value(k, v)))
        .collect();
    redacted.serialize(serializer)
}

/// Discovered local subscription credential summary (read-only metadata; secrets omitted from JSON).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionAccount {
    pub id: String,
    pub kind: SubscriptionKind,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    pub active: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    pub protocol: ProtocolKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    pub source_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_ms: Option<i64>,
    #[serde(skip_serializing, default)]
    pub access_token: Option<String>,
    #[serde(default, serialize_with = "serialize_redacted_headers")]
    pub headers: BTreeMap<String, String>,
}

impl std::fmt::Debug for SubscriptionAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let redacted_headers: BTreeMap<&String, String> = self
            .headers
            .iter()
            .map(|(k, v)| (k, redact_header_value(k, v)))
            .collect();
        let redacted_token = self.access_token.as_ref().map(|_| "***");
        f.debug_struct("SubscriptionAccount")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("label", &self.label)
            .field("email", &self.email)
            .field("account_id", &self.account_id)
            .field("plan", &self.plan)
            .field("active", &self.active)
            .field("base_url", &self.base_url)
            .field("protocol", &self.protocol)
            .field("default_model", &self.default_model)
            .field("source_path", &self.source_path)
            .field("expires_at_ms", &self.expires_at_ms)
            .field("access_token", &redacted_token)
            .field("headers", &redacted_headers)
            .finish()
    }
}

/// Snapshot of local subscription accounts discovered on this host.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionRegistry {
    #[serde(default)]
    pub accounts: Vec<SubscriptionAccount>,
    #[serde(default)]
    pub scanned_at_ms: i64,
}

impl SubscriptionRegistry {
    pub fn has_active_subscription(&self) -> bool {
        self.accounts.iter().any(|a| a.active)
    }

    pub fn active_accounts(&self) -> Vec<&SubscriptionAccount> {
        self.accounts.iter().filter(|a| a.active).collect()
    }

    pub fn find(&self, id: &str) -> Option<&SubscriptionAccount> {
        self.accounts.iter().find(|a| a.id == id)
    }
}

/// Parsed Google Vertex AI Application Default Credentials (ADC).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VertexAdcCredentials {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quota_project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
}

/// Parse `application_default_credentials.json` contents into `VertexAdcCredentials`.
pub fn parse_vertex_adc_credentials(raw: &str) -> Option<VertexAdcCredentials> {
    let val: Value = serde_json::from_str(raw).ok()?;
    let client_id = non_empty_str(val.get("client_id"))?;
    let client_secret = non_empty_str(val.get("client_secret"))?;
    let refresh_token = non_empty_str(val.get("refresh_token"))?;
    let quota_project_id = non_empty_str(val.get("quota_project_id"));
    let account = non_empty_str(val.get("account"));
    Some(VertexAdcCredentials {
        client_id,
        client_secret,
        refresh_token,
        quota_project_id,
        account,
    })
}

/// Fallback parser for gcloud `config_default` INI files.
/// Extracts `(project, account)` under the `[core]` section.
pub fn parse_gcloud_config_default(raw: &str) -> (Option<String>, Option<String>) {
    let mut project = None;
    let mut account = None;
    let mut current_section = String::new();
    for raw_line in raw.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current_section = line[1..line.len() - 1].trim().to_ascii_lowercase();
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let key = k.trim().to_ascii_lowercase();
            let val = v.trim().trim_matches('"').trim_matches('\'').trim().to_string();
            if val.is_empty() {
                continue;
            }
            if current_section == "core" {
                if key == "project" {
                    project = Some(val);
                } else if key == "account" {
                    account = Some(val);
                }
            }
        }
    }
    (project, account)
}

/// Scan for Google Vertex AI Application Default Credentials in `home` or `env_path`.
pub fn scan_vertex_adc(home: &Path, env_path: Option<&Path>) -> Option<SubscriptionAccount> {
    let cred_path = if let Some(p) = env_path {
        if p.is_file() {
            p.to_path_buf()
        } else {
            home.join(".config").join("gcloud").join("application_default_credentials.json")
        }
    } else {
        home.join(".config").join("gcloud").join("application_default_credentials.json")
    };

    let raw = std::fs::read_to_string(&cred_path).ok()?;
    let mut creds = parse_vertex_adc_credentials(&raw)?;

    // Fallback: check ~/.config/gcloud/configurations/config_default
    if creds.quota_project_id.is_none() || creds.account.is_none() {
        let config_default_path = home.join(".config").join("gcloud").join("configurations").join("config_default");
        if let Ok(cfg_raw) = std::fs::read_to_string(&config_default_path) {
            let (fb_proj, fb_acct) = parse_gcloud_config_default(&cfg_raw);
            if creds.quota_project_id.is_none() {
                creds.quota_project_id = fb_proj;
            }
            if creds.account.is_none() {
                creds.account = fb_acct;
            }
        }
    }

    let id_suffix = creds.account.as_deref()
        .or(creds.quota_project_id.as_deref())
        .unwrap_or("adc");
    let label = match (&creds.account, &creds.quota_project_id) {
        (Some(acct), Some(proj)) => format!("Google Vertex AI ({acct} · {proj})"),
        (Some(acct), None) => format!("Google Vertex AI ({acct})"),
        (None, Some(proj)) => format!("Google Vertex AI ({proj})"),
        (None, None) => "Google Vertex AI ADC".to_string(),
    };

    let mut headers = BTreeMap::new();
    if let Some(ref proj) = creds.quota_project_id {
        headers.insert("x-goog-user-project".to_string(), proj.clone());
    }

    Some(SubscriptionAccount {
        id: format!("google_vertex:{id_suffix}"),
        kind: SubscriptionKind::GoogleVertex,
        label,
        email: creds.account.clone(),
        account_id: creds.quota_project_id.clone(),
        plan: Some("vertex_adc".to_string()),
        active: !creds.refresh_token.is_empty(),
        base_url: Some("https://aiplatform.googleapis.com/v1".to_string()),
        protocol: ProtocolKind::GeminiGenerateContent,
        default_model: Some("gemini-2.5-flash".to_string()),
        source_path: cred_path.display().to_string(),
        expires_at_ms: None,
        access_token: None,
        headers,
    })
}

// --- Google Vertex AI OAuth2 Token Refresh & Cache ---

pub const DEFAULT_TOKEN_REFRESH_LEAD_TIME_MS: i64 = 5 * 60 * 1_000; // 5 minutes
pub const GOOGLE_OAUTH2_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub const OPENAI_OAUTH2_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
pub const DEFAULT_CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";

/// Redact secrets and truncate raw OAuth error response bodies before surfacing in errors/logs.
pub fn redact_oauth_error_body(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "empty error response".to_string();
    }
    let summary = if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
        let err_code = non_empty_str(val.get("error"));
        let err_desc = non_empty_str(val.get("error_description"))
            .or_else(|| non_empty_str(val.get("error").and_then(|e| e.get("message"))))
            .or_else(|| non_empty_str(val.get("message")));
        match (err_code, err_desc) {
            (Some(c), Some(d)) => format!("{c}: {d}"),
            (Some(c), None) => c,
            (None, Some(d)) => d,
            (None, None) => "[redacted non-standard OAuth JSON error]".to_string(),
        }
    } else {
        trimmed.to_string()
    };

    let masker = crate::masking::MaskingEngine::new(crate::masking::MaskingConfig {
        enabled: true,
        mask_secrets: true,
        mask_personal: false,
        ..Default::default()
    });
    let masked = masker.mask_text(&summary).masked_text;
    let max_chars = 200;
    if masked.chars().count() > max_chars {
        let truncated: String = masked.chars().take(max_chars).collect();
        format!("{truncated}...")
    } else {
        masked
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TokenRefreshError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("OAuth error ({status}): {body}")]
    OAuth { status: u16, body: String },
    #[error("Missing refresh token or credentials")]
    MissingCredentials,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleOAuthTokenResponse {
    pub access_token: String,
    pub expires_in: i64,
    #[serde(default)]
    pub token_type: String,
    #[serde(default)]
    pub scope: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedVertexToken {
    pub access_token: String,
    pub expires_at_ms: i64,
}

impl CachedVertexToken {
    pub fn new(access_token: String, expires_in_secs: i64, now_ms: i64) -> Self {
        Self {
            access_token,
            expires_at_ms: now_ms.saturating_add(expires_in_secs.saturating_mul(1_000)),
        }
    }

    /// Check if the token is still valid with the given lead time buffer.
    pub fn is_fresh(&self, now_ms: i64, lead_time_ms: i64) -> bool {
        self.expires_at_ms > now_ms.saturating_add(lead_time_ms)
    }
}

/// Thread-safe Google Vertex AI OAuth2 Token Manager with in-memory caching and lead-time refresh.
pub struct VertexTokenManager {
    credentials: VertexAdcCredentials,
    token_url: String,
    cache: std::sync::RwLock<Option<CachedVertexToken>>,
    lead_time_ms: i64,
}

impl VertexTokenManager {
    pub fn new(credentials: VertexAdcCredentials) -> Self {
        Self {
            credentials,
            token_url: GOOGLE_OAUTH2_TOKEN_URL.to_string(),
            cache: std::sync::RwLock::new(None),
            lead_time_ms: DEFAULT_TOKEN_REFRESH_LEAD_TIME_MS,
        }
    }

    pub fn with_token_url(mut self, url: impl Into<String>) -> Self {
        self.token_url = url.into();
        self
    }

    pub fn with_lead_time_ms(mut self, lead_time_ms: i64) -> Self {
        self.lead_time_ms = lead_time_ms;
        self
    }

    pub fn credentials(&self) -> &VertexAdcCredentials {
        &self.credentials
    }

    pub fn cached_token(&self) -> Option<CachedVertexToken> {
        self.cache.read().ok().and_then(|c| c.clone())
    }

    pub fn get_valid_cached_token(&self, now_ms: i64) -> Option<String> {
        let guard = self.cache.read().ok()?;
        guard
            .as_ref()
            .filter(|t| t.is_fresh(now_ms, self.lead_time_ms))
            .map(|t| t.access_token.clone())
    }

    pub fn set_cached_token(&self, token: CachedVertexToken) {
        if let Ok(mut guard) = self.cache.write() {
            *guard = Some(token);
        }
    }

    pub fn invalidate_cached_token(&self) {
        if let Ok(mut guard) = self.cache.write() {
            *guard = None;
        }
    }

    pub fn token_url(&self) -> &str {
        &self.token_url
    }

    pub fn build_refresh_form(&self) -> Vec<(&'static str, String)> {
        vec![
            ("grant_type", "refresh_token".to_string()),
            ("client_id", self.credentials.client_id.clone()),
            ("client_secret", self.credentials.client_secret.clone()),
            ("refresh_token", self.credentials.refresh_token.clone()),
        ]
    }

    /// Retrieve valid cached token or refresh it via POST request to Google OAuth token endpoint.
    pub fn get_or_refresh_token(
        &self,
        agent: &ureq::Agent,
        now_ms: i64,
    ) -> Result<String, TokenRefreshError> {
        if let Some(token) = self.get_valid_cached_token(now_ms) {
            return Ok(token);
        }

        let resp = self.execute_refresh(agent)?;
        let cached = CachedVertexToken::new(resp.access_token.clone(), resp.expires_in, now_ms);
        self.set_cached_token(cached);
        Ok(resp.access_token)
    }

    /// Execute direct HTTP POST refresh without checking cache.
    pub fn execute_refresh(
        &self,
        agent: &ureq::Agent,
    ) -> Result<GoogleOAuthTokenResponse, TokenRefreshError> {
        if self.credentials.refresh_token.is_empty() || self.credentials.client_id.is_empty() {
            return Err(TokenRefreshError::MissingCredentials);
        }
        let form = self.build_refresh_form();
        let form_refs: Vec<(&str, &str)> = form.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let body = match agent.post(&self.token_url).send_form(&form_refs) {
            Ok(resp) => resp
                .into_string()
                .map_err(|e| TokenRefreshError::Http(e.to_string()))?,
            Err(ureq::Error::Status(status, resp)) => {
                let err_body = resp.into_string().unwrap_or_default();
                return Err(TokenRefreshError::OAuth {
                    status,
                    body: redact_oauth_error_body(&err_body),
                });
            }
            Err(ureq::Error::Transport(tr)) => {
                return Err(TokenRefreshError::Http(tr.to_string()));
            }
        };

        let token_resp: GoogleOAuthTokenResponse = serde_json::from_str(&body)
            .map_err(|e| TokenRefreshError::Parse(format!("{e}: {}", redact_oauth_error_body(&body))))?;
        Ok(token_resp)
    }
}

/// Thread-safe Codex / ChatGPT OAuth2 Token Manager for refreshing expired `auth.json` sessions.
pub struct CodexTokenManager {
    refresh_token: String,
    client_id: String,
    token_url: String,
    cache: std::sync::RwLock<Option<CachedVertexToken>>,
    lead_time_ms: i64,
}

impl CodexTokenManager {
    pub fn new(refresh_token: impl Into<String>) -> Self {
        let token_url = std::env::var("CODEX_OAUTH_TOKEN_URL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| OPENAI_OAUTH2_TOKEN_URL.to_string());
        Self {
            refresh_token: refresh_token.into(),
            client_id: DEFAULT_CODEX_CLIENT_ID.to_string(),
            token_url,
            cache: std::sync::RwLock::new(None),
            lead_time_ms: DEFAULT_TOKEN_REFRESH_LEAD_TIME_MS,
        }
    }

    pub fn from_auth_json_str(raw: &str) -> Option<Self> {
        let val: Value = serde_json::from_str(raw).ok()?;
        let tokens = val.get("tokens");
        let refresh_token = non_empty_str(tokens.and_then(|t| t.get("refresh_token")))
            .or_else(|| non_empty_str(val.get("refresh_token")))?;
        let mut mgr = Self::new(refresh_token);
        if let Some(cid) = non_empty_str(tokens.and_then(|t| t.get("client_id")))
            .or_else(|| non_empty_str(val.get("client_id")))
        {
            mgr.client_id = cid;
        }
        if let Some(url) = non_empty_str(val.get("token_url")) {
            mgr.token_url = url;
        }
        if let Some(acc) = non_empty_str(tokens.and_then(|t| t.get("access_token"))) {
            if let Some(exp_ms) = extract_jwt_exp_ms(&acc) {
                mgr.set_cached_token(CachedVertexToken {
                    access_token: acc,
                    expires_at_ms: exp_ms,
                });
            }
        }
        Some(mgr)
    }

    pub fn from_logins_json_str(raw: &str, login_id: &str) -> Option<Self> {
        let val: Value = serde_json::from_str(raw).ok()?;
        let arr = val.as_array()?;
        let entry = arr.iter().find(|item| {
            let id_matches = non_empty_str(item.get("id")).as_deref() == Some(login_id);
            let agent_ok = matches!(
                non_empty_str(item.get("agent")).as_deref(),
                Some("codex") | None
            );
            id_matches && agent_ok
        })?;
        let auth = entry.get("auth")?;
        let tokens = auth.get("tokens");
        let refresh_token = non_empty_str(tokens.and_then(|t| t.get("refresh_token")))
            .or_else(|| non_empty_str(auth.get("refresh_token")))?;
        let mut mgr = Self::new(refresh_token);
        if let Some(cid) = non_empty_str(tokens.and_then(|t| t.get("client_id")))
            .or_else(|| non_empty_str(auth.get("client_id")))
            .or_else(|| non_empty_str(entry.get("client_id")))
        {
            mgr.client_id = cid;
        }
        if let Some(url) = non_empty_str(auth.get("token_url"))
            .or_else(|| non_empty_str(entry.get("token_url")))
        {
            mgr.token_url = url;
        }
        if let Some(acc) = non_empty_str(tokens.and_then(|t| t.get("access_token"))) {
            if let Some(exp_ms) = extract_jwt_exp_ms(&acc) {
                mgr.set_cached_token(CachedVertexToken {
                    access_token: acc,
                    expires_at_ms: exp_ms,
                });
            }
        }
        Some(mgr)
    }

    pub fn with_client_id(mut self, client_id: impl Into<String>) -> Self {
        self.client_id = client_id.into();
        self
    }

    pub fn with_token_url(mut self, url: impl Into<String>) -> Self {
        self.token_url = url.into();
        self
    }

    pub fn refresh_token(&self) -> &str {
        &self.refresh_token
    }

    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    pub fn token_url(&self) -> &str {
        &self.token_url
    }

    pub fn cached_token(&self) -> Option<CachedVertexToken> {
        self.cache.read().ok().and_then(|c| c.clone())
    }

    pub fn get_valid_cached_token(&self, now_ms: i64) -> Option<String> {
        let guard = self.cache.read().ok()?;
        guard
            .as_ref()
            .filter(|t| t.is_fresh(now_ms, self.lead_time_ms))
            .map(|t| t.access_token.clone())
    }

    pub fn set_cached_token(&self, token: CachedVertexToken) {
        if let Ok(mut guard) = self.cache.write() {
            *guard = Some(token);
        }
    }

    pub fn invalidate_cached_token(&self) {
        if let Ok(mut guard) = self.cache.write() {
            *guard = None;
        }
    }

    pub fn get_or_refresh_token(
        &self,
        agent: &ureq::Agent,
        now_ms: i64,
    ) -> Result<String, TokenRefreshError> {
        if let Some(token) = self.get_valid_cached_token(now_ms) {
            return Ok(token);
        }
        let resp = self.execute_refresh(agent)?;
        let expires_at_ms = extract_jwt_exp_ms(&resp.access_token)
            .unwrap_or_else(|| now_ms.saturating_add(resp.expires_in.max(60).saturating_mul(1_000)));
        self.set_cached_token(CachedVertexToken {
            access_token: resp.access_token.clone(),
            expires_at_ms,
        });
        Ok(resp.access_token)
    }

    pub fn execute_refresh(
        &self,
        agent: &ureq::Agent,
    ) -> Result<GoogleOAuthTokenResponse, TokenRefreshError> {
        if self.refresh_token.trim().is_empty() {
            return Err(TokenRefreshError::MissingCredentials);
        }
        let form = [
            ("grant_type", "refresh_token"),
            ("client_id", self.client_id.as_str()),
            ("refresh_token", self.refresh_token.as_str()),
        ];
        let body = match agent.post(&self.token_url).send_form(&form) {
            Ok(resp) => resp
                .into_string()
                .map_err(|e| TokenRefreshError::Http(e.to_string()))?,
            Err(ureq::Error::Status(status, resp)) => {
                let err_body = resp.into_string().unwrap_or_default();
                return Err(TokenRefreshError::OAuth {
                    status,
                    body: redact_oauth_error_body(&err_body),
                });
            }
            Err(ureq::Error::Transport(tr)) => {
                return Err(TokenRefreshError::Http(tr.to_string()));
            }
        };
        let val: Value = serde_json::from_str(&body)
            .map_err(|e| TokenRefreshError::Parse(format!("{e}: {}", redact_oauth_error_body(&body))))?;
        let access_token = non_empty_str(val.get("access_token"))
            .ok_or_else(|| TokenRefreshError::Parse("Missing access_token in Codex refresh response".to_string()))?;
        let expires_in = val.get("expires_in").and_then(Value::as_i64).unwrap_or(3600);
        let token_type = non_empty_str(val.get("token_type")).unwrap_or_else(|| "Bearer".to_string());
        let scope = non_empty_str(val.get("scope"));
        Ok(GoogleOAuthTokenResponse {
            access_token,
            expires_in,
            token_type,
            scope,
        })
    }
}

// --- Magpie State Scanner (~/.config/magpie/providers.json & logins.json) ---

/// Parse providers from Magpie's `providers.json`.
pub fn parse_magpie_providers(raw: &str) -> Vec<SubscriptionAccount> {
    let Ok(val) = serde_json::from_str::<Value>(raw) else {
        return Vec::new();
    };

    let entries = val
        .get("providers")
        .and_then(Value::as_array)
        .or_else(|| val.as_array());

    let Some(arr) = entries else {
        return Vec::new();
    };

    let mut accounts = Vec::new();
    for item in arr {
        let id = non_empty_str(item.get("id")).unwrap_or_default();
        let preset = non_empty_str(item.get("preset")).unwrap_or_default();
        let off = item.get("off").and_then(Value::as_bool).unwrap_or(false);

        if id == "google-vertex" || preset == "google-vertex" {
            let vertex_obj = item.get("vertex");
            let project = non_empty_str(vertex_obj.and_then(|v| v.get("project")));
            let cred_path_str = non_empty_str(vertex_obj.and_then(|v| v.get("credentials")));
            let mut headers = BTreeMap::new();
            if let Some(ref p) = project {
                headers.insert("x-goog-user-project".to_string(), p.clone());
            }

            let mut active = !off;
            let access_token = None;
            if let Some(ref p) = cred_path_str {
                active = !off
                    && std::fs::read_to_string(Path::new(p))
                        .ok()
                        .and_then(|adc_raw| parse_vertex_adc_credentials(&adc_raw))
                        .is_some_and(|adc| !adc.refresh_token.is_empty());
            }

            accounts.push(SubscriptionAccount {
                id: format!("magpie_provider:{}", id),
                kind: SubscriptionKind::GoogleVertex,
                label: non_empty_str(item.get("name"))
                    .unwrap_or_else(|| "Google Vertex AI (Magpie)".to_string()),
                email: None,
                account_id: project,
                plan: Some("vertex_magpie".to_string()),
                active,
                base_url: Some("https://aiplatform.googleapis.com/v1".to_string()),
                protocol: ProtocolKind::GeminiGenerateContent,
                default_model: Some("gemini-2.5-flash".to_string()),
                source_path: cred_path_str.unwrap_or_default(),
                expires_at_ms: None,
                access_token,
                headers,
            });
        }
    }
    accounts
}

/// Parse multi-account logins from Magpie's `logins.json`.
pub fn parse_magpie_logins(raw: &str, now_ms: i64) -> Vec<SubscriptionAccount> {
    let Ok(val) = serde_json::from_str::<Value>(raw) else {
        return Vec::new();
    };

    let Some(arr) = val.as_array() else {
        return Vec::new();
    };

    let mut accounts = Vec::new();
    for item in arr {
        let Some(login_id) = non_empty_str(item.get("id")) else {
            continue;
        };
        let agent = non_empty_str(item.get("agent")).unwrap_or_default();
        let user = non_empty_str(item.get("user")).unwrap_or_default();
        let plan = non_empty_str(item.get("plan"));
        let on = item.get("on").and_then(Value::as_bool).unwrap_or(true);
        let auth = item.get("auth");

        match agent.as_str() {
            "codex" => {
                let tokens = auth.and_then(|a| a.get("tokens"));
                let access_token = non_empty_str(tokens.and_then(|t| t.get("access_token")))
                    .or_else(|| non_empty_str(auth.and_then(|a| a.get("OPENAI_API_KEY"))));
                let id_token = non_empty_str(tokens.and_then(|t| t.get("id_token")));
                let mut account_id = non_empty_str(tokens.and_then(|t| t.get("account_id")));
                let mut email = None;
                let mut detected_plan = plan.clone();
                let mut expires_at_ms = None;

                for cand in [access_token.as_deref(), id_token.as_deref()]
                    .into_iter()
                    .flatten()
                {
                    if let Some(claims) = decode_jwt_payload(cand) {
                        if email.is_none() {
                            email = non_empty_str(claims.get("email"));
                        }
                        if expires_at_ms.is_none() {
                            expires_at_ms = claims
                                .get("exp")
                                .and_then(Value::as_i64)
                                .map(|s| s.saturating_mul(1_000));
                        }
                        if let Some(auth_obj) = claims.get("https://api.openai.com/auth") {
                            if account_id.is_none() {
                                account_id = non_empty_str(auth_obj.get("chatgpt_account_id"));
                            }
                            if detected_plan.is_none() {
                                detected_plan = non_empty_str(auth_obj.get("chatgpt_plan_type"));
                            }
                        }
                    }
                }

                let mut headers = BTreeMap::new();
                headers.insert("originator".to_string(), "magpie".to_string());
                if let Some(ref acct) = account_id {
                    headers.insert("ChatGPT-Account-ID".to_string(), acct.clone());
                }

                let active = on
                    && (access_token.is_some()
                        && (expires_at_ms.is_none_or(|exp| exp > now_ms)
                            || non_empty_str(tokens.and_then(|t| t.get("refresh_token"))).is_some()));

                accounts.push(SubscriptionAccount {
                    id: format!("magpie_codex:{login_id}"),
                    kind: SubscriptionKind::CodexChatGpt,
                    label: if !user.is_empty() {
                        format!("Magpie Codex ({user})")
                    } else {
                        "Magpie Codex".to_string()
                    },
                    email: email.or_else(|| {
                        if user.contains('@') {
                            Some(
                                user.split('·')
                                    .next()
                                    .unwrap_or(&user)
                                    .trim()
                                    .to_string(),
                            )
                        } else {
                            None
                        }
                    }),
                    account_id,
                    plan: detected_plan.or(plan),
                    active,
                    base_url: Some("https://chatgpt.com/backend-api/codex".to_string()),
                    protocol: ProtocolKind::CodexResponses,
                    default_model: Some("codex-mini-latest".to_string()),
                    source_path: "logins.json".to_string(),
                    expires_at_ms,
                    access_token,
                    headers,
                });
            }
            "gemini" => {
                let access_token = non_empty_str(auth.and_then(|a| a.get("access_token")));
                let refresh_token = non_empty_str(auth.and_then(|a| a.get("refresh_token")));
                let expires_at_ms =
                    auth.and_then(|a| a.get("expiry_date")).and_then(Value::as_i64);
                let active = on
                    && ((access_token.is_some() && expires_at_ms.is_none_or(|exp| exp > now_ms))
                        || refresh_token.is_some());

                accounts.push(SubscriptionAccount {
                    id: format!("magpie_gemini:{login_id}"),
                    kind: SubscriptionKind::GeminiCodeAssist,
                    label: if !user.is_empty() {
                        format!("Magpie Gemini ({user})")
                    } else {
                        "Magpie Gemini".to_string()
                    },
                    email: if user.contains('@') {
                        Some(user.clone())
                    } else {
                        None
                    },
                    account_id: None,
                    plan: Some("code_assist".to_string()),
                    active,
                    base_url: Some("https://cloudcode-pa.googleapis.com/v1internal".to_string()),
                    protocol: ProtocolKind::GeminiGenerateContent,
                    default_model: Some("gemini-2.5-pro".to_string()),
                    source_path: "logins.json".to_string(),
                    expires_at_ms,
                    access_token,
                    headers: BTreeMap::new(),
                });
            }
            _ => {}
        }
    }
    accounts
}

pub fn scan_magpie_dir(dir: &Path, now_ms: i64, out: &mut Vec<SubscriptionAccount>) {
    let prov_path = dir.join("providers.json");
    if let Ok(raw) = std::fs::read_to_string(&prov_path) {
        let mut provs = parse_magpie_providers(&raw);
        for p in &mut provs {
            p.source_path = prov_path.display().to_string();
        }
        out.extend(provs);
    }

    let logins_path = dir.join("logins.json");
    if let Ok(raw) = std::fs::read_to_string(&logins_path) {
        let mut logins = parse_magpie_logins(&raw, now_ms);
        for l in &mut logins {
            l.source_path = logins_path.display().to_string();
        }
        out.extend(logins);
    }
}

/// Read-only scan of standard local CLI subscription files under `home`.
/// Does not invoke any external process or perform network requests.
pub fn discover_subscriptions_in_home(
    home: &Path,
    codex_home_override: Option<&Path>,
) -> SubscriptionRegistry {
    let now_ms = Utc::now().timestamp_millis();
    let mut accounts = Vec::new();

    // 1. Google Vertex AI ADC
    let gcloud_env = std::env::var("GOOGLE_APPLICATION_CREDENTIALS")
        .ok()
        .map(PathBuf::from);
    if let Some(vertex_acc) = scan_vertex_adc(home, gcloud_env.as_deref()) {
        accounts.push(vertex_acc);
    }

    // 2. Magpie configuration state (~/.config/magpie/providers.json & logins.json)
    scan_magpie_dir(&home.join(".config").join("magpie"), now_ms, &mut accounts);

    // 3. Codex CLI (~/.codex/auth.json & config.toml)
    let codex_dir = codex_home_override
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".codex"));
    if let Some(codex_acc) = scan_codex_dir(&codex_dir, now_ms) {
        accounts.push(codex_acc);
    }

    // 4. Claude Code (~/.claude/settings.json & .credentials.json)
    scan_claude_dirs(home, now_ms, &mut accounts);

    // 5. GitHub Copilot
    scan_copilot_dir(&home.join(".config").join("github-copilot"), &mut accounts);

    // 6. Gemini Code Assist & Jetski Standalone (~/.gemini/oauth_creds.json & jetski-standalone-oauth-token)
    scan_gemini_dirs(&home.join(".gemini"), now_ms, &mut accounts);

    // 7. Antigravity OAuth (~/.antigravity or ~/.gemini/antigravity)
    if let Some(agy_acc) = scan_antigravity_dir(&home.join(".antigravity"))
        .or_else(|| scan_antigravity_dir(&home.join(".gemini").join("antigravity")))
    {
        accounts.push(agy_acc);
    }

    // 8. Grok SuperGrok
    if let Some(grok_acc) = scan_grok_dir(&home.join(".grok")) {
        accounts.push(grok_acc);
    }

    // Deduplicate accounts preserving order
    let mut seen_ids = HashSet::new();
    accounts.retain(|a| seen_ids.insert(a.id.clone()));

    SubscriptionRegistry {
        accounts,
        scanned_at_ms: now_ms,
    }
}

fn non_empty_str(val: Option<&Value>) -> Option<String> {
    val.and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

pub fn decode_jwt_payload(jwt: &str) -> Option<Value> {
    let mut parts = jwt.trim().split('.');
    let _hdr = parts.next()?;
    let payload_b64 = parts.next()?;
    let _sig = parts.next()?;
    let decoded = URL_SAFE_NO_PAD
        .decode(payload_b64.as_bytes())
        .or_else(|_| URL_SAFE.decode(payload_b64.as_bytes()))
        .ok()?;
    serde_json::from_slice::<Value>(&decoded).ok()
}

pub fn extract_jwt_exp_ms(jwt: &str) -> Option<i64> {
    decode_jwt_payload(jwt)?
        .get("exp")
        .and_then(Value::as_i64)
        .map(|sec| sec.saturating_mul(1_000))
}

fn scan_codex_dir(dir: &Path, now_ms: i64) -> Option<SubscriptionAccount> {
    let auth_path = dir.join("auth.json");
    let config_path = dir.join("config.toml");
    let auth_raw = std::fs::read_to_string(&auth_path).ok();
    let cfg_raw = std::fs::read_to_string(&config_path).ok();
    if auth_raw.is_none() && cfg_raw.is_none() {
        return None;
    }

    let parsed_cfg = cfg_raw
        .as_deref()
        .map(parse_codex_config_summary)
        .unwrap_or_default();

    let mut access_token = None;
    let mut account_id = None;
    let mut email = None;
    let mut plan = None;
    let mut expires_at_ms = None;

    if let Some(raw) = &auth_raw {
        if let Ok(val) = serde_json::from_str::<Value>(raw) {
            let tokens = val.get("tokens");
            access_token = non_empty_str(tokens.and_then(|t| t.get("access_token")))
                .or_else(|| non_empty_str(val.get("OPENAI_API_KEY")));
            let id_token = non_empty_str(tokens.and_then(|t| t.get("id_token")));
            account_id = non_empty_str(tokens.and_then(|t| t.get("account_id")))
                .or_else(|| non_empty_str(val.get("account_id")));
            email = non_empty_str(val.get("email"))
                .or_else(|| non_empty_str(tokens.and_then(|t| t.get("email"))));

            for cand in [access_token.as_deref(), id_token.as_deref()]
                .into_iter()
                .flatten()
            {
                if let Some(claims) = decode_jwt_payload(cand) {
                    if email.is_none() {
                        email = non_empty_str(claims.get("email"));
                    }
                    if expires_at_ms.is_none() {
                        expires_at_ms = claims
                            .get("exp")
                            .and_then(Value::as_i64)
                            .map(|sec| sec.saturating_mul(1_000));
                    }
                    if let Some(auth_obj) = claims.get("https://api.openai.com/auth") {
                        if account_id.is_none() {
                            account_id = non_empty_str(auth_obj.get("chatgpt_account_id"));
                        }
                        if plan.is_none() {
                            plan = non_empty_str(auth_obj.get("chatgpt_plan_type"));
                        }
                    }
                }
            }
        }
    }

    let has_token = access_token.is_some();
    let expired_no_refresh = expires_at_ms.is_some_and(|exp| exp <= now_ms)
        && auth_raw
            .as_deref()
            .and_then(|r| serde_json::from_str::<Value>(r).ok())
            .and_then(|v| {
                non_empty_str(v.get("tokens").and_then(|t| t.get("refresh_token")))
            })
            .is_none();
    let has_local_magpie = parsed_cfg
        .magpie_base_url
        .as_deref()
        .is_some_and(|u| !u.is_empty())
        || parsed_cfg
            .openai_base_url
            .as_deref()
            .is_some_and(|u| u.contains("127.0.0.1") || u.contains("localhost"));

    if !has_token && !has_local_magpie {
        return None;
    }

    let base_url = parsed_cfg
        .openai_base_url
        .or(parsed_cfg.magpie_base_url)
        .or_else(|| Some("https://chatgpt.com/backend-api/codex".to_string()));
    let mut headers = parsed_cfg.magpie_headers;
    headers.insert("originator".to_string(), "codex_cli_rs".to_string());
    if let Some(ref acct) = account_id {
        headers.insert("ChatGPT-Account-ID".to_string(), acct.clone());
    }

    let active = (has_token && !expired_no_refresh) || has_local_magpie;
    let id_suffix = email
        .as_deref()
        .or(account_id.as_deref())
        .unwrap_or("default");
    let label = match (&email, &plan) {
        (Some(em), Some(pl)) => format!("Codex ({em} · {pl})"),
        (Some(em), None) => format!("Codex ({em})"),
        (None, Some(pl)) => format!("Codex ({pl})"),
        (None, None) => "Codex ChatGPT".to_string(),
    };

    Some(SubscriptionAccount {
        id: format!("codex:{id_suffix}"),
        kind: SubscriptionKind::CodexChatGpt,
        label,
        email,
        account_id,
        plan,
        active,
        base_url,
        protocol: ProtocolKind::CodexResponses,
        default_model: parsed_cfg.model.or_else(|| Some("codex-mini-latest".to_string())),
        source_path: if auth_raw.is_some() {
            auth_path.display().to_string()
        } else {
            config_path.display().to_string()
        },
        expires_at_ms,
        access_token,
        headers,
    })
}

pub fn scan_claude_dirs(home: &Path, now_ms: i64, out: &mut Vec<SubscriptionAccount>) {
    // 1. Scan settings.json in ~/.claude/settings.json
    let settings_path = home.join(".claude").join("settings.json");
    if let Ok(raw) = std::fs::read_to_string(&settings_path) {
        if let Ok(val) = serde_json::from_str::<Value>(&raw) {
            let env_obj = val.get("env");
            let token = non_empty_str(env_obj.and_then(|e| e.get("ANTHROPIC_AUTH_TOKEN")));
            if let Some(tok) = token {
                let base_url = non_empty_str(env_obj.and_then(|e| e.get("ANTHROPIC_BASE_URL")))
                    .or_else(|| Some("https://api.anthropic.com/v1".to_string()));
                let model = non_empty_str(val.get("model"))
                    .or_else(|| Some("claude-sonnet-4-20250514".to_string()));
                out.push(SubscriptionAccount {
                    id: "claude_code:settings".to_string(),
                    kind: SubscriptionKind::ClaudeCode,
                    label: "Claude Code (Settings)".to_string(),
                    email: None,
                    account_id: None,
                    plan: Some("settings_env".to_string()),
                    active: true,
                    base_url,
                    protocol: ProtocolKind::AnthropicMessages,
                    default_model: model,
                    source_path: settings_path.display().to_string(),
                    expires_at_ms: None,
                    access_token: Some(tok),
                    headers: BTreeMap::new(),
                });
            }
        }
    }

    // 2. Scan profile directories (.credentials.json)
    let mut dirs = Vec::new();
    let default_claude = home.join(".claude");
    if default_claude.is_dir() {
        dirs.push(("default".to_string(), default_claude));
    }
    if let Ok(entries) = std::fs::read_dir(home) {
        let mut extras: Vec<(String, PathBuf)> = entries
            .filter_map(Result::ok)
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                let path = e.path();
                if path.is_dir() && name.starts_with(".claude") && name != ".claude" {
                    let suffix = name
                        .strip_prefix(".claude")
                        .unwrap_or("")
                        .trim_matches(|c| c == '-' || c == '_');
                    if !suffix.is_empty() && !suffix.starts_with('.') && suffix != "launch" {
                        return Some((suffix.to_string(), path));
                    }
                }
                None
            })
            .collect();
        extras.sort_by(|a, b| a.0.cmp(&b.0));
        dirs.extend(extras);
    }

    for (profile_name, dir) in dirs {
        let cred_path = dir.join(".credentials.json");
        let Ok(raw) = std::fs::read_to_string(&cred_path) else {
            continue;
        };
        let Ok(val) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        let oauth = val.get("claudeAiOauth").unwrap_or(&val);
        let access_token = non_empty_str(oauth.get("accessToken"))
            .or_else(|| non_empty_str(val.get("accessToken")));
        let refresh_token = non_empty_str(oauth.get("refreshToken"));
        if access_token.is_none() && refresh_token.is_none() {
            continue;
        }
        let expires_at_ms = oauth.get("expiresAt").and_then(Value::as_i64);
        let plan = non_empty_str(oauth.get("subscriptionType"))
            .or_else(|| non_empty_str(oauth.get("rateLimitTier")));
        let email = non_empty_str(val.get("oauthAccount").and_then(|o| o.get("emailAddress")))
            .or_else(|| non_empty_str(oauth.get("email")));
        let active = access_token.is_some()
            && (expires_at_ms.is_none_or(|exp| exp > now_ms) || refresh_token.is_some());

        let label = match (&profile_name, &email, &plan) {
            (p, Some(em), _) if p == "default" => format!("Claude Code ({em})"),
            (p, _, Some(pl)) if p == "default" => format!("Claude Code ({pl})"),
            (p, _, _) if p == "default" => "Claude Code".to_string(),
            (p, _, _) => format!("Claude Code · {p}"),
        };

        out.push(SubscriptionAccount {
            id: format!("claude_code:{profile_name}"),
            kind: SubscriptionKind::ClaudeCode,
            label,
            email,
            account_id: None,
            plan,
            active,
            base_url: Some("https://api.anthropic.com/v1".to_string()),
            protocol: ProtocolKind::AnthropicMessages,
            default_model: Some("claude-sonnet-4-20250514".to_string()),
            source_path: cred_path.display().to_string(),
            expires_at_ms,
            access_token,
            headers: BTreeMap::new(),
        });
    }
}

fn scan_copilot_dir(dir: &Path, out: &mut Vec<SubscriptionAccount>) {
    let mut seen_users = HashMap::<String, usize>::new();
    for filename in ["hosts.json", "apps.json"] {
        let path = dir.join(filename);
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        for (host_key, entry) in map {
            let Some(token) = non_empty_str(entry.get("oauth_token")) else {
                continue;
            };
            let user = non_empty_str(entry.get("user")).unwrap_or_else(|| host_key.clone());
            if seen_users.contains_key(&user) {
                continue;
            }
            seen_users.insert(user.clone(), out.len());
            out.push(SubscriptionAccount {
                id: format!("github_copilot:{user}"),
                kind: SubscriptionKind::GitHubCopilot,
                label: format!("GitHub Copilot ({user})"),
                email: None,
                account_id: Some(user),
                plan: Some("copilot".to_string()),
                active: true,
                base_url: Some("https://api.githubcopilot.com".to_string()),
                protocol: ProtocolKind::ChatCompletions,
                default_model: Some("gpt-4o".to_string()),
                source_path: path.display().to_string(),
                expires_at_ms: None,
                access_token: Some(token),
                headers: BTreeMap::new(),
            });
        }
    }
}

pub fn scan_gemini_dirs(dir: &Path, now_ms: i64, out: &mut Vec<SubscriptionAccount>) {
    // 1. Check oauth_creds.json
    if let Some(acc) = scan_gemini_dir(dir, now_ms) {
        out.push(acc);
    }

    // 2. Check jetski-standalone-oauth-token
    let jetski_path = dir.join("jetski-standalone-oauth-token");
    if let Ok(raw) = std::fs::read_to_string(&jetski_path) {
        if let Ok(val) = serde_json::from_str::<Value>(&raw) {
            let token_obj = val.get("token");
            let access_token = non_empty_str(token_obj.and_then(|t| t.get("access_token")));
            let refresh_token = non_empty_str(token_obj.and_then(|t| t.get("refresh_token")));
            let email = non_empty_str(val.get("id_token_email"));

            if access_token.is_some() || refresh_token.is_some() {
                let expires_at_ms = token_obj
                    .and_then(|t| t.get("expiry"))
                    .and_then(|e| {
                        if let Some(s) = e.as_str() {
                            chrono::DateTime::parse_from_rfc3339(s)
                                .ok()
                                .map(|dt| dt.timestamp_millis())
                        } else {
                            e.as_i64().map(|v| if v < 100_000_000_000 { v * 1000 } else { v })
                        }
                    })
                    .or_else(|| {
                        token_obj
                            .and_then(|t| t.get("expires_in"))
                            .and_then(Value::as_i64)
                            .map(|sec| now_ms.saturating_add(sec * 1000))
                    });

                let active = (access_token.is_some() && expires_at_ms.is_none_or(|exp| exp > now_ms))
                    || refresh_token.is_some();
                let label = match &email {
                    Some(em) => format!("Gemini Jetski ({em})"),
                    None => "Gemini Jetski Standalone".to_string(),
                };

                out.push(SubscriptionAccount {
                    id: format!("gemini_jetski:{}", email.as_deref().unwrap_or("default")),
                    kind: SubscriptionKind::GeminiCodeAssist,
                    label,
                    email,
                    account_id: None,
                    plan: non_empty_str(val.get("auth_method")).or_else(|| Some("consumer".to_string())),
                    active,
                    base_url: Some("https://cloudcode-pa.googleapis.com/v1internal".to_string()),
                    protocol: ProtocolKind::GeminiGenerateContent,
                    default_model: Some("gemini-2.5-pro".to_string()),
                    source_path: jetski_path.display().to_string(),
                    expires_at_ms,
                    access_token,
                    headers: BTreeMap::new(),
                });
            }
        }
    }
}

fn scan_gemini_dir(dir: &Path, now_ms: i64) -> Option<SubscriptionAccount> {
    let cred_path = dir.join("oauth_creds.json");
    let raw = std::fs::read_to_string(&cred_path).ok()?;
    let val = serde_json::from_str::<Value>(&raw).ok()?;
    let access_token = non_empty_str(val.get("access_token"));
    let refresh_token = non_empty_str(val.get("refresh_token"));
    if access_token.is_none() && refresh_token.is_none() {
        return None;
    }
    let expires_at_ms = val.get("expiry_date").and_then(Value::as_i64);
    let mut email = non_empty_str(val.get("email"));
    if email.is_none() {
        if let Some(idt) = non_empty_str(val.get("id_token")) {
            if let Some(claims) = decode_jwt_payload(&idt) {
                email = non_empty_str(claims.get("email"));
            }
        }
    }
    let active = (access_token.is_some() && expires_at_ms.is_none_or(|exp| exp > now_ms))
        || refresh_token.is_some();
    let label = match &email {
        Some(em) => format!("Gemini Code Assist ({em})"),
        None => "Gemini Code Assist".to_string(),
    };
    Some(SubscriptionAccount {
        id: format!(
            "gemini_code_assist:{}",
            email.as_deref().unwrap_or("default")
        ),
        kind: SubscriptionKind::GeminiCodeAssist,
        label,
        email,
        account_id: None,
        plan: Some("code_assist".to_string()),
        active,
        base_url: Some("https://cloudcode-pa.googleapis.com/v1internal".to_string()),
        protocol: ProtocolKind::GeminiGenerateContent,
        default_model: Some("gemini-2.5-pro".to_string()),
        source_path: cred_path.display().to_string(),
        expires_at_ms,
        access_token,
        headers: BTreeMap::new(),
    })
}

fn scan_antigravity_dir(dir: &Path) -> Option<SubscriptionAccount> {
    let path = dir.join("accounts.json");
    let raw = std::fs::read_to_string(&path).ok()?;
    let val = serde_json::from_str::<Value>(&raw).ok()?;
    let arr = val
        .get("accounts")
        .and_then(Value::as_array)
        .or_else(|| val.as_array())?;
    let first = arr.first()?;
    let token = non_empty_str(first.get("refresh_token"))
        .or_else(|| non_empty_str(first.get("token").and_then(|t| t.get("refresh_token"))));
    let email = non_empty_str(first.get("email"));
    if token.is_none() && email.is_none() {
        return None;
    }
    Some(SubscriptionAccount {
        id: format!("antigravity:{}", email.as_deref().unwrap_or("default")),
        kind: SubscriptionKind::Antigravity,
        label: email
            .as_ref()
            .map(|e| format!("Antigravity ({e})"))
            .unwrap_or_else(|| "Antigravity".to_string()),
        email,
        account_id: None,
        plan: non_empty_str(first.get("plan")),
        active: token.is_some(),
        base_url: Some("https://cloudcode-pa.googleapis.com/v1internal".to_string()),
        protocol: ProtocolKind::GeminiGenerateContent,
        default_model: Some("gemini-3.1-pro-preview".to_string()),
        source_path: path.display().to_string(),
        expires_at_ms: None,
        access_token: token,
        headers: BTreeMap::new(),
    })
}

fn scan_grok_dir(dir: &Path) -> Option<SubscriptionAccount> {
    let path = dir.join("session.json");
    let raw = std::fs::read_to_string(&path).ok()?;
    let val = serde_json::from_str::<Value>(&raw).ok()?;
    let token = non_empty_str(val.get("token"))
        .or_else(|| non_empty_str(val.get("sso")))?;
    let email = non_empty_str(val.get("email"));
    Some(SubscriptionAccount {
        id: format!("grok_supergrok:{}", email.as_deref().unwrap_or("default")),
        kind: SubscriptionKind::GrokSuperGrok,
        label: email
            .as_ref()
            .map(|e| format!("SuperGrok ({e})"))
            .unwrap_or_else(|| "SuperGrok".to_string()),
        email,
        account_id: None,
        plan: non_empty_str(val.get("plan")).or_else(|| Some("supergrok".to_string())),
        active: true,
        base_url: Some("https://api.x.ai/v1".to_string()),
        protocol: ProtocolKind::ChatCompletions,
        default_model: Some("grok-3-beta".to_string()),
        source_path: path.display().to_string(),
        expires_at_ms: None,
        access_token: Some(token),
        headers: BTreeMap::new(),
    })
}

#[derive(Default)]
struct CodexConfigSummary {
    model: Option<String>,
    openai_base_url: Option<String>,
    magpie_base_url: Option<String>,
    magpie_headers: BTreeMap<String, String>,
}

fn parse_codex_config_summary(raw: &str) -> CodexConfigSummary {
    let mut summary = CodexConfigSummary::default();
    let mut section = String::new();
    for raw_line in raw.lines() {
        let no_comment = raw_line.split('#').next().unwrap_or("").trim();
        if no_comment.is_empty() {
            continue;
        }
        if no_comment.starts_with('[') && no_comment.ends_with(']') {
            section = no_comment[1..no_comment.len() - 1]
                .trim()
                .to_ascii_lowercase();
            continue;
        }
        let Some((k, v)) = no_comment.split_once('=') else {
            continue;
        };
        let key = k.trim().trim_matches('"').trim_matches('\'');
        let val = v.trim().trim_matches('"').trim_matches('\'');
        if val.is_empty() {
            continue;
        }
        if section.is_empty() {
            if key == "model" {
                summary.model = Some(val.to_string());
            } else if key == "openai_base_url" {
                summary.openai_base_url = Some(val.to_string());
            }
        } else if section == "model_providers.magpie" && key == "base_url" {
            summary.magpie_base_url = Some(val.to_string());
        } else if section == "model_providers.magpie.http_headers" {
            summary
                .magpie_headers
                .insert(key.to_string(), val.to_string());
        }
    }
    summary
}

/// Refreshable Codex / ChatGPT OAuth2 credentials synced from a local machine.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexOAuthCredentials {
    pub refresh_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_url: Option<String>,
}

impl std::fmt::Debug for CodexOAuthCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodexOAuthCredentials")
            .field("refresh_token", &"***")
            .field("client_id", &self.client_id)
            .field("token_url", &self.token_url)
            .finish()
    }
}

/// Complete client-discovered local subscription bundle for syncing from desktop client to server.
/// Unlike `SubscriptionAccount` (which omits `access_token` and redacts headers in JSON for safe GET
/// responses), this struct is ONLY used on authenticated client->server sync requests and redacts all
/// secrets in `Debug`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientSubscriptionSyncItem {
    pub account: SubscriptionAccount,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    #[serde(default)]
    pub raw_headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertex_adc: Option<VertexAdcCredentials>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_oauth: Option<CodexOAuthCredentials>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub available_models: Vec<String>,
}

impl std::fmt::Debug for ClientSubscriptionSyncItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let redacted_headers: BTreeMap<&String, String> = self
            .raw_headers
            .iter()
            .map(|(k, v)| (k, redact_header_value(k, v)))
            .collect();
        f.debug_struct("ClientSubscriptionSyncItem")
            .field("account", &self.account)
            .field("access_token", &self.access_token.as_ref().map(|_| "***"))
            .field("raw_headers", &redacted_headers)
            .field("vertex_adc", &self.vertex_adc.as_ref().map(|_| "***"))
            .field("codex_oauth", &self.codex_oauth)
            .field("region_id", &self.region_id)
            .field("available_models", &self.available_models)
            .finish()
    }
}

/// Returns `true` if `url` targets the local host (`127.0.0.1`, `localhost`, or `[::1]`).
pub fn is_loopback_server_url(url: &str) -> bool {
    let trimmed = url.trim().to_ascii_lowercase();
    let without_scheme = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .or_else(|| trimmed.strip_prefix("ws://"))
        .or_else(|| trimmed.strip_prefix("wss://"))
        .unwrap_or(&trimmed);
    let authority = without_scheme.split('/').next().unwrap_or("");
    let host = if let Some(rest) = authority.strip_prefix('[') {
        rest.split(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };
    matches!(host, "127.0.0.1" | "localhost" | "::1" | "0.0.0.0")
}

fn read_vertex_adc_for_account(acc: &SubscriptionAccount, home: &Path) -> Option<VertexAdcCredentials> {
    let mut candidates = Vec::new();
    if !acc.source_path.trim().is_empty() {
        candidates.push(PathBuf::from(&acc.source_path));
    }
    if let Ok(env_p) = std::env::var("GOOGLE_APPLICATION_CREDENTIALS") {
        if !env_p.trim().is_empty() {
            candidates.push(PathBuf::from(env_p));
        }
    }
    candidates.push(
        home.join(".config")
            .join("gcloud")
            .join("application_default_credentials.json"),
    );
    if let Some(appdata) = std::env::var_os("APPDATA").filter(|v| !v.is_empty()) {
        candidates.push(
            PathBuf::from(appdata)
                .join("gcloud")
                .join("application_default_credentials.json"),
        );
    }

    for cand in candidates {
        if !cand.is_file() {
            continue;
        }
        if let Ok(raw) = std::fs::read_to_string(&cand) {
            if let Some(mut creds) = parse_vertex_adc_credentials(&raw) {
                if creds.quota_project_id.is_none() {
                    creds.quota_project_id = acc.account_id.clone();
                }
                if creds.account.is_none() {
                    creds.account = acc.email.clone();
                }
                return Some(creds);
            }
        }
    }
    None
}

fn read_codex_oauth_for_account(
    acc: &SubscriptionAccount,
    home: &Path,
    codex_home_override: Option<&Path>,
) -> Option<CodexOAuthCredentials> {
    let login_id_opt = acc.id.strip_prefix("magpie_codex:");
    let mut mgr_opt = None;

    if !acc.source_path.trim().is_empty() {
        let p = PathBuf::from(&acc.source_path);
        if p.is_file() {
            if let Ok(raw) = std::fs::read_to_string(&p) {
                mgr_opt = if let Some(login_id) = login_id_opt {
                    CodexTokenManager::from_logins_json_str(&raw, login_id)
                        .or_else(|| CodexTokenManager::from_auth_json_str(&raw))
                } else {
                    CodexTokenManager::from_auth_json_str(&raw)
                };
            }
        }
    }

    if mgr_opt.is_none() && login_id_opt.is_none() {
        let codex_dir = codex_home_override
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".codex"));
        let auth_path = codex_dir.join("auth.json");
        if let Ok(raw) = std::fs::read_to_string(&auth_path) {
            mgr_opt = CodexTokenManager::from_auth_json_str(&raw);
        }
    }

    mgr_opt.map(|mgr| CodexOAuthCredentials {
        refresh_token: mgr.refresh_token().to_string(),
        client_id: Some(mgr.client_id().to_string()),
        token_url: Some(mgr.token_url().to_string()),
    })
}

fn read_magpie_vertex_location(home: &Path) -> Option<String> {
    let prov_path = home.join(".config").join("magpie").join("providers.json");
    let raw = std::fs::read_to_string(&prov_path).ok()?;
    let val: Value = serde_json::from_str(&raw).ok()?;
    let arr = val
        .get("providers")
        .and_then(Value::as_array)
        .or_else(|| val.as_array())?;
    for item in arr {
        let id = non_empty_str(item.get("id")).unwrap_or_default();
        let preset = non_empty_str(item.get("preset")).unwrap_or_default();
        if id == "google-vertex" || preset == "google-vertex" {
            if let Some(loc) = non_empty_str(item.get("vertex").and_then(|v| v.get("location"))) {
                return Some(loc);
            }
        }
    }
    None
}

fn normalize_sync_display_label(label: &str, kind: SubscriptionKind) -> String {
    let trimmed = label.trim();
    if let Some(rest) = trimmed.strip_prefix("Magpie Codex") {
        return format!("Codex{rest}");
    }
    if let Some(rest) = trimmed.strip_prefix("Magpie Gemini") {
        return format!("Gemini CLI{rest}");
    }
    if trimmed == "Google Vertex AI (Magpie)" {
        return "Google Vertex AI".to_string();
    }
    if let Some(stripped) = trimmed.strip_suffix(" (Magpie)") {
        return stripped.to_string();
    }
    if trimmed.is_empty() {
        return kind.display_name().to_string();
    }
    trimmed.to_string()
}

fn normalize_sync_plan_label(plan: Option<String>, kind: SubscriptionKind) -> Option<String> {
    match plan.as_deref().map(str::trim) {
        Some("vertex_magpie") | Some("vertex_adc") => Some("Vertex ADC".to_string()),
        Some("code_assist") => Some("Code Assist".to_string()),
        Some(other) if !other.is_empty() => Some(other.to_string()),
        _ => match kind {
            SubscriptionKind::GoogleVertex => Some("Vertex ADC".to_string()),
            _ => None,
        },
    }
}

fn dedup_syncable_items(items: Vec<ClientSubscriptionSyncItem>) -> Vec<ClientSubscriptionSyncItem> {
    let mut out: Vec<ClientSubscriptionSyncItem> = Vec::with_capacity(items.len());

    for item in items {
        let duplicate_idx = out.iter().position(|existing| {
            if existing.account.kind != item.account.kind {
                return false;
            }
            match item.account.kind {
                SubscriptionKind::GoogleVertex => {
                    let same_path = !existing.account.source_path.is_empty()
                        && existing.account.source_path == item.account.source_path;
                    let same_project = existing.account.account_id.is_some()
                        && existing.account.account_id == item.account.account_id;
                    let same_refresh = existing
                        .vertex_adc
                        .as_ref()
                        .zip(item.vertex_adc.as_ref())
                        .is_some_and(|(a, b)| a.refresh_token == b.refresh_token);
                    same_path || same_project || same_refresh
                }
                SubscriptionKind::CodexChatGpt => {
                    let same_email = existing
                        .account
                        .email
                        .as_deref()
                        .zip(item.account.email.as_deref())
                        .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b));
                    let same_acct = existing.account.account_id.is_some()
                        && existing.account.account_id == item.account.account_id;
                    (same_email && same_acct)
                        || (same_email
                            && (existing.account.account_id.is_none()
                                || item.account.account_id.is_none()))
                }
                SubscriptionKind::GeminiCodeAssist => existing
                    .account
                    .email
                    .as_deref()
                    .zip(item.account.email.as_deref())
                    .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b)),
                _ => existing.account.id == item.account.id,
            }
        });

        if let Some(idx) = duplicate_idx {
            let existing = &mut out[idx];
            if existing.region_id.is_none() && item.region_id.is_some() {
                existing.region_id = item.region_id;
            }
            if existing.vertex_adc.is_none() && item.vertex_adc.is_some() {
                existing.vertex_adc = item.vertex_adc;
            }
            if existing.codex_oauth.is_none() && item.codex_oauth.is_some() {
                existing.codex_oauth = item.codex_oauth;
            }
            if existing.access_token.is_none() && item.access_token.is_some() {
                existing.access_token = item.access_token;
            }
            if existing.account.email.is_none() && item.account.email.is_some() {
                existing.account.email = item.account.email;
            }
            if existing.account.account_id.is_none() && item.account.account_id.is_some() {
                existing.account.account_id = item.account.account_id;
            }
            // Prefer canonical native ID/label over magpie_provider:* wrapper when merging
            if existing.account.id.starts_with("magpie_provider:")
                && !item.account.id.starts_with("magpie_provider:")
            {
                existing.account.id = item.account.id;
                existing.account.label = item.account.label;
            }
        } else {
            out.push(item);
        }
    }

    out
}

fn push_unique_model(out: &mut Vec<String>, raw: &str) {
    let m = raw.trim();
    if !m.is_empty() && !out.iter().any(|x| x == m) {
        out.push(m.to_string());
    }
}

fn read_local_codex_cached_models(home: &Path, codex_home_override: Option<&Path>) -> Vec<String> {
    let codex_dir = codex_home_override
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".codex"));
    let mut out = Vec::new();

    if let Ok(raw) = std::fs::read_to_string(codex_dir.join("models_cache.json")) {
        if let Ok(val) = serde_json::from_str::<Value>(&raw) {
            if let Some(arr) = val.get("models").and_then(Value::as_array) {
                for m in arr {
                    let hidden = m
                        .get("visibility")
                        .and_then(Value::as_str)
                        .is_some_and(|v| v.eq_ignore_ascii_case("hide"));
                    if hidden {
                        continue;
                    }
                    if let Some(slug) = non_empty_str(m.get("slug")).or_else(|| non_empty_str(m.get("id"))) {
                        if !slug.contains('/') && slug != "gpt-reserve" {
                            push_unique_model(&mut out, &slug);
                        }
                    }
                }
            }
        }
    }

    if let Ok(raw) = std::fs::read_to_string(codex_dir.join("magpie-desktop-models.json")) {
        if let Ok(val) = serde_json::from_str::<Value>(&raw) {
            if let Some(arr) = val.get("models").and_then(Value::as_array) {
                for m in arr {
                    if let Some(slug) = non_empty_str(m.get("slug")).or_else(|| non_empty_str(m.get("id"))) {
                        if !slug.contains('/')
                            && slug != "gpt-reserve"
                            && (slug.starts_with("gpt-")
                                || slug.starts_with("codex")
                                || slug.starts_with("o1")
                                || slug.starts_with("o3")
                                || slug.starts_with("o4"))
                        {
                            push_unique_model(&mut out, &slug);
                        }
                    }
                }
            }
        }
    }

    out
}

fn read_local_vertex_and_gemini_cached_models(
    home: &Path,
    codex_home_override: Option<&Path>,
) -> (Vec<String>, Vec<String>) {
    let codex_dir = codex_home_override
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".codex"));
    let mut vertex_models = Vec::new();
    let mut gemini_models = Vec::new();

    if let Ok(raw) = std::fs::read_to_string(codex_dir.join("magpie-desktop-models.json")) {
        if let Ok(val) = serde_json::from_str::<Value>(&raw) {
            if let Some(arr) = val.get("models").and_then(Value::as_array) {
                for m in arr {
                    if let Some(slug) = non_empty_str(m.get("slug")).or_else(|| non_empty_str(m.get("id"))) {
                        if let Some(rest) = slug.strip_prefix("google-vertex/") {
                            // Keep direct Google publisher Gemini models that work on
                            // `/publishers/google/models/{model}:generateContent`
                            if rest.starts_with("gemini-") && !rest.contains('/') {
                                push_unique_model(&mut vertex_models, rest);
                            }
                        } else if let Some(rest) = slug.strip_prefix("auto/") {
                            if rest.starts_with("gemini-") && !rest.contains('/') {
                                push_unique_model(&mut gemini_models, rest);
                            }
                        }
                    }
                }
            }
        }
    }

    let prov_path = home.join(".config").join("magpie").join("providers.json");
    if let Ok(raw) = std::fs::read_to_string(&prov_path) {
        if let Ok(val) = serde_json::from_str::<Value>(&raw) {
            if let Some(arr) = val
                .get("providers")
                .and_then(Value::as_array)
                .or_else(|| val.as_array())
            {
                for item in arr {
                    let id = non_empty_str(item.get("id")).unwrap_or_default();
                    let preset = non_empty_str(item.get("preset")).unwrap_or_default();
                    if let Some(models) = item.get("models").and_then(Value::as_array) {
                        for m in models {
                            if let Some(name) = m.as_str() {
                                if id == "google-vertex" || preset == "google-vertex" {
                                    if name.starts_with("gemini-") && !name.contains('/') {
                                        push_unique_model(&mut vertex_models, name);
                                    }
                                } else if id == "gemini" || preset == "google" {
                                    push_unique_model(&mut gemini_models, name);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    (vertex_models, gemini_models)
}

/// Returns the default built-in model catalog for a given `SubscriptionKind`.
pub fn default_models_for_subscription_kind(kind: SubscriptionKind) -> &'static [&'static str] {
    match kind {
        SubscriptionKind::GoogleVertex => &[
            "gemini-3.8-flash",
            "gemini-3.7-flash",
            "gemini-3.6-flash",
            "gemini-3.5-flash",
            "gemini-3.5-flash-lite",
            "gemini-3.1-pro-preview",
            "gemini-3.1-flash-lite",
            "gemini-3-pro-preview",
            "gemini-3-flash-preview",
            "gemini-2.5-pro",
            "gemini-2.5-flash",
            "gemini-2.5-flash-lite",
            "gemini-1.5-pro",
            "gemini-1.5-flash",
        ],
        SubscriptionKind::CodexChatGpt => &[
            "gpt-6.2-sol",
            "gpt-6.1-sol",
            "gpt-6.1-terra",
            "gpt-6-astra",
            "gpt-6-sol",
            "gpt-6-luna",
            "gpt-5.6-sol",
            "gpt-5.6-terra",
            "gpt-5.6-luna",
            "codex-mini-latest",
            "gpt-4o",
            "o3-mini",
        ],
        SubscriptionKind::GeminiCodeAssist => &[
            "gemini-3.8-flash",
            "gemini-3.5-flash",
            "gemini-3.5-flash-lite",
            "gemini-3.1-pro-preview",
            "gemini-3.1-flash-lite",
            "gemini-3-pro-preview",
            "gemini-3-flash-preview",
            "gemini-2.5-pro",
            "gemini-2.5-flash",
            "gemini-2.5-flash-lite",
        ],
        SubscriptionKind::ClaudeCode => &[
            "claude-opus-4-6",
            "claude-sonnet-4-6",
            "claude-haiku-4-5",
            "claude-sonnet-4-20250514",
            "claude-3-7-sonnet-20250219",
            "claude-3-5-sonnet-20241022",
            "claude-3-5-haiku-20241022",
        ],
        SubscriptionKind::GitHubCopilot => &[
            "gpt-4o",
            "gpt-4.1",
            "claude-sonnet-4-20250514",
            "gemini-2.5-pro",
            "o3-mini",
        ],
        SubscriptionKind::Antigravity => &[
            "gemini-3.8-flash",
            "gemini-3.1-pro-preview",
            "gemini-2.5-pro",
            "gemini-2.5-flash",
        ],
        SubscriptionKind::GrokSuperGrok => &["grok-4", "grok-3", "grok-3-mini"],
    }
}

fn build_available_models(
    kind: SubscriptionKind,
    default_model: Option<&str>,
    discovered_extra: &[String],
) -> Vec<String> {
    let mut out = Vec::new();
    for m in discovered_extra {
        push_unique_model(&mut out, m);
    }
    for m in default_models_for_subscription_kind(kind) {
        push_unique_model(&mut out, m);
    }
    if let Some(dm) = default_model {
        let trimmed = dm.trim();
        if !trimmed.is_empty() && !trimmed.starts_with("codex/") && !trimmed.starts_with("auto/") {
            push_unique_model(&mut out, trimmed);
        }
    }
    out
}

/// Discovers local subscriptions under `home` and enriches them with unredacted tokens and
/// refreshable OAuth credentials suitable for syncing from the desktop client to the backend server.
///
/// When `target_is_remote` is `true`, loopback proxy URLs (e.g. `http://127.0.0.1:3425/...` written
/// by Magpie into `~/.codex/config.toml`) and Magpie-specific model prefixes (`codex/...`,
/// `auto/...`) are normalized to direct upstream endpoints so a remote cloud server can invoke the
/// upstream AI APIs directly.
pub fn discover_syncable_subscriptions_in_home(
    home: &Path,
    codex_home_override: Option<&Path>,
    target_is_remote: bool,
) -> Vec<ClientSubscriptionSyncItem> {
    let reg = discover_subscriptions_in_home(home, codex_home_override);
    let vertex_location = read_magpie_vertex_location(home);
    let cached_codex_models = read_local_codex_cached_models(home, codex_home_override);
    let (cached_vertex_models, cached_gemini_models) =
        read_local_vertex_and_gemini_cached_models(home, codex_home_override);
    let mut items = Vec::with_capacity(reg.accounts.len());

    for mut acc in reg.accounts {
        let access_token = acc.access_token.clone();
        let mut raw_headers = acc.headers.clone();
        let mut vertex_adc = None;
        let mut codex_oauth = None;
        let mut region_id = None;

        acc.label = normalize_sync_display_label(&acc.label, acc.kind);
        acc.plan = normalize_sync_plan_label(acc.plan.take(), acc.kind);

        match acc.kind {
            SubscriptionKind::GoogleVertex => {
                vertex_adc = read_vertex_adc_for_account(&acc, home);
                region_id = vertex_location.clone().or_else(|| Some("global".to_string()));
                if let Some(ref adc) = vertex_adc {
                    if acc.account_id.is_none() {
                        acc.account_id = adc.quota_project_id.clone();
                    }
                    if let Some(ref proj) = adc.quota_project_id {
                        raw_headers
                            .entry("x-goog-user-project".to_string())
                            .or_insert_with(|| proj.clone());
                    }
                }
                // Keep Vertex title concise so it doesn't duplicate the project ID shown on the subtitle line
                if let Some(ref em) = acc.email {
                    acc.label = format!("Google Vertex AI ({em})");
                }
            }
            SubscriptionKind::CodexChatGpt => {
                codex_oauth = read_codex_oauth_for_account(&acc, home, codex_home_override);
                if target_is_remote {
                    let is_loopback_base = acc
                        .base_url
                        .as_deref()
                        .is_some_and(is_loopback_server_url);
                    if is_loopback_base {
                        acc.base_url = Some("https://chatgpt.com/backend-api/codex".to_string());
                        raw_headers.retain(|k, _| {
                            !k.eq_ignore_ascii_case("x-openai-actor-authorization")
                        });
                        acc.headers.retain(|k, _| {
                            !k.eq_ignore_ascii_case("x-openai-actor-authorization")
                        });
                        if let Some(ref model) = acc.default_model {
                            let m = model.trim();
                            if m.starts_with("codex/") || m.starts_with("auto/") {
                                acc.default_model = Some("codex-mini-latest".to_string());
                            }
                        }
                    }
                    // If the account only existed because of a local Magpie proxy without any token
                    // or refresh token, it cannot execute from a remote server.
                    if access_token.is_none() && codex_oauth.is_none() {
                        acc.active = false;
                    }
                }
            }
            SubscriptionKind::ClaudeCode => {
                if target_is_remote
                    && acc
                        .base_url
                        .as_deref()
                        .is_some_and(is_loopback_server_url)
                {
                    acc.base_url = Some("https://api.anthropic.com/v1".to_string());
                }
            }
            _ => {}
        }

        // Skip entries that have no usable credential at all when syncing to a remote server
        if target_is_remote
            && access_token.is_none()
            && vertex_adc.is_none()
            && codex_oauth.is_none()
        {
            continue;
        }

        let extra_models: &[String] = match acc.kind {
            SubscriptionKind::GoogleVertex => &cached_vertex_models,
            SubscriptionKind::CodexChatGpt => &cached_codex_models,
            SubscriptionKind::GeminiCodeAssist => &cached_gemini_models,
            _ => &[],
        };
        let available_models =
            build_available_models(acc.kind, acc.default_model.as_deref(), extra_models);

        items.push(ClientSubscriptionSyncItem {
            account: acc,
            access_token,
            raw_headers,
            vertex_adc,
            codex_oauth,
            region_id,
            available_models,
        });
    }

    dedup_syncable_items(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscription_account_debug_redacts_secrets() {
        assert_eq!(redact_header_value("X-Authorization", "éééé"), "***");
        assert_eq!(redact_header_value("X-Authorization", "令牌🔑secret"), "***");
        assert_eq!(redact_header_value("Authorization", "Bearer🔑secret"), "***");
        assert_eq!(
            redact_header_value("Authorization", "  bEaReR 🔑secret  "),
            "Bearer ***"
        );
        assert_eq!(
            redact_header_value("X-Safe-Tenant", "区域🔑public"),
            "区域🔑public"
        );

        let mut headers = BTreeMap::new();
        headers.insert("Authorization".to_string(), "Bearer super-secret-jwt-999".to_string());
        headers.insert(
            "Proxy-Authorization".to_string(),
            "  bEaReR 🔑utf8-secret  ".to_string(),
        );
        headers.insert("X-Access-Key".to_string(), "secret-access-key-abc".to_string());
        headers.insert("X-Auth".to_string(), "secret-x-auth-val".to_string());
        headers.insert("X-Auth-Key".to_string(), "secret-x-auth-key-val".to_string());
        headers.insert("X-Authorization".to_string(), "secret-x-authorization-val".to_string());
        headers.insert("X-Auth-Utf8".to_string(), "éééé".to_string());
        headers.insert("X-Auth-Token".to_string(), "令牌🔑secret".to_string());
        headers.insert("X-Auth-Secret".to_string(), "Bearer🔑secret".to_string());
        headers.insert("X-Safe-Tenant".to_string(), "tenant-public-1".to_string());
        headers.insert("X-Public-Utf8".to_string(), "区域🔑public".to_string());

        let account = SubscriptionAccount {
            id: "sub_debug_test".to_string(),
            kind: SubscriptionKind::CodexChatGpt,
            label: "Codex Test".to_string(),
            email: Some("dev@example.com".to_string()),
            account_id: Some("acct_1".to_string()),
            plan: Some("plus".to_string()),
            active: true,
            base_url: Some("https://chatgpt.com/backend-api/codex".to_string()),
            protocol: ProtocolKind::CodexResponses,
            default_model: Some("gpt-5".to_string()),
            source_path: "/tmp/auth.json".to_string(),
            expires_at_ms: Some(1_900_000_000_000),
            access_token: Some("raw-oauth-access-token-xyz".to_string()),
            headers,
        };

        let dbg = format!("{account:?}");
        assert!(!dbg.contains("raw-oauth-access-token-xyz"), "raw access_token leaked: {dbg}");
        assert!(!dbg.contains("super-secret-jwt-999"), "raw Authorization leaked: {dbg}");
        assert!(!dbg.contains("🔑utf8-secret"), "raw Proxy-Authorization leaked: {dbg}");
        assert!(!dbg.contains("secret-access-key-abc"), "raw X-Access-Key leaked: {dbg}");
        assert!(!dbg.contains("secret-x-auth-val"), "raw X-Auth leaked: {dbg}");
        assert!(!dbg.contains("secret-x-auth-key-val"), "raw X-Auth-Key leaked: {dbg}");
        assert!(!dbg.contains("secret-x-authorization-val"), "raw X-Authorization leaked: {dbg}");
        assert!(!dbg.contains("éééé"), "raw UTF-8 X-Auth-Utf8 leaked: {dbg}");
        assert!(!dbg.contains("令牌🔑secret"), "raw UTF-8 X-Auth-Token leaked: {dbg}");
        assert!(!dbg.contains("Bearer🔑secret"), "raw UTF-8 X-Auth-Secret leaked: {dbg}");
        assert!(dbg.contains("Bearer ***"), "expected Bearer *** in debug output: {dbg}");
        assert!(dbg.contains("tenant-public-1"), "safe header should be preserved: {dbg}");
        assert!(dbg.contains("区域🔑public"), "safe UTF-8 header should be preserved: {dbg}");

        let json_str = serde_json::to_string(&account).expect("serialize SubscriptionAccount");
        assert!(!json_str.contains("éééé"), "JSON leaked UTF-8 secret: {json_str}");
        assert!(!json_str.contains("令牌🔑secret"), "JSON leaked UTF-8 secret: {json_str}");
        assert!(!json_str.contains("Bearer🔑secret"), "JSON leaked UTF-8 secret: {json_str}");
        assert!(!json_str.contains("🔑utf8-secret"), "JSON leaked UTF-8 bearer secret: {json_str}");
        assert!(json_str.contains("区域🔑public"), "JSON dropped safe UTF-8 header: {json_str}");

        let reg = SubscriptionRegistry {
            accounts: vec![account],
            scanned_at_ms: 123,
        };
        let reg_dbg = format!("{reg:?}");
        assert!(!reg_dbg.contains("raw-oauth-access-token-xyz"));
        assert!(!reg_dbg.contains("secret-x-auth-key-val"));
        assert!(!reg_dbg.contains("令牌🔑secret"));
    }
}
