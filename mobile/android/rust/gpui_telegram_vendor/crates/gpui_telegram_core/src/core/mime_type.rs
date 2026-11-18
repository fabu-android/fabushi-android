//! Telegram Desktop 1:1 Parity Module: `MimeType`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/core/mime_type.h, Telegram/SourceFiles/core/mime_type.cpp

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;

/// Known Telegram custom and proprietary MIME classifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum KnownMimeType {
    #[default]
    Unknown,
    TDesktopTheme,
    TDesktopPalette,
    WebP,
    Tgs,
    Tgv,
}

/// Disjoint file category derived from path extension and signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NameType {
    #[default]
    Unknown,
    Executable,
    Image,
    Video,
    Audio,
    Document,
    Archive,
    ThemeFile,
    OtherBenign,
}

/// MIME type representation encapsulating custom Telegram types and MIME strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MimeType {
    pub known: KnownMimeType,
    pub raw_mime: String,
}

impl Default for MimeType {
    fn default() -> Self {
        Self {
            known: KnownMimeType::Unknown,
            raw_mime: "application/octet-stream".to_string(),
        }
    }
}

impl MimeType {
    pub fn new(known: KnownMimeType) -> Self {
        let raw_mime = match known {
            KnownMimeType::WebP => "image/webp",
            KnownMimeType::Tgs => "application/x-tgsticker",
            KnownMimeType::Tgv => "application/x-tgwallpattern",
            KnownMimeType::TDesktopTheme => "application/x-tdesktop-theme",
            KnownMimeType::TDesktopPalette => "application/x-tdesktop-palette",
            KnownMimeType::Unknown => "application/octet-stream",
        }
        .to_string();

        Self { known, raw_mime }
    }

    pub fn from_mime_string(mime: impl Into<String>) -> Self {
        let raw = mime.into();
        let known = match raw.as_str() {
            "image/webp" => KnownMimeType::WebP,
            "application/x-tgsticker" => KnownMimeType::Tgs,
            "application/x-tgwallpattern" => KnownMimeType::Tgv,
            "application/x-tdesktop-theme" | "application/x-tgtheme-tdesktop" => {
                KnownMimeType::TDesktopTheme
            }
            "application/x-tdesktop-palette" => KnownMimeType::TDesktopPalette,
            _ => KnownMimeType::Unknown,
        };
        Self {
            known,
            raw_mime: raw,
        }
    }

    pub fn name(&self) -> &str {
        &self.raw_mime
    }

    pub fn filter_string(&self) -> &'static str {
        match self.known {
            KnownMimeType::WebP => "WebP image (*.webp)",
            KnownMimeType::Tgs => "Telegram sticker (*.tgs)",
            KnownMimeType::Tgv => "Wallpaper pattern (*.tgv)",
            KnownMimeType::TDesktopTheme => "Theme files (*.tdesktop-theme)",
            KnownMimeType::TDesktopPalette => "Palette files (*.tdesktop-palette)",
            KnownMimeType::Unknown => "All files (*.*)",
        }
    }

    pub fn glob_patterns(&self) -> Vec<&'static str> {
        match self.known {
            KnownMimeType::WebP => vec!["*.webp"],
            KnownMimeType::Tgs => vec!["*.tgs"],
            KnownMimeType::Tgv => vec!["*.tgv"],
            KnownMimeType::TDesktopTheme => vec!["*.tdesktop-theme"],
            KnownMimeType::TDesktopPalette => vec!["*.tdesktop-palette"],
            KnownMimeType::Unknown => vec!["*.*"],
        }
    }
}

pub fn mime_type_for_name(mime: &str) -> MimeType {
    MimeType::from_mime_string(mime)
}

pub fn mime_type_for_file(path: &Path) -> MimeType {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "webp" => MimeType::new(KnownMimeType::WebP),
        "tgs" => MimeType::new(KnownMimeType::Tgs),
        "tgv" => MimeType::new(KnownMimeType::Tgv),
        "tdesktop-theme" => MimeType::new(KnownMimeType::TDesktopTheme),
        "tdesktop-palette" => MimeType::new(KnownMimeType::TDesktopPalette),
        "jpg" | "jpeg" => MimeType::from_mime_string("image/jpeg"),
        "png" => MimeType::from_mime_string("image/png"),
        "gif" => MimeType::from_mime_string("image/gif"),
        "mp4" => MimeType::from_mime_string("video/mp4"),
        "mov" => MimeType::from_mime_string("video/quicktime"),
        "webm" => MimeType::from_mime_string("video/webm"),
        "mp3" => MimeType::from_mime_string("audio/mp3"),
        "ogg" | "opus" => MimeType::from_mime_string("audio/ogg"),
        "pdf" => MimeType::from_mime_string("application/pdf"),
        "zip" => MimeType::from_mime_string("application/zip"),
        _ => {
            // Check magic bytes if file exists
            if let Ok(mut file) = std::fs::File::open(path) {
                use std::io::Read;
                let mut buf = [0u8; 12];
                if file.read_exact(&mut buf).is_ok() {
                    return mime_type_for_data(&buf);
                }
            }
            MimeType::default()
        }
    }
}

