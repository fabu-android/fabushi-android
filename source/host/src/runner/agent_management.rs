use super::AndroidRoutedToolBridge;
use crate::android_agent_roster::AndroidAgentRoster;
use crate::messaging_service::AndroidMessagingService;
use crate::extensions::transcript::transcript_store::TranscriptStore;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{SystemTime, UNIX_EPOCH};

pub const SEND_TO_AGENT_TOOL_NAME: &str = "SendToAgent";
pub const CREATE_AGENT_TOOL_NAME: &str = "CreateAgent";
pub const UPDATE_AGENT_TOOL_NAME: &str = "UpdateAgent";

const SELF_SEND_REJECTION: &str =
    "You can't message yourself with SendToAgent. Use SendMessage to talk to the user, or pick a different target id.";

#[derive(Clone)]
struct ActiveAgentTurn {
    operation_id: String,
    account_fence: String,
    protect_from_priority_interrupt: bool,
    cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct AgentTurnInterruptionRegistry {
    active: Mutex<BTreeMap<String, ActiveAgentTurn>>,
}

impl AgentTurnInterruptionRegistry {
    pub fn register(
        &self,
        agent_id: &str,
        operation_id: &str,
        account_fence: &str,
        protect_from_priority_interrupt: bool,
        cancelled: Arc<AtomicBool>,
    ) -> Result<(), String> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| "Agent turn interruption registry lock poisoned".to_string())?;
        active.insert(
            agent_id.to_string(),
            ActiveAgentTurn {
                operation_id: operation_id.to_string(),
                account_fence: account_fence.to_string(),
                protect_from_priority_interrupt,
                cancelled,
            },
        );
        Ok(())
    }

    pub fn unregister_operation(&self, operation_id: &str) {
        if let Ok(mut active) = self.active.lock() {
            active.retain(|_, turn| turn.operation_id != operation_id);
        }
    }

    pub fn has_active_turn(&self, agent_id: &str, account_fence: &str) -> bool {
        self.active
            .lock()
            .ok()
            .and_then(|active| active.get(agent_id).cloned())
            .is_some_and(|turn| turn.account_fence == account_fence)
    }

    pub fn interrupt_priority(
        &self,
        agent_id: &str,
        account_fence: &str,
    ) -> Result<Option<String>, String> {
        let active = self
            .active
            .lock()
            .map_err(|_| "Agent turn interruption registry lock poisoned".to_string())?;
        let Some(turn) = active.get(agent_id) else { return Ok(None); };
        if turn.account_fence != account_fence || turn.protect_from_priority_interrupt {
            return Ok(None);
        }
        turn.cancelled.store(true, Ordering::Release);
        Ok(Some(turn.operation_id.clone()))
    }
    pub fn interrupt_for_deletion(
        &self,
        agent_id: &str,
        account_fence: &str,
    ) -> Result<Option<String>, String> {
        let active = self
            .active
            .lock()
            .map_err(|_| "Agent turn interruption registry lock poisoned".to_string())?;
        let Some(turn) = active.get(agent_id) else { return Ok(None); };
        if turn.account_fence != account_fence {
            return Ok(None);
        }
        // Deletion is stronger than Agent-to-Agent priority interruption. A protected user lane
        // may not survive the removal of its canonical Agent owner and write back into a deleted
        // entity, so deletion always cancels the live turn inside the matching account epoch.
        turn.cancelled.store(true, Ordering::Release);
        Ok(Some(turn.operation_id.clone()))
    }

}

pub struct AgentManagementRoutedTools {
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    roster: Arc<Mutex<AndroidAgentRoster>>,
    messaging: Arc<Mutex<AndroidMessagingService>>,
    transcript: Option<Arc<Mutex<TranscriptStore>>>,
    live_account_fence: Arc<Mutex<Option<String>>>,
    account_fence: String,
    self_agent_id: String,
    cancelled: Arc<AtomicBool>,
    turn_interruptions: Arc<AgentTurnInterruptionRegistry>,
}

impl AgentManagementRoutedTools {
    fn new(
        delegate: Arc<dyn AndroidRoutedToolBridge>,
        roster: Arc<Mutex<AndroidAgentRoster>>,
        messaging: Arc<Mutex<AndroidMessagingService>>,
        transcript: Option<Arc<Mutex<TranscriptStore>>>,
        live_account_fence: Arc<Mutex<Option<String>>>,
        account_fence: &str,
        self_agent_id: &str,
        cancelled: Arc<AtomicBool>,
        turn_interruptions: Arc<AgentTurnInterruptionRegistry>,
    ) -> Self {
        Self {
            delegate,
            roster,
            messaging,
            transcript,
            live_account_fence,
            account_fence: account_fence.to_string(),
            self_agent_id: self_agent_id.to_string(),
            cancelled,
            turn_interruptions,
        }
    }

