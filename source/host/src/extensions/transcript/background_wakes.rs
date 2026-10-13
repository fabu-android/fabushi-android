use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::connectors::channel_delivery::ChannelOutboundMessage;

pub const TIMELINE_EVENT_WAKE_CUE: &str = "[event]";
pub const CHANNEL_INBOUND_WAKE_CUE: &str = "[inbound]";
pub const CHANNEL_DELIVERY_FAILED_WAKE_CUE: &str = "[channel-delivery-failed]";

pub fn format_channel_address_value(address: &Value) -> Option<String> {
    if let Some(raw) = address.as_str() {
        let trimmed = raw.trim();
        return (!trimmed.is_empty()).then(|| trimmed.to_string());
    }
    let platform = address.get("platform")?.as_str()?.trim();
    let chat = address.get("chat")?.as_str()?.trim();
    if platform.is_empty() || chat.is_empty() {
        return None;
    }
    Some(format!("{platform}:{chat}"))
}

fn channel_platform_display_name(address: &Value) -> String {
    let platform = address
        .get("platform")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match platform {
        "slack" => "Slack".to_string(),
        "discord" => "Discord".to_string(),
        other => other.to_string(),
    }
}

pub fn distinct_inbound_channel_addresses(envelopes: &[Value]) -> Vec<String> {
    distinct_channel_addresses(
        envelopes
            .iter()
            .filter_map(|envelope| envelope.get("address"))
            .filter_map(format_channel_address_value),
    )
}

pub fn build_channel_inbound_wake_prompt(envelopes: &[Value]) -> String {
    let mut grouped: Vec<(String, String, Vec<&Value>)> = Vec::new();
    for envelope in envelopes {
        let Some(address) = envelope.get("address") else {
            continue;
        };
        let Some(token) = format_channel_address_value(address) else {
            continue;
        };
        if let Some((_, _, bucket)) = grouped.iter_mut().find(|(key, _, _)| key == &token) {
            bucket.push(envelope);
        } else {
            grouped.push((
                token,
                channel_platform_display_name(address),
                vec![envelope],
            ));
        }
    }

    let has_message = envelopes
        .iter()
        .any(|envelope| envelope.get("reaction").map_or(true, Value::is_null));
    let plural = if envelopes.len() == 1 { "" } else { "s" };
    let opening = if has_message {
        format!("{CHANNEL_INBOUND_WAKE_CUE} New message{plural} on a channel you are connected to.")
    } else {
        format!(
            "{CHANNEL_INBOUND_WAKE_CUE} New reaction{plural} on a channel you are connected to."
        )
    };
    let mut blocks = Vec::new();
    for (token, platform_name, bucket) in grouped {
        let mut lines = Vec::new();
        for envelope in bucket {
            let sender = envelope
                .get("sender")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if let Some(reaction) = envelope.get("reaction").filter(|value| !value.is_null()) {
                let emoji = reaction
                    .get("emoji")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let summary = reaction
                    .get("messageQuote")
                    .and_then(Value::as_str)
                    .filter(|quote| !quote.is_empty())
                    .map(|quote| format!("reacted {emoji} to your message: \"{quote}\""))
                    .unwrap_or_else(|| format!("reacted {emoji} to a message"));
                lines.push(format!("  {sender} {summary}"));
            } else {
                let text = envelope
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                lines.push(format!("  {sender}: {text}"));
            }
        }
        blocks.push(format!(
            "On {platform_name}, from {token}:\n{}",
            lines.join("\n")
        ));
    }
    let closing = if has_message {
        "Reply to them by calling SendMessage with the channel target set to the address shown above. Open with a quick one-line acknowledgement first, then send progress and the result as separate messages as they happen, never one long message at the end. Keep each message short: this is a messaging app, so reply in brief, chat-style messages (lead with the answer, a sentence or two), not long ones. Keep working the rest of your task too, but do not leave them hanging."
    } else {
        "You don't need to reply; act on a reaction only if it's useful (e.g. acknowledge, adjust, or continue). If you do choose to respond, use SendMessage with the channel target shown above."
    };
    [
        opening,
        "This is activity from someone on an outside platform, not the user typing in this app."
            .to_string(),
        String::new(),
        blocks.join("\n\n"),
        String::new(),
        closing.to_string(),
    ]
    .join("\n")
}

pub fn build_channel_outbound_message(message: &Value) -> Option<ChannelOutboundMessage> {
    match message.get("type").and_then(Value::as_str)? {
        "text" => {
            let content = message.get("content").and_then(Value::as_str).unwrap_or_default();
            let image_url = message
                .get("images")
                .and_then(Value::as_array)
                .and_then(|images| {
                    images.iter().find_map(|image| {
                        image
                            .get("url")
                            .and_then(Value::as_str)
                            .filter(|url| !url.is_empty())
                    })
                });
            if let Some(url) = image_url {
                return Some(ChannelOutboundMessage::Attachment {
                    url: url.to_string(),
                    caption: (!content.is_empty()).then(|| content.to_string()),
                });
            }
            (!content.is_empty()).then(|| ChannelOutboundMessage::Text {
                text: content.to_string(),
            })
        }
        "attachment" => {
            let url = message.get("url").and_then(Value::as_str).unwrap_or_default();
            let alt = message
                .get("alt")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty());
            if !url.is_empty() {
                return Some(ChannelOutboundMessage::Attachment {
                    url: url.to_string(),
                    caption: alt.map(ToOwned::to_owned),
                });
            }
            alt.map(|text| ChannelOutboundMessage::Text {
                text: text.to_string(),
            })
        }
        _ => None,
    }
}

