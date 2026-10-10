use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
};

const GIB: u64 = 1024 * 1024 * 1024;
const SOFT_AVAILABLE_BYTES: u64 = 8 * GIB;
const HARD_AVAILABLE_BYTES: u64 = 2 * GIB;
const SOFT_RECOVERY_BYTES: u64 = 10 * GIB;
const HARD_RECOVERY_BYTES: u64 = 3 * GIB;
const SOFT_AVAILABLE_RATIO: f64 = 0.15;
const HARD_AVAILABLE_RATIO: f64 = 0.05;
const SOFT_RECOVERY_RATIO: f64 = 0.20;
const HARD_RECOVERY_RATIO: f64 = 0.08;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProductionDiskPressureLevel {
    Healthy,
    Soft,
    Hard,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct ActiveDiskPressureEpisode {
    account_fence: String,
    episode_id: String,
    level: ProductionDiskPressureLevel,
    updated_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductionDiskPressureObservation {
    pub level: ProductionDiskPressureLevel,
    pub episode_id: Option<String>,
    pub changed: bool,
}

pub fn classify_production_disk_pressure(
    total_bytes: u64,
    available_bytes: u64,
    previous: ProductionDiskPressureLevel,
) -> ProductionDiskPressureLevel {
    if total_bytes == 0 {
        return ProductionDiskPressureLevel::Healthy;
    }
    let ratio = available_bytes as f64 / total_bytes as f64;
    if available_bytes <= HARD_AVAILABLE_BYTES || ratio <= HARD_AVAILABLE_RATIO {
        return ProductionDiskPressureLevel::Hard;
    }
    if previous == ProductionDiskPressureLevel::Hard
        && (available_bytes <= HARD_RECOVERY_BYTES || ratio <= HARD_RECOVERY_RATIO)
    {
        return ProductionDiskPressureLevel::Hard;
    }
    if available_bytes <= SOFT_AVAILABLE_BYTES || ratio <= SOFT_AVAILABLE_RATIO {
        return ProductionDiskPressureLevel::Soft;
    }
    if previous == ProductionDiskPressureLevel::Soft
        && (available_bytes <= SOFT_RECOVERY_BYTES || ratio <= SOFT_RECOVERY_RATIO)
    {
        return ProductionDiskPressureLevel::Soft;
    }
    ProductionDiskPressureLevel::Healthy
}

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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum AwaitingUserState {
    Active,
    RecoveryRequired,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct AwaitingUserRecord {
    account_fence: String,
    agent_id: String,
    request_id: String,
    operation_id: String,
    turn_generation: u64,
    owner_process_epoch: u64,
    reason: String,
    state: AwaitingUserState,
    updated_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductionAwaitingUserProjection {
    pub request_id: String,
    pub operation_id: String,
    pub turn_generation: u64,
    pub reason: String,
    pub recovery_required: bool,
    pub updated_at_ms: u64,
}

#[derive(Default, Deserialize, Serialize)]
struct LifecycleState {
    disk_pressure: BTreeMap<String, DiskPressureEpisode>,
    profile_announcements: BTreeMap<String, ProfileAnnouncement>,
    #[serde(default)]
    active_disk_pressure: BTreeMap<String, ActiveDiskPressureEpisode>,
    #[serde(default)]
    awaiting_user: BTreeMap<String, AwaitingUserRecord>,
    #[serde(default)]
    process_epoch: u64,
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

        state.process_epoch = state.process_epoch.saturating_add(1).max(1);
        let current_process_epoch = state.process_epoch;
        let mut changed = true;
        for waiting in state.awaiting_user.values_mut() {
            if waiting.state == AwaitingUserState::Active {
                waiting.state = AwaitingUserState::RecoveryRequired;
                waiting.owner_process_epoch = current_process_epoch;
                waiting.updated_at_ms = now_ms;
            }
        }
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

    pub fn process_epoch(&self) -> u64 {
        self.state.process_epoch
    }

    pub fn mark_awaiting_user(
        &mut self,
        account_fence: &str,
        agent_id: &str,
        request_id: &str,
        operation_id: &str,
        turn_generation: u64,
        owner_process_epoch: u64,
        reason: &str,
        now_ms: u64,
    ) -> Result<(), String> {
        validate_identity(account_fence, "account fence")?;
        validate_identity(agent_id, "agent id")?;
        validate_identity(request_id, "request id")?;
        validate_identity(operation_id, "operation id")?;
        if turn_generation == 0 || owner_process_epoch != self.state.process_epoch {
            return Err("awaiting-user callback is fenced by turn/process epoch".into());
        }
        let reason = reason.trim();
        if reason.is_empty() {
            return Err("awaiting-user reason is invalid".into());
        }
        let key = conversation_key(account_fence, agent_id);
        if let Some(existing) = self.state.awaiting_user.get(&key) {
            if existing.request_id == request_id
                && existing.operation_id == operation_id
                && existing.turn_generation == turn_generation
                && existing.owner_process_epoch == owner_process_epoch
                && existing.state == AwaitingUserState::Active
            {
                return Ok(());
            }
            return Err("agent already has unresolved awaiting-user state".into());
        }
        self.state.awaiting_user.insert(
            key,
            AwaitingUserRecord {
                account_fence: account_fence.to_string(),
                agent_id: agent_id.to_string(),
                request_id: request_id.to_string(),
                operation_id: operation_id.to_string(),
                turn_generation,
                owner_process_epoch,
                reason: reason.chars().take(500).collect(),
                state: AwaitingUserState::Active,
                updated_at_ms: now_ms,
            },
        );
        self.persist()
    }

    pub fn clear_awaiting_user(
        &mut self,
        account_fence: &str,
        agent_id: &str,
        request_id: &str,
        operation_id: &str,
        turn_generation: u64,
        owner_process_epoch: u64,
    ) -> Result<bool, String> {
        if owner_process_epoch != self.state.process_epoch {
            return Err("stale awaiting-user callback fenced by process epoch".into());
        }
        let key = conversation_key(account_fence, agent_id);
        let Some(existing) = self.state.awaiting_user.get(&key) else {
            return Ok(false);
        };
        if existing.request_id != request_id
            || existing.operation_id != operation_id
            || existing.turn_generation != turn_generation
            || existing.owner_process_epoch != owner_process_epoch
            || existing.state != AwaitingUserState::Active
        {
            return Err("stale awaiting-user callback fenced by account/turn identity".into());
        }
        self.state.awaiting_user.remove(&key);
        self.persist()?;
        Ok(true)
    }

    pub fn reconcile_awaiting_user(
        &mut self,
        account_fence: &str,
        operation_id: &str,
        turn_generation: u64,
    ) -> Result<bool, String> {
        let key = self.state.awaiting_user.iter().find_map(|(key, record)| {
            (record.account_fence == account_fence
                && record.operation_id == operation_id
                && record.turn_generation == turn_generation)
                .then_some(key.clone())
        });
        let Some(key) = key else {
            return Ok(false);
        };
        self.state.awaiting_user.remove(&key);
        self.persist()?;
        Ok(true)
    }

    pub fn awaiting_user_projection(
        &self,
        account_fence: &str,
        agent_id: &str,
    ) -> Option<ProductionAwaitingUserProjection> {
        self.state
            .awaiting_user
            .get(&conversation_key(account_fence, agent_id))
            .map(|record| ProductionAwaitingUserProjection {
                request_id: record.request_id.clone(),
                operation_id: record.operation_id.clone(),
                turn_generation: record.turn_generation,
                reason: record.reason.clone(),
                recovery_required: record.state == AwaitingUserState::RecoveryRequired,
                updated_at_ms: record.updated_at_ms,
            })
    }

    pub fn observe_disk_pressure_sample(
        &mut self,
        account_fence: &str,
        total_bytes: u64,
        available_bytes: u64,
        now_ms: u64,
    ) -> Result<ProductionDiskPressureObservation, String> {
        validate_identity(account_fence, "account fence")?;
        if total_bytes == 0 || available_bytes > total_bytes {
            return Err("disk-pressure sample is invalid".into());
        }
        let previous = self
            .state
            .active_disk_pressure
            .get(account_fence)
            .map(|episode| episode.level)
            .unwrap_or(ProductionDiskPressureLevel::Healthy);
        let level = classify_production_disk_pressure(total_bytes, available_bytes, previous);

        if level == ProductionDiskPressureLevel::Healthy {
            let removed = self.state.active_disk_pressure.remove(account_fence);
            if removed.is_some() {
                self.persist()?;
            }
            return Ok(ProductionDiskPressureObservation {
                level,
                episode_id: None,
                changed: previous != ProductionDiskPressureLevel::Healthy,
            });
        }

        let existing = self.state.active_disk_pressure.get(account_fence).cloned();
        let episode_id = existing
            .as_ref()
            .map(|episode| episode.episode_id.clone())
            .unwrap_or_else(|| {
                let seed = format!(
                    "{account_fence}\n{now_ms}\n{total_bytes}\n{available_bytes}"
                );
                format!(
                    "android-disk-pressure:{}",
                    crate::sha256::sha256_hex(seed.as_bytes())
                )
            });
        let changed = existing
            .as_ref()
            .is_none_or(|episode| episode.level != level);
        if changed {
            self.state.active_disk_pressure.insert(
                account_fence.to_string(),
                ActiveDiskPressureEpisode {
                    account_fence: account_fence.to_string(),
                    episode_id: episode_id.clone(),
                    level,
                    updated_at_ms: now_ms,
                },
            );
            self.persist()?;
        }
        Ok(ProductionDiskPressureObservation {
            level,
            episode_id: Some(episode_id),
            changed,
        })
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
        if let Some(active) = self.state.active_disk_pressure.get(account_fence).cloned() {
            let should_seed = match self.state.disk_pressure.get(&key) {
                None => true,
                Some(existing) => {
                    existing.state == DiskPressureEpisodeState::Committed
                        && existing.episode_id != active.episode_id
                }
            };
            if should_seed {
                self.state.disk_pressure.insert(
                    key.clone(),
                    DiskPressureEpisode {
                        episode_id: active.episode_id,
                        account_fence: account_fence.to_string(),
                        conversation_id: conversation_id.to_string(),
                        claim_id: None,
                        state: DiskPressureEpisodeState::Available,
                        updated_at_ms: now_ms,
                    },
                );
                self.persist()?;
            }
        }
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
        let active_removed = self.state.active_disk_pressure.remove(account_fence).is_some();
        let process_epoch = self.state.process_epoch;
        let mut waiting_changed = false;
        for waiting in self.state.awaiting_user.values_mut() {
            if waiting.account_fence == account_fence
                && waiting.state == AwaitingUserState::Active
            {
                waiting.state = AwaitingUserState::RecoveryRequired;
                waiting.owner_process_epoch = process_epoch;
                waiting.updated_at_ms = now_ms;
                waiting_changed = true;
            }
        }
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
        if active_removed || waiting_changed || !claims.is_empty() {
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
    fn awaiting_user_is_durable_recovery_fenced_and_reconciled() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("turn-lifecycle.json");
        let mut first = ProductionTurnLifecycleStore::open(&path, 1).unwrap();
        let first_epoch = first.process_epoch();
        first
            .mark_awaiting_user(
                "session:a",
                "agent-a",
                "request-a",
                "operation-a",
                7,
                first_epoch,
                "Approval required",
                2,
            )
            .unwrap();
        let active = first
            .awaiting_user_projection("session:a", "agent-a")
            .unwrap();
        assert!(!active.recovery_required);
        assert_eq!(active.reason, "Approval required");
        drop(first);

        let mut reopened = ProductionTurnLifecycleStore::open(&path, 3).unwrap();
        let reopened_epoch = reopened.process_epoch();
        assert!(reopened_epoch > first_epoch);
        let restored = reopened
            .awaiting_user_projection("session:a", "agent-a")
            .unwrap();
        assert!(restored.recovery_required);
        assert!(reopened
            .clear_awaiting_user(
                "session:a",
                "agent-a",
                "request-a",
                "operation-a",
                7,
                first_epoch,
            )
            .unwrap_err()
            .contains("process epoch"));
        assert!(reopened
            .reconcile_awaiting_user("session:a", "operation-a", 7)
            .unwrap());
        assert!(reopened
            .awaiting_user_projection("session:a", "agent-a")
            .is_none());
    }

    #[test]
    fn platform_pressure_thresholds_and_hysteresis_match_desktop_contract() {
        assert_eq!(
            classify_production_disk_pressure(
                100 * GIB,
                14 * GIB,
                ProductionDiskPressureLevel::Healthy,
            ),
            ProductionDiskPressureLevel::Soft
        );
        assert_eq!(
            classify_production_disk_pressure(
                100 * GIB,
                4 * GIB,
                ProductionDiskPressureLevel::Healthy,
            ),
            ProductionDiskPressureLevel::Hard
        );
        assert_eq!(
            classify_production_disk_pressure(
                100 * GIB,
                9 * GIB,
                ProductionDiskPressureLevel::Soft,
            ),
            ProductionDiskPressureLevel::Soft
        );
        assert_eq!(
            classify_production_disk_pressure(
                100 * GIB,
                25 * GIB,
                ProductionDiskPressureLevel::Soft,
            ),
            ProductionDiskPressureLevel::Healthy
        );
        assert_eq!(
            classify_production_disk_pressure(
                100 * GIB,
                7 * GIB,
                ProductionDiskPressureLevel::Hard,
            ),
            ProductionDiskPressureLevel::Hard
        );
        assert_eq!(
            classify_production_disk_pressure(
                100 * GIB,
                9 * GIB,
                ProductionDiskPressureLevel::Hard,
            ),
            ProductionDiskPressureLevel::Soft
        );
    }

    #[test]
    fn platform_pressure_observation_seeds_turn_claim_and_survives_reopen() {
        let path = path("platform-pressure");
        let mut store = ProductionTurnLifecycleStore::open(&path, 1).unwrap();
        let observed = store
            .observe_disk_pressure_sample("acct:a", 100 * GIB, 10 * GIB, 2)
            .unwrap();
        assert_eq!(observed.level, ProductionDiskPressureLevel::Soft);
        let episode_id = observed.episode_id.unwrap();
        assert_eq!(
            store
                .claim_disk_pressure("acct:a", "agent-a", "op-1", 3)
                .unwrap(),
            Some(episode_id.clone())
        );
        drop(store);

        let mut reopened = ProductionTurnLifecycleStore::open(&path, 4).unwrap();
        assert_eq!(
            reopened.state.disk_pressure.values().next().unwrap().state,
            DiskPressureEpisodeState::OutcomeUnknown
        );
        assert_eq!(
            reopened
                .observe_disk_pressure_sample("acct:a", 100 * GIB, 10 * GIB, 5)
                .unwrap()
                .episode_id,
            Some(episode_id)
        );
        assert_eq!(
            reopened
                .claim_disk_pressure("acct:a", "agent-a", "op-2", 6)
                .unwrap(),
            None
        );
        assert!(reopened
            .mark_account_outcome_unknown("acct:a", 7)
            .unwrap()
            .is_empty());
        assert!(!reopened.state.active_disk_pressure.contains_key("acct:a"));
        let _ = fs::remove_file(path);
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
