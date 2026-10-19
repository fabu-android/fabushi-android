use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::conversation_state_binary::decode_summary_archive_message_ids;

pub const OVERSIZE_TRANSCRIPT_BLOB_THRESHOLD_BYTES: usize = 5_000_000;
const SERIALIZED_UINT8_ARRAY_MARKER: &str = "\"__type\":\"Uint8Array\"";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LegacyTranscriptState {
    pub root_prompt_messages_json: Vec<Vec<u8>>,
    pub summary_archives: Vec<Vec<u8>>,
    pub turns: Vec<Vec<u8>>,
}

pub trait LegacyTranscriptBlobStore {
    fn get_blob(&self, blob_id: &[u8]) -> Result<Option<Vec<u8>>, String>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LegacyTranscriptMirrorError {
    #[error("unsafe conversation id")]
    UnsafeConversationId,
    #[error("transcript mirror I/O failed: {0}")]
    Io(String),
    #[error("transcript blob read failed: {0}")]
    Blob(String),
}

#[derive(Debug, Clone)]
struct CoreMessage {
    value: Value,
}

impl CoreMessage {
    fn role(&self) -> &str {
        self.value
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or_default()
    }

    fn is_summary(&self) -> bool {
        self.value
            .get("providerOptions")
            .and_then(Value::as_object)
            .and_then(|value| value.get("cursor"))
            .and_then(Value::as_object)
            .and_then(|value| value.get("isSummary"))
            .and_then(Value::as_bool)
            == Some(true)
    }
}

pub struct LegacyFileTranscriptMirror {
    transcripts_dir: PathBuf,
}

impl LegacyFileTranscriptMirror {
    pub fn new(transcripts_dir: impl Into<PathBuf>) -> Self {
        Self {
            transcripts_dir: transcripts_dir.into(),
        }
    }

    pub fn jsonl_path_for(
        &self,
        conversation_id: &str,
    ) -> Result<PathBuf, LegacyTranscriptMirrorError> {
        let safe = safe_conversation_id(conversation_id)?;
        Ok(self
            .transcripts_dir
            .join(safe)
            .join(format!("{safe}.jsonl")))
    }

    pub fn write_incremental<Store: LegacyTranscriptBlobStore>(
        &self,
        conversation_id: &str,
        state: &LegacyTranscriptState,
        blob_store: &Store,
        previous_root_prompt_count: usize,
    ) -> Result<Option<usize>, LegacyTranscriptMirrorError> {
        let current_count = state.root_prompt_messages_json.len();
        if previous_root_prompt_count == 0 || current_count < previous_root_prompt_count {
            return Ok(None);
        }
        if current_count == previous_root_prompt_count {
            return Ok(Some(current_count));
        }

        let messages = hydrate_blob_ids(
            blob_store,
            &state.root_prompt_messages_json[previous_root_prompt_count..],
        )?;
        let lines = messages
            .iter()
            .filter_map(format_single_message_jsonl)
            .collect::<Vec<_>>();
        if lines.is_empty() {
            return Ok(Some(current_count));
        }

        let path = self.jsonl_path_for(conversation_id)?;
        let Some(parent) = path.parent() else {
            return Err(LegacyTranscriptMirrorError::Io(
                "transcript JSONL path has no parent".into(),
            ));
        };
        if fs::create_dir_all(parent).is_err() {
            return Ok(None);
        }
        let mut handle = match OpenOptions::new().create(true).append(true).open(&path) {
            Ok(handle) => handle,
            Err(_) => return Ok(None),
        };
        let content = ensure_trailing_newline(&lines.join("\n"));
        if handle.write_all(content.as_bytes()).is_err() {
            return Ok(None);
        }
        Ok(Some(current_count))
    }

    pub fn write_full<Store: LegacyTranscriptBlobStore>(
        &self,
        conversation_id: &str,
        state: &LegacyTranscriptState,
        blob_store: &Store,
    ) -> Result<bool, LegacyTranscriptMirrorError> {
        let messages = hydrate_messages(blob_store, state)?;
        if messages.is_empty() {
            return Ok(
                state.summary_archives.is_empty()
                    && state.root_prompt_messages_json.is_empty(),
            );
        }
        let content = format_transcript_jsonl(&messages);
        if content.is_empty() {
            return Ok(
                state.summary_archives.is_empty()
                    && state.root_prompt_messages_json.is_empty(),
            );
        }

        let path = self.jsonl_path_for(conversation_id)?;
        if let Ok(existing) = fs::read_to_string(&path) {
            if count_transcript_message_lines(&content)
                < count_transcript_message_lines(&existing)
            {
                return Ok(false);
            }
        }
        let Some(parent) = path.parent() else {
            return Err(LegacyTranscriptMirrorError::Io(
                "transcript JSONL path has no parent".into(),
            ));
        };
        fs::create_dir_all(parent)
            .map_err(|error| LegacyTranscriptMirrorError::Io(error.to_string()))?;
        fs::write(&path, ensure_trailing_newline(&content))
            .map_err(|error| LegacyTranscriptMirrorError::Io(error.to_string()))?;
        Ok(true)
    }
}

pub fn count_transcript_message_lines(jsonl: &str) -> usize {
    jsonl
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return false;
            }
            let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
                return true;
            };
            !matches!(
                value.get("type").and_then(Value::as_str),
                Some("metadata" | "turn_ended")
            )
        })
        .count()
}

