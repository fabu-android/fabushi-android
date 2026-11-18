//! Adversarial Stress Verification Suite for Module 02 (`mtproto & api`)
//! EMPIRICAL CHALLENGER: challenger_module_02_1
//! Tests MTProto session, transport, sequence math, deduplication, framing, DH utils, and auth key derivation.

use gpui_telegram_core::mtproto::*;
use std::collections::HashSet;
use sha1::Sha1;
use sha2::{Digest, Sha256};

/// Test 1: Monotonicity and divisibility by 4 of message ID generation across clock anomalies
#[test]
fn test_adversarial_msg_id_clock_skew_and_monotonicity_oracle() {
    let mut session = Session::with_dc(2, 0x11, 0x22, 0x33);

    // Scenario A: Time jumps backwards (NTP clock step backwards)
    let timestamps = [
        1700000000i64,
        1700000000i64, // identical timestamp
        1699999990i64, // time rolled back 10 seconds!
        1699999900i64, // time rolled back 100 seconds!
        1700000010i64, // time jumped forward
        1700000005i64, // backward again
    ];

    let mut prev_id = 0u64;
    for &ts in &timestamps {
        let id = session.generate_msg_id(ts);
        // Client msg_id divisibility by 4
        assert_eq!(id & 3, 0, "msg_id {} must be divisible by 4", id);
        assert!(id > prev_id, "msg_id must be strictly monotonic: {} not > {}", id, prev_id);
        prev_id = id;
    }

    // Scenario B: Rapid burst of 10,000 IDs in the exact same millisecond
    let burst_ts = 1712345678i64;
    let mut seen = HashSet::with_capacity(10_000);
    for _ in 0..10_000 {
        let id = session.generate_msg_id(burst_ts);
        assert_eq!(id & 3, 0);
        assert!(id > prev_id);
        assert!(seen.insert(id), "msg_id collision: {}", id);
        prev_id = id;
    }
}

/// Test 2: Sequence number math and boundary rollover behavior
#[test]
fn test_adversarial_seq_no_rollover_and_overflow_oracle() {
    let mut private = SessionPrivate::with_dc(2, 0x11, 0x22, 0x33);

    // Initial counter = 0
    assert_eq!(private.messages_counter, 0);

    // Stress near u32 boundary: counter = 0x7FFF_FFFF
    private.messages_counter = 0x7fff_ffff;
    // Non-ack request: counter unchanged, seq_no = 2 * counter = 0xFFFF_FFFE (even)
    let seq_noack = private.next_request_seq_number(false);
    assert_eq!(seq_noack, 0xffff_fffe);
    assert_eq!(private.messages_counter, 0x7fff_ffff);

    // Ack request: seq_no = 2 * counter + 1 = 0xFFFF_FFFF (odd), counter increments to 0x8000_0000
    let seq_ack = private.next_request_seq_number(true);
    assert_eq!(seq_ack, 0xffff_ffff);
    assert_eq!(private.messages_counter, 0x8000_0000);

    // Rollover test: counter = 0xFFFF_FFFF
    private.messages_counter = 0xffff_ffff;
    let seq_overflow = private.next_request_seq_number(true);
    // 0xFFFF_FFFF * 2 + 1 wrapping in u32: 0xFFFF_FFFE + 1 = 0xFFFF_FFFF
    assert_eq!(seq_overflow, 0xffff_ffff);
    // counter wrapping_add(1) -> 0
    assert_eq!(private.messages_counter, 0);

    // Next request from 0
    let seq_next = private.next_request_seq_number(true);
    assert_eq!(seq_next, 1);
    assert_eq!(private.messages_counter, 1);
}

