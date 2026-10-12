use crate::sha256::sha256_hex;
use std::{
    fs,
    io::{self, Write},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn read_machine_id(path: &Path) -> io::Result<String> {
    Ok(fs::read_to_string(path)?.trim().to_string())
}

pub fn write_machine_id(path: &Path, id: &str) -> io::Result<()> {
    let id = id.trim();
    if id.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty machine id"));
    }
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "machine id path has no parent")
    })?;
    fs::create_dir_all(parent)?;

    // Machine identity is durable protocol state. Never expose a partially written identity after
    // process death: write and fsync a sibling temporary file, then atomically replace the target.
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let result = (|| -> io::Result<()> {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(id.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn get_or_create_host_machine_id(path: &Path) -> io::Result<String> {
    if let Ok(id) = read_machine_id(path) {
        if !id.is_empty() {
            return Ok(id);
        }
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let material = format!("{}:{}:{}", std::process::id(), now, path.display());
    let id = sha256_hex(material.as_bytes());
    write_machine_id(path, &id)?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-host-machine-id-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[test]
    fn machine_id_survives_reopen_without_rotation() {
        let root = test_root("stable");
        let path = root.join("machine-id");
        let first = get_or_create_host_machine_id(&path).unwrap();
        let second = get_or_create_host_machine_id(&path).unwrap();
        assert_eq!(first, second);
        assert_eq!(read_machine_id(&path).unwrap(), first);
        assert_eq!(first.len(), 64);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn machine_id_write_is_atomic_and_rejects_empty_values() {
        let root = test_root("atomic");
        let path = root.join("machine-id");
        assert!(write_machine_id(&path, "   ").is_err());
        write_machine_id(&path, "machine-a").unwrap();
        assert_eq!(read_machine_id(&path).unwrap(), "machine-a");
        assert!(
            fs::read_dir(&root)
                .unwrap()
                .all(|entry| !entry.unwrap().file_name().to_string_lossy().contains("tmp-")),
            "successful machine-id replacement must not leave a temporary identity behind"
        );
        let _ = fs::remove_dir_all(root);
    }
}
