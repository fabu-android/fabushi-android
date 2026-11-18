//! Telegram Desktop 1:1 Parity Module: `TestText`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/tests/test_text.cpp

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestTextEntity {
    pub offset: usize,
    pub length: usize,
    pub entity_type: String,
}

/// Rich text layout and entity parser verification fixture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TestText {
    pub raw_text: String,
    pub entities: Vec<TestTextEntity>,
}

impl TestText {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            raw_text: text.into(),
            entities: Vec::new(),
        }
    }

    pub fn add_entity(&mut self, offset: usize, length: usize, entity_type: impl Into<String>) {
        self.entities.push(TestTextEntity {
            offset,
            length,
            entity_type: entity_type.into(),
        });
    }

    pub fn utf16_len(&self) -> usize {
        self.raw_text.encode_utf16().count()
    }
}
