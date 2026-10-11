pub const NONCE_DIGEST_MISMATCH: &str = "send/nonce-digest-mismatch";
pub const HOST_ACCOUNT_SLOT: &str = "host";
pub const DISABLE_SEND_ACCEPT_RETURN_ENV: &str = "SAND_DISABLE_SEND_ACCEPT_RETURN";

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protocol_constants_are_stable() {
        assert_eq!(NONCE_DIGEST_MISMATCH, "send/nonce-digest-mismatch");
        assert_eq!(HOST_ACCOUNT_SLOT, "host");
        assert_eq!(DISABLE_SEND_ACCEPT_RETURN_ENV, "SAND_DISABLE_SEND_ACCEPT_RETURN");
    }
}
