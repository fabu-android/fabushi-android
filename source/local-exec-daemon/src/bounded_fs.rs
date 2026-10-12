use std::{
    fs,
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct BoundedLocalFs {
    root: PathBuf,
    canonical_root: PathBuf,
    max_file_bytes: u64,
}

impl BoundedLocalFs {
    pub fn open(root: impl Into<PathBuf>, max_file_bytes: u64) -> io::Result<Self> {
        if max_file_bytes == 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "max file bytes must be positive"));
        }
        let root = root.into();
        fs::create_dir_all(&root)?;
        let canonical_root = fs::canonicalize(&root)?;
        Ok(Self { root: canonical_root.clone(), canonical_root, max_file_bytes })
    }

    pub fn root(&self) -> &Path { &self.root }

    pub fn resolve_contained(&self, requested: &Path) -> io::Result<PathBuf> {
        let candidate = if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            self.root.join(requested)
        };
        let normalized = normalize_lexically(&candidate)?;
        if !normalized.starts_with(&self.root) {
            return Err(permission_denied(requested));
        }

        let (existing, suffix) = nearest_existing(&normalized)?;
        let canonical_existing = fs::canonicalize(&existing)?;
        if !canonical_existing.starts_with(&self.canonical_root) {
            return Err(permission_denied(requested));
        }
        let resolved = suffix.iter().fold(canonical_existing, |path, segment| path.join(segment));
        if !resolved.starts_with(&self.canonical_root) {
            return Err(permission_denied(requested));
        }
        Ok(resolved)
    }

    pub fn resolve_working_directory(&self, requested: &Path) -> io::Result<(PathBuf, bool)> {
        if requested.as_os_str().is_empty() {
            return Ok((self.root.clone(), false));
        }
        match self.resolve_contained(requested) {
            Ok(path) if path.is_dir() => Ok((path, false)),
            Ok(_) => Ok((self.root.clone(), true)),
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => Err(error),
            Err(_) => Ok((self.root.clone(), true)),
        }
    }

    pub fn read_file(&self, requested: &Path) -> io::Result<Vec<u8>> {
        let path = self.resolve_contained(requested)?;
        let metadata = fs::metadata(&path)?;
        if !metadata.is_file() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "local-exec path is not a regular file"));
        }
        if metadata.len() > self.max_file_bytes {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "local-exec file exceeds bounded size"));
        }
        let mut file = fs::File::open(path)?;
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take(self.max_file_bytes.saturating_add(1)).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > self.max_file_bytes {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "local-exec file exceeds bounded size"));
        }
        Ok(bytes)
    }

    pub fn write_file_atomic(&self, requested: &Path, bytes: &[u8]) -> io::Result<()> {
        if bytes.len() as u64 > self.max_file_bytes {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "local-exec upload exceeds bounded size"));
        }
        let path = self.resolve_contained(requested)?;
        let parent = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "local-exec path has no parent"))?;
        fs::create_dir_all(parent)?;
        // Re-resolve after mkdir so a concurrently introduced symlink cannot widen the boundary.
        let path = self.resolve_contained(requested)?;
        let parent = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "local-exec path has no parent"))?;
        let name = path.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "local-exec path has no file name"))?;
        let temp = parent.join(format!(".{}.{}.tmp", name.to_string_lossy(), std::process::id()));
        let result = (|| -> io::Result<()> {
            let mut file = fs::File::create(&temp)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            fs::rename(&temp, &path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }
}

fn permission_denied(requested: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!("path is outside the allowed local-exec root: {}", requested.display()),
    )
}

fn normalize_lexically(path: &Path) -> io::Result<PathBuf> {
    let mut output = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => output.push(prefix.as_os_str()),
            Component::RootDir => output.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !output.pop() {
                    return Err(permission_denied(path));
                }
            }
            Component::Normal(segment) => output.push(segment),
        }
    }
    Ok(output)
}

fn nearest_existing(path: &Path) -> io::Result<(PathBuf, Vec<std::ffi::OsString>)> {
    let mut current = path.to_path_buf();
    let mut suffix = Vec::new();
    while !current.exists() {
        let name = current.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no existing local-exec ancestor"))?;
        suffix.push(name.to_os_string());
        if !current.pop() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "no existing local-exec ancestor"));
        }
    }
    suffix.reverse();
    Ok((current, suffix))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-local-fs-{label}-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ))
    }

    #[test]
    fn traversal_and_absolute_outside_root_are_rejected() {
        let root = root("contain");
        let fs = BoundedLocalFs::open(&root, 1024).unwrap();
        assert_eq!(fs.resolve_contained(Path::new("../escape")).unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        let outside = root.parent().unwrap().join("escape");
        assert_eq!(fs.resolve_contained(&outside).unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_rejected_for_existing_and_future_children() {
        use std::os::unix::fs::symlink;
        let sandbox = root("symlink");
        let outside = root("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let fs = BoundedLocalFs::open(&sandbox, 1024).unwrap();
        symlink(&outside, sandbox.join("link")).unwrap();
        assert_eq!(fs.resolve_contained(Path::new("link/file.txt")).unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        let _ = std::fs::remove_dir_all(sandbox);
        let _ = std::fs::remove_dir_all(outside);
    }

    #[test]
    fn bounded_atomic_write_and_read_preserve_root() {
        let root = root("io");
        let fs = BoundedLocalFs::open(&root, 8).unwrap();
        fs.write_file_atomic(Path::new("nested/file.txt"), b"12345678").unwrap();
        assert_eq!(fs.read_file(Path::new("nested/file.txt")).unwrap(), b"12345678");
        assert!(fs.write_file_atomic(Path::new("nested/too-big.txt"), b"123456789").is_err());
        std::fs::write(root.join("oversized.txt"), b"123456789").unwrap();
        assert!(fs.read_file(Path::new("oversized.txt")).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn missing_working_directory_falls_back_but_escape_does_not() {
        let root = root("cwd");
        let fs = BoundedLocalFs::open(&root, 1024).unwrap();
        let (resolved, fell_back) = fs.resolve_working_directory(Path::new("missing")).unwrap();
        assert_eq!(resolved, root);
        assert!(fell_back);
        assert_eq!(fs.resolve_working_directory(Path::new("../escape")).unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        let _ = std::fs::remove_dir_all(root);
    }
}
