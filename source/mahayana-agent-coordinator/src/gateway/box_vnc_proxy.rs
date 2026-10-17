#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoxVncTarget {
    pub session_id: String,
    pub endpoint: String,
}

impl BoxVncTarget {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.session_id.trim().is_empty() { return Err("session_id must not be empty"); }
        if !(self.endpoint.starts_with("https://") || self.endpoint.starts_with("wss://") || self.endpoint.starts_with("http://127.0.0.1")) {
            return Err("VNC endpoint must use an approved transport");
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VncLiveness { Unknown, Connecting, Live, Closed }
