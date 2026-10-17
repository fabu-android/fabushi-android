use std::error::Error;
use std::fmt;
use std::io;

const NETWORK_ERRNOS: &[&str] = &[
    "ECONNRESET",
    "EPIPE",
    "ECONNABORTED",
    "EHOSTUNREACH",
    "ENETUNREACH",
    "ENETDOWN",
    "ENETRESET",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GatewayCommandErrorKind {
    BadRequest,
    Unauthorized,
    NotFound,
    Conflict,
    Busy,
    Internal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewayCommandError {
    pub kind: GatewayCommandErrorKind,
    pub message: String,
}

impl GatewayCommandError {
    pub fn status(&self) -> u16 {
        match self.kind {
            GatewayCommandErrorKind::BadRequest => 400,
            GatewayCommandErrorKind::Unauthorized => 403,
            GatewayCommandErrorKind::NotFound => 404,
            GatewayCommandErrorKind::Conflict => 409,
            GatewayCommandErrorKind::Busy => 429,
            GatewayCommandErrorKind::Internal => 500,
        }
    }
}

impl fmt::Display for GatewayCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for GatewayCommandError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewayTransportErrorClassification {
    pub reason: &'static str,
    pub error_class: String,
    pub errno: Option<String>,
}

pub fn classify_gateway_command_error(message: &str) -> GatewayCommandErrorKind {
    let lower = message.to_ascii_lowercase();
    if lower.contains("unauthorized") || lower.contains("forbidden") {
        GatewayCommandErrorKind::Unauthorized
    } else if lower.contains("not found") || lower.contains("unknown") {
        GatewayCommandErrorKind::NotFound
    } else if lower.contains("already") || lower.contains("conflict") {
        GatewayCommandErrorKind::Conflict
    } else if lower.contains("busy") || lower.contains("in flight") {
        GatewayCommandErrorKind::Busy
    } else if lower.contains("invalid") || lower.contains("missing") {
        GatewayCommandErrorKind::BadRequest
    } else {
        GatewayCommandErrorKind::Internal
    }
}

fn known_errno_in_text(text: &str) -> Option<String> {
    const KNOWN: &[&str] = &[
        "ECONNREFUSED",
        "ENOTFOUND",
        "EAI_AGAIN",
        "ETIMEDOUT",
        "ECONNRESET",
        "EPIPE",
        "ECONNABORTED",
        "EHOSTUNREACH",
        "ENETUNREACH",
        "ENETDOWN",
        "ENETRESET",
    ];
    let upper = text.to_ascii_uppercase();
    KNOWN
        .iter()
        .find(|errno| upper.contains(*errno))
        .map(|errno| (*errno).to_string())
}

fn io_errno(error: &io::Error) -> Option<String> {
    match error.kind() {
        io::ErrorKind::ConnectionRefused => Some("ECONNREFUSED".into()),
        io::ErrorKind::TimedOut => Some("ETIMEDOUT".into()),
        io::ErrorKind::ConnectionReset => Some("ECONNRESET".into()),
        io::ErrorKind::BrokenPipe => Some("EPIPE".into()),
        io::ErrorKind::ConnectionAborted => Some("ECONNABORTED".into()),
        _ => known_errno_in_text(&error.to_string()),
    }
}

fn find_system_errno(error: &(dyn Error + 'static)) -> Option<String> {
    let mut current = Some(error);
    while let Some(node) = current {
        if let Some(io_error) = node.downcast_ref::<io::Error>() {
            if let Some(errno) = io_errno(io_error) {
                return Some(errno);
            }
        }
        if let Some(errno) = known_errno_in_text(&node.to_string()) {
            return Some(errno);
        }
        current = node.source();
    }
    None
}

fn has_timeout_semantics(error: &(dyn Error + 'static)) -> bool {
    let mut current = Some(error);
    while let Some(node) = current {
        if node
            .downcast_ref::<io::Error>()
            .is_some_and(|value| value.kind() == io::ErrorKind::TimedOut)
        {
            return true;
        }
        let lower = node.to_string().to_ascii_lowercase();
        if lower.contains("timeout")
            || lower.contains("timed out")
            || lower.contains("deadline exceeded")
        {
            return true;
        }
        current = node.source();
    }
    false
}

fn error_class(error: &(dyn Error + 'static)) -> String {
    if error.downcast_ref::<GatewayCommandError>().is_some() {
        "GatewayCommandError".into()
    } else if error.downcast_ref::<io::Error>().is_some() {
        "io::Error".into()
    } else {
        "Error".into()
    }
}

/// Android replacement for Desktop's gateway error telemetry classifier.
/// Local Desktop Box-daemon error subclasses do not exist on Android; Remote
/// Runner reachability is expressed through the same transport errno/timeout
/// contract without fabricating a local daemon owner.
pub fn classify_gateway_transport_error(
    error: &(dyn Error + 'static),
) -> GatewayTransportErrorClassification {
    let errno = find_system_errno(error);
    let reason = match errno.as_deref() {
        Some("ECONNREFUSED") => "refused",
        Some("ENOTFOUND") | Some("EAI_AGAIN") => "dns",
        Some("ETIMEDOUT") => "timeout",
        Some(errno) if NETWORK_ERRNOS.contains(&errno) => "network",
        _ if has_timeout_semantics(error) => "timeout",
        _ => "application",
    };
    GatewayTransportErrorClassification {
        reason,
        error_class: error_class(error),
        errno,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_classifier_preserves_refused_timeout_and_network_reasons() {
        let refused = io::Error::new(io::ErrorKind::ConnectionRefused, "connect failed");
        assert_eq!(
            classify_gateway_transport_error(&refused),
            GatewayTransportErrorClassification {
                reason: "refused",
                error_class: "io::Error".into(),
                errno: Some("ECONNREFUSED".into()),
            }
        );

        let timeout = io::Error::new(io::ErrorKind::Other, "deadline exceeded");
        assert_eq!(classify_gateway_transport_error(&timeout).reason, "timeout");

        let reset = io::Error::new(io::ErrorKind::ConnectionReset, "peer reset");
        assert_eq!(classify_gateway_transport_error(&reset).reason, "network");
    }

    #[test]
    fn textual_dns_errno_is_not_collapsed_to_internal() {
        let error = io::Error::new(io::ErrorKind::Other, "resolver EAI_AGAIN");
        let classified = classify_gateway_transport_error(&error);
        assert_eq!(classified.reason, "dns");
        assert_eq!(classified.errno.as_deref(), Some("EAI_AGAIN"));
    }

    #[test]
    fn command_error_keeps_android_http_mapping() {
        let error = GatewayCommandError {
            kind: GatewayCommandErrorKind::Conflict,
            message: "already running".into(),
        };
        assert_eq!(error.status(), 409);
        assert_eq!(
            classify_gateway_command_error(&error.message),
            GatewayCommandErrorKind::Conflict
        );
        assert_eq!(
            classify_gateway_transport_error(&error).reason,
            "application"
        );
    }
}
