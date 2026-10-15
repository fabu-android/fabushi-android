use std::sync::Arc;

use prost::Message;

use crate::mcp_auth::dashboard_mcp_auth_backend::CursorDashboardMcpAuthBackend;

pub const DASHBOARD_GET_SLACK_USER_SETTINGS_PATH: &str =
    "/aiserver.v1.DashboardService/GetSlackUserSettings";
pub const DASHBOARD_GET_SCM_CONNECTION_STATUS_PATH: &str =
    "/aiserver.v1.DashboardService/GetScmConnectionStatus";
pub const LISTENER_CONNECTIVITY_TIMEOUT_MS: u64 = 10_000;

#[derive(Clone, PartialEq, Message)]
struct GetSlackUserSettingsRequest {
    #[prost(int32, tag = "1")]
    team_id: i32,
}

#[derive(Clone, PartialEq, Message)]
struct GetSlackUserSettingsResponse {
    #[prost(bool, tag = "1")]
    has_slack_auth: bool,
}

#[derive(Clone, PartialEq, Message)]
struct GetScmConnectionStatusRequest {
    #[prost(string, optional, tag = "1")]
    ghe_application: Option<String>,
    #[prost(int32, optional, tag = "2")]
    team_id: Option<i32>,
}

#[derive(Clone, PartialEq, Message)]
struct GetScmConnectionStatusResponse {
    #[prost(bool, tag = "1")]
    connected: bool,
}

#[derive(Clone)]
pub struct AndroidListenerConnectivity {
    dashboard: Arc<CursorDashboardMcpAuthBackend>,
}

impl AndroidListenerConnectivity {
    pub fn new(dashboard: Arc<CursorDashboardMcpAuthBackend>) -> Self {
        Self { dashboard }
    }

    pub fn is_platform_connected(&self, platform: &str) -> Result<bool, String> {
        match platform {
            "slack" => {
                let request = GetSlackUserSettingsRequest { team_id: 0 };
                let response = self.dashboard.send_listener_integration_unary(
                    DASHBOARD_GET_SLACK_USER_SETTINGS_PATH,
                    &request.encode_to_vec(),
                    LISTENER_CONNECTIVITY_TIMEOUT_MS,
                )?;
                GetSlackUserSettingsResponse::decode(response.as_slice())
                    .map(|response| response.has_slack_auth)
                    .map_err(|error| format!("invalid Slack connection status response: {error}"))
            }
            "github" => {
                let request = GetScmConnectionStatusRequest {
                    ghe_application: None,
                    team_id: None,
                };
                let response = self.dashboard.send_listener_integration_unary(
                    DASHBOARD_GET_SCM_CONNECTION_STATUS_PATH,
                    &request.encode_to_vec(),
                    LISTENER_CONNECTIVITY_TIMEOUT_MS,
                )?;
                GetScmConnectionStatusResponse::decode(response.as_slice())
                    .map(|response| response.connected)
                    .map_err(|error| format!("invalid GitHub connection status response: {error}"))
            }
            other => Err(format!("unsupported listener integration platform {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listener_dashboard_contract_matches_desktop_shipping_rpc_paths() {
        assert_eq!(
            DASHBOARD_GET_SLACK_USER_SETTINGS_PATH,
            "/aiserver.v1.DashboardService/GetSlackUserSettings"
        );
        assert_eq!(
            DASHBOARD_GET_SCM_CONNECTION_STATUS_PATH,
            "/aiserver.v1.DashboardService/GetScmConnectionStatus"
        );
        assert!(GetSlackUserSettingsRequest { team_id: 0 }.encode_to_vec().is_empty());
        assert!(GetScmConnectionStatusRequest { ghe_application: None, team_id: None }
            .encode_to_vec()
            .is_empty());
        assert!(GetSlackUserSettingsResponse::decode([0x08, 0x01].as_slice()).unwrap().has_slack_auth);
        assert!(GetScmConnectionStatusResponse::decode([0x08, 0x01].as_slice()).unwrap().connected);
    }
}
