#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalExecFailureClass {
    Other,
    SpawnEnoent,
    SpawnPermissions,
    SpawnOther,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalExecFailureClassification {
    pub error_class: LocalExecFailureClass,
    pub errno: Option<String>,
}

fn errno_token(message: &str) -> Option<String> {
    message
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .find(|token| {
            token.len() >= 2
                && token.starts_with('E')
                && token
                    .chars()
                    .skip(1)
                    .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
        })
        .map(ToOwned::to_owned)
}

/// Preserve the Desktop local-exec failure taxonomy without adding arbitrary process execution to
/// Android. Bounded platform actions may use the classification for stable diagnostics while
/// AndroidLocalRunner / the authorized Remote Runner remain the only execution owners.
pub fn classify_local_exec_failure(message: &str) -> LocalExecFailureClassification {
    let lower = message.to_ascii_lowercase();
    let is_spawn = lower
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .any(|token| token == "spawn" || token == "spawnsync");
    let errno = errno_token(message);
    let error_class = if !is_spawn {
        LocalExecFailureClass::Other
    } else {
        match errno.as_deref() {
            Some("ENOENT") => LocalExecFailureClass::SpawnEnoent,
            Some("EACCES" | "EPERM") => LocalExecFailureClass::SpawnPermissions,
            _ => LocalExecFailureClass::SpawnOther,
        }
    };
    LocalExecFailureClassification { error_class, errno }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_spawn_enoent() {
        assert_eq!(
            classify_local_exec_failure("spawn helper ENOENT"),
            LocalExecFailureClassification {
                error_class: LocalExecFailureClass::SpawnEnoent,
                errno: Some("ENOENT".into()),
            },
        );
    }

    #[test]
    fn classifies_spawn_permissions() {
        for raw in ["spawnSync helper EACCES", "spawn helper EPERM"] {
            assert_eq!(
                classify_local_exec_failure(raw).error_class,
                LocalExecFailureClass::SpawnPermissions,
            );
        }
    }

    #[test]
    fn classifies_other_spawn_errno_without_overclaiming() {
        let classified = classify_local_exec_failure("spawn helper EMFILE");
        assert_eq!(classified.error_class, LocalExecFailureClass::SpawnOther);
        assert_eq!(classified.errno.as_deref(), Some("EMFILE"));
    }

    #[test]
    fn non_spawn_failure_is_other_even_with_errno_token() {
        let classified = classify_local_exec_failure("read failed ENOENT");
        assert_eq!(classified.error_class, LocalExecFailureClass::Other);
        assert_eq!(classified.errno.as_deref(), Some("ENOENT"));
    }
}
