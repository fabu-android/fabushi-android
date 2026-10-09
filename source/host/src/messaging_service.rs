use crate::messaging_child::{
    ConversationChildIdentity, ConversationChildPaginationState, ConversationChildStore,
    ConversationDestination, ConversationMessagePosition,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
};

const MESSAGING_PROTOCOL_VERSION: u64 = 2;
const MESSAGING_REPOSITORY_SCHEMA_VERSION: u32 = 1;
const MAX_REPLAY_RESULTS: usize = 512;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MessagingRepositoryState {
    schema_version: u32,
    cursor: u64,
    #[serde(default)]
    actors: BTreeMap<String, Value>,
    #[serde(default)]
    conversations: BTreeMap<String, Value>,
    #[serde(default)]
    messages: BTreeMap<String, BTreeMap<String, Value>>,
    #[serde(default)]
    request_results: BTreeMap<String, Value>,
    #[serde(default)]
    request_order: Vec<String>,
}

impl Default for MessagingRepositoryState {
    fn default() -> Self {
        Self {
            schema_version: MESSAGING_REPOSITORY_SCHEMA_VERSION,
            cursor: 0,
            actors: BTreeMap::new(),
            conversations: BTreeMap::new(),
            messages: BTreeMap::new(),
            request_results: BTreeMap::new(),
            request_order: Vec::new(),
        }
    }
}

/// Canonical Android messaging owner behind Host.
///
/// Presentation supplies commands, never authorization facts. This service owns
/// actor/conversation/message state, validates account-scoped membership, and
/// delegates actor-scoped child lifecycle state to ConversationChildStore.
pub struct AndroidMessagingService {
    path: PathBuf,
    state: MessagingRepositoryState,
    children: ConversationChildStore,
}

