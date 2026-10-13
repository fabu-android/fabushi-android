use std::sync::Arc;

use crate::extensions::experiments::{HostExperimentsExtension, StopGateSubscription};

use super::box_handoff_service::{
    BoxHandoffDeps, BoxHandoffService, HandoffRequest, HandoffStartResult, HandoffTrigger,
    PendingHandoff,
};
use super::conversation_size_limits::{
    ConversationSizeLimits, pin_conversation_gc, pin_conversation_size_limits_reader,
};
use super::production::ProductionSessionWorkers;
use super::session_diagnostics::{SessionDiagnostic, report_session_diagnostic};
use super::session_maintenance::{
    cleanup_legacy_group_member_dirs, default_legacy_member_dirname,
    pin_legacy_store_blob_retirement, pin_stale_root_gc, run_session_maintenance,
};

pub const SESSION_EXTENSION_ID: &str = "session";
pub const SESSION_EXTENSION_DEPENDENCIES: &[&str] =
    &["experiments", "forever-box", "settings", "telemetry"];

const SESSION_EXPERIMENT_GATES: &[&str] = &[
    "sand_stale_root_gc",
    "sand_legacy_store_blob_retirement",
    "grok_bot_conversation_gc",
];

pub struct SessionExtension {
    store: Arc<ProductionSessionWorkers>,
    handoff: BoxHandoffService,
    stop_experiment_subscriptions: Vec<StopGateSubscription>,
}

impl SessionExtension {
    pub fn store(&self) -> Arc<ProductionSessionWorkers> {
        Arc::clone(&self.store)
    }

    pub fn pending_handoff(&self, agent_id: &str) -> Option<PendingHandoff> {
        self.handoff.get(agent_id)
    }

    pub fn start_handoff(&self, request: HandoffRequest) -> HandoffStartResult {
        self.handoff.start(request)
    }

    pub fn end_handoff(
        &self,
        agent_id: &str,
        trigger: HandoffTrigger,
    ) -> Result<bool, String> {
        self.handoff.end(agent_id, trigger)
    }

    pub fn forget_handoff(&self, agent_id: &str) {
        self.handoff.forget(agent_id);
    }

    pub fn handoff_service(&self) -> BoxHandoffService {
        self.handoff.clone()
    }

    pub fn shutdown(&self) {
        self.store.shutdown();
    }
}

impl Drop for SessionExtension {
    fn drop(&mut self) {
        for stop in self.stop_experiment_subscriptions.drain(..) {
            stop();
        }
        self.store.shutdown();
    }
}

/// Android adaptation of Desktop's Session extension bootstrap.
///
/// Desktop subscribes to a generic experiments change bus. Android's canonical experiments owner
/// intentionally exposes gate-scoped subscriptions instead. Hydration/account-clear notifies every
/// registered gate listener, so listening to the three Session maintenance gates preserves the
/// Desktop invariant that all maintenance pins and the dynamic conversation-size reader are
/// refreshed whenever canonical experiment truth changes without introducing a second owner.
pub fn start_session_extension(
    experiments: Arc<HostExperimentsExtension>,
    store: Arc<ProductionSessionWorkers>,
    handoff_deps: BoxHandoffDeps,
) -> SessionExtension {
    let limits_experiments = Arc::clone(&experiments);
    pin_conversation_size_limits_reader(Some(Arc::new(move || {
        conversation_size_limits_from_experiments(&limits_experiments)
    })));
    apply_experiment_pins(&experiments);

    let mut stop_experiment_subscriptions = Vec::with_capacity(SESSION_EXPERIMENT_GATES.len());
    for gate in SESSION_EXPERIMENT_GATES {
        let subscribed_experiments = Arc::clone(&experiments);
        stop_experiment_subscriptions.push(experiments.subscribe_gate(
            *gate,
            Arc::new(move |_| {
                apply_experiment_pins(&subscribed_experiments);
            }),
        ));
    }

    let maintenance_root = store.agents_root().to_path_buf();
    run_session_maintenance(
        vec![Box::new(move || {
            for (agent_id, error_class) in cleanup_legacy_group_member_dirs(
                &maintenance_root,
                default_legacy_member_dirname(),
            ) {
                report_session_diagnostic(&SessionDiagnostic {
                    family: "maintenance".into(),
                    kind: "member_cleanup_failed".into(),
                    metadata: std::collections::BTreeMap::from([
                        ("agentId".into(), serde_json::Value::String(agent_id)),
                        ("errorClass".into(), serde_json::Value::String(error_class)),
                    ]),
                });
            }
            Ok(())
        })],
        |error, index| {
            report_session_diagnostic(&SessionDiagnostic {
                family: "maintenance".into(),
                kind: "startup_task_failed".into(),
                metadata: std::collections::BTreeMap::from([
                    ("index".into(), serde_json::json!(index)),
                    ("errorClass".into(), serde_json::Value::String(error.to_string())),
                ]),
            });
        },
    );

    SessionExtension {
        store,
        handoff: BoxHandoffService::new(handoff_deps),
        stop_experiment_subscriptions,
    }
}

pub fn conversation_size_limits_from_experiments(
    experiments: &HostExperimentsExtension,
) -> ConversationSizeLimits {
    let config = experiments.dynamic_config("grok_bot_conversation_size_limits");
    ConversationSizeLimits {
        soft_limit_mb: config
            .get("soft_limit_mb")
            .and_then(serde_json::Value::as_f64),
        hard_limit_mb: config
            .get("hard_limit_mb")
            .and_then(serde_json::Value::as_f64),
    }
}

fn apply_experiment_pins(experiments: &HostExperimentsExtension) {
    pin_stale_root_gc(experiments.check_feature_gate("sand_stale_root_gc"));
    pin_legacy_store_blob_retirement(
        experiments.check_feature_gate("sand_legacy_store_blob_retirement"),
    );
    pin_conversation_gc(experiments.check_feature_gate("grok_bot_conversation_gc"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::experiments::HostExperimentsOptions;

    #[test]
    fn conversation_size_limits_follow_android_canonical_experiment_config() {
        let experiments = HostExperimentsExtension::with_options(HostExperimentsOptions {
            is_dev_build: false,
            env_gate_overrides: None,
        });
        let defaults = conversation_size_limits_from_experiments(&experiments);
        assert_eq!(defaults.soft_limit_mb, Some(256.0));
        assert_eq!(defaults.hard_limit_mb, Some(1024.0));

        experiments
            .hydrate_authenticated_statsig(
                &serde_json::json!({
                    "user": {"userID": "android-session-extension-test"},
                    "dynamic_configs": {
                        "grok_bot_conversation_size_limits": {
                            "value": {"soft_limit_mb": 128.0, "hard_limit_mb": 512.0}
                        }
                    }
                })
                .to_string(),
            )
            .unwrap();
        let hydrated = conversation_size_limits_from_experiments(&experiments);
        assert_eq!(hydrated.soft_limit_mb, Some(128.0));
        assert_eq!(hydrated.hard_limit_mb, Some(512.0));
    }
}
