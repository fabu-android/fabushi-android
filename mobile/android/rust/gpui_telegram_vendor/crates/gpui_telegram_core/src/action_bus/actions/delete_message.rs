use crate::action_bus::{ActionCategory, ActionContext, ActionError, ActionRiskLevel, TypedPlatformAction};
use futures::future::BoxFuture;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteMessageInput {
    pub chat_id: i64,
    pub message_ids: Vec<i64>,
    #[serde(default)]
    pub for_everyone: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DeleteMessageOutput {
    pub deleted_count: u32,
    pub chat_id: i64,
}

#[derive(Debug, Clone, Default)]
pub struct DeleteMessageAction;

impl TypedPlatformAction for DeleteMessageAction {
    type Input = DeleteMessageInput;
    type Output = DeleteMessageOutput;

    fn name(&self) -> &'static str {
        "delete_message"
    }

    fn category(&self) -> ActionCategory {
        ActionCategory::Messaging
    }

    fn description(&self) -> &'static str {
        "Delete one or more messages from a chat (destructive, requires approval)"
    }

    fn risk_level(&self) -> ActionRiskLevel {
        ActionRiskLevel::High
    }

    fn execute<'a>(
        &'a self,
        ctx: ActionContext,
        input: Self::Input,
    ) -> BoxFuture<'a, Result<Self::Output, ActionError>> {
        Box::pin(async move {
            if !ctx.approved {
                return Err(ActionError::Forbidden(
                    "Action 'delete_message' requires explicit approval confirmation".into(),
                ));
            }
            if input.message_ids.is_empty() {
                return Err(ActionError::InvalidInput("No message IDs specified for deletion".into()));
            }

            let count = input.message_ids.len() as u32;
            Ok(DeleteMessageOutput {
                deleted_count: count,
                chat_id: input.chat_id,
            })
        })
    }
}
