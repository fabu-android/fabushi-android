use futures::future::BoxFuture;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActionCategory {
    Messaging,
    Dialogs,
    Chat,
    Media,
    Search,
    User,
    Call,
    MiniApp,
    Admin,
    System,
}

impl ActionCategory {
    pub const ALL: [ActionCategory; 10] = [
        Self::Messaging,
        Self::Dialogs,
        Self::Chat,
        Self::Media,
        Self::Search,
        Self::User,
        Self::Call,
        Self::MiniApp,
        Self::Admin,
        Self::System,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Messaging => "messaging",
            Self::Dialogs => "dialogs",
            Self::Chat => "chat",
            Self::Media => "media",
            Self::Search => "search",
            Self::User => "user",
            Self::Call => "call",
            Self::MiniApp => "miniapp",
            Self::Admin => "admin",
            Self::System => "system",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActionRiskLevel {
    ReadOnly,
    Low,
    Medium,
    High,
    CriticalDestructive,
}

impl ActionRiskLevel {
    pub fn is_read_only(&self) -> bool {
        matches!(self, Self::ReadOnly)
    }

    pub fn is_sensitive(&self) -> bool {
        matches!(self, Self::Medium | Self::High | Self::CriticalDestructive)
    }

    pub fn requires_explicit_confirmation(&self) -> bool {
        matches!(self, Self::High | Self::CriticalDestructive)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActionContext {
    pub user_id: Option<u64>,
    pub session_id: Option<String>,
    pub correlation_id: String,
    pub is_bot: bool,
    pub roles: Vec<String>,
    pub metadata: HashMap<String, Value>,
    #[serde(default)]
    pub approved: bool,
}

impl ActionContext {
    pub fn new() -> Self {
        Self {
            user_id: None,
            session_id: None,
            correlation_id: uuid::Uuid::new_v4().to_string(),
            is_bot: false,
            roles: Vec::new(),
            metadata: HashMap::new(),
            approved: false,
        }
    }

    pub fn with_user(mut self, user_id: u64) -> Self {
        self.user_id = Some(user_id);
        self
    }

    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    pub fn with_correlation_id(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = correlation_id.into();
        self
    }

    pub fn with_approval(mut self, approved: bool) -> Self {
        self.approved = approved;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum ActionError {
    #[error("Action not found: {0}")]
    NotFound(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Rate limited: {0}")]
    RateLimited(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

impl ActionError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "not_found",
            Self::InvalidInput(_) => "invalid_input",
            Self::Unauthorized(_) => "unauthorized",
            Self::Forbidden(_) => "forbidden",
            Self::ExecutionFailed(_) => "execution_failed",
            Self::RateLimited(_) => "rate_limited",
            Self::Serialization(_) => "serialization",
        }
    }

    pub fn details(&self) -> Option<Value> {
        Some(Value::String(self.to_string()))
    }
}

/// Dynamic, object-safe platform action contract
pub trait PlatformAction: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn category(&self) -> ActionCategory;
    fn description(&self) -> &'static str;
    fn risk_level(&self) -> ActionRiskLevel;
    fn input_schema(&self) -> Value;
    fn output_schema(&self) -> Value;
    fn execute<'a>(
        &'a self,
        ctx: ActionContext,
        input: Value,
    ) -> BoxFuture<'a, Result<Value, ActionError>>;
}

/// Strongly-typed platform action contract with automatic JSON Schema derivation
pub trait TypedPlatformAction: Send + Sync + 'static {
    type Input: serde::de::DeserializeOwned + JsonSchema + Send + 'static;
    type Output: Serialize + JsonSchema + Send + 'static;

    fn name(&self) -> &'static str;
    fn category(&self) -> ActionCategory;
    fn description(&self) -> &'static str;
    fn risk_level(&self) -> ActionRiskLevel;

    fn execute<'a>(
        &'a self,
        ctx: ActionContext,
        input: Self::Input,
    ) -> BoxFuture<'a, Result<Self::Output, ActionError>>;
}

impl<T: TypedPlatformAction> PlatformAction for T {
    fn name(&self) -> &'static str {
        TypedPlatformAction::name(self)
    }

    fn category(&self) -> ActionCategory {
        TypedPlatformAction::category(self)
    }

    fn description(&self) -> &'static str {
        TypedPlatformAction::description(self)
    }

    fn risk_level(&self) -> ActionRiskLevel {
        TypedPlatformAction::risk_level(self)
    }

    fn input_schema(&self) -> Value {
        serde_json::to_value(schemars::schema_for!(T::Input)).unwrap_or(Value::Null)
    }

    fn output_schema(&self) -> Value {
        serde_json::to_value(schemars::schema_for!(T::Output)).unwrap_or(Value::Null)
    }

    fn execute<'a>(
        &'a self,
        ctx: ActionContext,
        input: Value,
    ) -> BoxFuture<'a, Result<Value, ActionError>> {
        Box::pin(async move {
            let typed_input = serde_json::from_value::<T::Input>(input)
                .map_err(|e| ActionError::InvalidInput(format!("Validation error: {e}")))?;
            let typed_output = TypedPlatformAction::execute(self, ctx, typed_input).await?;
            serde_json::to_value(typed_output)
                .map_err(|e| ActionError::Serialization(format!("Serialization error: {e}")))
        })
    }
}
