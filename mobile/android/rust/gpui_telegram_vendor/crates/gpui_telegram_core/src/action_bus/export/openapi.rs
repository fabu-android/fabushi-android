use crate::action_bus::ActionRegistry;
use serde_json::{json, Map, Value};

pub struct OpenApiExporter;

impl OpenApiExporter {
    pub fn export_spec(registry: &ActionRegistry, title: &str, version: &str, base_url: &str) -> Value {
        let mut paths = Map::new();

        for action in registry.list() {
            let path_key = format!("/api/v1/actions/{}", action.name());
            let op = json!({
                "summary": action.description(),
                "description": format!("Platform Action '{}' [Category: {}]", action.name(), action.category().as_str()),
                "operationId": action.name(),
                "tags": [action.category().as_str()],
                "x-risk-level": action.risk_level(),
                "x-requires-approval": action.risk_level().requires_explicit_confirmation(),
                "requestBody": {
                    "required": true,
                    "content": {
                        "application/json": {
                            "schema": action.input_schema()
                        }
                    }
                },
                "responses": {
                    "200": {
                        "description": "Successful action response",
                        "content": {
                            "application/json": {
                                "schema": action.output_schema()
                            }
                        }
                    },
                    "400": { "description": "Malformed JSON payload" },
                    "401": { "description": "Unauthorized" },
                    "403": { "description": "Forbidden / Approval required" },
                    "404": { "description": "Action not found" },
                    "422": { "description": "Invalid input / validation failed" },
                    "429": { "description": "Rate limited" },
                    "500": { "description": "Internal server execution failure" }
                }
            });

            let mut path_item = Map::new();
            path_item.insert("post".to_string(), op);
            paths.insert(path_key, Value::Object(path_item));
        }

        json!({
            "openapi": "3.1.0",
            "info": {
                "title": title,
                "version": version,
                "description": "Schema-First Platform Action Bus HTTP/WS API"
            },
            "servers": [{ "url": base_url }],
            "paths": paths
        })
    }
}
