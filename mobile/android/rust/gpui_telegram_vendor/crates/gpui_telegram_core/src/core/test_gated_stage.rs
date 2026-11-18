//! Telegram Test Harness: Gated Stage Precondition Verifier
//! Upstream sources: `Telegram/SourceFiles/test/test_gated_stage.cpp`

use serde::{Deserialize, Serialize};

/// Test fixture ensuring staged workflow gates only open when all preconditions pass.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestGatedStage {
    pub stage_name: String,
    pub preconditions_met: bool,
    pub is_unlocked: bool,
    pub executions_count: u32,
}

impl TestGatedStage {
    pub fn new(stage_name: impl Into<String>) -> Self {
        Self {
            stage_name: stage_name.into(),
            preconditions_met: false,
            is_unlocked: false,
            executions_count: 0,
        }
    }

    pub fn satisfy_precondition(&mut self) {
        self.preconditions_met = true;
    }

    pub fn unlock(&mut self) -> bool {
        if self.preconditions_met {
            self.is_unlocked = true;
            true
        } else {
            false
        }
    }

    pub fn execute(&mut self) -> bool {
        if self.is_unlocked {
            self.executions_count = self.executions_count.saturating_add(1);
            true
        } else {
            false
        }
    }
}
