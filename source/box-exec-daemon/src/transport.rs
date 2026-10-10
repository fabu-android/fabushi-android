use fabushi_android_shared::{ExecutionError, ExecutionRequest, ExecutionResult};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io::Read;
use std::thread;
use std::time::Duration;
use url::Url;

const EXECUTE_PATH: &str = "fabushi.remote.v1.ExecutionService/Execute";
const RECONCILE_PATH: &str = "fabushi.remote.v1.ExecutionService/Reconcile";
const CANCEL_PATH: &str = "fabushi.remote.v1.ExecutionService/Cancel";
const MAX_IDENTITY_BYTES: usize = 512;
const MAX_BEARER_BYTES: usize = 16 * 1024;

#[derive(Clone, PartialEq, Eq)]
pub struct RemoteBearerCredential(String);

impl RemoteBearerCredential {
    pub fn new(value: impl Into<String>) -> Result<Self, ExecutionError> {
        let value = value.into();
        if value.len() < 16
            || value.len() > MAX_BEARER_BYTES
            || value.chars().any(char::is_whitespace)
            || value.chars().any(char::is_control)
        {
            return Err(ExecutionError::InvalidRequest(
                "remote bearer credential is invalid".into(),
            ));
        }
        Ok(Self(value))
    }

    fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for RemoteBearerCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RemoteBearerCredential(<redacted>)")
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct RemoteExecutionContext {
    pub bearer: RemoteBearerCredential,
    pub account_fence: String,
    pub account_epoch: u64,
    pub operation_id: String,
    pub request_id: String,
    pub permission_grant_id: String,
    pub device_id: String,
}

impl fmt::Debug for RemoteExecutionContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RemoteExecutionContext")
            .field("bearer", &self.bearer)
            .field("account_fence", &self.account_fence)
            .field("account_epoch", &self.account_epoch)
            .field("operation_id", &self.operation_id)
            .field("request_id", &self.request_id)
            .field("permission_grant_id", &self.permission_grant_id)
            .field("device_id", &self.device_id)
            .finish()
    }
}

