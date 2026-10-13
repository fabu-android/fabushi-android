use crate::extensions::content_search::agent_content_search::{
    build_content_snippet, entry_search_text, AgentContentRole, SearchableTranscriptEntry,
    SearchableTranscriptEntryKind, AGENT_CONTENT_SEARCH_MAX_MATCHES_PER_AGENT,
    AGENT_CONTENT_SEARCH_MAX_RESULTS,
};
use serde::Serialize;
use serde_json::Value;
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};
use std::collections::{HashMap, HashSet};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TranscriptRecoveryOutcome {
    RecoveredInterruptedWrite,
    RecoveredSalvage,
    ResetCorruptStore,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptRecoveryInfo {
    pub outcome: TranscriptRecoveryOutcome,
    pub quarantine_path: Option<PathBuf>,
}

pub const CONTENT_SEARCH_MAX_MATCHES_PER_AGENT: usize = AGENT_CONTENT_SEARCH_MAX_MATCHES_PER_AGENT;
pub const CONTENT_SEARCH_MAX_RESULTS: usize = AGENT_CONTENT_SEARCH_MAX_RESULTS;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptContentMatch {
    pub agent_id: String,
    pub entry_id: String,
    pub role: String,
    pub timestamp_ms: f64,
    pub snippet: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptMediaMatch {
    pub agent_id: String,
    pub entry_id: String,
    pub path: String,
    pub file_name: String,
    pub ext: String,
    pub mime: Option<String>,
    pub kind: String,
    pub timestamp_ms: f64,
    pub size_bytes: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct TranscriptStore {
    path: PathBuf,
    entries: Vec<Value>,
}

impl TranscriptStore {
    pub fn open(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        let entries = match read_entries(&path) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error),
        };
        Ok(Self { path, entries })
    }

    pub fn open_recovering(
        path: impl Into<PathBuf>,
    ) -> io::Result<(Self, Option<TranscriptRecoveryInfo>)> {
        let path = path.into();
        let temporary = temporary_path(&path);
        match read_entries(&path) {
            Ok(entries) => {
                let _ = fs::remove_file(&temporary);
                return Ok((Self { path, entries }, None));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if let Ok(entries) = read_entries(&temporary) {
                    install_replacement(&temporary, &path)?;
                    return Ok((
                        Self { path, entries },
                        Some(TranscriptRecoveryInfo {
                            outcome: TranscriptRecoveryOutcome::RecoveredInterruptedWrite,
                            quarantine_path: None,
                        }),
                    ));
                }
                let _ = fs::remove_file(&temporary);
                return Ok((Self { path, entries: Vec::new() }, None));
            }
            Err(error) if error.kind() != io::ErrorKind::InvalidData => return Err(error),
            Err(_) => {}
        }

        let quarantine_path = quarantine_corrupt_transcript(&path)?;
        if let Ok(entries) = read_entries(&temporary) {
            install_replacement(&temporary, &path)?;
            return Ok((
                Self { path, entries },
                Some(TranscriptRecoveryInfo {
                    outcome: TranscriptRecoveryOutcome::RecoveredInterruptedWrite,
                    quarantine_path: Some(quarantine_path),
                }),
            ));
        }

        let backup = backup_path(&path);
        if let Ok(entries) = read_entries(&backup) {
            let recovery_copy = temporary_path(&path);
            fs::copy(&backup, &recovery_copy)?;
            sync_file(&recovery_copy)?;
            install_replacement(&recovery_copy, &path)?;
            let _ = fs::remove_file(&temporary);
            return Ok((
                Self { path, entries },
                Some(TranscriptRecoveryInfo {
                    outcome: TranscriptRecoveryOutcome::RecoveredInterruptedWrite,
                    quarantine_path: Some(quarantine_path),
                }),
            ));
        }

        let _ = fs::remove_file(&temporary);
        let salvaged = salvage_valid_entries(&quarantine_path).unwrap_or_default();
        if !salvaged.is_empty() {
            let store = Self { path, entries: salvaged };
            store.persist()?;
            return Ok((
                store,
                Some(TranscriptRecoveryInfo {
                    outcome: TranscriptRecoveryOutcome::RecoveredSalvage,
                    quarantine_path: Some(quarantine_path),
                }),
            ));
        }

        let store = Self { path, entries: Vec::new() };
        store.persist()?;
        Ok((
            store,
            Some(TranscriptRecoveryInfo {
                outcome: TranscriptRecoveryOutcome::ResetCorruptStore,
                quarantine_path: Some(quarantine_path),
            }),
        ))
    }

    pub fn get_transcript(&self) -> Vec<Value> {
        self.entries.clone()
    }

    pub fn page_for_agent(
        &self,
        agent_id: &str,
        before_seq: Option<usize>,
        limit: usize,
    ) -> io::Result<(Vec<Value>, Option<usize>)> {
        self.page_for_agent_with_bounds(agent_id, before_seq, None, None, limit)
    }

    pub fn page_for_agent_with_bounds(
        &self,
        agent_id: &str,
        before_seq: Option<usize>,
        since_ms: Option<f64>,
        until_ms: Option<f64>,
        limit: usize,
    ) -> io::Result<(Vec<Value>, Option<usize>)> {
        let agent_id = agent_id.trim();
        if agent_id.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "transcript page agent id is required"));
        }
        if limit == 0 || limit > 5_000 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "transcript page limit must be between 1 and 5000"));
        }
        if since_ms.is_some_and(|value| !value.is_finite()) || until_ms.is_some_and(|value| !value.is_finite()) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "transcript page time bounds must be finite"));
        }
        let scoped = self
            .entries
            .iter()
            .filter(|entry| entry.get("agentId").and_then(Value::as_str) == Some(agent_id))
            .filter(|entry| is_main_transcript_page_entry(entry))
            .filter(|entry| {
                let Some(timestamp_ms) = entry.get("timestampMs").and_then(Value::as_f64) else { return true; };
                since_ms.is_none_or(|since| timestamp_ms >= since)
                    && until_ms.is_none_or(|until| timestamp_ms <= until)
            })
            .collect::<Vec<_>>();
        let end = before_seq.unwrap_or(scoped.len()).min(scoped.len());
        let start = end.saturating_sub(limit);
        let page = scoped[start..end].iter().map(|entry| (*entry).clone()).collect::<Vec<_>>();
        let next_before_seq = (start > 0).then_some(start);
        Ok((page, next_before_seq))
    }

    pub fn search_content(
        &self,
        query: &str,
        limit: usize,
    ) -> io::Result<Vec<TranscriptContentMatch>> {
        let normalized_query = query.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
        if normalized_query.is_empty() {
            return Ok(Vec::new());
        }
        if limit == 0 || limit > CONTENT_SEARCH_MAX_RESULTS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "content search limit must be between 1 and 50",
            ));
        }

        let mut per_agent = HashMap::<String, usize>::new();
        let mut matches = Vec::new();
        for entry in self.entries.iter().rev() {
            if matches.len() >= limit {
                break;
            }
            let Some(agent_id) = entry
                .get("agentId")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            if entry
                .get("hiddenOutboundAgentPeerMessage")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                || entry
                    .get("toAgent")
                    .and_then(|value| value.get("kind"))
                    .and_then(Value::as_str)
                    == Some("agent")
            {
                continue;
            }
            let count = per_agent.get(agent_id).copied().unwrap_or_default();
            if count >= CONTENT_SEARCH_MAX_MATCHES_PER_AGENT {
                continue;
            }
            let Some(searchable) = project_content_search_entry(entry) else {
                continue;
            };
            let Some(snippet) =
                build_content_snippet(entry_search_text(&searchable), &normalized_query)
            else {
                continue;
            };
            let role = match searchable.kind {
                SearchableTranscriptEntryKind::Message { role: AgentContentRole::User, .. } => {
                    "user"
                }
                _ => "assistant",
            };
            matches.push(TranscriptContentMatch {
                agent_id: agent_id.to_string(),
                entry_id: searchable.id,
                role: role.to_string(),
                timestamp_ms: searchable.timestamp_ms.unwrap_or(0.0),
                snippet,
            });
            per_agent.insert(agent_id.to_string(), count + 1);
        }
        Ok(matches)
    }

    pub fn search_media(
        &self,
        query: &str,
        limit: usize,
    ) -> io::Result<Vec<TranscriptMediaMatch>> {
        if limit == 0 || limit > CONTENT_SEARCH_MAX_RESULTS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "media search limit must be between 1 and 50",
            ));
        }
        let terms = normalize_media_search_text(query)
            .split_whitespace()
            .filter(|term| !term.is_empty())
            .take(8)
            .map(str::to_string)
            .collect::<Vec<_>>();
        let mut matches = self
            .entries
            .iter()
            .filter_map(project_media_search_entry)
            .filter(|item| media_name_matches_terms(&item.file_name, &terms))
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| {
            right
                .timestamp_ms
                .partial_cmp(&left.timestamp_ms)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| right.entry_id.cmp(&left.entry_id))
        });
        matches.truncate(limit);
        Ok(matches)
    }

    pub fn contains_id(&self, id: &str) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.get("id").and_then(Value::as_str) == Some(id))
    }

    pub fn entry(&self, id: &str) -> Option<Value> {
        self.entries
            .iter()
            .find(|entry| entry.get("id").and_then(Value::as_str) == Some(id))
            .cloned()
    }

    pub fn user_entry_by_client_nonce(
        &self,
        agent_id: &str,
        client_nonce: &str,
        account_fence: &str,
    ) -> Option<Value> {
        let agent_id = agent_id.trim();
        let client_nonce = client_nonce.trim();
        let account_fence = account_fence.trim();
        if agent_id.is_empty() || client_nonce.is_empty() || account_fence.is_empty() {
            return None;
        }
        self.entries
            .iter()
            .find(|entry| {
                entry.get("kind").and_then(Value::as_str) == Some("message")
                    && entry.get("role").and_then(Value::as_str) == Some("user")
                    && entry.get("agentId").and_then(Value::as_str) == Some(agent_id)
                    && entry.get("clientNonce").and_then(Value::as_str) == Some(client_nonce)
                    && entry.get("accountFence").and_then(Value::as_str) == Some(account_fence)
            })
            .cloned()
    }

    pub fn set_transcript(&mut self, entries: &[Value]) -> io::Result<()> {
        validate_entries(entries)?;
        let previous = std::mem::replace(&mut self.entries, entries.to_vec());
        if let Err(error) = self.persist() {
            self.entries = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn append_entry(&mut self, entry: Value) -> io::Result<()> {
        let id = entry_id(&entry)?.to_string();
        if self.contains_id(&id) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("duplicate transcript entry id: {id}"),
            ));
        }
        self.entries.push(entry);
        if let Err(error) = self.persist() {
            self.entries.pop();
            return Err(error);
        }
        Ok(())
    }

    pub fn append_entry_if_absent(&mut self, entry: Value) -> io::Result<bool> {
        let id = entry_id(&entry)?.to_string();
        if self.contains_id(&id) {
            return Ok(false);
        }
        self.append_entry(entry)?;
        Ok(true)
    }

    pub fn update_entry(
        &mut self,
        id: &str,
        mut update: impl FnMut(&Value) -> Value,
    ) -> io::Result<Option<Value>> {
        let previous = self.entries.clone();
        let mut updated = None;
        for entry in &mut self.entries {
            if entry.get("id").and_then(Value::as_str) != Some(id) {
                continue;
            }
            let next = update(entry);
            entry_id(&next)?;
            updated = Some(next.clone());
            *entry = next;
        }
        if updated.is_none() {
            return Ok(None);
        }
        if let Err(error) = self.persist() {
            self.entries = previous;
            return Err(error);
        }
        Ok(updated)
    }

    pub fn remove_entry(&mut self, id: &str) -> io::Result<bool> {
        let previous = self.entries.clone();
        self.entries
            .retain(|entry| entry.get("id").and_then(Value::as_str) != Some(id));
        if self.entries.len() == previous.len() {
            return Ok(false);
        }
        if let Err(error) = self.persist() {
            self.entries = previous;
            return Err(error);
        }
        Ok(true)
    }

    pub fn clear_transcript(&mut self) -> io::Result<()> {
        let previous = std::mem::take(&mut self.entries);
        if let Err(error) = self.persist() {
            self.entries = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn persist(&self) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = temporary_path(&self.path);
        let serialized = serde_json::to_string_pretty(&self.entries)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let result = (|| -> io::Result<()> {
            let mut file = fs::File::create(&temporary)?;
            file.write_all(format!("{serialized}\n").as_bytes())?;
            file.sync_all()?;

            if read_entries(&self.path).is_ok() {
                let backup = backup_path(&self.path);
                fs::copy(&self.path, &backup)?;
                sync_file(&backup)?;
                sync_parent(&backup)?;
            }

            install_replacement(&temporary, &self.path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

fn read_entries(path: &Path) -> io::Result<Vec<Value>> {
    let raw = fs::read_to_string(path)?;
    let value: Value = serde_json::from_str(&raw)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let entries = value
        .as_array()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "transcript root must be an array"))?
        .clone();
    validate_entries(&entries)?;
    Ok(entries)
}

fn salvage_valid_entries(path: &Path) -> io::Result<Vec<Value>> {
    let raw = fs::read_to_string(path)?;
    if !raw.trim_start().starts_with('[') {
        return Ok(Vec::new());
    }

    // A corrupt transcript may be syntactically invalid as a whole (for example, process death
    // after several complete array elements but before the final element/closing bracket). Mirror
    // Desktop's row-by-row SQLite salvage: only recover fully delimited top-level JSON objects,
    // never guess a partial object, and keep the first occurrence of a durable entry id.
    let mut entries = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut start = None;
    let mut object_depth = 0usize;
    let mut array_depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (index, ch) in raw.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '[' => array_depth = array_depth.saturating_add(1),
            ']' => array_depth = array_depth.saturating_sub(1),
            '{' if array_depth == 1 => {
                if object_depth == 0 {
                    start = Some(index);
                }
                object_depth = object_depth.saturating_add(1);
            }
            '{' if object_depth > 0 => {
                object_depth = object_depth.saturating_add(1);
            }
            '}' if object_depth > 0 => {
                object_depth -= 1;
                if object_depth == 0 {
                    let Some(start) = start.take() else {
                        continue;
                    };
                    let end = index + ch.len_utf8();
                    let Ok(entry) = serde_json::from_str::<Value>(&raw[start..end]) else {
                        continue;
                    };
                    let Ok(id) = entry_id(&entry) else {
                        continue;
                    };
                    if seen_ids.insert(id.to_string()) {
                        entries.push(entry);
                    }
                }
            }
            _ => {}
        }
    }

    Ok(entries)
}

fn temporary_path(path: &Path) -> PathBuf {
    path.with_extension("json.tmp")
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

fn install_replacement(temporary: &Path, path: &Path) -> io::Result<()> {
    fs::rename(temporary, path)?;
    sync_parent(path)
}

fn sync_file(path: &Path) -> io::Result<()> {
    fs::File::open(path)?.sync_all()
}

fn quarantine_corrupt_transcript(path: &Path) -> io::Result<PathBuf> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("transcript.json");
    let quarantine = path.with_file_name(format!("{file_name}.corrupt-{stamp}"));
    fs::rename(path, &quarantine)?;
    sync_parent(path)?;
    Ok(quarantine)
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> io::Result<()> {
    match path.parent() {
        Some(parent) => fs::File::open(parent)?.sync_all(),
        None => Ok(()),
    }
}

#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> io::Result<()> {
    Ok(())
}

pub(crate) fn is_main_transcript_page_entry(entry: &Value) -> bool {
    if entry.get("branched").and_then(Value::as_bool).unwrap_or(false) {
        return false;
    }
    match entry.get("kind").and_then(Value::as_str) {
        Some("send-message" | "sendMessage" | "user-attachment" | "userAttachment") => true,
        Some("message") => {
            entry.get("role").and_then(Value::as_str) == Some("user")
                || entry.get("fromAgent").is_some_and(|value| !value.is_null())
                || entry.get("toAgent").is_some_and(|value| !value.is_null())
        }
        _ => false,
    }
}

fn entry_id(entry: &Value) -> io::Result<&str> {
    entry
        .as_object()
        .and_then(|object| object.get("id"))
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "transcript entry id is required"))
}

