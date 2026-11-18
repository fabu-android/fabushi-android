use crate::action_bus::{ActionCategory, ActionContext, ActionError, ActionRiskLevel, TypedPlatformAction};
use futures::future::BoxFuture;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn default_dialogs_limit() -> u32 {
    30
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DialogSummaryItem {
    pub chat_id: i64,
    pub title: String,
    pub unread_count: u32,
    #[serde(default)]
    pub last_message_text: Option<String>,
    pub last_message_date_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListDialogsInput {
    #[serde(default = "default_dialogs_limit")]
    pub limit: u32,
    #[serde(default)]
    pub folder_id: Option<i32>,
    #[serde(default)]
    pub unread_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListDialogsOutput {
    pub dialogs: Vec<DialogSummaryItem>,
    pub total_unread_chats: u32,
}

#[derive(Debug, Clone, Default)]
pub struct ListDialogsAction;

impl TypedPlatformAction for ListDialogsAction {
    type Input = ListDialogsInput;
    type Output = ListDialogsOutput;

    fn name(&self) -> &'static str {
        "list_dialogs"
    }

    fn category(&self) -> ActionCategory {
        ActionCategory::Dialogs
    }

    fn description(&self) -> &'static str {
        "List recent dialogs, active chats, and conversation summaries"
    }

    fn risk_level(&self) -> ActionRiskLevel {
        ActionRiskLevel::ReadOnly
    }

    fn execute<'a>(
        &'a self,
        _ctx: ActionContext,
        _input: Self::Input,
    ) -> BoxFuture<'a, Result<Self::Output, ActionError>> {
        Box::pin(async move {
            Ok(ListDialogsOutput {
                dialogs: Vec::new(),
                total_unread_chats: 0,
            })
        })
    }
}
