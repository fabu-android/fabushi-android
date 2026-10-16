#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewayConnection {
    pub base_url: String,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostState {
    Disconnected,
    Connecting,
    Healthy(GatewayConnection),
    Unhealthy { reason: String, failures: u32 },
}

pub struct HostSupervisor {
    state: HostState,
    generation: u64,
    max_failures_before_restart: u32,
}

impl HostSupervisor {
    pub fn new(max_failures_before_restart: u32) -> Self {
        Self { state: HostState::Disconnected, generation: 0, max_failures_before_restart: max_failures_before_restart.max(1) }
    }
    pub fn begin_connect(&mut self) { self.state=HostState::Connecting; }
    pub fn connected(&mut self, base_url: impl Into<String>) -> GatewayConnection {
        self.generation=self.generation.saturating_add(1);
        let connection=GatewayConnection { base_url:base_url.into(), generation:self.generation };
        self.state=HostState::Healthy(connection.clone());
        connection
    }
    pub fn health_failure(&mut self, reason: impl Into<String>) -> bool {
        let failures=match &self.state { HostState::Unhealthy { failures, .. } => failures.saturating_add(1), _ => 1 };
        self.state=HostState::Unhealthy { reason:reason.into(), failures };
        failures >= self.max_failures_before_restart
    }
    pub fn disconnected(&mut self) { self.state=HostState::Disconnected; }
    pub fn state(&self) -> &HostState { &self.state }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_failure_requests_restart() {
        let mut supervisor=HostSupervisor::new(2);
        supervisor.begin_connect();
        supervisor.connected("http://127.0.0.1");
        assert!(!supervisor.health_failure("timeout"));
        assert!(supervisor.health_failure("timeout"));
    }
}
