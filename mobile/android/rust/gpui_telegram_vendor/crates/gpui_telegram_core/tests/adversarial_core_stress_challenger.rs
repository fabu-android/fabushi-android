//! Empirical Adversarial Stress Testing Harness for gpui_telegram_core::core
//! Author: challenger_module_01_1
//! Covers:
//! 1. Concurrency stress on GenerationFence (ABA protection, concurrent readers/bumpers).
//! 2. CoreSettings configuration recovery from malformed / corrupt payloads.
//! 3. Shortcuts conflict routing and unmapped keys.
//! 4. DeadlockDetector watchdog thresholds and clock boundaries.

use gpui_telegram_core::auth::GenerationFence;
use gpui_telegram_core::core::core_settings::{
    CloseBehavior, CoreSettings, NotifyView, ScreenCorner,
    WorkMode, DEFAULT_VOLUME,
};
use gpui_telegram_core::core::core_settings_proxy::{
    CoreSettingsProxy, ProxyEntry, ProxyMode,
    PROXY_ROTATION_TIMEOUTS,
};
use gpui_telegram_core::core::deadlock_detector::{
    DeadlockDetector, DEFAULT_ABORT_TIMEOUT_SECS, DEFAULT_PING_INTERVAL_SECS,
};
use gpui_telegram_core::core::shortcuts::{Command, Shortcuts};
use gpui_telegram_core::error::TelegramError;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

// ============================================================================
// 1. GenerationFence Concurrency Stress & ABA Protection
// ============================================================================

#[test]
fn test_adversarial_generation_fence_aba_and_monotonic_contention() {
    const NUM_BUMPERS: usize = 16;
    const BUMPS_PER_THREAD: usize = 2_500;
    const NUM_READERS: usize = 8;
    const TOTAL_BUMPS: usize = NUM_BUMPERS * BUMPS_PER_THREAD;

    let fence = Arc::new(GenerationFence::new(100));
    let running = Arc::new(AtomicBool::new(true));
    let monotonicity_violation = Arc::new(AtomicBool::new(false));

    // Spawn concurrent readers asserting strictly non-decreasing observations
    let mut reader_handles = Vec::with_capacity(NUM_READERS);
    for reader_id in 0..NUM_READERS {
        let f = Arc::clone(&fence);
        let r = Arc::clone(&running);
        let violated = Arc::clone(&monotonicity_violation);
        reader_handles.push(thread::spawn(move || {
            let mut last_seen = f.current();
            let mut reads = 0u64;
            while r.load(Ordering::Relaxed) {
                let current = f.current();
                if current < last_seen {
                    violated.store(true, Ordering::SeqCst);
                    eprintln!(
                        "Reader {} observed regression: current {} < last_seen {}",
                        reader_id, current, last_seen
                    );
                }
                last_seen = current;
                reads += 1;
                thread::yield_now();
            }
            reads
        }));
    }

    // Spawn concurrent bumpers collecting assigned tokens
    let mut bumper_handles = Vec::with_capacity(NUM_BUMPERS);
    for _ in 0..NUM_BUMPERS {
        let f = Arc::clone(&fence);
        bumper_handles.push(thread::spawn(move || {
            let mut assigned = Vec::with_capacity(BUMPS_PER_THREAD);
            for _ in 0..BUMPS_PER_THREAD {
                assigned.push(f.bump());
            }
            assigned
        }));
    }

    let mut all_tokens = HashSet::with_capacity(TOTAL_BUMPS);
    for h in bumper_handles {
        let tokens = h.join().expect("Bumper thread join failed");
        for tok in tokens {
            let is_unique = all_tokens.insert(tok);
            assert!(
                is_unique,
                "ABA Token Collision detected: token {} reused across threads!",
                tok
            );
        }
    }

    running.store(false, Ordering::Relaxed);
    for r in reader_handles {
        let reads = r.join().expect("Reader thread join failed");
        assert!(reads > 0, "Reader must have executed at least one read");
    }

    assert!(
        !monotonicity_violation.load(Ordering::SeqCst),
        "Generation fence violated monotonic non-decreasing guarantee!"
    );

    let final_val = fence.current();
    let expected_final = 100 + TOTAL_BUMPS as u64;
    assert_eq!(
        final_val, expected_final,
        "Final generation fence value mismatch: expected {}, got {}",
        expected_final, final_val
    );
    assert_eq!(all_tokens.len(), TOTAL_BUMPS);
}

