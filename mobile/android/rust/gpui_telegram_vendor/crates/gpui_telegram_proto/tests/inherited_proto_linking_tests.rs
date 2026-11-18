//! Verification tests for Mandate #10 inherited tdrp coordinator re-exports in gpui_telegram_proto.
#![cfg(feature = "inherited-coordinator")]

use gpui_telegram_proto::tdrp::{
    calls_group_calls, chat_admin_moderation, export_payments_bots, folders_filters, forum_topics,
    polls_todo, settings_cloud_passkeys, stories_gifts_stars, TdrpCoordinatorState,
};

#[test]
fn test_tdrp_reexports_accessible() {
    let mut coordinator = TdrpCoordinatorState::new();

    // Verify method recognition across tdrp modules
    assert!(TdrpCoordinatorState::is_handled("folders.list"));
    assert!(TdrpCoordinatorState::is_handled("topics.list"));
    assert!(TdrpCoordinatorState::is_handled("polls.vote"));
    assert!(TdrpCoordinatorState::is_handled("todo.create"));
    assert!(TdrpCoordinatorState::is_handled("stories.list"));
    assert!(TdrpCoordinatorState::is_handled("stars.gift"));
    assert!(TdrpCoordinatorState::is_handled("gifts.send"));
    assert!(TdrpCoordinatorState::is_handled("admin.ban"));
    assert!(TdrpCoordinatorState::is_handled("calls.start"));
    assert!(TdrpCoordinatorState::is_handled("groupCalls.join"));
    assert!(TdrpCoordinatorState::is_handled("settings.get"));
    assert!(TdrpCoordinatorState::is_handled("auth.passkey"));
    assert!(TdrpCoordinatorState::is_handled("export.history"));
    assert!(TdrpCoordinatorState::is_handled("payments.invoice"));
    assert!(TdrpCoordinatorState::is_handled("bots.command"));

    // Verify unhandled method returns false
    assert!(!TdrpCoordinatorState::is_handled("unknown.operation"));

    // Verify sub-module structs are directly constructible
    let _folders = folders_filters::FolderFilterState::default();
    let _topics = forum_topics::ForumTopicsState::default();
    let _polls = polls_todo::PollsTodoState::default();
    let _stories = stories_gifts_stars::StoriesGiftsStarsState::default();
    let _moderation = chat_admin_moderation::ChatAdminModerationState::default();
    let _calls = calls_group_calls::CallsGroupCallsState::default();
    let _settings = settings_cloud_passkeys::SettingsCloudPasskeysState::default();
    let _export_bots = export_payments_bots::ExportPaymentsBotsState::default();

    // Verify normalize_method
    assert_eq!(TdrpCoordinatorState::normalize_method("tdrp.folders.get"), "folders.get");
    assert_eq!(TdrpCoordinatorState::normalize_method("feature.topics.list"), "topics.list");

    // Verify dispatch with empty/unhandled returns None
    let unhandled_res = coordinator.dispatch("unknown.method", &serde_json::json!({}));
    assert!(unhandled_res.is_none());
}
