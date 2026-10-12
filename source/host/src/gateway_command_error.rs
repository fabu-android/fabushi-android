#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GatewayCommandErrorKind { BadRequest, Unauthorized, NotFound, Conflict, Busy, Internal }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewayCommandError {
    pub kind: GatewayCommandErrorKind,
    pub message: String,
}

pub fn classify_gateway_command_error(message: &str) -> GatewayCommandErrorKind {
    let lower = message.to_ascii_lowercase();
    if lower.contains("unauthorized") || lower.contains("forbidden") { GatewayCommandErrorKind::Unauthorized }
    else if lower.contains("not found") || lower.contains("unknown") { GatewayCommandErrorKind::NotFound }
    else if lower.contains("already") || lower.contains("conflict") { GatewayCommandErrorKind::Conflict }
    else if lower.contains("busy") || lower.contains("in flight") { GatewayCommandErrorKind::Busy }
    else if lower.contains("invalid") || lower.contains("missing") { GatewayCommandErrorKind::BadRequest }
    else { GatewayCommandErrorKind::Internal }
}
