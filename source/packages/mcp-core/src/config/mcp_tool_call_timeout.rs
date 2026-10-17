pub const DEFAULT_MCP_TOOL_CALL_MAX_TOTAL_TIMEOUT_MS: u64 = 60 * 60_000;
pub const LEGACY_MCP_TOOL_CALL_TIMEOUT_MS: u64 = DEFAULT_MCP_TOOL_CALL_MAX_TOTAL_TIMEOUT_MS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct McpToolCallTimeout {
    timeout_ms: u64,
}

impl Default for McpToolCallTimeout {
    fn default() -> Self {
        Self::new(DEFAULT_MCP_TOOL_CALL_MAX_TOTAL_TIMEOUT_MS)
    }
}

impl McpToolCallTimeout {
    pub fn new(timeout_ms: u64) -> Self {
        Self {
            timeout_ms: timeout_ms.clamp(1_000, DEFAULT_MCP_TOOL_CALL_MAX_TOTAL_TIMEOUT_MS),
        }
    }

    pub fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }

    pub fn deadline_from(&self, start_ms: u64) -> u64 {
        start_ms.saturating_add(self.timeout_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_default_and_legacy_timeouts_are_one_hour() {
        assert_eq!(DEFAULT_MCP_TOOL_CALL_MAX_TOTAL_TIMEOUT_MS, 60 * 60_000);
        assert_eq!(
            LEGACY_MCP_TOOL_CALL_TIMEOUT_MS,
            DEFAULT_MCP_TOOL_CALL_MAX_TOTAL_TIMEOUT_MS
        );
        assert_eq!(
            McpToolCallTimeout::default().timeout_ms(),
            DEFAULT_MCP_TOOL_CALL_MAX_TOTAL_TIMEOUT_MS
        );
        assert_eq!(
            McpToolCallTimeout::new(u64::MAX).timeout_ms(),
            DEFAULT_MCP_TOOL_CALL_MAX_TOTAL_TIMEOUT_MS
        );
        assert_eq!(McpToolCallTimeout::new(0).timeout_ms(), 1_000);
    }
}
