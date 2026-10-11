#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InferenceRequest {
    pub request_id: String,
    pub model: String,
    pub prompt: String,
}

impl InferenceRequest {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.request_id.trim().is_empty() || self.model.trim().is_empty() { return Err("request_id and model are required"); }
        if self.prompt.len() > 8 * 1024 * 1024 { return Err("prompt exceeds limit"); }
        Ok(())
    }
}

pub trait InferenceTransport {
    fn send(&mut self, request: &InferenceRequest) -> Result<String, String>;
}
