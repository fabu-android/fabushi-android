use crate::{
    messaging_blob::{BlobId, BlobMetadata, FileBlobStore, MAX_BLOB_RANGE_BYTES},
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
    time::{SystemTime, UNIX_EPOCH},
};

const MESSAGING_PROTOCOL_VERSION: u64 = 2;
const MESSAGING_REPOSITORY_SCHEMA_VERSION: u32 = 3;
const PREVIOUS_MESSAGING_REPOSITORY_SCHEMA_VERSION: u32 = 2;
const LEGACY_MESSAGING_REPOSITORY_SCHEMA_VERSION: u32 = 1;
const MAX_REPLAY_RESULTS: usize = 512;
const MESSAGING_EVENT_JOURNAL_LIMIT: usize = 10_000;

fn stable_message_id(actor_id: &str, client_message_id: &str) -> String {
    let mut identity = Vec::with_capacity(actor_id.len() + client_message_id.len() + 1);
    identity.extend_from_slice(actor_id.as_bytes());
    identity.push(0);
    identity.extend_from_slice(client_message_id.as_bytes());
    format!("msg:{}", crate::sha256::sha256_hex(&identity))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MessagingJournalEntry {
    cursor: u64,
    audience: Vec<String>,
    envelope: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MessagingRepositoryState {
    schema_version: u32,
    cursor: u64,
    #[serde(default)]
    actors: BTreeMap<String, Value>,
    #[serde(default)]
    contact_ids: BTreeSet<String>,
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
    #[serde(default)]
    event_journal: Vec<MessagingJournalEntry>,
    #[serde(default)]
    journal_floor_cursor: u64,
}

impl Default for MessagingRepositoryState {
    fn default() -> Self {
        Self {
            schema_version: MESSAGING_REPOSITORY_SCHEMA_VERSION,
            cursor: 0,
            actors: BTreeMap::new(),
            contact_ids: BTreeSet::new(),
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
            event_journal: Vec::new(),
            journal_floor_cursor: 0,
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
        let mut recovered_from_backup = false;
        let (state, migrated_legacy_repository) = match fs::read(&path) {
            Ok(bytes) => match decode_repository_state(&bytes) {
                Ok(decoded) => decoded,
                Err(primary_error) => {
                    // Preserve the damaged canonical bytes for diagnostics, but never treat a
                    // missing/invalid recovery replica as an empty repository: request replay and
                    // message journal state are side-effect truth.
                    let quarantine = repository_corrupt_path(&path);
                    copy_file_durably(&path, &quarantine)?;
                    let backup = repository_backup_path(&path);
                    let primary_kind = primary_error.kind();
                    let primary_message = primary_error.to_string();
                    let restore_error = || io::Error::new(primary_kind, primary_message.clone());
                    let backup_bytes = fs::read(&backup).map_err(|_| restore_error())?;
                    let decoded =
                        decode_repository_state(&backup_bytes).map_err(|_| restore_error())?;
                    recovered_from_backup = true;
                    decoded
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                (MessagingRepositoryState::default(), false)
            }
            Err(error) => return Err(error),
        };
        let children = ConversationChildStore::open(app_data_dir.join("conversation-children.json"))?;
        let blobs = FileBlobStore::new(app_data_dir.join("messaging-blobs"));
        let service = Self {
            path,
            state,
            children,
            blobs,
        };
        if migrated_legacy_repository || recovered_from_backup {
            service.persist()?;
        }
        Ok(service)
    }

    /// Reconcile the server-authoritative Human contact roster into the canonical Android
    /// messaging repository. This is deliberately Host-only: Presentation cannot upsert an
    /// arbitrary actor or manufacture friendship state.
    pub fn reconcile_remote_human_contacts(
        &mut self,
        contacts: &[Value],
    ) -> Result<usize, String> {
        let previous = self.state.clone();
        let mut next_contact_ids = BTreeSet::new();
        let mut normalized = Vec::with_capacity(contacts.len());

        for contact in contacts {
            let actor = contact
                .as_object()
                .ok_or("remote Human contact must be an actor object")?;
            let id = required_non_empty(actor.get("id"), "remote Human contact id")?;
            if !valid_actor_id(id) || !id.starts_with("human:") {
                return Err("remote Human contact id is invalid".into());
            }
            if required_non_empty(actor.get("kind"), "remote Human contact kind")? != "human" {
                return Err("remote Human contact kind must be human".into());
            }
            let display_name =
                required_non_empty(actor.get("displayName"), "remote Human contact displayName")?;
            if display_name.trim().is_empty() || display_name.len() > 200 {
                return Err("remote Human contact displayName is invalid".into());
            }
            if !next_contact_ids.insert(id.to_string()) {
                return Err("remote Human contact roster contains a duplicate actor".into());
            }
            normalized.push((id.to_string(), Value::Object(actor.clone())));
        }

        let mut changed = self.state.contact_ids != next_contact_ids;
        for (id, actor) in &normalized {
            if self.state.actors.get(id) != Some(actor) {
                self.state.actors.insert(id.clone(), actor.clone());
                changed = true;
            }
        }
        self.state.contact_ids = next_contact_ids;

        if !changed {
            return Ok(0);
        }

        // Contact membership has no UI-authored mutation journal. Invalidate previous deltas so
        // every observer heals from one canonical snapshot instead of replaying incomplete state.
        self.bump_cursor();
        self.state.event_journal.clear();
        self.state.journal_floor_cursor = self.state.cursor;
        if let Err(error) = self.persist() {
            self.state = previous;
            return Err(format!(
                "failed to persist canonical Android Human contact roster: {error}"
            ));
        }
        Ok(normalized.len())
    }

    /// Reconcile a server-authoritative page of Human direct messages into the canonical
    /// Android messaging repository. Backend user ids are namespaced at this boundary so the
    /// rest of Android keeps one actor-id contract, while remote message identity remains stable.
    pub fn reconcile_remote_human_messages(
        &mut self,
        authenticated_actor_id: &str,
        peer_actor_id: &str,
        messages: &[Value],
    ) -> Result<usize, String> {
        let authenticated_actor_id = authenticated_actor_id.trim();
        let peer_actor_id = peer_actor_id.trim();
        if !valid_actor_id(authenticated_actor_id)
            || !authenticated_actor_id.starts_with("human:")
            || !valid_actor_id(peer_actor_id)
            || !peer_actor_id.starts_with("human:")
            || authenticated_actor_id == peer_actor_id
        {
            return Err("remote Human message reconciliation actor identity is invalid".into());
        }

        let previous = self.state.clone();
        let conversation_id =
            stable_human_direct_conversation_id(authenticated_actor_id, peer_actor_id);
        let now_ms = current_messaging_time_ms();
        let peer = self
            .state
            .actors
            .get(peer_actor_id)
            .cloned()
            .unwrap_or_else(|| {
                json!({
                    "id":peer_actor_id,
                    "kind":"human",
                    "displayName":peer_actor_id.trim_start_matches("human:")
                })
            });
        self.state
            .actors
            .entry(peer_actor_id.to_string())
            .or_insert(peer);
        self.state.contact_ids.insert(peer_actor_id.to_string());
        self.state
            .actors
            .entry(authenticated_actor_id.to_string())
            .or_insert_with(|| {
                json!({
                    "id":authenticated_actor_id,
                    "kind":"human",
                    "displayName":authenticated_actor_id.trim_start_matches("human:")
                })
            });

        let peer_display_name = self
            .state
            .actors
            .get(peer_actor_id)
            .and_then(|actor| actor.get("displayName"))
            .and_then(Value::as_str)
            .unwrap_or(peer_actor_id)
            .to_string();
        let conversation = self
            .state
            .conversations
            .entry(conversation_id.clone())
            .or_insert_with(|| {
                json!({
                    "id":conversation_id,
                    "kind":"direct",
                    "title":peer_display_name,
                    "ownerId":authenticated_actor_id,
                    "participants":[
                        {"actorId":authenticated_actor_id,"role":"owner","joinedAtMs":now_ms},
                        {"actorId":peer_actor_id,"role":"member","joinedAtMs":now_ms}
                    ],
                    "permissions":{
                        "canSendMessages":true,
                        "canSendMedia":true,
                        "canSendPolls":true,
                        "canPinMessages":true
                    },
                    "createdAtMs":now_ms,
                    "updatedAtMs":now_ms
                })
            })
            .clone();
        if !conversation_has_access(&conversation, authenticated_actor_id)
            || !conversation_has_access(&conversation, peer_actor_id)
        {
            self.state = previous;
            return Err("remote Human conversation membership conflicts with canonical state".into());
        }

        let mut changed = 0usize;
        let mut newest: Option<(i64, String)> = None;
        for remote in messages {
            let remote = remote
                .as_object()
                .ok_or("remote Human message must be an object")?;
            let remote_id = remote_human_identity_text(remote.get("id"), "message id")?;
            let sender_id = canonical_remote_human_actor_id(
                remote.get("senderUserId"),
                "sender user id",
            )?;
            let recipient_id = canonical_remote_human_actor_id(
                remote.get("recipientUserId"),
                "recipient user id",
            )?;
            let belongs = (sender_id == authenticated_actor_id && recipient_id == peer_actor_id)
                || (sender_id == peer_actor_id && recipient_id == authenticated_actor_id);
            if !belongs {
                self.state = previous;
                return Err("remote Human message does not belong to the requested conversation".into());
            }
            let text = remote
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let created_at = required_non_empty(remote.get("createdAt"), "remote Human createdAt")?;
            let created_at_ms = chrono::DateTime::parse_from_rfc3339(created_at)
                .map_err(|error| format!("remote Human message timestamp is invalid: {error}"))?
                .timestamp_millis();
            let message_id = format!("human-server-message:{remote_id}");
            let client_message_id = remote
                .get("clientRequestId")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let reply_to = remote
                .get("replyToMessageId")
                .filter(|value| !value.is_null())
                .map(|value| remote_human_identity_text(Some(value), "reply message id"))
                .transpose()?
                .map(|value| format!("human-server-message:{value}"));
            let delivery_state = if remote.get("readAt").and_then(Value::as_str).is_some() {
                "read"
            } else {
                "delivered"
            };
            let reaction_actors = project_remote_human_reaction_actors(
                remote.get("reactions"),
                authenticated_actor_id,
                peer_actor_id,
            )?;
            let attachments = project_remote_human_attachments(remote.get("attachments"))?;
            let content = if attachments.is_empty() {
                json!({"type":"text","data":{"text":{"text":text,"entities":[]}}})
            } else {
                json!({
                    "type":"document",
                    "data":{
                        "text":{"text":text,"entities":[]},
                        "remoteAttachments":attachments
                    }
                })
            };
            let projected = json!({
                "id":message_id,
                "clientMessageId":client_message_id,
                "remoteMessageId":remote_id,
                "conversationId":conversation_id,
                "senderId":sender_id,
                "content":content,
                "replyToMessageId":reply_to,
                "threadRootMessageId":Value::Null,
                "createdAtMs":created_at_ms,
                "editedAtMs":Value::Null,
                "scheduledAtMs":Value::Null,
                "silent":false,
                "protectedContent":false,
                "deliveryState":delivery_state,
                "reactions":[],
                "pinned":false,
                "deleted":false
            });

            let messages_for_conversation =
                self.state.messages.entry(conversation_id.clone()).or_default();
            match messages_for_conversation.get(&message_id) {
                Some(current) if current == &projected => {}
                Some(current) => {
                    if current
                        .get("remoteMessageId")
                        .and_then(Value::as_str)
                        != projected.get("remoteMessageId").and_then(Value::as_str)
                    {
                        self.state = previous;
                        return Err("remote Human message identity conflicts with canonical state".into());
                    }
                    messages_for_conversation.insert(message_id.clone(), projected);
                    changed += 1;
                }
                None => {
                    if let Some(client_message_id) = client_message_id.as_deref() {
                        let settlement_key = stable_message_id(authenticated_actor_id, client_message_id);
                        if let Some(local) = messages_for_conversation.remove(&settlement_key) {
                            if local.get("senderId").and_then(Value::as_str)
                                != Some(authenticated_actor_id)
                            {
                                self.state = previous;
                                return Err("remote Human clientRequestId conflicts with another sender".into());
                            }
                        }
                    }
                    messages_for_conversation.insert(message_id.clone(), projected);
                    changed += 1;
                }
            }
            if reaction_actors.is_empty() {
                if let Some(messages) = self.state.reaction_actors.get_mut(&conversation_id) {
                    messages.remove(&message_id);
                    if messages.is_empty() {
                        self.state.reaction_actors.remove(&conversation_id);
                    }
                }
            } else {
                self.state
                    .reaction_actors
                    .entry(conversation_id.clone())
                    .or_default()
                    .insert(message_id.clone(), reaction_actors);
            }
            if newest
                .as_ref()
                .is_none_or(|(timestamp, _)| created_at_ms >= *timestamp)
            {
                newest = Some((created_at_ms, message_id));
            }
        }

        if changed == 0 {
            self.state = previous;
            return Ok(0);
        }
        if let Some((updated_at_ms, last_message_id)) = newest {
            if let Some(conversation) = self
                .state
                .conversations
                .get_mut(&conversation_id)
                .and_then(Value::as_object_mut)
            {
                conversation.insert("lastMessageId".into(), Value::String(last_message_id));
                conversation.insert("updatedAtMs".into(), json!(updated_at_ms));
            }
        }
        self.bump_cursor();
        self.state.event_journal.clear();
        self.state.journal_floor_cursor = self.state.cursor;
        if let Err(error) = self.persist() {
            self.state = previous;
            return Err(format!(
                "failed to persist canonical Android remote Human messages: {error}"
            ));
        }
        Ok(changed)
    }

    pub fn has_remote_human_message(
        &self,
        authenticated_actor_id: &str,
        peer_actor_id: &str,
        remote_message_id: &str,
    ) -> bool {
        let remote_message_id = remote_message_id.trim();
        if remote_message_id.is_empty() {
            return false;
        }
        let conversation_id =
            stable_human_direct_conversation_id(authenticated_actor_id, peer_actor_id);
        let message_id = format!("human-server-message:{remote_message_id}");
        self.state
            .messages
            .get(&conversation_id)
            .is_some_and(|messages| messages.contains_key(&message_id))
    }

    fn canonical_remote_human_attachment(
        &self,
        authenticated_actor_id: &str,
        conversation_id: &str,
        message_id: &str,
        resource_id: &str,
    ) -> Result<Value, String> {
        let authenticated_actor_id = authenticated_actor_id.trim();
        if !valid_actor_id(authenticated_actor_id) || !authenticated_actor_id.starts_with("human:") {
            return Err("remote Human attachment actor identity is invalid".into());
        }
        if !bounded_id(conversation_id) || !bounded_id(message_id) {
            return Err("remote Human attachment conversation/message identity is invalid".into());
        }
        if resource_id.trim().is_empty()
            || resource_id.len() > 128
            || resource_id.contains('/')
            || resource_id.contains('\\')
            || resource_id.chars().any(char::is_control)
        {
            return Err("remote Human attachment resource id is invalid".into());
        }
        let conversation = self
            .state
            .conversations
            .get(conversation_id)
            .ok_or("remote Human attachment conversation not found")?;
        if !conversation_has_access(conversation, authenticated_actor_id) {
            return Err("remote Human attachment conversation access denied".into());
        }
        let message = self
            .state
            .messages
            .get(conversation_id)
            .and_then(|messages| messages.get(message_id))
            .ok_or("remote Human attachment message not found")?;
        if message.get("conversationId").and_then(Value::as_str) != Some(conversation_id)
            || message
                .get("remoteMessageId")
                .and_then(Value::as_str)
                .is_none()
        {
            return Err("remote Human attachment must belong to a canonical server message".into());
        }
        let attachments = message
            .get("content")
            .and_then(|content| content.get("data"))
            .and_then(|data| data.get("remoteAttachments"))
            .and_then(Value::as_array)
            .ok_or("canonical remote Human message has no downloadable attachments")?;
        let attachment = attachments
            .iter()
            .find(|attachment| {
                attachment.get("resourceId").and_then(Value::as_str) == Some(resource_id)
            })
            .ok_or("remote Human attachment resource is not present on the canonical message")?;
        let name = required_non_empty(attachment.get("name"), "remote Human attachment name")?;
        let size = attachment
            .get("size")
            .and_then(Value::as_u64)
            .filter(|size| *size > 0 && *size <= 32 * 1024 * 1024)
            .ok_or("remote Human attachment size is invalid")?;
        let content_type = attachment
            .get("contentType")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty() && value.len() <= 255)
            .unwrap_or("application/octet-stream");
        Ok(json!({
            "conversationId":conversation_id,
            "messageId":message_id,
            "resourceId":resource_id,
            "fileName":name,
            "contentType":content_type,
            "sizeBytes":size,
            "localBlobId":attachment.get("localBlobId").cloned().unwrap_or(Value::Null),
            "localUrl":attachment.get("localUrl").cloned().unwrap_or(Value::Null),
        }))
    }

    /// Resolve a remote Human attachment strictly from canonical message state. Presentation may
    /// request a conversation/message/resource tuple, but it cannot invent size/name/MIME or bytes.
    pub fn remote_human_attachment_download_intent(
        &self,
        authenticated_actor_id: &str,
        command: &Map<String, Value>,
    ) -> Result<Option<Value>, String> {
        if command.get("type").and_then(Value::as_str)
            != Some("downloadRemoteHumanAttachment")
        {
            return Ok(None);
        }
        let conversation_id =
            required_non_empty(command.get("conversationId"), "remote Human conversation id")?;
        let message_id =
            required_non_empty(command.get("messageId"), "remote Human message id")?;
        let resource_id =
            required_non_empty(command.get("resourceId"), "remote Human resource id")?;
        self.canonical_remote_human_attachment(
            authenticated_actor_id,
            conversation_id,
            message_id,
            resource_id,
        )
        .map(Some)
    }

    /// Import account-owned downloaded bytes into the canonical messaging blob store and then
    /// atomically publish the local blob reference on the existing server message.
    pub fn import_remote_human_attachment(
        &mut self,
        authenticated_actor_id: &str,
        conversation_id: &str,
        message_id: &str,
        resource_id: &str,
        bytes: &[u8],
        now_ms: i64,
    ) -> Result<Value, String> {
        let descriptor = self.canonical_remote_human_attachment(
            authenticated_actor_id,
            conversation_id,
            message_id,
            resource_id,
        )?;
        let expected_size = descriptor
            .get("sizeBytes")
            .and_then(Value::as_u64)
            .ok_or("canonical remote Human attachment size is missing")?;
        if bytes.len() as u64 != expected_size {
            return Err(format!(
                "remote Human attachment body length mismatch: expected {expected_size}, received {}",
                bytes.len()
            ));
        }

        let mut identity = Vec::new();
        for value in [
            authenticated_actor_id,
            conversation_id,
            message_id,
            resource_id,
        ] {
            identity.extend_from_slice(value.as_bytes());
            identity.push(0);
        }
        let blob_id = BlobId::new(format!(
            "remote-{}",
            crate::sha256::sha256_hex(&identity)
        ))
        .map_err(|error| error.to_string())?;
        let file_name = required_non_empty(descriptor.get("fileName"), "remote Human file name")?;
        let mime_type = required_non_empty(
            descriptor.get("contentType"),
            "remote Human content type",
        )?;
        let content_hash = crate::sha256::sha256_hex(bytes);
        let metadata = BlobMetadata {
            id: blob_id.clone(),
            file_name: file_name.to_string(),
            mime_type: mime_type.to_string(),
            size_bytes: expected_size,
            content_hash: Some(content_hash),
            created_at_ms: now_ms,
            owner_actor_id: Some(authenticated_actor_id.to_string()),
        };

        let existing_local = descriptor
            .get("localBlobId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        if let Some(existing_local) = existing_local {
            if existing_local != blob_id.0 {
                return Err("canonical remote Human attachment local blob identity conflicts".into());
            }
            let current = self
                .blobs
                .metadata(&blob_id)
                .map_err(|error| format!("canonical remote Human blob is unavailable: {error}"))?;
            if current.owner_actor_id.as_deref() != Some(authenticated_actor_id)
                || current.size_bytes != expected_size
                || current.file_name != file_name
                || current.mime_type != mime_type
            {
                return Err("canonical remote Human blob metadata conflicts".into());
            }
            return Ok(json!({
                "blob":{
                    "id":blob_id.0.clone(),
                    "fileName":current.file_name,
                    "mimeType":current.mime_type,
                    "sizeBytes":current.size_bytes,
                    "remoteUrl":format!("fabushi-blob://{}", blob_id.0)
                },
                "alreadyDownloaded":true
            }));
        }

        let status = self
            .blobs
            .begin_upload(&metadata)
            .map_err(|error| format!("failed to begin remote Human blob import: {error}"))?;
        if !status.complete {
            let mut offset = usize::try_from(status.uploaded_bytes)
                .map_err(|_| "remote Human blob resume offset is invalid")?;
            if offset > bytes.len() {
                return Err("remote Human blob resume offset exceeds downloaded body".into());
            }
            while offset < bytes.len() {
                let end = (offset + MAX_BLOB_RANGE_BYTES as usize).min(bytes.len());
                self.blobs
                    .append_chunk(&blob_id, offset as u64, &bytes[offset..end])
                    .map_err(|error| format!("failed to append remote Human blob: {error}"))?;
                offset = end;
            }
            self.blobs
                .finish_upload(&blob_id)
                .map_err(|error| format!("failed to settle remote Human blob: {error}"))?;
        }

        let previous = self.state.clone();
        let previous_cursor = self.state.cursor;
        let updated_message = {
            let message = self
                .state
                .messages
                .get_mut(conversation_id)
                .and_then(|messages| messages.get_mut(message_id))
                .ok_or("canonical remote Human message disappeared during attachment import")?;
            let attachments = message
                .get_mut("content")
                .and_then(|content| content.get_mut("data"))
                .and_then(|data| data.get_mut("remoteAttachments"))
                .and_then(Value::as_array_mut)
                .ok_or("canonical remote Human attachment list disappeared during import")?;
            let attachment = attachments
                .iter_mut()
                .find(|attachment| {
                    attachment.get("resourceId").and_then(Value::as_str) == Some(resource_id)
                })
                .ok_or("canonical remote Human attachment disappeared during import")?;
            let object = attachment
                .as_object_mut()
                .ok_or("canonical remote Human attachment is invalid")?;
            object.insert("localBlobId".into(), Value::String(blob_id.0.clone()));
            object.insert(
                "localUrl".into(),
                Value::String(format!("fabushi-blob://{}", blob_id.0)),
            );
            object.insert("downloadedAtMs".into(), json!(now_ms));
            message.clone()
        };
        self.bump_cursor();
        let projected = self.project_message(&updated_message, authenticated_actor_id);
        let envelope = self.server_envelope(
            json!({"type":"messageChanged","message":projected.clone()}),
            now_ms,
        );
        if let Err(error) =
            self.record_journal_result(authenticated_actor_id, previous_cursor, &[envelope.clone()])
        {
            self.state = previous;
            return Err(error);
        }
        if let Err(error) = self.persist() {
            self.state = previous;
            return Err(format!(
                "failed to persist canonical remote Human attachment import: {error}"
            ));
        }
        Ok(json!({
            "blob":{
                "id":blob_id.0.clone(),
                "fileName":metadata.file_name,
                "mimeType":metadata.mime_type,
                "sizeBytes":metadata.size_bytes,
                "remoteUrl":format!("fabushi-blob://{}", blob_id.0)
            },
            "message":projected,
            "envelope":envelope,
            "alreadyDownloaded":false
        }))
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
        let previous_cursor = previous.cursor;
        let mut journal_cursor = previous_cursor;

        if command_type == "sync" {
            let delivery_envelopes =
                match self.mark_direct_messages_delivered(authenticated_actor_id, now_ms) {
                    Ok(envelopes) => envelopes,
                    Err(error) => {
                        self.state = previous;
                        return Err(error);
                    }
                };
            if let Err(error) = self.record_journal_result(
                authenticated_actor_id,
                journal_cursor,
                &delivery_envelopes,
            ) {
                self.state = previous;
                return Err(error);
            }
            journal_cursor = self.state.cursor;
        }

        let envelopes = match self.execute_command(command_type, command, authenticated_actor_id, now_ms) {
            Ok(envelopes) => envelopes,
            Err(error) => {
                self.state = previous;
                return Err(error);
            }
        };
        if let Err(error) = self.record_journal_result(
            authenticated_actor_id,
            journal_cursor,
            &envelopes,
        ) {
            self.state = previous;
            return Err(error);
        }
        let result = json!({"requestId":request_id,"envelopes":envelopes});

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
            "sync" => self.sync_response(command, actor_id, now_ms),
            "upsertProfile" => self.upsert_profile(command, actor_id, now_ms),
            "createConversation" => self.create_conversation(command, actor_id, now_ms),
            "ensureDirectConversation" => {
                self.ensure_direct_human_conversation(command, actor_id, now_ms)
            }
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
        if !valid_actor_id(target_id) {
            return Err("messaging actor id is invalid".into());
        }
        let display_name = required_non_empty(actor.get("displayName"), "messaging actor displayName")?;
        if display_name.trim().is_empty() {
            return Err("messaging actor displayName is invalid".into());
        }
        let kind = required_non_empty(actor.get("kind"), "messaging actor kind")?;
        if !matches!(kind, "human" | "assistant" | "bot" | "service") {
            return Err("messaging actor kind is invalid".into());
        }
        if let Some(presence) = actor.get("presence").filter(|value| !value.is_null()) {
            let presence = presence
                .as_object()
                .ok_or("messaging actor presence must be an object")?;
            let status = required_non_empty(presence.get("status"), "messaging presence status")?;
            if !matches!(status, "offline" | "online" | "away" | "doNotDisturb") {
                return Err("messaging presence status is invalid".into());
            }
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

    fn ensure_direct_human_conversation(
        &mut self,
        command: &Map<String, Value>,
        actor_id: &str,
        now_ms: i64,
    ) -> Result<Vec<Value>, String> {
        let peer_actor_id =
            required_non_empty(command.get("peerActorId"), "direct Human peer actor id")?;
        if !valid_actor_id(actor_id)
            || !actor_id.starts_with("human:")
            || !valid_actor_id(peer_actor_id)
            || !peer_actor_id.starts_with("human:")
            || peer_actor_id == actor_id
        {
            return Err("direct Human conversation actor identity is invalid".into());
        }
        if !self.state.contact_ids.contains(peer_actor_id) {
            return Err("direct Human conversation peer is not in the canonical contact roster".into());
        }
        let actor = self
            .state
            .actors
            .get(actor_id)
            .ok_or("authenticated Human actor is missing from canonical messaging state")?;
        if actor.get("kind").and_then(Value::as_str) != Some("human") {
            return Err("authenticated messaging actor is not Human".into());
        }
        let peer = self
            .state
            .actors
            .get(peer_actor_id)
            .ok_or("direct Human peer actor is missing from canonical messaging state")?;
        if peer.get("kind").and_then(Value::as_str) != Some("human") {
            return Err("direct Human peer actor is not Human".into());
        }
        let conversation_id = stable_human_direct_conversation_id(actor_id, peer_actor_id);
        if let Some(existing) = self.state.conversations.get(&conversation_id).cloned() {
            if conversation_kind(&existing) != Some("direct")
                || !conversation_has_access(&existing, actor_id)
                || !conversation_has_access(&existing, peer_actor_id)
            {
                return Err("canonical direct Human conversation identity conflicts with stored membership".into());
            }
            return Ok(vec![self.server_envelope(
                json!({"type":"conversationChanged","conversation":existing}),
                now_ms,
            )]);
        }
        let peer_display_name = peer
            .get("displayName")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(peer_actor_id.trim_start_matches("human:"));
        let conversation = json!({
            "id":conversation_id,
            "kind":"direct",
            "title":peer_display_name,
            "ownerId":actor_id,
            "participants":[
                {"actorId":actor_id,"role":"owner","joinedAtMs":now_ms},
                {"actorId":peer_actor_id,"role":"member","joinedAtMs":now_ms}
            ],
            "permissions":{
                "canSendMessages":true,
                "canSendMedia":true,
                "canSendPolls":true,
                "canPinMessages":true
            },
            "createdAtMs":now_ms,
            "updatedAtMs":now_ms
        });
        self.state
            .conversations
            .insert(conversation_id, conversation.clone());
        self.bump_cursor();
        Ok(vec![self.server_envelope(
            json!({"type":"conversationChanged","conversation":conversation}),
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
        if !bounded_id(conversation_id) {
            return Err("conversation id is invalid".into());
        }
        let title = required_non_empty(conversation.get("title"), "conversation title")?;
        if title.trim().is_empty() {
            return Err("conversation title is invalid".into());
        }
        let kind = required_non_empty(conversation.get("kind"), "conversation kind")?;
        if !matches!(kind, "direct" | "group" | "channel" | "savedMessages" | "secret") {
            return Err("conversation kind is invalid".into());
        }
        let participants = conversation
            .get("participants")
            .and_then(Value::as_array)
            .ok_or("conversation participants are required")?;
        if !participants.iter().any(|participant| {
            participant.get("actorId").and_then(Value::as_str) == Some(actor_id)
        }) {
            return Err("conversation creator must be an owner/participant".into());
        }
        if conversation
            .get("ownerId")
            .and_then(Value::as_str)
            .is_some_and(|owner| owner != actor_id)
        {
            return Err("conversation creator must be its owner/participant".into());
        }
        for participant in participants {
            let participant = participant
                .as_object()
                .ok_or("conversation participant must be an object")?;
            let participant_id =
                required_non_empty(participant.get("actorId"), "participant actorId")?;
            let role = required_non_empty(participant.get("role"), "participant role")?;
            if !valid_actor_id(participant_id)
                || !matches!(role, "owner" | "admin" | "member" | "restricted")
            {
                return Err("conversation participant data is invalid".into());
            }
            if !self.state.actors.contains_key(participant_id) {
                return Err("conversation participant actor does not exist".into());
            }
        }
        if kind == "secret" && participants.len() != 2 {
            return Err("secret conversations must contain exactly two participants".into());
        }
        if let Some(existing) = self.state.conversations.get(conversation_id) {
            if matches!(conversation_kind(existing), Some("group") | Some("channel")) {
                let owner_changed =
                    existing.get("ownerId").cloned().unwrap_or(Value::Null)
                        != conversation.get("ownerId").cloned().unwrap_or(Value::Null);
                let participants_changed =
                    existing.get("participants").cloned().unwrap_or(Value::Array(Vec::new()))
                        != conversation.get("participants").cloned().unwrap_or(Value::Array(Vec::new()));
                let topics_changed =
                    existing.get("topics").cloned().unwrap_or(Value::Array(Vec::new()))
                        != conversation.get("topics").cloned().unwrap_or(Value::Array(Vec::new()));
                if owner_changed || participants_changed || topics_changed {
                    return Err(
                        "group/channel membership and topic state require dedicated mutations".into(),
                    );
                }
            }
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

    /// Derive the server mutation for a canonical direct-Human text send.
    ///
    /// The Host owns transport; this repository owns membership/content truth. Returning None
    /// means the command is not a Human remote direct send. Unsupported remote content fails
    /// closed instead of silently becoming a local-only success.
    pub fn remote_human_send_intent(
        &self,
        actor_id: &str,
        command: &Map<String, Value>,
    ) -> Result<Option<Value>, String> {
        if command.get("type").and_then(Value::as_str) != Some("sendMessage") {
            return Ok(None);
        }
        let conversation_id =
            required_non_empty(command.get("conversationId"), "conversation id")?;
        let conversation = self.require_conversation_access(conversation_id, actor_id)?;
        if conversation_kind(conversation) != Some("direct") {
            return Ok(None);
        }
        let participants = conversation
            .get("participants")
            .and_then(Value::as_array)
            .ok_or("direct Human conversation participants are required")?;
        if participants.len() != 2 {
            return Ok(None);
        }
        let mut peer_actor_id = None;
        for participant in participants {
            let participant_id = required_non_empty(
                participant.get("actorId"),
                "direct Human participant actor id",
            )?;
            let is_human = self
                .state
                .actors
                .get(participant_id)
                .and_then(|actor| actor.get("kind"))
                .and_then(Value::as_str)
                == Some("human");
            if !is_human {
                return Ok(None);
            }
            if participant_id != actor_id {
                if peer_actor_id.replace(participant_id.to_string()).is_some() {
                    return Ok(None);
                }
            }
        }
        let peer_actor_id =
            peer_actor_id.ok_or("direct Human conversation omitted the peer actor")?;
        if !self.state.contact_ids.contains(&peer_actor_id) {
            return Ok(None);
        }
        if conversation_id != stable_human_direct_conversation_id(actor_id, &peer_actor_id) {
            return Err(
                "remote Human send requires the canonical server-backed direct conversation identity"
                    .into(),
            );
        }
        let client_request_id =
            required_non_empty(command.get("clientMessageId"), "clientMessageId")?;
        if !bounded_id(client_request_id) {
            return Err("remote Human clientMessageId is invalid".into());
        }
        if command
            .get("threadRootMessageId")
            .is_some_and(|value| !value.is_null())
            || command
                .get("scheduledAtMs")
                .is_some_and(|value| !value.is_null())
            || command.get("silent").and_then(Value::as_bool).unwrap_or(false)
            || command
                .get("protectedContent")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        {
            return Err(
                "remote Human send does not yet support thread, scheduled, silent, or protected-content semantics"
                    .into(),
            );
        }
        let content = command
            .get("content")
            .and_then(Value::as_object)
            .ok_or("remote Human send content object is required")?;
        self.validate_content_for_send(conversation, &Value::Object(content.clone()), actor_id)?;
        let content_type = content
            .get("type")
            .and_then(Value::as_str)
            .ok_or("remote Human send content type is required")?;
        let (text, attachments) = match content_type {
            "text" => {
                let text = content
                    .get("data")
                    .and_then(Value::as_object)
                    .and_then(|data| data.get("text"))
                    .and_then(Value::as_object)
                    .and_then(|text| text.get("text"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .ok_or("remote Human text content is empty")?;
                if text.len() > 2 * 1024 * 1024 {
                    return Err("remote Human text content is too large".into());
                }
                (text.to_string(), Vec::<Value>::new())
            }
            "photo" | "video" | "animation" | "audio" | "voice" | "document" => {
                let data = content
                    .get("data")
                    .and_then(Value::as_object)
                    .ok_or("remote Human media data is required")?;
                let media = data
                    .get("media")
                    .and_then(Value::as_object)
                    .ok_or("remote Human media descriptor is required")?;
                let blob_id = required_non_empty(media.get("id"), "remote Human attachment blob id")?;
                let id = BlobId::new(blob_id.to_string()).map_err(|error| error.to_string())?;
                let metadata = self.blobs.metadata(&id).map_err(|error| error.to_string())?;
                if metadata.owner_actor_id.as_deref() != Some(actor_id) {
                    return Err("remote Human attachment requires durable blob ownership".into());
                }
                if metadata.size_bytes == 0 || metadata.size_bytes > 32 * 1024 * 1024 {
                    return Err("remote Human attachment size exceeds the backend limit".into());
                }
                let caption = data
                    .get("caption")
                    .and_then(Value::as_object)
                    .and_then(|caption| caption.get("text"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or_default();
                if caption.len() > 2 * 1024 * 1024 {
                    return Err("remote Human attachment caption is too large".into());
                }
                (
                    caption.to_string(),
                    vec![json!({
                        "blobId":metadata.id.0,
                        "fileName":metadata.file_name,
                        "mimeType":metadata.mime_type,
                        "sizeBytes":metadata.size_bytes,
                    })],
                )
            }
            _ => {
                return Err(format!(
                    "remote Human send content type {content_type} is not supported by the backend"
                ));
            }
        };

        let reply_to_remote_message_id =
            if let Some(reply_to) = optional_string(command.get("replyToMessageId")) {
                let reply = self.require_message(conversation_id, &reply_to)?;
                Some(
                    required_non_empty(
                        reply.get("remoteMessageId"),
                        "remote Human reply server message id",
                    )?
                    .to_string(),
                )
            } else {
                None
            };
        Ok(Some(json!({
            "conversationId":conversation_id,
            "peerActorId":peer_actor_id,
            "targetUserId":peer_actor_id.trim_start_matches("human:"),
            "clientRequestId":client_request_id,
            "text":text,
            "replyToMessageId":reply_to_remote_message_id,
            "attachments":attachments,
        })))
    }

    pub fn remote_human_server_echo_for_client_request(
        &self,
        conversation_id: &str,
        actor_id: &str,
        client_request_id: &str,
    ) -> Option<(String, String)> {
        self.state
            .messages
            .get(conversation_id)?
            .values()
            .find(|message| {
                message.get("senderId").and_then(Value::as_str) == Some(actor_id)
                    && message.get("clientMessageId").and_then(Value::as_str)
                        == Some(client_request_id)
                    && message
                        .get("remoteMessageId")
                        .and_then(Value::as_str)
                        .is_some()
            })
            .and_then(|message| {
                Some((
                    message.get("id")?.as_str()?.to_string(),
                    message.get("remoteMessageId")?.as_str()?.to_string(),
                ))
            })
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
        let stable_id = stable_message_id(actor_id, client_message_id);
        if let Some(existing) = self
            .state
            .messages
            .get(conversation_id)
            .and_then(|messages| {
                messages
                    .get(&stable_id)
                    .or_else(|| messages.get(client_message_id))
                    .or_else(|| {
                        messages.values().find(|message| {
                            message.get("senderId").and_then(Value::as_str) == Some(actor_id)
                                && message.get("clientMessageId").and_then(Value::as_str)
                                    == Some(client_message_id)
                        })
                    })
            })
            .cloned()
        {
            let same_payload = existing.get("senderId").and_then(Value::as_str) == Some(actor_id)
                && existing.get("content").cloned().unwrap_or(Value::Null)
                    == command.get("content").cloned().unwrap_or(Value::Null)
                && existing.get("replyToMessageId").cloned().unwrap_or(Value::Null)
                    == command.get("replyToMessageId").cloned().unwrap_or(Value::Null)
                && existing.get("threadRootMessageId").cloned().unwrap_or(Value::Null)
                    == command.get("threadRootMessageId").cloned().unwrap_or(Value::Null)
                && existing.get("scheduledAtMs").cloned().unwrap_or(Value::Null)
                    == command.get("scheduledAtMs").cloned().unwrap_or(Value::Null)
                && existing.get("silent").and_then(Value::as_bool).unwrap_or(false)
                    == command.get("silent").and_then(Value::as_bool).unwrap_or(false)
                && existing
                    .get("protectedContent")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                    == command
                        .get("protectedContent")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
            if !same_payload {
                return Err(format!(
                    "clientMessageId {client_message_id} conflicts with the canonical message payload"
                ));
            }
            return Ok(vec![self.server_envelope(
                json!({"type":"messageChanged","message":self.project_message(&existing, actor_id)}),
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
            "id":stable_id,
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
            .insert(stable_id.clone(), message.clone());
        let mut updated_conversation = conversation;
        if let Some(object) = updated_conversation.as_object_mut() {
            object.insert("lastMessageId".into(), Value::String(stable_id));
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

    /// Derive forward-recipient candidates from canonical messaging truth.
    ///
    /// Presentation never supplies membership or permission facts. The source message and every
    /// candidate destination are revalidated against this repository before a candidate is
    /// projected.
    pub fn search_forward_recipients(
        &self,
        authenticated_actor_id: &str,
        source_conversation_id: &str,
        source_message_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Value>, String> {
        let actor_id = authenticated_actor_id.trim();
        let source_conversation_id = source_conversation_id.trim();
        let source_message_id = source_message_id.trim();
        if actor_id.is_empty() || source_conversation_id.is_empty() || source_message_id.is_empty() {
            return Err("recipient search requires actor, source conversation and source message".into());
        }
        self.require_conversation_access(source_conversation_id, actor_id)?;
        let source = self.require_message(source_conversation_id, source_message_id)?;
        if source.get("deleted").and_then(Value::as_bool).unwrap_or(false) {
            return Err("recipient search source message is deleted".into());
        }
        if !matches!(
            source.get("deliveryState").and_then(Value::as_str),
            Some("sent" | "delivered" | "read")
        ) {
            return Err("recipient search source must be a settled canonical message".into());
        }
        if source
            .get("protectedContent")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Ok(Vec::new());
        }
        let content = source
            .get("content")
            .ok_or("recipient search source content is missing")?;
        let normalized_query = query.trim().to_lowercase();
        let bounded_limit = limit.clamp(1, 100);

        let mut candidates = self
            .state
            .conversations
            .values()
            .filter(|conversation| conversation_has_access(conversation, actor_id))
            .filter(|conversation| conversation_can_send(conversation, actor_id, content))
            .filter(|conversation| {
                let Some(participants) = conversation.get("participants").and_then(Value::as_array) else {
                    return false;
                };
                let mut has_local_human = false;
                let mut has_other_human = false;
                for participant in participants {
                    let Some(participant_id) =
                        participant.get("actorId").and_then(Value::as_str)
                    else {
                        continue;
                    };
                    let is_human = self
                        .state
                        .actors
                        .get(participant_id)
                        .and_then(|actor| actor.get("kind"))
                        .and_then(Value::as_str)
                        == Some("human");
                    if participant_id == actor_id && is_human {
                        has_local_human = true;
                    } else if is_human {
                        has_other_human = true;
                    }
                }
                has_local_human && has_other_human
            })
            .filter(|conversation| {
                if normalized_query.is_empty() {
                    return true;
                }
                let title_matches = conversation
                    .get("title")
                    .and_then(Value::as_str)
                    .is_some_and(|title| title.to_lowercase().contains(&normalized_query));
                let participant_matches = conversation
                    .get("participants")
                    .and_then(Value::as_array)
                    .is_some_and(|participants| {
                        participants.iter().any(|participant| {
                            let Some(participant_id) =
                                participant.get("actorId").and_then(Value::as_str)
                            else {
                                return false;
                            };
                            participant_id.to_lowercase().contains(&normalized_query)
                                || self
                                    .state
                                    .actors
                                    .get(participant_id)
                                    .and_then(|actor| actor.get("displayName"))
                                    .and_then(Value::as_str)
                                    .is_some_and(|name| {
                                        name.to_lowercase().contains(&normalized_query)
                                    })
                        })
                    });
                title_matches || participant_matches
            })
            .map(|conversation| {
                let participant_ids = conversation
                    .get("participants")
                    .and_then(Value::as_array)
                    .map(|participants| {
                        participants
                            .iter()
                            .filter_map(|participant| {
                                participant
                                    .get("actorId")
                                    .and_then(Value::as_str)
                                    .map(str::to_string)
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                json!({
                    "id": conversation.get("id").cloned().unwrap_or(Value::Null),
                    "kind": "human",
                    "title": conversation.get("title").cloned().unwrap_or_else(|| Value::String("Human conversation".into())),
                    "participantIds": participant_ids,
                    "updatedAt": conversation
                        .get("updatedAtMs")
                        .cloned()
                        .unwrap_or_else(|| json!(0)),
                    "_sortUpdatedAtMs": conversation
                        .get("updatedAtMs")
                        .and_then(Value::as_i64)
                        .unwrap_or(0),
                })
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| {
            right
                .get("_sortUpdatedAtMs")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .cmp(
                    &left
                        .get("_sortUpdatedAtMs")
                        .and_then(Value::as_i64)
                        .unwrap_or(0),
                )
                .then_with(|| {
                    left.get("title")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .cmp(right.get("title").and_then(Value::as_str).unwrap_or_default())
                })
                .then_with(|| {
                    left.get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .cmp(right.get("id").and_then(Value::as_str).unwrap_or_default())
                })
        });
        candidates.truncate(bounded_limit);
        for candidate in &mut candidates {
            if let Some(object) = candidate.as_object_mut() {
                object.remove("_sortUpdatedAtMs");
            }
        }
        Ok(candidates)
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
        // Desktop group Agent messaging is intentionally text-only and priority is 1:1-only.
        // Keep the original arguments in the replay key, but never project unsupported group
        // media into canonical messages or wake payloads.
        let delivered_images: &[Value] = if target_is_group { &[] } else { images };
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
                    "agentImages":delivered_images,
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
                        "images":delivered_images,
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
            "createdAtMs":now_ms,
            "priorityApplied":!target_is_group && priority,
            "imagesDelivered":!target_is_group || images.is_empty(),
            "droppedImageCount":if target_is_group { images.len() } else { 0 },
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

    /// Remove only unsettled Agent-to-Agent wake work that references a deleted Agent.
    ///
    /// Historical delivered messages remain canonical transcript truth; deletion only prevents
    /// durable deferred work from resurrecting the removed Agent after process restart.
    pub fn clear_agent_wakes_for_deleted_agent(
        &mut self,
        account_fence: &str,
        agent_id: &str,
    ) -> Result<usize, String> {
        let account_fence = account_fence.trim();
        let agent_id = agent_id.trim();
        if account_fence.is_empty() || agent_id.is_empty() {
            return Err("Agent wake cleanup requires account fence and Agent id".into());
        }
        let previous = self.state.clone();
        let before = self.state.agent_wakes.len();
        self.state.agent_wakes.retain(|_, wake| {
            let same_account =
                wake.get("accountFence").and_then(Value::as_str) == Some(account_fence);
            let references_agent =
                wake.get("sourceAgentId").and_then(Value::as_str) == Some(agent_id)
                    || wake.get("targetAgentId").and_then(Value::as_str) == Some(agent_id);
            !(same_account && references_agent)
        });
        let removed = before.saturating_sub(self.state.agent_wakes.len());
        if removed == 0 {
            return Ok(0);
        }
        if let Err(error) = self.persist() {
            self.state = previous;
            return Err(format!("failed to persist deleted-Agent wake cleanup: {error}"));
        }
        Ok(removed)
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

    /// Read one complete canonical blob for account-owned remote transport.
    ///
    /// Presentation cannot call this through the public messaging command protocol. The Host must
    /// provide the authenticated actor id, and the durable blob owner remains authoritative.
    pub fn read_owned_blob_for_remote_transport(
        &self,
        actor_id: &str,
        blob_id: &str,
    ) -> Result<(String, String, u64, Vec<u8>), String> {
        const MAX_REMOTE_RESOURCE_BYTES: u64 = 32 * 1024 * 1024;
        let id = BlobId::new(blob_id.to_string()).map_err(|error| error.to_string())?;
        let metadata = self.blobs.metadata(&id).map_err(|error| error.to_string())?;
        if metadata.owner_actor_id.as_deref() != Some(actor_id) {
            return Err("remote Human attachment read requires durable blob ownership".into());
        }
        if metadata.size_bytes == 0 || metadata.size_bytes > MAX_REMOTE_RESOURCE_BYTES {
            return Err("remote Human attachment size exceeds the backend limit".into());
        }
        let mut offset = 0_u64;
        let mut bytes = Vec::with_capacity(metadata.size_bytes as usize);
        while offset < metadata.size_bytes {
            let length = (metadata.size_bytes - offset).min(MAX_BLOB_RANGE_BYTES);
            let chunk = self
                .blobs
                .read_range(&id, offset, length)
                .map_err(|error| error.to_string())?;
            if chunk.is_empty() {
                return Err("remote Human attachment blob ended before declared size".into());
            }
            offset = offset.saturating_add(chunk.len() as u64);
            bytes.extend_from_slice(&chunk);
        }
        if bytes.len() as u64 != metadata.size_bytes {
            return Err("remote Human attachment blob size mismatched durable metadata".into());
        }
        Ok((
            metadata.file_name,
            metadata.mime_type,
            metadata.size_bytes,
            bytes,
        ))
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
        let conversation = self.require_conversation_access(conversation_id, actor_id)?.clone();
        let message_id = required_non_empty(command.get("messageId"), "message id")?;
        let should_mark_message_read = {
            let message = self.require_message(conversation_id, message_id)?;
            matches!(conversation_kind(&conversation), Some("direct") | Some("secret"))
                && message.get("senderId").and_then(Value::as_str) != Some(actor_id)
        };

        let preferences = self.conversation_preferences_mut(actor_id, conversation_id);
        preferences.insert("lastReadMessageId".into(), Value::String(message_id.to_string()));
        preferences.insert("unreadCount".into(), json!(0));
        preferences.insert("markedUnread".into(), json!(false));

        if should_mark_message_read {
            let message = self
                .state
                .messages
                .get_mut(conversation_id)
                .and_then(|messages| messages.get_mut(message_id))
                .ok_or("message disappeared while marking it read")?;
            let object = message
                .as_object_mut()
                .ok_or("message is not a canonical object")?;
            object.insert("deliveryState".into(), Value::String("read".into()));
        }

        self.state.cursor = self
            .state
            .cursor
            .saturating_add(if should_mark_message_read { 2 } else { 1 });

        let conversation = self
            .state
            .conversations
            .get(conversation_id)
            .ok_or("conversation does not exist")?
            .clone();
        let mut responses = vec![self.server_envelope(
            json!({"type":"conversationChanged","conversation":self.project_conversation(&conversation, actor_id, now_ms)}),
            now_ms,
        )];
        if should_mark_message_read {
            let message = self
                .state
                .messages
                .get(conversation_id)
                .and_then(|messages| messages.get(message_id))
                .ok_or("message disappeared after marking it read")?
                .clone();
            responses.push(self.server_envelope(
                json!({"type":"messageChanged","message":self.project_message(&message, actor_id)}),
                now_ms,
            ));
        }
        Ok(responses)
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
            json!({"type":"conversationChanged","conversation":self.project_conversation(conversation, actor_id, now_ms)}),
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
            json!({"type":"conversationChanged","conversation":self.project_conversation(conversation, actor_id, now_ms)}),
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
            json!({"type":"conversationChanged","conversation":self.project_conversation(&conversation, actor_id, now_ms)}),
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
        let caller_is_owner = conversation_owner(&conversation) == Some(actor_id);
        let caller_role = conversation
            .get("participants")
            .and_then(Value::as_array)
            .and_then(|participants| {
                participants.iter().find(|participant| {
                    participant.get("actorId").and_then(Value::as_str) == Some(actor_id)
                })
            })
            .and_then(|participant| participant.get("role"))
            .and_then(Value::as_str);
        let existing_target_role = conversation
            .get("participants")
            .and_then(Value::as_array)
            .and_then(|participants| {
                participants.iter().find(|participant| {
                    participant.get("actorId").and_then(Value::as_str) == Some(target_actor_id)
                })
            })
            .and_then(|participant| participant.get("role"))
            .and_then(Value::as_str);
        if conversation_owner(&conversation) == Some(target_actor_id) && role != "owner" {
            return Err("conversation owner cannot be downgraded".into());
        }
        if role == "owner" && conversation_owner(&conversation) != Some(target_actor_id) {
            return Err("ownership transfer requires a dedicated verified contract".into());
        }
        if !caller_is_owner
            && caller_role == Some("admin")
            && (matches!(role, "owner" | "admin")
                || matches!(existing_target_role, Some("owner" | "admin")))
        {
            return Err("administrators cannot manage owner/admin participant roles".into());
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
            json!({"type":"conversationParticipantChanged","conversation":self.project_conversation(&conversation, actor_id, now_ms),"participant":Value::Object(participant)}),
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
        let caller_is_owner = conversation_owner(&conversation) == Some(actor_id);
        let caller_role = conversation
            .get("participants")
            .and_then(Value::as_array)
            .and_then(|participants| {
                participants.iter().find(|participant| {
                    participant.get("actorId").and_then(Value::as_str) == Some(actor_id)
                })
            })
            .and_then(|participant| participant.get("role"))
            .and_then(Value::as_str);
        let target_role = conversation
            .get("participants")
            .and_then(Value::as_array)
            .and_then(|participants| {
                participants.iter().find(|participant| {
                    participant.get("actorId").and_then(Value::as_str) == Some(target_actor_id)
                })
            })
            .and_then(|participant| participant.get("role"))
            .and_then(Value::as_str);
        if conversation_owner(&conversation) == Some(target_actor_id) {
            return Err("conversation owner cannot be removed".into());
        }
        if !caller_is_owner
            && caller_role == Some("admin")
            && matches!(target_role, Some("owner" | "admin"))
        {
            return Err("administrators cannot remove owner/admin participants".into());
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
            json!({"type":"conversationParticipantChanged","conversation":self.project_conversation(&conversation, actor_id, now_ms),"removedActorId":target_actor_id}),
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
        let stable_id = stable_message_id(actor_id, client_message_id);
        let privacy = command.get("privacy").and_then(Value::as_object);
        let drop_captions = privacy
            .and_then(|privacy| privacy.get("dropCaptions"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let drop_sender_names = drop_captions
            || privacy
                .and_then(|privacy| privacy.get("dropSenderNames"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
        let mut content = source.get("content").cloned().ok_or("source message content is missing")?;
        if drop_captions {
            clear_forward_caption(&mut content);
        }
        let forward_origin = if drop_sender_names {
            Value::Null
        } else {
            source
                .get("forwardOrigin")
                .filter(|value| !value.is_null())
                .cloned()
                .unwrap_or_else(|| Value::String(format!("{source_id}:{message_id}")))
        };
        let thread_root =
            command.get("threadRootMessageId").cloned().unwrap_or(Value::Null);
        let scheduled_at = command.get("scheduledAtMs").cloned().unwrap_or(Value::Null);
        let silent = command.get("silent").and_then(Value::as_bool).unwrap_or(false);
        if let Some(existing) = self
            .state
            .messages
            .get(destination_id)
            .and_then(|messages| {
                messages
                    .get(&stable_id)
                    .or_else(|| messages.get(client_message_id))
            })
            .cloned()
        {
            let same_payload =
                existing.get("senderId").and_then(Value::as_str) == Some(actor_id)
                    && existing.get("forwardOrigin").cloned().unwrap_or(Value::Null) == forward_origin
                    && existing.get("content").cloned().unwrap_or(Value::Null) == content
                    && existing.get("threadRootMessageId").cloned().unwrap_or(Value::Null) == thread_root
                    && existing.get("scheduledAtMs").cloned().unwrap_or(Value::Null) == scheduled_at
                    && existing.get("silent").and_then(Value::as_bool).unwrap_or(false) == silent;
            if !same_payload {
                return Err(format!(
                    "clientMessageId {client_message_id} conflicts with the canonical forward payload"
                ));
            }
            return Ok(vec![self.server_envelope(
                json!({"type":"messageChanged","message":self.project_message(&existing, actor_id)}),
                now_ms,
            )]);
        }
        if !conversation_can_send(&destination, actor_id, &content) {
            return Err("destination conversation does not permit this message".into());
        }
        let forwarded = json!({
            "id":stable_id,
            "clientMessageId":client_message_id,
            "conversationId":destination_id,
            "senderId":actor_id,
            "content":content,
            "replyToMessageId":Value::Null,
            "threadRootMessageId":thread_root,
            "createdAtMs":now_ms,
            "editedAtMs":Value::Null,
            "scheduledAtMs":scheduled_at,
            "silent":silent,
            "protectedContent":false,
            "deliveryState":"sent",
            "reactions":[],
            "pinned":false,
            "deleted":false,
            "forwardOrigin":forward_origin
        });
        self.state
            .messages
            .entry(destination_id.to_string())
            .or_default()
            .insert(stable_id.clone(), forwarded.clone());
        let mut updated = destination;
        if let Some(object) = updated.as_object_mut() {
            object.insert("lastMessageId".into(), Value::String(stable_id));
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

    fn project_conversation(
        &self,
        conversation: &Value,
        actor_id: &str,
        now_ms: i64,
    ) -> Value {
        let mut projected = conversation.clone();
        let Some(conversation_id) = conversation.get("id").and_then(Value::as_str) else {
            return projected;
        };
        let preferences = self
            .state
            .conversation_preferences
            .get(actor_id)
            .and_then(|values| values.get(conversation_id))
            .and_then(Value::as_object);
        if let Some(preferences) = preferences {
            if let Some(object) = projected.as_object_mut() {
                for (key, value) in preferences {
                    object.insert(key.clone(), value.clone());
                }
            }
        }

        let last_read_message_id = preferences
            .and_then(|values| values.get("lastReadMessageId"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let mut ordered_messages = self
            .state
            .messages
            .get(conversation_id)
            .into_iter()
            .flat_map(|messages| messages.values())
            .collect::<Vec<_>>();
        ordered_messages.sort_by(|left, right| {
            left.get("createdAtMs")
                .and_then(Value::as_i64)
                .unwrap_or_default()
                .cmp(
                    &right
                        .get("createdAtMs")
                        .and_then(Value::as_i64)
                        .unwrap_or_default(),
                )
                .then_with(|| {
                    left.get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .cmp(right.get("id").and_then(Value::as_str).unwrap_or_default())
                })
        });
        let read_index = last_read_message_id.and_then(|message_id| {
            ordered_messages
                .iter()
                .position(|message| message.get("id").and_then(Value::as_str) == Some(message_id))
        });
        let unread_count = ordered_messages
            .iter()
            .enumerate()
            .filter(|(index, message)| {
                let after_read = read_index.is_none_or(|read_index| *index > read_index);
                let scheduled_is_visible = message
                    .get("scheduledAtMs")
                    .and_then(Value::as_i64)
                    .is_none_or(|scheduled_at_ms| scheduled_at_ms <= now_ms);
                after_read
                    && scheduled_is_visible
                    && !message.get("deleted").and_then(Value::as_bool).unwrap_or(false)
                    && message.get("senderId").and_then(Value::as_str) != Some(actor_id)
            })
            .count();
        if let Some(object) = projected.as_object_mut() {
            object.insert(
                "lastReadMessageId".into(),
                last_read_message_id
                    .map(|value| Value::String(value.to_string()))
                    .unwrap_or(Value::Null),
            );
            object.insert("unreadCount".into(), json!(unread_count.min(u32::MAX as usize)));
            object.insert(
                "markedUnread".into(),
                json!(preferences
                    .and_then(|values| values.get("markedUnread"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false)),
            );
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

    fn mark_direct_messages_delivered(
        &mut self,
        actor_id: &str,
        now_ms: i64,
    ) -> Result<Vec<Value>, String> {
        let pending = self
            .state
            .conversations
            .iter()
            .filter(|(_, conversation)| {
                matches!(conversation_kind(conversation), Some("direct") | Some("secret"))
                    && conversation_has_access(conversation, actor_id)
            })
            .flat_map(|(conversation_id, _)| {
                self.state
                    .messages
                    .get(conversation_id)
                    .into_iter()
                    .flat_map(|messages| messages.iter())
                    .filter(|(_, message)| {
                        message.get("senderId").and_then(Value::as_str) != Some(actor_id)
                            && message.get("deliveryState").and_then(Value::as_str) == Some("sent")
                    })
                    .map(|(message_id, _)| (conversation_id.clone(), message_id.clone()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        if pending.is_empty() {
            return Ok(Vec::new());
        }

        self.state.cursor = self.state.cursor.saturating_add(pending.len() as u64);
        for (conversation_id, message_id) in &pending {
            let message = self
                .state
                .messages
                .get_mut(conversation_id)
                .and_then(|messages| messages.get_mut(message_id))
                .ok_or("message disappeared while marking delivery")?;
            let object = message
                .as_object_mut()
                .ok_or("message is not a canonical object")?;
            object.insert("deliveryState".into(), Value::String("delivered".into()));
        }

        let mut responses = Vec::with_capacity(pending.len());
        for (conversation_id, message_id) in pending {
            let message = self
                .state
                .messages
                .get(&conversation_id)
                .and_then(|messages| messages.get(&message_id))
                .ok_or("message disappeared after marking delivery")?
                .clone();
            responses.push(self.server_envelope(
                json!({"type":"messageChanged","message":self.project_message(&message, actor_id)}),
                now_ms,
            ));
        }
        Ok(responses)
    }

    fn sync_envelope(&self, actor_id: &str, limit: usize, now_ms: i64) -> Value {
        let max_items = limit.max(1);
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
            .map(|conversation| self.project_conversation(conversation, actor_id, now_ms))
            .collect::<Vec<_>>();
        let messages = visible_ids
            .iter()
            .filter_map(|id| self.state.messages.get(id))
            .flat_map(|messages| messages.values())
            .take(max_items)
            .map(|message| self.project_message(message, actor_id))
            .collect::<Vec<_>>();
        let actors = self
            .state
            .actors
            .iter()
            .filter(|(id, _)| {
                *id == actor_id
                    || self.state.contact_ids.contains(id.as_str())
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
            .take(max_items)
            .collect::<Vec<_>>();
        let folders = self
            .state
            .folders
            .get(actor_id)
            .map(|values| values.values().take(max_items).cloned().collect::<Vec<_>>())
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
                "fullSnapshot":true,
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

    fn sync_response(
        &self,
        command: &Map<String, Value>,
        actor_id: &str,
        now_ms: i64,
    ) -> Result<Vec<Value>, String> {
        let limit = command
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(1000)
            .clamp(1, 10_000) as usize;
        let Some(requested_cursor) = command
            .get("cursor")
            .and_then(Value::as_str)
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return Ok(vec![self.sync_envelope(actor_id, limit, now_ms)]);
        };
        if requested_cursor > self.state.cursor
            || requested_cursor < self.state.journal_floor_cursor
        {
            return Ok(vec![self.sync_envelope(actor_id, limit, now_ms)]);
        }

        let (slice, checkpoint_cursor, has_more) =
            self.journal_slice_after(requested_cursor, limit)?;
        if slice.is_empty() && requested_cursor < self.state.cursor {
            // A cursor-advancing command without a replayable public event invalidates
            // older deltas by moving journal_floor_cursor. Reaching this branch means
            // persisted state/journal was inconsistent, so heal with the canonical snapshot.
            return Ok(vec![self.sync_envelope(actor_id, limit, now_ms)]);
        }

        let mut responses = slice
            .into_iter()
            .filter(|entry| entry.audience.iter().any(|candidate| candidate == actor_id))
            .filter(|entry| {
                let event = entry.envelope.get("event").and_then(Value::as_object);
                if event.and_then(|event| event.get("type")).and_then(Value::as_str)
                    != Some("typingChanged")
                {
                    return true;
                }
                event.and_then(|event| event.get("expiresAtMs"))
                    .and_then(Value::as_i64)
                    .is_none_or(|expires_at_ms| expires_at_ms > now_ms)
            })
            .map(|entry| self.project_journal_envelope_for_actor(actor_id, &entry.envelope, now_ms))
            .collect::<Vec<_>>();
        responses.push(self.sync_checkpoint_envelope(
            checkpoint_cursor,
            has_more,
            now_ms,
        ));
        Ok(responses)
    }

    fn project_journal_envelope_for_actor(
        &self,
        actor_id: &str,
        envelope: &Value,
        now_ms: i64,
    ) -> Value {
        let mut projected = envelope.clone();
        let Some(event) = projected.get_mut("event").and_then(Value::as_object_mut) else {
            return projected;
        };
        match event.get("type").and_then(Value::as_str) {
            Some("conversationChanged") | Some("conversationParticipantChanged") => {
                let conversation_id = event
                    .get("conversation")
                    .and_then(|conversation| conversation.get("id"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                if let Some(conversation_id) = conversation_id {
                    if let Some(conversation) = self.state.conversations.get(&conversation_id) {
                        event.insert(
                            "conversation".into(),
                            self.project_conversation(conversation, actor_id, now_ms),
                        );
                    }
                }
            }
            Some("messageAdded") | Some("messageChanged") => {
                let identity = event.get("message").and_then(|message| {
                    Some((
                        message.get("conversationId")?.as_str()?.to_string(),
                        message.get("id")?.as_str()?.to_string(),
                    ))
                });
                if let Some((conversation_id, message_id)) = identity {
                    if let Some(message) = self
                        .state
                        .messages
                        .get(&conversation_id)
                        .and_then(|messages| messages.get(&message_id))
                    {
                        event.insert("message".into(), self.project_message(message, actor_id));
                    }
                }
            }
            _ => {}
        }
        projected
    }

    fn sync_checkpoint_envelope(
        &self,
        cursor: u64,
        has_more: bool,
        now_ms: i64,
    ) -> Value {
        json!({
            "protocolVersion":MESSAGING_PROTOCOL_VERSION,
            "cursor":cursor.to_string(),
            "serverTimeMs":now_ms,
            "event":{
                "type":"syncBatch",
                "fullSnapshot":false,
                "hasMore":has_more,
                "actors":[],
                "conversations":[],
                "messages":[],
                "folders":[],
                "drafts":[],
                "topicDrafts":[],
                "pendingPresenceSends":[],
                "conversationChildren":[],
                "invoices":[],
                "orders":[],
                "stories":[],
                "communities":[],
                "bots":[],
                "botExecutions":[],
                "miniApps":[],
                "nextCursor":cursor.to_string()
            }
        })
    }

    fn journal_slice_after(
        &self,
        requested_cursor: u64,
        cursor_group_limit: usize,
    ) -> Result<(Vec<MessagingJournalEntry>, u64, bool), String> {
        let limit = cursor_group_limit.max(1);
        let mut selected = Vec::new();
        let mut selected_groups = 0usize;
        let mut last_selected_cursor = None;
        let mut has_more = false;

        for entry in &self.state.event_journal {
            if entry.cursor <= requested_cursor {
                continue;
            }
            if last_selected_cursor != Some(entry.cursor) {
                if selected_groups >= limit {
                    has_more = true;
                    break;
                }
                selected_groups += 1;
                last_selected_cursor = Some(entry.cursor);
            }
            selected.push(entry.clone());
        }
        let checkpoint_cursor = if has_more {
            last_selected_cursor.unwrap_or(requested_cursor)
        } else {
            self.state.cursor
        };
        Ok((selected, checkpoint_cursor, has_more))
    }

    fn record_journal_result(
        &mut self,
        initiator: &str,
        previous_cursor: u64,
        envelopes: &[Value],
    ) -> Result<(), String> {
        if self.state.cursor == previous_cursor {
            // Idempotent/replayed responses and read-only service events must never create
            // a second replayable journal entry at an already-consumed cursor.
            return Ok(());
        }

        let mut recorded = 0usize;
        for envelope in envelopes {
            let Some(event) = envelope.get("event").and_then(Value::as_object) else {
                continue;
            };
            if event.get("type").and_then(Value::as_str) == Some("syncBatch") {
                continue;
            }
            let cursor = envelope
                .get("cursor")
                .and_then(Value::as_str)
                .ok_or("messaging event is missing a server cursor")?
                .parse::<u64>()
                .map_err(|_| "messaging event cursor is invalid".to_string())?;
            let audience = self.journal_event_audience(initiator, event);
            self.state.event_journal.push(MessagingJournalEntry {
                cursor,
                audience,
                envelope: envelope.clone(),
            });
            recorded += 1;
        }

        if self.state.cursor > previous_cursor && recorded == 0 {
            // Some durable child/state mutations intentionally have no public delta event.
            // Make every older cursor reconcile through a full canonical snapshot instead
            // of pretending the invisible cursor step can be replayed.
            let floor = self.state.cursor;
            self.state.journal_floor_cursor = floor;
            self.state
                .event_journal
                .retain(|entry| entry.cursor > floor);
        }
        self.prune_event_journal();
        Ok(())
    }

    fn prune_event_journal(&mut self) {
        if self.state.event_journal.len() <= MESSAGING_EVENT_JOURNAL_LIMIT {
            return;
        }
        let excess = self.state.event_journal.len() - MESSAGING_EVENT_JOURNAL_LIMIT;
        let prune_cursor = self.state.event_journal[excess - 1].cursor;
        self.state
            .event_journal
            .retain(|entry| entry.cursor > prune_cursor);
        self.state.journal_floor_cursor =
            self.state.journal_floor_cursor.max(prune_cursor);
    }

    fn journal_event_audience(
        &self,
        initiator: &str,
        event: &Map<String, Value>,
    ) -> Vec<String> {
        let mut audience = BTreeSet::from([initiator.to_string()]);
        let event_type = event.get("type").and_then(Value::as_str).unwrap_or_default();

        let conversation_id = match event_type {
            "conversationChanged" | "conversationParticipantChanged" => event
                .get("conversation")
                .and_then(|conversation| conversation.get("id"))
                .and_then(Value::as_str),
            "messageAdded" | "messageChanged" => event
                .get("message")
                .and_then(|message| message.get("conversationId"))
                .and_then(Value::as_str),
            "messagesDeleted" | "typingChanged" => {
                event.get("conversationId").and_then(Value::as_str)
            }
            _ => None,
        };
        if let Some(conversation_id) = conversation_id {
            self.extend_conversation_audience(&mut audience, conversation_id);
        }
        if event_type == "conversationParticipantChanged" {
            if let Some(removed_actor_id) =
                event.get("removedActorId").and_then(Value::as_str)
            {
                if !removed_actor_id.trim().is_empty() {
                    audience.insert(removed_actor_id.to_string());
                }
            }
        }
        if event_type == "actorChanged" {
            if let Some(actor_id) = event
                .get("actor")
                .and_then(|actor| actor.get("id"))
                .and_then(Value::as_str)
            {
                audience.insert(actor_id.to_string());
                for conversation in self.state.conversations.values() {
                    if conversation_has_access(conversation, actor_id) {
                        if let Some(conversation_id) =
                            conversation.get("id").and_then(Value::as_str)
                        {
                            self.extend_conversation_audience(
                                &mut audience,
                                conversation_id,
                            );
                        }
                    }
                }
            }
        }
        audience.into_iter().collect()
    }

    fn extend_conversation_audience(
        &self,
        audience: &mut BTreeSet<String>,
        conversation_id: &str,
    ) {
        let Some(conversation) = self.state.conversations.get(conversation_id) else {
            return;
        };
        if let Some(owner_id) = conversation.get("ownerId").and_then(Value::as_str) {
            audience.insert(owner_id.to_string());
        }
        if let Some(participants) = conversation.get("participants").and_then(Value::as_array) {
            for participant in participants {
                if let Some(actor_id) = participant.get("actorId").and_then(Value::as_str) {
                    audience.insert(actor_id.to_string());
                }
            }
        }
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

        // Mirror the exact next settled state into the recovery replica before replacing the
        // canonical file. This preserves request replay/journal truth across later corruption
        // without making the replica a second runtime owner.
        let backup = repository_backup_path(&self.path);
        let backup_temporary = PathBuf::from(format!("{}.tmp", backup.display()));
        {
            let mut file = File::create(&backup_temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
        }
        fs::rename(&backup_temporary, &backup)?;
        sync_parent_directory(&backup)?;

        fs::rename(&temporary, &self.path)?;
        sync_parent_directory(&self.path)
    }

}

fn decode_repository_state(bytes: &[u8]) -> io::Result<(MessagingRepositoryState, bool)> {
    let mut state: MessagingRepositoryState = serde_json::from_slice(bytes).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid Android messaging repository: {error}"),
        )
    })?;
    let migrated = match state.schema_version {
        MESSAGING_REPOSITORY_SCHEMA_VERSION => false,
        PREVIOUS_MESSAGING_REPOSITORY_SCHEMA_VERSION
        | LEGACY_MESSAGING_REPOSITORY_SCHEMA_VERSION => {
            state.schema_version = MESSAGING_REPOSITORY_SCHEMA_VERSION;
            state.event_journal.clear();
            state.journal_floor_cursor = state.cursor;
            true
        }
        actual => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "unsupported Android messaging repository schema {actual}; expected at most {MESSAGING_REPOSITORY_SCHEMA_VERSION}"
                ),
            ));
        }
    };
    validate_loaded_state(&state)?;
    Ok((state, migrated))
}

fn repository_backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

fn repository_corrupt_path(path: &Path) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    PathBuf::from(format!("{}.corrupt-{stamp}", path.display()))
}

fn copy_file_durably(source: &Path, destination: &Path) -> io::Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, destination)?;
    File::open(destination)?.sync_all()?;
    sync_parent_directory(destination)
}

fn sync_parent_directory(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if let Ok(directory) = File::open(parent) {
            directory.sync_all()?;
        }
    }
    Ok(())
}

fn validate_loaded_state(state: &MessagingRepositoryState) -> io::Result<()> {
    for contact_id in &state.contact_ids {
        let actor = state.actors.get(contact_id).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "messaging contact references a missing actor",
            )
        })?;
        if actor.get("id").and_then(Value::as_str) != Some(contact_id.as_str())
            || actor.get("kind").and_then(Value::as_str) != Some("human")
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "messaging contact actor is invalid",
            ));
        }
    }
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
    let mut previous_journal_cursor = state.journal_floor_cursor;
    for entry in &state.event_journal {
        if entry.cursor <= state.journal_floor_cursor
            || entry.cursor < previous_journal_cursor
            || entry.cursor > state.cursor
            || entry.audience.is_empty()
            || entry
                .envelope
                .get("cursor")
                .and_then(Value::as_str)
                .and_then(|value| value.parse::<u64>().ok())
                != Some(entry.cursor)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "messaging event journal cursor/audience is inconsistent",
            ));
        }
        previous_journal_cursor = entry.cursor;
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

fn remote_human_identity_text(value: Option<&Value>, label: &str) -> Result<String, String> {
    let value = value.ok_or_else(|| format!("remote Human {label} is required"))?;
    let rendered = match value {
        Value::String(value) => value.trim().to_string(),
        Value::Number(value) => value.to_string(),
        _ => return Err(format!("remote Human {label} has an unsupported type")),
    };
    if rendered.is_empty() || rendered.len() > 160 {
        return Err(format!("remote Human {label} is invalid"));
    }
    Ok(rendered)
}

fn canonical_remote_human_actor_id(
    value: Option<&Value>,
    label: &str,
) -> Result<String, String> {
    let raw = remote_human_identity_text(value, label)?;
    if raw.starts_with("human:") {
        Ok(raw)
    } else {
        Ok(format!("human:{raw}"))
    }
}

fn stable_human_direct_conversation_id(left: &str, right: &str) -> String {
    let mut participant_ids = [left, right];
    participant_ids.sort_unstable();
    let mut identity = Vec::new();
    for participant_id in participant_ids {
        identity.extend_from_slice(&(participant_id.len() as u64).to_be_bytes());
        identity.extend_from_slice(participant_id.as_bytes());
    }
    format!("human-direct-{}", crate::sha256::sha256_hex(&identity))
}

fn project_remote_human_reaction_actors(
    value: Option<&Value>,
    local_actor_id: &str,
    peer_actor_id: &str,
) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(BTreeMap::new());
    };
    let reactions = value
        .as_array()
        .ok_or("remote Human reactions must be an array")?;
    let mut projected = BTreeMap::new();
    for reaction in reactions {
        let reaction = reaction
            .as_object()
            .ok_or("remote Human reaction must be an object")?;
        let emoji = required_non_empty(reaction.get("emoji"), "remote Human reaction emoji")?;
        if emoji.len() > 64 {
            return Err("remote Human reaction emoji is invalid".into());
        }
        let count = reaction.get("count").and_then(Value::as_u64).unwrap_or(0);
        let reacted_by_me = reaction
            .get("reactedByMe")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let actors = projected
            .entry(emoji.to_string())
            .or_insert_with(BTreeSet::new);
        if reacted_by_me {
            actors.insert(local_actor_id.to_string());
        }
        if count > u64::from(reacted_by_me) {
            actors.insert(peer_actor_id.to_string());
        }
    }
    projected.retain(|_, actors| !actors.is_empty());
    Ok(projected)
}

fn project_remote_human_attachments(value: Option<&Value>) -> Result<Vec<Value>, String> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(Vec::new());
    };
    let attachments = value
        .as_array()
        .ok_or("remote Human attachments must be an array")?;
    if attachments.len() > 32 {
        return Err("remote Human attachment list is too large".into());
    }
    attachments
        .iter()
        .map(|attachment| {
            let attachment = attachment
                .as_object()
                .ok_or("remote Human attachment must be an object")?;
            let resource_id =
                required_non_empty(attachment.get("resourceId"), "remote Human resource id")?;
            if resource_id.len() > 128
                || resource_id.contains('/')
                || resource_id.contains('\\')
                || resource_id.chars().any(char::is_control)
            {
                return Err("remote Human resource id is invalid".into());
            }
            let name = required_non_empty(attachment.get("name"), "remote Human attachment name")?;
            let size = attachment
                .get("size")
                .and_then(Value::as_u64)
                .filter(|size| *size > 0 && *size <= 32 * 1024 * 1024)
                .ok_or("remote Human attachment size is invalid")?;
            Ok(json!({
                "resourceId":resource_id,
                "name":name,
                "contentType":attachment.get("contentType").cloned().unwrap_or(Value::Null),
                "size":size,
                "createdAt":attachment.get("createdAt").cloned().unwrap_or(Value::Null)
            }))
        })
        .collect()
}

fn current_messaging_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}

fn valid_actor_id(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty() && value.len() <= 160
}

fn bounded_id(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty() && value.len() <= 200
}

fn clear_forward_caption(content: &mut Value) {
    let kind = content.get("type").and_then(Value::as_str);
    if !matches!(
        kind,
        Some("photo" | "video" | "animation" | "audio" | "voice" | "document")
    ) {
        return;
    }
    if let Some(data) = content.get_mut("data").and_then(Value::as_object_mut) {
        data.insert(
            "caption".into(),
            json!({"text":"","entities":[]}),
        );
    }
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

    fn seed(service: &mut AndroidMessagingService, actor: &str) -> String {
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
        stable_message_id(actor, "message:root")
    }

    #[test]
    fn agent_group_delivery_is_text_only_priority_neutral_and_replay_safe() {
        let root = temp_root("agent-group-delivery");
        let images = vec![json!({"url":"content://image/1","alt":"photo"})];
        let first;
        {
            let mut service = AndroidMessagingService::open(&root).unwrap();
            execute(
                &mut service,
                "agent-a-profile",
                "agent-a",
                json!({"type":"upsertProfile","actor":{"id":"agent-a","kind":"agent","displayName":"Agent A"}}),
            )
            .unwrap();
            execute(
                &mut service,
                "agent-b-profile",
                "agent-a",
                json!({"type":"upsertProfile","actor":{"id":"agent-b","kind":"agent","displayName":"Agent B"}}),
            )
            .unwrap();
            execute(
                &mut service,
                "agent-group",
                "agent-a",
                json!({"type":"createConversation","conversation":{
                    "id":"agent-group",
                    "kind":"group",
                    "title":"Agents",
                    "ownerId":"agent-a",
                    "participants":[
                        {"actorId":"agent-a","role":"owner","joinedAtMs":1},
                        {"actorId":"agent-b","role":"member","joinedAtMs":1}
                    ]
                }}),
            )
            .unwrap();

            first = service
                .deliver_agent_message(
                    "acct:a",
                    "agent-a",
                    "agent-group",
                    true,
                    "group text",
                    &images,
                    true,
                    "tool-call-group-1",
                    10,
                )
                .unwrap();
            assert_eq!(first["priorityApplied"], false);
            assert_eq!(first["imagesDelivered"], false);
            assert_eq!(first["droppedImageCount"], 1);

            let message_id = first["messageId"].as_str().unwrap();
            let delivered = service.state.messages["agent-group"][message_id].clone();
            assert_eq!(delivered["content"]["data"]["agentImages"], json!([]));
            assert_eq!(delivered["content"]["data"]["agentPriority"], false);

            let wakes = service.pending_agent_wakes("acct:a", 10);
            assert_eq!(wakes.len(), 1);
            assert_eq!(wakes[0]["targetAgentId"], "agent-b");
            assert_eq!(wakes[0]["images"], json!([]));
            assert_eq!(wakes[0]["priority"], false);
        }

        let mut reopened = AndroidMessagingService::open(&root).unwrap();
        let replay = reopened
            .deliver_agent_message(
                "acct:a",
                "agent-a",
                "agent-group",
                true,
                "group text",
                &images,
                true,
                "tool-call-group-1",
                20,
            )
            .unwrap();
        assert_eq!(replay, first);
        assert_eq!(reopened.pending_agent_wakes("acct:a", 20).len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn deleted_agent_wake_cleanup_is_account_fenced_and_survives_reopen() {
        let root = temp_root("deleted-agent-wakes");
        {
            let mut service = AndroidMessagingService::open(&root).unwrap();
            service
                .deliver_agent_message(
                    "acct:a",
                    "agent-a",
                    "agent-b",
                    false,
                    "hello",
                    &[],
                    false,
                    "tool-call-a",
                    10,
                )
                .unwrap();
            service
                .deliver_agent_message(
                    "acct:b",
                    "agent-a",
                    "agent-b",
                    false,
                    "other account",
                    &[],
                    false,
                    "tool-call-b",
                    11,
                )
                .unwrap();
            assert_eq!(service.pending_agent_wakes("acct:a", 100).len(), 1);
            assert_eq!(service.pending_agent_wakes("acct:b", 100).len(), 1);

            assert_eq!(
                service
                    .clear_agent_wakes_for_deleted_agent("acct:a", "agent-b")
                    .unwrap(),
                1
            );
            assert!(service.pending_agent_wakes("acct:a", 100).is_empty());
            assert_eq!(
                service.pending_agent_wakes("acct:b", 100).len(),
                1,
                "deletion cleanup must never cross the account fence"
            );
        }

        let reopened = AndroidMessagingService::open(&root).unwrap();
        assert!(reopened.pending_agent_wakes("acct:a", 100).is_empty());
        assert_eq!(reopened.pending_agent_wakes("acct:b", 100).len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn agent_relationship_projection_is_durable_and_account_fenced() {
        let root = temp_root("agent-relationships");
        {
            let mut service = AndroidMessagingService::open(&root).unwrap();
            service
                .deliver_agent_message(
                    "acct:a",
                    "agent-a",
                    "agent-b",
                    false,
                    "hello",
                    &[],
                    false,
                    "tool-call-1",
                    10,
                )
                .unwrap();
            assert_eq!(
                service.agent_conversation_partner_ids("acct:a", "agent-a"),
                vec!["agent-b".to_string()]
            );
            assert_eq!(
                service.agent_conversation_partner_ids("acct:a", "agent-b"),
                vec!["agent-a".to_string()]
            );
            assert!(
                service
                    .agent_conversation_partner_ids("acct:b", "agent-a")
                    .is_empty(),
                "stable direct-conversation identity must fence another account"
            );
        }

        let reopened = AndroidMessagingService::open(&root).unwrap();
        assert_eq!(
            reopened.agent_conversation_partner_ids("acct:a", "agent-a"),
            vec!["agent-b".to_string()],
            "relationship must survive process restart from the canonical messaging repository"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn child_mutation_requires_canonical_parent_membership_and_survives_reopen() {
        let root = temp_root("child");
        let actor = "human:owner";
        let root_message_id = stable_message_id(actor, "message:root");
        let destination = json!({
            "conversationId":"conversation:parent",
            "child":{"kind":"topic","rootMessageId":root_message_id}
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
        let root_message_id = seed(&mut service, actor);
        let request = envelope(
            "same",
            actor,
            json!({"type":"setConversationChildPinned","destination":{
                "conversationId":"conversation:parent",
                "child":{"kind":"topic","rootMessageId":root_message_id}
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
        let root_message_id = seed(&mut service, actor);
        let request = envelope(
            "stale",
            actor,
            json!({"type":"setConversationChildPinned","destination":{
                "conversationId":"conversation:parent",
                "child":{"kind":"topic","rootMessageId":root_message_id}
            },"pinned":true}),
        );
        assert!(service
            .execute(&request, "human:new-account", 100)
            .is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ensure_direct_human_conversation_is_stable_contact_owned_and_idempotent() {
        let root = temp_root("ensure-direct-human");
        let actor = "human:self";
        let peer = "human:peer";
        let mut service = AndroidMessagingService::open(&root).unwrap();
        execute(
            &mut service,
            "self-profile",
            actor,
            json!({"type":"upsertProfile","actor":{"id":actor,"kind":"human","displayName":"Self"}}),
        )
        .unwrap();
        service
            .reconcile_remote_human_contacts(&[
                json!({"id":peer,"kind":"human","displayName":"Peer"})
            ])
            .unwrap();

        let first = execute(
            &mut service,
            "ensure-direct-1",
            actor,
            json!({"type":"ensureDirectConversation","peerActorId":peer}),
        )
        .unwrap();
        let first_conversation = &first["envelopes"][0]["event"]["conversation"];
        let expected = stable_human_direct_conversation_id(actor, peer);
        assert_eq!(first_conversation["id"], expected);
        assert_eq!(first_conversation["kind"], "direct");

        let second = execute(
            &mut service,
            "ensure-direct-2",
            actor,
            json!({"type":"ensureDirectConversation","peerActorId":peer}),
        )
        .unwrap();
        assert_eq!(second["envelopes"][0]["event"]["conversation"]["id"], expected);
        assert_eq!(service.state.conversations.len(), 1);

        assert!(execute(
            &mut service,
            "ensure-outsider",
            actor,
            json!({"type":"ensureDirectConversation","peerActorId":"human:not-a-contact"}),
        )
        .is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn remote_human_send_intent_is_owned_by_canonical_direct_conversation() {
        let root = temp_root("remote-send-intent");
        let actor = "human:self";
        let peer = "human:peer";
        let conversation_id = stable_human_direct_conversation_id(actor, peer);
        let mut service = AndroidMessagingService::open(&root).unwrap();
        service
            .reconcile_remote_human_contacts(&[
                json!({"id":peer,"kind":"human","displayName":"Peer"})
            ])
            .unwrap();
        execute(
            &mut service,
            "self-profile",
            actor,
            json!({"type":"upsertProfile","actor":{"id":actor,"kind":"human","displayName":"Self"}}),
        )
        .unwrap();
        execute(
            &mut service,
            "direct",
            actor,
            json!({"type":"createConversation","conversation":{
                "id":conversation_id,
                "kind":"direct",
                "title":"Peer",
                "ownerId":actor,
                "participants":[
                    {"actorId":actor,"role":"owner","joinedAtMs":1},
                    {"actorId":peer,"role":"member","joinedAtMs":1}
                ]
            }}),
        )
        .unwrap();

        let command = json!({
            "type":"sendMessage",
            "conversationId":conversation_id,
            "clientMessageId":"client-1",
            "content":{"type":"text","data":{"text":{"text":"hello","entities":[]}}}
        });
        let intent = service
            .remote_human_send_intent(actor, command.as_object().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(intent["targetUserId"], "peer");
        assert_eq!(intent["clientRequestId"], "client-1");
        assert_eq!(intent["text"], "hello");
        assert!(intent["attachments"].as_array().unwrap().is_empty());

        execute(
            &mut service,
            "blob-begin-remote",
            actor,
            json!({"type":"beginBlobUpload","metadata":{
                "id":"blob-a","fileName":"hello.txt","mimeType":"text/plain",
                "sizeBytes":5,"contentHash":Value::Null,"createdAtMs":1
            }}),
        )
        .unwrap();
        execute(
            &mut service,
            "blob-chunk-remote",
            actor,
            json!({"type":"appendBlobChunk","blobId":"blob-a","offset":0,"dataBase64":"aGVsbG8="}),
        )
        .unwrap();
        execute(
            &mut service,
            "blob-finish-remote",
            actor,
            json!({"type":"finishBlobUpload","blobId":"blob-a"}),
        )
        .unwrap();

        let attachment = json!({
            "type":"sendMessage",
            "conversationId":conversation_id,
            "clientMessageId":"client-2",
            "content":{"type":"document","data":{
                "media":{
                    "id":"blob-a","fileName":"forged-name.bin","mimeType":"application/x-forged",
                    "sizeBytes":999,"remoteUrl":"fabushi-blob://blob-a"
                },
                "caption":{"text":"caption","entities":[]}
            }}
        });
        let attachment_intent = service
            .remote_human_send_intent(actor, attachment.as_object().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(attachment_intent["text"], "caption");
        assert_eq!(attachment_intent["attachments"][0]["blobId"], "blob-a");
        assert_eq!(attachment_intent["attachments"][0]["fileName"], "hello.txt");
        assert_eq!(attachment_intent["attachments"][0]["mimeType"], "text/plain");
        assert_eq!(attachment_intent["attachments"][0]["sizeBytes"], 5);

        let (name, mime, size, bytes) = service
            .read_owned_blob_for_remote_transport(actor, "blob-a")
            .unwrap();
        assert_eq!((name.as_str(), mime.as_str(), size), ("hello.txt", "text/plain", 5));
        assert_eq!(bytes, b"hello");
        assert!(service
            .read_owned_blob_for_remote_transport("human:outsider", "blob-a")
            .is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn server_reconciled_client_request_prevents_duplicate_local_send() {
        let root = temp_root("remote-send-replay");
        let actor = "human:self";
        let peer = "human:peer";
        let conversation_id = stable_human_direct_conversation_id(actor, peer);
        let mut service = AndroidMessagingService::open(&root).unwrap();
        service
            .reconcile_remote_human_messages(
                actor,
                peer,
                &[json!({
                    "id":"42",
                    "senderUserId":"self",
                    "recipientUserId":"peer",
                    "text":"hello",
                    "clientRequestId":"client-1",
                    "createdAt":"2026-10-13T00:00:00Z",
                    "readAt":Value::Null,
                    "attachments":[],
                    "reactions":[]
                })],
            )
            .unwrap();
        assert_eq!(
            service
                .remote_human_server_echo_for_client_request(
                    &conversation_id,
                    actor,
                    "client-1",
                )
                .unwrap()
                .1,
            "42",
        );
        let result = execute(
            &mut service,
            "local-replay",
            actor,
            json!({
                "type":"sendMessage",
                "conversationId":conversation_id,
                "clientMessageId":"client-1",
                "content":{"type":"text","data":{"text":{"text":"hello","entities":[]}}}
            }),
        )
        .unwrap();
        assert_eq!(
            result["envelopes"][0]["event"]["message"]["id"],
            "human-server-message:42",
        );
        assert_eq!(
            service
                .state
                .messages
                .get(&conversation_id)
                .unwrap()
                .len(),
            1,
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn remote_human_attachment_metadata_rejects_unsafe_resource_ids() {
        assert!(project_remote_human_attachments(Some(&json!([{
            "resourceId":"../escape",
            "name":"file.txt",
            "size":5
        }]))).is_err());
        assert!(project_remote_human_attachments(Some(&json!([{
            "resourceId":"resource\\escape",
            "name":"file.txt",
            "size":5
        }]))).is_err());
        assert!(project_remote_human_attachments(Some(&json!([{
            "resourceId":"r".repeat(129),
            "name":"file.txt",
            "size":5
        }]))).is_err());
        let valid = project_remote_human_attachments(Some(&json!([{
            "resourceId":"resource-1",
            "name":"file.txt",
            "size":5
        }]))).unwrap();
        assert_eq!(valid[0]["resourceId"], "resource-1");
    }

    #[test]
    fn remote_human_attachment_download_and_import_are_canonical_and_durable() {
        let root = remote_root("remote-attachment-download");
        let owner = "human:owner";
        let peer = "human:peer";
        let conversation_id = stable_human_direct_conversation_id(owner, peer);
        let message_id = "human-server-message:remote-attachment";
        let mut remote = remote_message(
            "remote-attachment",
            "peer",
            "owner",
            None,
            "file",
            "2026-10-13T00:00:00Z",
        );
        remote["attachments"] = json!([{
            "resourceId":"resource-1",
            "name":"hello.txt",
            "contentType":"text/plain",
            "size":5
        }]);
        {
            let mut service = AndroidMessagingService::open(&root).unwrap();
            service
                .reconcile_remote_human_contacts(&[
                    json!({"id":peer,"kind":"human","displayName":"Peer"})
                ])
                .unwrap();
            service
                .reconcile_remote_human_messages(owner, peer, &[remote])
                .unwrap();

            let command = json!({
                "type":"downloadRemoteHumanAttachment",
                "conversationId":conversation_id,
                "messageId":message_id,
                "resourceId":"resource-1"
            });
            let intent = service
                .remote_human_attachment_download_intent(owner, command.as_object().unwrap())
                .unwrap()
                .unwrap();
            assert_eq!(intent["fileName"], "hello.txt");
            assert_eq!(intent["contentType"], "text/plain");
            assert_eq!(intent["sizeBytes"], 5);
            assert!(service
                .remote_human_attachment_download_intent(
                    "human:outsider",
                    command.as_object().unwrap(),
                )
                .is_err());

            let result = service
                .import_remote_human_attachment(
                    owner,
                    &conversation_id,
                    message_id,
                    "resource-1",
                    b"hello",
                    123,
                )
                .unwrap();
            let blob_id = result["blob"]["id"].as_str().unwrap().to_string();
            assert!(blob_id.starts_with("remote-"));
            assert_eq!(
                service.state.messages[&conversation_id][message_id]["content"]["data"]
                    ["remoteAttachments"][0]["localBlobId"],
                blob_id,
            );
            let (name, mime, size, bytes) = service
                .read_owned_blob_for_remote_transport(owner, &blob_id)
                .unwrap();
            assert_eq!((name.as_str(), mime.as_str(), size), ("hello.txt", "text/plain", 5));
            assert_eq!(bytes, b"hello");

            let replay = service
                .import_remote_human_attachment(
                    owner,
                    &conversation_id,
                    message_id,
                    "resource-1",
                    b"hello",
                    124,
                )
                .unwrap();
            assert_eq!(replay["alreadyDownloaded"], true);
        }
        let reopened = AndroidMessagingService::open(&root).unwrap();
        let local_blob_id = reopened.state.messages[&conversation_id][message_id]["content"]["data"]
            ["remoteAttachments"][0]["localBlobId"]
            .as_str()
            .unwrap()
            .to_string();
        let (_, _, _, bytes) = reopened
            .read_owned_blob_for_remote_transport(owner, &local_blob_id)
            .unwrap();
        assert_eq!(bytes, b"hello");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn remote_human_contacts_are_durable_visible_and_invalidate_stale_delta() {
        let root = temp_root("remote-contacts");
        let owner = "human:owner";
        {
            let mut service = AndroidMessagingService::open(&root).unwrap();
            execute(
                &mut service,
                "owner-profile",
                owner,
                json!({"type":"upsertProfile","actor":{"id":owner,"kind":"human","displayName":"Owner"}}),
            )
            .unwrap();
            let before = service.state.cursor;
            assert_eq!(
                service
                    .reconcile_remote_human_contacts(&[
                        json!({"id":"human:friend-1","kind":"human","displayName":"Friend One","username":"friend1"}),
                        json!({"id":"human:friend-2","kind":"human","displayName":"Friend Two","username":"friend2"})
                    ])
                    .unwrap(),
                2
            );
            assert!(service.state.cursor > before);
            let sync = execute(
                &mut service,
                "contact-sync",
                owner,
                json!({"type":"sync","cursor":before.to_string(),"limit":10}),
            )
            .unwrap();
            let batch = &sync["envelopes"][0]["event"];
            assert_eq!(batch["fullSnapshot"], true);
            let ids = batch["actors"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|actor| actor["id"].as_str())
                .collect::<BTreeSet<_>>();
            assert!(ids.contains("human:friend-1"));
            assert!(ids.contains("human:friend-2"));
        }

        let mut reopened = AndroidMessagingService::open(&root).unwrap();
        let snapshot = execute(
            &mut reopened,
            "reopen-contact-sync",
            owner,
            json!({"type":"sync","limit":10}),
        )
        .unwrap();
        let ids = snapshot["envelopes"][0]["event"]["actors"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|actor| actor["id"].as_str())
            .collect::<BTreeSet<_>>();
        assert!(ids.contains("human:friend-1"));
        assert!(ids.contains("human:friend-2"));

        // Server friendship removal drops contact membership without deleting an actor that may
        // still be referenced by a durable conversation or historical message.
        reopened
            .reconcile_remote_human_contacts(&[
                json!({"id":"human:friend-2","kind":"human","displayName":"Friend Two","username":"friend2"})
            ])
            .unwrap();
        assert!(!reopened.state.contact_ids.contains("human:friend-1"));
        assert!(reopened.state.actors.contains_key("human:friend-1"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn remote_human_contact_reconcile_rejects_invalid_or_duplicate_roster() {
        let root = temp_root("remote-contacts-invalid");
        let mut service = AndroidMessagingService::open(&root).unwrap();
        assert!(service
            .reconcile_remote_human_contacts(&[
                json!({"id":"agent:not-human","kind":"human","displayName":"Wrong prefix"})
            ])
            .is_err());
        assert!(service
            .reconcile_remote_human_contacts(&[
                json!({"id":"human:dup","kind":"human","displayName":"One"}),
                json!({"id":"human:dup","kind":"human","displayName":"Two"})
            ])
            .is_err());
        assert!(service.state.contact_ids.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn previous_v2_repository_migrates_to_v3_with_empty_contact_roster() {
        let root = temp_root("legacy-v2");
        fs::create_dir_all(&root).unwrap();
        let previous = json!({
            "schemaVersion":2,
            "cursor":9,
            "actors":{"human:owner":{"id":"human:owner","kind":"human","displayName":"Owner"}},
            "conversations":{},
            "messages":{},
            "drafts":{},
            "folders":{},
            "conversationPreferences":{},
            "reactionActors":{},
            "pollVotes":{},
            "requestResults":{},
            "requestOrder":[],
            "agentDeliveryCalls":{},
            "agentDeliveryCallOrder":[],
            "agentWakes":{},
            "eventJournal":[],
            "journalFloorCursor":0
        });
        fs::write(
            root.join("messaging-repository.json"),
            serde_json::to_vec(&previous).unwrap(),
        )
        .unwrap();
        let service = AndroidMessagingService::open(&root).unwrap();
        assert_eq!(service.state.schema_version, MESSAGING_REPOSITORY_SCHEMA_VERSION);
        assert_eq!(service.state.journal_floor_cursor, 9);
        assert!(service.state.contact_ids.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_repository_migrates_to_v3_and_forces_snapshot_reconcile() {
        let root = temp_root("legacy-v1");
        fs::create_dir_all(&root).unwrap();
        let legacy = json!({
            "schemaVersion":1,
            "cursor":7,
            "actors":{},
            "conversations":{},
            "messages":{},
            "drafts":{},
            "folders":{},
            "conversationPreferences":{},
            "reactionActors":{},
            "pollVotes":{},
            "requestResults":{},
            "requestOrder":[],
            "agentDeliveryCalls":{},
            "agentDeliveryCallOrder":[],
            "agentWakes":{}
        });
        fs::write(
            root.join("messaging-repository.json"),
            serde_json::to_vec(&legacy).unwrap(),
        )
        .unwrap();

        let mut service = AndroidMessagingService::open(&root).unwrap();
        assert_eq!(service.state.schema_version, MESSAGING_REPOSITORY_SCHEMA_VERSION);
        assert_eq!(service.state.journal_floor_cursor, 7);
        let response = execute(
            &mut service,
            "sync-legacy",
            "human:owner",
            json!({"type":"sync","cursor":"0","limit":10}),
        )
        .unwrap();
        assert_eq!(
            response["envelopes"][0]["event"]["fullSnapshot"],
            Value::Bool(true)
        );
        let persisted: Value = serde_json::from_slice(
            &fs::read(root.join("messaging-repository.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(persisted["schemaVersion"], MESSAGING_REPOSITORY_SCHEMA_VERSION);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn initial_snapshot_limit_keeps_complete_navigation_metadata() {
        let root = temp_root("snapshot-limit");
        let owner = "human:owner";
        let peer = "human:peer";
        let mut service = AndroidMessagingService::open(&root).unwrap();
        seed(&mut service, owner);
        execute(
            &mut service,
            "snapshot-peer",
            peer,
            json!({"type":"upsertProfile","actor":{"id":peer,"kind":"human","displayName":"Peer"}}),
        )
        .unwrap();
        execute(
            &mut service,
            "snapshot-second-conversation",
            owner,
            json!({"type":"createConversation","conversation":{
                "id":"conversation:second",
                "kind":"direct",
                "title":"Second",
                "ownerId":owner,
                "participants":[
                    {"actorId":owner,"role":"owner","joinedAtMs":2},
                    {"actorId":peer,"role":"member","joinedAtMs":2}
                ]
            }}),
        )
        .unwrap();
        execute(
            &mut service,
            "snapshot-second-message",
            owner,
            json!({"type":"sendMessage","conversationId":"conversation:second","clientMessageId":"message:second",
                "content":{"type":"text","data":{"text":{"text":"second","entities":[]}}}}),
        )
        .unwrap();

        let response = execute(
            &mut service,
            "snapshot-small",
            owner,
            json!({"type":"sync","limit":1}),
        )
        .unwrap();
        let batch = &response["envelopes"][0]["event"];
        assert_eq!(batch["fullSnapshot"], true);
        assert_eq!(
            batch["conversations"].as_array().unwrap().len(),
            2,
            "lightweight navigation metadata must not be truncated by the heavy payload limit"
        );
        assert_eq!(batch["actors"].as_array().unwrap().len(), 2);
        assert_eq!(
            batch["messages"].as_array().unwrap().len(),
            1,
            "heavy message payload must honor the requested snapshot limit"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn send_retry_replays_same_payload_without_cursor_or_journal_advance_and_rejects_conflict() {
        let root = temp_root("send-idempotency");
        let owner = "human:owner";
        let mut service = AndroidMessagingService::open(&root).unwrap();
        seed(&mut service, owner);
        let command = json!({"type":"sendMessage","conversationId":"conversation:parent","clientMessageId":"message:idem",
            "content":{"type":"text","data":{"text":{"text":"same","entities":[]}}}});
        let first = execute(&mut service, "send-idem-first", owner, command.clone()).unwrap();
        let canonical_id = first["envelopes"].as_array().unwrap().iter()
            .find(|envelope| envelope["event"]["type"] == "messageAdded")
            .unwrap()["event"]["message"]["id"].as_str().unwrap().to_string();
        assert!(canonical_id.starts_with("msg:"));
        assert_eq!(canonical_id, stable_message_id(owner, "message:idem"));
        let cursor = service.state.cursor;
        let journal_len = service.state.event_journal.len();

        let replay = execute(&mut service, "send-idem-retry", owner, command).unwrap();
        assert_eq!(service.state.cursor, cursor);
        assert_eq!(
            service.state.event_journal.len(),
            journal_len,
            "idempotent replay must not create a second journal delta at the same cursor"
        );
        assert!(replay["envelopes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|envelope| envelope["event"]["type"] == "messageChanged"));

        let conflict = execute(
            &mut service,
            "send-idem-conflict",
            owner,
            json!({"type":"sendMessage","conversationId":"conversation:parent","clientMessageId":"message:idem",
                "content":{"type":"text","data":{"text":{"text":"different","entities":[]}}}}),
        )
        .unwrap_err();
        assert!(conflict.contains("conflicts with the canonical message payload"));
        assert_eq!(service.state.cursor, cursor);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn actor_and_conversation_structural_validation_fail_closed() {
        let root = temp_root("structural-validation");
        let actor = "human:owner";
        let peer = "human:peer";
        let mut service = AndroidMessagingService::open(&root).unwrap();

        assert!(execute(
            &mut service,
            "invalid-kind",
            actor,
            json!({"type":"upsertProfile","actor":{"id":actor,"kind":"alien","displayName":"Owner"}}),
        )
        .is_err());
        assert!(execute(
            &mut service,
            "blank-name",
            actor,
            json!({"type":"upsertProfile","actor":{"id":actor,"kind":"human","displayName":"   "}}),
        )
        .is_err());
        execute(
            &mut service,
            "valid-owner",
            actor,
            json!({"type":"upsertProfile","actor":{"id":actor,"kind":"human","displayName":"Owner"}}),
        )
        .unwrap();

        assert!(execute(
            &mut service,
            "missing-peer",
            actor,
            json!({"type":"createConversation","conversation":{
                "id":"conversation:missing-peer","kind":"direct","title":"Missing peer",
                "participants":[
                    {"actorId":actor,"role":"owner","joinedAtMs":1},
                    {"actorId":peer,"role":"member","joinedAtMs":1}
                ]
            }}),
        )
        .is_err());

        execute(
            &mut service,
            "valid-peer",
            peer,
            json!({"type":"upsertProfile","actor":{"id":peer,"kind":"human","displayName":"Peer"}}),
        )
        .unwrap();
        assert!(execute(
            &mut service,
            "invalid-secret-cardinality",
            actor,
            json!({"type":"createConversation","conversation":{
                "id":"conversation:secret","kind":"secret","title":"Secret",
                "participants":[{"actorId":actor,"role":"owner","joinedAtMs":1}]
            }}),
        )
        .is_err());

        execute(
            &mut service,
            "group-create",
            actor,
            json!({"type":"createConversation","conversation":{
                "id":"conversation:group-guard","kind":"group","title":"Group","ownerId":actor,
                "participants":[{"actorId":actor,"role":"owner","joinedAtMs":1}],
                "topics":[]
            }}),
        )
        .unwrap();
        assert!(execute(
            &mut service,
            "group-overwrite-membership",
            actor,
            json!({"type":"createConversation","conversation":{
                "id":"conversation:group-guard","kind":"group","title":"Group renamed","ownerId":actor,
                "participants":[
                    {"actorId":actor,"role":"owner","joinedAtMs":1},
                    {"actorId":peer,"role":"member","joinedAtMs":2}
                ],
                "topics":[]
            }}),
        )
        .is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn forward_recipient_search_uses_canonical_permissions_and_protected_content() {
        let root = temp_root("forward-recipient-search");
        let me = "human:me";
        let peer = "human:peer";
        let other = "human:other";
        let mut service = AndroidMessagingService::open(&root).unwrap();

        for (request, id, name) in [
            ("profile-me", me, "Me"),
            ("profile-peer", peer, "Peer"),
            ("profile-other", other, "Other Person"),
        ] {
            execute(
                &mut service,
                request,
                id,
                json!({"type":"upsertProfile","actor":{"id":id,"kind":"human","displayName":name}}),
            )
            .unwrap();
        }

        for (request, id, title, participant, updated_at, can_send_media) in [
            ("conversation-source", "conversation:source", "Source", peer, 10, true),
            ("conversation-newer", "conversation:newer", "Other Room", other, 30, true),
            ("conversation-older", "conversation:older", "Peer Room", peer, 20, false),
        ] {
            execute(
                &mut service,
                request,
                me,
                json!({"type":"createConversation","conversation":{
                    "id":id,
                    "kind":"direct",
                    "title":title,
                    "ownerId":me,
                    "participants":[
                        {"actorId":me,"role":"owner","joinedAtMs":1},
                        {"actorId":participant,"role":"member","joinedAtMs":1}
                    ],
                    "permissions":{
                        "canSendMessages":true,
                        "canSendMedia":can_send_media
                    },
                    "updatedAtMs":updated_at
                }}),
            )
            .unwrap();
        }

        let sent = execute(
            &mut service,
            "source-message",
            me,
            json!({
                "type":"sendMessage",
                "conversationId":"conversation:source",
                "clientMessageId":"forward-source",
                "content":{"type":"text","data":{"text":{"text":"forward me","entities":[]}}}
            }),
        )
        .unwrap();
        let message_id = sent["envelopes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|envelope| envelope["event"]["type"] == "messageAdded")
            .unwrap()["event"]["message"]["id"]
            .as_str()
            .unwrap()
            .to_string();

        let all = service
            .search_forward_recipients(
                me,
                "conversation:source",
                &message_id,
                "",
                100,
            )
            .unwrap();
        assert_eq!(
            all.iter()
                .map(|row| row["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["conversation:newer", "conversation:older", "conversation:source"],
            "eligible Human destinations are ordered newest first",
        );

        let query = service
            .search_forward_recipients(
                me,
                "conversation:source",
                &message_id,
                "other person",
                50,
            )
            .unwrap();
        assert_eq!(query.len(), 1);
        assert_eq!(query[0]["id"], "conversation:newer");

        execute(
            &mut service,
            "protected-source",
            me,
            json!({
                "type":"sendMessage",
                "conversationId":"conversation:source",
                "clientMessageId":"protected-source",
                "protectedContent":true,
                "content":{"type":"text","data":{"text":{"text":"secret","entities":[]}}}
            }),
        )
        .unwrap();
        let protected_id = stable_message_id(me, "protected-source");
        assert!(service
            .search_forward_recipients(
                me,
                "conversation:source",
                &protected_id,
                "",
                50,
            )
            .unwrap()
            .is_empty());

        assert!(service
            .search_forward_recipients(
                me,
                "conversation:source",
                "missing-message",
                "",
                50,
            )
            .is_err());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn forward_retries_compare_options_and_privacy_fail_closed() {
        let root = temp_root("forward-idempotency");
        let actor = "human:forwarder";
        let mut service = AndroidMessagingService::open(&root).unwrap();
        execute(
            &mut service,
            "forward-profile",
            actor,
            json!({"type":"upsertProfile","actor":{"id":actor,"kind":"human","displayName":"Forwarder"}}),
        )
        .unwrap();
        for (id, title) in [
            ("conversation:forward-source", "Source"),
            ("conversation:forward-destination", "Destination"),
        ] {
            execute(
                &mut service,
                &format!("create:{id}"),
                actor,
                json!({"type":"createConversation","conversation":{
                    "id":id,"kind":"direct","title":title,
                    "participants":[{"actorId":actor,"role":"owner","joinedAtMs":1}]
                }}),
            )
            .unwrap();
        }
        execute(
            &mut service,
            "forward-source-message",
            actor,
            json!({"type":"sendMessage","conversationId":"conversation:forward-source",
                "clientMessageId":"client:source","content":{"type":"text","data":{"text":{"text":"forward me","entities":[]}}}}),
        )
        .unwrap();
        let source_id = stable_message_id(actor, "client:source");
        let base = json!({"type":"forwardMessage",
            "sourceConversationId":"conversation:forward-source",
            "messageId":source_id,
            "destinationConversationId":"conversation:forward-destination",
            "clientMessageId":"client:forward",
            "threadRootMessageId":Value::Null,
            "scheduledAtMs":50,
            "silent":true,
            "privacy":{"dropSenderNames":false,"dropCaptions":false}
        });
        execute(&mut service, "forward-first", actor, base.clone()).unwrap();
        let cursor = service.state.cursor;
        let replay = execute(&mut service, "forward-replay", actor, base.clone()).unwrap();
        assert_eq!(service.state.cursor, cursor);
        assert!(replay["envelopes"].as_array().unwrap().iter().any(|envelope| {
            envelope["event"]["type"] == "messageChanged"
        }));

        let mut silent_conflict = base.clone();
        silent_conflict["silent"] = json!(false);
        let error = execute(&mut service, "forward-silent-conflict", actor, silent_conflict)
            .unwrap_err();
        assert!(error.contains("conflicts with the canonical forward payload"));

        let mut privacy_conflict = base;
        privacy_conflict["privacy"]["dropSenderNames"] = json!(true);
        let error = execute(&mut service, "forward-privacy-conflict", actor, privacy_conflict)
            .unwrap_err();
        assert!(error.contains("conflicts with the canonical forward payload"));
        assert_eq!(service.state.cursor, cursor);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn different_actors_can_reuse_the_same_client_message_id_without_collision() {
        let root = temp_root("client-message-id-actor-scope");
        let owner = "human:owner";
        let peer = "human:peer";
        let mut service = AndroidMessagingService::open(&root).unwrap();
        for (request_id, actor, name) in [("actor-owner", owner, "Owner"), ("actor-peer", peer, "Peer")] {
            execute(&mut service, request_id, actor,
                json!({"type":"upsertProfile","actor":{"id":actor,"kind":"human","displayName":name}})).unwrap();
        }
        execute(&mut service, "actor-scope-conversation", owner, json!({"type":"createConversation","conversation":{
            "id":"conversation:actor-scope","kind":"direct","title":"Actor scoped client ids","ownerId":owner,
            "participants":[{"actorId":owner,"role":"owner","joinedAtMs":1},{"actorId":peer,"role":"member","joinedAtMs":1}],
            "permissions":{"canSendMessages":true,"canSendMedia":true}
        }})).unwrap();
        for (request_id, actor, text) in [("same-client-owner", owner, "from owner"), ("same-client-peer", peer, "from peer")] {
            execute(&mut service, request_id, actor, json!({"type":"sendMessage","conversationId":"conversation:actor-scope",
                "clientMessageId":"client:same","content":{"type":"text","data":{"text":{"text":text,"entities":[]}}}})).unwrap();
        }
        let messages = &service.state.messages["conversation:actor-scope"];
        assert_eq!(messages.len(), 2);
        let owner_id = stable_message_id(owner, "client:same");
        let peer_id = stable_message_id(peer, "client:same");
        assert_ne!(owner_id, peer_id);
        assert_eq!(messages[&owner_id]["clientMessageId"], "client:same");
        assert_eq!(messages[&peer_id]["clientMessageId"], "client:same");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn recipient_sync_marks_direct_message_delivered_and_mark_read_moves_it_to_read() {
        let root = temp_root("direct-delivery-state");
        let owner = "human:owner";
        let peer = "human:peer";
        let mut service = AndroidMessagingService::open(&root).unwrap();
        execute(
            &mut service,
            "delivery-owner",
            owner,
            json!({"type":"upsertProfile","actor":{"id":owner,"kind":"human","displayName":"Owner"}}),
        )
        .unwrap();
        execute(
            &mut service,
            "delivery-peer",
            peer,
            json!({"type":"upsertProfile","actor":{"id":peer,"kind":"human","displayName":"Peer"}}),
        )
        .unwrap();
        execute(
            &mut service,
            "delivery-conversation",
            owner,
            json!({"type":"createConversation","conversation":{
                "id":"conversation:delivery",
                "kind":"direct",
                "title":"Delivery",
                "ownerId":owner,
                "participants":[
                    {"actorId":owner,"role":"owner","joinedAtMs":2},
                    {"actorId":peer,"role":"member","joinedAtMs":2}
                ]
            }}),
        )
        .unwrap();
        execute(
            &mut service,
            "delivery-send",
            owner,
            json!({"type":"sendMessage","conversationId":"conversation:delivery","clientMessageId":"message:delivery",
                "content":{"type":"text","data":{"text":{"text":"hello","entities":[]}}}}),
        )
        .unwrap();
        let delivery_message_id = stable_message_id(owner, "message:delivery");
        assert_eq!(
            service.state.messages["conversation:delivery"][&delivery_message_id]["deliveryState"],
            "sent"
        );

        execute(
            &mut service,
            "delivery-sync-peer",
            peer,
            json!({"type":"sync","limit":100}),
        )
        .unwrap();
        assert_eq!(
            service.state.messages["conversation:delivery"][&delivery_message_id]["deliveryState"],
            "delivered"
        );

        execute(
            &mut service,
            "delivery-read-peer",
            peer,
            json!({"type":"markRead","conversationId":"conversation:delivery","messageId":delivery_message_id}),
        )
        .unwrap();
        assert_eq!(
            service.state.messages["conversation:delivery"][&delivery_message_id]["deliveryState"],
            "read"
        );
        drop(service);

        let reopened = AndroidMessagingService::open(&root).unwrap();
        assert_eq!(
            reopened.state.messages["conversation:delivery"][&delivery_message_id]["deliveryState"],
            "read",
            "delivery/read transition must survive Host process recreation"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unread_projection_is_actor_scoped_and_read_cursor_survives_reopen() {
        let root = temp_root("unread-projection");
        let me = "human:me";
        let peer = "human:peer";
        let conversation_id = "direct:unread-contract";
        let mut service = AndroidMessagingService::open(&root).unwrap();
        execute(
            &mut service,
            "unread-me",
            me,
            json!({"type":"upsertProfile","actor":{"id":me,"kind":"human","displayName":"Me"}}),
        )
        .unwrap();
        execute(
            &mut service,
            "unread-peer",
            peer,
            json!({"type":"upsertProfile","actor":{"id":peer,"kind":"human","displayName":"Peer"}}),
        )
        .unwrap();
        execute(
            &mut service,
            "unread-conversation",
            me,
            json!({"type":"createConversation","conversation":{
                "id":conversation_id,
                "kind":"direct",
                "title":"Unread contract",
                "ownerId":me,
                "participants":[
                    {"actorId":me,"role":"owner","joinedAtMs":3},
                    {"actorId":peer,"role":"member","joinedAtMs":3}
                ],
                "permissions":{
                    "canSendMessages":true,
                    "canSendMedia":true,
                    "canSendPolls":true,
                    "canPinMessages":true
                }
            }}),
        )
        .unwrap();
        execute(
            &mut service,
            "unread-incoming",
            peer,
            json!({"type":"sendMessage","conversationId":conversation_id,"clientMessageId":"message:unread",
                "content":{"type":"text","data":{"text":{"text":"first unread","entities":[]}}}}),
        )
        .unwrap();
        let unread_message_id = stable_message_id(peer, "message:unread");

        let mine = execute(&mut service, "unread-sync-me", me, json!({"type":"sync","limit":100}))
            .unwrap();
        let my_conversation = mine["envelopes"][0]["event"]["conversations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|conversation| conversation["id"] == conversation_id)
            .unwrap();
        assert_eq!(my_conversation["unreadCount"], 1);
        assert_eq!(my_conversation["lastReadMessageId"], Value::Null);

        let theirs =
            execute(&mut service, "unread-sync-peer", peer, json!({"type":"sync","limit":100}))
                .unwrap();
        let peer_conversation = theirs["envelopes"][0]["event"]["conversations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|conversation| conversation["id"] == conversation_id)
            .unwrap();
        assert_eq!(peer_conversation["unreadCount"], 0);

        execute(
            &mut service,
            "unread-mark-read",
            me,
            json!({"type":"markRead","conversationId":conversation_id,"messageId":unread_message_id}),
        )
        .unwrap();
        drop(service);

        let mut reopened = AndroidMessagingService::open(&root).unwrap();
        let restored =
            execute(&mut reopened, "unread-restored", me, json!({"type":"sync","limit":100}))
                .unwrap();
        let restored_conversation = restored["envelopes"][0]["event"]["conversations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|conversation| conversation["id"] == conversation_id)
            .unwrap();
        assert_eq!(restored_conversation["unreadCount"], 0);
        assert_eq!(restored_conversation["lastReadMessageId"], stable_message_id(peer, "message:unread"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn durable_delta_sync_survives_restart_and_is_audience_scoped() {
        let root = temp_root("delta-restart-audience");
        let owner = "human:owner";
        let peer = "human:peer";
        let outsider = "human:outsider";
        let mut service = AndroidMessagingService::open(&root).unwrap();
        execute(
            &mut service,
            "delta-owner",
            owner,
            json!({"type":"upsertProfile","actor":{"id":owner,"kind":"human","displayName":"Owner"}}),
        )
        .unwrap();
        execute(
            &mut service,
            "delta-peer",
            peer,
            json!({"type":"upsertProfile","actor":{"id":peer,"kind":"human","displayName":"Peer"}}),
        )
        .unwrap();
        execute(
            &mut service,
            "delta-outsider",
            outsider,
            json!({"type":"upsertProfile","actor":{"id":outsider,"kind":"human","displayName":"Outsider"}}),
        )
        .unwrap();
        execute(
            &mut service,
            "delta-conversation",
            owner,
            json!({"type":"createConversation","conversation":{
                "id":"conversation:delta",
                "kind":"direct",
                "title":"Delta",
                "ownerId":owner,
                "participants":[
                    {"actorId":owner,"role":"owner","joinedAtMs":2},
                    {"actorId":peer,"role":"member","joinedAtMs":2}
                ]
            }}),
        )
        .unwrap();
        let peer_baseline = execute(
            &mut service,
            "delta-peer-baseline",
            peer,
            json!({"type":"sync","limit":100}),
        )
        .unwrap();
        let peer_cursor = peer_baseline["envelopes"][0]["event"]["nextCursor"]
            .as_str()
            .unwrap()
            .to_string();
        let outsider_baseline = execute(
            &mut service,
            "delta-outsider-baseline",
            outsider,
            json!({"type":"sync","limit":100}),
        )
        .unwrap();
        let outsider_cursor = outsider_baseline["envelopes"][0]["event"]["nextCursor"]
            .as_str()
            .unwrap()
            .to_string();
        execute(
            &mut service,
            "delta-after-baseline",
            owner,
            json!({"type":"sendMessage","conversationId":"conversation:delta","clientMessageId":"message:delta-restart",
                "content":{"type":"text","data":{"text":{"text":"after baseline","entities":[]}}}}),
        )
        .unwrap();
        drop(service);

        let mut reopened = AndroidMessagingService::open(&root).unwrap();
        let peer_delta = execute(
            &mut reopened,
            "delta-peer-after-restart",
            peer,
            json!({"type":"sync","cursor":peer_cursor,"limit":100}),
        )
        .unwrap();
        assert!(peer_delta["envelopes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|envelope| {
                matches!(
                    envelope["event"]["type"].as_str(),
                    Some("messageAdded") | Some("messageChanged")
                ) && envelope["event"]["message"]["id"] == stable_message_id(owner, "message:delta-restart")
            }));

        let outsider_delta = execute(
            &mut reopened,
            "delta-outsider-after-restart",
            outsider,
            json!({"type":"sync","cursor":outsider_cursor,"limit":100}),
        )
        .unwrap();
        assert!(!outsider_delta["envelopes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|envelope| {
                matches!(
                    envelope["event"]["type"].as_str(),
                    Some("messageAdded") | Some("messageChanged")
                ) && envelope["event"]["message"]["id"] == stable_message_id(owner, "message:delta-restart")
            }));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn delta_sync_replays_whole_cursor_groups_and_emits_checkpoint() {
        let root = temp_root("delta-sync");
        let owner = "human:owner";
        let peer = "human:peer";
        let mut service = AndroidMessagingService::open(&root).unwrap();
        seed(&mut service, owner);
        execute(
            &mut service,
            "peer-profile",
            peer,
            json!({"type":"upsertProfile","actor":{"id":peer,"kind":"human","displayName":"Peer"}}),
        )
        .unwrap();
        execute(
            &mut service,
            "participant",
            owner,
            json!({"type":"setConversationParticipant","conversationId":"conversation:parent",
                "participant":{"actorId":peer,"role":"member","joinedAtMs":2}}),
        )
        .unwrap();
        let checkpoint = service.state.cursor;
        execute(
            &mut service,
            "message-after-checkpoint",
            owner,
            json!({"type":"sendMessage","conversationId":"conversation:parent","clientMessageId":"message:delta",
                "content":{"type":"text","data":{"text":{"text":"delta","entities":[]}}}}),
        )
        .unwrap();

        let response = execute(
            &mut service,
            "peer-delta",
            peer,
            json!({"type":"sync","cursor":checkpoint.to_string(),"limit":1}),
        )
        .unwrap();
        let envelopes = response["envelopes"].as_array().unwrap();
        assert!(envelopes.iter().any(|envelope| {
            envelope["event"]["type"] == "messageAdded"
                && envelope["event"]["message"]["id"] == stable_message_id(owner, "message:delta")
        }));
        let checkpoint_event = &envelopes.last().unwrap()["event"];
        assert_eq!(checkpoint_event["type"], "syncBatch");
        assert_eq!(checkpoint_event["fullSnapshot"], false);
        assert_eq!(
            checkpoint_event["nextCursor"].as_str().unwrap(),
            service.state.cursor.to_string()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn delta_replay_reprojects_actor_scoped_reaction_state() {
        let root = temp_root("delta-actor-projection");
        let owner = "human:owner";
        let peer = "human:peer";
        let mut service = AndroidMessagingService::open(&root).unwrap();
        let root_message_id = seed(&mut service, owner);
        execute(
            &mut service,
            "peer-profile-projection",
            peer,
            json!({"type":"upsertProfile","actor":{"id":peer,"kind":"human","displayName":"Peer"}}),
        )
        .unwrap();
        execute(
            &mut service,
            "participant-projection",
            owner,
            json!({"type":"setConversationParticipant","conversationId":"conversation:parent",
                "participant":{"actorId":peer,"role":"member","joinedAtMs":2}}),
        )
        .unwrap();
        let checkpoint = service.state.cursor;
        execute(
            &mut service,
            "owner-reaction",
            owner,
            json!({"type":"setReaction","conversationId":"conversation:parent","messageId":root_message_id,
                "reaction":{"reaction":"👍","chosenByMe":true}}),
        )
        .unwrap();

        let peer_delta = execute(
            &mut service,
            "peer-reaction-delta",
            peer,
            json!({"type":"sync","cursor":checkpoint.to_string(),"limit":10}),
        )
        .unwrap();
        let message = peer_delta["envelopes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|envelope| envelope["event"]["type"] == "messageChanged")
            .unwrap()["event"]["message"]
            .clone();
        assert_eq!(message["reactions"][0]["count"], 1);
        assert_eq!(message["reactions"][0]["chosenByMe"], false);

        let owner_delta = execute(
            &mut service,
            "owner-reaction-delta",
            owner,
            json!({"type":"sync","cursor":checkpoint.to_string(),"limit":10}),
        )
        .unwrap();
        let owner_message = owner_delta["envelopes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|envelope| envelope["event"]["type"] == "messageChanged")
            .unwrap()["event"]["message"]
            .clone();
        assert_eq!(owner_message["reactions"][0]["chosenByMe"], true);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn journal_pruning_drops_complete_cursor_groups() {
        let root = temp_root("journal-prune");
        let mut service = AndroidMessagingService::open(&root).unwrap();
        service.state.cursor = MESSAGING_EVENT_JOURNAL_LIMIT as u64;
        service.state.event_journal.push(MessagingJournalEntry {
            cursor: 1,
            audience: vec!["human:owner".into()],
            envelope: json!({"cursor":"1","event":{"type":"typingChanged"}}),
        });
        service.state.event_journal.push(MessagingJournalEntry {
            cursor: 1,
            audience: vec!["human:peer".into()],
            envelope: json!({"cursor":"1","event":{"type":"typingChanged"}}),
        });
        service.state.event_journal.extend(
            (2..=(MESSAGING_EVENT_JOURNAL_LIMIT as u64)).map(|cursor| {
                MessagingJournalEntry {
                    cursor,
                    audience: vec!["human:owner".into()],
                    envelope: json!({
                        "cursor":cursor.to_string(),
                        "event":{"type":"typingChanged"}
                    }),
                }
            }),
        );
        assert_eq!(
            service.state.event_journal.len(),
            MESSAGING_EVENT_JOURNAL_LIMIT + 1,
        );

        service.prune_event_journal();

        assert_eq!(service.state.journal_floor_cursor, 1);
        assert_eq!(
            service.state.event_journal.len(),
            MESSAGING_EVENT_JOURNAL_LIMIT - 1,
        );
        assert!(service
            .state
            .event_journal
            .iter()
            .all(|entry| entry.cursor > service.state.journal_floor_cursor));
        assert!(!service
            .state
            .event_journal
            .iter()
            .any(|entry| entry.cursor == 1));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn invisible_cursor_gap_invalidates_older_delta_and_falls_back_to_snapshot() {
        let root = temp_root("delta-gap");
        let actor = "human:owner";
        let mut service = AndroidMessagingService::open(&root).unwrap();
        let root_message_id = seed(&mut service, actor);
        let before = service.state.cursor;
        execute(
            &mut service,
            "child-pin-gap",
            actor,
            json!({"type":"setConversationChildPinned","destination":{
                "conversationId":"conversation:parent",
                "child":{"kind":"topic","rootMessageId":root_message_id}
            },"pinned":true}),
        )
        .unwrap();
        assert!(service.state.journal_floor_cursor > before);
        let response = execute(
            &mut service,
            "sync-gap",
            actor,
            json!({"type":"sync","cursor":before.to_string(),"limit":10}),
        )
        .unwrap();
        assert_eq!(
            response["envelopes"][0]["event"]["fullSnapshot"],
            Value::Bool(true)
        );
        assert_eq!(
            response["envelopes"][0]["event"]["conversationChildren"][0]["pinned"],
            Value::Bool(true)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn corrupt_repository_recovers_only_from_valid_last_known_good_snapshot() {
        let root = temp_root("corrupt-backup");
        let owner = "human:owner";
        {
            let mut service = AndroidMessagingService::open(&root).unwrap();
            seed(&mut service, owner);
            execute(
                &mut service,
                "backup-second-settlement",
                owner,
                json!({"type":"setDraft","conversationId":"conversation:parent","text":"second settlement","replyToMessageId":Value::Null}),
            )
            .unwrap();
        }
        let path = root.join("messaging-repository.json");
        let backup = repository_backup_path(&path);
        assert!(backup.exists(), "a settled predecessor must be retained for corruption recovery");
        let trusted_backup = fs::read(&backup).unwrap();
        let (expected, _) = decode_repository_state(&trusted_backup).unwrap();

        fs::write(&path, b"{broken-canonical").unwrap();
        let recovered = AndroidMessagingService::open(&root).unwrap();
        assert_eq!(recovered.state.cursor, expected.cursor);
        assert_eq!(recovered.state.request_results, expected.request_results);
        assert_eq!(recovered.state.event_journal.len(), expected.event_journal.len());
        assert!(AndroidMessagingService::open(&root).is_ok(), "recovery must reinstall a valid canonical repository");
        assert!(
            fs::read_dir(&root)
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| entry.file_name().to_string_lossy().starts_with("messaging-repository.json.corrupt-")),
            "damaged canonical bytes must remain quarantined for diagnostics"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn corrupt_repository_without_valid_backup_stays_fail_closed() {
        let root = temp_root("corrupt-no-backup");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("messaging-repository.json");
        fs::write(&path, b"{broken-canonical").unwrap();

        let error = AndroidMessagingService::open(&root).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(fs::read(&path).unwrap(), b"{broken-canonical");
        assert!(
            fs::read_dir(&root)
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| entry.file_name().to_string_lossy().starts_with("messaging-repository.json.corrupt-")),
            "fail-closed recovery should still retain a diagnostic quarantine copy"
        );
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
        let root_message_id = seed(&mut service, owner);
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
            json!({"type":"setReaction","conversationId":"conversation:parent","messageId":root_message_id,
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
    fn conversation_admin_cannot_promote_modify_or_remove_owner_admin_roles() {
        let root = temp_root("admin-role-fence");
        let owner = "human:owner";
        let admin = "human:admin";
        let other_admin = "human:other-admin";
        let member = "human:member";
        let mut service = AndroidMessagingService::open(&root).unwrap();

        for (request_id, actor, name) in [
            ("admin-profile-owner", owner, "Owner"),
            ("admin-profile-admin", admin, "Admin"),
            ("admin-profile-other", other_admin, "Other Admin"),
            ("admin-profile-member", member, "Member"),
        ] {
            execute(
                &mut service,
                request_id,
                actor,
                json!({"type":"upsertProfile","actor":{"id":actor,"kind":"human","displayName":name}}),
            )
            .unwrap();
        }
        execute(
            &mut service,
            "admin-conversation",
            owner,
            json!({"type":"createConversation","conversation":{
                "id":"conversation:admin-fence",
                "kind":"group",
                "title":"Admin fence",
                "ownerId":owner,
                "participants":[
                    {"actorId":owner,"role":"owner","joinedAtMs":1},
                    {"actorId":admin,"role":"admin","joinedAtMs":1},
                    {"actorId":other_admin,"role":"admin","joinedAtMs":1},
                    {"actorId":member,"role":"member","joinedAtMs":1}
                ]
            }}),
        )
        .unwrap();

        assert!(execute(
            &mut service,
            "admin-promote-member",
            admin,
            json!({"type":"setConversationParticipant","conversationId":"conversation:admin-fence",
                "participant":{"actorId":member,"role":"admin","joinedAtMs":1}}),
        )
        .is_err());
        assert!(execute(
            &mut service,
            "admin-demote-admin",
            admin,
            json!({"type":"setConversationParticipant","conversationId":"conversation:admin-fence",
                "participant":{"actorId":other_admin,"role":"member","joinedAtMs":1}}),
        )
        .is_err());
        assert!(execute(
            &mut service,
            "admin-remove-admin",
            admin,
            json!({"type":"removeConversationParticipant","conversationId":"conversation:admin-fence","actorId":other_admin}),
        )
        .is_err());
        assert!(execute(
            &mut service,
            "admin-remove-owner",
            admin,
            json!({"type":"removeConversationParticipant","conversationId":"conversation:admin-fence","actorId":owner}),
        )
        .is_err());

        execute(
            &mut service,
            "owner-remove-admin",
            owner,
            json!({"type":"removeConversationParticipant","conversationId":"conversation:admin-fence","actorId":other_admin}),
        )
        .unwrap();
        execute(
            &mut service,
            "admin-remove-member",
            admin,
            json!({"type":"removeConversationParticipant","conversationId":"conversation:admin-fence","actorId":member}),
        )
        .unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn poll_vote_projection_keeps_actor_choices_private_and_counts_shared() {
        let root = temp_root("poll-vote-projection");
        let alice = "human:alice";
        let bob = "human:bob";
        let conversation_id = "conversation:poll";
        let mut service = AndroidMessagingService::open(&root).unwrap();

        for (request_id, actor, name) in [
            ("poll-profile-alice", alice, "Alice"),
            ("poll-profile-bob", bob, "Bob"),
        ] {
            execute(
                &mut service,
                request_id,
                actor,
                json!({"type":"upsertProfile","actor":{"id":actor,"kind":"human","displayName":name}}),
            )
            .unwrap();
        }
        execute(
            &mut service,
            "poll-conversation",
            alice,
            json!({"type":"createConversation","conversation":{
                "id":conversation_id,
                "kind":"group",
                "title":"Poll",
                "ownerId":alice,
                "participants":[
                    {"actorId":alice,"role":"owner","joinedAtMs":1},
                    {"actorId":bob,"role":"member","joinedAtMs":1}
                ],
                "permissions":{"canSendMessages":true,"canSendPolls":true}
            }}),
        )
        .unwrap();
        execute(
            &mut service,
            "poll-send",
            alice,
            json!({"type":"sendMessage","conversationId":conversation_id,"clientMessageId":"poll:one",
                "content":{"type":"poll","data":{
                    "question":"Pick one",
                    "multipleAnswers":false,
                    "options":[
                        {"id":"a","text":"A","voterCount":0,"chosen":false},
                        {"id":"b","text":"B","voterCount":0,"chosen":false}
                    ]
                }}}),
        )
        .unwrap();
        let poll_message_id = stable_message_id(alice, "poll:one");
        execute(
            &mut service,
            "poll-vote-alice",
            alice,
            json!({"type":"votePoll","conversationId":conversation_id,"messageId":poll_message_id,"optionIds":["a"]}),
        )
        .unwrap();
        execute(
            &mut service,
            "poll-vote-bob",
            bob,
            json!({"type":"votePoll","conversationId":conversation_id,"messageId":poll_message_id,"optionIds":["b"]}),
        )
        .unwrap();

        for (actor, chosen_a, chosen_b) in [(alice, true, false), (bob, false, true)] {
            let sync = execute(
                &mut service,
                &format!("poll-sync-{actor}"),
                actor,
                json!({"type":"sync","limit":100}),
            )
            .unwrap();
            let message = sync["envelopes"][0]["event"]["messages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|message| message["id"] == poll_message_id)
                .unwrap();
            let options = message["content"]["data"]["options"].as_array().unwrap();
            assert_eq!(options[0]["voterCount"], 1);
            assert_eq!(options[1]["voterCount"], 1);
            assert_eq!(options[0]["chosen"], chosen_a);
            assert_eq!(options[1]["chosen"], chosen_b);
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn forward_edit_delete_pin_read_folder_and_typing_survive_repository_reopen() {
        let root = temp_root("commands");
        let actor = "human:owner";
        {
            let mut service = AndroidMessagingService::open(&root).unwrap();
            let root_message_id = seed(&mut service, actor);
            execute(
                &mut service,
                "edit",
                actor,
                json!({"type":"editMessage","conversationId":"conversation:parent","messageId":root_message_id.clone(),
                    "content":{"type":"text","data":{"text":{"text":"edited","entities":[]}}}}),
            )
            .unwrap();
            execute(
                &mut service,
                "pin-message",
                actor,
                json!({"type":"pinMessage","conversationId":"conversation:parent","messageId":root_message_id.clone(),"pinned":true}),
            )
            .unwrap();
            execute(
                &mut service,
                "mark-read",
                actor,
                json!({"type":"markRead","conversationId":"conversation:parent","messageId":root_message_id.clone()}),
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
                    "messageId":root_message_id.clone(),"destinationConversationId":"conversation:parent","clientMessageId":"message:forward"}),
            )
            .unwrap();
            execute(
                &mut service,
                "delete",
                actor,
                json!({"type":"deleteMessages","conversationId":"conversation:parent","messageIds":[stable_message_id(actor, "message:forward")],"forEveryone":true}),
            )
            .unwrap();
        }
        let mut reopened = AndroidMessagingService::open(&root).unwrap();
        let sync = execute(&mut reopened, "sync-reopen-commands", actor, json!({"type":"sync"})).unwrap();
        let event = &sync["envelopes"][0]["event"];
        assert_eq!(event["messages"][0]["content"]["data"]["text"]["text"], "edited");
        assert_eq!(event["messages"][0]["pinned"], true);
        assert_eq!(event["conversations"][0]["archived"], true);
        assert_eq!(event["conversations"][0]["lastReadMessageId"], stable_message_id(actor, "message:root"));
        assert_eq!(event["folders"][0]["id"], "folder:one");
        assert_eq!(event["messages"].as_array().unwrap().len(), 1);
        let _ = fs::remove_dir_all(root);
    }


}


#[cfg(test)]
mod remote_human_reconcile_tests {
    use super::*;

    fn remote_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-remote-human-reconcile-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    fn remote_message(
        id: &str,
        sender: &str,
        recipient: &str,
        client_request_id: Option<&str>,
        text: &str,
        created_at: &str,
    ) -> Value {
        json!({
            "id":id,
            "senderUserId":sender,
            "senderUsername":"sender",
            "recipientUserId":recipient,
            "recipientUsername":"recipient",
            "text":text,
            "clientRequestId":client_request_id,
            "createdAt":created_at,
            "readAt":Value::Null,
            "isOutgoing":sender == "owner",
            "replyToMessageId":Value::Null,
            "attachments":[],
            "reactions":[{"emoji":"👍","count":1,"reactedByMe":sender == "owner"}]
        })
    }

    #[test]
    fn remote_human_message_reconcile_is_idempotent_durable_and_account_fenced() {
        let root = remote_root("durable");
        let owner = "human:owner";
        let peer = "human:peer";
        let conversation_id = stable_human_direct_conversation_id(owner, peer);
        let messages = vec![
            remote_message(
                "remote-1",
                "peer",
                "owner",
                None,
                "hello",
                "2026-10-13T00:00:00Z",
            ),
            remote_message(
                "remote-2",
                "owner",
                "peer",
                Some("server-known-nonce"),
                "reply",
                "2026-10-13T00:01:00Z",
            ),
        ];
        {
            let mut service = AndroidMessagingService::open(&root).unwrap();
            service
                .reconcile_remote_human_contacts(&[
                    json!({"id":peer,"kind":"human","displayName":"Peer"})
                ])
                .unwrap();
            assert_eq!(
                service
                    .reconcile_remote_human_messages(owner, peer, &messages)
                    .unwrap(),
                2
            );
            let cursor = service.state.cursor;
            assert_eq!(
                service
                    .reconcile_remote_human_messages(owner, peer, &messages)
                    .unwrap(),
                0,
                "same server page must be an idempotent replay"
            );
            assert_eq!(service.state.cursor, cursor);
            assert_eq!(
                service.state.conversations[&conversation_id]["lastMessageId"],
                "human-server-message:remote-2"
            );
            let projected = service.project_message(
                &service.state.messages[&conversation_id]["human-server-message:remote-1"],
                owner,
            );
            assert_eq!(projected["reactions"][0]["reaction"], "👍");
            assert_eq!(projected["reactions"][0]["count"], 1);

            let before_foreign = service.state.clone();
            let foreign = vec![remote_message(
                "foreign",
                "other",
                "someone-else",
                None,
                "wrong account",
                "2026-10-13T00:02:00Z",
            )];
            assert!(service
                .reconcile_remote_human_messages(owner, peer, &foreign)
                .is_err());
            assert_eq!(service.state.cursor, before_foreign.cursor);
            assert_eq!(service.state.messages, before_foreign.messages);
        }

        let reopened = AndroidMessagingService::open(&root).unwrap();
        assert!(
            reopened.state.messages[&conversation_id]
                .contains_key("human-server-message:remote-1"),
            "remote history must survive process death/reopen"
        );
        assert!(
            reopened.state.messages[&conversation_id]
                .contains_key("human-server-message:remote-2")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn remote_human_client_request_id_settles_local_send_identity() {
        let root = remote_root("settlement");
        let owner = "human:owner";
        let peer = "human:peer";
        let conversation_id = stable_human_direct_conversation_id(owner, peer);
        let mut service = AndroidMessagingService::open(&root).unwrap();
        service
            .reconcile_remote_human_contacts(&[
                json!({"id":peer,"kind":"human","displayName":"Peer"})
            ])
            .unwrap();
        service
            .reconcile_remote_human_messages(
                owner,
                peer,
                &[remote_message(
                    "seed",
                    "peer",
                    "owner",
                    None,
                    "seed",
                    "2026-10-13T00:00:00Z",
                )],
            )
            .unwrap();

        let local_request = json!({
            "requestId":"local-send",
            "envelope":{
                "protocolVersion":2,
                "context":{
                    "requestId":"local-send",
                    "deviceId":"android:test",
                    "actorId":owner,
                    "sessionId":"session:test",
                    "sentAtMs":1
                },
                "command":{
                    "type":"sendMessage",
                    "conversationId":conversation_id,
                    "clientMessageId":"settle-me",
                    "content":{"type":"text","data":{"text":{"text":"settled","entities":[]}}}
                }
            }
        });
        service.execute(&local_request, owner, 100).unwrap();
        let local_id = stable_message_id(owner, "settle-me");
        assert!(service.state.messages[&conversation_id].contains_key(&local_id));

        service
            .reconcile_remote_human_messages(
                owner,
                peer,
                &[remote_message(
                    "server-settled",
                    "owner",
                    "peer",
                    Some("settle-me"),
                    "settled",
                    "2026-10-13T00:03:00Z",
                )],
            )
            .unwrap();
        assert!(
            !service.state.messages[&conversation_id].contains_key(&local_id),
            "local optimistic identity must be replaced after server acknowledgement"
        );
        assert!(
            service.state.messages[&conversation_id]
                .contains_key("human-server-message:server-settled")
        );
        let _ = fs::remove_dir_all(root);
    }
}