#[test]
fn test_adversarial_generation_fence_stale_token_invalidation_sweep() {
    let fence = Arc::new(GenerationFence::new(10));
    assert!(fence.is_valid(10));
    assert!(fence.assert_valid(10).is_ok());

    // Create historical tokens
    let mut historical_tokens = vec![10];
    for _ in 0..50 {
        historical_tokens.push(fence.bump());
    }

    // Bump one more time so all 51 historical tokens are strictly stale
    let active_token = fence.bump();
    assert_eq!(active_token, 61);
    assert!(fence.is_valid(active_token));
    assert!(fence.assert_valid(active_token).is_ok());

    // Every historical token must fail assert_valid with exact TelegramError::GenerationFenced
    for stale in historical_tokens {
        assert!(
            !fence.is_valid(stale),
            "Stale token {} must not be reported valid",
            stale
        );
        match fence.assert_valid(stale) {
            Err(TelegramError::GenerationFenced { expected, actual }) => {
                assert_eq!(expected, stale);
                assert_eq!(actual, active_token);
            }
            Ok(_) => panic!("Stale token {} was unexpectedly accepted!", stale),
            Err(other) => panic!("Unexpected error variant: {:?}", other),
        }
    }
}

// ============================================================================
// 2. CoreSettings Configuration Recovery & Malformed Payloads
// ============================================================================

#[test]
fn test_adversarial_core_settings_corrupt_payload_recovery() {
    let corrupt_payloads: Vec<(&'static str, &[u8])> = vec![
        ("empty slice", b""),
        ("random binary junk", &[0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0xFF, 0x88]),
        ("truncated JSON object", b"{\"window_position\": {\"x\": 100,"),
        ("unclosed string", b"{\"download_path\": \"/path/without/close"),
        ("wrong type for int field", b"{\"auto_lock\": \"not_a_number\"}"),
        ("wrong type for float field", b"{\"song_volume\": [1.0, 2.0]}"),
        ("wrong enum variant", b"{\"screen_corner\": \"MiddleEverywhere\"}"),
        ("null payload", b"null"),
        ("naked array", b"[1, 2, 3]"),
        ("naked boolean", b"true"),
    ];

    for (desc, payload) in corrupt_payloads {
        let res = CoreSettings::from_serialized(payload);
        assert!(
            res.is_err(),
            "Payload '{}' should fail deserialization, but succeeded!",
            desc
        );

        // Verify recovery fallback pattern: unwrap_or_default() produces safe defaults
        let recovered = res.unwrap_or_default();
        assert_eq!(recovered.song_volume, DEFAULT_VOLUME);
        assert_eq!(recovered.video_volume, DEFAULT_VOLUME);
        assert_eq!(recovered.auto_lock, 3600);
        assert_eq!(recovered.screen_corner, ScreenCorner::BottomRight);
        assert_eq!(recovered.notify_view, NotifyView::ShowPreview);
        assert_eq!(recovered.work_mode, WorkMode::WindowAndTray);
        assert_eq!(recovered.close_behavior, CloseBehavior::Quit);
        assert_eq!(recovered.window_position.scale, 100);
        assert_eq!(recovered.window_position.w, 960);
        assert_eq!(recovered.window_position.h, 680);
    }
}

