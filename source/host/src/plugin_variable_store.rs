use fabushi_android_shared::node::mcp::mcp_plugin_variables::{
    apply_catalog_defaults, find_missing_required_catalog_fields, plugin_variables_schema_to_fields,
    PluginVariableField,
};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

const STORE_VERSION: u64 = 1;
const MAX_STORE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedPluginVariableWrite {
    pub plugin_id: String,
    pub account_key: String,
    pub schema: Value,
    pub public_config: BTreeMap<String, String>,
    pub secret_values: BTreeMap<String, String>,
    pub secret_keys: BTreeSet<String>,
    pub team_configured: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginRuntimeVariableConfig {
    pub plugin_id: String,
    pub account_key: String,
    pub public_config: BTreeMap<String, String>,
    pub secret_keys: BTreeSet<String>,
    pub team_configured: bool,
}

#[derive(Clone, Debug)]
struct PluginVariableEntry {
    schema: Value,
    public_config: BTreeMap<String, String>,
    secret_keys: BTreeSet<String>,
    team_configured: bool,
}

pub struct PluginVariableStore {
    path: PathBuf,
    entries: BTreeMap<(String, String), PluginVariableEntry>,
}

impl PluginVariableStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, String> {
        let path = path.into();
        let entries = match fs::metadata(&path) {
            Ok(metadata) if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_STORE_BYTES => {
                let _ = fs::remove_file(&path);
                BTreeMap::new()
            }
            Ok(_) => {
                let raw = fs::read_to_string(&path)
                    .map_err(|error| format!("failed to read plugin variable store: {error}"))?;
                parse_store(&raw).unwrap_or_else(|_| {
                    let _ = fs::remove_file(&path);
                    BTreeMap::new()
                })
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(error) => return Err(format!("failed to inspect plugin variable store: {error}")),
        };
        Ok(Self { path, entries })
    }

    pub fn fields(schema: &Value) -> Vec<PluginVariableField> {
        plugin_variables_schema_to_fields(schema)
    }

    pub fn prepare_write(
        &self,
        account_key: &str,
        plugin_id: &str,
        schema: &Value,
        values: &Value,
        team_configured: bool,
    ) -> Result<PreparedPluginVariableWrite, String> {
        let account_key = validate_identity(account_key, "account key")?;
        let plugin_id = validate_identity(plugin_id, "plugin id")?;
        let values = values
            .as_object()
            .ok_or("plugin variable values must be a JSON object")?;
        if values.len() > 256 {
            return Err("plugin variable values exceed bounded field count".into());
        }
        let fields = plugin_variables_schema_to_fields(schema);
        let allowed: BTreeSet<&str> = fields.iter().map(|field| field.key.as_str()).collect();
        let mut requested = BTreeMap::new();
        for (key, value) in values {
            if !allowed.contains(key.as_str()) {
                return Err(format!("plugin variable {key} is not declared by the catalog schema"));
            }
            let value = value
                .as_str()
                .ok_or_else(|| format!("plugin variable {key} must be a string"))?;
            if value.len() > 64 * 1024 || value.chars().any(|ch| ch == '\0') {
                return Err(format!("plugin variable {key} exceeds the bounded value contract"));
            }
            requested.insert(key.clone(), value.to_string());
        }

        if !team_configured {
            let missing = find_missing_required_catalog_fields(&fields, &requested);
            if !missing.is_empty() {
                return Err(format!(
                    "missing required plugin variables: {}",
                    missing
                        .into_iter()
                        .map(|field| field.key.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }

        let effective = apply_catalog_defaults(&fields, &requested);
        let secret_keys: BTreeSet<String> = fields
            .iter()
            .filter(|field| field.is_secret)
            .map(|field| field.key.clone())
            .collect();
        let mut public_config = BTreeMap::new();
        let mut secret_values = BTreeMap::new();
        for (key, value) in effective {
            if secret_keys.contains(&key) {
                secret_values.insert(key, value);
            } else {
                public_config.insert(key, value);
            }
        }

        Ok(PreparedPluginVariableWrite {
            plugin_id,
            account_key,
            schema: schema.clone(),
            public_config,
            secret_values,
            secret_keys,
            team_configured,
        })
    }

    pub fn commit_write(
        &mut self,
        prepared: &PreparedPluginVariableWrite,
    ) -> Result<(), String> {
        self.entries.insert(
            (prepared.account_key.clone(), prepared.plugin_id.clone()),
            PluginVariableEntry {
                schema: prepared.schema.clone(),
                public_config: prepared.public_config.clone(),
                secret_keys: prepared.secret_keys.clone(),
                team_configured: prepared.team_configured,
            },
        );
        self.persist()
    }

    pub fn runtime_config(
        &self,
        account_key: &str,
        plugin_id: &str,
    ) -> Result<PluginRuntimeVariableConfig, String> {
        let account_key = validate_identity(account_key, "account key")?;
        let plugin_id = validate_identity(plugin_id, "plugin id")?;
        let entry = self
            .entries
            .get(&(account_key.clone(), plugin_id.clone()))
            .ok_or("plugin variables are not configured for the current account")?;
        Ok(PluginRuntimeVariableConfig {
            plugin_id,
            account_key,
            public_config: entry.public_config.clone(),
            secret_keys: entry.secret_keys.clone(),
            team_configured: entry.team_configured,
        })
    }

    pub fn has_entry(&self, account_key: &str, plugin_id: &str) -> bool {
        self.entries
            .contains_key(&(account_key.to_string(), plugin_id.to_string()))
    }

    pub fn remove_plugin(&mut self, account_key: &str, plugin_id: &str) -> Result<bool, String> {
        let removed = self
            .entries
            .remove(&(account_key.to_string(), plugin_id.to_string()))
            .is_some();
        if removed {
            self.persist()?;
        }
        Ok(removed)
    }

    fn persist(&self) -> Result<(), String> {
        if self.entries.is_empty() {
            match fs::remove_file(&self.path) {
                Ok(()) => return Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(error) => return Err(format!("failed to clear plugin variable store: {error}")),
            }
        }
        let parent = self.path.parent().ok_or("plugin variable store requires parent directory")?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create plugin variable store directory: {error}"))?;
        let entries = self
            .entries
            .iter()
            .map(|((account_key, plugin_id), entry)| {
                json!({
                    "accountKey":account_key,
                    "pluginId":plugin_id,
                    "schema":entry.schema,
                    "publicConfig":entry.public_config,
                    "secretKeys":entry.secret_keys,
                    "teamConfigured":entry.team_configured,
                })
            })
            .collect::<Vec<_>>();
        let payload = serde_json::to_vec(&json!({"version":STORE_VERSION,"entries":entries}))
            .map_err(|error| format!("failed to serialize plugin variable store: {error}"))?;
        if payload.len() as u64 > MAX_STORE_BYTES {
            return Err("plugin variable store exceeds bounded size".into());
        }
        let temporary = self.path.with_extension("tmp");
        let write_result = (|| -> Result<(), String> {
            let mut file = File::create(&temporary)
                .map_err(|error| format!("failed to create plugin variable store: {error}"))?;
            file.write_all(&payload)
                .map_err(|error| format!("failed to write plugin variable store: {error}"))?;
            file.sync_all()
                .map_err(|error| format!("failed to sync plugin variable store: {error}"))?;
            fs::rename(&temporary, &self.path)
                .map_err(|error| format!("failed to commit plugin variable store: {error}"))
        })();
        let _ = fs::remove_file(&temporary);
        write_result
    }
}

fn parse_store(raw: &str) -> Result<BTreeMap<(String, String), PluginVariableEntry>, String> {
    let value: Value = serde_json::from_str(raw).map_err(|_| "invalid plugin variable store JSON")?;
    if value.get("version").and_then(Value::as_u64) != Some(STORE_VERSION) {
        return Err("unsupported plugin variable store version".into());
    }
    let rows = value
        .get("entries")
        .and_then(Value::as_array)
        .ok_or("plugin variable store entries are missing")?;
    let mut entries = BTreeMap::new();
    for row in rows {
        let account_key = validate_identity(
            row.get("accountKey").and_then(Value::as_str).unwrap_or(""),
            "account key",
        )?;
        let plugin_id = validate_identity(
            row.get("pluginId").and_then(Value::as_str).unwrap_or(""),
            "plugin id",
        )?;
        let schema = row.get("schema").cloned().unwrap_or_else(|| json!({}));
        let public_config = string_map(row.get("publicConfig"))?;
        let secret_keys = row
            .get("secretKeys")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        entries.insert(
            (account_key, plugin_id),
            PluginVariableEntry {
                schema,
                public_config,
                secret_keys,
                team_configured: row
                    .get("teamConfigured")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            },
        );
    }
    Ok(entries)
}

fn string_map(value: Option<&Value>) -> Result<BTreeMap<String, String>, String> {
    let Some(object) = value.and_then(Value::as_object) else {
        return Ok(BTreeMap::new());
    };
    object
        .iter()
        .map(|(key, value)| {
            let value = value
                .as_str()
                .ok_or_else(|| "plugin variable store contains a non-string config value".to_string())?;
            Ok((key.clone(), value.to_string()))
        })
        .collect()
}

fn validate_identity(raw: &str, label: &str) -> Result<String, String> {
    let value = raw.trim();
    if value.is_empty()
        || value.len() > 512
        || value.chars().any(char::is_control)
        || value.contains('\0')
    {
        return Err(format!("plugin variable {label} is invalid"));
    }
    Ok(value.to_string())
}

pub fn variable_fields_json(fields: &[PluginVariableField]) -> Value {
    Value::Array(
        fields
            .iter()
            .map(|field| {
                let mut value = Map::new();
                value.insert("key".into(), Value::String(field.key.clone()));
                value.insert("label".into(), Value::String(field.label.clone()));
                value.insert("placeholder".into(), Value::String(field.placeholder.clone()));
                value.insert("isRequired".into(), Value::Bool(field.is_required));
                value.insert("isSecret".into(), Value::Bool(field.is_secret));
                if let Some(default_value) = &field.default_value {
                    value.insert("defaultValue".into(), Value::String(default_value.clone()));
                }
                if let Some(hint) = &field.hint {
                    value.insert("hint".into(), Value::String(hint.clone()));
                }
                Value::Object(value)
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> Value {
        json!({
            "properties":{
                "endpoint":{"default":"https://example.test"},
                "username":{},
                "ACCESS_TOKEN":{"format":"password"}
            },
            "required":["username","ACCESS_TOKEN"]
        })
    }

    #[test]
    fn required_defaults_and_secret_values_are_split_before_persistence() {
        let path = std::env::temp_dir().join(format!(
            "fabushi-plugin-vars-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let mut store = PluginVariableStore::open(&path).unwrap();
        assert!(store
            .prepare_write("account-a", "plugin-a", &schema(), &json!({"username":"alice"}), false)
            .is_err());
        let prepared = store
            .prepare_write(
                "account-a",
                "plugin-a",
                &schema(),
                &json!({"username":"alice","ACCESS_TOKEN":"super-secret"}),
                false,
            )
            .unwrap();
        assert_eq!(prepared.public_config.get("endpoint").map(String::as_str), Some("https://example.test"));
        assert_eq!(prepared.public_config.get("username").map(String::as_str), Some("alice"));
        assert!(!prepared.public_config.contains_key("ACCESS_TOKEN"));
        assert_eq!(prepared.secret_values.get("ACCESS_TOKEN").map(String::as_str), Some("super-secret"));
        store.commit_write(&prepared).unwrap();

        let raw = fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("super-secret"));
        assert!(raw.contains("ACCESS_TOKEN"));

        let reopened = PluginVariableStore::open(&path).unwrap();
        let config = reopened.runtime_config("account-a", "plugin-a").unwrap();
        assert_eq!(config.public_config.get("username").map(String::as_str), Some("alice"));
        assert!(config.secret_keys.contains("ACCESS_TOKEN"));
        assert!(reopened.runtime_config("account-b", "plugin-a").is_err());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn update_and_reinstall_keep_account_scoped_config_until_replaced() {
        let path = std::env::temp_dir().join(format!(
            "fabushi-plugin-vars-update-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let mut store = PluginVariableStore::open(&path).unwrap();
        let first = store
            .prepare_write(
                "account-a",
                "plugin-a",
                &schema(),
                &json!({"username":"alice","ACCESS_TOKEN":"one"}),
                false,
            )
            .unwrap();
        store.commit_write(&first).unwrap();
        let second = store
            .prepare_write(
                "account-a",
                "plugin-a",
                &schema(),
                &json!({"username":"bob","ACCESS_TOKEN":"two"}),
                false,
            )
            .unwrap();
        store.commit_write(&second).unwrap();
        let reopened = PluginVariableStore::open(&path).unwrap();
        assert_eq!(
            reopened
                .runtime_config("account-a", "plugin-a")
                .unwrap()
                .public_config
                .get("username")
                .map(String::as_str),
            Some("bob")
        );
        let _ = fs::remove_file(path);
    }
}
