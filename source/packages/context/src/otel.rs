#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceAttributes {
    pub operation: String,
    pub request_id: String,
    pub generation: u64,
}

impl TraceAttributes {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.operation.trim().is_empty() || self.request_id.trim().is_empty() || self.generation == 0 {
            return Err("trace attributes are incomplete");
        }
        Ok(())
    }
}
