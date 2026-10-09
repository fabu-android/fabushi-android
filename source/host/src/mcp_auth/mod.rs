pub mod android_mcp_auth_watch_manager;
pub mod dashboard_mcp_auth_backend;
pub mod host_mcp_auth_completion;
pub mod legacy_credential_cleanup;
pub mod mcp_auth_wait_registry;

pub use android_mcp_auth_watch_manager::AndroidMcpAuthWatchManager;
pub use dashboard_mcp_auth_backend::{
    CursorDashboardMcpAuthBackend, McpAuthBackendPort, ProcessSandMcpCredentialProvider,
    SandMcpBackendCredentials, SandMcpCredentialProvider,
};
pub use host_mcp_auth_completion::{
    HostMcpAuthCompletion, HostMcpAuthCompletionEvent, McpAuthCompletionRuntime,
};
pub use mcp_auth_wait_registry::{
    normalize_connector_name, McpAuthCompletionIdentity, McpAuthWaitRegistry,
    McpAuthWaitRegistration, DEFAULT_MCP_AUTH_WAIT_TTL_MS,
};

pub use legacy_credential_cleanup::{
    cleanup_legacy_mcp_auth_credentials, is_legacy_mcp_auth_file,
    LegacyMcpAuthCleanupOutcome, LegacyMcpAuthCleanupResult, LEGACY_MCP_AUTH_FILENAME,
};
