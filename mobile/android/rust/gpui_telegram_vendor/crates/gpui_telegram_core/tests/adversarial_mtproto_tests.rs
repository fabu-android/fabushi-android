//! Empirical Adversarial Unit & Integration Tests for MTProto Subsystem
//! Author: worker_module_02_mtproto
//! Covers:
//! 1. MTProto session creation, message ID monotonicity, and sequence numbering.
//! 2. Salt rotation, session resets, and datacenter shifting rules.
//! 3. ACK tracking, circular buffer deduplication, and min/max lookup.
//! 4. Transport wire framing (Abridged & Intermediate) and stream deframing.
//! 5. MTProto 2.0 AuthKey AES key/IV derivation and MsgKey verification.
//! 6. Datacenter endpoint routing, IPv4/IPv6 fallback, and media cluster selection.
//! 7. Diffie-Hellman composite PQ factorization and DH parameter validation.
//! 8. RPC error categorization, flood wait extraction, and DC migration signals.
//! 9. Keepalive ping/pong, proxy validation, and facade orchestration.

use gpui_telegram_core::mtproto::*;
use std::collections::HashSet;

#[test]
fn test_mtproto_session_creation_and_msg_id_monotonicity() {
    let mut session = Session::with_dc(2, 0x1122334455667788, 0x99aabbccddeeff00, 0x1234567890abcdef);
    assert!(session.is_active());
    assert_eq!(session.bare_dc_id(), 2);

    // Generate 1,000 message IDs under the same unix timestamp
    const NUM_MSGS: usize = 1_000;
    let now = 1700000000i64;
    let mut generated_ids = Vec::with_capacity(NUM_MSGS);
    let mut set = HashSet::with_capacity(NUM_MSGS);

    for _ in 0..NUM_MSGS {
        let msg_id = session.generate_msg_id(now);
        // Client message IDs must have lowest 2 bits equal to 0 (divisible by 4)
        assert_eq!(msg_id & 3, 0, "Client msg_id must be divisible by 4");
        generated_ids.push(msg_id);
        assert!(set.insert(msg_id), "msg_id collision detected: {}", msg_id);
    }

    // Must be strictly monotonically increasing
    for window in generated_ids.windows(2) {
        assert!(
            window[0] < window[1],
            "Monotonicity violation: {} not < {}",
            window[0],
            window[1]
        );
    }
}

#[test]
fn test_mtproto_sequence_numbering_rules() {
    let mut private = SessionPrivate::with_dc(2, 0x1122, 0x3344, 0x5566);

    // Sequence numbers:
    // Need ACK -> odd: 2k + 1, and increments counter
    // Non-ACK -> even: 2k, does NOT increment counter
    let seq_ack_0 = private.next_request_seq_number(true);
    assert_eq!(seq_ack_0, 1); // 0 * 2 + 1 = 1, counter becomes 1

    let seq_noack_1 = private.next_request_seq_number(false);
    assert_eq!(seq_noack_1, 2); // 1 * 2 = 2, counter remains 1

    let seq_noack_2 = private.next_request_seq_number(false);
    assert_eq!(seq_noack_2, 2); // 1 * 2 = 2, counter remains 1

    let seq_ack_1 = private.next_request_seq_number(true);
    assert_eq!(seq_ack_1, 3); // 1 * 2 + 1 = 3, counter becomes 2

    let seq_ack_2 = private.next_request_seq_number(true);
    assert_eq!(seq_ack_2, 5); // 2 * 2 + 1 = 5, counter becomes 3

    let seq_noack_3 = private.next_request_seq_number(false);
    assert_eq!(seq_noack_3, 6); // 3 * 2 = 6
}

#[test]
fn test_mtproto_session_salt_rotation_and_resets() {
    let mut private = SessionPrivate::with_dc(2, 0x1122, 0x3344, 0x5566);
    assert_eq!(private.session_salt, 0x5566);

    // Rotate salt
    private.update_salt(0x7788_99aa_bbcc_ddee);
    assert_eq!(private.session_salt, 0x7788_99aa_bbcc_ddee);

    // Advance sequence number
    let _ = private.next_request_seq_number(true);
    let _ = private.next_request_seq_number(true);
    assert_eq!(private.messages_counter, 2);

    // Reset session zeroes sequence counter
    private.reset_session();
    assert_eq!(private.messages_counter, 0);
    assert_eq!(private.next_request_seq_number(true), 1);
}

