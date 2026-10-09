use std::collections::{BTreeMap, VecDeque};

use fabushi_android_internal::MonotonicSequence;
use serde_json::Value;

use crate::client_side_tool_v2_relay::{ClientSideToolV2Relay, RendererToolEvent};
use fabushi_android_shared::{
    CancelRequest, CoordinatorEvent, CoordinatorFailure, CoordinatorFailureCode, CoordinatorReply,
    CoordinatorRequest, ResyncRequest, ResyncSnapshot,
};

pub const DEFAULT_EVENT_REPLAY_LIMIT: usize = 512;

pub trait HostPort {
    fn execute(&mut self, request: &CoordinatorRequest) -> Result<String, CoordinatorFailure>;
    fn cancel(&mut self, request_id: &str, reason: Option<&str>) -> Result<(), CoordinatorFailure>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingRequest {
    session_id: String,
    method: String,
    operation_id: Option<String>,
}

pub struct MahayanaCoordinator<H: HostPort> {
    host: H,
    generation: u64,
    sequence: MonotonicSequence,
    pending: BTreeMap<String, PendingRequest>,
    events: VecDeque<CoordinatorEvent>,
    replay_limit: usize,
    client_side_tool_v2: ClientSideToolV2Relay,
}

impl<H: HostPort> MahayanaCoordinator<H> {
    pub fn new(host: H) -> Self { Self::with_generation(host, 1, DEFAULT_EVENT_REPLAY_LIMIT) }

    pub fn with_replay_limit(host: H, replay_limit: usize) -> Self {
        Self::with_generation(host, 1, replay_limit)
    }

    pub fn with_generation(host: H, generation: u64, replay_limit: usize) -> Self {
        Self {
            host,
            generation: generation.max(1),
            sequence: MonotonicSequence::default(),
            pending: BTreeMap::new(),
            events: VecDeque::new(),
            replay_limit: replay_limit.max(1),
            client_side_tool_v2: ClientSideToolV2Relay::default(),
        }
    }

    pub fn generation(&self) -> u64 { self.generation }

    pub fn accept_client_side_tool_v2_wire(&mut self, raw: Value) -> Option<RendererToolEvent> {
        self.client_side_tool_v2.accept_value(raw)
    }

    pub fn replay_client_side_tool_v2(&self) -> Vec<RendererToolEvent> {
        self.client_side_tool_v2.replay()
    }

    pub fn retire_client_side_tool_v2_for_account_switch(&mut self) {
        self.client_side_tool_v2.retire_for_account_switch();
    }

    pub fn begin_request(&mut self, request: &CoordinatorRequest) -> Result<(), CoordinatorFailure> {
        request.validate()?;
        if self.pending.contains_key(&request.request_id) {
            return Err(CoordinatorFailure::new(
                CoordinatorFailureCode::DuplicateRequest,
                format!("request {} is already active", request.request_id),
            ));
        }
        self.pending.insert(request.request_id.clone(), PendingRequest {
            session_id: request.session_id.clone(),
            method: request.method.clone(),
            operation_id: None,
        });
        Ok(())
    }

    pub fn complete_request(&mut self, request_id: &str, result: Result<String, CoordinatorFailure>) -> CoordinatorReply {
        if self.pending.remove(request_id).is_none() {
            return CoordinatorReply::failed(
                request_id,
                CoordinatorFailure::new(CoordinatorFailureCode::UnknownRequest, "request is not active"),
            );
        }
        match result {
            Ok(value) => CoordinatorReply::ok(request_id, value),
            Err(error) => CoordinatorReply::failed(request_id, error),
        }
    }

    pub fn request(&mut self, request: CoordinatorRequest) -> CoordinatorReply {
        if let Err(error) = self.begin_request(&request) {
            return CoordinatorReply::failed(request.request_id, error);
        }
        let request_id = request.request_id.clone();
        let result = self.host.execute(&request);
        self.complete_request(&request_id, result)
    }

