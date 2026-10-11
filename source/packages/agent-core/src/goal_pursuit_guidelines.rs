#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoalPursuitGuidelines {
    pub max_steps: usize,
    pub require_user_confirmation_for_sensitive_actions: bool,
    pub stop_on_terminal_error: bool,
}

impl Default for GoalPursuitGuidelines {
    fn default() -> Self {
        Self {
            max_steps: 64,
            require_user_confirmation_for_sensitive_actions: true,
            stop_on_terminal_error: true,
        }
    }
}

impl GoalPursuitGuidelines {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.max_steps == 0 || self.max_steps > 10_000 {
            return Err("max_steps is outside the supported range");
        }
        Ok(())
    }
}
