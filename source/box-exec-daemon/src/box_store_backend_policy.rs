use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const SAND_BOX_STORE_LOCAL_DIR_ENV: &str = "SAND_BOX_STORE_LOCAL_DIR";
pub const SAND_BOX_STORE_BACKEND_ENV: &str = "SAND_BOX_STORE_BACKEND";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxStoreBackendKind {
    LocalFs,
    SandBoxStoreV2,
    AgentStore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoxStoreBackendPolicy {
    pub kind: BoxStoreBackendKind,
    pub local_dir: Option<PathBuf>,
}

pub fn resolve_backend_kind(
    local_dir: Option<&Path>,
    environment: &BTreeMap<String, String>,
) -> BoxStoreBackendKind {
    if local_dir.is_some() {
        return BoxStoreBackendKind::LocalFs;
    }
    match environment
        .get(SAND_BOX_STORE_BACKEND_ENV)
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("v2") => BoxStoreBackendKind::SandBoxStoreV2,
        _ => BoxStoreBackendKind::AgentStore,
    }
}

pub fn resolve_box_store_backend_policy(
    environment: &BTreeMap<String, String>,
) -> BoxStoreBackendPolicy {
    let local_dir = environment
        .get(SAND_BOX_STORE_LOCAL_DIR_ENV)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute());
    BoxStoreBackendPolicy {
        kind: resolve_backend_kind(local_dir.as_deref(), environment),
        local_dir,
    }
}

fn enabled(value: Option<&String>) -> bool {
    matches!(
        value.map(|value| value.trim().to_ascii_lowercase()).as_deref(),
        Some("1" | "true" | "yes")
    )
}

pub fn is_box_store_sync_enabled(environment: &BTreeMap<String, String>) -> bool {
    enabled(environment.get("SAND_BOX_STORE_SYNC"))
}

pub fn is_box_store_copy_in_enabled(environment: &BTreeMap<String, String>) -> bool {
    enabled(environment.get("SAND_BOX_STORE_COPY_IN"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_backend_policy_and_feature_flags_are_preserved() {
        let mut env = BTreeMap::new();
        assert_eq!(
            resolve_box_store_backend_policy(&env).kind,
            BoxStoreBackendKind::AgentStore
        );
        env.insert(SAND_BOX_STORE_BACKEND_ENV.into(), " V2 ".into());
        assert_eq!(
            resolve_box_store_backend_policy(&env).kind,
            BoxStoreBackendKind::SandBoxStoreV2
        );
        env.insert(SAND_BOX_STORE_LOCAL_DIR_ENV.into(), "/tmp/fabushi-box-store".into());
        let local = resolve_box_store_backend_policy(&env);
        assert_eq!(local.kind, BoxStoreBackendKind::LocalFs);
        assert_eq!(local.local_dir.as_deref(), Some(Path::new("/tmp/fabushi-box-store")));
        env.insert("SAND_BOX_STORE_SYNC".into(), "YES".into());
        env.insert("SAND_BOX_STORE_COPY_IN".into(), "true".into());
        assert!(is_box_store_sync_enabled(&env));
        assert!(is_box_store_copy_in_enabled(&env));
    }

    #[test]
    fn relative_local_directory_never_expands_the_runner_filesystem_boundary() {
        let mut env = BTreeMap::new();
        env.insert(SAND_BOX_STORE_LOCAL_DIR_ENV.into(), "../escape".into());
        let policy = resolve_box_store_backend_policy(&env);
        assert_eq!(policy.kind, BoxStoreBackendKind::AgentStore);
        assert!(policy.local_dir.is_none());
    }
}
