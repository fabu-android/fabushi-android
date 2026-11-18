//! Empirical Adversarial Fuzz & Stress Test Harness for Module 02 (MTProto & API)
//! Challenger: challenger_module_02_2
//! Tests:
//! 1. PTS updates sequence invariants & gap detection triggers
//! 2. Leaky bucket rate limiter burst limits & refill over time
//! 3. Flood wait error parsing (FLOOD_WAIT_0, FLOOD_WAIT_300, malformed errors)
//! 4. Admin and Banned rights bitfield permutations, empty, and full rights
//! 5. Upstream authentic bit-shift hash calculation (api_hash.rs) stability & collision resistance
//! 6. Spoiler parsing in api_text_entities.rs (unclosed, nested, emoji, surrogate pairs, CJK)

use gpui_telegram_core::mtproto::RpcError;
use gpui_telegram_proto::api::*;
use gpui_telegram_proto::message::MessageEntityType;
use std::collections::HashSet;

// ============================================================================
// 1. PTS Updates Sequence Invariants & Gap Detection
// ============================================================================

#[test]
fn test_pts_updates_sequence_invariants_and_gap_triggers() {
    let mut updates = ApiUpdates::with_pts(1000, 1700000000);
    assert_eq!(updates.pts(), 1000);
    assert!(!updates.requesting_difference());

    // 1.1 Contiguous sequential update: prev_pts = pts - count == current pts
    // Current pts = 1000, incoming pts = 1005, count = 5 -> prev_pts = 1000 == 1000
    assert!(updates.update_and_apply(1005, 5));
    assert_eq!(updates.pts(), 1005);
    assert!(!updates.requesting_difference());

    // 1.2 Gap detection: prev_pts = pts - count > current pts
    // Current pts = 1005, incoming pts = 1020, count = 2 -> prev_pts = 1018 > 1005
    // Missing range: 1006..=1018
    assert!(!updates.update_and_apply(1020, 2), "Gap update must not be applied directly");
    assert!(updates.requesting_difference(), "Gap must trigger requesting_difference");
    assert_eq!(updates.pts(), 1005, "Current pts must NOT advance when gap is detected");

    // Clear difference request via finish_difference
    updates.finish_difference(1020, 1700000100, 1);
    assert_eq!(updates.pts(), 1020);
    assert!(!updates.requesting_difference());

    // 1.3 Duplicate / Stale update: pts <= current pts
    assert!(!updates.update_and_apply(1015, 5), "Stale update must return false");
    assert_eq!(updates.pts(), 1020);
    assert!(!updates.update_and_apply(1020, 1), "Exact duplicate pts must return false");
    assert_eq!(updates.pts(), 1020);

    // 1.4 Overlapping update: pts > current, but prev_pts < current
    // Current pts = 1020, incoming pts = 1025, count = 10 -> prev_pts = 1015 < 1020
    // The update spans 1015..1025, covering already known 1015..1020 plus new 1021..1025
    assert!(updates.update_and_apply(1025, 10));
    assert_eq!(updates.pts(), 1025);

    // 1.5 Channel PTS independent tracking & gap triggers
    let channel_id = 88888u64;
    assert!(updates.update_channel_pts(channel_id, 100, 100), "Initial channel pts succeeds");
    assert_eq!(updates.channel_pts.get(&channel_id).copied(), Some(100));

    // Contiguous channel update: pts = 110, count = 10 -> prev = 100 == 100
    assert!(updates.update_channel_pts(channel_id, 110, 10));
    assert_eq!(updates.channel_pts.get(&channel_id).copied(), Some(110));

    // Channel gap: pts = 150, count = 5 -> prev = 145 > 110
    assert!(!updates.update_channel_pts(channel_id, 150, 5));
    assert!(updates.pending_channel_diffs.contains(&channel_id), "Channel gap must be recorded in pending_channel_diffs");
    assert_eq!(updates.channel_pts.get(&channel_id).copied(), Some(110), "Channel pts must not advance on gap");
}

// ============================================================================
// 2. Leaky Bucket Rate Limiter
// ============================================================================

