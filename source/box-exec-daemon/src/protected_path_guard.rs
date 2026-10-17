use std::ffi::OsString;
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectedPathError(pub String);

impl std::fmt::Display for ProtectedPathError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ProtectedPathError {}

pub fn refusal_message(path: &Path) -> String {
    format!(
        "Path is inside a protected host-only store and was refused: {}",
        path.display()
    )
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            Component::Normal(segment) => normalized.push(segment),
        }
    }
    normalized
}

fn realpath_nearest_existing(path: &Path) -> PathBuf {
    let mut cursor = path.to_path_buf();
    let mut suffix = Vec::<OsString>::new();
    loop {
        if let Ok(real) = fs::canonicalize(&cursor) {
            let mut result = real;
            for segment in suffix.iter().rev() {
                result.push(segment);
            }
            return lexical_normalize(&result);
        }
        let Some(name) = cursor.file_name().map(OsString::from) else {
            return lexical_normalize(path);
        };
        let Some(parent) = cursor.parent() else {
            return lexical_normalize(path);
        };
        suffix.push(name);
        cursor = parent.to_path_buf();
    }
}

fn is_path_within(root: &Path, candidate: &Path) -> bool {
    candidate == root || candidate.starts_with(root)
}

pub fn assert_path_outside_protected_roots(
    protected_roots: &[PathBuf],
    candidate_path: &Path,
    base_dir: &Path,
) -> Result<(), ProtectedPathError> {
    if protected_roots.is_empty() {
        return Ok(());
    }

    let resolved = if candidate_path.is_absolute() {
        lexical_normalize(candidate_path)
    } else {
        lexical_normalize(&base_dir.join(candidate_path))
    };

    for root in protected_roots {
        let root = lexical_normalize(root);
        if is_path_within(&root, &resolved) {
            return Err(ProtectedPathError(refusal_message(candidate_path)));
        }
    }

    let real_resolved = realpath_nearest_existing(&resolved);
    for root in protected_roots {
        let real_root = realpath_nearest_existing(root);
        if is_path_within(&real_root, &real_resolved) {
            return Err(ProtectedPathError(refusal_message(candidate_path)));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-protected-path-{label}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn rejects_direct_relative_and_symlink_aliases_of_protected_roots() {
        let root = root("guard");
        let _ = fs::remove_dir_all(&root);
        let protected = root.join("host-store");
        let workspace = root.join("workspace");
        fs::create_dir_all(&protected).unwrap();
        fs::create_dir_all(&workspace).unwrap();

        assert!(assert_path_outside_protected_roots(
            std::slice::from_ref(&protected),
            &protected.join("secret.db"),
            &workspace,
        ).is_err());
        assert!(assert_path_outside_protected_roots(
            std::slice::from_ref(&protected),
            Path::new("../host-store/secret.db"),
            &workspace,
        ).is_err());
        assert!(assert_path_outside_protected_roots(
            std::slice::from_ref(&protected),
            Path::new("safe/output.txt"),
            &workspace,
        ).is_ok());

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let alias = root.join("alias");
            symlink(&protected, &alias).unwrap();
            assert!(assert_path_outside_protected_roots(
                std::slice::from_ref(&protected),
                &alias.join("nested/secret.db"),
                &workspace,
            ).is_err());
        }
        let _ = fs::remove_dir_all(root);
    }
}
