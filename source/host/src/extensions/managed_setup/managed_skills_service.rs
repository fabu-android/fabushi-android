use std::sync::{Arc, Condvar, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{path::PathBuf, thread};

use crate::sha256::sha256_hex;

use super::managed_skills_cache::{
    ManagedSkill, get_managed_skill_file_path, read_managed_skills_cache,
    write_managed_skills_cache,
};
use super::sand_managed_skills::{FetchedManagedSkill, fetched_managed_skill_to_sand_skill};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedSkillsRefreshTrigger {
    Startup,
    AuthChange,
    OnDemand,
}

impl ManagedSkillsRefreshTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Startup => "startup",
            Self::AuthChange => "auth_change",
            Self::OnDemand => "on_demand",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedSkillsDiagnostic {
    pub extension: &'static str,
    pub kind: &'static str,
    pub error_class: String,
}

pub type ManagedSkillsFetch =
    Arc<dyn Fn() -> Result<Vec<FetchedManagedSkill>, String> + Send + Sync + 'static>;
pub type ManagedSkillsReport =
    Arc<dyn Fn(ManagedSkillsDiagnostic) + Send + Sync + 'static>;

#[derive(Default)]
struct RefreshState {
    disposed: bool,
    refreshing: bool,
    pending_trigger: Option<ManagedSkillsRefreshTrigger>,
    account_scope: Option<String>,
    scope_epoch: u64,
}

pub struct SandManagedSkillsService {
    cache_root: PathBuf,
    fetch: ManagedSkillsFetch,
    report: Option<ManagedSkillsReport>,
    state: Mutex<RefreshState>,
    wake: Condvar,
}

impl SandManagedSkillsService {
    pub fn new(
        cache_root: PathBuf,
        fetch: ManagedSkillsFetch,
        report: Option<ManagedSkillsReport>,
    ) -> Self {
        Self {
            cache_root,
            fetch,
            report,
            state: Mutex::new(RefreshState::default()),
            wake: Condvar::new(),
        }
    }

    pub fn set_account_scope(&self, account_scope: Option<&str>) -> Result<(), String> {
        let normalized = account_scope
            .map(str::trim)
            .filter(|scope| !scope.is_empty())
            .map(str::to_string);
        let mut state = self
            .state
            .lock()
            .map_err(|_| "managed skills state lock poisoned".to_string())?;
        if state.account_scope != normalized {
            state.account_scope = normalized;
            state.scope_epoch = state.scope_epoch.saturating_add(1);
            state.pending_trigger = None;
            self.wake.notify_all();
        }
        Ok(())
    }

    pub fn account_scope(&self) -> Option<String> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .account_scope
            .clone()
    }

    fn cache_dir_for_scope(&self, account_scope: &str) -> PathBuf {
        self.cache_root
            .join("accounts")
            .join(sha256_hex(account_scope.as_bytes()))
    }

    fn active_cache_dir(&self) -> Option<PathBuf> {
        self.account_scope()
            .map(|scope| self.cache_dir_for_scope(&scope))
    }

    pub fn start(self: &Arc<Self>) {
        self.spawn_refresh("host-managed-skills-startup", ManagedSkillsRefreshTrigger::Startup);
    }

    pub fn handle_auth_change(self: &Arc<Self>) {
        self.spawn_refresh(
            "host-managed-skills-auth-change",
            ManagedSkillsRefreshTrigger::AuthChange,
        );
    }

    fn spawn_refresh(
        self: &Arc<Self>,
        name: &'static str,
        trigger: ManagedSkillsRefreshTrigger,
    ) {
        let service = Arc::clone(self);
        let _ = thread::Builder::new()
            .name(name.into())
            .spawn(move || service.refresh(trigger));
    }

    pub fn ensure_skill(&self, id: &str) -> bool {
        if self.has_cached_skill(id) {
            return true;
        }
        self.refresh(ManagedSkillsRefreshTrigger::OnDemand);
        self.has_cached_skill(id)
    }

    pub fn cached_skills(&self) -> Vec<ManagedSkill> {
        self.active_cache_dir()
            .and_then(read_managed_skills_cache)
            .map(|cache| cache.skills)
            .unwrap_or_default()
    }

    /// Returns the account-fenced, materialized managed-skill files used by the
    /// root-Agent prompt projection. The service remains the single owner of
    /// account scope and cache layout; callers receive a read-only snapshot.
    pub fn cached_skill_prompt_files(&self) -> Vec<(PathBuf, String)> {
        let Some(cache_dir) = self.active_cache_dir() else {
            return Vec::new();
        };
        let Some(cache) = read_managed_skills_cache(&cache_dir) else {
            return Vec::new();
        };
        let mut entries = cache
            .skills
            .into_iter()
            .map(|skill| {
                (
                    get_managed_skill_file_path(&cache_dir, &skill.id),
                    skill.description,
                )
            })
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        entries
    }

    pub fn dispose(&self) {
        let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.disposed = true;
        state.pending_trigger = None;
        self.wake.notify_all();
    }

    pub fn refresh(&self, trigger: ManagedSkillsRefreshTrigger) {
        let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.disposed || state.account_scope.is_none() {
            return;
        }
        if state.refreshing {
            state.pending_trigger = Some(trigger);
            while state.refreshing && !state.disposed {
                state = self
                    .wake
                    .wait(state)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
            return;
        }
        state.refreshing = true;
        drop(state);

        let mut next_trigger = Some(trigger);
        while let Some(current_trigger) = next_trigger {
            let disposed = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .disposed;
            if disposed {
                break;
            }

            let scope_snapshot = {
                let state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                state.account_scope.clone().map(|scope| (scope, state.scope_epoch))
            };
            if let Some((account_scope, scope_epoch)) = scope_snapshot {
                self.run_one_refresh(current_trigger, &account_scope, scope_epoch);
            }

            let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if state.disposed {
                next_trigger = None;
            } else {
                next_trigger = state.pending_trigger.take();
            }
        }

        let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.refreshing = false;
        self.wake.notify_all();
    }

    fn run_one_refresh(
        &self,
        _trigger: ManagedSkillsRefreshTrigger,
        account_scope: &str,
        scope_epoch: u64,
    ) {
        let result = (self.fetch)().and_then(|fetched| {
            {
                let state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                if state.disposed
                    || state.scope_epoch != scope_epoch
                    || state.account_scope.as_deref() != Some(account_scope)
                {
                    return Err("managed skills account scope changed during refresh".into());
                }
            }
            let skills: Vec<ManagedSkill> = fetched
                .iter()
                .filter_map(fetched_managed_skill_to_sand_skill)
                .collect();
            let fetched_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_secs_f64() * 1000.0)
                .unwrap_or_default();
            write_managed_skills_cache(
                self.cache_dir_for_scope(account_scope),
                &skills,
                fetched_at,
            )
            .map_err(|error| error.to_string())
        });

        if let Err(error) = result {
            if let Some(report) = self.report.as_ref() {
                report(ManagedSkillsDiagnostic {
                    extension: "managed_setup",
                    kind: "managed_skills",
                    error_class: error_class(&error),
                });
            }
        }
    }

    fn has_cached_skill(&self, id: &str) -> bool {
        self.active_cache_dir()
            .and_then(read_managed_skills_cache)
            .is_some_and(|cache| cache.skills.iter().any(|skill| skill.id == id))
    }
}

