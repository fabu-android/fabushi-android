use crate::account_telemetry::AccountTelemetryProjection;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use fabushi_android_shared::node::mcp::mcp_auth_watch_lifecycle::{
    McpAuthServerSnapshot, McpAuthTransport, McpBackendAuthStatus,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::env;
use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use url::Url;

pub const DASHBOARD_CHECK_HTTP_MCP_STATUS_PATH: &str =
    "/aiserver.v1.DashboardService/CheckHttpMcpStatus";
pub const DASHBOARD_VALIDATE_MCP_OAUTH_TOKENS_PATH: &str =
    "/aiserver.v1.DashboardService/ValidateMcpOAuthTokens";
pub const DASHBOARD_COMPLETE_MCP_OAUTH_PATH: &str =
    "/aiserver.v1.DashboardService/CompleteMcpOAuth";
pub const DASHBOARD_DELETE_MCP_OAUTH_TOKEN_PATH: &str =
    "/aiserver.v1.DashboardService/DeleteMcpOAuthToken";
pub const DASHBOARD_RENAME_MCP_OAUTH_ACCOUNT_PATH: &str =
    "/aiserver.v1.DashboardService/RenameMcpOAuthAccount";
pub const DASHBOARD_DELETE_MCP_OAUTH_ACCOUNT_PATH: &str =
    "/aiserver.v1.DashboardService/DeleteMcpOAuthAccount";
pub const DASHBOARD_GET_USER_PRIVACY_MODE_PATH: &str =
    "/aiserver.v1.DashboardService/GetUserPrivacyMode";
pub const DASHBOARD_GET_TEAMS_PATH: &str =
    "/aiserver.v1.DashboardService/GetTeams";
pub const DASHBOARD_GET_TEAM_RULES_PATH: &str =
    "/aiserver.v1.DashboardService/GetTeamRules";
pub const DASHBOARD_GET_ME_PATH: &str =
    "/aiserver.v1.DashboardService/GetMe";
pub const DASHBOARD_GET_TEAM_PLUGIN_POPULARITY_PATH: &str =
    "/aiserver.v1.DashboardService/GetTeamPluginPopularity";
pub const TEAM_RULES_REQUEST_TIMEOUT_MS: u64 = 10_000;
pub const TEAM_PLUGIN_POPULARITY_REQUEST_TIMEOUT_MS: u64 = 12_000;
pub const TEAM_RULE_AGENT_TYPE_ALL: u64 = 1;
pub const TEAM_RULE_AGENT_TYPE_SAND: u64 = 2;
pub const DASHBOARD_GET_AVAILABLE_MCP_SERVERS_PATH: &str =
    "/aiserver.v1.DashboardService/GetAvailableMcpServers";
pub const DASHBOARD_LIST_SAND_MCP_TOOLS_PATH: &str =
    "/aiserver.v1.DashboardService/ListSandMcpTools";
pub const DASHBOARD_EXECUTE_SAND_MCP_TOOL_PATH: &str =
    "/aiserver.v1.DashboardService/ExecuteSandMcpTool";
pub const DASHBOARD_CLIENT_ACTION_PATH: &str =
    "/aiserver.v1.DashboardService/ClientAction";
pub const ANALYTICS_SUBMIT_LOGS_PATH: &str =
    "/aiserver.v1.AnalyticsService/SubmitLogs";
pub const ACCOUNT_TELEMETRY_SUBMIT_TIMEOUT_MS: u64 = 15_000;
pub const FEEDBACK_SUBMIT_PATH: &str = "/sand/feedback";
pub const FEEDBACK_SUBMIT_TIMEOUT_MS: u64 = 15_000;
pub const DESKTOP_SESSION_EVENT: &str = "sand.desktop.session";
pub const DESKTOP_SIGNIN_EVENT: &str = "sand.desktop.signin";
pub const ACCESS_BLOCKED_EVENT: &str = "sand.access_blocked";
pub const SAND_LOG_KEY: &str = "sand";
pub const DASHBOARD_GET_CURRENT_PERIOD_USAGE_PATH: &str =
    "/aiserver.v1.DashboardService/GetCurrentPeriodUsage";
pub const DASHBOARD_GET_SAND_USAGE_STATUS_PATH: &str =
    "/aiserver.v1.DashboardService/GetSandUsageStatus";
pub const DASHBOARD_GET_SAND_ACCESS_STATUS_PATH: &str =
    "/aiserver.v1.DashboardService/GetSandAccessStatus";
pub const SAND_ACCESS_REQUEST_TIMEOUT_MS: u64 = 10_000;
pub const DASHBOARD_GET_SAND_TRIAL_CLAIM_STATUS_PATH: &str =
    "/aiserver.v1.DashboardService/GetSandTrialClaimStatus";
pub const DASHBOARD_CANCEL_SAND_TRIAL_PATH: &str =
    "/aiserver.v1.DashboardService/CancelSandTrial";
pub const USAGE_REQUEST_TIMEOUT_MS: u64 = 15_000;
pub const BACKGROUND_COMPOSER_GET_USER_SETTINGS_PATH: &str =
    "/aiserver.v1.BackgroundComposerService/GetBackgroundComposerUserSettings";
pub const DASHBOARD_GET_TEAM_ADMIN_SETTINGS_PATH: &str =
    "/aiserver.v1.DashboardService/GetTeamAdminSettingsOrEmptyIfNotInTeam";
pub const PR_REVIEW_REQUEST_TIMEOUT_MS: u64 = 10_000;
pub const LIST_TOOLS_TIMEOUT_MS: u64 = 60_000;
pub const EXECUTE_TOOL_TIMEOUT_MS: u64 = 180_000;
const MAX_MCP_TOOL_ARGUMENT_BYTES: usize = 1024 * 1024;
const MAX_PROTO_JSON_DEPTH: usize = 32;
pub const CONTROL_RPC_TIMEOUT_MS: u64 = 30_000;
pub const RUN_GENERATE_IMAGE_PATH: &str = "/aiserver.v1.AiService/RunGenerateImage";
pub const GENERATE_IMAGE_REQUEST_TIMEOUT_MS: u64 = 60_000;
pub const AI_AVAILABLE_MODELS_PATH: &str = "/aiserver.v1.AiService/AvailableModels";
pub const AVAILABLE_MODELS_REQUEST_TIMEOUT_MS: u64 = 15_000;
pub const AVAILABLE_MODELS_SCOPE_USER_AVAILABLE: u64 = 1;
pub const SAND_DEFAULT_GENERATE_IMAGE_MODEL_ID: &str = "grok-4.5";
pub const DEFAULT_CURSOR_BACKEND_URL: &str = "https://api2.cursor.sh";
pub const SAND_INFERENCE_RENEWAL_CREDENTIAL_ENV: &str =
    "SAND_INFERENCE_RENEWAL_CREDENTIAL";
const RENEWAL_PATH: &str = "/sand-box/inference-credential";
const SAND_CLIENT_TYPE: &str = "sand";
const DEFAULT_CLIENT_VERSION: &str = "0.1.0";
const DEFAULT_CREDENTIAL_TTL_MS: u64 = 15 * 60_000;
const REFRESH_LEEWAY_MS: u64 = 60_000;
const MAX_RPC_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const PRIVACY_MODE_CACHE_MAX_AGE_MS: u64 = 5 * 60_000;
const PRIVACY_MODE_FALLBACK_CACHE_MAX_AGE_MS: u64 = 10_000;
const PRIVACY_MODE_FETCH_TIMEOUT_MS: u64 = 10_000;
static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum SandPrivacyMode {
    Unspecified = 0,
    NoStorage = 1,
    NoTraining = 2,
    UsageDataTrainingAllowed = 3,
    UsageCodebaseTrainingAllowed = 4,
}

impl SandPrivacyMode {
    fn from_proto(value: u64) -> Option<Self> {
        match value {
            0 => Some(Self::Unspecified),
            1 => Some(Self::NoStorage),
            2 => Some(Self::NoTraining),
            3 => Some(Self::UsageDataTrainingAllowed),
            4 => Some(Self::UsageCodebaseTrainingAllowed),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
struct PrivacyCacheEntry {
    backend_url: String,
    account_scope: String,
    value: Option<SandPrivacyMode>,
    expires_at: Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandMcpBackendConfiguration {
    pub backend_url: String,
    pub client_version: String,
    pub box_namespace: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandMcpBackendCredentials {
    pub backend_url: String,
    pub access_token: String,
    pub machine_id: String,
    pub client_version: String,
    pub box_namespace: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpServerToolSummary {
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpServerAccountSummary {
    pub account_key: String,
    pub row_server_identifier: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum McpToolExecutionBackendResult {
    Settled(Value),
    OutcomeUnknown { message: String },
}

pub trait SandMcpCredentialProvider: Send + Sync {
    fn configuration(&self) -> SandMcpBackendConfiguration;
    fn credentials(&self) -> Result<SandMcpBackendCredentials, String>;
}

#[derive(Clone, Debug)]
struct CachedCredential {
    access_token: String,
    expires_at_ms: u64,
}

pub struct ProcessSandMcpCredentialProvider {
    backend_url: String,
    renewal_credential: Option<String>,
    machine_id: String,
    client_version: String,
    box_namespace: String,
    cached: Mutex<Option<CachedCredential>>,
    agent: ureq::Agent,
}

impl ProcessSandMcpCredentialProvider {
    pub fn configuration(&self) -> SandMcpBackendConfiguration {
        SandMcpBackendConfiguration {
            backend_url: self.backend_url.clone(),
            client_version: self.client_version.clone(),
            box_namespace: self.box_namespace.clone(),
        }
    }

    pub fn from_environment(machine_id: impl Into<String>) -> Result<Self, String> {
        let machine_id = machine_id.into();
        if machine_id.trim().is_empty() {
            return Err("MCP backend machine id is required".into());
        }
        let backend_url = env::var("SAND_BACKEND_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                env::var("CURSOR_API_BASE_URL")
                    .ok()
                    .filter(|value| !value.trim().is_empty())
            })
            .unwrap_or_else(|| DEFAULT_CURSOR_BACKEND_URL.to_string());
        Url::parse(&backend_url)
            .map_err(|error| format!("invalid MCP backend URL: {error}"))?;
        let renewal_credential = env::var(SAND_INFERENCE_RENEWAL_CREDENTIAL_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty());
        Ok(Self {
            backend_url,
            renewal_credential,
            machine_id,
            client_version: sand_client_version(),
            box_namespace: sand_box_namespace(),
            cached: Mutex::new(None),
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(10))
                .timeout_read(Duration::from_secs(20))
                .timeout_write(Duration::from_secs(20))
                .redirects(0)
                .build(),
        })
    }

    #[cfg(test)]
    fn for_test(
        machine_id: &str,
        backend_url: &str,
        renewal_credential: Option<&str>,
    ) -> Self {
        Self {
            backend_url: backend_url.to_string(),
            renewal_credential: renewal_credential.map(str::to_string),
            machine_id: machine_id.to_string(),
            client_version: "0.1.0-dev".to_string(),
            box_namespace: "dev".to_string(),
            cached: Mutex::new(None),
            agent: ureq::AgentBuilder::new().redirects(0).build(),
        }
    }

    fn renew(&self, renewal_credential: &str) -> Result<CachedCredential, String> {
        let base = Url::parse(&self.backend_url)
            .map_err(|error| format!("invalid MCP backend URL: {error}"))?;
        let url = base
            .join(RENEWAL_PATH)
            .map_err(|error| format!("invalid MCP credential renewal URL: {error}"))?;
        let response = self
            .agent
            .post(url.as_str())
            .set("content-type", "application/json")
            .set("x-cursor-client-type", SAND_CLIENT_TYPE)
            .set("x-cursor-client-version", &self.client_version)
            .set("x-sand-box-namespace", &self.box_namespace)
            .send_json(json!({ "credential": renewal_credential }))
            .map_err(|error| format!("MCP credential renewal failed: {error}"))?;
        let parsed: Value = response
            .into_json()
            .map_err(|error| format!("MCP credential renewal response was invalid: {error}"))?;
        let access_token = parsed
            .get("accessToken")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or("MCP credential renewal returned no access token")?
            .to_string();
        let now = system_now_ms();
        let expires_at_ms = parsed
            .get("expiresAtMs")
            .and_then(Value::as_u64)
            .or_else(|| jwt_expiry_ms(&access_token))
            .unwrap_or_else(|| now.saturating_add(DEFAULT_CREDENTIAL_TTL_MS));
        Ok(CachedCredential {
            access_token,
            expires_at_ms,
        })
    }
}

impl SandMcpCredentialProvider for ProcessSandMcpCredentialProvider {
    fn configuration(&self) -> SandMcpBackendConfiguration {
        ProcessSandMcpCredentialProvider::configuration(self)
    }

    fn credentials(&self) -> Result<SandMcpBackendCredentials, String> {
        let now = system_now_ms();
        if let Some(cached) = self
            .cached
            .lock()
            .map_err(|_| "MCP credential cache lock poisoned".to_string())?
            .clone()
            .filter(|credential| {
                credential.expires_at_ms > now.saturating_add(REFRESH_LEEWAY_MS)
            })
        {
            return Ok(SandMcpBackendCredentials {
                backend_url: self.backend_url.clone(),
                access_token: cached.access_token,
                machine_id: self.machine_id.clone(),
                client_version: self.client_version.clone(),
                box_namespace: self.box_namespace.clone(),
            });
        }

        // Desktop explicitly keeps the Fabushi account bearer out of Cursor/Sand
        // Connect RPCs. Android follows the same audience boundary and only accepts
        // the dedicated Sand renewal credential here.
        let renewal = self
            .renewal_credential
            .as_deref()
            .ok_or("MCP backend credential is unavailable")?;
        let renewed = self.renew(renewal)?;
        *self
            .cached
            .lock()
            .map_err(|_| "MCP credential cache lock poisoned".to_string())? =
            Some(renewed.clone());
        Ok(SandMcpBackendCredentials {
            backend_url: self.backend_url.clone(),
            access_token: renewed.access_token,
            machine_id: self.machine_id.clone(),
            client_version: self.client_version.clone(),
            box_namespace: self.box_namespace.clone(),
        })
    }
}

pub trait McpAuthBackendPort: Send + Sync {
    fn check_auth_status(
        &self,
        server_id: i32,
        account_key: &str,
        oauth_redirect_uri: &str,
        force_reauth: bool,
    ) -> Result<McpBackendAuthStatus, String>;

    fn complete_oauth(&self, state_id: &str, authorization_code: &str) -> Result<(), String>;

    fn validate_token(&self, server_url: &str, account_key: &str) -> Result<bool, String>;

    fn reload_server_tools(&self, server_id: &str) -> Result<usize, String>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedImageBackendResult {
    pub image_data: String,
    pub mime_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GenerateImageBackendError {
    #[error("{0}")]
    Generate(String),
    #[error("{0}")]
    ModelRestricted(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedbackSubmitBackendError {
    NotSignedIn,
    InvalidFeedback,
    SubscriptionRequired,
    AccessDenied,
    RateLimited,
    Unavailable,
}

impl FeedbackSubmitBackendError {
    pub fn code(self) -> &'static str {
        match self {
            Self::NotSignedIn => "not-signed-in",
            Self::InvalidFeedback => "invalid-feedback",
            Self::SubscriptionRequired => "subscription-required",
            Self::AccessDenied => "access-denied",
            Self::RateLimited => "rate-limited",
            Self::Unavailable => "unavailable",
        }
    }
}

fn feedback_error_for_status(status: u16) -> FeedbackSubmitBackendError {
    match status {
        400 => FeedbackSubmitBackendError::InvalidFeedback,
        402 => FeedbackSubmitBackendError::SubscriptionRequired,
        403 => FeedbackSubmitBackendError::AccessDenied,
        429 => FeedbackSubmitBackendError::RateLimited,
        _ => FeedbackSubmitBackendError::Unavailable,
    }
}

pub struct CursorDashboardMcpAuthBackend {
    credentials: Arc<dyn SandMcpCredentialProvider>,
    privacy_cache: Mutex<Option<PrivacyCacheEntry>>,
}

impl CursorDashboardMcpAuthBackend {
    pub fn new(credentials: Arc<dyn SandMcpCredentialProvider>) -> Self {
        Self {
            credentials,
            privacy_cache: Mutex::new(None),
        }
    }

    pub fn from_process_environment(machine_id: impl Into<String>) -> Result<Self, String> {
        let credentials: Arc<dyn SandMcpCredentialProvider> = Arc::new(
            ProcessSandMcpCredentialProvider::from_environment(machine_id)?,
        );
        Ok(Self::new(credentials))
    }

    pub fn credential_provider(&self) -> Arc<dyn SandMcpCredentialProvider> {
        Arc::clone(&self.credentials)
    }

    fn send_unary(
        &self,
        credentials: &SandMcpBackendCredentials,
        path: &str,
        body: &[u8],
        timeout_ms: u64,
        ghost_mode: &str,
    ) -> Result<Vec<u8>, String> {
        let base = Url::parse(&credentials.backend_url)
            .map_err(|error| format!("invalid MCP backend URL: {error}"))?;
        let url = base
            .join(path)
            .map_err(|error| format!("invalid MCP backend RPC URL: {error}"))?;
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(timeout.min(Duration::from_secs(10)))
            .timeout_read(timeout)
            .timeout_write(timeout)
            .redirects(0)
            .build();
        let authorization = format!("Bearer {}", credentials.access_token);
        let checksum = create_cursor_checksum(&credentials.machine_id, system_now_ms());
        let request_id = next_request_id();
        let response = agent
            .post(url.as_str())
            .set("content-type", "application/proto")
            .set("connect-protocol-version", "1")
            .set("authorization", &authorization)
            .set("x-cursor-checksum", &checksum)
            .set("x-cursor-client-type", SAND_CLIENT_TYPE)
            .set("x-cursor-client-version", &credentials.client_version)
            .set("x-sand-box-namespace", &credentials.box_namespace)
            .set("x-ghost-mode", ghost_mode)
            .set("x-request-id", &request_id)
            .send_bytes(body)
            .map_err(|error| format!("MCP backend RPC failed: {error}"))?;
        let mut reader = response.into_reader();
        let mut bytes = Vec::new();
        reader
            .by_ref()
            .take(MAX_RPC_RESPONSE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("MCP backend RPC response read failed: {error}"))?;
        if bytes.len() > MAX_RPC_RESPONSE_BYTES {
            return Err("MCP backend RPC response exceeds bounded size".into());
        }
        Ok(bytes)
    }

    pub fn generate_image(
        &self,
        description: &str,
        reference_images: &[(String, String)],
    ) -> Result<GeneratedImageBackendResult, GenerateImageBackendError> {
        let description = description.trim();
        if description.is_empty() {
            return Err(GenerateImageBackendError::Generate(
                "Image generation requires a description.".into(),
            ));
        }
        let credentials = self
            .credentials
            .credentials()
            .map_err(GenerateImageBackendError::Generate)?;
        let model_id = env::var("SAND_AGENT_MODEL")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| SAND_DEFAULT_GENERATE_IMAGE_MODEL_ID.to_string());
        let request = encode_generate_image_request(description, reference_images, &model_id);
        let ghost_mode = self.resolve_ghost_mode(&credentials);
        let response = self
            .send_unary(
                &credentials,
                RUN_GENERATE_IMAGE_PATH,
                &request,
                GENERATE_IMAGE_REQUEST_TIMEOUT_MS,
                ghost_mode,
            )
            .map_err(GenerateImageBackendError::Generate)?;
        decode_generate_image_response(&response)
    }

    /// Resolve the authenticated user's model picker catalog from the same Cursor backend
    /// credential owner used by inference-adjacent account services. Presentation receives an
    /// immutable projection and never becomes a second catalog owner.
    pub fn resolve_available_models(&self) -> Result<Value, String> {
        let credentials = self.credentials.credentials()?;
        let ghost_mode = self.resolve_ghost_mode(&credentials);
        let response = self.send_unary(
            &credentials,
            AI_AVAILABLE_MODELS_PATH,
            &encode_available_models_request(),
            AVAILABLE_MODELS_REQUEST_TIMEOUT_MS,
            ghost_mode,
        )?;
        decode_available_models_response(&response)
    }

    pub fn submit_feedback_report(
        &self,
        payload: &Value,
    ) -> Result<(), FeedbackSubmitBackendError> {
        let credentials = self
            .credentials
            .credentials()
            .map_err(|_| FeedbackSubmitBackendError::NotSignedIn)?;
        let base = Url::parse(&credentials.backend_url)
            .map_err(|_| FeedbackSubmitBackendError::Unavailable)?;
        let url = base
            .join(FEEDBACK_SUBMIT_PATH)
            .map_err(|_| FeedbackSubmitBackendError::Unavailable)?;
        if url.origin() != base.origin() {
            return Err(FeedbackSubmitBackendError::Unavailable);
        }
        let timeout = Duration::from_millis(FEEDBACK_SUBMIT_TIMEOUT_MS);
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(timeout.min(Duration::from_secs(10)))
            .timeout_read(timeout)
            .timeout_write(timeout)
            .redirects(0)
            .build();
        let authorization = format!("Bearer {}", credentials.access_token);
        let checksum = create_cursor_checksum(&credentials.machine_id, system_now_ms());
        let request_id = next_request_id();
        match agent
            .post(url.as_str())
            .set("content-type", "application/json")
            .set("accept", "application/json")
            .set("authorization", &authorization)
            .set("x-cursor-checksum", &checksum)
            .set("x-cursor-client-type", SAND_CLIENT_TYPE)
            .set("x-cursor-client-version", &credentials.client_version)
            .set("x-sand-box-namespace", &credentials.box_namespace)
            .set("x-request-id", &request_id)
            .send_json(payload.clone())
        {
            Ok(_) => Ok(()),
            Err(ureq::Error::Status(status, _)) => Err(feedback_error_for_status(status)),
            Err(ureq::Error::Transport(_)) => Err(FeedbackSubmitBackendError::Unavailable),
        }
    }

    pub fn submit_account_telemetry(
        &self,
        entries: &[AccountTelemetryProjection],
    ) -> Result<(u64, u64), String> {
        if entries.is_empty() {
            return Ok((0, 0));
        }
        let credentials = self.credentials.credentials()?;
        let request = encode_account_telemetry_submit_logs_request(
            entries,
            &credentials.machine_id,
            &credentials.client_version,
            system_now_ms(),
        )?;
        let ghost_mode = self.resolve_ghost_mode(&credentials);
        let response = self.send_unary(
            &credentials,
            ANALYTICS_SUBMIT_LOGS_PATH,
            &request,
            ACCOUNT_TELEMETRY_SUBMIT_TIMEOUT_MS,
            ghost_mode,
        )?;
        let receipt = decode_submit_logs_response(&response)?;
        if !receipt.success {
            return Err(receipt
                .error_message
                .unwrap_or_else(|| "AnalyticsService.SubmitLogs rejected account telemetry".into()));
        }
        let expected = entries.len() as u64;
        if receipt.logs_processed.saturating_add(receipt.logs_dropped) != expected {
            return Err(format!(
                "AnalyticsService.SubmitLogs returned invalid receipt: processed={} dropped={} expected={expected}",
                receipt.logs_processed, receipt.logs_dropped,
            ));
        }
        Ok((receipt.logs_processed, receipt.logs_dropped))
    }

    pub fn list_server_snapshots(&self) -> Result<Vec<McpAuthServerSnapshot>, String> {
        let credentials = self.credentials.credentials()?;
        let ghost_mode = self.resolve_ghost_mode(&credentials);
        let response = self.send_unary(
            &credentials,
            DASHBOARD_GET_AVAILABLE_MCP_SERVERS_PATH,
            &[],
            CONTROL_RPC_TIMEOUT_MS,
            ghost_mode,
        )?;
        decode_available_mcp_servers_response(&response)?
            .into_iter()
            .filter(|server| server.enabled)
            .map(project_available_server)
            .collect()
    }

    pub fn list_server_tools(&self, server_id: &str) -> Result<Vec<McpServerToolSummary>, String> {
        let requested_id = server_id
            .parse::<i32>()
            .map_err(|_| "MCP server id is outside signed int32 range".to_string())?;
        let credentials = self.credentials.credentials()?;
        let ghost_mode = self.resolve_ghost_mode(&credentials);
        let available = self.send_unary(
            &credentials,
            DASHBOARD_GET_AVAILABLE_MCP_SERVERS_PATH,
            &[],
            CONTROL_RPC_TIMEOUT_MS,
            ghost_mode,
        )?;
        let server = decode_available_mcp_servers_response(&available)?
            .into_iter()
            .find(|server| server.id == requested_id)
            .ok_or("MCP server disappeared during tool discovery")?;
        if server.disabled_by_team_admin_policy {
            return Err("MCP server is blocked by current team admin policy".into());
        }
        let identifier = server
            .server_identifier
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or("MCP server has no backend server identifier")?;
        let request = encode_list_sand_mcp_tools_request(&[identifier]);
        let response = self.send_unary(
            &credentials,
            DASHBOARD_LIST_SAND_MCP_TOOLS_PATH,
            &request,
            LIST_TOOLS_TIMEOUT_MS,
            ghost_mode,
        )?;
        decode_list_sand_mcp_tools(&response, identifier)
    }

    pub fn reload_server_tools(&self, server_id: &str) -> Result<usize, String> {
        Ok(self.list_server_tools(server_id)?.len())
    }

    pub fn list_server_accounts(
        &self,
        server_id: &str,
    ) -> Result<Vec<McpServerAccountSummary>, String> {
        let requested_id = parse_mcp_server_id(server_id)?;
        let credentials = self.credentials.credentials()?;
        let ghost_mode = self.resolve_ghost_mode(&credentials);
        let available = self.send_unary(
            &credentials,
            DASHBOARD_GET_AVAILABLE_MCP_SERVERS_PATH,
            &[],
            CONTROL_RPC_TIMEOUT_MS,
            ghost_mode,
        )?;
        let server = decode_available_mcp_servers_response(&available)?
            .into_iter()
            .find(|server| server.id == requested_id)
            .ok_or("MCP server disappeared during account discovery")?;
        if server.disabled_by_team_admin_policy {
            return Err("MCP server is blocked by current team admin policy".into());
        }
        let identifier = server
            .server_identifier
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or("MCP server has no backend server identifier")?;
        let response = self.send_unary(
            &credentials,
            DASHBOARD_LIST_SAND_MCP_TOOLS_PATH,
            &encode_list_sand_mcp_tools_request(&[identifier]),
            LIST_TOOLS_TIMEOUT_MS,
            ghost_mode,
        )?;
        decode_list_sand_mcp_accounts(&response, identifier)
    }

    pub fn execute_server_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        args: &Value,
        tool_call_id: &str,
        agent_id: Option<&str>,
    ) -> Result<McpToolExecutionBackendResult, String> {
        let requested_id = parse_mcp_server_id(server_id)?;
        let tool_name = validate_mcp_tool_execution_string(tool_name, "tool name", 512)?;
        let tool_call_id =
            validate_mcp_tool_execution_string(tool_call_id, "tool call id", 512)?;
        let agent_id = agent_id
            .map(|value| validate_mcp_tool_execution_string(value, "agent id", 512))
            .transpose()?
            .unwrap_or("");
        let encoded_args = encode_json_struct(args, 0)?;
        if encoded_args.len() > MAX_MCP_TOOL_ARGUMENT_BYTES {
            return Err("MCP tool arguments exceed bounded size".into());
        }

        let credentials = self.credentials.credentials()?;
        let ghost_mode = self.resolve_ghost_mode(&credentials);
        let available = self.send_unary(
            &credentials,
            DASHBOARD_GET_AVAILABLE_MCP_SERVERS_PATH,
            &[],
            CONTROL_RPC_TIMEOUT_MS,
            ghost_mode,
        )?;
        let server = decode_available_mcp_servers_response(&available)?
            .into_iter()
            .find(|server| server.id == requested_id)
            .ok_or("MCP server disappeared during tool execution")?;
        if server.disabled_by_team_admin_policy {
            return Err("MCP server is blocked by current team admin policy".into());
        }
        let identifier = server
            .server_identifier
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or("MCP server has no backend server identifier")?;

        let request = encode_execute_sand_mcp_tool_request(
            identifier,
            tool_name,
            &encoded_args,
            tool_call_id,
            agent_id,
        );
        match self.send_unary(
            &credentials,
            DASHBOARD_EXECUTE_SAND_MCP_TOOL_PATH,
            &request,
            EXECUTE_TOOL_TIMEOUT_MS,
            ghost_mode,
        ) {
            Ok(response) => Ok(McpToolExecutionBackendResult::Settled(
                decode_execute_sand_mcp_tool_response(&response)?,
            )),
            Err(error) if is_timeout_like_rpc_error(&error) => {
                Ok(McpToolExecutionBackendResult::OutcomeUnknown {
                    message: format!(
                        "Backend MCP execution for \"{tool_name}\" timed out after {}s. The connector may still have applied it, so retry only if repeating the call is safe.",
                        EXECUTE_TOOL_TIMEOUT_MS / 1_000
                    ),
                })
            }
            Err(error) => Ok(McpToolExecutionBackendResult::Settled(json!({
                "case": "error",
                "error": format!("Backend MCP execution failed for \"{tool_name}\": {error}"),
            }))),
        }
    }

    pub fn fresh_server_snapshot(
        &self,
        server_id: &str,
    ) -> Result<Option<McpAuthServerSnapshot>, String> {
        let requested_id = server_id
            .parse::<i32>()
            .map_err(|_| "MCP server id is outside signed int32 range".to_string())?;
        let credentials = self.credentials.credentials()?;
        let ghost_mode = self.resolve_ghost_mode(&credentials);
        let response = self.send_unary(
            &credentials,
            DASHBOARD_GET_AVAILABLE_MCP_SERVERS_PATH,
            &[],
            CONTROL_RPC_TIMEOUT_MS,
            ghost_mode,
        )?;
        let servers = decode_available_mcp_servers_response(&response)?;
        servers
            .into_iter()
            .find(|server| server.id == requested_id)
            .map(project_available_server)
            .transpose()
    }

    pub fn logout_mcp_account(
        &self,
        server_url: &str,
        account_key: &str,
    ) -> Result<(), String> {
        let server_url = server_url.trim();
        let account_key = account_key.trim();
        let parsed = Url::parse(server_url)
            .map_err(|_| "MCP account logout server URL is invalid".to_string())?;
        if !matches!(parsed.scheme(), "http" | "https")
            || account_key.is_empty()
            || account_key.len() > 320
            || account_key.chars().any(char::is_control)
        {
            return Err("MCP account logout target is invalid".into());
        }
        let credentials = self.credentials.credentials()?;
        let ghost_mode = self.resolve_ghost_mode(&credentials);
        self.send_unary(
            &credentials,
            DASHBOARD_DELETE_MCP_OAUTH_TOKEN_PATH,
            &encode_delete_mcp_oauth_token_request(server_url, account_key),
            CONTROL_RPC_TIMEOUT_MS,
            ghost_mode,
        )?;
        Ok(())
    }

    pub fn rename_mcp_account(
        &self,
        server_id: &str,
        account_key: &str,
        new_account_key: &str,
    ) -> Result<(), String> {
        let server_id = parse_mcp_server_id(server_id)?;
        let account_key = validate_mcp_account_key(account_key, "account key")?;
        let new_account_key = validate_mcp_account_key(new_account_key, "new account key")?;
        if account_key == new_account_key {
            return Ok(());
        }
        let credentials = self.credentials.credentials()?;
        let ghost_mode = self.resolve_ghost_mode(&credentials);
        self.send_unary(
            &credentials,
            DASHBOARD_RENAME_MCP_OAUTH_ACCOUNT_PATH,
            &encode_rename_mcp_oauth_account_request(server_id, account_key, new_account_key),
            CONTROL_RPC_TIMEOUT_MS,
            ghost_mode,
        )?;
        Ok(())
    }

    pub fn delete_mcp_account(
        &self,
        server_id: &str,
        account_key: &str,
    ) -> Result<(), String> {
        let server_id = parse_mcp_server_id(server_id)?;
        let account_key = validate_mcp_account_key(account_key, "account key")?;
        let credentials = self.credentials.credentials()?;
        let ghost_mode = self.resolve_ghost_mode(&credentials);
        self.send_unary(
            &credentials,
            DASHBOARD_DELETE_MCP_OAUTH_ACCOUNT_PATH,
            &encode_delete_mcp_oauth_account_request(server_id, account_key),
            CONTROL_RPC_TIMEOUT_MS,
            ghost_mode,
        )?;
        Ok(())
    }

    pub fn resolve_sand_team_rules(&self) -> Result<Vec<Value>, String> {
        let credentials = self.credentials.credentials()?;
        let ghost_mode = self.resolve_ghost_mode(&credentials);
        let teams = self.send_unary(
            &credentials,
            DASHBOARD_GET_TEAMS_PATH,
            &[0x08, 0x01],
            TEAM_RULES_REQUEST_TIMEOUT_MS,
            ghost_mode,
        )?;
        let team_ids = decode_direct_team_ids(&teams)?;
        if team_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut merged = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for team_id in team_ids {
            let response = self.send_unary(
                &credentials,
                DASHBOARD_GET_TEAM_RULES_PATH,
                &encode_get_team_rules_request(team_id),
                TEAM_RULES_REQUEST_TIMEOUT_MS,
                ghost_mode,
            )?;
            for rule in decode_sand_team_rules(&response)? {
                let path = rule
                    .get("fullPath")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if !path.is_empty() && seen.insert(path) {
                    merged.push(rule);
                }
            }
        }
        Ok(merged)
    }

    /// Desktop marketplace parity: resolve team-wide plugin install popularity.
    ///
    /// The shipped Desktop surface treats this ranking metadata as best-effort: no team or any
    /// Dashboard failure yields an empty map rather than failing the marketplace itself.
    pub fn resolve_team_plugin_popularity(
        &self,
    ) -> std::collections::BTreeMap<String, u64> {
        self.try_resolve_team_plugin_popularity().unwrap_or_default()
    }

    fn try_resolve_team_plugin_popularity(
        &self,
    ) -> Result<std::collections::BTreeMap<String, u64>, String> {
        let credentials = self.credentials.credentials()?;
        let ghost_mode = self.resolve_ghost_mode(&credentials);
        let me = self.send_unary(
            &credentials,
            DASHBOARD_GET_ME_PATH,
            &[],
            TEAM_PLUGIN_POPULARITY_REQUEST_TIMEOUT_MS,
            ghost_mode,
        )?;
        let Some(team_id) = decode_optional_varint_field(&me, 7)? else {
            return Ok(std::collections::BTreeMap::new());
        };
        if team_id > i32::MAX as u64 {
            return Err("Dashboard team id is outside signed int32 range".into());
        }
        let popularity = self.send_unary(
            &credentials,
            DASHBOARD_GET_TEAM_PLUGIN_POPULARITY_PATH,
            &encode_get_team_plugin_popularity_request(team_id as i32),
            TEAM_PLUGIN_POPULARITY_REQUEST_TIMEOUT_MS,
            ghost_mode,
        )?;
        decode_team_plugin_popularity_response(&popularity)
    }

    pub fn resolve_local_tool_permission_ceiling(&self) -> Option<&'static str> {
        self.try_resolve_local_tool_permission_ceiling()
            .ok()
            .flatten()
    }

    pub fn try_resolve_local_tool_permission_ceiling(
        &self,
    ) -> Result<Option<&'static str>, String> {
        let credentials = self.credentials.credentials()?;
        let ghost_mode = self.resolve_ghost_mode(&credentials);
        let response = self.send_unary(
            &credentials,
            DASHBOARD_GET_TEAM_ADMIN_SETTINGS_PATH,
            &[],
            PR_REVIEW_REQUEST_TIMEOUT_MS,
            ghost_mode,
        )?;
        decode_local_tool_permission_ceiling(&response)
    }

    pub fn resolve_sand_pr_review_preferences(&self) -> Result<Value, String> {
        let credentials = self.credentials.credentials()?;
        let ghost_mode = self.resolve_ghost_mode(&credentials);
        std::thread::scope(|scope| {
            let user = scope.spawn(|| {
                self.send_unary(
                    &credentials,
                    BACKGROUND_COMPOSER_GET_USER_SETTINGS_PATH,
                    &[],
                    PR_REVIEW_REQUEST_TIMEOUT_MS,
                    ghost_mode,
                )
            });
            let team = scope.spawn(|| {
                self.send_unary(
                    &credentials,
                    DASHBOARD_GET_TEAM_ADMIN_SETTINGS_PATH,
                    &[],
                    PR_REVIEW_REQUEST_TIMEOUT_MS,
                    ghost_mode,
                )
            });
            let user_bytes = user
                .join()
                .map_err(|_| "PR review user preference request panicked".to_string())??;
            let team_bytes = team
                .join()
                .map_err(|_| "PR review team preference request panicked".to_string())??;
            Ok(json!({
                "user": decode_user_pr_review_destination(&user_bytes)?,
                "team": decode_team_pr_review_destination(&team_bytes)?,
            }))
        })
    }

    pub fn resolve_sand_access_status(&self) -> Result<Value, String> {
        let credentials = self.credentials.credentials()?;
        let ghost_mode = self.resolve_ghost_mode(&credentials);
        let response = self.send_unary(
            &credentials,
            DASHBOARD_GET_SAND_ACCESS_STATUS_PATH,
            &[],
            SAND_ACCESS_REQUEST_TIMEOUT_MS,
            ghost_mode,
        )?;
        decode_sand_access_status(&response)
    }

    pub fn resolve_sand_usage_summary(&self) -> Result<Value, String> {
        let credentials = self.credentials.credentials()?;
        let ghost_mode = self.resolve_ghost_mode(&credentials);
        let status = self.send_unary(
            &credentials,
            DASHBOARD_GET_SAND_USAGE_STATUS_PATH,
            &[],
            USAGE_REQUEST_TIMEOUT_MS,
            ghost_mode,
        )?;
        let current_period = self.send_unary(
            &credentials,
            DASHBOARD_GET_CURRENT_PERIOD_USAGE_PATH,
            &[],
            USAGE_REQUEST_TIMEOUT_MS,
            ghost_mode,
        )?;
        let trial = self.send_unary(
            &credentials,
            DASHBOARD_GET_SAND_TRIAL_CLAIM_STATUS_PATH,
            &[],
            USAGE_REQUEST_TIMEOUT_MS,
            ghost_mode,
        )?;
        let teams = self.send_unary(
            &credentials,
            DASHBOARD_GET_TEAMS_PATH,
            &[],
            USAGE_REQUEST_TIMEOUT_MS,
            ghost_mode,
        )?;
        decode_sand_usage_summary(&status, &current_period, &trial, &teams, system_now_ms())
    }

    pub fn cancel_sand_trial(&self) -> Result<(), String> {
        let credentials = self.credentials.credentials()?;
        let ghost_mode = self.resolve_ghost_mode(&credentials);
        let _ = self.send_unary(
            &credentials,
            DASHBOARD_CANCEL_SAND_TRIAL_PATH,
            &[],
            USAGE_REQUEST_TIMEOUT_MS,
            ghost_mode,
        )?;
        Ok(())
    }

    pub fn client_action(
        &self,
        action: &str,
        args: &std::collections::BTreeMap<String, String>,
    ) -> Result<Value, String> {
        let action = action.trim();
        if action.is_empty() {
            return Err("Dashboard client action is required".into());
        }
        let credentials = self.credentials.credentials()?;
        let ghost_mode = self.resolve_ghost_mode(&credentials);
        let response = self.send_unary(
            &credentials,
            DASHBOARD_CLIENT_ACTION_PATH,
            &encode_client_action_request(action, args),
            CONTROL_RPC_TIMEOUT_MS,
            ghost_mode,
        )?;
        decode_client_action_response(&response)
    }

    pub fn resolve_sand_privacy_mode(&self) -> Option<SandPrivacyMode> {
        let credentials = self.credentials.credentials().ok()?;
        self.resolve_sand_privacy_mode_with_credentials(&credentials)
    }

    fn resolve_sand_privacy_mode_with_credentials(
        &self,
        credentials: &SandMcpBackendCredentials,
    ) -> Option<SandPrivacyMode> {
        let account_scope = format!(
            "{:x}",
            Sha256::digest(credentials.access_token.as_bytes())
        );
        let now = Instant::now();
        if let Ok(cache) = self.privacy_cache.lock() {
            if let Some(entry) = cache.as_ref() {
                if entry.backend_url == credentials.backend_url
                    && entry.account_scope == account_scope
                    && now <= entry.expires_at
                {
                    return entry.value;
                }
            }
        }

        let value = self
            .send_unary(
                credentials,
                DASHBOARD_GET_USER_PRIVACY_MODE_PATH,
                &[0x08, SandPrivacyMode::NoStorage as u8],
                PRIVACY_MODE_FETCH_TIMEOUT_MS,
                "true",
            )
            .ok()
            .and_then(|bytes| decode_optional_varint_field(&bytes, 1).ok().flatten())
            .and_then(SandPrivacyMode::from_proto);
        let ttl = if value.is_some() {
            PRIVACY_MODE_CACHE_MAX_AGE_MS
        } else {
            PRIVACY_MODE_FALLBACK_CACHE_MAX_AGE_MS
        };
        if let Ok(mut cache) = self.privacy_cache.lock() {
            *cache = Some(PrivacyCacheEntry {
                backend_url: credentials.backend_url.clone(),
                account_scope,
                value,
                expires_at: now + Duration::from_millis(ttl),
            });
        }
        value
    }

    fn resolve_ghost_mode(
        &self,
        credentials: &SandMcpBackendCredentials,
    ) -> &'static str {
        ghost_mode_from_privacy_mode(
            self.resolve_sand_privacy_mode_with_credentials(credentials),
        )
    }
}

impl McpAuthBackendPort for CursorDashboardMcpAuthBackend {
    fn check_auth_status(
        &self,
        server_id: i32,
        account_key: &str,
        oauth_redirect_uri: &str,
        force_reauth: bool,
    ) -> Result<McpBackendAuthStatus, String> {
        let credentials = self.credentials.credentials()?;
        let body = encode_check_http_mcp_status_request(
            server_id,
            oauth_redirect_uri,
            force_reauth,
            account_key,
        );
        let ghost_mode = self.resolve_ghost_mode(&credentials);
        let response = self.send_unary(
            &credentials,
            DASHBOARD_CHECK_HTTP_MCP_STATUS_PATH,
            &body,
            CONTROL_RPC_TIMEOUT_MS,
            ghost_mode,
        )?;
        let statuses = decode_check_http_mcp_status_response(&response)?;
        let status = statuses
            .into_iter()
            .find(|status| status.id == server_id)
            .ok_or("MCP backend did not return the requested server status")?;
        Ok(McpBackendAuthStatus {
            is_available: status.is_available,
            requires_auth: status.requires_auth,
            has_valid_token: status.has_valid_token,
            auth_url: status.auth_url.unwrap_or_default(),
            error: status.error.unwrap_or_default(),
        })
    }

    fn complete_oauth(&self, state_id: &str, authorization_code: &str) -> Result<(), String> {
        if state_id.trim().is_empty() || authorization_code.trim().is_empty() {
            return Err("MCP OAuth completion requires state and authorization code".into());
        }
        let credentials = self.credentials.credentials()?;
        let body = encode_complete_mcp_oauth_request(state_id, authorization_code);
        let ghost_mode = self.resolve_ghost_mode(&credentials);
        self.send_unary(
            &credentials,
            DASHBOARD_COMPLETE_MCP_OAUTH_PATH,
            &body,
            CONTROL_RPC_TIMEOUT_MS,
            ghost_mode,
        )?;
        Ok(())
    }

    fn validate_token(&self, server_url: &str, account_key: &str) -> Result<bool, String> {
        let credentials = self.credentials.credentials()?;
        let body = encode_validate_mcp_oauth_tokens_request(server_url, account_key);
        let ghost_mode = self.resolve_ghost_mode(&credentials);
        let response = self.send_unary(
            &credentials,
            DASHBOARD_VALIDATE_MCP_OAUTH_TOKENS_PATH,
            &body,
            CONTROL_RPC_TIMEOUT_MS,
            ghost_mode,
        )?;
        let results = decode_validate_mcp_oauth_tokens_response(&response)?;
        Ok(results.into_iter().any(|result| {
            result.server_url == server_url
                && result.account_key == account_key
                && result.has_valid_token
        }))
    }

    fn reload_server_tools(&self, server_id: &str) -> Result<usize, String> {
        CursorDashboardMcpAuthBackend::reload_server_tools(self, server_id)
    }
}

fn project_available_server(
    server: AvailableServerWire,
) -> Result<McpAuthServerSnapshot, String> {
    if server.id <= 0 {
        return Err("MCP server id is invalid".into());
    }
    let name = server.name.trim();
    if name.is_empty() || name.len() > 320 || name.chars().any(char::is_control) {
        return Err("MCP server name is invalid".into());
    }
    let transport = match server.transport.trim().to_ascii_lowercase().as_str() {
        "stdio" => McpAuthTransport::Stdio,
        "sse" => McpAuthTransport::Sse,
        "http" | "streamable-http" | "streamable_http" => McpAuthTransport::Http,
        _ => return Err("MCP server transport is unsupported".into()),
    };
    let server_url = server
        .url
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if !matches!(transport, McpAuthTransport::Stdio) {
        let parsed = Url::parse(server_url.as_deref().ok_or("MCP server URL is required")?)
            .map_err(|_| "MCP server URL is invalid".to_string())?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("MCP server URL is invalid".into());
        }
    }
    Ok(McpAuthServerSnapshot {
        server_id: server.id.to_string(),
        server_name: name.to_string(),
        server_url,
        transport,
        disabled_by_team_admin_policy: server.disabled_by_team_admin_policy,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AvailableServerWire {
    id: i32,
    name: String,
    enabled: bool,
    transport: String,
    url: Option<String>,
    disabled_by_team_admin_policy: bool,
    server_identifier: Option<String>,
}

fn ghost_mode_from_privacy_mode(mode: Option<SandPrivacyMode>) -> &'static str {
    match mode {
        Some(SandPrivacyMode::UsageDataTrainingAllowed)
        | Some(SandPrivacyMode::UsageCodebaseTrainingAllowed) => "false",
        _ => "true",
    }
}

fn decode_available_mcp_servers_response(
    input: &[u8],
) -> Result<Vec<AvailableServerWire>, String> {
    let mut cursor = 0usize;
    let mut servers = Vec::new();
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        if field == 1 && wire == 2 {
            servers.push(decode_available_server(read_len_delimited(input, &mut cursor)?)?);
        } else {
            skip_field(input, &mut cursor, wire)?;
        }
    }
    Ok(servers)
}

fn decode_available_server(input: &[u8]) -> Result<AvailableServerWire, String> {
    let mut cursor = 0usize;
    let mut server = AvailableServerWire {
        id: 0,
        name: String::new(),
        enabled: false,
        transport: String::new(),
        url: None,
        disabled_by_team_admin_policy: false,
        server_identifier: None,
    };
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        match (field, wire) {
            (1, 0) => server.id = read_varint(input, &mut cursor)? as i32,
            (2, 2) => server.name = read_string(input, &mut cursor)?,
            (4, 0) => server.enabled = read_varint(input, &mut cursor)? != 0,
            (5, 2) => server.transport = read_string(input, &mut cursor)?,
            (8, 2) => server.url = Some(read_string(input, &mut cursor)?),
            (14, 0) => {
                server.disabled_by_team_admin_policy =
                    read_varint(input, &mut cursor)? != 0
            }
            (15, 2) => server.server_identifier = Some(read_string(input, &mut cursor)?),
            _ => skip_field(input, &mut cursor, wire)?,
        }
    }
    Ok(server)
}

fn validate_mcp_tool_execution_string<'a>(
    raw: &'a str,
    label: &str,
    max_len: usize,
) -> Result<&'a str, String> {
    let value = raw.trim();
    if value.is_empty() || value.len() > max_len || value.chars().any(char::is_control) {
        return Err(format!("MCP {label} is invalid"));
    }
    Ok(value)
}

fn encode_execute_sand_mcp_tool_request(
    server_identifier: &str,
    tool_name: &str,
    encoded_args: &[u8],
    tool_call_id: &str,
    agent_id: &str,
) -> Vec<u8> {
    let mut output = Vec::new();
    encode_string(1, server_identifier, &mut output);
    encode_string(2, tool_name, &mut output);
    encode_len_delimited(3, encoded_args, &mut output);
    encode_string(4, tool_call_id, &mut output);
    if !agent_id.is_empty() {
        encode_string(5, agent_id, &mut output);
    }
    output
}

fn encode_json_struct(value: &Value, depth: usize) -> Result<Vec<u8>, String> {
    if depth > MAX_PROTO_JSON_DEPTH {
        return Err("MCP tool arguments exceed maximum nesting depth".into());
    }
    let object = value
        .as_object()
        .ok_or("MCP tool arguments must be a JSON object")?;
    if object.len() > 4_096 {
        return Err("MCP tool arguments contain too many object fields".into());
    }
    let mut output = Vec::new();
    for (key, value) in object {
        if key.is_empty() || key.len() > 4_096 || key.chars().any(|ch| ch == '\0') {
            return Err("MCP tool argument key is invalid".into());
        }
        let mut entry = Vec::new();
        encode_string(1, key, &mut entry);
        let encoded_value = encode_json_value(value, depth + 1)?;
        encode_len_delimited(2, &encoded_value, &mut entry);
        encode_len_delimited(1, &entry, &mut output);
    }
    Ok(output)
}

fn encode_json_value(value: &Value, depth: usize) -> Result<Vec<u8>, String> {
    if depth > MAX_PROTO_JSON_DEPTH {
        return Err("MCP tool arguments exceed maximum nesting depth".into());
    }
    let mut output = Vec::new();
    match value {
        Value::Null => {
            encode_key(1, 0, &mut output);
            encode_varint(0, &mut output);
        }
        Value::Bool(value) => encode_bool(4, *value, &mut output),
        Value::Number(value) => {
            let number = value
                .as_f64()
                .ok_or("MCP tool argument number cannot be represented as protobuf double")?;
            if !number.is_finite() {
                return Err("MCP tool argument number must be finite".into());
            }
            encode_key(2, 1, &mut output);
            output.extend_from_slice(&number.to_bits().to_le_bytes());
        }
        Value::String(value) => encode_string(3, value, &mut output),
        Value::Array(values) => {
            if values.len() > 4_096 {
                return Err("MCP tool arguments contain too many list values".into());
            }
            let mut list = Vec::new();
            for value in values {
                let encoded = encode_json_value(value, depth + 1)?;
                encode_len_delimited(1, &encoded, &mut list);
            }
            encode_len_delimited(6, &list, &mut output);
        }
        Value::Object(_) => {
            let encoded = encode_json_struct(value, depth + 1)?;
            encode_len_delimited(5, &encoded, &mut output);
        }
    }
    Ok(output)
}

fn decode_execute_sand_mcp_tool_response(input: &[u8]) -> Result<Value, String> {
    let result = first_message_field(input, 1)?
        .ok_or("Backend MCP execution returned no result")?;
    decode_mcp_result(result)
}

fn decode_mcp_result(input: &[u8]) -> Result<Value, String> {
    let mut cursor = 0usize;
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        if wire != 2 {
            skip_field(input, &mut cursor, wire)?;
            continue;
        }
        let bytes = read_len_delimited(input, &mut cursor)?;
        return match field {
            1 => decode_mcp_success(bytes),
            2 => Ok(json!({"case":"error","error": optional_string_field(bytes, 1)?.unwrap_or_default()})),
            3 => Ok(json!({
                "case":"rejected",
                "reason": optional_string_field(bytes, 1)?.unwrap_or_default(),
                "readOnly": decode_optional_varint_field(bytes, 2)?.unwrap_or_default() != 0,
            })),
            4 => Ok(json!({
                "case":"permission-denied",
                "error": optional_string_field(bytes, 1)?.unwrap_or_default(),
                "readOnly": decode_optional_varint_field(bytes, 2)?.unwrap_or_default() != 0,
            })),
            5 => Ok(json!({
                "case":"tool-not-found",
                "name": optional_string_field(bytes, 1)?.unwrap_or_default(),
                "availableTools": string_fields(bytes, 2)?,
            })),
            6 => Ok(json!({
                "case":"server-not-found",
                "name": optional_string_field(bytes, 1)?.unwrap_or_default(),
                "availableServers": string_fields(bytes, 2)?,
            })),
            7 => Ok(json!({"case":"approved"})),
            _ => Err(format!("Backend MCP execution returned unknown result field {field}")),
        };
    }
    Err("Backend MCP execution returned empty result".into())
}

fn decode_mcp_success(input: &[u8]) -> Result<Value, String> {
    let mut content = Vec::new();
    for item in repeated_message_fields(input, 1)? {
        content.push(decode_mcp_content_item(item)?);
    }
    let is_error = decode_optional_varint_field(input, 2)?.unwrap_or_default() != 0;
    let structured = first_message_field(input, 3)?
        .map(|bytes| decode_proto_struct(bytes, 0))
        .transpose()?;
    Ok(json!({
        "case":"success",
        "content":content,
        "isError":is_error,
        "structuredContent":structured,
    }))
}

fn decode_mcp_content_item(input: &[u8]) -> Result<Value, String> {
    let mut cursor = 0usize;
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        if wire != 2 {
            skip_field(input, &mut cursor, wire)?;
            continue;
        }
        let bytes = read_len_delimited(input, &mut cursor)?;
        return match field {
            1 => Ok(json!({
                "type":"text",
                "text":optional_string_field(bytes, 1)?.unwrap_or_default(),
            })),
            2 => {
                let data = first_message_field(bytes, 1)?.unwrap_or_default();
                let mime_type = optional_string_field(bytes, 2)?.unwrap_or_default();
                Ok(json!({
                    "type":"image",
                    "dataBase64":base64::engine::general_purpose::STANDARD.encode(data),
                    "mimeType":mime_type,
                }))
            }
            _ => Err(format!("Backend MCP execution returned unknown content field {field}")),
        };
    }
    Err("Backend MCP execution returned empty content item".into())
}

