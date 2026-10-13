use super::AndroidRoutedToolBridge;
use crate::extensions::transcript::TranscriptStore;
use crate::runner::ProductionTurnLifecycleStore;
use serde_json::{json, Map, Value};
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{SystemTime, UNIX_EPOCH};

pub const SEND_MESSAGE_TOOL_NAME: &str = "SendMessage";
pub const AWAITING_USER_BLOCKED: &str =
    "This turn is already waiting on the user, so this message was not delivered. Wait for the user's response before sending another user-visible message.";

pub struct SendMessageRoutedTools {
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    transcript: Arc<Mutex<TranscriptStore>>,
    lifecycle: Arc<Mutex<ProductionTurnLifecycleStore>>,
    events: Arc<Mutex<VecDeque<Value>>>,
    live_account_fence: Arc<Mutex<Option<String>>>,
    account_fence: String,
    agent_id: String,
    request_id: String,
    operation_id: String,
    turn_generation: u64,
    process_epoch: u64,
    cancelled: Arc<AtomicBool>,
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
        request_id: &str,
        operation_id: &str,
        turn_generation: u64,
        process_epoch: u64,
        cancelled: Arc<AtomicBool>,
    ) -> Self {
        Self {
            delegate,
            transcript,
            lifecycle,
            events,
            live_account_fence,
            account_fence: account_fence.to_string(),
            agent_id: agent_id.to_string(),
            request_id: request_id.to_string(),
            operation_id: operation_id.to_string(),
            turn_generation,
            process_epoch,
            cancelled,
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
        if let Some(channel) = message.get("channel").and_then(Value::as_str) {
            if !channel.trim().is_empty() {
                return Err(
                    "Channel delivery is not registered on this Android build; the message was not written to the in-app transcript."
                        .into(),
                );
            }
        }

        let entry_id = format!("send-message:{}:{}", self.operation_id, tool_call_id);
        let entry = json!({
            "id":entry_id,
            "kind":"send-message",
            "role":"assistant",
            "agentId":self.agent_id,
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

        self.assert_post_commit(tool_call_id)?;
        self.events
            .lock()
            .map_err(|_| "turn event queue lock poisoned".to_string())?
            .push_back(json!({
                "type":"chat.message",
                "operationId":self.operation_id,
                "requestId":self.request_id,
                "role":"assistant",
                "agentId":self.agent_id,
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
    Arc::new(SendMessageRoutedTools::new(
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
    ))
}

fn send_message_definition() -> Value {
    json!({
        "type":"function",
        "name":SEND_MESSAGE_TOOL_NAME,
        "description":"Send a user-visible in-app message. Android currently supports text, attachment, and selectable widget messages. Channel provider delivery is fail-closed until a real provider is registered.",
        "parameters":{
            "type":"object",
            "required":["type"],
            "additionalProperties":false,
            "properties":{
                "type":{"type":"string","enum":["text","attachment","widget"]},
                "content":{"type":"string"},
                "url":{"type":"string"},
                "images":{"type":"array","items":{"type":"object","required":["url"],"additionalProperties":false,"properties":{"url":{"type":"string"},"alt":{"type":"string"}}}},
                "alt":{"type":"string"},
                "reply_to":{"type":"string"},
                "channel":{"type":"string"},
                "widget":{
                    "type":"object",
                    "required":["prompt","options"],
                    "additionalProperties":false,
                    "properties":{
                        "prompt":{"type":"string","minLength":1},
                        "helpText":{"type":"string","minLength":1},
                        "options":{"type":"array","minItems":1,"maxItems":6,"items":{"type":"object","required":["label"],"additionalProperties":false,"properties":{"label":{"type":"string","minLength":1},"value":{"type":"string","minLength":1},"description":{"type":"string","minLength":1},"style":{"type":"string","enum":["default","primary","danger"]}}}},
                        "allowCustom":{"type":"boolean"},
                        "dismissOnMoveOn":{"type":"boolean"}
                    }
                }
            }
        }
    })
}

fn normalize_message(args: &Value) -> Result<Value, String> {
    let object = args
        .as_object()
        .ok_or("SendMessage arguments must be an object")?;
    reject_unknown(
        object,
        &["type", "content", "url", "images", "alt", "reply_to", "channel", "widget"],
    )?;
    let message_type = required_field(object, "type")?;
    match message_type {
        "text" => normalize_text(object),
        "attachment" => normalize_attachment(object),
        "widget" => normalize_widget_message(object),
        _ => Err("SendMessage type must be text, attachment, or widget on Android".into()),
    }
}

fn normalize_text(object: &Map<String, Value>) -> Result<Value, String> {
    reject_cross_type(object, &["url", "alt", "widget"])?;
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
    reject_cross_type(object, &["content", "images", "widget"])?;
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
        &["content", "url", "images", "alt", "reply_to", "channel"],
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

    #[test]
    fn schema_exposes_only_current_android_canonical_message_types() {
        let definition = send_message_definition();
        assert_eq!(
            definition["parameters"]["properties"]["type"]["enum"],
            json!(["text","attachment","widget"])
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
}
