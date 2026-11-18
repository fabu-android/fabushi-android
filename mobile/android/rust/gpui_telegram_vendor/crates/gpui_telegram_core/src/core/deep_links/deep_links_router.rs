//! Telegram Desktop 1:1 Parity Module: `DeepLinksRouterDeepLinks`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/core/deep_links/deep_links_router.h, Telegram/SourceFiles/core/deep_links/deep_links_router.cpp

use super::deep_links_types::{
    DeepLinkAction, DeepLinkContext, DeepLinkEntry, DeepLinkResult,
    DeepLinkSettingsControl, DeepLinkSettingsSection,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Primary Deep Links Router dispatching incoming `tg://` commands and internal links.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeepLinksRouterDeepLinks {
    pub routes: HashMap<String, Vec<DeepLinkEntry>>,
}

impl Default for DeepLinksRouterDeepLinks {
    fn default() -> Self {
        let mut router = Self {
            routes: HashMap::new(),
        };
        router.register_defaults();
        router
    }
}

impl DeepLinksRouterDeepLinks {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, section: impl Into<String>, entry: DeepLinkEntry) {
        self.routes
            .entry(section.into().to_ascii_lowercase())
            .or_default()
            .push(entry);
    }

    /// Parses command string into structured DeepLinkContext.
    /// E.g. "tg://resolve?domain=telegram&post=1" -> section: "resolve", params: { domain: "telegram", post: "1" }
    pub fn parse_command(command: &str, has_session: bool) -> DeepLinkContext {
        let trimmed = command
            .trim()
            .trim_start_matches("tg://")
            .trim_start_matches('/');

        let (path_part, query_part) = match trimmed.find('?') {
            Some(idx) => (&trimmed[..idx], &trimmed[idx + 1..]),
            None => (trimmed, ""),
        };

        let mut params = HashMap::new();
        if !query_part.is_empty() {
            for pair in query_part.split('&') {
                if pair.is_empty() {
                    continue;
                }
                let mut kv = pair.splitn(2, '=');
                let key = kv.next().unwrap_or("").to_ascii_lowercase();
                let val = kv.next().unwrap_or("");
                // Basic percent decoding
                let decoded = percent_decode(val);
                params.insert(key, decoded);
            }
        }

        let clean_path = path_part.trim_matches('/');
        let (section, path) = match clean_path.find('/') {
            Some(idx) => (
                clean_path[..idx].to_ascii_lowercase(),
                clean_path[idx + 1..].to_string(),
            ),
            None => (clean_path.to_ascii_lowercase(), String::new()),
        };

        DeepLinkContext {
            section,
            path,
            params,
            has_active_session: has_session,
        }
    }

    /// Dispatches parsed context to matching action.
    pub fn dispatch(&self, ctx: &DeepLinkContext) -> (DeepLinkResult, bool) {
        if ctx.section.is_empty() {
            return (DeepLinkResult::NotFound, false);
        }

        let entries = match self.routes.get(&ctx.section) {
            Some(e) => e,
            None => return (DeepLinkResult::NotFound, false),
        };

        let target_path = ctx.path.to_ascii_lowercase();

        // Exact match
        for entry in entries {
            if entry.path.eq_ignore_ascii_case(&target_path) {
                if entry.requires_auth && !ctx.has_active_session {
                    return (DeepLinkResult::NeedsAuth, entry.skip_activation);
                }
                return (self.execute_action(&entry.action, ctx), entry.skip_activation);
            }
        }

        // Prefix match
        for entry in entries {
            if !entry.path.is_empty() && target_path.starts_with(&format!("{}/", entry.path)) {
                if entry.requires_auth && !ctx.has_active_session {
                    return (DeepLinkResult::NeedsAuth, entry.skip_activation);
                }
                return (self.execute_action(&entry.action, ctx), entry.skip_activation);
            }
        }

        (DeepLinkResult::Unsupported, false)
    }

    pub fn try_handle(&self, command: &str, has_session: bool) -> bool {
        let ctx = Self::parse_command(command, has_session);
        let (result, _skip_activation) = self.dispatch(&ctx);
        matches!(result, DeepLinkResult::Handled)
    }

    fn execute_action(&self, action: &DeepLinkAction, ctx: &DeepLinkContext) -> DeepLinkResult {
        match action {
            DeepLinkAction::SettingsSection(_) => DeepLinkResult::Handled,
            DeepLinkAction::SettingsControl(_) => DeepLinkResult::Handled,
            DeepLinkAction::HandlerFunction(_) => DeepLinkResult::Handled,
            DeepLinkAction::AliasTo(alias) => {
                let mut alias_ctx = ctx.clone();
                alias_ctx.section = alias.section.clone();
                alias_ctx.path = alias.path.clone();
                self.dispatch(&alias_ctx).0
            }
        }
    }

    fn register_defaults(&mut self) {
        // Chats
        self.add(
            "chats",
            DeepLinkEntry {
                path: "search".to_string(),
                action: DeepLinkAction::HandlerFunction("focus_search".to_string()),
                requires_auth: true,
                skip_activation: true,
            },
        );
        self.add(
            "chats",
            DeepLinkEntry {
                path: "emoji-status".to_string(),
                action: DeepLinkAction::HandlerFunction("show_emoji_status".to_string()),
                requires_auth: true,
                skip_activation: false,
            },
        );

        // Contacts
        self.add(
            "contacts",
            DeepLinkEntry {
                path: String::new(),
                action: DeepLinkAction::HandlerFunction("show_contacts".to_string()),
                requires_auth: true,
                skip_activation: false,
            },
        );
        self.add(
            "contacts",
            DeepLinkEntry {
                path: "new".to_string(),
                action: DeepLinkAction::HandlerFunction("show_add_contact".to_string()),
                requires_auth: true,
                skip_activation: false,
            },
        );

        // New group / channel
        self.add(
            "new",
            DeepLinkEntry {
                path: "group".to_string(),
                action: DeepLinkAction::HandlerFunction("new_group".to_string()),
                requires_auth: true,
                skip_activation: false,
            },
        );
        self.add(
            "new",
            DeepLinkEntry {
                path: "channel".to_string(),
                action: DeepLinkAction::HandlerFunction("new_channel".to_string()),
                requires_auth: true,
                skip_activation: false,
            },
        );

        // Settings
        self.add(
            "settings",
            DeepLinkEntry {
                path: String::new(),
                action: DeepLinkAction::SettingsSection(DeepLinkSettingsSection {
                    section_id: "main".to_string(),
                }),
                requires_auth: true,
                skip_activation: false,
            },
        );
        self.add(
            "settings",
            DeepLinkEntry {
                path: "privacy".to_string(),
                action: DeepLinkAction::SettingsSection(DeepLinkSettingsSection {
                    section_id: "privacy".to_string(),
                }),
                requires_auth: true,
                skip_activation: false,
            },
        );
        self.add(
            "settings",
            DeepLinkEntry {
                path: "notifications".to_string(),
                action: DeepLinkAction::SettingsControl(DeepLinkSettingsControl {
                    section_id: "notifications".to_string(),
                    control_id: "sound".to_string(),
                }),
                requires_auth: true,
                skip_activation: false,
            },
        );
    }
}

fn percent_decode(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '%' {
            let h1 = chars.next();
            let h2 = chars.next();
            if let (Some(c1), Some(c2)) = (h1, h2) {
                if let Ok(byte) = u8::from_str_radix(&format!("{}{}", c1, c2), 16) {
                    result.push(byte as char);
                    continue;
                }
            }
        } else if ch == '+' {
            result.push(' ');
            continue;
        }
        result.push(ch);
    }
    result
}