impl AndroidMessagingService {
    pub fn open(app_data_dir: impl AsRef<Path>) -> io::Result<Self> {
        let app_data_dir = app_data_dir.as_ref();
        let path = app_data_dir.join("messaging-repository.json");
        let state = match fs::read(&path) {
            Ok(bytes) => {
                let state: MessagingRepositoryState = serde_json::from_slice(&bytes).map_err(|error| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("invalid Android messaging repository: {error}"),
                    )
                })?;
                if state.schema_version != MESSAGING_REPOSITORY_SCHEMA_VERSION {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "unsupported Android messaging repository schema {}; expected {}",
                            state.schema_version, MESSAGING_REPOSITORY_SCHEMA_VERSION
                        ),
                    ));
                }
                validate_loaded_state(&state)?;
                state
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => MessagingRepositoryState::default(),
            Err(error) => return Err(error),
        };
        let children = ConversationChildStore::open(app_data_dir.join("conversation-children.json"))?;
        Ok(Self {
            path,
            state,
            children,
        })
    }

    pub fn execute(
        &mut self,
        params: &Value,
        authenticated_actor_id: &str,
        now_ms: i64,
    ) -> Result<Value, String> {
        let request_id = required_string(params, "requestId")?;
        if !bounded_id(request_id) {
            return Err("messaging requestId is invalid".into());
        }
        let replay_key = format!("{authenticated_actor_id}:{request_id}");
        if let Some(replayed) = self.state.request_results.get(&replay_key) {
            return Ok(replayed.clone());
        }

        let envelope = params
            .get("envelope")
            .and_then(Value::as_object)
            .ok_or("messaging envelope is required")?;
        if envelope
            .get("protocolVersion")
            .and_then(Value::as_u64)
            != Some(MESSAGING_PROTOCOL_VERSION)
        {
            return Err("unsupported messaging protocol version".into());
        }
        let context = envelope
            .get("context")
            .and_then(Value::as_object)
            .ok_or("messaging context is required")?;
        if context.get("requestId").and_then(Value::as_str) != Some(request_id) {
            return Err("messaging context requestId mismatch".into());
        }
        if context.get("actorId").and_then(Value::as_str) != Some(authenticated_actor_id) {
            return Err("messaging actor does not match the authenticated account".into());
        }
        required_non_empty(context.get("deviceId"), "messaging deviceId")?;
        required_non_empty(context.get("sessionId"), "messaging sessionId")?;

        let command = envelope
            .get("command")
            .and_then(Value::as_object)
            .ok_or("messaging command is required")?;
        let command_type = command
            .get("type")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or("messaging command type is required")?;

        let previous = self.state.clone();
        let result = match self.execute_command(command_type, command, authenticated_actor_id, now_ms) {
            Ok(envelopes) => json!({"requestId":request_id,"envelopes":envelopes}),
            Err(error) => {
                self.state = previous;
                return Err(error);
            }
        };

        self.remember_result(replay_key, result.clone());
        if let Err(error) = self.persist() {
            self.state = previous;
            return Err(format!("failed to persist canonical Android messaging repository: {error}"));
        }
        Ok(result)
    }

    fn execute_command(
        &mut self,
        command_type: &str,
        command: &Map<String, Value>,
        actor_id: &str,
        now_ms: i64,
    ) -> Result<Vec<Value>, String> {
        match command_type {
            "sync" => Ok(vec![self.sync_envelope(actor_id, now_ms)]),
            "upsertProfile" => self.upsert_profile(command, actor_id, now_ms),
            "createConversation" => self.create_conversation(command, actor_id, now_ms),
            "sendMessage" => self.send_message(command, actor_id, now_ms),
            "markConversationChildRead" => {
                let destination = parse_destination(command.get("destination"))?;
                self.validate_child_destination(&destination, actor_id)?;
                let message_id = required_non_empty(command.get("messageId"), "message id")?;
                let message = self.require_child_message(&destination, message_id)?;
                let position = ConversationMessagePosition {
                    created_at_ms: message
                        .get("createdAtMs")
                        .and_then(Value::as_i64)
                        .unwrap_or_default(),
                    message_id: message_id.to_string(),
                };
                self.children
                    .upsert_with(destination, actor_id, |state| {
                        if state.advance_inbox_read_till(position, None) {
                            Ok(())
                        } else {
                            Err("conversation child read position moved backwards")
                        }
                    })
                    .map_err(str::to_string)?;
                self.bump_cursor();
                Ok(Vec::new())
            }
            "setConversationChildDraft" => {
                let destination = parse_destination(command.get("destination"))?;
                self.validate_child_destination(&destination, actor_id)?;
                let text = command.get("text").and_then(Value::as_str).unwrap_or("").to_string();
                let reply_to = optional_string(command.get("replyToMessageId"));
                if let Some(reply_to) = reply_to.as_deref() {
                    self.require_message(destination_message_conversation_id(&destination), reply_to)?;
                }
                self.children
                    .upsert_with(destination, actor_id, move |state| {
                        if text.trim().is_empty() && reply_to.is_none() {
                            state.clear_draft();
                        } else {
                            state.set_draft(text, reply_to, now_ms);
                        }
                        Ok(())
                    })
                    .map_err(str::to_string)?;
                self.bump_cursor();
                Ok(Vec::new())
            }
            "replaceConversationChildWindow" => {
                let destination = parse_destination(command.get("destination"))?;
                self.validate_child_destination(&destination, actor_id)?;
                let message_ids = command
                    .get("messageIds")
                    .and_then(Value::as_array)
                    .ok_or("conversation child messageIds array is required")?
                    .iter()
                    .map(|value| {
                        value
                            .as_str()
                            .filter(|value| bounded_id(value))
                            .map(str::to_string)
                            .ok_or_else(|| "conversation child message id is invalid".to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if matches!(
                    &destination.child,
                    Some(ConversationChildIdentity::SavedSublist { .. })
                ) && !message_ids.is_empty()
                {
                    return Err(
                        "SavedSublist message membership is not source-neutrally provable; non-empty window rejected"
                            .into(),
                    );
                }
                for message_id in &message_ids {
                    self.require_child_message(&destination, message_id)?;
                }
                let skipped_before = optional_u32(command.get("skippedBefore"))?;
                let skipped_after = optional_u32(command.get("skippedAfter"))?;
                let full_count = optional_u32(command.get("fullCount"))?;
                let mut pagination = ConversationChildPaginationState::default();
                if !pagination.replace_window(
                    message_ids.clone(),
                    skipped_before,
                    skipped_after,
                    full_count,
                ) {
                    return Err("invalid conversation child pagination window".into());
                }
                self.children
                    .upsert_with(destination, actor_id, move |state| {
                        state.pagination = pagination;
                        if message_ids.is_empty() {
                            state.note_locally_empty();
                        } else {
                            state.note_non_empty();
                        }
                        Ok(())
                    })
                    .map_err(str::to_string)?;
                self.bump_cursor();
                Ok(Vec::new())
            }
            "setConversationChildPinned" => {
                let destination = parse_destination(command.get("destination"))?;
                self.validate_child_destination(&destination, actor_id)?;
                let pinned = required_bool(command.get("pinned"), "conversation child pinned")?;
                self.children
                    .upsert_with(destination, actor_id, |state| {
                        state.pinned = pinned;
                        if pinned {
                            state.restore_pinned_when_non_empty = false;
                        }
                        Ok(())
                    })
                    .map_err(str::to_string)?;
                self.bump_cursor();
                Ok(Vec::new())
            }
            "setConversationChildActive" => {
                let destination = parse_destination(command.get("destination"))?;
                self.validate_child_destination(&destination, actor_id)?;
                let active = required_bool(command.get("active"), "conversation child active")?;
                let parent = destination.conversation_id.clone();
                if active {
                    let siblings = self.children.states_for_actor(actor_id);
                    for sibling in siblings {
                        if sibling.destination.conversation_id == parent
                            && sibling.destination != destination
                            && sibling.active
                        {
                            self.children
                                .upsert_with(sibling.destination, actor_id, |state| {
                                    state.active = false;
                                    Ok(())
                                })
                                .map_err(str::to_string)?;
                        }
                    }
                }
                self.children
                    .upsert_with(destination, actor_id, |state| {
                        state.active = active;
                        Ok(())
                    })
                    .map_err(str::to_string)?;
                self.bump_cursor();
                Ok(Vec::new())
            }
            "setConversationChildMarkedUnread" => {
                let destination = parse_destination(command.get("destination"))?;
                self.validate_child_destination(&destination, actor_id)?;
                let marked_unread =
                    required_bool(command.get("markedUnread"), "conversation child markedUnread")?;
                let parent = self
                    .state
                    .conversations
                    .get(&destination.conversation_id.0)
                    .ok_or("conversation child target does not exist")?;
                let current = self
                    .children
                    .states_for_actor(actor_id)
                    .into_iter()
                    .find(|state| state.destination == destination);
                let currently_unread = current
                    .as_ref()
                    .is_some_and(|state| state.marked_unread || state.unread_count.unwrap_or(0) > 0);
                let context = crate::messaging_child::ConversationChildUnreadContext {
                    parent_is_self: conversation_kind(parent) == Some("savedMessages")
                        && conversation_owner(parent) == Some(actor_id),
                    parent_is_community: matches!(
                        conversation_kind(parent),
                        Some("group") | Some("channel")
                    ),
                    actor_is_monoforum_admin: false,
                };
                if !destination.can_toggle_unread(currently_unread, context) {
                    return Err("conversation child unread toggle is not allowed".into());
                }
                self.children
                    .upsert_with(destination, actor_id, |state| {
                        state.marked_unread = marked_unread;
                        Ok(())
                    })
                    .map_err(str::to_string)?;
                self.bump_cursor();
                Ok(Vec::new())
            }
            "setConversationChildNoPaidMessages" => {
                let destination = parse_destination(command.get("destination"))?;
                self.validate_child_destination(&destination, actor_id)?;
                if !matches!(
                    &destination.child,
                    Some(ConversationChildIdentity::SavedSublist { .. })
                ) {
                    return Err("noPaidMessages is only valid for SavedSublist".into());
                }
                let no_paid_messages = required_bool(
                    command.get("noPaidMessages"),
                    "conversation child noPaidMessages",
                )?;
                self.children
                    .upsert_with(destination, actor_id, |state| {
                        state.no_paid_messages = no_paid_messages;
                        Ok(())
                    })
                    .map_err(str::to_string)?;
                self.bump_cursor();
                Ok(Vec::new())
            }
            "destroyConversationChild" => {
                let destination = parse_destination(command.get("destination"))?;
                self.validate_child_destination(&destination, actor_id)?;
                self.children
                    .destroy_exact(&destination, actor_id)
                    .map_err(str::to_string)?;
                self.bump_cursor();
                Ok(Vec::new())
            }
            other => Err(format!(
                "messaging command {other} is not yet migrated into the canonical Android repository"
            )),
        }
    }

    fn upsert_profile(
        &mut self,
        command: &Map<String, Value>,
        actor_id: &str,
        now_ms: i64,
    ) -> Result<Vec<Value>, String> {
        let actor = required_object(command.get("actor"), "messaging actor")?.clone();
        let target_id = required_non_empty(actor.get("id"), "messaging actor id")?;
        if target_id != actor_id {
            return Err("profile actor id does not match authenticated actor".into());
        }
        self.state
            .actors
            .insert(target_id.to_string(), Value::Object(actor.clone()));
        self.bump_cursor();
        Ok(vec![self.server_envelope(
            json!({"type":"actorChanged","actor":Value::Object(actor)}),
            now_ms,
        )])
    }

    fn create_conversation(
        &mut self,
        command: &Map<String, Value>,
        actor_id: &str,
        now_ms: i64,
    ) -> Result<Vec<Value>, String> {
        let conversation = required_object(command.get("conversation"), "conversation")?.clone();
        let conversation_id = required_non_empty(conversation.get("id"), "conversation id")?;
        if conversation
            .get("ownerId")
            .and_then(Value::as_str)
            .is_some_and(|owner| owner != actor_id)
            || !conversation_has_access(&Value::Object(conversation.clone()), actor_id)
        {
            return Err("conversation creator must be its owner/participant".into());
        }
        self.state.conversations.insert(
            conversation_id.to_string(),
            Value::Object(conversation.clone()),
        );
        self.bump_cursor();
        Ok(vec![self.server_envelope(
            json!({"type":"conversationChanged","conversation":Value::Object(conversation)}),
            now_ms,
        )])
    }

    fn send_message(
        &mut self,
        command: &Map<String, Value>,
        actor_id: &str,
        now_ms: i64,
    ) -> Result<Vec<Value>, String> {
        let conversation_id = required_non_empty(command.get("conversationId"), "conversation id")?;
        let conversation = self
            .state
            .conversations
            .get(conversation_id)
            .ok_or("conversation does not exist")?;
        if !conversation_has_access(conversation, actor_id) {
            return Err("send message requires conversation membership".into());
        }
        let client_message_id =
            required_non_empty(command.get("clientMessageId"), "clientMessageId")?;
        if !bounded_id(client_message_id) {
            return Err("clientMessageId is invalid".into());
        }
        if let Some(existing) = self
            .state
            .messages
            .get(conversation_id)
            .and_then(|messages| messages.get(client_message_id))
            .cloned()
        {
            return Ok(vec![self.server_envelope(
                json!({"type":"messageAdded","message":existing}),
                now_ms,
            )]);
        }
        let content = command
            .get("content")
            .filter(|value| value.is_object())
            .cloned()
            .ok_or("message content is required")?;
        let message = json!({
            "id":client_message_id,
            "clientMessageId":client_message_id,
            "conversationId":conversation_id,
            "senderId":actor_id,
            "content":content,
            "replyToMessageId":command.get("replyToMessageId").cloned().unwrap_or(Value::Null),
            "threadRootMessageId":command.get("threadRootMessageId").cloned().unwrap_or(Value::Null),
            "createdAtMs":now_ms,
            "editedAtMs":Value::Null,
            "scheduledAtMs":command.get("scheduledAtMs").cloned().unwrap_or(Value::Null),
            "silent":command.get("silent").and_then(Value::as_bool).unwrap_or(false),
            "protectedContent":command.get("protectedContent").and_then(Value::as_bool).unwrap_or(false),
            "deliveryState":"sent",
            "reactions":[],
            "pinned":false,
            "deleted":false
        });
        self.state
            .messages
            .entry(conversation_id.to_string())
            .or_default()
            .insert(client_message_id.to_string(), message.clone());
        self.bump_cursor();
        Ok(vec![self.server_envelope(
            json!({"type":"messageAdded","message":message}),
            now_ms,
        )])
    }

    fn validate_child_destination(
        &self,
        destination: &ConversationDestination,
        actor_id: &str,
    ) -> Result<(), String> {
        if !destination.is_valid() {
            return Err("invalid conversation child destination".into());
        }
        let parent = self
            .state
            .conversations
            .get(&destination.conversation_id.0)
            .ok_or("conversation child target does not exist")?;
        if !conversation_has_access(parent, actor_id) {
            return Err("conversation child update requires canonical membership".into());
        }
        match destination.child.as_ref() {
            Some(ConversationChildIdentity::Topic { root_message_id }) => {
                if self
                    .state
                    .messages
                    .get(&destination.conversation_id.0)
                    .and_then(|messages| messages.get(root_message_id))
                    .is_none()
                {
                    return Err("topic root message does not exist".into());
                }
            }
            Some(ConversationChildIdentity::SavedSublist { participant_id }) => {
                if conversation_kind(parent) != Some("savedMessages")
                    || conversation_owner(parent) != Some(actor_id)
                    || !self.state.actors.contains_key(participant_id)
                {
                    return Err("invalid SavedSublist destination".into());
                }
            }
            Some(ConversationChildIdentity::Conversation { conversation_id }) => {
                let child = self
                    .state
                    .conversations
                    .get(&conversation_id.0)
                    .ok_or("nested conversation child does not exist")?;
                if !conversation_has_access(child, actor_id) {
                    return Err("nested conversation child requires canonical membership".into());
                }
            }
            None => {}
        }
        Ok(())
    }

    fn require_child_message(
        &self,
        destination: &ConversationDestination,
        message_id: &str,
    ) -> Result<&Value, String> {
        let message = self.require_message(destination_message_conversation_id(destination), message_id)?;
        if let Some(ConversationChildIdentity::Topic { root_message_id }) =
            destination.child.as_ref()
        {
            let belongs = message.get("threadRootMessageId").and_then(Value::as_str)
                == Some(root_message_id.as_str())
                || message.get("id").and_then(Value::as_str) == Some(root_message_id.as_str());
            if !belongs {
                return Err("message does not belong to requested topic child".into());
            }
        }
        Ok(message)
    }

    fn require_message(&self, conversation_id: &str, message_id: &str) -> Result<&Value, String> {
        self.state
            .messages
            .get(conversation_id)
            .and_then(|messages| messages.get(message_id))
            .ok_or_else(|| format!("message {message_id} does not exist in {conversation_id}"))
    }

    fn sync_envelope(&self, actor_id: &str, now_ms: i64) -> Value {
        let visible_ids = self
            .state
            .conversations
            .iter()
            .filter(|(_, conversation)| conversation_has_access(conversation, actor_id))
            .map(|(id, _)| id.clone())
            .collect::<BTreeSet<_>>();
        let conversations = visible_ids
            .iter()
            .filter_map(|id| self.state.conversations.get(id))
            .cloned()
            .collect::<Vec<_>>();
        let messages = visible_ids
            .iter()
            .filter_map(|id| self.state.messages.get(id))
            .flat_map(|messages| messages.values().cloned())
            .collect::<Vec<_>>();
        let actors = self
            .state
            .actors
            .iter()
            .filter(|(id, _)| {
                *id == actor_id
                    || conversations.iter().any(|conversation| {
                        conversation
                            .get("participants")
                            .and_then(Value::as_array)
                            .is_some_and(|participants| {
                                participants.iter().any(|participant| {
                                    participant.get("actorId").and_then(Value::as_str)
                                        == Some(id.as_str())
                                })
                            })
                    })
            })
            .map(|(_, actor)| actor.clone())
            .collect::<Vec<_>>();
        let children = self
            .children
            .states_for_actor(actor_id)
            .into_iter()
            .filter(|child| visible_ids.contains(&child.destination.conversation_id.0))
            .collect::<Vec<_>>();
        self.server_envelope(
            json!({
                "type":"syncBatch",
                "actors":actors,
                "conversations":conversations,
                "messages":messages,
                "folders":[],
                "drafts":[],
                "topicDrafts":[],
                "pendingPresenceSends":[],
                "conversationChildren":children,
                "invoices":[],
                "orders":[],
                "stories":[],
                "communities":[],
                "bots":[],
                "botExecutions":[],
                "miniApps":[],
                "nextCursor":self.state.cursor.to_string()
            }),
            now_ms,
        )
    }

    fn server_envelope(&self, event: Value, now_ms: i64) -> Value {
        json!({
            "protocolVersion":MESSAGING_PROTOCOL_VERSION,
            "cursor":self.state.cursor.to_string(),
            "serverTimeMs":now_ms,
            "event":event
        })
    }

    fn bump_cursor(&mut self) {
        self.state.cursor = self.state.cursor.saturating_add(1);
    }

    fn remember_result(&mut self, key: String, result: Value) {
        if !self.state.request_results.contains_key(&key) {
            self.state.request_order.push(key.clone());
        }
        self.state.request_results.insert(key, result);
        while self.state.request_order.len() > MAX_REPLAY_RESULTS {
            if let Some(oldest) = self.state.request_order.first().cloned() {
                self.state.request_order.remove(0);
                self.state.request_results.remove(&oldest);
            }
        }
    }

    fn persist(&self) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = self.path.with_extension("json.tmp");
        let bytes = serde_json::to_vec(&self.state)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        {
            let mut file = File::create(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
        }
        fs::rename(&temporary, &self.path)?;
        if let Some(parent) = self.path.parent() {
            if let Ok(directory) = File::open(parent) {
                let _ = directory.sync_all();
            }
        }
        Ok(())
    }
}

fn validate_loaded_state(state: &MessagingRepositoryState) -> io::Result<()> {
    if state
        .request_order
        .iter()
        .any(|key| !state.request_results.contains_key(key))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "messaging request replay index is inconsistent",
        ));
    }
    Ok(())
}

