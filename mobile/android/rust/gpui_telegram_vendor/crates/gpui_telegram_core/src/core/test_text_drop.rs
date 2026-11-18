//! Telegram Desktop 1:1 Parity Module: `TestTextDrop`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_text_drop.h, Telegram/SourceFiles/test/test_text_drop.cpp

use serde::{Deserialize, Serialize};

/// Text drag & drop payload fixture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TestTextDrop {
    pub dropped_text: String,
    pub has_rich_formatting: bool,
}

impl TestTextDrop {
    pub fn new(text: impl Into<String>, has_formatting: bool) -> Self {
        Self {
            dropped_text: text.into(),
            has_rich_formatting: has_formatting,
        }
    }

    pub fn sanitize_text(&self) -> String {
        self.dropped_text.replace('\r', "")
    }
}