    fn assert_live(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err("Agent management tool call was cancelled before side effects".into());
        }
        let live = self
            .live_account_fence
            .lock()
            .map_err(|_| "Agent management account fence lock poisoned".to_string())?;
        if live.as_deref() != Some(self.account_fence.as_str()) {
            return Err("Agent management tool call belongs to a stale account epoch".into());
        }
        Ok(())
    }

    fn post_commit_outcome(&self, tool: &str) -> Result<(), String> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(format!(
                "{tool} committed a durable side effect while cancellation raced completion; outcome-unknown must be reconciled by the stable tool_call_id"
            ));
        }
        let live = self
            .live_account_fence
            .lock()
            .map_err(|_| "Agent management account fence lock poisoned".to_string())?;
        if live.as_deref() != Some(self.account_fence.as_str()) {
            return Err(format!(
                "{tool} committed under a previous account epoch; outcome-unknown must be reconciled by the stable tool_call_id"
            ));
        }
        Ok(())
    }
}

impl AndroidRoutedToolBridge for AgentManagementRoutedTools {
    fn list_tools(&self) -> Result<Vec<Value>, String> {
        let mut tools = self.delegate.list_tools()?;
        tools.retain(|tool| {
            !matches!(
                tool.get("name").and_then(Value::as_str),
                Some(SEND_TO_AGENT_TOOL_NAME | CREATE_AGENT_TOOL_NAME | UPDATE_AGENT_TOOL_NAME)
            )
        });
        tools.insert(0, update_definition());
        tools.insert(0, create_definition());
        tools.insert(0, send_definition());
        Ok(tools)
    }

    fn call_tool(&self, name: &str, args: Value, tool_call_id: &str) -> Result<Value, String> {
        match name {
            SEND_TO_AGENT_TOOL_NAME => self.send_to_agent(args, tool_call_id),
            CREATE_AGENT_TOOL_NAME => self.create_agent(args, tool_call_id),
            UPDATE_AGENT_TOOL_NAME => self.update_agent(args, tool_call_id),
            _ => self.delegate.call_tool(name, args, tool_call_id),
        }
    }
}

