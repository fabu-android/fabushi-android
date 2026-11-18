use crate::action_bus::{ActionRegistry, ActionRiskLevel};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CliArgumentSpec {
    pub name: String,
    pub flag: String,
    pub description: String,
    pub value_type: String,
    pub required: bool,
    pub default_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CliSubcommandSpec {
    pub name: String,
    pub kebab_name: String,
    pub category: String,
    pub description: String,
    pub risk_level: ActionRiskLevel,
    pub requires_approval: bool,
    pub arguments: Vec<CliArgumentSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CliSchemaExport {
    pub program_name: String,
    pub version: String,
    pub subcommands: Vec<CliSubcommandSpec>,
}

pub struct CliExporter;

fn resolve_schema_ref<'a>(root: &'a Value, node: &'a Value, depth: usize) -> &'a Value {
    if depth >= 8 {
        return node;
    }
    if let Some(ref_str) = node.get("$ref").and_then(|r| r.as_str()) {
        if let Some(name) = ref_str.strip_prefix("#/$defs/") {
            if let Some(target) = root
                .get("$defs")
                .and_then(|d| d.as_object())
                .and_then(|m| m.get(name))
            {
                return resolve_schema_ref(root, target, depth + 1);
            }
        } else if let Some(name) = ref_str.strip_prefix("#/definitions/") {
            if let Some(target) = root
                .get("definitions")
                .and_then(|d| d.as_object())
                .and_then(|m| m.get(name))
            {
                return resolve_schema_ref(root, target, depth + 1);
            }
        }
    }
    node
}

fn is_null_schema(root: &Value, node: &Value, depth: usize) -> bool {
    let resolved = resolve_schema_ref(root, node, depth);
    match resolved.get("type") {
        Some(Value::String(s)) => s == "null",
        Some(Value::Array(arr)) => {
            !arr.is_empty() && arr.iter().all(|v| v.as_str() == Some("null"))
        }
        _ => false,
    }
}

fn extract_schema_type_inner(root: &Value, node: &Value, depth: usize) -> Option<String> {
    if depth >= 8 {
        return None;
    }
    let resolved = resolve_schema_ref(root, node, depth);

    match resolved.get("type") {
        Some(Value::String(s)) if s != "null" => return Some(s.clone()),
        Some(Value::Array(arr)) => {
            for item in arr {
                if let Some(s) = item.as_str() {
                    if s != "null" {
                        return Some(s.to_string());
                    }
                }
            }
        }
        _ => {}
    }

    for key in ["anyOf", "oneOf", "allOf"] {
        if let Some(arr) = resolved.get(key).and_then(|v| v.as_array()) {
            for sub in arr {
                if is_null_schema(root, sub, depth + 1) {
                    continue;
                }
                if let Some(t) = extract_schema_type_inner(root, sub, depth + 1) {
                    return Some(t);
                }
            }
        }
    }

    if resolved.get("properties").is_some() || resolved.get("additionalProperties").is_some() {
        return Some("object".to_string());
    }
    if resolved.get("items").is_some() {
        return Some("array".to_string());
    }

    None
}

fn extract_schema_type(root: &Value, node: &Value) -> String {
    extract_schema_type_inner(root, node, 0).unwrap_or_else(|| "string".to_string())
}

impl CliExporter {
    pub fn export_cli_schema(
        registry: &ActionRegistry,
        program_name: &str,
        version: &str,
    ) -> CliSchemaExport {
        let mut subcommands = Vec::new();

        for action in registry.list() {
            let kebab_name = action.name().replace('_', "-");
            let mut arguments = Vec::new();

            let schema = action.input_schema();
            let resolved_root = resolve_schema_ref(&schema, &schema, 0);
            let props_opt = resolved_root
                .get("properties")
                .and_then(|p| p.as_object());

            if let Some(props) = props_opt {
                let required_list: Vec<&str> = resolved_root
                    .get("required")
                    .and_then(|r| r.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();

                for (key, prop_val) in props {
                    let resolved_prop = resolve_schema_ref(&schema, prop_val, 0);
                    let desc = prop_val
                        .get("description")
                        .or_else(|| resolved_prop.get("description"))
                        .and_then(|d| d.as_str())
                        .unwrap_or("")
                        .to_string();
                    let vtype = extract_schema_type(&schema, prop_val);
                    let flag = format!("--{}", key.replace('_', "-"));
                    let required = required_list.contains(&key.as_str());
                    let default_val = prop_val
                        .get("default")
                        .or_else(|| resolved_prop.get("default"))
                        .map(|d| d.to_string());

                    arguments.push(CliArgumentSpec {
                        name: key.clone(),
                        flag,
                        description: desc,
                        value_type: vtype,
                        required,
                        default_value: default_val,
                    });
                }
            }

            // Stable order for arguments
            arguments.sort_by(|a, b| a.name.cmp(&b.name));

            let risk_level = action.risk_level();
            subcommands.push(CliSubcommandSpec {
                name: action.name().to_string(),
                kebab_name,
                category: action.category().as_str().to_string(),
                description: action.description().to_string(),
                risk_level,
                requires_approval: risk_level.requires_explicit_confirmation(),
                arguments,
            });
        }

        CliSchemaExport {
            program_name: program_name.to_string(),
            version: version.to_string(),
            subcommands,
        }
    }
}
