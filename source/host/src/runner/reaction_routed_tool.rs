use super::AndroidRoutedToolBridge;
use crate::extensions::transcript::TranscriptStore;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};

pub const REACT_TO_MESSAGE_TOOL_NAME: &str = "ReactToMessage";

#[derive(Clone, Default)]
pub struct ReactionDeliveryCounter {
    count: Arc<AtomicU64>,
}

impl ReactionDeliveryCounter {
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::SeqCst)
    }

    fn record_success(&self) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }
}

pub fn with_reaction_routed_tool(
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    transcript: Arc<Mutex<TranscriptStore>>,
    live_account_fence: Arc<Mutex<Option<String>>>,
    account_fence: &str,
    agent_id: &str,
    cancelled: Arc<AtomicBool>,
    counter: ReactionDeliveryCounter,
) -> Arc<dyn AndroidRoutedToolBridge> {
    Arc::new(ReactionRoutedTool {
        delegate,
        transcript,
        live_account_fence,
        account_fence: account_fence.to_string(),
        agent_id: agent_id.to_string(),
        cancelled,
        counter,
    })
}

struct ReactionRoutedTool {
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    transcript: Arc<Mutex<TranscriptStore>>,
    live_account_fence: Arc<Mutex<Option<String>>>,
    account_fence: String,
    agent_id: String,
    cancelled: Arc<AtomicBool>,
    counter: ReactionDeliveryCounter,
}

impl ReactionRoutedTool {
    fn assert_live(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err("ReactToMessage tool call cancelled".into());
        }
        let live = self
            .live_account_fence
            .lock()
            .map_err(|_| "ReactToMessage account fence lock poisoned".to_string())?;
        if live.as_deref() != Some(self.account_fence.as_str()) {
            return Err("ReactToMessage callback belongs to a stale account epoch".into());
        }
        Ok(())
    }

    fn react(&self, address: &str, emoji: &str) -> Result<(), String> {
        self.assert_live()?;
        if !is_message_address(address) {
            return Err(format!(
                "\"{address}\" isn't a valid message address. React with the [t3u]-style tag shown on the user's message."
            ));
        }
        if emoji.is_empty() || emoji.encode_utf16().count() > 16 || emoji.chars().any(char::is_control) {
            return Err("emoji must be a non-empty reaction of at most 16 UTF-16 code units".into());
        }
        let current = self
            .transcript
            .lock()
            .map_err(|_| "transcript lock poisoned".to_string())?
            .entry(address)
            .ok_or("reaction transcript entry was not found")?;
        if current.get("agentId").and_then(Value::as_str) != Some(self.agent_id.as_str()) {
            return Err("reaction entry is fenced to another Agent".into());
        }
        if current.get("accountFence").and_then(Value::as_str) != Some(self.account_fence.as_str()) {
            return Err("reaction entry is fenced to another account generation".into());
        }
        if current.get("kind").and_then(Value::as_str) != Some("message")
            || current.get("role").and_then(Value::as_str) != Some("user")
        {
            return Err("ReactToMessage may target only a user message".into());
        }
        let already = current
            .get("reactions")
            .and_then(Value::as_array)
            .is_some_and(|reactions| {
                reactions.iter().any(|reaction| {
                    reaction.get("emoji").and_then(Value::as_str) == Some(emoji)
                        && reaction.get("by").and_then(Value::as_str) == Some("me")
                })
            });
        let emoji_owned = emoji.to_string();
        self.transcript
            .lock()
            .map_err(|_| "transcript lock poisoned".to_string())?
            .update_entry(address, |entry| {
                let mut next = entry.clone();
                let mut reactions = next
                    .get("reactions")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                reactions.retain(|reaction| {
                    !(reaction.get("emoji").and_then(Value::as_str) == Some(emoji_owned.as_str())
                        && reaction.get("by").and_then(Value::as_str) == Some("me"))
                });
                if !already {
                    reactions.push(json!({"emoji": emoji_owned, "by": "me"}));
                }
                if reactions.is_empty() {
                    next.as_object_mut().expect("message transcript entry must be object").remove("reactions");
                } else {
                    next["reactions"] = Value::Array(reactions);
                }
                next
            })
            .map_err(|error| format!("failed to persist Agent reaction: {error}"))?
            .ok_or("reaction transcript entry disappeared before commit")?;
        self.assert_live()?;
        self.counter.record_success();
        Ok(())
    }
}

impl AndroidRoutedToolBridge for ReactionRoutedTool {
    fn list_tools(&self) -> Result<Vec<Value>, String> {
        let mut tools = self.delegate.list_tools()?;
        tools.retain(|tool| tool.get("name").and_then(Value::as_str) != Some(REACT_TO_MESSAGE_TOOL_NAME));
        tools.insert(0, json!({
            "type":"function",
            "name":REACT_TO_MESSAGE_TOOL_NAME,
            "description":"React to one of the user's messages with a single emoji tapback. Use the t3u-style message address shown on the user's message; reacting with the same emoji again toggles it off.",
            "parameters":{
                "type":"object",
                "additionalProperties":false,
                "required":["message_address","emoji"],
                "properties":{
                    "message_address":{"type":"string","minLength":1},
                    "emoji":{"type":"string","minLength":1,"maxLength":16}
                }
            }
        }));
        Ok(tools)
    }

