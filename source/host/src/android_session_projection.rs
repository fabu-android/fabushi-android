use serde_json::{json, Value};
use std::collections::BTreeMap;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastMessage {
    pub id: String,
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AndroidSessionProjection {
    pub last_entry: Option<Value>,
    pub last_message: Option<LastMessage>,
    pub newest_entry_id: Option<String>,
    pub updated_at_ms: u64,
}

pub fn project_agent_session(entries: &[Value]) -> AndroidSessionProjection {
    AndroidSessionProjection {
        last_entry: get_last_entry_from_transcript(entries),
        last_message: get_last_message_from_transcript(entries),
        newest_entry_id: entries
            .iter()
            .rev()
            .find_map(|entry| entry.get("id").and_then(Value::as_str))
            .map(ToOwned::to_owned),
        updated_at_ms: entries
            .iter()
            .filter_map(|entry| entry.get("timestampMs").and_then(Value::as_u64))
            .max()
            .unwrap_or_default(),
    }
}

pub fn is_https_url(raw: &str) -> bool {
    Url::parse(raw)
        .map(|url| url.scheme().eq_ignore_ascii_case("https"))
        .unwrap_or(false)
}

pub fn is_link_attachment_message(message: &Value) -> bool {
    message.get("type").and_then(Value::as_str) == Some("attachment")
        && message
            .get("url")
            .and_then(Value::as_str)
            .is_some_and(is_https_url)
        && message
            .get("file_name")
            .or_else(|| message.get("fileName"))
            .map(|value| value.is_null())
            .unwrap_or(true)
}

pub fn classify_attachment(file_name: Option<&str>, url_or_path: Option<&str>) -> &'static str {
    let text = format!(
        "{} {}",
        file_name.unwrap_or_default(),
        url_or_path.unwrap_or_default()
    )
    .to_ascii_lowercase();
    if has_media_extension(&text, &["png", "jpg", "jpeg", "gif", "webp", "svg"]) {
        "image"
    } else if has_media_extension(&text, &["mp4", "mov", "webm", "mkv"]) {
        "video"
    } else if has_media_extension(&text, &["mp3", "wav", "m4a", "ogg"]) {
        "audio"
    } else if has_media_extension(
        &text,
        &["pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "md"],
    ) {
        "document"
    } else {
        "other"
    }
}

fn has_media_extension(text: &str, extensions: &[&str]) -> bool {
    extensions.iter().any(|extension| {
        let needle = format!(".{extension}");
        text.match_indices(&needle).any(|(index, _)| {
            let tail = &text[index + needle.len()..];
            tail.is_empty()
                || tail.starts_with('?')
                || tail.starts_with('#')
                || tail.chars().next().is_some_and(char::is_whitespace)
        })
    })
}

fn batch_id(entry: &Value) -> Option<&str> {
    entry.get("batchId").and_then(Value::as_str)
}

fn same_batch(candidate: Option<&str>, anchor: Option<&str>) -> bool {
    anchor.is_some_and(|anchor| !anchor.is_empty() && candidate == Some(anchor))
}

pub fn collect_last_attachment_batch_kinds(entries: &[Value], index: usize) -> Vec<String> {
    let Some(last) = entries.get(index) else {
        return Vec::new();
    };
    let mut kinds = Vec::new();
    if last.get("kind").and_then(Value::as_str) == Some("user-attachment") {
        for i in (0..=index).rev() {
            let entry = &entries[i];
            if entry.get("kind").and_then(Value::as_str) != Some("user-attachment")
                || (i != index && !same_batch(batch_id(entry), batch_id(last)))
            {
                break;
            }
            kinds.push(
                classify_attachment(
                    entry.get("file_name").and_then(Value::as_str),
                    entry.get("file_path").and_then(Value::as_str),
                )
                .to_string(),
            );
        }
    } else if last.get("kind").and_then(Value::as_str) == Some("send-message")
        && last
            .get("message")
            .and_then(Value::as_object)
            .and_then(|message| message.get("type"))
            .and_then(Value::as_str)
            == Some("attachment")
    {
        for i in (0..=index).rev() {
            let entry = &entries[i];
            let Some(message) = entry.get("message") else {
                break;
            };
            if entry.get("kind").and_then(Value::as_str) != Some("send-message")
                || message.get("type").and_then(Value::as_str) != Some("attachment")
                || is_link_attachment_message(message)
                || (i != index && !same_batch(batch_id(entry), batch_id(last)))
            {
                break;
            }
            kinds.push(
                classify_attachment(
                    message
                        .get("file_name")
                        .or_else(|| message.get("fileName"))
                        .and_then(Value::as_str),
                    message.get("url").and_then(Value::as_str),
                )
                .to_string(),
            );
        }
    }
    kinds.reverse();
    kinds
}

pub fn build_attachment_last_entry(entries: &[Value], index: usize) -> Value {
    let kinds = collect_last_attachment_batch_kinds(entries, index);
    let mut counts = BTreeMap::<String, usize>::new();
    for kind in &kinds {
        *counts.entry(kind.clone()).or_default() += 1;
    }
    json!({"kind":"attachment","count":kinds.len(),"kinds":counts})
}

fn preview_text(raw: Option<&Value>) -> String {
    let raw = match raw {
        Some(Value::String(value)) => value.clone(),
        Some(value) if !value.is_null() => value.to_string(),
        _ => String::new(),
    };
    let mut normalized = String::new();
    let mut in_whitespace = false;
    for ch in raw.chars() {
        if ch.is_whitespace() {
            if !in_whitespace {
                normalized.push(' ');
                in_whitespace = true;
            }
            continue;
        }
        in_whitespace = false;
        if matches!(ch, '*' | '_' | '#' | '>') || ch == '`' {
            continue;
        }
        normalized.push(ch);
    }
    normalized.trim().chars().take(280).collect()
}

pub fn last_message_preview_text(message: &Value) -> String {
    let Some(object) = message.as_object() else {
        return String::new();
    };
    match object.get("type").and_then(Value::as_str) {
        Some("text") => preview_text(object.get("content")),
        Some("attachment") if is_link_attachment_message(message) => format!(
            "Sent a link · {}",
            object.get("url").and_then(Value::as_str).unwrap_or_default()
        ),
        Some("attachment") => format!(
            "Sent an attachment · {}",
            classify_attachment(
                object
                    .get("file_name")
                    .or_else(|| object.get("fileName"))
                    .and_then(Value::as_str),
                object.get("url").and_then(Value::as_str),
            )
        ),
        _ if object.get("title").and_then(Value::as_str).is_some() => preview_text(object.get("title")),
        _ if object.get("label").and_then(Value::as_str).is_some() => preview_text(object.get("label")),
        _ => preview_text(object.get("type")),
    }
}

pub fn get_last_message_from_transcript(entries: &[Value]) -> Option<LastMessage> {
    for entry in entries.iter().rev() {
        if entry.get("hidden").and_then(Value::as_bool) == Some(true)
            || entry.get("branched").and_then(Value::as_bool) == Some(true)
            || entry.get("peerAgentId").is_some_and(|value| !value.is_null())
        {
            continue;
        }
        let Some(id) = entry.get("id").and_then(Value::as_str) else {
            continue;
        };
        match entry.get("kind").and_then(Value::as_str) {
            Some("send-message") => {
                let Some(message) = entry.get("message") else {
                    continue;
                };
                if message.is_object() {
                    return Some(LastMessage {
                        id: id.to_string(),
                        preview: last_message_preview_text(message),
                    });
                }
            }
            Some("message") => {
                if let Some(content) = entry.get("content") {
                    return Some(LastMessage {
                        id: id.to_string(),
                        preview: preview_text(Some(content)),
                    });
                }
            }
            _ => {}
        }
    }
    None
}

pub fn get_last_entry_from_transcript(entries: &[Value]) -> Option<Value> {
    let visible = entries
        .iter()
        .filter(|entry| entry.get("hidden").and_then(Value::as_bool) != Some(true))
        .filter(|entry| entry.get("branched").and_then(Value::as_bool) != Some(true))
        .cloned()
        .collect::<Vec<_>>();
    for (index, entry) in visible.iter().enumerate().rev() {
        if entry.get("peerAgentId").is_some_and(|value| !value.is_null()) {
            continue;
        }
        match entry.get("kind").and_then(Value::as_str) {
            Some("send-message") => {
                let Some(message) = entry.get("message") else {
                    continue;
                };
                match message.get("type").and_then(Value::as_str) {
                    Some("text") => return Some(json!({
                        "kind":"text",
                        "text":message.get("content").and_then(Value::as_str).unwrap_or_default()
                    })),
                    Some("widget") => {
                        let prompt = message
                            .get("widget")
                            .and_then(|widget| widget.get("prompt"))
                            .and_then(Value::as_str)
                            .unwrap_or("Question");
                        let text = entry
                            .get("respondedValue")
                            .filter(|value| !value.is_null())
                            .map(|value| format!("{prompt} — {}", value_as_display(value)))
                            .unwrap_or_else(|| prompt.to_string());
                        return Some(json!({"kind":"text","text":text}));
                    }
                    Some("attachment") if is_link_attachment_message(message) => return Some(json!({
                        "kind":"link",
                        "url":message.get("url").and_then(Value::as_str).unwrap_or_default()
                    })),
                    Some("attachment") => return Some(build_attachment_last_entry(&visible, index)),
                    _ => return Some(json!({"kind":"text","text":last_message_preview_text(message)})),
                }
            }
            Some("message") => {
                if let Some(content) = entry
                    .get("content")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                {
                    return Some(json!({"kind":"text","text":content}));
                }
            }
            Some("user-attachment") => return Some(build_attachment_last_entry(&visible, index)),
            _ => {}
        }
    }
    None
}

fn value_as_display(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_android_and_desktop_message_shapes_with_visibility_fencing() {
        let entries = vec![
            json!({"id":"m1","kind":"message","content":"**hello**   world","timestampMs":10}),
            json!({"id":"hidden","kind":"message","content":"secret","hidden":true,"timestampMs":11}),
            json!({"id":"peer","kind":"message","content":"peer","peerAgentId":"agent-b","timestampMs":12}),
        ];
        let projected = project_agent_session(&entries);
        assert_eq!(
            projected.last_message,
            Some(LastMessage { id:"m1".into(), preview:"hello world".into() })
        );
        assert_eq!(
            projected.last_entry,
            Some(json!({"kind":"text","text":"**hello**   world"}))
        );
        assert_eq!(projected.newest_entry_id.as_deref(), Some("peer"));
        assert_eq!(projected.updated_at_ms, 12);
    }

    #[test]
    fn attachment_batches_match_desktop_projection_contract() {
        let entries = vec![
            json!({"id":"a1","kind":"user-attachment","file_name":"photo.png","file_path":"/tmp/photo.png","batchId":"b"}),
            json!({"id":"a2","kind":"user-attachment","file_name":"notes.pdf","file_path":"/tmp/notes.pdf","batchId":"b"}),
        ];
        assert_eq!(
            get_last_entry_from_transcript(&entries),
            Some(json!({"kind":"attachment","count":2,"kinds":{"document":1,"image":1}}))
        );
        assert_eq!(
            classify_attachment(None, Some("https://example.test/movie.mp4?download=1")),
            "video"
        );
    }

    #[test]
    fn link_and_preview_contracts_are_bounded() {
        let link = json!({"type":"attachment","url":"https://example.test/path"});
        assert!(is_link_attachment_message(&link));
        assert_eq!(
            last_message_preview_text(&link),
            "Sent a link · https://example.test/path"
        );
        let long = "x".repeat(400);
        let text = json!({"type":"text","content":long});
        assert_eq!(last_message_preview_text(&text).chars().count(), 280);
    }
}