fn decode_proto_struct(input: &[u8], depth: usize) -> Result<Value, String> {
    if depth > MAX_PROTO_JSON_DEPTH {
        return Err("Backend MCP structured content exceeds maximum nesting depth".into());
    }
    let mut object = serde_json::Map::new();
    for entry in repeated_message_fields(input, 1)? {
        let key = optional_string_field(entry, 1)?.unwrap_or_default();
        let value = first_message_field(entry, 2)?
            .ok_or("Backend MCP structured content map entry has no value")?;
        object.insert(key, decode_proto_value(value, depth + 1)?);
    }
    Ok(Value::Object(object))
}

fn decode_proto_value(input: &[u8], depth: usize) -> Result<Value, String> {
    if depth > MAX_PROTO_JSON_DEPTH {
        return Err("Backend MCP structured content exceeds maximum nesting depth".into());
    }
    let mut cursor = 0usize;
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        match (field, wire) {
            (1, 0) => {
                let _ = read_varint(input, &mut cursor)?;
                return Ok(Value::Null);
            }
            (2, 1) => {
                let bits = read_fixed64(input, &mut cursor)?;
                let number = f64::from_bits(bits);
                if !number.is_finite() {
                    return Err("Backend MCP structured number is non-finite".into());
                }
                return serde_json::Number::from_f64(number)
                    .map(Value::Number)
                    .ok_or_else(|| "Backend MCP structured number is invalid".into());
            }
            (3, 2) => return Ok(Value::String(read_string(input, &mut cursor)?)),
            (4, 0) => return Ok(Value::Bool(read_varint(input, &mut cursor)? != 0)),
            (5, 2) => {
                return decode_proto_struct(read_len_delimited(input, &mut cursor)?, depth + 1)
            }
            (6, 2) => {
                let list = read_len_delimited(input, &mut cursor)?;
                let values = repeated_message_fields(list, 1)?
                    .into_iter()
                    .map(|value| decode_proto_value(value, depth + 1))
                    .collect::<Result<Vec<_>, _>>()?;
                return Ok(Value::Array(values));
            }
            _ => skip_field(input, &mut cursor, wire)?,
        }
    }
    Err("Backend MCP structured content contains empty Value".into())
}

