use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use super::sand_pending_wake_store::{
    DurablePendingWakeMarker, PendingWakeKind, QuietWakeOrigin, SandPendingWakeStore,
};

pub const PENDING_WAKE_STALE_MAX_AGE_MS: u64 = 48 * 60 * 60 * 1_000;

fn system_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingWakeReport {
    pub account_fence: String,
    pub conversation_id: String,
    pub outcome: String,
    pub kind: PendingWakeKind,
    pub work_id: String,
    pub age_ms: Option<u64>,
    pub reason: Option<String>,
    pub is_quiet_origin: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LostSubagentWake {
    pub account_fence: String,
    pub parent_agent_id: String,
    pub subagent_agent_id: String,
    pub subagent_type: String,
    pub title: String,
    pub result: String,
    pub quiet_origin: Option<QuietWakeOrigin>,
}

/// Android adaptation of Desktop pending-wake rearm.
///
/// The Android Host data root can outlive an account session, so every runtime
/// callback is fenced by the persisted account identity. A watcher is never
/// allowed to claim or settle work from another account that reused an Agent id.
pub trait PendingWakeRuntimePort: Send + Sync {
    fn can_execute(&self, account_fence: &str) -> bool;
    fn is_agent_gone(&self, account_fence: &str, agent_id: &str) -> bool;
    fn is_group_session(&self, account_fence: &str, agent_id: &str) -> Result<bool, String>;

    fn cloud_watch_is_armed(
        &self,
        account_fence: &str,
        agent_id: &str,
        work_id: &str,
    ) -> bool;
    fn watch_cloud_agent(
        &self,
        account_fence: &str,
        agent_id: &str,
        work_id: &str,
        quiet_origin: Option<&QuietWakeOrigin>,
    ) -> Result<(), String>;

    fn watch_background_shell(
        &self,
        account_fence: &str,
        agent_id: &str,
        work_id: &str,
        title: Option<&str>,
        quiet_origin: Option<&QuietWakeOrigin>,
    ) -> Result<(), String>;

    fn deliver_recreate_interrupted_shell_notice(
        &self,
        marker: &DurablePendingWakeMarker,
    ) -> Result<(), String>;

    fn revive_lost_subagent(&self, wake: LostSubagentWake) -> Result<(), String>;

    fn emit_async_tasks_for_agent(&self, account_fence: &str, agent_id: &str);
    fn report_pending_wake(&self, report: PendingWakeReport);
}

#[derive(Clone)]
pub struct PendingWakeRearm {
    store: Option<SandPendingWakeStore>,
    runtime: Arc<dyn PendingWakeRuntimePort>,
    now_ms: Arc<dyn Fn() -> u64 + Send + Sync>,
}

impl PendingWakeRearm {
    pub fn new(
        store: Option<SandPendingWakeStore>,
        runtime: Arc<dyn PendingWakeRuntimePort>,
    ) -> Self {
        Self {
            store,
            runtime,
            now_ms: Arc::new(system_now_ms),
        }
    }

    pub fn with_now(mut self, now_ms: Arc<dyn Fn() -> u64 + Send + Sync>) -> Self {
        self.now_ms = now_ms;
        self
    }

    pub fn persist_pending_wake(&self, marker: DurablePendingWakeMarker) -> bool {
        let Some(store) = &self.store else {
            return false;
        };
        if marker.account_fence.trim().is_empty()
            || self
                .runtime
                .is_agent_gone(&marker.account_fence, &marker.agent_id)
        {
            return false;
        }
        let written = store.mark_pending(marker.clone());
        self.report(
            &marker,
            if written { "persisted" } else { "persist_failed" },
            None,
            None,
        );
        written
    }

    pub fn clear_settled_pending_wake(
        &self,
        account_fence: &str,
        agent_id: &str,
        kind: PendingWakeKind,
        work_id: &str,
    ) {
        let Some(store) = &self.store else {
            return;
        };
        if !store.clear_one(account_fence, agent_id, kind, work_id) {
            return;
        }
        let marker = marker_shell(account_fence, agent_id, kind, work_id);
        self.report(&marker, "settled", None, None);
        self.runtime
            .emit_async_tasks_for_agent(account_fence, agent_id);
    }

    pub fn disarm_pending_wake(
        &self,
        account_fence: &str,
        agent_id: &str,
        kind: PendingWakeKind,
        work_id: &str,
    ) {
        let Some(store) = &self.store else {
            return;
        };
        if !store.clear_one(account_fence, agent_id, kind, work_id) {
            return;
        }
        let marker = marker_shell(account_fence, agent_id, kind, work_id);
        self.report(&marker, "settled", Some("aborted"), None);
        self.runtime
            .emit_async_tasks_for_agent(account_fence, agent_id);
    }

    /// Restore typed carry produced by the Android Coordinator/Host boundary.
    /// Cross-account or unfenced carry is rejected instead of being coerced.
    pub fn restore_recreate_carried_pending_wakes(
        &self,
        account_fence: &str,
        carried: &[DurablePendingWakeMarker],
    ) -> usize {
        if account_fence.trim().is_empty()
            || carried.is_empty()
            || !self.runtime.can_execute(account_fence)
        {
            return 0;
        }
        let Some(store) = &self.store else {
            return 0;
        };
        let now_ms = (self.now_ms)();
        let mut restored = 0usize;
        for source in carried {
            if source.account_fence != account_fence
                || !matches!(source.kind, PendingWakeKind::CloudAgent | PendingWakeKind::Shell)
            {
                continue;
            }
            let mut marker = source.clone();
            let age_ms = Some(now_ms.saturating_sub(marker.marked_at_ms));
            if store.has_pending(
                account_fence,
                &marker.agent_id,
                marker.kind,
                &marker.work_id,
            ) {
                self.report(&marker, "rearm_skipped", Some("locally_owned"), age_ms);
                continue;
            }
            if self
                .runtime
                .is_agent_gone(account_fence, &marker.agent_id)
            {
                self.report(&marker, "rearm_skipped", Some("agent_gone"), age_ms);
                continue;
            }
            match self
                .runtime
                .is_group_session(account_fence, &marker.agent_id)
            {
                Ok(true) => {
                    self.report(&marker, "rearm_skipped", Some("group_session"), age_ms);
                    continue;
                }
                Err(_) => {
                    self.report(
                        &marker,
                        "rearm_failed",
                        Some("session_unavailable"),
                        age_ms,
                    );
                    continue;
                }
                Ok(false) => {}
            }
            if marker.kind == PendingWakeKind::Shell {
                marker.interrupted_by_recreate = true;
            }
            if !store.mark_pending(marker.clone()) {
                self.report(&marker, "persist_failed", None, age_ms);
            }
            self.report(&marker, "carried", None, age_ms);
            restored = restored.saturating_add(1);
            match marker.kind {
                PendingWakeKind::CloudAgent => {
                    self.rearm_pending_wake(marker, now_ms, Some("recreate_carry"));
                }
                PendingWakeKind::Shell => {
                    self.report(&marker, "dropped_with_notice", None, age_ms);
                    if self
                        .runtime
                        .deliver_recreate_interrupted_shell_notice(&marker)
                        .is_err()
                    {
                        self.report(&marker, "rearm_failed", Some("error"), age_ms);
                    }
                }
                PendingWakeKind::Subagent => unreachable!("filtered above"),
            }
        }
        restored
    }

    pub fn rearm_pending_wakes(&self, account_fence: &str) {
        if account_fence.trim().is_empty() || !self.runtime.can_execute(account_fence) {
            return;
        }
        let Some(store) = &self.store else {
            return;
        };
        let now = (self.now_ms)();
        for marker in
            store.prune_stale_for_account(account_fence, PENDING_WAKE_STALE_MAX_AGE_MS, now)
        {
            self.report(
                &marker,
                "pruned",
                Some("stale"),
                Some(now.saturating_sub(marker.marked_at_ms)),
            );
        }

        for marker in store.list_pending_for_account(account_fence) {
            if self
                .runtime
                .is_agent_gone(account_fence, &marker.agent_id)
            {
                self.report(
                    &marker,
                    "rearm_skipped",
                    Some("agent_gone"),
                    Some(now.saturating_sub(marker.marked_at_ms)),
                );
                continue;
            }

            if !(marker.kind == PendingWakeKind::Shell && marker.interrupted_by_recreate) {
                store.clear_one(
                    account_fence,
                    &marker.agent_id,
                    marker.kind,
                    &marker.work_id,
                );
            }
            self.rearm_pending_wake(marker, now, None);
        }
    }

    pub fn rearm_pending_wake(
        &self,
        marker: DurablePendingWakeMarker,
        now_ms: u64,
        success_reason: Option<&str>,
    ) {
        let account_fence = marker.account_fence.clone();
        let report = |service: &Self, outcome: &str, reason: Option<&str>| {
            if outcome == "rearm_failed" {
                if let Some(store) = &service.store {
                    store.mark_pending(marker.clone());
                }
            }
            let effective_reason =
                reason.or_else(|| (outcome == "rearmed").then_some(success_reason).flatten());
            service.report(
                &marker,
                outcome,
                effective_reason,
                Some(now_ms.saturating_sub(marker.marked_at_ms)),
            );
        };

        if marker.account_fence.trim().is_empty() {
            report(self, "rearm_failed", Some("account_unfenced"));
            return;
        }

        let group = match self
            .runtime
            .is_group_session(&account_fence, &marker.agent_id)
        {
            Ok(value) => value,
            Err(_) => {
                report(self, "rearm_failed", Some("session_unavailable"));
                return;
            }
        };

        if group {
            if marker.kind == PendingWakeKind::Shell && marker.interrupted_by_recreate {
                if let Some(store) = &self.store {
                    store.clear_one(
                        &account_fence,
                        &marker.agent_id,
                        marker.kind,
                        &marker.work_id,
                    );
                }
            }
            report(self, "rearm_skipped", Some("group_session"));
            return;
        }

        let result = match marker.kind {
            PendingWakeKind::CloudAgent => self.rearm_cloud_agent_wake(&marker),
            PendingWakeKind::Shell if marker.interrupted_by_recreate => self
                .runtime
                .deliver_recreate_interrupted_shell_notice(&marker),
            PendingWakeKind::Shell => self.runtime.watch_background_shell(
                &account_fence,
                &marker.agent_id,
                &marker.work_id,
                marker.title.as_deref(),
                marker.quiet_origin.as_ref(),
            ),
            PendingWakeKind::Subagent => self.revive_parent_for_lost_subagent_wake(&marker),
        };

        match result {
            Ok(()) => report(
                self,
                "rearmed",
                (marker.kind == PendingWakeKind::Subagent)
                    .then_some("interrupted_completion"),
            ),
            Err(_) => report(self, "rearm_failed", Some("error")),
        }
    }

    fn rearm_cloud_agent_wake(
        &self,
        marker: &DurablePendingWakeMarker,
    ) -> Result<(), String> {
        if self.runtime.cloud_watch_is_armed(
            &marker.account_fence,
            &marker.agent_id,
            &marker.work_id,
        ) {
            self.persist_pending_wake(marker.clone());
            return Ok(());
        }

        self.runtime.watch_cloud_agent(
            &marker.account_fence,
            &marker.agent_id,
            &marker.work_id,
            marker.quiet_origin.as_ref(),
        )?;
        if self.runtime.cloud_watch_is_armed(
            &marker.account_fence,
            &marker.agent_id,
            &marker.work_id,
        ) {
            Ok(())
        } else {
            Err("watch_not_armed".into())
        }
    }

    fn revive_parent_for_lost_subagent_wake(
        &self,
        marker: &DurablePendingWakeMarker,
    ) -> Result<(), String> {
        self.persist_pending_wake(marker.clone());
        self.runtime.revive_lost_subagent(LostSubagentWake {
            account_fence: marker.account_fence.clone(),
            parent_agent_id: marker.agent_id.clone(),
            subagent_agent_id: marker.work_id.clone(),
            subagent_type: marker
                .subagent_type
                .clone()
                .unwrap_or_else(|| "task".into()),
            title: marker
                .title
                .clone()
                .unwrap_or_else(|| "Background task".into()),
            result: "A Host restart interrupted this background task before its result could be delivered; its in-process run did not survive, so its final state is unknown. Reconcile the durable task before dispatching replacement work.".into(),
            quiet_origin: marker.quiet_origin.clone(),
        })
    }

    fn report(
        &self,
        marker: &DurablePendingWakeMarker,
        outcome: &str,
        reason: Option<&str>,
        age_ms: Option<u64>,
    ) {
        self.runtime.report_pending_wake(PendingWakeReport {
            account_fence: marker.account_fence.clone(),
            conversation_id: marker.agent_id.clone(),
            outcome: outcome.into(),
            kind: marker.kind,
            work_id: marker.work_id.clone(),
            age_ms,
            reason: reason.map(str::to_string),
            is_quiet_origin: marker.quiet_origin.is_some(),
        });
    }

    pub fn enqueue_pending_wake<T>(
        &self,
        queue: &mut HashMap<String, Vec<T>>,
        account_fence: &str,
        agent_id: &str,
        items: Vec<T>,
    ) -> bool {
        if account_fence.trim().is_empty()
            || self.runtime.is_agent_gone(account_fence, agent_id)
        {
            return false;
        }
        queue.entry(agent_id.to_string()).or_default().extend(items);
        true
    }
}

fn marker_shell(
    account_fence: &str,
    agent_id: &str,
    kind: PendingWakeKind,
    work_id: &str,
) -> DurablePendingWakeMarker {
    DurablePendingWakeMarker {
        account_fence: account_fence.into(),
        agent_id: agent_id.into(),
        kind,
        work_id: work_id.into(),
        marked_at_ms: 0,
        quiet_origin: None,
        title: None,
        subagent_type: None,
        interrupted_by_recreate: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::path::PathBuf;

    #[derive(Default)]
    struct FakeState {
        shell_watches: Vec<(String, String, String)>,
        cloud_watches: Vec<(String, String, String)>,
        armed_cloud: HashSet<(String, String, String)>,
        shell_notices: Vec<(String, String, String)>,
        lost_subagents: Vec<LostSubagentWake>,
        emitted: Vec<(String, String)>,
        reports: Vec<PendingWakeReport>,
        gone: HashSet<(String, String)>,
        groups: HashSet<(String, String)>,
        auto_arm_cloud: bool,
    }

    #[derive(Clone, Default)]
    struct FakeRuntime {
        state: Arc<Mutex<FakeState>>,
    }

    impl PendingWakeRuntimePort for FakeRuntime {
        fn can_execute(&self, _account_fence: &str) -> bool {
            true
        }

        fn is_agent_gone(&self, account_fence: &str, agent_id: &str) -> bool {
            self.state
                .lock()
                .unwrap()
                .gone
                .contains(&(account_fence.into(), agent_id.into()))
        }

        fn is_group_session(
            &self,
            account_fence: &str,
            agent_id: &str,
        ) -> Result<bool, String> {
            Ok(self
                .state
                .lock()
                .unwrap()
                .groups
                .contains(&(account_fence.into(), agent_id.into())))
        }

        fn cloud_watch_is_armed(
            &self,
            account_fence: &str,
            agent_id: &str,
            work_id: &str,
        ) -> bool {
            self.state.lock().unwrap().armed_cloud.contains(&(
                account_fence.into(),
                agent_id.into(),
                work_id.into(),
            ))
        }

        fn watch_cloud_agent(
            &self,
            account_fence: &str,
            agent_id: &str,
            work_id: &str,
            _quiet_origin: Option<&QuietWakeOrigin>,
        ) -> Result<(), String> {
            let mut state = self.state.lock().unwrap();
            let identity = (
                account_fence.to_string(),
                agent_id.to_string(),
                work_id.to_string(),
            );
            state.cloud_watches.push(identity.clone());
            if state.auto_arm_cloud {
                state.armed_cloud.insert(identity);
            }
            Ok(())
        }

        fn watch_background_shell(
            &self,
            account_fence: &str,
            agent_id: &str,
            work_id: &str,
            _title: Option<&str>,
            _quiet_origin: Option<&QuietWakeOrigin>,
        ) -> Result<(), String> {
            self.state.lock().unwrap().shell_watches.push((
                account_fence.into(),
                agent_id.into(),
                work_id.into(),
            ));
            Ok(())
        }

        fn deliver_recreate_interrupted_shell_notice(
            &self,
            marker: &DurablePendingWakeMarker,
        ) -> Result<(), String> {
            self.state.lock().unwrap().shell_notices.push((
                marker.account_fence.clone(),
                marker.agent_id.clone(),
                marker.work_id.clone(),
            ));
            Ok(())
        }

        fn revive_lost_subagent(&self, wake: LostSubagentWake) -> Result<(), String> {
            self.state.lock().unwrap().lost_subagents.push(wake);
            Ok(())
        }

        fn emit_async_tasks_for_agent(&self, account_fence: &str, agent_id: &str) {
            self.state
                .lock()
                .unwrap()
                .emitted
                .push((account_fence.into(), agent_id.into()));
        }

        fn report_pending_wake(&self, report: PendingWakeReport) {
            self.state.lock().unwrap().reports.push(report);
        }
    }

    fn root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-pending-rearm-{name}-{}",
            system_now_ms()
        ))
    }

    fn marker(
        account: &str,
        agent: &str,
        kind: PendingWakeKind,
        work: &str,
        marked_at_ms: u64,
    ) -> DurablePendingWakeMarker {
        DurablePendingWakeMarker {
            account_fence: account.into(),
            agent_id: agent.into(),
            kind,
            work_id: work.into(),
            marked_at_ms,
            quiet_origin: None,
            title: Some(work.into()),
            subagent_type: None,
            interrupted_by_recreate: false,
        }
    }

    #[test]
    fn rearm_is_account_fenced() {
        let root = root("account");
        let store = SandPendingWakeStore::new(&root);
        assert!(store.mark_pending(marker(
            "acct-a",
            "agent-1",
            PendingWakeKind::Shell,
            "shell-a",
            10,
        )));
        assert!(store.mark_pending(marker(
            "acct-b",
            "agent-1",
            PendingWakeKind::Shell,
            "shell-b",
            10,
        )));
        let runtime = FakeRuntime::default();
        let service = PendingWakeRearm::new(Some(store.clone()), Arc::new(runtime.clone()))
            .with_now(Arc::new(|| 100));
        service.rearm_pending_wakes("acct-a");

        assert_eq!(
            runtime.state.lock().unwrap().shell_watches,
            vec![("acct-a".into(), "agent-1".into(), "shell-a".into())]
        );
        assert!(store.list_pending_for("acct-a", "agent-1").is_empty());
        assert_eq!(store.list_pending_for("acct-b", "agent-1").len(), 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn cloud_rearm_fails_closed_until_watch_is_really_armed() {
        let root = root("cloud");
        let store = SandPendingWakeStore::new(&root);
        assert!(store.mark_pending(marker(
            "acct-a",
            "agent-1",
            PendingWakeKind::CloudAgent,
            "cloud-1",
            10,
        )));
        let runtime = FakeRuntime::default();
        let service = PendingWakeRearm::new(Some(store.clone()), Arc::new(runtime.clone()))
            .with_now(Arc::new(|| 100));
        service.rearm_pending_wakes("acct-a");

        assert_eq!(store.list_pending_for("acct-a", "agent-1").len(), 1);
        assert!(runtime
            .state
            .lock()
            .unwrap()
            .reports
            .iter()
            .any(|report| report.outcome == "rearm_failed"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn recreate_carry_marks_shell_interrupted_without_replaying_side_effect() {
        let root = root("carry-shell");
        let store = SandPendingWakeStore::new(&root);
        let runtime = FakeRuntime::default();
        let service = PendingWakeRearm::new(Some(store.clone()), Arc::new(runtime.clone()))
            .with_now(Arc::new(|| 100));
        let carried = vec![marker(
            "acct-a",
            "agent-1",
            PendingWakeKind::Shell,
            "shell-1",
            10,
        )];
        assert_eq!(
            service.restore_recreate_carried_pending_wakes("acct-a", &carried),
            1
        );

        let state = runtime.state.lock().unwrap();
        assert!(state.shell_watches.is_empty());
        assert_eq!(
            state.shell_notices,
            vec![("acct-a".into(), "agent-1".into(), "shell-1".into())]
        );
        drop(state);
        let pending = store.list_pending_for("acct-a", "agent-1");
        assert_eq!(pending.len(), 1);
        assert!(pending[0].interrupted_by_recreate);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn stale_prune_does_not_touch_other_accounts() {
        let root = root("stale");
        let store = SandPendingWakeStore::new(&root);
        assert!(store.mark_pending(marker(
            "acct-a",
            "agent-a",
            PendingWakeKind::Shell,
            "old",
            1,
        )));
        assert!(store.mark_pending(marker(
            "acct-b",
            "agent-b",
            PendingWakeKind::Shell,
            "other",
            1,
        )));
        let runtime = FakeRuntime::default();
        let service = PendingWakeRearm::new(Some(store.clone()), Arc::new(runtime))
            .with_now(Arc::new(|| PENDING_WAKE_STALE_MAX_AGE_MS + 2));
        service.rearm_pending_wakes("acct-a");

        assert!(store.list_pending_for("acct-a", "agent-a").is_empty());
        assert_eq!(store.list_pending_for("acct-b", "agent-b").len(), 1);
        let _ = std::fs::remove_dir_all(root);
    }
}
