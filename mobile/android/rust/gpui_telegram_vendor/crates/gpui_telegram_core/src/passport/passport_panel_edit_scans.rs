//! Telegram Passport Panel Edit Scans & Attachments
//! Upstream: `Telegram/SourceFiles/passport/passport_panel_edit_scans.h`, `passport_panel_edit_scans.cpp`
//! Subsystem: `passport`, Feature ID: `TCG-N02`

use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};
use super::passport_form_controller::FileType;

pub const MAX_SCAN_FILE_SIZE: usize = 10 * 1024 * 1024; // 10MB limit

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanItem {
    pub file_id: u64,
    pub file_type: FileType,
    pub title: String,
    pub file_size: usize,
    pub status: String,
    pub thumbnail: Option<Vec<u8>>,
    pub is_deleted: bool,
    pub error: Option<String>,
}

/// Controller managing document scans, translation pages, and special sides (front, reverse, selfie).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PassportPanelEditScans {
    pub header: String,
    pub error_missing: Option<String>,
    pub scans: Vec<ScanItem>,
    pub translations: Vec<ScanItem>,
    pub special_scans: BTreeMap<FileType, ScanItem>,
    pub has_changes: bool,
}

impl Default for PassportPanelEditScans {
    fn default() -> Self {
        Self {
            header: "Document Scans".to_string(),
            error_missing: None,
            scans: Vec::new(),
            translations: Vec::new(),
            special_scans: BTreeMap::new(),
            has_changes: false,
        }
    }
}

impl PassportPanelEditScans {
    pub fn new(header: impl Into<String>) -> Self {
        Self {
            header: header.into(),
            error_missing: None,
            scans: Vec::new(),
            translations: Vec::new(),
            special_scans: BTreeMap::new(),
            has_changes: false,
        }
    }

    pub fn add_scan(&mut self, scan: ScanItem) -> Result<(), String> {
        if scan.file_size > MAX_SCAN_FILE_SIZE {
            return Err("File size exceeds 10MB limit".to_string());
        }
        match scan.file_type {
            FileType::Scan => self.scans.push(scan),
            FileType::Translation => self.translations.push(scan),
            FileType::FrontSide | FileType::ReverseSide | FileType::Selfie => {
                self.special_scans.insert(scan.file_type, scan);
            }
        }
        self.has_changes = true;
        Ok(())
    }

    pub fn delete_scan(&mut self, file_id: u64) -> bool {
        for scan in self.scans.iter_mut().chain(self.translations.iter_mut()) {
            if scan.file_id == file_id {
                scan.is_deleted = true;
                self.has_changes = true;
                return true;
            }
        }
        for (_, scan) in self.special_scans.iter_mut() {
            if scan.file_id == file_id {
                scan.is_deleted = true;
                self.has_changes = true;
                return true;
            }
        }
        false
    }

    pub fn restore_scan(&mut self, file_id: u64) -> bool {
        for scan in self.scans.iter_mut().chain(self.translations.iter_mut()) {
            if scan.file_id == file_id {
                scan.is_deleted = false;
                self.has_changes = true;
                return true;
            }
        }
        for (_, scan) in self.special_scans.iter_mut() {
            if scan.file_id == file_id {
                scan.is_deleted = false;
                self.has_changes = true;
                return true;
            }
        }
        false
    }

    pub fn active_scans_count(&self) -> usize {
        self.scans.iter().filter(|s| !s.is_deleted).count()
            + self.special_scans.values().filter(|s| !s.is_deleted).count()
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.active_scans_count() == 0 {
            return Err("At least one document scan must be uploaded".to_string());
        }
        Ok(())
    }
}