fn hydrate_messages<Store: LegacyTranscriptBlobStore>(
    blob_store: &Store,
    state: &LegacyTranscriptState,
) -> Result<Vec<CoreMessage>, LegacyTranscriptMirrorError> {
    let mut all = Vec::new();
    for archive_id in &state.summary_archives {
        let archive = blob_store
            .get_blob(archive_id)
            .map_err(LegacyTranscriptMirrorError::Blob)?;
        let Some(archive) = archive else {
            continue;
        };
        let Ok(ids) = decode_summary_archive_message_ids(&archive) else {
            continue;
        };
        all.extend(hydrate_blob_ids(blob_store, &ids)?);
    }
    let prompt_messages = hydrate_blob_ids(blob_store, &state.root_prompt_messages_json)?;
    all.extend(
        prompt_messages
            .into_iter()
            .filter(|message| message.role() != "system" && !message.is_summary()),
    );
    Ok(all)
}

fn hydrate_blob_ids<Store: LegacyTranscriptBlobStore>(
    blob_store: &Store,
    blob_ids: &[Vec<u8>],
) -> Result<Vec<CoreMessage>, LegacyTranscriptMirrorError> {
    let mut messages = Vec::new();
    for blob_id in blob_ids {
        let blob = blob_store
            .get_blob(blob_id)
            .map_err(LegacyTranscriptMirrorError::Blob)?;
        let Some(blob) = blob else {
            continue;
        };
        if blob.len() > OVERSIZE_TRANSCRIPT_BLOB_THRESHOLD_BYTES {
            messages.push(CoreMessage {
                value: serde_json::json!({
                    "role": "assistant",
                    "content": format!(
                        "[Oversize transcript blob omitted: {:.1} MB]",
                        blob.len() as f64 / 1_000_000.0
                    )
                }),
            });
            continue;
        }
        if let Some(message) = deserialize_core_message(&blob) {
            messages.push(message);
        }
    }
    Ok(messages)
}

fn deserialize_core_message(blob: &[u8]) -> Option<CoreMessage> {
    let raw = std::str::from_utf8(blob).ok()?;
    let mut value: Value = serde_json::from_str(raw).ok()?;
    if raw.contains(SERIALIZED_UINT8_ARRAY_MARKER) {
        revive_binary_markers(&mut value);
    }
    value.as_object()?;
    Some(CoreMessage { value })
}

fn revive_binary_markers(value: &mut Value) {
    match value {
        Value::Object(object) => {
            if object.get("__type").and_then(Value::as_str) == Some("Uint8Array") {
                if let Some(hex) = object.get("hex").and_then(Value::as_str) {
                    *value = Value::String(format!(
                        "[Binary data omitted from transcript: {} bytes]",
                        hex.len() / 2
                    ));
                    return;
                }
            }
            for child in object.values_mut() {
                revive_binary_markers(child);
            }
        }
        Value::Array(values) => {
            for child in values {
                revive_binary_markers(child);
            }
        }
        _ => {}
    }
}

