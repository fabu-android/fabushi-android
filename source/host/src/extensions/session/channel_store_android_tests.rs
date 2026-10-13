use super::channel_store::{ChannelConnection, FileChannelStore};

#[test]
fn metadata_store_rejects_unsafe_platform_ids_and_round_trips_safe_connection() {
    let root = tempfile::tempdir().unwrap();
    let store = FileChannelStore::new(root.path().join("channels"));

    assert!(!store.write_metadata("../escape", "bad").unwrap());
    assert!(!store.write_metadata("nested/id", "bad").unwrap());
    assert!(!store.remove("../escape").unwrap());

    assert!(store.write_metadata("telegram", "Team Telegram").unwrap());
    assert_eq!(store.read_label("telegram").as_deref(), Some("Team Telegram"));
    assert_eq!(store.list_platforms(), vec!["telegram".to_string()]);
    assert_eq!(
        store.list_connections(),
        vec![ChannelConnection {
            platform: "telegram".into(),
            label: "Team Telegram".into(),
            status: "configured",
        }]
    );

    let raw = std::fs::read_to_string(store.config_path("telegram")).unwrap();
    assert!(raw.ends_with('\n'));
    assert!(!raw.contains("token"));

    assert!(store.remove("telegram").unwrap());
    assert!(store.list_platforms().is_empty());
    assert!(!store.remove("telegram").unwrap());
}