fn parse_destination(value: Option<&Value>) -> Result<ConversationDestination, String> {
    let destination: ConversationDestination = serde_json::from_value(
        value
            .cloned()
            .ok_or("conversation child destination is required")?,
    )
    .map_err(|error| format!("invalid conversation child destination: {error}"))?;
    if !destination.is_valid() {
        return Err("invalid conversation child destination".into());
    }
    Ok(destination)
}

fn destination_message_conversation_id(destination: &ConversationDestination) -> &str {
    match destination.child.as_ref() {
        Some(ConversationChildIdentity::Conversation { conversation_id }) => &conversation_id.0,
        _ => &destination.conversation_id.0,
    }
}

fn conversation_has_access(conversation: &Value, actor_id: &str) -> bool {
    conversation_owner(conversation) == Some(actor_id)
        || conversation
            .get("participants")
            .and_then(Value::as_array)
            .is_some_and(|participants| {
                participants.iter().any(|participant| {
                    participant.get("actorId").and_then(Value::as_str) == Some(actor_id)
                })
            })
}

fn conversation_owner(conversation: &Value) -> Option<&str> {
    conversation.get("ownerId").and_then(Value::as_str)
}

fn conversation_kind(conversation: &Value) -> Option<&str> {
    conversation.get("kind").and_then(Value::as_str)
}

