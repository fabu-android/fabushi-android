use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkedUploadInitRequest {
    pub size_bytes: u64,
    pub filename: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chunk_size: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkedUploadStatusResponse {
    pub upload_id: String,
    pub total_size: u64,
    pub part_size: u32,
    pub total_parts: u32,
    pub uploaded_parts: Vec<bool>,
    pub missing_parts: Vec<u32>,
    pub is_complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaUploadInitResponse {
    pub upload_id: String,
    pub chunk_size: u64,
    #[serde(default)]
    pub expires_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaUploadCompleteResponse {
    pub file_id: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub mime_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waveform_b64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waveform_bars: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_data_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StickerItemDto {
    pub id: String,
    pub set_id: String,
    pub emoji: String,
    #[serde(default)]
    pub file_id: Option<String>,
    pub mime_type: String,
    pub width: u32,
    pub height: u32,
    pub is_animated: bool,
    pub is_video: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StickerSetDto {
    pub set_id: String,
    pub short_name: String,
    pub title: String,
    pub count: u32,
    pub is_installed: bool,
    #[serde(default)]
    pub stickers: Vec<StickerItemDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GifResultDto {
    pub id: String,
    pub title: String,
    pub file_id: String,
    pub url: String,
    pub preview_url: String,
    pub width: u32,
    pub height: u32,
    pub mime_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoryDto {
    pub story_id: String,
    pub user_id: u64,
    #[serde(default)]
    pub username: Option<String>,
    pub first_name: String,
    #[serde(default)]
    pub media_id: Option<String>,
    pub media_type: String,
    #[serde(default)]
    pub caption: Option<String>,
    pub privacy: String,
    pub views_count: u32,
    pub is_viewed: bool,
    pub created_at_ms: i64,
    pub expires_at_ms: i64,
}