/// Test 3: ReceivedIdsManager circular buffer capacity (400) eviction with 600+ IDs
#[test]
fn test_adversarial_received_ids_eviction_and_deduplication_stress() {
    let mut manager = MtprotoReceivedIdsManagerDetails::with_capacity(400);
    assert_eq!(manager.buffer_capacity, 400);

    // Register 600 packets with monotonically increasing IDs
    let base_id = 1_000_000u64;
    for i in 0..600 {
        let msg_id = base_id + (i as u64) * 4;
        let res = manager.register_msg_id(msg_id, i % 2 == 0);
        assert_eq!(res, RegisterResult::Success);
    }

    // Buffer capacity must NOT exceed 400
    assert_eq!(manager.len(), 400);

    // First 200 items (0..200) should have been evicted
    let min_remaining = manager.min();
    let expected_min = base_id + (200 as u64) * 4;
    assert_eq!(min_remaining, expected_min, "Oldest kept item must be index 200");

    let max_remaining = manager.max();
    let expected_max = base_id + (599 as u64) * 4;
    assert_eq!(max_remaining, expected_max);

    // Test rejection of stale / evicted packets
    for i in 0..200 {
        let stale_id = base_id + (i as u64) * 4;
        let res = manager.register_msg_id(stale_id, false);
        assert_eq!(res, RegisterResult::TooOld, "Evicted ID {} must return TooOld", stale_id);
    }

    // Test rejection of duplicates among surviving packets
    for i in 200..600 {
        let dup_id = base_id + (i as u64) * 4;
        let res = manager.register_msg_id(dup_id, false);
        assert_eq!(res, RegisterResult::Duplicate, "Existing ID {} must return Duplicate", dup_id);
    }

    // Test pending ACKs
    let acks = manager.collect_pending_acks();
    // In items 200..600, even indices needed ack: 200 items
    assert_eq!(acks.len(), 200);

    // Subsequent collect must be empty
    assert!(manager.collect_pending_acks().is_empty());
}

/// Test 4: Wire framing & deframing edge cases: zero-length, fragmentation, oversized frames
#[test]
fn test_adversarial_wire_framing_and_fragmentation_stream() {
    let mut tcp = ConnectionTcp::new();

    // 1. Zero-length payload
    tcp.base.protocol = TransportProtocol::Abridged;
    let framed_empty = tcp.frame_packet(&[]);
    assert_eq!(framed_empty, vec![0x00]);
    let deframed_empty = tcp.deframe_packets(&framed_empty);
    assert_eq!(deframed_empty.len(), 1);
    assert_eq!(deframed_empty[0], Vec::<u8>::new());

    tcp.base.protocol = TransportProtocol::Intermediate;
    let framed_empty_inter = tcp.frame_packet(&[]);
    assert_eq!(framed_empty_inter, vec![0x00, 0x00, 0x00, 0x00]);
    let deframed_empty_inter = tcp.deframe_packets(&framed_empty_inter);
    assert_eq!(deframed_empty_inter.len(), 1);
    assert_eq!(deframed_empty_inter[0], Vec::<u8>::new());

    // 2. Fragmented packet arrival simulation
    let payload = vec![0xaa; 120]; // 30 words
    let framed = tcp.frame_packet(&payload);
    assert_eq!(framed.len(), 4 + 120);

    // Split stream into 3 fragments
    let frag1 = &framed[0..10];
    let frag2 = &framed[10..50];
    let frag3 = &framed[50..];

    // Feed fragment 1: incomplete header or body
    let mut stream_buffer = Vec::new();
    stream_buffer.extend_from_slice(frag1);
    let deframed = tcp.deframe_packets(&stream_buffer);
    assert_eq!(deframed.len(), 0, "Partial packet must yield 0 deframed packets");

    // Feed fragment 2: still incomplete body
    stream_buffer.extend_from_slice(frag2);
    let deframed = tcp.deframe_packets(&stream_buffer);
    assert_eq!(deframed.len(), 0, "Partial packet must yield 0 deframed packets");

    // Feed fragment 3: completed packet
    stream_buffer.extend_from_slice(frag3);
    let deframed = tcp.deframe_packets(&stream_buffer);
    assert_eq!(deframed.len(), 1);
    assert_eq!(deframed[0], payload);

    // 3. Oversized frame test (> 16MB)
    // MTProto protocol max packet is 16MB (0x01000000)
    let mut oversized_header = Vec::new();
    let oversized_len = 17 * 1024 * 1024u32; // 17MB
    oversized_header.extend_from_slice(&oversized_len.to_le_bytes());
    // Truncated buffer with oversized length header
    let deframed = tcp.deframe_packets(&oversized_header);
    assert_eq!(deframed.len(), 0, "Oversized truncated header must not produce packets");
}

