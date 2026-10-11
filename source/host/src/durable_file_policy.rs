use std::collections::BTreeSet;

pub const SAND_UPGRADE_RESUME_FILE_NAME: &str = "host-upgrade-resume.json";
pub const SAND_ACK_OBLIGATIONS_FILE_NAME: &str = "ack-obligations.json";
pub const SAND_PENDING_WAKE_FILE_NAME: &str = "host-pending-wakes.json";
pub const SAND_XUSER_TURN_DEDUPE_FILE_NAME: &str = "host-xuser-turn-nonces.json";
pub const SAND_DISK_PRESSURE_REMINDERS_FILE_NAME: &str = "host-disk-pressure-reminders.json";

pub const BOX_STORE_SAND_DATA_EXCLUDED_FILE_NAMES: [&str; 5] = [
    SAND_UPGRADE_RESUME_FILE_NAME,
    SAND_ACK_OBLIGATIONS_FILE_NAME,
    SAND_PENDING_WAKE_FILE_NAME,
    SAND_XUSER_TURN_DEDUPE_FILE_NAME,
    SAND_DISK_PRESSURE_REMINDERS_FILE_NAME,
];

pub fn excluded_from_box_snapshot() -> BTreeSet<&'static str> {
    BOX_STORE_SAND_DATA_EXCLUDED_FILE_NAMES.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durable_names_match_the_host_contract() {
        assert_eq!(SAND_UPGRADE_RESUME_FILE_NAME, "host-upgrade-resume.json");
        assert_eq!(SAND_PENDING_WAKE_FILE_NAME, "host-pending-wakes.json");
        assert_eq!(SAND_XUSER_TURN_DEDUPE_FILE_NAME, "host-xuser-turn-nonces.json");
        assert_eq!(SAND_DISK_PRESSURE_REMINDERS_FILE_NAME, "host-disk-pressure-reminders.json");
        assert_eq!(excluded_from_box_snapshot().len(), 5);
    }
}
