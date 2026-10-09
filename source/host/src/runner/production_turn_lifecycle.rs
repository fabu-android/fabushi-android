use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum DiskPressureEpisodeState {
    Available,
    Claimed,
    OutcomeUnknown,
    Committed,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct DiskPressureEpisode {
    episode_id: String,
    account_fence: String,
    conversation_id: String,
    claim_id: Option<String>,
    state: DiskPressureEpisodeState,
    updated_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct ProfileAnnouncement {
    account_fence: String,
    agent_id: String,
    revision: String,
    committed_at_ms: u64,
}

#[derive(Default, Deserialize, Serialize)]
struct LifecycleState {
    disk_pressure: BTreeMap<String, DiskPressureEpisode>,
    profile_announcements: BTreeMap<String, ProfileAnnouncement>,
}

/// Android-owned durable lifecycle backing for one-turn claims that cannot be
/// reconstructed from Compose state after process death.
///
/// A claimed disk-pressure reminder is deliberately promoted to
/// outcome_unknown on reopen. The next turn cannot silently claim it again
/// until the prior run is reconciled, avoiding duplicate reminder-side effects
/// when the old provider result may have crossed a process-death boundary.
pub struct ProductionTurnLifecycleStore {
    path: PathBuf,
    state: LifecycleState,
}

impl ProductionTurnLifecycleStore {
    pub fn open(path: impl Into<PathBuf>, now_ms: u64) -> Result<Self, String> {
        let path = path.into();
        let mut state = if path.exists() {
            serde_json::from_slice::<LifecycleState>(
                &fs::read(&path).map_err(|error| error.to_string())?,
            )
            .map_err(|error| format!("invalid production turn lifecycle store: {error}"))?
        } else {
            LifecycleState::default()
        };

        let mut changed = false;
        for episode in state.disk_pressure.values_mut() {
            if episode.state == DiskPressureEpisodeState::Claimed {
                episode.state = DiskPressureEpisodeState::OutcomeUnknown;
                episode.updated_at_ms = now_ms;
                changed = true;
            }
        }

        let store = Self { path, state };
        if changed {
            store.persist()?;
        }
        Ok(store)
    }

    pub fn record_disk_pressure_episode(
        &mut self,
        account_fence: &str,
        conversation_id: &str,
        episode_id: &str,
        now_ms: u64,
    ) -> Result<(), String> {
        validate_identity(account_fence, "account fence")?;
        validate_identity(conversation_id, "conversation id")?;
        validate_identity(episode_id, "disk-pressure episode id")?;
        let key = conversation_key(account_fence, conversation_id);
        if let Some(existing) = self.state.disk_pressure.get(&key) {
            if existing.episode_id == episode_id {
                return Ok(());
            }
            if matches!(
                existing.state,
                DiskPressureEpisodeState::Claimed | DiskPressureEpisodeState::OutcomeUnknown
            ) {
                return Err(
                    "cannot replace an unresolved disk-pressure reminder episode".into(),
                );
            }
        }
        self.state.disk_pressure.insert(
            key,
            DiskPressureEpisode {
                episode_id: episode_id.to_string(),
                account_fence: account_fence.to_string(),
                conversation_id: conversation_id.to_string(),
                claim_id: None,
                state: DiskPressureEpisodeState::Available,
                updated_at_ms: now_ms,
            },
        );
        self.persist()
    }

    pub fn claim_disk_pressure(
        &mut self,
        account_fence: &str,
        conversation_id: &str,
        claim_id: &str,
        now_ms: u64,
    ) -> Result<Option<String>, String> {
        validate_identity(account_fence, "account fence")?;
        validate_identity(conversation_id, "conversation id")?;
        validate_identity(claim_id, "disk-pressure claim id")?;
        let key = conversation_key(account_fence, conversation_id);
        let Some(episode) = self.state.disk_pressure.get_mut(&key) else {
            return Ok(None);
        };
        match episode.state {
            DiskPressureEpisodeState::Available => {
                episode.state = DiskPressureEpisodeState::Claimed;
                episode.claim_id = Some(claim_id.to_string());
                episode.updated_at_ms = now_ms;
                let episode_id = episode.episode_id.clone();
                self.persist()?;
                Ok(Some(episode_id))
            }
            DiskPressureEpisodeState::Claimed
                if episode.claim_id.as_deref() == Some(claim_id) =>
            {
                Ok(Some(episode.episode_id.clone()))
            }
            DiskPressureEpisodeState::Claimed
            | DiskPressureEpisodeState::OutcomeUnknown
            | DiskPressureEpisodeState::Committed => Ok(None),
        }
    }

    pub fn commit_disk_pressure(
        &mut self,
        account_fence: &str,
        conversation_id: &str,
        claim_id: &str,
        now_ms: u64,
    ) -> Result<bool, String> {
        self.transition_claim(
            account_fence,
            conversation_id,
            claim_id,
            DiskPressureEpisodeState::Committed,
            now_ms,
        )
    }

    pub fn release_disk_pressure(
        &mut self,
        account_fence: &str,
        conversation_id: &str,
        claim_id: &str,
        now_ms: u64,
    ) -> Result<bool, String> {
        validate_identity(account_fence, "account fence")?;
        validate_identity(conversation_id, "conversation id")?;
        validate_identity(claim_id, "disk-pressure claim id")?;
        let key = conversation_key(account_fence, conversation_id);
        let Some(episode) = self.state.disk_pressure.get_mut(&key) else {
            return Ok(false);
        };
        if episode.state != DiskPressureEpisodeState::Claimed
            || episode.claim_id.as_deref() != Some(claim_id)
        {
            return Ok(false);
        }
        episode.state = DiskPressureEpisodeState::Available;
        episode.claim_id = None;
        episode.updated_at_ms = now_ms;
        self.persist()?;
        Ok(true)
    }

    pub fn mark_account_outcome_unknown(
        &mut self,
        account_fence: &str,
        now_ms: u64,
    ) -> Result<Vec<String>, String> {
        let mut claims = Vec::new();
        for episode in self.state.disk_pressure.values_mut() {
            if episode.account_fence == account_fence
                && episode.state == DiskPressureEpisodeState::Claimed
            {
                episode.state = DiskPressureEpisodeState::OutcomeUnknown;
                episode.updated_at_ms = now_ms;
                if let Some(claim_id) = episode.claim_id.as_ref() {
                    claims.push(claim_id.clone());
                }
            }
        }
        if !claims.is_empty() {
            self.persist()?;
        }
        Ok(claims)
    }

    pub fn reconcile_disk_pressure_claim(
        &mut self,
        account_fence: &str,
        claim_id: &str,
        completed: bool,
        now_ms: u64,
    ) -> Result<bool, String> {
        validate_identity(account_fence, "account fence")?;
        validate_identity(claim_id, "disk-pressure claim id")?;
        let key = self.state.disk_pressure.iter().find_map(|(key, episode)| {
            (episode.account_fence == account_fence
                && episode.claim_id.as_deref() == Some(claim_id)
                && episode.state == DiskPressureEpisodeState::OutcomeUnknown)
                .then_some(key.clone())
        });
        let Some(key) = key else {
            return Ok(false);
        };
        let episode = self.state.disk_pressure.get_mut(&key).expect("located episode");
        if completed {
            episode.state = DiskPressureEpisodeState::Committed;
        } else {
            episode.state = DiskPressureEpisodeState::Available;
            episode.claim_id = None;
        }
        episode.updated_at_ms = now_ms;
        self.persist()?;
        Ok(true)
    }

    pub fn profile_announcement_needed(
        &self,
        account_fence: &str,
        agent_id: &str,
        revision: &str,
    ) -> bool {
        let key = profile_key(account_fence, agent_id);
        self.state
            .profile_announcements
            .get(&key)
            .is_none_or(|current| current.revision != revision)
    }

    pub fn commit_profile_announcement(
        &mut self,
        account_fence: &str,
        agent_id: &str,
        revision: &str,
        now_ms: u64,
    ) -> Result<(), String> {
        validate_identity(account_fence, "account fence")?;
        validate_identity(agent_id, "agent id")?;
        validate_identity(revision, "profile revision")?;
        self.state.profile_announcements.insert(
            profile_key(account_fence, agent_id),
            ProfileAnnouncement {
                account_fence: account_fence.to_string(),
                agent_id: agent_id.to_string(),
                revision: revision.to_string(),
                committed_at_ms: now_ms,
            },
        );
        self.persist()
    }

    fn transition_claim(
        &mut self,
        account_fence: &str,
        conversation_id: &str,
        claim_id: &str,
        target: DiskPressureEpisodeState,
        now_ms: u64,
    ) -> Result<bool, String> {
        validate_identity(account_fence, "account fence")?;
        validate_identity(conversation_id, "conversation id")?;
        validate_identity(claim_id, "disk-pressure claim id")?;
        let key = conversation_key(account_fence, conversation_id);
        let Some(episode) = self.state.disk_pressure.get_mut(&key) else {
            return Ok(false);
        };
        if episode.state == DiskPressureEpisodeState::Committed
            && target == DiskPressureEpisodeState::Committed
            && episode.claim_id.as_deref() == Some(claim_id)
        {
            return Ok(true);
        }
        if episode.state != DiskPressureEpisodeState::Claimed
            || episode.claim_id.as_deref() != Some(claim_id)
        {
            return Ok(false);
        }
        episode.state = target;
        episode.updated_at_ms = now_ms;
        self.persist()?;
        Ok(true)
    }

    fn persist(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let temporary = self.path.with_extension("json.tmp");
        fs::write(
            &temporary,
            serde_json::to_vec_pretty(&self.state).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        fs::rename(temporary, &self.path).map_err(|error| error.to_string())
    }
}

fn validate_identity(value: &str, label: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        return Err(format!("{label} is invalid"));
    }
    Ok(())
}

fn conversation_key(account_fence: &str, conversation_id: &str) -> String {
    format!("{account_fence}\n{conversation_id}")
}

fn profile_key(account_fence: &str, agent_id: &str) -> String {
    format!("{account_fence}\n{agent_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-turn-lifecycle-{label}-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
        ))
    }

    #[test]
    fn claim_release_commit_and_duplicate_commit_are_durable() {
        let path = path("claim");
        let mut store = ProductionTurnLifecycleStore::open(&path, 1).unwrap();
        store
            .record_disk_pressure_episode("acct:a", "agent-a", "episode-1", 2)
            .unwrap();
        assert_eq!(
            store
                .claim_disk_pressure("acct:a", "agent-a", "op-1", 3)
                .unwrap(),
            Some("episode-1".into())
        );
        assert!(store
            .release_disk_pressure("acct:a", "agent-a", "op-1", 4)
            .unwrap());
        assert_eq!(
            store
                .claim_disk_pressure("acct:a", "agent-a", "op-2", 5)
                .unwrap(),
            Some("episode-1".into())
        );
        assert!(store
            .commit_disk_pressure("acct:a", "agent-a", "op-2", 6)
            .unwrap());
        assert!(store
            .commit_disk_pressure("acct:a", "agent-a", "op-2", 7)
            .unwrap());
        let reopened = ProductionTurnLifecycleStore::open(&path, 8).unwrap();
        assert_eq!(
            reopened.state.disk_pressure.values().next().unwrap().state,
            DiskPressureEpisodeState::Committed
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn reopen_preserves_unknown_claim_until_explicit_reconciliation() {
        let path = path("reopen");
        let mut store = ProductionTurnLifecycleStore::open(&path, 1).unwrap();
        store
            .record_disk_pressure_episode("acct:a", "agent-a", "episode-1", 2)
            .unwrap();
        store
            .claim_disk_pressure("acct:a", "agent-a", "op-1", 3)
            .unwrap();
        drop(store);

        let mut reopened = ProductionTurnLifecycleStore::open(&path, 4).unwrap();
        assert_eq!(
            reopened.state.disk_pressure.values().next().unwrap().state,
            DiskPressureEpisodeState::OutcomeUnknown
        );
        assert_eq!(
            reopened
                .claim_disk_pressure("acct:a", "agent-a", "op-2", 5)
                .unwrap(),
            None
        );
        assert!(reopened
            .reconcile_disk_pressure_claim("acct:a", "op-1", false, 6)
            .unwrap());
        assert_eq!(
            reopened
                .claim_disk_pressure("acct:a", "agent-a", "op-2", 7)
                .unwrap(),
            Some("episode-1".into())
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn account_fence_moves_claim_to_outcome_unknown_and_blocks_cross_account_reuse() {
        let path = path("account");
        let mut store = ProductionTurnLifecycleStore::open(&path, 1).unwrap();
        store
            .record_disk_pressure_episode("acct:a", "agent-a", "episode-1", 2)
            .unwrap();
        store
            .claim_disk_pressure("acct:a", "agent-a", "op-1", 3)
            .unwrap();
        assert_eq!(
            store.mark_account_outcome_unknown("acct:a", 4).unwrap(),
            vec!["op-1".to_string()]
        );
        assert!(!store
            .reconcile_disk_pressure_claim("acct:b", "op-1", true, 5)
            .unwrap());
        assert!(store
            .reconcile_disk_pressure_claim("acct:a", "op-1", true, 6)
            .unwrap());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn profile_announcement_is_revision_and_account_scoped() {
        let path = path("profile");
        let mut store = ProductionTurnLifecycleStore::open(&path, 1).unwrap();
        assert!(store.profile_announcement_needed("acct:a", "agent-a", "r1"));
        store
            .commit_profile_announcement("acct:a", "agent-a", "r1", 2)
            .unwrap();
        assert!(!store.profile_announcement_needed("acct:a", "agent-a", "r1"));
        assert!(store.profile_announcement_needed("acct:a", "agent-a", "r2"));
        assert!(store.profile_announcement_needed("acct:b", "agent-a", "r1"));
        let _ = fs::remove_file(path);
    }
}
