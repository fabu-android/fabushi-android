use super::media_extensions::{extension_of, video_mime_from_extension};
use std::{error::Error, fmt};

pub const BYTES_PER_MB: u64 = 1024 * 1024;
pub const ATTACHMENT_BYTE_LIMIT: u64 = 25 * BYTES_PER_MB;
pub const VIDEO_BYTE_LIMIT: u64 = 200 * BYTES_PER_MB;

pub fn name_looks_like_video(name: &str) -> bool {
    video_mime_from_extension(&extension_of(name)).is_some()
}

pub fn attachment_byte_limit_for_name(name: &str) -> u64 {
    if name_looks_like_video(name) {
        VIDEO_BYTE_LIMIT
    } else {
        ATTACHMENT_BYTE_LIMIT
    }
}

pub fn format_megabytes(bytes: u64) -> String {
    format!("{} MB", (bytes + BYTES_PER_MB / 2) / BYTES_PER_MB)
}

pub fn format_attachment_too_large_notice(filename: &str) -> String {
    let video = name_looks_like_video(filename);
    let limit = if video {
        VIDEO_BYTE_LIMIT
    } else {
        ATTACHMENT_BYTE_LIMIT
    };
    format!(
        "\"{filename}\" is too large to attach (max {}{}).",
        format_megabytes(limit),
        if video { " for video" } else { "" }
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachmentTooLargeError {
    pub limit_bytes: u64,
}

impl fmt::Display for AttachmentTooLargeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Attachment exceeds {} bytes.", self.limit_bytes)
    }
}

impl Error for AttachmentTooLargeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_gets_the_larger_limit_and_notice() {
        assert_eq!(attachment_byte_limit_for_name("clip.mp4"), VIDEO_BYTE_LIMIT);
        assert_eq!(
            attachment_byte_limit_for_name("notes.txt"),
            ATTACHMENT_BYTE_LIMIT
        );
        assert!(format_attachment_too_large_notice("clip.mp4").contains("200 MB for video"));
        assert!(format_attachment_too_large_notice("notes.txt").contains("25 MB"));
    }
}
