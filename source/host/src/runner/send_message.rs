use super::AndroidRoutedToolBridge;
use crate::connectors::channel_delivery::HostChannelDelivery;
use crate::extensions::transcript::background_wakes::{build_channel_outbound_message, humanize_channel_delivery_failure};
use crate::extensions::transcript::TranscriptStore;
use crate::runner::ProductionTurnLifecycleStore;
use serde_json::{json, Map, Value};
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::{SystemTime, UNIX_EPOCH};

pub const SEND_MESSAGE_TOOL_NAME: &str = "SendMessage";
pub const AWAITING_USER_BLOCKED: &str =
    "This turn is already waiting on the user, so this message was not delivered. Wait for the user's response before sending another user-visible message.";

pub type ChannelDeliveryFailureSink =
    Arc<dyn Fn(Value) -> Result<(), String> + Send + Sync>;

pub struct SendMessageRoutedTools {
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    transcript: Arc<Mutex<TranscriptStore>>,
    lifecycle: Arc<Mutex<ProductionTurnLifecycleStore>>,
    events: Arc<Mutex<VecDeque<Value>>>,
    live_account_fence: Arc<Mutex<Option<String>>>,
    account_fence: String,
    agent_id: String,
    delivery_agent_id: String,
    request_id: String,
    operation_id: String,
    turn_generation: u64,
    process_epoch: u64,
    cancelled: Arc<AtomicBool>,
    committed_send_count: Arc<AtomicUsize>,
    channel_delivery: Option<HostChannelDelivery>,
    channel_failure_sink: Option<ChannelDeliveryFailureSink>,
}

impl SendMessageRoutedTools {
    #[allow(clippy::too_many_arguments)]
    fn new(
        delegate: Arc<dyn AndroidRoutedToolBridge>,
        transcript: Arc<Mutex<TranscriptStore>>,
        lifecycle: Arc<Mutex<ProductionTurnLifecycleStore>>,
        events: Arc<Mutex<VecDeque<Value>>>,
        live_account_fence: Arc<Mutex<Option<String>>>,
        account_fence: &str,
        agent_id: &str,
        delivery_agent_id: &str,
        request_id: &str,
        operation_id: &str,
        turn_generation: u64,
        process_epoch: u64,
        cancelled: Arc<AtomicBool>,
        committed_send_count: Arc<AtomicUsize>,
        channel_delivery: Option<HostChannelDelivery>,
        channel_failure_sink: Option<ChannelDeliveryFailureSink>,
    ) -> Self {
        Self {
            delegate,
            transcript,
            lifecycle,
            events,
            live_account_fence,
            account_fence: account_fence.to_string(),
            agent_id: agent_id.to_string(),
            delivery_agent_id: delivery_agent_id.to_string(),
            request_id: request_id.to_string(),
            operation_id: operation_id.to_string(),
            turn_generation,
            process_epoch,
            cancelled,
            committed_send_count,
            channel_delivery,
            channel_failure_sink,
        }
    }

