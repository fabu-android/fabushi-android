use super::agent_db::{read_persisted_transcript_entries, SandAgentDb};
use super::pending_card_sweeps::{
    expire_pending_auto_review_approval_entries,
    expire_pending_local_tool_permission_ask_entries,
};
use crate::storage::store_db::DB_BUSY_TIMEOUT_MS;
use serde_json::Value;

fn pending_entry(
    id: &str,
    message_type: &str,
    nested_key: &str,
    request_id: &str,
    timestamp_ms: f64,
) -> Value {
    serde_json::json!({
        "kind":"send-message",
        "id":id,
        "timestampMs":timestamp_ms,
        "message":{
            "type":message_type,
            (nested_key):{
                "requestId":request_id,
                "status":"pending"
            }
        }
    })
}

#[test]
fn restart_sweep_expires_only_requested_auto_review_card() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("agent").join("store.db");
    let db = SandAgentDb::open(&path, DB_BUSY_TIMEOUT_MS).unwrap();
    assert!(db.append_transcript_entry(&pending_entry(
        "a1","auto-review-approval","approval","req-1",10.0
    )).unwrap());
    assert!(db.append_transcript_entry(&pending_entry(
        "a2","auto-review-approval","approval","req-2",20.0
    )).unwrap());
    db.close(true);

    assert_eq!(
        expire_pending_auto_review_approval_entries(
            &path, DB_BUSY_TIMEOUT_MS, Some("req-1")
        ).unwrap(),
        vec!["req-1".to_string()]
    );
    let rows = read_persisted_transcript_entries(&path, DB_BUSY_TIMEOUT_MS).unwrap();
    assert_eq!(
        rows.iter().find(|row| row["id"] == "a1").unwrap()
            ["message"]["approval"]["status"],
        "expired"
    );
    assert_eq!(
        rows.iter().find(|row| row["id"] == "a2").unwrap()
            ["message"]["approval"]["status"],
        "pending"
    );
}

#[test]
fn permission_sweep_respects_pending_timestamp_cutoff() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("agent").join("store.db");
    let db = SandAgentDb::open(&path, DB_BUSY_TIMEOUT_MS).unwrap();
    assert!(db.append_transcript_entry(&pending_entry(
        "p1","local-tool-permission","ask","old",10.0
    )).unwrap());
    assert!(db.append_transcript_entry(&pending_entry(
        "p2","local-tool-permission","ask","new",100.0
    )).unwrap());
    db.close(true);

    assert_eq!(
        expire_pending_local_tool_permission_ask_entries(
            &path, DB_BUSY_TIMEOUT_MS, None, Some(50.0)
        ).unwrap(),
        vec!["old".to_string()]
    );
    let rows = read_persisted_transcript_entries(&path, DB_BUSY_TIMEOUT_MS).unwrap();
    assert_eq!(
        rows.iter().find(|row| row["id"] == "p1").unwrap()
            ["message"]["ask"]["status"],
        "expired"
    );
    assert_eq!(
        rows.iter().find(|row| row["id"] == "p2").unwrap()
            ["message"]["ask"]["status"],
        "pending"
    );
}
