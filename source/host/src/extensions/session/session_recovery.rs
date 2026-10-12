use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fmt,
    fs,
    io,
    path::{Path, PathBuf},
};

pub const SAND_PROFILE_FILENAME: &str = "profile.json";
pub const SAND_SETTINGS_FILENAME: &str = "settings.json";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConversationRecoveryScanError {
    pub detail: String,
}

impl ConversationRecoveryScanError {
    pub fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl fmt::Display for ConversationRecoveryScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "conversation recovery scan failed: {}", self.detail)
    }
}

impl std::error::Error for ConversationRecoveryScanError {}

pub fn transcript_entry_matches_recovered(a: &Value, b: &Value) -> bool {
    let kind_a = a.get("kind").and_then(Value::as_str);
    let kind_b = b.get("kind").and_then(Value::as_str);
    if kind_a != kind_b {
        return false;
    }

    match kind_a {
        Some("message") => {
            a.get("role") == b.get("role") && a.get("content") == b.get("content")
        }
        Some("send-message") => a.get("message") == b.get("message"),
        Some("tool-call") => {
            a.get("name") == b.get("name")
                && a.get("status") == b.get("status")
                && a.get("summary") == b.get("summary")
        }
        _ => false,
    }
}

pub trait BlobStore<T> {
    type Error: Clone;

    fn get_blob(&mut self, context: &str, id: &[u8]) -> Result<Option<T>, Self::Error>;
    fn set_blob(
        &mut self,
        context: &str,
        id: &[u8],
        data: T,
    ) -> Result<(), Self::Error>;
    fn flush(&mut self, context: &str) -> Result<(), Self::Error>;
}

pub struct CachedBlobReads<S, T>
where
    S: BlobStore<T>,
    S::Error: Clone,
    T: Clone,
{
    inner: S,
    reads: HashMap<Vec<u8>, Result<Option<T>, S::Error>>,
}

impl<S, T> CachedBlobReads<S, T>
where
    S: BlobStore<T>,
    S::Error: Clone,
    T: Clone,
{
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            reads: HashMap::new(),
        }
    }

    pub fn get_blob(
        &mut self,
        context: &str,
        id: &[u8],
    ) -> Result<Option<T>, S::Error> {
        if let Some(cached) = self.reads.get(id) {
            return cached.clone();
        }
        let result = self.inner.get_blob(context, id);
        self.reads.insert(id.to_vec(), result.clone());
        result
    }

    pub fn set_blob(
        &mut self,
        context: &str,
        id: &[u8],
        data: T,
    ) -> Result<(), S::Error> {
        self.reads.insert(id.to_vec(), Ok(Some(data.clone())));
        self.inner.set_blob(context, id, data)
    }

    pub fn flush(&mut self, context: &str) -> Result<(), S::Error> {
        self.inner.flush(context)
    }

    pub fn into_inner(self) -> S {
        self.inner
    }
}

pub trait RecoveryProfileSource {
    fn name(&self) -> Option<String>;
    fn description(&self) -> Option<String>;
}

fn atomic_write_json(path: &Path, value: &Value) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("json")
    ));
    let serialized = serde_json::to_string_pretty(value)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    fs::write(&temporary, format!("{serialized}\n"))?;
    fs::rename(temporary, path)
}

