//! Telegram Desktop 1:1 Parity Module: `DeepLinksContactsDeepLinks`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/core/deep_links/deep_links_contacts.h, Telegram/SourceFiles/core/deep_links/deep_links_contacts.cpp

use super::deep_links_router::DeepLinksRouterDeepLinks;
use super::deep_links_types::{DeepLinkAction, DeepLinkEntry};
use serde::{Deserialize, Serialize};

/// Deep links registration for `tg://contacts/*` routes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DeepLinksContactsDeepLinks;

impl DeepLinksContactsDeepLinks {
    pub fn register(router: &mut DeepLinksRouterDeepLinks) {
        // tg://contacts -> Open contacts dialog
        router.add(
            "contacts",
            DeepLinkEntry {
                path: String::new(),
                action: DeepLinkAction::HandlerFunction("show_contacts".to_string()),
                requires_auth: true,
                skip_activation: false,
            },
        );

        // tg://contacts/search -> Open contacts search
        router.add(
            "contacts",
            DeepLinkEntry {
                path: "search".to_string(),
                action: DeepLinkAction::HandlerFunction("search_contacts".to_string()),
                requires_auth: true,
                skip_activation: false,
            },
        );

        // tg://contacts/new -> Open Add Contact modal
        router.add(
            "contacts",
            DeepLinkEntry {
                path: "new".to_string(),
                action: DeepLinkAction::HandlerFunction("show_add_contact".to_string()),
                requires_auth: true,
                skip_activation: false,
            },
        );
    }
}