    /// Dispatches a request whose accepted operation remains active until a terminal Host event
    /// or explicit cancellation settles it.
    pub fn request_deferred(&mut self, request: CoordinatorRequest) -> CoordinatorReply {
        if let Err(error) = self.begin_request(&request) {
            return CoordinatorReply::failed(request.request_id, error);
        }
        let request_id = request.request_id.clone();
        match self.host.execute(&request) {
            Ok(value) => CoordinatorReply::ok(request_id, value),
            Err(error) => {
                self.pending.remove(&request_id);
                CoordinatorReply::failed(request_id, error)
            }
        }
    }

    pub fn bind_operation(
        &mut self,
        request_id: &str,
        operation_id: &str,
    ) -> Result<(), CoordinatorFailure> {
        if operation_id.trim().is_empty() {
            return Err(CoordinatorFailure::new(
                CoordinatorFailureCode::MalformedRequest,
                "operation id must not be empty",
            ));
        }
        if self.pending.values().any(|pending| {
            pending.operation_id.as_deref() == Some(operation_id)
        }) {
            return Err(CoordinatorFailure::new(
                CoordinatorFailureCode::DuplicateRequest,
                format!("operation {operation_id} is already active"),
            ));
        }
        let pending = self.pending.get_mut(request_id).ok_or_else(|| {
            CoordinatorFailure::new(
                CoordinatorFailureCode::UnknownRequest,
                "request is not active",
            )
        })?;
        pending.operation_id = Some(operation_id.to_string());
        Ok(())
    }

    pub fn complete_operation(
        &mut self,
        operation_id: &str,
        result: Result<String, CoordinatorFailure>,
    ) -> CoordinatorReply {
        let request_id = self
            .pending
            .iter()
            .find_map(|(request_id, pending)| {
                (pending.operation_id.as_deref() == Some(operation_id)
                    || request_id == operation_id)
                    .then(|| request_id.clone())
            });
        match request_id {
            Some(request_id) => self.complete_request(&request_id, result),
            None => CoordinatorReply::failed(
                operation_id,
                CoordinatorFailure::new(
                    CoordinatorFailureCode::UnknownRequest,
                    "operation is not active",
                ),
            ),
        }
    }

    pub fn record_operation_event(
        &mut self,
        session_id: impl Into<String>,
        family: impl Into<String>,
        payload_json: impl Into<String>,
        operation_id: Option<&str>,
        terminal: bool,
    ) -> CoordinatorEvent {
        let event = self.publish_event(session_id, family, payload_json);
        if terminal {
            if let Some(operation_id) = operation_id {
                let _ = self.complete_operation(operation_id, Ok("{}".into()));
            }
        }
        event
    }

    pub fn cancel_operation(
        &mut self,
        operation_id: &str,
        reason: Option<&str>,
    ) -> CoordinatorReply {
        let request_id = self
            .pending
            .iter()
            .find_map(|(request_id, pending)| {
                (pending.operation_id.as_deref() == Some(operation_id)
                    || request_id == operation_id)
                    .then(|| request_id.clone())
            });
        let Some(request_id) = request_id else {
            return CoordinatorReply::failed(
                operation_id,
                CoordinatorFailure::new(
                    CoordinatorFailureCode::UnknownRequest,
                    "operation is not active",
                ),
            );
        };
        let host_result = self.host.cancel(operation_id, reason);
        self.pending.remove(&request_id);
        match host_result {
            Ok(()) => CoordinatorReply::failed(
                request_id,
                CoordinatorFailure::new(
                    CoordinatorFailureCode::Cancelled,
                    "request cancelled",
                ),
            ),
            Err(error) => CoordinatorReply::failed(request_id, error),
        }
    }

