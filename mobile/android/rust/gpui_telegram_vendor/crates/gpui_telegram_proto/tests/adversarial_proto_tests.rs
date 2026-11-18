use gpui_telegram_core::types::{ChatId, MessageId, UserId};
use gpui_telegram_proto::cards::{
    InvoiceCard, PaymentStatus, PollCard, PollOption, PriceLine, TodoCard, TodoItem,
};
use gpui_telegram_proto::message::{MessageEntity, MessageEntityType};
use gpui_telegram_proto::sync::{
    PtsUpdate, SyncGetDifferenceRequest, TypingAction, TypingEvent, UpdateEvent,
};
use std::collections::{BTreeMap, HashSet};

// ============================================================================
// 1. Adversarial PTS Sequence Gap & Continuity Engine Tests
// ============================================================================

#[test]
fn test_pts_ephemeral_zero_count_advanced_invariants() {
    let chat = ChatId(42);
    let typing = UpdateEvent::UserTyping(TypingEvent::new(chat, UserId(1), TypingAction::Typing));

    // Ephemeral update with pts_count = 0 at pts = 100
    let ephemeral = PtsUpdate::new(chat, 100, 0, typing.clone(), 1000);
    assert_eq!(ephemeral.prev_pts(), 100);

    // Case 1: Client is strictly ahead of or equal to ephemeral update PTS
    // current_pts = 100 -> continuous
    assert!(ephemeral.is_continuous(100));
    assert_eq!(ephemeral.gap(100), None);
    assert!(!ephemeral.is_stale_or_duplicate(100));

    // current_pts = 105 -> continuous (client has already seen up to 105)
    assert!(ephemeral.is_continuous(105));
    assert_eq!(ephemeral.gap(105), None);
    assert!(!ephemeral.is_stale_or_duplicate(105));

    // Case 2: Client is BEHIND ephemeral update PTS
    // Client is at 95, but ephemeral has pts = 100
    // Client cannot safely apply this without knowing about updates between 95 and 100!
    assert!(!ephemeral.is_continuous(95));
    // Gap should cover missing updates: (95 + 1)..=100 -> 96..=100
    assert_eq!(ephemeral.gap(95), Some(96..=100));
    assert!(!ephemeral.is_stale_or_duplicate(95));

    // Case 3: Ephemeral at pts = 0 (e.g. client connection initialization)
    let init_ephemeral = PtsUpdate::new(chat, 0, 0, typing, 1000);
    assert!(init_ephemeral.is_continuous(0));
    assert_eq!(init_ephemeral.gap(0), None);
    assert!(!init_ephemeral.is_stale_or_duplicate(0));
}

#[test]
fn test_pts_out_of_order_and_multi_packet_gaps() {
    let chat = ChatId(10);
    let event = UpdateEvent::ReadHistoryOut {
        max_id: MessageId(1),
    };

    // Client at PTS 50
    let mut current_pts = 50u64;

    // Packet 3 arrives prematurely: pts=60, count=2 -> prev_pts=58
    let p3 = PtsUpdate::new(chat, 60, 2, event.clone(), 1003);
    assert!(!p3.is_continuous(current_pts));
    assert_eq!(p3.gap(current_pts), Some(51..=58));

    // Packet 2 arrives prematurely: pts=58, count=3 -> prev_pts=55
    let p2 = PtsUpdate::new(chat, 58, 3, event.clone(), 1002);
    assert!(!p2.is_continuous(current_pts));
    assert_eq!(p2.gap(current_pts), Some(51..=55));

    // Packet 1 arrives (the missing link): pts=55, count=5 -> prev_pts=50
    let p1 = PtsUpdate::new(chat, 55, 5, event.clone(), 1001);
    assert!(p1.is_continuous(current_pts));
    assert_eq!(p1.gap(current_pts), None);

    // Apply Packet 1
    current_pts = p1.pts; // 55
    assert_eq!(current_pts, 55);

    // Now Packet 2 is continuous with current_pts = 55!
    assert!(p2.is_continuous(current_pts));
    current_pts = p2.pts; // 58
    assert_eq!(current_pts, 58);

    // Now Packet 3 is continuous with current_pts = 58!
    assert!(p3.is_continuous(current_pts));
    current_pts = p3.pts; // 60
    assert_eq!(current_pts, 60);

    // Duplicate arrival of p1, p2, p3 must now be rejected as stale
    assert!(p1.is_stale_or_duplicate(current_pts));
    assert!(p2.is_stale_or_duplicate(current_pts));
    assert!(p3.is_stale_or_duplicate(current_pts));
}

