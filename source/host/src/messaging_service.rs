use crate::{
    messaging_blob::{BlobId, BlobMetadata, FileBlobStore},
    messaging_child::{
        ConversationChildIdentity, ConversationChildPaginationState, ConversationChildStore,
        ConversationDestination, ConversationMessagePosition,
    },
};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
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
    drafts: BTreeMap<String, BTreeMap<String, Value>>,
    #[serde(default)]
    folders: BTreeMap<String, BTreeMap<String, Value>>,
    #[serde(default)]
    conversation_preferences: BTreeMap<String, BTreeMap<String, Value>>,
    #[serde(default)]
    reaction_actors: BTreeMap<String, BTreeMap<String, BTreeMap<String, BTreeSet<String>>>>,
    #[serde(default)]
    poll_votes: BTreeMap<String, BTreeMap<String, BTreeMap<String, BTreeSet<String>>>>,
    #[serde(default)]
    request_results: BTreeMap<String, Value>,
    #[serde(default)]
    request_order: Vec<String>,
    #[serde(default)]
    agent_delivery_calls: BTreeMap<String, Value>,
    #[serde(default)]
    agent_delivery_call_order: Vec<String>,
    #[serde(default)]
    agent_wakes: BTreeMap<String, Value>,
}

impl Default for MessagingRepositoryState {
    fn default() -> Self {
        Self {
            schema_version: MESSAGING_REPOSITORY_SCHEMA_VERSION,
            cursor: 0,
            actors: BTreeMap::new(),
            conversations: BTreeMap::new(),
            messages: BTreeMap::new(),
            drafts: BTreeMap::new(),
            folders: BTreeMap::new(),
            conversation_preferences: BTreeMap::new(),
            reaction_actors: BTreeMap::new(),
            poll_votes: BTreeMap::new(),
            request_results: BTreeMap::new(),
            request_order: Vec::new(),
            agent_delivery_calls: BTreeMap::new(),
            agent_delivery_call_order: Vec::new(),
            agent_wakes: BTreeMap::new(),
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
    blobs: FileBlobStore,
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
        let blobs = FileBlobStore::new(app_data_dir.join("messaging-blobs"));
        Ok(Self {
            path,
            state,
            children,
            blobs,
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
            "beginBlobUpload" => self.begin_blob_upload(command, actor_id),
            "appendBlobChunk" => self.append_blob_chunk(command, actor_id),
            "finishBlobUpload" => self.finish_blob_upload(command, actor_id),
            "deleteBlob" => self.delete_blob(command, actor_id),
            "editMessage" => self.edit_message(command, actor_id, now_ms),
            "deleteMessages" => self.delete_messages(command, actor_id, now_ms),
            "markRead" => self.mark_read(command, actor_id, now_ms),
            "setMarkedUnread" => self.set_marked_unread(command, actor_id, now_ms),
            "setDraft" => self.set_draft(command, actor_id, now_ms),
            "setConversationNotifications" => {
                self.set_conversation_notifications(command, actor_id, now_ms)
            }
            "upsertFolder" => self.upsert_folder(command, actor_id, now_ms),
            "deleteFolder" => self.delete_folder(command, actor_id, now_ms),
            "archiveConversation" => self.set_conversation_flag(
                command,
                actor_id,
                now_ms,
                "archived",
                "archived",
            ),
            "pinConversation" => self.set_conversation_flag(
                command,
                actor_id,
                now_ms,
                "pinned",
                "pinned",
            ),
            "updateConversationInfo" => self.update_conversation_info(command, actor_id, now_ms),
            "setConversationParticipant" => {
                self.set_conversation_participant(command, actor_id, now_ms)
            }
            "removeConversationParticipant" => {
                self.remove_conversation_participant(command, actor_id, now_ms)
            }
            "forwardMessage" => self.forward_message(command, actor_id, now_ms),
            "setReaction" => self.set_reaction(command, actor_id, now_ms),
            "pinMessage" => self.pin_message(command, actor_id, now_ms),
            "votePoll" => self.vote_poll(command, actor_id, now_ms),
            "startTyping" => self.typing(command, actor_id, now_ms, true),
            "stopTyping" => self.typing(command, actor_id, now_ms, false),
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
        let conversation = self.require_conversation_access(conversation_id, actor_id)?.clone();
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
                json!({"type":"messageAdded","message":self.project_message(&existing, actor_id)}),
                now_ms,
            )]);
        }
        let content = command
            .get("content")
            .filter(|value| value.is_object())
            .cloned()
            .ok_or("message content is required")?;
        self.validate_content_for_send(&conversation, &content, actor_id)?;
        if let Some(reply_to) = optional_string(command.get("replyToMessageId")) {
            self.require_message(conversation_id, &reply_to)?;
        }
        if let Some(thread_root) = optional_string(command.get("threadRootMessageId")) {
            self.require_message(conversation_id, &thread_root)?;
        }
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
        let mut updated_conversation = conversation;
        if let Some(object) = updated_conversation.as_object_mut() {
            object.insert("lastMessageId".into(), Value::String(client_message_id.to_string()));
            object.insert("updatedAtMs".into(), json!(now_ms));
        }
        self.state
            .conversations
            .insert(conversation_id.to_string(), updated_conversation);
        self.bump_cursor();
        Ok(vec![self.server_envelope(
            json!({"type":"messageAdded","message":self.project_message(&message, actor_id)}),
            now_ms,
        )])
    }

    /// Durable Agent-to-Agent/group delivery through the canonical messaging owner.
    ///
    /// The caller owns tool-call argument idempotency. This repository owns the
    /// actual delivered message and replays the same stable message id after
    /// process death rather than reissuing the side effect.
    pub fn deliver_agent_message(
        &mut self,
        account_fence: &str,
        sender_id: &str,
        target_id: &str,
        target_is_group: bool,
        message: &str,
        images: &[Value],
        priority: bool,
        tool_call_id: &str,
        now_ms: i64,
    ) -> Result<Value, String> {
        let account_fence = account_fence.trim();
        let sender_id = sender_id.trim();
        let target_id = target_id.trim();
        let message = message.trim();
        let tool_call_id = tool_call_id.trim();
        if account_fence.is_empty() || sender_id.is_empty() || target_id.is_empty()
            || message.is_empty() || tool_call_id.is_empty()
        {
            return Err("agent delivery requires account, sender, target, message and tool-call identity".into());
        }
        if sender_id == target_id {
            return Err("agent delivery cannot target the sending agent".into());
        }

        let replay_key = format!(
            "agent-delivery:{}",
            crate::sha256::sha256_hex(
                format!("{account_fence}\n{sender_id}\n{tool_call_id}").as_bytes()
            )
        );
        let replay_args = json!({
            "senderId":sender_id,
            "targetId":target_id,
            "targetIsGroup":target_is_group,
            "message":message,
            "images":images,
            "priority":priority
        });
        if let Some(call) = self.state.agent_delivery_calls.get(&replay_key) {
            if call.get("args") != Some(&replay_args) {
                return Err("SendToAgent tool_call_id was reused with mismatched arguments".into());
            }
            return call
                .get("result")
                .cloned()
                .ok_or_else(|| "durable Agent delivery replay result is missing".to_string());
        }

        let previous = self.state.clone();
        let conversation_id = if target_is_group {
            let conversation = self
                .state
                .conversations
                .get(target_id)
                .ok_or("target Agent group has no canonical messaging conversation")?;
            if !conversation_has_access(conversation, sender_id) {
                return Err("sending Agent is not a member of the target group".into());
            }
            target_id.to_string()
        } else {
            let mut pair = [sender_id, target_id];
            pair.sort_unstable();
            let digest = crate::sha256::sha256_hex(
                format!("{account_fence}\n{}\n{}", pair[0], pair[1]).as_bytes(),
            );
            let conversation_id = format!("agent-direct:{}", &digest[..32]);
            self.state.conversations.entry(conversation_id.clone()).or_insert_with(|| {
                json!({
                    "id":conversation_id,
                    "kind":"direct",
                    "ownerId":sender_id,
                    "participants":[
                        {"actorId":sender_id,"role":"owner","joinedAtMs":now_ms},
                        {"actorId":target_id,"role":"member","joinedAtMs":now_ms}
                    ],
                    "permissions":{
                        "canSendMessages":true,
                        "canSendMedia":true
                    },
                    "updatedAtMs":now_ms
                })
            });
            conversation_id
        };

        let message_id = format!(
            "agent-message:{}",
            &crate::sha256::sha256_hex(
                format!("{account_fence}\n{sender_id}\n{target_id}\n{tool_call_id}").as_bytes()
            )[..32]
        );
        if self
            .state
            .messages
            .get(&conversation_id)
            .and_then(|messages| messages.get(&message_id))
            .is_none()
        {
            let content = json!({
                "type":"text",
                "data":{
                    "text":{"text":message,"entities":[]},
                    "agentImages":images,
                    "agentPriority": if target_is_group { false } else { priority }
                }
            });
            let conversation = self
                .state
                .conversations
                .get(&conversation_id)
                .cloned()
                .ok_or("canonical Agent conversation disappeared")?;
            self.validate_content_for_send(&conversation, &content, sender_id)?;
            let delivered = json!({
                "id":message_id,
                "clientMessageId":message_id,
                "conversationId":conversation_id,
                "senderId":sender_id,
                "content":content,
                "createdAtMs":now_ms,
                "editedAtMs":Value::Null,
                "scheduledAtMs":Value::Null,
                "silent":false,
                "protectedContent":false,
                "deliveryState":"sent",
                "reactions":[],
                "pinned":false,
                "deleted":false
            });
            self.state
                .messages
                .entry(conversation_id.clone())
                .or_default()
                .insert(message_id.clone(), delivered);
            if let Some(conversation) = self.state.conversations.get_mut(&conversation_id) {
                if let Some(object) = conversation.as_object_mut() {
                    object.insert("lastMessageId".into(), Value::String(message_id.clone()));
                    object.insert("updatedAtMs".into(), json!(now_ms));
                }
            }
            self.bump_cursor();

            let recipients = if target_is_group {
                conversation
                    .get("participants")
                    .and_then(Value::as_array)
                    .map(|participants| {
                        participants
                            .iter()
                            .filter_map(|participant| participant.get("actorId").and_then(Value::as_str))
                            .filter(|actor_id| *actor_id != sender_id)
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            } else {
                vec![target_id.to_string()]
            };
            for recipient_id in recipients {
                let wake_id = format!(
                    "agent-wake:{}",
                    &crate::sha256::sha256_hex(
                        format!("{account_fence}\n{message_id}\n{recipient_id}").as_bytes()
                    )[..32]
                );
                self.state.agent_wakes.entry(wake_id.clone()).or_insert_with(|| {
                    json!({
                        "wakeId":wake_id,
                        "accountFence":account_fence,
                        "targetAgentId":recipient_id,
                        "sourceAgentId":sender_id,
                        "conversationId":conversation_id,
                        "messageId":message_id,
                        "message":message,
                        "images":images,
                        "priority": if target_is_group { false } else { priority },
                        "createdAtMs":now_ms,
                        "attempts":0,
                        "nextAttemptAtMs":now_ms,
                        "lastError":Value::Null
                    })
                });
            }
        }

        let result = json!({
            "status":"delivered",
            "conversationId":conversation_id,
            "messageId":message_id,
            "targetId":target_id,
            "priorityApplied":!target_is_group && priority,
            "deliveryMode":"async-new-turn"
        });
        self.state.agent_delivery_calls.insert(
            replay_key.clone(),
            json!({"args":replay_args,"result":result.clone()}),
        );
        self.state.agent_delivery_call_order.retain(|key| key != &replay_key);
        self.state.agent_delivery_call_order.push(replay_key.clone());
        while self.state.agent_delivery_call_order.len() > MAX_REPLAY_RESULTS {
            if let Some(expired) = self.state.agent_delivery_call_order.first().cloned() {
                self.state.agent_delivery_call_order.remove(0);
                self.state.agent_delivery_calls.remove(&expired);
            }
        }
        if let Err(error) = self.persist() {
            self.state = previous;
            return Err(format!("failed to persist Agent delivery: {error}"));
        }
        Ok(result)
    }

    /// Project canonical one-to-one Agent conversation relationships for the current account.
    ///
    /// Relationships are derived from the durable messaging repository itself. The account fence is
    /// verified by recomputing the stable direct-conversation identity, so another account's
    /// conversation cannot leak into this projection even though all conversations share one store.
    pub fn agent_conversation_partner_ids(
        &self,
        account_fence: &str,
        agent_id: &str,
    ) -> Vec<String> {
        let account_fence = account_fence.trim();
        let agent_id = agent_id.trim();
        if account_fence.is_empty() || agent_id.is_empty() {
            return Vec::new();
        }
        let mut partners = BTreeSet::new();
        for (conversation_id, conversation) in &self.state.conversations {
            if conversation.get("kind").and_then(Value::as_str) != Some("direct") {
                continue;
            }
            let Some(participants) = conversation.get("participants").and_then(Value::as_array) else {
                continue;
            };
            let actor_ids = participants
                .iter()
                .filter_map(|participant| participant.get("actorId").and_then(Value::as_str))
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .collect::<BTreeSet<_>>();
            if actor_ids.len() != 2 || !actor_ids.contains(agent_id) {
                continue;
            }
            let Some(partner_id) = actor_ids.iter().copied().find(|value| *value != agent_id) else {
                continue;
            };
            let mut pair = [agent_id, partner_id];
            pair.sort_unstable();
            let digest = crate::sha256::sha256_hex(
                format!("{account_fence}\n{}\n{}", pair[0], pair[1]).as_bytes(),
            );
            let expected_id = format!("agent-direct:{}", &digest[..32]);
            if conversation_id == &expected_id {
                partners.insert(partner_id.to_string());
            }
        }
        partners.into_iter().collect()
    }

    pub fn pending_agent_wakes(
        &self,
        account_fence: &str,
        now_ms: i64,
    ) -> Vec<Value> {
        let mut wakes = self
            .state
            .agent_wakes
            .values()
            .filter(|wake| wake.get("accountFence").and_then(Value::as_str) == Some(account_fence))
            .filter(|wake| wake.get("nextAttemptAtMs").and_then(Value::as_i64).unwrap_or(0) <= now_ms)
            .cloned()
            .collect::<Vec<_>>();
        wakes.sort_by_key(|wake| wake.get("createdAtMs").and_then(Value::as_i64).unwrap_or(0));
        wakes
    }

    pub fn complete_agent_wake(&mut self, wake_id: &str) -> Result<(), String> {
        let previous = self.state.clone();
        if self.state.agent_wakes.remove(wake_id).is_none() {
            return Ok(());
        }
        if let Err(error) = self.persist() {
            self.state = previous;
            return Err(format!("failed to persist completed Agent wake: {error}"));
        }
        Ok(())
    }

    pub fn defer_agent_wake(
        &mut self,
        wake_id: &str,
        now_ms: i64,
        reason: &str,
    ) -> Result<(), String> {
        let previous = self.state.clone();
        let wake = self
            .state
            .agent_wakes
            .get_mut(wake_id)
            .ok_or_else(|| "Agent wake is missing".to_string())?;
        let attempts = wake.get("attempts").and_then(Value::as_u64).unwrap_or(0).saturating_add(1);
        let delay_ms = (5_000_u64.saturating_mul(1_u64 << attempts.min(5))).min(120_000);
        let object = wake
            .as_object_mut()
            .ok_or_else(|| "Agent wake record is invalid".to_string())?;
        object.insert("attempts".into(), json!(attempts));
        object.insert(
            "nextAttemptAtMs".into(),
            json!(now_ms.saturating_add(i64::try_from(delay_ms).unwrap_or(i64::MAX))),
        );
        object.insert("lastError".into(), Value::String(reason.to_string()));
        if let Err(error) = self.persist() {
            self.state = previous;
            return Err(format!("failed to persist deferred Agent wake: {error}"));
        }
        Ok(())
    }

    pub fn read_blob_range(
        &self,
        params: &Value,
        actor_id: &str,
    ) -> Result<Value, String> {
        let id = BlobId::new(required_string(params, "blobId")?.to_string())
            .map_err(|error| error.to_string())?;
        let offset = required_u64(params.get("offset"), "blob offset")?;
        let length = required_u64(params.get("length"), "blob length")?;
        let metadata = self.blobs.metadata(&id).map_err(|error| error.to_string())?;
        let owns = metadata.owner_actor_id.as_deref() == Some(actor_id);
        let referenced = self.state.conversations.iter().any(|(conversation_id, conversation)| {
            conversation_has_access(conversation, actor_id)
                && self
                    .state
                    .messages
                    .get(conversation_id)
                    .is_some_and(|messages| {
                        messages
                            .values()
                            .any(|message| message_blob_id(message) == Some(id.0.as_str()))
                    })
        });
        if !owns && !referenced {
            return Err("blob read requires owner or canonical conversation membership".into());
        }
        let bytes = self
            .blobs
            .read_range(&id, offset, length)
            .map_err(|error| error.to_string())?;
        let returned = bytes.len() as u64;
        Ok(json!({
            "blobId":id.0,
            "offset":offset,
            "length":returned,
            "sizeBytes":metadata.size_bytes,
            "dataBase64":BASE64_STANDARD.encode(bytes),
            "eof":offset.saturating_add(returned) >= metadata.size_bytes
        }))
    }

    fn begin_blob_upload(
        &mut self,
        command: &Map<String, Value>,
        actor_id: &str,
    ) -> Result<Vec<Value>, String> {
        let mut metadata: BlobMetadata = serde_json::from_value(
            command
                .get("metadata")
                .cloned()
                .ok_or("blob metadata is required")?,
        )
        .map_err(|error| format!("invalid blob metadata: {error}"))?;
        metadata.owner_actor_id = Some(actor_id.to_string());
        self.blobs
            .begin_upload(&metadata)
            .map_err(|error| error.to_string())?;
        Ok(Vec::new())
    }

    fn append_blob_chunk(
        &mut self,
        command: &Map<String, Value>,
        actor_id: &str,
    ) -> Result<Vec<Value>, String> {
        let id = BlobId::new(required_non_empty(command.get("blobId"), "blob id")?.to_string())
            .map_err(|error| error.to_string())?;
        self.require_blob_owner(&id, actor_id)?;
        let offset = required_u64(command.get("offset"), "blob offset")?;
        let encoded = required_non_empty(command.get("dataBase64"), "blob dataBase64")?;
        let bytes = BASE64_STANDARD
            .decode(encoded)
            .map_err(|_| "blob chunk is not valid base64".to_string())?;
        self.blobs
            .append_chunk(&id, offset, &bytes)
            .map_err(|error| error.to_string())?;
        Ok(Vec::new())
    }

    fn finish_blob_upload(
        &mut self,
        command: &Map<String, Value>,
        actor_id: &str,
    ) -> Result<Vec<Value>, String> {
        let id = BlobId::new(required_non_empty(command.get("blobId"), "blob id")?.to_string())
            .map_err(|error| error.to_string())?;
        self.require_blob_owner(&id, actor_id)?;
        self.blobs
            .finish_upload(&id)
            .map_err(|error| error.to_string())?;
        Ok(Vec::new())
    }

    fn delete_blob(
        &mut self,
        command: &Map<String, Value>,
        actor_id: &str,
    ) -> Result<Vec<Value>, String> {
        let id = BlobId::new(required_non_empty(command.get("blobId"), "blob id")?.to_string())
            .map_err(|error| error.to_string())?;
        self.require_blob_owner(&id, actor_id)?;
        if self
            .state
            .messages
            .values()
            .flat_map(|messages| messages.values())
            .any(|message| message_blob_id(message) == Some(id.0.as_str()))
        {
            return Err("referenced messaging blob cannot be deleted".into());
        }
        self.blobs.delete(&id).map_err(|error| error.to_string())?;
        Ok(Vec::new())
    }

    fn edit_message(
        &mut self,
        command: &Map<String, Value>,
        actor_id: &str,
        now_ms: i64,
    ) -> Result<Vec<Value>, String> {
        let conversation_id = required_non_empty(command.get("conversationId"), "conversation id")?;
        self.require_conversation_access(conversation_id, actor_id)?;
        let message_id = required_non_empty(command.get("messageId"), "message id")?;
        let current = self.require_message(conversation_id, message_id)?.clone();
        if current.get("senderId").and_then(Value::as_str) != Some(actor_id) {
            return Err("message edit requires original sender".into());
        }
        let content = command
            .get("content")
            .filter(|value| value.is_object())
            .cloned()
            .ok_or("edited message content is required")?;
        if content.get("type").and_then(Value::as_str) != Some("text") {
            return Err("Android message editing currently permits text content only".into());
        }
        let mut updated = current;
        if let Some(object) = updated.as_object_mut() {
            object.insert("content".into(), content);
            object.insert("editedAtMs".into(), json!(now_ms));
        }
        self.state
            .messages
            .get_mut(conversation_id)
            .ok_or("conversation message state is missing")?
            .insert(message_id.to_string(), updated.clone());
        self.bump_cursor();
        Ok(vec![self.server_envelope(
            json!({"type":"messageChanged","message":self.project_message(&updated, actor_id)}),
            now_ms,
        )])
    }

    fn delete_messages(
        &mut self,
        command: &Map<String, Value>,
        actor_id: &str,
        now_ms: i64,
    ) -> Result<Vec<Value>, String> {
        let conversation_id = required_non_empty(command.get("conversationId"), "conversation id")?;
        let conversation = self.require_conversation_access(conversation_id, actor_id)?.clone();
        if command.get("forEveryone").and_then(Value::as_bool) != Some(true) {
            return Err("local-only deletion is not yet represented by the canonical Android repository".into());
        }
        let ids = command
            .get("messageIds")
            .and_then(Value::as_array)
            .ok_or("messageIds array is required")?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .filter(|value| bounded_id(value))
                    .map(str::to_string)
                    .ok_or_else(|| "message id is invalid".to_string())
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        if ids.is_empty() {
            return Err("at least one message id is required".into());
        }
        let can_manage = conversation_can_manage(&conversation, actor_id);
        for message_id in &ids {
            let message = self.require_message(conversation_id, message_id)?;
            if message.get("senderId").and_then(Value::as_str) != Some(actor_id) && !can_manage {
                return Err("message deletion requires sender or conversation administrator".into());
            }
        }
        let messages = self
            .state
            .messages
            .get_mut(conversation_id)
            .ok_or("conversation message state is missing")?;
        for message_id in &ids {
            messages.remove(message_id);
        }
        if let Some(reactions) = self.state.reaction_actors.get_mut(conversation_id) {
            for message_id in &ids {
                reactions.remove(message_id);
            }
        }
        if let Some(votes) = self.state.poll_votes.get_mut(conversation_id) {
            for message_id in &ids {
                votes.remove(message_id);
            }
        }
        let latest = messages
            .values()
            .max_by_key(|message| message.get("createdAtMs").and_then(Value::as_i64).unwrap_or_default())
            .and_then(|message| message.get("id").and_then(Value::as_str))
            .map(str::to_string);
        let mut updated_conversation = conversation;
        if let Some(object) = updated_conversation.as_object_mut() {
            object.insert(
                "lastMessageId".into(),
                latest.map(Value::String).unwrap_or(Value::Null),
            );
            let pinned = object
                .get("pinnedMessageIds")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|value| value.as_str().is_none_or(|id| !ids.contains(id)))
                .collect::<Vec<_>>();
            object.insert("pinnedMessageIds".into(), Value::Array(pinned));
            object.insert("updatedAtMs".into(), json!(now_ms));
        }
        self.state
            .conversations
            .insert(conversation_id.to_string(), updated_conversation);
        self.bump_cursor();
        Ok(vec![self.server_envelope(
            json!({"type":"messagesDeleted","conversationId":conversation_id,"messageIds":ids}),
            now_ms,
        )])
    }

    fn mark_read(
        &mut self,
        command: &Map<String, Value>,
        actor_id: &str,
        now_ms: i64,
    ) -> Result<Vec<Value>, String> {
        let conversation_id = required_non_empty(command.get("conversationId"), "conversation id")?;
        self.require_conversation_access(conversation_id, actor_id)?;
        let message_id = required_non_empty(command.get("messageId"), "message id")?;
        self.require_message(conversation_id, message_id)?;
        let preferences = self.conversation_preferences_mut(actor_id, conversation_id);
        preferences.insert("lastReadMessageId".into(), Value::String(message_id.to_string()));
        preferences.insert("unreadCount".into(), json!(0));
        preferences.insert("markedUnread".into(), json!(false));
        self.bump_cursor();
        let conversation = self
            .state
            .conversations
            .get(conversation_id)
            .ok_or("conversation does not exist")?;
        Ok(vec![self.server_envelope(
            json!({"type":"conversationChanged","conversation":self.project_conversation(conversation, actor_id)}),
            now_ms,
        )])
    }

    fn set_marked_unread(
        &mut self,
        command: &Map<String, Value>,
        actor_id: &str,
        now_ms: i64,
    ) -> Result<Vec<Value>, String> {
        let conversation_id = required_non_empty(command.get("conversationId"), "conversation id")?;
        self.require_conversation_access(conversation_id, actor_id)?;
        let marked_unread = required_bool(command.get("markedUnread"), "markedUnread")?;
        self.conversation_preferences_mut(actor_id, conversation_id)
            .insert("markedUnread".into(), json!(marked_unread));
        self.bump_cursor();
        Ok(vec![self.server_envelope(
            json!({"type":"markedUnreadChanged","conversationId":conversation_id,"markedUnread":marked_unread}),
            now_ms,
        )])
    }

    fn set_draft(
        &mut self,
        command: &Map<String, Value>,
        actor_id: &str,
        now_ms: i64,
    ) -> Result<Vec<Value>, String> {
        let conversation_id = required_non_empty(command.get("conversationId"), "conversation id")?;
        self.require_conversation_access(conversation_id, actor_id)?;
        let text = command.get("text").and_then(Value::as_str).unwrap_or("");
        if text.len() > 64 * 1024 {
            return Err("draft is too large".into());
        }
        let reply_to = optional_string(command.get("replyToMessageId"));
        if let Some(message_id) = reply_to.as_deref() {
            self.require_message(conversation_id, message_id)?;
        }
        let draft = json!({
            "conversationId":conversation_id,
            "actorId":actor_id,
            "text":text,
            "replyToMessageId":reply_to,
            "updatedAtMs":now_ms
        });
        let actor_drafts = self.state.drafts.entry(actor_id.to_string()).or_default();
        if text.trim().is_empty() && reply_to.is_none() {
            actor_drafts.remove(conversation_id);
        } else {
            actor_drafts.insert(conversation_id.to_string(), draft.clone());
        }
        self.bump_cursor();
        Ok(vec![self.server_envelope(
            json!({"type":"draftChanged","draft":draft}),
            now_ms,
        )])
    }

    fn set_conversation_notifications(
        &mut self,
        command: &Map<String, Value>,
        actor_id: &str,
        now_ms: i64,
    ) -> Result<Vec<Value>, String> {
        let conversation_id = required_non_empty(command.get("conversationId"), "conversation id")?;
        self.require_conversation_access(conversation_id, actor_id)?;
        let settings = command
            .get("settings")
            .filter(|value| value.is_object())
            .cloned()
            .ok_or("notification settings object is required")?;
        self.conversation_preferences_mut(actor_id, conversation_id)
            .insert("notificationSettings".into(), settings);
        self.bump_cursor();
        let conversation = self
            .state
            .conversations
            .get(conversation_id)
            .ok_or("conversation does not exist")?;
        Ok(vec![self.server_envelope(
            json!({"type":"conversationChanged","conversation":self.project_conversation(conversation, actor_id)}),
            now_ms,
        )])
    }

    fn upsert_folder(
        &mut self,
        command: &Map<String, Value>,
        actor_id: &str,
        now_ms: i64,
    ) -> Result<Vec<Value>, String> {
        let folder = required_object(command.get("folder"), "folder")?.clone();
        let folder_id = required_non_empty(folder.get("id"), "folder id")?.to_string();
        let title = required_non_empty(folder.get("title"), "folder title")?;
        if !bounded_id(&folder_id) || title.len() > 200 {
            return Err("folder id or title is invalid".into());
        }
        let mut seen = BTreeSet::new();
        for conversation_id in folder
            .get("conversationIds")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            let conversation_id = conversation_id
                .as_str()
                .filter(|value| bounded_id(value))
                .ok_or("folder conversation id is invalid")?;
            if !seen.insert(conversation_id.to_string()) {
                return Err("folder contains duplicate conversation id".into());
            }
            self.require_conversation_access(conversation_id, actor_id)?;
        }
        let value = Value::Object(folder);
        self.state
            .folders
            .entry(actor_id.to_string())
            .or_default()
            .insert(folder_id, value.clone());
        self.bump_cursor();
        Ok(vec![self.server_envelope(
            json!({"type":"folderChanged","folder":value}),
            now_ms,
        )])
    }

    fn delete_folder(
        &mut self,
        command: &Map<String, Value>,
        actor_id: &str,
        now_ms: i64,
    ) -> Result<Vec<Value>, String> {
        let folder_id = required_non_empty(command.get("folderId"), "folder id")?;
        let removed = self
            .state
            .folders
            .entry(actor_id.to_string())
            .or_default()
            .remove(folder_id);
        if removed.is_none() {
            return Err("folder does not exist for authenticated actor".into());
        }
        self.bump_cursor();
        Ok(vec![self.server_envelope(
            json!({"type":"folderDeleted","folderId":folder_id}),
            now_ms,
        )])
    }

    fn set_conversation_flag(
        &mut self,
        command: &Map<String, Value>,
        actor_id: &str,
        now_ms: i64,
        command_key: &str,
        state_key: &str,
    ) -> Result<Vec<Value>, String> {
        let conversation_id = required_non_empty(command.get("conversationId"), "conversation id")?;
        self.require_conversation_access(conversation_id, actor_id)?;
        let value = required_bool(command.get(command_key), command_key)?;
        self.conversation_preferences_mut(actor_id, conversation_id)
            .insert(state_key.to_string(), json!(value));
        self.bump_cursor();
        let conversation = self
            .state
            .conversations
            .get(conversation_id)
            .ok_or("conversation does not exist")?;
        Ok(vec![self.server_envelope(
            json!({"type":"conversationChanged","conversation":self.project_conversation(conversation, actor_id)}),
            now_ms,
        )])
    }

    fn update_conversation_info(
        &mut self,
        command: &Map<String, Value>,
        actor_id: &str,
        now_ms: i64,
    ) -> Result<Vec<Value>, String> {
        let conversation_id = required_non_empty(command.get("conversationId"), "conversation id")?;
        let mut conversation = self.require_conversation_access(conversation_id, actor_id)?.clone();
        if !conversation_can_manage(&conversation, actor_id) {
            return Err("conversation info update requires owner or administrator".into());
        }
        if !matches!(conversation_kind(&conversation), Some("group") | Some("channel")) {
            return Err("conversation info update is only valid for groups or channels".into());
        }
        let title = required_non_empty(command.get("title"), "conversation title")?.trim();
        if title.len() > 200 {
            return Err("conversation title is too long".into());
        }
        let description = optional_string(command.get("description"));
        if description.as_ref().is_some_and(|value| value.len() > 4096) {
            return Err("conversation description is too long".into());
        }
        if let Some(object) = conversation.as_object_mut() {
            object.insert("title".into(), Value::String(title.to_string()));
            object.insert(
                "description".into(),
                description.map(Value::String).unwrap_or(Value::Null),
            );
            object.insert("updatedAtMs".into(), json!(now_ms));
        }
        self.state
            .conversations
            .insert(conversation_id.to_string(), conversation.clone());
        self.bump_cursor();
        Ok(vec![self.server_envelope(
            json!({"type":"conversationChanged","conversation":self.project_conversation(&conversation, actor_id)}),
            now_ms,
        )])
    }

    fn set_conversation_participant(
        &mut self,
        command: &Map<String, Value>,
        actor_id: &str,
        now_ms: i64,
    ) -> Result<Vec<Value>, String> {
        let conversation_id = required_non_empty(command.get("conversationId"), "conversation id")?;
        let mut conversation = self.require_conversation_access(conversation_id, actor_id)?.clone();
        if !conversation_can_manage(&conversation, actor_id) {
            return Err("participant update requires owner or administrator".into());
        }
        if !matches!(conversation_kind(&conversation), Some("group") | Some("channel")) {
            return Err("participant updates are only valid for groups or channels".into());
        }
        let participant = required_object(command.get("participant"), "participant")?.clone();
        let target_actor_id = required_non_empty(participant.get("actorId"), "participant actorId")?;
        let role = required_non_empty(participant.get("role"), "participant role")?;
        if !matches!(role, "owner" | "admin" | "member" | "restricted") {
            return Err("participant role is invalid".into());
        }
        if !self.state.actors.contains_key(target_actor_id) && target_actor_id != actor_id {
            return Err("participant actor does not exist in canonical messaging state".into());
        }
        if conversation_owner(&conversation) == Some(target_actor_id) && role != "owner" {
            return Err("conversation owner cannot be downgraded".into());
        }
        if role == "owner" && conversation_owner(&conversation) != Some(target_actor_id) {
            return Err("ownership transfer requires a dedicated verified contract".into());
        }
        let participants = conversation
            .as_object_mut()
            .ok_or("conversation object is invalid")?
            .entry("participants")
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or("conversation participants are invalid")?;
        if let Some(existing) = participants
            .iter_mut()
            .find(|value| value.get("actorId").and_then(Value::as_str) == Some(target_actor_id))
        {
            *existing = Value::Object(participant.clone());
        } else {
            participants.push(Value::Object(participant.clone()));
        }
        if let Some(object) = conversation.as_object_mut() {
            object.insert("updatedAtMs".into(), json!(now_ms));
        }
        self.state
            .conversations
            .insert(conversation_id.to_string(), conversation.clone());
        self.bump_cursor();
        Ok(vec![self.server_envelope(
            json!({"type":"conversationParticipantChanged","conversation":self.project_conversation(&conversation, actor_id),"participant":Value::Object(participant)}),
            now_ms,
        )])
    }

    fn remove_conversation_participant(
        &mut self,
        command: &Map<String, Value>,
        actor_id: &str,
        now_ms: i64,
    ) -> Result<Vec<Value>, String> {
        let conversation_id = required_non_empty(command.get("conversationId"), "conversation id")?;
        let mut conversation = self.require_conversation_access(conversation_id, actor_id)?.clone();
        if !conversation_can_manage(&conversation, actor_id) {
            return Err("participant removal requires owner or administrator".into());
        }
        if !matches!(conversation_kind(&conversation), Some("group") | Some("channel")) {
            return Err("participant removal is only valid for groups or channels".into());
        }
        let target_actor_id = required_non_empty(command.get("actorId"), "participant actorId")?;
        if conversation_owner(&conversation) == Some(target_actor_id) {
            return Err("conversation owner cannot be removed".into());
        }
        let participants = conversation
            .as_object_mut()
            .ok_or("conversation object is invalid")?
            .get_mut("participants")
            .and_then(Value::as_array_mut)
            .ok_or("conversation participants are invalid")?;
        let before = participants.len();
        participants.retain(|value| value.get("actorId").and_then(Value::as_str) != Some(target_actor_id));
        if participants.len() == before {
            return Err("participant does not exist in conversation".into());
        }
        if let Some(object) = conversation.as_object_mut() {
            object.insert("updatedAtMs".into(), json!(now_ms));
        }
        self.state
            .conversations
            .insert(conversation_id.to_string(), conversation.clone());
        self.bump_cursor();
        Ok(vec![self.server_envelope(
            json!({"type":"conversationParticipantChanged","conversation":self.project_conversation(&conversation, actor_id),"removedActorId":target_actor_id}),
            now_ms,
        )])
    }

    fn forward_message(
        &mut self,
        command: &Map<String, Value>,
        actor_id: &str,
        now_ms: i64,
    ) -> Result<Vec<Value>, String> {
        let source_id = required_non_empty(command.get("sourceConversationId"), "source conversation id")?;
        let destination_id = required_non_empty(command.get("destinationConversationId"), "destination conversation id")?;
        self.require_conversation_access(source_id, actor_id)?;
        let destination = self.require_conversation_access(destination_id, actor_id)?.clone();
        let message_id = required_non_empty(command.get("messageId"), "message id")?;
        let source = self.require_message(source_id, message_id)?.clone();
        if source.get("protectedContent").and_then(Value::as_bool) == Some(true) {
            return Err("protected content cannot be forwarded".into());
        }
        let client_message_id = required_non_empty(command.get("clientMessageId"), "clientMessageId")?;
        if !bounded_id(client_message_id) {
            return Err("clientMessageId is invalid".into());
        }
        if let Some(existing) = self
            .state
            .messages
            .get(destination_id)
            .and_then(|messages| messages.get(client_message_id))
            .cloned()
        {
            return Ok(vec![self.server_envelope(
                json!({"type":"messageAdded","message":self.project_message(&existing, actor_id)}),
                now_ms,
            )]);
        }
        let content = source.get("content").cloned().ok_or("source message content is missing")?;
        if !conversation_can_send(&destination, actor_id, &content) {
            return Err("destination conversation does not permit this message".into());
        }
        let forwarded = json!({
            "id":client_message_id,
            "clientMessageId":client_message_id,
            "conversationId":destination_id,
            "senderId":actor_id,
            "content":content,
            "replyToMessageId":Value::Null,
            "threadRootMessageId":command.get("threadRootMessageId").cloned().unwrap_or(Value::Null),
            "createdAtMs":now_ms,
            "editedAtMs":Value::Null,
            "scheduledAtMs":command.get("scheduledAtMs").cloned().unwrap_or(Value::Null),
            "silent":command.get("silent").and_then(Value::as_bool).unwrap_or(false),
            "protectedContent":false,
            "deliveryState":"sent",
            "reactions":[],
            "pinned":false,
            "deleted":false,
            "forwardOrigin":source.get("senderId").cloned().unwrap_or(Value::Null)
        });
        self.state
            .messages
            .entry(destination_id.to_string())
            .or_default()
            .insert(client_message_id.to_string(), forwarded.clone());
        let mut updated = destination;
        if let Some(object) = updated.as_object_mut() {
            object.insert("lastMessageId".into(), Value::String(client_message_id.to_string()));
            object.insert("updatedAtMs".into(), json!(now_ms));
        }
        self.state
            .conversations
            .insert(destination_id.to_string(), updated);
        self.bump_cursor();
        Ok(vec![self.server_envelope(
            json!({"type":"messageAdded","message":self.project_message(&forwarded, actor_id)}),
            now_ms,
        )])
    }

    fn set_reaction(
        &mut self,
        command: &Map<String, Value>,
        actor_id: &str,
        now_ms: i64,
    ) -> Result<Vec<Value>, String> {
        let conversation_id = required_non_empty(command.get("conversationId"), "conversation id")?;
        self.require_conversation_access(conversation_id, actor_id)?;
        let message_id = required_non_empty(command.get("messageId"), "message id")?;
        self.require_message(conversation_id, message_id)?;
        let reaction = required_object(command.get("reaction"), "reaction")?;
        let symbol = required_non_empty(reaction.get("reaction"), "reaction value")?;
        if symbol.chars().count() > 32 {
            return Err("reaction value is too long".into());
        }
        let enabled = reaction
            .get("chosenByMe")
            .and_then(Value::as_bool)
            .ok_or("reaction chosenByMe is required")?;
        let actors = self
            .state
            .reaction_actors
            .entry(conversation_id.to_string())
            .or_default()
            .entry(message_id.to_string())
            .or_default()
            .entry(symbol.to_string())
            .or_default();
        if enabled {
            actors.insert(actor_id.to_string());
        } else {
            actors.remove(actor_id);
        }
        let message = self.require_message(conversation_id, message_id)?.clone();
        self.bump_cursor();
        Ok(vec![self.server_envelope(
            json!({"type":"messageChanged","message":self.project_message(&message, actor_id)}),
            now_ms,
        )])
    }

    fn pin_message(
        &mut self,
        command: &Map<String, Value>,
        actor_id: &str,
        now_ms: i64,
    ) -> Result<Vec<Value>, String> {
        let conversation_id = required_non_empty(command.get("conversationId"), "conversation id")?;
        let mut conversation = self.require_conversation_access(conversation_id, actor_id)?.clone();
        if !conversation_can_pin(&conversation, actor_id) {
            return Err("pinning messages is not permitted for authenticated actor".into());
        }
        let message_id = required_non_empty(command.get("messageId"), "message id")?;
        let mut message = self.require_message(conversation_id, message_id)?.clone();
        let pinned = required_bool(command.get("pinned"), "pinned")?;
        if let Some(object) = message.as_object_mut() {
            object.insert("pinned".into(), json!(pinned));
        }
        self.state
            .messages
            .get_mut(conversation_id)
            .ok_or("conversation message state is missing")?
            .insert(message_id.to_string(), message.clone());
        let object = conversation.as_object_mut().ok_or("conversation object is invalid")?;
        let mut ids = object
            .get("pinnedMessageIds")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        ids.retain(|value| value.as_str() != Some(message_id));
        if pinned {
            ids.push(Value::String(message_id.to_string()));
        }
        object.insert("pinnedMessageIds".into(), Value::Array(ids));
        object.insert("updatedAtMs".into(), json!(now_ms));
        self.state
            .conversations
            .insert(conversation_id.to_string(), conversation);
        self.bump_cursor();
        Ok(vec![self.server_envelope(
            json!({"type":"messageChanged","message":self.project_message(&message, actor_id)}),
            now_ms,
        )])
    }

    fn vote_poll(
        &mut self,
        command: &Map<String, Value>,
        actor_id: &str,
        now_ms: i64,
    ) -> Result<Vec<Value>, String> {
        let conversation_id = required_non_empty(command.get("conversationId"), "conversation id")?;
        let conversation = self.require_conversation_access(conversation_id, actor_id)?;
        if !conversation_can_send_kind(conversation, actor_id, "poll") {
            return Err("poll voting is not permitted for authenticated actor".into());
        }
        let message_id = required_non_empty(command.get("messageId"), "message id")?;
        let message = self.require_message(conversation_id, message_id)?.clone();
        let data = message
            .get("content")
            .and_then(|value| value.get("data"))
            .and_then(Value::as_object)
            .ok_or("poll message data is missing")?;
        if message
            .get("content")
            .and_then(|value| value.get("type"))
            .and_then(Value::as_str)
            != Some("poll")
        {
            return Err("votePoll target is not a poll".into());
        }
        let allowed = data
            .get("options")
            .and_then(Value::as_array)
            .ok_or("poll options are missing")?
            .iter()
            .filter_map(|option| option.get("id").and_then(Value::as_str))
            .map(str::to_string)
            .collect::<BTreeSet<_>>();
        let selected = command
            .get("optionIds")
            .and_then(Value::as_array)
            .ok_or("optionIds array is required")?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .filter(|id| allowed.contains(*id))
                    .map(str::to_string)
                    .ok_or_else(|| "poll option is invalid".to_string())
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        if !data
            .get("multipleAnswers")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            && selected.len() > 1
        {
            return Err("poll permits only one answer".into());
        }
        self.state
            .poll_votes
            .entry(conversation_id.to_string())
            .or_default()
            .entry(message_id.to_string())
            .or_default()
            .insert(actor_id.to_string(), selected);
        self.bump_cursor();
        Ok(vec![self.server_envelope(
            json!({"type":"messageChanged","message":self.project_message(&message, actor_id)}),
            now_ms,
        )])
    }

    fn typing(
        &mut self,
        command: &Map<String, Value>,
        actor_id: &str,
        now_ms: i64,
        started: bool,
    ) -> Result<Vec<Value>, String> {
        let conversation_id = required_non_empty(command.get("conversationId"), "conversation id")?;
        self.require_conversation_access(conversation_id, actor_id)?;
        let action = if started {
            Value::String(
                command
                    .get("action")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or("typing")
                    .to_string(),
            )
        } else {
            Value::Null
        };
        self.bump_cursor();
        Ok(vec![self.server_envelope(
            json!({"type":"typingChanged","conversationId":conversation_id,"actorId":actor_id,"action":action}),
            now_ms,
        )])
    }

    fn require_blob_owner(&self, id: &BlobId, actor_id: &str) -> Result<BlobMetadata, String> {
        let metadata = self
            .blobs
            .upload_metadata(id)
            .map_err(|error| error.to_string())?;
        if metadata.owner_actor_id.as_deref() != Some(actor_id) {
            return Err("blob mutation requires durable owner".into());
        }
        Ok(metadata)
    }

    fn require_conversation_access(
        &self,
        conversation_id: &str,
        actor_id: &str,
    ) -> Result<&Value, String> {
        let conversation = self
            .state
            .conversations
            .get(conversation_id)
            .ok_or("conversation does not exist")?;
        if !conversation_has_access(conversation, actor_id) {
            return Err("messaging command requires canonical conversation membership".into());
        }
        Ok(conversation)
    }

    fn validate_content_for_send(
        &self,
        conversation: &Value,
        content: &Value,
        actor_id: &str,
    ) -> Result<(), String> {
        let kind = content
            .get("type")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or("message content type is required")?;
        if !conversation_can_send_kind(conversation, actor_id, kind) {
            return Err("conversation permissions reject message content".into());
        }
        if let Some(blob_id) = content_blob_id(content) {
            let id = BlobId::new(blob_id.to_string()).map_err(|error| error.to_string())?;
            let metadata = self.blobs.metadata(&id).map_err(|error| error.to_string())?;
            if metadata.owner_actor_id.as_deref() != Some(actor_id) {
                return Err("sending local media requires durable blob ownership".into());
            }
        }
        Ok(())
    }

    fn conversation_preferences_mut(
        &mut self,
        actor_id: &str,
        conversation_id: &str,
    ) -> &mut Map<String, Value> {
        let value = self
            .state
            .conversation_preferences
            .entry(actor_id.to_string())
            .or_default()
            .entry(conversation_id.to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if !value.is_object() {
            *value = Value::Object(Map::new());
        }
        value.as_object_mut().expect("preference value is object")
    }

    fn project_conversation(&self, conversation: &Value, actor_id: &str) -> Value {
        let mut projected = conversation.clone();
        let Some(conversation_id) = conversation.get("id").and_then(Value::as_str) else {
            return projected;
        };
        if let Some(preferences) = self
            .state
            .conversation_preferences
            .get(actor_id)
            .and_then(|values| values.get(conversation_id))
            .and_then(Value::as_object)
        {
            if let Some(object) = projected.as_object_mut() {
                for (key, value) in preferences {
                    object.insert(key.clone(), value.clone());
                }
            }
        }
        projected
    }

    fn project_message(&self, message: &Value, actor_id: &str) -> Value {
        let mut projected = message.clone();
        let Some(conversation_id) = message.get("conversationId").and_then(Value::as_str) else {
            return projected;
        };
        let Some(message_id) = message.get("id").and_then(Value::as_str) else {
            return projected;
        };
        if let Some(reactions) = self
            .state
            .reaction_actors
            .get(conversation_id)
            .and_then(|messages| messages.get(message_id))
        {
            let values = reactions
                .iter()
                .filter(|(_, actors)| !actors.is_empty())
                .map(|(reaction, actors)| {
                    json!({
                        "reaction":reaction,
                        "count":actors.len(),
                        "chosenByMe":actors.contains(actor_id),
                        "recentActorIds":actors.iter().take(8).cloned().collect::<Vec<_>>()
                    })
                })
                .collect::<Vec<_>>();
            if let Some(object) = projected.as_object_mut() {
                object.insert("reactions".into(), Value::Array(values));
            }
        }
        if let Some(votes) = self
            .state
            .poll_votes
            .get(conversation_id)
            .and_then(|messages| messages.get(message_id))
        {
            if let Some(options) = projected
                .get_mut("content")
                .and_then(|content| content.get_mut("data"))
                .and_then(|data| data.get_mut("options"))
                .and_then(Value::as_array_mut)
            {
                let selected = votes.get(actor_id);
                for option in options {
                    let Some(option_id) = option.get("id").and_then(Value::as_str).map(str::to_string) else {
                        continue;
                    };
                    if let Some(object) = option.as_object_mut() {
                        let count = votes
                            .values()
                            .filter(|choices| choices.contains(&option_id))
                            .count();
                        object.insert("voterCount".into(), json!(count));
                        object.insert(
                            "chosen".into(),
                            json!(selected.is_some_and(|choices| choices.contains(&option_id))),
                        );
                    }
                }
            }
        }
        projected
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
            .map(|conversation| self.project_conversation(conversation, actor_id))
            .collect::<Vec<_>>();
        let messages = visible_ids
            .iter()
            .filter_map(|id| self.state.messages.get(id))
            .flat_map(|messages| messages.values())
            .map(|message| self.project_message(message, actor_id))
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
        let folders = self
            .state
            .folders
            .get(actor_id)
            .map(|values| values.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        let drafts = self
            .state
            .drafts
            .get(actor_id)
            .map(|values| {
                values
                    .iter()
                    .filter(|(conversation_id, _)| visible_ids.contains(*conversation_id))
                    .map(|(_, draft)| draft.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        self.server_envelope(
            json!({
                "type":"syncBatch",
                "actors":actors,
                "conversations":conversations,
                "messages":messages,
                "folders":folders,
                "drafts":drafts,
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
    let mut seen_requests = BTreeSet::new();
    if state.request_order.iter().any(|key| {
        !seen_requests.insert(key)
            || !state.request_results.contains_key(key)
    }) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "messaging request replay index is inconsistent",
        ));
    }
    for (conversation_id, conversation) in &state.conversations {
        if conversation.get("id").and_then(Value::as_str) != Some(conversation_id.as_str()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "messaging conversation key does not match stored id",
            ));
        }
    }
    for (conversation_id, messages) in &state.messages {
        if !state.conversations.contains_key(conversation_id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "messaging message references missing conversation",
            ));
        }
        for (message_id, message) in messages {
            if message.get("id").and_then(Value::as_str) != Some(message_id.as_str())
                || message.get("conversationId").and_then(Value::as_str)
                    != Some(conversation_id.as_str())
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "messaging message key or conversation is inconsistent",
                ));
            }
        }
    }
    Ok(())
}

