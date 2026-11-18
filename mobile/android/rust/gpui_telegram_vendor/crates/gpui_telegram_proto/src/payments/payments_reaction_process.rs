//! Telegram Desktop 1:1 Parity Submodule: `PaymentsReactionProcess`
//! Subsystem: `payments` | Feature ID: `TCG-N14`
//! Upstream Sources: `Telegram/SourceFiles/payments/payments_reaction_process.cpp`, `Telegram/SourceFiles/payments/payments_reaction_process.h`

use serde::{Deserialize, Serialize};

pub const K_MAX_PER_REACTION_FALLBACK: i32 = 10_000;
pub const K_DEFAULT_PER_REACTION: i32 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum HistoryReactionSource {
    #[default]
    Quick,
    Menu,
    Dialog,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PaidReactionDonor {
    pub peer_id: i64,
    pub count: i32,
    pub is_anonymous: bool,
    pub is_my: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaymentsReactionProcess {
    pub is_enabled: bool,
    pub message_peer_id: i64,
    pub message_id: i32,
    pub selected_count: i32,
    pub is_anonymous: bool,
    pub top_donors: Vec<PaidReactionDonor>,
    pub my_paid_anonymous: bool,
    pub reaction_flying: bool,
    pub updated_at: i64,
}

impl Default for PaymentsReactionProcess {
    fn default() -> Self {
        Self {
            is_enabled: true,
            message_peer_id: 0,
            message_id: 0,
            selected_count: K_DEFAULT_PER_REACTION,
            is_anonymous: false,
            top_donors: Vec::new(),
            my_paid_anonymous: false,
            reaction_flying: false,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

impl PaymentsReactionProcess {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn for_message(peer_id: i64, message_id: i32) -> Self {
        Self {
            message_peer_id: peer_id,
            message_id,
            ..Self::default()
        }
    }

    pub fn set_reaction_count(&mut self, count: i32) {
        self.selected_count = count.clamp(1, K_MAX_PER_REACTION_FALLBACK);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn set_anonymous(&mut self, anonymous: bool) {
        self.is_anonymous = anonymous;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn try_adding_paid_reaction(&mut self, count: i32, _shown_peer: Option<i64>) -> Result<bool, &'static str> {
        let clamped = count.clamp(1, K_MAX_PER_REACTION_FALLBACK);
        self.selected_count = clamped;
        self.reaction_flying = true;
        self.updated_at = chrono::Utc::now().timestamp();
        Ok(true)
    }

    pub fn lookup_my_paid_anonymous(&self) -> bool {
        self.top_donors.iter().any(|d| d.is_my && d.is_anonymous)
    }

    pub fn add_donor(&mut self, donor: PaidReactionDonor) {
        if donor.is_my {
            self.my_paid_anonymous = donor.is_anonymous;
        }
        self.top_donors.push(donor);
        self.top_donors.sort_by(|a, b| b.count.cmp(&a.count));
        self.updated_at = chrono::Utc::now().timestamp();
    }
}
