use gpui_telegram_core::types::{ChatId, MessageId, PeerId, UserId, UserStatus};
use gpui_telegram_proto::cards::{
    CardActionButton, CardActionType, InvoiceCard, PaymentStatus, PollCard, PollOption, PriceLine,
    TodoCard, TodoItem,
};
use gpui_telegram_proto::message::{
    DeleteTombstone, MediaItem, MediaThumbnail, MessageContent, MessageEntity, MessageEntityType,
    MessageItem, ReactionCount, VideoNoteItem, VoiceItem,
};
use gpui_telegram_proto::sync::{
    ChatActionEvent, PtsUpdate, ReadHistoryEvent, ReactionEvent,
    SyncGetDifferenceRequest, SyncGetDifferenceResponse, TypingAction, TypingEvent, UpdateEvent,
};
use gpui_telegram_proto::ws::{
    DeleteMessagesRequest, EditMessageRequest, ReactionRequest, ReadHistoryRequest,
    SendMessageRequest, WsClientFrame, WsServerFrame,
};

// ============================================================================
// 1. PTS Continuity & Gap Detection Tests
// ============================================================================

#[test]
fn test_pts_prev_pts_invariant() {
    let dummy_event = UpdateEvent::ReadHistoryOut {
        max_id: MessageId(1),
    };
    let update = PtsUpdate::new(ChatId(100), 50, 5, dummy_event, 1000);
    assert_eq!(update.prev_pts(), 45);

    let underflow_check = PtsUpdate::new(
        ChatId(100),
        2,
        10,
        UpdateEvent::ReadHistoryOut {
            max_id: MessageId(1),
        },
        1000,
    );
    assert_eq!(underflow_check.prev_pts(), 0);
}

#[test]
fn test_pts_continuity_exact() {
    let dummy_event = UpdateEvent::ReadHistoryOut {
        max_id: MessageId(1),
    };

    // Client at PTS 100, receives update covering 101 (pts=101, count=1 -> prev_pts=100)
    let update1 = PtsUpdate::new(ChatId(1), 101, 1, dummy_event.clone(), 1000);
    assert!(update1.is_continuous(100));
    assert_eq!(update1.gap(100), None);
    assert!(!update1.is_stale_or_duplicate(100));

    // Batch update: pts=105, count=5 -> prev_pts=100
    let update_batch = PtsUpdate::new(ChatId(1), 105, 5, dummy_event.clone(), 1000);
    assert!(update_batch.is_continuous(100));
    assert_eq!(update_batch.gap(100), None);
}

#[test]
fn test_pts_gap_detection_range() {
    let dummy_event = UpdateEvent::ReadHistoryOut {
        max_id: MessageId(1),
    };

    // Client at PTS 10, update arrives with pts=20, count=3 -> prev_pts=17
    // Gap should be (10 + 1)..=17 -> 11..=17
    let update = PtsUpdate::new(ChatId(1), 20, 3, dummy_event, 1000);
    assert!(!update.is_continuous(10));
    assert_eq!(update.gap(10), Some(11..=17));
    assert!(!update.is_stale_or_duplicate(10));
}

#[test]
fn test_pts_stale_and_duplicate() {
    let dummy_event = UpdateEvent::ReadHistoryOut {
        max_id: MessageId(1),
    };

    // Client is at PTS 50
    // An update with pts=49 is stale
    let update_stale = PtsUpdate::new(ChatId(1), 49, 1, dummy_event.clone(), 1000);
    assert!(update_stale.is_stale_or_duplicate(50));
    assert!(!update_stale.is_continuous(50));
    assert_eq!(update_stale.gap(50), None);

    // An update with pts=50 is a duplicate retransmission
    let update_dup = PtsUpdate::new(ChatId(1), 50, 1, dummy_event, 1000);
    assert!(update_dup.is_stale_or_duplicate(50));
    assert!(!update_dup.is_continuous(50));
}