#[test]
fn test_pts_simulated_stream_recovery_stress() {
    let chat = ChatId(777);

    // Generate a sequence of 50 contiguous updates on the server
    let mut server_updates = Vec::new();
    let mut server_pts = 100u64;
    for i in 1..=50 {
        let count = match i % 4 {
            0 => 0, // ephemeral
            1 => 1,
            2 => 2,
            _ => 3,
        };
        server_pts += count as u64;
        let event = UpdateEvent::ReadHistoryOut {
            max_id: MessageId(i),
        };
        server_updates.push(PtsUpdate::new(chat, server_pts, count, event, 1000 + i));
    }
    let final_server_pts = server_pts;

    // Simulate Client State
    let mut client_pts = 100u64;
    let mut applied_ids = HashSet::new();
    let mut pending_buffer: BTreeMap<u64, PtsUpdate> = BTreeMap::new();

    // Adversarially scramble delivery order:
    // Deliver updates in chunks or permuted order (e.g. reverse chunks of 5)
    let mut delivery_queue = Vec::new();
    for chunk in server_updates.chunks(5) {
        let mut rev_chunk = chunk.to_vec();
        rev_chunk.reverse();
        delivery_queue.extend(rev_chunk);
    }

    // Process delivery queue
    for update in delivery_queue {
        if update.is_stale_or_duplicate(client_pts) {
            continue;
        }

        if update.is_continuous(client_pts) {
            // Apply immediately
            if let UpdateEvent::ReadHistoryOut { max_id } = &update.event {
                applied_ids.insert(max_id.0);
            }
            if update.pts_count > 0 {
                client_pts = update.pts;
            }

            // Drain any pending updates in buffer that have now become continuous
            loop {
                let next_key = pending_buffer
                    .iter()
                    .find(|(_, u)| u.is_continuous(client_pts))
                    .map(|(k, _)| *k);

                if let Some(key) = next_key {
                    let u = pending_buffer.remove(&key).unwrap();
                    if let UpdateEvent::ReadHistoryOut { max_id } = &u.event {
                        applied_ids.insert(max_id.0);
                    }
                    if u.pts_count > 0 {
                        client_pts = u.pts;
                    }
                } else {
                    break;
                }
            }
        } else if let Some(gap_range) = update.gap(client_pts) {
            // Gap detected! Store future update in buffer
            let prev = update.prev_pts();
            pending_buffer.insert(prev, update);

            // Simulate difference reconciliation query (SyncGetDifference)
            let diff_req = SyncGetDifferenceRequest::new(chat, client_pts, 100);
            assert_eq!(diff_req.pts, client_pts);

            // Server responds with updates covering the gap
            let recovered: Vec<PtsUpdate> = server_updates
                .iter()
                .filter(|u| {
                    if u.pts_count == 0 {
                        u.pts >= *gap_range.start() && u.pts <= *gap_range.end()
                    } else {
                        u.pts >= *gap_range.start() && u.prev_pts() < *gap_range.end()
                    }
                })
                .cloned()
                .collect();

            // Client applies recovered updates
            for rec in recovered {
                if rec.is_continuous(client_pts) {
                    if let UpdateEvent::ReadHistoryOut { max_id } = &rec.event {
                        applied_ids.insert(max_id.0);
                    }
                    if rec.pts_count > 0 {
                        client_pts = rec.pts;
                    }
                }
            }

            // Drain buffer again
            loop {
                let next_key = pending_buffer
                    .iter()
                    .find(|(_, u)| u.is_continuous(client_pts))
                    .map(|(k, _)| *k);

                if let Some(key) = next_key {
                    let u = pending_buffer.remove(&key).unwrap();
                    if let UpdateEvent::ReadHistoryOut { max_id } = &u.event {
                        applied_ids.insert(max_id.0);
                    }
                    if u.pts_count > 0 {
                        client_pts = u.pts;
                    }
                } else {
                    break;
                }
            }
        }
    }

    // Verification invariants:
    // 1. Client PTS must exactly match final server PTS
    assert_eq!(client_pts, final_server_pts);
    // 2. Pending buffer must be empty
    assert!(pending_buffer.is_empty(), "Unapplied updates remain in buffer");
    // 3. All 50 events must have been applied exactly once
    assert_eq!(applied_ids.len(), 50);
}

