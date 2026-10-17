use std::collections::BTreeMap;
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::Value;

use crate::extensions::session::session_diagnostics::{
    SessionDiagnostic, report_session_diagnostic,
};
use crate::storage::agent_paths::{SandInvalidAgentIdError, assert_valid_sand_agent_id};

pub use super::session_path_contracts::{
    ACTIVE_AGENT_FILENAME, CONNECTOR_SECRETS_DIRNAME, CONVERSATION_BLOBS_FILENAME,
    HIDDEN_ENTRY_REPAIR_VERSION, LEGACY_GROUP_MEMBERS_DIRNAME, NATIVE_CONVERSATIONS_DIRNAME,
    SAND_CONVERSATION_ROOT_SLOT_ID, STALE_ROOT_CLEANUP_VERSION, STORE_FILENAME,
};

static PINNED_STALE_ROOT_GC_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn pin_stale_root_gc(enabled: bool) {
    PINNED_STALE_ROOT_GC_ENABLED.store(enabled, Ordering::Release);
}

/// Android production config owns this flag. Environment-variable probing is deliberately omitted:
/// app-process configuration is injected through the typed runtime instead of Desktop shell state.
pub fn is_stale_root_gc_enabled() -> bool {
    PINNED_STALE_ROOT_GC_ENABLED.load(Ordering::Acquire)
}

pub fn get_agent_db_path(
    agents_root: &Path,
    agent_id: &str,
) -> Result<PathBuf, SandInvalidAgentIdError> {
    assert_valid_sand_agent_id(agent_id)?;
    Ok(agents_root.join(agent_id).join(STORE_FILENAME))
}

pub fn get_native_conversations_root(agents_root: &Path) -> PathBuf {
    agents_root.join(NATIVE_CONVERSATIONS_DIRNAME)
}

pub fn get_native_conversation_db_path(
    agents_root: &Path,
    conversation_id: &str,
) -> Result<PathBuf, SandInvalidAgentIdError> {
    assert_valid_sand_agent_id(conversation_id)?;
    Ok(get_native_conversations_root(agents_root)
        .join(conversation_id)
        .join(STORE_FILENAME))
}

pub fn get_connector_secrets_root(agents_root: &Path) -> PathBuf {
    agents_root
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(CONNECTOR_SECRETS_DIRNAME)
}

pub fn stat_if_exists(path: &Path) -> Option<Metadata> {
    match fs::metadata(path) {
        Ok(metadata) => Some(metadata),
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                let agent_id = path
                    .parent()
                    .and_then(Path::file_name)
                    .map(|value| value.to_string_lossy().into_owned())
                    .unwrap_or_default();
                report_session_diagnostic(&SessionDiagnostic {
                    family: "store_db".into(),
                    kind: "path_stat_failed".into(),
                    metadata: BTreeMap::from([
                        ("agentId".into(), Value::String(agent_id)),
                        (
                            "errorClass".into(),
                            Value::String(format!("{:?}", error.kind())),
                        ),
                    ]),
                });
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_beneath_explicit_android_agents_root() {
        let root = Path::new("/data/user/0/fabushi/files/agents");
        assert_eq!(
            get_agent_db_path(root, "agent-a").unwrap(),
            root.join("agent-a").join(STORE_FILENAME)
        );
        assert_eq!(
            get_native_conversation_db_path(root, "conversation-a").unwrap(),
            root.join(NATIVE_CONVERSATIONS_DIRNAME)
                .join("conversation-a")
                .join(STORE_FILENAME)
        );
        assert_eq!(
            get_connector_secrets_root(root),
            root.parent().unwrap().join(CONNECTOR_SECRETS_DIRNAME)
        );
        assert!(get_agent_db_path(root, "../escape").is_err());
    }

    #[test]
    fn stale_root_gc_is_owned_by_typed_runtime_pin() {
        pin_stale_root_gc(false);
        assert!(!is_stale_root_gc_enabled());
        pin_stale_root_gc(true);
        assert!(is_stale_root_gc_enabled());
        pin_stale_root_gc(false);
    }
}
