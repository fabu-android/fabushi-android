//! Telegram Stars & Ton Fixed-Point Currency Representation
//! Upstream sources: `Telegram/SourceFiles/core/credits_amount.h`

use serde::{Deserialize, Serialize};
use std::ops::{Add, AddAssign, Mul, MulAssign, Sub, SubAssign};

pub const ONE_STAR_IN_NANO: i64 = 1_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum CreditsType {
    #[default]
    Stars,
    Ton,
}

/// Fixed-point currency amount for Telegram Stars and TON ($10^9$ nanos).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreditsAmount {
    pub whole: i64,
    pub nano: i64,
    pub credits_type: CreditsType,
}

impl Default for CreditsAmount {
    fn default() -> Self {
        Self {
            whole: 0,
            nano: 0,
            credits_type: CreditsType::Stars,
        }
    }
}

impl CreditsAmount {
    pub fn new(whole: i64, credits_type: CreditsType) -> Self {
        Self {
            whole,
            nano: 0,
            credits_type,
        }
    }

    pub fn new_nano(whole: i64, nano: i64, credits_type: CreditsType) -> Self {
        let mut res = Self {
            whole,
            nano,
            credits_type,
        };
        res.normalize();
        res
    }

    pub fn stars(whole: i64) -> Self {
        Self::new(whole, CreditsType::Stars)
    }

    pub fn ton(whole: i64) -> Self {
        Self::new(whole, CreditsType::Ton)
    }

    pub fn value(&self) -> f64 {
        (self.whole as f64) + (self.nano as f64) / (ONE_STAR_IN_NANO as f64)
    }

    pub fn is_empty(&self) -> bool {
        self.whole == 0 && self.nano == 0
    }

    pub fn is_ton(&self) -> bool {
        self.credits_type == CreditsType::Ton
    }

    pub fn is_stars(&self) -> bool {
        self.credits_type == CreditsType::Stars
    }

    pub fn normalize(&mut self) {
        if self.nano < 0 {
            let shifts = (-self.nano + ONE_STAR_IN_NANO - 1) / ONE_STAR_IN_NANO;
            self.nano += shifts * ONE_STAR_IN_NANO;
            self.whole -= shifts;
        } else if self.nano >= ONE_STAR_IN_NANO {
            let shifts = self.nano / ONE_STAR_IN_NANO;
            self.nano -= shifts * ONE_STAR_IN_NANO;
            self.whole += shifts;
        }
    }

    pub fn abs(&self) -> Self {
        if self.whole < 0 || (self.whole == 0 && self.nano < 0) {
            Self::new_nano(-self.whole, -self.nano, self.credits_type)
        } else {
            *self
        }
    }
}

impl Add for CreditsAmount {
    type Output = Self;
    fn add(mut self, rhs: Self) -> Self::Output {
        self += rhs;
        self
    }
}

impl AddAssign for CreditsAmount {
    fn add_assign(&mut self, rhs: Self) {
        self.whole += rhs.whole;
        self.nano += rhs.nano;
        self.normalize();
    }
}

impl Sub for CreditsAmount {
    type Output = Self;
    fn sub(mut self, rhs: Self) -> Self::Output {
        self -= rhs;
        self
    }
}

impl SubAssign for CreditsAmount {
    fn sub_assign(&mut self, rhs: Self) {
        self.whole -= rhs.whole;
        self.nano -= rhs.nano;
        self.normalize();
    }
}

impl Mul<i64> for CreditsAmount {
    type Output = Self;
    fn mul(mut self, rhs: i64) -> Self::Output {
        self *= rhs;
        self
    }
}

impl MulAssign<i64> for CreditsAmount {
    fn mul_assign(&mut self, rhs: i64) {
        self.whole *= rhs;
        self.nano *= rhs;
        self.normalize();
    }
}

impl PartialOrd for CreditsAmount {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CreditsAmount {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.whole.cmp(&other.whole) {
            std::cmp::Ordering::Equal => self.nano.cmp(&other.nano),
            ord => ord,
        }
    }
}