fn read_fixed64(input: &[u8], cursor: &mut usize) -> Result<u64, String> {
    let end = cursor
        .checked_add(8)
        .filter(|end| *end <= input.len())
        .ok_or("truncated MCP backend fixed64 value")?;
    let bytes: [u8; 8] = input[*cursor..end]
        .try_into()
        .map_err(|_| "truncated MCP backend fixed64 value")?;
    *cursor = end;
    Ok(u64::from_le_bytes(bytes))
}

fn is_timeout_like_rpc_error(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    normalized.contains("timed out")
        || normalized.contains("timeout")
        || normalized.contains("deadline")
}

fn encode_list_sand_mcp_tools_request(server_identifiers: &[&str]) -> Vec<u8> {
    let mut output = Vec::new();
    for identifier in server_identifiers {
        encode_string(1, identifier, &mut output);
    }
    output
}

fn decode_list_sand_mcp_tools(
    input: &[u8],
    requested_identifier: &str,
) -> Result<Vec<McpServerToolSummary>, String> {
    let mut cursor = 0usize;
    let mut matched = None;
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        if field == 1 && wire == 2 {
            let bytes = read_len_delimited(input, &mut cursor)?;
            let (identifier, tools) = decode_list_sand_mcp_server(bytes)?;
            if identifier == requested_identifier {
                matched = Some(tools);
            }
        } else {
            skip_field(input, &mut cursor, wire)?;
        }
    }
    matched.ok_or_else(|| {
        format!("MCP tool reload did not return server identifier {requested_identifier}")
    })
}

