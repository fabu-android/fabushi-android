//! Telegram Desktop 1:1 Parity Module: `TestTextReads`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_text_reads.h, Telegram/SourceFiles/test/test_text_reads.cpp

use serde::{Deserialize, Serialize};

/// Accessibility label text reader fixture for screen reader verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TestTextReads {
    pub label_text: String,
    pub accessibility_name: String,
}

impl TestTextReads {
    pub fn new(label: impl Into<String>) -> Self {
        let text = label.into();
        let accessible = text.trim().to_string();
        Self {
            label_text: text,
            accessibility_name: accessible,
        }
    }
}
