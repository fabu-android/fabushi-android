#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteConversationAction {
    pub session_id: String,
    pub operation_id: String,
    pub action: String,
    pub payload: String,
}

impl RemoteConversationAction {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.session_id.trim().is_empty() {
            return Err("session_id is required");
        }
        if self.operation_id.trim().is_empty() {
            return Err("operation_id is required");
        }
        if self.action.trim().is_empty() {
            return Err("action is required");
        }
        Ok(())
    }
}