    pub fn cancel(&mut self, cancel: CancelRequest) -> CoordinatorReply {
        if !self.pending.contains_key(&cancel.request_id) {
            return CoordinatorReply::failed(
                cancel.request_id,
                CoordinatorFailure::new(CoordinatorFailureCode::UnknownRequest, "request is not active"),
            );
        }
        let operation_id = self.pending
            .get(&cancel.request_id)
            .and_then(|pending| pending.operation_id.as_deref())
            .unwrap_or(cancel.request_id.as_str())
            .to_string();
        let host_result = self.host.cancel(&operation_id, cancel.reason.as_deref());
        self.pending.remove(&cancel.request_id);
        match host_result {
            Ok(()) => CoordinatorReply::failed(
                cancel.request_id,
                CoordinatorFailure::new(CoordinatorFailureCode::Cancelled, "request cancelled"),
            ),
            Err(error) => CoordinatorReply::failed(cancel.request_id, error),
        }
    }

    pub fn emit_for_request(
        &mut self,
        request_id: &str,
        family: impl Into<String>,
        payload_json: impl Into<String>,
    ) -> Result<CoordinatorEvent, CoordinatorFailure> {
        let session_id = self.pending.get(request_id)
            .map(|request| request.session_id.clone())
            .ok_or_else(|| CoordinatorFailure::new(CoordinatorFailureCode::UnknownRequest, "request is not active"))?;
        Ok(self.publish_event(session_id, family, payload_json))
    }

    pub fn publish_event(&mut self, session_id: impl Into<String>, family: impl Into<String>, payload_json: impl Into<String>) -> CoordinatorEvent {
        let sequence = self.sequence.next_value();
        let event = CoordinatorEvent {
            event_id: format!("g{}-e{}", self.generation, sequence),
            session_id: session_id.into(),
            sequence,
            family: family.into(),
            payload_json: payload_json.into(),
        };
        if self.events.len() >= self.replay_limit { self.events.pop_front(); }
        self.events.push_back(event.clone());
        event
    }

    pub fn resync(&self, request: ResyncRequest) -> Result<ResyncSnapshot, CoordinatorFailure> {
        if request.generation != self.generation {
            return Err(CoordinatorFailure::new(
                CoordinatorFailureCode::StaleGeneration,
                format!(
                    "client generation {} is stale; current generation is {}",
                    request.generation, self.generation
                ),
            ));
        }
        if let Some(first) = self.events.front() {
            if request.after_sequence.saturating_add(1) < first.sequence {
                return Err(CoordinatorFailure::new(
                    CoordinatorFailureCode::ReplayUnavailable,
                    format!(
                        "replay gap: requested after sequence {}, earliest retained sequence is {}",
                        request.after_sequence, first.sequence
                    ),
                ));
            }
        }
        Ok(ResyncSnapshot {
            generation: self.generation,
            latest_sequence: self.sequence.current(),
            events: self.events.iter().filter(|event| event.sequence > request.after_sequence).cloned().collect(),
        })
    }

    pub fn resync_since(&self, after_sequence: u64) -> ResyncSnapshot {
        self.resync(ResyncRequest { generation: self.generation, after_sequence })
            .expect("current coordinator generation must resync")
    }

    pub fn settle_host_crash(&mut self, detail: impl Into<String>) -> Vec<CoordinatorReply> {
        let detail = detail.into();
        self.generation = self.generation.saturating_add(1);
        self.sequence = MonotonicSequence::default();
        self.events.clear();
        std::mem::take(&mut self.pending).into_keys().map(|request_id| {
            CoordinatorReply::failed(
                request_id,
                CoordinatorFailure::new(CoordinatorFailureCode::HostCrashed, detail.clone()),
            )
        }).collect()
    }

    pub fn active_request_count(&self) -> usize { self.pending.len() }

    pub fn latest_sequence(&self) -> u64 { self.sequence.current() }

    pub fn active_request_metadata(&self, request_id: &str) -> Option<(&str, &str)> {
        self.pending.get(request_id).map(|request| (request.session_id.as_str(), request.method.as_str()))
    }