impl AgentManagementRoutedTools {
    fn reconcile_direct_agent_transcript(
        &self,
        target_id: &str,
        target_name: &str,
        sender_name: &str,
        message: &str,
        images: &[Value],
        delivery: &Value,
    ) -> Result<(), String> {
        let Some(transcript) = self.transcript.as_ref() else {
            return Ok(());
        };
        let message_id = delivery
            .get("messageId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or("Agent delivery is missing durable messageId")?;
        let timestamp_ms = delivery
            .get("createdAtMs")
            .and_then(Value::as_i64)
            .ok_or("Agent delivery is missing durable createdAtMs")?;
        let outbound_id = format!("agent-peer-out:{message_id}");
        let inbound_id = format!("agent-peer-in:{message_id}");
        let mut outbound = json!({
            "kind":"message",
            "id":outbound_id,
            "role":"assistant",
            "content":message,
            "isStreaming":false,
            "timestampMs":timestamp_ms,
            "agentId":self.self_agent_id,
            "accountFence":self.account_fence,
            "toAgent":{"id":target_id,"name":target_name,"kind":"agent"},
            "hiddenOutboundAgentPeerMessage":true,
        });
        let mut inbound = json!({
            "kind":"message",
            "id":inbound_id,
            "role":"user",
            "content":message,
            "isStreaming":false,
            "timestampMs":timestamp_ms,
            "agentId":target_id,
            "accountFence":self.account_fence,
            "fromAgent":{"id":self.self_agent_id,"name":sender_name},
        });
        if !images.is_empty() {
            outbound["images"] = Value::Array(images.to_vec());
            inbound["images"] = Value::Array(images.to_vec());
        }
        let mut transcript = transcript
            .lock()
            .map_err(|_| "canonical Android transcript lock poisoned".to_string())?;
        transcript
            .append_entry_if_absent(outbound)
            .map_err(|error| format!("failed to reconcile outbound Agent peer transcript: {error}"))?;
        transcript
            .append_entry_if_absent(inbound)
            .map_err(|error| format!("failed to reconcile inbound Agent peer transcript: {error}"))?;
        Ok(())
    }

    fn send_to_agent(&self, args: Value, tool_call_id: &str) -> Result<Value, String> {
        self.assert_live()?;
        let object = require_object(&args, SEND_TO_AGENT_TOOL_NAME)?;
        reject_unknown(object, &["target_id", "message", "images", "priority"], SEND_TO_AGENT_TOOL_NAME)?;
        let target_id = required_field(object, "target_id", SEND_TO_AGENT_TOOL_NAME)?;
        let message = clamp_agent_message(required_field(object, "message", SEND_TO_AGENT_TOOL_NAME)?);
        if message.is_empty() {
            return Err("message is required for SendToAgent".into());
        }
        if target_id == self.self_agent_id {
            return Ok(Value::String(SELF_SEND_REJECTION.into()));
        }
        let priority = match object.get("priority") {
            None => false,
            Some(Value::Bool(value)) => *value,
            Some(_) => return Err("priority must be a boolean for SendToAgent".into()),
        };
        let images = parse_images(object.get("images"))?;
        let target = self
            .roster
            .lock()
            .map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?
            .get(target_id)
            .ok_or_else(|| format!("No agent or group found with id {target_id}."))?;
        let sender_name = self
            .roster
            .lock()
            .map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?
            .get(&self.self_agent_id)
            .map(|agent| agent.name)
            .unwrap_or_else(|| "An agent".into());
        let delivery = self
            .messaging
            .lock()
            .map_err(|_| "canonical Android messaging owner lock poisoned".to_string())?
            .deliver_agent_message(
                &self.account_fence,
                &self.self_agent_id,
                target_id,
                target.is_group,
                &message,
                &images,
                priority,
                required(tool_call_id, "SendToAgent tool_call_id")?,
                now_ms_i64(),
            )
            .map_err(|error| {
                if error.contains("failed to persist Agent delivery") {
                    format!(
                        "SendToAgent outcome-unknown: durable delivery settlement failed; reconcile with the same tool_call_id before retry: {error}"
                    )
                } else {
                    error
                }
            })?;
        if !target.is_group {
            self.reconcile_direct_agent_transcript(
                target_id,
                &target.name,
                &sender_name,
                &message,
                &images,
                &delivery,
            )
            .map_err(|error| {
                format!(
                    "SendToAgent outcome-unknown: canonical delivery committed but Agent transcript reconciliation is incomplete; retry with the same tool_call_id: {error}"
                )
            })?;
        }
        if priority && !target.is_group {
            let _ = self
                .turn_interruptions
                .interrupt_priority(target_id, &self.account_fence)?;
        }
        self.post_commit_outcome(SEND_TO_AGENT_TOOL_NAME)?;
        let priority_note = if target.is_group && priority {
            " Group delivery ignored priority as required."
        } else if priority {
            " Priority STOP/supersede semantics were applied to this 1:1 delivery."
        } else {
            ""
        };
        Ok(Value::String(format!(
            "Message delivered asynchronously to {} (id: {}). Replies arrive later on a fresh turn.{}",
            target.name, target.id, priority_note
        )))
    }

    fn create_agent(&self, args: Value, tool_call_id: &str) -> Result<Value, String> {
        self.assert_live()?;
        let object = require_object(&args, CREATE_AGENT_TOOL_NAME)?;
        reject_unknown(object, &["name", "description"], CREATE_AGENT_TOOL_NAME)?;
        let name = required_field(object, "name", CREATE_AGENT_TOOL_NAME)?;
        let description = optional_field(object.get("description"))?.unwrap_or_default();
        let created = self
            .roster
            .lock()
            .map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?
            .create_for_tool_call(
                &self.account_fence,
                &self.self_agent_id,
                required(tool_call_id, "CreateAgent tool_call_id")?,
                name,
                description,
            )?;
        self.post_commit_outcome(CREATE_AGENT_TOOL_NAME)?;
        Ok(Value::String(format!(
            "Created agent \"{}\" (id: {}). Message it with SendToAgent using that id.",
            created.name, created.id
        )))
    }

    fn update_agent(&self, args: Value, tool_call_id: &str) -> Result<Value, String> {
        self.assert_live()?;
        let object = require_object(&args, UPDATE_AGENT_TOOL_NAME)?;
        reject_unknown(object, &["agent_id", "name", "description"], UPDATE_AGENT_TOOL_NAME)?;
        let id = required_field(object, "agent_id", UPDATE_AGENT_TOOL_NAME)?;
        let name = optional_field(object.get("name"))?;
        let description = optional_field(object.get("description"))?;
        if name.is_none() && description.is_none() {
            return Ok(Value::String(
                "Nothing to update: provide a new name and/or description.".into(),
            ));
        }
        let updated = self
            .roster
            .lock()
            .map_err(|_| "canonical Android Agent roster lock poisoned".to_string())?
            .update_for_tool_call(
                &self.account_fence,
                &self.self_agent_id,
                required(tool_call_id, "UpdateAgent tool_call_id")?,
                id,
                name,
                description,
            )?;
        self.post_commit_outcome(UPDATE_AGENT_TOOL_NAME)?;
        Ok(Value::String(match updated {
            Some(updated) => format!("Updated agent \"{}\" (id: {}).", updated.name, updated.id),
            None => format!("No agent found with id {id}."),
        }))
    }
}

pub fn with_agent_management_tools(
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    roster: Arc<Mutex<AndroidAgentRoster>>,
    messaging: Arc<Mutex<AndroidMessagingService>>,
    live_account_fence: Arc<Mutex<Option<String>>>,
    account_fence: &str,
    self_agent_id: &str,
    cancelled: Arc<AtomicBool>,
    turn_interruptions: Arc<AgentTurnInterruptionRegistry>,
) -> Arc<dyn AndroidRoutedToolBridge> {
    Arc::new(AgentManagementRoutedTools::new(
        delegate,
        roster,
        messaging,
        None,
        live_account_fence,
        account_fence,
        self_agent_id,
        cancelled,
        turn_interruptions,
    ))
}

pub fn with_agent_management_tools_and_transcript(
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    roster: Arc<Mutex<AndroidAgentRoster>>,
    messaging: Arc<Mutex<AndroidMessagingService>>,
    transcript: Arc<Mutex<TranscriptStore>>,
    live_account_fence: Arc<Mutex<Option<String>>>,
    account_fence: &str,
    self_agent_id: &str,
    cancelled: Arc<AtomicBool>,
    turn_interruptions: Arc<AgentTurnInterruptionRegistry>,
) -> Arc<dyn AndroidRoutedToolBridge> {
    Arc::new(AgentManagementRoutedTools::new(
        delegate,
        roster,
        messaging,
        Some(transcript),
        live_account_fence,
        account_fence,
        self_agent_id,
        cancelled,
        turn_interruptions,
    ))
}

fn send_definition() -> Value {
    json!({
        "type":"function",
        "name":SEND_TO_AGENT_TOOL_NAME,
        "description":"Send a fire-and-forget asynchronous message to another agent or a group by id. Use priority=true only for a 1:1 STOP/supersede instruction; group posts ignore priority. Replies arrive later on a fresh turn.",
        "parameters":{
            "type":"object",
            "required":["target_id","message"],
            "additionalProperties":false,
            "properties":{
                "target_id":{"type":"string","minLength":1},
                "message":{"type":"string","minLength":1},
                "images":{"type":"array","items":{"type":"object","required":["url"],"additionalProperties":false,"properties":{"url":{"type":"string","minLength":1},"alt":{"type":"string"}}}},
                "priority":{"type":"boolean"}
            }
        }
    })
}

fn create_definition() -> Value {
    json!({
        "type":"function",
        "name":CREATE_AGENT_TOOL_NAME,
        "description":"Create a new teammate agent with a name and optional persona/description. Returns its id so it can be messaged with SendToAgent.",
        "parameters":{
            "type":"object","required":["name"],"additionalProperties":false,
            "properties":{"name":{"type":"string","minLength":1},"description":{"type":"string"}}
        }
    })
}

fn update_definition() -> Value {
    json!({
        "type":"function",
        "name":UPDATE_AGENT_TOOL_NAME,
        "description":"Edit another agent's name and/or description. Omitted fields remain unchanged.",
        "parameters":{
            "type":"object","required":["agent_id"],"additionalProperties":false,
            "properties":{"agent_id":{"type":"string","minLength":1},"name":{"type":"string"},"description":{"type":"string"}}
        }
    })
}

fn clamp_agent_message(raw: &str) -> String {
    const MAX_UTF16_UNITS: usize = 8_000;
    let trimmed = raw.trim();
    let mut used = 0usize;
    trimmed
        .chars()
        .take_while(|character| {
            let next = used.saturating_add(character.len_utf16());
            if next > MAX_UTF16_UNITS {
                false
            } else {
                used = next;
                true
            }
        })
        .collect()
}

fn parse_images(value: Option<&Value>) -> Result<Vec<Value>, String> {
    let Some(value) = value else { return Ok(Vec::new()); };
    let rows = value.as_array().ok_or("images must be an array")?;
    let mut images = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        let object = row
            .as_object()
            .ok_or_else(|| format!("images[{index}] must be an object"))?;
        reject_unknown(object, &["url", "alt"], "SendToAgent image")?;
        let url = object
            .get("url")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("images[{index}].url is required"))?;
        validate_image_url(url)
            .map_err(|_| format!("images[{index}].url must use https:// or a canonical fabushi-attachment:// identity"))?;
        let alt = optional_field(object.get("alt"))?;
        images.push(match alt {
            Some(alt) => json!({"url":url,"alt":alt}),
            None => json!({"url":url}),
        });
    }
    Ok(images)
}

