//! Telegram Desktop Core Common Header Prelude
//! Upstream sources: `Telegram/SourceFiles/stdafx.h`

use serde::{Deserialize, Serialize};

/// Precompiled common header context for foundational Telegram desktop primitives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Stdafx {
    pub initialized: bool,
}

impl Stdafx {
    pub fn new() -> Self {
        Self { initialized: true }
    }

    pub fn is_ready(&self) -> bool {
        self.initialized
    }
}
