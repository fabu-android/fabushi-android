use crate::runner::AndroidRoutedToolBridge;
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use fabushi_android_shared::media::attachment_limits::{
    attachment_byte_limit_for_name, ATTACHMENT_BYTE_LIMIT, VIDEO_BYTE_LIMIT,
};
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
pub const MEDIA_READ_CHUNK_LIMIT_BYTES: u64 = 4 * 1024 * 1024;

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
        validate_attachment_name(&self.name)?;
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
        let staging_root = root.join("staging");
        let completed_root = root.join("files");
        fs::create_dir_all(&staging_root).map_err(|error| error.to_string())?;
        fs::create_dir_all(&completed_root).map_err(|error| error.to_string())?;
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

        // An interrupted ContentResolver stream is outcome-unknown, not a completed upload.
        // Never resume it blindly after process death: clear both the durable stage records and
        // every orphan .part file, including a file created before its state update reached disk.
        state.staged.clear();
        for entry in fs::read_dir(&staging_root).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            if entry.file_type().map_err(|error| error.to_string())?.is_file()
                && entry.path().extension().and_then(|value| value.to_str()) == Some("part")
            {
                fs::remove_file(entry.path()).map_err(|error| error.to_string())?;
            }
        }

        // A crash can happen on either side of the staging -> completed rename. Only a completed
        // file that also has matching durable metadata is canonical. Prune stale metadata and
        // delete untracked completed files rather than guessing that an uncertain upload succeeded.
        state.completed.retain(|id, record| {
            fs::metadata(completed_root.join(format!("{id}.bin")))
                .is_ok_and(|metadata| metadata.is_file() && metadata.len() == record.attachment.size_bytes)
        });
        let canonical_completed = state.completed.keys().cloned().collect::<std::collections::BTreeSet<_>>();
        for entry in fs::read_dir(&completed_root).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            if !entry.file_type().map_err(|error| error.to_string())?.is_file() {
                continue;
            }
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else { continue; };
            let Some(id) = name.strip_suffix(".bin") else { continue; };
            if !canonical_completed.contains(id) {
                fs::remove_file(path).map_err(|error| error.to_string())?;
            }
        }

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
        validate_attachment_name(name)?;
        let limit = attachment_byte_limit_for_name(name);
        if declared_size_bytes == Some(0) {
            return Err(format!("\"{name}\" is empty, so it wasn't attached."));
        }
        if declared_size_bytes.is_some_and(|size| size > limit) {
            return Err(stage_too_large_message(name, limit));
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
        if end > limit {
            return Err(stage_too_large_message(&staged.name, limit));
        }
        if staged.declared_size_bytes.is_some_and(|size| end > size) {
            return Err("Agent attachment chunk exceeds the declared size".into());
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
        // Never persist received_bytes ahead of the actual attachment bytes. A process death after
        // this point may discard the stage, but it must not expose bytes that were only in userspace.
        file.sync_data().map_err(|error| error.to_string())?;
        let record = self.state.staged.get_mut(attachment_id).expect("stage retained");
        record.received_bytes = end;
        self.persist()?;
        Ok(end)
    }

    pub fn read_completed_media_chunk(
        &self,
        account_slot: Option<&str>,
        account_fence: &str,
        agent_id: &str,
        attachment_id: &str,
        offset: u64,
        requested_length: u64,
    ) -> Result<(AgentAttachment, Vec<u8>), String> {
        validate_scope(account_slot, account_fence, agent_id)?;
        validate_identity(attachment_id, "attachment id")?;
        if requested_length > MEDIA_READ_CHUNK_LIMIT_BYTES {
            return Err("media attachment read exceeds the four MiB bridge limit".into());
        }
        let completed = self
            .state
            .completed
            .get(attachment_id)
            .ok_or("completed media attachment is missing")?;
        if completed.account_slot.as_deref() != account_slot
            || completed.account_fence != account_fence
            || completed.agent_id != agent_id
        {
            return Err("completed media attachment is fenced to another scope".into());
        }
        let attachment = completed.attachment.clone();
        if offset > attachment.size_bytes {
            return Err("media attachment range starts beyond file length".into());
        }
        if requested_length == 0 {
            return Ok((attachment, Vec::new()));
        }
        if offset == attachment.size_bytes {
            return Err("media attachment range is not satisfiable".into());
        }
        let read_len = attachment
            .size_bytes
            .saturating_sub(offset)
            .min(requested_length) as usize;
        let mut file = fs::File::open(self.completed_path(attachment_id))
            .map_err(|error| error.to_string())?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|error| error.to_string())?;
        let mut bytes = vec![0u8; read_len];
        file.read_exact(&mut bytes).map_err(|error| error.to_string())?;
        Ok((attachment, bytes))
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
        if staged.received_bytes == 0 {
            self.state.staged.remove(attachment_id);
            let _ = fs::remove_file(self.staging_path(attachment_id));
            self.persist()?;
            return Err(format!("\"{}\" is empty, so it wasn't attached.", staged.name));
        }
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
        let encoded =
            serde_json::to_vec_pretty(&self.state).map_err(|error| error.to_string())?;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp)
            .map_err(|error| error.to_string())?;
        file.write_all(&encoded).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        drop(file);
        fs::rename(&tmp, &self.state_path).map_err(|error| error.to_string())?;
        #[cfg(unix)]
        if let Some(parent) = self.state_path.parent() {
            fs::File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|error| error.to_string())?;
        }
        Ok(())
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

