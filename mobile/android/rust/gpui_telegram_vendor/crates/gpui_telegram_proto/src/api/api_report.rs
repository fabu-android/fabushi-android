//! Telegram Desktop 1:1 Parity Submodule: `ApiReport`
//! Subsystem: `api` | Feature ID: `TCG-N01`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/api/api_report.cpp, Telegram/SourceFiles/api/api_report.h
//! Inherited Paths: inherited/host/src/device_pairing_tokens.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ReportReason {
    #[default]
    Spam,
    Violence,
    Pornography,
    ChildAbuse,
    Copyright,
    Fake,
    GeoIrrelevant,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportSubmission {
    pub peer_id: u64,
    pub message_ids: Vec<i64>,
    pub reason: ReportReason,
    pub comment: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiReport {
    pub is_enabled: bool,
    pub version: u32,
    pub label: String,
    pub submitted_reports: Vec<ReportSubmission>,
    pub updated_at: i64,
}

impl Default for ApiReport {
    fn default() -> Self {
        Self {
            is_enabled: true,
            version: 1,
            label: "ApiReport".to_string(),
            submitted_reports: Vec::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl ApiReport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_label(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ..Self::default()
        }
    }

    pub fn is_active(&self) -> bool {
        self.is_enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.is_enabled = enabled;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn report_peer(&mut self, peer_id: u64, reason: ReportReason, comment: impl Into<String>) {
        self.submitted_reports.push(ReportSubmission {
            peer_id,
            message_ids: Vec::new(),
            reason,
            comment: comment.into(),
        });
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn report_messages(&mut self, peer_id: u64, message_ids: Vec<i64>, reason: ReportReason, comment: impl Into<String>) {
        self.submitted_reports.push(ReportSubmission {
            peer_id,
            message_ids,
            reason,
            comment: comment.into(),
        });
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
