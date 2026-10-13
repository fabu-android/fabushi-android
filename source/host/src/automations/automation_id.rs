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