#[test]
fn test_leaky_bucket_rate_limiter_burst_and_refill() {
    // Capacity 5.0 tokens, refill rate 1.0 token/sec
    let mut limiter = RateLimiter::new(5.0, 1.0);

    // 2.1 Burst capacity test: can acquire exactly 5 tokens
    for i in 0..5 {
        assert!(
            limiter.try_acquire(),
            "Token {} must be successfully acquired from full bucket",
            i + 1
        );
    }

    // 2.2 Exhaustion: 6th token must be rejected in the same second
    assert!(
        !limiter.try_acquire(),
        "Attempting to acquire from exhausted bucket must fail"
    );

    // 2.3 Simulated time refill: advance last_refill_timestamp by 3 seconds
    limiter.last_refill_timestamp -= 3;
    // Should have refilled ~3 tokens
    assert!(limiter.try_acquire(), "Refilled token 1 must succeed");
    assert!(limiter.try_acquire(), "Refilled token 2 must succeed");
    assert!(limiter.try_acquire(), "Refilled token 3 must succeed");
    assert!(!limiter.try_acquire(), "4th token must fail as only 3 were refilled");

    // 2.4 Capping at capacity: advance time by 100 seconds
    limiter.last_refill_timestamp -= 100;
    // Must not exceed capacity of 5.0 tokens
    for _ in 0..5 {
        assert!(limiter.try_acquire());
    }
    assert!(!limiter.try_acquire(), "Tokens must be clamped at capacity 5.0");

    // 2.5 Clock skew resilience: negative elapsed time (future timestamp)
    limiter.last_refill_timestamp = chrono::Utc::now().timestamp() + 3600;
    // .max(0) ensures elapsed is 0, does not crash or underflow
    assert!(!limiter.try_acquire(), "Future timestamp must not grant extra tokens");
}

// ============================================================================
// 3. Flood Wait Error Parsing
// ============================================================================

#[test]
fn test_flood_wait_parsing_empirical() {
    // 3.1 Standard flood wait zero seconds
    let err_0 = RpcError::new(420, "FLOOD_WAIT_0", "Flood wait 0");
    assert!(err_0.is_flood());
    assert_eq!(err_0.flood_wait_seconds(), Some(0));

    // 3.2 Standard flood wait 300 seconds (5 minutes)
    let err_300 = RpcError::new(420, "FLOOD_WAIT_300", "Flood wait 300s");
    assert!(err_300.is_flood());
    assert_eq!(err_300.flood_wait_seconds(), Some(300));

    // 3.3 Large value
    let err_large = RpcError::new(420, "FLOOD_WAIT_86400", "1 day wait");
    assert_eq!(err_large.flood_wait_seconds(), Some(86400));

    // 3.4 Premium flood wait
    let err_prem = RpcError::new(420, "FLOOD_PREMIUM_WAIT_15", "Premium wait");
    assert!(err_prem.is_flood());
    assert_eq!(err_prem.flood_wait_seconds(), Some(15));

    // 3.5 Malformed errors: must safely return None without panics
    let malformed_cases = vec![
        "FLOOD_WAIT_",                      // Missing number
        "FLOOD_WAIT_-1",                    // Negative number
        "FLOOD_WAIT_abc",                   // Non-numeric
        "FLOOD_WAIT_300_extra",             // Trailing garbage
        "FLOOD_WAIT_ 300",                  // Space prefix
        "FLOOD_WAIT_300 ",                  // Space suffix
        "flood_wait_300",                   // Lowercase
        "FLOOD_WAIT_99999999999999999999999999999999", // u64 overflow
        "OTHER_RPC_ERROR",                  // Unrelated error
        "",                                 // Empty string
    ];

    for case in malformed_cases {
        let err = RpcError::new(420, case, "Testing malformed error");
        assert_eq!(
            err.flood_wait_seconds(),
            None,
            "Malformed error '{}' must parse as None",
            case
        );
    }
}

// ============================================================================
// 4. Admin & Banned Rights Bitfields Permutations
// ============================================================================

