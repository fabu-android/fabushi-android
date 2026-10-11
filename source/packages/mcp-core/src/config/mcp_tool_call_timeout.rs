#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct McpToolCallTimeout {
    timeout_ms: u64,
}

impl McpToolCallTimeout {
    pub fn new(timeout_ms: u64) -> Self {
        Self { timeout_ms: timeout_ms.clamp(1_000, 300_000) }
    }

    pub fn timeout_ms(&self) -> u64 { self.timeout_ms }

    pub fn deadline_from(&self, start_ms: u64) -> u64 {
        start_ms.saturating_add(self.timeout_ms)
    }
}
