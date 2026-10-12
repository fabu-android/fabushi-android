use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use fabushi_android_shared::{ExecutionError, ExecutionRequest};
use serde::{Deserialize, Serialize};

use crate::transport::{CANCEL_PATH, EXECUTE_PATH, RECONCILE_PATH};

const STORE_VERSION: u32 = 1;
const WIRE_VERSION: u32 = 1;
const MAX_IDENTITY_BYTES: usize = 512;
const MAX_BEARER_BYTES: usize = 16 * 1024;
const MAX_PROGRESS_BYTES: usize = 128 * 1024;
const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteRequestIdentity {
    pub credential_id: String,
    pub operation_id: String,
    pub request_id: String,
    pub account_fence: String,
    pub account_epoch: u64,
    pub permission_grant_id: String,
    pub device_id: String,
}

impl RemoteRequestIdentity {
    fn validate(&self) -> Result<(), ExecutionError> {
        validate_identity("credential", &self.credential_id)?;
        validate_identity("operation", &self.operation_id)?;
        validate_identity("request", &self.request_id)?;
        validate_identity("account fence", &self.account_fence)?;
        validate_identity("permission grant", &self.permission_grant_id)?;
        validate_identity("device", &self.device_id)?;
        if self.account_epoch == 0 {
            return Err(ExecutionError::InvalidRequest(
                "remote account epoch must be positive".into(),
            ));
        }
        Ok(())
    }
}

