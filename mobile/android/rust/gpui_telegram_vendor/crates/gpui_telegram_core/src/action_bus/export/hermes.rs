use crate::action_bus::{ActionCategory, ActionRegistry, ActionRiskLevel};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HermesToolDefinition {
    pub name: String,
    pub description: String,
    pub category: ActionCategory,
    pub parameters_schema: Value,
    pub risk_level: ActionRiskLevel,
    pub requires_approval: bool,
    pub read_only: bool,
}

impl HermesToolDefinition {
    pub fn to_openai_tool(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters_schema,
            }
        })
    }

    pub fn to_anthropic_tool(&self) -> Value {
        json!({
            "name": self.name,
            "description": self.description,
            "input_schema": self.parameters_schema,
        })
    }
}

pub struct HermesToolExporter;

impl HermesToolExporter {
    pub fn export_tools(registry: &ActionRegistry) -> Vec<HermesToolDefinition> {
        registry
            .list()
            .into_iter()
            .map(|action| {
                let risk = action.risk_level();
                HermesToolDefinition {
                    name: action.name().to_string(),
                    description: action.description().to_string(),
                    category: action.category(),
                    parameters_schema: action.input_schema(),
                    risk_level: risk,
                    requires_approval: risk.requires_explicit_confirmation(),
                    read_only: risk.is_read_only(),
                }
            })
            .collect()
    }

    pub fn export_openai_tools(registry: &ActionRegistry) -> Vec<Value> {
        Self::export_tools(registry)
            .into_iter()
            .map(|t| t.to_openai_tool())
            .collect()
    }

    pub fn export_anthropic_tools(registry: &ActionRegistry) -> Vec<Value> {
        Self::export_tools(registry)
            .into_iter()
            .map(|t| t.to_anthropic_tool())
            .collect()
    }
}
