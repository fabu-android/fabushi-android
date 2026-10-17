use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub const ATTACHMENTS_DIRNAME: &str = "attachments";
pub const ASSETS_DIRNAME: &str = "assets";

static MEDIA_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentMediaKind {
    Image,
    Attachment,
}

pub fn get_agent_attachments_dir(root: &Path, agent_id: &str) -> Result<PathBuf, &'static str> {
    safe_agent_dir(root, agent_id, ATTACHMENTS_DIRNAME)
}

pub fn get_agent_assets_dir(root: &Path, agent_id: &str) -> Result<PathBuf, &'static str> {
    safe_agent_dir(root, agent_id, ASSETS_DIRNAME)
}

fn safe_agent_dir(root: &Path, agent_id: &str, leaf: &str) -> Result<PathBuf, &'static str> {
    if agent_id.trim().is_empty()
        || agent_id.contains('/')
        || agent_id.contains('\\')
        || agent_id.contains("..")
    {
        return Err("invalid agent id");
    }
    Ok(root.join("agents").join(agent_id).join(leaf))
}

pub fn get_agent_media_store_roots(
    root: &Path,
    agent_id: &str,
) -> Result<Vec<PathBuf>, &'static str> {
    Ok(vec![
        get_agent_attachments_dir(root, agent_id)?,
        get_agent_assets_dir(root, agent_id)?,
    ])
}

fn safe_media_leaf(source_name: &str) -> String {
    let leaf = Path::new(source_name)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("attachment");
    let mut safe = leaf
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    while safe.starts_with('.') {
        safe.remove(0);
    }
    if safe.is_empty() {
        safe.push_str("attachment");
    }
    safe.truncate(160);
    safe
}

fn unique_media_prefix() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = MEDIA_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{}-{now:x}-{sequence:x}", std::process::id())
}

pub fn persist_agent_media_bytes(
    root: &Path,
    agent_id: &str,
    source_name: &str,
    bytes: &[u8],
    kind: AgentMediaKind,
) -> io::Result<PathBuf> {
    let media_root = match kind {
        AgentMediaKind::Image => get_agent_assets_dir(root, agent_id),
        AgentMediaKind::Attachment => get_agent_attachments_dir(root, agent_id),
    }
    .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?;
    fs::create_dir_all(&media_root)?;

    let prefix = unique_media_prefix();
    let target = media_root.join(format!("{prefix}-{}", safe_media_leaf(source_name)));
    let temporary = media_root.join(format!(".{prefix}.tmp"));
    let result = (|| -> io::Result<()> {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, &target)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-attachment-paths-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[test]
    fn rejects_agent_id_traversal() {
        assert!(get_agent_assets_dir(Path::new("/tmp"), "../x").is_err());
        assert!(get_agent_attachments_dir(Path::new("/tmp"), "a/b").is_err());
        assert!(get_agent_attachments_dir(Path::new("/tmp"), "a\\b").is_err());
    }

    #[test]
    fn media_persistence_sanitizes_source_leaf_and_is_atomic() {
        let root = test_root("persist");
        let target = persist_agent_media_bytes(
            &root,
            "agent-a",
            "../.unsafe name?.txt",
            b"payload",
            AgentMediaKind::Attachment,
        )
        .unwrap();
        assert!(target.starts_with(root.join("agents/agent-a/attachments")));
        let name = target.file_name().unwrap().to_string_lossy();
        assert!(!name.starts_with('.'));
        assert!(!name.contains(' '));
        assert!(!name.contains('?'));
        assert_eq!(fs::read(&target).unwrap(), b"payload");
        let leftovers = fs::read_dir(target.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(leftovers, 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn image_and_attachment_media_use_separate_canonical_roots() {
        let root = test_root("kinds");
        let image = persist_agent_media_bytes(
            &root,
            "agent-a",
            "image.png",
            b"image",
            AgentMediaKind::Image,
        )
        .unwrap();
        let attachment = persist_agent_media_bytes(
            &root,
            "agent-a",
            "doc.txt",
            b"doc",
            AgentMediaKind::Attachment,
        )
        .unwrap();
        assert!(image.starts_with(root.join("agents/agent-a/assets")));
        assert!(attachment.starts_with(root.join("agents/agent-a/attachments")));
        let _ = fs::remove_dir_all(root);
    }
}
