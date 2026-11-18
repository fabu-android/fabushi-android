//! Telegram Desktop 1:1 Parity Module: `DeepLinksTypesDeepLinks`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/core/deep_links/deep_links_types.h

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Result of deep link dispatching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DeepLinkResult {
    #[default]
    Handled,
    NotFound,
    NeedsAuth,
    Unsupported,
}

/// Parsed command context passed to deep link route handlers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DeepLinkContext {
    pub section: String,
    pub path: String,
    pub params: HashMap<String, String>,
    pub has_active_session: bool,
}

impl DeepLinkContext {
    pub fn param(&self, key: &str) -> Option<&str> {
        self.params.get(&key.to_ascii_lowercase()).map(|s| s.as_str())
    }

    pub fn param_or<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.param(key).unwrap_or(default)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeepLinkSettingsSection {
    pub section_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeepLinkSettingsControl {
    pub section_id: String,
    pub control_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeepLinkAliasTo {
    pub section: String,
    pub path: String,
}

/// Semantic action executed when a deep link route matches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeepLinkAction {
    SettingsSection(DeepLinkSettingsSection),
    SettingsControl(DeepLinkSettingsControl),
    HandlerFunction(String),
    AliasTo(DeepLinkAliasTo),
}

/// Registered route entry with auth and activation metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeepLinkEntry {
    pub path: String,
    pub action: DeepLinkAction,
    pub requires_auth: bool,
    pub skip_activation: bool,
}

/// Primary container type for deep link types and route definitions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DeepLinksTypesDeepLinks;
