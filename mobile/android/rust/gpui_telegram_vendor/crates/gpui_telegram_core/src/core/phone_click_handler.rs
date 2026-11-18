//! Telegram Desktop 1:1 Parity Module: `PhoneClickHandler`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/core/phone_click_handler.h, Telegram/SourceFiles/core/phone_click_handler.cpp

use serde::{Deserialize, Serialize};

/// Click handler for phone numbers detected in messages or profiles.
/// Supports E.164 normalization, international calling URI generation, and actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhoneClickHandler {
    pub raw_text: String,
    pub normalized_e164: String,
}

impl Default for PhoneClickHandler {
    fn default() -> Self {
        Self {
            raw_text: String::new(),
            normalized_e164: String::new(),
        }
    }
}

impl PhoneClickHandler {
    pub fn new(phone_text: impl Into<String>) -> Self {
        let raw = phone_text.into();
        let normalized = normalize_phone(&raw);
        Self {
            raw_text: raw,
            normalized_e164: normalized,
        }
    }

    pub fn phone_number(&self) -> &str {
        &self.normalized_e164
    }

    pub fn display_text(&self) -> &str {
        if self.raw_text.is_empty() {
            &self.normalized_e164
        } else {
            &self.raw_text
        }
    }

    pub fn is_valid_phone(&self) -> bool {
        let digits = self
            .normalized_e164
            .chars()
            .filter(|c| c.is_ascii_digit())
            .count();
        (7..=15).contains(&digits)
    }

    pub fn tel_uri(&self) -> String {
        format!("tel:{}", self.normalized_e164)
    }

    pub fn tg_call_uri(&self) -> String {
        format!("tg://resolve?phone={}", self.normalized_e164.trim_start_matches('+'))
    }

    pub fn tooltip(&self) -> String {
        format!("Call {}", self.normalized_e164)
    }
}

fn normalize_phone(raw: &str) -> String {
    let mut result = String::with_capacity(raw.len() + 1);
    let trimmed = raw.trim();
    if trimmed.starts_with('+') {
        result.push('+');
    }
    for ch in trimmed.chars() {
        if ch.is_ascii_digit() {
            result.push(ch);
        }
    }
    if !result.starts_with('+') && !result.is_empty() {
        result.insert(0, '+');
    }
    result
}