    fn assert_live(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err("SendMessage was cancelled before side effects".into());
        }
        let live = self
            .live_account_fence
            .lock()
            .map_err(|_| "SendMessage account fence lock poisoned".to_string())?;
        if live.as_deref() != Some(self.account_fence.as_str()) {
            return Err("SendMessage belongs to a stale account epoch".into());
        }
        Ok(())
    }

    fn is_awaiting_user(&self) -> Result<bool, String> {
        Ok(self
            .lifecycle
            .lock()
            .map_err(|_| "turn lifecycle lock poisoned".to_string())?
            .awaiting_user_projection(&self.account_fence, &self.agent_id)
            .is_some())
    }

    fn send(&self, args: Value, tool_call_id: &str) -> Result<Value, String> {
        self.assert_live()?;
        if self.is_awaiting_user()? {
            return Err(AWAITING_USER_BLOCKED.into());
        }
        let tool_call_id = required(tool_call_id, "SendMessage tool_call_id")?;
        let message = normalize_message(&args)?;
        let message_type = message
            .get("type")
            .and_then(Value::as_str)
            .ok_or("SendMessage type is required")?;
        if let Some(channel) = message
            .get("channel")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return self.send_to_channel(channel, &message, tool_call_id);
        }

        let entry_id = format!("send-message:{}:{}", self.operation_id, tool_call_id);
        let entry = json!({
            "id":entry_id,
            "kind":"send-message",
            "role":"assistant",
            "agentId":self.delivery_agent_id,
            "actorAgentId":self.agent_id,
            "operationId":self.operation_id,
            "toolCallId":tool_call_id,
            "message":message,
            "timestampMs":now_ms(),
        });

        {
            let mut transcript = self
                .transcript
                .lock()
                .map_err(|_| "transcript lock poisoned".to_string())?;
            if let Some(existing) = transcript.entry(&entry_id) {
                if existing.get("message") != entry.get("message")
                    || existing.get("agentId") != entry.get("agentId")
                    || existing.get("actorAgentId") != entry.get("actorAgentId")
                    || existing.get("operationId") != entry.get("operationId")
                {
                    return Err(
                        "SendMessage tool_call_id was already committed with different arguments"
                            .into(),
                    );
                }
                return Ok(json!({
                    "sent":true,
                    "messageId":entry_id,
                    "deduped":true,
                }));
            }
            transcript
                .append_entry_if_absent(entry.clone())
                .map_err(|error| format!("failed to persist SendMessage transcript entry: {error}"))?;
        }

        if message_type == "widget" {
            let marked = self
                .lifecycle
                .lock()
                .map_err(|_| "turn lifecycle lock poisoned".to_string())?
                .mark_awaiting_user(
                    &self.account_fence,
                    &self.agent_id,
                    &self.request_id,
                    &self.operation_id,
                    self.turn_generation,
                    self.process_epoch,
                    "send-message-widget",
                    now_ms(),
                );
            if let Err(error) = marked {
                let _ = self
                    .transcript
                    .lock()
                    .map_err(|_| "transcript lock poisoned".to_string())?
                    .remove_entry(&entry_id);
                return Err(format!(
                    "SendMessage widget failed to establish durable awaiting-user state: {error}"
                ));
            }
        }

        // Count only a newly committed canonical SendMessage. Idempotent replays return
        // before this point, while widget state failures roll the transcript entry back.
        // A cancellation/account switch racing after commit is still a real committed
        // side effect and must remain visible to revival reconciliation.
        self.committed_send_count.fetch_add(1, Ordering::AcqRel);
        self.assert_post_commit(tool_call_id)?;
        self.events
            .lock()
            .map_err(|_| "turn event queue lock poisoned".to_string())?
            .push_back(json!({
                "type":"chat.message",
                "operationId":self.operation_id,
                "requestId":self.request_id,
                "role":"assistant",
                "agentId":self.delivery_agent_id,
                "actorAgentId":self.agent_id,
                "messageId":entry_id,
                "message":entry["message"].clone(),
                "toolCallId":tool_call_id,
            }));
        Ok(json!({
            "sent":true,
            "messageId":entry_id,
            "deduped":false,
        }))
    }

    fn send_to_channel(
        &self,
        channel: &str,
        message: &Value,
        tool_call_id: &str,
    ) -> Result<Value, String> {
        let delivery = self.channel_delivery.as_ref().ok_or(
            "Channel delivery is not registered on this Android build; no side effect was attempted.",
        )?;
        let outbound = build_channel_outbound_message(message)
            .ok_or("SendMessage channel payload is not deliverable")?;
        let attempt_id = format!("channel-send-attempt:{}:{}", self.operation_id, tool_call_id);
        let success_id = format!("channel-send-success:{}:{}", self.operation_id, tool_call_id);
        let failure_id = format!("channel-send-failure:{}:{}", self.operation_id, tool_call_id);
        let transcript = self
            .transcript
            .lock()
            .map_err(|_| "transcript lock poisoned".to_string())?;
        if transcript.entry(&success_id).is_some() {
            return Ok(json!({"sent":true,"channel":channel,"deduped":true}));
        }
        if let Some(failure) = transcript.entry(&failure_id) {
            return Err(failure
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("channel delivery previously failed")
                .to_string());
        }
        if transcript.entry(&attempt_id).is_some() {
            return Err(
                "Channel delivery outcome is unknown after process recovery; the Host will not blindly retry this side effect."
                    .into(),
            );
        }
        drop(transcript);

        self.transcript
            .lock()
            .map_err(|_| "transcript lock poisoned".to_string())?
            .append_entry_if_absent(json!({
                "id":attempt_id,
                "kind":"channel-send-attempt",
                "agentId":self.agent_id,
                "operationId":self.operation_id,
                "toolCallId":tool_call_id,
                "channel":channel,
                "message":message,
                "timestampMs":now_ms(),
            }))
            .map_err(|error| format!("failed to persist channel delivery attempt: {error}"))?;

        self.assert_live()?;
        match delivery.deliver(&self.agent_id, channel, &outbound) {
            Ok(()) => {
                self.transcript
                    .lock()
                    .map_err(|_| "transcript lock poisoned".to_string())?
                    .append_entry_if_absent(json!({
                        "id":success_id,
                        "kind":"channel-send-success",
                        "agentId":self.agent_id,
                        "operationId":self.operation_id,
                        "toolCallId":tool_call_id,
                        "channel":channel,
                        "timestampMs":now_ms(),
                    }))
                    .map_err(|error| format!("failed to persist channel delivery settlement: {error}"))?;
                self.committed_send_count.fetch_add(1, Ordering::AcqRel);
                self.assert_post_commit(tool_call_id)?;
                Ok(json!({"sent":true,"channel":channel,"deduped":false}))
            }
            Err(error) => {
                let reason = humanize_channel_delivery_failure(channel, &error);
                let failure = json!({
                    "id":failure_id,
                    "kind":"channel-send-failure",
                    "agentId":self.agent_id,
                    "operationId":self.operation_id,
                    "toolCallId":tool_call_id,
                    "channel":channel,
                    "reason":reason,
                    "rawReason":error,
                    "timestampMs":now_ms(),
                });
                self.transcript
                    .lock()
                    .map_err(|_| "transcript lock poisoned".to_string())?
                    .append_entry_if_absent(failure.clone())
                    .map_err(|persist| format!("failed to persist channel delivery failure: {persist}"))?;
                if let Some(sink) = self.channel_failure_sink.as_ref() {
                    sink(failure.clone())?;
                }
                self.events
                    .lock()
                    .map_err(|_| "turn event queue lock poisoned".to_string())?
                    .push_back(json!({
                        "type":"channel.delivery.failed",
                        "agentId":self.agent_id,
                        "channel":channel,
                        "operationId":self.operation_id,
                        "toolCallId":tool_call_id,
                        "reason":failure["reason"].clone(),
                    }));
                Err(failure["reason"].as_str().unwrap_or("channel delivery failed").to_string())
            }
        }
    }

    fn assert_post_commit(&self, tool_call_id: &str) -> Result<(), String> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(format!(
                "SendMessage committed while cancellation raced completion; outcome-unknown must be reconciled using tool_call_id {tool_call_id}"
            ));
        }
        let live = self
            .live_account_fence
            .lock()
            .map_err(|_| "SendMessage account fence lock poisoned".to_string())?;
        if live.as_deref() != Some(self.account_fence.as_str()) {
            return Err(format!(
                "SendMessage committed under a previous account epoch; outcome-unknown must be reconciled using tool_call_id {tool_call_id}"
            ));
        }
        Ok(())
    }
}

