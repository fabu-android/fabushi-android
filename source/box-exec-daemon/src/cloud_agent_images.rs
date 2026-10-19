use std::fs;
use std::path::{Path, PathBuf};

pub const CLOUD_AGENT_IMAGE_BYTE_LIMIT: u64 = 25 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadedCloudAgentImage {
    pub data: Vec<u8>,
    pub path: PathBuf,
    pub mime_type: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloudAgentImageLoadError {
    NotFileUrl,
    NotImage,
    Refused,
    Unreadable,
    TooLarge,
}

#[derive(Clone, Debug)]
pub struct ScopedCloudAgentImageLoader {
    workspace_root: PathBuf,
    media_roots: Vec<PathBuf>,
}

impl ScopedCloudAgentImageLoader {
    pub fn new(
        workspace_root: impl Into<PathBuf>,
        media_roots: impl IntoIterator<Item = PathBuf>,
    ) -> Self {
        Self {
            workspace_root: workspace_root.into(),
            media_roots: media_roots.into_iter().collect(),
        }
    }

    pub fn load(&self, raw_url: &str) -> Result<LoadedCloudAgentImage, CloudAgentImageLoadError> {
        let parsed = url::Url::parse(raw_url).map_err(|_| CloudAgentImageLoadError::NotFileUrl)?;
        if parsed.scheme() != "file" {
            return Err(CloudAgentImageLoadError::NotFileUrl);
        }
        let requested = parsed
            .to_file_path()
            .map_err(|_| CloudAgentImageLoadError::Unreadable)?;
        let mime_type =
            image_mime_from_path(&requested).ok_or(CloudAgentImageLoadError::NotImage)?;
        let canonical = fs::canonicalize(&requested).map_err(|_| CloudAgentImageLoadError::Unreadable)?;
        let allowed = self
            .media_roots
            .iter()
            .chain(std::iter::once(&self.workspace_root))
            .any(|root| is_strict_descendant(root, &canonical));
        if !allowed {
            return Err(CloudAgentImageLoadError::Refused);
        }
        let metadata = fs::metadata(&canonical).map_err(|_| CloudAgentImageLoadError::Unreadable)?;
        if !metadata.is_file() {
            return Err(CloudAgentImageLoadError::Unreadable);
        }
        if metadata.len() > CLOUD_AGENT_IMAGE_BYTE_LIMIT {
            return Err(CloudAgentImageLoadError::TooLarge);
        }
        let data = fs::read(&canonical).map_err(|_| CloudAgentImageLoadError::Unreadable)?;
        if data.len() as u64 > CLOUD_AGENT_IMAGE_BYTE_LIMIT {
            return Err(CloudAgentImageLoadError::TooLarge);
        }
        Ok(LoadedCloudAgentImage {
            data,
            path: canonical,
            mime_type,
        })
    }
}

fn is_strict_descendant(root: &Path, candidate: &Path) -> bool {
    let Ok(root) = fs::canonicalize(root) else {
        return false;
    };
    candidate != root && candidate.starts_with(root)
}

pub fn image_mime_from_path(path: &Path) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("avif") => Some("image/avif"),
        Some("bmp") => Some("image/bmp"),
        Some("gif") => Some("image/gif"),
        Some("ico") => Some("image/x-icon"),
        Some("jpeg" | "jpg") => Some("image/jpeg"),
        Some("png") => Some("image/png"),
        Some("svg") => Some("image/svg+xml"),
        Some("webp") => Some("image/webp"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-cloud-agent-image-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[test]
    fn loads_only_images_inside_explicit_media_or_workspace_roots() {
        let root = root("allowed");
        let workspace = root.join("workspace");
        let media = root.join("media");
        let outside = root.join("outside");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&media).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let workspace_image = workspace.join("shot.png");
        let media_image = media.join("photo.jpg");
        let outside_image = outside.join("secret.png");
        fs::write(&workspace_image, b"png-bytes").unwrap();
        fs::write(&media_image, b"jpeg-bytes").unwrap();
        fs::write(&outside_image, b"secret").unwrap();

        let loader = ScopedCloudAgentImageLoader::new(&workspace, vec![media.clone()]);
        let workspace_loaded = loader
            .load(url::Url::from_file_path(&workspace_image).unwrap().as_str())
            .unwrap();
        assert_eq!(workspace_loaded.mime_type, "image/png");
        let media_loaded = loader
            .load(url::Url::from_file_path(&media_image).unwrap().as_str())
            .unwrap();
        assert_eq!(media_loaded.mime_type, "image/jpeg");
        assert_eq!(
            loader.load(url::Url::from_file_path(&outside_image).unwrap().as_str()),
            Err(CloudAgentImageLoadError::Refused)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_https_non_image_and_root_itself() {
        let root = root("rejected");
        let workspace = root.join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        let text = workspace.join("notes.txt");
        fs::write(&text, b"not image").unwrap();
        let loader = ScopedCloudAgentImageLoader::new(&workspace, Vec::<PathBuf>::new());

        assert_eq!(
            loader.load("https://example.com/shot.png"),
            Err(CloudAgentImageLoadError::NotFileUrl)
        );
        assert_eq!(
            loader.load(url::Url::from_file_path(&text).unwrap().as_str()),
            Err(CloudAgentImageLoadError::NotImage)
        );
        assert_eq!(
            loader.load(url::Url::from_directory_path(&workspace).unwrap().as_str()),
            Err(CloudAgentImageLoadError::NotImage)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn canonicalization_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let root = root("symlink");
        let workspace = root.join("workspace");
        let outside = root.join("outside");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let secret = outside.join("secret.png");
        fs::write(&secret, b"secret").unwrap();
        let escape = workspace.join("escape.png");
        symlink(&secret, &escape).unwrap();

        let loader = ScopedCloudAgentImageLoader::new(&workspace, Vec::<PathBuf>::new());
        assert_eq!(
            loader.load(url::Url::from_file_path(&escape).unwrap().as_str()),
            Err(CloudAgentImageLoadError::Refused)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_file_larger_than_desktop_attachment_limit() {
        let root = root("limit");
        let workspace = root.join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        let image = workspace.join("large.png");
        let file = fs::File::create(&image).unwrap();
        file.set_len(CLOUD_AGENT_IMAGE_BYTE_LIMIT + 1).unwrap();

        let loader = ScopedCloudAgentImageLoader::new(&workspace, Vec::<PathBuf>::new());
        assert_eq!(
            loader.load(url::Url::from_file_path(&image).unwrap().as_str()),
            Err(CloudAgentImageLoadError::TooLarge)
        );
        let _ = fs::remove_dir_all(root);
    }
}
