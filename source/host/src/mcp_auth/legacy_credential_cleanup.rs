use std::fs;
use std::path::Path;

pub const LEGACY_MCP_AUTH_FILENAME: &str = "mcp-auth.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyMcpAuthCleanupOutcome {
    NotFound,
    Error,
    Deleted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LegacyMcpAuthCleanupResult {
    pub outcome: LegacyMcpAuthCleanupOutcome,
    pub removed_count: usize,
}

pub fn is_legacy_mcp_auth_file(name: &str) -> bool {
    name == LEGACY_MCP_AUTH_FILENAME
        || name
            .strip_prefix(LEGACY_MCP_AUTH_FILENAME)
            .is_some_and(|suffix| suffix.starts_with('.'))
}

pub fn cleanup_legacy_mcp_auth_credentials(root_dir: &Path) -> LegacyMcpAuthCleanupResult {
    let entries = match fs::read_dir(root_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return LegacyMcpAuthCleanupResult {
                outcome: LegacyMcpAuthCleanupOutcome::NotFound,
                removed_count: 0,
            };
        }
        Err(_) => {
            return LegacyMcpAuthCleanupResult {
                outcome: LegacyMcpAuthCleanupOutcome::Error,
                removed_count: 0,
            };
        }
    };

    let mut candidates = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else {
            return LegacyMcpAuthCleanupResult {
                outcome: LegacyMcpAuthCleanupOutcome::Error,
                removed_count: 0,
            };
        };
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if is_legacy_mcp_auth_file(name) {
            candidates.push(entry.path());
        }
    }

    if candidates.is_empty() {
        return LegacyMcpAuthCleanupResult {
            outcome: LegacyMcpAuthCleanupOutcome::NotFound,
            removed_count: 0,
        };
    }

    let mut removed_count = 0;
    let mut saw_error = false;
    for path in candidates {
        match fs::remove_file(&path) {
            Ok(()) => removed_count += 1,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => removed_count += 1,
            Err(_) => saw_error = true,
        }
    }

    LegacyMcpAuthCleanupResult {
        outcome: if saw_error {
            LegacyMcpAuthCleanupOutcome::Error
        } else {
            LegacyMcpAuthCleanupOutcome::Deleted
        },
        removed_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_root(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "fabushi-mcp-auth-cleanup-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn legacy_name_matching_is_exact_or_dot_suffixed() {
        assert!(is_legacy_mcp_auth_file("mcp-auth.json"));
        assert!(is_legacy_mcp_auth_file("mcp-auth.json.backup"));
        assert!(!is_legacy_mcp_auth_file("mcp-auth.jsonx"));
        assert!(!is_legacy_mcp_auth_file("other-mcp-auth.json"));
    }

    #[test]
    fn missing_root_and_empty_root_are_not_found() {
        let root = test_root("missing");
        assert_eq!(
            cleanup_legacy_mcp_auth_credentials(&root),
            LegacyMcpAuthCleanupResult {
                outcome: LegacyMcpAuthCleanupOutcome::NotFound,
                removed_count: 0,
            }
        );

        fs::create_dir_all(&root).unwrap();
        assert_eq!(
            cleanup_legacy_mcp_auth_credentials(&root),
            LegacyMcpAuthCleanupResult {
                outcome: LegacyMcpAuthCleanupOutcome::NotFound,
                removed_count: 0,
            }
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn deletes_only_legacy_mcp_auth_files() {
        let root = test_root("delete");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("mcp-auth.json"), b"secret").unwrap();
        fs::write(root.join("mcp-auth.json.backup"), b"secret").unwrap();
        fs::write(root.join("plugin-permissions.json"), b"keep").unwrap();

        assert_eq!(
            cleanup_legacy_mcp_auth_credentials(&root),
            LegacyMcpAuthCleanupResult {
                outcome: LegacyMcpAuthCleanupOutcome::Deleted,
                removed_count: 2,
            }
        );
        assert!(!root.join("mcp-auth.json").exists());
        assert!(!root.join("mcp-auth.json.backup").exists());
        assert!(root.join("plugin-permissions.json").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn directory_candidate_reports_error_without_deleting_unrelated_state() {
        let root = test_root("error");
        fs::create_dir_all(root.join("mcp-auth.json")).unwrap();
        fs::write(root.join("keep"), b"keep").unwrap();

        let result = cleanup_legacy_mcp_auth_credentials(&root);
        assert_eq!(result.outcome, LegacyMcpAuthCleanupOutcome::Error);
        assert_eq!(result.removed_count, 0);
        assert!(root.join("keep").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
