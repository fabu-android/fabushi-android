//! Telegram Desktop 1:1 Parity Submodule: `PaymentsReactionBoxUi`
//! Subsystem: `payments` | Feature ID: `TCG-N14`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/payments/ui/payments_reaction_box.cpp, Telegram/SourceFiles/payments/ui/payments_reaction_box.h
//! Inherited Paths: None

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PaidReactionTop {
    pub name: String,
    pub stars_count: i64,
    pub peer_id: u64,
    pub is_anonymous: bool,
    pub rank: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaidReactionBoxArgs {
    pub current_balance: i64,
    pub default_stars: i64,
    pub min_stars: i64,
    pub max_stars: i64,
    pub is_anonymous: bool,
    pub top_reactors: Vec<PaidReactionTop>,
}

impl Default for PaidReactionBoxArgs {
    fn default() -> Self {
        Self {
            current_balance: 0,
            default_stars: 50,
            min_stars: 1,
            max_stars: 10_000,
            is_anonymous: false,
            top_reactors: Vec::new(),
        }
    }
}

pub struct StarSelectDiscreter;

impl StarSelectDiscreter {
    pub const STEPS: &'static [i64] = &[1, 5, 10, 25, 50, 100, 250, 500, 1000, 2500, 5000, 10000];

    pub fn value_to_index(value: i64) -> usize {
        Self::STEPS
            .iter()
            .position(|&v| v >= value)
            .unwrap_or(Self::STEPS.len() - 1)
    }

    pub fn index_to_value(index: usize) -> i64 {
        if index < Self::STEPS.len() {
            Self::STEPS[index]
        } else {
            *Self::STEPS.last().unwrap_or(&1)
        }
    }
}

pub fn format_count_decimal(count: i64) -> String {
    let s = count.to_string();
    let mut result = String::new();
    let mut count = 0;
    for c in s.chars().rev() {
        if count > 0 && count % 3 == 0 {
            result.push(',');
        }
        result.push(c);
        count += 1;
    }
    result.chars().rev().collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PaymentsReactionBoxUi {
    pub is_enabled: bool,
    pub args: PaidReactionBoxArgs,
    pub selected_stars: i64,
    pub is_anonymous: bool,
    pub is_sent: bool,
    pub is_balance_insufficient: bool,
}

impl PaymentsReactionBoxUi {
    pub fn new(args: PaidReactionBoxArgs) -> Self {
        let initial_stars = args.default_stars;
        let is_anon = args.is_anonymous;
        let balance_low = args.current_balance < initial_stars;
        Self {
            is_enabled: true,
            args,
            selected_stars: initial_stars,
            is_anonymous: is_anon,
            is_sent: false,
            is_balance_insufficient: balance_low,
        }
    }

    pub fn set_stars(&mut self, stars: i64) {
        let clamped = stars.clamp(self.args.min_stars, self.args.max_stars);
        self.selected_stars = clamped;
        self.is_balance_insufficient = self.args.current_balance < clamped;
    }

    pub fn toggle_anonymous(&mut self) {
        self.is_anonymous = !self.is_anonymous;
    }

    pub fn formatted_selected_stars(&self) -> String {
        format_count_decimal(self.selected_stars)
    }

    pub fn top_reactors_sorted(&self) -> Vec<PaidReactionTop> {
        let mut list = self.args.top_reactors.clone();
        list.sort_by(|a, b| b.stars_count.cmp(&a.stars_count));
        for (i, item) in list.iter_mut().enumerate() {
            item.rank = (i + 1) as u32;
        }
        list
    }

    pub fn send(&mut self) -> Result<i64, &'static str> {
        if self.is_balance_insufficient {
            return Err("Insufficient Telegram Stars balance");
        }
        self.is_sent = true;
        Ok(self.selected_stars)
    }
}
