use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::storage::folder_id::is_safe_folder_id;

pub type SecretRecord = Map<String, Value>;

#[derive(Debug, thiserror::Error)]
pub enum ConnectorSecretStoreError {
    #[error("Android connector secrets must be stored by the protected platform connector vault")]
    ProtectedStoreRequired,
}

/// Compatibility boundary for Desktop Session APIs that historically owned connector credentials.
///
/// Android deliberately does not persist connector tokens under the Rust Session filesystem. The
/// canonical production connector/MCP owners keep credentials behind the Android protected-store
/// boundary and project only non-secret connection state into Host/Presentation. Keeping this type
/// fail-closed lets platform-independent Session code compile without introducing a second token
/// owner or silently downgrading AndroidKeyStore protection to a JSON file.
#[derive(Debug, Clone)]
pub struct SandConnectorSecretStore {
    legacy_root: PathBuf,
}

impl SandConnectorSecretStore {
    pub fn new(secrets_root: impl Into<PathBuf>) -> Self {
        Self {
            legacy_root: secrets_root.into(),
        }
    }

    pub fn secrets_root(&self) -> &Path {
        &self.legacy_root
    }

    pub fn file_path(&self, agent_id: &str, platform: &str) -> PathBuf {
        self.legacy_root
            .join(agent_id)
            .join(format!("{platform}.json"))
    }

    pub fn read(&self, _agent_id: &str, _platform: &str) -> SecretRecord {
        SecretRecord::new()
    }

    pub fn set_secret(
        &self,
        agent_id: &str,
        platform: &str,
        field: &str,
        _value: &str,
    ) -> Result<bool, ConnectorSecretStoreError> {
        if !is_safe_folder_id(agent_id)
            || !is_safe_folder_id(platform)
            || field.trim().is_empty()
        {
            return Ok(false);
        }
        Err(ConnectorSecretStoreError::ProtectedStoreRequired)
    }

    pub fn get_secret(
        &self,
        agent_id: &str,
        platform: &str,
        field: &str,
    ) -> Option<String> {
        if !is_safe_folder_id(agent_id)
            || !is_safe_folder_id(platform)
            || field.trim().is_empty()
        {
            return None;
        }
        None
    }

    pub fn remove_agent_platform(
        &self,
        agent_id: &str,
        platform: &str,
    ) -> Result<bool, ConnectorSecretStoreError> {
        if !is_safe_folder_id(agent_id) || !is_safe_folder_id(platform) {
            return Ok(false);
        }
        Err(ConnectorSecretStoreError::ProtectedStoreRequired)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_compatibility_store_never_materializes_plaintext_secrets() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-android-connector-secret-{}",
            std::process::id()
        ));
        let store = SandConnectorSecretStore::new(&root);
        assert!(matches!(
            store.set_secret("agent-a", "slack", "token", "secret"),
            Err(ConnectorSecretStoreError::ProtectedStoreRequired)
        ));
        assert_eq!(store.get_secret("agent-a", "slack", "token"), None);
        assert!(matches!(
            store.remove_agent_platform("agent-a", "slack"),
            Err(ConnectorSecretStoreError::ProtectedStoreRequired)
        ));
        assert!(!root.exists(), "legacy Session adapter must not create a secret directory");
    }

    #[test]
    fn invalid_legacy_secret_coordinates_fail_closed_without_platform_calls() {
        let store = SandConnectorSecretStore::new("/unused");
        assert_eq!(store.set_secret("../escape", "slack", "token", "x").unwrap(), false);
        assert_eq!(store.get_secret("../escape", "slack", "token"), None);
        assert_eq!(store.remove_agent_platform("agent", "../escape").unwrap(), false);
    }
}
