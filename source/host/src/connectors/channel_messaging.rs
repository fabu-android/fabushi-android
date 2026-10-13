use std::collections::{HashMap, HashSet};

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::channel_delivery::ChannelOutboundMessage;

pub const TIMELINE_EVENT_WAKE_CUE: &str = "[event]";
pub const TIMELINE_EVENT_WAKE_CUE: &str = "[event]";
pub const CHANNEL_INBOUND_WAKE_CUE: &str = "[inbound]";
pub const CHANNEL_DELIVERY_FAILED_WAKE_CUE: &str = "[channel-delivery-failed]";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelAddress {
    pub platform: String,
    pub chat: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelDeliveryFailure {
    pub address_token: String,
    pub reason: String,
}

pub fn parse_channel_address(raw: &str) -> Option<ChannelAddress> {
    let trimmed = raw.trim();
    let separator = trimmed.find(':')?;
    if separator == 0 {
        return None;
    }
    let platform = trimmed[..separator].trim();
    let chat = trimmed[separator + 1..].trim();
    if platform.is_empty() || chat.is_empty() {
        return None;
    }
    Some(ChannelAddress {
        platform: platform.to_string(),
        chat: chat.to_string(),
    })
}

pub fn format_channel_address(address: &ChannelAddress) -> String {
    format!("{}:{}", address.platform, address.chat)
}

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
        let platform = address
            .get("platform")
            .and_then(Value::as_str)
            .map(platform_name)
            .unwrap_or_default()
            .to_string();
        if let Some((_, _, bucket)) = grouped.iter_mut().find(|(key, _, _)| key == &token) {
            bucket.push(envelope);
        } else {
            grouped.push((token, platform, vec![envelope]));
        }
    }

    let has_message = envelopes
        .iter()
        .any(|envelope| envelope.get("reaction").map_or(true, Value::is_null));
    let plural = if envelopes.len() == 1 { "" } else { "s" };
    let opening = if has_message {
        format!("{CHANNEL_INBOUND_WAKE_CUE} New message{plural} on a channel you are connected to.")
    } else {
        format!("{CHANNEL_INBOUND_WAKE_CUE} New reaction{plural} on a channel you are connected to.")
    };
    let blocks = grouped
        .into_iter()
        .map(|(token, platform, bucket)| {
            let lines = bucket
                .into_iter()
                .map(|envelope| {
                    let sender = envelope.get("sender").and_then(Value::as_str).unwrap_or_default();
                    if let Some(reaction) = envelope.get("reaction").filter(|value| !value.is_null()) {
                        let emoji = reaction.get("emoji").and_then(Value::as_str).unwrap_or_default();
                        let summary = reaction
                            .get("messageQuote")
                            .and_then(Value::as_str)
                            .filter(|quote| !quote.is_empty())
                            .map(|quote| format!("reacted {emoji} to your message: \"{quote}\""))
                            .unwrap_or_else(|| format!("reacted {emoji} to a message"));
                        format!("  {sender} {summary}")
                    } else {
                        let text = envelope.get("text").and_then(Value::as_str).unwrap_or_default();
                        format!("  {sender}: {text}")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!("On {platform}, from {token}:\n{lines}")
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let closing = if has_message {
        "Reply to them by calling SendMessage with the channel target set to the address shown above. Open with a quick one-line acknowledgement first, then send progress and the result as separate messages as they happen, never one long message at the end. Keep each message short: this is a messaging app, so reply in brief, chat-style messages (lead with the answer, a sentence or two), not long ones. Keep working the rest of your task too, but do not leave them hanging."
    } else {
        "You don't need to reply; act on a reaction only if it's useful (e.g. acknowledge, adjust, or continue). If you do choose to respond, use SendMessage with the channel target shown above."
    };
    [
        opening,
        "This is activity from someone on an outside platform, not the user typing in this app.".to_string(),
        String::new(),
        blocks,
        String::new(),
        closing.to_string(),
    ]
    .join("\n")
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
    match event.get("type").and_then(Value::as_str).unwrap_or_default() {
        "name-changed" => format!(
            "Renamed to {}",
            event.get("to").and_then(Value::as_str).unwrap_or_default()
        ),
        "channel-connected" => format!(
            "Connected to {}",
            event.get("label").and_then(Value::as_str).unwrap_or_default()
        ),
        "channel-disconnected" => format!(
            "Disconnected from {}",
            event.get("label").and_then(Value::as_str).unwrap_or_default()
        ),
        "automation-changed" => {
            let action = match event.get("action").and_then(Value::as_str).unwrap_or_default() {
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
                event.get("automationName").and_then(Value::as_str).unwrap_or_default()
            )
        }
        _ => "Updated this conversation".to_string(),
    }
}

pub fn build_timeline_event_wake_prompt(events: &[Value]) -> String {
    let mut lines = vec![
        format!("{TIMELINE_EVENT_WAKE_CUE} Something about this conversation just changed."),
        "This is a system event recorded in your timeline, not the user typing in this app, and possibly something you did yourself.".to_string(),
    ];
    lines.extend(events.iter().map(|event| format!("- {}", describe_timeline_event(event))));
    lines.push(
        "If it is worth acknowledging to the user, reply with SendMessage; otherwise it is fine to stay silent.".to_string(),
    );
    lines.join("\n")
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

pub fn build_channel_outbound_message(message: &Value) -> Option<ChannelOutboundMessage> {
    match message.get("type").and_then(Value::as_str)? {
        "text" => {
            let content = message
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if let Some(url) = message
                .get("images")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|entry| entry.get("url").and_then(Value::as_str))
                .find(|url| !url.is_empty())
            {
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
            let caption = message
                .get("alt")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            if !url.is_empty() {
                Some(ChannelOutboundMessage::Attachment {
                    url: url.to_string(),
                    caption,
                })
            } else {
                caption.map(|text| ChannelOutboundMessage::Text { text })
            }
        }
        _ => None,
    }
}


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

pub fn distinct_inbound_channel_addresses(envelopes: &[Value]) -> Vec<String> {
    distinct_channel_addresses(
        envelopes
            .iter()
            .filter_map(|envelope| envelope.get("address"))
            .filter_map(format_channel_address_value),
    )
}

fn channel_platform_display_name(address: &Value) -> String {
    let platform = address
        .get("platform")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            address
                .as_str()
                .and_then(parse_channel_address)
                .map(|parsed| parsed.platform)
        })
        .unwrap_or_default();
    match platform.as_str() {
        "slack" => "Slack".into(),
        "discord" => "Discord".into(),
        _ => platform,
    }
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
            grouped.push((token, channel_platform_display_name(address), vec![envelope]));
        }
    }

    let has_message = envelopes
        .iter()
        .any(|envelope| envelope.get("reaction").map_or(true, Value::is_null));
    let plural = if envelopes.len() == 1 { "" } else { "s" };
    let opening = if has_message {
        format!("{CHANNEL_INBOUND_WAKE_CUE} New message{plural} on a channel you are connected to.")
    } else {
        format!("{CHANNEL_INBOUND_WAKE_CUE} New reaction{plural} on a channel you are connected to.")
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
    match event.get("type").and_then(Value::as_str).unwrap_or_default() {
        "name-changed" => format!(
            "Renamed to {}",
            event.get("to").and_then(Value::as_str).unwrap_or_default()
        ),
        "channel-connected" => format!(
            "Connected to {}",
            event.get("label").and_then(Value::as_str).unwrap_or_default()
        ),
        "channel-disconnected" => format!(
            "Disconnected from {}",
            event.get("label").and_then(Value::as_str).unwrap_or_default()
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
        _ => "Updated this conversation".into(),
    }
}

pub fn build_timeline_event_wake_prompt(events: &[Value]) -> String {
    let mut lines = vec![
        format!(
            "{TIMELINE_EVENT_WAKE_CUE} Something about this conversation just changed."
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

fn platform_name(platform: &str) -> &str {
    match platform {
        "discord" => "Discord",
        "slack" => "Slack",
        _ => platform,
    }
}

pub fn humanize_channel_delivery_failure(address_token: &str, raw_message: &str) -> String {
    let trimmed = raw_message.trim();
    let Some(address) = parse_channel_address(address_token) else {
        return format!(
            "\"{address_token}\" isn't a valid channel address, so that message wasn't delivered."
        );
    };
    if trimmed.to_ascii_lowercase().contains("not a valid channel address") {
        return format!(
            "\"{address_token}\" isn't a valid channel address, so that message wasn't delivered."
        );
    }
    if trimmed == "No channel delivery mechanism is registered." {
        return "Channel messaging isn't available on this device, so that message wasn't delivered."
            .to_string();
    }
    if trimmed.to_ascii_lowercase().contains("no live ")
        && trimmed.to_ascii_lowercase().contains(" connection")
    {
        let name = platform_name(&address.platform);
        return format!(
            "{name} isn't connected on this device, so that message wasn't delivered. Connect {name} to send there."
        );
    }
    format!(
        "Couldn't deliver that message to {}: {}",
        platform_name(&address.platform),
        trimmed
    )
}

pub fn build_channel_delivery_failure_wake_prompt(failures: &[ChannelDeliveryFailure]) -> String {
    let lines = failures
        .iter()
        .map(|failure| format!("- To {}: {}", failure.address_token, failure.reason))
        .collect::<Vec<_>>();
    let plural = if failures.len() == 1 { "" } else { "s" };
    [
        format!(
            "{CHANNEL_DELIVERY_FAILED_WAKE_CUE} A message you tried to send to a channel did not go through."
        ),
        "This is a system notice about your own outbound send, not the user typing in this app. You may have already told the user it was sent, so correct the record.".to_string(),
        lines.join("\n"),
        format!(
            "Tell the user plainly here, in this in-app chat, that the message{plural} didn't go through and why, so they aren't left believing it was delivered. Don't silently retry the same channel; if it isn't connected, offer to help connect it."
        ),
    ]
    .join("\n")
}


pub const TIMELINE_EVENT_WAKE_CUE: &str = "[event]";

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

pub fn distinct_inbound_channel_addresses(envelopes: &[Value]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for address in envelopes
        .iter()
        .filter_map(|envelope| envelope.get("address"))
        .filter_map(format_channel_address_value)
    {
        if seen.insert(address.clone()) {
            out.push(address);
        }
    }
    out
}

pub fn build_channel_inbound_wake_prompt(envelopes: &[Value]) -> String {
    let has_message = envelopes
        .iter()
        .any(|envelope| envelope.get("reaction").map_or(true, Value::is_null));
    let plural = if envelopes.len() == 1 { "" } else { "s" };
    let opening = if has_message {
        format!("{CHANNEL_INBOUND_WAKE_CUE} New message{plural} on a channel you are connected to.")
    } else {
        format!("{CHANNEL_INBOUND_WAKE_CUE} New reaction{plural} on a channel you are connected to.")
    };

    let mut grouped = std::collections::BTreeMap::<String, Vec<&Value>>::new();
    for envelope in envelopes {
        let Some(token) = envelope
            .get("address")
            .and_then(format_channel_address_value)
        else {
            continue;
        };
        grouped.entry(token).or_default().push(envelope);
    }

    let mut blocks = Vec::new();
    for (token, bucket) in grouped {
        let platform = parse_channel_address(&token)
            .map(|address| platform_name(&address.platform).to_string())
            .unwrap_or_default();
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
        blocks.push(format!("On {platform}, from {token}:\n{}", lines.join("\n")));
    }

    let closing = if has_message {
        "Reply to them through the authorized channel target shown above. Acknowledge briefly, then send progress/results as short chat messages; do not leave the sender hanging."
    } else {
        "You do not need to reply to a reaction unless it is useful; if you respond, use the authorized channel target shown above."
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

pub fn redrivable_inbound_envelopes(envelopes: &[Value]) -> Vec<Value> {
    envelopes
        .iter()
        .filter(|envelope| envelope.get("isRedriven").and_then(Value::as_bool) != Some(true))
        .map(|envelope| {
            let mut next = envelope.clone();
            if let Some(object) = next.as_object_mut() {
                object.insert("isDisplayed".into(), Value::Bool(true));
                object.insert("isRedriven".into(), Value::Bool(true));
            }
            next
        })
        .collect()
}

pub fn describe_timeline_event(event: &Value) -> String {
    match event.get("type").and_then(Value::as_str).unwrap_or_default() {
        "name-changed" => format!(
            "Renamed to {}",
            event.get("to").and_then(Value::as_str).unwrap_or_default()
        ),
        "channel-connected" => format!(
            "Connected to {}",
            event.get("label").and_then(Value::as_str).unwrap_or_default()
        ),
        "channel-disconnected" => format!(
            "Disconnected from {}",
            event.get("label").and_then(Value::as_str).unwrap_or_default()
        ),
        "automation-changed" => {
            let action = match event.get("action").and_then(Value::as_str).unwrap_or_default() {
                "created" => "Created",
                "updated" => "Updated",
                "enabled" => "Enabled",
                "disabled" => "Disabled",
                "deleted" => "Deleted",
                _ => "Changed",
            };
            format!(
                "{action} automation \"{}\"",
                event.get("automationName").and_then(Value::as_str).unwrap_or_default()
            )
        }
        _ => "Updated this conversation".to_string(),
    }
}

pub fn build_timeline_event_wake_prompt(events: &[Value]) -> String {
    let mut lines = vec![
        format!("{TIMELINE_EVENT_WAKE_CUE} Something about this conversation just changed."),
        "This is a system event recorded in your timeline, not the user typing in this app, and possibly something you did yourself.".to_string(),
    ];
    lines.extend(events.iter().map(|event| format!("- {}", describe_timeline_event(event))));
    lines.push(
        "If it is worth acknowledging to the user, reply through the normal canonical send path; otherwise it is fine to stay silent."
            .to_string(),
    );
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn channel_address_requires_platform_and_chat() {
        assert_eq!(
            parse_channel_address(" slack : C123 "),
            Some(ChannelAddress {
                platform: "slack".into(),
                chat: "C123".into(),
            })
        );
        assert_eq!(
            format_channel_address(&parse_channel_address("slack:C123").unwrap()),
            "slack:C123"
        );
        assert!(parse_channel_address("slack").is_none());
        assert!(parse_channel_address(":C123").is_none());
        assert!(parse_channel_address("slack: ").is_none());
    }

    #[test]
    fn outbound_text_prefers_first_image_as_attachment() {
        assert_eq!(
            build_channel_outbound_message(&json!({
                "type":"text",
                "content":"caption",
                "images":[{"url":""},{"url":"fabushi-attachment://a"}]
            })),
            Some(ChannelOutboundMessage::Attachment {
                url: "fabushi-attachment://a".into(),
                caption: Some("caption".into()),
            })
        );
        assert_eq!(
            build_channel_outbound_message(&json!({"type":"text","content":"hello"})),
            Some(ChannelOutboundMessage::Text { text:"hello".into() })
        );
    }

    #[test]
    fn attachment_without_url_degrades_to_text_caption() {
        assert_eq!(
            build_channel_outbound_message(&json!({
                "type":"attachment",
                "url":"",
                "alt":"fallback"
            })),
            Some(ChannelOutboundMessage::Text {
                text:"fallback".into()
            })
        );
        assert!(build_channel_outbound_message(&json!({"type":"widget"})).is_none());
    }


    #[test]
    fn inbound_wake_groups_addresses_and_redrive_is_one_shot() {
        let envelopes = vec![
            json!({"address":{"platform":"slack","chat":"C1"},"sender":"Ada","text":"ship it"}),
            json!({"address":{"platform":"slack","chat":"C1"},"sender":"Bob","reaction":{"emoji":"✅","messageQuote":"done"}}),
            json!({"address":"discord:room","sender":"Cara","text":"status?"}),
        ];
        assert_eq!(
            distinct_inbound_channel_addresses(&envelopes),
            vec!["slack:C1".to_string(), "discord:room".to_string()]
        );
        let prompt = build_channel_inbound_wake_prompt(&envelopes);
        assert!(prompt.starts_with(CHANNEL_INBOUND_WAKE_CUE));
        assert!(prompt.contains("On Slack, from slack:C1:"));
        assert!(prompt.contains("Ada: ship it"));
        assert!(prompt.contains("reacted ✅ to your message"));
        assert!(prompt.contains("On Discord, from discord:room:"));

        let redriven = redrivable_inbound_envelopes(&[
            json!({"id":"a"}),
            json!({"id":"b","isRedriven":true}),
        ]);
        assert_eq!(redriven.len(), 1);
        assert_eq!(redriven[0].get("id").and_then(Value::as_str), Some("a"));
        assert_eq!(redriven[0].get("isDisplayed").and_then(Value::as_bool), Some(true));
        assert_eq!(redriven[0].get("isRedriven").and_then(Value::as_bool), Some(true));
        assert!(redrivable_inbound_envelopes(&redriven).is_empty());
    }

    #[test]
    fn timeline_wake_preserves_system_event_identity() {
        let prompt = build_timeline_event_wake_prompt(&[
            json!({"type":"name-changed","to":"Research"}),
            json!({"type":"automation-changed","action":"enabled","automationName":"Daily brief"}),
        ]);
        assert!(prompt.starts_with(TIMELINE_EVENT_WAKE_CUE));
        assert!(prompt.contains("Renamed to Research"));
        assert!(prompt.contains("Enabled automation \"Daily brief\""));
        assert!(prompt.contains("not the user typing"));
    }

    #[test]
    fn inbound_wake_groups_addresses_redrives_once_and_dedupes_revival() {
        let envelopes = vec![
            json!({"address":{"platform":"slack","chat":"C123"},"sender":"Ada","text":"hello"}),
            json!({"address":{"platform":"slack","chat":"C123"},"sender":"Lin","reaction":{"emoji":"👍","messageQuote":"done"}}),
        ];
        assert_eq!(distinct_inbound_channel_addresses(&envelopes), vec!["slack:C123"]);
        let prompt = build_channel_inbound_wake_prompt(&envelopes);
        assert!(prompt.starts_with(CHANNEL_INBOUND_WAKE_CUE));
        assert!(prompt.contains("On Slack, from slack:C123"));
        assert!(prompt.contains("Ada: hello"));
        assert!(prompt.contains("Lin reacted 👍 to your message: \"done\""));

        let redriven = redrivable_inbound_envelopes(&envelopes);
        assert_eq!(redriven.len(), 2);
        assert!(redriven.iter().all(|value| value["isDisplayed"] == true && value["isRedriven"] == true));
        assert!(redrivable_inbound_envelopes(&redriven).is_empty());

        let mut wakes = BackgroundWakes::<Value>::default();
        BackgroundWakes::enqueue(&mut wakes.pending_inbound, "agent-a", envelopes[0].clone());
        assert!(BackgroundWakes::<Value>::begin_revival(&mut wakes.reviving_inbound_agent_ids, "agent-a"));
        assert!(!BackgroundWakes::<Value>::begin_revival(&mut wakes.reviving_inbound_agent_ids, "agent-a"));
        assert_eq!(BackgroundWakes::take_pending(&mut wakes.pending_inbound, "agent-a").len(), 1);
        BackgroundWakes::<Value>::end_revival(&mut wakes.reviving_inbound_agent_ids, "agent-a");
        assert!(wakes.reviving_inbound_agent_ids.is_empty());
    }

    #[test]
    fn timeline_wake_preserves_user_visible_event_descriptions() {
        let prompt = build_timeline_event_wake_prompt(&[
            json!({"type":"name-changed","to":"Research"}),
            json!({"type":"automation-changed","action":"enabled","automationName":"Daily brief"}),
        ]);
        assert!(prompt.starts_with(TIMELINE_EVENT_WAKE_CUE));
        assert!(prompt.contains("- Renamed to Research"));
        assert!(prompt.contains("- Enabled automation \"Daily brief\""));
    }

    #[test]
    fn inbound_wake_groups_channels_and_distinguishes_reactions() {
        let messages = vec![
            json!({"address":{"platform":"slack","chat":"C1"},"sender":"Ada","text":"hello"}),
            json!({"address":{"platform":"slack","chat":"C1"},"sender":"Lin","text":"status?"}),
            json!({"address":"discord:room-1","sender":"Grace","text":"ship it"}),
        ];
        assert_eq!(
            distinct_inbound_channel_addresses(&messages),
            vec!["slack:C1".to_string(), "discord:room-1".to_string()]
        );
        let wake = build_channel_inbound_wake_prompt(&messages);
        assert!(wake.starts_with(CHANNEL_INBOUND_WAKE_CUE));
        assert!(wake.contains("New messages"));
        assert!(wake.contains("On Slack, from slack:C1:"));
        assert!(wake.contains("Ada: hello"));
        assert!(wake.contains("discord:room-1"));

        let reactions = vec![json!({
            "address":{"platform":"slack","chat":"C1"},
            "sender":"Ada",
            "reaction":{"emoji":"👍","messageQuote":"done"}
        })];
        let wake = build_channel_inbound_wake_prompt(&reactions);
        assert!(wake.contains("New reaction"));
        assert!(wake.contains("reacted 👍 to your message"));
    }

    #[test]
    fn redrive_and_revival_are_idempotent_and_agent_scoped() {
        let envelopes = vec![
            json!({"id":"fresh"}),
            json!({"id":"already","isRedriven":true}),
        ];
        let redrivable = redrivable_inbound_envelopes(&envelopes);
        assert_eq!(redrivable.len(), 1);
        assert_eq!(redrivable[0]["id"], "fresh");
        assert_eq!(redrivable[0]["isDisplayed"], true);
        assert_eq!(redrivable[0]["isRedriven"], true);

        let mut wakes = BackgroundWakes::<Value>::default();
        BackgroundWakes::enqueue(&mut wakes.pending_inbound, "agent-a", json!({"id":1}));
        BackgroundWakes::enqueue(&mut wakes.pending_inbound, "agent-a", json!({"id":2}));
        assert!(BackgroundWakes::<Value>::begin_revival(
            &mut wakes.reviving_inbound_agent_ids,
            "agent-a",
        ));
        assert!(!BackgroundWakes::<Value>::begin_revival(
            &mut wakes.reviving_inbound_agent_ids,
            "agent-a",
        ));
        let pending = BackgroundWakes::take_pending(&mut wakes.pending_inbound, "agent-a");
        assert_eq!(pending.len(), 2);
        assert!(BackgroundWakes::take_pending(&mut wakes.pending_inbound, "agent-a").is_empty());
        BackgroundWakes::<Value>::end_revival(
            &mut wakes.reviving_inbound_agent_ids,
            "agent-a",
        );
        assert!(BackgroundWakes::<Value>::begin_revival(
            &mut wakes.reviving_inbound_agent_ids,
            "agent-a",
        ));
    }

    #[test]
    fn timeline_wake_preserves_desktop_event_descriptions() {
        let events = vec![
            json!({"type":"name-changed","to":"Research"}),
            json!({"type":"channel-connected","label":"Slack"}),
            json!({"type":"automation-changed","action":"enabled","automationName":"Daily brief"}),
        ];
        let wake = build_timeline_event_wake_prompt(&events);
        assert!(wake.starts_with(TIMELINE_EVENT_WAKE_CUE));
        assert!(wake.contains("- Renamed to Research"));
        assert!(wake.contains("- Connected to Slack"));
        assert!(wake.contains("- Enabled automation \"Daily brief\""));
    }

    #[test]
    fn delivery_failure_is_humanized_and_never_requests_blind_retry() {
        assert_eq!(
            humanize_channel_delivery_failure(
                "slack:C123",
                "No channel delivery mechanism is registered."
            ),
            "Channel messaging isn't available on this device, so that message wasn't delivered."
        );
        let wake = build_channel_delivery_failure_wake_prompt(&[ChannelDeliveryFailure {
            address_token:"slack:C123".into(),
            reason:"Slack isn't connected".into(),
        }]);
        assert!(wake.starts_with(CHANNEL_DELIVERY_FAILED_WAKE_CUE));
        assert!(wake.contains("Don't silently retry"));
        assert!(wake.contains("- To slack:C123: Slack isn't connected"));
    }
}
