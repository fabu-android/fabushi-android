//! Comprehensive Unit & Integration Tests for Module 02 API Client & RPC Subsystem
//! Covers:
//! - RPC request serialization and rate limiter mechanics
//! - Updates sequence routing, PTS tracking & gap detection
//! - PTS synchronization events & channel differences
//! - Reaction addition/removal & who reacted state
//! - Chat invite link validation, creation & revocation
//! - Chat participants rights and promotion
//! - User privacy, authorizations & hash calculation

use gpui_telegram_proto::api::*;
use gpui_telegram_proto::cards::PollCard;
use gpui_telegram_proto::message::MessageEntityType;

#[test]
fn test_rpc_request_serialization_and_rate_limiting() {
    let api_common = ApiCommon::new();
    assert!(api_common.is_active());

    // 1. Monotonic request ID generation
    let req_id_1 = ApiCommon::next_req_id();
    let req_id_2 = ApiCommon::next_req_id();
    assert!(req_id_2 > req_id_1, "Request IDs must monotonically increase");

    // 2. Scheduled options & SendAction construction
    let mut options = SendOptions::default();
    options.scheduled = 1700000000;
    options.silent = true;
    assert!(options.is_scheduled());
    assert!(!options.is_scheduled_until_online());

    let online_opts = default_send_when_online_options();
    assert!(online_opts.is_scheduled_until_online());
    assert_eq!(online_opts.scheduled, K_SCHEDULED_UNTIL_ONLINE_TIMESTAMP);

    let action = SendAction::with_reply(1001, 42);
    assert_eq!(action.peer_id, 1001);
    assert_eq!(action.reply_to_msg_id, Some(42));
    assert!(action.clear_draft);

    let msg_to_send = MessageToSend::new(action, "Hello from GPUI Telegram!");
    assert_eq!(msg_to_send.text, "Hello from GPUI Telegram!");

    // 3. Serde roundtrip
    let json_str = serde_json::to_string(&msg_to_send).expect("Must serialize MessageToSend");
    let deserialized: MessageToSend = serde_json::from_str(&json_str).expect("Must deserialize MessageToSend");
    assert_eq!(msg_to_send, deserialized);

    // 4. Rate limiter leaky bucket mechanics
    let mut limiter = RateLimiter::new(3.0, 1.0);
    assert!(limiter.try_acquire());
    assert!(limiter.try_acquire());
    assert!(limiter.try_acquire());
    assert!(!limiter.try_acquire(), "Exhausted tokens must be rejected");
}

#[test]
fn test_updates_sequence_routing_and_gap_detection() {
    let mut updates = ApiUpdates::with_pts(100, 1700000000);
    assert_eq!(updates.pts(), 100);
    assert_eq!(updates.seq(), 0);
    assert!(!updates.requesting_difference());

    // 1. Contiguous sequential update
    // Update pts: 105, pts_count: 5 -> prev_pts: 100 == current pts
    let applied = updates.update_and_apply(105, 5);
    assert!(applied, "Contiguous update must be applied immediately");
    assert_eq!(updates.pts(), 105);
    assert!(!updates.requesting_difference());

    // 2. Duplicate or stale update
    let stale = updates.update_and_apply(103, 1);
    assert!(!stale, "Stale update (pts <= current) must return false");
    assert_eq!(updates.pts(), 105);

    // 3. Sequence gap detection
    // Incoming pts: 120, pts_count: 2 -> prev_pts: 118 > current pts: 105
    let gap_detected = updates.update_and_apply(120, 2);
    assert!(!gap_detected, "Gap update must not be applied directly");
    assert!(updates.requesting_difference(), "Must flag requesting difference on gap");

    // 4. Difference completion
    updates.finish_difference(150, 1700001000, 12);
    assert_eq!(updates.pts(), 150);
    assert_eq!(updates.seq(), 12);
    assert!(!updates.requesting_difference(), "Requesting difference cleared");

    // 5. Active chat tracking
    assert!(!updates.in_active_chats(999));
    updates.add_active_chat(999);
    assert!(updates.in_active_chats(999));

    // 6. Channel PTS independent tracking
    let ch_applied = updates.update_channel_pts(5001, 10, 10);
    assert!(ch_applied);
    let ch_gap = updates.update_channel_pts(5001, 25, 2);
    assert!(!ch_gap, "Channel gap detected");
    assert!(updates.pending_channel_diffs.contains(&5001));
}

