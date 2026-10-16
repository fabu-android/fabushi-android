use std::sync::Arc;

use super::trays_service::{
    ErrorTray, PushErrorOptions, TrayEvent, TrayManager, TraySubscription,
};

#[derive(Clone, Default)]
pub struct HostTraysExtension {
    trays: TrayManager,
}

impl HostTraysExtension {
    pub fn list(&self) -> Vec<ErrorTray> {
        self.trays.get_trays()
    }

    pub fn dismiss(&self, id: &str) -> bool {
        self.trays.dismiss(id)
    }

    pub fn clear_all(&self) {
        self.trays.clear_all();
    }

    pub fn push_error(&self, options: PushErrorOptions) -> ErrorTray {
        self.trays.push_error(options)
    }

    pub fn clear_for_agent(&self, agent_id: &str) {
        self.trays.clear_for_agent(agent_id);
    }

    pub fn subscribe(
        &self,
        listener: Arc<dyn Fn(TrayEvent) + Send + Sync>,
    ) -> TraySubscription {
        self.trays.subscribe(listener)
    }
}

pub fn start_trays_extension() -> HostTraysExtension {
    HostTraysExtension::default()
}
