#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractionKind {
    Message,
    Thinking,
    ToolCall,
    ToolResult,
    Completed,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InteractionUpdate {
    pub sequence: u64,
    pub kind: InteractionKind,
    pub payload: String,
}

impl InteractionUpdate {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.sequence == 0 {
            return Err("sequence must be non-zero");
        }
        if self.payload.len() > 4 * 1024 * 1024 {
            return Err("payload exceeds interaction limit");
        }
        Ok(())
    }
}
