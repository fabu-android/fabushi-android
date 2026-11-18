use crate::action_bus::{ActionRegistry, ActionRiskLevel};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpToolSpec {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk_level: Option<ActionRiskLevel>,
    #[serde(default)]
    pub requires_approval: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpToolsListResponse {
    pub tools: Vec<McpToolSpec>,
}

pub struct McpExporter;

impl McpExporter {
    pub fn export_tools_list(registry: &ActionRegistry) -> McpToolsListResponse {
        let tools = registry
            .list()
            .into_iter()
            .map(|action| {
                let risk = action.risk_level();
                McpToolSpec {
                    name: action.name().to_string(),
                    description: action.description().to_string(),
                    input_schema: action.input_schema(),
                    risk_level: Some(risk),
                    requires_approval: risk.requires_explicit_confirmation(),
                }
            })
            .collect();
        McpToolsListResponse { tools }
    }

    pub fn export_jsonrpc_list(registry: &ActionRegistry, request_id: Value) -> Value {
        let list = Self::export_tools_list(registry);
        json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "result": list
        })
    }
}
