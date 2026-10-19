use std::{fs, path::{Path, PathBuf}};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedImageInput {
    pub data: Vec<u8>,
    pub path: PathBuf,
    pub mime_type: Option<String>,
}

pub fn image_mime_from_path(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => return None,
    }.to_string())
}

pub fn load_selected_image_inputs(paths: &[PathBuf]) -> Vec<SelectedImageInput> {
    paths.iter().filter_map(|path| {
        let data = fs::read(path).ok()?;
        Some(SelectedImageInput {
            data,
            path: path.clone(),
            mime_type: image_mime_from_path(path),
        })
    }).collect()
}

pub fn is_supported_image(path: &Path) -> bool {
    image_mime_from_path(path).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn loads_bytes_and_keeps_unknown_mime_when_file_is_readable() {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("fabushi-selected-image-{nonce}"));
        fs::create_dir_all(&root).unwrap();
        let png = root.join("a.png");
        let unknown = root.join("b.bin");
        fs::write(&png, [1u8, 2, 3]).unwrap();
        fs::write(&unknown, [4u8, 5]).unwrap();
        let loaded = load_selected_image_inputs(&[png.clone(), unknown.clone(), root.join("missing")]);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].data, vec![1, 2, 3]);
        assert_eq!(loaded[0].mime_type.as_deref(), Some("image/png"));
        assert_eq!(loaded[1].mime_type, None);
        let _ = fs::remove_dir_all(root);
    }
}
