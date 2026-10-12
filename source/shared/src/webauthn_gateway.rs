pub const GATEWAY_WEBAUTHN_REQUESTS_PATH: &str = "/webauthn/requests";
pub const GATEWAY_WEBAUTHN_RESPONSES_PATH: &str = "/webauthn/responses";
pub const SAND_WEBAUTHN_HEARTBEAT_INTERVAL_MS: u64 = 10_000;
pub const SAND_WEBAUTHN_LIVENESS_WINDOW_MS: u64 = 30_000;
pub const SAND_WEBAUTHN_CEREMONY_TIMEOUT_MS: u64 = 120_000;
pub const SAND_NO_WEBAUTHN_MACHINE_MESSAGE: &str =
    "Your computer isn't connected right now, so the security key can't be reached. Open Grok Bot on the machine your key is plugged into and try again.";
pub const SAND_WEBAUTHN_MACHINE_UNAVAILABLE_MESSAGE: &str =
    "Your computer looks disconnected, so the security key can't be reached. Reconnect it and try again.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebAuthnOriginClass {
    CursorCom,
    Subdomain,
    External,
}

pub fn webauthn_origin_class(origin: &str) -> WebAuthnOriginClass {
    let Ok(url) = url::Url::parse(origin) else {
        return WebAuthnOriginClass::External;
    };
    match url.host_str() {
        Some("cursor.com") => WebAuthnOriginClass::CursorCom,
        Some(host) if host.ends_with(".cursor.com") => WebAuthnOriginClass::Subdomain,
        _ => WebAuthnOriginClass::External,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebAuthnCeremony {
    pub kind: String,
    pub origin: String,
    pub payload_json: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WebAuthnRequestFrame {
    Welcome { provider_id: String },
    Ceremony { request_id: String, ceremony: WebAuthnCeremony },
    Cancel { request_id: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebAuthnStage {
    Grant,
    Sign,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebAuthnStageOutcome {
    Ok,
    Declined,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WebAuthnResponseFrame {
    Hello { computer_id: Option<String>, label: Option<String> },
    Ping,
    Stage {
        request_id: String,
        stage: WebAuthnStage,
        outcome: WebAuthnStageOutcome,
    },
    Result { request_id: String, credential_json: String },
    Error {
        request_id: String,
        name: String,
        message: String,
        code: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_classification_is_fail_closed() {
        assert_eq!(
            webauthn_origin_class("https://cursor.com"),
            WebAuthnOriginClass::CursorCom
        );
        assert_eq!(
            webauthn_origin_class("https://welcome.cursor.com"),
            WebAuthnOriginClass::Subdomain
        );
        assert_eq!(
            webauthn_origin_class("https://cursor.com.evil.example"),
            WebAuthnOriginClass::External
        );
        assert_eq!(webauthn_origin_class("not-a-url"), WebAuthnOriginClass::External);
    }
}
