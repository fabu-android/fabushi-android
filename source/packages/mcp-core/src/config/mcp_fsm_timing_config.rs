#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct McpFsmTimingConfig {
    pub connect_timeout_ms: u64,
    pub heartbeat_ms: u64,
    pub retry_backoff_ms: u64,
}

impl Default for McpFsmTimingConfig {
    fn default() -> Self {
        Self { connect_timeout_ms: 15_000, heartbeat_ms: 25_000, retry_backoff_ms: 1_000 }
    }
}

impl McpFsmTimingConfig {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.connect_timeout_ms == 0 || self.heartbeat_ms == 0 || self.retry_backoff_ms == 0 {
            return Err("MCP timing values must be non-zero");
        }
        if self.connect_timeout_ms > 120_000 || self.heartbeat_ms > 300_000 || self.retry_backoff_ms > 60_000 {
            return Err("MCP timing value exceeds bound");
        }
        Ok(())
    }
}
