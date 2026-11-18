//! Telegram Test Harness: Custom Emoji Document Rendering & Lookup
//! Upstream sources: `Telegram/SourceFiles/test/test_custom_emoji.cpp`

use serde::{Deserialize, Serialize};

/// Test fixture verifying custom animated emoji rendering pipelines and document resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCustomEmoji {
    pub document_id: u64,
    pub emoji_char: String,
    pub is_loaded: bool,
    pub frame_count: u32,
}

impl Default for TestCustomEmoji {
    fn default() -> Self {
        Self {
            document_id: 10001,
            emoji_char: "🔥".to_string(),
            is_loaded: false,
            frame_count: 60,
        }
    }
}

impl TestCustomEmoji {
    pub fn new(document_id: u64, emoji_char: impl Into<String>) -> Self {
        Self {
            document_id,
            emoji_char: emoji_char.into(),
            is_loaded: false,
            frame_count: 60,
        }
    }

    pub fn mark_loaded(&mut self) {
        self.is_loaded = true;
    }
}
