use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::agent_isolation::{
    ProductionAgentStoreWorkerBackend, WorkerBlobStore,
};
use crate::transcript_mirror::conversation_state_binary::{
    ConversationTurnStructureFields, SubagentPersistedStateFields,
    decode_conversation_state_recovery_fields, decode_conversation_turn_structure_fields,
    decode_subagent_persisted_state_fields,
};

use super::agent_db::{AgentDbSubscription, SandAgentDb};
use super::session_conversation_state::{
    ResolvedConversationState, SessionConversationState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductionAgentMetadataKey {
    AgentId,
    LatestRootBlobId,
    Name,
    Mode,
    IsRunEverything,
    ApprovalMode,
    CreatedAt,
    LastUsedModel,
    LastDebugServerPort,
    CurrentPlanUri,
    SubagentInfo,
    BlobEncryptionKey,
}

impl ProductionAgentMetadataKey {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AgentId => "agentId",
            Self::LatestRootBlobId => "latestRootBlobId",
            Self::Name => "name",
            Self::Mode => "mode",
            Self::IsRunEverything => "isRunEverything",
            Self::ApprovalMode => "approvalMode",
            Self::CreatedAt => "createdAt",
            Self::LastUsedModel => "lastUsedModel",
            Self::LastDebugServerPort => "lastDebugServerPort",
            Self::CurrentPlanUri => "currentPlanUri",
            Self::SubagentInfo => "subagentInfo",
            Self::BlobEncryptionKey => "blobEncryptionKey",
        }
    }
}

pub type ProductionMetadataListener =
    Arc<dyn Fn(Option<Value>) + Send + Sync + 'static>;

pub type ProductionWorkerBlobStore =
    WorkerBlobStore<ProductionAgentStoreWorkerBackend>;

#[derive(Debug, Clone, PartialEq)]
pub struct ProductionResolvedSubagentState {
    pub conversation_state: ResolvedConversationState,
    pub created_timestamp_ms: u64,
    pub last_used_timestamp_ms: u64,
    pub subagent_type: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct ProductionAgentCheckpointState {
    latest_root_blob_id: Vec<u8>,
    checkpoint_bytes: Option<Vec<u8>>,
}

/// Shipping Rust owner for the frozen AgentStore2 checkpoint boundary.
///
/// WorkerBlobStore owns content-addressed bytes and SandAgentDb owns metadata.
/// This composition advances latestRootBlobId only after the checkpoint blob
/// has been durably handed to the production blob worker.
pub struct ProductionAgentStore {
    pub agent_id: String,
    pub blob_db_path: PathBuf,
    pub legacy_blob_db_path: Option<PathBuf>,
    db: Arc<SandAgentDb>,
    blob_store: ProductionWorkerBlobStore,
    state: Mutex<ProductionAgentCheckpointState>,
}

impl ProductionAgentStore {
    pub fn new(
        db: Arc<SandAgentDb>,
        blob_store: ProductionWorkerBlobStore,
    ) -> Self {
        Self {
            agent_id: blob_store.agent_id.clone(),
            blob_db_path: blob_store.blob_db_path.clone(),
            legacy_blob_db_path: blob_store.legacy_blob_db_path.clone(),
            db,
            blob_store,
            state: Mutex::new(ProductionAgentCheckpointState::default()),
        }
    }

    pub fn db(&self) -> Arc<SandAgentDb> {
        Arc::clone(&self.db)
    }

    pub fn subscribe_to_metadata(
        &self,
        key: ProductionAgentMetadataKey,
        callback: ProductionMetadataListener,
    ) -> AgentDbSubscription {
        let db = Arc::clone(&self.db);
        let key_name = key.as_str().to_string();
        let callback_for_listener = Arc::clone(&callback);
        self.db.subscribe_metadata(
            key_name.clone(),
            Arc::new(move || {
                let value = db.get_metadata(&key_name).ok().flatten();
                callback_for_listener(value);
            }),
        )
    }

    pub fn set_metadata(
        &self,
        key: ProductionAgentMetadataKey,
        value: Value,
    ) -> Result<bool, String> {
        self.db
            .set_metadata(key.as_str(), value)
            .map_err(|error| error.to_string())
    }

    pub fn get_metadata(
        &self,
        key: ProductionAgentMetadataKey,
    ) -> Result<Option<Value>, String> {
        self.db
            .get_metadata(key.as_str())
            .map_err(|error| error.to_string())
    }

    pub fn get_id(&self) -> String {
        self.get_metadata(ProductionAgentMetadataKey::AgentId)
            .ok()
            .flatten()
            .and_then(|value| value.as_str().map(ToOwned::to_owned))
            .unwrap_or_else(|| self.agent_id.clone())
    }