#[test]
fn test_filter_updates_and_read_metrics() {
    let mut filter = ApiFilterUpdates::new();
    assert!(filter.matches_type("UpdateNewMessage"));
    assert!(filter.matches_type("UpdateUserStatus"));

    filter.set_filter(UpdateTypeFilter::MessagesOnly);
    assert!(filter.matches_type("UpdateNewMessage"));
    assert!(!filter.matches_type("UpdateUserStatus"));

    let mut metrics = ApiReadMetrics::new();
    metrics.add(1001, FinalizedReadMetric {
        msg_id: 42,
        view_id: 1,
        time_in_view_ms: 2500,
        active_time_in_view_ms: 2000,
        height_to_viewport_ratio_permille: 500,
        seen_range_ratio_permille: 1000,
    });
    assert_eq!(metrics.pending_count(1001), 1);
    assert_eq!(metrics.total_recorded, 1);

    let taken = metrics.take_pending(1001);
    assert_eq!(taken.len(), 1);
    assert_eq!(metrics.pending_count(1001), 0);
}

#[test]
fn test_reaction_addition_removal_and_notifications() {
    let mut who_reacted = ApiWhoReacted::new();
    let peer_id = 777;
    let msg_id = 12345;

    assert!(who_reacted.get_reactors(peer_id, msg_id).is_empty());

    // 1. Add reaction
    who_reacted.add_reactor(peer_id, msg_id, 101, "👍");
    who_reacted.add_reactor(peer_id, msg_id, 102, "🔥");

    let reactors = who_reacted.get_reactors(peer_id, msg_id);
    assert_eq!(reactors.len(), 2);
    assert_eq!(reactors[0].reaction.as_deref(), Some("👍"));
    assert_eq!(reactors[1].reaction.as_deref(), Some("🔥"));

    // 2. Change reaction
    who_reacted.add_reactor(peer_id, msg_id, 101, "❤️");
    let updated_reactors = who_reacted.get_reactors(peer_id, msg_id);
    assert_eq!(updated_reactors.len(), 2);
    assert_eq!(updated_reactors[0].reaction.as_deref(), Some("❤️"));

    // 3. Remove reaction
    let removed = who_reacted.remove_reactor(peer_id, msg_id, 102);
    assert!(removed);
    assert_eq!(who_reacted.get_reactors(peer_id, msg_id).len(), 1);

    // 4. Notifications settings
    let mut notify = ApiReactionsNotifySettings::new();
    assert_eq!(notify.messages_from, ReactionsNotifyFrom::All);
    notify.update_messages_from(ReactionsNotifyFrom::Contacts);
    assert_eq!(notify.messages_from, ReactionsNotifyFrom::Contacts);
    notify.update_show_previews(false);
    assert!(!notify.show_previews);
}

#[test]
fn test_chat_invite_link_validation_and_lifecycle() {
    // 1. Chat invite checking
    let mut chat_invite = ApiChatInvite::new();
    let info = chat_invite.check_chat_invite("AbCdEf123").expect("Must resolve");
    assert_eq!(info.hash, "AbCdEf123");
    assert!(!info.is_channel);

    let joined = chat_invite.import_chat_invite("AbCdEf123");
    assert!(joined);
    let duplicate_join = chat_invite.import_chat_invite("AbCdEf123");
    assert!(!duplicate_join, "Duplicate join must return false");

    // 2. Chat Links
    let mut chat_links = ApiChatLinks::new();
    let link = chat_links.create("Test Invite", "Welcome to the group!");
    assert!(link.link.starts_with("https://t.me/+link_"));
    assert_eq!(link.title, "Test Invite");

    let edited = chat_links.edit(&link.link, "Updated Title", "Updated message");
    assert!(edited.is_some());
    assert_eq!(edited.unwrap().title, "Updated Title");

    let destroyed = chat_links.destroy(&link.link);
    assert!(destroyed);

    // 3. Invite Links with revocations
    let mut invite_links = ApiInviteLinks::new();
    let created_invite = invite_links.create(CreateInviteLinkArgs {
        peer_id: 888,
        label: "VIP Group".to_string(),
        expire_date: 1800000000,
        usage_limit: 50,
        request_approval: true,
    });
    assert!(!created_invite.revoked);
    assert!(created_invite.request_approval);

    let revoked = invite_links.revoke(888, &created_invite.link);
    assert!(revoked);
    let peer_links = invite_links.peer_links.get(&888).unwrap();
    assert!(peer_links[0].revoked);
}

