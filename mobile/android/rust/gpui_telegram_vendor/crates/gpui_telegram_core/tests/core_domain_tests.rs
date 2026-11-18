use chrono::{Duration, Utc};
use gpui_telegram_core::auth::{GenerationFence, SessionToken};
use gpui_telegram_core::error::{Result, TelegramError};
use gpui_telegram_core::types::{
    ChannelId, ChatFlags, ChatId, MessageFlags, MessageId, MessageKey, MsgId, PeerId, PrivacyFlags,
    TopicId, TypingStatus, UserId, UserStatus,
};
use std::str::FromStr;
use std::sync::Arc;
use std::thread;

#[test]
fn test_user_id_display_and_parse() {
    let id = UserId(123456789);
    assert_eq!(format!("{}", id), "user_123456789");
    assert_eq!(id.raw(), 123456789);

    let parsed = UserId::from_str("user_123456789").expect("parse user_");
    assert_eq!(parsed, id);

    let parsed_raw = UserId::from_str("123456789").expect("parse raw");
    assert_eq!(parsed_raw, id);

    let from_u64: UserId = 999u64.into();
    assert_eq!(from_u64, UserId(999));
}

#[test]
fn test_chat_id_display_and_parse() {
    let id = ChatId(-100123456789);
    assert_eq!(format!("{}", id), "chat_-100123456789");
    assert_eq!(id.raw(), -100123456789);

    let parsed = ChatId::from_str("chat_-100123456789").expect("parse chat_");
    assert_eq!(parsed, id);

    let parsed_raw = ChatId::from_str("-100123456789").expect("parse raw");
    assert_eq!(parsed_raw, id);

    let from_i64: ChatId = (-42i64).into();
    assert_eq!(from_i64, ChatId(-42));
}

#[test]
fn test_channel_id_display_and_parse() {
    let id = ChannelId(987654321);
    assert_eq!(format!("{}", id), "channel_987654321");
    assert_eq!(id.raw(), 987654321);

    let parsed = ChannelId::from_str("channel_987654321").expect("parse channel_");
    assert_eq!(parsed, id);

    let parsed_raw = ChannelId::from_str("987654321").expect("parse raw");
    assert_eq!(parsed_raw, id);
}

#[test]
fn test_topic_id_display_and_parse() {
    let topic = TopicId(42);
    assert_eq!(format!("{}", topic), "topic_42");
    assert_eq!(topic.raw(), 42);

    let msg = topic.to_msg_id();
    assert_eq!(msg, MsgId(42));
    assert_eq!(TopicId::from_msg_id(msg), topic);

    let parsed = TopicId::from_str("topic_42").expect("parse topic_");
    assert_eq!(parsed, topic);
}

#[test]
fn test_msg_id_ranges_and_client_generation() {
    // Server range
    let s1 = MsgId(1);
    let s2 = MsgId(100_000);
    let s3 = MsgId(MsgId::SERVER_MAX - 1);
    assert!(s1.is_server());
    assert!(!s1.is_client());
    assert!(s2.is_server());
    assert!(!s2.is_client());
    assert!(s3.is_server());
    assert!(!s3.is_client());

    // Client range
    let c0 = MsgId::generate_client(0);
    let c1 = MsgId::generate_client(100);
    let c_max = MsgId(MsgId::END_CLIENT - 1);
    assert!(c0.is_client());
    assert!(!c0.is_server());
    assert!(c1.is_client());
    assert!(!c1.is_server());
    assert!(c_max.is_client());
    assert!(!c_max.is_server());

    // Boundary conditions
    let zero = MsgId(0);
    assert!(!zero.is_server());
    assert!(!zero.is_client());

    let too_large_server = MsgId(MsgId::SERVER_MAX);
    assert!(!too_large_server.is_server());

    let past_client = MsgId(MsgId::END_CLIENT);
    assert!(!past_client.is_client());

    // Backward compat alias
    let compat_id: MessageId = MessageId(55);
    assert_eq!(compat_id.0, 55);
}

