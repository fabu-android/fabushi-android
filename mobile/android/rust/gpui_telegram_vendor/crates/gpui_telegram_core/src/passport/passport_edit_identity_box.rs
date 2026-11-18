//! Telegram Passport Edit Identity Box Dialog
//! Upstream: `Telegram/SourceFiles/passport/passport_edit_identity_box.h`, `passport_edit_identity_box.cpp`
//! Subsystem: `passport`, Feature ID: `TCG-N02`

use serde::{Deserialize, Serialize};

/// Identity details payload containing user full legal name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct IdentityData {
    pub name: String,
    pub surname: String,
    pub middle_name: Option<String>,
}

/// Metadata and state for an uploaded identity document scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanInfo {
    pub file_id: u64,
    pub file_name: String,
    pub file_size: usize,
    pub mime_type: String,
    pub thumbnail: Option<Vec<u8>>,
    pub status: String,
    pub is_deleted: bool,
    pub error: Option<String>,
}

impl Default for ScanInfo {
    fn default() -> Self {
        Self {
            file_id: 0,
            file_name: String::new(),
            file_size: 0,
            mime_type: "image/jpeg".to_string(),
            thumbnail: None,
            status: "ready".to_string(),
            is_deleted: false,
            error: None,
        }
    }
}

/// Interactive scan preview item with delete/restore toggle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanButton {
    pub title: String,
    pub description: String,
    pub has_image: bool,
    pub is_deleted: bool,
}

/// Modal box state for editing identity documents and associated document scans.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PassportEditIdentityBox {
    pub value_index: i32,
    pub data: IdentityData,
    pub files: Vec<ScanInfo>,
    pub scans: Vec<ScanButton>,
    pub has_changes: bool,
    pub is_submitting: bool,
}

impl Default for PassportEditIdentityBox {
    fn default() -> Self {
        Self {
            value_index: -1,
            data: IdentityData::default(),
            files: Vec::new(),
            scans: Vec::new(),
            has_changes: false,
            is_submitting: false,
        }
    }
}

impl PassportEditIdentityBox {
    pub fn new(value_index: i32, data: IdentityData, files: Vec<ScanInfo>) -> Self {
        let scans = files
            .iter()
            .map(|f| ScanButton {
                title: f.file_name.clone(),
                description: format!("{} bytes", f.file_size),
                has_image: f.thumbnail.is_some(),
                is_deleted: f.is_deleted,
            })
            .collect();
        Self {
            value_index,
            data,
            files,
            scans,
            has_changes: false,
            is_submitting: false,
        }
    }

    pub fn update_name(&mut self, name: String, surname: String) {
        if self.data.name != name || self.data.surname != surname {
            self.data.name = name;
            self.data.surname = surname;
            self.has_changes = true;
        }
    }

    pub fn add_scan(&mut self, scan: ScanInfo) {
        self.scans.push(ScanButton {
            title: scan.file_name.clone(),
            description: format!("{} bytes", scan.file_size),
            has_image: scan.thumbnail.is_some(),
            is_deleted: false,
        });
        self.files.push(scan);
        self.has_changes = true;
    }

    pub fn delete_scan(&mut self, file_id: u64) -> bool {
        if let Some(pos) = self.files.iter().position(|f| f.file_id == file_id) {
            self.files[pos].is_deleted = true;
            if pos < self.scans.len() {
                self.scans[pos].is_deleted = true;
            }
            self.has_changes = true;
            return true;
        }
        false
    }

    pub fn restore_scan(&mut self, file_id: u64) -> bool {
        if let Some(pos) = self.files.iter().position(|f| f.file_id == file_id) {
            self.files[pos].is_deleted = false;
            if pos < self.scans.len() {
                self.scans[pos].is_deleted = false;
            }
            self.has_changes = true;
            return true;
        }
        false
    }

    pub fn active_scans_count(&self) -> usize {
        self.files.iter().filter(|f| !f.is_deleted).count()
    }

    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        if self.data.name.trim().is_empty() {
            errors.push("First name cannot be empty".to_string());
        }
        if self.data.surname.trim().is_empty() {
            errors.push("Last name cannot be empty".to_string());
        }
        if self.active_scans_count() == 0 {
            errors.push("At least one document scan must be uploaded".to_string());
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    pub fn count_height(&self, base_height: f32) -> f32 {
        let field_height = 48.0;
        let scan_row_height = 42.0;
        let padding = 32.0;
        base_height + (field_height * 2.0) + (self.scans.len() as f32 * scan_row_height) + padding
    }
}
