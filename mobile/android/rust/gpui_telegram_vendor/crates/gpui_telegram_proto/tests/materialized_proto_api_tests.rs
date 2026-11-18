use gpui_telegram_proto::api::*;

#[test]
fn test_proto_api_modules_instantiation_and_serialization() {
    let attached_stickers = ApiAttachedStickers::new();
    assert!(attached_stickers.is_active());
    assert_eq!(attached_stickers.label, "ApiAttachedStickers");

    let auths = ApiAuthorizations::new();
    assert!(auths.is_active());

    let blocked = ApiBlockedPeers::new();
    assert!(blocked.is_active());

    let bot = ApiBot::new();
    assert!(bot.is_active());

    let filters = ApiChatFilters::new();
    assert!(filters.is_active());

    let invite = ApiChatInvite::new();
    assert!(invite.is_active());

    let participants = ApiChatParticipants::new();
    assert!(participants.is_active());

    let cloud_pwd = ApiCloudPassword::new();
    assert!(cloud_pwd.is_active());

    let serialized = serde_json::to_string(&attached_stickers).expect("Must serialize");
    let deserialized: ApiAttachedStickers = serde_json::from_str(&serialized).expect("Must deserialize");
    assert_eq!(attached_stickers, deserialized);
}

#[test]
fn test_proto_api_rich_tasks_and_stickers() {
    let tasks = ApiRichTasks::new();
    assert!(tasks.is_active());

    let stickers = ApiStickersCreator::new();
    assert!(stickers.is_active());

    let todos = ApiTodoLists::new();
    assert!(todos.is_active());

    let transcribes = ApiTranscribes::new();
    assert!(transcribes.is_active());
}