#[test]
fn test_chat_participants_and_admin_rights() {
    let mut participants = ApiChatParticipants::new();
    let channel_id = 9999;

    let rights = ChatAdminRights {
        change_info: true,
        post_messages: true,
        edit_messages: true,
        delete_messages: true,
        ban_users: true,
        invite_users: true,
        pin_messages: true,
        add_admins: false,
        anonymous: false,
        manage_call: true,
        manage_topics: true,
        post_stories: false,
        edit_stories: false,
        delete_stories: false,
    };

    participants.promote_admin(channel_id, 1001, rights, Some("Moderator".to_string()));
    let list = participants.channel_participants.get(&channel_id).unwrap();
    assert_eq!(list.len(), 1);
    assert!(list[0].is_creator_or_admin());
    assert_eq!(list[0].rank.as_deref(), Some("Moderator"));

    participants.kick_participant(channel_id, 1001);
    let after_kick = participants.channel_participants.get(&channel_id).unwrap();
    assert!(after_kick.is_empty());
}

#[test]
fn test_authorizations_privacy_and_hash() {
    // 1. Authorizations management
    let mut auths = ApiAuthorizations::new();
    auths.set_sessions(vec![
        AuthorizationEntry {
            hash: 1001,
            incomplete: false,
            calls_disabled: false,
            api_id: 2040,
            active_time: 1700000000,
            app_name: "Telegram Desktop GPUI".to_string(),
            app_version: "5.0.0".to_string(),
            ip: "127.0.0.1".to_string(),
            country: "US".to_string(),
            region: "California".to_string(),
            system_version: "macOS 15.1".to_string(),
            platform: "macOS".to_string(),
            device_model: "MacBook Pro".to_string(),
        },
        AuthorizationEntry {
            hash: 1002,
            incomplete: false,
            calls_disabled: false,
            api_id: 2040,
            active_time: 1690000000,
            app_name: "Telegram iOS".to_string(),
            app_version: "10.0".to_string(),
            ip: "10.0.0.1".to_string(),
            country: "DE".to_string(),
            region: "Berlin".to_string(),
            system_version: "iOS 17".to_string(),
            platform: "iOS".to_string(),
            device_model: "iPhone 15".to_string(),
        },
    ]);
    assert_eq!(auths.sessions.len(), 2);

    let terminated = auths.terminate(1002);
    assert!(terminated);
    assert_eq!(auths.sessions.len(), 1);

    // 2. User Privacy
    let mut privacy = ApiUserPrivacy::new();
    privacy.set_rule(PrivacyKey::PhoneNumber, PrivacyRule {
        option: PrivacyOption::Nobody,
        always: PrivacyExceptions::default(),
        never: PrivacyExceptions::default(),
        ignore_always: false,
        ignore_never: false,
    });
    assert_eq!(privacy.get_rule(PrivacyKey::PhoneNumber).option, PrivacyOption::Nobody);

    // 3. Upstream authentic Bit-shift hash calculation
    let hash_1 = count_hash([101u64, 102u64, 103u64]);
    let hash_2 = count_hash([101u64, 102u64, 103u64]);
    let hash_diff = count_hash([101u64, 102u64, 104u64]);
    assert_eq!(hash_1, hash_2, "Hash must be deterministic");
    assert_ne!(hash_1, hash_diff, "Different items must produce different hashes");

    let mut api_hash = ApiHash::new();
    let computed = api_hash.compute_hash([1u64, 2u64, 3u64]);
    assert_eq!(computed, api_hash.last_computed_hash);
}

#[test]
fn test_polls_and_media_toggle() {
    let mut polls = ApiPolls::new();
    let card = PollCard {
        id: "poll_1".to_string(),
        poll_id: 5555,
        question: "Which UI framework is best?".to_string(),
        options: vec![],
        answers: vec![],
        total_voters: 10,
        total_votes: 10,
        is_closed: false,
        is_anonymous: true,
        allows_multiple_answers: false,
        is_quiz: false,
        correct_option_id: None,
        solution: None,
        solution_entities: vec![],
        close_period: None,
        close_date: None,
    };

    polls.register_poll(101, 202, card);
    polls.send_votes(101, 202, vec![vec![1, 2, 3]]);
    polls.close_poll(101, 202);
    assert!(polls.closed_polls.contains(&(101, 202)));

    let mut media = ApiTogglingMedia::new();
    media.toggle_faved_sticker(99901, true);
    assert!(media.faved_stickers.contains(&99901));
    media.toggle_faved_sticker(99901, false);
    assert!(!media.faved_stickers.contains(&99901));
}

#[test]
fn test_text_entities_spoiler_parsing() {
    let raw = "Check this ||secret spoiler|| text";
    let parsed = ApiTextEntities::parse_markdown_spoilers(raw);
    assert_eq!(parsed.text, "Check this secret spoiler text");
    assert_eq!(parsed.entities.len(), 1);
    assert_eq!(parsed.entities[0].entity_type, MessageEntityType::Spoiler);
    assert_eq!(parsed.entities[0].offset, 11);
    assert_eq!(parsed.entities[0].length, 14);
}