fn agent_directory_from_db_path(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn ensure_profile_file(
    db_path: &Path,
    source: &impl RecoveryProfileSource,
) -> io::Result<PathBuf> {
    let path = agent_directory_from_db_path(db_path).join(SAND_PROFILE_FILENAME);
    if path.exists() {
        return Ok(path);
    }

    let name = source
        .name()
        .unwrap_or_else(|| "Grok".to_string())
        .trim()
        .to_string();
    let name = if name.is_empty() {
        "Grok".to_string()
    } else {
        name
    };
    let description = source
        .description()
        .unwrap_or_default()
        .trim()
        .to_string();

    atomic_write_json(
        &path,
        &json!({
            "name": name,
            "description": description,
            "title": "",
            "avatarShape": "",
            "avatarColor": ""
        }),
    )?;
    Ok(path)
}

pub fn ensure_settings_file(db_path: &Path) -> io::Result<PathBuf> {
    let path = agent_directory_from_db_path(db_path).join(SAND_SETTINGS_FILENAME);
    if !path.exists() {
        atomic_write_json(&path, &json!({}))?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn recovered_entry_matching_uses_behavioral_fields_not_rebuilt_ids() {
        let a = json!({
            "kind": "message",
            "id": "recovered-a",
            "role": "user",
            "content": "hello"
        });
        let b = json!({
            "kind": "message",
            "id": "different",
            "role": "user",
            "content": "hello"
        });
        assert!(transcript_entry_matches_recovered(&a, &b));
        assert!(!transcript_entry_matches_recovered(
            &a,
            &json!({"kind":"message","role":"assistant","content":"hello"})
        ));

        assert!(transcript_entry_matches_recovered(
            &json!({"kind":"send-message","message":{"text":"x"}}),
            &json!({"kind":"send-message","message":{"text":"x"}})
        ));
        assert!(transcript_entry_matches_recovered(
            &json!({"kind":"tool-call","name":"mcp","status":"done","summary":"ok"}),
            &json!({"kind":"tool-call","name":"mcp","status":"done","summary":"ok"})
        ));
    }

    #[derive(Default)]
    struct MemoryBlobStore {
        reads: usize,
        writes: usize,
        value: Option<String>,
    }

    impl BlobStore<String> for MemoryBlobStore {
        type Error = String;

        fn get_blob(
            &mut self,
            _context: &str,
            _id: &[u8],
        ) -> Result<Option<String>, Self::Error> {
            self.reads += 1;
            Ok(self.value.clone())
        }

        fn set_blob(
            &mut self,
            _context: &str,
            _id: &[u8],
            data: String,
        ) -> Result<(), Self::Error> {
            self.writes += 1;
            self.value = Some(data);
            Ok(())
        }

        fn flush(&mut self, _context: &str) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    #[test]
    fn blob_reads_and_writes_share_one_recovery_cache() {
        let mut cached = CachedBlobReads::new(MemoryBlobStore {
            value: Some("old".into()),
            ..Default::default()
        });
        assert_eq!(
            cached.get_blob("c", b"id").unwrap().as_deref(),
            Some("old")
        );
        assert_eq!(
            cached.get_blob("c", b"id").unwrap().as_deref(),
            Some("old")
        );
        assert_eq!(cached.inner.reads, 1);

        cached.set_blob("c", b"id", "new".into()).unwrap();
        assert_eq!(
            cached.get_blob("c", b"id").unwrap().as_deref(),
            Some("new")
        );
        assert_eq!(cached.inner.reads, 1);
    }

    struct ProfileSource;
    impl RecoveryProfileSource for ProfileSource {
        fn name(&self) -> Option<String> {
            Some("  Mahayana  ".into())
        }
        fn description(&self) -> Option<String> {
            Some("  recovered  ".into())
        }
    }

    fn temp_dir() -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "fabushi-session-recovery-{}-{stamp}",
            std::process::id()
        ))
    }

    #[test]
    fn missing_profile_and_settings_are_recreated_without_overwriting_existing_files() {
        let root = temp_dir();
        fs::create_dir_all(&root).unwrap();
        let db = root.join("store.db");

        let profile = ensure_profile_file(&db, &ProfileSource).unwrap();
        let settings = ensure_settings_file(&db).unwrap();
        let profile_json: Value =
            serde_json::from_str(&fs::read_to_string(&profile).unwrap()).unwrap();
        assert_eq!(profile_json["name"], "Mahayana");
        assert_eq!(profile_json["description"], "recovered");
        assert_eq!(fs::read_to_string(&settings).unwrap().trim(), "{}");

        fs::write(&profile, "{\"name\":\"existing\"}\n").unwrap();
        ensure_profile_file(&db, &ProfileSource).unwrap();
        assert!(fs::read_to_string(&profile).unwrap().contains("existing"));

        let _ = fs::remove_dir_all(root);
    }
}