pub(crate) fn project_media_search_entry(entry: &Value) -> Option<TranscriptMediaMatch> {
    let agent_id = entry.get("agentId").and_then(Value::as_str)?.trim();
    let entry_id = entry.get("id").and_then(Value::as_str)?.trim();
    if agent_id.is_empty() || entry_id.is_empty() {
        return None;
    }
    let kind = entry.get("kind").and_then(Value::as_str).unwrap_or_default();
    let (raw_path, explicit_name, size_bytes, width, height) = match kind {
        "user-attachment" | "userAttachment" => (
            entry.get("filePath").and_then(Value::as_str)?,
            entry.get("fileName").and_then(Value::as_str),
            whole_media_dimension(entry.get("sizeBytes").or_else(|| entry.get("size"))),
            whole_media_dimension(entry.get("width")),
            whole_media_dimension(entry.get("height")),
        ),
        "send-message" | "sendMessage" => {
            let message = entry.get("message")?;
            if message.get("type").and_then(Value::as_str) != Some("attachment")
                && message.get("messageType").and_then(Value::as_str) != Some("attachment")
            {
                return None;
            }
            (
                message.get("url").or_else(|| message.get("filePath")).and_then(Value::as_str)?,
                message.get("fileName").and_then(Value::as_str),
                whole_media_dimension(message.get("sizeBytes").or_else(|| message.get("size"))),
                None,
                None,
            )
        }
        _ => return None,
    };
    let file_name = explicit_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| media_basename(raw_path));
    if file_name.is_empty() {
        return None;
    }
    let extension = media_extension(&file_name).unwrap_or_default();
    Some(TranscriptMediaMatch {
        agent_id: agent_id.to_string(),
        entry_id: entry_id.to_string(),
        path: raw_path.to_string(),
        ext: if extension.is_empty() { String::new() } else { format!(".{extension}") },
        mime: media_mime(&extension).map(str::to_string),
        kind: media_kind(&extension).to_string(),
        timestamp_ms: entry.get("timestampMs").and_then(Value::as_f64).unwrap_or(0.0),
        width,
        height,
        size_bytes,
        file_name,
    })
}

