#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewayServerConfig {
    pub host: String,
    pub port: u16,
    pub tls: bool,
}

pub fn is_loopback_host(host: &str) -> bool {
    matches!(host.trim_matches(&['[', ']'][..]), "127.0.0.1" | "::1") || host.eq_ignore_ascii_case("localhost")
}

pub fn resolve_gateway_server_config(host: &str, port: u16, allow_remote: bool, tls: bool) -> Result<GatewayServerConfig, &'static str> {
    if port == 0 { return Err("gateway port must be nonzero"); }
    if !allow_remote && !is_loopback_host(host) { return Err("non-loopback gateway binding is disabled"); }
    Ok(GatewayServerConfig { host: host.to_string(), port, tls })
}

pub fn gateway_scheme(config: &GatewayServerConfig) -> &'static str { if config.tls { "https" } else { "http" } }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remote_binding_is_fail_closed() {
        assert!(resolve_gateway_server_config("0.0.0.0", 8080, false, false).is_err());
        assert!(resolve_gateway_server_config("127.0.0.1", 8080, false, false).is_ok());
    }
}
