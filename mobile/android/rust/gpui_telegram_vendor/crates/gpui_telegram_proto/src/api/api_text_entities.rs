//! Telegram Desktop 1:1 Parity Submodule: `ApiTextEntities`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_text_entities.cpp, Telegram/SourceFiles/api/api_text_entities.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};
use crate::message::{MessageEntity, MessageEntityType};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ConvertOption {
    #[default]
    WithLocal,
    SkipLocal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TextWithEntities {
    pub text: String,
    pub entities: Vec<MessageEntity>,
}

impl TextWithEntities {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            entities: Vec::new(),
        }
    }

    pub fn with_entity(mut self, entity: MessageEntity) -> Self {
        self.entities.push(entity);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiTextEntities {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub parsed_cache: Vec<TextWithEntities>,
    pub updated_at: i64,
}

impl Default for ApiTextEntities {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiTextEntities".to_string(),
            parsed_cache: Vec::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiTextEntities {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_label(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ..Self::default()
        }
    }

    pub fn is_active(&self) -> bool {
        self.is_enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.is_enabled = enabled;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn parse_markdown_spoilers(text: &str) -> TextWithEntities {
        let mut clean_text = String::new();
        let mut entities = Vec::new();
        let mut chars = text.chars().peekable();
        let mut in_spoiler = false;
        let mut spoiler_start_offset = 0u32;
        let mut current_offset = 0u32;
        let mut spoiler_buffer = String::new();

        while let Some(ch) = chars.next() {
            if ch == '|' && chars.peek() == Some(&'|') {
                chars.next(); // Consume second '|'
                if in_spoiler {
                    let len: u32 = spoiler_buffer.chars().map(|c| c.len_utf16() as u32).sum();
                    clean_text.push_str(&spoiler_buffer);
                    current_offset += len;
                    if len > 0 {
                        entities.push(MessageEntity::new(spoiler_start_offset, len, MessageEntityType::Spoiler));
                    }
                    spoiler_buffer.clear();
                    in_spoiler = false;
                } else {
                    spoiler_start_offset = current_offset;
                    in_spoiler = true;
                }
            } else if in_spoiler {
                spoiler_buffer.push(ch);
            } else {
                clean_text.push(ch);
                current_offset += ch.len_utf16() as u32;
            }
        }

        if in_spoiler {
            // Unclosed spoiler: preserve opening '||' verbatim and buffer contents
            clean_text.push_str("||");
            clean_text.push_str(&spoiler_buffer);
        }

        TextWithEntities {
            text: clean_text,
            entities,
        }
    }

    pub fn filter_entities(entities: &[MessageEntity], option: ConvertOption) -> Vec<MessageEntity> {
        match option {
            ConvertOption::WithLocal => entities.to_vec(),
            ConvertOption::SkipLocal => entities
                .iter()
                .filter(|e| !matches!(e.entity_type, MessageEntityType::CustomEmoji { document_id: 0 }))
                .cloned()
                .collect(),
        }
    }
}