fn format_transcript_jsonl(messages: &[CoreMessage]) -> String {
    let mut filtered = messages
        .iter()
        .filter(|message| message.role() != "system")
        .collect::<Vec<_>>();
    if filtered.len() >= 2
        && filtered[0].role() == "user"
        && filtered[1].role() == "user"
    {
        filtered.remove(0);
    }
    filtered
        .into_iter()
        .filter_map(format_single_message_jsonl)
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_single_message_jsonl(message: &CoreMessage) -> Option<String> {
    if message.role() == "system" || message.is_summary() {
        return None;
    }
    let role = message.role();
    if role.is_empty() {
        return None;
    }

    let mut text_parts = Vec::<String>::new();
    let mut thinking_parts = Vec::<String>::new();
    let mut tool_calls = Vec::<Value>::new();

    match message.value.get("content") {
        Some(Value::String(text)) => {
            let processed = if role == "user" {
                strip_context_tags(text)
            } else if role == "assistant" {
                strip_hidden_thinking_tags(text)
            } else {
                text.clone()
            };
            if !processed.trim().is_empty() {
                text_parts.push(processed);
            }
        }
        Some(Value::Array(parts)) => {
            for part in parts {
                let Some(object) = part.as_object() else {
                    continue;
                };
                match object.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        let Some(text) = object.get("text").and_then(Value::as_str) else {
                            continue;
                        };
                        let processed = if role == "user" {
                            strip_context_tags(text)
                        } else if role == "assistant" {
                            strip_hidden_thinking_tags(text)
                        } else {
                            text.to_string()
                        };
                        if !processed.trim().is_empty() {
                            text_parts.push(processed);
                        }
                    }
                    Some("reasoning") => {
                        if let Some(text) = object
                            .get("text")
                            .and_then(Value::as_str)
                            .filter(|value| !value.trim().is_empty())
                        {
                            thinking_parts.push(text.to_string());
                        }
                    }
                    Some("redacted-reasoning") => thinking_parts.push("[REDACTED]".into()),
                    Some("image") => text_parts.push("[Image]".into()),
                    Some("file") => {
                        let text = object
                            .get("filename")
                            .and_then(Value::as_str)
                            .map(|name| format!("[File: {name}]"))
                            .unwrap_or_else(|| "[File]".into());
                        text_parts.push(text);
                    }
                    Some("tool-call") => {
                        let Some(name) = object.get("toolName").and_then(Value::as_str) else {
                            continue;
                        };
                        let mut call = Map::new();
                        call.insert("type".into(), Value::String("tool_use".into()));
                        call.insert("name".into(), Value::String(name.to_string()));
                        if let Some(args) = object.get("args") {
                            call.insert("input".into(), args.clone());
                        }
                        tool_calls.push(Value::Object(call));
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }

    let mut content = Vec::<Value>::new();
    let mut combined_text = text_parts;
    if !thinking_parts.is_empty() {
        combined_text.push(thinking_parts.join("\n"));
    }
    if !combined_text.is_empty() {
        content.push(serde_json::json!({
            "type": "text",
            "text": combined_text.join("\n\n")
        }));
    }
    content.extend(tool_calls);
    if content.is_empty() {
        return None;
    }
    Some(
        serde_json::json!({
            "role": role,
            "message": { "content": content }
        })
        .to_string(),
    )
}

fn strip_context_tags(text: &str) -> String {
    const TAGS: &[&str] = &[
        "user_info", "project_layout", "rules", "always_applied_workspace_rules",
        "agent_requestable_workspace_rules", "user_rules", "agent_skills", "available_skills",
        "cloud_instructions", "cloud_task_instructions", "open_and_recently_viewed_files",
        "system_reminder", "system-reminder", "mcp_instructions", "mcp_file_system",
        "mcp_file_system_servers", "git_status", "agent_transcripts", "cursor_rules_context",
        "attached_files", "system_notification", "task_notification", "agent_notification",
    ];
    let mut output = text.to_string();
    for tag in TAGS {
        output = strip_tag_contents(&output, tag);
    }
    collapse_blank_lines(&output)
}

fn strip_hidden_thinking_tags(text: &str) -> String {
    collapse_blank_lines(&strip_tag_contents(
        &strip_tag_contents(text, "think"),
        "thinking",
    ))
}

fn strip_tag_contents(text: &str, tag: &str) -> String {
    let mut output = text.to_string();
    let open_prefix = format!("<{tag}");
    let close_tag = format!("</{tag}>");
    loop {
        let lower = output.to_ascii_lowercase();
        let Some(mut start) = lower.find(&open_prefix) else {
            break;
        };
        loop {
            let next = lower
                .as_bytes()
                .get(start + open_prefix.len())
                .copied();
            if matches!(next, Some(b'>') | Some(b' ') | Some(b'\t') | Some(b'\r') | Some(b'\n')) {
                break;
            }
            let search_from = start + open_prefix.len();
            let Some(relative) = lower[search_from..].find(&open_prefix) else {
                return output;
            };
            start = search_from + relative;
        }
        let Some(open_tail) = lower[start..].find('>') else {
            break;
        };
        let open_end = start + open_tail + 1;
        let Some(close_relative) = lower[open_end..].find(&close_tag) else {
            break;
        };
        let close_end = open_end + close_relative + close_tag.len();
        output.replace_range(start..close_end, "");
    }
    output
}

fn collapse_blank_lines(text: &str) -> String {
    let mut output = text.to_string();
    while output.contains("\n\n\n") {
        output = output.replace("\n\n\n", "\n\n");
    }
    output.trim().to_string()
}

fn ensure_trailing_newline(content: &str) -> String {
    if content.ends_with('\n') {
        content.to_string()
    } else {
        format!("{content}\n")
    }
}

fn safe_conversation_id(id: &str) -> Result<&str, LegacyTranscriptMirrorError> {
    if !id.is_empty()
        && id != "."
        && id != ".."
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        Ok(id)
    } else {
        Err(LegacyTranscriptMirrorError::UnsafeConversationId)
    }
}

pub fn transcript_jsonl_path(
    transcripts_dir: &Path,
    conversation_id: &str,
) -> Result<PathBuf, LegacyTranscriptMirrorError> {
    LegacyFileTranscriptMirror::new(transcripts_dir).jsonl_path_for(conversation_id)
}
