use std::collections::BTreeSet;
use fabushi_android_shared::{CoordinatorFailure, CoordinatorFailureCode, CoordinatorReply};
use fabushi_android_shared::rpc::coordinator_port::{CoordinatorFrame, LifecyclePhase, COORDINATOR_PROTOCOL_VERSION};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RendererPortPhase { AwaitingHello, Serving, Settled }

pub struct RendererPortServer {
    phase: RendererPortPhase,
    in_flight: BTreeSet<String>,
}

impl RendererPortServer {
    pub fn new() -> Self { Self { phase: RendererPortPhase::AwaitingHello, in_flight: BTreeSet::new() } }

    pub fn handle(&mut self, frame: CoordinatorFrame) -> Result<Option<CoordinatorFrame>, CoordinatorFailure> {
        frame.validate()?;
        match (self.phase, frame) {
            (RendererPortPhase::AwaitingHello, CoordinatorFrame::Lifecycle { phase: LifecyclePhase::Hello, protocol_version: Some(COORDINATOR_PROTOCOL_VERSION) }) => {
                self.phase = RendererPortPhase::Serving;
                Ok(Some(CoordinatorFrame::Lifecycle { phase: LifecyclePhase::Ready, protocol_version: Some(COORDINATOR_PROTOCOL_VERSION) }))
            }
            (RendererPortPhase::Serving, CoordinatorFrame::Request { request_id, .. }) => {
                if !self.in_flight.insert(request_id) {
                    return Err(CoordinatorFailure::protocol("request id reused while in flight"));
                }
                Ok(None)
            }
            (RendererPortPhase::Serving, CoordinatorFrame::Cancel { request_id }) => {
                if self.in_flight.remove(&request_id) {
                    Ok(Some(CoordinatorFrame::Reply(CoordinatorReply::failed(
                        request_id,
                        CoordinatorFailure::new(CoordinatorFailureCode::Cancelled, "request cancelled"),
                    ))))
                } else { Ok(None) }
            }
            (_, CoordinatorFrame::Lifecycle { phase: LifecyclePhase::ShutdownRequested, .. }) => {
                self.phase = RendererPortPhase::Settled;
                self.in_flight.clear();
                Ok(None)
            }
            _ => Err(CoordinatorFailure::protocol("invalid frame direction or phase on renderer server")),
        }
    }

    pub fn settle_request(&mut self, request_id: &str) { self.in_flight.remove(request_id); }
    pub fn phase(&self) -> RendererPortPhase { self.phase }
}

impl Default for RendererPortServer { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handshake_and_duplicate_request_are_fail_closed() {
        let mut server=RendererPortServer::new();
        let ready=server.handle(CoordinatorFrame::Lifecycle { phase: LifecyclePhase::Hello, protocol_version: Some(COORDINATOR_PROTOCOL_VERSION) }).unwrap();
        assert!(matches!(ready, Some(CoordinatorFrame::Lifecycle { phase: LifecyclePhase::Ready, .. })));
        let request=CoordinatorFrame::Request { request_id:"r".into(),method:"m".into(),args_json:"{}".into() };
        server.handle(request.clone()).unwrap();
        assert!(server.handle(request).is_err());
    }
}
