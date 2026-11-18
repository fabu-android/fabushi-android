use crate::action_bus::{ActionCategory, ActionContext, ActionError, ActionRiskLevel, TypedPlatformAction};
use futures::future::BoxFuture;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SendMessageInput {
    pub chat_id: i64,
    pub text: String,
    #[serde(default)]
    pub reply_to_id: Option<i64>,
    #[serde(default)]
    pub topic_id: Option<i64>,
    #[serde(default)]
    pub silent: bool,
    #[serde(default)]
    pub client_nonce: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SendMessageOutput {
    pub message_id: i64,
    pub chat_id: i64,
    pub pts: u64,
    pub date_ms: i64,
    pub client_nonce: u64,
}

#[derive(Debug, Clone, Default)]
pub struct SendMessageAction;

impl TypedPlatformAction for SendMessageAction {
    type Input = SendMessageInput;
    type Output = SendMessageOutput;

    fn name(&self) -> &'static str {
        "send_message"
    }

    fn category(&self) -> ActionCategory {
        ActionCategory::Messaging
    }

    fn description(&self) -> &'static str {
        "Send a text message to a specific chat or topic"
    }

    fn risk_level(&self) -> ActionRiskLevel {
        ActionRiskLevel::Low
    }

    fn execute<'a>(
        &'a self,
        _ctx: ActionContext,
        input: Self::Input,
    ) -> BoxFuture<'a, Result<Self::Output, ActionError>> {
        Box::pin(async move {
            if input.text.trim().is_empty() {
                return Err(ActionError::InvalidInput("Message text cannot be empty".into()));
            }

            let now_ms = chrono::Utc::now().timestamp_millis();
            let nonce = input.client_nonce.unwrap_or_else(|| {
                (now_ms as u64) ^ (input.chat_id.unsigned_abs())
            });

            Ok(SendMessageOutput {
                message_id: 1,
                chat_id: input.chat_id,
                pts: 1,
                date_ms: now_ms,
                client_nonce: nonce,
            })
        })
    }
}
