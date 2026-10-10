use super::subagent_runtime::{
    status_label, DurableSubagentOwner, DurableSubagentRecord, SubagentFrozenTurnConfig,
    SubagentLaunch, SubagentLineage,
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

pub type SubagentTaskReviewCallback = Arc<
    dyn Fn(&str, &str, &str) -> Result<Option<String>, String> + Send + Sync + 'static,
>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubagentSteerReview {
    pub allowed: bool,
    pub reason: String,
}

pub type SubagentSteerReviewCallback = Arc<
    dyn Fn(&str, &str, &str) -> Result<SubagentSteerReview, String> + Send + Sync + 'static,
>;

pub const TASK_TOOL_NAME: &str = "Task";
pub const CHECK_SUBAGENT_TOOL_NAME: &str = "CheckSubagent";
pub const MESSAGE_SUBAGENT_TOOL_NAME: &str = "MessageSubagent";
pub const STOP_SUBAGENT_TOOL_NAME: &str = "StopSubagent";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubagentToolContext {
    pub parent_agent_id: String,
    pub parent_request_id: String,
    pub root_parent_request_id: Option<String>,
    pub account_fence: String,
    pub box_id: String,
    pub quiet_origin: Option<String>,
    pub frozen_turn: SubagentFrozenTurnConfig,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SubagentToolResult {
    pub value: Value,
    pub launch: Option<SubagentLaunch>,
}

#[derive(Clone)]
pub struct SubagentToolBridge {
    owner: Arc<Mutex<DurableSubagentOwner>>,
    review_task: Option<SubagentTaskReviewCallback>,
    review_steer: Option<SubagentSteerReviewCallback>,
}

impl SubagentToolBridge {
    pub fn new(owner: Arc<Mutex<DurableSubagentOwner>>) -> Self {
        Self {
            owner,
            review_task: None,
            review_steer: None,
        }
    }

    pub fn with_task_review(mut self, review: SubagentTaskReviewCallback) -> Self {
        self.review_task = Some(review);
        self
    }

    pub fn with_steer_review(mut self, review: SubagentSteerReviewCallback) -> Self {
        self.review_steer = Some(review);
        self
    }

    pub fn tool_definitions(&self, context: &SubagentToolContext) -> Vec<Value> {
        let allowed = &context.frozen_turn.allowed_subagent_types;
        vec![
            json!({
                "name": TASK_TOOL_NAME,
                "description": "Delegate a self-contained task to a background subagent.",
                "inputSchema": {
                    "type":"object",
                    "required":["prompt"],
                    "additionalProperties":false,
                    "properties":{
                        "prompt":{"type":"string","minLength":1},
                        "subagent_type":{"type":"string","enum":allowed},
                    }
                }
            }),
            json!({
                "name":CHECK_SUBAGENT_TOOL_NAME,
                "inputSchema":{"type":"object","additionalProperties":false,"properties":{"subagent_id":{"type":"string"}}}
            }),
            json!({
                "name":MESSAGE_SUBAGENT_TOOL_NAME,
                "inputSchema":{"type":"object","required":["subagent_id","message"],"additionalProperties":false,"properties":{"subagent_id":{"type":"string","minLength":1},"message":{"type":"string","minLength":1}}}
            }),
            json!({
                "name":STOP_SUBAGENT_TOOL_NAME,
                "inputSchema":{"type":"object","required":["subagent_id"],"additionalProperties":false,"properties":{"subagent_id":{"type":"string","minLength":1}}}
            }),
        ]
    }

    pub fn owner(&self) -> Arc<Mutex<DurableSubagentOwner>> {
        Arc::clone(&self.owner)
    }

    pub fn call(
        &self,
        tool_name: &str,
        args: &Value,
        tool_call_id: &str,
        context: &SubagentToolContext,
        now_ms: u64,
    ) -> Result<SubagentToolResult, String> {
        match tool_name {
            TASK_TOOL_NAME => self.task(args, tool_call_id, context, now_ms),
            CHECK_SUBAGENT_TOOL_NAME => self.check(args, context, now_ms),
            MESSAGE_SUBAGENT_TOOL_NAME => self.message(args, tool_call_id, context, now_ms),
            STOP_SUBAGENT_TOOL_NAME => self.stop(args, context, now_ms),
            _ => Err(format!("unknown generated-subagent tool {tool_name}")),
        }
    }

    fn task(
        &self,
        args: &Value,
        tool_call_id: &str,
        context: &SubagentToolContext,
        now_ms: u64,
    ) -> Result<SubagentToolResult, String> {
        let prompt = required(args, "prompt")?;
        let requested_subagent_type = args
            .get("subagent_type")
            .or_else(|| args.get("subagentType"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let allowed = &context.frozen_turn.allowed_subagent_types;
        if allowed.is_empty() {
            return Err("Task has no available subagent types for this turn".into());
        }
        let subagent_type = match requested_subagent_type {
            Some(value) => allowed
                .iter()
                .find(|candidate| candidate.eq_ignore_ascii_case(value))
                .map(String::as_str)
                .ok_or_else(|| format!("Task subagent type is unavailable for this turn: {value}"))?,
            None => allowed
                .first()
                .map(String::as_str)
                .ok_or_else(|| "Task has no available subagent types for this turn".to_string())?,
        };
        if let Some(review) = self.review_task.as_ref() {
            if let Some(reason) = review(prompt, subagent_type, tool_call_id)? {
                return Ok(SubagentToolResult {
                    value: json!({"status":"review-denied","reason":reason}),
                    launch: None,
                });
            }
        }
        let lineage = SubagentLineage {
            parent_request_id: Some(context.parent_request_id.clone()),
            root_parent_request_id: context
                .root_parent_request_id
                .clone()
                .or_else(|| Some(context.parent_request_id.clone())),
            parent_agent_tool_call_id: Some(tool_call_id.to_string()),
        };
        let launch = self
            .owner
            .lock()
            .map_err(|_| "subagent owner lock poisoned".to_string())?
            .launch(
                &context.parent_agent_id,
                lineage,
                &context.box_id,
                subagent_type,
                tool_call_id,
                prompt,
                &context.account_fence,
                context.quiet_origin.as_deref(),
                context.frozen_turn.clone(),
                now_ms,
            )?;
        let value = json!({
            "subagentId": launch.record.subagent_id,
            "subagentRequestId": launch.record.subagent_request_id,
            "subagentType": launch.record.subagent_type,
            "status": status_label(launch.record.status),
            "duplicate": launch.duplicate,
        });
        Ok(SubagentToolResult {
            value,
            launch: (!launch.duplicate).then_some(launch),
        })
    }

    fn check(
        &self,
        args: &Value,
        context: &SubagentToolContext,
        now_ms: u64,
    ) -> Result<SubagentToolResult, String> {
        let owner = self
            .owner
            .lock()
            .map_err(|_| "subagent owner lock poisoned".to_string())?;
        let value = if let Some(id) = optional(args, "subagent_id", "subagentId") {
            match owner.get(id) {
                Some(record) if record.parent_agent_id == context.parent_agent_id
                    && record.account_fence == context.account_fence =>
                {
                    record_json(record, now_ms)
                }
                _ => json!({"status":"not-running","subagentId":id}),
            }
        } else {
            Value::Array(
                owner
                    .list_running_for_parent(&context.parent_agent_id)
                    .into_iter()
                    .filter(|record| record.account_fence == context.account_fence)
                    .map(|record| record_json(&record, now_ms))
                    .collect(),
            )
        };
        Ok(SubagentToolResult { value, launch: None })
    }

    fn message(
        &self,
        args: &Value,
        _tool_call_id: &str,
        context: &SubagentToolContext,
        now_ms: u64,
    ) -> Result<SubagentToolResult, String> {
        let id = required_alias(args, "subagent_id", "subagentId")?;
        let message = required(args, "message")?;
        if let Some(review) = self.review_steer.as_ref() {
            let decision = review(id, message, _tool_call_id)?;
            if !decision.allowed {
                return Ok(SubagentToolResult {
                    value: json!({
                        "subagentId":id,
                        "status":"review-denied",
                        "delivered":false,
                        "reason":decision.reason,
                    }),
                    launch: None,
                });
            }
        }
        let mut owner = self
            .owner
            .lock()
            .map_err(|_| "subagent owner lock poisoned".to_string())?;
        let epoch = owner.process_epoch();
        let delivered = owner.steer(id, message, &context.account_fence, epoch, now_ms)?;
        Ok(SubagentToolResult {
            value: json!({
                "subagentId": id,
                "status": if delivered {"steering"} else {"not-running"},
                "delivered": delivered,
            }),
            launch: None,
        })
    }

    fn stop(
        &self,
        args: &Value,
        context: &SubagentToolContext,
        now_ms: u64,
    ) -> Result<SubagentToolResult, String> {
        let id = required_alias(args, "subagent_id", "subagentId")?;
        let mut owner = self
            .owner
            .lock()
            .map_err(|_| "subagent owner lock poisoned".to_string())?;
        let epoch = owner.process_epoch();
        let stopped = owner.abort(id, &context.account_fence, epoch, now_ms)?;
        Ok(SubagentToolResult {
            value: json!({
                "subagentId": id,
                "status": if stopped {"aborted"} else {"not-running"},
                "stopped": stopped,
            }),
            launch: None,
        })
    }
}

fn record_json(record: &DurableSubagentRecord, now_ms: u64) -> Value {
    json!({
        "subagentId": record.subagent_id,
        "subagentRequestId": record.subagent_request_id,
        "subagentType": record.subagent_type,
        "title": record.title,
        "status": status_label(record.status),
        "elapsedMs": now_ms.saturating_sub(record.started_at_ms),
        "toolCallCount": record.snapshot.observed_tool_call_count,
        "recentActivity": record.snapshot.recent_activity,
        "transcriptPath": record.snapshot.transcript_path,
        "pendingWake": record.pending_wake,
        "completionResult": record.completion_result,
        "completionError": record.completion_error,
    })
}

fn required<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{key} is required"))
}

fn required_alias<'a>(args: &'a Value, first: &str, second: &str) -> Result<&'a str, String> {
    optional(args, first, second).ok_or_else(|| format!("{first} is required"))
}

fn optional<'a>(args: &'a Value, first: &str, second: &str) -> Option<&'a str> {
    args.get(first)
        .or_else(|| args.get(second))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-subagent-bridge-{label}-{}-{}.json",
            std::process::id(),
            crate::android_json_runtime::now_ms()
        ))
    }

    fn context() -> SubagentToolContext {
        SubagentToolContext {
            parent_agent_id: "parent".into(),
            parent_request_id: "parent-run".into(),
            root_parent_request_id: None,
            account_fence: "acct".into(),
            box_id: "box".into(),
            quiet_origin: None,
            frozen_turn: SubagentFrozenTurnConfig {
                provider_id: "android-host-inference".into(),
                model_id: "default".into(),
                tool_names: vec![
                    CHECK_SUBAGENT_TOOL_NAME.into(),
                    MESSAGE_SUBAGENT_TOOL_NAME.into(),
                    STOP_SUBAGENT_TOOL_NAME.into(),
                ],
                allowed_subagent_types: vec!["general-purpose".into()],
                privacy_mode: "no-storage".into(),
                summarization_binding_id: "android-host-inference:same-provider".into(),
            },
        }
    }

    #[test]
    fn task_definition_and_launch_fail_closed_to_frozen_allowed_types() {
        let path = path("allowed-types");
        let owner = Arc::new(Mutex::new(DurableSubagentOwner::open(&path, 1).unwrap()));
        let bridge = SubagentToolBridge::new(Arc::clone(&owner));
        let ctx = context();
        let definitions = bridge.tool_definitions(&ctx);
        assert_eq!(definitions[0]["inputSchema"]["properties"]["subagent_type"]["enum"], json!(["general-purpose"]));
        assert!(bridge
            .call(
                TASK_TOOL_NAME,
                &json!({"prompt":"research","subagent_type":"computeruse"}),
                "tool-invalid",
                &ctx,
                2,
            )
            .unwrap_err()
            .contains("unavailable"));
        assert!(owner.lock().unwrap().all_records().is_empty());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn task_and_steer_review_run_before_any_durable_mutation() {
        let path = path("review");
        let owner = Arc::new(Mutex::new(DurableSubagentOwner::open(&path, 1).unwrap()));
        let deny_task: SubagentTaskReviewCallback =
            Arc::new(|_, _, _| Ok(Some("launch denied".into())));
        let bridge = SubagentToolBridge::new(Arc::clone(&owner)).with_task_review(deny_task);
        let denied = bridge
            .call(TASK_TOOL_NAME, &json!({"prompt":"research"}), "tool-denied", &context(), 2)
            .unwrap();
        assert_eq!(denied.value["status"], "review-denied");
        assert!(owner.lock().unwrap().all_records().is_empty());

        let allow = SubagentToolBridge::new(Arc::clone(&owner));
        let launched = allow
            .call(TASK_TOOL_NAME, &json!({"prompt":"research"}), "tool-ok", &context(), 3)
            .unwrap();
        let id = launched.value["subagentId"].as_str().unwrap().to_string();
        let deny_steer: SubagentSteerReviewCallback = Arc::new(|_, _, _| {
            Ok(SubagentSteerReview { allowed: false, reason: "steer denied".into() })
        });
        let bridge = SubagentToolBridge::new(Arc::clone(&owner)).with_steer_review(deny_steer);
        let denied = bridge
            .call(
                MESSAGE_SUBAGENT_TOOL_NAME,
                &json!({"subagent_id":id,"message":"redirect"}),
                "steer-denied",
                &context(),
                4,
            )
            .unwrap();
        assert_eq!(denied.value["status"], "review-denied");
        assert_eq!(
            owner.lock().unwrap().get(&id).unwrap().pending_steer,
            None,
            "review denial must precede owner mutation"
        );

        let review_error: SubagentSteerReviewCallback =
            Arc::new(|_, _, _| Err("review unavailable".into()));
        let bridge = SubagentToolBridge::new(Arc::clone(&owner)).with_steer_review(review_error);
        assert!(bridge
            .call(
                MESSAGE_SUBAGENT_TOOL_NAME,
                &json!({"subagent_id":id,"message":"redirect"}),
                "steer-error",
                &context(),
                5,
            )
            .unwrap_err()
            .contains("review unavailable"));
        assert_eq!(owner.lock().unwrap().get(&id).unwrap().pending_steer, None);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn task_check_message_stop_share_one_durable_owner() {
        let path = path("tools");
        let owner = Arc::new(Mutex::new(DurableSubagentOwner::open(&path, 1).unwrap()));
        let bridge = SubagentToolBridge::new(Arc::clone(&owner));
        let launched = bridge
            .call(TASK_TOOL_NAME, &json!({"prompt":"research"}), "tool-1", &context(), 2)
            .unwrap();
        let id = launched.value["subagentId"].as_str().unwrap().to_string();
        assert!(launched.launch.is_some());
        let duplicate = bridge
            .call(TASK_TOOL_NAME, &json!({"prompt":"research"}), "tool-1", &context(), 3)
            .unwrap();
        assert!(duplicate.value["duplicate"].as_bool().unwrap());
        assert!(duplicate.launch.is_none());

        let checked = bridge
            .call(CHECK_SUBAGENT_TOOL_NAME, &json!({"subagent_id":id}), "check", &context(), 4)
            .unwrap();
        assert_eq!(checked.value["status"], "running");

        let messaged = bridge
            .call(
                MESSAGE_SUBAGENT_TOOL_NAME,
                &json!({"subagent_id":id,"message":"change direction"}),
                "steer",
                &context(),
                5,
            )
            .unwrap();
        assert_eq!(messaged.value["delivered"], true);

        let stopped = bridge
            .call(
                STOP_SUBAGENT_TOOL_NAME,
                &json!({"subagent_id":id}),
                "stop",
                &context(),
                6,
            )
            .unwrap();
        assert_eq!(stopped.value["stopped"], true);
        let _ = fs::remove_file(path);
    }
}
