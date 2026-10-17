use std::sync::Arc;

use crate::extensions::extension_ids_generated::HostExtensionId;

use super::state_backstop_service::{
    SandStateBackstop, StateBackstopOptions, StateBackstopSnapshotResult,
};

pub const STATE_BACKSTOP_DEPENDENCIES: &[HostExtensionId] = &[
    HostExtensionId::BoxStoreSync,
    HostExtensionId::SourceMap,
];

pub fn state_backstop_extension_id() -> HostExtensionId {
    HostExtensionId::StateBackstop
}

pub enum HostStateBackstopExtension {
    Disabled,
    Enabled(Arc<SandStateBackstop>),
}

impl HostStateBackstopExtension {
    pub fn is_enabled(&self) -> bool {
        matches!(self, Self::Enabled(_))
    }

    pub fn schedule_snapshot(&self, agent_id: impl Into<String>) {
        if let Self::Enabled(service) = self {
            service.schedule_snapshot(agent_id);
        }
    }

    pub fn snapshot_now(&self, agent_id: &str) -> StateBackstopSnapshotResult {
        match self {
            Self::Disabled => StateBackstopSnapshotResult::Skipped {
                reason: "backstop disabled".into(),
            },
            Self::Enabled(service) => service.snapshot_now(agent_id),
        }
    }

    pub fn read_snapshot(&self, agent_id: &str) -> Result<Option<Vec<u8>>, String> {
        match self {
            Self::Disabled => Ok(None),
            Self::Enabled(service) => service.read_snapshot(agent_id),
        }
    }

    pub fn stop(&self) {
        if let Self::Enabled(service) = self {
            service.dispose();
        }
    }
}

/// Android production policy is supplied by the typed Host composition. Environment variables are
/// intentionally not consulted here, so shell state cannot silently enable durable remote writes.
pub fn start_state_backstop_extension_with_gate(
    enabled: bool,
    options: StateBackstopOptions,
) -> HostStateBackstopExtension {
    if !enabled {
        return HostStateBackstopExtension::Disabled;
    }
    HostStateBackstopExtension::Enabled(SandStateBackstop::new(options))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_gate_is_fail_closed() {
        let object_store_provider = Arc::new(|_: &str| -> Arc<dyn super::super::state_backstop_service::StateBackstopObjectStore> {
            struct Store;
            impl super::super::state_backstop_service::StateBackstopObjectStore for Store {
                fn put(&self, _: &str, _: &[u8]) -> Result<(), String> { Ok(()) }
                fn get(&self, _: &str) -> Result<Option<Vec<u8>>, String> { Ok(None) }
            }
            Arc::new(Store)
        });
        let options = StateBackstopOptions::new(
            object_store_provider,
            Arc::new(|agent_id| Ok(format!("source-{agent_id}"))),
            std::path::PathBuf::from("/tmp/unused-android-state-backstop-test"),
            Arc::new(|_| Ok(None)),
        );
        let extension = start_state_backstop_extension_with_gate(false, options);
        assert!(!extension.is_enabled());
        assert!(matches!(
            extension.snapshot_now("agent"),
            StateBackstopSnapshotResult::Skipped { .. }
        ));
    }
}