fn conversation_role<'a>(conversation: &'a Value, actor_id: &str) -> Option<&'a str> {
    if conversation_owner(conversation) == Some(actor_id) {
        return Some("owner");
    }
    conversation
        .get("participants")
        .and_then(Value::as_array)
        .and_then(|participants| {
            participants.iter().find_map(|participant| {
                (participant.get("actorId").and_then(Value::as_str) == Some(actor_id))
                    .then(|| participant.get("role").and_then(Value::as_str))
                    .flatten()
            })
        })
}

fn conversation_can_manage(conversation: &Value, actor_id: &str) -> bool {
    matches!(conversation_role(conversation, actor_id), Some("owner") | Some("admin"))
}

fn conversation_permission(conversation: &Value, key: &str) -> bool {
    conversation
        .get("permissions")
        .and_then(|permissions| permissions.get(key))
        .and_then(Value::as_bool)
        == Some(true)
}

fn conversation_can_send_kind(conversation: &Value, actor_id: &str, kind: &str) -> bool {
    if !conversation_has_access(conversation, actor_id) {
        return false;
    }
    if conversation_can_manage(conversation, actor_id) {
        return true;
    }
    let permission = match kind {
        "photo" | "video" | "document" | "voice" | "audio" => "canSendMedia",
        "poll" => "canSendPolls",
        _ => "canSendMessages",
    };
    conversation_permission(conversation, permission)
}

