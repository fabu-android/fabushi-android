//! Telegram Desktop 1:1 Parity Module: `DeepLinksNewDeepLinks`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/core/deep_links/deep_links_new.h, Telegram/SourceFiles/core/deep_links/deep_links_new.cpp

use super::deep_links_router::DeepLinksRouterDeepLinks;
use super::deep_links_types::{DeepLinkAction, DeepLinkEntry};
use serde::{Deserialize, Serialize};

/// Deep links registration for creation flows: groups, channels, contacts, and managed bots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DeepLinksNewDeepLinks;

impl DeepLinksNewDeepLinks {
    pub fn register(router: &mut DeepLinksRouterDeepLinks) {
        // tg://new/group -> Create New Group wizard
        router.add(
            "new",
            DeepLinkEntry {
                path: "group".to_string(),
                action: DeepLinkAction::HandlerFunction("show_new_group".to_string()),
                requires_auth: true,
                skip_activation: false,
            },
        );

        // tg://new/channel -> Create New Channel wizard
        router.add(
            "new",
            DeepLinkEntry {
                path: "channel".to_string(),
                action: DeepLinkAction::HandlerFunction("show_new_channel".to_string()),
                requires_auth: true,
                skip_activation: false,
            },
        );

        // tg://new/contact -> Add Contact dialog
        router.add(
            "new",
            DeepLinkEntry {
                path: "contact".to_string(),
                action: DeepLinkAction::HandlerFunction("show_add_contact".to_string()),
                requires_auth: true,
                skip_activation: false,
            },
        );

        // tg://newbot?manager=<bot>&name=<name> -> Create Managed Bot dialog
        router.add(
            "newbot",
            DeepLinkEntry {
                path: String::new(),
                action: DeepLinkAction::HandlerFunction("show_create_managed_bot".to_string()),
                requires_auth: true,
                skip_activation: false,
            },
        );
    }
}
