use crate::cards::{InvoiceCard, PollCard, TodoCard};
use crate::e2ee::EncryptedEnvelope;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum MessageEntityType {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    Spoiler,
    Code,
    Pre { language: Option<String> },
    Url,
    TextUrl { url: String },
    Mention,
    Hashtag,
    BotCommand,
    CustomEmoji { document_id: u64 },
}

pub type MessageEntityKind = MessageEntityType;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageEntity {
    /// Offset in UTF-16 code units from the start of the message text.
    pub offset: u32,
    /// Length of the entity in UTF-16 code units.
    pub length: u32,
    pub entity_type: MessageEntityType,
}

impl MessageEntity {
    pub fn new(offset: u32, length: u32, entity_type: MessageEntityType) -> Self {
        Self {
            offset,
            length,
            entity_type,
        }
    }

    /// Extracts the UTF-16 code units corresponding to this entity from a UTF-16 slice.
    pub fn extract_utf16<'a>(&self, utf16_units: &'a [u16]) -> Option<&'a [u16]> {
        let start = self.offset as usize;
        let end = start.checked_add(self.length as usize)?;
        if end <= utf16_units.len() {
            Some(&utf16_units[start..end])
        } else {
            None
        }
    }

    /// Extracts the text covered by this entity from a string slice, respecting UTF-16 code-unit boundaries.
    pub fn extract_text(&self, text: &str) -> Option<String> {
        let utf16: Vec<u16> = text.encode_utf16().collect();
        let slice = self.extract_utf16(&utf16)?;
        String::from_utf16(slice).ok()
    }

    /// Creates an entity from a substring of a text, computing UTF-16 code units offsets.
    /// Handles both direct memory subslices and string pattern matches.
    pub fn from_str_subslice(full_text: &str, subslice: &str, entity_type: MessageEntityType) -> Option<Self> {
        let full_ptr = full_text.as_ptr() as usize;
        let sub_ptr = subslice.as_ptr() as usize;
        let byte_offset = if sub_ptr >= full_ptr && sub_ptr + subslice.len() <= full_ptr + full_text.len() {
            sub_ptr - full_ptr
        } else {
            full_text.find(subslice)?
        };
        let prefix = &full_text[..byte_offset];
        let offset = prefix.encode_utf16().count() as u32;
        let length = subslice.encode_utf16().count() as u32;
        Some(Self::new(offset, length, entity_type))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaThumbnail {
    pub file_id: String,
    pub width: u32,
    pub height: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_base64: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaItem {
    pub file_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    pub mime_type: String,
    pub size_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<MediaThumbnail>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceItem {
    pub file_id: String,
    pub duration_seconds: u32,
    pub waveform: Vec<u8>,
    pub mime_type: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoNoteItem {
    pub file_id: String,
    pub duration_seconds: u32,
    pub diameter: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<MediaThumbnail>,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum MessageContent {
    Text {
        text: String,
        #[serde(default)]
        entities: Vec<MessageEntity>,
    },
    Media(MediaItem),
    Voice(VoiceItem),
    VideoNote(VideoNoteItem),
    Todo(TodoCard),
    Poll(PollCard),
    Invoice(InvoiceCard),
    GenerativeCard(crate::cards::GenerativeCard),
    Photo {
        file_id: String,
        caption: Option<String>,
        width: u32,
        height: u32,
    },
    Video {
        file_id: String,
        duration: u32,
        width: u32,
        height: u32,
        caption: Option<String>,
    },
    Audio {
        file_id: String,
        duration: u32,
        title: Option<String>,
        performer: Option<String>,
    },
    Document {
        file_id: String,
        file_name: String,
        mime_type: String,
        size_bytes: u64,
    },
    Encrypted(EncryptedEnvelope),
}

impl MessageContent {
    pub fn plain_text(text: impl Into<String>) -> Self {
        Self::Text {
            text: text.into(),
            entities: Vec::new(),
        }
    }

    pub fn text_with_entities(text: impl Into<String>, entities: Vec<MessageEntity>) -> Self {
        Self::Text {
            text: text.into(),
            entities,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_text_content_omitting_entities() {
        let json_str = r#"{"type":"Text","data":{"text":"Hello world without entities"}}"#;
        let content: MessageContent = serde_json::from_str(json_str)
            .expect("Should deserialize text content when entities field is omitted");
        match content {
            MessageContent::Text { text, entities } => {
                assert_eq!(text, "Hello world without entities");
                assert!(entities.is_empty());
            }
            _ => panic!("Expected MessageContent::Text variant"),
        }
    }
}
