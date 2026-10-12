pub mod config {
    pub mod mcp_focus_retry_cooldown;
    pub mod mcp_fsm_timing_config;
    pub mod mcp_inline_reconnect_cooldown;
    pub mod mcp_tool_call_timeout;
}

#[cfg(test)]
mod tests {
    use super::config::{
        mcp_fsm_timing_config::McpFsmTimingConfig,
        mcp_inline_reconnect_cooldown::ReconnectCooldown,
        mcp_tool_call_timeout::McpToolCallTimeout,
    };

    #[test]
    fn timing_contracts_are_bounded_and_deterministic() {
        assert!(McpFsmTimingConfig::default().validate().is_ok());
        assert!(McpFsmTimingConfig { connect_timeout_ms: 0, heartbeat_ms: 1, retry_backoff_ms: 1 }.validate().is_err());
        let mut cooldown = ReconnectCooldown::new(1_000);
        assert!(cooldown.try_begin(10_000));
        assert!(!cooldown.try_begin(10_500));
        assert!(cooldown.try_begin(11_000));
        assert_eq!(McpToolCallTimeout::new(0).timeout_ms(), 1_000);
    }
}
