use std::{path::Path, sync::Arc};

use crate::extensions::experiments::{HostExperimentsExtension, StopGateSubscription};

use super::{
    conversation_size_limits::{
        ConversationSizeLimits, pin_conversation_gc, pin_conversation_size_limits_reader,
    },
    session_diagnostics::{SessionDiagnostic, report_session_diagnostic},
    session_maintenance::{
        cleanup_legacy_group_member_dirs, default_legacy_member_dirname,
        pin_legacy_store_blob_retirement, pin_stale_root_gc,
    },
};

const CONVERSATION_SIZE_CONFIG: &str = "grok_bot_conversation_size_limits";
const CONVERSATION_GC_GATE: &str = "grok_bot_conversation_gc";
const STALE_ROOT_GC_GATE: &str = "sand_stale_root_gc";
const LEGACY_BLOB_RETIREMENT_GATE: &str = "sand_legacy_store_blob_retirement";

/// Android-adapted lifecycle half of Desktop SessionExtension.
///
/// This binder intentionally does not own an Agent/session store. AndroidJsonHost still has one
/// canonical production data owner while the remaining Session DB migration is completed. Its
/// responsibility is limited to wiring Host experiment truth into the already-ported session
/// maintenance/GC code and running the idempotent legacy-member cleanup at Host startup.
pub struct AndroidSessionLifecycle {
    stop_gate_subscriptions: Vec<StopGateSubscription>,
}

impl AndroidSessionLifecycle {
    pub fn start(experiments: Arc<HostExperimentsExtension>, agents_root: &Path) -> Self {
        let limits_experiments = Arc::clone(&experiments);
        pin_conversation_size_limits_reader(Some(Arc::new(move || {
            conversation_size_limits_from_experiments(&limits_experiments)
        })));

        apply_gate_pins(&experiments);

        let mut stop_gate_subscriptions = Vec::with_capacity(3);
        for gate in [
            CONVERSATION_GC_GATE,
            STALE_ROOT_GC_GATE,
            LEGACY_BLOB_RETIREMENT_GATE,
        ] {
            let subscribed = Arc::clone(&experiments);
            stop_gate_subscriptions.push(experiments.subscribe_gate(
                gate,
                Arc::new(move |_| apply_gate_pins(&subscribed)),
            ));
        }

        for (agent_id, error_class) in
            cleanup_legacy_group_member_dirs(agents_root, default_legacy_member_dirname())
        {
            report_session_diagnostic(&SessionDiagnostic {
                family: "maintenance".into(),
                kind: "member_cleanup_failed".into(),
                metadata: std::collections::BTreeMap::from([
                    ("agentId".into(), serde_json::Value::String(agent_id)),
                    ("errorClass".into(), serde_json::Value::String(error_class)),
                ]),
            });
        }

        Self {
            stop_gate_subscriptions,
        }
    }
}

impl Drop for AndroidSessionLifecycle {
    fn drop(&mut self) {
        for stop in self.stop_gate_subscriptions.drain(..) {
            stop();
        }
        pin_conversation_size_limits_reader(None);
        pin_conversation_gc(false);
        pin_stale_root_gc(false);
        pin_legacy_store_blob_retirement(false);
    }
}

pub fn conversation_size_limits_from_experiments(
    experiments: &HostExperimentsExtension,
) -> ConversationSizeLimits {
    let config = experiments.dynamic_config(CONVERSATION_SIZE_CONFIG);
    ConversationSizeLimits {
        soft_limit_mb: config.get("soft_limit_mb").and_then(serde_json::Value::as_f64),
        hard_limit_mb: config.get("hard_limit_mb").and_then(serde_json::Value::as_f64),
    }
}

fn apply_gate_pins(experiments: &HostExperimentsExtension) {
    pin_conversation_gc(experiments.check_feature_gate(CONVERSATION_GC_GATE));
    pin_stale_root_gc(experiments.check_feature_gate(STALE_ROOT_GC_GATE));
    pin_legacy_store_blob_retirement(
        experiments.check_feature_gate(LEGACY_BLOB_RETIREMENT_GATE),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::experiments::HostExperimentsOptions;

    #[test]
    fn conversation_limits_use_the_android_experiment_owner() {
        let experiments = HostExperimentsExtension::with_options(HostExperimentsOptions {
            is_dev_build: false,
            env_gate_overrides: None,
        });
        assert_eq!(
            conversation_size_limits_from_experiments(&experiments),
            ConversationSizeLimits {
                soft_limit_mb: Some(256.0),
                hard_limit_mb: Some(1024.0),
            }
        );
    }
}
