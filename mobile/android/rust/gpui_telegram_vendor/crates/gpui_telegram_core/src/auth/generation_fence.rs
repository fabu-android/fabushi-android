use crate::error::{Result, TelegramError};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::sync::atomic::{AtomicU64, Ordering};

/// Monotonic generation fence for TCG-N50 account scope isolation
/// matching Telegram Desktop `Main::Domain` and `data_session.cpp`.
///
/// Prevents asynchronous stale responses from older session lifecycles
/// from contaminating active state across account switches or re-logins.
#[derive(Debug, Default)]
pub struct GenerationFence {
    generation: AtomicU64,
}

impl GenerationFence {
    /// Create a new generation fence with initial counter value
    pub fn new(initial: u64) -> Self {
        Self {
            generation: AtomicU64::new(initial),
        }
    }

    /// Read the current generation counter with Acquire ordering
    pub fn current(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// Monotonically bump generation counter and return the new value
    pub fn bump(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Overwrite the generation counter with a specific value
    pub fn set(&self, val: u64) {
        self.generation.store(val, Ordering::SeqCst);
    }

    /// Check if expected generation matches current generation
    pub fn is_valid(&self, expected: u64) -> bool {
        self.current() == expected
    }

    /// Assert that expected generation matches current generation,
    /// returning `TelegramError::GenerationFenced` on staleness
    pub fn assert_valid(&self, expected: u64) -> Result<()> {
        let current = self.current();
        if expected != current {
            Err(TelegramError::GenerationFenced {
                expected,
                actual: current,
            })
        } else {
            Ok(())
        }
    }
}

impl Clone for GenerationFence {
    fn clone(&self) -> Self {
        Self::new(self.current())
    }
}

impl Serialize for GenerationFence {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.current())
    }
}

impl<'de> Deserialize<'de> for GenerationFence {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let val = u64::deserialize(deserializer)?;
        Ok(Self::new(val))
    }
}