impl AndroidRoutedToolBridge for SendMessageRoutedTools {
    fn list_tools(&self) -> Result<Vec<Value>, String> {
        let mut tools = self.delegate.list_tools()?;
        tools.retain(|tool| {
            tool.get("name").and_then(Value::as_str) != Some(SEND_MESSAGE_TOOL_NAME)
        });
        tools.insert(0, send_message_definition());
        Ok(tools)
    }

    fn call_tool(&self, name: &str, args: Value, tool_call_id: &str) -> Result<Value, String> {
        if name == SEND_MESSAGE_TOOL_NAME {
            self.send(args, tool_call_id)
        } else {
            self.delegate.call_tool(name, args, tool_call_id)
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn with_send_message_tools(
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    transcript: Arc<Mutex<TranscriptStore>>,
    lifecycle: Arc<Mutex<ProductionTurnLifecycleStore>>,
    events: Arc<Mutex<VecDeque<Value>>>,
    live_account_fence: Arc<Mutex<Option<String>>>,
    account_fence: &str,
    agent_id: &str,
    request_id: &str,
    operation_id: &str,
    turn_generation: u64,
    process_epoch: u64,
    cancelled: Arc<AtomicBool>,
) -> Arc<dyn AndroidRoutedToolBridge> {
    with_send_message_tools_counted(
        delegate,
        transcript,
        lifecycle,
        events,
        live_account_fence,
        account_fence,
        agent_id,
        request_id,
        operation_id,
        turn_generation,
        process_epoch,
        cancelled,
        Arc::new(AtomicUsize::new(0)),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn with_send_message_tools_counted(
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    transcript: Arc<Mutex<TranscriptStore>>,
    lifecycle: Arc<Mutex<ProductionTurnLifecycleStore>>,
    events: Arc<Mutex<VecDeque<Value>>>,
    live_account_fence: Arc<Mutex<Option<String>>>,
    account_fence: &str,
    agent_id: &str,
    request_id: &str,
    operation_id: &str,
    turn_generation: u64,
    process_epoch: u64,
    cancelled: Arc<AtomicBool>,
    committed_send_count: Arc<AtomicUsize>,
) -> Arc<dyn AndroidRoutedToolBridge> {
    with_send_message_tools_counted_to_target(
        delegate,
        transcript,
        lifecycle,
        events,
        live_account_fence,
        account_fence,
        agent_id,
        agent_id,
        request_id,
        operation_id,
        turn_generation,
        process_epoch,
        cancelled,
        committed_send_count,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn with_send_message_tools_counted_to_target(
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    transcript: Arc<Mutex<TranscriptStore>>,
    lifecycle: Arc<Mutex<ProductionTurnLifecycleStore>>,
    events: Arc<Mutex<VecDeque<Value>>>,
    live_account_fence: Arc<Mutex<Option<String>>>,
    account_fence: &str,
    actor_agent_id: &str,
    delivery_agent_id: &str,
    request_id: &str,
    operation_id: &str,
    turn_generation: u64,
    process_epoch: u64,
    cancelled: Arc<AtomicBool>,
    committed_send_count: Arc<AtomicUsize>,
) -> Arc<dyn AndroidRoutedToolBridge> {
    Arc::new(SendMessageRoutedTools::new(
        delegate,
        transcript,
        lifecycle,
        events,
        live_account_fence,
        account_fence,
        actor_agent_id,
        delivery_agent_id,
        request_id,
        operation_id,
        turn_generation,
        process_epoch,
        cancelled,
        committed_send_count,
        None,
        None,
    ))
}

#[allow(clippy::too_many_arguments)]
pub fn with_send_message_tools_counted_to_target_with_channel_delivery(
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    transcript: Arc<Mutex<TranscriptStore>>,
    lifecycle: Arc<Mutex<ProductionTurnLifecycleStore>>,
    events: Arc<Mutex<VecDeque<Value>>>,
    live_account_fence: Arc<Mutex<Option<String>>>,
    account_fence: &str,
    actor_agent_id: &str,
    delivery_agent_id: &str,
    request_id: &str,
    operation_id: &str,
    turn_generation: u64,
    process_epoch: u64,
    cancelled: Arc<AtomicBool>,
    committed_send_count: Arc<AtomicUsize>,
    channel_delivery: HostChannelDelivery,
    channel_failure_sink: ChannelDeliveryFailureSink,
) -> Arc<dyn AndroidRoutedToolBridge> {
    Arc::new(SendMessageRoutedTools::new(
        delegate,
        transcript,
        lifecycle,
        events,
        live_account_fence,
        account_fence,
        actor_agent_id,
        delivery_agent_id,
        request_id,
        operation_id,
        turn_generation,
        process_epoch,
        cancelled,
        committed_send_count,
        Some(channel_delivery),
        Some(channel_failure_sink),
    ))
}

fn send_message_definition() -> Value {
    let secret_schema = json!({
        "type":"object",
        "required":["label","connector","field"],
        "additionalProperties":false,
        "properties":{
            "label":{"type":"string","minLength":1},
            "description":{"type":"string"},
            "connector":{"type":"string","minLength":1,"maxLength":64},
            "field":{"type":"string","minLength":1,"maxLength":64}
        }
    });
    let widget_schema = json!({
        "type":"object",
        "required":["prompt","options"],
        "additionalProperties":false,
        "properties":{
            "prompt":{"type":"string","minLength":1},
            "helpText":{"type":"string","minLength":1},
            "options":{
                "type":"array",
                "minItems":1,
                "maxItems":6,
                "items":{
                    "type":"object",
                    "required":["label"],
                    "additionalProperties":false,
                    "properties":{
                        "label":{"type":"string","minLength":1},
                        "value":{"type":"string","minLength":1},
                        "description":{"type":"string","minLength":1},
                        "style":{"type":"string","enum":["default","primary","danger"]}
                    }
                }
            },
            "allowCustom":{"type":"boolean"},
            "dismissOnMoveOn":{"type":"boolean"}
        }
    });
    let images_schema = json!({
        "type":"array",
        "items":{
            "type":"object",
            "required":["url"],
            "additionalProperties":false,
            "properties":{
                "url":{"type":"string"},
                "alt":{"type":"string"}
            }
        }
    });
    let mut properties = Map::new();
    properties.insert("type".into(), json!({"type":"string","enum":["text","attachment","widget","cursor-agent","secret-request"]}));
    properties.insert("content".into(), json!({"type":"string"}));
    properties.insert("url".into(), json!({"type":"string"}));
    properties.insert("images".into(), images_schema);
    properties.insert("alt".into(), json!({"type":"string"}));
    properties.insert("reply_to".into(), json!({"type":"string"}));
    properties.insert("channel".into(), json!({"type":"string"}));
    properties.insert("bcId".into(), json!({"type":"string","minLength":1,"maxLength":240}));
    properties.insert("secret".into(), secret_schema);
    properties.insert("widget".into(), widget_schema);
    json!({
        "type":"function",
        "name":SEND_MESSAGE_TOOL_NAME,
        "description":"Send a user-visible in-app message. Android supports text, attachment, widget, cursor-agent, and secure secret-request messages. Channel provider delivery is fail-closed until a real provider is registered.",
        "parameters":{
            "type":"object",
            "required":["type"],
            "additionalProperties":false,
            "properties":Value::Object(properties)
        }
    })
}

fn normalize_message(args: &Value) -> Result<Value, String> {
    let object = args
        .as_object()
        .ok_or("SendMessage arguments must be an object")?;
    reject_unknown(
        object,
        &["type", "content", "url", "images", "alt", "reply_to", "channel", "widget", "bcId", "secret"],
    )?;
    let message_type = required_field(object, "type")?;
    match message_type {
        "text" => normalize_text(object),
        "attachment" => normalize_attachment(object),
        "widget" => normalize_widget_message(object),
        "cursor-agent" => normalize_cursor_agent(object),
        "secret-request" => normalize_secret_request(object),
        _ => Err("SendMessage type must be text, attachment, widget, cursor-agent, or secret-request on Android".into()),
    }
}

fn normalize_text(object: &Map<String, Value>) -> Result<Value, String> {
    reject_cross_type(object, &["url", "alt", "widget", "bcId", "secret"])?;
    let content = required_field(object, "content")?;
    let mut message = json!({"type":"text","content":content});
    if let Some(reply) = optional_string(object.get("reply_to"))? {
        message["reply_to"] = Value::String(reply);
    }
    if let Some(channel) = optional_string(object.get("channel"))? {
        message["channel"] = Value::String(channel);
    }
    if let Some(images) = object.get("images") {
        let rows = images.as_array().ok_or("images must be an array")?;
        let mut normalized = Vec::with_capacity(rows.len());
        for (index, row) in rows.iter().enumerate() {
            let image = row
                .as_object()
                .ok_or_else(|| format!("images[{index}] must be an object"))?;
            reject_unknown(image, &["url", "alt"])?;
            let url = required_field(image, "url")?;
            validate_attachment_identity(url)
                .map_err(|message| format!("images[{index}].url {message}"))?;
            let mut next = json!({"url":url});
            if let Some(alt) = optional_string(image.get("alt"))? {
                next["alt"] = Value::String(alt);
            }
            normalized.push(next);
        }
        if !normalized.is_empty() {
            message["images"] = Value::Array(normalized);
        }
    }
    Ok(message)
}

fn normalize_attachment(object: &Map<String, Value>) -> Result<Value, String> {
    reject_cross_type(object, &["content", "images", "widget", "bcId", "secret"])?;
    let url = required_field(object, "url")?;
    validate_attachment_identity(url)?;
    let mut message = json!({"type":"attachment","url":url});
    if let Some(alt) = optional_string(object.get("alt"))? {
        message["alt"] = Value::String(alt);
    }
    if let Some(reply) = optional_string(object.get("reply_to"))? {
        message["reply_to"] = Value::String(reply);
    }
    if let Some(channel) = optional_string(object.get("channel"))? {
        message["channel"] = Value::String(channel);
    }
    Ok(message)
}

fn normalize_widget_message(object: &Map<String, Value>) -> Result<Value, String> {
    reject_cross_type(
        object,
        &["content", "url", "images", "alt", "reply_to", "channel", "bcId", "secret"],
    )?;
    let widget = object
        .get("widget")
        .and_then(Value::as_object)
        .ok_or("widget is required when SendMessage type is widget")?;
    reject_unknown(
        widget,
        &["prompt", "helpText", "options", "allowCustom", "dismissOnMoveOn"],
    )?;
    let prompt = required_field(widget, "prompt")?;
    let options = widget
        .get("options")
        .and_then(Value::as_array)
        .ok_or("widget options must be an array")?;
    if !(1..=6).contains(&options.len()) {
        return Err("widget options must contain between 1 and 6 choices".into());
    }
    let mut normalized_options = Vec::with_capacity(options.len());
    for (index, option) in options.iter().enumerate() {
        let option = option
            .as_object()
            .ok_or_else(|| format!("widget options[{index}] must be an object"))?;
        reject_unknown(option, &["label", "value", "description", "style"])?;
        let label = required_field(option, "label")?;
        let mut next = json!({"label":label});
        if let Some(value) = optional_string(option.get("value"))? {
            next["value"] = Value::String(value);
        }
        if let Some(description) = optional_string(option.get("description"))? {
            next["description"] = Value::String(description);
        }
        if let Some(style) = optional_string(option.get("style"))? {
            if !matches!(style.as_str(), "default" | "primary" | "danger") {
                return Err(format!(
                    "widget options[{index}].style must be default, primary, or danger"
                ));
            }
            next["style"] = Value::String(style);
        }
        normalized_options.push(next);
    }
    let mut normalized = json!({
        "prompt":prompt,
        "options":normalized_options,
    });
    if let Some(help) = optional_string(widget.get("helpText"))? {
        normalized["helpText"] = Value::String(help);
    }
    for key in ["allowCustom", "dismissOnMoveOn"] {
        if let Some(value) = widget.get(key) {
            let value = value
                .as_bool()
                .ok_or_else(|| format!("widget {key} must be a boolean"))?;
            normalized[key] = Value::Bool(value);
        }
    }
    Ok(json!({"type":"widget","widget":normalized}))
}

fn normalize_cursor_agent(object: &Map<String, Value>) -> Result<Value, String> {
    reject_cross_type(
        object,
        &["content", "url", "images", "alt", "reply_to", "channel", "widget", "secret"],
    )?;
    let bc_id = required_field(object, "bcId")?.trim();
    if bc_id.is_empty() || bc_id.len() > 240 || bc_id.chars().any(char::is_control) {
        return Err("bcId must be a non-empty canonical cloud-agent id".into());
    }
    Ok(json!({"type":"cursor-agent","bcId":bc_id}))
}

fn normalize_secret_request(object: &Map<String, Value>) -> Result<Value, String> {
    reject_cross_type(
        object,
        &["content", "url", "images", "alt", "reply_to", "channel", "widget", "bcId"],
    )?;
    let secret = object
        .get("secret")
        .and_then(Value::as_object)
        .ok_or("secret is required when SendMessage type is secret-request")?;
    reject_unknown(secret, &["label", "description", "connector", "field"])?;
    let label = required_field(secret, "label")?
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(120)
        .collect::<String>();
    if label.is_empty() {
        return Err("secret label is required".into());
    }
    let connector = required_field(secret, "connector")?;
    let field = required_field(secret, "field")?;
    if connector.len() > 64
        || !connector.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
    {
        return Err("secret connector must be a canonical lowercase platform id".into());
    }
    if field.len() > 64
        || !field.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
        })
    {
        return Err("secret field must be a canonical credential field id".into());
    }
    let mut request = json!({
        "label":label,
        "target":{
            "kind":"channel-credential",
            "platform":connector,
            "field":field,
        }
    });
    if let Some(description) = optional_string(secret.get("description"))? {
        request["description"] = Value::String(description.chars().take(400).collect());
    }
    Ok(json!({"type":"secret-request","secretRequest":request}))
}

fn validate_attachment_identity(url: &str) -> Result<(), String> {
    if let Some(rest) = url.strip_prefix("https://") {
        let host = rest.split('/').next().unwrap_or_default();
        if !host.is_empty()
            && !host.chars().any(|ch| ch.is_whitespace() || ch.is_control())
        {
            return Ok(());
        }
    }
    if let Some(id) = url.strip_prefix("fabushi-attachment://") {
        if !id.is_empty()
            && !id.chars().any(|ch| {
                ch.is_whitespace() || ch.is_control() || matches!(ch, '/' | '?' | '#')
            })
        {
            return Ok(());
        }
    }
    Err("must use https:// or a canonical fabushi-attachment:// identity".into())
}

fn reject_cross_type(object: &Map<String, Value>, forbidden: &[&str]) -> Result<(), String> {
    if let Some(key) = forbidden.iter().find(|key| object.get(**key).is_some()) {
        return Err(format!(
            "{key} is not valid with this SendMessage type; nothing was sent"
        ));
    }
    Ok(())
}

fn reject_unknown(object: &Map<String, Value>, allowed: &[&str]) -> Result<(), String> {
    if let Some(key) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
        Err(format!("SendMessage received unsupported argument {key}"))
    } else {
        Ok(())
    }
}

fn required_field<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a str, String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{key} is required for SendMessage"))
}

fn optional_string(value: Option<&Value>) -> Result<Option<String>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.trim().to_string()).filter(|value| !value.is_empty())),
        Some(_) => Err("optional SendMessage text fields must be strings".into()),
    }
}