fn required_object<'a>(
    value: Option<&'a Value>,
    label: &str,
) -> Result<&'a Map<String, Value>, String> {
    value
        .and_then(Value::as_object)
        .ok_or_else(|| format!("{label} object is required"))
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{key} is required"))
}

fn required_non_empty<'a>(value: Option<&'a Value>, label: &str) -> Result<&'a str, String> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{label} is required"))
}

fn required_bool(value: Option<&Value>, label: &str) -> Result<bool, String> {
    value
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("{label} is required"))
}

fn optional_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

fn optional_u32(value: Option<&Value>) -> Result<Option<u32>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .map(Some)
            .ok_or_else(|| "conversation child pagination count is invalid".to_string()),
    }
}

fn bounded_id(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty() && value.len() <= 200
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "fabushi-android-messaging-service-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn envelope(request_id: &str, actor_id: &str, command: Value) -> Value {
        json!({
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
                "command":command
            }
        })
    }

    fn execute(
        service: &mut AndroidMessagingService,
        request_id: &str,
        actor_id: &str,
        command: Value,
    ) -> Result<Value, String> {
        service.execute(&envelope(request_id, actor_id, command), actor_id, 100)
    }

    fn seed(service: &mut AndroidMessagingService, actor: &str) {
        execute(
            service,
            "profile",
            actor,
            json!({"type":"upsertProfile","actor":{"id":actor,"kind":"human","displayName":"Owner"}}),
        )
        .unwrap();
        execute(
            service,
            "conversation",
            actor,
            json!({"type":"createConversation","conversation":{
                "id":"conversation:parent",
                "kind":"group",
                "title":"Parent",
                "ownerId":actor,
                "participants":[{"actorId":actor,"role":"owner","joinedAtMs":1}]
            }}),
        )
        .unwrap();
        execute(
            service,
            "root-message",
            actor,
            json!({"type":"sendMessage","conversationId":"conversation:parent","clientMessageId":"message:root",
                "content":{"type":"text","data":{"text":{"text":"root","entities":[]}}}}),
        )
        .unwrap();
    }

    #[test]
    fn child_mutation_requires_canonical_parent_membership_and_survives_reopen() {
        let root = temp_root("child");
        let actor = "human:owner";
        let destination = json!({
            "conversationId":"conversation:parent",
            "child":{"kind":"topic","rootMessageId":"message:root"}
        });
        {
            let mut service = AndroidMessagingService::open(&root).unwrap();
            seed(&mut service, actor);
            execute(
                &mut service,
                "pin",
                actor,
                json!({"type":"setConversationChildPinned","destination":destination.clone(),"pinned":true}),
            )
            .unwrap();
            let outsider = service.execute(
                &envelope(
                    "outsider",
                    "human:outsider",
                    json!({"type":"setConversationChildPinned","destination":destination.clone(),"pinned":false}),
                ),
                "human:outsider",
                101,
            );
            assert!(outsider.is_err());
        }
        let mut reopened = AndroidMessagingService::open(&root).unwrap();
        let sync = execute(&mut reopened, "sync-reopen", actor, json!({"type":"sync"})).unwrap();
        let children = sync["envelopes"][0]["event"]["conversationChildren"]
            .as_array()
            .unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0]["pinned"], true);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn duplicate_request_is_replayed_without_second_cursor_advance() {
        let root = temp_root("dedupe");
        let actor = "human:owner";
        let mut service = AndroidMessagingService::open(&root).unwrap();
        seed(&mut service, actor);
        let request = envelope(
            "same",
            actor,
            json!({"type":"setConversationChildPinned","destination":{
                "conversationId":"conversation:parent",
                "child":{"kind":"topic","rootMessageId":"message:root"}
            },"pinned":true}),
        );
        let first = service.execute(&request, actor, 100).unwrap();
        let cursor = service.state.cursor;
        let second = service.execute(&request, actor, 200).unwrap();
        assert_eq!(first, second);
        assert_eq!(service.state.cursor, cursor);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn stale_account_actor_is_rejected_before_mutation() {
        let root = temp_root("account-fence");
        let actor = "human:owner";
        let mut service = AndroidMessagingService::open(&root).unwrap();
        seed(&mut service, actor);
        let request = envelope(
            "stale",
            actor,
            json!({"type":"setConversationChildPinned","destination":{
                "conversationId":"conversation:parent",
                "child":{"kind":"topic","rootMessageId":"message:root"}
            },"pinned":true}),
        );
        assert!(service
            .execute(&request, "human:new-account", 100)
            .is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn corrupt_repository_fails_closed() {
        let root = temp_root("corrupt");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("messaging-repository.json"), b"{broken").unwrap();
        let error = AndroidMessagingService::open(&root).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        let _ = fs::remove_dir_all(root);
    }
}
