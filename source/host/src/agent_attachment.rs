use crate::runner::AndroidRoutedToolBridge;
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use fabushi_android_shared::media::attachment_limits::attachment_byte_limit_for_name;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

const SCHEMA_VERSION: u8 = 1;
pub const COMPOSER_ATTACHMENT_LIMIT: usize = 6;
const TOOL_READ_LIMIT_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentAttachment {
    pub id: String,
    pub path: String,
    pub name: String,
    #[serde(rename = "size")]
    pub size_bytes: u64,
    pub mime_type: Option<String>,
}

impl AgentAttachment {
    pub fn list_from_json(value: Option<&Value>) -> Result<Vec<Self>, String> {
        let Some(value) = value else { return Ok(Vec::new()); };
        let rows = value
            .as_array()
            .ok_or("agent attachments must be an array")?;
        if rows.len() > COMPOSER_ATTACHMENT_LIMIT {
            return Err("too many Agent attachments".into());
        }
        rows.iter()
            .map(|row| {
                let attachment: Self = serde_json::from_value(row.clone())
                    .map_err(|error| format!("invalid Agent attachment: {error}"))?;
                attachment.validate_public()?;
                Ok(attachment)
            })
            .collect()
    }

    pub fn validate_public(&self) -> Result<(), String> {
        validate_identity(&self.id, "attachment id")?;
        validate_identity(&self.name, "attachment name")?;
        if self.path != format!("fabushi-attachment://{}", self.id) {
            return Err("attachment path is not a canonical Fabushi attachment identity".into());
        }
        if self.size_bytes > attachment_byte_limit_for_name(&self.name) {
            return Err("attachment exceeds the canonical Android byte limit".into());
        }
        if let Some(mime) = self.mime_type.as_deref() {
            if mime.len() > 255 || mime.chars().any(char::is_control) {
                return Err("attachment MIME type is invalid".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StagedAttachment {
    account_slot: Option<String>,
    account_fence: String,
    agent_id: String,
    attachment_id: String,
    name: String,
    mime_type: Option<String>,
    declared_size_bytes: Option<u64>,
    received_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CompletedAttachment {
    account_slot: Option<String>,
    account_fence: String,
    agent_id: String,
    attachment: AgentAttachment,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AttachmentState {
    schema_version: u8,
    next_id: u64,
    staged: BTreeMap<String, StagedAttachment>,
    completed: BTreeMap<String, CompletedAttachment>,
}

impl Default for AttachmentState {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            next_id: 0,
            staged: BTreeMap::new(),
            completed: BTreeMap::new(),
        }
    }
}

pub struct DurableAgentAttachmentOwner {
    root: PathBuf,
    state_path: PathBuf,
    state: AttachmentState,
}

impl DurableAgentAttachmentOwner {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, String> {
        let root = root.into();
        fs::create_dir_all(root.join("staging")).map_err(|error| error.to_string())?;
        fs::create_dir_all(root.join("files")).map_err(|error| error.to_string())?;
        let state_path = root.join("state.json");
        let mut state = match fs::read(&state_path) {
            Ok(raw) => serde_json::from_slice::<AttachmentState>(&raw)
                .map_err(|error| format!("invalid Agent attachment state: {error}"))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => AttachmentState::default(),
            Err(error) => return Err(error.to_string()),
        };
        if state.schema_version != SCHEMA_VERSION {
            return Err("unsupported Agent attachment state schema".into());
        }
        // Incomplete ContentResolver streams are never resumed blindly after process death.
        for id in state.staged.keys() {
            let _ = fs::remove_file(root.join("staging").join(format!("{id}.part")));
        }
        state.staged.clear();
        let owner = Self { root, state_path, state };
        owner.persist()?;
        Ok(owner)
    }

    pub fn completed_root(&self) -> PathBuf {
        self.root.join("files")
    }

    pub fn begin(
        &mut self,
        account_slot: Option<&str>,
        account_fence: &str,
        agent_id: &str,
        name: &str,
        mime_type: Option<&str>,
        declared_size_bytes: Option<u64>,
    ) -> Result<String, String> {
        validate_scope(account_slot, account_fence, agent_id)?;
        validate_identity(name, "attachment name")?;
        let limit = attachment_byte_limit_for_name(name);
        if declared_size_bytes.is_some_and(|size| size > limit) {
            return Err("attachment exceeds the canonical Android byte limit".into());
        }
        if let Some(mime) = mime_type {
            if mime.len() > 255 || mime.chars().any(char::is_control) {
                return Err("attachment MIME type is invalid".into());
            }
        }
        self.state.next_id = self.state.next_id.saturating_add(1).max(1);
        let id = format!("agent-attachment-{:016x}", self.state.next_id);
        let staged = StagedAttachment {
            account_slot: account_slot.map(str::to_string),
            account_fence: account_fence.to_string(),
            agent_id: agent_id.to_string(),
            attachment_id: id.clone(),
            name: name.to_string(),
            mime_type: mime_type.map(str::to_string),
            declared_size_bytes,
            received_bytes: 0,
        };
        fs::write(self.staging_path(&id), []).map_err(|error| error.to_string())?;
        self.state.staged.insert(id.clone(), staged);
        self.persist()?;
        Ok(id)
    }

    pub fn append(
        &mut self,
        account_slot: Option<&str>,
        account_fence: &str,
        agent_id: &str,
        attachment_id: &str,
        offset: u64,
        bytes: &[u8],
    ) -> Result<u64, String> {
        validate_scope(account_slot, account_fence, agent_id)?;
        let staged = self
            .state
            .staged
            .get(attachment_id)
            .cloned()
            .ok_or("Agent attachment stage is missing")?;
        assert_stage_scope(&staged, account_slot, account_fence, agent_id)?;
        let limit = attachment_byte_limit_for_name(&staged.name);
        let end = offset
            .checked_add(bytes.len() as u64)
            .ok_or("Agent attachment offset overflow")?;
        if end > limit || staged.declared_size_bytes.is_some_and(|size| end > size) {
            return Err("Agent attachment chunk exceeds declared or canonical size".into());
        }
        let path = self.staging_path(attachment_id);
        if offset < staged.received_bytes {
            let mut file = fs::File::open(&path).map_err(|error| error.to_string())?;
            file.seek(SeekFrom::Start(offset)).map_err(|error| error.to_string())?;
            let mut existing = vec![0u8; bytes.len()];
            file.read_exact(&mut existing).map_err(|error| error.to_string())?;
            if existing == bytes {
                return Ok(staged.received_bytes);
            }
            return Err("Agent attachment replay bytes differ at an already committed offset".into());
        }
        if offset != staged.received_bytes {
            return Err("Agent attachment chunk offset is not contiguous".into());
        }
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .map_err(|error| error.to_string())?;
        file.write_all(bytes).map_err(|error| error.to_string())?;
        let record = self.state.staged.get_mut(attachment_id).expect("stage retained");
        record.received_bytes = end;
        self.persist()?;
        Ok(end)
    }

    pub fn finish(
        &mut self,
        account_slot: Option<&str>,
        account_fence: &str,
        agent_id: &str,
        attachment_id: &str,
    ) -> Result<AgentAttachment, String> {
        validate_scope(account_slot, account_fence, agent_id)?;
        let staged = self
            .state
            .staged
            .get(attachment_id)
            .cloned()
            .ok_or("Agent attachment stage is missing")?;
        assert_stage_scope(&staged, account_slot, account_fence, agent_id)?;
        if staged.declared_size_bytes.is_some_and(|size| size != staged.received_bytes) {
            return Err("Agent attachment upload is incomplete".into());
        }
        let attachment = AgentAttachment {
            id: staged.attachment_id.clone(),
            path: format!("fabushi-attachment://{}", staged.attachment_id),
            name: staged.name.clone(),
            size_bytes: staged.received_bytes,
            mime_type: staged.mime_type.clone(),
        };
        attachment.validate_public()?;
        let completed_path = self.completed_path(attachment_id);
        fs::rename(self.staging_path(attachment_id), &completed_path)
            .map_err(|error| error.to_string())?;
        self.state.staged.remove(attachment_id);
        self.state.completed.insert(
            attachment_id.to_string(),
            CompletedAttachment {
                account_slot: account_slot.map(str::to_string),
                account_fence: account_fence.to_string(),
                agent_id: agent_id.to_string(),
                attachment: attachment.clone(),
            },
        );
        self.persist()?;
        Ok(attachment)
    }

    pub fn abort(
        &mut self,
        account_slot: Option<&str>,
        account_fence: &str,
        agent_id: &str,
        attachment_id: &str,
    ) -> Result<bool, String> {
        let Some(staged) = self.state.staged.get(attachment_id).cloned() else {
            return Ok(false);
        };
        assert_stage_scope(&staged, account_slot, account_fence, agent_id)?;
        self.state.staged.remove(attachment_id);
        let _ = fs::remove_file(self.staging_path(attachment_id));
        self.persist()?;
        Ok(true)
    }

    pub fn remove_completed(
        &mut self,
        account_slot: Option<&str>,
        account_fence: &str,
        agent_id: &str,
        attachment_id: &str,
    ) -> Result<bool, String> {
        let Some(record) = self.state.completed.get(attachment_id).cloned() else {
            return Ok(false);
        };
        if record.account_slot.as_deref() != account_slot
            || record.account_fence != account_fence
            || record.agent_id != agent_id
        {
            return Err("Agent attachment is fenced to another scope".into());
        }
        self.state.completed.remove(attachment_id);
        let _ = fs::remove_file(self.completed_path(attachment_id));
        self.persist()?;
        Ok(true)
    }

    pub fn validate_completed(
        &self,
        account_slot: Option<&str>,
        account_fence: &str,
        agent_id: &str,
        attachments: &[AgentAttachment],
    ) -> Result<(), String> {
        if attachments.len() > COMPOSER_ATTACHMENT_LIMIT {
            return Err("too many Agent attachments".into());
        }
        for attachment in attachments {
            attachment.validate_public()?;
            let record = self
                .state
                .completed
                .get(&attachment.id)
                .ok_or("Agent attachment is not durably staged")?;
            if record.account_slot.as_deref() != account_slot
                || record.account_fence != account_fence
                || record.agent_id != agent_id
                || record.attachment != *attachment
            {
                return Err("Agent attachment metadata or scope does not match the durable owner".into());
            }
            let metadata = fs::metadata(self.completed_path(&attachment.id))
                .map_err(|error| error.to_string())?;
            if metadata.len() != attachment.size_bytes {
                return Err("Agent attachment file length no longer matches durable metadata".into());
            }
        }
        Ok(())
    }

    fn staging_path(&self, id: &str) -> PathBuf {
        self.root.join("staging").join(format!("{id}.part"))
    }

    fn completed_path(&self, id: &str) -> PathBuf {
        self.completed_root().join(format!("{id}.bin"))
    }

    fn persist(&self) -> Result<(), String> {
        let tmp = self.state_path.with_extension("json.tmp");
        fs::write(
            &tmp,
            serde_json::to_vec_pretty(&self.state).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        fs::rename(tmp, &self.state_path).map_err(|error| error.to_string())
    }
}

pub fn prompt_with_agent_attachments(prompt: &str, attachments: &[AgentAttachment]) -> String {
    if attachments.is_empty() {
        return prompt.to_string();
    }
    let mut projected = String::new();
    if !prompt.trim().is_empty() {
        projected.push_str(prompt);
        projected.push_str("\n\n");
    }
    projected.push_str("<fabushi_attachments>\n");
    projected.push_str("The user attached the following files. Read their actual bytes with the FabushiAttachmentRead tool when needed; never infer contents from filenames.\n");
    for attachment in attachments {
        projected.push_str(&format!(
            "- id={} name={} mime={} size={}\n",
            attachment.id,
            attachment.name,
            attachment.mime_type.as_deref().unwrap_or("application/octet-stream"),
            attachment.size_bytes,
        ));
    }
    projected.push_str("</fabushi_attachments>");
    projected
}

pub fn with_agent_attachment_tools(
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    completed_root: PathBuf,
    attachments: Vec<AgentAttachment>,
) -> Arc<dyn AndroidRoutedToolBridge> {
    Arc::new(AgentAttachmentTools {
        delegate,
        completed_root,
        attachments: attachments
            .into_iter()
            .map(|attachment| (attachment.id.clone(), attachment))
            .collect(),
    })
}

struct AgentAttachmentTools {
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    completed_root: PathBuf,
    attachments: BTreeMap<String, AgentAttachment>,
}

impl AndroidRoutedToolBridge for AgentAttachmentTools {
    fn list_tools(&self) -> Result<Vec<Value>, String> {
        let mut tools = self.delegate.list_tools()?;
        if !self.attachments.is_empty() {
            tools.push(json!({
                "type":"function",
                "name":"FabushiAttachmentRead",
                "description":"Read a bounded byte range from a file the user explicitly attached to this Agent turn.",
                "parameters":{
                    "type":"object",
                    "additionalProperties":false,
                    "required":["attachmentId"],
                    "properties":{
                        "attachmentId":{"type":"string"},
                        "offset":{"type":"integer","minimum":0},
                        "limit":{"type":"integer","minimum":1,"maximum":TOOL_READ_LIMIT_BYTES}
                    }
                }
            }));
        }
        Ok(tools)
    }

    fn call_tool(&self, name: &str, args: Value, tool_call_id: &str) -> Result<Value, String> {
        if name != "FabushiAttachmentRead" {
            return self.delegate.call_tool(name, args, tool_call_id);
        }
        if tool_call_id.trim().is_empty() {
            return Err("attachment read tool call id is required".into());
        }
        let attachment_id = args
            .get("attachmentId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or("attachmentId is required")?;
        let attachment = self
            .attachments
            .get(attachment_id)
            .ok_or("attachment is not granted to this Agent turn")?;
        let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0);
        let limit = args
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(TOOL_READ_LIMIT_BYTES)
            .clamp(1, TOOL_READ_LIMIT_BYTES);
        if offset > attachment.size_bytes {
            return Err("attachment read offset exceeds file length".into());
        }
        let remaining = attachment.size_bytes.saturating_sub(offset);
        let read_len = remaining.min(limit) as usize;
        let mut file = fs::File::open(
            self.completed_root.join(format!("{}.bin", attachment.id)),
        )
        .map_err(|error| error.to_string())?;
        file.seek(SeekFrom::Start(offset)).map_err(|error| error.to_string())?;
        let mut bytes = vec![0u8; read_len];
        file.read_exact(&mut bytes).map_err(|error| error.to_string())?;
        let next_offset = offset + bytes.len() as u64;
        let textual = attachment
            .mime_type
            .as_deref()
            .is_some_and(|mime| mime.starts_with("text/") || mime.contains("json") || mime.contains("xml"))
            || matches!(
                attachment.name.rsplit('.').next().unwrap_or_default().to_ascii_lowercase().as_str(),
                "txt" | "md" | "json" | "csv" | "tsv" | "xml" | "yaml" | "yml" | "log"
            );
        let mut result = json!({
            "attachmentId":attachment.id,
            "name":attachment.name,
            "mimeType":attachment.mime_type,
            "offset":offset,
            "nextOffset":next_offset,
            "size":attachment.size_bytes,
            "eof":next_offset >= attachment.size_bytes,
        });
        if textual {
            if let Ok(text) = String::from_utf8(bytes.clone()) {
                result["text"] = Value::String(text);
                return Ok(result);
            }
        }
        result["dataBase64"] = Value::String(BASE64_STANDARD.encode(bytes));
        Ok(result)
    }
}

fn assert_stage_scope(
    staged: &StagedAttachment,
    account_slot: Option<&str>,
    account_fence: &str,
    agent_id: &str,
) -> Result<(), String> {
    if staged.account_slot.as_deref() != account_slot
        || staged.account_fence != account_fence
        || staged.agent_id != agent_id
    {
        return Err("Agent attachment stage is fenced to another scope".into());
    }
    Ok(())
}

fn validate_scope(
    account_slot: Option<&str>,
    account_fence: &str,
    agent_id: &str,
) -> Result<(), String> {
    if let Some(slot) = account_slot {
        validate_identity(slot, "account slot")?;
    }
    validate_identity(account_fence, "account fence")?;
    validate_identity(agent_id, "agent id")
}

fn validate_identity(value: &str, label: &str) -> Result<(), String> {
    if value.trim().is_empty()
        || value.len() > 1024
        || value.chars().any(|ch| ch == '\0' || ch == '\r' || ch == '\n')
    {
        return Err(format!("{label} is invalid"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EmptyTools;
    impl AndroidRoutedToolBridge for EmptyTools {
        fn list_tools(&self) -> Result<Vec<Value>, String> { Ok(Vec::new()) }
        fn call_tool(&self, _name: &str, _args: Value, _tool_call_id: &str) -> Result<Value, String> {
            Err("unknown tool".into())
        }
    }

    #[test]
    fn staged_bytes_are_scoped_durable_and_turn_read_is_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("attachments");
        let attachment = {
            let mut owner = DurableAgentAttachmentOwner::open(&root).unwrap();
            let id = owner.begin(Some("slot-a"), "session:a", "agent-a", "notes.txt", Some("text/plain"), Some(5)).unwrap();
            assert_eq!(owner.append(Some("slot-a"), "session:a", "agent-a", &id, 0, b"hello").unwrap(), 5);
            let attachment = owner.finish(Some("slot-a"), "session:a", "agent-a", &id).unwrap();
            owner.validate_completed(Some("slot-a"), "session:a", "agent-a", std::slice::from_ref(&attachment)).unwrap();
            attachment
        };
        let reopened = DurableAgentAttachmentOwner::open(&root).unwrap();
        reopened.validate_completed(Some("slot-a"), "session:a", "agent-a", std::slice::from_ref(&attachment)).unwrap();
        assert!(reopened.validate_completed(Some("slot-b"), "session:a", "agent-a", std::slice::from_ref(&attachment)).is_err());
        let tools = with_agent_attachment_tools(
            Arc::new(EmptyTools),
            reopened.completed_root(),
            vec![attachment.clone()],
        );
        let value = tools.call_tool("FabushiAttachmentRead", json!({"attachmentId":attachment.id,"offset":1,"limit":3}), "tool-1").unwrap();
        assert_eq!(value["text"], "ell");
        assert_eq!(value["eof"], false);
    }

    #[test]
    fn process_reopen_discards_partial_stage_instead_of_guessing_completion() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("attachments");
        let id = {
            let mut owner = DurableAgentAttachmentOwner::open(&root).unwrap();
            let id = owner.begin(None, "session:a", "agent-a", "a.txt", None, None).unwrap();
            owner.append(None, "session:a", "agent-a", &id, 0, b"partial").unwrap();
            id
        };
        let mut reopened = DurableAgentAttachmentOwner::open(&root).unwrap();
        assert!(!reopened.abort(None, "session:a", "agent-a", &id).unwrap());
    }
}
