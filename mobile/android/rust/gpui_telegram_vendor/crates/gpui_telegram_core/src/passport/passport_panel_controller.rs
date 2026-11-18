//! Telegram Passport Panel Controller & Document Schemes
//! Upstream: `Telegram/SourceFiles/passport/passport_panel_controller.h`, `passport_panel_controller.cpp`
//! Subsystem: `passport`, Feature ID: `TCG-N02`

use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReadScanError {
    FileTooLarge,
    CantReadImage,
    BadImageSize,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScopeErrorGeneral {
    WholeValue,
    ScanMissing,
    TranslationMissing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScopeErrorKey {
    FileId(u64),
    General(ScopeErrorGeneral),
    FieldKey(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeError {
    pub key: ScopeErrorKey,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditDocumentCountry {
    pub country_code: String,
    pub language_code: String,
}

/// Orchestrator bridging form data models, document editing schemes and presentation panel.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PassportPanelController {
    pub active_scope_index: Option<usize>,
    pub scope_errors: Vec<ScopeError>,
    pub password_hint: Option<String>,
    pub unconfirmed_email_pattern: Option<String>,
    pub preferred_languages: BTreeMap<String, String>,
}

impl PassportPanelController {
    pub fn new() -> Self {
        let mut preferred = BTreeMap::new();
        preferred.insert("RU".to_string(), "ru".to_string());
        preferred.insert("UA".to_string(), "uk".to_string());
        preferred.insert("BY".to_string(), "be".to_string());
        preferred.insert("KZ".to_string(), "kk".to_string());
        Self {
            active_scope_index: None,
            scope_errors: Vec::new(),
            password_hint: None,
            unconfirmed_email_pattern: None,
            preferred_languages: preferred,
        }
    }

    pub fn set_active_scope(&mut self, index: usize) {
        self.active_scope_index = Some(index);
    }

    pub fn clear_active_scope(&mut self) {
        self.active_scope_index = None;
    }

    pub fn add_scope_error(&mut self, key: ScopeErrorKey, message: impl Into<String>) {
        self.scope_errors.push(ScopeError {
            key,
            message: message.into(),
        });
    }

    pub fn clear_errors(&mut self) {
        self.scope_errors.clear();
    }

    pub fn preferred_language_for_country(&self, country_code: &str) -> EditDocumentCountry {
        let lang = self
            .preferred_languages
            .get(country_code)
            .cloned()
            .unwrap_or_else(|| "en".to_string());
        EditDocumentCountry {
            country_code: country_code.to_string(),
            language_code: lang,
        }
    }

    /// Transliteration helper from Cyrillic native names to Latin script.
    pub fn native_to_latin(text: &str) -> String {
        let mut result = String::with_capacity(text.len() * 2);
        for c in text.chars() {
            let latin = match c {
                'А' | 'а' => "a", 'Б' | 'б' => "b", 'В' | 'в' => "v", 'Г' | 'г' => "g",
                'Д' | 'д' => "d", 'Е' | 'е' => "e", 'Ё' | 'ё' => "yo", 'Ж' | 'ж' => "zh",
                'З' | 'з' => "z", 'И' | 'и' => "i", 'Й' | 'й' => "y", 'К' | 'к' => "k",
                'Л' | 'л' => "l", 'М' | 'м' => "m", 'Н' | 'н' => "n", 'О' | 'о' => "o",
                'П' | 'п' => "p", 'Р' | 'р' => "r", 'С' | 'с' => "s", 'Т' | 'т' => "t",
                'У' | 'у' => "u", 'Ф' | 'ф' => "f", 'Х' | 'х' => "kh", 'Ц' | 'ц' => "ts",
                'Ч' | 'ч' => "ch", 'Ш' | 'ш' => "sh", 'Щ' | 'щ' => "shch", 'Ъ' | 'ъ' => "",
                'Ы' | 'ы' => "y", 'Ь' | 'ь' => "", 'Э' | 'э' => "e", 'Ю' | 'ю' => "yu",
                'Я' | 'я' => "ya",
                other => {
                    result.push(other);
                    continue;
                }
            };
            if c.is_uppercase() {
                let mut chars = latin.chars();
                if let Some(first) = chars.next() {
                    result.push(first.to_ascii_uppercase());
                    result.extend(chars);
                }
            } else {
                result.push_str(latin);
            }
        }
        result
    }
}
