//! Telegram Desktop 1:1 Parity Module: `DeepLinksChatsDeepLinks`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/core/deep_links/deep_links_chats.h, Telegram/SourceFiles/core/deep_links/deep_links_chats.cpp

use super::deep_links_router::DeepLinksRouterDeepLinks;
use super::deep_links_types::{DeepLinkAction, DeepLinkEntry};
use serde::{Deserialize, Serialize};

/// Deep links registration for `tg://chats/*` routes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DeepLinksChatsDeepLinks;

impl DeepLinksChatsDeepLinks {
    pub fn register(router: &mut DeepLinksRouterDeepLinks) {
        // tg://chats/search -> Focus search input
        router.add(
            "chats",
            DeepLinkEntry {
                path: "search".to_string(),
                action: DeepLinkAction::HandlerFunction("focus_search".to_string()),
                requires_auth: true,
                skip_activation: true,
            },
        );

        // tg://chats/emoji-status -> Open emoji status picker
        router.add(
            "chats",
            DeepLinkEntry {
                path: "emoji-status".to_string(),
                action: DeepLinkAction::HandlerFunction("show_emoji_status".to_string()),
                requires_auth: true,
                skip_activation: false,
            },
        );
    }
}
