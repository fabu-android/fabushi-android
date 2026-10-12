use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use super::xuser_wire_normalization::MAX_INLINE_IMAGES_PER_ENTRY;

pub const SAND_SHARED_ROOM_IMAGE_BYTES_MAX: usize = 1_100_000;
pub const REMOTE_AGENT_ID_PREFIX: &str = "sand-remote:";

pub fn parse_remote_agent_id(id: &str) -> Option<(String, String)> {
    let body = id.strip_prefix(REMOTE_AGENT_ID_PREFIX)?;
    let (owner, agent) = body.split_once('/')?;
    let owner = urlencoding_decode(owner)?;
    let agent = urlencoding_decode(agent)?;
    if owner.is_empty() || agent.is_empty() {
        None
    } else {
        Some((owner, agent))
    }
}

fn urlencoding_decode(value: &str) -> Option<String> {
    url::form_urlencoded::parse(value.as_bytes())
        .next()
        .map(|(value, _)| value.into_owned())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedXuserAttachment {
    pub data: Vec<u8>,
    pub mime_type: String,
}

pub trait XuserEntryPublisherHost: Send + Sync {
    fn relay_send(&self, payload: &Value) -> Result<Value, String>;
    fn restamp_room_entry(
        &self,
        room_id: &str,
        entry_id: &str,
        timestamp_ms: f64,
    ) -> Result<(), String>;
    fn now_ms(&self) -> u64;
    fn is_enabled(&self) -> bool;
    fn self_auth_id(&self) -> Option<String>;
    fn resolve_attachment(&self, url: &str) -> Result<Option<ResolvedXuserAttachment>, String>;
}

enum PublisherCommand {
    Publish(Value),
    PublishAndWait(Value, mpsc::SyncSender<Result<(), String>>),
    Barrier(mpsc::SyncSender<()>),
}

pub struct SandXuserEntryPublisher {
    host: Arc<dyn XuserEntryPublisherHost>,
    room_workers: Mutex<HashMap<String, mpsc::Sender<PublisherCommand>>>,
}

impl SandXuserEntryPublisher {
    pub fn new(host: Arc<dyn XuserEntryPublisherHost>) -> Self {
        Self {
            host,
            room_workers: Mutex::new(HashMap::new()),
        }
    }

    fn sender_for_room(&self, shared_room_id: &str) -> Result<mpsc::Sender<PublisherCommand>, String> {
        let mut workers = self
            .room_workers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(sender) = workers.get(shared_room_id) {
            return Ok(sender.clone());
        }

        let room_id = shared_room_id.to_string();
        let worker_room_id = room_id.clone();
        let host = Arc::clone(&self.host);
        let (sender, receiver) = mpsc::channel::<PublisherCommand>();
        thread::Builder::new()
            .name("xuser-entry-publisher".into())
            .spawn(move || {
                while let Ok(command) = receiver.recv() {
                    match command {
                        PublisherCommand::Publish(entry) => {
                            let _ = publish_entry_with_host(host.as_ref(), &worker_room_id, &entry);
                        }
                        PublisherCommand::PublishAndWait(entry, done) => {
                            let result =
                                publish_entry_with_host(host.as_ref(), &worker_room_id, &entry);
                            let _ = done.send(result);
                        }
                        PublisherCommand::Barrier(done) => {
                            let _ = done.send(());
                        }
                    }
                }
            })
            .map_err(|error| format!("could not start xuser entry publisher: {error}"))?;
        workers.insert(room_id, sender.clone());
        Ok(sender)
    }

    pub fn enqueue_publish(&self, shared_room_id: &str, entry: &Value) -> Result<(), String> {
        self.sender_for_room(shared_room_id)?
            .send(PublisherCommand::Publish(entry.clone()))
            .map_err(|_| "xuser entry publisher worker stopped".to_string())
    }

    pub fn flush_room(&self, shared_room_id: &str) -> Result<(), String> {
        let sender = self.sender_for_room(shared_room_id)?;
        let (done, receive) = mpsc::sync_channel(1);
        sender
            .send(PublisherCommand::Barrier(done))
            .map_err(|_| "xuser entry publisher worker stopped".to_string())?;
        receive
            .recv()
            .map_err(|_| "xuser entry publisher worker stopped".to_string())
    }

    pub fn publish_entry_ordered_and_wait(
        &self,
        shared_room_id: &str,
        entry: &Value,
    ) -> Result<(), String> {
        let sender = self.sender_for_room(shared_room_id)?;
        let (done, receive) = mpsc::sync_channel(1);
        sender
            .send(PublisherCommand::PublishAndWait(entry.clone(), done))
            .map_err(|_| "xuser entry publisher worker stopped".to_string())?;
        receive
            .recv()
            .map_err(|_| "xuser entry publisher worker stopped".to_string())?
    }

    pub fn publish_entry(&self, shared_room_id: &str, entry: &Value) -> Result<(), String> {
        publish_entry_with_host(self.host.as_ref(), shared_room_id, entry)
    }

    pub fn serialize_entry(&self, entry: &Value, self_auth_id: &str) -> Option<Value> {
        serialize_entry_with_host(self.host.as_ref(), entry, self_auth_id)
    }
}

fn publish_entry_with_host(
    host: &dyn XuserEntryPublisherHost,
    shared_room_id: &str,
    entry: &Value,
) -> Result<(), String> {
    if !host.is_enabled() {
        return Ok(());
    }
    let Some(self_auth_id) = host.self_auth_id() else {
        return Ok(());
    };
    let Some(wire) = serialize_entry_with_host(host, entry, &self_auth_id) else {
        return Ok(());
    };

    let result = host.relay_send(&json!({
        "kind": "room-entry",
        "roomId": shared_room_id,
        "entry": wire,
    }))?;
    if let Some(timestamp_ms) = result
        .get("timestampMs")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
    {
        if let Some(entry_id) = entry.get("id").and_then(Value::as_str) {
            host.restamp_room_entry(shared_room_id, entry_id, timestamp_ms)?;
        }
    }
    Ok(())
}

fn serialize_entry_with_host(
    host: &dyn XuserEntryPublisherHost,
    entry: &Value,
    self_auth_id: &str,
) -> Option<Value> {
    let kind = entry.get("kind").and_then(Value::as_str)?;
    if kind == "message" && entry.get("role").and_then(Value::as_str) == Some("user") {
        if ["fromAgent", "toAgent", "channel", "fromUser"]
            .iter()
            .any(|field| entry.get(*field).is_some_and(|value| !value.is_null()))
        {
            return None;
        }
        let content = entry.get("content").and_then(Value::as_str).unwrap_or_default();
        let images = inline_images(host, entry.get("images"));
        if content.trim().is_empty() && images.is_empty() {
            return None;
        }
        let entry_id = entry.get("id").and_then(Value::as_str)?;
        let mut wire = json!({
            "kind": "human-message",
            "entryId": entry_id,
            "authorAuthId": self_auth_id,
            "authorName": "Host",
            "text": content,
            "images": images,
            "timestampMs": timestamp_or_now(entry, host),
        });
        if let Some(client_nonce) = entry
            .get("clientNonce")
            .filter(|value| !value.is_null())
            .cloned()
        {
            wire["clientNonce"] = client_nonce;
        }
        return Some(wire);
    }

    if kind == "send-message" && entry.get("streaming").and_then(Value::as_bool) != Some(true) {
        let message = entry.get("message")?;
        if message.get("type").and_then(Value::as_str) != Some("text") {
            return None;
        }
        let author = entry.get("author")?;
        let author_id = author.get("id").and_then(Value::as_str)?;
        if parse_remote_agent_id(author_id).is_some() {
            return None;
        }
        let author_name = author.get("name").and_then(Value::as_str)?;
        let content = message.get("content").and_then(Value::as_str)?;
        let entry_id = entry.get("id").and_then(Value::as_str)?;
        return Some(json!({
            "kind": "agent-message",
            "entryId": entry_id,
            "agentOwnerAuthId": self_auth_id,
            "agentId": author_id,
            "authorName": author_name,
            "text": content,
            "images": inline_images(host, message.get("images")),
            "timestampMs": timestamp_or_now(entry, host),
        }));
    }

    None
}

fn timestamp_or_now(entry: &Value, host: &dyn XuserEntryPublisherHost) -> f64 {
    entry
        .get("timestampMs")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .unwrap_or_else(|| host.now_ms() as f64)
}

fn inline_images(host: &dyn XuserEntryPublisherHost, raw: Option<&Value>) -> Vec<Value> {
    let Some(images) = raw.and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut inlined = Vec::new();
    let mut total_bytes = 0usize;
    for image in images.iter().take(MAX_INLINE_IMAGES_PER_ENTRY) {
        let Some(url) = image.get("url").and_then(Value::as_str) else {
            continue;
        };
        let Ok(Some(resolved)) = host.resolve_attachment(url) else {
            continue;
        };
        let next_total = total_bytes.saturating_add(resolved.data.len());
        if next_total > SAND_SHARED_ROOM_IMAGE_BYTES_MAX
            || !resolved.mime_type.starts_with("image/")
        {
            continue;
        }
        total_bytes = next_total;
        let mut value = json!({
            "base64": STANDARD.encode(&resolved.data),
            "mediaType": resolved.mime_type,
        });
        if let Some(alt) = image
            .get("alt")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            value["alt"] = Value::String(alt.to_string());
        }
        inlined.push(value);
    }
    inlined
}