// ============================================================================
// 2. Adversarial UTF-16 Text Entity Offsets Tests
// ============================================================================

#[test]
fn test_utf16_complex_emoji_zwj_sequences() {
    // 1. Family ZWJ sequence: 👨‍👩‍👧‍👦
    // Composed of: 👨 (U+1F468, 2 code units) + ZWJ (U+200D, 1 unit) + 👩 (U+1F469, 2) + ZWJ (1) + 👧 (U+1F467, 2) + ZWJ (1) + 👦 (U+1F466, 2)
    // Total UTF-16 code units = 11. (UTF-8 bytes = 25).
    let family_emoji = "👨‍👩‍👧‍👦";
    let text = format!("Family: {} forever", family_emoji);

    let _prefix = "Family: "; // 8 ASCII chars = 8 UTF-16 units
    let entity = MessageEntity::from_str_subslice(&text, family_emoji, MessageEntityType::Bold)
        .expect("Entity from subslice");

    assert_eq!(entity.offset, 8);
    assert_eq!(entity.length, 11);
    let extracted = entity.extract_text(&text).expect("Valid extraction");
    assert_eq!(extracted, family_emoji);

    // 2. Female technologist ZWJ sequence: 👩‍💻
    // 👩 (2 units) + ZWJ (1 unit) + 💻 (2 units) = 5 UTF-16 units
    let coder_emoji = "👩‍💻";
    let text2 = format!("She is a {}", coder_emoji);
    let entity2 = MessageEntity::from_str_subslice(&text2, coder_emoji, MessageEntityType::Italic)
        .expect("Entity from subslice");
    assert_eq!(entity2.offset, 9); // "She is a " = 9 units
    assert_eq!(entity2.length, 5);
    assert_eq!(entity2.extract_text(&text2).unwrap(), coder_emoji);
}

#[test]
fn test_utf16_regional_indicator_flags() {
    // Flag: United States 🇺🇸
    // U+1F1FA (Regional Indicator Symbol Letter U, 2 code units: 0xD83C 0xDDFA)
    // U+1F1F8 (Regional Indicator Symbol Letter S, 2 code units: 0xD83C 0xDDF8)
    // Total = 4 UTF-16 units. (UTF-8 bytes = 8).
    let us_flag = "🇺🇸";
    let text = format!("Welcome to {} USA", us_flag);
    let entity = MessageEntity::from_str_subslice(&text, us_flag, MessageEntityType::Underline)
        .expect("Entity for flag");

    assert_eq!(entity.offset, 11); // "Welcome to " = 11 units
    assert_eq!(entity.length, 4);
    assert_eq!(entity.extract_text(&text).unwrap(), us_flag);

    // Flag: Japan 🇯🇵 (4 UTF-16 units)
    let jp_flag = "🇯🇵";
    let text_jp = format!("Tokyo: {}", jp_flag);
    let entity_jp = MessageEntity::from_str_subslice(&text_jp, jp_flag, MessageEntityType::Code)
        .expect("Entity for JP flag");
    assert_eq!(entity_jp.offset, 7); // "Tokyo: " = 7 units
    assert_eq!(entity_jp.length, 4);
    assert_eq!(entity_jp.extract_text(&text_jp).unwrap(), jp_flag);
}

#[test]
fn test_utf16_emoji_skin_tone_modifiers() {
    // Thumbs up with medium skin tone: 👍🏽
    // 👍 (U+1F44D, 2 code units: 0xD83D 0xDC4D)
    // 🏽 (U+1F3FD Emoji Modifier Fitzpatrick Type-4, 2 code units: 0xD83C 0xDFFD)
    // Total = 4 UTF-16 units.
    let thumbs_up_tone = "👍🏽";
    let text = format!("Status: {} approved", thumbs_up_tone);
    let entity = MessageEntity::from_str_subslice(&text, thumbs_up_tone, MessageEntityType::Bold)
        .expect("Entity for modified emoji");

    assert_eq!(entity.offset, 8); // "Status: " = 8 units
    assert_eq!(entity.length, 4);
    assert_eq!(entity.extract_text(&text).unwrap(), thumbs_up_tone);
}

