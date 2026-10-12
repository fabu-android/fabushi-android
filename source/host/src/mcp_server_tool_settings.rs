use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

const STORE_VERSION: u8 = 1;
const MAX_STORE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_ID_BYTES: usize = 512;
const MAX_TOOLS_PER_SERVER: usize = 2_048;
const MAX_INSTRUCTIONS_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedStore {
    version: u8,
    #[serde(default)]
    accounts: BTreeMap<String, BTreeMap<String, BTreeSet<String>>>,
    #[serde(default)]
    instructions: BTreeMap<String, BTreeMap<String, String>>,
}

pub struct McpServerToolSettings {
    path: PathBuf,
    state: PersistedStore,
}

impl McpServerToolSettings {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, String> {
        let path = path.into();
        let state = match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() && metadata.len() > 0 && metadata.len() <= MAX_STORE_BYTES => {
                let raw = fs::read_to_string(&path)
                    .map_err(|error| format!("failed to read MCP tool settings: {error}"))?;
                let state: PersistedStore = serde_json::from_str(&raw)
                    .map_err(|_| "MCP tool settings are corrupt".to_string())?;
                if state.version != STORE_VERSION {
                    return Err("unsupported MCP tool settings version".into());
                }
                validate_state(&state)?;
                state
            }
            Ok(_) => return Err("MCP tool settings file violates bounded storage contract".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => PersistedStore {
                version: STORE_VERSION,
                accounts: BTreeMap::new(),
                instructions: BTreeMap::new(),
            },
            Err(error) => return Err(format!("failed to inspect MCP tool settings: {error}")),
        };
        Ok(Self { path, state })
    }

    pub fn disabled_for(
        &self,
        account_fence: &str,
        server_id: &str,
    ) -> Result<BTreeSet<String>, String> {
        validate_id(account_fence, "account fence")?;
        validate_id(server_id, "server id")?;
        Ok(self
            .state
            .accounts
            .get(account_fence)
            .and_then(|servers| servers.get(server_id))
            .cloned()
            .unwrap_or_default())
    }

    pub fn toggle(
        &mut self,
        account_fence: &str,
        server_id: &str,
        tool_name: &str,
    ) -> Result<BTreeSet<String>, String> {
        validate_id(account_fence, "account fence")?;
        validate_id(server_id, "server id")?;
        validate_id(tool_name, "tool name")?;
        let mut next = self.state.clone();
        let servers = next.accounts.entry(account_fence.to_string()).or_default();
        let disabled = servers.entry(server_id.to_string()).or_default();
        if !disabled.insert(tool_name.to_string()) {
            disabled.remove(tool_name);
        }
        if disabled.len() > MAX_TOOLS_PER_SERVER {
            return Err("MCP disabled tool set exceeds bounded size".into());
        }
        if disabled.is_empty() {
            servers.remove(server_id);
        }
        if servers.is_empty() {
            next.accounts.remove(account_fence);
        }
        persist(&self.path, &next)?;
        self.state = next;
        self.disabled_for(account_fence, server_id)
    }

    pub fn custom_instructions_for(
        &self,
        account_fence: &str,
        server_id: &str,
    ) -> Result<Option<String>, String> {
        validate_id(account_fence, "account fence")?;
        validate_id(server_id, "server id")?;
        Ok(self
            .state
            .instructions
            .get(account_fence)
            .and_then(|servers| servers.get(server_id))
            .cloned())
    }

    pub fn set_custom_instructions(
        &mut self,
        account_fence: &str,
        server_id: &str,
        instructions: &str,
    ) -> Result<Option<String>, String> {
        validate_id(account_fence, "account fence")?;
        validate_id(server_id, "server id")?;
        validate_instructions(instructions)?;
        let normalized = instructions.trim().to_string();
        let mut next = self.state.clone();
        if normalized.is_empty() {
            if let Some(servers) = next.instructions.get_mut(account_fence) {
                servers.remove(server_id);
                if servers.is_empty() {
                    next.instructions.remove(account_fence);
                }
            }
        } else {
            next.instructions
                .entry(account_fence.to_string())
                .or_default()
                .insert(server_id.to_string(), normalized.clone());
        }
        persist(&self.path, &next)?;
        self.state = next;
        self.custom_instructions_for(account_fence, server_id)
    }

    pub fn clear_server(&mut self, account_fence: &str, server_id: &str) -> Result<(), String> {
        validate_id(account_fence, "account fence")?;
        validate_id(server_id, "server id")?;
        let mut next = self.state.clone();
        if let Some(servers) = next.accounts.get_mut(account_fence) {
            servers.remove(server_id);
            if servers.is_empty() {
                next.accounts.remove(account_fence);
            }
        }
        if let Some(servers) = next.instructions.get_mut(account_fence) {
            servers.remove(server_id);
            if servers.is_empty() {
                next.instructions.remove(account_fence);
            }
        }
        persist(&self.path, &next)?;
        self.state = next;
        Ok(())
    }
}

