//! Telegram Test Harness: Simulated System Clipboard Mock
//! Upstream sources: `Telegram/SourceFiles/test/test_clipboard.cpp`

use serde::{Deserialize, Serialize};

/// In-memory clipboard simulator verifying copy/paste text and image transfers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestClipboard {
    pub text_content: Option<String>,
    pub image_bytes: Option<Vec<u8>>,
    pub copy_count: u32,
    pub paste_count: u32,
}

impl TestClipboard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text_content = Some(text.into());
        self.copy_count = self.copy_count.saturating_add(1);
    }

    pub fn get_text(&mut self) -> Option<String> {
        self.paste_count = self.paste_count.saturating_add(1);
        self.text_content.clone()
    }

    pub fn set_image(&mut self, bytes: Vec<u8>) {
        self.image_bytes = Some(bytes);
        self.copy_count = self.copy_count.saturating_add(1);
    }

    pub fn clear(&mut self) {
        self.text_content = None;
        self.image_bytes = None;
    }
}
