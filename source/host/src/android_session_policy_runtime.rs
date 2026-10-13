use std::sync::{Arc, Weak};

use crate::extensions::{
    experiments::{HostExperimentsExtension, StopGateSubscription},
    session::{
        conversation_size_limits::{
            pin_conversation_gc, pin_conversation_size_limits_reader, ConversationSizeLimits,
        },
        session_maintenance::{pin_legacy_store_blob_retirement, pin_stale_root_gc},
    },
};

const CONVERSATION_LIMITS_CONFIG: &str = "grok_bot_conversation_size_limits";
const CONVERSATION_GC_GATE: &str = "grok_bot_conversation_gc";
const STALE_ROOT_GC_GATE: &str = "sand_stale_root_gc";
const LEGACY_BLOB_RETIREMENT_GATE: &str = "sand_legacy_store_blob_retirement";

/// Android-adapted composition for the non-destructive parts of Desktop SessionExtension.
///
/// The canonical experiment owner remains HostExperimentsExtension. This binding only projects
/// its current gates/config into the platform-independent session policy primitives; it does not
/// create another session/store owner and deliberately does not guess a Desktop agents-root for
/// destructive maintenance while Android still ships its native transcript repository.
pub struct AndroidSessionPolicyBindings {
    stops: Vec<StopGateSubscription>,
}

impl AndroidSessionPolicyBindings {
    pub fn bind(experiments: Arc<HostExperimentsExtension>) -> Self {
        apply_gate_pins(&experiments);

        let weak: Weak<HostExperimentsExtension> = Arc::downgrade(&experiments);
        pin_conversation_size_limits_reader(Some(Arc::new(move || {
            weak.upgrade()
                .map(|experiments| conversation_limits_from_experiments(&experiments))
                .unwrap_or_default()
        })));

        let mut stops = Vec::with_capacity(3);
        stops.push(experiments.subscribe_gate(
            CONVERSATION_GC_GATE,
            Arc::new(pin_conversation_gc),
        ));
        stops.push(experiments.subscribe_gate(
            STALE_ROOT_GC_GATE,
            Arc::new(pin_stale_root_gc),
        ));
        stops.push(experiments.subscribe_gate(
            LEGACY_BLOB_RETIREMENT_GATE,
            Arc::new(pin_legacy_store_blob_retirement),
        ));
        Self { stops }
    }
}

impl Drop for AndroidSessionPolicyBindings {
    fn drop(&mut self) {
        for stop in self.stops.drain(..) {
            stop();
        }
    }
}

pub fn conversation_limits_from_experiments(
    experiments: &HostExperimentsExtension,
) -> ConversationSizeLimits {
    let config = experiments.dynamic_config(CONVERSATION_LIMITS_CONFIG);
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

    #[test]
    fn dynamic_conversation_limits_follow_canonical_authenticated_experiment_state() {
        let experiments = HostExperimentsExtension::new();
        experiments
            .hydrate_authenticated_statsig(
                &serde_json::json!({
                    "user":{"userID":"user-a"},
                    "dynamic_configs":{
                        CONVERSATION_LIMITS_CONFIG:{
                            "value":{"soft_limit_mb":32.5,"hard_limit_mb":64.0}
                        }
                    }
                })
                .to_string(),
            )
            .unwrap();

        let limits = conversation_limits_from_experiments(&experiments);
        assert_eq!(limits.soft_limit_mb, Some(32.5));
        assert_eq!(limits.hard_limit_mb, Some(64.0));

        experiments.clear_account_scope();
        let fallback = conversation_limits_from_experiments(&experiments);
        assert_eq!(
            fallback,
            conversation_limits_from_experiments(&HostExperimentsExtension::new())
        );
    }
}
