//! Telegram Test Harness: Single-Use Launch Fuse Verifier
//! Upstream sources: `Telegram/SourceFiles/test/test_launch_fuse.cpp`

use std::sync::atomic::{AtomicBool, Ordering};

/// Test fixture ensuring initialization fuses blow on first trigger and never repeat.
#[derive(Debug, Default)]
pub struct TestLaunchFuse {
    blown: AtomicBool,
}

impl TestLaunchFuse {
    pub fn new() -> Self {
        Self {
            blown: AtomicBool::new(false),
        }
    }

    pub fn trigger(&self) -> bool {
        self.blown.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok()
    }

    pub fn is_blown(&self) -> bool {
        self.blown.load(Ordering::SeqCst)
    }

    pub fn reset(&self) {
        self.blown.store(false, Ordering::SeqCst);
    }
}
