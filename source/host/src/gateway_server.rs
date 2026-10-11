use super::gateway_command_error::GatewayCommandErrorKind;

pub const GATEWAY_REQUEST_ID_HEADER: &str = "x-fabushi-request-id";
pub const SSE_HEARTBEAT_MS: u64 = 15_000;
pub const MAX_REQUEST_PAYLOAD_BYTES: usize = 1_048_576;
pub const MAX_BODY_BYTES: usize = 1_048_576;
pub const GZIP_MIN_BYTES: usize = 1_024;

pub fn status_for_command_error(kind: &GatewayCommandErrorKind) -> u16 {
    match kind {
        GatewayCommandErrorKind::BadRequest => 400,
        GatewayCommandErrorKind::Unauthorized => 403,
        GatewayCommandErrorKind::NotFound => 404,
        GatewayCommandErrorKind::Conflict => 409,
        GatewayCommandErrorKind::Busy => 429,
        GatewayCommandErrorKind::Internal => 500,
    }
}

pub fn is_authorized(expected: &str, supplied: Option<&str>) -> bool {
    !expected.is_empty() && supplied.is_some_and(|value| constant_time_eq(expected.as_bytes(), value.as_bytes()))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() { return false; }
    let mut diff = 0u8;
    for (a,b) in left.iter().zip(right) { diff |= a ^ b; }
    diff == 0
}

pub fn parse_subscribed_channels(raw: &str) -> Vec<String> {
    let mut out: Vec<_> = raw.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect();
    out.sort();
    out.dedup();
    out
}
