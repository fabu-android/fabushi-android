#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestContext {
    pub request_id: String,
    pub trace_id: String,
    pub cancelled: bool,
}

impl RequestContext {
    pub fn new(request_id: impl Into<String>, trace_id: impl Into<String>) -> Result<Self, &'static str> {
        let request_id = request_id.into();
        let trace_id = trace_id.into();
        if request_id.trim().is_empty() || trace_id.trim().is_empty() { return Err("request_id and trace_id are required"); }
        Ok(Self { request_id, trace_id, cancelled: false })
    }
    pub fn cancel(&mut self) { self.cancelled = true; }
}
