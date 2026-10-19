pub const GATEWAY_API_PREFIX: &str = "/api";
pub const GATEWAY_EVENTS_PATH: &str = "/events";
pub const GATEWAY_HEALTH_PATH: &str = "/health";
pub const GATEWAY_AUTH_SCHEME: &str = "Bearer";
pub const GATEWAY_SLIM_AVATARS_HEADER: &str = "x-sand-slim-avatars";
pub const GATEWAY_MINT_DEDUPE_HEADER: &str = "x-sand-mint-dedupe";
pub const GATEWAY_TRACEPARENT_HEADER: &str = "traceparent";
pub const GATEWAY_AVATARS_PATH: &str = "/avatars";
pub const GATEWAY_NETWORK_TOKEN_HEADER: &str = "x-anyrun-network-token";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_constants_are_stable() {
        assert_eq!(GATEWAY_API_PREFIX, "/api");
        assert_eq!(GATEWAY_AUTH_SCHEME, "Bearer");
        assert_eq!(GATEWAY_TRACEPARENT_HEADER, "traceparent");
    }
}