    /// Frozen AgentStore2.getLastRequestIdFromConversation semantics. Missing
    /// turn blobs are skipped and the newest agent turn with a request id wins.
    pub fn get_last_request_id_from_conversation(&self) -> Result<Option<String>, String> {
        let checkpoint = self.latest_checkpoint_bytes().unwrap_or_default();
        let structure = decode_conversation_state_recovery_fields(&checkpoint)
            .map_err(|error| format!("invalid ConversationStateStructure checkpoint: {error}"))?;
        for turn_id in structure.turns.iter().rev() {
            let Some(turn_blob) = self.get_blob(turn_id)? else {
                continue;
            };
            if let Some(ConversationTurnStructureFields::Agent { request_id, .. }) =
                decode_conversation_turn_structure_fields(&turn_blob)
                    .map_err(|error| format!("invalid ConversationTurnStructure blob: {error}"))?
            {
                return Ok(request_id);
            }
        }
        Ok(None)
    }

    /// Shipping equivalent of AgentStore2.getFullConversation. It hydrates the
    /// store's current in-memory checkpoint leniently, matching the frozen API's
    /// skip-missing-blob behavior while reusing the production session decoder.
    pub fn get_full_conversation(&self) -> Result<ResolvedConversationState, String> {
        let Some(checkpoint) = self.latest_checkpoint_bytes() else {
            return Ok(ResolvedConversationState {
                turns: Vec::new(),
                todos: Vec::new(),
                summary: None,
            });
        };
        SessionConversationState::new(self.db.busy_timeout_ms())
            .resolve_conversation_state_bytes_lenient(
                Arc::clone(&self.blob_store.pool),
                &self.agent_id,
                self.db.db_path(),
                &self.blob_db_path,
                &checkpoint,
            )
            .map_err(|error| error.to_string())
    }

    /// Frozen AgentStore2.getFullConversationWithSubagents semantics. Inline
    /// persisted states are the fallback; valid ref blobs override them. A
    /// missing ref is fatal only when no inline entry exists for that subagent.
    pub fn get_full_conversation_with_subagents(
        &self,
    ) -> Result<
        (
            ResolvedConversationState,
            BTreeMap<String, ProductionResolvedSubagentState>,
        ),
        String,
    > {
        let conversation_state = self.get_full_conversation()?;
        let checkpoint = self.latest_checkpoint_bytes().unwrap_or_default();
        let structure = decode_conversation_state_recovery_fields(&checkpoint)
            .map_err(|error| format!("invalid ConversationStateStructure checkpoint: {error}"))?;

        let mut persisted = BTreeMap::<String, SubagentPersistedStateFields>::new();
        for (subagent_id, bytes) in structure.subagent_states {
            persisted.insert(
                subagent_id,
                decode_subagent_persisted_state_fields(&bytes)
                    .map_err(|error| format!("invalid inline subagent state: {error}"))?,
            );
        }

        let mut missing = Vec::new();
        for (subagent_id, blob_id) in structure.subagent_state_refs {
            match self.get_blob(&blob_id)? {
                Some(bytes) => {
                    persisted.insert(
                        subagent_id,
                        decode_subagent_persisted_state_fields(&bytes)
                            .map_err(|error| format!("invalid referenced subagent state: {error}"))?,
                    );
                }
                None if persisted.contains_key(&subagent_id) => {}
                None => missing.push(encode_hex(&blob_id)),
            }
        }
        if !missing.is_empty() {
            return Err(format!(
                "subagent state ref blob not found: {}",
                missing.join(",")
            ));
        }

        let resolver = SessionConversationState::new(self.db.busy_timeout_ms());
        let mut subagent_states = BTreeMap::new();
        for (subagent_id, state) in persisted {
            let Some(root_blob) = state.conversation_state else {
                continue;
            };
            let resolved = resolver
                .resolve_conversation_state_bytes_lenient(
                    Arc::clone(&self.blob_store.pool),
                    &self.agent_id,
                    self.db.db_path(),
                    &self.blob_db_path,
                    &root_blob,
                )
                .map_err(|error| error.to_string())?;
            subagent_states.insert(
                subagent_id,
                ProductionResolvedSubagentState {
                    conversation_state: resolved,
                    created_timestamp_ms: state.created_timestamp_ms,
                    last_used_timestamp_ms: state.last_used_timestamp_ms,
                    subagent_type: state.subagent_type,
                },
            );
        }
        Ok((conversation_state, subagent_states))
    }

    /// Frozen AgentStore2.tryResetFromDb semantics: bad metadata, missing blob,
    /// or malformed protobuf resets the in-memory checkpoint and returns false.
    pub fn try_reset_from_db(&self) -> bool {
        let root = match self.db.get_latest_root_blob_id() {
            Ok(root) => root,
            Err(_) => {
                self.clear_checkpoint();
                return false;
            }
        };
        if root.is_empty() {
            self.clear_checkpoint();
            return false;
        }
        let blob = match self.blob_store.get_blob_blocking(&root) {
            Ok(Some(blob)) => blob,
            _ => {
                self.clear_checkpoint();
                return false;
            }
        };
        if decode_conversation_state_recovery_fields(&blob).is_err() {
            self.clear_checkpoint();
            return false;
        }
        if let Ok(mut state) = self.state.lock() {
            state.latest_root_blob_id = root;
            state.checkpoint_bytes = Some(blob);
            true
        } else {
            false
        }
    }

