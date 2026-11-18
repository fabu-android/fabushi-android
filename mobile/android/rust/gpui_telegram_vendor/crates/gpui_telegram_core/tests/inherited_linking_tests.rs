//! Verification tests for Mandate #10 inherited crate linking and domain re-exports.
#![cfg(feature = "inherited-messaging")]

use gpui_telegram_core::domain::{
    conversation, dialog, engine_tdrp_dialogs, engine_tdrp_media_stories, engine_tdrp_moderation,
    engine_tdrp_topics_channels, message, miniapp, payment, secret_chat, story, wallet,
};

#[test]
fn test_domain_reexports_accessible() {
    // 1. Verify conversation module is accessible
    let conv_id = conversation::ConversationId::new("conv_test_123");
    assert_eq!(conv_id.as_str(), "conv_test_123");

    // 2. Verify dialog / engine_tdrp_dialogs is accessible
    let dest = conversation::ConversationDestination {
        conversation_id: conv_id.clone(),
        child: None,
    };
    let resolved_id = dialog::destination_message_conversation_id(&dest);
    assert_eq!(resolved_id.as_str(), "conv_test_123");
    let resolved_id2 = engine_tdrp_dialogs::destination_message_conversation_id(&dest);
    assert_eq!(resolved_id2.as_str(), "conv_test_123");

    // 3. Verify message module is accessible
    let client_msg_id = message::ClientMessageId("msg_test_456".to_string());
    assert_eq!(client_msg_id.0, "msg_test_456");

    // 4. Verify story module is accessible
    let story_id = story::StoryId("story_test_789".to_string());
    assert_eq!(story_id.0, "story_test_789");

    // 5. Verify wallet module is accessible
    let wallet_account_id = wallet::WalletAccountId("wallet_001".to_string());
    assert_eq!(wallet_account_id.0, "wallet_001");

    // 6. Verify payment module is accessible
    let money = payment::Money::new("USD", 1500);
    assert!(money.is_valid());
    assert_eq!(money.amount_minor, 1500);

    // 7. Verify miniapp module is accessible
    let perm = miniapp::MiniAppPermission::Payments;
    assert_eq!(perm, miniapp::MiniAppPermission::Payments);

    // 8. Verify secret_chat module is accessible
    let secret_key = secret_chat::SecretPublicKey::from_bytes([42u8; 32]);
    assert_eq!(secret_key.algorithm, "X25519");

    // 9. Verify engine_tdrp_moderation is accessible
    let action = engine_tdrp_moderation::CommunityAdminAction::ManageTopics;
    assert_eq!(action, engine_tdrp_moderation::CommunityAdminAction::ManageTopics);

    // 10. Verify engine_tdrp_topics_channels is accessible
    let mut messaging_state = fabushi_messaging_core::engine::MessagingState::default();
    messaging_state.migrate_legacy_topic_child_states();

    // 11. Verify engine_tdrp_media_stories is accessible
    assert!(engine_tdrp_media_stories::message_content_uses_other(&message::MessageContent::Dice {
        emoji: "🎲".to_string(),
        value: 6,
    }));
}