fn decode_list_sand_mcp_tools_count(
    input: &[u8],
    requested_identifier: &str,
) -> Result<usize, String> {
    Ok(decode_list_sand_mcp_tools(input, requested_identifier)?.len())
}

fn decode_list_sand_mcp_accounts(
    input: &[u8],
    requested_identifier: &str,
) -> Result<Vec<McpServerAccountSummary>, String> {
    let mut cursor = 0usize;
    let mut accounts = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        if field == 1 && wire == 2 {
            let bytes = read_len_delimited(input, &mut cursor)?;
            let (server_identifier, account_label, row_server_identifier) =
                decode_list_sand_mcp_account_row(bytes)?;
            let row_owner = row_server_identifier
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(server_identifier.as_str());
            if row_owner == requested_identifier {
                if let Some(account_key) = account_label
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                {
                    if account_key.len() > 320 || account_key.chars().any(char::is_control) {
                        return Err("MCP account label exceeds bounded metadata limits".into());
                    }
                    if seen.insert(account_key.clone()) {
                        accounts.push(McpServerAccountSummary {
                            account_key,
                            row_server_identifier: row_owner.to_string(),
                        });
                    }
                }
            }
        } else {
            skip_field(input, &mut cursor, wire)?;
        }
    }
    Ok(accounts)
}

fn decode_list_sand_mcp_account_row(
    input: &[u8],
) -> Result<(String, Option<String>, Option<String>), String> {
    let mut cursor = 0usize;
    let mut identifier = String::new();
    let mut account_label = None;
    let mut row_server_identifier = None;
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        match (field, wire) {
            (1, 2) => identifier = read_string(input, &mut cursor)?,
            (4, 2) => account_label = Some(read_string(input, &mut cursor)?),
            (5, 2) => row_server_identifier = Some(read_string(input, &mut cursor)?),
            _ => skip_field(input, &mut cursor, wire)?,
        }
    }
    Ok((identifier, account_label, row_server_identifier))
}

fn decode_list_sand_mcp_server(
    input: &[u8],
) -> Result<(String, Vec<McpServerToolSummary>), String> {
    let mut cursor = 0usize;
    let mut identifier = String::new();
    let mut tools = Vec::new();
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        match (field, wire) {
            (1, 2) => identifier = read_string(input, &mut cursor)?,
            (3, 2) => {
                let tool = decode_mcp_tool_definition(read_len_delimited(input, &mut cursor)?)?;
                if !tool.name.is_empty() && !tools.iter().any(|existing: &McpServerToolSummary| existing.name == tool.name) {
                    tools.push(tool);
                }
            }
            _ => skip_field(input, &mut cursor, wire)?,
        }
    }
    Ok((identifier, tools))
}

fn decode_mcp_tool_definition(input: &[u8]) -> Result<McpServerToolSummary, String> {
    let mut cursor = 0usize;
    let mut name = String::new();
    let mut title = None;
    let mut description = None;
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        match (field, wire) {
            // agent.v1.McpToolDefinition:
            // name=1, description=2, provider_identifier=4, tool_name=5.
            (1, 2) => {
                let value = read_string(input, &mut cursor)?;
                if !value.trim().is_empty() {
                    title = Some(value);
                }
            }
            (2, 2) => {
                let value = read_string(input, &mut cursor)?;
                if !value.trim().is_empty() {
                    description = Some(value);
                }
            }
            (5, 2) => name = read_string(input, &mut cursor)?,
            _ => skip_field(input, &mut cursor, wire)?,
        }
    }
    if name.trim().is_empty() {
        return Err("MCP tool definition is missing tool_name".into());
    }
    if name.len() > 512 || title.as_ref().is_some_and(|value| value.len() > 1_024)
        || description.as_ref().is_some_and(|value| value.len() > 16_384)
    {
        return Err("MCP tool definition exceeds bounded metadata limits".into());
    }
    Ok(McpServerToolSummary {
        name,
        title,
        description,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CheckStatusWire {
    id: i32,
    is_available: bool,
    requires_auth: bool,
    auth_url: Option<String>,
    error: Option<String>,
    has_valid_token: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ValidateResultWire {
    server_url: String,
    has_valid_token: bool,
    account_key: String,
}

fn encode_check_http_mcp_status_request(
    server_id: i32,
    oauth_redirect_uri: &str,
    force_reauth: bool,
    account_key: &str,
) -> Vec<u8> {
    let mut output = Vec::new();
    let mut packed = Vec::new();
    encode_varint(server_id as u32 as u64, &mut packed);
    encode_len_delimited(1, &packed, &mut output);
    encode_string(2, oauth_redirect_uri, &mut output);
    if force_reauth {
        encode_bool(5, true, &mut output);
    }
    encode_string(8, account_key, &mut output);
    output
}

fn encode_complete_mcp_oauth_request(state_id: &str, authorization_code: &str) -> Vec<u8> {
    let mut output = Vec::new();
    encode_string(1, state_id, &mut output);
    encode_string(2, authorization_code, &mut output);
    output
}

fn parse_mcp_server_id(server_id: &str) -> Result<i32, String> {
    let value = server_id
        .trim()
        .parse::<i32>()
        .map_err(|_| "MCP server id is outside signed int32 range".to_string())?;
    if value <= 0 {
        return Err("MCP server id must be positive".into());
    }
    Ok(value)
}

fn validate_mcp_account_key<'a>(raw: &'a str, label: &str) -> Result<&'a str, String> {
    let value = raw.trim();
    if value.is_empty() || value.len() > 320 || value.chars().any(char::is_control) {
        return Err(format!("MCP {label} is invalid"));
    }
    Ok(value)
}

fn encode_delete_mcp_oauth_token_request(server_url: &str, account_key: &str) -> Vec<u8> {
    let mut output = Vec::new();
    encode_string(1, server_url, &mut output);
    encode_string(3, "sand", &mut output);
    encode_string(4, account_key, &mut output);
    output
}

fn encode_rename_mcp_oauth_account_request(
    server_id: i32,
    account_key: &str,
    new_account_key: &str,
) -> Vec<u8> {
    let mut output = Vec::new();
    encode_key(1, 0, &mut output);
    encode_varint(server_id as u32 as u64, &mut output);
    encode_string(2, account_key, &mut output);
    encode_string(3, new_account_key, &mut output);
    output
}

fn encode_delete_mcp_oauth_account_request(server_id: i32, account_key: &str) -> Vec<u8> {
    let mut output = Vec::new();
    encode_key(1, 0, &mut output);
    encode_varint(server_id as u32 as u64, &mut output);
    encode_string(2, account_key, &mut output);
    output
}

fn encode_validate_mcp_oauth_tokens_request(
    server_url: &str,
    account_key: &str,
) -> Vec<u8> {
    let mut target = Vec::new();
    encode_string(1, server_url, &mut target);
    encode_string(2, account_key, &mut target);
    let mut output = Vec::new();
    encode_len_delimited(3, &target, &mut output);
    output
}

fn decode_check_http_mcp_status_response(input: &[u8]) -> Result<Vec<CheckStatusWire>, String> {
    let mut cursor = 0usize;
    let mut statuses = Vec::new();
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        if field == 1 && wire == 2 {
            let bytes = read_len_delimited(input, &mut cursor)?;
            statuses.push(decode_check_status(bytes)?);
        } else {
            skip_field(input, &mut cursor, wire)?;
        }
    }
    Ok(statuses)
}

fn decode_check_status(input: &[u8]) -> Result<CheckStatusWire, String> {
    let mut cursor = 0usize;
    let mut status = CheckStatusWire {
        id: 0,
        is_available: false,
        requires_auth: false,
        auth_url: None,
        error: None,
        has_valid_token: false,
    };
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        match (field, wire) {
            (1, 0) => status.id = read_varint(input, &mut cursor)? as i32,
            (2, 0) => status.is_available = read_varint(input, &mut cursor)? != 0,
            (3, 0) => status.requires_auth = read_varint(input, &mut cursor)? != 0,
            (4, 2) => status.auth_url = Some(read_string(input, &mut cursor)?),
            (5, 2) => status.error = Some(read_string(input, &mut cursor)?),
            (6, 0) => status.has_valid_token = read_varint(input, &mut cursor)? != 0,
            _ => skip_field(input, &mut cursor, wire)?,
        }
    }
    Ok(status)
}

fn decode_validate_mcp_oauth_tokens_response(
    input: &[u8],
) -> Result<Vec<ValidateResultWire>, String> {
    let mut cursor = 0usize;
    let mut results = Vec::new();
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        if field == 1 && wire == 2 {
            let bytes = read_len_delimited(input, &mut cursor)?;
            results.push(decode_validate_result(bytes)?);
        } else {
            skip_field(input, &mut cursor, wire)?;
        }
    }
    Ok(results)
}

fn decode_validate_result(input: &[u8]) -> Result<ValidateResultWire, String> {
    let mut cursor = 0usize;
    let mut server_url = String::new();
    let mut has_valid_token = false;
    let mut account_key = String::new();
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        match (field, wire) {
            (1, 2) => server_url = read_string(input, &mut cursor)?,
            (2, 0) => has_valid_token = read_varint(input, &mut cursor)? != 0,
            (3, 2) => account_key = read_string(input, &mut cursor)?,
            _ => skip_field(input, &mut cursor, wire)?,
        }
    }
    Ok(ValidateResultWire {
        server_url,
        has_valid_token,
        account_key,
    })
}

fn encode_available_models_request() -> Vec<u8> {
    let mut output = Vec::new();
    encode_bool(5, true, &mut output);
    encode_key(10, 0, &mut output);
    encode_varint(AVAILABLE_MODELS_SCOPE_USER_AVAILABLE, &mut output);
    output
}

fn optional_string_field(input: &[u8], wanted_field: u32) -> Result<Option<String>, String> {
    Ok(string_fields(input, wanted_field)?.into_iter().next())
}

fn decode_model_parameter_definition(input: &[u8]) -> Result<Value, String> {
    let id = optional_string_field(input, 1)?.unwrap_or_default();
    let name = optional_string_field(input, 2)?.unwrap_or_default();
    let markdown_tooltip = optional_string_field(input, 3)?;
    let parameter_type = first_message_field(input, 4)?;
    let parameter_kind = parameter_type
        .map(|value| -> Result<&'static str, String> {
            if first_message_field(value, 1)?.is_some() {
                Ok("boolean")
            } else if first_message_field(value, 2)?.is_some() {
                Ok("enum")
            } else {
                Ok("unknown")
            }
        })
        .transpose()?
        .unwrap_or("unknown");
    let is_cycleable_by_hotkey = decode_optional_varint_field(input, 5)?.map(|value| value != 0);
    Ok(json!({
        "id": id,
        "name": name,
        "markdownTooltip": markdown_tooltip,
        "kind": parameter_kind,
        "isCycleableByHotkey": is_cycleable_by_hotkey,
    }))
}