#[test]
fn test_admin_and_banned_rights_bitfields() {
    // 4.1 Empty rights (all false)
    let empty_admin = ChatAdminRights::default();
    assert!(!empty_admin.change_info);
    assert!(!empty_admin.post_messages);
    assert!(!empty_admin.edit_messages);
    assert!(!empty_admin.delete_messages);
    assert!(!empty_admin.ban_users);
    assert!(!empty_admin.invite_users);
    assert!(!empty_admin.pin_messages);
    assert!(!empty_admin.add_admins);
    assert!(!empty_admin.anonymous);
    assert!(!empty_admin.manage_call);
    assert!(!empty_admin.manage_topics);
    assert!(!empty_admin.post_stories);
    assert!(!empty_admin.edit_stories);
    assert!(!empty_admin.delete_stories);

    let empty_banned = ChatBannedRights::default();
    assert!(!empty_banned.view_messages);
    assert!(!empty_banned.send_messages);
    assert!(!empty_banned.send_media);
    assert!(!empty_banned.send_stickers);
    assert!(!empty_banned.send_gifs);
    assert!(!empty_banned.embed_links);
    assert!(!empty_banned.send_polls);
    assert!(!empty_banned.change_info);
    assert!(!empty_banned.invite_users);
    assert!(!empty_banned.pin_messages);
    assert!(!empty_banned.manage_topics);

    // 4.2 Full rights (all true)
    let full_admin = ChatAdminRights {
        change_info: true,
        post_messages: true,
        edit_messages: true,
        delete_messages: true,
        ban_users: true,
        invite_users: true,
        pin_messages: true,
        add_admins: true,
        anonymous: true,
        manage_call: true,
        manage_topics: true,
        post_stories: true,
        edit_stories: true,
        delete_stories: true,
    };

    let full_banned = ChatBannedRights {
        view_messages: true,
        send_messages: true,
        send_media: true,
        send_stickers: true,
        send_gifs: true,
        embed_links: true,
        send_polls: true,
        change_info: true,
        invite_users: true,
        pin_messages: true,
        manage_topics: true,
    };

    // 4.3 Serde round-trip verification for full rights
    let json_admin = serde_json::to_string(&full_admin).unwrap();
    let de_admin: ChatAdminRights = serde_json::from_str(&json_admin).unwrap();
    assert_eq!(full_admin, de_admin);

    let json_banned = serde_json::to_string(&full_banned).unwrap();
    let de_banned: ChatBannedRights = serde_json::from_str(&json_banned).unwrap();
    assert_eq!(full_banned, de_banned);

    // 4.4 All 14 single-bit permutations for ChatAdminRights
    let admin_permutations = [
        ChatAdminRights { change_info: true, ..ChatAdminRights::default() },
        ChatAdminRights { post_messages: true, ..ChatAdminRights::default() },
        ChatAdminRights { edit_messages: true, ..ChatAdminRights::default() },
        ChatAdminRights { delete_messages: true, ..ChatAdminRights::default() },
        ChatAdminRights { ban_users: true, ..ChatAdminRights::default() },
        ChatAdminRights { invite_users: true, ..ChatAdminRights::default() },
        ChatAdminRights { pin_messages: true, ..ChatAdminRights::default() },
        ChatAdminRights { add_admins: true, ..ChatAdminRights::default() },
        ChatAdminRights { anonymous: true, ..ChatAdminRights::default() },
        ChatAdminRights { manage_call: true, ..ChatAdminRights::default() },
        ChatAdminRights { manage_topics: true, ..ChatAdminRights::default() },
        ChatAdminRights { post_stories: true, ..ChatAdminRights::default() },
        ChatAdminRights { edit_stories: true, ..ChatAdminRights::default() },
        ChatAdminRights { delete_stories: true, ..ChatAdminRights::default() },
    ];

    let mut set = HashSet::new();
    for (idx, perm) in admin_permutations.iter().enumerate() {
        let serialized = serde_json::to_string(perm).unwrap();
        assert!(set.insert(serialized.clone()), "Duplicate permutation detected at {}", idx);
        let deserialized: ChatAdminRights = serde_json::from_str(&serialized).unwrap();
        assert_eq!(perm, &deserialized);
    }
    assert_eq!(set.len(), 14, "All 14 single-bit admin permutations must be distinct");

    // 4.5 All 11 single-bit permutations for ChatBannedRights
    let banned_permutations = [
        ChatBannedRights { view_messages: true, ..ChatBannedRights::default() },
        ChatBannedRights { send_messages: true, ..ChatBannedRights::default() },
        ChatBannedRights { send_media: true, ..ChatBannedRights::default() },
        ChatBannedRights { send_stickers: true, ..ChatBannedRights::default() },
        ChatBannedRights { send_gifs: true, ..ChatBannedRights::default() },
        ChatBannedRights { embed_links: true, ..ChatBannedRights::default() },
        ChatBannedRights { send_polls: true, ..ChatBannedRights::default() },
        ChatBannedRights { change_info: true, ..ChatBannedRights::default() },
        ChatBannedRights { invite_users: true, ..ChatBannedRights::default() },
        ChatBannedRights { pin_messages: true, ..ChatBannedRights::default() },
        ChatBannedRights { manage_topics: true, ..ChatBannedRights::default() },
    ];

    let mut banned_set = HashSet::new();
    for (idx, perm) in banned_permutations.iter().enumerate() {
        let serialized = serde_json::to_string(perm).unwrap();
        assert!(banned_set.insert(serialized.clone()), "Duplicate banned permutation at {}", idx);
        let deserialized: ChatBannedRights = serde_json::from_str(&serialized).unwrap();
        assert_eq!(perm, &deserialized);
    }
    assert_eq!(banned_set.len(), 11, "All 11 single-bit banned permutations must be distinct");
}

