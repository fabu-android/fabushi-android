use prost::{Enumeration, Message, Oneof};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Enumeration)]
#[repr(i32)]
pub enum InferenceReason {
    Unspecified = 0,
    GeminiVideoSubagent = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Enumeration)]
#[repr(i32)]
pub enum InferenceMessageRole {
    Unspecified = 0,
    User = 1,
    Assistant = 2,
    Tool = 3,
    System = 4,
}

#[derive(Clone, PartialEq, Message)]
pub struct InferenceCoreMessage {
    #[prost(enumeration = "InferenceMessageRole", tag = "1")]
    pub role: i32,
    #[prost(oneof = "inference_core_message::Content", tags = "2")]
    pub content: Option<inference_core_message::Content>,
}

pub mod inference_core_message {
    use super::*;

    #[derive(Clone, PartialEq, Oneof)]
    pub enum Content {
        #[prost(string, tag = "2")]
        Text(String),
    }
}

pub fn core_text_message_to_proto(role: &str, content: impl Into<String>) -> InferenceCoreMessage {
    let role = match role {
        "user" => InferenceMessageRole::User,
        "assistant" => InferenceMessageRole::Assistant,
        "tool" => InferenceMessageRole::Tool,
        "system" => InferenceMessageRole::System,
        _ => InferenceMessageRole::Unspecified,
    };
    InferenceCoreMessage {
        role: role as i32,
        content: Some(inference_core_message::Content::Text(content.into())),
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct AgentFollowupCategorizationRequest {
    #[prost(string, tag = "1")]
    pub request_id: String,
    #[prost(string, tag = "2")]
    pub replying_to_request_id: String,
    #[prost(message, repeated, tag = "3")]
    pub messages: Vec<InferenceCoreMessage>,
    #[prost(string, optional, tag = "4")]
    pub conversation_id: Option<String>,
    #[prost(string, optional, tag = "5")]
    pub agent_mode: Option<String>,
    #[prost(string, optional, tag = "6")]
    pub model_name: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct AgentPostTurnLabelingRequest {
    #[prost(string, tag = "1")]
    pub request_id: String,
    #[prost(message, repeated, tag = "2")]
    pub messages: Vec<InferenceCoreMessage>,
    #[prost(string, optional, tag = "3")]
    pub conversation_id: Option<String>,
    #[prost(string, optional, tag = "4")]
    pub agent_mode: Option<String>,
    #[prost(string, optional, tag = "5")]
    pub model_name: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct GetSignedUrlForAttachedMediaRequest {
    #[prost(string, optional, tag = "1")]
    pub key: Option<String>,
    #[prost(string, optional, tag = "2")]
    pub mime_type: Option<String>,
    #[prost(string, tag = "3")]
    pub conversation_id: String,
    #[prost(int64, optional, tag = "4")]
    pub content_length_bytes: Option<i64>,
}

#[derive(Clone, PartialEq, Message)]
pub struct GetSignedUrlForAttachedMediaResponse {
    #[prost(string, tag = "1")]
    pub key: String,
    #[prost(string, tag = "2")]
    pub post_url: String,
    #[prost(string, tag = "3")]
    pub get_url: String,
    #[prost(int64, tag = "4")]
    pub expires_at_unix_ms: i64,
    #[prost(int64, tag = "5")]
    pub refresh_after_unix_ms: i64,
    #[prost(map = "string, string", tag = "6")]
    pub post_fields: std::collections::HashMap<String, String>,
    #[prost(string, tag = "7")]
    pub put_url: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_message_roles_and_text_match_desktop_wire_contract() {
        let roles = [
            ("user", InferenceMessageRole::User),
            ("assistant", InferenceMessageRole::Assistant),
            ("tool", InferenceMessageRole::Tool),
            ("system", InferenceMessageRole::System),
            ("unknown", InferenceMessageRole::Unspecified),
        ];
        for (role, expected) in roles {
            let message = core_text_message_to_proto(role, "hello");
            assert_eq!(message.role, expected as i32);
            assert!(matches!(
                message.content,
                Some(inference_core_message::Content::Text(ref text)) if text == "hello"
            ));
        }
    }

    #[test]
    fn attachment_signed_url_request_round_trips_proto_tags() {
        let request = GetSignedUrlForAttachedMediaRequest {
            key: Some("media/key".into()),
            mime_type: Some("image/png".into()),
            conversation_id: "conversation-1".into(),
            content_length_bytes: Some(42),
        };
        let bytes = request.encode_to_vec();
        let decoded = GetSignedUrlForAttachedMediaRequest::decode(bytes.as_slice()).unwrap();
        assert_eq!(decoded.key.as_deref(), Some("media/key"));
        assert_eq!(decoded.mime_type.as_deref(), Some("image/png"));
        assert_eq!(decoded.conversation_id, "conversation-1");
        assert_eq!(decoded.content_length_bytes, Some(42));
    }

    #[test]
    fn followup_request_round_trips_optional_provider_context() {
        let request = AgentFollowupCategorizationRequest {
            request_id: "request-2".into(),
            replying_to_request_id: "request-1".into(),
            messages: vec![core_text_message_to_proto("user", "continue")],
            conversation_id: Some("conversation-1".into()),
            agent_mode: Some("agent".into()),
            model_name: Some("grok-4.5".into()),
        };
        let bytes = request.encode_to_vec();
        let decoded = AgentFollowupCategorizationRequest::decode(bytes.as_slice()).unwrap();
        assert_eq!(decoded.request_id, "request-2");
        assert_eq!(decoded.replying_to_request_id, "request-1");
        assert_eq!(decoded.messages.len(), 1);
        assert_eq!(decoded.conversation_id.as_deref(), Some("conversation-1"));
        assert_eq!(decoded.agent_mode.as_deref(), Some("agent"));
        assert_eq!(decoded.model_name.as_deref(), Some("grok-4.5"));
    }
}