/// Pure no-I/O projection kept for call sites that only need a quick
/// publishability check. The production publisher above owns the full frozen
/// role/attachment/restamp contract.
pub fn build_publish_payload(entry: &Value, self_auth_id: &str) -> Option<Value> {
    let id = entry.get("id")?.as_str()?.trim();
    if id.is_empty() {
        return None;
    }
    let kind = entry.get("kind")?.as_str()?;
    match kind {
        "message" => {
            let content = entry
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            let images = entry
                .get("images")
                .and_then(Value::as_array)
                .map(|value| {
                    value
                        .iter()
                        .take(MAX_INLINE_IMAGES_PER_ENTRY)
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if content.is_empty() && images.is_empty() {
                return None;
            }
            Some(json!({
                "kind": "entry",
                "entryId": id,
                "authorAuthId": self_auth_id,
                "text": content,
                "images": images,
                "clientNonce": entry.get("clientNonce").cloned().unwrap_or(Value::Null),
            }))
        }
        "send-message" => {
            if entry.get("streaming").and_then(Value::as_bool) == Some(true) {
                return None;
            }
            let message = entry.get("message")?;
            let content = message
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            if content.is_empty() {
                return None;
            }
            Some(json!({
                "kind": "entry",
                "entryId": id,
                "authorAuthId": self_auth_id,
                "text": content,
            }))
        }
        _ => None,
    }
}
