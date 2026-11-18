//! Telegram Desktop 1:1 Parity Module: `TestSecrecyScan`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_secrecy_scan.h, Telegram/SourceFiles/test/test_secrecy_scan.cpp

use serde::{Deserialize, Serialize};

/// Security scanner checking logs and memory buffers for leaked auth secrets or tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TestSecrecyScan {
    pub prohibited_patterns: Vec<String>,
    pub leaks_detected: Vec<String>,
}

impl TestSecrecyScan {
    pub fn new() -> Self {
        Self {
            prohibited_patterns: vec![
                "auth_key".to_string(),
                "session_token".to_string(),
                "password_hash".to_string(),
            ],
            leaks_detected: Vec::new(),
        }
    }

    pub fn scan_text(&mut self, text: &str) -> bool {
        let mut clean = true;
        for pattern in &self.prohibited_patterns {
            if text.contains(pattern) {
                self.leaks_detected.push(pattern.clone());
                clean = false;
            }
        }
        clean
    }
}
