//! Telegram Desktop 1:1 Parity Submodule: `SmartglocalCallbacksSmartglocal`
//! Subsystem: `payments` | Feature ID: `TCG-N24`
//! Upstream Sources: `Telegram/SourceFiles/payments/smartglocal/smartglocal_callbacks.h`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SmartglocalTokenData {
    pub token_id: String,
    pub card_type: String,
    pub masked_number: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SmartglocalErrorData {
    pub code: i32,
    pub description: String,
    pub message: String,
}

pub type TokenCompletionHandler = Box<dyn Fn(Option<SmartglocalTokenData>, Option<SmartglocalErrorData>) + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmartglocalCallbacksSmartglocal {
    pub is_enabled: bool,
    pub token: Option<SmartglocalTokenData>,
    pub error: Option<SmartglocalErrorData>,
    pub is_completed: bool,
    pub updated_at: i64,
}

impl Default for SmartglocalCallbacksSmartglocal {
    fn default() -> Self {
        Self {
            is_enabled: true,
            token: None,
            error: None,
            is_completed: false,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl SmartglocalCallbacksSmartglocal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.is_enabled && !self.is_completed
    }

    pub fn on_success(&mut self, token: SmartglocalTokenData) {
        self.token = Some(token);
        self.error = None;
        self.is_completed = true;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn on_error(&mut self, error: SmartglocalErrorData) {
        self.error = Some(error);
        self.token = None;
        self.is_completed = true;
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
