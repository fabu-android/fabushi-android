//! Deep Links routing and handling submodules.

pub mod deep_links_chats;
pub use deep_links_chats::DeepLinksChatsDeepLinks;

pub mod deep_links_contacts;
pub use deep_links_contacts::DeepLinksContactsDeepLinks;

pub mod deep_links_new;
pub use deep_links_new::DeepLinksNewDeepLinks;

pub mod deep_links_router;
pub use deep_links_router::DeepLinksRouterDeepLinks;

pub mod deep_links_settings;
pub use deep_links_settings::DeepLinksSettingsDeepLinks;

pub mod deep_links_types;
pub use deep_links_types::{
    DeepLinkAction, DeepLinkContext, DeepLinkEntry, DeepLinkResult,
    DeepLinkSettingsControl, DeepLinkSettingsSection, DeepLinksTypesDeepLinks,
};
