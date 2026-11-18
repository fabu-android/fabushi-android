use gpui_telegram_core::auth::GenerationFence;
use gpui_telegram_core::error::TelegramError;
use gpui_telegram_core::types::{
    ChannelId, ChatId, MessageKey, MsgId, PeerId, UserId,
};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

// ============================================================================
// 1. PeerId Bitfield Shifts & ID Bounds (Adversarial Verification)
// ============================================================================

#[test]
fn test_peer_id_max_48bit_roundtrip() {
    let max_bare = PeerId::MASK_BARE; // 0x0000_FFFF_FFFF_FFFF = 281474976710655

    // 1. User
    let peer_user = PeerId::User(UserId(max_bare));
    let raw_user = peer_user.to_raw_u64();
    assert_eq!(raw_user >> 48, PeerId::SHIFT_USER as u64);
    assert_eq!(raw_user & PeerId::MASK_BARE, max_bare);
    assert_eq!(PeerId::from_raw_u64(raw_user), peer_user);

    // 2. Chat
    let peer_chat = PeerId::Chat(ChatId(max_bare as i64));
    let raw_chat = peer_chat.to_raw_u64();
    assert_eq!(raw_chat >> 48, PeerId::SHIFT_CHAT as u64);
    assert_eq!(raw_chat & PeerId::MASK_BARE, max_bare);
    assert_eq!(PeerId::from_raw_u64(raw_chat), peer_chat);

    // 3. Channel
    let peer_channel = PeerId::Channel(ChannelId(max_bare));
    let raw_channel = peer_channel.to_raw_u64();
    assert_eq!(raw_channel >> 48, PeerId::SHIFT_CHANNEL as u64);
    assert_eq!(raw_channel & PeerId::MASK_BARE, max_bare);
    assert_eq!(PeerId::from_raw_u64(raw_channel), peer_channel);

    // 4. Fake
    let peer_fake = PeerId::Fake(max_bare);
    let raw_fake = peer_fake.to_raw_u64();
    assert_eq!(raw_fake >> 48, PeerId::SHIFT_FAKE as u64);
    assert_eq!(raw_fake & PeerId::MASK_BARE, max_bare);
    assert_eq!(PeerId::from_raw_u64(raw_fake), peer_fake);
}

#[test]
fn test_peer_id_zero_bare_id() {
    let zero_bare = 0u64;

    let peer_user = PeerId::User(UserId(zero_bare));
    assert_eq!(peer_user.to_raw_u64(), 0u64);
    assert_eq!(PeerId::from_raw_u64(0), peer_user);

    let peer_chat = PeerId::Chat(ChatId(0));
    assert_eq!(peer_chat.to_raw_u64(), (PeerId::SHIFT_CHAT as u64) << 48);
    assert_eq!(PeerId::from_raw_u64(peer_chat.to_raw_u64()), peer_chat);

    let peer_channel = PeerId::Channel(ChannelId(0));
    assert_eq!(peer_channel.to_raw_u64(), (PeerId::SHIFT_CHANNEL as u64) << 48);
    assert_eq!(PeerId::from_raw_u64(peer_channel.to_raw_u64()), peer_channel);

    let peer_fake = PeerId::Fake(0);
    assert_eq!(peer_fake.to_raw_u64(), (PeerId::SHIFT_FAKE as u64) << 48);
    assert_eq!(PeerId::from_raw_u64(peer_fake.to_raw_u64()), peer_fake);
}

#[test]
fn test_peer_id_shifts_exact_values() {
    assert_eq!(PeerId::SHIFT_USER, 0);
    assert_eq!(PeerId::SHIFT_CHAT, 1);
    assert_eq!(PeerId::SHIFT_CHANNEL, 2);
    assert_eq!(PeerId::SHIFT_FAKE, 0x7F);

    let bare = 42u64;
    let u = PeerId::User(UserId(bare)).to_raw_u64();
    let c = PeerId::Chat(ChatId(bare as i64)).to_raw_u64();
    let ch = PeerId::Channel(ChannelId(bare)).to_raw_u64();
    let f = PeerId::Fake(bare).to_raw_u64();

    assert_eq!(u, 42 | (0u64 << 48));
    assert_eq!(c, 42 | (1u64 << 48));
    assert_eq!(ch, 42 | (2u64 << 48));
    assert_eq!(f, 42 | (0x7Fu64 << 48));
}

#[test]
fn test_peer_id_out_of_bounds_shifts_mapped_to_fake() {
    let unmapped_shifts = [3u8, 4, 10, 0x40, 0x7E, 0x80, 0xFE, 0xFF];
    let bare = 12345u64;

    for &shift in &unmapped_shifts {
        let raw = ((shift as u64) << 48) | bare;
        let decoded = PeerId::from_raw_u64(raw);
        // All unknown shifts fall through to PeerId::Fake
        assert_eq!(decoded, PeerId::Fake(bare), "Shift {} should map to Fake", shift);
        // When encoded back, it acquires the canonical SHIFT_FAKE (0x7F)
        assert_eq!(decoded.to_raw_u64(), ((PeerId::SHIFT_FAKE as u64) << 48) | bare);
    }
}

