use std::collections::{HashMap, HashSet};
use std::future::Future;

use rusqlite::Connection;
use serde_json::{Map, Value};
use crate::transcript_mirror::conversation_state_binary::decode_conversation_state_recovery_fields;

pub const MAX_ROOT_BLOB_BYTES: usize = 8 * 1024 * 1024;
pub const REBUILT_ENTRY_ID_PREFIX: &str = "recovered-";

#[derive(Debug, Clone, PartialEq)]
pub enum OutlineItem {
    User {
        id: String,
        hidden: bool,
        text: String,
        timestamp_ms: Option<f64>,
    },
    SendMessage {
        id: String,
        message: Value,
        timestamp_ms: Option<f64>,
    },
    ToolCall {
        id: String,
        name: String,
        status: String,
        summary: Option<String>,
        timestamp_ms: Option<f64>,
    },
}

impl OutlineItem {
    pub fn id(&self) -> &str {
        match self {
            Self::User { id, .. }
            | Self::SendMessage { id, .. }
            | Self::ToolCall { id, .. } => id,
        }
    }
}

pub fn rebuild_transcript_entries_from_state(
    turns: &[Vec<OutlineItem>],
) -> Vec<Value> {
    let mut entries = Vec::new();
    for turn in turns {
        for item in turn {
            let mut entry = Map::new();
            match item {
                OutlineItem::User {
                    id,
                    hidden: false,
                    text,
                    timestamp_ms,
                } => {
                    entry.insert("kind".into(), Value::String("message".into()));
                    entry.insert(
                        "id".into(),
                        Value::String(format!("{REBUILT_ENTRY_ID_PREFIX}{id}")),
                    );
                    entry.insert("role".into(), Value::String("user".into()));
                    entry.insert("content".into(), Value::String(text.clone()));
                    entry.insert("isStreaming".into(), Value::Bool(false));
                    if let Some(timestamp_ms) = timestamp_ms.and_then(finite_number) {
                        entry.insert("timestampMs".into(), number_value(timestamp_ms));
                    }
                }
                OutlineItem::SendMessage {
                    id,
                    message,
                    timestamp_ms,
                } => {
                    entry.insert("kind".into(), Value::String("send-message".into()));
                    entry.insert(
                        "id".into(),
                        Value::String(format!("{REBUILT_ENTRY_ID_PREFIX}{id}")),
                    );
                    entry.insert("message".into(), message.clone());
                    if let Some(timestamp_ms) = timestamp_ms.and_then(finite_number) {
                        entry.insert("timestampMs".into(), number_value(timestamp_ms));
                    }
                }
                OutlineItem::ToolCall {
                    id,
                    name,
                    status,
                    summary,
                    timestamp_ms,
                } => {
                    entry.insert("kind".into(), Value::String("tool-call".into()));
                    entry.insert(
                        "id".into(),
                        Value::String(format!("{REBUILT_ENTRY_ID_PREFIX}{id}")),
                    );
                    entry.insert("name".into(), Value::String(name.clone()));
                    entry.insert("status".into(), Value::String(status.clone()));
                    if let Some(summary) = summary {
                        entry.insert("summary".into(), Value::String(summary.clone()));
                    }
                    if let Some(timestamp_ms) = timestamp_ms.and_then(finite_number) {
                        entry.insert("timestampMs".into(), number_value(timestamp_ms));
                    }
                }
                OutlineItem::User { hidden: true, .. } => continue,
            }
            entries.push(Value::Object(entry));
        }
    }
    entries
}

