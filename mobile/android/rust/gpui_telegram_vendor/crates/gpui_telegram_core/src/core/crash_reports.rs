//! Telegram Desktop Crash Reporting Subsystem
//! Upstream sources: `Telegram/SourceFiles/core/crash_reports.h`, `crash_reports.cpp`

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrashReportStatus {
    CantOpen,
    Started,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashDump {
    pub timestamp: i64,
    pub platform: String,
    pub version: String,
    pub panic_message: String,
    pub annotations: HashMap<String, String>,
}

/// Global crash telemetry and breadcrumb collector.
#[derive(Debug, Clone, Default)]
pub struct CrashReports {
    pub status: Option<CrashReportStatus>,
    pub annotations: HashMap<String, String>,
}

impl CrashReports {
    pub fn new() -> Self {
        Self {
            status: Some(CrashReportStatus::Started),
            annotations: HashMap::new(),
        }
    }

    pub fn start() -> Self {
        let mut cr = Self::new();
        cr.status = Some(CrashReportStatus::Started);
        cr.set_annotation("platform", std::env::consts::OS);
        cr.set_annotation("arch", std::env::consts::ARCH);
        cr
    }

    pub fn set_annotation(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.annotations.insert(key.into(), value.into());
    }

    pub fn clear_annotation(&mut self, key: &str) {
        self.annotations.remove(key);
    }

    pub fn build_dump(&self, panic_message: &str) -> CrashDump {
        CrashDump {
            timestamp: chrono::Utc::now().timestamp(),
            platform: std::env::consts::OS.to_string(),
            version: crate::core::version::APP_VERSION_STR.to_string(),
            panic_message: panic_message.to_string(),
            annotations: self.annotations.clone(),
        }
    }

    pub fn serialize_dump(&self, dump: &CrashDump) -> Vec<u8> {
        serde_json::to_vec_pretty(dump).unwrap_or_default()
    }

    pub fn finish(&mut self) {
        self.status = None;
        self.annotations.clear();
    }
}