#[test]
fn test_adversarial_core_settings_fuzz_ranges_and_clamping() {
    let mut settings = CoreSettings::new();

    // 1. Song volume extreme values
    settings.set_song_volume(-100.0);
    assert_eq!(settings.song_volume, 0.0, "Negative song volume must clamp to 0.0");
    settings.set_song_volume(100.0);
    assert_eq!(settings.song_volume, 1.0, "Excessive song volume must clamp to 1.0");
    settings.set_song_volume(0.42);
    assert!((settings.song_volume - 0.42).abs() < 1e-6);

    // 2. Video volume extreme values
    settings.set_video_volume(-1e9);
    assert_eq!(settings.video_volume, 0.0);
    settings.set_video_volume(1e9);
    assert_eq!(settings.video_volume, 1.0);

    // 3. Auto lock negative value protection
    settings.set_auto_lock(-500);
    assert_eq!(settings.auto_lock, 0, "Negative auto-lock must clamp to 0");
    settings.set_auto_lock(7200);
    assert_eq!(settings.auto_lock, 7200);

    // 4. Download path Unicode and special characters
    let unicode_path = "/home/用户/Telegram/📁 downloads / 🚀 / test.dat";
    settings.set_download_path(unicode_path);
    assert_eq!(settings.download_path, unicode_path);

    // 5. Serialize and deserialize roundtrip must preserve clamped fields
    let serialized = settings.serialize();
    assert!(!serialized.is_empty());

    let deserialized = CoreSettings::from_serialized(&serialized).expect("Roundtrip deserialize failed");
    assert_eq!(deserialized.song_volume, settings.song_volume);
    assert_eq!(deserialized.video_volume, settings.video_volume);
    assert_eq!(deserialized.auto_lock, settings.auto_lock);
    assert_eq!(deserialized.download_path, settings.download_path);
}

#[test]
fn test_adversarial_core_settings_proxy_edge_cases() {
    let mut proxy = CoreSettingsProxy::new();
    assert!(proxy.is_system());
    assert!(!proxy.is_enabled());

    // 1. Rotation timeout validation: only valid timeouts must be accepted
    for &valid in &PROXY_ROTATION_TIMEOUTS {
        proxy.set_proxy_rotation_timeout(valid);
        assert_eq!(proxy.proxy_rotation_timeout, valid);
    }

    // Invalid timeouts must be ignored
    let prev_timeout = proxy.proxy_rotation_timeout;
    for &invalid in &[0u32, 1, 7, 20, 45, 999] {
        proxy.set_proxy_rotation_timeout(invalid);
        assert_eq!(
            proxy.proxy_rotation_timeout, prev_timeout,
            "Invalid timeout {} was accepted!",
            invalid
        );
    }

    // 2. Add proxies and prevent exact duplicates (same host + port)
    let p1 = ProxyEntry::new_socks5("127.0.0.1", 1080);
    let p1_duplicate = ProxyEntry::new_socks5("127.0.0.1", 1080);
    let p2 = ProxyEntry::new_socks5("127.0.0.1", 1081);
    let p3 = ProxyEntry::new_mtproto("proxy.example.com", 443, "d41d8cd98f00b204e9800998ecf8427e");

    proxy.add_to_list(p1.clone());
    assert_eq!(proxy.list().len(), 1);

    proxy.add_to_list(p1_duplicate);
    assert_eq!(proxy.list().len(), 1, "Duplicate proxy should not be added");

    proxy.add_to_list(p2.clone());
    proxy.add_to_list(p3.clone());
    assert_eq!(proxy.list().len(), 3);

    // 3. Selection and automatic mode change
    proxy.set_selected(Some(p1.clone()));
    assert_eq!(proxy.mode, ProxyMode::Manual);
    assert!(proxy.is_enabled());
    assert_eq!(proxy.selected().unwrap().port, 1080);

    // 4. Remove active proxy -> selected should fallback to next or first
    let removed = proxy.remove_from_list("127.0.0.1", 1080);
    assert!(removed);
    assert_eq!(proxy.list().len(), 2);
    // Selected was 1080, removed => should now point to first in remaining list (p2: 1081)
    assert_eq!(proxy.selected().unwrap().port, 1081);

    // 5. Remove non-existent proxy returns false without crashing
    assert!(!proxy.remove_from_list("non.existent.host", 9999));
    assert_eq!(proxy.list().len(), 2);

    // 6. Corrupt payload handling in set_from_serialized
    let corrupt = b"not a json";
    assert!(proxy.set_from_serialized(corrupt).is_err());
    assert_eq!(proxy.list().len(), 2, "Failed deserialize must not alter existing proxy state");
}

// ============================================================================
// 3. Shortcuts Conflict Routing and Unmapped Keys
// ============================================================================