pub fn select_hidden_artifact_entry_ids(
    entries: &[Value],
    outline: &[OutlineItem],
) -> Vec<String> {
    let by_id = outline
        .iter()
        .map(|item| (item.id(), item))
        .collect::<HashMap<_, _>>();
    let mut ids = Vec::new();
    for entry in entries {
        if entry.get("kind").and_then(Value::as_str) != Some("message")
            || entry.get("role").and_then(Value::as_str) != Some("user")
        {
            continue;
        }
        let Some(id) = entry.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(source_id) = id.strip_prefix(REBUILT_ENTRY_ID_PREFIX) else {
            continue;
        };
        let Some(OutlineItem::User {
            hidden: true, text, ..
        }) = by_id.get(source_id)
        else {
            continue;
        };
        if entry.get("content").and_then(Value::as_str) == Some(text.as_str()) {
            ids.push(id.to_string());
        }
    }
    ids
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConversationStructureRefs {
    pub turns: Vec<Vec<u8>>,
    pub todos: Vec<Vec<u8>>,
    pub summary: Option<Vec<u8>>,
}

pub async fn conversation_structure_fully_resolves<E, F, Fut>(
    structure: &ConversationStructureRefs,
    mut get_blob: F,
) -> bool
where
    F: FnMut(Vec<u8>) -> Fut,
    Fut: Future<Output = Result<Option<Vec<u8>>, E>>,
{
    if structure.turns.is_empty() {
        return false;
    }
    for id in structure.turns.iter().chain(structure.todos.iter()) {
        let Ok(Some(blob)) = get_blob(id.clone()).await else {
            return false;
        };
        if blob.len() > MAX_ROOT_BLOB_BYTES {
            return false;
        }
    }
    if let Some(summary) = structure.summary.as_ref().filter(|summary| !summary.is_empty()) {
        let Ok(Some(blob)) = get_blob(summary.clone()).await else {
            return false;
        };
        if blob.len() > MAX_ROOT_BLOB_BYTES {
            return false;
        }
    }
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RootScore {
    pub turns: usize,
    pub root_prompts: usize,
    pub bytes: usize,
}

pub fn is_better_root(candidate: RootScore, best: Option<RootScore>) -> bool {
    let Some(best) = best else {
        return true;
    };
    candidate.turns > best.turns
        || (candidate.turns == best.turns && candidate.root_prompts > best.root_prompts)
        || (candidate.turns == best.turns
            && candidate.root_prompts == best.root_prompts
            && candidate.bytes > best.bytes)
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MinimalConversationState {
    pub turns: Vec<Vec<u8>>,
    pub todos: Vec<Vec<u8>>,
    pub summary: Option<Vec<u8>>,
    pub root_prompts: usize,
}

impl MinimalConversationState {
    pub fn refs(&self) -> ConversationStructureRefs {
        ConversationStructureRefs {
            turns: self.turns.clone(),
            todos: self.todos.clone(),
            summary: self.summary.clone(),
        }
    }
}

pub fn score_root_candidate(
    data: &[u8],
    present_ids: &HashSet<String>,
) -> Option<RootScore> {
    if data.len() > MAX_ROOT_BLOB_BYTES {
        return None;
    }
    let parsed = parse_conversation_state_structure(data)?;
    if parsed.turns.is_empty()
        || parsed
            .turns
            .iter()
            .any(|turn| !present_ids.contains(&to_hex(turn)))
    {
        return None;
    }
    Some(RootScore {
        turns: parsed.turns.len(),
        root_prompts: parsed.root_prompts,
        bytes: data.len(),
    })
}

pub fn find_latest_root_blob_id_in_database(
    db: &Connection,
) -> Result<Option<Vec<u8>>, rusqlite::Error> {
    let mut present_ids = HashSet::new();
    {
        let mut statement = db.prepare("SELECT id FROM blobs")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        for id in rows {
            present_ids.insert(id?);
        }
    }

    let mut best_id: Option<String> = None;
    let mut best_score: Option<RootScore> = None;
    let mut statement = db.prepare("SELECT id, data FROM blobs")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
    })?;
    for row in rows {
        let (id, data) = row?;
        let Some(score) = score_root_candidate(&data, &present_ids) else {
            continue;
        };
        if is_better_root(score, best_score) {
            best_id = Some(id);
            best_score = Some(score);
        }
    }
    Ok(best_id.and_then(|id| from_hex(&id)))
}

pub fn parse_conversation_state_structure(
    data: &[u8],
) -> Option<MinimalConversationState> {
    let decoded = decode_conversation_state_recovery_fields(data).ok()?;
    Some(MinimalConversationState {
        turns: decoded.turns,
        todos: decoded.todos,
        summary: decoded.summary,
        root_prompts: decoded.root_prompt_messages_json.len(),
    })
}

fn finite_number(value: f64) -> Option<f64> {
    value.is_finite().then_some(value)
}

fn number_value(value: f64) -> Value {
    serde_json::Number::from_f64(value)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn from_hex(value: &str) -> Option<Vec<u8>> {
    if value.len() % 2 != 0 {
        return None;
    }
    let mut output = Vec::with_capacity(value.len() / 2);
    for index in (0..value.len()).step_by(2) {
        output.push(u8::from_str_radix(&value[index..index + 2], 16).ok()?);
    }
    Some(output)
}
