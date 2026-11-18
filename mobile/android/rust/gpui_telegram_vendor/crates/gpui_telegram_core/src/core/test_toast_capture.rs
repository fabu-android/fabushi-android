//! Telegram Desktop 1:1 Parity Module: `TestToastCapture`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_toast_capture.h, Telegram/SourceFiles/test/test_toast_capture.cpp

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapturedToast {
    pub text: String,
    pub duration_ms: u32,
}

/// Toast notification capture fixture for automated UI verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TestToastCapture {
    pub toasts: Vec<CapturedToast>,
}

impl TestToastCapture {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn capture(&mut self, text: impl Into<String>, duration_ms: u32) {
        self.toasts.push(CapturedToast {
            text: text.into(),
            duration_ms,
        });
    }

    pub fn has_toast_containing(&self, substring: &str) -> bool {
        self.toasts.iter().any(|t| t.text.contains(substring))
    }
}
