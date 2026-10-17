use crate::{CoordinatorFailure, CoordinatorReply};

pub const COORDINATOR_PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LifecyclePhase {
    Hello,
    Ready,
    ShutdownRequested,
    ShutdownProtocolError(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoordinatorFrame {
    Lifecycle {
        phase: LifecyclePhase,
        protocol_version: Option<u32>,
    },
    Request {
        request_id: String,
        method: String,
        args_json: String,
    },
    Cancel {
        request_id: String,
    },
    Reply(CoordinatorReply),
    Event {
        family: String,
        payload_json: String,
    },
}

impl CoordinatorFrame {
    pub fn validate(&self) -> Result<(), CoordinatorFailure> {
        match self {
            Self::Lifecycle {
                phase: LifecyclePhase::Hello | LifecyclePhase::Ready,
                protocol_version: Some(version),
            } if *version == COORDINATOR_PROTOCOL_VERSION => Ok(()),
            Self::Lifecycle {
                phase: LifecyclePhase::Hello | LifecyclePhase::Ready,
                protocol_version: Some(version),
            } => Err(CoordinatorFailure::protocol(format!(
                "unsupported coordinator protocol version {version}"
            ))),
            Self::Lifecycle {
                phase: LifecyclePhase::ShutdownRequested,
                protocol_version: None,
            } => Ok(()),
            Self::Lifecycle {
                phase: LifecyclePhase::ShutdownProtocolError(detail),
                protocol_version: None,
            } if !detail.trim().is_empty() => Ok(()),
            Self::Request {
                request_id,
                method,
                ..
            } if !request_id.trim().is_empty() && !method.trim().is_empty() => Ok(()),
            Self::Cancel { request_id } if !request_id.trim().is_empty() => Ok(()),
            Self::Reply(reply) if !reply.request_id.trim().is_empty() => Ok(()),
            Self::Event { family, .. } if !family.trim().is_empty() => Ok(()),
            _ => Err(CoordinatorFailure::protocol("malformed coordinator frame")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_wrong_protocol_version() {
        let frame = CoordinatorFrame::Lifecycle {
            phase: LifecyclePhase::Hello,
            protocol_version: Some(COORDINATOR_PROTOCOL_VERSION + 1),
        };
        assert!(frame.validate().is_err());
    }
}