// ============================================================================
// 5. Upstream Authentic Bit-Shift Hash Calculation (api_hash.rs)
// ============================================================================

#[test]
fn test_api_hash_stability_and_collision_resistance() {
    // 5.1 Determinism & Stability across repeated calls
    let sequence = [1001u64, 2002u64, 3003u64, 4004u64, 5005u64];
    let hash_a = count_hash(sequence);
    let hash_b = count_hash(sequence);
    assert_eq!(hash_a, hash_b, "Hash calculation must be strictly deterministic");

    // 5.2 Order sensitivity (non-commutative)
    let hash_rev = count_hash([5005u64, 4004u64, 3003u64, 2002u64, 1001u64]);
    assert_ne!(hash_a, hash_rev, "Hash must be sensitive to element ordering");

    // 5.3 Empty sequence base case
    assert_eq!(count_hash(Vec::<u64>::new()), 0);

    // 5.4 Single-bit input avalanche / collision resistance
    let base = 0x1234_5678_9ABC_DEF0u64;
    let mut hashes = HashSet::new();
    hashes.insert(count_hash([base]));

    for bit in 0..64 {
        let flipped = base ^ (1u64 << bit);
        let h = count_hash([flipped]);
        assert!(
            hashes.insert(h),
            "Hash collision on 1-bit flip at bit {}: input=0x{:016x}, hash=0x{:016x}",
            bit,
            flipped,
            h
        );
    }
    assert_eq!(hashes.len(), 65, "Base + 64 single-bit flips must produce 65 distinct hashes");

    // 5.5 High-volume sequential distinctness
    const NUM_ITEMS: usize = 10_000;
    let mut seq_hashes = HashSet::with_capacity(NUM_ITEMS);
    for i in 0..NUM_ITEMS as u64 {
        let h = count_hash([i]);
        assert!(seq_hashes.insert(h), "Collision on sequential index {}", i);
    }
    assert_eq!(seq_hashes.len(), NUM_ITEMS);

    // 5.6 ApiHash struct state test
    let mut api_hash = ApiHash::new();
    assert!(api_hash.is_active());
    let computed = api_hash.compute_hash([42u64, 99u64]);
    assert_eq!(computed, api_hash.last_computed_hash);
    assert_eq!(computed, count_hash([42u64, 99u64]));
}

// ============================================================================
// 6. Spoiler Parsing Adversarial Fuzzing (api_text_entities.rs)
// ============================================================================

#[test]
fn test_spoiler_parsing_ascii_and_edge_cases() {
    // 6.1 Standard clean spoiler
    let parsed = ApiTextEntities::parse_markdown_spoilers("Hello ||secret|| world");
    assert_eq!(parsed.text, "Hello secret world");
    assert_eq!(parsed.entities.len(), 1);
    assert_eq!(parsed.entities[0].offset, 6);
    assert_eq!(parsed.entities[0].length, 6);
    assert_eq!(parsed.entities[0].entity_type, MessageEntityType::Spoiler);
    assert_eq!(parsed.entities[0].extract_text(&parsed.text).as_deref(), Some("secret"));

    // 6.2 Multiple spoilers
    let multi = ApiTextEntities::parse_markdown_spoilers("||first|| middle ||second||");
    assert_eq!(multi.text, "first middle second");
    assert_eq!(multi.entities.len(), 2);
    assert_eq!(multi.entities[0].offset, 0);
    assert_eq!(multi.entities[0].length, 5);
    assert_eq!(multi.entities[1].offset, 13);
    assert_eq!(multi.entities[1].length, 6);
    assert_eq!(multi.entities[0].extract_text(&multi.text).as_deref(), Some("first"));
    assert_eq!(multi.entities[1].extract_text(&multi.text).as_deref(), Some("second"));

    // 6.3 Empty string
    let empty = ApiTextEntities::parse_markdown_spoilers("");
    assert_eq!(empty.text, "");
    assert!(empty.entities.is_empty());

    // 6.4 Text without spoilers
    let no_spoilers = ApiTextEntities::parse_markdown_spoilers("Just plain text with | single pipe");
    assert_eq!(no_spoilers.text, "Just plain text with | single pipe");
    assert!(no_spoilers.entities.is_empty());
}

