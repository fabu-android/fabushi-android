use serde::Serialize;
use serde_json::Value;

pub const AGENT_CONTENT_SEARCH_MAX_MATCHES_PER_AGENT: usize = 5;
pub const AGENT_CONTENT_SEARCH_MAX_RESULTS: usize = 50;

const SNIPPET_LEAD: usize = 30;
const SNIPPET_TRAIL: usize = 60;
const ELLIPSIS: &str = "…";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentContentRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SearchableTranscriptEntryKind {
    Message {
        content: String,
        role: AgentContentRole,
    },
    SendMessage {
        message_type: String,
        content: Option<String>,
    },
    Notice {
        text: String,
    },
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchableTranscriptEntry {
    pub id: String,
    pub kind: SearchableTranscriptEntryKind,
    pub timestamp_ms: Option<f64>,
    pub hidden_outbound_agent_peer_message: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentContentMatch {
    pub entry_id: String,
    pub role: AgentContentRole,
    pub timestamp_ms: f64,
    pub snippet: String,
}

pub fn entry_search_text(entry: &SearchableTranscriptEntry) -> &str {
    match &entry.kind {
        SearchableTranscriptEntryKind::Message { content, .. } => content,
        SearchableTranscriptEntryKind::SendMessage {
            message_type,
            content,
        } if message_type == "text" => content.as_deref().unwrap_or(""),
        SearchableTranscriptEntryKind::Notice { text } => text,
        SearchableTranscriptEntryKind::SendMessage { .. }
        | SearchableTranscriptEntryKind::Other => "",
    }
}

fn flatten_whitespace(text: &str) -> String {
    let mut flat = String::new();
    let mut pending_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !flat.is_empty() {
                pending_space = true;
            }
            continue;
        }
        if pending_space {
            flat.push(' ');
            pending_space = false;
        }
        flat.push(ch);
    }
    flat
}

fn find_normalized_query(flat_chars: &[char], normalized_query: &str) -> Option<(usize, usize)> {
    if normalized_query.is_empty() {
        return None;
    }

    let query = normalized_query.chars().collect::<Vec<_>>();
    if query.is_empty() {
        return None;
    }

    let mut lowered = Vec::new();
    let mut source_index = Vec::new();
    for (index, ch) in flat_chars.iter().copied().enumerate() {
        for lower in ch.to_lowercase() {
            lowered.push(lower);
            source_index.push(index);
        }
    }

    if query.len() > lowered.len() {
        return None;
    }

    for start in 0..=lowered.len() - query.len() {
        if lowered[start..start + query.len()] == query[..] {
            let source_start = source_index[start];
            let source_end = source_index[start + query.len() - 1] + 1;
            return Some((source_start, source_end));
        }
    }
    None
}

pub fn build_content_snippet(text: &str, normalized_query: &str) -> Option<String> {
    if normalized_query.is_empty() {
        return None;
    }

    let flat = flatten_whitespace(text);
    let chars = flat.chars().collect::<Vec<_>>();
    let (match_start, match_end) = find_normalized_query(&chars, normalized_query)?;
    let start = match_start.saturating_sub(SNIPPET_LEAD);
    let end = chars.len().min(match_end.saturating_add(SNIPPET_TRAIL));

    let mut snippet = String::new();
    if start > 0 {
        snippet.push_str(ELLIPSIS);
    }
    snippet.extend(chars[start..end].iter());
    if end < chars.len() {
        snippet.push_str(ELLIPSIS);
    }
    Some(snippet)
}

pub fn find_agent_content_matches(
    entries: &[SearchableTranscriptEntry],
    normalized_query: &str,
    limit: usize,
) -> Vec<AgentContentMatch> {
    if normalized_query.is_empty() || limit == 0 {
        return Vec::new();
    }

    let mut matches = Vec::new();
    for entry in entries.iter().rev() {
        if matches.len() >= limit {
            break;
        }
        if entry.hidden_outbound_agent_peer_message {
            continue;
        }
        let Some(snippet) = build_content_snippet(entry_search_text(entry), normalized_query) else {
            continue;
        };
        let role = match entry.kind {
            SearchableTranscriptEntryKind::Message { role, .. } => role,
            _ => AgentContentRole::Assistant,
        };
        matches.push(AgentContentMatch {
            entry_id: entry.id.clone(),
            role,
            timestamp_ms: entry.timestamp_ms.unwrap_or(0.0),
            snippet,
        });
    }
    matches
}

