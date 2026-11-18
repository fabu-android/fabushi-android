//! Telegram Desktop 1:1 Parity Module: `BankCardClickHandler`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/core/bank_card_click_handler.h, Telegram/SourceFiles/core/bank_card_click_handler.cpp

use serde::{Deserialize, Serialize};

/// Click handler for inline bank card numbers, providing Luhn validation,
/// card network detection, clean 4-digit grouping, and copy tooltip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankCardClickHandler {
    pub raw_number: String,
}

impl Default for BankCardClickHandler {
    fn default() -> Self {
        Self {
            raw_number: String::new(),
        }
    }
}

impl BankCardClickHandler {
    pub fn new(card_number: impl Into<String>) -> Self {
        let raw: String = card_number
            .into()
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect();

        Self { raw_number: raw }
    }

    pub fn card_number(&self) -> &str {
        &self.raw_number
    }

    /// Formats card number into 4-digit blocks, e.g. "4532 1234 5678 9012".
    pub fn formatted_card_number(&self) -> String {
        let mut result = String::with_capacity(self.raw_number.len() + 4);
        for (i, ch) in self.raw_number.chars().enumerate() {
            if i > 0 && i % 4 == 0 {
                result.push(' ');
            }
            result.push(ch);
        }
        result
    }

    /// Validates bank card number via standard Luhn mod-10 algorithm.
    pub fn is_valid_luhn(&self) -> bool {
        if self.raw_number.len() < 12 || self.raw_number.len() > 19 {
            return false;
        }

        let mut sum = 0u32;
        let mut alternate = false;

        for ch in self.raw_number.chars().rev() {
            let digit = match ch.to_digit(10) {
                Some(d) => d,
                None => return false,
            };

            if alternate {
                let doubled = digit * 2;
                sum += if doubled > 9 { doubled - 9 } else { doubled };
            } else {
                sum += digit;
            }
            alternate = !alternate;
        }

        sum % 10 == 0
    }

    /// Identifies payment card network from issuer identification number (IIN/BIN).
    pub fn card_network(&self) -> &'static str {
        if self.raw_number.starts_with('4') {
            "Visa"
        } else if self.raw_number.starts_with("220") {
            "Mir"
        } else if self.raw_number.starts_with("51")
            || self.raw_number.starts_with("52")
            || self.raw_number.starts_with("53")
            || self.raw_number.starts_with("54")
            || self.raw_number.starts_with("55")
            || (self.raw_number.starts_with("22") && self.raw_number.len() >= 4)
        {
            "Mastercard"
        } else if self.raw_number.starts_with("34") || self.raw_number.starts_with("37") {
            "American Express"
        } else if self.raw_number.starts_with("6011") || self.raw_number.starts_with("65") {
            "Discover"
        } else if self.raw_number.starts_with("62") {
            "UnionPay"
        } else {
            "Unknown"
        }
    }

    pub fn tooltip(&self) -> String {
        format!("{} ({})", self.formatted_card_number(), self.card_network())
    }
}