pub fn mime_type_for_data(data: &[u8]) -> MimeType {
    if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        return MimeType::new(KnownMimeType::WebP);
    }
    if data.len() >= 3 && &data[0..3] == b"\x1f\x8b\x08" {
        // Gzip header (often .tgs sticker)
        return MimeType::new(KnownMimeType::Tgs);
    }
    if data.len() >= 4 && &data[0..4] == b"%PDF" {
        return MimeType::from_mime_string("application/pdf");
    }
    if data.len() >= 8 && &data[0..8] == b"\x89PNG\r\n\x1a\n" {
        return MimeType::from_mime_string("image/png");
    }
    if data.len() >= 3 && &data[0..3] == b"\xff\xd8\xff" {
        return MimeType::from_mime_string("image/jpeg");
    }
    if data.len() >= 6 && (&data[0..6] == b"GIF87a" || &data[0..6] == b"GIF89a") {
        return MimeType::from_mime_string("image/gif");
    }
    MimeType::default()
}

pub fn file_extension(filepath: &str) -> String {
    let path = Path::new(filepath);
    path.extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

pub fn detect_name_type(filepath: &str) -> NameType {
    static EXT_MAP: OnceLock<ExtensionMap> = OnceLock::new();
    let map = EXT_MAP.get_or_init(init_extension_map);
    let ext = file_extension(filepath);

    if ext.is_empty() {
        return NameType::Unknown;
    }

    if map.executable.contains(&ext) {
        NameType::Executable
    } else if map.image.contains(&ext) {
        NameType::Image
    } else if map.video.contains(&ext) {
        NameType::Video
    } else if map.audio.contains(&ext) {
        NameType::Audio
    } else if map.document.contains(&ext) {
        NameType::Document
    } else if map.archive.contains(&ext) {
        NameType::Archive
    } else if map.theme.contains(&ext) {
        NameType::ThemeFile
    } else if map.other_benign.contains(&ext) {
        NameType::OtherBenign
    } else {
        NameType::Unknown
    }
}

pub fn name_type_allows_thumbnail(name_type: NameType) -> bool {
    matches!(
        name_type,
        NameType::Image | NameType::Video | NameType::Audio | NameType::Document | NameType::ThemeFile
    )
}

pub fn is_ip_revealing_path(filepath: &str) -> bool {
    static REVEALING: OnceLock<HashSet<&'static str>> = OnceLock::new();
    let set = REVEALING.get_or_init(|| {
        [
            "htm", "html", "svg", "m4v", "m3u", "m3u8", "xhtml", "xml", "kml", "kmz", "xspf",
            "wpl", "docx", "dotx", "docm", "dotm", "xlsx", "xltx", "xlsm", "xltm", "xlsb",
            "pptx", "ppsx", "potx", "pptm", "ppsm", "potm",
        ]
        .into_iter()
        .collect()
    });

    let ext = file_extension(filepath);
    set.contains(ext.as_str())
}

pub fn is_mime_sticker_lottie(mime: &str) -> bool {
    mime == "application/x-tgsticker"
}

pub fn is_mime_sticker_webm(mime: &str) -> bool {
    mime == "video/webm"
}

pub fn is_mime_sticker_animated(mime: &str) -> bool {
    mime == "application/x-tgsticker"
}

pub fn is_mime_sticker(mime: &str) -> bool {
    mime == "image/webp" || is_mime_sticker_animated(mime) || is_mime_sticker_webm(mime)
}

pub fn is_mime_sent_as_video(mime: &str) -> bool {
    mime == "video/mp4" || mime == "video/quicktime"
}

pub fn is_mime_accepted_for_photo_video_album(mime: &str) -> bool {
    mime == "image/jpeg" || mime == "image/png" || is_mime_sent_as_video(mime)
}

pub fn file_is_image(name: &str, mime: &str) -> bool {
    if name.is_empty() {
        mime.to_ascii_lowercase().starts_with("image/")
    } else {
        detect_name_type(name) == NameType::Image
    }
}

struct ExtensionMap {
    executable: HashSet<String>,
    image: HashSet<String>,
    video: HashSet<String>,
    audio: HashSet<String>,
    document: HashSet<String>,
    archive: HashSet<String>,
    theme: HashSet<String>,
    other_benign: HashSet<String>,
}

fn init_extension_map() -> ExtensionMap {
    let parse = |s: &str| -> HashSet<String> {
        s.split_whitespace().map(|x| x.to_string()).collect()
    };

    let executable = parse(
        "exe bat cmd com msi vbs ps1 reg scr lnk dll cpl sh bin run app command \
         terminal workflow applescript action pkg mpkg deb rpm csh ksh out",
    );
    let image = parse(
        "jpg jpeg png gif webp svg bmp ico tiff tif heic avif jxl psd raw dng \
         sketch ai afdesign icns jfif qoi",
    );
    let video = parse("mp4 mov avi mkv webm flv 3gp 3gpp 3g2 m4v mpeg mpg ogv vob wmv tgs tgv");
    let audio = parse("mp3 ogg opus wav flac m4a aac wma mid midi amr caf aif aac ac3");
    let document = parse(
        "pdf doc docx xls xlsx ppt pptx txt rtf odt ods odp csv epub mobi fb2 \
         djvu log tex kml kmz m3u m3u8 htm html xhtml key",
    );
    let archive = parse("zip rar 7z tar gz bz2 xz zst z arj");
    let theme = parse("tdesktop-theme tdesktop-palette tgios-theme attheme");
    let other_benign = parse(
        "c cc cpp h hpp rs go py js ts json toml yaml yml xml css md sql java \
         swift patch diff",
    );

    ExtensionMap {
        executable,
        image,
        video,
        audio,
        document,
        archive,
        theme,
        other_benign,
    }
}
