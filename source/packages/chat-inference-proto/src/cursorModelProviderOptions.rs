#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CursorModelProviderOptions {
    pub model: String,
    pub max_output_tokens: u32,
}

impl CursorModelProviderOptions {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.model.trim().is_empty() { return Err("model is required"); }
        if self.max_output_tokens == 0 || self.max_output_tokens > 1_000_000 { return Err("max_output_tokens is invalid"); }
        Ok(())
    }
}