#[test]
fn test_pts_ephemeral_updates_zero_count() {
    let typing_event = UpdateEvent::UserTyping(TypingEvent::new(
        ChatId(1),
        UserId(42),
        TypingAction::Typing,
    ));

    // Ephemeral update with pts_count = 0 does not advance pts
    let ephemeral = PtsUpdate::new(ChatId(1), 50, 0, typing_event, 1000);
    assert_eq!(ephemeral.prev_pts(), 50);

    // Continuous if client is at or beyond referenced pts
    assert!(ephemeral.is_continuous(50));
    assert!(ephemeral.is_continuous(60));
    assert!(!ephemeral.is_stale_or_duplicate(50));
    assert!(!ephemeral.is_stale_or_duplicate(60));
}

#[test]
fn test_pts_simulated_stream_recovery() {
    let chat = ChatId(42);
    let mut current_pts = 0u64;

    let updates = vec![
        PtsUpdate::new(chat, 1, 1, UpdateEvent::ReadHistoryOut { max_id: MessageId(1) }, 100),
        PtsUpdate::new(chat, 2, 1, UpdateEvent::ReadHistoryOut { max_id: MessageId(2) }, 200),
        // Hole: skipped pts 3 and 4!
        PtsUpdate::new(chat, 6, 2, UpdateEvent::ReadHistoryOut { max_id: MessageId(5) }, 300), // prev_pts = 4
    ];

    assert!(updates[0].is_continuous(current_pts));
    current_pts = updates[0].pts;

    assert!(updates[1].is_continuous(current_pts));
    current_pts = updates[1].pts;

    // Next update has gap!
    assert!(!updates[2].is_continuous(current_pts));
    let missing_range = updates[2].gap(current_pts).expect("must detect gap");
    assert_eq!(missing_range, 3..=4);

    // Simulate catch-up retrieval
    let catch_up = vec![
        PtsUpdate::new(chat, 3, 1, UpdateEvent::ReadHistoryOut { max_id: MessageId(3) }, 250),
        PtsUpdate::new(chat, 4, 1, UpdateEvent::ReadHistoryOut { max_id: MessageId(4) }, 260),
    ];

    for cu in catch_up {
        assert!(cu.is_continuous(current_pts));
        current_pts = cu.pts;
    }

    // Now buffered update[2] is contiguous
    assert!(updates[2].is_continuous(current_pts));
    current_pts = updates[2].pts;
    assert_eq!(current_pts, 6);
}

// ============================================================================
// 2. UTF-16 Code-Unit Entity Offset Tests
// ============================================================================

#[test]
fn test_utf16_entity_extraction_ascii() {
    let text = "Hello world";
    let entity = MessageEntity::new(6, 5, MessageEntityType::Bold);
    let extracted = entity.extract_text(text).expect("extract text");
    assert_eq!(extracted, "world");
}

#[test]
fn test_utf16_entity_extraction_multibyte_cjk() {
    // In UTF-8, each Chinese char is 3 bytes (total 6 bytes).
    // In UTF-16, each Chinese char is 1 code unit (total 2 units).
    let text = "你好世界";
    let utf16: Vec<u16> = text.encode_utf16().collect();
    assert_eq!(utf16.len(), 4);

    // Entity for "世界" -> offset=2, length=2
    let entity = MessageEntity::new(2, 2, MessageEntityType::Italic);
    let extracted = entity.extract_text(text).expect("extract text");
    assert_eq!(extracted, "世界");
}

#[test]
fn test_utf16_entity_extraction_astral_plane_emoji() {
    // "🚀" (U+1F680 rocket) is 4 bytes in UTF-8, but 2 code units in UTF-16 (surrogate pair: 0xD83D, 0xDE80).
    let text = "Hi 🚀 there";
    let utf16: Vec<u16> = text.encode_utf16().collect();
    // "Hi " = 3 units
    // "🚀" = 2 units
    // " there" = 6 units
    // Total = 11 units
    assert_eq!(utf16.len(), 11);

    // Select the rocket emoji: offset 3, length 2
    let rocket_entity = MessageEntity::new(3, 2, MessageEntityType::CustomEmoji { document_id: 12345 });
    let extracted = rocket_entity.extract_text(text).expect("extract emoji");
    assert_eq!(extracted, "🚀");

    // Select "there": offset = 3 + 2 + 1 = 6, length = 5
    let word_entity = MessageEntity::new(6, 5, MessageEntityType::Code);
    let extracted_word = word_entity.extract_text(text).expect("extract word");
    assert_eq!(extracted_word, "there");
}

