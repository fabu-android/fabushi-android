use super::daemon_files::DaemonDescriptor;

pub const DEFAULT_RESPAWN_LIMIT: u32 = 10;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DaemonAction { Spawn, Adopt(u32), Replace(u32) }

pub fn decide_action(existing: Option<&DaemonDescriptor>) -> DaemonAction {
    match existing {
        None => DaemonAction::Spawn,
        Some(descriptor) if descriptor.inflight_count > 0 => DaemonAction::Adopt(descriptor.pid),
        Some(descriptor) => DaemonAction::Replace(descriptor.pid),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SupervisorState {
    Absent,
    Active(DaemonDescriptor),
    Paused,
    Failed(String),
}

pub struct LocalExecSupervisor {
    state: SupervisorState,
    consecutive_respawns: u32,
    respawn_limit: u32,
}

impl LocalExecSupervisor {
    pub fn new(respawn_limit: u32) -> Self {
        Self { state:SupervisorState::Absent, consecutive_respawns:0, respawn_limit:respawn_limit.max(1) }
    }
    pub fn adopt(&mut self, descriptor: DaemonDescriptor) -> Result<(), &'static str> {
        if !descriptor.validate() { return Err("invalid daemon descriptor"); }
        self.state=SupervisorState::Active(descriptor);
        self.consecutive_respawns=0;
        Ok(())
    }
    pub fn note_exit(&mut self) -> bool {
        self.state=SupervisorState::Absent;
        self.consecutive_respawns=self.consecutive_respawns.saturating_add(1);
        self.consecutive_respawns <= self.respawn_limit
    }
    pub fn pause(&mut self) { self.state=SupervisorState::Paused; }
    pub fn state(&self) -> &SupervisorState { &self.state }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn descriptor(inflight:u32)->DaemonDescriptor { DaemonDescriptor { pid:1,started_at_ms:1,generation_token:"g".into(),entry_identity:"runner".into(),inflight_count:inflight } }
    #[test]
    fn busy_daemon_is_adopted_idle_is_replaced() {
        assert_eq!(decide_action(Some(&descriptor(1))), DaemonAction::Adopt(1));
        assert_eq!(decide_action(Some(&descriptor(0))), DaemonAction::Replace(1));
    }
}
