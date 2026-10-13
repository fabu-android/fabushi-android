use serde_json::{json, Value};

/// Android-adapted subset of Desktop group-chat glue.
///
/// Canonical group membership remains owned by AndroidAgentRoster. This module only owns
/// transient member preview projection and pure routing/redrive rules; it intentionally does not
/// introduce a second group store.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupMemberPreview {
    room_id: String,
    member_id: String,
    member_name: String,
    entry_id: String,
    is_open: bool,
}

impl GroupMemberPreview {
    pub fn new(room_id: &str, member_id: &str, member_name: &str, stream_id: &str) -> Self {
        Self {
            room_id: room_id.to_string(),
            member_id: member_id.to_string(),
            member_name: member_name.to_string(),
            entry_id: format!("group-preview:{stream_id}"),
            is_open: false,
        }
    }

    pub fn on_text_delta(&mut self, accumulated: &str) -> Option<Value> {
        if is_potential_pass_prefix(accumulated) {
            return self.finish();
        }
        let content = accumulated.trim_end();
        if content.trim().is_empty() {
            return None;
        }
        let event_type = if self.is_open {
            "updated"
        } else {
            self.is_open = true;
            "appended"
        };
        Some(json!({
            "type": event_type,
            "agentId": self.room_id,
            "entry": {
                "kind": "send-message",
                "id": self.entry_id,
                "message": {"type": "text", "content": content},
                "author": {"id": self.member_id, "name": self.member_name},
                "streaming": true,
            }
        }))
    }

    pub fn finish(&mut self) -> Option<Value> {
        if !self.is_open {
            return None;
        }
        self.is_open = false;
        Some(json!({
            "type": "removed",
            "agentId": self.room_id,
            "id": self.entry_id,
        }))
    }
}

pub fn group_member_reaction_target(group_room_id: Option<&str>, member_agent_id: &str) -> String {
    group_room_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(member_agent_id)
        .to_string()
}

pub fn should_redrive_group_member_after_preemption(
    sent_message_count: u64,
    reacted: bool,
) -> bool {
    sent_message_count == 0 && !reacted
}

/// Desktop hides a member preview while the member may still be producing the PASS sentinel.
/// Keep the same prefix behavior without importing Desktop session ownership.
fn is_potential_pass_prefix(value: &str) -> bool {
    const PASS: &str = "PASS";
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed.len() <= PASS.len()
        && PASS.starts_with(&trimmed.to_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_stays_hidden_for_pass_prefix_then_projects_and_removes() {
        let mut preview = GroupMemberPreview::new("room-1", "agent-1", "A", "stream-1");
        assert!(preview.on_text_delta("P").is_none());
        let appended = preview.on_text_delta("Hello").unwrap();
        assert_eq!(appended["type"], "appended");
        assert_eq!(appended["entry"]["author"]["id"], "agent-1");
        let updated = preview.on_text_delta("Hello world").unwrap();
        assert_eq!(updated["type"], "updated");
        let removed = preview.finish().unwrap();
        assert_eq!(removed["type"], "removed");
        assert!(preview.finish().is_none());
    }

    #[test]
    fn reaction_target_and_preemption_redrive_match_desktop_rules() {
        assert_eq!(group_member_reaction_target(Some(" room "), "member"), "room");
        assert_eq!(group_member_reaction_target(Some(""), "member"), "member");
        assert!(should_redrive_group_member_after_preemption(0, false));
        assert!(!should_redrive_group_member_after_preemption(1, false));
        assert!(!should_redrive_group_member_after_preemption(0, true));
    }
}
