use std::sync::{Arc, Weak};

use crate::extensions::{
    experiments::{HostExperimentsExtension, StopGateSubscription},
    session::{
        box_handoff_service::{
            BoxHandoffDeps, BoxHandoffService, HandoffRequest, HandoffStartResult, HandoffTrigger,
            PendingHandoff,
        },
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
    handoff: BoxHandoffService,
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
        Self {
            handoff: BoxHandoffService::new(BoxHandoffDeps::default()),
            stops,
        }
    }

    pub fn pending_handoff(&self, agent_id: &str) -> Option<PendingHandoff> {
        self.handoff.get(agent_id)
    }

    pub fn start_handoff(&self, request: HandoffRequest) -> HandoffStartResult {
        self.handoff.start(request)
    }

    pub fn end_handoff(&self, agent_id: &str, trigger: HandoffTrigger) -> Result<bool, String> {
        self.handoff.end(agent_id, trigger)
    }

    pub fn forget_handoff(&self, agent_id: &str) {
        self.handoff.forget(agent_id);
    }
}

impl Drop for AndroidSessionPolicyBindings {
    fn drop(&mut self) {
        self.handoff.clear_all();
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
    fn shipping_binding_owns_one_handoff_registry_and_clears_terminal_state() {
        let bindings = AndroidSessionPolicyBindings::bind(Arc::new(HostExperimentsExtension::new()));
        let request = HandoffRequest {
            agent_id: "agent-a".into(),
            instruction: "Take over the remote step".into(),
            telemetry: Default::default(),
        };
        let started = bindings.start_handoff(request.clone());
        let request_id = match started {
            HandoffStartResult::Started { request_id } => request_id,
            other => panic!("unexpected first handoff result: {other:?}"),
        };
        assert_eq!(
            bindings.pending_handoff("agent-a").as_ref().map(|value| value.request_id.as_str()),
            Some(request_id.as_str()),
        );
        assert!(matches!(
            bindings.start_handoff(request),
            HandoffStartResult::AlreadyPending { .. }
        ));
        assert!(bindings
            .end_handoff("agent-a", HandoffTrigger::Name("cancel".into()))
            .unwrap());
        assert!(bindings.pending_handoff("agent-a").is_none());
        assert!(!bindings
            .end_handoff("agent-a", HandoffTrigger::Name("cancel".into()))
            .unwrap());
    }

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
