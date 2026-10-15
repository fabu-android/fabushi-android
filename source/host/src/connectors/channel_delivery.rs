use std::sync::{Arc, RwLock};

use serde_json::Value;

use crate::extensions::transcript::channel_delivery_unregistered_error::SandChannelDeliveryUnregisteredError;


pub const MAX_CHANNEL_INBOUND_ENVELOPE_BYTES: usize = 256 * 1024;

pub fn validate_channel_inbound_envelope(envelope: &Value) -> Result<(), String> {
    let object = envelope.as_object().ok_or("channel inbound envelope must be an object")?;
    let address = object.get("address").ok_or("channel inbound envelope requires address")?;
    let address_valid = if let Some(raw) = address.as_str() {
        let token = raw.trim();
        token.len() <= 2048 && token.split_once(':').is_some_and(|(platform, chat)| !platform.trim().is_empty() && !chat.trim().is_empty())
    } else {
        let platform = address.get("platform").and_then(Value::as_str).unwrap_or_default().trim();
        let chat = address.get("chat").and_then(Value::as_str).unwrap_or_default().trim();
        !platform.is_empty() && !chat.is_empty() && platform.len() <= 128 && chat.len() <= 1900
    };
    if !address_valid { return Err("channel inbound envelope address is invalid".into()); }
    let sender = object.get("sender").and_then(Value::as_str).unwrap_or_default();
    if sender.is_empty() || sender.len() > 4096 || sender.chars().any(char::is_control) {
        return Err("channel inbound envelope sender is invalid".into());
    }
    let has_text = object.get("text").and_then(Value::as_str).is_some_and(|text| !text.is_empty());
    let has_reaction = object.get("reaction").is_some_and(|reaction| !reaction.is_null());
    if has_text == has_reaction {
        return Err("channel inbound envelope must contain exactly one message or reaction payload".into());
    }
    let encoded = serde_json::to_vec(envelope).map_err(|error| format!("failed to encode channel inbound envelope: {error}"))?;
    if encoded.len() > MAX_CHANNEL_INBOUND_ENVELOPE_BYTES {
        return Err("channel inbound envelope exceeds bounded 256 KiB payload".into());
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelOutboundMessage {
    Text { text: String },
    Attachment { url: String, caption: Option<String> },
}

pub trait ChannelDeliveryPort: Send + Sync {
    fn deliver(
        &self,
        agent_id: &str,
        address_token: &str,
        message: &ChannelOutboundMessage,
    ) -> Result<(), String>;
}

#[derive(Default)]
pub struct UnregisteredChannelDeliveryPort;

impl ChannelDeliveryPort for UnregisteredChannelDeliveryPort {
    fn deliver(
        &self,
        _agent_id: &str,
        _address_token: &str,
        _message: &ChannelOutboundMessage,
    ) -> Result<(), String> {
        Err(SandChannelDeliveryUnregisteredError.to_string())
    }
}

#[derive(Clone)]
pub struct HostChannelDelivery {
    port: Arc<RwLock<Arc<dyn ChannelDeliveryPort>>>,
}

impl Default for HostChannelDelivery {
    fn default() -> Self {
        Self::new(Arc::new(UnregisteredChannelDeliveryPort))
    }
}

impl HostChannelDelivery {
    pub fn new(port: Arc<dyn ChannelDeliveryPort>) -> Self {
        Self {
            port: Arc::new(RwLock::new(port)),
        }
    }

    pub fn set_port(&self, port: Arc<dyn ChannelDeliveryPort>) {
        *self
            .port
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = port;
    }

    pub fn reset(&self) {
        self.set_port(Arc::new(UnregisteredChannelDeliveryPort));
    }

    pub fn deliver(
        &self,
        agent_id: &str,
        address_token: &str,
        message: &ChannelOutboundMessage,
    ) -> Result<(), String> {
        let port = self
            .port
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        port.deliver(agent_id, address_token, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inbound_envelope_contract_is_bounded_and_fail_closed() {
        let valid = serde_json::json!({
            "address":{"platform":"slack","chat":"C1"},
            "sender":"alice",
            "text":"hello"
        });
        assert!(validate_channel_inbound_envelope(&valid).is_ok());
        assert!(validate_channel_inbound_envelope(&serde_json::json!({
            "address":{"platform":"slack","chat":""},
            "sender":"alice",
            "text":"hello"
        })).is_err());
        assert!(validate_channel_inbound_envelope(&serde_json::json!({
            "address":{"platform":"slack","chat":"C1"},
            "sender":"alice",
            "text":"hello",
            "reaction":{"emoji":"+1"}
        })).is_err());
    }

    #[test]
    fn default_delivery_fails_closed_until_a_port_is_registered() {
        let delivery = HostChannelDelivery::default();
        let error = delivery
            .deliver(
                "agent-1",
                "address-1",
                &ChannelOutboundMessage::Text {
                    text: "hello".into(),
                },
            )
            .unwrap_err();
        assert_eq!(error, "No channel delivery mechanism is registered.");
    }
}