fn error_class(error: &str) -> String {
    error
        .split_once(':')
        .map(|(head, _)| head.trim())
        .filter(|head| !head.is_empty())
        .unwrap_or("Error")
        .to_string()
}

#[cfg(test)]
mod android_parity_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[test]
    fn ensure_skill_refreshes_once_then_uses_cache() {
        let root = tempfile::tempdir().unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let fetch: ManagedSkillsFetch = Arc::new(move || { seen.fetch_add(1, Ordering::SeqCst); Ok(vec![FetchedManagedSkill { id: "managed".into(), description: "Managed".into(), content: "body".into(), enabled: true }]) });
        let service = SandManagedSkillsService::new(root.path().to_path_buf(), fetch, None);
        service.set_account_scope(Some("account:a")).unwrap();
        assert!(service.ensure_skill("managed"));
        assert!(service.ensure_skill("managed"));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn cached_skills_are_account_fenced() {
        let root = tempfile::tempdir().unwrap();
        let fetch: ManagedSkillsFetch = Arc::new(|| Ok(vec![FetchedManagedSkill {
            id: "managed".into(),
            description: "Managed".into(),
            content: "body".into(),
            enabled: true,
        }]));
        let service = SandManagedSkillsService::new(root.path().to_path_buf(), fetch, None);
        service.set_account_scope(Some("account:a")).unwrap();
        assert!(service.ensure_skill("managed"));
        service.set_account_scope(Some("account:b")).unwrap();
        assert!(!service.has_cached_skill("managed"));
        assert!(service.ensure_skill("managed"));
        service.set_account_scope(Some("account:a")).unwrap();
        assert!(service.has_cached_skill("managed"));
        service.set_account_scope(None).unwrap();
        assert!(!service.has_cached_skill("managed"));
    }
}