#[test]
fn test_peer_id_bitfield_shift_roundtrip() {
    // 1. User: Shift 0
    let user_bare = 0x0000_1234_5678_9ABC_u64;
    let peer_user = PeerId::User(UserId(user_bare));
    let raw_user = peer_user.to_raw();
    assert_eq!(raw_user >> 48, PeerId::SHIFT_USER as u64);
    assert_eq!(raw_user & PeerId::MASK_BARE, user_bare);
    assert_eq!(PeerId::from_raw(raw_user), peer_user);
    assert!(peer_user.is_user());
    assert!(!peer_user.is_chat());
    assert_eq!(peer_user.as_user(), Some(UserId(user_bare)));

    // 2. Chat: Shift 1
    let chat_bare = 0x0000_0000_0001_F4E0_i64; // 128224
    let peer_chat = PeerId::Chat(ChatId(chat_bare));
    let raw_chat = peer_chat.to_raw();
    assert_eq!(raw_chat >> 48, PeerId::SHIFT_CHAT as u64);
    assert_eq!(raw_chat & PeerId::MASK_BARE, chat_bare as u64);
    assert_eq!(PeerId::from_raw(raw_chat), peer_chat);
    assert!(peer_chat.is_chat());
    assert!(!peer_chat.is_channel());
    assert_eq!(peer_chat.as_chat(), Some(ChatId(chat_bare)));

    // 3. Channel: Shift 2
    let channel_bare = 0x0000_0000_3B9A_CA00_u64; // 1_000_000_000
    let peer_channel = PeerId::Channel(ChannelId(channel_bare));
    let raw_channel = peer_channel.to_raw();
    assert_eq!(raw_channel >> 48, PeerId::SHIFT_CHANNEL as u64);
    assert_eq!(raw_channel & PeerId::MASK_BARE, channel_bare);
    assert_eq!(PeerId::from_raw(raw_channel), peer_channel);
    assert!(peer_channel.is_channel());
    assert!(!peer_channel.is_user());
    assert_eq!(peer_channel.as_channel(), Some(ChannelId(channel_bare)));

    // 4. Fake: Shift 0x7F
    let fake_bare = 0x0000_0000_0000_0042_u64;
    let peer_fake = PeerId::Fake(fake_bare);
    let raw_fake = peer_fake.to_raw();
    assert_eq!(raw_fake >> 48, PeerId::SHIFT_FAKE as u64);
    assert_eq!(raw_fake & PeerId::MASK_BARE, fake_bare);
    assert_eq!(PeerId::from_raw(raw_fake), peer_fake);
    assert!(peer_fake.is_fake());
}

#[test]
fn test_peer_id_mask_and_bounds() {
    // Verify that bits beyond 48 are cleanly truncated by the mask
    let overflowing_bare = 0xFFFF_0000_1234_5678_u64;
    let peer = PeerId::User(UserId(overflowing_bare));
    let raw = peer.to_raw();
    assert_eq!(raw >> 48, PeerId::SHIFT_USER as u64);
    assert_eq!(raw & PeerId::MASK_BARE, overflowing_bare & PeerId::MASK_BARE);
}


#[test]
fn test_peer_id_display_and_parse() {
    let u = PeerId::User(UserId(1001));
    assert_eq!(format!("{}", u), "peer_user_1001");
    assert_eq!(PeerId::from_str("peer_user_1001").unwrap(), u);

    let c = PeerId::Chat(ChatId(-2002));
    assert_eq!(format!("{}", c), "peer_chat_-2002");
    assert_eq!(PeerId::from_str("peer_chat_-2002").unwrap(), c);

    let ch = PeerId::Channel(ChannelId(3003));
    assert_eq!(format!("{}", ch), "peer_channel_3003");
    assert_eq!(PeerId::from_str("peer_channel_3003").unwrap(), ch);

    let f = PeerId::Fake(4004);
    assert_eq!(format!("{}", f), "peer_fake_4004");
    assert_eq!(PeerId::from_str("peer_fake_4004").unwrap(), f);
}

