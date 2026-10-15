use serde::{Deserialize, Serialize};

/// A concrete provider continuation boundary.
///
/// The cursor is provider-owned opaque data. A retry after visible output is
/// eligible only when this checkpoint exists and its cursor is non-empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptCheckpoint {
    pub cursor: String,
    pub emitted_text_bytes: usize,
    pub tool_calls_completed: usize,
}

impl AttemptCheckpoint {
    pub fn new(
        cursor: impl Into<String>,
        emitted_text_bytes: usize,
        tool_calls_completed: usize,
    ) -> Self {
        Self {
            cursor: cursor.into(),
            emitted_text_bytes,
            tool_calls_completed,
        }
    }

    pub fn is_resumable(&self) -> bool {
        !self.cursor.trim().is_empty()
    }
}
