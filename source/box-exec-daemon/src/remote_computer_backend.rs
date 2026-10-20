use crate::server::{
    RemoteBackendCancelOutcome, RemoteBackendReconcileOutcome, RemoteBackendStartOutcome,
    RemoteExecutionBackend, RemoteRequestIdentity,
};
use fabushi_android_shared::{ExecutionError, ExecutionRequest};
use serde_json::{json, Map, Value};
use std::time::Duration;

pub const REMOTE_COMPUTER_CAPABILITY: &str = "computer.use";
pub const REMOTE_COMPUTER_SCREENSHOT_SETTLE_DELAY_MS: u64 = 2_000;
const MAX_COMPUTER_ACTIONS: usize = 11;

#[derive(Clone, Debug, PartialEq)]
pub enum RemoteComputerAction {
    MouseMove { x: i64, y: i64 },
    Click { arguments: Value },
    MouseDown { arguments: Value },
    MouseUp { arguments: Value },
    Drag { arguments: Value },
    Scroll { arguments: Value },
    Type { text: String },
    Key { key: String, arguments: Value },
    Wait { duration_ms: u64 },
    Screenshot,
    CursorPosition,
}

impl RemoteComputerAction {
    pub fn wire_name(&self) -> &'static str {
        match self {
            Self::MouseMove { .. } => "mouseMove",
            Self::Click { .. } => "click",
            Self::MouseDown { .. } => "mouseDown",
            Self::MouseUp { .. } => "mouseUp",
            Self::Drag { .. } => "drag",
            Self::Scroll { .. } => "scroll",
            Self::Type { .. } => "type",
            Self::Key { .. } => "key",
            Self::Wait { .. } => "wait",
            Self::Screenshot => "screenshot",
            Self::CursorPosition => "cursorPosition",
        }
    }

    fn is_screenshot(&self) -> bool {
        matches!(self, Self::Screenshot)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RemoteComputerRequest {
    pub tool: String,
    pub actions: Vec<RemoteComputerAction>,
}

impl RemoteComputerRequest {
    pub fn parse(request: &ExecutionRequest) -> Result<Self, ExecutionError> {
        if request.capability_id != REMOTE_COMPUTER_CAPABILITY {
            return Err(ExecutionError::CapabilityUnavailable(
                request.capability_id.clone(),
            ));
        }
        let root: Value = serde_json::from_str(&request.input_json).map_err(|error| {
            ExecutionError::InvalidRequest(format!("remote Computer input JSON is malformed: {error}"))
        })?;
        let object = root.as_object().ok_or_else(|| {
            ExecutionError::InvalidRequest("remote Computer input must be an object".into())
        })?;
        if object.keys().any(|key| !matches!(key.as_str(), "tool" | "arguments")) {
            return Err(ExecutionError::InvalidRequest(
                "remote Computer wrapper contains unknown fields".into(),
            ));
        }
        let tool = object
            .get("tool")
            .and_then(Value::as_str)
            .ok_or_else(|| ExecutionError::InvalidRequest("remote Computer wrapper omitted tool".into()))?;
        let arguments = object
            .get("arguments")
            .and_then(Value::as_object)
            .ok_or_else(|| ExecutionError::InvalidRequest("remote Computer wrapper omitted object arguments".into()))?;

        let actions = match tool {
            "Computer" => parse_computer_actions(arguments)?,
            "Screenshot" => {
                if !arguments.is_empty() {
                    return Err(ExecutionError::InvalidRequest(
                        "Remote Screenshot does not accept arguments".into(),
                    ));
                }
                vec![RemoteComputerAction::Screenshot]
            }
            other => {
                return Err(ExecutionError::CapabilityUnavailable(format!(
                    "computer.use does not execute Remote tool {other}"
                )))
            }
        };
        Ok(Self {
            tool: tool.to_string(),
            actions,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RemoteComputerActionOutput {
    pub screenshot: Option<String>,
    pub screenshot_path: Option<String>,
    pub cursor_position: Option<(i64, i64)>,
    pub log: Option<String>,
}

pub trait RemoteComputerExecutor: Send {
    fn execute_action(
        &mut self,
        identity: &RemoteRequestIdentity,
        action: &RemoteComputerAction,
    ) -> Result<RemoteComputerActionOutput, ExecutionError>;

    fn settle_after_screenshot(&mut self, delay_ms: u64) -> Result<(), ExecutionError> {
        std::thread::sleep(Duration::from_millis(delay_ms));
        Ok(())
    }

    fn reconcile(
        &mut self,
        _identity: &RemoteRequestIdentity,
        _request: &RemoteComputerRequest,
    ) -> Result<RemoteBackendReconcileOutcome, ExecutionError> {
        // Computer actions are side-effecting. A generic runner cannot prove whether a
        // transport-lost action crossed the OS boundary, so it must not replay it.
        Ok(RemoteBackendReconcileOutcome::Pending)
    }

    fn cancel(
        &mut self,
        _identity: &RemoteRequestIdentity,
        _request: &RemoteComputerRequest,
    ) -> Result<RemoteBackendCancelOutcome, ExecutionError> {
        Ok(RemoteBackendCancelOutcome::OutcomeUnknown {
            reason: "Remote Computer cancellation requires executor acknowledgement after dispatch".into(),
        })
    }
}

pub struct RemoteComputerBackend<E: RemoteComputerExecutor> {
    executor: E,
}

impl<E: RemoteComputerExecutor> RemoteComputerBackend<E> {
    pub fn new(executor: E) -> Self {
        Self { executor }
    }

    pub fn into_executor(self) -> E {
        self.executor
    }
}

impl<E: RemoteComputerExecutor> RemoteExecutionBackend for RemoteComputerBackend<E> {
    fn start(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
    ) -> Result<RemoteBackendStartOutcome, ExecutionError> {
        let parsed = RemoteComputerRequest::parse(request)?;
        let mut screenshot = None;
        let mut screenshot_path = None;
        let mut cursor_position = None;
        let mut logs = Vec::new();

        for (index, action) in parsed.actions.iter().enumerate() {
            let output = self.executor.execute_action(identity, action)?;
            if output.screenshot.is_some() {
                screenshot = output.screenshot;
            }
            if output.screenshot_path.is_some() {
                screenshot_path = output.screenshot_path;
            }
            if output.cursor_position.is_some() {
                cursor_position = output.cursor_position;
            }
            if let Some(log) = output.log {
                logs.push(log);
            }
            if action.is_screenshot() && index + 1 < parsed.actions.len() {
                self.executor
                    .settle_after_screenshot(REMOTE_COMPUTER_SCREENSHOT_SETTLE_DELAY_MS)?;
            }
        }

        let output = json!({
            "actionCount": parsed.actions.len(),
            "screenshot": screenshot,
            "screenshotPath": screenshot_path,
            "cursorPosition": cursor_position.map(|(x, y)| json!({"x":x,"y":y})),
            "log": (!logs.is_empty()).then(|| logs.join("\n")),
        });
        Ok(RemoteBackendStartOutcome::Completed {
            output_json: output.to_string(),
        })
    }

    fn reconcile(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
        _progress_json: Option<&str>,
    ) -> Result<RemoteBackendReconcileOutcome, ExecutionError> {
        let parsed = RemoteComputerRequest::parse(request)?;
        self.executor.reconcile(identity, &parsed)
    }

    fn cancel(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &ExecutionRequest,
        _progress_json: Option<&str>,
    ) -> Result<RemoteBackendCancelOutcome, ExecutionError> {
        let parsed = RemoteComputerRequest::parse(request)?;
        self.executor.cancel(identity, &parsed)
    }
}

fn parse_computer_actions(arguments: &Map<String, Value>) -> Result<Vec<RemoteComputerAction>, ExecutionError> {
    let mut raw = vec![Value::Object(arguments.clone())];
    if let Some(then) = arguments.get("then") {
        let then = then.as_array().ok_or_else(|| {
            ExecutionError::InvalidRequest("Remote Computer then must be an array".into())
        })?;
        if then.is_empty() || then.len() > MAX_COMPUTER_ACTIONS - 1 {
            return Err(ExecutionError::InvalidRequest(
                "Remote Computer then must contain between 1 and 10 actions".into(),
            ));
        }
        raw.extend(then.iter().cloned());
    }
    if raw.len() > MAX_COMPUTER_ACTIONS {
        return Err(ExecutionError::InvalidRequest(
            "Remote Computer request exceeds 11 actions".into(),
        ));
    }
    raw.iter().map(parse_action).collect()
}

fn parse_action(value: &Value) -> Result<RemoteComputerAction, ExecutionError> {
    let object = value.as_object().ok_or_else(|| {
        ExecutionError::InvalidRequest("Remote Computer action must be an object".into())
    })?;
    let action = object
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| ExecutionError::InvalidRequest("Remote Computer action is required".into()))?;
    let integer = |key: &str| -> Result<i64, ExecutionError> {
        object.get(key).and_then(Value::as_i64).ok_or_else(|| {
            ExecutionError::InvalidRequest(format!("Remote Computer {action} requires integer {key}"))
        })
    };
    let copied = || Value::Object(object.clone());
    match action {
        "mouseMove" | "move" => Ok(RemoteComputerAction::MouseMove {
            x: integer("x")?,
            y: integer("y")?,
        }),
        "click" => Ok(RemoteComputerAction::Click { arguments: copied() }),
        "mouseDown" => Ok(RemoteComputerAction::MouseDown { arguments: copied() }),
        "mouseUp" => Ok(RemoteComputerAction::MouseUp { arguments: copied() }),
        "drag" => Ok(RemoteComputerAction::Drag { arguments: copied() }),
        "scroll" => Ok(RemoteComputerAction::Scroll { arguments: copied() }),
        "type" => Ok(RemoteComputerAction::Type {
            text: object.get("text").and_then(Value::as_str).ok_or_else(|| {
                ExecutionError::InvalidRequest("Remote Computer type requires text".into())
            })?.to_string(),
        }),
        "key" => Ok(RemoteComputerAction::Key {
            key: object.get("key").and_then(Value::as_str).ok_or_else(|| {
                ExecutionError::InvalidRequest("Remote Computer key requires key".into())
            })?.to_string(),
            arguments: copied(),
        }),
        "wait" => Ok(RemoteComputerAction::Wait {
            duration_ms: object.get("durationMs").and_then(Value::as_u64).unwrap_or(0),
        }),
        "screenshot" => Ok(RemoteComputerAction::Screenshot),
        "cursorPosition" => Ok(RemoteComputerAction::CursorPosition),
        other => Err(ExecutionError::InvalidRequest(format!(
            "Remote Computer action is unsupported: {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingExecutor {
        actions: Vec<&'static str>,
        identities: Vec<(String, u64, String)>,
        settles: Vec<u64>,
        reconcile_calls: usize,
        cancel_calls: usize,
    }

    impl RemoteComputerExecutor for RecordingExecutor {
        fn execute_action(
            &mut self,
            identity: &RemoteRequestIdentity,
            action: &RemoteComputerAction,
        ) -> Result<RemoteComputerActionOutput, ExecutionError> {
            self.actions.push(action.wire_name());
            self.identities.push((
                identity.account_fence.clone(),
                identity.account_epoch,
                identity.device_id.clone(),
            ));
            Ok(match action {
                RemoteComputerAction::Screenshot => RemoteComputerActionOutput {
                    screenshot: Some("image-base64".into()),
                    ..Default::default()
                },
                RemoteComputerAction::CursorPosition => RemoteComputerActionOutput {
                    cursor_position: Some((7, 9)),
                    ..Default::default()
                },
                _ => RemoteComputerActionOutput::default(),
            })
        }

        fn settle_after_screenshot(&mut self, delay_ms: u64) -> Result<(), ExecutionError> {
            self.settles.push(delay_ms);
            Ok(())
        }

        fn reconcile(
            &mut self,
            _identity: &RemoteRequestIdentity,
            _request: &RemoteComputerRequest,
        ) -> Result<RemoteBackendReconcileOutcome, ExecutionError> {
            self.reconcile_calls += 1;
            Ok(RemoteBackendReconcileOutcome::Pending)
        }

        fn cancel(
            &mut self,
            _identity: &RemoteRequestIdentity,
            _request: &RemoteComputerRequest,
        ) -> Result<RemoteBackendCancelOutcome, ExecutionError> {
            self.cancel_calls += 1;
            Ok(RemoteBackendCancelOutcome::OutcomeUnknown {
                reason: "already dispatched".into(),
            })
        }
    }

    fn identity() -> RemoteRequestIdentity {
        RemoteRequestIdentity {
            credential_id: "credential-1".into(),
            operation_id: "operation-1".into(),
            request_id: "request-1".into(),
            account_fence: "account-a:epoch-7".into(),
            account_epoch: 7,
            permission_grant_id: "grant-1".into(),
            device_id: "device-1".into(),
        }
    }

    fn request(arguments: Value) -> ExecutionRequest {
        ExecutionRequest {
            operation_id: "operation-1".into(),
            capability_id: REMOTE_COMPUTER_CAPABILITY.into(),
            input_json: json!({"tool":"Computer","arguments":arguments}).to_string(),
            timeout_ms: 30_000,
        }
    }

    #[test]
    fn wire_decodes_and_executes_all_eleven_desktop_actions_in_order() {
        let req = request(json!({
            "action":"mouseMove","x":1,"y":2,
            "then":[
                {"action":"click","x":3,"y":4,"button":"back","count":1,"modifierKeys":"shift"},
                {"action":"mouseDown","button":"left"},
                {"action":"mouseUp","button":"left"},
                {"action":"drag","path":[{"x":1,"y":2},{"x":3,"y":4}],"button":"left"},
                {"action":"scroll","direction":"down","amount":120},
                {"action":"type","text":"hello"},
                {"action":"key","key":"ENTER","holdDurationMs":50},
                {"action":"wait","durationMs":10},
                {"action":"screenshot"},
                {"action":"cursorPosition"}
            ]
        }));
        let mut backend = RemoteComputerBackend::new(RecordingExecutor::default());
        let outcome = backend.start(&identity(), &req).unwrap();
        let executor = backend.into_executor();
        assert_eq!(executor.actions, vec![
            "mouseMove","click","mouseDown","mouseUp","drag","scroll",
            "type","key","wait","screenshot","cursorPosition"
        ]);
        assert!(executor.identities.iter().all(|(fence, epoch, device)|
            fence == "account-a:epoch-7" && *epoch == 7 && device == "device-1"
        ));
        assert_eq!(executor.settles, vec![REMOTE_COMPUTER_SCREENSHOT_SETTLE_DELAY_MS]);
        match outcome {
            RemoteBackendStartOutcome::Completed { output_json } => {
                let output: Value = serde_json::from_str(&output_json).unwrap();
                assert_eq!(output["actionCount"], 11);
                assert_eq!(output["screenshot"], "image-base64");
                assert_eq!(output["cursorPosition"]["x"], 7);
                assert_eq!(output["cursorPosition"]["y"], 9);
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
    }

    #[test]
    fn screenshot_settles_only_when_a_following_action_exists() {
        let mut backend = RemoteComputerBackend::new(RecordingExecutor::default());
        backend.start(&identity(), &request(json!({
            "action":"screenshot",
            "then":[{"action":"click","x":1,"y":2}]
        }))).unwrap();
        assert_eq!(
            backend.executor.settles,
            vec![REMOTE_COMPUTER_SCREENSHOT_SETTLE_DELAY_MS]
        );

        let standalone = ExecutionRequest {
            operation_id: "operation-1".into(),
            capability_id: REMOTE_COMPUTER_CAPABILITY.into(),
            input_json: json!({"tool":"Screenshot","arguments":{}}).to_string(),
            timeout_ms: 30_000,
        };
        let mut backend = RemoteComputerBackend::new(RecordingExecutor::default());
        backend.start(&identity(), &standalone).unwrap();
        assert!(backend.executor.settles.is_empty());
    }

    #[test]
    fn reconciliation_and_cancel_never_blindly_replay_actions() {
        let req = request(json!({"action":"click","x":1,"y":2}));
        let mut backend = RemoteComputerBackend::new(RecordingExecutor::default());
        assert!(matches!(
            backend.reconcile(&identity(), &req, None).unwrap(),
            RemoteBackendReconcileOutcome::Pending
        ));
        assert!(matches!(
            backend.cancel(&identity(), &req, None).unwrap(),
            RemoteBackendCancelOutcome::OutcomeUnknown { .. }
        ));
        assert!(backend.executor.actions.is_empty());
        assert_eq!(backend.executor.reconcile_calls, 1);
        assert_eq!(backend.executor.cancel_calls, 1);
    }

    #[test]
    fn malformed_tool_and_oversized_followups_fail_closed() {
        let mut wrong = request(json!({"action":"click"}));
        wrong.input_json = json!({"tool":"Shell","arguments":{"command":"pwd"}}).to_string();
        assert!(RemoteComputerRequest::parse(&wrong).is_err());

        let followups = (0..11)
            .map(|_| json!({"action":"wait","durationMs":1}))
            .collect::<Vec<_>>();
        assert!(RemoteComputerRequest::parse(&request(json!({
            "action":"wait","durationMs":1,"then":followups
        }))).is_err());
    }
}
