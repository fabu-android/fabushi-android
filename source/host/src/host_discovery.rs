use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GatewayScheme {
    Http,
    Https,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayDiscoveryInfo {
    pub port: u16,
    pub pid: u32,
    pub started_at_ms: u64,
    pub scheme: GatewayScheme,
    pub host: String,
    pub token: String,
    /// Android extension used to reject stale process-recreation clients.
    pub generation: u64,
}

impl GatewayDiscoveryInfo {
    pub fn validate(&self) -> bool {
        self.port > 0
            && self.pid > 0
            && self.started_at_ms > 0
            && !self.host.trim().is_empty()
            && self.token.len() >= 16
            && self.generation > 0
    }
}

pub fn write_gateway_discovery(path: &Path, info: &GatewayDiscoveryInfo) -> io::Result<()> {
    if !info.validate() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid gateway discovery"));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file_name = path.file_name().and_then(|value| value.to_str()).unwrap_or("gateway.json");
    let temp = path.with_file_name(format!("{file_name}.{}.tmp", std::process::id()));
    let encoded = serde_json::to_vec_pretty(info)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    fs::write(&temp, encoded)?;
    fs::rename(temp, path)
}

pub fn read_gateway_discovery(path: &Path) -> io::Result<GatewayDiscoveryInfo> {
    let bytes = fs::read(path)?;
    let info: GatewayDiscoveryInfo = serde_json::from_slice(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if info.validate() {
        Ok(info)
    } else {
        Err(io::Error::new(io::ErrorKind::InvalidData, "invalid gateway discovery"))
    }
}

pub fn clear_gateway_discovery(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path() -> std::path::PathBuf {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("fabushi-host-discovery-{nonce}.json"))
    }

    #[test]
    fn discovery_round_trip_is_atomic_and_generation_aware() {
        let path = temp_path();
        let info = GatewayDiscoveryInfo {
            port: 40123,
            pid: 42,
            started_at_ms: 7,
            scheme: GatewayScheme::Https,
            host: "127.0.0.1".into(),
            token: "0123456789abcdef0123456789abcdef".into(),
            generation: 9,
        };
        write_gateway_discovery(&path, &info).unwrap();
        assert_eq!(read_gateway_discovery(&path).unwrap(), info);
        clear_gateway_discovery(&path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn discovery_fails_closed_for_missing_auth_or_generation() {
        let invalid = GatewayDiscoveryInfo {
            port: 1,
            pid: 1,
            started_at_ms: 1,
            scheme: GatewayScheme::Http,
            host: "localhost".into(),
            token: "short".into(),
            generation: 0,
        };
        assert!(!invalid.validate());
    }
}
