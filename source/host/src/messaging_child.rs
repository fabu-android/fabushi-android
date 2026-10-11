use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{self, Write},
    path::PathBuf,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConversationId(pub String);

impl ConversationId {
    pub fn is_valid(&self) -> bool {
        bounded_id(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "kind")]
pub enum ConversationChildIdentity {
    Topic { root_message_id: String },
    SavedSublist { participant_id: String },
    Conversation { conversation_id: ConversationId },
}

impl ConversationChildIdentity {
    pub fn is_valid(&self) -> bool {
        match self {
            Self::Topic { root_message_id } => bounded_id(root_message_id),
            Self::SavedSublist { participant_id } => bounded_id(participant_id),
            Self::Conversation { conversation_id } => conversation_id.is_valid(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationDestination {
    pub conversation_id: ConversationId,
    pub child: Option<ConversationChildIdentity>,
}

impl ConversationDestination {
    pub fn is_valid(&self) -> bool {
        self.conversation_id.is_valid()
            && self
                .child
                .as_ref()
                .is_none_or(ConversationChildIdentity::is_valid)
            && !matches!(
                &self.child,
                Some(ConversationChildIdentity::Conversation { conversation_id })
                    if conversation_id == &self.conversation_id
            )
    }

    pub fn can_toggle_unread(
        &self,
        currently_unread: bool,
        context: ConversationChildUnreadContext,
    ) -> bool {
        if (matches!(&self.child, Some(ConversationChildIdentity::Topic { .. }))
            || context.parent_is_community)
            && !currently_unread
        {
            return false;
        }
        if matches!(
            &self.child,
            Some(ConversationChildIdentity::SavedSublist { .. })
        ) && context.parent_is_self
        {
            return false;
        }
        if self.child.is_none() && context.actor_is_monoforum_admin {
            return false;
        }
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMessagePosition {
    pub created_at_ms: i64,
    pub message_id: String,
}

impl ConversationMessagePosition {
    pub fn is_valid(&self) -> bool {
        bounded_id(&self.message_id)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationChildPaginationState {
    pub message_ids: Vec<String>,
    pub skipped_before: Option<u32>,
    pub skipped_after: Option<u32>,
    pub full_count: Option<u32>,
}

impl ConversationChildPaginationState {
    pub fn replace_window(
        &mut self,
        message_ids: Vec<String>,
        skipped_before: Option<u32>,
        skipped_after: Option<u32>,
        full_count: Option<u32>,
    ) -> bool {
        if message_ids.iter().any(|value| !bounded_id(value)) {
            return false;
        }
        let unique = message_ids.iter().collect::<std::collections::BTreeSet<_>>();
        if unique.len() != message_ids.len() {
            return false;
        }
        if let (Some(before), Some(after), Some(full)) =
            (skipped_before, skipped_after, full_count)
        {
            let visible = u32::try_from(message_ids.len()).unwrap_or(u32::MAX);
            if before.saturating_add(visible).saturating_add(after) != full {
                return false;
            }
        }
        self.message_ids = message_ids;
        self.skipped_before = skipped_before;
        self.skipped_after = skipped_after;
        self.full_count = full_count;
        true
    }

    pub fn has_gap_before(&self) -> bool {
        self.skipped_before.is_none_or(|count| count > 0)
    }

    pub fn has_gap_after(&self) -> bool {
        self.skipped_after.is_none_or(|count| count > 0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationChildRuntimeState {
    pub destination: ConversationDestination,
    pub actor_id: String,
    pub inbox_read_till: Option<ConversationMessagePosition>,
    pub outbox_read_till: Option<ConversationMessagePosition>,
    pub unread_count: Option<u32>,
    pub marked_unread: bool,
    pub draft_text: String,
    pub draft_reply_to_message_id: Option<String>,
    pub draft_updated_at_ms: Option<i64>,
    pub pinned: bool,
    pub restore_pinned_when_non_empty: bool,
    pub active: bool,
    pub no_paid_messages: bool,
    pub pagination: ConversationChildPaginationState,
}

impl ConversationChildRuntimeState {
    pub fn new(destination: ConversationDestination, actor_id: impl Into<String>) -> Option<Self> {
        let actor_id = actor_id.into();
        if !destination.is_valid() || !bounded_id(&actor_id) {
            return None;
        }
        Some(Self {
            destination,
            actor_id,
            inbox_read_till: None,
            outbox_read_till: None,
            unread_count: None,
            marked_unread: false,
            draft_text: String::new(),
            draft_reply_to_message_id: None,
            draft_updated_at_ms: None,
            pinned: false,
            restore_pinned_when_non_empty: false,
            active: false,
            no_paid_messages: false,
            pagination: ConversationChildPaginationState::default(),
        })
    }

    pub fn advance_inbox_read_till(
        &mut self,
        position: ConversationMessagePosition,
        unread_count: Option<u32>,
    ) -> bool {
        if !position.is_valid()
            || self
                .inbox_read_till
                .as_ref()
                .is_some_and(|current| position < *current)
        {
            return false;
        }
        self.inbox_read_till = Some(position);
        if unread_count.is_some() || self.unread_count.is_none() {
            self.unread_count = unread_count;
        }
        self.marked_unread = false;
        true
    }

    pub fn advance_outbox_read_till(&mut self, position: ConversationMessagePosition) -> bool {
        if !position.is_valid()
            || self
                .outbox_read_till
                .as_ref()
                .is_some_and(|current| position < *current)
        {
            return false;
        }
        self.outbox_read_till = Some(position);
        true
    }

    pub fn set_draft(
        &mut self,
        text: impl Into<String>,
        reply_to_message_id: Option<String>,
        updated_at_ms: i64,
    ) {
        self.draft_text = text.into();
        self.draft_reply_to_message_id = reply_to_message_id;
        self.draft_updated_at_ms = Some(updated_at_ms);
    }

    pub fn clear_draft(&mut self) {
        self.draft_text.clear();
        self.draft_reply_to_message_id = None;
        self.draft_updated_at_ms = None;
    }

    pub fn note_locally_empty(&mut self) {
        if self.pinned {
            self.pinned = false;
            self.restore_pinned_when_non_empty = true;
        }
    }

    pub fn note_non_empty(&mut self) {
        if self.restore_pinned_when_non_empty {
            self.pinned = true;
            self.restore_pinned_when_non_empty = false;
        }
    }

    pub fn destroy(&mut self) {
        self.active = false;
        self.clear_draft();
        self.pagination = ConversationChildPaginationState::default();
        self.unread_count = None;
        self.marked_unread = false;
        self.pinned = false;
        self.restore_pinned_when_non_empty = false;
        self.no_paid_messages = false;
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConversationChildUnreadContext {
    pub parent_is_self: bool,
    pub parent_is_community: bool,
    pub actor_is_monoforum_admin: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedChildState {
    #[serde(default)]
    states: Vec<ConversationChildRuntimeState>,
}

pub struct ConversationChildStore {
    path: PathBuf,
    state: PersistedChildState,
}

impl ConversationChildStore {
    pub fn open(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        let state = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| {
                io::Error::new(io::ErrorKind::InvalidData, format!("invalid conversation-child store: {error}"))
            })?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => PersistedChildState::default(),
            Err(error) => return Err(error),
        };
        validate_persisted_child_state(&state)?;
        Ok(Self { path, state })
    }

    pub fn states_for_actor(&self, actor_id: &str) -> Vec<ConversationChildRuntimeState> {
        self.state
            .states
            .iter()
            .filter(|state| state.actor_id == actor_id)
            .cloned()
            .collect()
    }

    pub fn upsert_with(
        &mut self,
        destination: ConversationDestination,
        actor_id: &str,
        mutate: impl FnOnce(&mut ConversationChildRuntimeState) -> Result<(), &'static str>,
    ) -> Result<ConversationChildRuntimeState, &'static str> {
        if !destination.is_valid() || !bounded_id(actor_id) {
            return Err("invalid conversation child identity");
        }
        let previous = self.state.clone();
        let index = self
            .state
            .states
            .iter()
            .position(|state| state.destination == destination && state.actor_id == actor_id);
        let entry = if let Some(index) = index {
            &mut self.state.states[index]
        } else {
            self.state.states.push(
                ConversationChildRuntimeState::new(destination, actor_id)
                    .expect("validated conversation child state"),
            );
            self.state.states.last_mut().expect("inserted conversation child state")
        };
        mutate(entry)?;
        let projected = entry.clone();
        if self.persist().is_err() {
            self.state = previous;
            return Err("conversation child persistence failed");
        }
        Ok(projected)
    }

    pub fn destroy_exact(
        &mut self,
        destination: &ConversationDestination,
        actor_id: &str,
    ) -> Result<bool, &'static str> {
        if !destination.is_valid() || !bounded_id(actor_id) {
            return Err("invalid conversation child identity");
        }
        let previous = self.state.clone();
        let before = self.state.states.len();
        self.state.states.retain(|state| {
            !(state.destination == *destination && state.actor_id == actor_id)
        });
        let removed = self.state.states.len() != before;
        if removed && self.persist().is_err() {
            self.state = previous;
            return Err("conversation child persistence failed");
        }
        Ok(removed)
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

fn validate_persisted_child_state(state: &PersistedChildState) -> io::Result<()> {
    let mut identities = std::collections::BTreeSet::new();
    for child in &state.states {
        if !child.destination.is_valid()
            || !bounded_id(&child.actor_id)
            || child
                .pagination
                .message_ids
                .iter()
                .any(|message_id| !bounded_id(message_id))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "conversation-child store contains invalid identity or pagination state",
            ));
        }
        let key = (child.actor_id.clone(), child.destination.clone());
        if !identities.insert(key) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "conversation-child store contains duplicate actor/destination state",
            ));
        }
    }
    Ok(())
}

fn bounded_id(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty() && value.len() <= 200
}

#[cfg(test)]
mod tests {
    use super::*;

    fn destination() -> ConversationDestination {
        ConversationDestination {
            conversation_id: ConversationId("conversation:parent".into()),
            child: Some(ConversationChildIdentity::Conversation {
                conversation_id: ConversationId("conversation:child".into()),
            }),
        }
    }

    #[test]
    fn pagination_rejects_duplicates_and_inconsistent_full_count() {
        let mut page = ConversationChildPaginationState::default();
        assert!(!page.replace_window(
            vec!["m1".into(), "m1".into()],
            Some(0),
            Some(0),
            Some(2),
        ));
        assert!(!page.replace_window(
            vec!["m1".into()],
            Some(1),
            Some(1),
            Some(99),
        ));
        assert!(page.replace_window(
            vec!["m1".into()],
            Some(1),
            Some(1),
            Some(3),
        ));
    }

    #[test]
    fn read_positions_are_monotonic_and_clear_marked_unread() {
        let mut state = ConversationChildRuntimeState::new(destination(), "human:1").unwrap();
        state.marked_unread = true;
        assert!(state.advance_inbox_read_till(
            ConversationMessagePosition { created_at_ms: 20, message_id: "m20".into() },
            Some(3),
        ));
        assert!(!state.marked_unread);
        assert!(!state.advance_inbox_read_till(
            ConversationMessagePosition { created_at_ms: 10, message_id: "m10".into() },
            Some(4),
        ));
        assert_eq!(state.unread_count, Some(3));
    }

    #[test]
    fn empty_child_preserves_pin_intent_and_destroy_is_exact() {
        let mut state = ConversationChildRuntimeState::new(destination(), "human:1").unwrap();
        state.pinned = true;
        state.note_locally_empty();
        assert!(!state.pinned);
        assert!(state.restore_pinned_when_non_empty);
        state.note_non_empty();
        assert!(state.pinned);
        assert!(!state.restore_pinned_when_non_empty);
        state.active = true;
        state.no_paid_messages = true;
        state.destroy();
        assert!(!state.active);
        assert!(!state.no_paid_messages);
        assert!(state.pagination.message_ids.is_empty());
    }

    #[test]
    fn topic_and_community_unread_policy_matches_desktop_contract() {
        let topic = ConversationDestination {
            conversation_id: ConversationId("conversation:parent".into()),
            child: Some(ConversationChildIdentity::Topic { root_message_id: "topic:1".into() }),
        };
        assert!(!topic.can_toggle_unread(false, ConversationChildUnreadContext::default()));
        assert!(topic.can_toggle_unread(true, ConversationChildUnreadContext::default()));
        let root = ConversationDestination {
            conversation_id: ConversationId("conversation:community".into()),
            child: None,
        };
        assert!(!root.can_toggle_unread(
            false,
            ConversationChildUnreadContext {
                parent_is_community: true,
                ..ConversationChildUnreadContext::default()
            },
        ));
    }

    #[test]
    fn corrupt_or_duplicate_persisted_state_fails_closed() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-android-child-store-corrupt-{}",
            std::process::id(),
        ));
        let path = root.join("children.json");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(&path, b"{broken").unwrap();
        assert_eq!(
            ConversationChildStore::open(&path).err().unwrap().kind(),
            io::ErrorKind::InvalidData
        );

        let child = ConversationChildRuntimeState::new(destination(), "human:1").unwrap();
        fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({"states":[child.clone(), child]})).unwrap(),
        )
        .unwrap();
        assert_eq!(
            ConversationChildStore::open(&path).err().unwrap().kind(),
            io::ErrorKind::InvalidData
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn durable_store_restores_actor_scoped_state_and_exact_destroy() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-android-child-store-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test"),
        ));
        let path = root.join("children.json");
        let _ = fs::remove_dir_all(&root);
        {
            let mut store = ConversationChildStore::open(&path).unwrap();
            store
                .upsert_with(destination(), "human:1", |state| {
                    state.pinned = true;
                    state.active = true;
                    Ok(())
                })
                .unwrap();
            store
                .upsert_with(destination(), "human:2", |state| {
                    state.marked_unread = true;
                    Ok(())
                })
                .unwrap();
        }
        let mut restored = ConversationChildStore::open(&path).unwrap();
        assert_eq!(restored.states_for_actor("human:1").len(), 1);
        assert_eq!(restored.states_for_actor("human:2").len(), 1);
        assert!(restored.destroy_exact(&destination(), "human:1").unwrap());
        assert!(restored.states_for_actor("human:1").is_empty());
        assert_eq!(restored.states_for_actor("human:2").len(), 1);
        let _ = fs::remove_dir_all(root);
    }
}