#[test]
fn test_message_key() {
    let peer = PeerId::User(UserId(777));
    let msg = MsgId(888);
    let key = MessageKey::new(peer, msg);

    assert_eq!(key.peer, peer);
    assert_eq!(key.msg_id, msg);

    let key_str = format!("{}", key);
    assert_eq!(key_str, "peer_user_777:msg_888");

    let parsed = MessageKey::from_str(&key_str).expect("parse MessageKey");
    assert_eq!(parsed, key);

    // Serde roundtrip
    let json = serde_json::to_string(&key).expect("serialize MessageKey");
    let deser: MessageKey = serde_json::from_str(&json).expect("deserialize MessageKey");
    assert_eq!(deser, key);
}

#[test]
fn test_user_status_variants_and_serde() {
    let now = Utc::now();
    let expires = now + Duration::minutes(5);
    let online = UserStatus::online(expires);
    assert!(online.is_online(now));
    assert_eq!(online.online_expires_at(), Some(expires));

    let offline_recent = UserStatus::offline_at(now);
    assert!(!offline_recent.is_online(now));
    assert_eq!(offline_recent.was_online(), Some(now));

    let offline_none = UserStatus::offline();
    assert_eq!(offline_none.was_online(), None);

    let recently = UserStatus::Recently;
    assert!(recently.is_recently());
    assert!(!recently.is_hidden());

    let hidden = UserStatus::Hidden;
    assert!(hidden.is_hidden());
    assert!(!hidden.is_recently());

    let variants = vec![
        online,
        offline_recent,
        offline_none,
        recently,
        UserStatus::LastWeek,
        UserStatus::LastMonth,
        UserStatus::LongAgo,
        hidden,
    ];

    for var in variants {
        let json = serde_json::to_string(&var).expect("serialize UserStatus");
        let deser: UserStatus = serde_json::from_str(&json).expect("deserialize UserStatus");
        assert_eq!(deser, var);
    }
}

#[test]
fn test_typing_status_15_variants_and_serde() {
    let all_15 = [
        (TypingStatus::Typing, "typing"),
        (TypingStatus::RecordAudio, "record_audio"),
        (TypingStatus::UploadAudio, "upload_audio"),
        (TypingStatus::RecordVideo, "record_video"),
        (TypingStatus::UploadVideo, "upload_video"),
        (TypingStatus::RecordRound, "record_round"),
        (TypingStatus::UploadRound, "upload_round"),
        (TypingStatus::UploadPhoto, "upload_photo"),
        (TypingStatus::UploadDocument, "upload_document"),
        (TypingStatus::ChooseLocation, "choose_location"),
        (TypingStatus::ChooseContact, "choose_contact"),
        (TypingStatus::ChooseSticker, "choose_sticker"),
        (TypingStatus::PlayGame, "play_game"),
        (TypingStatus::Speaking, "speaking"),
        (TypingStatus::Cancel, "cancel"),
    ];

    assert_eq!(all_15.len(), 15, "Must have exactly 15 TypingStatus variants");

    for (status, expected_action) in all_15 {
        assert_eq!(status.action_str(), expected_action);

        // Classification helpers
        if status == TypingStatus::Cancel {
            assert!(status.is_cancelling());
        } else {
            assert!(!status.is_cancelling());
        }

        if matches!(
            status,
            TypingStatus::RecordAudio | TypingStatus::UploadAudio | TypingStatus::Speaking
        ) {
            assert!(status.is_audio());
        }

        if matches!(
            status,
            TypingStatus::RecordVideo
                | TypingStatus::UploadVideo
                | TypingStatus::RecordRound
                | TypingStatus::UploadRound
        ) {
            assert!(status.is_video());
        }

        if matches!(
            status,
            TypingStatus::UploadAudio
                | TypingStatus::UploadVideo
                | TypingStatus::UploadRound
                | TypingStatus::UploadPhoto
                | TypingStatus::UploadDocument
        ) {
            assert!(status.is_upload());
        }

        // Serde roundtrip
        let json = serde_json::to_string(&status).expect("serialize TypingStatus");
        let deser: TypingStatus = serde_json::from_str(&json).expect("deserialize TypingStatus");
        assert_eq!(deser, status);
    }
}

