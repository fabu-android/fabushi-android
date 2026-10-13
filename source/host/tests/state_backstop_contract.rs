use std::collections::BTreeMap;
use std::fs;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use mahayana_host_runtime::extensions::extension_ids_generated::HostExtensionId;
use mahayana_host_runtime::extensions::state_backstop::extension::{
    STATE_BACKSTOP_DEPENDENCIES, start_state_backstop_extension_with_gate,
    state_backstop_extension_id,
};
use mahayana_host_runtime::extensions::state_backstop::state_backstop_service::{
    SAND_STATE_BACKSTOP_REL_PATH, SandStateBackstop, StateBackstopObjectStore,
    StateBackstopOptions, StateBackstopSnapshotResult,
    is_state_backstop_enabled_value, read_store_db_bytes,
};

#[derive(Default)]
struct MemoryStore {
    values: Mutex<BTreeMap<String, Vec<u8>>>,
    writes: Mutex<Vec<(String, Vec<u8>)>>,
}

impl StateBackstopObjectStore for MemoryStore {
    fn put(&self, path: &str, bytes: &[u8]) -> Result<(), String> {
        self.values
            .lock()
            .expect("values")
            .insert(path.to_string(), bytes.to_vec());
        self.writes
            .lock()
            .expect("writes")
            .push((path.to_string(), bytes.to_vec()));
        Ok(())
    }

    fn get(&self, path: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.values.lock().expect("values").get(path).cloned())
    }
}

fn temp_dir(label: &str) -> std::path::PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "fabushi-state-backstop-{label}-{}-{suffix}",
        std::process::id()
    ))
}

fn service(
    root: std::path::PathBuf,
    store: Arc<MemoryStore>,
    debounce: Duration,
    max_bytes: usize,
) -> Arc<SandStateBackstop> {
    let object_store = Arc::clone(&store);
    let provider = Arc::new(move |_: &str| -> Arc<dyn StateBackstopObjectStore> {
        object_store.clone()
    });
    let resolver = Arc::new(|agent_id: &str| Ok(format!("source-{agent_id}")));
    let read = Arc::new(|path: &std::path::Path| {
        fs::read(path)
            .map(Some)
            .or_else(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    Ok(None)
                } else {
                    Err(error)
                }
            })
            .map_err(|error| error.to_string())
    });
    let mut options = StateBackstopOptions::new(provider, resolver, root, read);
    options.debounce = debounce;
    options.max_snapshot_bytes = max_bytes;
    options.log = Arc::new(|_| {});
    SandStateBackstop::new(options)
}

#[test]
fn frozen_state_backstop_gate_and_extension_contract_are_preserved() {
    for enabled in ["1", "true", " yes ", "TRUE"] {
        assert!(is_state_backstop_enabled_value(Some(enabled)));
    }
    for disabled in ["", "0", "false", "on", "random"] {
        assert!(!is_state_backstop_enabled_value(Some(disabled)));
    }
    assert!(!is_state_backstop_enabled_value(None));
    assert_eq!(state_backstop_extension_id(), HostExtensionId::StateBackstop);
    assert_eq!(
        STATE_BACKSTOP_DEPENDENCIES,
        &[HostExtensionId::BoxStoreSync, HostExtensionId::SourceMap]
    );

    let root = temp_dir("disabled");
    let store = Arc::new(MemoryStore::default());
    let object_store = Arc::clone(&store);
    let provider = Arc::new(move |_: &str| -> Arc<dyn StateBackstopObjectStore> {
        object_store.clone()
    });
    let resolver = Arc::new(|agent_id: &str| Ok(format!("source-{agent_id}")));
    let read = Arc::new(|_: &std::path::Path| Ok(None));
    let options = StateBackstopOptions::new(provider, resolver, root, read);
    let extension = start_state_backstop_extension_with_gate(false, options);
    assert!(!extension.is_enabled());
    assert_eq!(
        extension.snapshot_now("agent-1"),
        StateBackstopSnapshotResult::Skipped {
            reason: "backstop disabled".into()
        }
    );
}

#[test]
fn snapshot_upload_read_size_cap_and_missing_db_match_frozen_semantics() {
    let root = temp_dir("snapshot");
    fs::create_dir_all(root.join("agent-1")).expect("agent dir");
    fs::write(root.join("agent-1/store.db"), b"sqlite-bytes").expect("seed db");
    let store = Arc::new(MemoryStore::default());
    let backstop = service(root.clone(), Arc::clone(&store), Duration::from_millis(5), 64);
    assert_eq!(
        backstop.snapshot_now("agent-1"),
        StateBackstopSnapshotResult::Uploaded { bytes: 12 }
    );
    assert_eq!(
        store
            .values
            .lock()
            .expect("values")
            .get(SAND_STATE_BACKSTOP_REL_PATH)
            .map(Vec::as_slice),
        Some(b"sqlite-bytes".as_slice())
    );
    assert_eq!(
        backstop.read_snapshot("agent-1").expect("snapshot read").as_deref(),
        Some(b"sqlite-bytes".as_slice())
    );
    assert_eq!(
        backstop.snapshot_now("missing"),
        StateBackstopSnapshotResult::Skipped {
            reason: "no store.db".into()
        }
    );

    fs::write(root.join("agent-1/store.db"), vec![0u8; 65]).expect("oversize db");
    assert_eq!(
        backstop.snapshot_now("agent-1"),
        StateBackstopSnapshotResult::Skipped {
            reason: "store.db 65B over 64B cap".into()
        }
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn schedule_snapshot_is_debounced_per_agent_and_dispose_cancels_pending_work() {
    let root = temp_dir("debounce");
    fs::create_dir_all(root.join("agent-1")).expect("agent dir");
    fs::write(root.join("agent-1/store.db"), b"one").expect("seed db");
    let store = Arc::new(MemoryStore::default());
    let backstop = service(root.clone(), Arc::clone(&store), Duration::from_millis(25), 64);
    backstop.schedule_snapshot("agent-1");
    backstop.schedule_snapshot("agent-1");
    backstop.schedule_snapshot("agent-1");
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        let writes = store.writes.lock().expect("writes").len();
        if writes >= 1 {
            assert_eq!(writes, 1);
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "debounced snapshot did not complete before deterministic test deadline"
        );
        thread::sleep(Duration::from_millis(5));
    }

    backstop.schedule_snapshot("agent-1");
    backstop.dispose();
    thread::sleep(Duration::from_millis(50));
    assert_eq!(store.writes.lock().expect("writes").len(), 1);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn read_store_db_bytes_checkpoints_only_existing_files() {
    let root = temp_dir("checkpoint");
    fs::create_dir_all(&root).expect("root");
    let path = root.join("store.db");
    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_missing = Arc::clone(&calls);
    assert_eq!(
        read_store_db_bytes(&path, move |_| {
            *calls_for_missing.lock().expect("calls") += 1;
            Ok(())
        })
        .expect("missing read"),
        None
    );
    assert_eq!(*calls.lock().expect("calls"), 0);

    fs::write(&path, b"db").expect("db");
    let calls_for_existing = Arc::clone(&calls);
    assert_eq!(
        read_store_db_bytes(&path, move |_| {
            *calls_for_existing.lock().expect("calls") += 1;
            Ok(())
        })
        .expect("existing read")
        .as_deref(),
        Some(b"db".as_slice())
    );
    assert_eq!(*calls.lock().expect("calls"), 1);
    let _ = fs::remove_dir_all(root);
}
