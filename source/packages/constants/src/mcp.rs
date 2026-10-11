#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct McpOAuthProviderPolicy {
    pub provider: &'static str,
    pub client_registration: &'static str,
    pub unauthenticated_connect: bool,
    pub rejects_custom_scheme_redirects: bool,
    pub access_type: &'static str,
    pub prompt: &'static str,
    pub scopes: &'static [&'static str],
}

const GMAIL_SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/gmail.readonly",
    "https://www.googleapis.com/auth/gmail.compose",
    "https://www.googleapis.com/auth/gmail.modify",
];
const DRIVE_SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/drive.readonly",
    "https://www.googleapis.com/auth/drive.file",
];
const CALENDAR_SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/calendar.events",
    "https://www.googleapis.com/auth/calendar.calendarlist.readonly",
    "https://www.googleapis.com/auth/calendar.events.readonly",
    "https://www.googleapis.com/auth/calendar.events.freebusy",
];
const DOCS_SCOPES: &[&str] = &["https://www.googleapis.com/auth/documents"];
const SHEETS_SCOPES: &[&str] = &["https://www.googleapis.com/auth/spreadsheets"];
const SLIDES_SCOPES: &[&str] = &["https://www.googleapis.com/auth/presentations"];

pub const GOOGLE_WORKSPACE_MCP_HOSTS: &[&str] = &[
    "gmailmcp.googleapis.com",
    "drivemcp.googleapis.com",
    "calendarmcp.googleapis.com",
    "docsmcp.googleapis.com",
    "sheetsmcp.googleapis.com",
    "slidesmcp.googleapis.com",
];

pub const MCP_OAUTH_EXTENSION_ID: &str = "anysphere.cursor-mcp";
pub const MCP_OAUTH_RETURN_PATH: &str = "/oauth/return";
pub const MCP_OAUTH_DESKTOP_RETURN_URL: &str = "cursor://anysphere.cursor-mcp/oauth/return";
pub const MCP_OAUTH_LOOPBACK_CALLBACK_URL: &str = "http://localhost:8787/callback";
pub const FABUSHI_ANDROID_MCP_OAUTH_CALLBACK_URL: &str = "fabushi://mcp-oauth/callback";

fn google_workspace_policy(scopes: &'static [&'static str]) -> McpOAuthProviderPolicy {
    McpOAuthProviderPolicy {
        provider: "google-workspace",
        client_registration: "static",
        unauthenticated_connect: true,
        rejects_custom_scheme_redirects: true,
        access_type: "offline",
        prompt: "consent",
        scopes,
    }
}

pub fn mcp_oauth_provider_policy(hostname: &str) -> Option<McpOAuthProviderPolicy> {
    match hostname.to_ascii_lowercase().as_str() {
        "gmailmcp.googleapis.com" => Some(google_workspace_policy(GMAIL_SCOPES)),
        "drivemcp.googleapis.com" => Some(google_workspace_policy(DRIVE_SCOPES)),
        "calendarmcp.googleapis.com" => Some(google_workspace_policy(CALENDAR_SCOPES)),
        "docsmcp.googleapis.com" => Some(google_workspace_policy(DOCS_SCOPES)),
        "sheetsmcp.googleapis.com" => Some(google_workspace_policy(SHEETS_SCOPES)),
        "slidesmcp.googleapis.com" => Some(google_workspace_policy(SLIDES_SCOPES)),
        _ => None,
    }
}

pub fn is_google_workspace_mcp_host(hostname: &str) -> bool {
    mcp_oauth_provider_policy(hostname)
        .is_some_and(|policy| policy.provider == "google-workspace")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_provider_policy_is_case_insensitive_and_requires_offline_consent() {
        let policy = mcp_oauth_provider_policy("GMAILMCP.GOOGLEAPIS.COM").unwrap();
        assert_eq!(policy.provider, "google-workspace");
        assert_eq!(policy.access_type, "offline");
        assert_eq!(policy.prompt, "consent");
        assert!(policy.scopes.iter().any(|scope| scope.ends_with("gmail.modify")));
        assert!(is_google_workspace_mcp_host("drivemcp.googleapis.com"));
        assert!(mcp_oauth_provider_policy("evil.example").is_none());
    }

    #[test]
    fn android_callback_is_not_the_desktop_or_loopback_transport() {
        assert!(FABUSHI_ANDROID_MCP_OAUTH_CALLBACK_URL.starts_with("fabushi://"));
        assert_ne!(FABUSHI_ANDROID_MCP_OAUTH_CALLBACK_URL, MCP_OAUTH_DESKTOP_RETURN_URL);
        assert_ne!(FABUSHI_ANDROID_MCP_OAUTH_CALLBACK_URL, MCP_OAUTH_LOOPBACK_CALLBACK_URL);
    }
}