fn required<'a>(value: &'a str, label: &str) -> Result<&'a str, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("{label} is required"))
    } else {
        Ok(value)
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::channel_delivery::{ChannelDeliveryPort, ChannelOutboundMessage};


    #[test]
    fn schema_exposes_only_current_android_canonical_message_types() {
        let definition = send_message_definition();
        assert_eq!(
            definition["parameters"]["properties"]["type"]["enum"],
            json!(["text","attachment","widget","cursor-agent","secret-request"])
        );
    }

    #[test]
    fn validation_rejects_local_files_cross_type_fields_and_bad_widgets() {
        assert!(normalize_message(&json!({"type":"attachment","url":"file:///sdcard/secret"})).is_err());
        assert!(normalize_message(&json!({"type":"text","content":"x","url":"https://example.com/a"})).is_err());
        assert!(normalize_message(&json!({"type":"widget","widget":{"prompt":"x","options":[]}})).is_err());
        assert!(normalize_message(&json!({"type":"attachment","url":"fabushi-attachment://a"})).is_ok());
    }

    struct EmptyTools;
    impl AndroidRoutedToolBridge for EmptyTools {
        fn list_tools(&self) -> Result<Vec<Value>, String> {
            Ok(Vec::new())
        }

        fn call_tool(&self, name: &str, _args: Value, _tool_call_id: &str) -> Result<Value, String> {
            Err(format!("unexpected delegated tool: {name}"))
        }
    }

    fn fixture(label: &str) -> (
        Arc<dyn AndroidRoutedToolBridge>,
        Arc<Mutex<TranscriptStore>>,
        Arc<Mutex<ProductionTurnLifecycleStore>>,
        Arc<Mutex<VecDeque<Value>>>,
        Arc<Mutex<Option<String>>>,
        tempfile::TempDir,
    ) {
        let root = tempfile::Builder::new()
            .prefix(&format!("fabushi-send-message-{label}-"))
            .tempdir()
            .unwrap();
        let transcript = Arc::new(Mutex::new(
            TranscriptStore::open(root.path().join("transcript.json")).unwrap(),
        ));
        let lifecycle = Arc::new(Mutex::new(
            ProductionTurnLifecycleStore::open(root.path().join("turn-lifecycle.json"), 1).unwrap(),
        ));
        let process_epoch = lifecycle.lock().unwrap().process_epoch();
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let live = Arc::new(Mutex::new(Some("acct:a".into())));
        let tools = with_send_message_tools(
            Arc::new(EmptyTools),
            Arc::clone(&transcript),
            Arc::clone(&lifecycle),
            Arc::clone(&events),
            Arc::clone(&live),
            "acct:a",
            "agent-a",
            "request-a",
            "operation-a",
            1,
            process_epoch,
            Arc::new(AtomicBool::new(false)),
        );
        (tools, transcript, lifecycle, events, live, root)
    }

    #[test]
    fn committed_send_counter_counts_canonical_side_effect_once() {
        let root = tempfile::Builder::new()
            .prefix("fabushi-send-message-counted-")
            .tempdir()
            .unwrap();
        let transcript = Arc::new(Mutex::new(
            TranscriptStore::open(root.path().join("transcript.json")).unwrap(),
        ));
        let lifecycle = Arc::new(Mutex::new(
            ProductionTurnLifecycleStore::open(root.path().join("turn-lifecycle.json"), 1).unwrap(),
        ));
        let process_epoch = lifecycle.lock().unwrap().process_epoch();
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let live = Arc::new(Mutex::new(Some("acct:a".into())));
        let count = Arc::new(AtomicUsize::new(0));
        let tools = with_send_message_tools_counted(
            Arc::new(EmptyTools),
            Arc::clone(&transcript),
            Arc::clone(&lifecycle),
            events,
            live,
            "acct:a",
            "agent-a",
            "request-a",
            "operation-a",
            1,
            process_epoch,
            Arc::new(AtomicBool::new(false)),
            Arc::clone(&count),
        );
        let args = json!({"type":"text","content":"hello"});
        tools.call_tool(SEND_MESSAGE_TOOL_NAME, args.clone(), "call-1").unwrap();
        tools.call_tool(SEND_MESSAGE_TOOL_NAME, args, "call-1").unwrap();
        assert_eq!(count.load(Ordering::Acquire), 1);
    }

    #[test]
    fn explicit_delivery_target_keeps_actor_lifecycle_separate_from_room_transcript() {
        let root = tempfile::Builder::new()
            .prefix("fabushi-send-message-room-target-")
            .tempdir()
            .unwrap();
        let transcript = Arc::new(Mutex::new(
            TranscriptStore::open(root.path().join("transcript.json")).unwrap(),
        ));
        let lifecycle = Arc::new(Mutex::new(
            ProductionTurnLifecycleStore::open(root.path().join("turn-lifecycle.json"), 1).unwrap(),
        ));
        let process_epoch = lifecycle.lock().unwrap().process_epoch();
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let live = Arc::new(Mutex::new(Some("acct:a".into())));
        let tools = with_send_message_tools_counted_to_target(
            Arc::new(EmptyTools),
            Arc::clone(&transcript),
            Arc::clone(&lifecycle),
            Arc::clone(&events),
            live,
            "acct:a",
            "member-a",
            "group-room-a",
            "request-a",
            "operation-a",
            1,
            process_epoch,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicUsize::new(0)),
        );

        tools
            .call_tool(
                SEND_MESSAGE_TOOL_NAME,
                json!({"type":"text","content":"room-visible"}),
                "call-room",
            )
            .unwrap();

        let entries = transcript.lock().unwrap().get_transcript();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["agentId"], "group-room-a");
        assert_eq!(entries[0]["actorAgentId"], "member-a");
        let event = events.lock().unwrap().front().cloned().unwrap();
        assert_eq!(event["agentId"], "group-room-a");
        assert_eq!(event["actorAgentId"], "member-a");
        assert!(
            lifecycle
                .lock()
                .unwrap()
                .awaiting_user_projection("acct:a", "member-a")
                .is_none()
        );
    }

    #[test]
    fn text_send_is_durable_idempotent_and_emits_one_visible_event() {
        let (tools, transcript, _lifecycle, events, _live, _root) = fixture("text");
        let args = json!({"type":"text","content":"hello"});
        let first = tools.call_tool(SEND_MESSAGE_TOOL_NAME, args.clone(), "call-1").unwrap();
        assert_eq!(first["sent"], true);
        assert_eq!(first["deduped"], false);
        let second = tools.call_tool(SEND_MESSAGE_TOOL_NAME, args, "call-1").unwrap();
        assert_eq!(second["deduped"], true);
        let entries = transcript.lock().unwrap().get_transcript();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["message"]["content"], "hello");
        assert_eq!(events.lock().unwrap().len(), 1);
        assert!(tools
            .call_tool(
                SEND_MESSAGE_TOOL_NAME,
                json!({"type":"text","content":"different"}),
                "call-1",
            )
            .unwrap_err()
            .contains("different arguments"));
    }

    #[test]
    fn widget_establishes_durable_awaiting_user_and_blocks_followup_send() {
        let (tools, transcript, lifecycle, _events, _live, _root) = fixture("widget");
        tools
            .call_tool(
                SEND_MESSAGE_TOOL_NAME,
                json!({
                    "type":"widget",
                    "widget":{
                        "prompt":"Choose",
                        "options":[{"label":"Alpha","value":"a"}],
                        "allowCustom":false
                    }
                }),
                "call-widget",
            )
            .unwrap();
        assert!(lifecycle
            .lock()
            .unwrap()
            .awaiting_user_projection("acct:a", "agent-a")
            .is_some());
        assert_eq!(transcript.lock().unwrap().get_transcript().len(), 1);
        assert_eq!(
            tools
                .call_tool(
                    SEND_MESSAGE_TOOL_NAME,
                    json!({"type":"text","content":"should not send"}),
                    "call-after-widget",
                )
                .unwrap_err(),
            AWAITING_USER_BLOCKED
        );
        assert_eq!(transcript.lock().unwrap().get_transcript().len(), 1);
    }

    #[test]
    fn cursor_agent_reference_is_durable_and_title_fail_soft() {
        let (tools, transcript, _lifecycle, events, _live, _root) = fixture("cursor-agent");
        let result = tools
            .call_tool(
                SEND_MESSAGE_TOOL_NAME,
                json!({"type":"cursor-agent","bcId":"bc-cloud-42"}),
                "cursor-agent-call",
            )
            .unwrap();
        assert_eq!(result["sent"], true);
        let rows = transcript.lock().unwrap().get_transcript();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["message"]["type"], "cursor-agent");
        assert_eq!(rows[0]["message"]["bcId"], "bc-cloud-42");
        assert!(rows[0]["message"].get("title").is_none());
        assert_eq!(events.lock().unwrap().len(), 1);
    }

    #[test]
    fn cursor_agent_validation_fails_before_transcript_side_effect() {
        let (tools, transcript, _lifecycle, _events, _live, _root) = fixture("cursor-invalid");
        assert!(tools
            .call_tool(
                SEND_MESSAGE_TOOL_NAME,
                json!({"type":"cursor-agent","bcId":"  "}),
                "cursor-invalid",
            )
            .is_err());
        assert!(transcript.lock().unwrap().get_transcript().is_empty());
    }

    #[test]
    fn secret_request_is_canonical_durable_and_never_contains_a_secret_value() {
        let (tools, transcript, _lifecycle, events, _live, _root) = fixture("secret-request");
        let result = tools
            .call_tool(
                SEND_MESSAGE_TOOL_NAME,
                json!({
                    "type":"secret-request",
                    "secret":{
                        "label":"  Discord   bot token  ",
                        "description":"  Paste the bot token securely  ",
                        "connector":"discord",
                        "field":"token"
                    }
                }),
                "secret-call",
            )
            .unwrap();
        assert_eq!(result["sent"], true);
        let rows = transcript.lock().unwrap().get_transcript();
        assert_eq!(rows.len(), 1);
        let message = &rows[0]["message"];
        assert_eq!(message["type"], "secret-request");
        assert_eq!(message["secretRequest"]["label"], "Discord bot token");
        assert_eq!(message["secretRequest"]["target"]["kind"], "channel-credential");
        assert_eq!(message["secretRequest"]["target"]["platform"], "discord");
        assert_eq!(message["secretRequest"]["target"]["field"], "token");
        assert!(message.get("value").is_none());
        assert!(message["secretRequest"].get("value").is_none());
        assert_eq!(events.lock().unwrap().len(), 1);
    }

    #[test]
    fn secret_request_validation_fails_before_transcript_side_effect() {
        let (tools, transcript, _lifecycle, _events, _live, _root) = fixture("secret-invalid");
        assert!(tools
            .call_tool(
                SEND_MESSAGE_TOOL_NAME,
                json!({"type":"secret-request","secret":{"label":"Token","connector":"Discord","field":"token"}}),
                "secret-invalid",
            )
            .is_err());
        assert!(transcript.lock().unwrap().get_transcript().is_empty());

        assert!(tools
            .call_tool(
                SEND_MESSAGE_TOOL_NAME,
                json!({
                    "type":"secret-request",
                    "bcId":"must-not-mix",
                    "secret":{"label":"Token","connector":"discord","field":"token"}
                }),
                "secret-mixed",
            )
            .unwrap_err()
            .contains("not valid"));
        assert!(transcript.lock().unwrap().get_transcript().is_empty());
    }

    #[test]
    fn stale_account_fails_before_transcript_side_effect() {
        let (tools, transcript, _lifecycle, _events, live, _root) = fixture("stale");
        *live.lock().unwrap() = Some("acct:b".into());
        assert!(tools
            .call_tool(
                SEND_MESSAGE_TOOL_NAME,
                json!({"type":"text","content":"blocked"}),
                "stale-call",
            )
            .unwrap_err()
            .contains("stale account epoch"));
        assert!(transcript.lock().unwrap().get_transcript().is_empty());
    }
    struct CountingChannelPort {
        calls: Arc<AtomicUsize>,
    }

    impl ChannelDeliveryPort for CountingChannelPort {
        fn deliver(
            &self,
            _agent_id: &str,
            _address_token: &str,
            _message: &ChannelOutboundMessage,
        ) -> Result<(), String> {
            self.calls.fetch_add(1, Ordering::AcqRel);
            Ok(())
        }
    }

    fn channel_tools(
        label: &str,
        delivery: HostChannelDelivery,
        sink: ChannelDeliveryFailureSink,
    ) -> (
        Arc<dyn AndroidRoutedToolBridge>,
        Arc<Mutex<TranscriptStore>>,
        Arc<AtomicUsize>,
        tempfile::TempDir,
    ) {
        let root = tempfile::Builder::new()
            .prefix(&format!("fabushi-send-channel-{label}-"))
            .tempdir()
            .unwrap();
        let transcript = Arc::new(Mutex::new(
            TranscriptStore::open(root.path().join("transcript.json")).unwrap(),
        ));
        let lifecycle = Arc::new(Mutex::new(
            ProductionTurnLifecycleStore::open(root.path().join("turn-lifecycle.json"), 1).unwrap(),
        ));
        let process_epoch = lifecycle.lock().unwrap().process_epoch();
        let committed = Arc::new(AtomicUsize::new(0));
        let tools = with_send_message_tools_counted_to_target_with_channel_delivery(
            Arc::new(EmptyTools),
            Arc::clone(&transcript),
            lifecycle,
            Arc::new(Mutex::new(VecDeque::new())),
            Arc::new(Mutex::new(Some("acct:a".into()))),
            "acct:a",
            "agent-a",
            "agent-a",
            "request-a",
            "operation-a",
            1,
            process_epoch,
            Arc::new(AtomicBool::new(false)),
            Arc::clone(&committed),
            delivery,
            sink,
        );
        (tools, transcript, committed, root)
    }

    #[test]
    fn channel_success_is_prepared_settled_and_deduped_without_second_side_effect() {
        let calls = Arc::new(AtomicUsize::new(0));
        let delivery = HostChannelDelivery::new(Arc::new(CountingChannelPort {
            calls: Arc::clone(&calls),
        }));
        let sink: ChannelDeliveryFailureSink = Arc::new(|_| Ok(()));
        let (tools, transcript, committed, _root) = channel_tools("success", delivery, sink);
        let args = json!({"type":"text","content":"hello","channel":"slack:C1"});
        assert_eq!(tools.call_tool(SEND_MESSAGE_TOOL_NAME, args.clone(), "call-1").unwrap()["sent"], true);
        assert_eq!(tools.call_tool(SEND_MESSAGE_TOOL_NAME, args, "call-1").unwrap()["deduped"], true);
        assert_eq!(calls.load(Ordering::Acquire), 1);
        assert_eq!(committed.load(Ordering::Acquire), 1);
        let rows = transcript.lock().unwrap().get_transcript();
        assert!(rows.iter().any(|row| row["kind"] == "channel-send-attempt"));
        assert!(rows.iter().any(|row| row["kind"] == "channel-send-success"));
    }

    #[test]
    fn channel_failure_is_durable_and_replay_does_not_blindly_retry_or_duplicate_wake() {
        let wake_count = Arc::new(AtomicUsize::new(0));
        let sink_count = Arc::clone(&wake_count);
        let sink: ChannelDeliveryFailureSink = Arc::new(move |_| {
            sink_count.fetch_add(1, Ordering::AcqRel);
            Ok(())
        });
        let (tools, transcript, committed, _root) =
            channel_tools("failure", HostChannelDelivery::default(), sink);
        let args = json!({"type":"text","content":"hello","channel":"slack:C1"});
        let first = tools.call_tool(SEND_MESSAGE_TOOL_NAME, args.clone(), "call-1").unwrap_err();
        let second = tools.call_tool(SEND_MESSAGE_TOOL_NAME, args, "call-1").unwrap_err();
        assert_eq!(first, second);
        assert!(first.contains("Channel messaging isn't available"));
        assert_eq!(wake_count.load(Ordering::Acquire), 1);
        assert_eq!(committed.load(Ordering::Acquire), 0);
        let rows = transcript.lock().unwrap().get_transcript();
        assert!(rows.iter().any(|row| row["kind"] == "channel-send-attempt"));
        assert!(rows.iter().any(|row| row["kind"] == "channel-send-failure"));
    }

    #[test]
    fn prepared_without_settlement_reopens_as_outcome_unknown_without_dispatch() {
        let calls = Arc::new(AtomicUsize::new(0));
        let delivery = HostChannelDelivery::new(Arc::new(CountingChannelPort {
            calls: Arc::clone(&calls),
        }));
        let sink: ChannelDeliveryFailureSink = Arc::new(|_| Ok(()));
        let (tools, transcript, _committed, _root) = channel_tools("unknown", delivery, sink);
        transcript.lock().unwrap().append_entry_if_absent(json!({
            "id":"channel-send-attempt:operation-a:call-1",
            "kind":"channel-send-attempt",
            "agentId":"agent-a",
            "operationId":"operation-a",
            "toolCallId":"call-1",
            "channel":"slack:C1",
            "message":{"type":"text","content":"hello","channel":"slack:C1"},
            "timestampMs":1,
        })).unwrap();
        let error = tools.call_tool(
            SEND_MESSAGE_TOOL_NAME,
            json!({"type":"text","content":"hello","channel":"slack:C1"}),
            "call-1",
        ).unwrap_err();
        assert!(error.contains("outcome is unknown"));
        assert_eq!(calls.load(Ordering::Acquire), 0);
    }

}
