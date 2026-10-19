#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("No channel delivery mechanism is registered.")]
pub struct SandChannelDeliveryUnregisteredError;