#[test]
fn test_utf16_entity_from_str_subslice() {
    let text = "Rust is 🦀 blazing fast!";
    let needle = "blazing";
    let entity = MessageEntity::from_str_subslice(text, needle, MessageEntityType::Bold)
        .expect("build entity from subslice");

    // "Rust is " = 8 code units
    // "🦀" = 2 code units (surrogate pair U+1F980)
    // " " = 1 code unit
    // offset to "blazing" = 8 + 2 + 1 = 11
    assert_eq!(entity.offset, 11);
    assert_eq!(entity.length, 7);

    let extracted = entity.extract_text(text).expect("extract");
    assert_eq!(extracted, needle);
}

#[test]
fn test_utf16_entity_out_of_bounds() {
    let text = "Short";
    let entity = MessageEntity::new(10, 5, MessageEntityType::Bold);
    assert_eq!(entity.extract_text(text), None);
}

// ============================================================================
// 3. Tagged Serde JSON Roundtrip Tests (Frames & Events)
// ============================================================================

#[test]
fn test_ws_client_frames_serde_roundtrip() {
    let chat = ChatId(777);
    let user = UserId(1001);
    let _peer = PeerId::Chat(chat);

    let frames = vec![
        WsClientFrame::Ping { nonce: 987654 },
        WsClientFrame::Sync(SyncGetDifferenceRequest::new(chat, 42, 50).with_filter("unread")),
        WsClientFrame::SendMessage(SendMessageRequest::new(
            chat,
            123456,
            MessageContent::plain_text("Hello server"),
        )),
        WsClientFrame::EditMessage(EditMessageRequest::new(
            chat,
            MessageId(5),
            MessageContent::plain_text("Edited text"),
        )),
        WsClientFrame::DeleteMessages(DeleteMessagesRequest::new(
            chat,
            vec![MessageId(1), MessageId(2)],
            true,
        )),
        WsClientFrame::SendReaction(ReactionRequest::new(chat, MessageId(5), "👍", true)),
        WsClientFrame::ReadHistory(ReadHistoryRequest::new(chat, MessageId(10))),
        WsClientFrame::SendTyping(TypingEvent::new(chat, user, TypingAction::RecordAudio)),
    ];

    for frame in &frames {
        let json = serde_json::to_string(frame).expect("serialize client frame");
        // Verify tagged structure
        assert!(json.contains(r#""type":""#), "Must contain type tag: {}", json);
        let deserialized: WsClientFrame =
            serde_json::from_str(&json).expect("deserialize client frame");
        assert_eq!(&deserialized, frame);
    }
}

#[test]
fn test_ws_server_frames_serde_roundtrip() {
    let chat = ChatId(888);
    let user = UserId(2002);
    let peer = PeerId::User(user);

    let dummy_msg = MessageItem::new(
        MessageId(1),
        chat,
        user,
        peer,
        MessageContent::plain_text("Push message"),
    );

    let frames = vec![
        WsServerFrame::Pong { nonce: 42 },
        WsServerFrame::Update(UpdateEvent::NewMessage(dummy_msg.clone())),
        WsServerFrame::PtsPush(PtsUpdate::new(
            chat,
            10,
            1,
            UpdateEvent::NewMessage(dummy_msg),
            123456789,
        )),
        WsServerFrame::SyncDiff(SyncGetDifferenceResponse::new(
            chat,
            Vec::new(),
            Vec::new(),
            10,
            false,
        )),
        WsServerFrame::Ack {
            correlation_id: 111,
            client_nonce: 222,
            message_id: 333,
            pts: 15,
        },
        WsServerFrame::Error {
            code: 400,
            message: "Bad request".into(),
            details: Some("Malformed JSON".into()),
        },
    ];

    for frame in &frames {
        let json = serde_json::to_string(frame).expect("serialize server frame");
        assert!(json.contains(r#""type":""#), "Must contain type tag: {}", json);
        let deserialized: WsServerFrame =
            serde_json::from_str(&json).expect("deserialize server frame");
        assert_eq!(&deserialized, frame);
    }
}

// ============================================================================
// 4. DeleteTombstone & UpdateEvent Serialization Tests
// ============================================================================

#[test]
fn test_delete_tombstone_serialization() {
    let tombstone = DeleteTombstone::new(
        PeerId::Chat(ChatId(99)),
        vec![MessageId(101), MessageId(102), MessageId(103)],
        true,
    );

    assert!(tombstone.is_deleted(MessageId(102)));
    assert!(!tombstone.is_deleted(MessageId(104)));

    let json = serde_json::to_string(&tombstone).expect("serialize tombstone");
    let deserialized: DeleteTombstone =
        serde_json::from_str(&json).expect("deserialize tombstone");
    assert_eq!(deserialized, tombstone);
}

#[test]
fn test_update_event_variants_serialization() {
    let chat = ChatId(12);
    let user = UserId(34);
    let peer = PeerId::Chat(chat);

    let msg = MessageItem::new(
        MessageId(1),
        chat,
        user,
        peer,
        MessageContent::plain_text("test"),
    );

    let tombstone = DeleteTombstone::new(peer, vec![MessageId(1)], true);

    let events = vec![
        UpdateEvent::NewMessage(msg.clone()),
        UpdateEvent::EditMessage(msg),
        UpdateEvent::DeleteMessages(tombstone),
        UpdateEvent::UserTyping(TypingEvent::new(chat, user, TypingAction::Typing)),
        UpdateEvent::ChatAction(ChatActionEvent::TitleChanged {
            new_title: "Rust Developers".into(),
        }),
        UpdateEvent::ReadHistory(ReadHistoryEvent::new(chat, MessageId(50), false, 0)),
        UpdateEvent::MessageReactions(ReactionEvent::new(
            chat,
            MessageId(50),
            user,
            vec![ReactionCount::new("🚀", 5, true)],
        )),
        UpdateEvent::UserStatusChanged {
            user_id: user,
            status: UserStatus::offline(),
        },
    ];

    for event in &events {
        let json = serde_json::to_string(event).expect("serialize event");
        let deserialized: UpdateEvent = serde_json::from_str(&json).expect("deserialize event");
        assert_eq!(&deserialized, event);
    }
}

// ============================================================================
// 5. A2UI Interactive Cards Lifecycle & Mathematics Tests
// ============================================================================

#[test]
fn test_todo_card_lifecycle_and_percentage() {
    let items = vec![
        TodoItem::new("1", "Setup workspace").with_completed(true, Some(100)),
        TodoItem::new("2", "Write proto models").with_assignee("worker_m2_proto"),
        TodoItem::new("3", "Write unit tests"),
        TodoItem::new("4", "Run cargo check"),
    ];

    let mut todo = TodoCard::new("list_1", "Sprint 1 Tasks", items);
    assert_eq!(todo.completed_count, 1);
    assert_eq!(todo.completion_percentage(), 25.0);

    // Toggle item 2 to completed
    assert!(todo.toggle_item("2"));
    assert_eq!(todo.completed_count, 2);
    assert_eq!(todo.completion_percentage(), 50.0);

    // Add item 5 as completed
    todo.add_item(TodoItem::new("5", "Deploy binary").with_completed(true, Some(200)));
    assert_eq!(todo.completed_count, 3);
    assert_eq!(todo.items.len(), 5);
    assert!((todo.completion_percentage() - 60.0).abs() < 1e-4);

    // Remove item 1
    assert!(todo.remove_item("1"));
    assert_eq!(todo.completed_count, 2);
    assert_eq!(todo.items.len(), 4);
    assert_eq!(todo.completion_percentage(), 50.0);

    // Add action button
    todo.add_action_button(CardActionButton::new(
        "btn_add",
        "Add Task",
        CardActionType::Callback,
        "action:add",
    ));
    assert_eq!(todo.action_buttons.len(), 1);

    // Serde roundtrip
    let json = serde_json::to_string(&todo).expect("serialize todo");
    let deserialized: TodoCard = serde_json::from_str(&json).expect("deserialize todo");
    assert_eq!(deserialized, todo);
}

#[test]
fn test_poll_card_voting_math() {
    let options = vec![
        PollOption::new(1, "Rust"),
        PollOption::new(2, "C++"),
        PollOption::new(3, "TypeScript"),
    ];

    let mut poll = PollCard::new("poll_100", "Preferred systems language?", options, true);
    assert_eq!(poll.total_voters, 0);
    assert_eq!(poll.vote_percentage(1), 0.0);

    // Cast vote for option 1 (Rust)
    assert!(poll.cast_vote(1));
    assert_eq!(poll.total_voters, 1);
    assert_eq!(poll.vote_percentage(1), 100.0);
    assert_eq!(poll.vote_percentage(2), 0.0);

    // Single-choice mode: casting vote for option 2 unsets option 1
    assert!(poll.cast_vote(2));
    assert_eq!(poll.total_voters, 1);
    assert_eq!(poll.vote_percentage(1), 0.0);
    assert_eq!(poll.vote_percentage(2), 100.0);

    // Close poll: voting should now be rejected
    poll.close();
    assert!(!poll.cast_vote(3));
    assert_eq!(poll.total_voters, 1);
    assert_eq!(poll.vote_percentage(2), 100.0);

    // Serde roundtrip
    let json = serde_json::to_string(&poll).expect("serialize poll");
    let deserialized: PollCard = serde_json::from_str(&json).expect("deserialize poll");
    assert_eq!(deserialized, poll);
}

#[test]
fn test_invoice_card_pricing_and_status() {
    let prices = vec![
        PriceLine::new("Telegram Premium 1 Year", 2999),
        PriceLine::new("Cloud Storage Add-on", 500),
    ];

    let mut invoice = InvoiceCard::new(
        "inv_999",
        "Telegram Subscription",
        "Annual premium tier",
        "USD",
        3499,
    )
    .with_prices(prices);

    assert_eq!(invoice.total_from_prices(), 3499);
    assert_eq!(invoice.status, PaymentStatus::Pending);
    assert!(!invoice.is_paid);

    // Mark paid
    invoice.mark_paid();
    assert_eq!(invoice.status, PaymentStatus::Paid);
    assert!(invoice.is_paid);

    // Serde roundtrip
    let json = serde_json::to_string(&invoice).expect("serialize invoice");
    assert!(json.contains(r#""status":"paid""#));
    let deserialized: InvoiceCard = serde_json::from_str(&json).expect("deserialize invoice");
    assert_eq!(deserialized, invoice);
}

// ============================================================================
// 6. Rich Message Schemas with Content Variants
// ============================================================================

#[test]
fn test_message_content_rich_variants_serde() {
    let contents = vec![
        MessageContent::Text {
            text: "Rich text formatting".into(),
            entities: vec![
                MessageEntity::new(0, 4, MessageEntityType::Bold),
                MessageEntity::new(5, 4, MessageEntityType::Italic),
            ],
        },
        MessageContent::Media(MediaItem {
            file_id: "file_123".into(),
            file_name: Some("photo.jpg".into()),
            mime_type: "image/jpeg".into(),
            size_bytes: 204800,
            width: Some(1920),
            height: Some(1080),
            duration_seconds: None,
            thumbnail: Some(MediaThumbnail {
                file_id: "thumb_123".into(),
                width: 320,
                height: 180,
                data_base64: None,
            }),
            caption: Some("Holiday snapshot".into()),
        }),
        MessageContent::Voice(VoiceItem {
            file_id: "voice_456".into(),
            duration_seconds: 14,
            waveform: vec![0, 5, 12, 28, 31, 20, 8, 2],
            mime_type: "audio/ogg".into(),
            size_bytes: 16384,
        }),
        MessageContent::VideoNote(VideoNoteItem {
            file_id: "video_note_789".into(),
            duration_seconds: 8,
            diameter: 384,
            thumbnail: None,
            size_bytes: 65536,
        }),
        MessageContent::Todo(TodoCard::new("t1", "Tasks", Vec::new())),
        MessageContent::Poll(PollCard::new("p1", "Question", Vec::new(), false)),
        MessageContent::Invoice(InvoiceCard::new("i1", "Item", "Desc", "USD", 100)),
    ];

    for content in &contents {
        let json = serde_json::to_string(content).expect("serialize content");
        assert!(json.contains(r#""type":""#), "Must contain type tag: {}", json);
        let deserialized: MessageContent =
            serde_json::from_str(&json).expect("deserialize content");
        assert_eq!(&deserialized, content);
    }
}