#[test]
fn test_utf16_surrogate_boundary_splitting_rejection() {
    // Rocket emoji: 🚀 = [0xD83D, 0xDE80] (high surrogate + low surrogate)
    let text = "Blast off 🚀 now!";
    // "Blast off " = 10 units. Rocket starts at 10. Length is 2.

    // Malicious or corrupted entity splitting the surrogate pair:
    // Starts at 10, length 1 (only high surrogate)
    let corrupt_high = MessageEntity::new(10, 1, MessageEntityType::Bold);
    // Must return None because [0xD83D] is an invalid standalone UTF-16 sequence
    assert_eq!(corrupt_high.extract_text(text), None);

    // Starts at 11, length 1 (only low surrogate)
    let corrupt_low = MessageEntity::new(11, 1, MessageEntityType::Bold);
    // Must return None because [0xDE80] is an invalid standalone UTF-16 sequence
    assert_eq!(corrupt_low.extract_text(text), None);

    // Correct full surrogate pair
    let correct = MessageEntity::new(10, 2, MessageEntityType::Bold);
    assert_eq!(correct.extract_text(text), Some("🚀".to_string()));
}

#[test]
fn test_utf16_overflow_and_out_of_bounds_protection() {
    let text = "Sample text for boundary checks";

    // Offset beyond text length
    let oob1 = MessageEntity::new(1000, 5, MessageEntityType::Bold);
    assert_eq!(oob1.extract_text(text), None);

    // Length beyond text length
    let oob2 = MessageEntity::new(5, 500, MessageEntityType::Bold);
    assert_eq!(oob2.extract_text(text), None);

    // Offset + Length arithmetic overflow (u32::MAX)
    let overflow = MessageEntity::new(u32::MAX - 2, 10, MessageEntityType::Bold);
    assert_eq!(overflow.extract_text(text), None);

    let overflow2 = MessageEntity::new(10, u32::MAX, MessageEntityType::Bold);
    assert_eq!(overflow2.extract_text(text), None);
}

// ============================================================================
// 3. Adversarial Interactive Cards Tests
// ============================================================================

#[test]
fn test_poll_card_zero_votes_and_tie_distributions() {
    let mut poll = PollCard::new(
        "poll_1",
        "Favorite Language?",
        vec![
            PollOption::new(1, "Rust"),
            PollOption::new(2, "C++"),
            PollOption::new(3, "TypeScript"),
        ],
        true,
    );

    // Zero votes edge case:
    assert_eq!(poll.total_voters, 0);
    assert_eq!(poll.total_votes, 0);
    assert_eq!(poll.vote_percentage(1), 0.0);
    assert_eq!(poll.vote_percentage(2), 0.0);
    assert_eq!(poll.vote_percentage(3), 0.0);
    assert!(!poll.vote_percentage(1).is_nan());

    // Single vote for option 1
    assert!(poll.cast_vote(1));
    assert_eq!(poll.total_voters, 1);
    assert_eq!(poll.vote_percentage(1), 100.0);
    assert_eq!(poll.vote_percentage(2), 0.0);

    // Switching vote to option 2 (single-choice poll)
    assert!(poll.cast_vote(2));
    assert_eq!(poll.total_voters, 1);
    assert_eq!(poll.vote_percentage(1), 0.0);
    assert_eq!(poll.vote_percentage(2), 100.0);

    // Toggling vote off on option 2
    assert!(poll.cast_vote(2));
    assert_eq!(poll.total_voters, 0);
    assert_eq!(poll.vote_percentage(2), 0.0);

    // Tie distribution: 1 vote for Option 1, 1 vote for Option 2
    // Setup multiple answers allow
    poll.allows_multiple_answers = true;
    assert!(poll.cast_vote(1));
    assert!(poll.cast_vote(2));
    assert_eq!(poll.total_voters, 2);
    assert_eq!(poll.vote_percentage(1), 50.0);
    assert_eq!(poll.vote_percentage(2), 50.0);
    assert_eq!(poll.vote_percentage(3), 0.0);

    // 3-way tie: 1 vote for each
    assert!(poll.cast_vote(3));
    assert_eq!(poll.total_voters, 3);
    assert!((poll.vote_percentage(1) - 33.33333).abs() < 0.01);
    assert!((poll.vote_percentage(2) - 33.33333).abs() < 0.01);
    assert!((poll.vote_percentage(3) - 33.33333).abs() < 0.01);

    // Closed poll rejects any further votes
    poll.close();
    assert!(poll.is_closed);
    assert!(!poll.cast_vote(1));
    assert_eq!(poll.total_voters, 3);
}