#[test]
fn test_adversarial_shortcuts_unmapped_keys_and_unhandled_commands() {
    let shortcuts = Shortcuts::new();

    // 1. Unmapped key combinations must return false, never panic
    let unmapped_combos = [
        "",
        "Ctrl+Alt+Shift+Meta+Super+Z",
        "NonExistentKey",
        "F24",
        "12345",
        "ctrl+w", // case difference: default is "Ctrl+W"
        "Shift+Escape",
    ];

    for combo in &unmapped_combos {
        assert!(
            !shortcuts.dispatch_key_sequence(combo),
            "Unmapped key combo '{}' should return false",
            combo
        );
    }

    // 2. Bound key combo with NO registered handler must return false
    // Default has "Ctrl+L" -> Command::Lock, but no handler registered yet
    assert!(
        !shortcuts.dispatch_key_sequence("Ctrl+L"),
        "Bound key combo with no handlers should return false"
    );
    assert!(
        !shortcuts.dispatch(Command::Lock),
        "Command with no handlers should return false"
    );
}

#[test]
fn test_adversarial_shortcuts_conflict_routing_and_rebinding() {
    let mut shortcuts = Shortcuts::new();

    let close_executed = Arc::new(AtomicBool::new(false));
    let quit_executed = Arc::new(AtomicBool::new(false));

    let c_flag = Arc::clone(&close_executed);
    shortcuts.register(Command::Close, move || {
        c_flag.store(true, Ordering::SeqCst);
        true
    });

    let q_flag = Arc::clone(&quit_executed);
    shortcuts.register(Command::Quit, move || {
        q_flag.store(true, Ordering::SeqCst);
        true
    });

    // Default: "Ctrl+W" is bound to Command::Close
    assert!(shortcuts.dispatch_key_sequence("Ctrl+W"));
    assert!(close_executed.load(Ordering::SeqCst));
    assert!(!quit_executed.load(Ordering::SeqCst));

    // Reset flags
    close_executed.store(false, Ordering::SeqCst);
    quit_executed.store(false, Ordering::SeqCst);

    // Adversarial action: Rebind "Ctrl+W" to Command::Quit (conflict routing override)
    shortcuts.bind("Ctrl+W", Command::Quit);

    // Dispatching "Ctrl+W" must now execute Quit, NOT Close
    assert!(shortcuts.dispatch_key_sequence("Ctrl+W"));
    assert!(!close_executed.load(Ordering::SeqCst), "Old handler was unexpectedly executed!");
    assert!(quit_executed.load(Ordering::SeqCst), "New handler was not executed after rebinding!");
}

#[test]
fn test_adversarial_shortcuts_multi_handler_chaining_and_pause() {
    let mut shortcuts = Shortcuts::new();

    let handler1_called = Arc::new(AtomicBool::new(false));
    let handler2_called = Arc::new(AtomicBool::new(false));

    // Handler 1 returns false (pass-through / veto)
    let h1 = Arc::clone(&handler1_called);
    shortcuts.register(Command::Search, move || {
        h1.store(true, Ordering::SeqCst);
        false
    });

    // Handler 2 returns true (consumed)
    let h2 = Arc::clone(&handler2_called);
    shortcuts.register(Command::Search, move || {
        h2.store(true, Ordering::SeqCst);
        true
    });

    // Dispatch: both handlers should run until one returns true
    assert!(shortcuts.dispatch(Command::Search));
    assert!(handler1_called.load(Ordering::SeqCst));
    assert!(handler2_called.load(Ordering::SeqCst));

    // Reset
    handler1_called.store(false, Ordering::SeqCst);
    handler2_called.store(false, Ordering::SeqCst);

    // Pause shortcuts
    shortcuts.pause();
    assert!(
        !shortcuts.dispatch(Command::Search),
        "Paused shortcuts must not dispatch"
    );
    assert!(!handler1_called.load(Ordering::SeqCst));
    assert!(!handler2_called.load(Ordering::SeqCst));

    // Unpause shortcuts
    shortcuts.unpause();
    assert!(shortcuts.dispatch(Command::Search));
    assert!(handler1_called.load(Ordering::SeqCst));
    assert!(handler2_called.load(Ordering::SeqCst));
}

