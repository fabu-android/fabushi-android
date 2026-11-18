//! Telegram Desktop 1:1 Parity Submodule: `StripeCardValidatorStripe`
//! Subsystem: `payments` | Feature ID: `CAP-06`
//! Disposition: `inherited_rust_reused`
//! Upstream Sources: Telegram/SourceFiles/payments/stripe/stripe_card_validator.cpp, Telegram/SourceFiles/payments/stripe/stripe_card_validator.h
//! Inherited Paths: inherited/mahayana-messaging/src/payment.rs

use chrono::{Datelike, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ValidationState {
    #[default]
    Invalid,
    Incomplete,
    Valid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CardBrand {
    Visa,
    Amex,
    MasterCard,
    Discover,
    JCB,
    DinersClub,
    UnionPay,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BinRange {
    pub low: String,
    pub high: String,
    pub length: usize,
    pub brand: CardBrand,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CardValidationResult {
    pub state: ValidationState,
    pub brand: CardBrand,
    pub finished: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ExpireDateValidationResult {
    pub state: ValidationState,
    pub finished: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CvcValidationResult {
    pub state: ValidationState,
    pub finished: bool,
}

pub fn all_ranges() -> Vec<BinRange> {
    vec![
        BinRange { low: "".into(), high: "".into(), length: 19, brand: CardBrand::Unknown },
        BinRange { low: "34".into(), high: "34".into(), length: 15, brand: CardBrand::Amex },
        BinRange { low: "37".into(), high: "37".into(), length: 15, brand: CardBrand::Amex },
        BinRange { low: "30".into(), high: "30".into(), length: 16, brand: CardBrand::DinersClub },
        BinRange { low: "36".into(), high: "36".into(), length: 14, brand: CardBrand::DinersClub },
        BinRange { low: "38".into(), high: "39".into(), length: 16, brand: CardBrand::DinersClub },
        BinRange { low: "60".into(), high: "60".into(), length: 16, brand: CardBrand::Discover },
        BinRange { low: "64".into(), high: "65".into(), length: 16, brand: CardBrand::Discover },
        BinRange { low: "35".into(), high: "35".into(), length: 16, brand: CardBrand::JCB },
        BinRange { low: "50".into(), high: "59".into(), length: 16, brand: CardBrand::MasterCard },
        BinRange { low: "22".into(), high: "27".into(), length: 16, brand: CardBrand::MasterCard },
        BinRange { low: "67".into(), high: "67".into(), length: 16, brand: CardBrand::MasterCard },
        BinRange { low: "62".into(), high: "62".into(), length: 16, brand: CardBrand::UnionPay },
        BinRange { low: "81".into(), high: "81".into(), length: 16, brand: CardBrand::UnionPay },
        BinRange { low: "40".into(), high: "49".into(), length: 16, brand: CardBrand::Visa },
    ]
}

pub fn remove_whitespaces(value: &str) -> String {
    value.chars().filter(|c| !c.is_whitespace()).collect()
}

pub fn is_numeric(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|c| c.is_ascii_digit())
}

pub fn bin_range_matches(range: &BinRange, sanitized: &str) -> bool {
    if range.low.is_empty() {
        return true;
    }
    let min_low = sanitized.len().min(range.low.len());
    let san_low_prefix = &sanitized[..min_low];
    let range_low_prefix = &range.low[..min_low];
    if san_low_prefix < range_low_prefix {
        return false;
    }
    let min_high = sanitized.len().min(range.high.len());
    let san_high_prefix = &sanitized[..min_high];
    let range_high_prefix = &range.high[..min_high];
    if san_high_prefix > range_high_prefix {
        return false;
    }
    true
}

pub fn is_valid_luhn(sanitized: &str) -> bool {
    let mut odd = true;
    let mut sum: u32 = 0;
    for c in sanitized.chars().rev() {
        let Some(mut digit) = c.to_digit(10) else {
            return false;
        };
        odd = !odd;
        if odd {
            digit *= 2;
        }
        if digit > 9 {
            digit -= 9;
        }
        sum += digit;
    }
    (sum % 10) == 0
}

pub fn most_specific_bin_range(sanitized: &str) -> BinRange {
    let ranges = all_ranges();
    let mut matches: Vec<BinRange> = ranges
        .into_iter()
        .filter(|r| bin_range_matches(r, sanitized))
        .collect();
    matches.sort_by(|a, b| a.low.len().cmp(&b.low.len()));
    matches.pop().unwrap_or(BinRange {
        low: "".into(),
        high: "".into(),
        length: 16,
        brand: CardBrand::Unknown,
    })
}

pub fn brand_for_number(number: &str) -> CardBrand {
    let sanitized = remove_whitespaces(number);
    if !is_numeric(&sanitized) {
        return CardBrand::Unknown;
    }
    most_specific_bin_range(&sanitized).brand
}

pub fn validate_card(number: &str) -> CardValidationResult {
    let sanitized = remove_whitespaces(number);
    if sanitized.is_empty() {
        return CardValidationResult {
            state: ValidationState::Incomplete,
            brand: CardBrand::Unknown,
            finished: false,
        };
    }
    if !is_numeric(&sanitized) {
        return CardValidationResult {
            state: ValidationState::Invalid,
            brand: CardBrand::Unknown,
            finished: false,
        };
    }
    let range = most_specific_bin_range(&sanitized);
    let brand = range.brand;
    let target_len = if range.length > 0 { range.length } else { 16 };

    if sanitized.len() > 19 {
        CardValidationResult { state: ValidationState::Invalid, brand, finished: false }
    } else if sanitized.len() < target_len {
        CardValidationResult { state: ValidationState::Incomplete, brand, finished: false }
    } else if !is_valid_luhn(&sanitized) {
        CardValidationResult { state: ValidationState::Invalid, brand, finished: false }
    } else {
        CardValidationResult {
            state: ValidationState::Valid,
            brand,
            finished: sanitized.len() >= target_len,
        }
    }
}

pub fn validate_expire_date(date: &str, threshold: Option<chrono::NaiveDate>) -> ExpireDateValidationResult {
    let sanitized = remove_whitespaces(date).replace('/', "");
    if sanitized.is_empty() || !is_numeric(&sanitized) {
        return ExpireDateValidationResult { state: ValidationState::Invalid, finished: false };
    }
    if sanitized.len() < 2 {
        return ExpireDateValidationResult { state: ValidationState::Incomplete, finished: false };
    }
    let normalized = if sanitized.starts_with('2') || sanitized.starts_with('3') || sanitized.starts_with('4') || sanitized.starts_with('5') || sanitized.starts_with('6') || sanitized.starts_with('7') || sanitized.starts_with('8') || sanitized.starts_with('9') {
        format!("0{}", sanitized)
    } else {
        sanitized
    };
    if normalized.len() < 2 {
        return ExpireDateValidationResult { state: ValidationState::Incomplete, finished: false };
    }
    let month: u32 = match normalized[..2].parse() {
        Ok(m) if (1..=12).contains(&m) => m,
        _ => return ExpireDateValidationResult { state: ValidationState::Invalid, finished: false },
    };
    if normalized.len() < 4 {
        return ExpireDateValidationResult { state: ValidationState::Incomplete, finished: false };
    }
    if normalized.len() > 4 {
        return ExpireDateValidationResult { state: ValidationState::Invalid, finished: false };
    }
    let year: u32 = match normalized[2..4].parse::<u32>() {
        Ok(y) => 2000 + y,
        _ => return ExpireDateValidationResult { state: ValidationState::Invalid, finished: false },
    };
    let now = threshold.unwrap_or_else(|| Utc::now().date_naive());
    let current_year = now.year() as u32;
    let current_month = now.month();
    if year < current_year || (year == current_year && month < current_month) {
        return ExpireDateValidationResult { state: ValidationState::Invalid, finished: false };
    }
    ExpireDateValidationResult { state: ValidationState::Valid, finished: true }
}

pub fn validate_cvc(number: &str, cvc: &str) -> CvcValidationResult {
    let sanitized = remove_whitespaces(cvc);
    if sanitized.is_empty() || !is_numeric(&sanitized) {
        return CvcValidationResult { state: ValidationState::Invalid, finished: false };
    }
    let brand = brand_for_number(number);
    let max_len = if brand == CardBrand::Amex { 4 } else { 3 };
    if sanitized.len() < 3 {
        CvcValidationResult { state: ValidationState::Incomplete, finished: false }
    } else if sanitized.len() > max_len {
        CvcValidationResult { state: ValidationState::Invalid, finished: false }
    } else {
        CvcValidationResult {
            state: ValidationState::Valid,
            finished: sanitized.len() == max_len,
        }
    }
}

pub fn card_number_format(number: &str) -> Vec<usize> {
    let brand = brand_for_number(number);
    match brand {
        CardBrand::Amex => vec![4, 6, 5],
        CardBrand::DinersClub => vec![4, 6, 4],
        _ => vec![4, 4, 4, 4],
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StripeCardValidatorStripe {
    pub is_enabled: bool,
    pub validations_performed: u64,
}

impl StripeCardValidatorStripe {
    pub fn new() -> Self {
        Self { is_enabled: true, validations_performed: 0 }
    }

    pub fn validate_card(&mut self, number: &str) -> CardValidationResult {
        self.validations_performed += 1;
        validate_card(number)
    }

    pub fn validate_cvc(&mut self, number: &str, cvc: &str) -> CvcValidationResult {
        self.validations_performed += 1;
        validate_cvc(number, cvc)
    }

    pub fn validate_expire_date(&mut self, date: &str) -> ExpireDateValidationResult {
        self.validations_performed += 1;
        validate_expire_date(date, None)
    }

    pub fn card_number_format(&self, number: &str) -> Vec<usize> {
        card_number_format(number)
    }
}