#[test]
fn test_flags_bit_packing() {
    // 1. ChatFlags
    let chat_flags = ChatFlags {
        is_group: true,
        is_supergroup: true,
        is_channel: false,
        is_forum: true,
        is_verified: true,
        is_restricted: false,
        is_scam: false,
        is_fake: true,
        no_forwards: true,
        slowmode_enabled: false,
    };
    let bits = chat_flags.to_bits();
    let recovered_chat = ChatFlags::from_bits(bits);
    assert_eq!(recovered_chat, chat_flags);

    // Check specific bits
    assert_ne!(bits & ChatFlags::BIT_GROUP, 0);
    assert_ne!(bits & ChatFlags::BIT_SUPERGROUP, 0);
    assert_eq!(bits & ChatFlags::BIT_CHANNEL, 0);
    assert_ne!(bits & ChatFlags::BIT_FORUM, 0);
    assert_ne!(bits & ChatFlags::BIT_VERIFIED, 0);
    assert_eq!(bits & ChatFlags::BIT_RESTRICTED, 0);
    assert_eq!(bits & ChatFlags::BIT_SCAM, 0);
    assert_ne!(bits & ChatFlags::BIT_FAKE, 0);
    assert_ne!(bits & ChatFlags::BIT_NO_FORWARDS, 0);
    assert_eq!(bits & ChatFlags::BIT_SLOWMODE_ENABLED, 0);

    // 2. MessageFlags
    let msg_flags = MessageFlags {
        is_out: true,
        is_pinned: false,
        is_silent: true,
        is_edited: true,
        is_scheduled: false,
        media_unread: true,
        has_reactions: true,
        no_forwards: false,
        is_forwarded: true,
    };
    let msg_bits = msg_flags.to_bits();
    let recovered_msg = MessageFlags::from_bits(msg_bits);
    assert_eq!(recovered_msg, msg_flags);

    // 3. PrivacyFlags
    let priv_flags = PrivacyFlags {
        hide_last_seen: true,
        hide_phone_number: true,
        allow_calls_from_contacts_only: false,
        allow_voice_messages: true,
        hide_profile_photo: false,
        no_forward_link: true,
    };
    let priv_bits = priv_flags.to_bits();
    let recovered_priv = PrivacyFlags::from_bits(priv_bits);
    assert_eq!(recovered_priv, priv_flags);
}

#[test]
fn test_session_token_and_constant_time_verification() {
    let user_id = UserId(1001);
    let raw_token = "secret_session_token_xyz_123456789";
    let expires = Utc::now() + Duration::hours(24);

    let token = SessionToken::new("sess_abc", user_id, expires, "MacBook Pro M2", raw_token);

    assert_eq!(token.session_id, "sess_abc");
    assert_eq!(token.user_id, user_id);
    assert_eq!(token.device_name, "MacBook Pro M2");
    assert!(!token.is_expired());
    assert!(token.is_valid());

    // Verify token hash
    let expected_hash = SessionToken::hash_token(raw_token);
    assert_eq!(token.token_hash, expected_hash);

    // Verification methods
    assert!(token.verify_raw_token(raw_token));
    assert!(!token.verify_raw_token("wrong_token_guess"));
    assert!(token.verify_token_hash(&expected_hash));
    assert!(!token.verify_token_hash("deadbeef"));

    // Expiration check
    let expired_token = SessionToken::new(
        "sess_old",
        user_id,
        Utc::now() - Duration::seconds(10),
        "Old Device",
        raw_token,
    );
    assert!(expired_token.is_expired());
    assert!(!expired_token.is_valid());

    // Serde JSON roundtrip
    let json = serde_json::to_string(&token).expect("serialize SessionToken");
    let deser: SessionToken = serde_json::from_str(&json).expect("deserialize SessionToken");
    assert_eq!(deser, token);
}

