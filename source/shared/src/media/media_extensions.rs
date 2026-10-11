pub fn extension_of(name: &str) -> String {
    let slash = name.rfind('/').unwrap_or(0);
    let backslash = name.rfind('\\').unwrap_or(0);
    let start = if name.contains('/') || name.contains('\\') {
        slash.max(backslash).saturating_add(1)
    } else {
        0
    };
    let base = &name[start..];
    match base.rfind('.') {
        Some(dot) if dot > 0 => base[dot..].to_ascii_lowercase(),
        _ => String::new(),
    }
}

pub fn image_mime_from_extension(extension: &str) -> Option<&'static str> {
    match extension {
        ".avif" => Some("image/avif"),
        ".bmp" => Some("image/bmp"),
        ".gif" => Some("image/gif"),
        ".ico" => Some("image/x-icon"),
        ".jpeg" | ".jpg" => Some("image/jpeg"),
        ".png" => Some("image/png"),
        ".svg" => Some("image/svg+xml"),
        ".webp" => Some("image/webp"),
        _ => None,
    }
}

pub fn client_native_image_mime_from_extension(extension: &str) -> Option<&'static str> {
    match extension {
        ".heic" => Some("image/heic"),
        ".heif" => Some("image/heif"),
        _ => None,
    }
}

pub fn extension_from_image_mime(mime: &str) -> Option<&'static str> {
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

pub fn video_mime_from_extension(extension: &str) -> Option<&'static str> {
    match extension {
        ".m4v" | ".mp4" => Some("video/mp4"),
        ".mov" => Some("video/quicktime"),
        ".ogv" => Some("video/ogg"),
        ".webm" => Some("video/webm"),
        _ => None,
    }
}

pub fn audio_mime_from_extension(extension: &str) -> Option<&'static str> {
    match extension {
        ".aac" => Some("audio/aac"),
        ".flac" => Some("audio/flac"),
        ".m4a" => Some("audio/mp4"),
        ".mp3" => Some("audio/mpeg"),
        ".oga" | ".ogg" | ".opus" => Some("audio/ogg"),
        ".wav" => Some("audio/wav"),
        ".weba" => Some("audio/webm"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_and_mime_mapping_ignore_parent_dots_and_case() {
        assert_eq!(extension_of("/tmp.v1/Photo.JPG"), ".jpg");
        assert_eq!(extension_of(".hidden"), "");
        assert_eq!(image_mime_from_extension(".jpg"), Some("image/jpeg"));
        assert_eq!(video_mime_from_extension(".mov"), Some("video/quicktime"));
        assert_eq!(audio_mime_from_extension(".opus"), Some("audio/ogg"));
    }
}