fn validate_image_url(url: &str) -> Result<(), ()> {
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
                ch.is_whitespace()
                    || ch.is_control()
                    || matches!(ch, '/' | '?' | '#')
            })
        {
            return Ok(());
        }
    }
    Err(())
}

fn require_object<'a>(args: &'a Value, tool: &str) -> Result<&'a Map<String, Value>, String> {
    args.as_object()
        .ok_or_else(|| format!("{tool} arguments must be an object"))
}

fn reject_unknown(object: &Map<String, Value>, allowed: &[&str], tool: &str) -> Result<(), String> {
    if let Some(key) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(format!("{tool} received unsupported argument {key}"));
    }
    Ok(())
}

fn required_field<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    tool: &str,
) -> Result<&'a str, String> {
    object
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{field} is required for {tool}"))
}

fn optional_field(value: Option<&Value>) -> Result<Option<&str>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.trim()).filter(|value| !value.is_empty())),
        Some(_) => Err("optional Agent management text fields must be strings".into()),
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

fn now_ms_i64() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messaging_service::AndroidMessagingService;
    use std::fs;

    struct EmptyTools;
    impl AndroidRoutedToolBridge for EmptyTools {
        fn list_tools(&self) -> Result<Vec<Value>, String> { Ok(Vec::new()) }
        fn call_tool(&self, name: &str, _args: Value, _tool_call_id: &str) -> Result<Value, String> {
            Err(format!("unexpected delegated tool: {name}"))
        }
    }

    fn fixture(name: &str) -> (
        Arc<dyn AndroidRoutedToolBridge>,
        Arc<Mutex<AndroidAgentRoster>>,
        Arc<Mutex<AndroidMessagingService>>,
        Arc<Mutex<Option<String>>>,
        std::path::PathBuf,
    ) {
        let root = std::env::temp_dir().join(format!(
            "fabushi-agent-management-{name}-{}-{}",
            std::process::id(),
            now_ms_i64()
        ));
        fs::create_dir_all(&root).unwrap();
        let roster = Arc::new(Mutex::new(AndroidAgentRoster::open(root.join("agents.json")).unwrap()));
        let target = roster.lock().unwrap().create("Target", "target").unwrap();
        assert_eq!(target.id, "agent-00000001");
        let messaging = Arc::new(Mutex::new(AndroidMessagingService::open(&root).unwrap()));
        let transcript = Arc::new(Mutex::new(TranscriptStore::open(root.join("transcript.json")).unwrap()));
        let live = Arc::new(Mutex::new(Some("acct:a".into())));
        let tools = with_agent_management_tools_and_transcript(
            Arc::new(EmptyTools),
            Arc::clone(&roster),
            Arc::clone(&messaging),
            transcript,
            Arc::clone(&live),
            "acct:a",
            "agent-root",
            Arc::new(AtomicBool::new(false)),
            Arc::new(AgentTurnInterruptionRegistry::default()),
        );
        (tools, roster, messaging, live, root)
    }

    #[test]
    fn exposes_root_tools_and_delivers_real_message() {
        let (tools, _, _, _, root) = fixture("send");
        let names = tools.list_tools().unwrap().into_iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str).map(str::to_string))
            .collect::<Vec<_>>();
        assert_eq!(&names[..3], &[SEND_TO_AGENT_TOOL_NAME, CREATE_AGENT_TOOL_NAME, UPDATE_AGENT_TOOL_NAME]);
        let result = tools.call_tool(
            SEND_TO_AGENT_TOOL_NAME,
            json!({"target_id":"agent-00000001","message":"hello","images":[{"url":"https://example.com/a.png"}],"priority":true}),
            "call-send",
        ).unwrap();
        assert!(result.as_str().unwrap().contains("fresh turn"));
        let replay = tools.call_tool(
            SEND_TO_AGENT_TOOL_NAME,
            json!({"target_id":"agent-00000001","message":"hello","images":[{"url":"https://example.com/a.png"}],"priority":true}),
            "call-send",
        ).unwrap();
        assert_eq!(replay, result);
        drop(tools);
        assert!(root.join("messaging-repository.json").exists());
        let transcript = TranscriptStore::open(root.join("transcript.json")).unwrap().get_transcript();
        assert_eq!(transcript.len(), 2, "delivery replay must not duplicate peer transcript entries");
        let outbound = transcript.iter().find(|entry| entry["agentId"] == "agent-root").unwrap();
        assert_eq!(outbound["role"], "assistant");
        assert_eq!(outbound["toAgent"]["id"], "agent-00000001");
        assert_eq!(outbound["images"].as_array().unwrap().len(), 1);
        let inbound = transcript.iter().find(|entry| entry["agentId"] == "agent-00000001").unwrap();
        assert_eq!(inbound["role"], "user");
        assert_eq!(inbound["fromAgent"]["id"], "agent-root");
        assert_eq!(inbound["content"], "hello");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_invalid_missing_self_and_bad_image_input() {
        let (tools, _, _, _, root) = fixture("invalid");
        assert!(tools.call_tool(SEND_TO_AGENT_TOOL_NAME, json!({"target_id":"missing","message":"x"}), "a").is_err());
        assert!(tools.call_tool(SEND_TO_AGENT_TOOL_NAME, json!({"target_id":"agent-00000001","message":"x","images":[{"url":"http://example.com/a.png"}]}), "b").is_err());
        let self_result = tools.call_tool(SEND_TO_AGENT_TOOL_NAME, json!({"target_id":"agent-root","message":"x"}), "c").unwrap();
        assert_eq!(self_result, Value::String(SELF_SEND_REJECTION.into()));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn create_update_duplicates_survive_reopen_and_mismatch_fails_closed() {
        let (tools, roster, _, _, root) = fixture("crud");
        let created = tools.call_tool(CREATE_AGENT_TOOL_NAME, json!({"name":"New","description":"d"}), "stable-create").unwrap();
        assert_eq!(created, tools.call_tool(CREATE_AGENT_TOOL_NAME, json!({"name":"New","description":"d"}), "stable-create").unwrap());
        assert!(tools.call_tool(CREATE_AGENT_TOOL_NAME, json!({"name":"Other"}), "stable-create").is_err());
        let id = roster.lock().unwrap().list().into_iter().find(|row| row.name == "New").unwrap().id;
        let updated = tools.call_tool(UPDATE_AGENT_TOOL_NAME, json!({"agent_id":id,"name":"Renamed"}), "stable-update").unwrap();
        assert_eq!(updated, tools.call_tool(UPDATE_AGENT_TOOL_NAME, json!({"agent_id":id,"name":"Renamed"}), "stable-update").unwrap());
        drop(tools);
        drop(roster);
        let reopened = AndroidAgentRoster::open(root.join("agents.json")).unwrap();
        assert_eq!(reopened.list().into_iter().filter(|row| row.name == "Renamed").count(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn account_switch_and_cancellation_fail_before_side_effect() {
        let (tools, roster, _, live, root) = fixture("fence");
        *live.lock().unwrap() = Some("acct:b".into());
        assert!(tools.call_tool(CREATE_AGENT_TOOL_NAME, json!({"name":"Blocked"}), "stale").is_err());
        assert_eq!(roster.lock().unwrap().list().len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cancellation_fails_before_agent_management_side_effect() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-agent-management-cancel-{}-{}",
            std::process::id(),
            now_ms_i64()
        ));
        fs::create_dir_all(&root).unwrap();
        let roster = Arc::new(Mutex::new(AndroidAgentRoster::open(root.join("agents.json")).unwrap()));
        let messaging = Arc::new(Mutex::new(AndroidMessagingService::open(&root).unwrap()));
        let live = Arc::new(Mutex::new(Some("acct:a".into())));
        let cancelled = Arc::new(AtomicBool::new(true));
        let tools = with_agent_management_tools(
            Arc::new(EmptyTools),
            Arc::clone(&roster),
            messaging,
            live,
            "acct:a",
            "agent-root",
            cancelled,
            Arc::new(AgentTurnInterruptionRegistry::default()),
        );
        assert!(tools
            .call_tool(CREATE_AGENT_TOOL_NAME, json!({"name":"Blocked"}), "cancelled-call")
            .unwrap_err()
            .contains("cancelled before side effects"));
        assert_eq!(roster.lock().unwrap().count(), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn priority_direct_send_interrupts_only_the_live_target_turn() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-agent-management-priority-{}-{}",
            std::process::id(),
            now_ms_i64()
        ));
        fs::create_dir_all(&root).unwrap();
        let roster = Arc::new(Mutex::new(AndroidAgentRoster::open(root.join("agents.json")).unwrap()));
        let target = roster.lock().unwrap().create("Target", "target").unwrap();
        let messaging = Arc::new(Mutex::new(AndroidMessagingService::open(&root).unwrap()));
        let live = Arc::new(Mutex::new(Some("acct:a".into())));
        let target_cancelled = Arc::new(AtomicBool::new(false));
        let unrelated_cancelled = Arc::new(AtomicBool::new(false));
        let interruptions = Arc::new(AgentTurnInterruptionRegistry::default());
        interruptions.register(
            &target.id,
            "operation-target",
            "acct:a",
            false,
            Arc::clone(&target_cancelled),
        ).unwrap();
        interruptions.register(
            "agent-unrelated",
            "operation-unrelated",
            "acct:a",
            false,
            Arc::clone(&unrelated_cancelled),
        ).unwrap();
        let tools = with_agent_management_tools(
            Arc::new(EmptyTools),
            roster,
            messaging,
            live,
            "acct:a",
            "agent-root",
            Arc::new(AtomicBool::new(false)),
            Arc::clone(&interruptions),
        );
        tools.call_tool(
            SEND_TO_AGENT_TOOL_NAME,
            json!({"target_id":target.id,"message":"STOP the old work and do this","priority":true}),
            "priority-direct",
        ).unwrap();
        assert!(target_cancelled.load(Ordering::Acquire));
        assert!(!unrelated_cancelled.load(Ordering::Acquire));
        assert!(interruptions.has_active_turn(&target.id, "acct:a"));
        interruptions.unregister_operation("operation-target");
        assert!(!interruptions.has_active_turn(&target.id, "acct:a"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn deletion_interrupts_even_a_protected_user_lane_but_never_crosses_account_fence() {
        let interruptions = AgentTurnInterruptionRegistry::default();
        let cancelled = Arc::new(AtomicBool::new(false));
        interruptions.register(
            "agent-delete",
            "operation-delete",
            "acct:a",
            true,
            Arc::clone(&cancelled),
        ).unwrap();

        assert_eq!(
            interruptions.interrupt_for_deletion("agent-delete", "acct:b").unwrap(),
            None,
        );
        assert!(!cancelled.load(Ordering::Acquire));
        assert_eq!(
            interruptions.interrupt_for_deletion("agent-delete", "acct:a").unwrap().as_deref(),
            Some("operation-delete"),
        );
        assert!(cancelled.load(Ordering::Acquire));
    }

    #[test]
    fn priority_direct_send_never_interrupts_a_user_lane() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-agent-management-priority-user-lane-{}-{}",
            std::process::id(),
            now_ms_i64()
        ));
        fs::create_dir_all(&root).unwrap();
        let roster = Arc::new(Mutex::new(AndroidAgentRoster::open(root.join("agents.json")).unwrap()));
        let target = roster.lock().unwrap().create("Target", "target").unwrap();
        let messaging = Arc::new(Mutex::new(AndroidMessagingService::open(&root).unwrap()));
        let live = Arc::new(Mutex::new(Some("acct:a".into())));
        let user_lane_cancelled = Arc::new(AtomicBool::new(false));
        let interruptions = Arc::new(AgentTurnInterruptionRegistry::default());
        interruptions.register(
            &target.id,
            "operation-user-lane",
            "acct:a",
            true,
            Arc::clone(&user_lane_cancelled),
        ).unwrap();
        let tools = with_agent_management_tools(
            Arc::new(EmptyTools),
            roster,
            messaging,
            live,
            "acct:a",
            "agent-root",
            Arc::new(AtomicBool::new(false)),
            Arc::clone(&interruptions),
        );
        tools.call_tool(
            SEND_TO_AGENT_TOOL_NAME,
            json!({"target_id":target.id,"message":"priority peer update","priority":true}),
            "priority-user-lane",
        ).unwrap();
        assert!(
            !user_lane_cancelled.load(Ordering::Acquire),
            "Desktop parity: a priority Agent message must never interrupt the user's active lane"
        );
        assert!(interruptions.has_active_turn(&target.id, "acct:a"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn group_send_uses_canonical_group_conversation_and_ignores_priority() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-agent-management-group-{}-{}",
            std::process::id(),
            now_ms_i64()
        ));
        fs::create_dir_all(&root).unwrap();
        let roster_path = root.join("agents.json");
        let mut roster_seed = AndroidAgentRoster::open(&roster_path).unwrap();
        let group = roster_seed.create("Group", "group").unwrap();
        drop(roster_seed);
        let mut roster_json: Value =
            serde_json::from_slice(&fs::read(&roster_path).unwrap()).unwrap();
        roster_json["agents"][0]["isGroup"] = json!(true);
        fs::write(&roster_path, serde_json::to_vec_pretty(&roster_json).unwrap()).unwrap();

        let roster = Arc::new(Mutex::new(AndroidAgentRoster::open(&roster_path).unwrap()));
        let messaging = Arc::new(Mutex::new(AndroidMessagingService::open(&root).unwrap()));
        for (request_id, actor_id, display_name, kind) in [
            ("group-profile-root", "agent-root", "Root", "assistant"),
            ("group-profile-target", group.id.as_str(), group.name.as_str(), "bot"),
        ] {
            let request = json!({
                "requestId":request_id,
                "envelope":{
                    "protocolVersion":2,
                    "context":{
                        "requestId":request_id,
                        "deviceId":"android:test",
                        "actorId":actor_id,
                        "sessionId":"session:test",
                        "sentAtMs":1
                    },
                    "command":{
                        "type":"upsertProfile",
                        "actor":{"id":actor_id,"kind":kind,"displayName":display_name}
                    }
                }
            });
            messaging.lock().unwrap().execute(&request, actor_id, 1).unwrap();
        }
        let request = json!({
            "requestId":"group-seed",
            "envelope":{
                "protocolVersion":2,
                "context":{
                    "requestId":"group-seed",
                    "deviceId":"android:test",
                    "actorId":"agent-root",
                    "sessionId":"session:test",
                    "sentAtMs":1
                },
                "command":{
                    "type":"createConversation",
                    "conversation":{
                        "id":group.id,
                        "kind":"group",
                        "title":group.name,
                        "ownerId":"agent-root",
                        "participants":[
                            {"actorId":"agent-root","role":"owner","joinedAtMs":1},
                            {"actorId":group.id,"role":"member","joinedAtMs":1}
                        ],
                        "permissions":{"canSendMessages":true,"canSendMedia":true}
                    }
                }
            }
        });
        messaging.lock().unwrap().execute(&request, "agent-root", 1).unwrap();

        let live = Arc::new(Mutex::new(Some("acct:a".into())));
        let tools = with_agent_management_tools(
            Arc::new(EmptyTools),
            roster,
            Arc::clone(&messaging),
            live,
            "acct:a",
            "agent-root",
            Arc::new(AtomicBool::new(false)),
            Arc::new(AgentTurnInterruptionRegistry::default()),
        );
        let result = tools.call_tool(
            SEND_TO_AGENT_TOOL_NAME,
            json!({"target_id":group.id,"message":"group message","priority":true}),
            "group-send",
        ).unwrap();
        assert!(result.as_str().unwrap().contains("ignored priority"));

        let stored: Value =
            serde_json::from_slice(&fs::read(root.join("messaging-repository.json")).unwrap()).unwrap();
        let found_priority = stored["messages"]
            .get(&group.id)
            .and_then(Value::as_object)
            .and_then(|messages| messages.values().next())
            .and_then(|message| message.get("content"))
            .and_then(|content| content.get("data"))
            .and_then(|data| data.get("agentPriority"))
            .and_then(Value::as_bool);
        assert_eq!(found_priority, Some(false));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn message_clamp_matches_desktop_utf16_limit() {
        assert_eq!(clamp_agent_message("  hello  "), "hello");
        let ascii = "a".repeat(8_100);
        assert_eq!(clamp_agent_message(&ascii).encode_utf16().count(), 8_000);
        let astral = "😀".repeat(4_100);
        assert_eq!(clamp_agent_message(&astral).encode_utf16().count(), 8_000);
    }

    #[test]
    fn image_scheme_validation_accepts_https_and_canonical_attachment_only() {
        assert!(validate_image_url("https://example.com/a.png").is_ok());
        assert!(validate_image_url("fabushi-attachment://attachment-1").is_ok());
        assert!(validate_image_url("http://example.com/a.png").is_err());
        assert!(validate_image_url("file:///sdcard/secret.png").is_err());
        assert!(validate_image_url("fabushi-attachment://attachment-1/extra").is_err());
    }
}