fn whole_media_dimension(value: Option<&Value>) -> Option<i64> {
    let value = value?.as_f64()?;
    value.is_finite().then_some((value + 0.5).floor() as i64)
}

fn media_basename(value: &str) -> String {
    let without_query = value.split(['?', '#']).next().unwrap_or(value);
    let base = without_query.rsplit(['/', '\\']).next().unwrap_or_default();
    percent_decode_utf8(base).unwrap_or_else(|| base.to_string())
}

fn percent_decode_utf8(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() { return None; }
            let hi = hex_value(bytes[index + 1])?;
            let lo = hex_value(bytes[index + 2])?;
            out.push((hi << 4) | lo);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn media_extension(file_name: &str) -> Option<String> {
    let base = file_name.rsplit(['/', '\\']).next().unwrap_or_default();
    let dot = base.rfind('.')?;
    if dot == 0 || dot + 1 >= base.len() { return None; }
    Some(base[dot + 1..].to_ascii_lowercase())
}

fn media_mime(extension: &str) -> Option<&'static str> {
    match extension {
        "avif" => Some("image/avif"), "bmp" => Some("image/bmp"), "gif" => Some("image/gif"),
        "ico" => Some("image/x-icon"), "jpeg" | "jpg" => Some("image/jpeg"), "png" => Some("image/png"),
        "svg" => Some("image/svg+xml"), "webp" => Some("image/webp"), "m4v" | "mp4" => Some("video/mp4"),
        "mov" => Some("video/quicktime"), "ogv" => Some("video/ogg"), "webm" => Some("video/webm"),
        "aac" => Some("audio/aac"), "flac" => Some("audio/flac"), "m4a" => Some("audio/mp4"),
        "mp3" => Some("audio/mpeg"), "oga" | "ogg" | "opus" => Some("audio/ogg"),
        "wav" => Some("audio/wav"), "weba" => Some("audio/webm"), _ => None,
    }
}

