#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WebAuthnCeremony { Create, Get }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebAuthnRequest {
    pub request_id: String,
    pub relying_party_id: String,
    pub challenge: Vec<u8>,
    pub ceremony: WebAuthnCeremony,
}

impl WebAuthnRequest {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.request_id.is_empty() { return Err("request_id is required"); }
        if self.relying_party_id.is_empty() { return Err("relying_party_id is required"); }
        if self.challenge.len() < 16 { return Err("WebAuthn challenge is too short"); }
        Ok(())
    }
}

pub trait WebAuthnProvider {
    fn perform(&mut self, request: &WebAuthnRequest) -> Result<Vec<u8>, String>;
}