fn stage_too_large_message(name: &str, limit: u64) -> String {
    if limit == VIDEO_BYTE_LIMIT {
        format!("\"{name}\" is too large to attach (max 200 MB for video).")
    } else if limit == ATTACHMENT_BYTE_LIMIT {
        format!("\"{name}\" is too large to attach (max 25 MB).")
    } else {
        format!("\"{name}\" is too large to attach.")
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

fn validate_attachment_name(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 255
        || value.contains('/')
        || value.contains('\\')
        || value.contains('\0')
    {
        return Err("attachment name is invalid".into());
    }
    Ok(())
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
    fn media_range_read_is_bounded_scoped_and_reopen_safe() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("attachments");
        let attachment = {
            let mut owner = DurableAgentAttachmentOwner::open(&root).unwrap();
            let id = owner
                .begin(Some("slot-a"), "session:a", "agent-a", "voice.m4a", Some("audio/mp4"), Some(8))
                .unwrap();
            owner
                .append(Some("slot-a"), "session:a", "agent-a", &id, 0, b"abcdefgh")
                .unwrap();
            owner.finish(Some("slot-a"), "session:a", "agent-a", &id).unwrap()
        };
        let reopened = DurableAgentAttachmentOwner::open(&root).unwrap();
        let (meta, bytes) = reopened
            .read_completed_media_chunk(
                Some("slot-a"),
                "session:a",
                "agent-a",
                &attachment.id,
                2,
                4,
            )
            .unwrap();
        assert_eq!(meta, attachment);
        assert_eq!(bytes, b"cdef");
        let (_, probe) = reopened
            .read_completed_media_chunk(
                Some("slot-a"),
                "session:a",
                "agent-a",
                &attachment.id,
                0,
                0,
            )
            .unwrap();
        assert!(probe.is_empty());
        assert!(reopened
            .read_completed_media_chunk(
                Some("slot-b"),
                "session:a",
                "agent-a",
                &attachment.id,
                0,
                1,
            )
            .unwrap_err()
            .contains("fenced"));
        assert!(reopened
            .read_completed_media_chunk(
                Some("slot-a"),
                "session:a",
                "agent-a",
                &attachment.id,
                attachment.size_bytes,
                1,
            )
            .unwrap_err()
            .contains("not satisfiable"));
        assert!(reopened
            .read_completed_media_chunk(
                Some("slot-a"),
                "session:a",
                "agent-a",
                &attachment.id,
                0,
                MEDIA_READ_CHUNK_LIMIT_BYTES + 1,
            )
            .unwrap_err()
            .contains("four MiB"));
    }

    #[test]
    fn empty_and_oversized_files_fail_with_desktop_equivalent_notices() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("attachments");
        let mut owner = DurableAgentAttachmentOwner::open(&root).unwrap();
        let empty = owner.begin(None, "session:a", "agent-a", "empty.txt", Some("text/plain"), Some(0));
        assert_eq!(empty.unwrap_err(), "\"empty.txt\" is empty, so it wasn't attached.");
        let oversized = owner.begin(
            None,
            "session:a",
            "agent-a",
            "movie.mp4",
            Some("video/mp4"),
            Some(VIDEO_BYTE_LIMIT + 1),
        );
        assert_eq!(
            oversized.unwrap_err(),
            "\"movie.mp4\" is too large to attach (max 200 MB for video)."
        );
    }

    #[test]
    fn unsafe_desktop_attachment_filenames_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("attachments");
        let mut owner = DurableAgentAttachmentOwner::open(&root).unwrap();

        for name in ["../secret.txt", "folder/file.txt", "folder\\file.txt", "bad\0name.txt"] {
            assert_eq!(
                owner
                    .begin(None, "session:a", "agent-a", name, Some("text/plain"), Some(1))
                    .unwrap_err(),
                "attachment name is invalid",
            );
        }
        assert_eq!(
            owner
                .begin(
                    None,
                    "session:a",
                    "agent-a",
                    &"a".repeat(256),
                    Some("text/plain"),
                    Some(1),
                )
                .unwrap_err(),
            "attachment name is invalid",
        );
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

    #[test]
    fn process_reopen_reconciles_orphan_and_missing_completed_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("attachments");
        let attachment = {
            let mut owner = DurableAgentAttachmentOwner::open(&root).unwrap();
            let id = owner
                .begin(None, "session:a", "agent-a", "kept.txt", Some("text/plain"), Some(4))
                .unwrap();
            owner.append(None, "session:a", "agent-a", &id, 0, b"kept").unwrap();
            owner.finish(None, "session:a", "agent-a", &id).unwrap()
        };

        fs::write(root.join("staging").join("orphan.part"), b"partial").unwrap();
        fs::write(root.join("files").join("orphan.bin"), b"uncertain").unwrap();
        fs::remove_file(root.join("files").join(format!("{}.bin", attachment.id))).unwrap();

        let mut reopened = DurableAgentAttachmentOwner::open(&root).unwrap();
        assert!(!root.join("staging").join("orphan.part").exists());
        assert!(!root.join("files").join("orphan.bin").exists());
        assert!(!reopened
            .remove_completed(None, "session:a", "agent-a", &attachment.id)
            .unwrap());
    }
}