fn channel_platform(address_token: &str) -> Option<&str> {
    let trimmed = address_token.trim();
    let separator = trimmed.find(':')?;
    if separator == 0 {
        return None;
    }
    let platform = trimmed[..separator].trim();
    let chat = trimmed[separator + 1..].trim();
    if platform.is_empty() || chat.is_empty() {
        return None;
    }
    Some(platform)
}

fn channel_platform_name(platform: &str) -> &str {
    match platform {
        "slack" => "Slack",
        "discord" => "Discord",
        other => other,
    }
}

pub fn humanize_channel_delivery_failure(address_token: &str, raw_message: &str) -> String {
    let trimmed = raw_message.trim();
    let Some(platform) = channel_platform(address_token) else {
        return format!(
            "\"{address_token}\" isn't a valid channel address, so that message wasn't delivered."
        );
    };
    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("not a valid channel address") {
        return format!(
            "\"{address_token}\" isn't a valid channel address, so that message wasn't delivered."
        );
    }
    if trimmed == "No channel delivery mechanism is registered." {
        return "Channel messaging isn't available on this computer, so that message wasn't delivered."
            .to_string();
    }
    let platform_name = channel_platform_name(platform);
    if lower.contains("no live ") && lower.contains(" connection") {
        return format!(
            "{platform_name} isn't connected on this computer, so that message wasn't delivered. Connect {platform_name} (add its token) to send there."
        );
    }
    format!("Couldn't deliver that message to {platform_name}: {trimmed}")
}

pub fn build_channel_delivery_failure_wake_prompt(failures: &[Value]) -> String {
    let mut lines = vec![
        format!(
            "{CHANNEL_DELIVERY_FAILED_WAKE_CUE} A message you tried to send to a channel did not go through."
        ),
        "This is a system notice about your own outbound send, not the user typing in this app. You may have already told the user it was sent, so correct the record.".to_string(),
    ];
    lines.extend(failures.iter().map(|failure| {
        format!(
            "- To {}: {}",
            failure
                .get("addressToken")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            failure
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or_default()
        )
    }));
    let plural = if failures.len() == 1 { "" } else { "s" };
    lines.push(format!(
        "Tell the user plainly here, in this in-app chat (a SendMessage with no channel target), that the message{plural} didn't go through and why, so they aren't left believing it was delivered. Don't silently retry the same channel; if it isn't connected, offer to help connect it."
    ));
    lines.join("\n")
}

pub fn redrivable_inbound_envelopes(envelopes: &[Value]) -> Vec<Value> {
    envelopes
        .iter()
        .filter(|envelope| envelope.get("isRedriven").and_then(Value::as_bool) != Some(true))
        .map(|envelope| {
            let mut envelope = envelope.clone();
            if let Some(object) = envelope.as_object_mut() {
                object.insert("isDisplayed".into(), Value::Bool(true));
                object.insert("isRedriven".into(), Value::Bool(true));
            }
            envelope
        })
        .collect()
}

pub fn describe_timeline_event(event: &Value) -> String {
    let kind = event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match kind {
        "name-changed" => format!(
            "Renamed to {}",
            event.get("to").and_then(Value::as_str).unwrap_or_default()
        ),
        "channel-connected" => format!(
            "Connected to {}",
            event
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or_default()
        ),
        "channel-disconnected" => format!(
            "Disconnected from {}",
            event
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or_default()
        ),
        "automation-changed" => {
            let action = match event
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or_default()
            {
                "created" => "Created",
                "updated" => "Updated",
                "enabled" => "Enabled",
                "disabled" => "Disabled",
                "deleted" => "Deleted",
                _ => "Changed",
            };
            format!(
                "{} automation \"{}\"",
                action,
                event
                    .get("automationName")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
            )
        }
        _ => "Updated this conversation".to_string(),
    }
}

pub fn build_timeline_event_wake_prompt(events: &[Value]) -> String {
    let mut lines = vec![
        format!(
            "{} Something about this conversation just changed.",
            TIMELINE_EVENT_WAKE_CUE
        ),
        "This is a system event recorded in your timeline, not the user typing in this app, and possibly something you did yourself.".to_string(),
    ];
    lines.extend(
        events
            .iter()
            .map(|event| format!("- {}", describe_timeline_event(event))),
    );
    lines.push(
        "If it is worth acknowledging to the user, reply with SendMessage; otherwise it is fine to stay silent.".to_string(),
    );
    lines.join("\n")
}

