use std::sync::{Arc, Condvar, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{path::PathBuf, thread};

use super::managed_skills_cache::{ManagedSkill, read_managed_skills_cache, write_managed_skills_cache};
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
}

pub struct SandManagedSkillsService {
    cache_dir: PathBuf,
    fetch: ManagedSkillsFetch,
    report: Option<ManagedSkillsReport>,
    state: Mutex<RefreshState>,
    wake: Condvar,
}

impl SandManagedSkillsService {
    pub fn new(
        cache_dir: PathBuf,
        fetch: ManagedSkillsFetch,
        report: Option<ManagedSkillsReport>,
    ) -> Self {
        Self {
            cache_dir,
            fetch,
            report,
            state: Mutex::new(RefreshState::default()),
            wake: Condvar::new(),
        }
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

    pub fn dispose(&self) {
        let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.disposed = true;
        state.pending_trigger = None;
        self.wake.notify_all();
    }

    pub fn refresh(&self, trigger: ManagedSkillsRefreshTrigger) {
        let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.disposed {
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

            self.run_one_refresh(current_trigger);

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

    fn run_one_refresh(&self, _trigger: ManagedSkillsRefreshTrigger) {
        let result = (self.fetch)().and_then(|fetched| {
            let skills: Vec<ManagedSkill> = fetched
                .iter()
                .filter_map(fetched_managed_skill_to_sand_skill)
                .collect();
            let fetched_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_secs_f64() * 1000.0)
                .unwrap_or_default();
            write_managed_skills_cache(&self.cache_dir, &skills, fetched_at)
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
        read_managed_skills_cache(&self.cache_dir)
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
        assert!(service.ensure_skill("managed"));
        assert!(service.ensure_skill("managed"));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