impl RemoteExecutionContext {
    pub fn validate(&self) -> Result<(), ExecutionError> {
        validate_identity("account fence", &self.account_fence)?;
        if self.account_epoch == 0 {
            return Err(ExecutionError::InvalidRequest(
                "remote account epoch must be positive".into(),
            ));
        }
        validate_identity("operation", &self.operation_id)?;
        validate_identity("request", &self.request_id)?;
        validate_identity("permission grant", &self.permission_grant_id)?;
        validate_identity("device", &self.device_id)?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteDispatchOutcome {
    Completed {
        ack_id: String,
        result: ExecutionResult,
    },
    Rejected {
        ack_id: Option<String>,
        reason: String,
    },
    OutcomeUnknown {
        ack_id: Option<String>,
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteReconcileOutcome {
    Completed {
        ack_id: String,
        result: ExecutionResult,
    },
    Rejected {
        ack_id: Option<String>,
        reason: String,
    },
    Pending {
        ack_id: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteCancelOutcome {
    Confirmed { ack_id: Option<String> },
    OutcomeUnknown {
        ack_id: Option<String>,
        reason: String,
    },
}

pub trait RemoteExecutionTransport {
    fn execute(
        &mut self,
        context: &RemoteExecutionContext,
        request: &ExecutionRequest,
    ) -> Result<RemoteDispatchOutcome, ExecutionError>;

    fn reconcile(
        &mut self,
        context: &RemoteExecutionContext,
        operation_id: &str,
        request_id: &str,
    ) -> Result<RemoteReconcileOutcome, ExecutionError>;

    fn cancel(
        &mut self,
        context: &RemoteExecutionContext,
        operation_id: &str,
        request_id: &str,
    ) -> Result<RemoteCancelOutcome, ExecutionError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteTransportPolicy {
    pub connect_timeout_ms: u64,
    pub request_timeout_ms: u64,
    pub reconcile_attempts: u8,
    pub reconcile_backoff_ms: u64,
    pub max_response_bytes: usize,
}

impl Default for RemoteTransportPolicy {
    fn default() -> Self {
        Self {
            connect_timeout_ms: 10_000,
            request_timeout_ms: 30_000,
            reconcile_attempts: 3,
            reconcile_backoff_ms: 250,
            max_response_bytes: 1024 * 1024,
        }
    }
}

pub struct AuthenticatedRemoteHttpTransport {
    endpoint: Url,
    policy: RemoteTransportPolicy,
    agent: ureq::Agent,
}

impl AuthenticatedRemoteHttpTransport {
    pub fn new(
        endpoint: &str,
        policy: RemoteTransportPolicy,
    ) -> Result<Self, ExecutionError> {
        validate_policy(&policy)?;
        let endpoint = validate_endpoint(endpoint)?;
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_millis(policy.connect_timeout_ms))
            .timeout_read(Duration::from_millis(policy.request_timeout_ms))
            .timeout_write(Duration::from_millis(policy.request_timeout_ms))
            .build();
        Ok(Self {
            endpoint,
            policy,
            agent,
        })
    }

    pub fn endpoint(&self) -> &Url {
        &self.endpoint
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.endpoint.as_str().trim_end_matches('/'), path)
    }

    fn request(
        &self,
        path: &str,
        context: &RemoteExecutionContext,
        body: &impl Serialize,
    ) -> Result<WireResponse, HttpFailure> {
        let authorization = format!("Bearer {}", context.bearer.expose());
        let account_epoch = context.account_epoch.to_string();
        let request = self
            .agent
            .post(&self.url(path))
            .set("Authorization", &authorization)
            .set("Content-Type", "application/json")
            .set("Accept", "application/json")
            .set("X-Fabushi-Account-Fence", &context.account_fence)
            .set("X-Fabushi-Account-Epoch", &account_epoch)
            .set("X-Fabushi-Operation-Id", &context.operation_id)
            .set("X-Fabushi-Request-Id", &context.request_id)
            .set("X-Fabushi-Permission-Grant-Id", &context.permission_grant_id)
            .set("X-Fabushi-Device-Id", &context.device_id);

        match request.send_json(body) {
            Ok(response) => decode_response(response, self.policy.max_response_bytes)
                .map_err(HttpFailure::Protocol),
            Err(ureq::Error::Status(status, response)) => {
                let header_ack_id = response
                    .header("X-Fabushi-Ack-Id")
                    .map(str::to_string);
                let decoded = decode_response(response, self.policy.max_response_bytes).ok();
                let ack_id = decoded
                    .as_ref()
                    .and_then(|wire| wire.ack_id.clone())
                    .or(header_ack_id);
                let detail = decoded
                    .and_then(|wire| wire.error.or(wire.output_json))
                    .map(sanitize_reason)
                    .unwrap_or_else(|| format!("remote runner returned HTTP {status}"));
                Err(HttpFailure::Status {
                    status,
                    ack_id,
                    detail,
                })
            }
            Err(ureq::Error::Transport(error)) => {
                Err(HttpFailure::Transport(sanitize_reason(error.to_string())))
            }
        }
    }
}

impl RemoteExecutionTransport for AuthenticatedRemoteHttpTransport {
    fn execute(
        &mut self,
        context: &RemoteExecutionContext,
        request: &ExecutionRequest,
    ) -> Result<RemoteDispatchOutcome, ExecutionError> {
        context.validate()?;
        request.validate()?;
        if context.operation_id != request.operation_id {
            return Err(ExecutionError::InvalidRequest(
                "remote context operation does not match execution request".into(),
            ));
        }

        let body = ExecuteWireRequest {
            version: 1,
            operation_id: &context.operation_id,
            request_id: &context.request_id,
            device_id: &context.device_id,
            capability_id: &request.capability_id,
            input_json: &request.input_json,
            timeout_ms: request.timeout_ms,
            account_fence: &context.account_fence,
            account_epoch: context.account_epoch,
            permission_grant_id: &context.permission_grant_id,
        };

        // Side-effecting execution is deliberately sent exactly once. A transport
        // failure after send is outcome-unknown and must be reconciled, never replayed.
        match self.request(EXECUTE_PATH, context, &body) {
            Ok(response) => dispatch_from_wire(response, request),
            Err(HttpFailure::Status {
                status,
                ack_id,
                detail,
            }) if (400..500).contains(&status) && status != 408 && status != 429 => {
                Ok(RemoteDispatchOutcome::Rejected {
                    ack_id,
                    reason: detail,
                })
            }
            Err(HttpFailure::Status {
                ack_id, detail, ..
            }) => Ok(RemoteDispatchOutcome::OutcomeUnknown {
                ack_id,
                reason: detail,
            }),
            Err(HttpFailure::Transport(reason) | HttpFailure::Protocol(reason)) => {
                Ok(RemoteDispatchOutcome::OutcomeUnknown {
                    ack_id: None,
                    reason,
                })
            }
        }
    }

    fn reconcile(
        &mut self,
        context: &RemoteExecutionContext,
        operation_id: &str,
        request_id: &str,
    ) -> Result<RemoteReconcileOutcome, ExecutionError> {
        context.validate()?;
        validate_identity("operation", operation_id)?;
        validate_identity("request", request_id)?;
        if context.operation_id != operation_id || context.request_id != request_id {
            return Err(ExecutionError::InvalidRequest(
                "remote reconciliation identity mismatch".into(),
            ));
        }

        let body = IdentityWireRequest {
            version: 1,
            operation_id,
            request_id,
            device_id: &context.device_id,
            account_fence: &context.account_fence,
            account_epoch: context.account_epoch,
            permission_grant_id: &context.permission_grant_id,
        };

        let mut last_ack = None;
        for attempt in 0..self.policy.reconcile_attempts.max(1) {
            match self.request(RECONCILE_PATH, context, &body) {
                Ok(response) => {
                    last_ack = response.ack_id.clone().or(last_ack);
                    match response.status.as_str() {
                        "completed" => {
                            let output_json = response.output_json.ok_or_else(|| {
                                ExecutionError::Transport(
                                    "completed reconciliation omitted output_json".into(),
                                )
                            })?;
                            let ack_id = response
                                .ack_id
                                .or(last_ack)
                                .ok_or_else(|| {
                                    ExecutionError::Transport(
                                        "completed reconciliation omitted ack_id".into(),
                                    )
                                })?;
                            return Ok(RemoteReconcileOutcome::Completed {
                                ack_id,
                                result: ExecutionResult {
                                    operation_id: operation_id.to_string(),
                                    output_json,
                                },
                            });
                        }
                        "rejected" | "cancelled" => {
                            return Ok(RemoteReconcileOutcome::Rejected {
                                ack_id: response.ack_id.or(last_ack),
                                reason: sanitize_reason(
                                    response
                                        .error
                                        .unwrap_or_else(|| response.status.clone()),
                                ),
                            });
                        }
                        "accepted" | "pending" | "running" | "outcome_unknown" => {}
                        other => {
                            return Err(ExecutionError::Transport(format!(
                                "unsupported remote reconciliation status {other}"
                            )));
                        }
                    }
                }
                Err(HttpFailure::Status {
                    status,
                    ack_id,
                    detail,
                }) if (400..500).contains(&status) && status != 408 && status != 429 => {
                    return Ok(RemoteReconcileOutcome::Rejected {
                        ack_id,
                        reason: detail,
                    });
                }
                Err(HttpFailure::Status { ack_id, .. }) => {
                    last_ack = ack_id.or(last_ack);
                }
                Err(HttpFailure::Transport(_) | HttpFailure::Protocol(_)) => {}
            }

            if attempt + 1 < self.policy.reconcile_attempts.max(1)
                && self.policy.reconcile_backoff_ms > 0
            {
                let shift = u32::from(attempt.min(8));
                let multiplier = 1_u64 << shift;
                let delay = self
                    .policy
                    .reconcile_backoff_ms
                    .saturating_mul(multiplier)
                    .min(4_000);
                thread::sleep(Duration::from_millis(delay));
            }
        }

        Ok(RemoteReconcileOutcome::Pending { ack_id: last_ack })
    }

    fn cancel(
        &mut self,
        context: &RemoteExecutionContext,
        operation_id: &str,
        request_id: &str,
    ) -> Result<RemoteCancelOutcome, ExecutionError> {
        context.validate()?;
        validate_identity("operation", operation_id)?;
        validate_identity("request", request_id)?;
        if context.operation_id != operation_id || context.request_id != request_id {
            return Err(ExecutionError::InvalidRequest(
                "remote cancellation identity mismatch".into(),
            ));
        }
        let body = IdentityWireRequest {
            version: 1,
            operation_id,
            request_id,
            device_id: &context.device_id,
            account_fence: &context.account_fence,
            account_epoch: context.account_epoch,
            permission_grant_id: &context.permission_grant_id,
        };
        match self.request(CANCEL_PATH, context, &body) {
            Ok(response) if matches!(response.status.as_str(), "cancelled" | "rejected") => {
                Ok(RemoteCancelOutcome::Confirmed {
                    ack_id: response.ack_id,
                })
            }
            Ok(response) => Ok(RemoteCancelOutcome::OutcomeUnknown {
                ack_id: response.ack_id,
                reason: sanitize_reason(format!(
                    "remote cancel returned status {}",
                    response.status
                )),
            }),
            Err(HttpFailure::Status {
                status,
                ack_id,
                detail,
            }) if (400..500).contains(&status) && status != 408 && status != 429 => {
                Ok(RemoteCancelOutcome::Confirmed { ack_id })
            }
            Err(HttpFailure::Status {
                ack_id, detail, ..
            }) => Ok(RemoteCancelOutcome::OutcomeUnknown {
                ack_id,
                reason: detail,
            }),
            Err(HttpFailure::Transport(reason) | HttpFailure::Protocol(reason)) => {
                Ok(RemoteCancelOutcome::OutcomeUnknown {
                    ack_id: None,
                    reason,
                })
            }
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExecuteWireRequest<'a> {
    version: u32,
    operation_id: &'a str,
    request_id: &'a str,
    device_id: &'a str,
    capability_id: &'a str,
    input_json: &'a str,
    timeout_ms: u64,
    account_fence: &'a str,
    account_epoch: u64,
    permission_grant_id: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IdentityWireRequest<'a> {
    version: u32,
    operation_id: &'a str,
    request_id: &'a str,
    device_id: &'a str,
    account_fence: &'a str,
    account_epoch: u64,
    permission_grant_id: &'a str,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireResponse {
    #[serde(default)]
    ack_id: Option<String>,
    status: String,
    #[serde(default)]
    output_json: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

enum HttpFailure {
    Status {
        status: u16,
        ack_id: Option<String>,
        detail: String,
    },
    Transport(String),
    Protocol(String),
}

fn dispatch_from_wire(
    response: WireResponse,
    request: &ExecutionRequest,
) -> Result<RemoteDispatchOutcome, ExecutionError> {
    match response.status.as_str() {
        "completed" => {
            let ack_id = response
                .ack_id
                .ok_or_else(|| ExecutionError::Transport("remote completion omitted ack_id".into()))?;
            let output_json = response.output_json.ok_or_else(|| {
                ExecutionError::Transport("remote completion omitted output_json".into())
            })?;
            Ok(RemoteDispatchOutcome::Completed {
                ack_id,
                result: ExecutionResult {
                    operation_id: request.operation_id.clone(),
                    output_json,
                },
            })
        }
        "rejected" | "cancelled" => Ok(RemoteDispatchOutcome::Rejected {
            ack_id: response.ack_id,
            reason: sanitize_reason(
                response
                    .error
                    .unwrap_or_else(|| response.status.clone()),
            ),
        }),
        "accepted" | "pending" | "running" | "outcome_unknown" => {
            Ok(RemoteDispatchOutcome::OutcomeUnknown {
                ack_id: response.ack_id,
                reason: sanitize_reason(format!(
                    "remote execution acknowledged without terminal result: {}",
                    response.status
                )),
            })
        }
        other => Err(ExecutionError::Transport(format!(
            "unsupported remote execution status {other}"
        ))),
    }
}

fn validate_policy(policy: &RemoteTransportPolicy) -> Result<(), ExecutionError> {
    if policy.connect_timeout_ms == 0
        || policy.request_timeout_ms == 0
        || policy.reconcile_attempts == 0
        || policy.max_response_bytes == 0
        || policy.max_response_bytes > 32 * 1024 * 1024
    {
        return Err(ExecutionError::InvalidRequest(
            "remote transport policy is invalid".into(),
        ));
    }
    Ok(())
}

fn validate_endpoint(value: &str) -> Result<Url, ExecutionError> {
    let parsed = Url::parse(value)
        .map_err(|_| ExecutionError::InvalidRequest("remote endpoint is invalid".into()))?;
    let is_tls = parsed.scheme() == "https";
    let is_test_loopback = cfg!(test)
        && parsed.scheme() == "http"
        && matches!(parsed.host_str(), Some("127.0.0.1" | "localhost" | "::1"));
    if !is_tls && !is_test_loopback {
        return Err(ExecutionError::InvalidRequest(
            "remote endpoint must use HTTPS".into(),
        ));
    }
    if parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(ExecutionError::InvalidRequest(
            "remote endpoint contains forbidden authority components".into(),
        ));
    }
    Ok(parsed)
}

fn validate_identity(label: &str, value: &str) -> Result<(), ExecutionError> {
    if value.trim().is_empty()
        || value.len() > MAX_IDENTITY_BYTES
        || value.chars().any(char::is_control)
        || value.contains('\n')
        || value.contains('\r')
    {
        return Err(ExecutionError::InvalidRequest(format!(
            "remote {label} identity is invalid"
        )));
    }
    Ok(())
}

fn decode_response(
    response: ureq::Response,
    max_response_bytes: usize,
) -> Result<WireResponse, String> {
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take((max_response_bytes as u64).saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| sanitize_reason(error.to_string()))?;
    if bytes.len() > max_response_bytes {
        return Err("remote response exceeded bounded size".into());
    }
    serde_json::from_slice(&bytes)
        .map_err(|error| sanitize_reason(format!("invalid remote response JSON: {error}")))
}

fn sanitize_reason(value: impl Into<String>) -> String {
    let value = value.into();
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(512)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read as _, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    fn context(port: u16) -> (AuthenticatedRemoteHttpTransport, RemoteExecutionContext) {
        let transport = AuthenticatedRemoteHttpTransport::new(
            &format!("http://127.0.0.1:{port}"),
            RemoteTransportPolicy {
                connect_timeout_ms: 1_000,
                request_timeout_ms: 1_000,
                reconcile_attempts: 3,
                reconcile_backoff_ms: 1,
                max_response_bytes: 64 * 1024,
            },
        )
        .unwrap();
        let context = RemoteExecutionContext {
            bearer: RemoteBearerCredential::new("real-enough-secret-token-for-test").unwrap(),
            account_fence: "session:abc123".into(),
            account_epoch: 7,
            operation_id: "op-1".into(),
            request_id: "req-1".into(),
            permission_grant_id: "grant-1".into(),
            device_id: "device-1".into(),
        };
        (transport, context)
    }

    fn execution_request() -> ExecutionRequest {
        ExecutionRequest {
            operation_id: "op-1".into(),
            capability_id: "computer.use".into(),
            input_json: r#"{"action":"click","x":10,"y":20}"#.into(),
            timeout_ms: 30_000,
        }
    }

    fn spawn_json_server(
        responses: Vec<(u16, &'static str)>,
    ) -> (u16, Arc<Mutex<Vec<String>>>, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let captured = Arc::new(Mutex::new(Vec::new()));
        let captured_worker = Arc::clone(&captured);
        let handle = std::thread::spawn(move || {
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut data = Vec::new();
                let mut chunk = [0_u8; 4096];
                let header_end = loop {
                    let count = stream.read(&mut chunk).unwrap();
                    assert!(count > 0);
                    data.extend_from_slice(&chunk[..count]);
                    if let Some(index) = data.windows(4).position(|window| window == b"\r\n\r\n") {
                        break index + 4;
                    }
                };
                let header_text = String::from_utf8_lossy(&data[..header_end]);
                let content_length = header_text
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                    .unwrap_or(0);
                while data.len() < header_end + content_length {
                    let count = stream.read(&mut chunk).unwrap();
                    if count == 0 {
                        break;
                    }
                    data.extend_from_slice(&chunk[..count]);
                }
                captured_worker
                    .lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&data).into_owned());
                let reason = if status == 200 { "OK" } else { "Rejected" };
                write!(
                    stream,
                    "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .unwrap();
                stream.flush().unwrap();
            }
        });
        (port, captured, handle)
    }

    #[test]
    fn execute_binds_bearer_account_epoch_operation_request_grant_and_device() {
        let (port, captured, handle) = spawn_json_server(vec![(
            200,
            r#"{"ackId":"ack-1","status":"completed","outputJson":"{\"ok\":true}"}"#,
        )]);
        let (mut transport, context) = context(port);
        let outcome = transport.execute(&context, &execution_request()).unwrap();
        assert!(matches!(
            outcome,
            RemoteDispatchOutcome::Completed {
                ack_id,
                result: ExecutionResult { output_json, .. }
            } if ack_id == "ack-1" && output_json == r#"{"ok":true}"#
        ));
        handle.join().unwrap();
        let request = captured.lock().unwrap().join("\n");
        assert!(request.contains("Authorization: Bearer real-enough-secret-token-for-test"));
        assert!(request.contains("X-Fabushi-Account-Fence: session:abc123"));
        assert!(request.contains("X-Fabushi-Account-Epoch: 7"));
        assert!(request.contains("X-Fabushi-Operation-Id: op-1"));
        assert!(request.contains("X-Fabushi-Request-Id: req-1"));
        assert!(request.contains("X-Fabushi-Permission-Grant-Id: grant-1"));
        assert!(request.contains("X-Fabushi-Device-Id: device-1"));
        assert!(request.contains(EXECUTE_PATH));
        assert!(!format!("{context:?}").contains("real-enough-secret-token-for-test"));
    }

    #[test]
    fn explicit_remote_rejection_is_terminal_and_not_retried() {
        let (port, captured, handle) = spawn_json_server(vec![(
            403,
            r#"{"ackId":"ack-denied","status":"rejected","error":"grant_rejected"}"#,
        )]);
        let (mut transport, context) = context(port);
        let outcome = transport.execute(&context, &execution_request()).unwrap();
        assert!(matches!(
            outcome,
            RemoteDispatchOutcome::Rejected {
                ack_id: Some(ack_id),
                reason
            } if ack_id == "ack-denied" && reason == "grant_rejected"
        ));
        handle.join().unwrap();
        assert_eq!(captured.lock().unwrap().len(), 1);
    }

    #[test]
    fn network_interruption_after_dispatch_is_outcome_unknown_without_blind_replay() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let accepted = Arc::new(Mutex::new(0_usize));
        let accepted_worker = Arc::clone(&accepted);
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            *accepted_worker.lock().unwrap() += 1;
            let mut buffer = [0_u8; 2048];
            let _ = stream.read(&mut buffer);
            drop(stream);
        });
        let (mut transport, context) = context(port);
        let outcome = transport.execute(&context, &execution_request()).unwrap();
        assert!(matches!(outcome, RemoteDispatchOutcome::OutcomeUnknown { .. }));
        handle.join().unwrap();
        assert_eq!(*accepted.lock().unwrap(), 1);
    }

    #[test]
    fn reconciliation_retries_only_the_idempotent_query_until_terminal() {
        let (port, captured, handle) = spawn_json_server(vec![
            (200, r#"{"ackId":"ack-1","status":"pending"}"#),
            (
                200,
                r#"{"ackId":"ack-1","status":"completed","outputJson":"{\"status\":\"done\"}"}"#,
            ),
        ]);
        let (mut transport, context) = context(port);
        let result = transport.reconcile(&context, "op-1", "req-1").unwrap();
        assert!(matches!(
            result,
            RemoteReconcileOutcome::Completed {
                ack_id,
                result: ExecutionResult { output_json, .. }
            } if ack_id == "ack-1" && output_json.contains("done")
        ));
        handle.join().unwrap();
        let requests = captured.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests.iter().all(|request| request.contains(RECONCILE_PATH)));
    }

    #[test]
    fn cancellation_is_bound_to_the_same_identity() {
        let (port, captured, handle) = spawn_json_server(vec![(
            200,
            r#"{"ackId":"cancel-ack","status":"cancelled"}"#,
        )]);
        let (mut transport, context) = context(port);
        let outcome = transport.cancel(&context, "op-1", "req-1").unwrap();
        assert_eq!(
            outcome,
            RemoteCancelOutcome::Confirmed {
                ack_id: Some("cancel-ack".into())
            }
        );
        handle.join().unwrap();
        let request = captured.lock().unwrap().join("\n");
        assert!(request.contains(CANCEL_PATH));
        assert!(request.contains("X-Fabushi-Permission-Grant-Id: grant-1"));
    }

    #[test]
    fn production_transport_rejects_plaintext_non_loopback_and_credential_debug_is_redacted() {
        assert!(AuthenticatedRemoteHttpTransport::new(
            "http://example.com",
            RemoteTransportPolicy::default()
        )
        .is_err());
        let secret = RemoteBearerCredential::new("this-is-a-long-enough-secret").unwrap();
        assert_eq!(format!("{secret:?}"), "RemoteBearerCredential(<redacted>)");
    }
}