#[test]
fn test_adversarial_shortcuts_concurrent_dispatch() {
    let mut shortcuts = Shortcuts::new();
    let counter = Arc::new(AtomicU64::new(0));

    let cnt = Arc::clone(&counter);
    shortcuts.register(Command::JustSendMessage, move || {
        cnt.fetch_add(1, Ordering::SeqCst);
        true
    });

    let shortcuts = Arc::new(shortcuts);
    let num_threads = 16;
    let dispatches_per_thread = 1_000;
    let mut handles = Vec::with_capacity(num_threads);

    for _ in 0..num_threads {
        let sc = Arc::clone(&shortcuts);
        handles.push(thread::spawn(move || {
            for _ in 0..dispatches_per_thread {
                let res = sc.dispatch_key_sequence("Return");
                assert!(res);
            }
        }));
    }

    for h in handles {
        h.join().expect("Concurrent shortcut dispatch thread failed");
    }

    assert_eq!(
        counter.load(Ordering::SeqCst),
        (num_threads * dispatches_per_thread) as u64
    );
}

// ============================================================================
// 4. DeadlockDetector Watchdog Thresholds & State Transitions
// ============================================================================

#[test]
fn test_adversarial_deadlock_detector_watchdog_exact_thresholds() {
    let mut detector = DeadlockDetector::new();
    assert_eq!(detector.timeout_secs, DEFAULT_ABORT_TIMEOUT_SECS); // 30s
    assert_eq!(detector.ping_interval_secs, DEFAULT_PING_INTERVAL_SECS); // 60s
    assert!(detector.is_running);
    assert!(!detector.is_waiting_pong);
    assert!(!detector.deadlock_detected);

    // Initial check without ping must return false
    assert!(!detector.check_hang(1000));

    // Post a ping at t = 1000
    detector.post_ping(1000);
    assert!(detector.is_waiting_pong);
    assert_eq!(detector.last_ping_timestamp, 1000);

    // Exactly 1 second before timeout (elapsed = 29s < 30s)
    assert!(
        !detector.check_hang(1029),
        "At 29s elapsed, hang must NOT be triggered"
    );
    assert!(!detector.deadlock_detected);

    // At exact threshold boundary (elapsed = 30s == timeout_secs)
    assert!(
        detector.check_hang(1030),
        "At exactly 30s elapsed, hang MUST be triggered"
    );
    assert!(detector.deadlock_detected);

    // Past threshold boundary (elapsed = 35s > timeout_secs)
    assert!(
        detector.check_hang(1035),
        "Past timeout threshold, hang must still be triggered"
    );
    assert!(detector.deadlock_detected);

    // Receive pong at t = 1036 -> resolves deadlock state
    detector.receive_pong(1036);
    assert!(!detector.is_waiting_pong);
    assert!(!detector.deadlock_detected);
    assert_eq!(detector.last_pong_timestamp, 1036);

    // Check hang after pong received -> must return false
    assert!(
        !detector.check_hang(1040),
        "After pong received, hang must NOT be triggered"
    );
    assert!(!detector.deadlock_detected);
}

#[test]
fn test_adversarial_deadlock_detector_stopped_behavior() {
    let mut detector = DeadlockDetector::new();
    detector.post_ping(1000);

    // Stop watchdog
    detector.stop();
    assert!(!detector.is_running);

    // Even if 10000 seconds have elapsed, stopped watchdog must return false
    assert!(
        !detector.check_hang(11000),
        "Stopped deadlock detector must never report hang"
    );
    assert!(!detector.deadlock_detected);
}

#[test]
fn test_adversarial_deadlock_detector_clock_skew_empirical_characterization() {
    let mut detector = DeadlockDetector::new();
    detector.post_ping(1000);

    // Case 1: monotonic clock advancement within safe boundary
    assert!(!detector.check_hang(1015));

    // Case 2: Clock steps backward (e.g. NTP synchronization jumps back to 950)
    // Non-monotonic backward clock jump must NOT cause false-positive deadlock report.
    let backward_hang = detector.check_hang(950);
    assert!(
        !backward_hang,
        "Remediated: negative elapsed time must not trigger false-positive deadlock warning"
    );
    assert!(!detector.deadlock_detected);
}
