use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlledAction {
    pub capability: String,
    pub payload: String,
}

#[derive(Clone, Debug, Default)]
pub struct ControlPolicy {
    allowed: BTreeSet<String>,
}

impl ControlPolicy {
    pub fn allow(mut self, capability: impl Into<String>) -> Self {
        let capability = capability.into();
        if !capability.trim().is_empty() {
            self.allowed.insert(capability);
        }
        self
    }

    pub fn authorize(&self, action: &ControlledAction) -> Result<(), &'static str> {
        if action.capability.trim().is_empty() {
            return Err("capability is required");
        }
        if !self.allowed.contains(&action.capability) {
            return Err("capability is not allowed");
        }
        Ok(())
    }
}
