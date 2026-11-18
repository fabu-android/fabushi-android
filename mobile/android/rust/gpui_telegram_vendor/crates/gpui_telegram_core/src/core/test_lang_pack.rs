//! Telegram Test Harness: Language Pack Localization Lookup & Plurals
//! Upstream sources: `Telegram/SourceFiles/test/test_lang_pack.cpp`

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Test fixture verifying localization string lookups and parameter interpolation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestLangPack {
    pub language_id: String,
    pub strings: HashMap<String, String>,
}

impl TestLangPack {
    pub fn new(lang_id: impl Into<String>) -> Self {
        Self {
            language_id: lang_id.into(),
            strings: HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.strings.insert(key.into(), value.into());
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.strings.get(key).map(|s| s.as_str())
    }

    pub fn format(&self, key: &str, arg: &str) -> String {
        if let Some(template) = self.strings.get(key) {
            template.replace("{0}", arg)
        } else {
            key.to_string()
        }
    }
}