fn media_kind(extension: &str) -> &'static str {
    const IMAGES: &[&str] = &["avif", "bmp", "gif", "ico", "jpeg", "jpg", "png", "svg", "webp"];
    const VIDEOS: &[&str] = &["m4v", "mov", "mp4", "ogv", "webm"];
    const AUDIO: &[&str] = &["aac", "flac", "m4a", "mp3", "oga", "ogg", "opus", "wav", "weba"];
    const MARKDOWN: &[&str] = &["md", "markdown", "mdx"];
    const TABLES: &[&str] = &["csv", "tsv", "xlsx", "xls"];
    const JSON: &[&str] = &["json", "jsonc", "json5", "ndjson"];
    const ARCHIVES: &[&str] = &["zip", "tar", "gz", "tgz", "bz2", "tbz2", "xz", "txz", "zst", "7z", "rar"];
    const TEXT: &[&str] = &[
        "txt", "text", "log", "rst", "adoc", "tex", "xml", "yaml", "yml", "toml", "ini", "cfg", "conf", "env",
        "properties", "plist", "gradle", "html", "htm", "css", "scss", "sass", "less", "js", "jsx", "mjs", "cjs",
        "ts", "tsx", "mts", "cts", "py", "pyi", "rb", "go", "rs", "java", "kt", "kts", "c", "h", "cc", "cpp",
        "cxx", "hpp", "hh", "cs", "php", "swift", "scala", "dart", "lua", "pl", "pm", "r", "sql", "graphql", "gql",
        "proto", "vue", "svelte", "astro", "sh", "bash", "zsh", "fish", "bat", "ps1", "tf", "tfvars", "dockerfile", "diff", "patch",
    ];
    if IMAGES.contains(&extension) { "image" }
    else if VIDEOS.contains(&extension) { "video" }
    else if AUDIO.contains(&extension) { "audio" }
    else if extension == "pdf" { "pdf" }
    else if TABLES.contains(&extension) { "table" }
    else if JSON.contains(&extension) { "json" }
    else if MARKDOWN.contains(&extension) { "markdown" }
    else if extension == "docx" { "document" }
    else if TEXT.contains(&extension) { "text" }
    else if ARCHIVES.contains(&extension) { "archive" }
    else { "file" }
}
fn normalize_media_search_text(value: &str) -> String {
    value.nfkd().filter(|ch| !is_combining_mark(*ch)).flat_map(char::to_lowercase).collect()
}

