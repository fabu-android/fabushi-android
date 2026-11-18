//! Domain module re-exporting core messaging domain models and services from inherited `fabushi-messaging-core`.

pub use fabushi_messaging_core::{
    conversation, dialog, engine_tdrp_dialogs, engine_tdrp_media_stories, engine_tdrp_moderation,
    engine_tdrp_topics_channels, message, miniapp, payment, secret_chat, story, wallet,
};