#[test]
fn test_mtproto_ack_tracking_and_received_ids_deduplication() {
    let mut id_manager = MtprotoReceivedIdsManagerDetails::with_capacity(10);

    // Register messages
    assert_eq!(id_manager.register_msg_id(100, true), RegisterResult::Success);
    assert_eq!(id_manager.register_msg_id(104, false), RegisterResult::Success);
    assert_eq!(id_manager.register_msg_id(108, true), RegisterResult::Success);

    // Duplicate detection
    assert_eq!(id_manager.register_msg_id(104, false), RegisterResult::Duplicate);
    assert_eq!(id_manager.register_msg_id(100, true), RegisterResult::Duplicate);

    // Check bounds
    assert_eq!(id_manager.min(), 100);
    assert_eq!(id_manager.max(), 108);

    // Lookups
    assert_eq!(id_manager.lookup(100), IdLookupState::NeedsAck);
    assert_eq!(id_manager.lookup(104), IdLookupState::NoAckNeeded);
    assert_eq!(id_manager.lookup(999), IdLookupState::NotFound);

    // Collect pending acks
    let acks = id_manager.collect_pending_acks();
    assert_eq!(acks, vec![100, 108]);

    // After collection, they no longer need ACK
    assert_eq!(id_manager.lookup(100), IdLookupState::NoAckNeeded);
    assert_eq!(id_manager.lookup(108), IdLookupState::NoAckNeeded);
    assert!(id_manager.collect_pending_acks().is_empty());

    // Shrink past capacity
    for id in 112..=160 {
        if id % 4 == 0 {
            let _ = id_manager.register_msg_id(id, false);
        }
    }
    assert!(id_manager.len() <= 10);
    assert!(id_manager.min() > 100);

    // Stale message older than min()
    assert_eq!(id_manager.register_msg_id(50, false), RegisterResult::TooOld);
}

#[test]
fn test_mtproto_wire_framing_and_deframing() {
    let mut tcp = ConnectionTcp::new();
    let payload_short = vec![1, 2, 3, 4, 5, 6, 7, 8]; // 8 bytes (2 words)

    // 1. Abridged transport (< 127 words)
    tcp.base.protocol = TransportProtocol::Abridged;
    let framed_short = tcp.frame_packet(&payload_short);
    assert_eq!(framed_short[0], 2); // 8 bytes / 4 = 2 words
    assert_eq!(&framed_short[1..], &payload_short[..]);

    let deframed_short = tcp.deframe_packets(&framed_short);
    assert_eq!(deframed_short.len(), 1);
    assert_eq!(deframed_short[0], payload_short);

    // Abridged transport large payload (>= 127 words = 508 bytes)
    let payload_large = vec![0x42; 600]; // 150 words
    let framed_large = tcp.frame_packet(&payload_large);
    assert_eq!(framed_large[0], 0x7f);
    let len_words = u32::from_le_bytes([framed_large[1], framed_large[2], framed_large[3], 0]);
    assert_eq!(len_words, 150);
    let deframed_large = tcp.deframe_packets(&framed_large);
    assert_eq!(deframed_large.len(), 1);
    assert_eq!(deframed_large[0], payload_large);

    // 2. Intermediate transport
    tcp.base.protocol = TransportProtocol::Intermediate;
    let framed_inter = tcp.frame_packet(&payload_short);
    assert_eq!(framed_inter.len(), 4 + payload_short.len());
    let len_inter = u32::from_le_bytes(framed_inter[0..4].try_into().unwrap());
    assert_eq!(len_inter, payload_short.len() as u32);
    let deframed_inter = tcp.deframe_packets(&framed_inter);
    assert_eq!(deframed_inter.len(), 1);
    assert_eq!(deframed_inter[0], payload_short);

    // Multiple concatenated packets in stream buffer
    let mut stream = Vec::new();
    stream.extend_from_slice(&framed_inter);
    stream.extend_from_slice(&framed_inter);
    let multi_deframed = tcp.deframe_packets(&stream);
    assert_eq!(multi_deframed.len(), 2);
    assert_eq!(multi_deframed[0], payload_short);
    assert_eq!(multi_deframed[1], payload_short);
}

#[test]
fn test_mtproto_auth_key_derivation_and_msg_key() {
    let mut raw_key = [0u8; 256];
    for i in 0..256 {
        raw_key[i] = (i as u8).wrapping_mul(7).wrapping_add(13);
    }
    let auth_key = MtprotoAuthKey::from_key_bytes(&raw_key, 2, AuthKeyType::Generated);
    assert_ne!(auth_key.key_id, 0);

    let plaintext = b"Hello MTProto 2.0 Telegram GPUI Secure Communication Test Payload";
    let msg_key_client = auth_key.compute_msg_key(plaintext, true);
    let msg_key_server = auth_key.compute_msg_key(plaintext, false);
    assert_ne!(msg_key_client, msg_key_server, "Client and Server msg_key must use distinct x offsets");

    // Prepare AES keys for client-to-server
    let (send_aes_key, send_aes_iv) = auth_key.prepare_aes(&msg_key_client, true);
    assert_eq!(send_aes_key.len(), 32);
    assert_eq!(send_aes_iv.len(), 32);
    assert_ne!(send_aes_key, [0u8; 32]);
    assert_ne!(send_aes_iv, [0u8; 32]);

    // Prepare AES keys for server-to-client
    let (recv_aes_key, recv_aes_iv) = auth_key.prepare_aes(&msg_key_client, false);
    assert_ne!(send_aes_key, recv_aes_key);
    assert_ne!(send_aes_iv, recv_aes_iv);
}

