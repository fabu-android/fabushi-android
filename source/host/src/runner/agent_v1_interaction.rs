use super::android_host_inference::AndroidRoutedToolBridge;
use super::production_turn_agent_owner::ProductionTurnPrivacyMode;
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use prost::Message;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};
use std::time::{Duration, Instant};

pub const ASK_QUESTION_TOOL_NAME: &str = "AskQuestion";
pub const FROZEN_AGENT_PB_BLOB_SHA: &str = "3cd3d020e69b54d1a6df27c0f967afe7e9fc942c";
pub const FROZEN_ASK_QUESTION_PB_BLOB_SHA: &str = "79a7cbf0decd0bc9e8fa5940378a1ec28bbf8ba5";

#[derive(Clone, PartialEq, Message)]
pub struct AskQuestionOption {
    #[prost(string, tag = "1")]
    pub id: String,
    #[prost(string, tag = "2")]
    pub label: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct AskQuestionQuestion {
    #[prost(string, tag = "1")]
    pub id: String,
    #[prost(string, tag = "2")]
    pub prompt: String,
    #[prost(message, repeated, tag = "3")]
    pub options: Vec<AskQuestionOption>,
    #[prost(bool, tag = "4")]
    pub allow_multiple: bool,
}

#[derive(Clone, PartialEq, Message)]
pub struct AskQuestionArgs {
    #[prost(string, tag = "1")]
    pub title: String,
    #[prost(message, repeated, tag = "2")]
    pub questions: Vec<AskQuestionQuestion>,
    #[prost(bool, tag = "5")]
    pub run_async: bool,
    #[prost(string, tag = "6")]
    pub async_original_tool_call_id: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct AskQuestionInteractionQuery {
    #[prost(message, optional, tag = "1")]
    pub args: Option<AskQuestionArgs>,
    #[prost(string, tag = "2")]
    pub tool_call_id: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct InteractionQuery {
    #[prost(uint32, tag = "1")]
    pub id: u32,
    #[prost(oneof = "interaction_query::Query", tags = "3")]
    pub query: Option<interaction_query::Query>,
}
pub mod interaction_query {
    #[derive(Clone, PartialEq, prost::Oneof)]
    pub enum Query {
        #[prost(message, tag = "3")]
        AskQuestionInteractionQuery(super::AskQuestionInteractionQuery),
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct AskQuestionAnswer {
    #[prost(string, tag = "1")]
    pub question_id: String,
    #[prost(string, repeated, tag = "2")]
    pub selected_option_ids: Vec<String>,
    #[prost(string, tag = "3")]
    pub freeform_text: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct AskQuestionSuccess {
    #[prost(message, repeated, tag = "1")]
    pub answers: Vec<AskQuestionAnswer>,
}
#[derive(Clone, PartialEq, Message)]
pub struct AskQuestionError {
    #[prost(string, tag = "1")]
    pub error_message: String,
}
#[derive(Clone, PartialEq, Message)]
pub struct AskQuestionRejected {
    #[prost(string, tag = "1")]
    pub reason: String,
}
#[derive(Clone, PartialEq, Message)]
pub struct AskQuestionAsync {}

#[derive(Clone, PartialEq, Message)]
pub struct AskQuestionResult {
    #[prost(oneof = "ask_question_result::Result", tags = "1, 2, 3, 4")]
    pub result: Option<ask_question_result::Result>,
}
pub mod ask_question_result {
    #[derive(Clone, PartialEq, prost::Oneof)]
    pub enum Result {
        #[prost(message, tag = "1")]
        Success(super::AskQuestionSuccess),
        #[prost(message, tag = "2")]
        Error(super::AskQuestionError),
        #[prost(message, tag = "3")]
        Rejected(super::AskQuestionRejected),
        #[prost(message, tag = "4")]
        Async(super::AskQuestionAsync),
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct AskQuestionInteractionResponse {
    #[prost(message, optional, tag = "1")]
    pub result: Option<AskQuestionResult>,
}

#[derive(Clone, PartialEq, Message)]
pub struct InteractionResponse {
    #[prost(uint32, tag = "1")]
    pub id: u32,
    #[prost(oneof = "interaction_response::Result", tags = "3")]
    pub result: Option<interaction_response::Result>,
}
pub mod interaction_response {
    #[derive(Clone, PartialEq, prost::Oneof)]
    pub enum Result {
        #[prost(message, tag = "3")]
        AskQuestionInteractionResponse(super::AskQuestionInteractionResponse),
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct TurnEndedUpdate {
    #[prost(int64, optional, tag = "1")]
    pub input_tokens: Option<i64>,
    #[prost(int64, optional, tag = "2")]
    pub output_tokens: Option<i64>,
    #[prost(int64, optional, tag = "3")]
    pub cache_read_tokens: Option<i64>,
    #[prost(int64, optional, tag = "4")]
    pub cache_write_tokens: Option<i64>,
    #[prost(int64, optional, tag = "5")]
    pub reasoning_tokens: Option<i64>,
}

#[derive(Clone, PartialEq, Message)]
pub struct InteractionUpdate {
    #[prost(oneof = "interaction_update::Message", tags = "14")]
    pub message: Option<interaction_update::Message>,
}
pub mod interaction_update {
    #[derive(Clone, PartialEq, prost::Oneof)]
    pub enum Message {
        #[prost(message, tag = "14")]
        TurnEnded(super::TurnEndedUpdate),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InteractionFence {
    pub account_fence: String,
    pub process_epoch: u64,
}

#[derive(Clone)]
struct PendingAskQuestion {
    fence: InteractionFence,
    tool_call_id: String,
    original_tool_call_id: String,
    original_args: AskQuestionArgs,
    run_async: bool,
    privacy_mode: ProductionTurnPrivacyMode,
    response: Option<AskQuestionResult>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct AsyncCompletionProjection {
    pub query_id: u32,
    pub tool_call_id: String,
    pub original_tool_call_id: String,
    pub original_args_base64: String,
    pub result_base64: String,
    pub account_fence: String,
    pub process_epoch: u64,
    pub result_kind: &'static str,
    pub redacted: bool,
}

impl std::fmt::Debug for AsyncCompletionProjection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AsyncCompletionProjection")
            .field("query_id", &self.query_id)
            .field("tool_call_id", &self.tool_call_id)
            .field("original_tool_call_id", &self.original_tool_call_id)
            .field("original_args_base64", &"[REDACTED]")
            .field("result_base64", &"[REDACTED]")
            .field("account_fence", &self.account_fence)
            .field("process_epoch", &self.process_epoch)
            .field("result_kind", &self.result_kind)
            .field("redacted", &self.redacted)
            .finish()
    }
}

#[derive(Default)]
struct InteractionState {
    next_query_id: u32,
    pending: BTreeMap<u32, PendingAskQuestion>,
    pending_async_original_tool_call_ids: BTreeSet<(String, u64, String)>,
    completed_async_original_tool_call_ids: BTreeSet<(String, u64, String)>,
    post_turn: VecDeque<AsyncCompletionProjection>,
    turn_ended: bool,
}

#[derive(Clone, Default)]
pub struct SharedAgentInteractionOwner {
    state: Arc<(Mutex<InteractionState>, Condvar)>,
}

impl SharedAgentInteractionOwner {
    pub fn begin_ask_question(
        &self,
        fence: InteractionFence,
        tool_call_id: &str,
        mut args: AskQuestionArgs,
        privacy_mode: ProductionTurnPrivacyMode,
    ) -> Result<(u32, Vec<u8>), String> {
        if fence.account_fence.trim().is_empty() || fence.process_epoch == 0 {
            return Err("interaction fence is invalid".into());
        }
        if tool_call_id.trim().is_empty() {
            return Err("AskQuestion tool_call_id is required".into());
        }
        if args.questions.is_empty() {
            return Err("AskQuestion requires at least one question".into());
        }
        let supplied_original_id = args.async_original_tool_call_id.trim();
        let original_tool_call_id = if supplied_original_id.is_empty() {
            tool_call_id.to_string()
        } else {
            supplied_original_id.to_string()
        };
        // Desktop replay-horizon matching falls back to the tool call ID when
        // async_original_tool_call_id is absent. Canonicalize it before sending
        // the interaction query so a later completion has the same identity.
        args.async_original_tool_call_id = original_tool_call_id.clone();

        let (lock, _) = &*self.state;
        let mut state = lock.lock().map_err(|_| "interaction owner lock poisoned".to_string())?;
        let original_key = (
            fence.account_fence.clone(),
            fence.process_epoch,
            original_tool_call_id.clone(),
        );
        if args.run_async
            && (state.pending_async_original_tool_call_ids.contains(&original_key)
                || state.completed_async_original_tool_call_ids.contains(&original_key))
        {
            return Err("duplicate async AskQuestion original tool call ID".into());
        }
        state.next_query_id = state.next_query_id.checked_add(1).ok_or("interaction query id exhausted")?;
        if state.next_query_id == 0 {
            state.next_query_id = 1;
        }
        let id = state.next_query_id;
        let run_async = args.run_async;
        if run_async {
            state.pending_async_original_tool_call_ids.insert(original_key);
        }
        state.pending.insert(
            id,
            PendingAskQuestion {
                fence,
                tool_call_id: tool_call_id.to_string(),
                original_tool_call_id,
                original_args: args.clone(),
                run_async,
                privacy_mode,
                response: None,
            },
        );
        let query = InteractionQuery {
            id,
            query: Some(interaction_query::Query::AskQuestionInteractionQuery(
                AskQuestionInteractionQuery {
                    args: Some(args),
                    tool_call_id: tool_call_id.to_string(),
                },
            )),
        };
        Ok((id, query.encode_to_vec()))
    }

    pub fn settle_response(
        &self,
        expected: &InteractionFence,
        bytes: &[u8],
    ) -> Result<bool, String> {
        let response = InteractionResponse::decode(bytes)
            .map_err(|error| format!("invalid InteractionResponse protobuf: {error}"))?;
        let result = match response.result {
            Some(interaction_response::Result::AskQuestionInteractionResponse(value)) => value
                .result
                .ok_or("AskQuestionInteractionResponse.result is required")?,
            None => return Err("InteractionResponse result is required".into()),
        };
        let (lock, ready) = &*self.state;
        let mut state = lock.lock().map_err(|_| "interaction owner lock poisoned".to_string())?;
        let (run_async, projection, original_key, should_receipt) = {
            let pending = state
                .pending
                .get_mut(&response.id)
                .ok_or("interaction response is stale or unknown")?;
            if &pending.fence != expected {
                return Err("interaction response belongs to a stale account/process epoch".into());
            }
            if pending.response.is_some() {
                return Err("interaction response already settled".into());
            }
            if pending.run_async && !is_valid_async_completion_result(&result) {
                return Err("async AskQuestion completion must contain success, error, or rejected result".into());
            }
            pending.response = Some(result.clone());
            let run_async = pending.run_async;
            let projection = run_async.then(|| completion_projection(response.id, pending, &result));
            let original_key = (
                pending.fence.account_fence.clone(),
                pending.fence.process_epoch,
                pending.original_tool_call_id.clone(),
            );
            let should_receipt = run_async && should_receipt_ask_question_result(&result);
            (run_async, projection, original_key, should_receipt)
        };
        if run_async {
            let projection = projection.ok_or("async AskQuestion completion projection is missing")?;
            state.post_turn.push_back(projection);
            state.pending_async_original_tool_call_ids.remove(&original_key);
            if should_receipt {
                state.completed_async_original_tool_call_ids.insert(original_key);
            }
            state.pending.remove(&response.id);
        }
        ready.notify_all();
        Ok(run_async)
    }

    pub fn wait_sync(
        &self,
        query_id: u32,
        expected: &InteractionFence,
        cancelled: &AtomicBool,
        timeout: Duration,
    ) -> Result<AskQuestionResult, String> {
        let deadline = Instant::now() + timeout;
        let (lock, ready) = &*self.state;
        let mut state = lock.lock().map_err(|_| "interaction owner lock poisoned".to_string())?;
        loop {
            if cancelled.load(Ordering::Acquire) {
                state.pending.remove(&query_id);
                return Err("AskQuestion cancelled".into());
            }
            let pending = state.pending.get(&query_id).ok_or("AskQuestion query is no longer pending")?;
            if &pending.fence != expected {
                state.pending.remove(&query_id);
                return Err("AskQuestion query belongs to a stale account/process epoch".into());
            }
            if pending.run_async {
                return Ok(AskQuestionResult {
                    result: Some(ask_question_result::Result::Async(AskQuestionAsync {})),
                });
            }
            if let Some(result) = pending.response.clone() {
                state.pending.remove(&query_id);
                return Ok(result);
            }
            let now = Instant::now();
            if now >= deadline {
                state.pending.remove(&query_id);
                return Err("AskQuestion response timed out".into());
            }
            let remaining = deadline.saturating_duration_since(now).min(Duration::from_millis(250));
            let (next, _) = ready
                .wait_timeout(state, remaining)
                .map_err(|_| "interaction owner condvar poisoned".to_string())?;
            state = next;
        }
    }

    /// Prunes only completed async receipts for this account/process fence. Pending
    /// interactions are intentionally untouched, and another account's receipts are isolated.
    pub fn prune_completed_async_receipts_from_transcript_window(
        &self,
        expected: &InteractionFence,
        transcript_window: &[serde_json::Value],
    ) -> Result<usize, String> {
        if expected.account_fence.trim().is_empty() || expected.process_epoch == 0 {
            return Err("interaction fence is invalid".into());
        }
        let live_ids =
            crate::extensions::transcript::TranscriptStore::live_async_ask_question_original_tool_call_ids(
                transcript_window,
            );
        let (lock, _) = &*self.state;
        let mut state = lock
            .lock()
            .map_err(|_| "interaction owner lock poisoned".to_string())?;
        let before = state.completed_async_original_tool_call_ids.len();
        state.completed_async_original_tool_call_ids.retain(
            |(account_fence, process_epoch, original_tool_call_id)| {
                account_fence != &expected.account_fence
                    || *process_epoch != expected.process_epoch
                    || live_ids.contains(original_tool_call_id)
            },
        );
        Ok(before.saturating_sub(state.completed_async_original_tool_call_ids.len()))
    }

    pub fn turn_ended(
        &self,
        expected: &InteractionFence,
        usage: TurnEndedUpdate,
        transcript_window: &[serde_json::Value],
    ) -> Result<Vec<u8>, String> {
        self.prune_completed_async_receipts_from_transcript_window(expected, transcript_window)?;
        let (lock, _) = &*self.state;
        let mut state = lock.lock().map_err(|_| "interaction owner lock poisoned".to_string())?;
        for pending in state.pending.values() {
            if &pending.fence != expected {
                return Err("turnEnded attempted across a stale account/process epoch".into());
            }
        }
        state.turn_ended = true;
        Ok(InteractionUpdate {
            message: Some(interaction_update::Message::TurnEnded(usage)),
        }
        .encode_to_vec())
    }

    pub fn drain_post_turn(
        &self,
        expected: &InteractionFence,
    ) -> Result<Vec<AsyncCompletionProjection>, String> {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().map_err(|_| "interaction owner lock poisoned".to_string())?;
        if !state.turn_ended {
            return Ok(Vec::new());
        }
        let mut drained = Vec::new();
        let mut retained = VecDeque::new();
        while let Some(item) = state.post_turn.pop_front() {
            if item.account_fence == expected.account_fence && item.process_epoch == expected.process_epoch {
                drained.push(item);
            } else {
                retained.push_back(item);
            }
        }
        state.post_turn = retained;
        Ok(drained)
    }

    pub fn invalidate_stale(&self, current: &InteractionFence) -> Result<usize, String> {
        let (lock, ready) = &*self.state;
        let mut state = lock.lock().map_err(|_| "interaction owner lock poisoned".to_string())?;
        let before = state.pending.len();
        state.pending.retain(|_, pending| pending.fence == *current);
        state.pending_async_original_tool_call_ids = state
            .pending
            .values()
            .filter(|pending| pending.run_async)
            .map(|pending| (
                pending.fence.account_fence.clone(),
                pending.fence.process_epoch,
                pending.original_tool_call_id.clone(),
            ))
            .collect();
        state.post_turn.retain(|item| {
            item.account_fence == current.account_fence && item.process_epoch == current.process_epoch
        });
        let removed = before.saturating_sub(state.pending.len());
        if removed > 0 {
            ready.notify_all();
        }
        Ok(removed)
    }
}

fn completion_projection(
    query_id: u32,
    pending: &PendingAskQuestion,
    result: &AskQuestionResult,
) -> AsyncCompletionProjection {
    AsyncCompletionProjection {
        query_id,
        tool_call_id: pending.tool_call_id.clone(),
        original_tool_call_id: pending.original_tool_call_id.clone(),
        original_args_base64: BASE64_STANDARD.encode(pending.original_args.encode_to_vec()),
        result_base64: BASE64_STANDARD.encode(result.encode_to_vec()),
        account_fence: pending.fence.account_fence.clone(),
        process_epoch: pending.fence.process_epoch,
        result_kind: ask_question_result_kind(result),
        redacted: pending.privacy_mode != ProductionTurnPrivacyMode::UsageCodebaseTrainingAllowed,
    }
}

fn is_valid_async_completion_result(result: &AskQuestionResult) -> bool {
    matches!(
        result.result.as_ref(),
        Some(ask_question_result::Result::Success(_))
            | Some(ask_question_result::Result::Error(_))
            | Some(ask_question_result::Result::Rejected(_))
    )
}

fn should_receipt_ask_question_result(result: &AskQuestionResult) -> bool {
    match result.result.as_ref() {
        Some(ask_question_result::Result::Success(_))
        | Some(ask_question_result::Result::Error(_)) => true,
        Some(ask_question_result::Result::Rejected(value)) => {
            !fabushi_constants::ask_question::is_ask_question_auto_answer_reason(Some(value.reason.as_str()))
        }
        Some(ask_question_result::Result::Async(_)) | None => false,
    }
}

pub fn ask_question_result_kind(result: &AskQuestionResult) -> &'static str {
    match result.result {
        Some(ask_question_result::Result::Success(_)) => "success",
        Some(ask_question_result::Result::Error(_)) => "error",
        Some(ask_question_result::Result::Rejected(_)) => "rejected",
        Some(ask_question_result::Result::Async(_)) => "async",
        None => "unknown",
    }
}

pub fn format_ask_question_result(result: &AskQuestionResult) -> String {
    match result.result.as_ref() {
        Some(ask_question_result::Result::Success(success)) => {
            let answers = success
                .answers
                .iter()
                .map(|answer| {
                    let freeform = answer.freeform_text.trim();
                    if !answer.selected_option_ids.is_empty() && !freeform.is_empty() {
                        format!(
                            "Question {}: Selected option(s) {}, freeform: {}",
                            answer.question_id,
                            answer.selected_option_ids.join(", "),
                            freeform
                        )
                    } else if !freeform.is_empty() {
                        format!("Question {}: {}", answer.question_id, freeform)
                    } else if !answer.selected_option_ids.is_empty() {
                        format!(
                            "Question {}: Selected option(s) {}",
                            answer.question_id,
                            answer.selected_option_ids.join(", ")
                        )
                    } else {
                        format!("Question {}: No answer provided", answer.question_id)
                    }
                })
                .collect::<Vec<_>>();
            format!("User questions responses:\n{}", answers.join("\n"))
        }
        Some(ask_question_result::Result::Async(_)) => {
            "Questions have been sent to the user asynchronously. You will receive their answers later as a separate tool result. Continue with your reasoning using current information.".into()
        }
        Some(ask_question_result::Result::Rejected(value)) => {
            let reason = value.reason.trim();
            if reason.is_empty() {
                "Questions skipped by the user, continue with the information you already have".into()
            } else {
                reason.to_string()
            }
        }
        Some(ask_question_result::Result::Error(value)) => format!("Error: {}", value.error_message),
        None => "Unknown error".into(),
    }
}

pub fn ask_question_args_from_json(args: &Value) -> Result<AskQuestionArgs, String> {
    let object = args.as_object().ok_or("AskQuestion arguments must be an object")?;
    let title = object.get("title").and_then(Value::as_str).unwrap_or_default().trim().to_string();
    let questions = object
        .get("questions")
        .and_then(Value::as_array)
        .ok_or("AskQuestion questions must be an array")?
        .iter()
        .map(|question| {
            let q = question.as_object().ok_or("AskQuestion question must be an object")?;
            let id = q.get("id").and_then(Value::as_str).unwrap_or_default().trim().to_string();
            let prompt = q.get("prompt").and_then(Value::as_str).unwrap_or_default().trim().to_string();
            if id.is_empty() || prompt.is_empty() {
                return Err("AskQuestion question id/prompt is required".to_string());
            }
            let options = q
                .get("options")
                .and_then(Value::as_array)
                .ok_or("AskQuestion options must be an array")?
                .iter()
                .map(|option| {
                    let option = option.as_object().ok_or("AskQuestion option must be an object")?;
                    let id = option.get("id").and_then(Value::as_str).unwrap_or_default().trim().to_string();
                    let label = option.get("label").and_then(Value::as_str).unwrap_or_default().trim().to_string();
                    if id.is_empty() || label.is_empty() {
                        return Err("AskQuestion option id/label is required".to_string());
                    }
                    Ok(AskQuestionOption { id, label })
                })
                .collect::<Result<Vec<_>, String>>()?;
            if options.len() < 2 {
                return Err("AskQuestion requires at least two options per question".into());
            }
            Ok(AskQuestionQuestion {
                id,
                prompt,
                options,
                allow_multiple: q.get("allow_multiple")
                    .or_else(|| q.get("allowMultiple"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(AskQuestionArgs {
        title,
        questions,
        run_async: object
            .get("run_async")
            .or_else(|| object.get("runAsync"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        async_original_tool_call_id: object
            .get("async_original_tool_call_id")
            .or_else(|| object.get("asyncOriginalToolCallId"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}

pub fn encode_response_base64(response: &InteractionResponse) -> String {
    BASE64_STANDARD.encode(response.encode_to_vec())
}

pub fn decode_response_base64(value: &str) -> Result<Vec<u8>, String> {
    BASE64_STANDARD
        .decode(value)
        .map_err(|error| format!("invalid InteractionResponse base64: {error}"))
}

pub type AskQuestionTranscriptWindowProvider =
    Arc<dyn Fn() -> Result<Vec<serde_json::Value>, String> + Send + Sync>;

pub fn with_ask_question_tool(
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    owner: SharedAgentInteractionOwner,
    fence: InteractionFence,
    privacy_mode: ProductionTurnPrivacyMode,
    cancelled: Arc<AtomicBool>,
    publish_query: Arc<dyn Fn(u32, &[u8], &InteractionFence) -> Result<(), String> + Send + Sync>,
    transcript_window_provider: AskQuestionTranscriptWindowProvider,
) -> Arc<dyn AndroidRoutedToolBridge> {
    Arc::new(AskQuestionRoutedTool {
        delegate,
        owner,
        fence,
        privacy_mode,
        cancelled,
        publish_query,
        transcript_window_provider,
    })
}

struct AskQuestionRoutedTool {
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    owner: SharedAgentInteractionOwner,
    fence: InteractionFence,
    privacy_mode: ProductionTurnPrivacyMode,
    cancelled: Arc<AtomicBool>,
    publish_query: Arc<dyn Fn(u32, &[u8], &InteractionFence) -> Result<(), String> + Send + Sync>,
    transcript_window_provider: AskQuestionTranscriptWindowProvider,
}

impl AndroidRoutedToolBridge for AskQuestionRoutedTool {
    fn list_tools(&self) -> Result<Vec<Value>, String> {
        let mut tools = self.delegate.list_tools()?;
        tools.retain(|tool| tool.get("name").and_then(Value::as_str) != Some(ASK_QUESTION_TOOL_NAME));
        tools.insert(0, json!({
            "type":"function",
            "name":ASK_QUESTION_TOOL_NAME,
            "description":"Ask the user one or more bounded multiple-choice questions, optionally with freeform text.",
            "parameters":{
                "type":"object",
                "additionalProperties":false,
                "required":["questions"],
                "properties":{
                    "title":{"type":"string"},
                    "questions":{"type":"array","minItems":1,"items":{
                        "type":"object","additionalProperties":false,
                        "required":["id","prompt","options"],
                        "properties":{
                            "id":{"type":"string","minLength":1},
                            "prompt":{"type":"string","minLength":1},
                            "options":{"type":"array","minItems":2,"items":{
                                "type":"object","additionalProperties":false,"required":["id","label"],
                                "properties":{"id":{"type":"string","minLength":1},"label":{"type":"string","minLength":1}}
                            }},
                            "allow_multiple":{"type":"boolean"}
                        }
                    }},
                    "run_async":{"type":"boolean"},
                    "async_original_tool_call_id":{"type":"string"}
                }
            }
        }));
        Ok(tools)
    }

    fn call_tool(&self, name: &str, args: Value, tool_call_id: &str) -> Result<Value, String> {
        if name != ASK_QUESTION_TOOL_NAME {
            return self.delegate.call_tool(name, args, tool_call_id);
        }
        let args = ask_question_args_from_json(&args)?;
        let run_async = args.run_async;
        let mut transcript_window = (self.transcript_window_provider)()?;
        // The model's current tool call may not have been persisted into the canonical
        // transcript yet. Include it in the live window before pruning so a replayed
        // completed receipt cannot be removed just before the duplicate is rejected.
        transcript_window.push(json!({
            "kind":"message",
            "role":"assistant",
            "content":[{
                "type":"tool-call",
                "toolName":ASK_QUESTION_TOOL_NAME,
                "toolCallId":tool_call_id,
                "args":{"async_original_tool_call_id":args.async_original_tool_call_id}
            }]
        }));
        self.owner.prune_completed_async_receipts_from_transcript_window(
            &self.fence,
            &transcript_window,
        )?;
        let (query_id, wire) = self.owner.begin_ask_question(
            self.fence.clone(),
            tool_call_id,
            args,
            self.privacy_mode,
        )?;
        (self.publish_query)(query_id, &wire, &self.fence)?;
        let result = if run_async {
            AskQuestionResult {
                result: Some(ask_question_result::Result::Async(AskQuestionAsync {})),
            }
        } else {
            self.owner.wait_sync(
                query_id,
                &self.fence,
                &self.cancelled,
                Duration::from_secs(300),
            )?
        };
        Ok(Value::String(format_ask_question_result(&result)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fence() -> InteractionFence {
        InteractionFence { account_fence: "account-a".into(), process_epoch: 7 }
    }

    fn args(run_async: bool) -> AskQuestionArgs {
        AskQuestionArgs {
            title: "Pick".into(),
            questions: vec![AskQuestionQuestion {
                id: "q1".into(),
                prompt: "Choose".into(),
                options: vec![
                    AskQuestionOption { id: "a".into(), label: "A".into() },
                    AskQuestionOption { id: "b".into(), label: "B".into() },
                ],
                allow_multiple: false,
            }],
            run_async,
            async_original_tool_call_id: String::new(),
        }
    }

    fn assistant_window(tool_calls: &[(&str, &str, Option<&str>)]) -> Vec<serde_json::Value> {
        let content = tool_calls
            .iter()
            .map(|&(name, tool_call_id, original_id)| {
                let mut part = serde_json::json!({
                    "type":"tool-call",
                    "toolName":name,
                    "toolCallId":tool_call_id
                });
                if let Some(original_id) = original_id {
                    part["args"] = serde_json::json!({
                        "async_original_tool_call_id":original_id
                    });
                }
                part
            })
            .collect::<Vec<_>>();
        vec![serde_json::json!({
            "id":"assistant-window",
            "kind":"message",
            "role":"assistant",
            "content":content
        })]
    }

    fn settle_success(owner: &SharedAgentInteractionOwner, fence: &InteractionFence, query_id: u32) {
        let response = InteractionResponse {
            id: query_id,
            result: Some(interaction_response::Result::AskQuestionInteractionResponse(
                AskQuestionInteractionResponse {
                    result: Some(AskQuestionResult {
                        result: Some(ask_question_result::Result::Success(AskQuestionSuccess {
                            answers: vec![],
                        })),
                    }),
                },
            )),
        };
        assert!(owner.settle_response(fence, &response.encode_to_vec()).unwrap());
    }

    #[test]
    fn generated_compatible_interaction_query_response_wire_roundtrip() {
        let owner = SharedAgentInteractionOwner::default();
        let (id, wire) = owner.begin_ask_question(
            fence(),
            "tool-1",
            args(false),
            ProductionTurnPrivacyMode::NoTraining,
        ).unwrap();
        let query = InteractionQuery::decode(wire.as_slice()).unwrap();
        assert_eq!(query.id, id);
        let interaction_query::Query::AskQuestionInteractionQuery(query) = query.query.unwrap();
        assert_eq!(query.tool_call_id, "tool-1");
        assert_eq!(query.args.unwrap().questions[0].options.len(), 2);

        let response = InteractionResponse {
            id,
            result: Some(interaction_response::Result::AskQuestionInteractionResponse(
                AskQuestionInteractionResponse {
                    result: Some(AskQuestionResult {
                        result: Some(ask_question_result::Result::Rejected(AskQuestionRejected {
                            reason: "skip".into(),
                        })),
                    }),
                },
            )),
        };
        assert!(!owner.settle_response(&fence(), &response.encode_to_vec()).unwrap());
    }

    #[test]
    fn run_async_completion_is_enqueued_and_privacy_projection_is_redacted() {
        let owner = SharedAgentInteractionOwner::default();
        let (id, _) = owner.begin_ask_question(
            fence(),
            "async-1",
            args(true),
            ProductionTurnPrivacyMode::NoStorage,
        ).unwrap();
        let response = InteractionResponse {
            id,
            result: Some(interaction_response::Result::AskQuestionInteractionResponse(
                AskQuestionInteractionResponse {
                    result: Some(AskQuestionResult {
                        result: Some(ask_question_result::Result::Success(AskQuestionSuccess {
                            answers: vec![AskQuestionAnswer {
                                question_id: "q1".into(),
                                selected_option_ids: vec!["a".into()],
                                freeform_text: "private answer".into(),
                            }],
                        })),
                    }),
                },
            )),
        };
        assert!(owner.settle_response(&fence(), &response.encode_to_vec()).unwrap());
        assert!(owner.drain_post_turn(&fence()).unwrap().is_empty());
        let window = assistant_window(&[("AskQuestion", "async-1", None)]);
        let wire = owner.turn_ended(&fence(), TurnEndedUpdate::default(), &window).unwrap();
        let update = InteractionUpdate::decode(wire.as_slice()).unwrap();
        assert!(matches!(update.message, Some(interaction_update::Message::TurnEnded(_))));
        let completion = owner.drain_post_turn(&fence()).unwrap();
        assert_eq!(completion.len(), 1);
        assert!(completion[0].redacted);
        assert_eq!(completion[0].result_kind, "success");
        assert_eq!(completion[0].tool_call_id, "async-1");
        assert_eq!(completion[0].original_tool_call_id, "async-1");
        let original_args = AskQuestionArgs::decode(
            BASE64_STANDARD.decode(&completion[0].original_args_base64).unwrap().as_slice(),
        ).unwrap();
        assert_eq!(original_args.async_original_tool_call_id, "async-1");
        let completion_result = AskQuestionResult::decode(
            BASE64_STANDARD.decode(&completion[0].result_base64).unwrap().as_slice(),
        ).unwrap();
        let Some(ask_question_result::Result::Success(success)) = completion_result.result else {
            panic!("expected successful async AskQuestion result");
        };
        assert_eq!(success.answers[0].freeform_text, "private answer");
        let debug = format!("{completion:?}");
        assert!(!debug.contains("private answer"));
        assert!(!debug.contains(&completion[0].result_base64));
        assert!(!debug.contains(&completion[0].original_args_base64));
        assert!(owner.begin_ask_question(
            fence(),
            "async-1",
            args(true),
            ProductionTurnPrivacyMode::NoStorage,
        ).unwrap_err().contains("duplicate async AskQuestion"));
    }

    #[test]
    fn explicit_async_original_tool_call_id_is_the_completion_and_dedup_identity() {
        let owner = SharedAgentInteractionOwner::default();
        let mut question_args = args(true);
        question_args.async_original_tool_call_id = "original-call-7".into();
        let (id, _) = owner.begin_ask_question(
            fence(),
            "runtime-call-7",
            question_args.clone(),
            ProductionTurnPrivacyMode::NoTraining,
        ).unwrap();
        let response = InteractionResponse {
            id,
            result: Some(interaction_response::Result::AskQuestionInteractionResponse(
                AskQuestionInteractionResponse {
                    result: Some(AskQuestionResult {
                        result: Some(ask_question_result::Result::Rejected(AskQuestionRejected {
                            reason: "user skipped".into(),
                        })),
                    }),
                },
            )),
        };
        assert!(owner.settle_response(&fence(), &response.encode_to_vec()).unwrap());
        owner
            .turn_ended(
                &fence(),
                TurnEndedUpdate::default(),
                &assistant_window(&[("AskQuestion", "runtime-call-7", Some("original-call-7"))]),
            )
            .unwrap();
        let completion = owner.drain_post_turn(&fence()).unwrap().pop().unwrap();
        assert_eq!(completion.tool_call_id, "runtime-call-7");
        assert_eq!(completion.original_tool_call_id, "original-call-7");
        let original_args = AskQuestionArgs::decode(
            BASE64_STANDARD.decode(&completion.original_args_base64).unwrap().as_slice(),
        ).unwrap();
        assert_eq!(original_args.async_original_tool_call_id, "original-call-7");
        assert!(owner.begin_ask_question(
            fence(),
            "runtime-call-8",
            question_args,
            ProductionTurnPrivacyMode::NoTraining,
        ).unwrap_err().contains("duplicate async AskQuestion"));
    }

    #[test]
    fn auto_answer_rejection_does_not_consume_the_completion_receipt_identity() {
        let owner = SharedAgentInteractionOwner::default();
        let mut question_args = args(true);
        question_args.async_original_tool_call_id = "auto-answer-call".into();
        let (id, _) = owner.begin_ask_question(
            fence(),
            "runtime-auto-answer",
            question_args.clone(),
            ProductionTurnPrivacyMode::NoTraining,
        ).unwrap();
        let response = InteractionResponse {
            id,
            result: Some(interaction_response::Result::AskQuestionInteractionResponse(
                AskQuestionInteractionResponse {
                    result: Some(AskQuestionResult {
                        result: Some(ask_question_result::Result::Rejected(AskQuestionRejected {
                            reason: fabushi_constants::ask_question::default_ask_question_auto_answer_reason(),
                        })),
                    }),
                },
            )),
        };
        assert!(owner.settle_response(&fence(), &response.encode_to_vec()).unwrap());
        owner
            .turn_ended(
                &fence(),
                TurnEndedUpdate::default(),
                &assistant_window(&[("AskQuestion", "runtime-auto-answer", Some("auto-answer-call"))]),
            )
            .unwrap();
        assert_eq!(owner.drain_post_turn(&fence()).unwrap().len(), 1);
        assert!(owner.begin_ask_question(
            fence(),
            "runtime-auto-answer-retry",
            question_args,
            ProductionTurnPrivacyMode::NoTraining,
        ).is_ok());
    }

    #[test]
    fn async_completion_rejects_async_placeholder_without_consuming_pending_query() {
        let owner = SharedAgentInteractionOwner::default();
        let (id, _) = owner.begin_ask_question(
            fence(),
            "async-placeholder",
            args(true),
            ProductionTurnPrivacyMode::NoTraining,
        ).unwrap();
        let placeholder = InteractionResponse {
            id,
            result: Some(interaction_response::Result::AskQuestionInteractionResponse(
                AskQuestionInteractionResponse {
                    result: Some(AskQuestionResult {
                        result: Some(ask_question_result::Result::Async(AskQuestionAsync {})),
                    }),
                },
            )),
        };
        assert!(owner.settle_response(&fence(), &placeholder.encode_to_vec())
            .unwrap_err().contains("must contain success, error, or rejected"));
        let valid = InteractionResponse {
            id,
            result: Some(interaction_response::Result::AskQuestionInteractionResponse(
                AskQuestionInteractionResponse {
                    result: Some(AskQuestionResult {
                        result: Some(ask_question_result::Result::Success(AskQuestionSuccess {
                            answers: vec![],
                        })),
                    }),
                },
            )),
        };
        assert!(owner.settle_response(&fence(), &valid.encode_to_vec()).unwrap());
    }

    #[test]
    fn stale_account_and_process_epoch_responses_fail_closed() {
        let owner = SharedAgentInteractionOwner::default();
        let (id, _) = owner.begin_ask_question(
            fence(),
            "tool-stale",
            args(false),
            ProductionTurnPrivacyMode::NoTraining,
        ).unwrap();
        let response = InteractionResponse {
            id,
            result: Some(interaction_response::Result::AskQuestionInteractionResponse(
                AskQuestionInteractionResponse {
                    result: Some(AskQuestionResult {
                        result: Some(ask_question_result::Result::Rejected(AskQuestionRejected {
                            reason: "stale".into(),
                        })),
                    }),
                },
            )),
        };
        let stale = InteractionFence { account_fence: "account-b".into(), process_epoch: 8 };
        assert!(owner.settle_response(&stale, &response.encode_to_vec()).unwrap_err().contains("stale"));
        assert_eq!(owner.invalidate_stale(&stale).unwrap(), 1);
    }
    #[test]
    fn completed_receipts_follow_the_live_window_while_pending_receipts_are_preserved() {
        let owner = SharedAgentInteractionOwner::default();
        let mut pending_args = args(true);
        pending_args.async_original_tool_call_id = "pending-original".into();
        owner
            .begin_ask_question(
                fence(),
                "pending-runtime",
                pending_args.clone(),
                ProductionTurnPrivacyMode::NoTraining,
            )
            .unwrap();

        let mut completed_args = args(true);
        completed_args.async_original_tool_call_id = "completed-original".into();
        let (query_id, _) = owner
            .begin_ask_question(
                fence(),
                "completed-runtime",
                completed_args.clone(),
                ProductionTurnPrivacyMode::NoTraining,
            )
            .unwrap();
        settle_success(&owner, &fence(), query_id);

        let window = assistant_window(&[("AskQuestion", "live-runtime", Some("live-original"))]);
        assert_eq!(
            owner
                .prune_completed_async_receipts_from_transcript_window(&fence(), &window)
                .unwrap(),
            1
        );
        assert!(owner
            .begin_ask_question(
                fence(),
                "completed-retry",
                completed_args,
                ProductionTurnPrivacyMode::NoTraining,
            )
            .is_ok());
        assert!(owner
            .begin_ask_question(
                fence(),
                "pending-retry",
                pending_args,
                ProductionTurnPrivacyMode::NoTraining,
            )
            .unwrap_err()
            .contains("duplicate async AskQuestion"));
    }

    #[test]
    fn concurrent_live_window_keeps_each_receipt_and_prunes_only_the_old_call() {
        let owner = SharedAgentInteractionOwner::default();
        let mut args_a = args(true);
        args_a.async_original_tool_call_id = "original-a".into();
        let (id_a, _) = owner
            .begin_ask_question(
                fence(),
                "runtime-a",
                args_a.clone(),
                ProductionTurnPrivacyMode::NoTraining,
            )
            .unwrap();
        settle_success(&owner, &fence(), id_a);

        let mut args_b = args(true);
        args_b.async_original_tool_call_id = "original-b".into();
        let (id_b, _) = owner
            .begin_ask_question(
                fence(),
                "runtime-b",
                args_b.clone(),
                ProductionTurnPrivacyMode::NoTraining,
            )
            .unwrap();
        settle_success(&owner, &fence(), id_b);

        let both_live = assistant_window(&[
            ("AskQuestion", "runtime-a", Some("original-a")),
            ("MultipleChoice", "runtime-b", Some("original-b")),
        ]);
        assert_eq!(
            owner
                .prune_completed_async_receipts_from_transcript_window(&fence(), &both_live)
                .unwrap(),
            0
        );
        assert!(owner
            .begin_ask_question(
                fence(),
                "runtime-a-retry",
                args_a.clone(),
                ProductionTurnPrivacyMode::NoTraining,
            )
            .unwrap_err()
            .contains("duplicate async AskQuestion"));
        assert!(owner
            .begin_ask_question(
                fence(),
                "runtime-b-retry",
                args_b.clone(),
                ProductionTurnPrivacyMode::NoTraining,
            )
            .unwrap_err()
            .contains("duplicate async AskQuestion"));

        let only_b_live = assistant_window(&[("MultipleChoice", "runtime-b", Some("original-b"))]);
        assert_eq!(
            owner
                .prune_completed_async_receipts_from_transcript_window(&fence(), &only_b_live)
                .unwrap(),
            1
        );
        assert!(owner
            .begin_ask_question(
                fence(),
                "runtime-a-retry",
                args_a,
                ProductionTurnPrivacyMode::NoTraining,
            )
            .is_ok());
        assert!(owner
            .begin_ask_question(
                fence(),
                "runtime-b-retry",
                args_b,
                ProductionTurnPrivacyMode::NoTraining,
            )
            .unwrap_err()
            .contains("duplicate async AskQuestion"));
    }

    #[test]
    fn completed_receipt_deduplication_is_account_and_process_fenced() {
        let owner = SharedAgentInteractionOwner::default();
        let (query_id, _) = owner
            .begin_ask_question(
                fence(),
                "same-original",
                args(true),
                ProductionTurnPrivacyMode::NoTraining,
            )
            .unwrap();
        settle_success(&owner, &fence(), query_id);

        let other_account = InteractionFence {
            account_fence: "account-b".into(),
            process_epoch: 7,
        };
        assert!(owner
            .begin_ask_question(
                other_account,
                "same-original",
                args(true),
                ProductionTurnPrivacyMode::NoTraining,
            )
            .is_ok());
        let new_process = InteractionFence {
            account_fence: "account-a".into(),
            process_epoch: 8,
        };
        assert!(owner
            .begin_ask_question(
                new_process,
                "same-original",
                args(true),
                ProductionTurnPrivacyMode::NoTraining,
            )
            .is_ok());
        assert!(owner
            .begin_ask_question(
                fence(),
                "same-original-retry",
                args(true),
                ProductionTurnPrivacyMode::NoTraining,
            )
            .unwrap_err()
            .contains("duplicate async AskQuestion"));
    }


    struct EmptyDelegate;

    impl AndroidRoutedToolBridge for EmptyDelegate {
        fn list_tools(&self) -> Result<Vec<Value>, String> {
            Ok(vec![])
        }

        fn call_tool(&self, _name: &str, _args: Value, _tool_call_id: &str) -> Result<Value, String> {
            Err("unexpected delegated tool call".into())
        }
    }

    #[test]
    fn live_tool_window_keeps_completed_receipt_until_duplicate_is_rejected() {
        let owner = SharedAgentInteractionOwner::default();
        let mut completed_args = args(true);
        completed_args.async_original_tool_call_id = "original-live".into();
        let (query_id, _) = owner
            .begin_ask_question(
                fence(),
                "original-live",
                completed_args,
                ProductionTurnPrivacyMode::NoTraining,
            )
            .unwrap();
        settle_success(&owner, &fence(), query_id);

        let published = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let published_clone = Arc::clone(&published);
        let bridge = with_ask_question_tool(
            Arc::new(EmptyDelegate),
            owner,
            fence(),
            ProductionTurnPrivacyMode::NoTraining,
            Arc::new(AtomicBool::new(false)),
            Arc::new(move |_query_id, _wire, _fence| {
                published_clone.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
            Arc::new(|| Ok(vec![])),
        );
        let error = bridge
            .call_tool(
                ASK_QUESTION_TOOL_NAME,
                serde_json::json!({
                    "questions":[{
                        "id":"q1",
                        "prompt":"Choose",
                        "options":[{"id":"a","label":"A"},{"id":"b","label":"B"}]
                    }],
                    "run_async":true,
                    "async_original_tool_call_id":"original-live"
                }),
                "runtime-retry",
            )
            .unwrap_err();
        assert!(error.contains("duplicate async AskQuestion"));
        assert_eq!(published.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn transcript_window_failure_preserves_completed_receipt_and_never_publishes_retry() {
        let owner = SharedAgentInteractionOwner::default();
        let mut completed_args = args(true);
        completed_args.async_original_tool_call_id = "failed-transcript-window".into();
        let (query_id, _) = owner
            .begin_ask_question(
                fence(),
                "failed-transcript-window",
                completed_args.clone(),
                ProductionTurnPrivacyMode::NoTraining,
            )
            .unwrap();
        settle_success(&owner, &fence(), query_id);

        let published = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let published_clone = Arc::clone(&published);
        let bridge = with_ask_question_tool(
            Arc::new(EmptyDelegate),
            owner.clone(),
            fence(),
            ProductionTurnPrivacyMode::NoTraining,
            Arc::new(AtomicBool::new(false)),
            Arc::new(move |_query_id, _wire, _fence| {
                published_clone.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
            Arc::new(|| Err("transcript update failed".into())),
        );
        let error = bridge
            .call_tool(
                ASK_QUESTION_TOOL_NAME,
                serde_json::json!({
                    "questions":[{
                        "id":"q1","prompt":"Choose",
                        "options":[{"id":"a","label":"A"},{"id":"b","label":"B"}]
                    }],
                    "run_async":true,
                    "async_original_tool_call_id":"failed-transcript-window"
                }),
                "runtime-retry",
            )
            .unwrap_err();
        assert!(error.contains("transcript update failed"));
        assert_eq!(published.load(Ordering::SeqCst), 0);
        assert!(owner
            .begin_ask_question(
                fence(),
                "retry-after-failed-transcript-window",
                completed_args,
                ProductionTurnPrivacyMode::NoTraining,
            )
            .unwrap_err()
            .contains("duplicate async AskQuestion"));
    }


}
