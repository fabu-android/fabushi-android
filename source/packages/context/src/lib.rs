pub mod abort_reason;
pub mod browser_bridge;
pub mod core;
pub mod index;
pub mod logger;
pub mod otel;

#[cfg(test)]
mod tests {
    use super::{browser_bridge::BrowserBridgeCommand, core::RequestContext, logger::SafeLogEvent};
    #[test]
    fn context_and_browser_bridge_fail_closed() {
        assert!(RequestContext::new("", "trace").is_err());
        assert!(BrowserBridgeCommand::open("http://example.com").is_err());
        assert!(BrowserBridgeCommand::open("https://example.com/path").is_ok());
        assert_eq!(SafeLogEvent::new("token", "authorization=secret").message, "[redacted]");
    }
}