    fn call_tool(&self, name: &str, args: Value, tool_call_id: &str) -> Result<Value, String> {
        if name != REACT_TO_MESSAGE_TOOL_NAME {
            return self.delegate.call_tool(name, args, tool_call_id);
        }
        let object = args.as_object().ok_or("ReactToMessage arguments must be an object")?;
        let address = object
            .get("message_address")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or("message_address is required for ReactToMessage")?;
        let emoji = object
            .get("emoji")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or("emoji is required for ReactToMessage")?;
        if !is_message_address(address) {
            return Ok(Value::String(format!(
                "\"{address}\" isn't a valid message address. React with the [t3u]-style tag shown on the user's message."
            )));
        }
        self.react(address, emoji)?;
        Ok(Value::String(format!(
            "Reacted {emoji} on {address}. (Reactions toggle: react the same emoji again to take it back.)"
        )))
    }
}

pub fn is_message_address(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.first().copied() != Some(b't') || bytes.len() < 3 {
        return false;
    }
    fn digits(bytes: &[u8], index: &mut usize) -> bool {
        let start = *index;
        while *index < bytes.len() && bytes[*index].is_ascii_digit() {
            *index += 1;
        }
        *index > start
    }
    let mut index = 1usize;
    if bytes[index] == b'b' {
        index += 1;
        if index >= bytes.len() || !matches!(bytes[index], b'a' | b's') {
            return false;
        }
        index += 1;
        return digits(bytes, &mut index) && index == bytes.len();
    }
    if !digits(bytes, &mut index) || index >= bytes.len() {
        return false;
    }
    match bytes[index] {
        b'u' => {
            index += 1;
            if index == bytes.len() {
                return true;
            }
            if bytes[index] != b'a' {
                return false;
            }
            index += 1;
            digits(bytes, &mut index) && index == bytes.len()
        }
        b'a' | b's' => {
            index += 1;
            digits(bytes, &mut index) && index == bytes.len()
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct EmptyBridge;
    impl AndroidRoutedToolBridge for EmptyBridge {
        fn list_tools(&self) -> Result<Vec<Value>, String> {
            Ok(vec![json!({"type":"function","name":"delegate","parameters":{"type":"object"}})])
        }
        fn call_tool(&self, name: &str, _args: Value, _tool_call_id: &str) -> Result<Value, String> {
            Ok(json!({"delegated":name}))
        }
    }

    fn temp_store() -> (std::path::PathBuf, Arc<Mutex<TranscriptStore>>) {
        let root = std::env::temp_dir().join(format!("fabushi-reaction-tool-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let store = TranscriptStore::open(root.join("transcript.json")).unwrap();
        (root, Arc::new(Mutex::new(store)))
    }

    #[test]
    fn address_validation_matches_desktop_contract() {
        for valid in ["t3u", "t3ua0", "t3a1", "t3s1", "tba1", "tbs1"] {
            assert!(is_message_address(valid), "{valid}");
        }
        for invalid in ["", "u3", "t", "t3", "tb1", "t3x1", "t3ua"] {
            assert!(!is_message_address(invalid), "{invalid}");
        }
    }

    #[test]
    fn routed_reaction_toggles_canonical_user_entry_and_fences_account() {
        let (root, transcript) = temp_store();
        transcript.lock().unwrap().append_entry(json!({
            "id":"t3u","kind":"message","role":"user","content":"hello",
            "agentId":"agent-a","accountFence":"account-a"
        })).unwrap();
        let live = Arc::new(Mutex::new(Some("account-a".to_string())));
        let counter = ReactionDeliveryCounter::default();
        let bridge = with_reaction_routed_tool(
            Arc::new(EmptyBridge), Arc::clone(&transcript), Arc::clone(&live),
            "account-a", "agent-a", Arc::new(AtomicBool::new(false)), counter.clone(),
        );
        assert!(bridge.list_tools().unwrap().iter().any(|tool| tool["name"] == REACT_TO_MESSAGE_TOOL_NAME));
        bridge.call_tool(REACT_TO_MESSAGE_TOOL_NAME, json!({"message_address":"t3u","emoji":"👍"}), "c1").unwrap();
        assert_eq!(transcript.lock().unwrap().entry("t3u").unwrap()["reactions"][0]["emoji"], "👍");
        bridge.call_tool(REACT_TO_MESSAGE_TOOL_NAME, json!({"message_address":"t3u","emoji":"👍"}), "c2").unwrap();
        assert!(transcript.lock().unwrap().entry("t3u").unwrap().get("reactions").is_none());
        assert_eq!(counter.count(), 2);
        *live.lock().unwrap() = Some("account-b".into());
        assert!(bridge.call_tool(REACT_TO_MESSAGE_TOOL_NAME, json!({"message_address":"t3u","emoji":"🔥"}), "c3").unwrap_err().contains("stale account"));
        assert_eq!(counter.count(), 2);
        let _ = fs::remove_dir_all(root);
    }
}
