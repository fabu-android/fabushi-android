use gpui_telegram_core::auth::GenerationFence;
use gpui_telegram_core::error::TelegramError;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

// =========================================================================
// 4. GenerationFence Concurrency Stress (TCG-N50)
// =========================================================================

#[test]
fn test_generation_fence_high_concurrency_stress_no_aba() {
    // 24 concurrent threads (exceeds 16+ mandate)
    const NUM_THREADS: usize = 24;
    const BUMPS_PER_THREAD: usize = 10_000;
    const EXPECTED_TOTAL: usize = NUM_THREADS * BUMPS_PER_THREAD;

    let fence = Arc::new(GenerationFence::new(0));
    let mut handles = Vec::with_capacity(NUM_THREADS);

    for _ in 0..NUM_THREADS {
        let fence_clone = Arc::clone(&fence);
        let handle = thread::spawn(move || {
            let mut recorded = Vec::with_capacity(BUMPS_PER_THREAD);
            for _ in 0..BUMPS_PER_THREAD {
                let next_val = fence_clone.bump();
                recorded.push(next_val);
            }
            recorded
        });
        handles.push(handle);
    }

    let mut all_values = HashSet::with_capacity(EXPECTED_TOTAL);
    let mut min_val = u64::MAX;
    let mut max_val = 0u64;

    for handle in handles {
        let thread_values = handle.join().expect("Worker thread panicked");
        for v in thread_values {
            min_val = min_val.min(v);
            max_val = max_val.max(v);
            let is_unique = all_values.insert(v);
            assert!(
                is_unique,
                "Duplicate generation detected: {} (ABA / race condition violation!)",
                v
            );
        }
    }

    assert_eq!(
        all_values.len(),
        EXPECTED_TOTAL,
        "Total unique generation count mismatch"
    );
    assert_eq!(min_val, 1, "Minimum generated value must be 1");
    assert_eq!(
        max_val, EXPECTED_TOTAL as u64,
        "Maximum generated value must match total operations"
    );
    assert_eq!(
        fence.current(),
        EXPECTED_TOTAL as u64,
        "Final fence current() must match expected total"
    );
}

#[test]
fn test_generation_fence_concurrent_readers_monotonicity() {
    const NUM_BUMPERS: usize = 16;
    const BUMPS_PER_THREAD: usize = 2_000;
    const NUM_READERS: usize = 8;

    let fence = Arc::new(GenerationFence::new(0));
    let running = Arc::new(AtomicBool::new(true));
    let start_barrier = Arc::new(std::sync::Barrier::new(NUM_READERS + NUM_BUMPERS + 1));

    let mut reader_handles = Vec::with_capacity(NUM_READERS);
    for reader_id in 0..NUM_READERS {
        let fence_clone = Arc::clone(&fence);
        let running_clone = Arc::clone(&running);
        let barrier_clone = Arc::clone(&start_barrier);
        let handle = thread::spawn(move || {
            barrier_clone.wait();
            let mut prev_seen = 0u64;
            let mut read_count = 0u64;
            loop {
                let curr = fence_clone.current();
                assert!(
                    curr >= prev_seen,
                    "Reader {} detected non-monotonic backward step: curr {} < prev {}",
                    reader_id,
                    curr,
                    prev_seen
                );
                prev_seen = curr;
                read_count += 1;
                if !running_clone.load(Ordering::Relaxed) {
                    break;
                }
                thread::yield_now();
            }
            read_count
        });
        reader_handles.push(handle);
    }

    let mut bumper_handles = Vec::with_capacity(NUM_BUMPERS);
    for _ in 0..NUM_BUMPERS {
        let fence_clone = Arc::clone(&fence);
        let barrier_clone = Arc::clone(&start_barrier);
        let handle = thread::spawn(move || {
            barrier_clone.wait();
            for _ in 0..BUMPS_PER_THREAD {
                fence_clone.bump();
            }
        });
        bumper_handles.push(handle);
    }

    start_barrier.wait();

    for b in bumper_handles {
        b.join().expect("Bumper panicked");
    }

    running.store(false, Ordering::Relaxed);

    for r in reader_handles {
        let reads = r.join().expect("Reader panicked");
        assert!(reads > 0, "Reader must have executed reads");
    }

    assert_eq!(
        fence.current(),
        (NUM_BUMPERS * BUMPS_PER_THREAD) as u64,
        "All bumps must be reflected in final fence"
    );
}

#[test]
fn test_generation_fence_assert_valid_staleness_rejection() {
    const NUM_WORKERS: usize = 16;
    let fence = Arc::new(GenerationFence::new(100));

    // Initial valid state
    assert!(fence.is_valid(100));
    assert!(fence.assert_valid(100).is_ok());

    let mut handles = Vec::with_capacity(NUM_WORKERS);
    for _ in 0..NUM_WORKERS {
        let fence_clone = Arc::clone(&fence);
        let handle = thread::spawn(move || {
            // Snapshot initial generation
            let snapshot = 100u64;
            // Sleep briefly to allow coordinator to bump
            thread::sleep(Duration::from_millis(5));
            // Assert with snapshot: MUST fail once bumped
            fence_clone.assert_valid(snapshot)
        });
        handles.push(handle);
    }

    // Bump generation while workers are waiting
    let new_gen = fence.bump();
    assert_eq!(new_gen, 101);

    for h in handles {
        let result = h.join().expect("Worker panicked");
        match result {
            Err(TelegramError::GenerationFenced { expected, actual }) => {
                assert_eq!(expected, 100);
                assert!(actual >= 101);
            }
            Ok(_) => panic!("Worker accepted obsolete generation 100 after bump!"),
            Err(other) => panic!("Unexpected error variant: {:?}", other),
        }
    }

    assert!(!fence.is_valid(100));
    assert!(fence.is_valid(101));
}
