use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::path::PathBuf;

use fabushi_android_internal::MonotonicSequence;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PendingRequest {
    session_id: String,
    method: String,
    operation_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PersistedCoordinatorEvent {
    session_id: String,
    sequence: u64,
    family: String,
    payload_json: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PersistedCoordinatorState {
    version: u32,
    generation: u64,
    latest_sequence: u64,
    pending: BTreeMap<String, PendingRequest>,
    events: Vec<PersistedCoordinatorEvent>,
}

pub struct MahayanaCoordinator<H: HostPort> {
    host: H,
    generation: u64,
    sequence: MonotonicSequence,
    pending: BTreeMap<String, PendingRequest>,
    events: VecDeque<CoordinatorEvent>,
    replay_limit: usize,
    client_side_tool_v2: ClientSideToolV2Relay,
    state_path: Option<PathBuf>,
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
            state_path: None,
        }
    }

    pub fn with_generation_persistent(
        host: H,
        generation: u64,
        replay_limit: usize,
        state_path: impl Into<PathBuf>,
    ) -> Result<Self, CoordinatorFailure> {
        let state_path = state_path.into();
        let persisted = if state_path.exists() {
            let raw = fs::read_to_string(&state_path).map_err(|error| CoordinatorFailure::new(
                CoordinatorFailureCode::Internal,
                format!("failed to read coordinator state: {error}"),
            ))?;
            let state: PersistedCoordinatorState = serde_json::from_str(&raw).map_err(|error| CoordinatorFailure::new(
                CoordinatorFailureCode::Internal,
                format!("failed to decode coordinator state: {error}"),
            ))?;
            if state.version != 1 {
                return Err(CoordinatorFailure::new(
                    CoordinatorFailureCode::Internal,
                    format!("unsupported coordinator state version {}", state.version),
                ));
            }
            Some(state)
        } else {
            None
        };
        let recovered_generation = persisted.as_ref()
            .map(|state| state.generation.saturating_add(1))
            .unwrap_or(generation)
            .max(generation)
            .max(1);
        let mut coordinator = Self {
            host,
            generation: recovered_generation,
            sequence: MonotonicSequence::default(),
            pending: BTreeMap::new(),
            events: VecDeque::new(),
            replay_limit: replay_limit.max(1),
            client_side_tool_v2: ClientSideToolV2Relay::default(),
            state_path: Some(state_path),
        };
        if let Some(state) = persisted {
            for (request_id, pending) in state.pending {
                let sequence = coordinator.sequence.next_value();
                coordinator.events.push_back(CoordinatorEvent {
                    event_id: format!("g{}-e{}", coordinator.generation, sequence),
                    session_id: pending.session_id,
                    sequence,
                    family: "operation.outcome-unknown".into(),
                    payload_json: json!({
                        "requestId": request_id,
                        "operationId": pending.operation_id,
                        "method": pending.method,
                        "reason": "process-death",
                        "outcome": "outcome-unknown"
                    }).to_string(),
                });
            }
            while coordinator.events.len() > coordinator.replay_limit {
                coordinator.events.pop_front();
            }
        }
        coordinator.persist_state()?;
        Ok(coordinator)
    }

    fn persist_state(&self) -> Result<(), CoordinatorFailure> {
        let Some(path) = self.state_path.as_ref() else { return Ok(()); };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| CoordinatorFailure::new(
                CoordinatorFailureCode::Internal,
                format!("failed to create coordinator state directory: {error}"),
            ))?;
        }
        let state = PersistedCoordinatorState {
            version: 1,
            generation: self.generation,
            latest_sequence: self.sequence.current(),
            pending: self.pending.clone(),
            events: self.events.iter().map(|event| PersistedCoordinatorEvent {
                session_id: event.session_id.clone(),
                sequence: event.sequence,
                family: event.family.clone(),
                payload_json: event.payload_json.clone(),
            }).collect(),
        };
        let encoded = serde_json::to_vec_pretty(&state).map_err(|error| CoordinatorFailure::new(
            CoordinatorFailureCode::Internal,
            format!("failed to encode coordinator state: {error}"),
        ))?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, encoded).map_err(|error| CoordinatorFailure::new(
            CoordinatorFailureCode::Internal,
            format!("failed to write coordinator state: {error}"),
        ))?;
        fs::rename(&tmp, path).map_err(|error| CoordinatorFailure::new(
            CoordinatorFailureCode::Internal,
            format!("failed to replace coordinator state: {error}"),
        ))
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
        if let Err(error) = self.persist_state() {
            self.pending.remove(&request.request_id);
            return Err(error);
        }
        Ok(())
    }

    pub fn complete_request(&mut self, request_id: &str, result: Result<String, CoordinatorFailure>) -> CoordinatorReply {
        let Some(pending) = self.pending.remove(request_id) else {
            return CoordinatorReply::failed(
                request_id,
                CoordinatorFailure::new(CoordinatorFailureCode::UnknownRequest, "request is not active"),
            );
        };
        if let Err(error) = self.persist_state() {
            self.pending.insert(request_id.to_string(), pending);
            return CoordinatorReply::failed(
                request_id,
                CoordinatorFailure::new(
                    CoordinatorFailureCode::Internal,
                    format!("coordinator terminal state is not durable; outcome unknown: {}", error.message),
                ),
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
            Err(error) => self.complete_request(&request_id, Err(error))
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
        let previous = pending.operation_id.replace(operation_id.to_string());
        if let Err(error) = self.persist_state() {
            if let Some(pending) = self.pending.get_mut(request_id) {
                pending.operation_id = previous;
            }
            return Err(error);
        }
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
        match host_result {
            Ok(()) => {
                let removed = self.pending.remove(&request_id);
                if let Err(error) = self.persist_state() {
                    if let Some(pending) = removed {
                        self.pending.insert(request_id.clone(), pending);
                    }
                    return CoordinatorReply::failed(
                        request_id,
                        CoordinatorFailure::new(
                            CoordinatorFailureCode::Internal,
                            format!("coordinator cancellation is not durable; outcome unknown: {}", error.message),
                        ),
                    );
                }
                CoordinatorReply::failed(
                    request_id,
                    CoordinatorFailure::new(
                        CoordinatorFailureCode::Cancelled,
                        "request cancelled",
                    ),
                )
            }
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
        match host_result {
            Ok(()) => {
                let removed = self.pending.remove(&cancel.request_id);
                if let Err(error) = self.persist_state() {
                    if let Some(pending) = removed {
                        self.pending.insert(cancel.request_id.clone(), pending);
                    }
                    return CoordinatorReply::failed(
                        cancel.request_id,
                        CoordinatorFailure::new(
                            CoordinatorFailureCode::Internal,
                            format!("coordinator cancellation is not durable; outcome unknown: {}", error.message),
                        ),
                    );
                }
                CoordinatorReply::failed(
                    cancel.request_id,
                    CoordinatorFailure::new(CoordinatorFailureCode::Cancelled, "request cancelled"),
                )
            }
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
        let _ = self.persist_state();
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
        let replies = std::mem::take(&mut self.pending).into_keys().map(|request_id| {
            CoordinatorReply::failed(
                request_id,
                CoordinatorFailure::new(CoordinatorFailureCode::HostCrashed, detail.clone()),
            )
        }).collect::<Vec<_>>();
        let _ = self.persist_state();
        replies
    }

    pub fn active_request_count(&self) -> usize { self.pending.len() }

    pub fn latest_sequence(&self) -> u64 { self.sequence.current() }

    pub fn active_request_metadata(&self, request_id: &str) -> Option<(&str, &str)> {
        self.pending.get(request_id).map(|request| (request.session_id.as_str(), request.method.as_str()))
    }

    pub fn host_mut(&mut self) -> &mut H { &mut self.host }

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
    fn durable_cancel_is_not_recovered_as_in_flight_after_reopen() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-coordinator-cancel-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let state = root.join("coordinator-state.json");
        {
            let mut coordinator = MahayanaCoordinator::with_generation_persistent(
                FakeHost::default(), 5, 8, &state
            ).unwrap();
            coordinator.begin_request(&request("cancel-1")).unwrap();
            coordinator.bind_operation("cancel-1", "operation-cancel-1").unwrap();
            let cancelled = coordinator.cancel_operation("operation-cancel-1", Some("user"));
            assert_eq!(cancelled.result_json.unwrap_err().code, CoordinatorFailureCode::Cancelled);
            assert_eq!(coordinator.active_request_count(), 0);
        }
        let coordinator = MahayanaCoordinator::with_generation_persistent(
            FakeHost::default(), 5, 8, &state
        ).unwrap();
        assert_eq!(coordinator.active_request_count(), 0);
        assert!(coordinator.resync_since(0).events.is_empty());
        let _ = std::fs::remove_dir_all(root);
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
    fn persistent_reopen_fences_generation_and_surfaces_outcome_unknown_without_replay() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-coordinator-recovery-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let state = root.join("coordinator-state.json");
        {
            let mut coordinator = MahayanaCoordinator::with_generation_persistent(
                FakeHost::default(), 7, 8, &state
            ).unwrap();
            coordinator.begin_request(&request("recover-1")).unwrap();
            coordinator.bind_operation("recover-1", "operation-1").unwrap();
            coordinator.publish_event("session-a", "chat.delta", r#"{"delta":"a"}"#);
            assert_eq!(coordinator.active_request_count(), 1);
        }
        let coordinator = MahayanaCoordinator::with_generation_persistent(
            FakeHost::default(), 7, 8, &state
        ).unwrap();
        assert_eq!(coordinator.generation(), 8);
        assert_eq!(coordinator.active_request_count(), 0);
        let recovered = coordinator.resync_since(0);
        assert_eq!(recovered.events.len(), 1);
        assert_eq!(recovered.events[0].family, "operation.outcome-unknown");
        assert!(recovered.events[0].payload_json.contains("recover-1"));
        assert!(recovered.events[0].payload_json.contains("operation-1"));
        let _ = std::fs::remove_dir_all(root);
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
