pub const STORE_FILENAME: &str = "store.db";
pub const CONVERSATION_BLOBS_FILENAME: &str = "conversation-blobs.db";
pub const SAND_CONVERSATION_ROOT_SLOT_ID: &[u8] = b"sand-live-conversation-root-v1__";
pub const STALE_ROOT_CLEANUP_VERSION: u32 = 1;
pub const ACTIVE_AGENT_FILENAME: &str = "active-agent.json";
pub const NATIVE_CONVERSATIONS_DIRNAME: &str = ".conversations";
pub const HIDDEN_ENTRY_REPAIR_VERSION: u32 = 1;
pub const LEGACY_GROUP_MEMBERS_DIRNAME: &str = "members";
pub const CONNECTOR_SECRETS_DIRNAME: &str = "connector-secrets";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_session_storage_names_remain_stable() {
        assert_eq!(STORE_FILENAME, "store.db");
        assert_eq!(CONVERSATION_BLOBS_FILENAME, "conversation-blobs.db");
        assert_eq!(SAND_CONVERSATION_ROOT_SLOT_ID, b"sand-live-conversation-root-v1__");
        assert_eq!(STALE_ROOT_CLEANUP_VERSION, 1);
        assert_eq!(ACTIVE_AGENT_FILENAME, "active-agent.json");
        assert_eq!(NATIVE_CONVERSATIONS_DIRNAME, ".conversations");
        assert_eq!(HIDDEN_ENTRY_REPAIR_VERSION, 1);
        assert_eq!(LEGACY_GROUP_MEMBERS_DIRNAME, "members");
        assert_eq!(CONNECTOR_SECRETS_DIRNAME, "connector-secrets");
    }
}
