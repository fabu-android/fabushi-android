use crate::action_bus::{ActionCategory, ActionContext, ActionError, ActionRiskLevel, TypedPlatformAction};
use futures::future::BoxFuture;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn default_history_limit() -> u32 {
    50
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ChatHistoryMessageItem {
    pub message_id: i64,
    pub sender_id: i64,
    pub text: String,
    pub date_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetChatHistoryInput {
    pub chat_id: i64,
    #[serde(default = "default_history_limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset_id: Option<i64>,
    #[serde(default)]
    pub topic_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GetChatHistoryOutput {
    pub chat_id: i64,
    pub messages: Vec<ChatHistoryMessageItem>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Default)]
pub struct GetChatHistoryAction;

impl TypedPlatformAction for GetChatHistoryAction {
    type Input = GetChatHistoryInput;
    type Output = GetChatHistoryOutput;

    fn name(&self) -> &'static str {
        "get_chat_history"
    }

    fn category(&self) -> ActionCategory {
        ActionCategory::Messaging
    }

    fn description(&self) -> &'static str {
        "Retrieve recent messages and history for a given chat"
    }

    fn risk_level(&self) -> ActionRiskLevel {
        ActionRiskLevel::ReadOnly
    }

    fn execute<'a>(
        &'a self,
        _ctx: ActionContext,
        input: Self::Input,
    ) -> BoxFuture<'a, Result<Self::Output, ActionError>> {
        Box::pin(async move {
            if input.chat_id == 0 {
                return Err(ActionError::InvalidInput("Invalid chat_id 0".into()));
            }

            Ok(GetChatHistoryOutput {
                chat_id: input.chat_id,
                messages: Vec::new(),
                has_more: false,
            })
        })
    }
}
