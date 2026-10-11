use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginVariableField {
    pub key: String,
    pub label: String,
    pub placeholder: String,
    pub is_required: bool,
    pub is_secret: bool,
    pub default_value: Option<String>,
    pub hint: Option<String>,
}

const ACRONYMS: &[&str] = &[
    "url", "uri", "api", "id", "ssl", "tls", "http", "https", "db", "aws", "gcp",
];

pub fn humanize_variable_name(name: &str) -> String {
    name.split('_')
        .filter(|word| !word.is_empty())
        .map(|word| {
            let lower = word.to_ascii_lowercase();
            if ACRONYMS.contains(&lower.as_str()) {
                lower.to_ascii_uppercase()
            } else {
                let mut chars = lower.chars();
                match chars.next() {
                    Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                    None => String::new(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn key_looks_secret(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    ["TOKEN", "SECRET", "KEY", "PASSWORD", "CREDENTIAL"]
        .iter()
        .any(|needle| upper.contains(needle))
}

pub fn plugin_variables_schema_to_fields(schema: &Value) -> Vec<PluginVariableField> {
    let Some(root) = schema.as_object() else {
        return Vec::new();
    };
    let Some(properties) = root.get("properties").and_then(Value::as_object) else {
        return Vec::new();
    };
    let required: BTreeSet<&str> = root
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();

    properties
        .iter()
        .map(|(key, value)| {
            let property = value.as_object();
            let title = property
                .and_then(|property| property.get("title"))
                .and_then(Value::as_str);
            let description = property
                .and_then(|property| property.get("description"))
                .and_then(Value::as_str);
            let default_value = property
                .and_then(|property| property.get("default"))
                .and_then(Value::as_str);
            let is_secret = property
                .and_then(|property| property.get("format"))
                .and_then(Value::as_str)
                == Some("password")
                || property
                    .and_then(|property| property.get("writeOnly"))
                    .and_then(Value::as_bool)
                    == Some(true)
                || key_looks_secret(key);
            PluginVariableField {
                key: key.clone(),
                label: title
                    .map(str::to_string)
                    .unwrap_or_else(|| humanize_variable_name(key)),
                placeholder: key.clone(),
                is_required: required.contains(key.as_str()),
                is_secret,
                default_value: default_value.map(str::to_string),
                hint: description.map(str::to_string),
            }
        })
        .collect()
}

pub fn find_missing_required_catalog_fields<'a>(
    fields: &'a [PluginVariableField],
    values: &BTreeMap<String, String>,
) -> Vec<&'a PluginVariableField> {
    fields
        .iter()
        .filter(|field| {
            if !field.is_required {
                return false;
            }
            let value = values
                .get(&field.key)
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .or_else(|| {
                    field
                        .default_value
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                });
            value.is_none()
        })
        .collect()
}

pub fn apply_catalog_defaults(
    fields: &[PluginVariableField],
    values: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    fields
        .iter()
        .filter_map(|field| {
            values
                .get(&field.key)
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .or_else(|| {
                    field
                        .default_value
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned)
                })
                .map(|value| (field.key.clone(), value))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn schema_fields_match_desktop_labels_required_defaults_and_secret_rules() {
        let fields = plugin_variables_schema_to_fields(&json!({
            "type":"object",
            "properties":{
                "api_url":{"description":"Endpoint","default":"https://example.test"},
                "ACCESS_TOKEN":{"title":"Access token"},
                "username":{"type":"string"},
                "passphrase":{"format":"password"},
                "client_id":{"writeOnly":true}
            },
            "required":["ACCESS_TOKEN","username"]
        }));
        let api_url = fields.iter().find(|field| field.key == "api_url").unwrap();
        assert_eq!(api_url.label, "API URL");
        assert_eq!(api_url.placeholder, "api_url");
        assert_eq!(api_url.default_value.as_deref(), Some("https://example.test"));
        assert_eq!(api_url.hint.as_deref(), Some("Endpoint"));
        assert!(fields.iter().find(|field| field.key == "ACCESS_TOKEN").unwrap().is_secret);
        assert!(fields.iter().find(|field| field.key == "passphrase").unwrap().is_secret);
        assert!(fields.iter().find(|field| field.key == "client_id").unwrap().is_secret);
        assert!(!fields.iter().find(|field| field.key == "username").unwrap().is_secret);
    }

    #[test]
    fn missing_required_uses_trimmed_value_or_default() {
        let fields = plugin_variables_schema_to_fields(&json!({
            "properties":{
                "required_value":{},
                "defaulted":{"default":"fallback"},
                "optional":{}
            },
            "required":["required_value","defaulted"]
        }));
        let values = BTreeMap::from([
            ("required_value".into(), "   ".into()),
            ("optional".into(), "ignored".into()),
        ]);
        let missing = find_missing_required_catalog_fields(&fields, &values);
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].key, "required_value");

        let values = BTreeMap::from([("required_value".into(), "present".into())]);
        assert!(find_missing_required_catalog_fields(&fields, &values).is_empty());
        assert_eq!(
            apply_catalog_defaults(&fields, &values)
                .get("defaulted")
                .map(String::as_str),
            Some("fallback")
        );
    }
}