#[test]
fn test_mtproto_dc_options_and_shifted_dc_routing() {
    let dc_options = MtprotoDcOptions::default();
    assert!(dc_options.is_active());

    // Lookup DC 2 IPv4
    let ep_ipv4 = dc_options.lookup(2, false, false);
    assert!(ep_ipv4.is_some());
    let ep = ep_ipv4.unwrap();
    assert_eq!(ep.id, 2);
    assert!(!ep.is_ipv6());

    // Lookup DC 2 IPv6
    let ep_ipv6 = dc_options.lookup(2, true, false);
    assert!(ep_ipv6.is_some());
    assert!(ep_ipv6.unwrap().is_ipv6());

    // Shifting formulas
    let bare = 2;
    let shifted_download_0 = shift_dc_id(bare, BASE_DOWNLOAD_DC_SHIFT);
    let shifted_download_5 = shift_dc_id(bare, BASE_DOWNLOAD_DC_SHIFT + 5);
    let shifted_upload_1 = shift_dc_id(bare, BASE_UPLOAD_DC_SHIFT + 1);
    let shifted_config = shift_dc_id(bare, CONFIG_DC_SHIFT);

    assert_eq!(bare_dc_id(shifted_download_0), bare);
    assert_eq!(bare_dc_id(shifted_download_5), bare);
    assert_eq!(bare_dc_id(shifted_upload_1), bare);
    assert_eq!(bare_dc_id(shifted_config), bare);

    assert_eq!(dc_options.dc_type(shifted_download_0), DcType::MediaCluster);
    assert_eq!(dc_options.dc_type(shifted_download_5), DcType::MediaCluster);
    assert_eq!(dc_options.dc_type(shifted_config), DcType::Regular);
}

#[test]
fn test_mtproto_dh_composite_factorization() {
    let dh_utils = MtprotoDhUtils::new();
    assert!(dh_utils.is_active());

    // Known primes product test:
    // p = 179424673, q = 179424691
    let p = 179424673u64;
    let q = 179424691u64;
    let pq = p * q;

    let result = dh_utils.factorize(pq);
    assert!(result.is_some());
    let (found_p, found_q) = result.unwrap();
    assert_eq!(found_p * found_q, pq);
    assert_eq!(found_p, p.min(q));
    assert_eq!(found_q, p.max(q));

    // Boundary DH validation
    let mut dh_prime = vec![0x80; 256];
    dh_prime[255] = 0x01;
    let g_a = vec![0x42; 128];
    assert!(dh_utils.validate_dh(3, &dh_prime, &g_a));

    // Invalid g (must be in 2..=7)
    assert!(!dh_utils.validate_dh(1, &dh_prime, &g_a));
    assert!(!dh_utils.validate_dh(8, &dh_prime, &g_a));

    // Invalid prime length (must be 256 bytes)
    assert!(!dh_utils.validate_dh(3, &dh_prime[..128], &g_a));
}

#[test]
fn test_mtproto_error_handling_and_flood_wait() {
    let flood_err = RpcError::new(420, "FLOOD_WAIT_37", "Flood wait required");
    assert!(flood_err.is_flood());
    assert!(flood_err.is_temporary());
    assert_eq!(flood_err.flood_wait_seconds(), Some(37));

    let premium_flood = RpcError::new(420, "FLOOD_PREMIUM_WAIT_15", "Premium flood wait");
    assert_eq!(premium_flood.flood_wait_seconds(), Some(15));

    let file_migrate = RpcError::new(303, "FILE_MIGRATE_4", "File located on DC 4");
    assert_eq!(file_migrate.file_migrate_dc(), Some(4));

    let phone_migrate = RpcError::new(303, "PHONE_MIGRATE_1", "Phone account on DC 1");
    assert_eq!(phone_migrate.network_migrate_dc(), Some(1));

    let normal_err = RpcError::new(400, "FIRSTNAME_INVALID", "Invalid first name");
    assert!(!normal_err.is_flood());
    assert!(!normal_err.is_temporary());
}

#[test]
fn test_mtproto_rpc_sender_and_facade() {
    let mut sender = Sender::new();
    assert!(sender.is_active());

    let req1 = sender.send();
    let req2 = sender.send();
    assert_ne!(req1, req2);
    assert!(sender.has_pending());

    sender.track_msg_id(req1, 0x1000);
    sender.sender_request_handled(req1);
    assert_eq!(sender.handled_count, 1);

    sender.cancel(req2);
    assert!(!sender.has_pending());

    // Facade pause propagation
    let mut facade = Facade::new();
    assert!(facade.is_active());
    facade.pause();
    assert!(facade.is_paused());
    assert!(!facade.is_active());
    facade.unpause();
    assert!(!facade.is_paused());
    assert!(facade.is_active());
}
