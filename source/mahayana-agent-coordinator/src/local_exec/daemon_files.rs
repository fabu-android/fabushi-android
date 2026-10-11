#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DaemonDescriptor {
    pub pid: u32,
    pub started_at_ms: u64,
    pub generation_token: String,
    pub entry_identity: String,
    pub inflight_count: u32,
}

impl DaemonDescriptor {
    pub fn validate(&self) -> bool {
        self.pid > 0 && self.started_at_ms > 0 && !self.generation_token.is_empty() && !self.entry_identity.is_empty()
    }
    pub fn same_generation(&self, other: &Self) -> bool {
        self.pid == other.pid && self.generation_token == other.generation_token && self.entry_identity == other.entry_identity
    }
}

pub fn discovery_name() -> &'static str { "local-exec-daemon.json" }
pub fn connection_name() -> &'static str { "local-exec-connection.json" }
pub fn credential_name() -> &'static str { "local-exec-credential.json" }
