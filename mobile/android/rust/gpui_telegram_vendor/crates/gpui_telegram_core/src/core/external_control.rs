//! Telegram Desktop External Control IPC & Automation Interface
//! Upstream sources: `Telegram/SourceFiles/core/external_control.h`, `external_control.cpp`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalCommand {
    pub action: String,
    pub target: Option<String>,
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalResponse {
    pub ok: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

/// Local IPC automation dispatcher allowing external control over Telegram windows and accounts.
#[derive(Debug, Clone, Default)]
pub struct ExternalControl {
    pub automation_enabled: bool,
}

impl ExternalControl {
    pub fn new() -> Self {
        Self {
            automation_enabled: true,
        }
    }

    pub fn handle_command(&mut self, command_json: &str) -> String {
        let cmd: Result<ExternalCommand, _> = serde_json::from_str(command_json);
        let resp = match cmd {
            Ok(c) => self.execute(c),
            Err(e) => ExternalResponse {
                ok: false,
                result: None,
                error: Some(format!("Invalid JSON: {}", e)),
            },
        };
        serde_json::to_string(&resp).unwrap_or_else(|_| "{\"ok\":false}".to_string())
    }

    fn execute(&mut self, cmd: ExternalCommand) -> ExternalResponse {
        if !self.automation_enabled {
            return ExternalResponse {
                ok: false,
                result: None,
                error: Some("Automation disabled".to_string()),
            };
        }

        match cmd.action.as_str() {
            "ping" => ExternalResponse {
                ok: true,
                result: Some(serde_json::json!({ "pong": true })),
                error: None,
            },
            "get_status" => ExternalResponse {
                ok: true,
                result: Some(serde_json::json!({
                    "version": crate::core::version::APP_VERSION_STR,
                    "automation": self.automation_enabled,
                })),
                error: None,
            },
            other => ExternalResponse {
                ok: false,
                result: None,
                error: Some(format!("Unknown action: {}", other)),
            },
        }
    }
}
