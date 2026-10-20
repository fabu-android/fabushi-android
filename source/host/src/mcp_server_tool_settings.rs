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
const MAX_INSTRUCTIONS_UTF16_UNITS: usize = 500;

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
        let normalized = clamp_instructions(instructions)?;
        let mut next = self.state.clone();
        // An explicit empty value is canonical state: Desktop uses it to suppress a connector's
        // built-in default instructions. Do not collapse Some("") into the unset/None state.
        next.instructions
            .entry(account_fence.to_string())
            .or_default()
            .insert(server_id.to_string(), normalized);
        persist(&self.path, &next)?;
        self.state = next;
        self.custom_instructions_for(account_fence, server_id)
    }

    pub fn resolved_custom_instructions(
        &self,
        account_fence: &str,
        server_id: &str,
        server_name: &str,
    ) -> Result<String, String> {
        let stored = self.custom_instructions_for(account_fence, server_id)?;
        Ok(resolve_mcp_custom_instruction(server_name, stored.as_deref()))
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

    pub fn clear_account(&mut self, account_fence: &str) -> Result<(), String> {
        validate_id(account_fence, "account fence")?;
        let mut next = self.state.clone();
        next.accounts.remove(account_fence);
        next.instructions.remove(account_fence);
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

const DEFAULT_HEX_MCP_CUSTOM_INSTRUCTION: &str =
    "When using Hex, get the underlying numbers as data: download/export the results as CSV or use the data the connector returns, and analyze those raw values directly. Don't read rendered charts or graphs from screenshots (computer-use chart reading is unreliable) — work from the actual data.";

pub fn resolve_mcp_custom_instruction(server_name: &str, stored: Option<&str>) -> String {
    match stored {
        Some(value) => value.trim().to_string(),
        None if server_name.trim().eq_ignore_ascii_case("hex") => {
            DEFAULT_HEX_MCP_CUSTOM_INSTRUCTION.to_string()
        }
        None => String::new(),
    }
}

pub fn format_mcp_custom_instruction_tool_note(server_name: &str, instructions: &str) -> String {
    format!(
        "Custom instructions for the \"{}\" connector (always follow them when using this tool):\n{}",
        server_name,
        instructions.trim()
    )
}

pub fn build_mcp_custom_instructions_system_prompt_section(
    entries: &[(String, String)],
) -> Option<String> {
    let mut unique = BTreeMap::<String, String>::new();
    for (name, instructions) in entries {
        let instructions = instructions.trim();
        if !instructions.is_empty() {
            unique.entry(name.clone()).or_insert_with(|| instructions.to_string());
        }
    }
    if unique.is_empty() {
        return None;
    }
    let mut lines = vec![
        "## Connector custom instructions".to_string(),
        "Custom instructions are configured for some connected tools (MCP connectors). Always follow the matching instruction whenever you use that connector's tools, even before your first call to it:".to_string(),
    ];
    lines.extend(
        unique
            .into_iter()
            .map(|(name, instructions)| format!("- {name}: {instructions}")),
    );
    Some(lines.join("\n"))
}

fn clamp_instructions(raw: &str) -> Result<String, String> {
    if raw.contains('\0') {
        return Err("MCP custom instructions are invalid".into());
    }
    // Desktop clamps at 500 JavaScript UTF-16 code units. Rust strings cannot contain isolated
    // surrogate halves, so preserve complete Unicode scalars while never exceeding that bound.
    let mut used = 0usize;
    let mut out = String::new();
    for ch in raw.chars() {
        let units = ch.len_utf16();
        if used + units > MAX_INSTRUCTIONS_UTF16_UNITS {
            break;
        }
        used += units;
        out.push(ch);
    }
    Ok(out)
}

fn validate_instructions(raw: &str) -> Result<(), String> {
    if raw.contains('\0') || raw.encode_utf16().count() > MAX_INSTRUCTIONS_UTF16_UNITS {
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
    fn account_departure_clears_disabled_tools_and_custom_instructions_only_for_departed_account() {
        let path = path("account-departure");
        let mut store = McpServerToolSettings::open(&path).unwrap();
        store.toggle("account:a", "17", "calendar.list").unwrap();
        store.set_custom_instructions("account:a", "17", "Read only.").unwrap();
        store.toggle("account:b", "17", "drive.read").unwrap();
        store.set_custom_instructions("account:b", "17", "Keep links.").unwrap();

        store.clear_account("account:a").unwrap();
        assert!(store.disabled_for("account:a", "17").unwrap().is_empty());
        assert_eq!(store.custom_instructions_for("account:a", "17").unwrap(), None);
        assert!(store.disabled_for("account:b", "17").unwrap().contains("drive.read"));
        assert_eq!(
            store.custom_instructions_for("account:b", "17").unwrap().as_deref(),
            Some("Keep links.")
        );

        drop(store);
        let reopened = McpServerToolSettings::open(&path).unwrap();
        assert!(reopened.disabled_for("account:a", "17").unwrap().is_empty());
        assert_eq!(
            reopened.custom_instructions_for("account:b", "17").unwrap().as_deref(),
            Some("Keep links.")
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn custom_instructions_clamp_to_desktop_limit_and_preserve_explicit_empty() {
        let path = path("instructions-clamp");
        let mut store = McpServerToolSettings::open(&path).unwrap();
        let long = "x".repeat(MAX_INSTRUCTIONS_UTF16_UNITS + 25);
        assert_eq!(
            store
                .set_custom_instructions("account:a", "17", &long)
                .unwrap()
                .as_deref()
                .map(str::len),
            Some(MAX_INSTRUCTIONS_UTF16_UNITS)
        );
        assert_eq!(
            store
                .set_custom_instructions("account:a", "17", "")
                .unwrap(),
            Some(String::new())
        );
        drop(store);
        let reopened = McpServerToolSettings::open(&path).unwrap();
        assert_eq!(
            reopened.custom_instructions_for("account:a", "17").unwrap(),
            Some(String::new())
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn custom_instructions_reject_nul_and_clamp_without_splitting_unicode_scalar() {
        let path = path("instructions-invalid");
        let mut store = McpServerToolSettings::open(&path).unwrap();
        assert!(store
            .set_custom_instructions("account:a", "17", "bad\0value")
            .is_err());
        let emoji = "a".repeat(499) + "😀";
        let saved = store
            .set_custom_instructions("account:a", "17", &emoji)
            .unwrap()
            .unwrap();
        assert_eq!(saved.encode_utf16().count(), 499);
        assert!(saved.ends_with('a'));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn connector_defaults_and_explicit_empty_match_desktop_resolution() {
        let path = path("instructions-default");
        let mut store = McpServerToolSettings::open(&path).unwrap();
        let default = store
            .resolved_custom_instructions("account:a", "17", " Hex ")
            .unwrap();
        assert!(default.contains("underlying numbers as data"));
        assert_eq!(
            store
                .set_custom_instructions("account:a", "17", "")
                .unwrap(),
            Some(String::new())
        );
        assert_eq!(
            store
                .resolved_custom_instructions("account:a", "17", "Hex")
                .unwrap(),
            ""
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn connector_instruction_prompt_helpers_are_deterministic() {
        let note = format_mcp_custom_instruction_tool_note("Calendar", "  Read only.  ");
        assert_eq!(
            note,
            "Custom instructions for the \"Calendar\" connector (always follow them when using this tool):\nRead only."
        );
        let section = build_mcp_custom_instructions_system_prompt_section(&[
            ("Drive".into(), "Use source links.".into()),
            ("Calendar".into(), "Read only.".into()),
            ("Drive".into(), "ignored duplicate".into()),
            ("Empty".into(), "   ".into()),
        ])
        .unwrap();
        assert!(section.starts_with("## Connector custom instructions\n"));
        assert!(section.contains("- Calendar: Read only."));
        assert!(section.contains("- Drive: Use source links."));
        assert!(!section.contains("ignored duplicate"));
        assert!(!section.contains("- Empty:"));
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