fn media_name_matches_terms(file_name: &str, terms: &[String]) -> bool {
    if terms.is_empty() { return true; }
    let normalized = normalize_media_search_text(file_name);
    let tokens = normalized.split(|ch: char| !ch.is_alphanumeric()).filter(|token| !token.is_empty()).collect::<Vec<_>>();
    terms.iter().all(|term| tokens.iter().any(|token| token.starts_with(term)))
}
fn project_content_search_entry(entry: &Value) -> Option<SearchableTranscriptEntry> {
    let id = entry.get("id")?.as_str()?.trim();
    if id.is_empty() {
        return None;
    }
    let hidden_outbound_agent_peer_message = entry
        .get("hiddenOutboundAgentPeerMessage")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || entry
            .get("toAgent")
            .and_then(|value| value.get("kind"))
            .and_then(Value::as_str)
            == Some("agent");
    let kind = match entry.get("kind").and_then(Value::as_str).unwrap_or_default() {
        "message" => SearchableTranscriptEntryKind::Message {
            content: entry
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            role: if entry.get("role").and_then(Value::as_str) == Some("user") {
                AgentContentRole::User
            } else {
                AgentContentRole::Assistant
            },
        },
        "send-message" => SearchableTranscriptEntryKind::SendMessage {
            message_type: entry
                .get("message")
                .and_then(|message| message.get("type"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            content: entry
                .get("message")
                .and_then(|message| message.get("content"))
                .and_then(Value::as_str)
                .map(str::to_string),
        },
        "notice" => SearchableTranscriptEntryKind::Notice {
            text: entry
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        },
        _ => SearchableTranscriptEntryKind::Other,
    };
    Some(SearchableTranscriptEntry {
        id: id.to_string(),
        kind,
        timestamp_ms: entry.get("timestampMs").and_then(Value::as_f64),
        hidden_outbound_agent_peer_message,
    })
}

fn validate_entries(entries: &[Value]) -> io::Result<()> {
    let mut ids = HashSet::with_capacity(entries.len());
    for entry in entries {
        let id = entry_id(entry)?;
        if !ids.insert(id.to_string()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("duplicate transcript entry id: {id}"),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn page_filters_non_main_entries_and_applies_time_bounds() {
        let root = tempfile::tempdir().unwrap();
        let mut store = TranscriptStore::open(root.path().join("transcript.json")).unwrap();
        for entry in [
            serde_json::json!({"id":"old","agentId":"agent-a","kind":"message","role":"user","content":"old","timestampMs":10}),
            serde_json::json!({"id":"tool","agentId":"agent-a","kind":"tool-call","name":"x","timestampMs":20}),
            serde_json::json!({"id":"assistant","agentId":"agent-a","kind":"message","role":"assistant","content":"hidden","timestampMs":30}),
            serde_json::json!({"id":"branched","agentId":"agent-a","kind":"message","role":"user","content":"branch","branched":true,"timestampMs":40}),
            serde_json::json!({"id":"peer","agentId":"agent-a","kind":"message","role":"assistant","content":"peer","fromAgent":{"id":"agent-b"},"timestampMs":50}),
            serde_json::json!({"id":"attachment","agentId":"agent-a","kind":"user-attachment","filePath":"/tmp/a","timestampMs":60}),
            serde_json::json!({"id":"future","agentId":"agent-a","kind":"send-message","message":{"type":"text","content":"future"},"timestampMs":90}),
        ] { store.append_entry(entry).unwrap(); }
        let (page, next) = store.page_for_agent_with_bounds("agent-a", None, Some(40.0), Some(70.0), 10).unwrap();
        assert_eq!(page.iter().map(|entry| entry["id"].as_str().unwrap()).collect::<Vec<_>>(), vec!["peer", "attachment"]);
        assert_eq!(next, None);
    }

    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path() -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join(format!("fabushi-transcript-{}-{stamp}", std::process::id()))
            .join("transcript.json")
    }

    #[test]
    fn agent_scoped_pages_use_exclusive_cursor_and_survive_reopen() {
        let path = temp_path();
        let mut store = TranscriptStore::open(&path).unwrap();
        for (id, agent_id) in [
            ("a-1", "agent-a"),
            ("b-1", "agent-b"),
            ("a-2", "agent-a"),
            ("a-3", "agent-a"),
            ("b-2", "agent-b"),
            ("a-4", "agent-a"),
            ("a-5", "agent-a"),
        ] {
            store
                .append_entry(json!({"id":id,"agentId":agent_id,"kind":"message","role":"user","content":id}))
                .unwrap();
        }

        let (latest, cursor) = store.page_for_agent("agent-a", None, 2).unwrap();
        assert_eq!(
            latest.iter().map(|entry| entry["id"].as_str().unwrap()).collect::<Vec<_>>(),
            vec!["a-4", "a-5"],
        );
        assert_eq!(cursor, Some(3));

        store
            .append_entry(json!({"id":"a-6","agentId":"agent-a","kind":"message","content":"a-6"}))
            .unwrap();
        let (older, next) = store.page_for_agent("agent-a", cursor, 2).unwrap();
        assert_eq!(
            older.iter().map(|entry| entry["id"].as_str().unwrap()).collect::<Vec<_>>(),
            vec!["a-2", "a-3"],
            "exclusive cursor must not be shifted by newer appends",
        );
        assert_eq!(next, Some(1));

        drop(store);
        let reopened = TranscriptStore::open(&path).unwrap();
        let (oldest, exhausted) = reopened.page_for_agent("agent-a", next, 2).unwrap();
        assert_eq!(
            oldest.iter().map(|entry| entry["id"].as_str().unwrap()).collect::<Vec<_>>(),
            vec!["a-1"],
        );
        assert_eq!(exhausted, None);
        assert!(reopened.page_for_agent("agent-b", None, 0).is_err());
        assert!(reopened.page_for_agent("agent-b", None, 5_000).is_ok());
        assert!(reopened.page_for_agent("agent-b", None, 5_001).is_err());
        assert!(reopened.page_for_agent("", None, 2).is_err());

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn content_search_is_canonical_agent_scoped_bounded_and_peer_safe() {
        let path = temp_path();
        let mut store = TranscriptStore::open(&path).unwrap();
        for index in 0..7 {
            store
                .append_entry(json!({
                    "id": format!("a-{index}"),
                    "agentId": "agent-a",
                    "kind": "message",
                    "role": if index % 2 == 0 { "user" } else { "assistant" },
                    "content": format!("  Need\nProject Alpha result {index}  "),
                    "timestampMs": index as f64,
                }))
                .unwrap();
        }
        store
            .append_entry(json!({
                "id":"peer-hidden",
                "agentId":"agent-b",
                "kind":"message",
                "role":"assistant",
                "content":"Project Alpha secret peer copy",
                "toAgent":{"kind":"agent","id":"agent-a"},
            }))
            .unwrap();
        store
            .append_entry(json!({
                "id":"notice",
                "agentId":"agent-b",
                "kind":"notice",
                "text":"Project Alpha deployment ready",
                "timestampMs":99,
            }))
            .unwrap();

        let matches = store.search_content("project   alpha", 50).unwrap();
        assert_eq!(matches.len(), 6, "per-agent cap plus one notice");
        assert_eq!(matches[0].entry_id, "notice");
        assert!(!matches.iter().any(|entry| entry.entry_id == "peer-hidden"));
        assert_eq!(
            matches.iter().filter(|entry| entry.agent_id == "agent-a").count(),
            CONTENT_SEARCH_MAX_MATCHES_PER_AGENT,
        );
        assert!(matches.iter().all(|entry| entry.snippet.contains("Project Alpha")));
        assert!(store.search_content("", 50).unwrap().is_empty());
        assert!(store.search_content("alpha", 0).is_err());
        assert!(store.search_content("alpha", 51).is_err());

        drop(store);
        let reopened = TranscriptStore::open(&path).unwrap();
        assert_eq!(reopened.search_content("project alpha", 50).unwrap().len(), 6);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn media_search_projects_canonical_attachments_and_survives_reopen() {
        let path = temp_path();
        let mut store = TranscriptStore::open(&path).unwrap();
        store.append_entry(json!({
            "id":"old-image","agentId":"agent-a","kind":"user-attachment",
            "filePath":"content://files/Próject%20Alpha.png","timestampMs":5,
            "width":640.4,"height":480.6
        })).unwrap();
        store.append_entry(json!({
            "id":"new-doc","agentId":"agent-b","kind":"send-message",
            "message":{"type":"attachment","url":"https://example.invalid/reports/Project-Alpha.pdf?download=1"},
            "timestampMs":9
        })).unwrap();
        store.append_entry(json!({
            "id":"not-media","agentId":"agent-a","kind":"message","content":"Project Alpha","timestampMs":10
        })).unwrap();
        store.append_entry(json!({"id":"icon","agentId":"agent-c","kind":"user-attachment","filePath":"content://files/favicon.ico","timestampMs":3})).unwrap();
        store.append_entry(json!({"id":"audio","agentId":"agent-c","kind":"user-attachment","filePath":"content://files/voice.weba","timestampMs":2})).unwrap();

        let all = store.search_media("", 50).unwrap();
        assert_eq!(all.len(), 4);
        assert_eq!(all[0].entry_id, "new-doc");
        assert_eq!(all[0].file_name, "Project-Alpha.pdf");
        assert_eq!(all[0].path, "https://example.invalid/reports/Project-Alpha.pdf?download=1");
        assert_eq!(all[0].kind, "pdf");
        assert_eq!(all[1].file_name, "Próject Alpha.png");
        assert_eq!(all[1].path, "content://files/Próject%20Alpha.png");
        assert_eq!(all[1].mime.as_deref(), Some("image/png"));
        assert_eq!(all[1].width, Some(640));
        assert_eq!(all[1].height, Some(481));
        let icon = all.iter().find(|item| item.entry_id == "icon").unwrap();
        assert_eq!(icon.mime.as_deref(), Some("image/x-icon"));
        assert_eq!(icon.kind, "image");
        let audio = all.iter().find(|item| item.entry_id == "audio").unwrap();
        assert_eq!(audio.mime.as_deref(), Some("audio/webm"));
        assert_eq!(audio.kind, "audio");
        assert_eq!(store.search_media("proj alp", 50).unwrap().len(), 2);
        assert_eq!(store.search_media("project alpha", 50).unwrap().len(), 2, "Desktop FTS removes diacritics for media filename matching");
        assert!(store.search_media("missing", 50).unwrap().is_empty());
        assert!(store.search_media("", 0).is_err());
        assert!(store.search_media("", 51).is_err());

        drop(store);
        let reopened = TranscriptStore::open(&path).unwrap();
        assert_eq!(reopened.search_media("project alpha", 50).unwrap().len(), 2);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn recovering_open_promotes_valid_interrupted_write_when_canonical_is_missing() {
        let path = temp_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let temporary = temporary_path(&path);
        fs::write(&temporary, r#"[{"id":"recovered","agentId":"agent-a","kind":"message"}]"#).unwrap();

        let (store, recovery) = TranscriptStore::open_recovering(&path).unwrap();

        assert_eq!(
            recovery.unwrap().outcome,
            TranscriptRecoveryOutcome::RecoveredInterruptedWrite
        );
        assert_eq!(store.get_transcript()[0]["id"], "recovered");
        assert!(!temporary.exists());
        assert_eq!(TranscriptStore::open(&path).unwrap().get_transcript()[0]["id"], "recovered");
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn recovering_open_quarantines_corruption_and_prefers_valid_interrupted_write() {
        let path = temp_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{not-json").unwrap();
        let temporary = temporary_path(&path);
        fs::write(&temporary, r#"[{"id":"from-temp","agentId":"agent-a","kind":"message"}]"#).unwrap();

        let (store, recovery) = TranscriptStore::open_recovering(&path).unwrap();
        let recovery = recovery.unwrap();

        assert_eq!(recovery.outcome, TranscriptRecoveryOutcome::RecoveredInterruptedWrite);
        let quarantine = recovery.quarantine_path.unwrap();
        assert!(quarantine.exists());
        assert_eq!(fs::read_to_string(&quarantine).unwrap(), "{not-json");
        assert_eq!(store.get_transcript()[0]["id"], "from-temp");
        assert!(!temporary.exists());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn duplicate_entry_ids_are_never_accepted_as_canonical_transcript() {
        let path = temp_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            r#"[
                {"id":"duplicate","agentId":"agent-a","kind":"message"},
                {"id":"duplicate","agentId":"agent-a","kind":"message"}
            ]"#,
        )
        .unwrap();

        assert_eq!(
            TranscriptStore::open(&path).unwrap_err().kind(),
            io::ErrorKind::InvalidData,
        );

        let (store, recovery) = TranscriptStore::open_recovering(&path).unwrap();
        let recovery = recovery.expect("duplicate ids must be treated as canonical corruption");
        assert_eq!(recovery.outcome, TranscriptRecoveryOutcome::RecoveredSalvage);
        assert!(recovery.quarantine_path.unwrap().exists());
        assert_eq!(store.get_transcript().len(), 1);
        assert_eq!(store.get_transcript()[0]["id"], "duplicate");

        let mut store = TranscriptStore::open(&path).unwrap();
        store
            .append_entry(json!({"id":"one","agentId":"agent-a","kind":"message"}))
            .unwrap();
        let error = store
            .append_entry(json!({"id":"one","agentId":"agent-a","kind":"message"}))
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(store.get_transcript().len(), 1);

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn recovering_open_uses_last_known_good_backup_before_resetting_corruption() {
        let path = temp_path();
        let mut store = TranscriptStore::open(&path).unwrap();
        store
            .append_entry(json!({"id":"first","agentId":"agent-a","kind":"message","content":"first"}))
            .unwrap();
        store
            .append_entry(json!({"id":"second","agentId":"agent-a","kind":"message","content":"second"}))
            .unwrap();
        assert!(backup_path(&path).exists(), "second commit must retain the prior valid snapshot");

        fs::write(&path, "{broken-canonical").unwrap();
        fs::write(temporary_path(&path), "{broken-temp").unwrap();

        let (recovered, recovery) = TranscriptStore::open_recovering(&path).unwrap();
        let recovery = recovery.unwrap();
        assert_eq!(recovery.outcome, TranscriptRecoveryOutcome::RecoveredInterruptedWrite);
        assert!(recovery.quarantine_path.unwrap().exists());
        assert_eq!(recovered.get_transcript().len(), 1);
        assert_eq!(recovered.get_transcript()[0]["id"], "first");
        assert_eq!(TranscriptStore::open(&path).unwrap().get_transcript()[0]["id"], "first");
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn recovering_open_salvages_valid_entries_from_partially_corrupt_store() {
        let path = temp_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            r#"[{"id":"keep-a","kind":"message","content":"a"},{"kind":"message","content":"broken"},{"id":"keep-b","kind":"message","content":"b"}]"#,
        )
        .unwrap();

        let (store, recovery) = TranscriptStore::open_recovering(&path).unwrap();
        let recovery = recovery.unwrap();

        assert_eq!(recovery.outcome, TranscriptRecoveryOutcome::RecoveredSalvage);
        assert!(recovery.quarantine_path.unwrap().exists());
        assert_eq!(
            store
                .get_transcript()
                .iter()
                .map(|entry| entry["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["keep-a", "keep-b"],
        );
        assert_eq!(TranscriptStore::open(&path).unwrap().get_transcript().len(), 2);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn recovering_open_salvages_complete_prefix_from_truncated_store_and_deduplicates_ids() {
        let path = temp_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            r#"[{"id":"keep-a","kind":"message","content":"brace } in string"},{"id":"same","kind":"notice"},{"id":"same","kind":"notice","text":"duplicate"},{"kind":"message"},{"id":"keep-b","kind":"message"},{"id":"partial""#,
        )
        .unwrap();

        let (store, recovery) = TranscriptStore::open_recovering(&path).unwrap();
        let recovery = recovery.unwrap();

        assert_eq!(recovery.outcome, TranscriptRecoveryOutcome::RecoveredSalvage);
        assert!(recovery.quarantine_path.unwrap().exists());
        assert_eq!(
            store
                .get_transcript()
                .iter()
                .map(|entry| entry["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["keep-a", "same", "keep-b"],
        );
        assert_eq!(TranscriptStore::open(&path).unwrap().get_transcript().len(), 3);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn recovering_open_quarantines_and_resets_unrecoverable_corruption() {
        let path = temp_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, r#"{"unexpected":"object"}"#).unwrap();
        fs::write(temporary_path(&path), "also-corrupt").unwrap();

        let (store, recovery) = TranscriptStore::open_recovering(&path).unwrap();
        let recovery = recovery.unwrap();

        assert_eq!(recovery.outcome, TranscriptRecoveryOutcome::ResetCorruptStore);
        let quarantine = recovery.quarantine_path.unwrap();
        assert!(quarantine.exists());
        assert_eq!(fs::read_to_string(&quarantine).unwrap(), r#"{"unexpected":"object"}"#);
        assert!(store.get_transcript().is_empty());
        assert!(TranscriptStore::open(&path).unwrap().get_transcript().is_empty());
        assert!(!temporary_path(&path).exists());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn client_nonce_lookup_is_agent_and_account_fenced() {
        let path = temp_path();
        let mut store = TranscriptStore::open(&path).unwrap();
        store
            .append_entry(json!({
                "id":"request-a",
                "kind":"message",
                "role":"user",
                "agentId":"agent-a",
                "accountFence":"account-a",
                "clientNonce":"nonce-a",
                "content":"hello"
            }))
            .unwrap();
        assert_eq!(
            store
                .user_entry_by_client_nonce("agent-a", "nonce-a", "account-a")
                .unwrap()["id"],
            "request-a"
        );
        assert!(store
            .user_entry_by_client_nonce("agent-b", "nonce-a", "account-a")
            .is_none());
        assert!(store
            .user_entry_by_client_nonce("agent-a", "nonce-a", "account-b")
            .is_none());
        drop(store);
        let reopened = TranscriptStore::open(&path).unwrap();
        assert!(reopened
            .user_entry_by_client_nonce("agent-a", "nonce-a", "account-a")
            .is_some());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn append_update_remove_clear_follow_in_memory_contract_and_survive_reopen() {
        let path = temp_path();
        let mut store = TranscriptStore::open(&path).unwrap();
        assert!(store.get_transcript().is_empty());

        assert!(store
            .append_entry_if_absent(json!({"id":"u1","kind":"message","role":"user","content":"hi"}))
            .unwrap());
        assert!(!store
            .append_entry_if_absent(json!({"id":"u1","kind":"message","role":"user","content":"duplicate"}))
            .unwrap());
        store
            .append_entry(json!({"id":"a1","kind":"message","role":"assistant","content":"hello"}))
            .unwrap();

        drop(store);
        let mut reopened = TranscriptStore::open(&path).unwrap();
        assert_eq!(reopened.get_transcript().len(), 2);
        let updated = reopened
            .update_entry("a1", |entry| {
                let mut entry = entry.clone();
                entry["content"] = Value::String("updated".into());
                entry
            })
            .unwrap()
            .unwrap();
        assert_eq!(updated["content"], "updated");
        assert!(reopened.remove_entry("u1").unwrap());
        assert!(!reopened.remove_entry("missing").unwrap());
        reopened.clear_transcript().unwrap();
        assert!(TranscriptStore::open(&path).unwrap().get_transcript().is_empty());

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
