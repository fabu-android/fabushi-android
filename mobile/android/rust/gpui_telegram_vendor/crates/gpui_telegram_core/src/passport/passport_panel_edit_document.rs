//! Telegram Passport Panel Edit Document Fields & Native Names
//! Upstream: `Telegram/SourceFiles/passport/passport_panel_edit_document.h`, `passport_panel_edit_document.cpp`
//! Subsystem: `passport`, Feature ID: `TCG-N02`

use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditDocumentValueClass {
    Fields,
    Additional,
    Scans,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdditionalVisibility {
    Hidden,
    OnlyIfError,
    Shown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditDocumentSchemeRow {
    pub value_class: EditDocumentValueClass,
    pub key: String,
    pub label: String,
    pub length_limit: usize,
    pub key_for_attachment_to: Option<String>,
    pub additional_fallback_key: Option<String>,
}

/// View model and field validation for identity and address document forms.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PassportPanelEditDocument {
    pub rows: Vec<EditDocumentSchemeRow>,
    pub fields: BTreeMap<String, String>,
    pub errors: BTreeMap<String, String>,
    pub has_unsaved_changes: bool,
    pub additional_visibility: AdditionalVisibility,
}

impl Default for PassportPanelEditDocument {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            fields: BTreeMap::new(),
            errors: BTreeMap::new(),
            has_unsaved_changes: false,
            additional_visibility: AdditionalVisibility::Hidden,
        }
    }
}

impl PassportPanelEditDocument {
    pub fn new(rows: Vec<EditDocumentSchemeRow>) -> Self {
        Self {
            rows,
            fields: BTreeMap::new(),
            errors: BTreeMap::new(),
            has_unsaved_changes: false,
            additional_visibility: AdditionalVisibility::Hidden,
        }
    }

    pub fn set_field(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let k = key.into();
        let v = value.into();
        if self.fields.get(&k) != Some(&v) {
            self.fields.insert(k, v);
            self.has_unsaved_changes = true;
        }
    }

    pub fn get_field(&self, key: &str) -> Option<&str> {
        self.fields.get(key).map(|s| s.as_str())
    }

    pub fn fill_additional_from_fallbacks(&mut self) {
        let mut fallbacks = Vec::new();
        for row in &self.rows {
            if let Some(fallback_key) = &row.additional_fallback_key {
                if !self.fields.contains_key(&row.key) {
                    if let Some(fallback_val) = self.fields.get(fallback_key) {
                        fallbacks.push((row.key.clone(), fallback_val.clone()));
                    }
                }
            }
        }
        for (k, v) in fallbacks {
            self.fields.insert(k, v);
        }
    }

    pub fn validate(&mut self) -> bool {
        self.errors.clear();
        for row in &self.rows {
            if row.value_class == EditDocumentValueClass::Additional
                && self.additional_visibility == AdditionalVisibility::Hidden
            {
                continue;
            }
            let val = self.fields.get(&row.key).map(|s| s.trim()).unwrap_or("");
            if val.is_empty() {
                self.errors.insert(row.key.clone(), format!("{} cannot be empty", row.label));
            } else if row.length_limit > 0 && val.len() > row.length_limit {
                self.errors.insert(
                    row.key.clone(),
                    format!("{} exceeds {} characters", row.label, row.length_limit),
                );
            }
        }
        self.errors.is_empty()
    }
}