#[test]
fn test_peer_id_bare_mask_prevents_shift_pollution() {
    // Construct IDs where upper 16 bits are non-zero (exceeding 48-bit bare capacity)
    let dirty_bare = 0xFFFF_0000_1234_5678u64;
    let expected_bare = dirty_bare & PeerId::MASK_BARE; // 0x0000_0000_1234_5678

    let user = PeerId::User(UserId(dirty_bare));
    let raw_user = user.to_raw_u64();
    // Shift bits must remain strictly SHIFT_USER (0), not contaminated by 0xFFFF
    assert_eq!(raw_user >> 48, PeerId::SHIFT_USER as u64);
    assert_eq!(raw_user & PeerId::MASK_BARE, expected_bare);

    let chat = PeerId::Chat(ChatId(dirty_bare as i64));
    let raw_chat = chat.to_raw_u64();
    assert_eq!(raw_chat >> 48, PeerId::SHIFT_CHAT as u64);
    assert_eq!(raw_chat & PeerId::MASK_BARE, expected_bare);

    let channel = PeerId::Channel(ChannelId(dirty_bare));
    let raw_channel = channel.to_raw_u64();
    assert_eq!(raw_channel >> 48, PeerId::SHIFT_CHANNEL as u64);
    assert_eq!(raw_channel & PeerId::MASK_BARE, expected_bare);
}

#[test]
fn test_peer_id_chat_id_negative_bitfield_handling() {
    // Basic chat IDs in Telegram are often represented as negative in client UI (e.g. -123456)
    let neg_chat = ChatId(-123456);
    let peer = PeerId::Chat(neg_chat);
    let raw = peer.to_raw_u64();

    // The shift must still be SHIFT_CHAT
    assert_eq!(raw >> 48, PeerId::SHIFT_CHAT as u64);

    // Decoding from raw converts the masked bare bits into i64
    let decoded = PeerId::from_raw_u64(raw);
    if let PeerId::Chat(decoded_chat) = decoded {
        // Because bits 48..63 are zero in bare, bare as i64 is positive
        let expected_bare_i64 = ((-123456i64 as u64) & PeerId::MASK_BARE) as i64;
        assert_eq!(decoded_chat.0, expected_bare_i64);
        assert!(decoded_chat.0 > 0);
    } else {
        panic!("Expected PeerId::Chat");
    }
}

// ============================================================================
// 2. MsgId Boundaries & Range Classifications
// ============================================================================

#[test]
fn test_msg_id_extreme_boundaries() {
    // Client bounds
    let client_start = MsgId(MsgId::START_CLIENT);
    let before_client_start = MsgId(MsgId::START_CLIENT - 1);
    let client_end_minus_one = MsgId(MsgId::END_CLIENT - 1);
    let client_end = MsgId(MsgId::END_CLIENT);

    assert!(client_start.is_client());
    assert!(!before_client_start.is_client());
    assert!(client_end_minus_one.is_client());
    assert!(!client_end.is_client());

    // Server bounds
    let server_min = MsgId(1);
    let server_zero = MsgId(0);
    let server_max_minus_one = MsgId(MsgId::SERVER_MAX - 1);
    let server_max = MsgId(MsgId::SERVER_MAX);

    assert!(server_min.is_server());
    assert!(!server_zero.is_server());
    assert!(server_max_minus_one.is_server());
    assert!(!server_max.is_server());

    // Extreme integer values
    assert!(!MsgId(i64::MIN).is_client());
    assert!(!MsgId(i64::MIN).is_server());
    assert!(!MsgId(i64::MAX).is_client());
    assert!(!MsgId(i64::MAX).is_server());
    assert!(!MsgId(-1).is_client());
    assert!(!MsgId(-1).is_server());
}

#[test]
fn test_message_key_string_roundtrip_edge_cases() {
    // Maximum possible bare ID with client message ID
    let max_peer = PeerId::User(UserId(PeerId::MASK_BARE));
    let client_msg = MsgId::generate_client(u32::MAX / 2);
    let key = MessageKey::new(max_peer, client_msg);
    let formatted = format!("{}", key);
    let parsed = MessageKey::from_str(&formatted).expect("Valid MessageKey parse");
    assert_eq!(key, parsed);

    // Malformed strings
    assert!(MessageKey::from_str("").is_err());
    assert!(MessageKey::from_str("peer_user_123").is_err()); // missing ':'
    assert!(MessageKey::from_str(":msg_123").is_err()); // empty peer
    assert!(MessageKey::from_str("peer_user_123:").is_err()); // empty msg
    assert!(MessageKey::from_str("invalid_peer:msg_123").is_err()); // invalid peer
    assert!(MessageKey::from_str("peer_user_123:not_a_msg").is_err()); // invalid msg
}

// ============================================================================
// 3. GenerationFence Concurrent Stress Test
// ============================================================================

#[test]
fn test_generation_fence_concurrent_multithread_stress() {
    let fence = Arc::new(GenerationFence::new(100));
    let thread_count = 16;
    let bumps_per_thread = 1_000;
    let failed = Arc::new(AtomicBool::new(false));

    let mut handles = Vec::new();
    for _ in 0..thread_count {
        let f = Arc::clone(&fence);
        let fail_flag = Arc::clone(&failed);
        handles.push(thread::spawn(move || {
            for _ in 0..bumps_per_thread {
                let prev = f.current();
                let next = f.bump();
                if next <= prev {
                    fail_flag.store(true, Ordering::SeqCst);
                }
            }
        }));
    }

    for h in handles {
        h.join().expect("thread join");
    }

    assert!(!failed.load(Ordering::SeqCst), "Fence bump was not strictly monotonic!");
    let expected_final = 100 + (thread_count as u64 * bumps_per_thread as u64);
    assert_eq!(fence.current(), expected_final);

    // Stale generation assertion must return TelegramError::GenerationFenced
    let stale_gen = expected_final - 1;
    let res = fence.assert_valid(stale_gen);
    match res {
        Err(TelegramError::GenerationFenced { expected, actual }) => {
            assert_eq!(expected, stale_gen);
            assert_eq!(actual, expected_final);
        }
        _ => panic!("Expected GenerationFenced error"),
    }
}
