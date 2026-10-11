#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpAuthState {
    Idle,
    Pending { provider: String, state: String },
    Ready { provider: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpAuthFlow {
    state: McpAuthState,
}

impl Default for McpAuthFlow {
    fn default() -> Self {
        Self { state: McpAuthState::Idle }
    }
}

impl McpAuthFlow {
    pub fn state(&self) -> &McpAuthState {
        &self.state
    }

    pub fn begin(&mut self, provider: impl Into<String>, state: impl Into<String>) -> Result<(), &'static str> {
        let provider = provider.into();
        let state = state.into();
        if provider.trim().is_empty() || state.len() < 16 {
            return Err("provider and strong state are required");
        }
        if !matches!(self.state, McpAuthState::Idle) {
            return Err("authorization flow already active");
        }
        self.state = McpAuthState::Pending { provider, state };
        Ok(())
    }

    pub fn complete(&mut self, returned_state: &str) -> Result<String, &'static str> {
        let McpAuthState::Pending { provider, state } = &self.state else {
            return Err("no authorization flow is active");
        };
        if state != returned_state {
            return Err("authorization state mismatch");
        }
        let provider = provider.clone();
        self.state = McpAuthState::Ready { provider: provider.clone() };
        Ok(provider)
    }

    pub fn reset(&mut self) {
        self.state = McpAuthState::Idle;
    }
}
