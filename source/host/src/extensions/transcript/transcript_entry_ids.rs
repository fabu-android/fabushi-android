use std::collections::HashSet;

use serde_json::Value;

pub const BOOT_TURN: &str = "b";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptEntryIdKind {
    UserMessage,
    UserAttachment,
    AssistantMessage,
    SendMessage,
}

pub fn next_entry_id(entries: &[Value], kind: TranscriptEntryIdKind) -> String {
    let ids = entries
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .map(ToOwned::to_owned)
        .collect::<HashSet<_>>();
    let users = count_user_messages(entries);
    match kind {
        TranscriptEntryIdKind::UserMessage => {
            first_unused_id(&ids, |index| format!("t{index}u"), users)
        }
        TranscriptEntryIdKind::UserAttachment => first_unused_id(
            &ids,
            |index| format!("t{users}ua{index}"),
            count_trailing_user_attachments(entries),
        ),
        TranscriptEntryIdKind::AssistantMessage => {
            let turn = if users == 0 { BOOT_TURN.to_string() } else { (users - 1).to_string() };
            first_unused_id(
                &ids,
                |index| format!("t{turn}a{index}"),
                count_trailing_assistant_messages(entries),
            )
        }
        TranscriptEntryIdKind::SendMessage => {
            let turn = if users == 0 { BOOT_TURN.to_string() } else { (users - 1).to_string() };
            first_unused_id(
                &ids,
                |index| format!("t{turn}s{index}"),
                count_trailing_send_messages(entries),
            )
        }
    }
}

pub fn first_unused_id(
    existing_ids: &HashSet<String>,
    mut mint: impl FnMut(usize) -> String,
    start_index: usize,
) -> String {
    let mut index = start_index;
    let mut id = mint(index);
    while existing_ids.contains(&id) {
        index = index.saturating_add(1);
        id = mint(index);
    }
    id
}

pub fn count_user_messages(entries: &[Value]) -> usize {
    entries
        .iter()
        .filter(|entry| {
            entry.get("kind").and_then(Value::as_str) == Some("message")
                && entry.get("role").and_then(Value::as_str) == Some("user")
        })
        .count()
}

fn count_trailing(entries: &[Value], predicate: impl Fn(&Value) -> bool) -> usize {
    let mut count = 0usize;
    for entry in entries.iter().rev() {
        if entry.get("kind").and_then(Value::as_str) == Some("message")
            && entry.get("role").and_then(Value::as_str) == Some("user")
        {
            break;
        }
        if predicate(entry) {
            count = count.saturating_add(1);
        }
    }
    count
}

pub fn count_trailing_user_attachments(entries: &[Value]) -> usize {
    count_trailing(entries, |entry| {
        entry.get("kind").and_then(Value::as_str) == Some("user-attachment")
    })
}

pub fn count_trailing_assistant_messages(entries: &[Value]) -> usize {
    count_trailing(entries, |entry| {
        entry.get("kind").and_then(Value::as_str) == Some("message")
            && entry.get("role").and_then(Value::as_str) == Some("assistant")
    })
}

pub fn count_trailing_send_messages(entries: &[Value]) -> usize {
    count_trailing(entries, |entry| {
        entry.get("kind").and_then(Value::as_str) == Some("send-message")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ids_preserve_desktop_turn_shape_and_skip_collisions() {
        let entries = vec![
            json!({"kind":"message","id":"t0u","role":"user"}),
            json!({"kind":"user-attachment","id":"t1ua0"}),
            json!({"kind":"message","id":"t0a0","role":"assistant"}),
            json!({"kind":"send-message","id":"t0s0"}),
        ];
        assert_eq!(next_entry_id(&entries, TranscriptEntryIdKind::UserMessage), "t1u");
        assert_eq!(next_entry_id(&entries, TranscriptEntryIdKind::UserAttachment), "t1ua1");
        assert_eq!(next_entry_id(&entries, TranscriptEntryIdKind::AssistantMessage), "t0a1");
        assert_eq!(next_entry_id(&entries, TranscriptEntryIdKind::SendMessage), "t0s1");
    }

    #[test]
    fn boot_turn_ids_remain_stable_without_user_messages() {
        let entries = vec![json!({"kind":"message","id":"tba0","role":"assistant"})];
        assert_eq!(next_entry_id(&entries, TranscriptEntryIdKind::AssistantMessage), "tba1");
        assert_eq!(next_entry_id(&entries, TranscriptEntryIdKind::SendMessage), "tbs0");
    }
}
