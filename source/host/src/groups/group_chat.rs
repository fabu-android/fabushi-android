use std::collections::HashSet;

pub const GROUP_CONFIG_VERSION: u64 = 1;
pub const GROUP_MAX_MEMBER_TURNS: usize = 10;
pub const GROUP_MAX_ROUNDS: usize = 3;
pub const GROUP_PROMPT_HISTORY_LIMIT: usize = 24;
pub const GROUP_MAX_MESSAGES_PER_TURN: usize = 2;
pub const SHARED_ROOM_HISTORY_LIMIT: usize = 24;
pub const GROUP_CHAT_TAG_PREFIX: &str = "[Group chat: ";
pub const SAND_HIDDEN_PROMPT_MARKER: &str = "[SAND_HIDDEN_PROMPT]";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupMember {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupDescription {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupSpeaker {
    User { name: Option<String> },
    Member { id: String, name: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupMessage {
    pub speaker: GroupSpeaker,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("A group chat can only contain individual agents, not other group chats. Remove the group chat{plural} from the member list.")]
pub struct SandGroupNestingError {
    pub nested_group_ids: Vec<String>,
    plural: &'static str,
}

pub fn order_round_speakers<T: Clone>(member_ids: &[T], round: i64) -> Vec<T> {
    if member_ids.is_empty() {
        return Vec::new();
    }
    let len = member_ids.len() as i64;
    let offset = ((round % len) + len) % len;
    let offset = offset as usize;
    member_ids[offset..]
        .iter()
        .chain(member_ids[..offset].iter())
        .cloned()
        .collect()
}

pub fn is_same_member_set(a: &[String], b: &[String]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let set = a.iter().collect::<HashSet<_>>();
    b.iter().all(|id| set.contains(id))
}

pub fn assert_members_are_not_groups<F>(
    ids: &[String],
    mut is_group_id: F,
) -> Result<(), SandGroupNestingError>
where
    F: FnMut(&str) -> bool,
{
    let mut seen = HashSet::new();
    let nested = ids
        .iter()
        .filter(|id| seen.insert((*id).clone()) && is_group_id(id))
        .cloned()
        .collect::<Vec<_>>();
    if nested.is_empty() {
        Ok(())
    } else {
        Err(SandGroupNestingError {
            plural: if nested.len() == 1 { "" } else { "s" },
            nested_group_ids: nested,
        })
    }
}

pub fn member_mention_handles(name: &str) -> Vec<String> {
    let lower = name.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return Vec::new();
    }
    let mut handles = Vec::new();
    push_unique(&mut handles, lower.clone());
    push_unique(&mut handles, lower.split_whitespace().collect::<String>());
    if let Some(first) = lower.split_whitespace().next().filter(|value| !value.is_empty()) {
        push_unique(&mut handles, first.to_string());
    }
    handles
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|candidate| candidate == &value) {
        values.push(value);
    }
}

fn is_word_char(value: Option<u8>) -> bool {
    value.is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

fn has_mention_at(lower: &str, handle: &str) -> bool {
    let needle = format!("@{handle}");
    let bytes = lower.as_bytes();
    let needle_bytes = needle.as_bytes();
    if needle_bytes.is_empty() || needle_bytes.len() > bytes.len() {
        return false;
    }
    for index in 0..=bytes.len() - needle_bytes.len() {
        if &bytes[index..index + needle_bytes.len()] != needle_bytes {
            continue;
        }
        let before = index.checked_sub(1).and_then(|offset| bytes.get(offset).copied());
        let after = bytes.get(index + needle_bytes.len()).copied();
        if !is_word_char(before) && !is_word_char(after) {
            return true;
        }
    }
    false
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupMentions {
    pub is_everyone: bool,
    pub member_ids: Vec<String>,
}

pub fn parse_group_mentions(text: &str, members: &[GroupMember]) -> GroupMentions {
    let lower = text.to_ascii_lowercase();
    let mut member_ids = Vec::new();
    let mut seen = HashSet::new();
    for member in members {
        if seen.contains(&member.id) {
            continue;
        }
        if member_mention_handles(&member.name)
            .iter()
            .any(|handle| has_mention_at(&lower, handle))
        {
            seen.insert(member.id.clone());
            member_ids.push(member.id.clone());
        }
    }
    let everyone = ["@everyone", "@all"]
        .iter()
        .any(|handle| has_mention_at(&lower, &handle[1..]));
    GroupMentions {
        is_everyone: everyone,
        member_ids,
    }
}

pub fn resolve_responders(members: &[GroupMember], history: &[GroupMessage]) -> Vec<GroupMember> {
    let start = history
        .iter()
        .rposition(|message| matches!(message.speaker, GroupSpeaker::User { .. }))
        .unwrap_or(0);
    let mut everyone = false;
    let mut mentioned = HashSet::new();
    for message in &history[start..] {
        let targets = parse_group_mentions(&message.content, members);
        everyone |= targets.is_everyone;
        mentioned.extend(targets.member_ids);
    }
    if everyone || mentioned.is_empty() {
        members.to_vec()
    } else {
        members
            .iter()
            .filter(|member| mentioned.contains(&member.id))
            .cloned()
            .collect()
    }
}

fn normalized_pass_token(content: &str) -> String {
    let mut value = content.trim().to_ascii_lowercase();
    if value.ends_with('.') {
        value.pop();
        value = value.trim_end().to_string();
    }
    if value.starts_with('(') && value.ends_with(')') && value.len() >= 2 {
        value = value[1..value.len() - 1].trim().to_string();
    }
    value
}

pub fn is_pass_content(content: &str) -> bool {
    let trimmed = content.trim();
    trimmed.is_empty() || normalized_pass_token(trimmed) == "pass"
}

pub fn is_potential_pass_prefix(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() || is_pass_content(trimmed) {
        return true;
    }
    let mut value = trimmed.to_ascii_lowercase();
    if value.starts_with('(') {
        value.remove(0);
    }
    while value.ends_with('.') || value.ends_with(')') {
        value.pop();
    }
    let value = value.trim();
    "pass".starts_with(value) && value.chars().all(|ch| ch.is_ascii_lowercase())
}

pub fn build_group_redrive_note() -> &'static str {
    "\n(Redelivery: your previous attempt at this turn was interrupted by a direct message to you. The room has NOT seen any reply from you for the messages above — anything you said or did while handling that direct message stayed in that private chat. If you already did the work, send the result to this room with SendMessage now; otherwise take the turn normally.)"
}

pub fn format_group_line(message: &GroupMessage, viewer_id: &str) -> String {
    match &message.speaker {
        GroupSpeaker::User { name: Some(name) } => format!("{name} (user): {}", message.content),
        GroupSpeaker::User { name: None } => format!("User: {}", message.content),
        GroupSpeaker::Member { id, name } => format!(
            "{name}{}: {}",
            if id == viewer_id { " (you)" } else { "" },
            message.content
        ),
    }
}

pub fn format_group_history(history: &[GroupMessage], viewer_id: &str, limit: usize) -> String {
    let start = history.len().saturating_sub(limit);
    if start == history.len() {
        "(no messages yet)".to_string()
    } else {
        history[start..]
            .iter()
            .map(|message| format_group_line(message, viewer_id))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub fn is_group_turn_prompt_text(text: &str) -> bool {
    let body = text
        .strip_prefix(SAND_HIDDEN_PROMPT_MARKER)
        .unwrap_or(text);
    body.starts_with(GROUP_CHAT_TAG_PREFIX)
}

pub fn group_display_name(group: &GroupDescription) -> String {
    let name = group.name.trim();
    if name.is_empty() {
        "the group".to_string()
    } else {
        name.to_string()
    }
}

pub fn describe_group(group: &GroupDescription) -> String {
    let name = group_display_name(group);
    let description = group.description.trim();
    if description.is_empty() {
        format!("\"{name}\"")
    } else {
        format!("\"{name}\" — {description}")
    }
}

pub fn format_group_chat_tag(group: &GroupDescription, peers: &[GroupMember]) -> String {
    let suffix = if peers.is_empty() {
        String::new()
    } else {
        format!(
            " - with {}",
            peers.iter().map(|peer| peer.name.as_str()).collect::<Vec<_>>().join(", ")
        )
    };
    format!("{GROUP_CHAT_TAG_PREFIX}\"{}\"{suffix}]", group_display_name(group))
}

pub fn build_group_member_system_prompt(
    member: &GroupMember,
    group: &GroupDescription,
    peers: &[GroupMember],
    is_shared_room: bool,
) -> String {
    let mut lines = vec![format!(
        "You are {}, one participant in a group chat ({}).",
        member.name,
        describe_group(group)
    )];
    if !member.description.trim().is_empty() {
        lines.push(format!("Your persona: {}", member.description.trim()));
    }
    if !peers.is_empty() {
        lines.push(String::new());
        lines.push("Other participants in the room:".into());
        lines.extend(peers.iter().map(|peer| {
            if peer.description.trim().is_empty() {
                format!("- {}", peer.name)
            } else {
                format!("- {} ({})", peer.name, peer.description.trim())
            }
        }));
    }
    lines.push(String::new());
    if peers.is_empty() {
        lines.push("Right now you are speaking in this group chat.".into());
    } else {
        lines.push(format!(
            "Right now you are speaking in this group chat, with {}.",
            peers.iter().map(|peer| peer.name.as_str()).collect::<Vec<_>>().join(", ")
        ));
    }
    lines.push(if is_shared_room {
        "This is a cross-user room turn. Tool calls and plain text are private scratch space; only SendMessage plain text is delivered to the room.".into()
    } else {
        "You have your full toolkit in this room. Do the work first, then deliver the result with SendMessage.".into()
    });
    lines.push(String::new());
    lines.push(format!(
        "Stay fully in character as {}. The ONLY way to say something the room can see is the SendMessage tool. Keep each message short and conversational. If you have nothing new worth adding, send exactly \"(pass)\". Never reveal private one-on-one context.",
        member.name
    ));
    lines.join("\n")
}

pub fn messages_since_member_last_spoke<'a>(
    history: &'a [GroupMessage],
    member_id: &str,
) -> &'a [GroupMessage] {
    if let Some(index) = history.iter().rposition(|message| {
        matches!(&message.speaker, GroupSpeaker::Member { id, .. } if id == member_id)
    }) {
        &history[index + 1..]
    } else {
        history
    }
}

pub fn build_group_turn_prompt(
    member: &GroupMember,
    group: &GroupDescription,
    peers: &[GroupMember],
    new_messages: &[GroupMessage],
) -> String {
    let history = if new_messages.is_empty() {
        "No new messages in the room since your last turn.".to_string()
    } else {
        format!(
            "New messages in the room (oldest first):\n{}",
            format_group_history(new_messages, &member.id, GROUP_PROMPT_HISTORY_LIMIT)
        )
    };
    format!(
        "{}\n{}\n\nIt's your turn, {}. Reply in character with a single SendMessage if you have something worth adding, or send \"(pass)\" if you don't.",
        format_group_chat_tag(group, peers),
        history,
        member.name
    )
}