#[test]
fn test_todo_card_zero_items_and_completion_percentage() {
    // 1. Zero items edge case
    let mut todo_empty = TodoCard::new("todo_empty", "Empty List", Vec::new());
    assert_eq!(todo_empty.items.len(), 0);
    assert_eq!(todo_empty.completed_count, 0);
    assert_eq!(todo_empty.completion_percentage(), 0.0);
    assert!(!todo_empty.completion_percentage().is_nan());
    assert!(!todo_empty.toggle_item("non_existent"));
    assert!(!todo_empty.remove_item("non_existent"));

    // 2. All items toggled to completed
    let mut todo = TodoCard::new(
        "todo_work",
        "Sprint Tasks",
        vec![
            TodoItem::new("t1", "Design Protocol"),
            TodoItem::new("t2", "Implement Models"),
            TodoItem::new("t3", "Run Stress Tests"),
        ],
    );
    assert_eq!(todo.completion_percentage(), 0.0);

    assert!(todo.toggle_item("t1"));
    assert_eq!(todo.completed_count, 1);
    assert!((todo.completion_percentage() - 33.33333).abs() < 0.01);

    assert!(todo.toggle_item("t2"));
    assert_eq!(todo.completed_count, 2);
    assert!((todo.completion_percentage() - 66.66667).abs() < 0.01);

    assert!(todo.toggle_item("t3"));
    assert_eq!(todo.completed_count, 3);
    assert_eq!(todo.completion_percentage(), 100.0);

    // 3. Adding an already completed item
    let completed_item = TodoItem::new("t4", "Documentation").with_completed(true, Some(123456));
    todo.add_item(completed_item);
    assert_eq!(todo.items.len(), 4);
    assert_eq!(todo.completed_count, 4);
    assert_eq!(todo.completion_percentage(), 100.0);

    // 4. Removing items
    assert!(todo.remove_item("t1")); // completed item removed
    assert_eq!(todo.items.len(), 3);
    assert_eq!(todo.completed_count, 3);
    assert_eq!(todo.completion_percentage(), 100.0);

    // Toggle t2 back to incomplete
    assert!(todo.toggle_item("t2"));
    assert_eq!(todo.completed_count, 2);
    assert!((todo.completion_percentage() - 66.66667).abs() < 0.01);

    assert!(todo.remove_item("t2")); // uncompleted item removed
    assert_eq!(todo.items.len(), 2);
    assert_eq!(todo.completed_count, 2); // remains 2
    assert_eq!(todo.completion_percentage(), 100.0);
}

#[test]
fn test_invoice_card_pricing_zero_and_status_transitions() {
    // 1. Zero prices / free invoice
    let mut invoice = InvoiceCard::new(
        "inv_zero",
        "Free Registration",
        "Community Event",
        "USD",
        0,
    );
    assert_eq!(invoice.amount_cents, 0);
    assert_eq!(invoice.total_amount, 0);
    assert_eq!(invoice.total_from_prices(), 0);
    assert_eq!(invoice.status, PaymentStatus::Pending);
    assert!(!invoice.is_paid);

    // Status transition: mark_paid
    invoice.mark_paid();
    assert_eq!(invoice.status, PaymentStatus::Paid);
    assert!(invoice.is_paid);

    // Status transition: mark_failed
    invoice.mark_failed();
    assert_eq!(invoice.status, PaymentStatus::Failed);
    assert!(!invoice.is_paid);

    // Status transition: mark_cancelled
    invoice.mark_cancelled();
    assert_eq!(invoice.status, PaymentStatus::Cancelled);
    assert!(!invoice.is_paid);

    // 2. Invoice with price breakdown
    let invoice_with_lines = InvoiceCard::new(
        "inv_breakdown",
        "Telegram Premium",
        "Annual Subscription",
        "USD",
        4999,
    ).with_prices(vec![
        PriceLine::new("Base Subscription", 3999),
        PriceLine::new("Taxes & Fees", 1000),
    ]);

    assert_eq!(invoice_with_lines.amount_cents, 4999);
    assert_eq!(invoice_with_lines.total_amount, 4999);
    assert_eq!(invoice_with_lines.total_from_prices(), 4999);

    // 3. Free item in breakdown
    let invoice_free_item = InvoiceCard::new(
        "inv_free",
        "Bundle",
        "Bundle with promo",
        "USD",
        2000,
    ).with_prices(vec![
        PriceLine::new("Product A", 2000),
        PriceLine::new("Bonus Item", 0),
    ]);
    assert_eq!(invoice_free_item.total_from_prices(), 2000);
}
