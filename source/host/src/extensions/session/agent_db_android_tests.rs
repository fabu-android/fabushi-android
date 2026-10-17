use super::agent_db::SandAgentDb;
use crate::storage::store_db::{DB_BUSY_TIMEOUT_MS, live_db_handle_count};

#[test]
fn owner_reopens_durable_transcript_without_duplicate_insert_and_releases_handle() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("agent-a").join("store.db");
    assert_eq!(live_db_handle_count(&path), 0);

    let owner = SandAgentDb::open(&path, DB_BUSY_TIMEOUT_MS).unwrap();
    assert_eq!(live_db_handle_count(&path), 1);
    let entry = serde_json::json!({
        "kind": "message",
        "id": "message-1",
        "content": "hello"
    });
    assert!(owner.append_transcript_entry(&entry).unwrap());
    assert!(!owner.append_transcript_entry(&entry).unwrap());
    assert_eq!(owner.get_transcript_entries().unwrap(), vec![entry.clone()]);
    owner.close(true);
    assert!(owner.is_closed());
    assert_eq!(live_db_handle_count(&path), 0);

    let reopened = SandAgentDb::open(&path, DB_BUSY_TIMEOUT_MS).unwrap();
    assert_eq!(reopened.get_transcript_entries().unwrap(), vec![entry]);
    reopened.close(true);
    assert_eq!(live_db_handle_count(&path), 0);
}

#[test]
fn latest_root_compare_and_set_fences_stale_checkpoint_writer() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("agent-cas").join("store.db");
    let owner = SandAgentDb::open(&path, DB_BUSY_TIMEOUT_MS).unwrap();

    assert_eq!(owner.get_latest_root_blob_id().unwrap(), Vec::<u8>::new());
    assert!(owner
        .compare_and_set_latest_root_blob_id(&[], &[1, 2, 3])
        .unwrap());
    assert!(!owner
        .compare_and_set_latest_root_blob_id(&[], &[9, 9, 9])
        .unwrap());
    assert_eq!(owner.get_latest_root_blob_id().unwrap(), vec![1, 2, 3]);
    owner.close(true);
}

#[test]
fn closed_owner_does_not_accept_new_side_effecting_writes() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("agent-closed").join("store.db");
    let owner = SandAgentDb::open(&path, DB_BUSY_TIMEOUT_MS).unwrap();
    owner.close(false);
    assert!(!owner.write_kv("should-not-write", "value").unwrap());
    assert_eq!(owner.read_kv("should-not-write").unwrap(), None);
}
