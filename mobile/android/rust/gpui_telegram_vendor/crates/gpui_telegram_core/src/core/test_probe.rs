//! Telegram Desktop 1:1 Parity Module: `TestProbe`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_probe.h, Telegram/SourceFiles/test/test_probe.cpp

use serde::{Deserialize, Serialize};

/// Diagnostic probe for runtime assertion and health inspection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestProbe {
    pub probe_name: String,
    pub samples_count: usize,
    pub last_sample: Option<i64>,
}

impl Default for TestProbe {
    fn default() -> Self {
        Self {
            probe_name: "generic_probe".to_string(),
            samples_count: 0,
            last_sample: None,
        }
    }
}

impl TestProbe {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            probe_name: name.into(),
            samples_count: 0,
            last_sample: None,
        }
    }

    pub fn sample(&mut self, value: i64) {
        self.samples_count += 1;
        self.last_sample = Some(value);
    }
}