#[test]
fn test_generation_fence_monotonic_and_staleness() {
    let fence = GenerationFence::new(10);
    assert_eq!(fence.current(), 10);
    assert!(fence.is_valid(10));
    assert!(!fence.is_valid(9));

    assert!(fence.assert_valid(10).is_ok());
    let err = fence.assert_valid(9).unwrap_err();
    match err {
        TelegramError::GenerationFenced { expected, actual } => {
            assert_eq!(expected, 9);
            assert_eq!(actual, 10);
        }
        _ => panic!("Expected GenerationFenced error"),
    }

    // Bump
    let next = fence.bump();
    assert_eq!(next, 11);
    assert_eq!(fence.current(), 11);
    assert!(fence.assert_valid(11).is_ok());
    assert!(fence.assert_valid(10).is_err());

    // Set
    fence.set(100);
    assert_eq!(fence.current(), 100);

    // Serde roundtrip
    let json = serde_json::to_string(&fence).expect("serialize GenerationFence");
    let deser: GenerationFence = serde_json::from_str(&json).expect("deserialize GenerationFence");
    assert_eq!(deser.current(), 100);
}

#[test]
fn test_generation_fence_concurrent_multi_thread() {
    let fence = Arc::new(GenerationFence::new(0));
    let num_threads = 8;
    let bumps_per_thread = 100;
    let mut handles = Vec::new();

    for _ in 0..num_threads {
        let fence_clone = Arc::clone(&fence);
        handles.push(thread::spawn(move || {
            for _ in 0..bumps_per_thread {
                fence_clone.bump();
            }
        }));
    }

    for handle in handles {
        handle.join().expect("thread join");
    }

    assert_eq!(fence.current(), (num_threads * bumps_per_thread) as u64);
    assert!(fence
        .assert_valid((num_threads * bumps_per_thread) as u64)
        .is_ok());
    assert!(fence.assert_valid(0).is_err());
}

#[test]
fn test_telegram_error_hierarchy() {
    let errs: Vec<(TelegramError, &str)> = vec![
        (
            TelegramError::Auth("invalid signature".into()),
            "Authentication error: invalid signature",
        ),
        (
            TelegramError::AuthFailed("bad password".into()),
            "Authentication failed: bad password",
        ),
        (
            TelegramError::GenerationFenced {
                expected: 5,
                actual: 6,
            },
            "Session stale or generation fenced: expected 5, actual 6",
        ),
        (
            TelegramError::NotFound("chat 123".into()),
            "Entity not found: chat 123",
        ),
        (
            TelegramError::PtsSequenceGap {
                last_seen: 10,
                incoming: 15,
            },
            "PTS sequence gap detected: last_seen 10, incoming 15",
        ),
        (
            TelegramError::SnapshotRequired,
            "Snapshot required for recovery",
        ),
        (
            TelegramError::PermissionDenied("admin only".into()),
            "Permission denied: admin only",
        ),
        (
            TelegramError::Validation("invalid range".into()),
            "Validation error: invalid range",
        ),
        (
            TelegramError::Database("sqlite locked".into()),
            "Database error: sqlite locked",
        ),
        (
            TelegramError::Network("connection reset".into()),
            "Network error: connection reset",
        ),
        (
            TelegramError::Serialization("json syntax error".into()),
            "Serialization error: json syntax error",
        ),
        (
            TelegramError::Internal("unexpected bug".into()),
            "Internal error: unexpected bug",
        ),
    ];

    for (err, expected_str) in errs {
        assert_eq!(format!("{}", err), expected_str);
    }

    // From std conversions
    let parse_err: Result<u32> = "abc"
        .parse::<u32>()
        .map_err(TelegramError::from);
    assert!(matches!(parse_err, Err(TelegramError::Validation(_))));

    let io_err: Result<()> = Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "file missing",
    ))
    .map_err(TelegramError::from);
    assert!(matches!(io_err, Err(TelegramError::Internal(_))));
}
