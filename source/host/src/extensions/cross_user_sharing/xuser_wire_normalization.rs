use serde_json::Value;

pub const MAX_INLINE_IMAGES_PER_ENTRY: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineImage {
    pub base64: String,
    pub media_type: String,
    pub alt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnMessage {
    pub speaker_kind: String,
    pub speaker_name: String,
    pub is_self: bool,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirrorEntry {
    HumanMessage {
        entry_id: String,
        author_auth_id: String,
        author_name: String,
        author_avatar_url: Option<String>,
        text: String,
        images: Vec<InlineImage>,
        client_nonce: Option<String>,
        timestamp_ms: u64,
    },
    AgentMessage {
        entry_id: String,
        agent_owner_auth_id: String,
        agent_id: String,
        author_name: String,
        text: String,
        images: Vec<InlineImage>,
        timestamp_ms: u64,
    },
}

fn clamp_guest_name(value: &str) -> String {
    let one = value
        .replace(['\r', '\n'], " ")
        .trim()
        .chars()
        .take(120)
        .collect::<String>();
    if one.is_empty() {
        "Someone".into()
    } else {
        one
    }
}

pub fn normalize_inline_images(raw: &Value) -> Vec<InlineImage> {
    let Some(items) = raw.as_array() else {
        return vec![];
    };
    let mut out = Vec::new();
    for value in items.iter().take(MAX_INLINE_IMAGES_PER_ENTRY) {
        let Some(object) = value.as_object() else {
            continue;
        };
        let Some(base64) = object
            .get("base64")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let media_type = object
            .get("mediaType")
            .and_then(Value::as_str)
            .filter(|value| value.starts_with("image/"))
            .unwrap_or("image/png");
        let alt = object
            .get("alt")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        out.push(InlineImage {
            base64: base64.into(),
            media_type: media_type.into(),
            alt,
        });
    }
    out
}

pub fn normalize_mirror_entry(value: &Value, now_ms: u64) -> Option<MirrorEntry> {
    let object = value.as_object()?;
    let entry_id = object.get("entryId")?.as_str()?.to_string();
    if entry_id.is_empty() {
        return None;
    }
    let text = object
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let images = normalize_inline_images(object.get("images").unwrap_or(&Value::Null));
    if text.is_empty() && images.is_empty() {
        return None;
    }
    let timestamp_ms = object
        .get("timestampMs")
        .and_then(Value::as_u64)
        .unwrap_or(now_ms);

    match object.get("kind").and_then(Value::as_str) {
        Some("human-message") => {
            let author_auth_id = object.get("authorAuthId")?.as_str()?.to_string();
            if author_auth_id.is_empty() {
                return None;
            }
            Some(MirrorEntry::HumanMessage {
                entry_id,
                author_auth_id,
                author_name: clamp_guest_name(
                    object.get("authorName").and_then(Value::as_str).unwrap_or(""),
                ),
                author_avatar_url: object
                    .get("authorAvatarUrl")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string),
                text,
                images,
                client_nonce: object
                    .get("clientNonce")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string),
                timestamp_ms,
            })
        }
        Some("agent-message") => {
            let agent_owner_auth_id = object.get("agentOwnerAuthId")?.as_str()?.to_string();
            let agent_id = object.get("agentId")?.as_str()?.to_string();
            if agent_owner_auth_id.is_empty() || agent_id.is_empty() {
                return None;
            }
            Some(MirrorEntry::AgentMessage {
                entry_id,
                agent_owner_auth_id,
                agent_id,
                author_name: clamp_guest_name(
                    object.get("authorName").and_then(Value::as_str).unwrap_or(""),
                ),
                text,
                images,
                timestamp_ms,
            })
        }
        _ => None,
    }
}

pub fn normalize_turn_messages(raw: &Value) -> Vec<TurnMessage> {
    let Some(items) = raw.as_array() else {
        return vec![];
    };
    let mut out = Vec::new();
    for value in items.iter().take(24) {
        let Some(object) = value.as_object() else {
            continue;
        };
        let Some(text) = object
            .get("text")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        out.push(TurnMessage {
            speaker_kind: if object.get("speakerKind").and_then(Value::as_str) == Some("agent") {
                "agent".into()
            } else {
                "human".into()
            },
            speaker_name: clamp_guest_name(
                object.get("speakerName").and_then(Value::as_str).unwrap_or(""),
            ),
            is_self: object.get("isSelf").and_then(Value::as_bool) == Some(true),
            text: text.into(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn inline_images_are_bounded_and_fail_closed_to_image_mime() {
        let images = normalize_inline_images(&json!([
            {"base64":"a","mediaType":"image/png","alt":"A"},
            {"base64":"b","mediaType":"text/html"},
            {"base64":"c","mediaType":"image/webp"},
            {"base64":"d"},
            {"base64":"e","mediaType":"image/jpeg"}
        ]));
        assert_eq!(images.len(), 4);
        assert_eq!(images[0].alt.as_deref(), Some("A"));
        assert_eq!(images[1].media_type, "image/png");
        assert_eq!(images[2].media_type, "image/webp");
        assert_eq!(images[3].media_type, "image/png");
    }

    #[test]
    fn mirror_entries_require_identity_and_visible_content() {
        let human = normalize_mirror_entry(
            &json!({
                "kind":"human-message",
                "entryId":"e1",
                "authorAuthId":"human-1",
                "authorName":"  Alice\nExample  ",
                "text":"hello",
                "clientNonce":"n1"
            }),
            42,
        )
        .expect("human entry");
        match human {
            MirrorEntry::HumanMessage {
                author_name,
                timestamp_ms,
                client_nonce,
                ..
            } => {
                assert_eq!(author_name, "Alice Example");
                assert_eq!(timestamp_ms, 42);
                assert_eq!(client_nonce.as_deref(), Some("n1"));
            }
            _ => panic!("expected human entry"),
        }

        assert!(normalize_mirror_entry(
            &json!({"kind":"agent-message","entryId":"e2","agentOwnerAuthId":"","agentId":"a","text":"x"}),
            1,
        ).is_none());
        assert!(normalize_mirror_entry(
            &json!({"kind":"human-message","entryId":"e3","authorAuthId":"u","text":"","images":[]}),
            1,
        ).is_none());
    }

    #[test]
    fn turn_messages_are_bounded_and_normalized() {
        let raw = Value::Array(
            (0..30)
                .map(|index| json!({
                    "speakerKind": if index % 2 == 0 {"agent"} else {"other"},
                    "speakerName":"",
                    "isSelf": index == 0,
                    "text":format!("m{index}")
                }))
                .collect(),
        );
        let messages = normalize_turn_messages(&raw);
        assert_eq!(messages.len(), 24);
        assert_eq!(messages[0].speaker_kind, "agent");
        assert_eq!(messages[1].speaker_kind, "human");
        assert_eq!(messages[0].speaker_name, "Someone");
        assert!(messages[0].is_self);
    }
}
