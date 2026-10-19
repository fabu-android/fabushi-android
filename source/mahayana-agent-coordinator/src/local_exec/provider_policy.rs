pub const SUPERVISED_RANK: i32 = 4;
pub const SAND_LOCAL_EXEC_LIVENESS_WINDOW_MS: u64 = 30_000;
pub const SAND_LOCAL_EXEC_RESPONSE_TIMEOUT_MS: u64 = 10_000;
pub const DEFAULT_SAND_COMPUTER_ID: &str = "this-computer";

pub fn local_exec_provider_rank(supervised: bool, variant: Option<&str>) -> i32 {
    let variant_rank = match variant {
        Some("sand") => 2,
        Some("sand-lab") => 1,
        _ => 0,
    };
    (if supervised { SUPERVISED_RANK } else { 0 }) + variant_rank
}

pub fn bounded_local_exec_variant(variant: Option<&str>) -> Option<&str> {
    match variant {
        Some("sand" | "sand-lab" | "sand-dev") => variant,
        Some(_) => Some("unknown"),
        None => None,
    }
}

pub fn local_exec_provider_is_live(
    has_heartbeat: bool,
    last_seen_at_ms: u64,
    now_ms: u64,
) -> bool {
    !has_heartbeat
        || now_ms.saturating_sub(last_seen_at_ms) <= SAND_LOCAL_EXEC_LIVENESS_WINDOW_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_rank_matches_desktop_supervision_and_variant_order() {
        assert_eq!(local_exec_provider_rank(false, None), 0);
        assert_eq!(local_exec_provider_rank(false, Some("sand-lab")), 1);
        assert_eq!(local_exec_provider_rank(false, Some("sand")), 2);
        assert_eq!(local_exec_provider_rank(true, None), 4);
        assert_eq!(local_exec_provider_rank(true, Some("sand-lab")), 5);
        assert_eq!(local_exec_provider_rank(true, Some("sand")), 6);
    }

    #[test]
    fn variants_are_bounded_before_provider_selection() {
        assert_eq!(bounded_local_exec_variant(None), None);
        assert_eq!(bounded_local_exec_variant(Some("sand")), Some("sand"));
        assert_eq!(bounded_local_exec_variant(Some("sand-lab")), Some("sand-lab"));
        assert_eq!(bounded_local_exec_variant(Some("sand-dev")), Some("sand-dev"));
        assert_eq!(bounded_local_exec_variant(Some("arbitrary")), Some("unknown"));
    }

    #[test]
    fn heartbeat_liveness_matches_desktop_thirty_second_window() {
        assert!(local_exec_provider_is_live(false, 0, u64::MAX));
        assert!(local_exec_provider_is_live(true, 1_000, 31_000));
        assert!(!local_exec_provider_is_live(true, 1_000, 31_001));
    }

    #[test]
    fn response_timeout_matches_desktop_wire_contract() {
        assert_eq!(SAND_LOCAL_EXEC_RESPONSE_TIMEOUT_MS, 10_000);
        assert_eq!(DEFAULT_SAND_COMPUTER_ID, "this-computer");
    }
}