fn validate_state(state: &PersistedStore) -> Result<(), String> {
    for (account, servers) in &state.accounts {
        validate_id(account, "account fence")?;
        for (server, tools) in servers {
            validate_id(server, "server id")?;
            if tools.len() > MAX_TOOLS_PER_SERVER {
                return Err("MCP disabled tool set exceeds bounded size".into());
            }
            for tool in tools {
                validate_id(tool, "tool name")?;
            }
        }
    }
    for (account, servers) in &state.instructions {
        validate_id(account, "account fence")?;
        for (server, instructions) in servers {
            validate_id(server, "server id")?;
            validate_instructions(instructions)?;
        }
    }
    Ok(())
}

fn validate_instructions(raw: &str) -> Result<(), String> {
    if raw.as_bytes().len() > MAX_INSTRUCTIONS_BYTES || raw.contains('\0') {
        return Err("MCP custom instructions are invalid".into());
    }
    Ok(())
}

fn validate_id(raw: &str, label: &str) -> Result<(), String> {
    let value = raw.trim();
    if value.is_empty()
        || value.len() > MAX_ID_BYTES
        || value.chars().any(char::is_control)
        || value.contains('\0')
    {
        return Err(format!("MCP {label} is invalid"));
    }
    Ok(())
}

fn persist(path: &Path, state: &PersistedStore) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create MCP tool settings directory: {error}"))?;
    }
    let bytes = serde_json::to_vec(state)
        .map_err(|error| format!("failed to serialize MCP tool settings: {error}"))?;
    if bytes.len() as u64 > MAX_STORE_BYTES {
        return Err("MCP tool settings exceed bounded store size".into());
    }
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let result = (|| -> Result<(), String> {
        let mut file = File::create(&temporary)
            .map_err(|error| format!("failed to create MCP tool settings: {error}"))?;
        file.write_all(&bytes)
            .map_err(|error| format!("failed to write MCP tool settings: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("failed to sync MCP tool settings: {error}"))?;
        fs::rename(&temporary, path)
            .map_err(|error| format!("failed to commit MCP tool settings: {error}"))
    })();
    let _ = fs::remove_file(&temporary);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-mcp-tool-settings-{label}-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[test]
    fn toggle_is_account_scoped_and_durable() {
        let path = path("durable");
        let mut store = McpServerToolSettings::open(&path).unwrap();
        assert!(store.toggle("account:a", "17", "calendar.list").unwrap().contains("calendar.list"));
        assert!(store.disabled_for("account:b", "17").unwrap().is_empty());
        drop(store);
        let mut reopened = McpServerToolSettings::open(&path).unwrap();
        assert!(reopened.disabled_for("account:a", "17").unwrap().contains("calendar.list"));
        assert!(reopened.toggle("account:a", "17", "calendar.list").unwrap().is_empty());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn custom_instructions_are_account_scoped_durable_and_cleared_with_server() {
        let path = path("instructions");
        let mut store = McpServerToolSettings::open(&path).unwrap();
        assert_eq!(
            store
                .set_custom_instructions("account:a", "17", "Use read-only calendar tools.")
                .unwrap(),
            Some("Use read-only calendar tools.".into())
        );
        assert_eq!(
            store.custom_instructions_for("account:a", "17").unwrap(),
            Some("Use read-only calendar tools.".into())
        );
        assert_eq!(
            store.custom_instructions_for("account:b", "17").unwrap(),
            None
        );
        drop(store);
        let mut reopened = McpServerToolSettings::open(&path).unwrap();
        assert_eq!(
            reopened.custom_instructions_for("account:a", "17").unwrap(),
            Some("Use read-only calendar tools.".into())
        );
        reopened.clear_server("account:a", "17").unwrap();
        assert_eq!(
            reopened.custom_instructions_for("account:a", "17").unwrap(),
            None
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn custom_instructions_reject_unbounded_or_nul_values() {
        let path = path("instructions-invalid");
        let mut store = McpServerToolSettings::open(&path).unwrap();
        assert!(store
            .set_custom_instructions("account:a", "17", &"x".repeat(MAX_INSTRUCTIONS_BYTES + 1))
            .is_err());
        assert!(store
            .set_custom_instructions("account:a", "17", "bad\0value")
            .is_err());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn corrupt_and_unbounded_state_fail_closed() {
        let path = path("invalid");
        fs::write(&path, b"{not-json").unwrap();
        assert!(McpServerToolSettings::open(&path).is_err());
        let _ = fs::remove_file(&path);

        let mut store = McpServerToolSettings::open(&path).unwrap();
        assert!(store.toggle("", "17", "tool").is_err());
        assert!(store.toggle("account:a", "17", "").is_err());
        assert!(store.toggle("account:a", "17", &"x".repeat(MAX_ID_BYTES + 1)).is_err());
        let _ = fs::remove_file(path);
    }
}
