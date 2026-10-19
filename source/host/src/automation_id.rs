use sha2::{Digest, Sha256};

pub fn stable_automation_id(agent_id: &str, local_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(agent_id.as_bytes());
    hasher.update([0]);
    hasher.update(local_id.as_bytes());
    let hex = format!("{:x}", hasher.finalize());
    let variant_value = (u8::from_str_radix(&hex[16..17], 16).expect("sha256 hex") & 3) | 8;
    format!(
        "{}-{}-5{}-{:x}{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[13..16],
        variant_value,
        &hex[17..20],
        &hex[20..32]
    )
}

/// Android keeps automation ids account-isolated even if two accounts reuse the same Agent/local
/// identifiers. The inner UUID derivation is the Desktop canonical algorithm; only its namespace is
/// widened by the Android account fence.
pub fn stable_account_automation_id(account_fence: &str, agent_id: &str, local_id: &str) -> String {
    stable_automation_id(&format!("{account_fence}\0{agent_id}"), local_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_id_is_deterministic_and_namespaced() {
        let first = stable_automation_id("agent-a", "daily-brief");
        assert_eq!(first, stable_automation_id("agent-a", "daily-brief"));
        assert_ne!(first, stable_automation_id("agent-b", "daily-brief"));
        assert_ne!(first, stable_automation_id("agent-a", "weekly-brief"));
        assert_eq!(first.len(), 36);
        assert_eq!(&first[14..15], "5");
    }

    #[test]
    fn android_account_namespace_prevents_cross_account_collisions() {
        let a = stable_account_automation_id("acct:a", "agent", "local");
        let b = stable_account_automation_id("acct:b", "agent", "local");
        assert_ne!(a, b);
        assert_eq!(a, stable_account_automation_id("acct:a", "agent", "local"));
    }
}
