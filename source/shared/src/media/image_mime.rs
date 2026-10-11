use super::media_extensions::{
    audio_mime_from_extension, client_native_image_mime_from_extension, extension_from_image_mime,
    extension_of, image_mime_from_extension, video_mime_from_extension,
};

pub fn image_mime_from_path(file_path: &str) -> Option<&'static str> {
    image_mime_from_extension(&extension_of(file_path))
}

pub fn servable_image_mime_from_path(file_path: &str) -> Option<&'static str> {
    let extension = extension_of(file_path);
    image_mime_from_extension(&extension)
        .or_else(|| client_native_image_mime_from_extension(&extension))
}

pub fn image_extension_from_mime(mime: &str) -> Option<&'static str> {
    extension_from_image_mime(mime)
}

pub fn video_mime_from_path(file_path: &str) -> Option<&'static str> {
    video_mime_from_extension(&extension_of(file_path))
}

pub fn audio_mime_from_path(file_path: &str) -> Option<&'static str> {
    audio_mime_from_extension(&extension_of(file_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_helpers_cover_native_and_web_media() {
        assert_eq!(image_mime_from_path("a.PNG"), Some("image/png"));
        assert_eq!(servable_image_mime_from_path("camera.HEIC"), Some("image/heic"));
        assert_eq!(video_mime_from_path("clip.mp4"), Some("video/mp4"));
        assert_eq!(audio_mime_from_path("voice.m4a"), Some("audio/mp4"));
        assert_eq!(image_extension_from_mime("IMAGE/JPEG"), Some(".jpg"));
    }
}