pub fn find_agent_content_matches_default(
    entries: &[SearchableTranscriptEntry],
    normalized_query: &str,
) -> Vec<AgentContentMatch> {
    find_agent_content_matches(
        entries,
        normalized_query,
        AGENT_CONTENT_SEARCH_MAX_MATCHES_PER_AGENT,
    )
}

pub fn search_canonical_agent_transcript(
    entries: &[Value],
    agent_id: &str,
    query: &str,
    limit: usize,
) -> Vec<AgentContentMatch> {
    let agent_id = agent_id.trim();
    let normalized_query = query.trim().to_lowercase();
    if agent_id.is_empty() || normalized_query.is_empty() || limit == 0 {
        return Vec::new();
    }
    let limit = limit.min(AGENT_CONTENT_SEARCH_MAX_MATCHES_PER_AGENT);
    let projected = entries
        .iter()
        .filter(|entry| entry.get("agentId").and_then(Value::as_str) == Some(agent_id))
        .filter_map(project_entry)
        .collect::<Vec<_>>();
    find_agent_content_matches(&projected, &normalized_query, limit)
}

fn project_entry(entry: &Value) -> Option<SearchableTranscriptEntry> {
    let id = entry.get("id")?.as_str()?.trim();
    if id.is_empty() {
        return None;
    }
    let hidden = entry
        .get("hiddenOutboundAgentPeerMessage")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let timestamp_ms = entry.get("timestampMs").and_then(Value::as_f64);
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
        "sendMessage" | "send-message" | "send_message" => {
            SearchableTranscriptEntryKind::SendMessage {
                message_type: entry
                    .get("messageType")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                content: entry.get("content").and_then(Value::as_str).map(str::to_string),
            }
        }
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
        timestamp_ms,
        hidden_outbound_agent_peer_message: hidden,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonical_projection_preserves_desktop_match_semantics() {
        let entries = vec![
            json!({"id":"old","agentId":"a","kind":"message","role":"user","content":"  Hello\nWorld  ","timestampMs":1}),
            json!({"id":"hidden","agentId":"a","kind":"message","role":"assistant","content":"hello hidden","timestampMs":2,"hiddenOutboundAgentPeerMessage":true}),
            json!({"id":"other-agent","agentId":"b","kind":"message","role":"assistant","content":"hello other","timestampMs":3}),
            json!({"id":"new","agentId":"a","kind":"notice","text":"HELLO notice","timestampMs":4}),
        ];
        let found = search_canonical_agent_transcript(&entries, "a", "hello", 5);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].entry_id, "new");
        assert_eq!(found[0].role, AgentContentRole::Assistant);
        assert_eq!(found[1].entry_id, "old");
        assert_eq!(found[1].role, AgentContentRole::User);
        assert_eq!(found[1].snippet, "Hello World");
    }

    #[test]
    fn canonical_projection_caps_per_agent_and_supports_text_send_message_only() {
        let entries = (0..8)
            .map(|index| json!({
                "id":format!("m-{index}"),
                "agentId":"a",
                "kind":"sendMessage",
                "messageType":if index == 0 {"image"} else {"text"},
                "content":"needle",
                "timestampMs":index,
            }))
            .collect::<Vec<_>>();
        let found = search_canonical_agent_transcript(&entries, "a", "needle", 50);
        assert_eq!(found.len(), AGENT_CONTENT_SEARCH_MAX_MATCHES_PER_AGENT);
        assert_eq!(found[0].entry_id, "m-7");
        assert!(!found.iter().any(|entry| entry.entry_id == "m-0"));
    }

    #[test]
    fn unicode_lowercase_expansion_keeps_original_snippet_coordinates() {
        let snippet = build_content_snippet("İSTANBUL", "i̇stanbul").unwrap();
        assert_eq!(snippet, "İSTANBUL");
    }
}
