use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LocalExecConnection {
    pub base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
}

impl LocalExecConnection {
    pub fn validate(&self) -> bool {
        !self.base_url.trim().is_empty()
            && self.token.as_ref().map_or(true, |value| !value.is_empty())
            && self
                .headers
                .values()
                .all(|value| !value.contains('\r') && !value.contains('\n'))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DaemonDescriptor {
    pub pid: u32,
    pub started_at_ms: u64,
    pub generation_token: String,
    pub entry_identity: String,
    #[serde(default)]
    pub inflight_count: u32,
}

impl DaemonDescriptor {
    pub fn validate(&self) -> bool {
        self.pid > 0
            && self.started_at_ms > 0
            && !self.generation_token.is_empty()
            && !self.entry_identity.is_empty()
    }

    pub fn same_generation(&self, other: &Self) -> bool {
        self.pid == other.pid
            && self.started_at_ms == other.started_at_ms
            && self.generation_token == other.generation_token
            && self.entry_identity == other.entry_identity
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorHeartbeat {
    pub pid: u32,
    pub at: u64,
}

impl SupervisorHeartbeat {
    pub fn validate(&self) -> bool {
        self.pid > 0 && self.at > 0
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LocalExecCredential {
    pub credential: String,
    pub backend_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_ms: Option<u64>,
}

impl LocalExecCredential {
    pub fn validate(&self) -> bool {
        !self.credential.is_empty() && !self.backend_url.trim().is_empty()
    }
}

pub fn discovery_name() -> &'static str { "local-exec-daemon.json" }
pub fn connection_name() -> &'static str { "local-exec-connection.json" }
pub fn credential_name() -> &'static str { "local-exec-credential.json" }
pub fn supervisor_heartbeat_name() -> &'static str { "local-exec-supervisor-heartbeat.json" }

pub fn read_connection(path: &Path) -> io::Result<Option<LocalExecConnection>> {
    read_valid_json(path, LocalExecConnection::validate)
}

pub fn write_connection(path: &Path, value: &LocalExecConnection) -> io::Result<()> {
    if !value.validate() { return Err(invalid_data("invalid local exec connection")); }
    write_json_atomic(path, value, true)
}

pub fn read_discovery(path: &Path) -> io::Result<Option<DaemonDescriptor>> {
    read_valid_json(path, DaemonDescriptor::validate)
}

pub fn write_discovery(path: &Path, value: &DaemonDescriptor) -> io::Result<()> {
    if !value.validate() { return Err(invalid_data("invalid local exec discovery")); }
    write_json_atomic(path, value, false)
}

pub fn read_credential(path: &Path) -> io::Result<Option<LocalExecCredential>> {
    read_valid_json(path, LocalExecCredential::validate)
}

pub fn write_credential(path: &Path, value: &LocalExecCredential) -> io::Result<()> {
    if !value.validate() { return Err(invalid_data("invalid local exec credential")); }
    write_json_atomic(path, value, true)
}

pub fn read_supervisor_heartbeat(path: &Path) -> io::Result<Option<SupervisorHeartbeat>> {
    read_valid_json(path, SupervisorHeartbeat::validate)
}

pub fn write_supervisor_heartbeat(path: &Path, value: &SupervisorHeartbeat) -> io::Result<()> {
    if !value.validate() { return Err(invalid_data("invalid local exec supervisor heartbeat")); }
    write_json_atomic(path, value, false)
}

pub fn clear_discovery_if_matches(path: &Path, expected: &DaemonDescriptor) -> io::Result<bool> {
    let quarantine = sibling_temp_path(path, "retired");
    match fs::rename(path, &quarantine) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    }

    let result = (|| -> io::Result<bool> {
        let observed = read_discovery(&quarantine)?;
        if observed.as_ref().is_some_and(|value| value.same_generation(expected)) {
            return Ok(true);
        }
        match fs::hard_link(&quarantine, path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        Ok(false)
    })();
    let _ = fs::remove_file(&quarantine);
    result
}

fn read_valid_json<T>(path: &Path, validate: impl Fn(&T) -> bool) -> io::Result<Option<T>>
where
    T: DeserializeOwned,
{
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let parsed: T = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    Ok(validate(&parsed).then_some(parsed))
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T, owner_only: bool) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| invalid_data("local exec path has no parent"))?;
    fs::create_dir_all(parent)?;
    let temporary = sibling_temp_path(path, "tmp");
    let serialized =
        serde_json::to_vec(value).map_err(|error| invalid_data(&error.to_string()))?;

    let result = (|| -> io::Result<()> {
        let mut file = create_new_file(&temporary, owner_only)?;
        file.write_all(&serialized)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        sync_directory(parent)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn sibling_temp_path(path: &Path, suffix: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("local-exec");
    path.with_file_name(format!(
        "{name}.{}.{}.{}",
        std::process::id(),
        stamp,
        suffix
    ))
}

#[cfg(unix)]
fn create_new_file(path: &Path, owner_only: bool) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    if owner_only {
        options.mode(0o600);
    }
    options.open(path)
}

#[cfg(not(unix))]
fn create_new_file(path: &Path, _owner_only: bool) -> io::Result<File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn invalid_data(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-local-exec-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
        ))
    }

    fn descriptor(generation: &str) -> DaemonDescriptor {
        DaemonDescriptor {
            pid: 77,
            started_at_ms: 1_234,
            generation_token: generation.into(),
            entry_identity: "/app/local-exec".into(),
            inflight_count: 2,
        }
    }

    #[test]
    fn validates_and_atomically_round_trips_protocol_files() {
        let dir = root("roundtrip");
        let connection_path = dir.join(connection_name());
        let credential_path = dir.join(credential_name());
        let discovery_path = dir.join(discovery_name());
        let heartbeat_path = dir.join(supervisor_heartbeat_name());

        let connection = LocalExecConnection {
            base_url: "http://127.0.0.1:4123".into(),
            token: Some("token".into()),
            headers: BTreeMap::from([("x-test".into(), "ok".into())]),
        };
        write_connection(&connection_path, &connection).unwrap();
        assert_eq!(read_connection(&connection_path).unwrap(), Some(connection));

        let credential = LocalExecCredential {
            credential: "renew-me".into(),
            backend_url: "https://api2.cursor.sh".into(),
            expires_at_ms: Some(42),
        };
        write_credential(&credential_path, &credential).unwrap();
        assert_eq!(read_credential(&credential_path).unwrap(), Some(credential));

        let discovery = descriptor("g-1");
        write_discovery(&discovery_path, &discovery).unwrap();
        assert_eq!(read_discovery(&discovery_path).unwrap(), Some(discovery));

        let heartbeat = SupervisorHeartbeat { pid: 77, at: 9_999 };
        write_supervisor_heartbeat(&heartbeat_path, &heartbeat).unwrap();
        assert_eq!(
            read_supervisor_heartbeat(&heartbeat_path).unwrap(),
            Some(heartbeat)
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn malformed_files_fail_closed_and_stale_generation_cannot_clear_current_discovery() {
        let dir = root("generation");
        fs::create_dir_all(&dir).unwrap();
        let discovery_path = dir.join(discovery_name());
        fs::write(dir.join(connection_name()), br#"{"baseUrl":""}"#).unwrap();
        assert_eq!(
            read_connection(&dir.join(connection_name())).unwrap(),
            None
        );

        let current = descriptor("current");
        write_discovery(&discovery_path, &current).unwrap();
        assert!(
            !clear_discovery_if_matches(&discovery_path, &descriptor("stale")).unwrap()
        );
        assert_eq!(
            read_discovery(&discovery_path).unwrap(),
            Some(current.clone())
        );
        assert!(clear_discovery_if_matches(&discovery_path, &current).unwrap());
        assert_eq!(read_discovery(&discovery_path).unwrap(), None);
        let _ = fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn secret_protocol_files_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = root("permissions");
        let connection_path = dir.join(connection_name());
        let credential_path = dir.join(credential_name());
        write_connection(
            &connection_path,
            &LocalExecConnection {
                base_url: "http://127.0.0.1:1".into(),
                token: Some("secret".into()),
                headers: BTreeMap::new(),
            },
        )
        .unwrap();
        write_credential(
            &credential_path,
            &LocalExecCredential {
                credential: "secret".into(),
                backend_url: "https://backend.invalid".into(),
                expires_at_ms: None,
            },
        )
        .unwrap();
        assert_eq!(
            fs::metadata(&connection_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(&credential_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let _ = fs::remove_dir_all(dir);
    }
}
