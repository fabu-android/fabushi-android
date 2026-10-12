pub const AGENT_CONTENT_SEARCH_MAX_MATCHES_PER_AGENT: usize = 5;
pub const AGENT_CONTENT_SEARCH_MAX_RESULTS: usize = 50;

const SNIPPET_LEAD: usize = 30;
const SNIPPET_TRAIL: usize = 60;
const ELLIPSIS: &str = "…";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq)]
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

fn find_normalized_query(
    flat_chars: &[char],
    normalized_query: &str,
) -> Option<(usize, usize)> {
    if normalized_query.is_empty() {
        return None;
    }

    let query = normalized_query.chars().collect::<Vec<_>>();
    if query.is_empty() {
        return None;
    }

    // JavaScript lower-cases the flattened text before indexOf(). Preserve a
    // map back to the original flattened character positions so the returned
    // snippet keeps original casing even when Unicode lower-casing expands.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn message(id: &str, content: &str, role: AgentContentRole, timestamp_ms: f64) -> SearchableTranscriptEntry {
        SearchableTranscriptEntry {
            id: id.into(),
            kind: SearchableTranscriptEntryKind::Message { content: content.into(), role },
            timestamp_ms: Some(timestamp_ms),
            hidden_outbound_agent_peer_message: false,
        }
    }

    #[test]
    fn search_matches_desktop_whitespace_case_recent_order_and_limit() {
        let entries = vec![
            message("old", "  Alpha\n   BETA context  ", AgentContentRole::User, 1.0),
            message("new", "alpha beta latest", AgentContentRole::Assistant, 2.0),
        ];
        let matches = find_agent_content_matches(&entries, "alpha beta", 1);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].entry_id, "new");
        assert_eq!(matches[0].role, AgentContentRole::Assistant);
        assert_eq!(build_content_snippet("  Alpha\n   BETA context  ", "alpha beta"), Some("Alpha BETA context".into()));
    }

    #[test]
    fn hidden_agent_peer_rows_and_non_text_send_messages_are_excluded() {
        let entries = vec![
            SearchableTranscriptEntry {
                id: "hidden".into(),
                kind: SearchableTranscriptEntryKind::Message {
                    content: "secret alpha".into(),
                    role: AgentContentRole::Assistant,
                },
                timestamp_ms: Some(3.0),
                hidden_outbound_agent_peer_message: true,
            },
            SearchableTranscriptEntry {
                id: "attachment".into(),
                kind: SearchableTranscriptEntryKind::SendMessage {
                    message_type: "attachment".into(),
                    content: Some("alpha.pdf".into()),
                },
                timestamp_ms: Some(2.0),
                hidden_outbound_agent_peer_message: false,
            },
            SearchableTranscriptEntry {
                id: "text".into(),
                kind: SearchableTranscriptEntryKind::SendMessage {
                    message_type: "text".into(),
                    content: Some("visible alpha".into()),
                },
                timestamp_ms: Some(1.0),
                hidden_outbound_agent_peer_message: false,
            },
        ];
        assert_eq!(
            find_agent_content_matches_default(&entries, "alpha")
                .into_iter()
                .map(|item| item.entry_id)
                .collect::<Vec<_>>(),
            vec!["text"],
        );
    }
}
