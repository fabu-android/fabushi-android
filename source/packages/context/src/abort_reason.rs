#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AbortReason { UserCancelled, DeadlineExceeded, ProcessRecreated, HostCrashed, TransportLost, ProtocolError }
