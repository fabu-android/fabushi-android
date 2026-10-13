use serde_json::Value;

use super::channel_delivery::ChannelOutboundMessage;

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