pub fn distinct_channel_addresses<I, S>(addresses: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for address in addresses {
        let address = address.into();
        if seen.insert(address.clone()) {
            out.push(address);
        }
    }
    out
}

#[derive(Debug, Default)]
pub struct BackgroundWakes<T> {
    pub pending_inbound: HashMap<String, Vec<T>>,
    pub reviving_inbound_agent_ids: HashSet<String>,
    pub pending_channel_failures: HashMap<String, Vec<T>>,
    pub reviving_channel_failure_agent_ids: HashSet<String>,
    pub pending_event_wakes: HashMap<String, Vec<T>>,
    pub reviving_event_agent_ids: HashSet<String>,
    pub dm_preempted_wake_agent_ids: HashSet<String>,
}

impl<T> BackgroundWakes<T> {
    pub fn enqueue(map: &mut HashMap<String, Vec<T>>, agent_id: impl Into<String>, item: T) {
        map.entry(agent_id.into()).or_default().push(item);
    }

    pub fn begin_revival(set: &mut HashSet<String>, agent_id: &str) -> bool {
        set.insert(agent_id.to_string())
    }

    pub fn end_revival(set: &mut HashSet<String>, agent_id: &str) {
        set.remove(agent_id);
    }

    pub fn take_pending(map: &mut HashMap<String, Vec<T>>, agent_id: &str) -> Vec<T> {
        map.remove(agent_id).unwrap_or_default()
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn inbound_prompt_groups_addresses_and_marks_redrive_once() {
        let envelopes = vec![
            json!({"address":{"platform":"slack","chat":"C1"},"sender":"alice","text":"hello"}),
            json!({"address":{"platform":"slack","chat":"C1"},"sender":"bob","text":"status?"}),
            json!({"address":{"platform":"discord","chat":"42"},"sender":"carol","reaction":{"emoji":"👍","messageQuote":"done"}}),
        ];
        assert_eq!(
            distinct_inbound_channel_addresses(&envelopes),
            vec!["slack:C1".to_string(), "discord:42".to_string()]
        );
        let prompt = build_channel_inbound_wake_prompt(&envelopes);
        assert!(prompt.starts_with(CHANNEL_INBOUND_WAKE_CUE));
        assert!(prompt.contains("On Slack, from slack:C1:"));
        assert!(prompt.contains("alice: hello"));
        assert!(prompt.contains("carol reacted 👍 to your message: \"done\""));

        let redriven = redrivable_inbound_envelopes(&envelopes);
        assert_eq!(redriven.len(), 3);
        assert!(redriven.iter().all(|entry| entry["isDisplayed"] == true && entry["isRedriven"] == true));
        assert!(redrivable_inbound_envelopes(&redriven).is_empty());
    }

    #[test]
    fn outbound_message_and_failure_humanization_preserve_channel_semantics() {
        assert_eq!(
            build_channel_outbound_message(&json!({"type":"text","content":"hello"})),
            Some(ChannelOutboundMessage::Text { text: "hello".into() })
        );
        assert_eq!(
            build_channel_outbound_message(&json!({
                "type":"text",
                "content":"caption",
                "images":[{"url":"https://example.test/image.png"}]
            })),
            Some(ChannelOutboundMessage::Attachment {
                url: "https://example.test/image.png".into(),
                caption: Some("caption".into()),
            })
        );
        assert!(humanize_channel_delivery_failure("slack:C1", "No live Slack connection")
            .contains("Slack isn't connected"));
        assert!(humanize_channel_delivery_failure("invalid", "bad")
            .contains("isn't a valid channel address"));
    }

    #[test]
    fn background_wake_registry_fences_duplicate_revival_and_drains_atomically() {
        let mut wakes = BackgroundWakes::<u64>::default();
        BackgroundWakes::enqueue(&mut wakes.pending_inbound, "agent-a", 1);
        BackgroundWakes::enqueue(&mut wakes.pending_inbound, "agent-a", 2);
        assert!(BackgroundWakes::<u64>::begin_revival(
            &mut wakes.reviving_inbound_agent_ids,
            "agent-a",
        ));
        assert!(!BackgroundWakes::<u64>::begin_revival(
            &mut wakes.reviving_inbound_agent_ids,
            "agent-a",
        ));
        assert_eq!(
            BackgroundWakes::<u64>::take_pending(&mut wakes.pending_inbound, "agent-a"),
            vec![1, 2]
        );
        assert!(BackgroundWakes::<u64>::take_pending(
            &mut wakes.pending_inbound,
            "agent-a",
        )
        .is_empty());
        BackgroundWakes::<u64>::end_revival(
            &mut wakes.reviving_inbound_agent_ids,
            "agent-a",
        );
        assert!(BackgroundWakes::<u64>::begin_revival(
            &mut wakes.reviving_inbound_agent_ids,
            "agent-a",
        ));
    }
}