    pub fn into_host(self) -> H { self.host }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fabushi_android_shared::COORDINATOR_PROTOCOL_VERSION;

    #[derive(Default)]
    struct FakeHost { cancelled: Vec<String> }

    impl HostPort for FakeHost {
        fn execute(&mut self, request: &CoordinatorRequest) -> Result<String, CoordinatorFailure> {
            Ok(format!(r#"{{"method":"{}"}}"#, request.method))
        }
        fn cancel(&mut self, request_id: &str, _reason: Option<&str>) -> Result<(), CoordinatorFailure> {
            self.cancelled.push(request_id.to_string());
            Ok(())
        }
    }

    fn request(id: &str) -> CoordinatorRequest {
        CoordinatorRequest {
            protocol_version: COORDINATOR_PROTOCOL_VERSION,
            request_id: id.into(),
            session_id: "session-a".into(),
            method: "sendPrompt".into(),
            params_json: "{}".into(),
            deadline_ms: None,
        }
    }

    #[test]
    fn normal_request_settles() {
        let mut coordinator = MahayanaCoordinator::new(FakeHost::default());
        assert!(coordinator.request(request("r1")).result_json.is_ok());
        assert_eq!(coordinator.active_request_count(), 0);
    }

    #[test]
    fn duplicate_cancel_resync_and_crash_are_deterministic() {
        let mut coordinator = MahayanaCoordinator::with_replay_limit(FakeHost::default(), 2);
        coordinator.begin_request(&request("r1")).unwrap();
        assert_eq!(coordinator.begin_request(&request("r1")).unwrap_err().code, CoordinatorFailureCode::DuplicateRequest);
        assert_eq!(coordinator.active_request_metadata("r1"), Some(("session-a", "sendPrompt")));
        let cancelled = coordinator.cancel(CancelRequest { request_id: "r1".into(), reason: Some("user".into()) });
        assert_eq!(cancelled.result_json.unwrap_err().code, CoordinatorFailureCode::Cancelled);

        coordinator.publish_event("s", "delta", "1");
        coordinator.publish_event("s", "delta", "2");
        coordinator.publish_event("s", "delta", "3");
        let gap = coordinator.resync(ResyncRequest { generation: coordinator.generation(), after_sequence: 0 }).unwrap_err();
        assert_eq!(gap.code, CoordinatorFailureCode::ReplayUnavailable);
        assert_eq!(coordinator.resync_since(1).events.iter().map(|e| e.sequence).collect::<Vec<_>>(), vec![2, 3]);

        coordinator.begin_request(&request("r2")).unwrap();
        let settled = coordinator.settle_host_crash("host exited");
        assert_eq!(settled.len(), 1);
        assert_eq!(coordinator.generation(), 2);
        assert_eq!(coordinator.active_request_count(), 0);
        assert!(coordinator.resync(ResyncRequest { generation: 1, after_sequence: 0 }).is_err());
        let fresh = coordinator.resync(ResyncRequest { generation: 2, after_sequence: 0 }).unwrap();
        assert!(fresh.events.is_empty());
    }

    #[test]
    fn deferred_operation_streams_cancels_and_settles_by_operation_id() {
        let mut coordinator = MahayanaCoordinator::with_generation(FakeHost::default(), 7, 8);
        assert_eq!(coordinator.generation(), 7);

        let accepted = coordinator.request_deferred(request("request-1"));
        assert!(accepted.result_json.is_ok());
        assert_eq!(coordinator.active_request_count(), 1);
        coordinator.bind_operation("request-1", "operation-1").unwrap();

        let first = coordinator.record_operation_event(
            "session-a",
            "chat.delta",
            r#"{"operationId":"operation-1","delta":"a"}"#,
            Some("operation-1"),
            false,
        );
        assert_eq!(first.sequence, 1);
        assert_eq!(coordinator.active_request_count(), 1);

        let cancelled = coordinator.cancel_operation("operation-1", Some("user"));
        assert_eq!(
            cancelled.result_json.unwrap_err().code,
            CoordinatorFailureCode::Cancelled
        );
        assert_eq!(coordinator.active_request_count(), 0);

        coordinator.request_deferred(request("request-2"));
        coordinator.bind_operation("request-2", "operation-2").unwrap();
        coordinator.record_operation_event(
            "session-a",
            "operation.completed",
            r#"{"operationId":"operation-2"}"#,
            Some("operation-2"),
            true,
        );
        assert_eq!(coordinator.active_request_count(), 0);
        assert_eq!(coordinator.latest_sequence(), 2);

        let replay = coordinator.resync(ResyncRequest {
            generation: 7,
            after_sequence: 0,
        }).unwrap();
        assert_eq!(replay.events.len(), 2);
    }

    #[test]
    fn streaming_replay_is_ordered_and_stale_generation_is_rejected() {
        let mut coordinator = MahayanaCoordinator::with_replay_limit(FakeHost::default(), 8);
        let generation = coordinator.generation();
        coordinator.begin_request(&request("stream-1")).unwrap();
        coordinator.emit_for_request("stream-1", "chat.delta", r#"{"delta":"a"}"#).unwrap();
        coordinator.emit_for_request("stream-1", "chat.delta", r#"{"delta":"b"}"#).unwrap();
        coordinator.emit_for_request("stream-1", "operation.completed", "{}").unwrap();
        let completed = coordinator.complete_request("stream-1", Ok(r#"{"ok":true}"#.into()));
        assert!(completed.result_json.is_ok());
        assert_eq!(
            coordinator.emit_for_request("stream-1", "chat.delta", "{}").unwrap_err().code,
            CoordinatorFailureCode::UnknownRequest
        );
        let replay = coordinator.resync(ResyncRequest { generation, after_sequence: 1 }).unwrap();
        assert_eq!(replay.events.iter().map(|event| event.sequence).collect::<Vec<_>>(), vec![2, 3]);

        coordinator.settle_host_crash("host killed");
        let error = coordinator.resync(ResyncRequest { generation, after_sequence: 0 }).unwrap_err();
        assert_eq!(error.code, CoordinatorFailureCode::StaleGeneration);
        let fresh = coordinator.resync(ResyncRequest { generation: coordinator.generation(), after_sequence: 0 }).unwrap();
        assert!(fresh.events.is_empty());
        assert_eq!(fresh.latest_sequence, 0);
    }

    #[test]
    fn coordinator_owns_client_side_tool_v2_wire_ingress() {
        let mut coordinator = MahayanaCoordinator::new(FakeHost::default());
        let accepted = coordinator.accept_client_side_tool_v2_wire(serde_json::json!({
            "version": 1,
            "kind": "call",
            "accountSlot": "host",
            "agentId": "agent-a",
            "epoch": "epoch-a",
            "sequence": 1,
            "message": {
                "encoding": "protobuf-base64",
                "messageType": "aiserver.v1.ClientSideToolV2Call",
                "bytes": "GgZjYWxsLTE="
            }
        }));
        assert!(accepted.is_some());
        assert_eq!(coordinator.replay_client_side_tool_v2().len(), 1);

        coordinator.retire_client_side_tool_v2_for_account_switch();
        assert!(coordinator.replay_client_side_tool_v2().is_empty());
        assert!(coordinator.accept_client_side_tool_v2_wire(serde_json::json!({
            "version": 1,
            "kind": "call",
            "accountSlot": "host",
            "agentId": "agent-a",
            "epoch": "epoch-a",
            "sequence": 2,
            "message": {
                "encoding": "protobuf-base64",
                "messageType": "aiserver.v1.ClientSideToolV2Call",
                "bytes": "GgZjYWxsLTI="
            }
        })).is_none());
    }
}