/// Test 5: Diffie-Hellman factorize_pq stress testing with known Telegram test vectors
#[test]
fn test_adversarial_dh_factorize_pq_stress() {
    let dh_utils = MtprotoDhUtils::new();

    // 1. Telegram test vector: pq = 0x17ED48941A08F981 = 1724114452581128577
    // Factorization: 0x494C553B (1229782331) * 0x539247E3 (1402095587)
    let pq_tg = 0x17ED48941A08F981u64;
    let factors = dh_utils.factorize(pq_tg);
    assert!(factors.is_some(), "Telegram MTProto test vector PQ must be factorized");
    let (p, q) = factors.unwrap();
    assert_eq!(p * q, pq_tg);
    assert_eq!(p, 1229739323);
    assert_eq!(q, 1402015859);

    // 2. Small primes
    assert_eq!(dh_utils.factorize(35), Some((5, 7)));
    assert_eq!(dh_utils.factorize(15), Some((3, 5)));
    assert_eq!(dh_utils.factorize(77), Some((7, 11)));

    // 3. Even numbers
    assert_eq!(dh_utils.factorize(100), Some((2, 50)));

    // 4. Boundary primes (must return None, no infinite loop)
    assert_eq!(dh_utils.factorize(17), None);
    assert_eq!(dh_utils.factorize(1), None);
    assert_eq!(dh_utils.factorize(0), None);
}

/// Test 6: MTProto 2.0 AuthKey AES Key/IV derivation and msg_key calculation against upstream C++ spec
#[test]
fn test_adversarial_auth_key_derivation_oracle() {
    let mut raw_key = [0u8; 256];
    for i in 0..256 {
        raw_key[i] = ((i as u8).wrapping_mul(17)).wrapping_add(42);
    }
    let auth_key = MtprotoAuthKey::from_key_bytes(&raw_key, 2, AuthKeyType::Generated);

    let plaintext = b"Adversarial MTProto 2.0 Telegram Test Payload For Oracle Verification";

    // Client-to-server msg_key
    let client_msg_key = auth_key.compute_msg_key(plaintext, true);
    // Server-to-client msg_key
    let server_msg_key = auth_key.compute_msg_key(plaintext, false);
    assert_ne!(client_msg_key, server_msg_key);

    // Prepare AES keys
    let (client_aes_key, client_aes_iv) = auth_key.prepare_aes(&client_msg_key, true);
    let (server_aes_key, server_aes_iv) = auth_key.prepare_aes(&server_msg_key, false);

    assert_eq!(client_aes_key.len(), 32);
    assert_eq!(client_aes_iv.len(), 32);
    assert_eq!(server_aes_key.len(), 32);
    assert_eq!(server_aes_iv.len(), 32);

    // Verify against manual SHA256 Oracle following tdesktop prepareAES
    // sha256_a = SHA256(msg_key + key[0..36])
    let mut ha = Sha256::new();
    ha.update(&client_msg_key);
    ha.update(&raw_key[0..36]);
    let sha256_a = ha.finalize();

    // sha256_b = SHA256(key[40..76] + msg_key)
    let mut hb = Sha256::new();
    hb.update(&raw_key[40..76]);
    hb.update(&client_msg_key);
    let sha256_b = hb.finalize();

    let mut expected_aes_key = [0u8; 32];
    expected_aes_key[0..8].copy_from_slice(&sha256_a[0..8]);
    expected_aes_key[8..24].copy_from_slice(&sha256_b[8..24]);
    expected_aes_key[24..32].copy_from_slice(&sha256_a[24..32]);

    let mut expected_aes_iv = [0u8; 32];
    expected_aes_iv[0..8].copy_from_slice(&sha256_b[0..8]);
    expected_aes_iv[8..24].copy_from_slice(&sha256_a[8..24]);
    expected_aes_iv[24..32].copy_from_slice(&sha256_b[24..32]);

    assert_eq!(client_aes_key, expected_aes_key, "Derived AES key must match tdesktop prepareAES oracle");
    assert_eq!(client_aes_iv, expected_aes_iv, "Derived AES IV must match tdesktop prepareAES oracle");
}