fn decode_available_model(input: &[u8]) -> Result<Value, String> {
    let name = optional_string_field(input, 1)?.unwrap_or_default();
    if name.trim().is_empty() {
        return Err("available model row is missing name".into());
    }
    let default_on = decode_optional_varint_field(input, 2)?.unwrap_or_default() != 0;
    let supports_agent = decode_optional_varint_field(input, 5)?.map(|value| value != 0);
    let degradation_status = decode_optional_varint_field(input, 6)?;
    let supports_thinking = decode_optional_varint_field(input, 9)?.map(|value| value != 0);
    let supports_images = decode_optional_varint_field(input, 10)?.map(|value| value != 0);
    let supports_auto_context = decode_optional_varint_field(input, 11)?.map(|value| value != 0);
    let supports_max_mode = decode_optional_varint_field(input, 14)?.map(|value| value != 0);
    let context_token_limit = decode_optional_varint_field(input, 15)?;
    let client_display_name = optional_string_field(input, 17)?;
    let server_model_name = optional_string_field(input, 18)?;
    let supports_non_max_mode = decode_optional_varint_field(input, 19)?.map(|value| value != 0);
    let supports_plan_mode = decode_optional_varint_field(input, 22)?.map(|value| value != 0);
    let inputbox_short_model_name = optional_string_field(input, 24)?;
    let is_hidden = decode_optional_varint_field(input, 35)?.map(|value| value != 0);
    let legacy_slugs = string_fields(input, 36)?;
    let id_aliases = string_fields(input, 37)?;
    let tagline = optional_string_field(input, 39)?;
    let visible_in_routed_model_view = decode_optional_varint_field(input, 40)?.map(|value| value != 0);
    let vendor_name = optional_string_field(input, 41)?;
    let requires_data_retention = decode_optional_varint_field(input, 46)?.map(|value| value != 0);
    let reason_for_zdr_consent_block = optional_string_field(input, 47)?;
    let parameter_definitions = repeated_message_fields(input, 29)?
        .into_iter()
        .map(decode_model_parameter_definition)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({
        "name": name,
        "displayName": client_display_name.clone().unwrap_or_else(|| name.clone()),
        "clientDisplayName": client_display_name,
        "serverModelName": server_model_name,
        "inputboxShortModelName": inputbox_short_model_name,
        "defaultOn": default_on,
        "supportsAgent": supports_agent,
        "supportsThinking": supports_thinking,
        "supportsImages": supports_images,
        "supportsAutoContext": supports_auto_context,
        "supportsMaxMode": supports_max_mode,
        "supportsNonMaxMode": supports_non_max_mode,
        "supportsPlanMode": supports_plan_mode,
        "contextTokenLimit": context_token_limit,
        "degradationStatus": degradation_status,
        "isHidden": is_hidden,
        "visibleInRoutedModelView": visible_in_routed_model_view,
        "legacySlugs": legacy_slugs,
        "idAliases": id_aliases,
        "tagline": tagline,
        "vendorName": vendor_name,
        "requiresDataRetention": requires_data_retention,
        "reasonForZdrConsentBlock": reason_for_zdr_consent_block,
        "parameterDefinitions": parameter_definitions,
    }))
}

fn decode_available_models_response(input: &[u8]) -> Result<Value, String> {
    let model_names = string_fields(input, 1)?;
    let models = repeated_message_fields(input, 2)?
        .into_iter()
        .map(decode_available_model)
        .collect::<Result<Vec<_>, _>>()?;
    let use_model_parameters = decode_optional_varint_field(input, 11)?.unwrap_or_default() != 0;
    Ok(json!({
        "scope": "user-available",
        "useModelParameters": use_model_parameters,
        "modelNames": model_names,
        "models": models,
    }))
}

fn encode_generate_image_request(
    description: &str,
    reference_images: &[(String, String)],
    model_id: &str,
) -> Vec<u8> {
    let mut output = Vec::new();
    encode_string(1, description, &mut output);
    for (data, mime_type) in reference_images {
        let mut reference = Vec::new();
        encode_string(1, data, &mut reference);
        encode_string(2, mime_type, &mut reference);
        encode_len_delimited(2, &reference, &mut output);
    }
    encode_string(3, model_id, &mut output);
    // Desktop avatar generation does not pass maxMode. In proto3 the false default is omitted
    // from the wire; emitting field 4=true changes backend model routing.
    output
}

fn decode_generate_image_response(
    input: &[u8],
) -> Result<GeneratedImageBackendResult, GenerateImageBackendError> {
    let mut cursor = 0usize;
    let mut success = None;
    let mut failure = None;
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor).map_err(GenerateImageBackendError::Generate)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        match (field, wire) {
            (1, 2) => {
                success = Some(decode_generate_image_success(
                    read_len_delimited(input, &mut cursor).map_err(GenerateImageBackendError::Generate)?,
                )?);
            }
            (2, 2) => {
                failure = Some(decode_generate_image_failure(
                    read_len_delimited(input, &mut cursor).map_err(GenerateImageBackendError::Generate)?,
                )?);
            }
            _ => skip_field(input, &mut cursor, wire).map_err(GenerateImageBackendError::Generate)?,
        }
    }
    if let Some(result) = success {
        if result.image_data.trim().is_empty() || result.mime_type.trim().is_empty() {
            return Err(GenerateImageBackendError::Generate(
                "Image generation returned an incomplete result.".into(),
            ));
        }
        return Ok(result);
    }
    if let Some((message, model_restricted)) = failure {
        let message = if message.trim().is_empty() {
            "Image generation failed.".to_string()
        } else {
            message
        };
        return if model_restricted {
            Err(GenerateImageBackendError::ModelRestricted(message))
        } else {
            Err(GenerateImageBackendError::Generate(message))
        };
    }
    Err(GenerateImageBackendError::Generate(
        "Image generation returned no result.".into(),
    ))
}

fn decode_generate_image_success(
    input: &[u8],
) -> Result<GeneratedImageBackendResult, GenerateImageBackendError> {
    let mut cursor = 0usize;
    let mut image_data = String::new();
    let mut mime_type = String::new();
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor).map_err(GenerateImageBackendError::Generate)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        match (field, wire) {
            (1, 2) => image_data = read_string(input, &mut cursor).map_err(GenerateImageBackendError::Generate)?,
            (2, 2) => mime_type = read_string(input, &mut cursor).map_err(GenerateImageBackendError::Generate)?,
            _ => skip_field(input, &mut cursor, wire).map_err(GenerateImageBackendError::Generate)?,
        }
    }
    Ok(GeneratedImageBackendResult { image_data, mime_type })
}

fn decode_generate_image_failure(
    input: &[u8],
) -> Result<(String, bool), GenerateImageBackendError> {
    let mut cursor = 0usize;
    let mut message = String::new();
    let mut model_restricted = false;
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor).map_err(GenerateImageBackendError::Generate)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        match (field, wire) {
            (1, 2) => message = read_string(input, &mut cursor).map_err(GenerateImageBackendError::Generate)?,
            (2, 0) => model_restricted = read_varint(input, &mut cursor).map_err(GenerateImageBackendError::Generate)? != 0,
            _ => skip_field(input, &mut cursor, wire).map_err(GenerateImageBackendError::Generate)?,
        }
    }
    Ok((message, model_restricted))
}

fn encode_bool(field: u32, value: bool, output: &mut Vec<u8>) {
    encode_key(field, 0, output);
    encode_varint(value as u64, output);
}

fn encode_string(field: u32, value: &str, output: &mut Vec<u8>) {
    if !value.is_empty() {
        encode_len_delimited(field, value.as_bytes(), output);
    }
}

fn encode_len_delimited(field: u32, value: &[u8], output: &mut Vec<u8>) {
    encode_key(field, 2, output);
    encode_varint(value.len() as u64, output);
    output.extend_from_slice(value);
}

fn encode_key(field: u32, wire: u8, output: &mut Vec<u8>) {
    encode_varint(((field as u64) << 3) | wire as u64, output);
}

fn encode_varint(mut value: u64, output: &mut Vec<u8>) {
    while value >= 0x80 {
        output.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    output.push(value as u8);
}

fn read_varint(input: &[u8], cursor: &mut usize) -> Result<u64, String> {
    let mut value = 0u64;
    for shift in (0..70).step_by(7) {
        let byte = *input
            .get(*cursor)
            .ok_or("truncated MCP backend protobuf varint")?;
        *cursor += 1;
        value |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err("malformed MCP backend protobuf varint".into())
}

fn read_len_delimited<'a>(input: &'a [u8], cursor: &mut usize) -> Result<&'a [u8], String> {
    let len = read_varint(input, cursor)? as usize;
    let end = cursor
        .checked_add(len)
        .filter(|end| *end <= input.len())
        .ok_or("truncated MCP backend protobuf field")?;
    let value = &input[*cursor..end];
    *cursor = end;
    Ok(value)
}

fn read_string(input: &[u8], cursor: &mut usize) -> Result<String, String> {
    String::from_utf8(read_len_delimited(input, cursor)?.to_vec())
        .map_err(|_| "MCP backend protobuf string is not UTF-8".into())
}

fn skip_field(input: &[u8], cursor: &mut usize, wire: u8) -> Result<(), String> {
    match wire {
        0 => {
            let _ = read_varint(input, cursor)?;
            Ok(())
        }
        1 => {
            *cursor = cursor
                .checked_add(8)
                .filter(|end| *end <= input.len())
                .ok_or("truncated MCP backend fixed64 field")?;
            Ok(())
        }
        2 => {
            let _ = read_len_delimited(input, cursor)?;
            Ok(())
        }
        5 => {
            *cursor = cursor
                .checked_add(4)
                .filter(|end| *end <= input.len())
                .ok_or("truncated MCP backend fixed32 field")?;
            Ok(())
        }
        _ => Err(format!("unsupported MCP backend protobuf wire type {wire}")),
    }
}

fn decode_optional_varint_field(
    input: &[u8],
    wanted_field: u32,
) -> Result<Option<u64>, String> {
    let mut cursor = 0usize;
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        if field == wanted_field && wire == 0 {
            return Ok(Some(read_varint(input, &mut cursor)?));
        }
        skip_field(input, &mut cursor, wire)?;
    }
    Ok(None)
}

fn repeated_message_fields<'a>(
    input: &'a [u8],
    wanted_field: u32,
) -> Result<Vec<&'a [u8]>, String> {
    let mut cursor = 0usize;
    let mut values = Vec::new();
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        if field == wanted_field && wire == 2 {
            values.push(read_len_delimited(input, &mut cursor)?);
        } else {
            skip_field(input, &mut cursor, wire)?;
        }
    }
    Ok(values)
}

fn string_fields(input: &[u8], wanted_field: u32) -> Result<Vec<String>, String> {
    let mut cursor = 0usize;
    let mut values = Vec::new();
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        if field == wanted_field && wire == 2 {
            let raw = read_len_delimited(input, &mut cursor)?;
            let value = std::str::from_utf8(raw)
                .map_err(|_| "MCP backend protobuf string is not UTF-8")?
                .trim();
            if !value.is_empty() {
                values.push(value.to_string());
            }
        } else {
            skip_field(input, &mut cursor, wire)?;
        }
    }
    Ok(values)
}

fn decode_optional_fixed64_field(input: &[u8], wanted_field: u32) -> Result<Option<u64>, String> {
    let mut cursor = 0usize;
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        if field == wanted_field && wire == 1 {
            let end = cursor.checked_add(8).filter(|end| *end <= input.len())
                .ok_or("truncated Dashboard fixed64 field")?;
            let bytes: [u8; 8] = input[cursor..end].try_into()
                .map_err(|_| "invalid Dashboard fixed64 field")?;
            return Ok(Some(u64::from_le_bytes(bytes)));
        }
        skip_field(input, &mut cursor, wire)?;
    }
    Ok(None)
}

fn first_message_field<'a>(input: &'a [u8], wanted_field: u32) -> Result<Option<&'a [u8]>, String> {
    Ok(repeated_message_fields(input, wanted_field)?.into_iter().next())
}

fn decode_timestamp_ms(input: &[u8]) -> Result<u64, String> {
    let seconds = decode_optional_varint_field(input, 1)?.unwrap_or_default();
    let nanos = decode_optional_varint_field(input, 2)?.unwrap_or_default();
    seconds.checked_mul(1_000)
        .and_then(|value| value.checked_add(nanos / 1_000_000))
        .ok_or_else(|| "Dashboard timestamp exceeds supported range".to_string())
}

fn decode_sand_access_status(bytes: &[u8]) -> Result<Value, String> {
    let state = decode_optional_varint_field(bytes, 1)?.unwrap_or(0);
    let purchase_channel = decode_optional_varint_field(bytes, 2)?.unwrap_or(0);
    let block_reason = decode_optional_varint_field(bytes, 3)?.unwrap_or(0);
    let state = match state {
        0 => "unknown",
        1 => "granted",
        2 => "unavailable",
        3 => "paymentRequired",
        other => return Err(format!("unsupported Sand access state {other}")),
    };
    let purchase_channel = match purchase_channel {
        0 => "unspecified",
        1 => "inApp",
        2 => "manageInCursor",
        3 => "manageOnWeb",
        other => return Err(format!("unsupported Sand purchase channel {other}")),
    };
    let reason = match block_reason {
        0 => "unspecified",
        1 => "none",
        2 => "teamPrivacyMode",
        3 => "teamSetupRequired",
        4 => "teamAccessRequired",
        5 => "notOffered",
        6 => "freeTrialAvailable",
        7 => "paywallIndividual",
        8 => "paywallTeamMember",
        9 => "paywallTeamAdmin",
        other => return Err(format!("unsupported Sand access block reason {other}")),
    };
    Ok(json!({
        "state": state,
        "purchaseChannel": purchase_channel,
        "reason": reason,
    }))
}

fn decode_sand_usage_summary(
    status: &[u8],
    current_period: &[u8],
    trial: &[u8],
    teams: &[u8],
    now_ms: u64,
) -> Result<Value, String> {
    let usage_percent = decode_optional_fixed64_field(status, 3)?
        .map(f64::from_bits)
        .filter(|value| value.is_finite() && *value >= 0.0);
    let uses_pooled_enterprise_allowance =
        decode_optional_varint_field(status, 6)?.unwrap_or_default() != 0;
    let has_available_usage = decode_optional_varint_field(status, 7)?.unwrap_or_default() != 0;
    let has_non_zero_included_limit =
        decode_optional_varint_field(status, 8)?.unwrap_or_default() != 0;
    let next_reset_ms = first_message_field(status, 2)?
        .map(decode_timestamp_ms)
        .transpose()?
        .filter(|value| *value > 0);
    let trial_expires_at_ms = first_message_field(status, 10)?
        .map(decode_timestamp_ms)
        .transpose()?
        .filter(|value| *value > 0);
    let trial_cancelable = decode_optional_varint_field(status, 11)?.unwrap_or_default() != 0;
    let claim_status = decode_optional_varint_field(trial, 1)?.unwrap_or_default();
    let is_sand_trial = trial_expires_at_ms.is_some_and(|expires| expires > now_ms);

    let is_enterprise = repeated_message_fields(teams, 1)?
        .into_iter()
        .any(|team| decode_optional_varint_field(team, 13).ok().flatten().unwrap_or_default() != 0);
    let upgrade_cta = decode_sand_upgrade_cta(status)?;

    let billing_cycle_end = decode_optional_varint_field(current_period, 2)?
        .filter(|value| *value > 0);
    let on_demand = first_message_field(current_period, 4)?
        .map(|spend| -> Result<Option<Value>, String> {
            let limit = decode_optional_varint_field(spend, 5)?;
            let used = decode_optional_varint_field(spend, 6)?.unwrap_or_default();
            let normalized_limit = limit.filter(|value| *value > 0 && *value < 2_147_483_647);
            Ok(normalized_limit.map(|limit| json!({
                "usedCents": used,
                "limitCents": limit,
                "resetTimestampMs": billing_cycle_end,
            })))
        })
        .transpose()?
        .flatten();

    Ok(json!({
        "sandUsagePercent": usage_percent,
        "sandUsageResetTimestampMs": next_reset_ms,
        "hasAvailableUsage": has_available_usage,
        "hasNonZeroIncludedLimit": has_non_zero_included_limit,
        "usesPooledEnterpriseAllowance": uses_pooled_enterprise_allowance,
        "isEnterprise": is_enterprise,
        "upgradeCta": upgrade_cta,
        "isSandTrial": is_sand_trial,
        "hasEndedSandTrial": claim_status == 3 && !is_sand_trial,
        "canCancelSandTrial": is_sand_trial && trial_cancelable,
        "onDemand": on_demand,
    }))
}

