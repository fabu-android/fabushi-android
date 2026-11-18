//! Telegram Desktop Base System & Event Integration Layer
//! Upstream sources: `Telegram/SourceFiles/core/base_integration.h`, `base_integration.cpp`

use std::sync::{Arc, Mutex};

/// Low-level platform logging, crash annotations, and assertion hook integration.
#[derive(Debug, Clone, Default)]
pub struct BaseIntegration {
    pub arguments: Vec<String>,
    pub skip_debug_log: bool,
    pub log_messages: Arc<Mutex<Vec<String>>>,
}

impl BaseIntegration {
    pub fn new(arguments: Vec<String>) -> Self {
        Self {
            arguments,
            skip_debug_log: false,
            log_messages: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn log_message(&self, message: &str) {
        if let Ok(mut logs) = self.log_messages.lock() {
            logs.push(format!("[INFO] {}", message));
        }
    }

    pub fn log_message_debug(&self, message: &str) {
        if !self.skip_debug_log {
            if let Ok(mut logs) = self.log_messages.lock() {
                logs.push(format!("[DEBUG] {}", message));
            }
        }
    }

    pub fn log_assertion_violation(&self, info: &str) {
        eprintln!("[ASSERTION VIOLATION] {}", info);
        if let Ok(mut logs) = self.log_messages.lock() {
            logs.push(format!("[ASSERTION VIOLATION] {}", info));
        }
    }
}
