pub struct ClientPersistenceChannels;

impl ClientPersistenceChannels {
    pub const READ: &'static str = "sand:client-persistence-read";
    pub const WRITE: &'static str = "sand:client-persistence-write";
    pub const REMOVE: &'static str = "sand:client-persistence-remove";
    pub const LIST_KEYS: &'static str = "sand:client-persistence-list-keys";
    pub const MIGRATE: &'static str = "sand:client-persistence-migrate";
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persistence_channels_are_distinct() {
        let values = [
            ClientPersistenceChannels::READ,
            ClientPersistenceChannels::WRITE,
            ClientPersistenceChannels::REMOVE,
            ClientPersistenceChannels::LIST_KEYS,
            ClientPersistenceChannels::MIGRATE,
        ];
        let mut sorted = values.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 5);
    }
}
