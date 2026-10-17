use crate::interaction_updates::InteractionUpdate;

const SENSITIVE_MARKERS: [&str; 5] = ["access_token", "authorization", "cookie", "password", "refresh_token"];

pub fn redact_update(update: &InteractionUpdate) -> InteractionUpdate {
    let normalized = update.payload.to_ascii_lowercase();
    let payload = if SENSITIVE_MARKERS.iter().any(|marker| normalized.contains(marker)) {
        "[redacted]".to_string()
    } else {
        update.payload.clone()
    };
    InteractionUpdate { sequence: update.sequence, kind: update.kind, payload }
}
