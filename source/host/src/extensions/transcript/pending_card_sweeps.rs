use std::io;

use serde_json::Value;

use super::transcript_store::TranscriptStore;

pub fn expire_pending_auto_review_approval_entries(
    store: &mut TranscriptStore,
    only_request_id: Option<&str>,
) -> io::Result<Vec<String>> {
    expire_pending_entries(
        store,
        "auto-review-approval",
        "approval",
        only_request_id,
        None,
    )
}

pub fn expire_pending_local_tool_permission_ask_entries(
    store: &mut TranscriptStore,
    only_request_id: Option<&str>,
    if_pending_before_ms: Option<f64>,
) -> io::Result<Vec<String>> {
    expire_pending_entries(
        store,
        "local-tool-permission",
        "ask",
        only_request_id,
        if_pending_before_ms,
    )
}

fn expire_pending_entries(
    store: &mut TranscriptStore,
    message_type: &str,
    nested_key: &str,
    only_request_id: Option<&str>,
    if_pending_before_ms: Option<f64>,
) -> io::Result<Vec<String>> {
    let entries = store.get_transcript();
    let mut expired = Vec::new();
    for entry in entries {
        let Some(request_id) = pending_request_id(&entry, message_type, nested_key) else {
            continue;
        };
        if only_request_id.is_some_and(|only| only != request_id) {
            continue;
        }
        if if_pending_before_ms.is_some_and(|cutoff| {
            entry
                .get("timestampMs")
                .and_then(Value::as_f64)
                .is_some_and(|timestamp| timestamp >= cutoff)
        }) {
            continue;
        }
        let request_id = request_id.to_string();
        let Some(entry_id) = entry.get("id").and_then(Value::as_str).map(str::to_string) else {
            continue;
        };
        let settled_request_id = request_id.clone();
        if store
            .update_entry(&entry_id, |current| {
                settle_pending_entry(current, message_type, nested_key, &settled_request_id)
                    .unwrap_or_else(|| current.clone())
            })?
            .is_some()
        {
            expired.push(request_id);
        }
    }
    Ok(expired)
}

fn pending_request_id<'a>(
    entry: &'a Value,
    message_type: &str,
    nested_key: &str,
) -> Option<&'a str> {
    if entry.get("kind").and_then(Value::as_str) != Some("send-message") {
        return None;
    }
    let message = entry.get("message")?.as_object()?;
    if message.get("type").and_then(Value::as_str) != Some(message_type) {
        return None;
    }
    let nested = message.get(nested_key)?.as_object()?;
    if nested.get("status").and_then(Value::as_str) != Some("pending") {
        return None;
    }
    nested
        .get("requestId")
        .and_then(Value::as_str)
        .filter(|request_id| !request_id.trim().is_empty())
}

fn settle_pending_entry(
    entry: &Value,
    message_type: &str,
    nested_key: &str,
    request_id: &str,
) -> Option<Value> {
    if pending_request_id(entry, message_type, nested_key)? != request_id {
        return None;
    }
    let mut settled = entry.clone();
    settled
        .get_mut("message")?
        .as_object_mut()?
        .get_mut(nested_key)?
        .as_object_mut()?
        .insert("status".into(), Value::String("expired".into()));
    Some(settled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_path(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join(format!("fabushi-pending-card-{label}-{}-{stamp}", std::process::id()))
            .join("transcript.json")
    }

    #[test]
    fn local_tool_sweep_expires_only_old_matching_pending_cards_and_survives_reopen() {
        let path = temp_path("local-tool");
        let mut store = TranscriptStore::open(&path).unwrap();
        for entry in [
            json!({"id":"old","kind":"send-message","timestampMs":100.0,"message":{"type":"local-tool-permission","ask":{"status":"pending","requestId":"r-old"}}}),
            json!({"id":"new","kind":"send-message","timestampMs":900.0,"message":{"type":"local-tool-permission","ask":{"status":"pending","requestId":"r-new"}}}),
            json!({"id":"settled","kind":"send-message","timestampMs":10.0,"message":{"type":"local-tool-permission","ask":{"status":"allowed","requestId":"r-settled"}}}),
        ] {
            store.append_entry(entry).unwrap();
        }

        let expired =
            expire_pending_local_tool_permission_ask_entries(&mut store, None, Some(500.0)).unwrap();
        assert_eq!(expired, vec!["r-old"]);
        assert_eq!(
            store.entry("old").unwrap()["message"]["ask"]["status"],
            "expired"
        );
        assert_eq!(
            store.entry("new").unwrap()["message"]["ask"]["status"],
            "pending"
        );

        drop(store);
        let reopened = TranscriptStore::open(&path).unwrap();
        assert_eq!(
            reopened.entry("old").unwrap()["message"]["ask"]["status"],
            "expired"
        );
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn auto_review_sweep_is_request_scoped_and_idempotent() {
        let path = temp_path("auto-review");
        let mut store = TranscriptStore::open(&path).unwrap();
        for request_id in ["r-a", "r-b"] {
            store
                .append_entry(json!({
                    "id":format!("entry-{request_id}"),
                    "kind":"send-message",
                    "message":{"type":"auto-review-approval","approval":{"status":"pending","requestId":request_id}},
                }))
                .unwrap();
        }

        assert_eq!(
            expire_pending_auto_review_approval_entries(&mut store, Some("r-a")).unwrap(),
            vec!["r-a"]
        );
        assert!(expire_pending_auto_review_approval_entries(&mut store, Some("r-a"))
            .unwrap()
            .is_empty());
        assert_eq!(
            store.entry("entry-r-b").unwrap()["message"]["approval"]["status"],
            "pending"
        );
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