fn decode_sand_upgrade_cta(status: &[u8]) -> Result<Option<Value>, String> {
    let Some(recommendation) = first_message_field(status, 9)? else {
        return Ok(None);
    };
    let disabled = decode_optional_varint_field(recommendation, 2)?.unwrap_or_default() != 0;
    let Some(button) = first_message_field(recommendation, 1)? else {
        return Ok(None);
    };
    let Some(label) = string_fields(button, 1)?.into_iter().next() else {
        return Ok(None);
    };

    if let Some(url_action) = first_message_field(button, 5)? {
        let Some(url) = string_fields(url_action, 1)?.into_iter().next() else {
            return Ok(None);
        };
        let parsed = Url::parse(&url).map_err(|_| "Sand upgrade CTA URL is invalid")?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Ok(None);
        }
        return Ok(Some(json!({
            "label": label,
            "disabled": disabled,
            "action": {"kind":"open-url","url":url},
        })));
    }

    if let Some(dashboard_action) = first_message_field(button, 7)? {
        let Some(action) = string_fields(dashboard_action, 1)?.into_iter().next() else {
            return Ok(None);
        };
        if action != "requestLimitIncrease" {
            return Ok(None);
        }
        let mut args = std::collections::BTreeMap::<String, String>::new();
        for raw in repeated_message_fields(dashboard_action, 2)? {
            let Some(key) = string_fields(raw, 1)?.into_iter().next() else {
                continue;
            };
            let Some(value) = string_fields(raw, 2)?.into_iter().next() else {
                continue;
            };
            args.insert(key, value);
        }
        let success_message = string_fields(dashboard_action, 3)?
            .into_iter()
            .next();
        return Ok(Some(json!({
            "label": label,
            "disabled": disabled,
            "action": {
                "kind":"dashboard-action",
                "action":action,
                "args":args,
                "successMessage":success_message,
            },
        })));
    }

    Ok(None)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SubmitLogsReceipt {
    success: bool,
    error_message: Option<String>,
    logs_processed: u64,
    logs_dropped: u64,
}

fn encode_account_telemetry_submit_logs_request(
    entries: &[AccountTelemetryProjection],
    machine_id: &str,
    client_version: &str,
    timestamp_ms: u64,
) -> Result<Vec<u8>, String> {
    let machine_id = machine_id.trim();
    if machine_id.is_empty() {
        return Err("account telemetry machine id is required".into());
    }
    let client_version = client_version.trim();
    if client_version.is_empty() {
        return Err("account telemetry client version is required".into());
    }
    let app_version = env::var("SAND_CLIENT_APP_VERSION")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "0.18.0".into());
    let mut request = Vec::new();
    for projection in entries {
        let (level, message) = match (projection.level.as_str(), projection.stream.as_str()) {
            ("info", "session") => (1_u64, DESKTOP_SESSION_EVENT),
            ("warn", "session") => (3_u64, DESKTOP_SESSION_EVENT),
            ("info", "signin") => (1_u64, DESKTOP_SIGNIN_EVENT),
            ("warn", "signin") => (3_u64, DESKTOP_SIGNIN_EVENT),
            ("info", "access-blocked") => (1_u64, ACCESS_BLOCKED_EVENT),
            ("warn", "access-blocked") => (3_u64, ACCESS_BLOCKED_EVENT),
            (level, stream) => {
                return Err(format!(
                    "unsupported account telemetry projection level={level} stream={stream}"
                ))
            }
        };
        let mut metadata = std::collections::BTreeMap::<String, String>::from([
            ("client".into(), "sand".into()),
            ("client.type".into(), "sand".into()),
            ("client.machine_id".into(), machine_id.to_string()),
            ("client_version".into(), client_version.to_string()),
            ("app_version".into(), app_version.clone()),
            ("arch".into(), std::env::consts::ARCH.to_string()),
            ("platform".into(), "android".into()),
        ]);
        if let Some(values) = projection.metadata.as_object() {
            for (key, value) in values {
                if key.len() > 96 {
                    continue;
                }
                let value = match value {
                    Value::String(value) => value.clone(),
                    Value::Bool(value) => value.to_string(),
                    Value::Number(value) => value.to_string(),
                    Value::Null => continue,
                    _ => continue,
                };
                metadata.insert(key.clone(), value.chars().take(512).collect());
            }
        }

        let mut log = Vec::new();
        encode_key(1, 0, &mut log);
        encode_varint(level, &mut log);
        encode_string(2, message, &mut log);
        for (key, value) in metadata {
            let mut pair = Vec::new();
            encode_string(1, &key, &mut pair);
            encode_string(2, &value, &mut pair);
            encode_len_delimited(3, &pair, &mut log);
        }
        encode_key(4, 0, &mut log);
        encode_varint(timestamp_ms, &mut log);
        encode_string(7, SAND_LOG_KEY, &mut log);
        encode_len_delimited(1, &log, &mut request);
    }
    Ok(request)
}

fn decode_submit_logs_response(input: &[u8]) -> Result<SubmitLogsReceipt, String> {
    let mut cursor = 0usize;
    let mut receipt = SubmitLogsReceipt {
        success: false,
        error_message: None,
        logs_processed: 0,
        logs_dropped: 0,
    };
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        match (field, wire) {
            (1, 0) => receipt.success = read_varint(input, &mut cursor)? != 0,
            (2, 2) => receipt.error_message = Some(read_string(input, &mut cursor)?),
            (3, 0) => receipt.logs_processed = read_varint(input, &mut cursor)?,
            (4, 0) => receipt.logs_dropped = read_varint(input, &mut cursor)?,
            _ => skip_field(input, &mut cursor, wire)?,
        }
    }
    Ok(receipt)
}

fn encode_client_action_request(
    action: &str,
    args: &std::collections::BTreeMap<String, String>,
) -> Vec<u8> {
    let mut body = Vec::new();
    encode_string(1, action, &mut body);
    for (key, value) in args {
        let mut entry = Vec::new();
        encode_string(1, key, &mut entry);
        encode_string(2, value, &mut entry);
        encode_len_delimited(2, &entry, &mut body);
    }
    body
}

fn decode_client_action_response(input: &[u8]) -> Result<Value, String> {
    let mut cursor = 0usize;
    let mut success = false;
    let mut error_message: Option<String> = None;
    let mut info_message: Option<String> = None;
    while cursor < input.len() {
        let key = read_varint(input, &mut cursor)?;
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        match (field, wire) {
            (1, 0) => success = read_varint(input, &mut cursor)? != 0,
            (2, 2) => error_message = Some(read_string(input, &mut cursor)?),
            (3, 2) => info_message = Some(read_string(input, &mut cursor)?),
            _ => skip_field(input, &mut cursor, wire)?,
        }
    }
    let message = if success { info_message } else { error_message }
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    Ok(json!({"ok":success,"message":message}))
}

fn encode_get_team_plugin_popularity_request(team_id: i32) -> Vec<u8> {
    let mut body = Vec::new();
    if team_id != 0 {
        encode_key(1, 0, &mut body);
        encode_varint(team_id as u64, &mut body);
    }
    body
}

fn decode_team_plugin_popularity_response(
    response: &[u8],
) -> Result<std::collections::BTreeMap<String, u64>, String> {
    let mut counts = std::collections::BTreeMap::new();
    for entry in repeated_message_fields(response, 1)? {
        let plugin_id = decode_optional_varint_field(entry, 1)?
            .ok_or("team plugin popularity row is missing plugin id")?;
        let member_install_count = decode_optional_varint_field(entry, 2)?.unwrap_or_default();
        if member_install_count > i32::MAX as u64 {
            return Err("team plugin popularity count is outside signed int32 range".into());
        }
        counts.insert(plugin_id.to_string(), member_install_count);
    }
    Ok(counts)
}

fn encode_get_team_rules_request(team_id: u64) -> Vec<u8> {
    let mut body = vec![0x08, 0x01];
    encode_key(2, 0, &mut body);
    encode_varint(team_id, &mut body);
    body
}

fn decode_direct_team_ids(response: &[u8]) -> Result<Vec<u64>, String> {
    let mut ids = Vec::new();
    for team in repeated_message_fields(response, 1)? {
        let id = decode_optional_varint_field(team, 2)?.unwrap_or_default();
        let direct = decode_optional_varint_field(team, 36)?.unwrap_or_default() != 0;
        if id > 0 && direct {
            ids.push(id);
        }
    }
    Ok(ids)
}

fn decode_sand_team_rules(response: &[u8]) -> Result<Vec<Value>, String> {
    let mut rules = Vec::new();
    for raw in repeated_message_fields(response, 1)? {
        let agent_type = decode_optional_varint_field(raw, 7)?.unwrap_or_default();
        if agent_type != TEAM_RULE_AGENT_TYPE_ALL && agent_type != TEAM_RULE_AGENT_TYPE_SAND {
            continue;
        }
        let Some(full_path) = string_fields(raw, 2)?.into_iter().next() else {
            continue;
        };
        let Some(content) = string_fields(raw, 3)?.into_iter().next() else {
            continue;
        };
        let globs = string_fields(raw, 6)?;
        let required = decode_optional_varint_field(raw, 5)?.unwrap_or_default() != 0;
        rules.push(json!({
            "fullPath": full_path,
            "content": content,
            "type": if globs.is_empty() {
                json!({"kind":"global"})
            } else {
                json!({"kind":"fileGlobbed","globs":globs})
            },
            "source":"team",
            "isRequired":required,
        }));
    }
    Ok(rules)
}

fn pr_review_destination_from_proto(value: u64) -> Option<&'static str> {
    match value {
        1 => Some("github"),
        2 => Some("graphite"),
        3 => Some("reviewCursor"),
        _ => None,
    }
}

fn decode_user_pr_review_destination(input: &[u8]) -> Result<Option<&'static str>, String> {
    decode_optional_varint_field(input, 6).map(|value| value.and_then(pr_review_destination_from_proto))
}

fn decode_nested_pr_review_destination(
    input: &[u8],
    outer_field: u32,
    inner_field: u32,
) -> Result<Option<&'static str>, String> {
    for nested in repeated_message_fields(input, outer_field)? {
        if let Some(value) = decode_optional_varint_field(nested, inner_field)? {
            if let Some(destination) = pr_review_destination_from_proto(value) {
                return Ok(Some(destination));
            }
        }
    }
    Ok(None)
}

fn decode_local_tool_permission_ceiling(
    input: &[u8],
) -> Result<Option<&'static str>, String> {
    // Desktop generated schema at canonical main:
    // GetTeamAdminSettingsResponse.local_tool_controls = field 60,
    // LocalToolControls.permission_ceiling = field 1,
    // enum 0=unspecified, 1=never, 2=ask, 3=always.
    for controls in repeated_message_fields(input, 60)? {
        if let Some(value) = decode_optional_varint_field(controls, 1)? {
            return Ok(match value {
                1 => Some("never"),
                2 => Some("ask"),
                3 => Some("always"),
                _ => None,
            });
        }
    }
    Ok(None)
}

fn decode_team_pr_review_destination(input: &[u8]) -> Result<Option<&'static str>, String> {
    // Current Desktop generated schema prefers pull_request_preferences (field 37, inner field 1)
    // and falls back to background_agent_settings (field 7, inner field 5).
    Ok(decode_nested_pr_review_destination(input, 37, 1)?
        .or(decode_nested_pr_review_destination(input, 7, 5)?))
}

pub fn create_cursor_checksum(machine_id: &str, now_ms: u64) -> String {
    let kilo_seconds = now_ms / 1_000_000;
    let mut bytes = [
        ((kilo_seconds >> 40) & 0xff) as u8,
        ((kilo_seconds >> 32) & 0xff) as u8,
        ((kilo_seconds >> 24) & 0xff) as u8,
        ((kilo_seconds >> 16) & 0xff) as u8,
        ((kilo_seconds >> 8) & 0xff) as u8,
        (kilo_seconds & 0xff) as u8,
    ];
    let mut last = 165_u8;
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = ((*byte ^ last).wrapping_add((index % 256) as u8)) & 0xff;
        last = *byte;
    }
    format!("{}{}", URL_SAFE_NO_PAD.encode(bytes), machine_id)
}

fn jwt_expiry_ms(token: &str) -> Option<u64> {
    let payload = token.split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD.decode(payload.as_bytes()).ok()?;
    let parsed: Value = serde_json::from_slice(&decoded).ok()?;
    parsed
        .get("exp")
        .and_then(Value::as_u64)
        .and_then(|seconds| seconds.checked_mul(1_000))
}

fn system_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn next_request_id() -> String {
    let sequence = REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let material = format!("{}:{sequence}", system_now_ms());
    let digest = Sha256::digest(material.as_bytes());
    format!(
        "android-mcp-{}",
        digest.iter().take(12).map(|byte| format!("{byte:02x}")).collect::<String>()
    )
}

fn sand_box_namespace() -> String {
    match env::var("SAND_BOX_OWNER_NAMESPACE").ok().as_deref() {
        Some("dev") => "dev".into(),
        Some("lab") => "lab".into(),
        _ if env::var("SAND_PACKAGED").ok().as_deref() != Some("1") => "dev".into(),
        _ if env::var("SAND_LAB").ok().as_deref() == Some("1") => "lab".into(),
        _ => "prod".into(),
    }
}

