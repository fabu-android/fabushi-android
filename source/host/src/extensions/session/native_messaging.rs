use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

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

#[derive(Clone)]
pub struct FabushiNativeMessagingClient {
    credential_provider: Arc<dyn FabushiNativeMessagingCredentialProvider>,
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
        Self { credential_provider }
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
            "Android native Human messaging {operation} is unavailable until the canonical account credential owner is injected."
        ))
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
        self.unavailable("contact sync")
    }

    pub fn list_direct_messages(
        &self,
        _peer_human_id: &str,
        _before: Option<&str>,
        _limit: usize,
    ) -> Result<Vec<FabushiRemoteHumanMessage>, String> {
        self.unavailable("message sync")
    }

    pub fn get_human_call_ice_servers(
        &self,
    ) -> Result<(Vec<FabushiCallIceServer>, u64), String> {
        self.unavailable("call ICE credentials")
    }

    pub fn list_human_calls(
        &self,
        _limit: usize,
    ) -> Result<Vec<FabushiRemoteHumanCall>, String> {
        self.unavailable("call discovery")
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
        _call_id: &str,
        _after_seq: u64,
        _limit: usize,
    ) -> Result<(FabushiRemoteHumanCall, Vec<FabushiRemoteHumanCallEvent>, u64), String> {
        self.unavailable("call sync")
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
    fn identity_text_preserves_string_and_numeric_backend_ids() {
        assert_eq!(
            fabushi_identity_text(&Value::String("human:abc".into())).unwrap(),
            "human:abc"
        );
        assert_eq!(fabushi_identity_text(&Value::from(42)).unwrap(), "42");
        assert!(fabushi_identity_text(&Value::Null).is_err());
    }
}
