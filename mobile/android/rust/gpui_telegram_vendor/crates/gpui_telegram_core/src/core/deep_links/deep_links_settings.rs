//! Telegram Desktop 1:1 Parity Module: `DeepLinksSettingsDeepLinks`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/core/deep_links/deep_links_settings.h, Telegram/SourceFiles/core/deep_links/deep_links_settings.cpp

use super::deep_links_router::DeepLinksRouterDeepLinks;
use super::deep_links_types::{
    DeepLinkAction, DeepLinkEntry, DeepLinkSettingsControl, DeepLinkSettingsSection,
};
use serde::{Deserialize, Serialize};

/// Deep links registration for Settings drawer sections and highlighted controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DeepLinksSettingsDeepLinks;

impl DeepLinksSettingsDeepLinks {
    pub fn register(router: &mut DeepLinksRouterDeepLinks) {
        // Main settings
        router.add(
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

        // Privacy & Security
        router.add(
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
        router.add(
            "settings",
            DeepLinkEntry {
                path: "passcode".to_string(),
                action: DeepLinkAction::SettingsControl(DeepLinkSettingsControl {
                    section_id: "privacy".to_string(),
                    control_id: "local_passcode".to_string(),
                }),
                requires_auth: true,
                skip_activation: false,
            },
        );
        router.add(
            "settings",
            DeepLinkEntry {
                path: "two-step".to_string(),
                action: DeepLinkAction::SettingsControl(DeepLinkSettingsControl {
                    section_id: "privacy".to_string(),
                    control_id: "cloud_password_2fa".to_string(),
                }),
                requires_auth: true,
                skip_activation: false,
            },
        );

        // Notifications
        router.add(
            "settings",
            DeepLinkEntry {
                path: "notifications".to_string(),
                action: DeepLinkAction::SettingsSection(DeepLinkSettingsSection {
                    section_id: "notifications".to_string(),
                }),
                requires_auth: true,
                skip_activation: false,
            },
        );

        // Folders
        router.add(
            "settings",
            DeepLinkEntry {
                path: "folders".to_string(),
                action: DeepLinkAction::SettingsSection(DeepLinkSettingsSection {
                    section_id: "folders".to_string(),
                }),
                requires_auth: true,
                skip_activation: false,
            },
        );

        // Language
        router.add(
            "settings",
            DeepLinkEntry {
                path: "language".to_string(),
                action: DeepLinkAction::SettingsSection(DeepLinkSettingsSection {
                    section_id: "language".to_string(),
                }),
                requires_auth: true,
                skip_activation: false,
            },
        );

        // Active Devices
        router.add(
            "settings",
            DeepLinkEntry {
                path: "devices".to_string(),
                action: DeepLinkAction::SettingsSection(DeepLinkSettingsSection {
                    section_id: "devices".to_string(),
                }),
                requires_auth: true,
                skip_activation: false,
            },
        );
    }

    /// Generates canonical settings deep link URL.
    pub fn settings_deep_link(section: &str, control_id: &str) -> String {
        if control_id.is_empty() {
            if section.is_empty() || section == "main" {
                "tg://settings".to_string()
            } else {
                format!("tg://settings/{}", section)
            }
        } else {
            format!("tg://settings/{}?highlight={}", section, control_id)
        }
    }
}