fn sand_client_version() -> String {
    let stamped = env::var("SAND_CLIENT_APP_VERSION").unwrap_or_default();
    let base = stamped.split('-').next().unwrap_or_default();
    let valid = {
        let parts = base.split('.').collect::<Vec<_>>();
        parts.len() == 3
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    };
    let base = if valid {
        base.to_string()
    } else {
        DEFAULT_CLIENT_VERSION.into()
    };
    match sand_box_namespace().as_str() {
        "dev" => format!("{base}-dev"),
        "lab" => format!("{base}-lab"),
        _ => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn len_field(field: u32, payload: &[u8]) -> Vec<u8> {
        let mut value = Vec::new();
        encode_len_delimited(field, payload, &mut value);
        value
    }

    #[test]
    fn available_models_wire_matches_desktop_generated_schema() {
        assert_eq!(AI_AVAILABLE_MODELS_PATH, "/aiserver.v1.AiService/AvailableModels");
        assert_eq!(AVAILABLE_MODELS_SCOPE_USER_AVAILABLE, 1);
        assert_eq!(AVAILABLE_MODELS_REQUEST_TIMEOUT_MS, 15_000);
        assert_eq!(encode_available_models_request(), vec![0x28, 0x01, 0x50, 0x01]);

        let mut parameter = Vec::new();
        encode_string(1, "thinking", &mut parameter);
        encode_string(2, "Thinking", &mut parameter);
        let mut parameter_type = Vec::new();
        encode_len_delimited(1, &[], &mut parameter_type);
        encode_len_delimited(4, &parameter_type, &mut parameter);
        encode_bool(5, true, &mut parameter);
        let mut model = Vec::new();
        encode_string(1, "grok-test", &mut model);
        encode_bool(2, true, &mut model);
        encode_bool(5, true, &mut model);
        encode_bool(9, true, &mut model);
        encode_string(17, "Grok Test", &mut model);
        encode_key(15, 0, &mut model);
        encode_varint(131_072, &mut model);
        encode_len_delimited(29, &parameter, &mut model);
        let mut response = Vec::new();
        encode_string(1, "grok-test", &mut response);
        encode_len_delimited(2, &model, &mut response);
        encode_bool(11, true, &mut response);
        let decoded = decode_available_models_response(&response).unwrap();
        assert_eq!(decoded["scope"], "user-available");
        assert_eq!(decoded["useModelParameters"], true);
        assert_eq!(decoded["modelNames"][0], "grok-test");
        assert_eq!(decoded["models"][0]["name"], "grok-test");
        assert_eq!(decoded["models"][0]["displayName"], "Grok Test");
        assert_eq!(decoded["models"][0]["supportsAgent"], true);
        assert_eq!(decoded["models"][0]["supportsThinking"], true);
        assert_eq!(decoded["models"][0]["contextTokenLimit"], 131_072);
        assert_eq!(decoded["models"][0]["parameterDefinitions"][0]["kind"], "boolean");
    }

    #[test]
    fn generate_image_wire_matches_desktop_contract() {
        assert_eq!(RUN_GENERATE_IMAGE_PATH, "/aiserver.v1.AiService/RunGenerateImage");
        assert_eq!(SAND_DEFAULT_GENERATE_IMAGE_MODEL_ID, "grok-4.5");
        let request = encode_generate_image_request(
            "a lotus",
            &[("cmVmZXJlbmNl".into(), "image/png".into())],
            "grok-image-test",
        );
        assert_eq!(string_fields(&request, 1).unwrap(), vec!["a lotus".to_string()]);
        let references = repeated_message_fields(&request, 2).unwrap();
        assert_eq!(references.len(), 1);
        assert_eq!(string_fields(references[0], 1).unwrap(), vec!["cmVmZXJlbmNl".to_string()]);
        assert_eq!(string_fields(references[0], 2).unwrap(), vec!["image/png".to_string()]);
        assert_eq!(string_fields(&request, 3).unwrap(), vec!["grok-image-test".to_string()]);
        assert_eq!(decode_optional_varint_field(&request, 4).unwrap(), None);

        let mut success = Vec::new();
        encode_string(1, "aW1hZ2U=", &mut success);
        encode_string(2, "image/webp", &mut success);
        let mut response = Vec::new();
        encode_len_delimited(1, &success, &mut response);
        assert_eq!(
            decode_generate_image_response(&response).unwrap(),
            GeneratedImageBackendResult {
                image_data: "aW1hZ2U=".into(),
                mime_type: "image/webp".into(),
            }
        );

        let mut failure = Vec::new();
        encode_string(1, "model unavailable", &mut failure);
        encode_bool(2, true, &mut failure);
        let mut response = Vec::new();
        encode_len_delimited(2, &failure, &mut response);
        assert_eq!(
            decode_generate_image_response(&response).unwrap_err(),
            GenerateImageBackendError::ModelRestricted("model unavailable".into())
        );
    }

    #[test]
    fn sand_access_path_and_decoder_match_desktop_generated_contract() {
        assert_eq!(
            DASHBOARD_GET_SAND_ACCESS_STATUS_PATH,
            "/aiserver.v1.DashboardService/GetSandAccessStatus"
        );
        assert_eq!(SAND_ACCESS_REQUEST_TIMEOUT_MS, 10_000);

        let mut response = Vec::new();
        encode_key(1, 0, &mut response);
        encode_varint(3, &mut response);
        encode_key(2, 0, &mut response);
        encode_varint(1, &mut response);
        encode_key(3, 0, &mut response);
        encode_varint(7, &mut response);
        assert_eq!(
            decode_sand_access_status(&response).unwrap(),
            json!({
                "state":"paymentRequired",
                "purchaseChannel":"inApp",
                "reason":"paywallIndividual"
            })
        );
        assert_eq!(
            decode_sand_access_status(&[]).unwrap(),
            json!({
                "state":"unknown",
                "purchaseChannel":"unspecified",
                "reason":"unspecified"
            })
        );
        let mut invalid = Vec::new();
        encode_key(3, 0, &mut invalid);
        encode_varint(10, &mut invalid);
        assert!(decode_sand_access_status(&invalid).unwrap_err().contains("block reason"));
    }

    #[test]
    fn usage_paths_and_decoder_match_desktop_dashboard_contract() {
        assert_eq!(DASHBOARD_GET_CURRENT_PERIOD_USAGE_PATH, "/aiserver.v1.DashboardService/GetCurrentPeriodUsage");
        assert_eq!(DASHBOARD_GET_SAND_USAGE_STATUS_PATH, "/aiserver.v1.DashboardService/GetSandUsageStatus");
        assert_eq!(DASHBOARD_GET_SAND_TRIAL_CLAIM_STATUS_PATH, "/aiserver.v1.DashboardService/GetSandTrialClaimStatus");
        assert_eq!(DASHBOARD_CANCEL_SAND_TRIAL_PATH, "/aiserver.v1.DashboardService/CancelSandTrial");
        assert_eq!(USAGE_REQUEST_TIMEOUT_MS, 15_000);

        let mut reset = Vec::new();
        encode_key(1, 0, &mut reset);
        encode_varint(2_000_000_000, &mut reset);
        let mut trial_expiry = Vec::new();
        encode_key(1, 0, &mut trial_expiry);
        encode_varint(2_000_000_100, &mut trial_expiry);
        let mut status = Vec::new();
        encode_len_delimited(2, &reset, &mut status);
        encode_key(3, 1, &mut status);
        status.extend_from_slice(&42.5_f64.to_bits().to_le_bytes());
        encode_bool(7, true, &mut status);
        encode_bool(8, true, &mut status);
        encode_len_delimited(10, &trial_expiry, &mut status);
        encode_bool(11, true, &mut status);

        let mut spend = Vec::new();
        encode_key(5, 0, &mut spend);
        encode_varint(5_000, &mut spend);
        encode_key(6, 0, &mut spend);
        encode_varint(1_250, &mut spend);
        let mut period = Vec::new();
        encode_key(2, 0, &mut period);
        encode_varint(2_000_000_200_000, &mut period);
        encode_len_delimited(4, &spend, &mut period);
        let trial = vec![0x08, 0x03];

        assert_eq!(
            decode_sand_usage_summary(&status, &period, &trial, &[], 2_000_000_000_000).unwrap(),
            json!({
                "sandUsagePercent":42.5,
                "sandUsageResetTimestampMs":2_000_000_000_000_u64,
                "hasAvailableUsage":true,
                "hasNonZeroIncludedLimit":true,
                "usesPooledEnterpriseAllowance":false,
                "isEnterprise":false,
                "upgradeCta":null,
                "isSandTrial":true,
                "hasEndedSandTrial":false,
                "canCancelSandTrial":true,
                "onDemand":{"usedCents":1_250_u64,"limitCents":5_000_u64,"resetTimestampMs":2_000_000_200_000_u64},
            })
        );
    }


    #[test]
    fn usage_decoder_preserves_enterprise_and_safe_upgrade_cta_contract() {
        let mut url = Vec::new();
        encode_string(1, "https://cursor.com/settings", &mut url);
        let mut button = Vec::new();
        encode_string(1, "Upgrade", &mut button);
        encode_len_delimited(5, &url, &mut button);
        let mut recommendation = Vec::new();
        encode_len_delimited(1, &button, &mut recommendation);
        encode_bool(2, true, &mut recommendation);
        let mut status = Vec::new();
        encode_len_delimited(9, &recommendation, &mut status);

        let mut enterprise_team = Vec::new();
        encode_bool(13, true, &mut enterprise_team);
        let mut teams = Vec::new();
        encode_len_delimited(1, &enterprise_team, &mut teams);

        let decoded = decode_sand_usage_summary(&status, &[], &[], &teams, 0).unwrap();
        assert_eq!(decoded["isEnterprise"], true);
        assert_eq!(
            decoded["upgradeCta"],
            json!({
                "label":"Upgrade",
                "disabled":true,
                "action":{"kind":"open-url","url":"https://cursor.com/settings"}
            })
        );

        let mut args_entry = Vec::new();
        encode_string(1, "plan", &mut args_entry);
        encode_string(2, "pro", &mut args_entry);
        let mut dashboard = Vec::new();
        encode_string(1, "requestLimitIncrease", &mut dashboard);
        encode_len_delimited(2, &args_entry, &mut dashboard);
        encode_string(3, "Request sent", &mut dashboard);
        let mut button = Vec::new();
        encode_string(1, "Request more", &mut button);
        encode_len_delimited(7, &dashboard, &mut button);
        let mut recommendation = Vec::new();
        encode_len_delimited(1, &button, &mut recommendation);
        let mut status = Vec::new();
        encode_len_delimited(9, &recommendation, &mut status);
        let decoded = decode_sand_usage_summary(&status, &[], &[], &[], 0).unwrap();
        assert_eq!(
            decoded["upgradeCta"],
            json!({
                "label":"Request more",
                "disabled":false,
                "action":{
                    "kind":"dashboard-action",
                    "action":"requestLimitIncrease",
                    "args":{"plan":"pro"},
                    "successMessage":"Request sent"
                }
            })
        );
    }

    #[test]
    fn usage_decoder_rejects_unsafe_upgrade_cta_variants() {
        let mut javascript = Vec::new();
        encode_string(1, "javascript:alert(1)", &mut javascript);
        let mut button = Vec::new();
        encode_string(1, "Bad", &mut button);
        encode_len_delimited(5, &javascript, &mut button);
        let mut recommendation = Vec::new();
        encode_len_delimited(1, &button, &mut recommendation);
        let mut status = Vec::new();
        encode_len_delimited(9, &recommendation, &mut status);
        assert_eq!(
            decode_sand_usage_summary(&status, &[], &[], &[], 0).unwrap()["upgradeCta"],
            Value::Null
        );

        let mut dashboard = Vec::new();
        encode_string(1, "deleteEverything", &mut dashboard);
        let mut button = Vec::new();
        encode_string(1, "Bad", &mut button);
        encode_len_delimited(7, &dashboard, &mut button);
        let mut recommendation = Vec::new();
        encode_len_delimited(1, &button, &mut recommendation);
        let mut status = Vec::new();
        encode_len_delimited(9, &recommendation, &mut status);
        assert_eq!(
            decode_sand_usage_summary(&status, &[], &[], &[], 0).unwrap()["upgradeCta"],
            Value::Null
        );
    }

    #[test]
    fn account_telemetry_submit_logs_wire_matches_desktop_contract() {
        assert_eq!(
            ANALYTICS_SUBMIT_LOGS_PATH,
            "/aiserver.v1.AnalyticsService/SubmitLogs"
        );
        assert_eq!(ACCOUNT_TELEMETRY_SUBMIT_TIMEOUT_MS, 15_000);
        let projection = AccountTelemetryProjection {
            stream: "session".into(),
            level: "warn".into(),
            metadata: json!({
                "phase":"signed_out",
                "cause":"session_revoked",
                "durable":"true"
            }),
        };
        let request = encode_account_telemetry_submit_logs_request(
            &[projection],
            "machine-1",
            "0.18.0",
            1_728_000_000_000,
        )
        .unwrap();
        let logs = repeated_message_fields(&request, 1).unwrap();
        assert_eq!(logs.len(), 1);
        let log = logs[0];
        assert_eq!(decode_optional_varint_field(log, 1).unwrap(), Some(3));
        assert_eq!(
            string_fields(log, 2).unwrap(),
            vec![DESKTOP_SESSION_EVENT.to_string()]
        );
        assert_eq!(decode_optional_varint_field(log, 4).unwrap(), Some(1_728_000_000_000));
        assert_eq!(string_fields(log, 7).unwrap(), vec![SAND_LOG_KEY.to_string()]);
        let metadata = repeated_message_fields(log, 3).unwrap();
        let metadata = metadata
            .into_iter()
            .map(|entry| {
                let key = string_fields(entry, 1).unwrap().into_iter().next().unwrap();
                let value = string_fields(entry, 2).unwrap().into_iter().next().unwrap();
                (key, value)
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(metadata.get("client.machine_id").map(String::as_str), Some("machine-1"));
        assert_eq!(metadata.get("platform").map(String::as_str), Some("android"));
        assert_eq!(metadata.get("phase").map(String::as_str), Some("signed_out"));
        assert_eq!(metadata.get("cause").map(String::as_str), Some("session_revoked"));
    }

    #[test]
    fn submit_logs_receipt_requires_backend_success_shape() {
        let mut response = Vec::new();
        encode_bool(1, true, &mut response);
        encode_key(3, 0, &mut response);
        encode_varint(1, &mut response);
        encode_key(4, 0, &mut response);
        encode_varint(0, &mut response);
        assert_eq!(
            decode_submit_logs_response(&response).unwrap(),
            SubmitLogsReceipt {
                success: true,
                error_message: None,
                logs_processed: 1,
                logs_dropped: 0,
            }
        );
    }

    #[test]
    fn client_action_wire_matches_desktop_dashboard_contract() {
        assert_eq!(
            DASHBOARD_CLIENT_ACTION_PATH,
            "/aiserver.v1.DashboardService/ClientAction"
        );
        let args = std::collections::BTreeMap::from([
            ("plan".to_string(), "pro".to_string()),
            ("source".to_string(), "tray".to_string()),
        ]);
        let request = encode_client_action_request("upgrade", &args);
        assert_eq!(request.first().copied(), Some(0x0a));
        assert!(request.windows("upgrade".len()).any(|w| w == b"upgrade"));
        assert!(request.windows("plan".len()).any(|w| w == b"plan"));
        assert!(request.windows("pro".len()).any(|w| w == b"pro"));

        let mut ok = Vec::new();
        encode_bool(1, true, &mut ok);
        encode_string(3, "Opened billing", &mut ok);
        assert_eq!(
            decode_client_action_response(&ok).unwrap(),
            json!({"ok":true,"message":"Opened billing"})
        );

        let mut failed = Vec::new();
        encode_bool(1, false, &mut failed);
        encode_string(2, "Not allowed", &mut failed);
        assert_eq!(
            decode_client_action_response(&failed).unwrap(),
            json!({"ok":false,"message":"Not allowed"})
        );
    }

    #[test]
    fn team_plugin_popularity_wire_matches_desktop_generated_schema() {
        assert_eq!(
            DASHBOARD_GET_ME_PATH,
            "/aiserver.v1.DashboardService/GetMe"
        );
        assert_eq!(
            DASHBOARD_GET_TEAM_PLUGIN_POPULARITY_PATH,
            "/aiserver.v1.DashboardService/GetTeamPluginPopularity"
        );
        assert_eq!(TEAM_PLUGIN_POPULARITY_REQUEST_TIMEOUT_MS, 12_000);

        let mut me = Vec::new();
        encode_key(7, 0, &mut me);
        encode_varint(42, &mut me);
        assert_eq!(decode_optional_varint_field(&me, 7).unwrap(), Some(42));
        assert_eq!(encode_get_team_plugin_popularity_request(42), vec![0x08, 0x2a]);
        assert!(encode_get_team_plugin_popularity_request(0).is_empty());

        let mut first = Vec::new();
        encode_key(1, 0, &mut first);
        encode_varint(123, &mut first);
        encode_key(2, 0, &mut first);
        encode_varint(7, &mut first);
        let mut second = Vec::new();
        encode_key(1, 0, &mut second);
        encode_varint(456, &mut second);
        encode_key(2, 0, &mut second);
        encode_varint(11, &mut second);
        let mut response = Vec::new();
        encode_len_delimited(1, &first, &mut response);
        encode_len_delimited(1, &second, &mut response);
        assert_eq!(
            decode_team_plugin_popularity_response(&response).unwrap(),
            std::collections::BTreeMap::from([
                ("123".to_string(), 7),
                ("456".to_string(), 11),
            ])
        );
    }

    #[test]
    fn managed_team_rules_wire_matches_desktop_contract() {
        assert_eq!(DASHBOARD_GET_TEAMS_PATH, "/aiserver.v1.DashboardService/GetTeams");
        assert_eq!(
            DASHBOARD_GET_TEAM_RULES_PATH,
            "/aiserver.v1.DashboardService/GetTeamRules"
        );
        assert_eq!(TEAM_RULES_REQUEST_TIMEOUT_MS, 10_000);
        assert_eq!(encode_get_team_rules_request(7), vec![0x08, 0x01, 0x10, 0x07]);

        let mut direct_team = Vec::new();
        encode_key(2, 0, &mut direct_team);
        encode_varint(7, &mut direct_team);
        encode_key(36, 0, &mut direct_team);
        encode_varint(1, &mut direct_team);
        let mut indirect_team = Vec::new();
        encode_key(2, 0, &mut indirect_team);
        encode_varint(9, &mut indirect_team);
        encode_key(36, 0, &mut indirect_team);
        encode_varint(0, &mut indirect_team);
        let mut teams = Vec::new();
        encode_len_delimited(1, &direct_team, &mut teams);
        encode_len_delimited(1, &indirect_team, &mut teams);
        assert_eq!(decode_direct_team_ids(&teams).unwrap(), vec![7]);

        let mut rule = Vec::new();
        encode_string(2, "security", &mut rule);
        encode_string(3, "Never publish credentials.", &mut rule);
        encode_key(5, 0, &mut rule);
        encode_varint(1, &mut rule);
        encode_string(6, "**/*.env", &mut rule);
        encode_key(7, 0, &mut rule);
        encode_varint(TEAM_RULE_AGENT_TYPE_SAND, &mut rule);
        let mut ignored = Vec::new();
        encode_string(2, "cursor-only", &mut ignored);
        encode_string(3, "Ignore in Sand.", &mut ignored);
        encode_key(7, 0, &mut ignored);
        encode_varint(3, &mut ignored);
        let mut rules = Vec::new();
        encode_len_delimited(1, &rule, &mut rules);
        encode_len_delimited(1, &ignored, &mut rules);
        assert_eq!(
            decode_sand_team_rules(&rules).unwrap(),
            vec![json!({
                "fullPath":"security",
                "content":"Never publish credentials.",
                "type":{"kind":"fileGlobbed","globs":["**/*.env"]},
                "source":"team",
                "isRequired":true,
            })]
        );
    }

    #[test]
    fn local_tool_permission_ceiling_wire_matches_desktop_generated_schema() {
        assert_eq!(
            DASHBOARD_GET_TEAM_ADMIN_SETTINGS_PATH,
            "/aiserver.v1.DashboardService/GetTeamAdminSettingsOrEmptyIfNotInTeam"
        );
        assert_eq!(PR_REVIEW_REQUEST_TIMEOUT_MS, 10_000);

        for (raw, expected) in [(1, "never"), (2, "ask"), (3, "always")] {
            let mut controls = Vec::new();
            encode_key(1, 0, &mut controls);
            encode_varint(raw, &mut controls);
            let mut response = Vec::new();
            encode_len_delimited(60, &controls, &mut response);
            assert_eq!(
                decode_local_tool_permission_ceiling(&response).unwrap(),
                Some(expected)
            );
        }

        let mut unknown_controls = Vec::new();
        encode_key(1, 0, &mut unknown_controls);
        encode_varint(99, &mut unknown_controls);
        let mut unknown_response = Vec::new();
        encode_len_delimited(60, &unknown_controls, &mut unknown_response);
        assert_eq!(decode_local_tool_permission_ceiling(&unknown_response).unwrap(), None);
        assert_eq!(decode_local_tool_permission_ceiling(&[]).unwrap(), None);
    }

    #[test]
    fn pr_review_preference_wire_matches_desktop_generated_schema() {
        assert_eq!(
            BACKGROUND_COMPOSER_GET_USER_SETTINGS_PATH,
            "/aiserver.v1.BackgroundComposerService/GetBackgroundComposerUserSettings"
        );
        assert_eq!(
            DASHBOARD_GET_TEAM_ADMIN_SETTINGS_PATH,
            "/aiserver.v1.DashboardService/GetTeamAdminSettingsOrEmptyIfNotInTeam"
        );
        assert_eq!(PR_REVIEW_REQUEST_TIMEOUT_MS, 10_000);
        assert_eq!(pr_review_destination_from_proto(1), Some("github"));
        assert_eq!(pr_review_destination_from_proto(2), Some("graphite"));
        assert_eq!(pr_review_destination_from_proto(3), Some("reviewCursor"));
        assert_eq!(pr_review_destination_from_proto(0), None);
        assert_eq!(pr_review_destination_from_proto(99), None);

        let mut user = Vec::new();
        encode_key(6, 0, &mut user);
        encode_varint(3, &mut user);
        assert_eq!(decode_user_pr_review_destination(&user).unwrap(), Some("reviewCursor"));

        let mut preferred = Vec::new();
        encode_key(1, 0, &mut preferred);
        encode_varint(2, &mut preferred);
        let mut fallback = Vec::new();
        encode_key(5, 0, &mut fallback);
        encode_varint(1, &mut fallback);
        let mut team = Vec::new();
        encode_len_delimited(7, &fallback, &mut team);
        encode_len_delimited(37, &preferred, &mut team);
        assert_eq!(decode_team_pr_review_destination(&team).unwrap(), Some("graphite"));

        let mut fallback_only = Vec::new();
        encode_len_delimited(7, &fallback, &mut fallback_only);
        assert_eq!(decode_team_pr_review_destination(&fallback_only).unwrap(), Some("github"));
    }

    #[test]
    fn dashboard_paths_and_request_fields_match_desktop_contract() {
        assert_eq!(
            DASHBOARD_CHECK_HTTP_MCP_STATUS_PATH,
            "/aiserver.v1.DashboardService/CheckHttpMcpStatus"
        );
        assert_eq!(
            DASHBOARD_VALIDATE_MCP_OAUTH_TOKENS_PATH,
            "/aiserver.v1.DashboardService/ValidateMcpOAuthTokens"
        );
        assert_eq!(
            DASHBOARD_COMPLETE_MCP_OAUTH_PATH,
            "/aiserver.v1.DashboardService/CompleteMcpOAuth"
        );
        assert_eq!(
            DASHBOARD_DELETE_MCP_OAUTH_TOKEN_PATH,
            "/aiserver.v1.DashboardService/DeleteMcpOAuthToken"
        );
        assert_eq!(
            DASHBOARD_RENAME_MCP_OAUTH_ACCOUNT_PATH,
            "/aiserver.v1.DashboardService/RenameMcpOAuthAccount"
        );
        assert_eq!(
            DASHBOARD_DELETE_MCP_OAUTH_ACCOUNT_PATH,
            "/aiserver.v1.DashboardService/DeleteMcpOAuthAccount"
        );
        let request = encode_check_http_mcp_status_request(
            17,
            "http://127.0.0.1:18080/oauth/callback",
            true,
            "work",
        );
        assert!(request.windows(2).any(|window| window == [0x28, 0x01]));
        assert!(request.contains(&0x42));
        let complete = encode_complete_mcp_oauth_request("state-0123456789", "authorization-code");
        assert_eq!(complete.first().copied(), Some(0x0a));
        assert!(complete.windows(2).any(|window| window[0] == 0x12));
        assert!(complete.ends_with(b"authorization-code"));
        let validate =
            encode_validate_mcp_oauth_tokens_request("https://mcp.example.test", "work");
        assert_eq!(validate.first().copied(), Some(0x1a));

        let logout =
            encode_delete_mcp_oauth_token_request("https://mcp.example.test", "work");
        assert_eq!(logout.first().copied(), Some(0x0a));
        assert!(logout.windows(4).any(|window| window == b"sand"));
        assert!(logout.ends_with(b"work"));

        let rename = encode_rename_mcp_oauth_account_request(17, "work", "personal");
        assert_eq!(rename[..2], [0x08, 0x11]);
        assert!(rename.ends_with(b"personal"));

        let delete = encode_delete_mcp_oauth_account_request(17, "work");
        assert_eq!(delete[..2], [0x08, 0x11]);
        assert!(delete.ends_with(b"work"));
        assert!(parse_mcp_server_id("0").is_err());
        assert!(validate_mcp_account_key("bad\nkey", "account key").is_err());
    }

    #[test]
    fn decodes_available_server_admin_policy_wire_shape() {
        let mut nested = Vec::new();
        encode_key(1, 0, &mut nested);
        encode_varint(17, &mut nested);
        encode_string(2, "Calendar", &mut nested);
        encode_bool(4, false, &mut nested);
        encode_string(5, "http", &mut nested);
        encode_string(8, "https://mcp.example.test", &mut nested);
        encode_bool(14, true, &mut nested);
        encode_string(15, "calendar/default", &mut nested);
        let response = len_field(1, &nested);
        assert_eq!(
            decode_available_mcp_servers_response(&response).unwrap(),
            vec![AvailableServerWire {
                id: 17,
                name: "Calendar".into(),
                enabled: false,
                transport: "http".into(),
                url: Some("https://mcp.example.test".into()),
                disabled_by_team_admin_policy: true,
                server_identifier: Some("calendar/default".into()),
            }]
        );
    }

    #[test]
    fn available_server_projection_is_bounded_and_fail_closed() {
        let projected = project_available_server(AvailableServerWire {
            id: 17,
            name: " Calendar ".into(),
            enabled: true,
            transport: "streamable-http".into(),
            url: Some("https://mcp.example.test/api".into()),
            disabled_by_team_admin_policy: true,
            server_identifier: Some("calendar/default".into()),
        })
        .unwrap();
        assert_eq!(projected.server_id, "17");
        assert_eq!(projected.server_name, "Calendar");
        assert_eq!(projected.server_url.as_deref(), Some("https://mcp.example.test/api"));
        assert_eq!(projected.transport, McpAuthTransport::Http);
        assert!(projected.disabled_by_team_admin_policy);

        let invalid_id = AvailableServerWire {
            id: 0,
            name: "Calendar".into(),
            enabled: true,
            transport: "http".into(),
            url: Some("https://mcp.example.test".into()),
            disabled_by_team_admin_policy: false,
            server_identifier: None,
        };
        assert!(project_available_server(invalid_id).is_err());

        let invalid_transport = AvailableServerWire {
            id: 18,
            name: "Drive".into(),
            enabled: true,
            transport: "pipe".into(),
            url: Some("https://mcp.example.test".into()),
            disabled_by_team_admin_policy: false,
            server_identifier: None,
        };
        assert!(project_available_server(invalid_transport).is_err());

        let invalid_url = AvailableServerWire {
            id: 19,
            name: "Docs".into(),
            enabled: true,
            transport: "sse".into(),
            url: Some("file:///tmp/mcp.sock".into()),
            disabled_by_team_admin_policy: false,
            server_identifier: None,
        };
        assert!(project_available_server(invalid_url).is_err());

        let stdio = project_available_server(AvailableServerWire {
            id: 20,
            name: "Local".into(),
            enabled: true,
            transport: "stdio".into(),
            url: None,
            disabled_by_team_admin_policy: false,
            server_identifier: Some("local/default".into()),
        })
        .unwrap();
        assert_eq!(stdio.transport, McpAuthTransport::Stdio);
        assert!(stdio.server_url.is_none());
    }

    #[test]
    fn list_tools_accounts_preserve_account_label_and_display_row_owner() {
        let mut primary = Vec::new();
        encode_string(1, "calendar/work", &mut primary);
        encode_string(4, "work", &mut primary);
        encode_string(5, "calendar/default", &mut primary);

        let mut personal = Vec::new();
        encode_string(1, "calendar/personal", &mut personal);
        encode_string(4, "personal", &mut personal);
        encode_string(5, "calendar/default", &mut personal);

        let mut unrelated = Vec::new();
        encode_string(1, "drive/work", &mut unrelated);
        encode_string(4, "work", &mut unrelated);
        encode_string(5, "drive/default", &mut unrelated);

        let mut response = Vec::new();
        encode_len_delimited(1, &primary, &mut response);
        encode_len_delimited(1, &personal, &mut response);
        encode_len_delimited(1, &unrelated, &mut response);

        assert_eq!(
            decode_list_sand_mcp_accounts(&response, "calendar/default").unwrap(),
            vec![
                McpServerAccountSummary {
                    account_key: "work".into(),
                    row_server_identifier: "calendar/default".into(),
                },
                McpServerAccountSummary {
                    account_key: "personal".into(),
                    row_server_identifier: "calendar/default".into(),
                },
            ]
        );
    }

    #[test]
    fn execute_tool_request_and_result_match_desktop_wire_contract() {
        assert_eq!(
            DASHBOARD_EXECUTE_SAND_MCP_TOOL_PATH,
            "/aiserver.v1.DashboardService/ExecuteSandMcpTool"
        );
        let args = json!({"query":"today","limit":2,"flags":[true,null]});
        let encoded_args = encode_json_struct(&args, 0).unwrap();
        assert_eq!(decode_proto_struct(&encoded_args, 0).unwrap(), args);
        let request = encode_execute_sand_mcp_tool_request(
            "calendar/default",
            "events.list",
            &encoded_args,
            "call-17",
            "agent-2",
        );
        assert_eq!(optional_string_field(&request, 1).unwrap().as_deref(), Some("calendar/default"));
        assert_eq!(optional_string_field(&request, 2).unwrap().as_deref(), Some("events.list"));
        assert_eq!(optional_string_field(&request, 4).unwrap().as_deref(), Some("call-17"));
        assert_eq!(optional_string_field(&request, 5).unwrap().as_deref(), Some("agent-2"));
        assert_eq!(
            decode_proto_struct(first_message_field(&request, 3).unwrap().unwrap(), 0).unwrap(),
            args
        );

        let mut text = Vec::new();
        encode_string(1, "done", &mut text);
        let mut content_item = Vec::new();
        encode_len_delimited(1, &text, &mut content_item);
        let mut success = Vec::new();
        encode_len_delimited(1, &content_item, &mut success);
        encode_bool(2, false, &mut success);
        let mut result = Vec::new();
        encode_len_delimited(1, &success, &mut result);
        let mut response = Vec::new();
        encode_len_delimited(1, &result, &mut response);
        assert_eq!(
            decode_execute_sand_mcp_tool_response(&response).unwrap(),
            json!({
                "case":"success",
                "content":[{"type":"text","text":"done"}],
                "isError":false,
                "structuredContent":Value::Null,
            })
        );
        assert!(is_timeout_like_rpc_error("MCP backend RPC failed: timed out"));
        assert!(!is_timeout_like_rpc_error("MCP backend RPC failed: HTTP 401"));
    }

    #[test]
    fn list_tools_reload_request_and_response_match_desktop_wire_contract() {
        let request = encode_list_sand_mcp_tools_request(&["calendar/default"]);
        assert_eq!(request[0], 0x0a);
        assert!(request.ends_with(b"calendar/default"));

        let mut server = Vec::new();
        encode_string(1, "calendar/default", &mut server);
        encode_string(2, "ready", &mut server);
        let mut list_tool = Vec::new();
        encode_string(1, "List events", &mut list_tool);
        encode_string(2, "List calendar events", &mut list_tool);
        encode_string(4, "calendar/default", &mut list_tool);
        encode_string(5, "list", &mut list_tool);
        encode_len_delimited(3, &list_tool, &mut server);
        let mut create_tool = Vec::new();
        encode_string(1, "Create event", &mut create_tool);
        encode_string(4, "calendar/default", &mut create_tool);
        encode_string(5, "create", &mut create_tool);
        encode_len_delimited(3, &create_tool, &mut server);
        let response = len_field(1, &server);
        let tools = decode_list_sand_mcp_tools(&response, "calendar/default").unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "list");
        assert_eq!(tools[0].title.as_deref(), Some("List events"));
        assert_eq!(tools[0].description.as_deref(), Some("List calendar events"));
        assert_eq!(tools[1].name, "create");
        assert_eq!(decode_list_sand_mcp_tools_count(&response, "calendar/default").unwrap(), 2);
        assert!(decode_list_sand_mcp_tools_count(&response, "other").is_err());
    }

    #[test]
    fn decodes_desktop_check_status_wire_shape() {
        let mut nested = Vec::new();
        encode_key(1, 0, &mut nested);
        encode_varint(17, &mut nested);
        encode_bool(2, true, &mut nested);
        encode_bool(3, true, &mut nested);
        encode_string(4, "https://auth.example.test", &mut nested);
        encode_bool(6, false, &mut nested);
        let response = len_field(1, &nested);
        let decoded = decode_check_http_mcp_status_response(&response).unwrap();
        assert_eq!(
            decoded,
            vec![CheckStatusWire {
                id: 17,
                is_available: true,
                requires_auth: true,
                auth_url: Some("https://auth.example.test".into()),
                error: None,
                has_valid_token: false,
            }]
        );
    }

    #[test]
    fn decodes_desktop_validate_tokens_wire_shape_and_account_fence() {
        let mut nested = Vec::new();
        encode_string(1, "https://mcp.example.test", &mut nested);
        encode_bool(2, true, &mut nested);
        encode_string(3, "work", &mut nested);
        let response = len_field(1, &nested);
        let decoded = decode_validate_mcp_oauth_tokens_response(&response).unwrap();
        assert_eq!(
            decoded,
            vec![ValidateResultWire {
                server_url: "https://mcp.example.test".into(),
                has_valid_token: true,
                account_key: "work".into(),
            }]
        );
    }

    #[test]
    fn checksum_matches_desktop_algorithm_and_binds_machine_id() {
        let first = create_cursor_checksum("machine-a", 1_728_000_000_000);
        let second = create_cursor_checksum("machine-b", 1_728_000_000_000);
        assert!(first.ends_with("machine-a"));
        assert!(second.ends_with("machine-b"));
        assert_ne!(first, second);
    }

    #[test]
    fn privacy_mode_wire_values_and_ghost_mode_match_desktop_contract() {
        assert_eq!(
            PRIVACY_MODE_FETCH_TIMEOUT_MS, 10_000,
            "Desktop cursorAccount.getPrivacyModeEnabled uses the 10s Dashboard profile RPC contract"
        );
        assert_eq!(SandPrivacyMode::from_proto(0), Some(SandPrivacyMode::Unspecified));
        assert_eq!(SandPrivacyMode::from_proto(1), Some(SandPrivacyMode::NoStorage));
        assert_eq!(SandPrivacyMode::from_proto(2), Some(SandPrivacyMode::NoTraining));
        assert_eq!(
            SandPrivacyMode::from_proto(3),
            Some(SandPrivacyMode::UsageDataTrainingAllowed)
        );
        assert_eq!(
            SandPrivacyMode::from_proto(4),
            Some(SandPrivacyMode::UsageCodebaseTrainingAllowed)
        );
        assert_eq!(SandPrivacyMode::from_proto(5), None);
        assert_eq!(ghost_mode_from_privacy_mode(None), "true");
        assert_eq!(
            ghost_mode_from_privacy_mode(Some(SandPrivacyMode::NoStorage)),
            "true"
        );
        assert_eq!(
            ghost_mode_from_privacy_mode(Some(SandPrivacyMode::NoTraining)),
            "true"
        );
        assert_eq!(
            ghost_mode_from_privacy_mode(Some(SandPrivacyMode::UsageDataTrainingAllowed)),
            "false"
        );
        assert_eq!(
            ghost_mode_from_privacy_mode(Some(SandPrivacyMode::UsageCodebaseTrainingAllowed)),
            "false"
        );
    }

    #[test]
    fn process_provider_fails_closed_without_dedicated_sand_credential() {
        let provider = ProcessSandMcpCredentialProvider::for_test(
            "machine-a",
            DEFAULT_CURSOR_BACKEND_URL,
            None,
        );
        let error = provider.credentials().unwrap_err();
        assert!(error.contains("credential is unavailable"));
    }
}

#[cfg(test)]
mod access_blocked_submit_logs_tests {
    use super::*;
    #[test]
    fn access_blocked_projection_encodes_for_analytics_submit_logs() {
        let projection = AccountTelemetryProjection { stream:"access-blocked".into(), level:"warn".into(), metadata:json!({"edge":"entered","cause":"seat_revoked","access_reason":"teamAccessRequired","ms_blocked":"42"}) };
        let request = encode_account_telemetry_submit_logs_request(&[projection], "machine-1", "0.18.0", 1_728_000_000_000).unwrap();
        let logs = repeated_message_fields(&request, 1).unwrap();
        assert_eq!(decode_optional_varint_field(logs[0], 1).unwrap(), Some(3));
        assert_eq!(string_fields(logs[0], 2).unwrap(), vec![ACCESS_BLOCKED_EVENT.to_string()]);
    }
}

#[cfg(test)]
mod feedback_submit_contract_tests {
    use super::*;

    #[test]
    fn feedback_http_statuses_preserve_desktop_error_contract() {
        assert_eq!(feedback_error_for_status(400).code(), "invalid-feedback");
        assert_eq!(feedback_error_for_status(402).code(), "subscription-required");
        assert_eq!(feedback_error_for_status(403).code(), "access-denied");
        assert_eq!(feedback_error_for_status(429).code(), "rate-limited");
        assert_eq!(feedback_error_for_status(500).code(), "unavailable");
    }
}
