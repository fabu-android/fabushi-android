pub const AUTH_WATCH_POLL_INTERVAL_MS: u64 = 5_000;
pub const AUTH_WATCH_TIMEOUT_MS: u64 = 15 * 60 * 1_000;
pub const AUTH_WATCH_POLL_TIMEOUT_MS: u64 = 30_000;

pub fn auth_watch_key(server_id: &str, account_key: &str) -> String {
    format!("{server_id}::{account_key}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_auth_watch_contract_is_preserved() {
        assert_eq!(AUTH_WATCH_POLL_INTERVAL_MS, 5_000);
        assert_eq!(AUTH_WATCH_TIMEOUT_MS, 900_000);
        assert_eq!(AUTH_WATCH_POLL_TIMEOUT_MS, 30_000);
        assert_eq!(auth_watch_key("17", "default"), "17::default");
    }
}
