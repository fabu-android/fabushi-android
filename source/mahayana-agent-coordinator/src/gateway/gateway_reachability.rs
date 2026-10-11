#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReachabilityOutcome {
    NoStorage, BoxBlocked, AccessDenied, Refused, Dns, Timeout, Network, Http5xx,
}

pub fn outcome_for_http_status(status: u16) -> Option<ReachabilityOutcome> {
    if status >= 500 { Some(ReachabilityOutcome::Http5xx) }
    else if status == 401 || status == 403 { Some(ReachabilityOutcome::AccessDenied) }
    else { None }
}

pub fn classify_errno(errno: &str) -> ReachabilityOutcome {
    match errno {
        "ECONNREFUSED" => ReachabilityOutcome::Refused,
        "ENOTFOUND" | "EAI_AGAIN" => ReachabilityOutcome::Dns,
        "ETIMEDOUT" | "ETIMEOUT" => ReachabilityOutcome::Timeout,
        _ => ReachabilityOutcome::Network,
    }
}

pub fn classify_base_url(url: &str) -> &'static str {
    let lower=url.to_ascii_lowercase();
    if lower.contains("://127.0.0.1") || lower.contains("://localhost") || lower.contains("://[::1]") { "loopback" }
    else if lower.contains("://") { "remote" } else { "unknown" }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_and_errno_classification() {
        assert_eq!(outcome_for_http_status(503), Some(ReachabilityOutcome::Http5xx));
        assert_eq!(outcome_for_http_status(403), Some(ReachabilityOutcome::AccessDenied));
        assert_eq!(classify_errno("ENOTFOUND"), ReachabilityOutcome::Dns);
    }
}
