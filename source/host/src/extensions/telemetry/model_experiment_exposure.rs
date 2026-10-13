use std::sync::Mutex;

use crate::extensions::experiments::model_selection::{
    SandModelExperimentArm, SandModelExperimentState,
};

pub const MODEL_EXPERIMENT_EXPOSURE_EVENT: &str = "sand.model_experiment.exposure";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelExperimentExposureDecision {
    pub event_name: &'static str,
    pub arm: &'static str,
}

/// Non-blocking Android adaptation of Desktop's one-shot exposure latch.
///
/// The canonical experiments owner calls this only after assignment state settles. Normal
/// assignments require provider exposure success; only an explicit process override may use
/// Product Analytics as the fallback, matching the Desktop attribution/privacy contract.
pub struct ModelExperimentExposureLatch {
    logged: Mutex<bool>,
}

impl Default for ModelExperimentExposureLatch {
    fn default() -> Self { Self::new() }
}

impl ModelExperimentExposureLatch {
    pub fn new() -> Self { Self { logged: Mutex::new(false) } }

    pub fn note(
        &self,
        state: Option<&SandModelExperimentState>,
        has_process_override: bool,
        provider_exposed: bool,
        analytics_can_record: bool,
    ) -> Option<ModelExperimentExposureDecision> {
        let mut logged = self.logged.lock().unwrap_or_else(|p| p.into_inner());
        if *logged { return None; }
        let state = state.filter(|state| state.active)?;
        if !provider_exposed && (!has_process_override || !analytics_can_record) { return None; }
        *logged = true;
        Some(ModelExperimentExposureDecision {
            event_name: MODEL_EXPERIMENT_EXPOSURE_EVENT,
            arm: match state.arm {
                SandModelExperimentArm::Control => "control",
                SandModelExperimentArm::Treatment => "treatment",
            },
        })
    }

    pub fn is_logged(&self) -> bool {
        *self.logged.lock().unwrap_or_else(|p| p.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(arm: SandModelExperimentArm) -> SandModelExperimentState {
        SandModelExperimentState { active: true, arm }
    }

    #[test]
    fn normal_assignment_requires_provider_exposure_before_logging() {
        let latch = ModelExperimentExposureLatch::new();
        assert_eq!(
            latch.note(Some(&state(SandModelExperimentArm::Treatment)), false, false, true),
            None,
        );
        assert!(!latch.is_logged());
        let decision = latch
            .note(Some(&state(SandModelExperimentArm::Treatment)), false, true, true)
            .unwrap();
        assert_eq!(decision.event_name, MODEL_EXPERIMENT_EXPOSURE_EVENT);
        assert_eq!(decision.arm, "treatment");
        assert!(latch.is_logged());
    }

    #[test]
    fn explicit_override_can_use_analytics_fallback_only_when_recordable() {
        let latch = ModelExperimentExposureLatch::new();
        assert_eq!(
            latch.note(Some(&state(SandModelExperimentArm::Control)), true, false, false),
            None,
        );
        assert_eq!(
            latch
                .note(Some(&state(SandModelExperimentArm::Control)), true, false, true)
                .unwrap()
                .arm,
            "control",
        );
    }

    #[test]
    fn inactive_or_missing_state_does_not_consume_latch() {
        let latch = ModelExperimentExposureLatch::new();
        assert_eq!(latch.note(None, false, true, true), None);
        let inactive = SandModelExperimentState {
            active: false,
            arm: SandModelExperimentArm::Control,
        };
        assert_eq!(latch.note(Some(&inactive), false, true, true), None);
        assert!(!latch.is_logged());
    }

    #[test]
    fn exposure_is_process_lifetime_single_fire() {
        let latch = ModelExperimentExposureLatch::new();
        assert!(latch
            .note(Some(&state(SandModelExperimentArm::Control)), false, true, true)
            .is_some());
        assert_eq!(
            latch.note(Some(&state(SandModelExperimentArm::Treatment)), false, true, true),
            None,
        );
    }
}
