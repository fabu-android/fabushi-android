//! Telegram Test Harness: Console Lock & Concurrent Log Synchronization
//! Upstream sources: `Telegram/SourceFiles/test/test_console_lock.cpp`

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Test fixture ensuring synchronized console writing without thread interleaved garble.
#[derive(Debug, Default)]
pub struct TestConsoleLock {
    is_locked: Arc<AtomicBool>,
    lock_acquisitions: usize,
}

impl TestConsoleLock {
    pub fn new() -> Self {
        Self {
            is_locked: Arc::new(AtomicBool::new(false)),
            lock_acquisitions: 0,
        }
    }

    pub fn acquire(&mut self) -> bool {
        if self.is_locked.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            self.lock_acquisitions += 1;
            true
        } else {
            false
        }
    }

    pub fn release(&mut self) {
        self.is_locked.store(false, Ordering::SeqCst);
    }

    pub fn is_locked(&self) -> bool {
        self.is_locked.load(Ordering::SeqCst)
    }
}
