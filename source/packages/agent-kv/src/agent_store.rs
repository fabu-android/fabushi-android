use std::collections::BTreeMap;
use crate::reference::BlobRef;

#[derive(Default)]
pub struct AgentStore { entries: BTreeMap<String, BlobRef> }

impl AgentStore {
    pub fn set(&mut self, agent_id: impl Into<String>, reference: BlobRef) -> Result<(), &'static str> {
        let agent_id = agent_id.into();
        if agent_id.trim().is_empty() { return Err("agent_id is required"); }
        self.entries.insert(agent_id, reference);
        Ok(())
    }
    pub fn get(&self, agent_id: &str) -> Option<&BlobRef> { self.entries.get(agent_id) }
}
