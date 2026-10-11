#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OAuthCallback {
    pub state: String,
    pub code: Option<String>,
    pub error: Option<String>,
}

impl OAuthCallback {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.state.trim().is_empty() { return Err("OAuth callback state is missing"); }
        if self.code.is_some() == self.error.is_some() { return Err("OAuth callback must contain exactly one of code or error"); }
        Ok(())
    }
}
