use serde::Serialize;
use serde_json::Value;

pub const REQUEST_SOURCES: &[&str] = &[
    "turn",
    "automation",
    "notification",
    "connector",
    "event",
    "handoff-resume",
    "background-revival",
    "web-search",
    "web-fetch",
];
pub const REQUEST_ID_HISTORY_MAX: usize = 200;
pub const REQUEST_ID_PROMPT_MAX: usize = 200;
pub const EPISODE_TURN_TEXT_CAP: usize = 2_000;
pub const EPISODE_PENDING_MAX: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandProfile {
    pub description: String,
    pub avatar_path: Option<String>,
}

impl Default for SandProfile {
    fn default() -> Self {
        Self {
            description: String::new(),
            avatar_path: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnreadState {
    pub last_activity_at: f64,
    pub last_viewed_at: f64,
    pub is_manually_unread: bool,
    pub unread_count: f64,
}

impl Default for UnreadState {
    fn default() -> Self {
        Self {
            last_activity_at: 0.0,
            last_viewed_at: 0.0,
            is_manually_unread: false,
            unread_count: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendGuardState {
    pub nudged_at_ms: Option<f64>,
    pub snoozed_until_ms: Option<f64>,
    pub opted_out: bool,
    pub card_entry_ids: Vec<String>,
    pub paused_automation_ids: Vec<String>,
}

impl Default for SpendGuardState {
    fn default() -> Self {
        Self {
            nudged_at_ms: None,
            snoozed_until_ms: None,
            opted_out: false,
            card_entry_ids: Vec::new(),
            paused_automation_ids: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AwaitingUserResponse {
    pub tab_id: String,
    pub reason: String,
    pub since: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RequestRecord {
    pub id: String,
    pub at: f64,
    pub prompt: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MemoryPromptSnapshot {
    pub render: String,
    pub compaction_epoch: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EpisodeTurn {
    pub ts: f64,
    pub user: String,
    pub agent: String,
}

fn parse_json(raw: Option<&str>) -> Option<Value> {
    serde_json::from_str(raw?).ok()
}

fn finite_number(value: Option<&Value>) -> f64 {
    value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .unwrap_or(0.0)
}

fn positive_number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value > 0.0)
}

fn non_empty_string_ids(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

pub fn is_valid_transcript_entry(entry: &Value) -> bool {
    let Some(value) = entry.as_object() else {
        return false;
    };
    match value.get("kind").and_then(Value::as_str) {
        Some("send-message") => {
            let Some(message) = value.get("message").and_then(Value::as_object) else {
                return false;
            };
            message.get("type").and_then(Value::as_str) != Some("text")
                || message.get("content").and_then(Value::as_str).is_some()
        }
        Some("message") => value.get("content").and_then(Value::as_str).is_some(),
        Some("user-attachment") => value.get("file_path").and_then(Value::as_str).is_some(),
        Some("tool-call") => value.get("name").and_then(Value::as_str).is_some(),
        Some("notice") => value.get("text").and_then(Value::as_str).is_some(),
        Some("event") => value
            .get("event")
            .and_then(Value::as_object)
            .and_then(|event| event.get("type"))
            .and_then(Value::as_str)
            .is_some(),
        _ => false,
    }
}

pub fn parse_transcript_entry(raw: &str) -> Option<Value> {
    let entry = serde_json::from_str::<Value>(raw).ok()?;
    if entry.get("kind").and_then(Value::as_str) == Some("error") {
        return None;
    }
    is_valid_transcript_entry(&entry).then_some(entry)
}

pub fn parse_profile(raw: Option<&str>) -> SandProfile {
    let Some(value) = parse_json(raw).and_then(|value| value.as_object().cloned()) else {
        return SandProfile::default();
    };
    let description = value
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let avatar_path = value
        .get("avatarPath")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    SandProfile {
        description,
        avatar_path,
    }
}

pub fn parse_unread_state(raw: Option<&str>) -> UnreadState {
    let Some(value) = parse_json(raw).and_then(|value| value.as_object().cloned()) else {
        return UnreadState::default();
    };
    let unread = finite_number(value.get("unreadCount"));
    UnreadState {
        last_activity_at: finite_number(value.get("lastActivityAt")),
        last_viewed_at: finite_number(value.get("lastViewedAt")),
        is_manually_unread: value.get("isManuallyUnread").and_then(Value::as_bool) == Some(true),
        unread_count: if unread > 0.0 { unread.floor() } else { 0.0 },
    }
}

fn legacy_positive_number(raw: Option<&str>) -> Option<f64> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }
    let parsed = raw.parse::<f64>().ok()?;
    (parsed.is_finite() && parsed > 0.0).then_some(parsed)
}

pub fn resolve_spend_guard_state(
    state: Option<&str>,
    legacy_nudged_at: Option<&str>,
) -> SpendGuardState {
    let Some(raw_state) = state else {
        return SpendGuardState {
            nudged_at_ms: legacy_positive_number(legacy_nudged_at),
            ..SpendGuardState::default()
        };
    };
    let Some(value) = parse_json(Some(raw_state)).and_then(|value| value.as_object().cloned()) else {
        return SpendGuardState::default();
    };
    SpendGuardState {
        nudged_at_ms: positive_number(value.get("nudgedAtMs")),
        snoozed_until_ms: positive_number(value.get("snoozedUntilMs")),
        opted_out: value.get("optedOut").and_then(Value::as_bool) == Some(true),
        card_entry_ids: non_empty_string_ids(value.get("cardEntryIds")),
        paused_automation_ids: non_empty_string_ids(value.get("pausedAutomationIds")),
    }
}

pub fn serialize_spend_guard_state(state: &SpendGuardState) -> Option<String> {
    if state.nudged_at_ms.is_none()
        && state.snoozed_until_ms.is_none()
        && !state.opted_out
        && state.card_entry_ids.is_empty()
        && state.paused_automation_ids.is_empty()
    {
        return None;
    }
    Some(serde_json::to_string(state).expect("SpendGuardState serialization cannot fail"))
}

pub fn parse_awaiting_state(raw: Option<&str>) -> Option<AwaitingUserResponse> {
    let value = parse_json(raw)?;
    let value = value.as_object()?;
    let tab_id = value.get("tabId").and_then(Value::as_str).unwrap_or_default();
    if tab_id.is_empty() {
        return None;
    }
    Some(AwaitingUserResponse {
        tab_id: tab_id.to_string(),
        reason: value
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        since: finite_number(value.get("since")),
    })
}

pub fn parse_request_records(raw: Option<&str>) -> Vec<RequestRecord> {
    let Some(value) = parse_json(raw) else {
        return Vec::new();
    };
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    let mut records = Vec::new();
    for item in items {
        let Some(entry) = item.as_object() else {
            continue;
        };
        let id = entry
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        if id.is_empty() {
            continue;
        }
        let at = entry
            .get("at")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(0.0);
        let prompt = entry
            .get("prompt")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let source = entry
            .get("source")
            .and_then(Value::as_str)
            .filter(|value| REQUEST_SOURCES.contains(value))
            .map(ToOwned::to_owned);
        records.push(RequestRecord {
            id: id.to_string(),
            at,
            prompt,
            source,
        });
    }
    records
}

pub fn parse_memory_prompt_snapshot(raw: Option<&str>) -> Option<MemoryPromptSnapshot> {
    let value = parse_json(raw)?;
    let value = value.as_object()?;
    let render = value.get("render").and_then(Value::as_str)?;
    let compaction_epoch = value.get("compactionEpoch").and_then(Value::as_f64)?;
    if !compaction_epoch.is_finite() {
        return None;
    }
    Some(MemoryPromptSnapshot {
        render: render.to_string(),
        compaction_epoch,
    })
}

pub fn parse_pending_episode_turns(raw: Option<&str>) -> Vec<EpisodeTurn> {
    let Some(value) = parse_json(raw) else {
        return Vec::new();
    };
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    let mut turns = Vec::new();
    for item in items {
        let Some(entry) = item.as_object() else {
            continue;
        };
        let user = entry
            .get("user")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let agent = entry
            .get("agent")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if user.is_empty() && agent.is_empty() {
            continue;
        }
        turns.push(EpisodeTurn {
            ts: finite_number(entry.get("ts")),
            user,
            agent,
        });
    }
    turns
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_parser_rejects_error_and_malformed_entries() {
        assert!(parse_transcript_entry(r#"{"kind":"error","message":"x"}"#).is_none());
        assert!(parse_transcript_entry(r#"{"kind":"message","content":"ok"}"#).is_some());
        assert!(parse_transcript_entry(r#"{"kind":"message"}"#).is_none());
        assert!(parse_transcript_entry("not-json").is_none());
    }

    #[test]
    fn request_sources_are_whitelisted_without_dropping_valid_identity() {
        let rows = parse_request_records(Some(
            r#"[{"id":"a","at":4,"prompt":"p","source":"turn"},{"id":"b","source":"unknown"},{"id":"","source":"turn"}]"#,
        ));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "a");
        assert_eq!(rows[0].source.as_deref(), Some("turn"));
        assert_eq!(rows[1].id, "b");
        assert_eq!(rows[1].source, None);
    }
}