#[test]
fn test_spoiler_parsing_cjk_multibyte() {
    // Multi-byte CJK (BMP characters): each char is 3 bytes in UTF-8, 1 code unit in UTF-16
    let cjk = "你好||机密内容||世界";
    let parsed = ApiTextEntities::parse_markdown_spoilers(cjk);
    assert_eq!(parsed.text, "你好机密内容世界");
    assert_eq!(parsed.entities.len(), 1);
    assert_eq!(parsed.entities[0].offset, 2);
    assert_eq!(parsed.entities[0].length, 4);
    assert_eq!(
        parsed.entities[0].extract_text(&parsed.text).as_deref(),
        Some("机密内容")
    );
}

#[test]
fn test_spoiler_parsing_unclosed_and_nested_delimiters() {
    // Unclosed || delimiter: must preserve '||' verbatim in clean_text without emitting entity
    let unclosed = ApiTextEntities::parse_markdown_spoilers("Hello ||unclosed spoiler");
    assert_eq!(unclosed.text, "Hello ||unclosed spoiler");
    assert!(
        unclosed.entities.is_empty(),
        "Unclosed spoiler delimiter must NOT emit an entity"
    );

    // Adjacent delimiters |||| (empty spoiler): must NOT emit zero-length entity
    let nested = ApiTextEntities::parse_markdown_spoilers("||||");
    assert_eq!(nested.text, "");
    assert!(
        nested.entities.is_empty(),
        "Adjacent delimiters |||| must NOT emit a 0-length entity"
    );
}

#[test]
fn test_spoiler_parsing_unicode_emoji_surrogate_pairs() {
    // Astral-plane emoji: 😀 (U+1F600) is 1 char in Rust, 4 bytes in UTF-8, but 2 code units in UTF-16!
    // In Telegram MTProto / MessageEntity specification:
    // MessageEntity.offset and MessageEntity.length are explicitly measured in UTF-16 code units!
    let raw = "😀||secret||";
    let parsed = ApiTextEntities::parse_markdown_spoilers(raw);
    assert_eq!(parsed.text, "😀secret");

    assert_eq!(parsed.entities.len(), 1);
    let entity = &parsed.entities[0];
    assert_eq!(entity.offset, 2, "Offset must be 2 UTF-16 code units (skipping high/low surrogate pair for 😀)");
    assert_eq!(entity.length, 6, "Length must be 6 UTF-16 code units for 'secret'");
    let extracted = entity.extract_text(&parsed.text);
    assert_eq!(
        extracted.as_deref(),
        Some("secret"),
        "extract_text must successfully extract text with UTF-16 code units offset"
    );
}

#[test]
fn test_spoiler_parsing_complex_surrogates_and_edge_cases() {
    // Multiple astral emojis before, inside, and after spoiler
    let raw = "🎉🎊||🚀🛸||💥";
    let parsed = ApiTextEntities::parse_markdown_spoilers(raw);
    assert_eq!(parsed.text, "🎉🎊🚀🛸💥");
    assert_eq!(parsed.entities.len(), 1);
    let entity = &parsed.entities[0];
    // 🎉(2) + 🎊(2) = 4 UTF-16 code units offset
    assert_eq!(entity.offset, 4);
    // 🚀(2) + 🛸(2) = 4 UTF-16 code units length
    assert_eq!(entity.length, 4);
    assert_eq!(entity.extract_text(&parsed.text).as_deref(), Some("🚀🛸"));

    // Odd number of pipes: ||| (unclosed spoiler containing single '|')
    let odd = ApiTextEntities::parse_markdown_spoilers("|||");
    assert_eq!(odd.text, "|||");
    assert!(odd.entities.is_empty());

    // Single pipe preserved verbatim
    let single = ApiTextEntities::parse_markdown_spoilers("a | b");
    assert_eq!(single.text, "a | b");
    assert!(single.entities.is_empty());

    // Unclosed trailing ||
    let trailing = ApiTextEntities::parse_markdown_spoilers("Test ||");
    assert_eq!(trailing.text, "Test ||");
    assert!(trailing.entities.is_empty());
}
