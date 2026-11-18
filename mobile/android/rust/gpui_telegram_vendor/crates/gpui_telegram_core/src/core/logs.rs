//! Telegram Desktop Structured Rotating Logger & MTP Packet Dump
//! Upstream sources: `Telegram/SourceFiles/logs.h`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    Main,
    Debug,
    Mtp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRecord {
    pub timestamp: i64,
    pub level: LogLevel,
    pub dc: Option<i32>,
    pub message: String,
}

/// Central application logger handling console and file logging.
#[derive(Debug, Clone, Default)]
pub struct Logs {
    pub debug_enabled: bool,
    pub is_started: bool,
    pub buffer: Vec<LogRecord>,
}

impl Logs {
    pub fn new() -> Self {
        Self {
            debug_enabled: false,
            is_started: false,
            buffer: Vec::new(),
        }
    }

    pub fn start(&mut self) {
        self.is_started = true;
    }

    pub fn finish(&mut self) {
        self.is_started = false;
    }

    pub fn set_debug_enabled(&mut self, enabled: bool) {
        self.debug_enabled = enabled;
    }

    pub fn write_main(&mut self, msg: impl Into<String>) {
        if !self.is_started {
            return;
        }
        self.buffer.push(LogRecord {
            timestamp: chrono::Utc::now().timestamp_millis(),
            level: LogLevel::Main,
            dc: None,
            message: msg.into(),
        });
    }

    pub fn write_debug(&mut self, msg: impl Into<String>) {
        if !self.is_started || !self.debug_enabled {
            return;
        }
        self.buffer.push(LogRecord {
            timestamp: chrono::Utc::now().timestamp_millis(),
            level: LogLevel::Debug,
            dc: None,
            message: msg.into(),
        });
    }

    pub fn write_mtp(&mut self, dc: i32, msg: impl Into<String>) {
        if !self.is_started {
            return;
        }
        self.buffer.push(LogRecord {
            timestamp: chrono::Utc::now().timestamp_millis(),
            level: LogLevel::Mtp,
            dc: Some(dc),
            message: msg.into(),
        });
    }

    pub fn format_hex(data: &[u8]) -> String {
        data.iter()
            .map(|b| format!("{:02X}", b))
            .collect::<Vec<String>>()
            .join(" ")
    }
}