fn conversation_can_send(conversation: &Value, actor_id: &str, content: &Value) -> bool {
    content
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|kind| conversation_can_send_kind(conversation, actor_id, kind))
}

fn conversation_can_pin(conversation: &Value, actor_id: &str) -> bool {
    conversation_can_manage(conversation, actor_id)
        || (conversation_has_access(conversation, actor_id)
            && conversation_permission(conversation, "canPinMessages"))
}

fn content_blob_id(content: &Value) -> Option<&str> {
    content
        .get("data")
        .and_then(|data| data.get("media"))
        .and_then(|media| media.get("id"))
        .and_then(Value::as_str)
}

fn message_blob_id(message: &Value) -> Option<&str> {
    message.get("content").and_then(content_blob_id)
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

fn required_u64(value: Option<&Value>, label: &str) -> Result<u64, String> {
    value
        .and_then(Value::as_u64)
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
    #[test]
    fn blob_upload_range_read_reopen_and_account_fence_are_durable() {
        let root = temp_root("blob");
        let actor = "human:owner";
        {
            let mut service = AndroidMessagingService::open(&root).unwrap();
            seed(&mut service, actor);
            execute(
                &mut service,
                "blob-begin",
                actor,
                json!({"type":"beginBlobUpload","metadata":{
                    "id":"blob-1","fileName":"hello.txt","mimeType":"text/plain",
                    "sizeBytes":11,"contentHash":Value::Null,"createdAtMs":1
                }}),
            )
            .unwrap();
            execute(
                &mut service,
                "blob-append-1",
                actor,
                json!({"type":"appendBlobChunk","blobId":"blob-1","offset":0,"dataBase64":"aGVsbG8g"}),
            )
            .unwrap();
            execute(
                &mut service,
                "blob-append-1-retry",
                actor,
                json!({"type":"appendBlobChunk","blobId":"blob-1","offset":0,"dataBase64":"aGVsbG8g"}),
            )
            .unwrap();
            execute(
                &mut service,
                "blob-append-2",
                actor,
                json!({"type":"appendBlobChunk","blobId":"blob-1","offset":6,"dataBase64":"d29ybGQ="}),
            )
            .unwrap();
            execute(
                &mut service,
                "blob-finish",
                actor,
                json!({"type":"finishBlobUpload","blobId":"blob-1"}),
            )
            .unwrap();
            execute(
                &mut service,
                "media-message",
                actor,
                json!({"type":"sendMessage","conversationId":"conversation:parent","clientMessageId":"message:media",
                    "content":{"type":"document","data":{"media":{
                        "id":"blob-1","fileName":"hello.txt","mimeType":"text/plain","sizeBytes":11,
                        "remoteUrl":"fabushi-blob://blob-1"
                    },"caption":{"text":"","entities":[]}}}}),
            )
            .unwrap();
            let read = service
                .read_blob_range(&json!({"blobId":"blob-1","offset":6,"length":5}), actor)
                .unwrap();
            assert_eq!(read["dataBase64"], "d29ybGQ=");
            assert!(service
                .read_blob_range(
                    &json!({"blobId":"blob-1","offset":0,"length":5}),
                    "human:outsider"
                )
                .is_err());
        }
        let reopened = AndroidMessagingService::open(&root).unwrap();
        assert_eq!(
            reopened
                .read_blob_range(&json!({"blobId":"blob-1","offset":0,"length":5}), actor)
                .unwrap()["dataBase64"],
            "aGVsbG8="
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn canonical_mutations_enforce_admin_and_keep_account_scoped_draft_and_reaction_projection() {
        let root = temp_root("mutations");
        let owner = "human:owner";
        let other = "human:other";
        let mut service = AndroidMessagingService::open(&root).unwrap();
        seed(&mut service, owner);
        execute(
            &mut service,
            "other-profile",
            other,
            json!({"type":"upsertProfile","actor":{"id":other,"kind":"human","displayName":"Other"}}),
        )
        .unwrap();
        execute(
            &mut service,
            "add-other",
            owner,
            json!({"type":"setConversationParticipant","conversationId":"conversation:parent",
                "participant":{"actorId":other,"role":"member","joinedAtMs":2,"mutedUntilMs":Value::Null}}),
        )
        .unwrap();

        execute(
            &mut service,
            "draft-owner",
            owner,
            json!({"type":"setDraft","conversationId":"conversation:parent","text":"owner draft","replyToMessageId":Value::Null}),
        )
        .unwrap();
        execute(
            &mut service,
            "react-owner",
            owner,
            json!({"type":"setReaction","conversationId":"conversation:parent","messageId":"message:root",
                "reaction":{"reaction":"👍","chosenByMe":true}}),
        )
        .unwrap();
        assert!(execute(
            &mut service,
            "info-other",
            other,
            json!({"type":"updateConversationInfo","conversationId":"conversation:parent","title":"forged","description":Value::Null}),
        )
        .is_err());

        let owner_sync = execute(&mut service, "sync-owner", owner, json!({"type":"sync"})).unwrap();
        assert_eq!(
            owner_sync["envelopes"][0]["event"]["drafts"][0]["text"],
            "owner draft"
        );
        assert_eq!(
            owner_sync["envelopes"][0]["event"]["messages"][0]["reactions"][0]["chosenByMe"],
            true
        );
        let other_sync = execute(&mut service, "sync-other", other, json!({"type":"sync"})).unwrap();
        assert!(other_sync["envelopes"][0]["event"]["drafts"]
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(
            other_sync["envelopes"][0]["event"]["messages"][0]["reactions"][0]["chosenByMe"],
            false
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn forward_edit_delete_pin_read_folder_and_typing_survive_repository_reopen() {
        let root = temp_root("commands");
        let actor = "human:owner";
        {
            let mut service = AndroidMessagingService::open(&root).unwrap();
            seed(&mut service, actor);
            execute(
                &mut service,
                "edit",
                actor,
                json!({"type":"editMessage","conversationId":"conversation:parent","messageId":"message:root",
                    "content":{"type":"text","data":{"text":{"text":"edited","entities":[]}}}}),
            )
            .unwrap();
            execute(
                &mut service,
                "pin-message",
                actor,
                json!({"type":"pinMessage","conversationId":"conversation:parent","messageId":"message:root","pinned":true}),
            )
            .unwrap();
            execute(
                &mut service,
                "mark-read",
                actor,
                json!({"type":"markRead","conversationId":"conversation:parent","messageId":"message:root"}),
            )
            .unwrap();
            execute(
                &mut service,
                "archive",
                actor,
                json!({"type":"archiveConversation","conversationId":"conversation:parent","archived":true}),
            )
            .unwrap();
            execute(
                &mut service,
                "folder",
                actor,
                json!({"type":"upsertFolder","folder":{"id":"folder:one","title":"One","conversationIds":["conversation:parent"]}}),
            )
            .unwrap();
            let typing = execute(
                &mut service,
                "typing",
                actor,
                json!({"type":"startTyping","conversationId":"conversation:parent","action":"typing"}),
            )
            .unwrap();
            assert_eq!(typing["envelopes"][0]["event"]["type"], "typingChanged");
            execute(
                &mut service,
                "forward",
                actor,
                json!({"type":"forwardMessage","sourceConversationId":"conversation:parent",
                    "messageId":"message:root","destinationConversationId":"conversation:parent","clientMessageId":"message:forward"}),
            )
            .unwrap();
            execute(
                &mut service,
                "delete",
                actor,
                json!({"type":"deleteMessages","conversationId":"conversation:parent","messageIds":["message:forward"],"forEveryone":true}),
            )
            .unwrap();
        }
        let mut reopened = AndroidMessagingService::open(&root).unwrap();
        let sync = execute(&mut reopened, "sync-reopen-commands", actor, json!({"type":"sync"})).unwrap();
        let event = &sync["envelopes"][0]["event"];
        assert_eq!(event["messages"][0]["content"]["data"]["text"]["text"], "edited");
        assert_eq!(event["messages"][0]["pinned"], true);
        assert_eq!(event["conversations"][0]["archived"], true);
        assert_eq!(event["conversations"][0]["lastReadMessageId"], "message:root");
        assert_eq!(event["folders"][0]["id"], "folder:one");
        assert_eq!(event["messages"].as_array().unwrap().len(), 1);
        let _ = fs::remove_dir_all(root);
    }


}
