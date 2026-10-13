use std::sync::{Arc, RwLock};

use crate::extensions::transcript::channel_delivery_unregistered_error::SandChannelDeliveryUnregisteredError;

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
