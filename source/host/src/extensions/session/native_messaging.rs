use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use url::Url;

const DEFAULT_FABUSHI_API_BASE_URL: &str = "https://api.ombhrum.com";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FabushiNativeIdentity {
    pub user_id: String,
    pub device_id: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FabushiRemoteHumanContact {
    pub id: Value,
    pub user_id: Value,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FabushiRemoteHumanAttachment {
    pub resource_id: String,
    pub name: String,
    pub content_type: String,
    pub size: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FabushiRemoteHumanReaction {
    pub emoji: String,
    pub count: u64,
    pub reacted_by_me: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FabushiRemoteHumanMessage {
    pub id: Value,
    pub sender_user_id: Value,
    pub sender_username: Option<String>,
    pub recipient_user_id: Value,
    pub recipient_username: Option<String>,
    pub text: String,
    pub client_request_id: Option<String>,
    pub created_at: String,
    pub read_at: Option<String>,
    #[serde(default)]
    pub is_outgoing: bool,
    pub reply_to_message_id: Option<Value>,
    #[serde(default)]
    pub attachments: Vec<FabushiRemoteHumanAttachment>,
    #[serde(default)]
    pub reactions: Vec<FabushiRemoteHumanReaction>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FabushiRemoteHumanCall {
    pub call_id: String,
    pub creator_user_id: Value,
    pub peer_user_id: Value,
    #[serde(default)]
    pub creator_device_id: Option<String>,
    #[serde(default)]
    pub peer_device_id: Option<String>,
    pub state: String,
    pub generation: u64,
    pub event_seq: u64,
    pub terminal_state: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FabushiRemoteHumanCallEvent {
    pub call_id: String,
    pub seq: u64,
    pub generation: u64,
    pub user_id: Value,
    pub device_id: String,
    pub client_event_id: String,
    pub kind: String,
    pub payload: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FabushiCallIceServer {
    pub urls: Value,
    pub username: Option<String>,
    pub credential: Option<String>,
}

/// Android compile/runtime boundary for Desktop native Human messaging.
///
/// Desktop resolves credentials from a private Host credential file. Android must not grow that
/// second credential owner: the canonical bearer token belongs to AndroidAccountService /
/// AndroidKeyStore and must eventually be injected into a live transport through a typed Host
/// adapter. Until that injection exists, production construction fails closed instead of reading
/// process environment or pretending that a remote side effect succeeded.
#[derive(Clone)]
pub struct FabushiNativeMessagingCredential {
    pub access_token: String,
    pub device_id: String,
    pub user_id: Value,
}

pub trait FabushiNativeMessagingCredentialProvider: Send + Sync {
    fn current_credential(&self) -> Result<FabushiNativeMessagingCredential, String>;
}

#[derive(Debug, Deserialize)]
struct FriendData { friends: Vec<FabushiRemoteHumanContact> }
#[derive(Debug, Deserialize)]
struct FriendEnvelope { success: bool, data: Option<FriendData>, error: Option<String> }
#[derive(Debug, Deserialize)]
struct ListData { messages: Vec<FabushiRemoteHumanMessage> }
#[derive(Debug, Deserialize)]
struct ListEnvelope { success: bool, data: Option<ListData>, error: Option<String> }
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CallEnvelope {
    success: bool,
    call: Option<FabushiRemoteHumanCall>,
    #[serde(default)] calls: Vec<FabushiRemoteHumanCall>,
    #[serde(default)] events: Vec<FabushiRemoteHumanCallEvent>,
    next_after_seq: Option<u64>,
    error: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CallIceEnvelope {
    success: bool,
    #[serde(default)] ice_servers: Vec<FabushiCallIceServer>,
    ttl_seconds: Option<u64>,
    error: Option<String>,
}

#[derive(Clone)]
pub struct FabushiNativeMessagingClient {
    base_url: Url,
    credential_provider: Arc<dyn FabushiNativeMessagingCredentialProvider>,
    agent: ureq::Agent,
}

impl std::fmt::Debug for FabushiNativeMessagingClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FabushiNativeMessagingClient")
            .field("credential_provider", &"<redacted canonical account owner>")
            .finish()
    }
}

impl FabushiNativeMessagingClient {
    pub fn from_env() -> Result<Self, String> {
        Err(
            "Android native Human messaging requires account-scoped credential injection; environment/credential-file auth is disabled."
                .into(),
        )
    }

    pub fn with_credential_provider(
        credential_provider: Arc<dyn FabushiNativeMessagingCredentialProvider>,
    ) -> Self {
        Self::with_credential_provider_and_base_url(credential_provider, DEFAULT_FABUSHI_API_BASE_URL)
            .expect("the built-in Fabushi API base URL must be valid")
    }

    pub fn with_credential_provider_and_base_url(
        credential_provider: Arc<dyn FabushiNativeMessagingCredentialProvider>,
        base_url: &str,
    ) -> Result<Self, String> {
        let base_url = validate_base_url(base_url)?;
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .redirects(0)
            .build();
        Ok(Self { base_url, credential_provider, agent })
    }

    fn current_credential(&self) -> Result<FabushiNativeMessagingCredential, String> {
        let credential = self.credential_provider.current_credential()?;
        required_trimmed(&credential.access_token, "accessToken")?;
        required_trimmed(&credential.device_id, "deviceId")?;
        let _ = fabushi_identity_text(&credential.user_id)?;
        Ok(credential)
    }

    fn unavailable<T>(&self, operation: &str) -> Result<T, String> {
        Err(format!(
            "Android native Human messaging {operation} is unavailable until the durable side-effect transport is bound."
        ))
    }

    fn endpoint(&self, path: &str) -> Result<Url, String> {
        self.base_url.join(path.trim_start_matches('/'))
            .map_err(|error| format!("Fabushi messaging endpoint is invalid: {error}"))
    }

    fn get_json<T: DeserializeOwned>(&self, operation: &str, url: Url) -> Result<(u16, T), String> {
        let credential = self.current_credential()?;
        let token = required_trimmed(&credential.access_token, "accessToken")?;
        let device_id = required_trimmed(&credential.device_id, "deviceId")?;
        let authorization = format!("Bearer {token}");
        let response = self.agent.get(url.as_str())
            .set("authorization", &authorization)
            .set("x-fabushi-device-id", device_id)
            .set("accept", "application/json")
            .call();
        let (status, response) = match response {
            Ok(response) => (response.status(), response),
            Err(ureq::Error::Status(status, response)) => (status, response),
            Err(ureq::Error::Transport(error)) =>
                return Err(format!("Fabushi Human {operation} transport failed: {error}")),
        };
        let body = response.into_string()
            .map_err(|error| format!("Fabushi Human {operation} response could not be read: {error}"))?;
        let parsed = serde_json::from_str::<T>(&body)
            .map_err(|error| format!("Fabushi Human {operation} response was invalid JSON (HTTP {status}): {error}"))?;
        Ok((status, parsed))
    }

    pub fn identity(&self) -> Result<FabushiNativeIdentity, String> {
        let credential = self.current_credential()?;
        Ok(FabushiNativeIdentity {
            user_id: fabushi_identity_text(&credential.user_id)?,
            device_id: required_trimmed(&credential.device_id, "deviceId")?.to_string(),
        })
    }

    pub fn upload_direct_message_resource(
        &self,
        _path: &Path,
        _name: &str,
    ) -> Result<FabushiRemoteHumanAttachment, String> {
        self.unavailable("attachment upload")
    }

    pub fn download_direct_message_resource(
        &self,
        _resource_id: &str,
        _expected_size: u64,
    ) -> Result<Vec<u8>, String> {
        self.unavailable("attachment download")
    }

    pub fn send_direct_message(
        &self,
        _peer_human_id: &str,
        _text: &str,
        _client_request_id: &str,
        _reply_to_message_id: Option<&str>,
        _attachments: &[FabushiRemoteHumanAttachment],
    ) -> Result<FabushiRemoteHumanMessage, String> {
        self.unavailable("send")
    }

    pub fn set_direct_message_reaction(
        &self,
        _message_id: &str,
        _emoji: &str,
        _active: bool,
    ) -> Result<Vec<FabushiRemoteHumanReaction>, String> {
        self.unavailable("reaction mutation")
    }

    pub fn list_friends(&self) -> Result<Vec<FabushiRemoteHumanContact>, String> {
        let (status, envelope): (u16, FriendEnvelope) =
            self.get_json("contact sync", self.endpoint("/api/social/friends")?)?;
        if !(200..300).contains(&status) || !envelope.success {
            return Err(format!("Fabushi Human contact sync was rejected (HTTP {status}): {}",
                envelope.error.unwrap_or_else(|| "unknown backend error".into())));
        }
        Ok(envelope.data.map(|data| data.friends).unwrap_or_default())
    }

    pub fn list_direct_messages(
        &self,
        peer_human_id: &str,
        before: Option<&str>,
        limit: usize,
    ) -> Result<Vec<FabushiRemoteHumanMessage>, String> {
        let peer_human_id = required_trimmed(peer_human_id, "peer Human id")?;
        let mut url = self.endpoint("/api/social/messages")?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("contactId", peer_human_id);
            query.append_pair("limit", &limit.clamp(1, 200).to_string());
            if let Some(before) = before.map(str::trim).filter(|value| !value.is_empty()) {
                query.append_pair("before", before);
            }
        }
        let (status, envelope): (u16, ListEnvelope) = self.get_json("message sync", url)?;
        if !(200..300).contains(&status) || !envelope.success {
            return Err(format!("Fabushi Human message sync was rejected (HTTP {status}): {}",
                envelope.error.unwrap_or_else(|| "unknown backend error".into())));
        }
        Ok(envelope.data.map(|data| data.messages).unwrap_or_default())
    }

    pub fn get_human_call_ice_servers(&self) -> Result<(Vec<FabushiCallIceServer>, u64), String> {
        let (status, envelope): (u16, CallIceEnvelope) =
            self.get_json("call ICE credential request", self.endpoint("/api/social/calls/ice")?)?;
        if !(200..300).contains(&status) || !envelope.success {
            return Err(format!("Fabushi Human call ICE credentials were rejected (HTTP {status}): {}",
                envelope.error.unwrap_or_else(|| "unknown backend error".into())));
        }
        let ttl_seconds = envelope.ttl_seconds.filter(|ttl| (300..=86_400).contains(ttl))
            .ok_or_else(|| "Fabushi Human call ICE response contained an invalid TTL".to_string())?;
        if envelope.ice_servers.is_empty() || envelope.ice_servers.len() > 16 {
            return Err("Fabushi Human call ICE response contained an invalid server list".into());
        }
        for server in &envelope.ice_servers {
            if !valid_call_ice_urls(&server.urls)
                || server.username.as_deref().is_some_and(|v| v.len() > 2048)
                || server.credential.as_deref().is_some_and(|v| v.len() > 2048) {
                return Err("Fabushi Human call ICE response contained an invalid server".into());
            }
        }
        Ok((envelope.ice_servers, ttl_seconds))
    }

    pub fn list_human_calls(&self, limit: usize) -> Result<Vec<FabushiRemoteHumanCall>, String> {
        let mut url = self.endpoint("/api/social/calls")?;
        url.query_pairs_mut().append_pair("limit", &limit.clamp(1, 200).to_string());
        let (status, envelope): (u16, CallEnvelope) = self.get_json("call discovery", url)?;
        if !(200..300).contains(&status) || !envelope.success {
            return Err(format!("Fabushi Human call discovery was rejected (HTTP {status}): {}",
                envelope.error.unwrap_or_else(|| "unknown backend error".into())));
        }
        Ok(envelope.calls)
    }

    pub fn create_human_call(
        &self,
        _call_id: &str,
        _peer_human_id: &str,
    ) -> Result<FabushiRemoteHumanCall, String> {
        self.unavailable("call creation")
    }

    pub fn get_human_call(
        &self,
        call_id: &str,
        after_seq: u64,
        limit: usize,
    ) -> Result<(FabushiRemoteHumanCall, Vec<FabushiRemoteHumanCallEvent>, u64), String> {
        let call_id = required_trimmed(call_id, "call id")?;
        let encoded: String = url::form_urlencoded::byte_serialize(call_id.as_bytes()).collect();
        let mut url = self.endpoint(&format!("/api/social/calls/{encoded}"))?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("afterSeq", &after_seq.to_string());
            query.append_pair("limit", &limit.clamp(1, 200).to_string());
        }
        let (status, mut envelope): (u16, CallEnvelope) = self.get_json("call sync", url)?;
        if !(200..300).contains(&status) || !envelope.success {
            return Err(format!("Fabushi Human call sync was rejected (HTTP {status}): {}",
                envelope.error.unwrap_or_else(|| "unknown backend error".into())));
        }
        let call = envelope.call.take()
            .ok_or_else(|| "Fabushi Human call sync response omitted the call.".to_string())?;
        if call.call_id != call_id {
            return Err("Fabushi Human call backend returned a mismatched call id".into());
        }
        let next = envelope.next_after_seq
            .unwrap_or_else(|| envelope.events.last().map(|event| event.seq).unwrap_or(after_seq));
        Ok((call, envelope.events, next))
    }

    pub fn append_human_call_event(
        &self,
        _call_id: &str,
        _client_event_id: &str,
        _generation: u64,
        _kind: &str,
        _payload: &Value,
    ) -> Result<(FabushiRemoteHumanCall, FabushiRemoteHumanCallEvent), String> {
        self.unavailable("call event")
    }
}

fn valid_call_ice_urls(value: &Value) -> bool {
    let valid = |url: &str| {
        let trimmed = url.trim();
        !trimmed.is_empty() && trimmed.len() <= 1024
            && !trimmed.chars().any(char::is_whitespace)
            && (trimmed.starts_with("stun:") || trimmed.starts_with("turn:") || trimmed.starts_with("turns:"))
    };
    match value {
        Value::String(url) => valid(url),
        Value::Array(urls) => !urls.is_empty() && urls.len() <= 16
            && urls.iter().all(|url| url.as_str().is_some_and(valid)),
        _ => false,
    }
}

fn validate_base_url(raw: &str) -> Result<Url, String> {
    let normalized = raw.trim().trim_end_matches('/');
    let mut url = Url::parse(&format!("{normalized}/"))
        .map_err(|error| format!("Fabushi messaging API base URL is invalid: {error}"))?;
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
    if url.scheme() != "https" && !(loopback && url.scheme() == "http") {
        return Err("Fabushi messaging API must use HTTPS outside loopback development.".into());
    }
    if !url.username().is_empty() || url.password().is_some() || url.query().is_some() || url.fragment().is_some() {
        return Err("Fabushi messaging API base URL must not contain credentials, query, or fragment.".into());
    }
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

fn required_trimmed<'a>(value: &'a str, name: &str) -> Result<&'a str, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("Fabushi messaging {name} is empty."))
    } else {
        Ok(value)
    }
}

pub fn fabushi_identity_text(value: &Value) -> Result<String, String> {
    match value {
        Value::String(value) => required_trimmed(value, "userId").map(str::to_string),
        Value::Number(value) => Ok(value.to_string()),
        _ => Err("Fabushi messaging userId is invalid.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_environment_constructor_fails_closed_without_second_credential_owner() {
        let error = FabushiNativeMessagingClient::from_env().unwrap_err();
        assert!(error.contains("credential injection"));
        assert!(error.contains("disabled"));
    }

    struct TestCredentialProvider;

    impl FabushiNativeMessagingCredentialProvider for TestCredentialProvider {
        fn current_credential(&self) -> Result<FabushiNativeMessagingCredential, String> {
            Ok(FabushiNativeMessagingCredential {
                access_token: "test-token".into(),
                device_id: "android-device".into(),
                user_id: Value::String("human:test".into()),
            })
        }
    }

    #[test]
    fn injected_identity_comes_from_the_canonical_provider_without_exposing_token() {
        let client = FabushiNativeMessagingClient::with_credential_provider(Arc::new(
            TestCredentialProvider,
        ));
        assert_eq!(
            client.identity().unwrap(),
            FabushiNativeIdentity {
                user_id: "human:test".into(),
                device_id: "android-device".into(),
            }
        );
        assert!(!format!("{client:?}").contains("test-token"));
    }

    #[test]
    fn remote_operations_do_not_report_fake_success_before_transport_is_bound() {
        let client = FabushiNativeMessagingClient::with_credential_provider(Arc::new(
            TestCredentialProvider,
        ));
        assert!(client.identity().is_ok());
        assert!(client.list_friends().is_err());
        assert!(client.list_direct_messages("human:peer", None, 20).is_err());
        assert!(client
            .send_direct_message("human:peer", "hello", "nonce-1", None, &[])
            .is_err());
        assert!(client.list_human_calls(20).is_err());
    }

    #[test]
    fn base_url_policy_rejects_cleartext_remote_and_embedded_credentials() {
        assert!(FabushiNativeMessagingClient::with_credential_provider_and_base_url(
            Arc::new(TestCredentialProvider), "http://api.example.com").is_err());
        assert!(FabushiNativeMessagingClient::with_credential_provider_and_base_url(
            Arc::new(TestCredentialProvider), "https://user:secret@api.example.com").is_err());
        assert!(FabushiNativeMessagingClient::with_credential_provider_and_base_url(
            Arc::new(TestCredentialProvider), "http://127.0.0.1:18080").is_ok());
    }

    #[test]
    fn identity_text_preserves_string_and_numeric_backend_ids() {
        assert_eq!(
            fabushi_identity_text(&Value::String("human:abc".into())).unwrap(),
            "human:abc"
        );
        assert_eq!(fabushi_identity_text(&Value::from(42)).unwrap(), "42");
        assert!(fabushi_identity_text(&Value::Null).is_err());
    }
}