    /// Frozen AgentStore2.handleCheckpoint semantics:
    /// protobuf bytes -> SHA-256 blob id -> blob write -> metadata root update.
    ///
    /// The async owner is the production checkpoint path. The synchronous
    /// wrapper remains for legacy synchronous callers, but Runner settle must
    /// await this method so it never nests a LocalPool executor.
    /// Shipping direct-user checkpoint boundary. Blob bytes are content
    /// addressed first; then SandAgentDb advances latestRootBlobId and the
    /// transcript recovery watermark in one IMMEDIATE SQLite transaction.
    pub fn handle_checkpoint_bytes_and_confirm_user_message_blocking(
        &self,
        checkpoint: &[u8],
        message_id: &str,
    ) -> Result<Vec<u8>, String> {
        decode_conversation_state_recovery_fields(checkpoint)
            .map_err(|error| format!("invalid ConversationStateStructure checkpoint: {error}"))?;

        let root_blob_id = Sha256::digest(checkpoint).to_vec();
        self.blob_store
            .set_blob_blocking(&root_blob_id, checkpoint)
            .map_err(|error| error.to_string())?;

        let expected_root = self.latest_root_blob_id();
        let confirmed = self
            .db
            .commit_checkpoint_root_and_confirm_user_message(
                &expected_root,
                &root_blob_id,
                message_id,
            )
            .map_err(|error| error.to_string())?;
        if confirmed.is_none() {
            return Err(
                "latestRootBlobId and confirmed user transcript entry were not committed atomically"
                    .into(),
            );
        }

        let mut state = self
            .state
            .lock()
            .map_err(|_| "production AgentStore checkpoint state poisoned".to_string())?;
        state.latest_root_blob_id = root_blob_id.clone();
        state.checkpoint_bytes = Some(checkpoint.to_vec());
        Ok(root_blob_id)
    }

    pub async fn handle_checkpoint_bytes_and_confirm_user_message_async(
        &self,
        checkpoint: &[u8],
        message_id: &str,
    ) -> Result<Vec<u8>, String> {
        self.handle_checkpoint_bytes_and_confirm_user_message_blocking(
            checkpoint,
            message_id,
        )
    }

    pub fn handle_checkpoint_bytes_blocking(
        &self,
        checkpoint: &[u8],
    ) -> Result<Vec<u8>, String> {
        decode_conversation_state_recovery_fields(checkpoint)
            .map_err(|error| format!("invalid ConversationStateStructure checkpoint: {error}"))?;

        let root_blob_id = Sha256::digest(checkpoint).to_vec();
        self.blob_store
            .set_blob_blocking(&root_blob_id, checkpoint)
            .map_err(|error| error.to_string())?;

        let advanced = self
            .db
            .set_metadata(
                "latestRootBlobId",
                Value::String(encode_hex(&root_blob_id)),
            )
            .map_err(|error| error.to_string())?;
        if !advanced {
            return Err("latestRootBlobId metadata update was not committed".into());
        }

        let mut state = self
            .state
            .lock()
            .map_err(|_| "production AgentStore checkpoint state poisoned".to_string())?;
        state.latest_root_blob_id = root_blob_id.clone();
        state.checkpoint_bytes = Some(checkpoint.to_vec());
        Ok(root_blob_id)
    }

    pub async fn handle_checkpoint_bytes_async(
        &self,
        checkpoint: &[u8],
    ) -> Result<Vec<u8>, String> {
        self.handle_checkpoint_bytes_blocking(checkpoint)
    }

    pub fn handle_checkpoint_bytes(
        &self,
        checkpoint: &[u8],
    ) -> Result<Vec<u8>, String> {
        self.handle_checkpoint_bytes_blocking(checkpoint)
    }

    pub fn latest_root_blob_id(&self) -> Vec<u8> {
        self.state
            .lock()
            .map(|state| state.latest_root_blob_id.clone())
            .unwrap_or_default()
    }

    pub fn latest_checkpoint_bytes(&self) -> Option<Vec<u8>> {
        self.state
            .lock()
            .ok()
            .and_then(|state| state.checkpoint_bytes.clone())
    }

    pub fn get_blob(
        &self,
        blob_id: &[u8],
    ) -> Result<Option<Vec<u8>>, String> {
        self.blob_store
            .get_blob_blocking(blob_id)
            .map_err(|error| error.to_string())
    }

    pub fn flush(&self) {
        self.blob_store.flush_blocking();
    }

    fn clear_checkpoint(&self) {
        if let Ok(mut state) = self.state.lock() {
            *state = ProductionAgentCheckpointState::default();
        }
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
