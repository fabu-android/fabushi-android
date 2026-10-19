use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineImage {
    pub base64: String,
    pub media_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterializedInlineImage {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
}

fn extension_from_image_mime(mime: &str) -> Option<&'static str> {
    match mime.to_ascii_lowercase().as_str() {
        "image/avif" => Some(".avif"),
        "image/bmp" => Some(".bmp"),
        "image/gif" => Some(".gif"),
        "image/jpeg" => Some(".jpg"),
        "image/png" => Some(".png"),
        "image/svg+xml" => Some(".svg"),
        "image/webp" => Some(".webp"),
        "image/x-icon" | "image/vnd.microsoft.icon" => Some(".ico"),
        _ => None,
    }
}

fn materialize_one(
    db_path: &Path,
    image: &InlineImage,
) -> Option<MaterializedInlineImage> {
    let bytes = STANDARD.decode(image.base64.as_bytes()).ok()?;
    if bytes.is_empty() {
        return None;
    }

    let agent_dir = db_path.parent()?;
    let dir = agent_dir.join("xuser-attachments");
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let extension = extension_from_image_mime(&image.media_type).unwrap_or(".png");
    let file_path: PathBuf = dir.join(format!("{hash}{extension}"));

    if !file_path.exists() {
        fs::create_dir_all(&dir).ok()?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temporary = dir.join(format!(".{hash}.tmp-{}-{nonce}", std::process::id()));
        let result = (|| -> std::io::Result<()> {
            let mut file = fs::File::create(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, &file_path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
            return None;
        }
    }

    let url = Url::from_file_path(&file_path).ok()?.to_string();
    Some(MaterializedInlineImage {
        url,
        alt: image.alt.clone(),
    })
}

pub fn materialize_inline_images(
    db_path: impl AsRef<Path>,
    images: &[InlineImage],
) -> Vec<MaterializedInlineImage> {
    if images.is_empty() {
        return Vec::new();
    }

    images
        .iter()
        .take(4)
        .filter_map(|image| materialize_one(db_path.as_ref(), image))
        .collect()
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materialization_is_bounded_deterministic_and_reopen_safe() {
        let root = tempfile::tempdir().unwrap();
        let db = root.path().join("agent").join("transcript.sqlite");
        fs::create_dir_all(db.parent().unwrap()).unwrap();
        let png = STANDARD.encode(b"not-a-real-png-but-stable-bytes");
        let images = (0..6).map(|index| InlineImage {
            base64: png.clone(),
            media_type: if index % 2 == 0 { "image/png".into() } else { "image/jpeg".into() },
            alt: Some(format!("image-{index}")),
        }).collect::<Vec<_>>();
        let first = materialize_inline_images(&db, &images);
        assert_eq!(first.len(), 4);
        let second = materialize_inline_images(&db, &images);
        assert_eq!(first, second);
        for image in &first {
            let path = Url::parse(&image.url).unwrap().to_file_path().unwrap();
            assert!(path.is_file());
        }
        let attachment_dir = db.parent().unwrap().join("xuser-attachments");
        assert!(fs::read_dir(attachment_dir).unwrap().all(|entry| {
            !entry.unwrap().file_name().to_string_lossy().contains(".tmp-")
        }));
    }

    #[test]
    fn malformed_or_empty_images_are_not_materialized() {
        let root = tempfile::tempdir().unwrap();
        let db = root.path().join("transcript.sqlite");
        let images = vec![
            InlineImage { base64: "%%%".into(), media_type: "image/png".into(), alt: None },
            InlineImage { base64: STANDARD.encode([]), media_type: "image/png".into(), alt: None },
        ];
        assert!(materialize_inline_images(&db, &images).is_empty());
    }
}