/// Test 7: MTProto 2.0 msg_key middle 128-bit specification compliance (tdesktop session_private.cpp:2738)
#[test]
fn test_adversarial_msg_key_middle_128_bits_oracle() {
    let mut raw_key = [0u8; 256];
    for i in 0..256 {
        raw_key[i] = ((i as u8).wrapping_mul(17)).wrapping_add(42);
    }
    let auth_key = MtprotoAuthKey::from_key_bytes(&raw_key, 2, AuthKeyType::Generated);
    let plaintext = b"Adversarial MTProto 2.0 Telegram Test Payload For Oracle Verification";

    let actual_msg_key = auth_key.compute_msg_key(plaintext, true);

    // According to Telegram MTProto 2.0 specification:
    // msg_key_large = SHA256 (substr (auth_key, 88+x, 32) + plaintext + random_padding);
    // msg_key = substr (msg_key_large, 8, 16); (middle 128 bits)
    // And tdesktop session_private.cpp:2738:
    // uchar encryptedSHA256[32];
    // MTPint128 &msgKey(*(MTPint128*)(encryptedSHA256 + 8));
    let mut hasher = Sha256::new();
    hasher.update(&raw_key[88..88 + 32]);
    hasher.update(plaintext);
    let full_hash = hasher.finalize();

    let mut expected_tdesktop_msg_key = [0u8; 16];
    expected_tdesktop_msg_key.copy_from_slice(&full_hash[8..24]);

    assert_eq!(
        actual_msg_key,
        expected_tdesktop_msg_key,
        "FAIL: compute_msg_key sliced hash[0..16] instead of hash[8..24] (middle 128-bits per MTProto 2.0 spec and tdesktop session_private.cpp:2738)"
    );
}

/// Test 8: MTProto 2.0 auth_key_id calculation against upstream C++ spec (tdesktop mtproto_auth_key.cpp:146)
#[test]
fn test_adversarial_key_id_sha1_oracle() {
    let mut raw_key = [0u8; 256];
    for i in 0..256 {
        raw_key[i] = ((i as u8).wrapping_mul(31)).wrapping_add(7);
    }
    let auth_key = MtprotoAuthKey::from_key_bytes(&raw_key, 2, AuthKeyType::Generated);

    // Upstream: tdesktop mtproto_auth_key.cpp:146:
    // void AuthKey::countKeyId() {
    //     const auto hash = openssl::Sha1(_key);
    //     // Lower 64 bits = 8 bytes of 20 byte SHA1 hash.
    //     _keyId = qFromUnaligned<KeyId>(hash.data() + 12);
    // }
    let mut hasher = Sha1::new();
    hasher.update(&raw_key);
    let sha1_hash = hasher.finalize();

    let mut expected_bytes = [0u8; 8];
    expected_bytes.copy_from_slice(&sha1_hash[12..20]);
    let expected_key_id = u64::from_le_bytes(expected_bytes);

    assert_eq!(
        auth_key.key_id, expected_key_id,
        "AuthKey key_id must match lower 64 bits of SHA-1 hash (tdesktop mtproto_auth_key.cpp:146)"
    );
    assert_eq!(
        MtprotoAuthKey::compute_key_id(&raw_key),
        expected_key_id,
        "compute_key_id must match lower 64 bits of SHA-1 hash"
    );
}