pub trait RemoteExecutionAuthorizer: Send {
    fn authorize(
        &mut self,
        bearer: &str,
        identity: &RemoteRequestIdentity,
    ) -> Result<(), ExecutionError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteBackendStartOutcome {
    Accepted,
    Completed { output_json: String },
    Rejected { reason: String },
    OutcomeUnknown { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteBackendReconcileOutcome {
    Pending,
    Completed { output_json: String },
    Rejected { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteBackendCancelOutcome {
    Confirmed,
    OutcomeUnknown { reason: String },
}

pub trait RemoteExecutionBackend: Send {
    fn start(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
    ) -> Result<RemoteBackendStartOutcome, ExecutionError>;

    fn reconcile(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
    ) -> Result<RemoteBackendReconcileOutcome, ExecutionError>;

    fn cancel(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
    ) -> Result<RemoteBackendCancelOutcome, ExecutionError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteHttpRequest {
    pub path: String,
    pub headers: BTreeMap<String, String>,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteHttpResponse {
    pub status_code: u16,
    pub headers: BTreeMap<String, String>,
    pub body: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum StoredState {
    Accepted,
    Running,
    Completed,
    Rejected,
    Cancelled,
    OutcomeUnknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredRecord {
    #[serde(default)]
    credential_id: String,
    operation_id: String,
    request_id: String,
    capability_id: String,
    input_json: String,
    timeout_ms: u64,
    account_fence: String,
    account_epoch: u64,
    permission_grant_id: String,
    device_id: String,
    ack_id: String,
    state: StoredState,
    progress_json: Option<String>,
    output_json: Option<String>,
    error: Option<String>,
    updated_at_ms: u64,
}

impl StoredRecord {
    fn identity(&self) -> RemoteRequestIdentity {
        RemoteRequestIdentity {
            credential_id: self.credential_id.clone(),
            operation_id: self.operation_id.clone(),
            request_id: self.request_id.clone(),
            account_fence: self.account_fence.clone(),
            account_epoch: self.account_epoch,
            permission_grant_id: self.permission_grant_id.clone(),
            device_id: self.device_id.clone(),
        }
    }

    fn request(&self) -> ExecutionRequest {
        ExecutionRequest {
            operation_id: self.operation_id.clone(),
            capability_id: self.capability_id.clone(),
            input_json: self.input_json.clone(),
            timeout_ms: self.timeout_ms,
        }
    }

    fn matches_execute(&self, request: &ExecuteWireRequest) -> bool {
        self.credential_id == request.credential_id
            && self.operation_id == request.operation_id
            && self.request_id == request.request_id
            && self.capability_id == request.capability_id
            && self.input_json == request.input_json
            && self.timeout_ms == request.timeout_ms
            && self.account_fence == request.account_fence
            && self.account_epoch == request.account_epoch
            && self.permission_grant_id == request.permission_grant_id
            && self.device_id == request.device_id
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoreSnapshot {
    version: u32,
    records: Vec<StoredRecord>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExecuteWireRequest {
    version: u32,
    credential_id: String,
    operation_id: String,
    request_id: String,
    device_id: String,
    capability_id: String,
    input_json: String,
    timeout_ms: u64,
    account_fence: String,
    account_epoch: u64,
    permission_grant_id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdentityWireRequest {
    version: u32,
    credential_id: String,
    operation_id: String,
    request_id: String,
    device_id: String,
    account_fence: String,
    account_epoch: u64,
    permission_grant_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WireResponse<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    ack_id: Option<&'a str>,
    status: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    progress_json: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_json: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a str>,
}

pub struct RemoteExecutionService<A: RemoteExecutionAuthorizer, B: RemoteExecutionBackend> {
    store_path: PathBuf,
    records: BTreeMap<String, StoredRecord>,
    authorizer: A,
    backend: B,
}

impl<A: RemoteExecutionAuthorizer, B: RemoteExecutionBackend> RemoteExecutionService<A, B> {
    pub fn open(
        store_path: impl Into<PathBuf>,
        authorizer: A,
        backend: B,
        now_ms: u64,
    ) -> Result<Self, ExecutionError> {
        let store_path = store_path.into();
        let mut records = BTreeMap::new();
        if store_path.exists() {
            let bytes = fs::read(&store_path).map_err(io_error)?;
            let snapshot: StoreSnapshot = serde_json::from_slice(&bytes)
                .map_err(|error| ExecutionError::Transport(format!("remote execution journal is corrupt: {error}")))?;
            if snapshot.version != STORE_VERSION {
                return Err(ExecutionError::Transport(format!(
                    "unsupported remote execution journal version {}",
                    snapshot.version
                )));
            }
            for mut record in snapshot.records {
                match record.state {
                    StoredState::Accepted => {
                        record.state = StoredState::Cancelled;
                        record.error = Some("server_restarted_before_dispatch".into());
                        record.updated_at_ms = now_ms;
                    }
                    StoredState::Running => {
                        record.state = StoredState::OutcomeUnknown;
                        record.error = Some("server_restarted_after_dispatch".into());
                        record.updated_at_ms = now_ms;
                    }
                    StoredState::Completed
                    | StoredState::Rejected
                    | StoredState::Cancelled
                    | StoredState::OutcomeUnknown => {}
                }
                if records.insert(record.operation_id.clone(), record).is_some() {
                    return Err(ExecutionError::Transport(
                        "remote execution journal contains duplicate operations".into(),
                    ));
                }
            }
        }
        let service = Self {
            store_path,
            records,
            authorizer,
            backend,
        };
        service.persist()?;
        Ok(service)
    }

    pub fn handle_http(&mut self, request: RemoteHttpRequest, now_ms: u64) -> RemoteHttpResponse {
        match request.path.as_str() {
            EXECUTE_PATH => self.handle_execute(request, now_ms),
            RECONCILE_PATH => self.handle_reconcile(request, now_ms),
            CANCEL_PATH => self.handle_cancel(request, now_ms),
            _ => error_response(404, "unknown remote execution service path"),
        }
    }

    pub fn report_progress(
        &mut self,
        identity: &RemoteRequestIdentity,
        progress_json: &str,
        now_ms: u64,
    ) -> Result<(), ExecutionError> {
        identity.validate()?;
        validate_json_payload("progress", progress_json, MAX_PROGRESS_BYTES)?;
        {
            let record = self
                .records
                .get_mut(&identity.operation_id)
                .ok_or_else(|| ExecutionError::InvalidRequest("remote operation is unknown".into()))?;
            require_record_identity(record, identity)?;
            if !matches!(record.state, StoredState::Running | StoredState::OutcomeUnknown) {
                return Err(ExecutionError::InvalidRequest(
                    "remote operation is not active".into(),
                ));
            }
            record.progress_json = Some(progress_json.to_string());
            record.updated_at_ms = now_ms;
        }
        self.persist()
    }

    pub fn settle_completed(
        &mut self,
        identity: &RemoteRequestIdentity,
        output_json: &str,
        now_ms: u64,
    ) -> Result<(), ExecutionError> {
        identity.validate()?;
        validate_json_payload("output", output_json, MAX_OUTPUT_BYTES)?;
        {
            let record = self
                .records
                .get_mut(&identity.operation_id)
                .ok_or_else(|| ExecutionError::InvalidRequest("remote operation is unknown".into()))?;
            require_record_identity(record, identity)?;
            if matches!(record.state, StoredState::Rejected | StoredState::Cancelled) {
                return Err(ExecutionError::InvalidRequest(
                    "remote operation already settled without completion".into(),
                ));
            }
            if record.state == StoredState::Completed {
                if record.output_json.as_deref() == Some(output_json) {
                    return Ok(());
                }
                return Err(ExecutionError::InvalidRequest(
                    "remote completion conflicts with durable result".into(),
                ));
            }
            record.state = StoredState::Completed;
            record.output_json = Some(output_json.to_string());
            record.error = None;
            record.updated_at_ms = now_ms;
        }
        self.persist()
    }

    pub fn settle_rejected(
        &mut self,
        identity: &RemoteRequestIdentity,
        reason: &str,
        now_ms: u64,
    ) -> Result<(), ExecutionError> {
        identity.validate()?;
        {
            let record = self
                .records
                .get_mut(&identity.operation_id)
                .ok_or_else(|| ExecutionError::InvalidRequest("remote operation is unknown".into()))?;
            require_record_identity(record, identity)?;
            if record.state == StoredState::Completed {
                return Err(ExecutionError::InvalidRequest(
                    "completed remote operation cannot become rejected".into(),
                ));
            }
            record.state = StoredState::Rejected;
            record.error = Some(sanitize_reason(reason));
            record.updated_at_ms = now_ms;
        }
        self.persist()
    }

    fn handle_execute(&mut self, http: RemoteHttpRequest, now_ms: u64) -> RemoteHttpResponse {
        let request: ExecuteWireRequest = match parse_json(&http.body) {
            Ok(request) => request,
            Err(response) => return response,
        };
        if request.version != WIRE_VERSION {
            return error_response(400, "unsupported remote execution wire version");
        }
        let identity = RemoteRequestIdentity {
            credential_id: request.credential_id.clone(),
            operation_id: request.operation_id.clone(),
            request_id: request.request_id.clone(),
            account_fence: request.account_fence.clone(),
            account_epoch: request.account_epoch,
            permission_grant_id: request.permission_grant_id.clone(),
            device_id: request.device_id.clone(),
        };
        let bearer = match validate_http_identity(&http.headers, &identity) {
            Ok(bearer) => bearer,
            Err(response) => return response,
        };
        if let Err(error) = self.authorizer.authorize(&bearer, &identity) {
            return authorization_error(error);
        }
        let execution = ExecutionRequest {
            operation_id: request.operation_id.clone(),
            capability_id: request.capability_id.clone(),
            input_json: request.input_json.clone(),
            timeout_ms: request.timeout_ms,
        };
        if let Err(error) = execution.validate().and_then(|_| identity.validate()) {
            return request_error(error);
        }

        if let Some(existing) = self.records.get(&request.operation_id) {
            if !existing.matches_execute(&request) {
                return error_response(409, "operation id is already bound to a different request");
            }
            return record_response(existing);
        }

        let ack_id = stable_ack_id(&identity);
        self.records.insert(
            request.operation_id.clone(),
            StoredRecord {
                credential_id: request.credential_id.clone(),
                operation_id: request.operation_id.clone(),
                request_id: request.request_id.clone(),
                capability_id: request.capability_id.clone(),
                input_json: request.input_json.clone(),
                timeout_ms: request.timeout_ms,
                account_fence: request.account_fence.clone(),
                account_epoch: request.account_epoch,
                permission_grant_id: request.permission_grant_id.clone(),
                device_id: request.device_id.clone(),
                ack_id,
                state: StoredState::Accepted,
                progress_json: None,
                output_json: None,
                error: None,
                updated_at_ms: now_ms,
            },
        );
        if let Err(error) = self.persist() {
            self.records.remove(&request.operation_id);
            return internal_error(error);
        }

        // Crossing Accepted -> Running is the durable side-effect boundary. If the
        // service dies after this write but before a terminal settlement, reopen
        // converts Running to OutcomeUnknown and reconciliation is mandatory.
        if let Some(record) = self.records.get_mut(&request.operation_id) {
            record.state = StoredState::Running;
            record.updated_at_ms = now_ms;
        }
        if let Err(error) = self.persist() {
            return internal_error(error);
        }

        let outcome = self.backend.start(&identity, &execution);
        if let Some(record) = self.records.get_mut(&request.operation_id) {
            match outcome {
                Ok(RemoteBackendStartOutcome::Accepted) => {}
                Ok(RemoteBackendStartOutcome::Completed { output_json }) => {
                    if validate_json_payload("output", &output_json, MAX_OUTPUT_BYTES).is_ok() {
                        record.state = StoredState::Completed;
                        record.output_json = Some(output_json);
                        record.error = None;
                    } else {
                        record.state = StoredState::OutcomeUnknown;
                        record.error = Some("backend returned malformed completion output".into());
                    }
                }
                Ok(RemoteBackendStartOutcome::Rejected { reason }) => {
                    record.state = StoredState::Rejected;
                    record.error = Some(sanitize_reason(reason));
                }
                Ok(RemoteBackendStartOutcome::OutcomeUnknown { reason }) => {
                    record.state = StoredState::OutcomeUnknown;
                    record.error = Some(sanitize_reason(reason));
                }
                Err(ExecutionError::InvalidRequest(reason))
                | Err(ExecutionError::CapabilityUnavailable(reason)) => {
                    record.state = StoredState::Rejected;
                    record.error = Some(sanitize_reason(reason));
                }
                Err(ExecutionError::Cancelled) => {
                    record.state = StoredState::Cancelled;
                    record.error = Some("cancelled".into());
                }
                Err(ExecutionError::TimedOut) => {
                    record.state = StoredState::OutcomeUnknown;
                    record.error = Some("backend timed out after dispatch".into());
                }
                Err(ExecutionError::Transport(reason)) => {
                    record.state = StoredState::OutcomeUnknown;
                    record.error = Some(sanitize_reason(reason));
                }
            }
            record.updated_at_ms = now_ms;
        }
        if let Err(error) = self.persist() {
            return outcome_unknown_response(
                self.records.get(&request.operation_id),
                format!("failed to persist remote result: {}", error_message(error)),
            );
        }
        record_response(self.records.get(&request.operation_id).expect("record exists"))
    }

    fn handle_reconcile(&mut self, http: RemoteHttpRequest, now_ms: u64) -> RemoteHttpResponse {
        let request: IdentityWireRequest = match parse_json(&http.body) {
            Ok(request) => request,
            Err(response) => return response,
        };
        let identity = match validate_identity_request(&http.headers, request) {
            Ok(identity) => identity,
            Err(response) => return response,
        };
        let bearer = match bearer_from_headers(&http.headers) {
            Ok(bearer) => bearer,
            Err(response) => return response,
        };
        if let Err(error) = self.authorizer.authorize(&bearer, &identity) {
            return authorization_error(error);
        }

        let Some(existing) = self.records.get(&identity.operation_id) else {
            return wire_response(
                200,
                None,
                "outcome_unknown",
                None,
                None,
                Some("remote service has no durable outcome for this operation"),
            );
        };
        if let Err(error) = require_record_identity(existing, &identity) {
            return request_error(error);
        }

        if matches!(existing.state, StoredState::Running | StoredState::OutcomeUnknown) {
            let execution = existing.request();
            match self.backend.reconcile(&identity, &execution) {
                Ok(RemoteBackendReconcileOutcome::Pending) => {}
                Ok(RemoteBackendReconcileOutcome::Completed { output_json }) => {
                    if validate_json_payload("output", &output_json, MAX_OUTPUT_BYTES).is_ok() {
                        if let Some(record) = self.records.get_mut(&identity.operation_id) {
                            record.state = StoredState::Completed;
                            record.output_json = Some(output_json);
                            record.error = None;
                            record.updated_at_ms = now_ms;
                        }
                    }
                }
                Ok(RemoteBackendReconcileOutcome::Rejected { reason }) => {
                    if let Some(record) = self.records.get_mut(&identity.operation_id) {
                        record.state = StoredState::Rejected;
                        record.error = Some(sanitize_reason(reason));
                        record.updated_at_ms = now_ms;
                    }
                }
                Err(_) => {
                    if let Some(record) = self.records.get_mut(&identity.operation_id) {
                        if record.state == StoredState::Running {
                            record.state = StoredState::OutcomeUnknown;
                        }
                        record.updated_at_ms = now_ms;
                    }
                }
            }
            if let Err(error) = self.persist() {
                return outcome_unknown_response(
                    self.records.get(&identity.operation_id),
                    format!("failed to persist reconciliation: {}", error_message(error)),
                );
            }
        }
        record_response(self.records.get(&identity.operation_id).expect("record exists"))
    }

    fn handle_cancel(&mut self, http: RemoteHttpRequest, now_ms: u64) -> RemoteHttpResponse {
        let request: IdentityWireRequest = match parse_json(&http.body) {
            Ok(request) => request,
            Err(response) => return response,
        };
        let identity = match validate_identity_request(&http.headers, request) {
            Ok(identity) => identity,
            Err(response) => return response,
        };
        let bearer = match bearer_from_headers(&http.headers) {
            Ok(bearer) => bearer,
            Err(response) => return response,
        };
        if let Err(error) = self.authorizer.authorize(&bearer, &identity) {
            return authorization_error(error);
        }

        let Some(existing) = self.records.get(&identity.operation_id) else {
            return wire_response(
                200,
                None,
                "outcome_unknown",
                None,
                None,
                Some("remote service has no durable operation to cancel"),
            );
        };
        if let Err(error) = require_record_identity(existing, &identity) {
            return request_error(error);
        }

        if matches!(
            existing.state,
            StoredState::Completed | StoredState::Rejected | StoredState::Cancelled
        ) {
            return record_response(existing);
        }
        if existing.state == StoredState::Accepted {
            if let Some(record) = self.records.get_mut(&identity.operation_id) {
                record.state = StoredState::Cancelled;
                record.error = Some("cancelled_before_dispatch".into());
                record.updated_at_ms = now_ms;
            }
        } else {
            let execution = existing.request();
            match self.backend.cancel(&identity, &execution) {
                Ok(RemoteBackendCancelOutcome::Confirmed) | Err(ExecutionError::Cancelled) => {
                    if let Some(record) = self.records.get_mut(&identity.operation_id) {
                        record.state = StoredState::Cancelled;
                        record.error = Some("cancelled".into());
                        record.updated_at_ms = now_ms;
                    }
                }
                Ok(RemoteBackendCancelOutcome::OutcomeUnknown { reason })
                | Err(ExecutionError::InvalidRequest(reason))
                | Err(ExecutionError::CapabilityUnavailable(reason))
                | Err(ExecutionError::Transport(reason)) => {
                    if let Some(record) = self.records.get_mut(&identity.operation_id) {
                        record.state = StoredState::OutcomeUnknown;
                        record.error = Some(sanitize_reason(reason));
                        record.updated_at_ms = now_ms;
                    }
                }
                Err(ExecutionError::TimedOut) => {
                    if let Some(record) = self.records.get_mut(&identity.operation_id) {
                        record.state = StoredState::OutcomeUnknown;
                        record.error = Some("cancel timed out".into());
                        record.updated_at_ms = now_ms;
                    }
                }
            }
        }
        if let Err(error) = self.persist() {
            return outcome_unknown_response(
                self.records.get(&identity.operation_id),
                format!("failed to persist cancellation: {}", error_message(error)),
            );
        }
        record_response(self.records.get(&identity.operation_id).expect("record exists"))
    }

    fn persist(&self) -> Result<(), ExecutionError> {
        if let Some(parent) = self.store_path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        let snapshot = StoreSnapshot {
            version: STORE_VERSION,
            records: self.records.values().cloned().collect(),
        };
        let bytes = serde_json::to_vec_pretty(&snapshot)
            .map_err(|error| ExecutionError::Transport(format!("failed to encode remote execution journal: {error}")))?;
        let mut temp = self.store_path.as_os_str().to_os_string();
        temp.push(".tmp");
        let temp = PathBuf::from(temp);
        fs::write(&temp, bytes).map_err(io_error)?;
        if let Err(first) = fs::rename(&temp, &self.store_path) {
            if self.store_path.exists() {
                fs::remove_file(&self.store_path).map_err(io_error)?;
                fs::rename(&temp, &self.store_path).map_err(io_error)?;
            } else {
                return Err(io_error(first));
            }
        }
        Ok(())
    }
}

fn validate_identity_request(
    headers: &BTreeMap<String, String>,
    request: IdentityWireRequest,
) -> Result<RemoteRequestIdentity, RemoteHttpResponse> {
    if request.version != WIRE_VERSION {
        return Err(error_response(400, "unsupported remote execution wire version"));
    }
    let identity = RemoteRequestIdentity {
        credential_id: request.credential_id,
        operation_id: request.operation_id,
        request_id: request.request_id,
        account_fence: request.account_fence,
        account_epoch: request.account_epoch,
        permission_grant_id: request.permission_grant_id,
        device_id: request.device_id,
    };
    validate_http_identity(headers, &identity)?;
    identity.validate().map_err(request_error)?;
    Ok(identity)
}

fn validate_http_identity(
    headers: &BTreeMap<String, String>,
    identity: &RemoteRequestIdentity,
) -> Result<String, RemoteHttpResponse> {
    identity.validate().map_err(request_error)?;
    require_header(headers, "X-Fabushi-Credential-Id", &identity.credential_id)?;
    require_header(headers, "X-Fabushi-Account-Fence", &identity.account_fence)?;
    require_header(
        headers,
        "X-Fabushi-Account-Epoch",
        &identity.account_epoch.to_string(),
    )?;
    require_header(headers, "X-Fabushi-Operation-Id", &identity.operation_id)?;
    require_header(headers, "X-Fabushi-Request-Id", &identity.request_id)?;
    require_header(
        headers,
        "X-Fabushi-Permission-Grant-Id",
        &identity.permission_grant_id,
    )?;
    require_header(headers, "X-Fabushi-Device-Id", &identity.device_id)?;
    bearer_from_headers(headers)
}

fn require_header(
    headers: &BTreeMap<String, String>,
    name: &str,
    expected: &str,
) -> Result<(), RemoteHttpResponse> {
    match header(headers, name) {
        Some(actual) if actual == expected => Ok(()),
        Some(_) => Err(error_response(
            400,
            &format!("{name} does not match the signed request body"),
        )),
        None => Err(error_response(400, &format!("missing {name}"))),
    }
}

fn bearer_from_headers(
    headers: &BTreeMap<String, String>,
) -> Result<String, RemoteHttpResponse> {
    let Some(value) = header(headers, "Authorization") else {
        return Err(error_response(401, "missing bearer authorization"));
    };
    let Some(bearer) = value.strip_prefix("Bearer ") else {
        return Err(error_response(401, "invalid bearer authorization"));
    };
    if bearer.len() < 16
        || bearer.len() > MAX_BEARER_BYTES
        || bearer.chars().any(char::is_whitespace)
        || bearer.chars().any(char::is_control)
    {
        return Err(error_response(401, "invalid bearer authorization"));
    }
    Ok(bearer.to_string())
}

fn header<'a>(headers: &'a BTreeMap<String, String>, name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn require_record_identity(
    record: &StoredRecord,
    identity: &RemoteRequestIdentity,
) -> Result<(), ExecutionError> {
    if &record.identity() != identity {
        return Err(ExecutionError::InvalidRequest(
            "remote operation identity does not match durable record".into(),
        ));
    }
    Ok(())
}

fn parse_json<T: for<'de> Deserialize<'de>>(body: &str) -> Result<T, RemoteHttpResponse> {
    serde_json::from_str(body).map_err(|_| error_response(400, "malformed remote execution JSON"))
}

fn record_response(record: &StoredRecord) -> RemoteHttpResponse {
    let status = match record.state {
        StoredState::Accepted => "accepted",
        StoredState::Running => "running",
        StoredState::Completed => "completed",
        StoredState::Rejected => "rejected",
        StoredState::Cancelled => "cancelled",
        StoredState::OutcomeUnknown => "outcome_unknown",
    };
    wire_response(
        200,
        Some(&record.ack_id),
        status,
        record.progress_json.as_deref(),
        record.output_json.as_deref(),
        record.error.as_deref(),
    )
}

fn outcome_unknown_response(record: Option<&StoredRecord>, reason: String) -> RemoteHttpResponse {
    wire_response(
        503,
        record.map(|record| record.ack_id.as_str()),
        "outcome_unknown",
        record.and_then(|record| record.progress_json.as_deref()),
        None,
        Some(&reason),
    )
}

fn authorization_error(error: ExecutionError) -> RemoteHttpResponse {
    error_response(403, &format!("remote authorization rejected: {}", error_message(error)))
}

fn request_error(error: ExecutionError) -> RemoteHttpResponse {
    error_response(400, &error_message(error))
}

fn internal_error(error: ExecutionError) -> RemoteHttpResponse {
    error_response(503, &format!("remote execution service unavailable: {}", error_message(error)))
}

fn error_response(status_code: u16, reason: &str) -> RemoteHttpResponse {
    wire_response(
        status_code,
        None,
        "rejected",
        None,
        None,
        Some(&sanitize_reason(reason)),
    )
}

fn wire_response(
    status_code: u16,
    ack_id: Option<&str>,
    status: &str,
    progress_json: Option<&str>,
    output_json: Option<&str>,
    error: Option<&str>,
) -> RemoteHttpResponse {
    let body = serde_json::to_string(&WireResponse {
        ack_id,
        status,
        progress_json,
        output_json,
        error,
    })
    .expect("wire response serialization is infallible");
    let mut headers = BTreeMap::new();
    headers.insert("Content-Type".into(), "application/json".into());
    if let Some(ack_id) = ack_id {
        headers.insert("X-Fabushi-Ack-Id".into(), ack_id.to_string());
    }
    RemoteHttpResponse {
        status_code,
        headers,
        body,
    }
}

fn stable_ack_id(identity: &RemoteRequestIdentity) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for value in [
        identity.credential_id.as_str(),
        identity.operation_id.as_str(),
        identity.request_id.as_str(),
        identity.account_fence.as_str(),
        identity.permission_grant_id.as_str(),
        identity.device_id.as_str(),
    ] {
        for byte in value.as_bytes().iter().copied().chain([0xff]) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    for byte in identity.account_epoch.to_le_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("ack-{hash:016x}")
}

fn validate_identity(label: &str, value: &str) -> Result<(), ExecutionError> {
    if value.trim().is_empty()
        || value.len() > MAX_IDENTITY_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(ExecutionError::InvalidRequest(format!(
            "remote {label} identity is invalid"
        )));
    }
    Ok(())
}

fn validate_json_payload(
    label: &str,
    value: &str,
    max_bytes: usize,
) -> Result<(), ExecutionError> {
    if value.len() > max_bytes {
        return Err(ExecutionError::InvalidRequest(format!(
            "remote {label} JSON exceeds bounded size"
        )));
    }
    serde_json::from_str::<serde_json::Value>(value).map_err(|_| {
        ExecutionError::InvalidRequest(format!("remote {label} JSON is malformed"))
    })?;
    Ok(())
}

fn sanitize_reason(value: impl AsRef<str>) -> String {
    value
        .as_ref()
        .chars()
        .filter(|character| !character.is_control())
        .take(512)
        .collect()
}

fn error_message(error: ExecutionError) -> String {
    match error {
        ExecutionError::InvalidRequest(reason)
        | ExecutionError::CapabilityUnavailable(reason)
        | ExecutionError::Transport(reason) => sanitize_reason(reason),
        ExecutionError::Cancelled => "cancelled".into(),
        ExecutionError::TimedOut => "timed out".into(),
    }
}

fn io_error(error: std::io::Error) -> ExecutionError {
    ExecutionError::Transport(format!("remote execution journal I/O failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Default)]
    struct TestAuthorizer {
        calls: usize,
    }

    impl RemoteExecutionAuthorizer for TestAuthorizer {
        fn authorize(
            &mut self,
            bearer: &str,
            identity: &RemoteRequestIdentity,
        ) -> Result<(), ExecutionError> {
            self.calls += 1;
            if bearer != "remote-secret-token-1234" {
                return Err(ExecutionError::InvalidRequest("credential rejected".into()));
            }
            if identity.credential_id != "runner-credential-1"
                || identity.account_fence != "session:abc123"
                || identity.device_id != "device-1"
            {
                return Err(ExecutionError::InvalidRequest("identity rejected".into()));
            }
            Ok(())
        }
    }

    struct TestBackend {
        starts: usize,
        cancels: usize,
        reconciles: usize,
        start_outcome: RemoteBackendStartOutcome,
        reconcile_outcomes: VecDeque<RemoteBackendReconcileOutcome>,
    }

    impl TestBackend {
        fn completed() -> Self {
            Self {
                starts: 0,
                cancels: 0,
                reconciles: 0,
                start_outcome: RemoteBackendStartOutcome::Completed {
                    output_json: r#"{"ok":true}"#.into(),
                },
                reconcile_outcomes: VecDeque::new(),
            }
        }

        fn asynchronous() -> Self {
            Self {
                starts: 0,
                cancels: 0,
                reconciles: 0,
                start_outcome: RemoteBackendStartOutcome::Accepted,
                reconcile_outcomes: VecDeque::new(),
            }
        }
    }

    impl RemoteExecutionBackend for TestBackend {
        fn start(
            &mut self,
            _identity: &RemoteRequestIdentity,
            _request: &ExecutionRequest,
        ) -> Result<RemoteBackendStartOutcome, ExecutionError> {
            self.starts += 1;
            Ok(self.start_outcome.clone())
        }

        fn reconcile(
            &mut self,
            _identity: &RemoteRequestIdentity,
            _request: &ExecutionRequest,
        ) -> Result<RemoteBackendReconcileOutcome, ExecutionError> {
            self.reconciles += 1;
            Ok(self
                .reconcile_outcomes
                .pop_front()
                .unwrap_or(RemoteBackendReconcileOutcome::Pending))
        }

        fn cancel(
            &mut self,
            _identity: &RemoteRequestIdentity,
            _request: &ExecutionRequest,
        ) -> Result<RemoteBackendCancelOutcome, ExecutionError> {
            self.cancels += 1;
            Ok(RemoteBackendCancelOutcome::Confirmed)
        }
    }

    fn temp_store(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "fabushi-remote-service-{name}-{}-{nanos}.json",
            std::process::id()
        ))
    }

    fn headers() -> BTreeMap<String, String> {
        BTreeMap::from([
            ("Authorization".into(), "Bearer remote-secret-token-1234".into()),
            ("X-Fabushi-Credential-Id".into(), "runner-credential-1".into()),
            ("X-Fabushi-Account-Fence".into(), "session:abc123".into()),
            ("X-Fabushi-Account-Epoch".into(), "7".into()),
            ("X-Fabushi-Operation-Id".into(), "op-1".into()),
            ("X-Fabushi-Request-Id".into(), "req-1".into()),
            ("X-Fabushi-Permission-Grant-Id".into(), "grant-1".into()),
            ("X-Fabushi-Device-Id".into(), "device-1".into()),
        ])
    }

    fn execute_request() -> RemoteHttpRequest {
        RemoteHttpRequest {
            path: EXECUTE_PATH.into(),
            headers: headers(),
            body: r#"{"version":1,"credentialId":"runner-credential-1","operationId":"op-1","requestId":"req-1","deviceId":"device-1","capabilityId":"computer.use","inputJson":"{\"action\":\"click\",\"x\":10,\"y\":20}","timeoutMs":30000,"accountFence":"session:abc123","accountEpoch":7,"permissionGrantId":"grant-1"}"#.into(),
        }
    }

    fn identity_request(path: &str) -> RemoteHttpRequest {
        RemoteHttpRequest {
            path: path.into(),
            headers: headers(),
            body: r#"{"version":1,"credentialId":"runner-credential-1","operationId":"op-1","requestId":"req-1","deviceId":"device-1","accountFence":"session:abc123","accountEpoch":7,"permissionGrantId":"grant-1"}"#.into(),
        }
    }

    fn json(response: &RemoteHttpResponse) -> serde_json::Value {
        serde_json::from_str(&response.body).unwrap()
    }

    #[test]
    fn execute_is_authorized_durable_and_duplicate_safe() {
        let path = temp_store("execute");
        let mut service =
            RemoteExecutionService::open(&path, TestAuthorizer::default(), TestBackend::completed(), 1)
                .unwrap();

        let first = service.handle_http(execute_request(), 2);
        assert_eq!(first.status_code, 200);
        assert_eq!(json(&first)["status"], "completed");
        assert_eq!(json(&first)["outputJson"], r#"{"ok":true}"#);
        assert_eq!(service.backend.starts, 1);

        let second = service.handle_http(execute_request(), 3);
        assert_eq!(second.status_code, 200);
        assert_eq!(json(&second)["status"], "completed");
        assert_eq!(service.backend.starts, 1, "duplicate execute must never replay");
        assert_eq!(
            first.headers.get("X-Fabushi-Ack-Id"),
            second.headers.get("X-Fabushi-Ack-Id")
        );
        assert!(path.exists());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn conflicting_duplicate_operation_fails_closed_without_second_side_effect() {
        let path = temp_store("conflict");
        let mut service =
            RemoteExecutionService::open(&path, TestAuthorizer::default(), TestBackend::completed(), 1)
                .unwrap();
        assert_eq!(service.handle_http(execute_request(), 2).status_code, 200);

        let mut conflicting = execute_request();
        conflicting.body = conflicting
            .body
            .replace(r#"\"x\":10"#, r#"\"x\":11"#);
        let response = service.handle_http(conflicting, 3);
        assert_eq!(response.status_code, 409);
        assert_eq!(service.backend.starts, 1);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn authorization_and_header_body_identity_are_checked_before_execution() {
        let path = temp_store("auth");
        let mut service =
            RemoteExecutionService::open(&path, TestAuthorizer::default(), TestBackend::completed(), 1)
                .unwrap();

        let mut unauthorized = execute_request();
        unauthorized
            .headers
            .insert("Authorization".into(), "Bearer wrong-but-long-enough-token".into());
        assert_eq!(service.handle_http(unauthorized, 2).status_code, 403);
        assert_eq!(service.backend.starts, 0);

        let mut confused = execute_request();
        confused
            .headers
            .insert("X-Fabushi-Device-Id".into(), "device-other".into());
        assert_eq!(service.handle_http(confused, 3).status_code, 400);
        assert_eq!(service.backend.starts, 0);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn async_ack_progress_result_and_reconcile_share_one_durable_identity() {
        let path = temp_store("progress");
        let mut backend = TestBackend::asynchronous();
        backend
            .reconcile_outcomes
            .push_back(RemoteBackendReconcileOutcome::Pending);
        backend
            .reconcile_outcomes
            .push_back(RemoteBackendReconcileOutcome::Completed {
                output_json: r#"{"done":true}"#.into(),
            });
        let mut service =
            RemoteExecutionService::open(&path, TestAuthorizer::default(), backend, 1).unwrap();

        let accepted = service.handle_http(execute_request(), 2);
        assert_eq!(json(&accepted)["status"], "running");
        let identity = service.records["op-1"].identity();
        service
            .report_progress(&identity, r#"{"percent":50}"#, 3)
            .unwrap();

        let pending = service.handle_http(identity_request(RECONCILE_PATH), 4);
        assert_eq!(json(&pending)["status"], "running");
        assert_eq!(json(&pending)["progressJson"], r#"{"percent":50}"#);

        let completed = service.handle_http(identity_request(RECONCILE_PATH), 5);
        assert_eq!(json(&completed)["status"], "completed");
        assert_eq!(json(&completed)["outputJson"], r#"{"done":true}"#);
        assert_eq!(service.backend.starts, 1);
        assert_eq!(service.backend.reconciles, 2);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn process_death_turns_inflight_execution_into_outcome_unknown_without_replay() {
        let path = temp_store("restart");
        {
            let mut service = RemoteExecutionService::open(
                &path,
                TestAuthorizer::default(),
                TestBackend::asynchronous(),
                1,
            )
            .unwrap();
            let response = service.handle_http(execute_request(), 2);
            assert_eq!(json(&response)["status"], "running");
            assert_eq!(service.backend.starts, 1);
        }

        let mut reopened = RemoteExecutionService::open(
            &path,
            TestAuthorizer::default(),
            TestBackend::asynchronous(),
            10,
        )
        .unwrap();
        assert_eq!(reopened.records["op-1"].state, StoredState::OutcomeUnknown);
        let response = reopened.handle_http(identity_request(RECONCILE_PATH), 11);
        assert_eq!(json(&response)["status"], "outcome_unknown");
        assert_eq!(reopened.backend.starts, 0, "reopen must not replay execute");
        assert_eq!(reopened.backend.reconciles, 1);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn cancellation_is_identity_bound_and_terminal_only_after_backend_confirmation() {
        let path = temp_store("cancel");
        let mut service = RemoteExecutionService::open(
            &path,
            TestAuthorizer::default(),
            TestBackend::asynchronous(),
            1,
        )
        .unwrap();
        let _ = service.handle_http(execute_request(), 2);

        let response = service.handle_http(identity_request(CANCEL_PATH), 3);
        assert_eq!(json(&response)["status"], "cancelled");
        assert_eq!(service.backend.cancels, 1);

        let duplicate = service.handle_http(identity_request(CANCEL_PATH), 4);
        assert_eq!(json(&duplicate)["status"], "cancelled");
        assert_eq!(service.backend.cancels, 1);
        let _ = fs::remove_file(path);
    }
}
