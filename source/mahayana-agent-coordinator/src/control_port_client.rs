use std::collections::BTreeSet;
use fabushi_android_shared::{CoordinatorFailure, CoordinatorFailureCode};
use fabushi_android_shared::rpc::coordinator_port::{CoordinatorFrame, LifecyclePhase, COORDINATOR_PROTOCOL_VERSION};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlPhase { AwaitingReady, Serving, Settled }

pub struct ControlPortClient {
    phase: ControlPhase,
    next_request_id: u64,
    pending: BTreeSet<String>,
}

impl ControlPortClient {
    pub fn new() -> Self { Self { phase: ControlPhase::AwaitingReady, next_request_id: 0, pending: BTreeSet::new() } }
    pub fn hello(&self) -> CoordinatorFrame {
        CoordinatorFrame::Lifecycle { phase: LifecyclePhase::Hello, protocol_version: Some(COORDINATOR_PROTOCOL_VERSION) }
    }
    pub fn handle(&mut self, frame: &CoordinatorFrame) -> Result<(), CoordinatorFailure> {
        frame.validate()?;
        match (&self.phase, frame) {
            (ControlPhase::AwaitingReady, CoordinatorFrame::Lifecycle { phase: LifecyclePhase::Ready, .. }) => { self.phase = ControlPhase::Serving; Ok(()) }
            (_, CoordinatorFrame::Lifecycle { phase: LifecyclePhase::ShutdownRequested | LifecyclePhase::ShutdownProtocolError(_), .. }) => {
                self.phase = ControlPhase::Settled;
                self.pending.clear();
                Ok(())
            }
            (ControlPhase::Serving, CoordinatorFrame::Reply(reply)) => { self.pending.remove(&reply.request_id); Ok(()) }
            (_, CoordinatorFrame::Event { .. }) => Ok(()),
            _ => Err(CoordinatorFailure::new(CoordinatorFailureCode::ProtocolBreach, "invalid frame direction or phase on control client")),
        }
    }
    pub fn begin_call(&mut self, method: impl Into<String>, args_json: impl Into<String>) -> Result<CoordinatorFrame, CoordinatorFailure> {
        if self.phase != ControlPhase::Serving {
            return Err(CoordinatorFailure::protocol("control client is not serving"));
        }
        self.next_request_id += 1;
        let request_id = format!("c-{}", self.next_request_id);
        self.pending.insert(request_id.clone());
        Ok(CoordinatorFrame::Request { request_id, method: method.into(), args_json: args_json.into() })
    }
    pub fn phase(&self) -> ControlPhase { self.phase }
    pub fn pending_count(&self) -> usize { self.pending.len() }
}

impl Default for ControlPortClient { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requires_ready_before_calls() {
        let mut client = ControlPortClient::new();
        assert!(client.begin_call("x", "{}").is_err());
        client.handle(&CoordinatorFrame::Lifecycle { phase: LifecyclePhase::Ready, protocol_version: Some(COORDINATOR_PROTOCOL_VERSION) }).unwrap();
        assert!(client.begin_call("x", "{}").is_ok());
    }
}
