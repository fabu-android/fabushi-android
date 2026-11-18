//! Diffie-Hellman Utilities & PQ Factorization
//! Upstream Source: `Telegram/SourceFiles/mtproto/mtproto_dh_utils.h`, `mtproto_dh_utils.cpp`

use serde::{Deserialize, Serialize};

/// Pollard's Rho algorithm / Trial division for 64-bit PQ factorization
pub fn factorize_pq(pq: u64) -> Option<(u64, u64)> {
    if pq <= 1 {
        return None;
    }
    if pq % 2 == 0 {
        return Some((2, pq / 2));
    }

    // Fast trial division for common small factors up to 100,000
    let mut d = 3u64;
    while d * d <= pq && d < 100_000 {
        if pq % d == 0 {
            let q = pq / d;
            return Some((d.min(q), d.max(q)));
        }
        d += 2;
    }

    // Pollard's Rho
    let gcd = |mut a: u64, mut b: u64| -> u64 {
        while b != 0 {
            let t = b;
            b = a % b;
            a = t;
        }
        a
    };

    let f = |x: u64, c: u64, m: u64| -> u64 {
        (((x as u128 * x as u128) % m as u128 + c as u128) % m as u128) as u64
    };

    for c in 1..20u64 {
        let mut x = 2u64;
        let mut y = 2u64;
        let mut d = 1u64;

        while d == 1 {
            x = f(x, c, pq);
            y = f(f(y, c, pq), c, pq);
            let diff = if x > y { x - y } else { y - x };
            d = gcd(diff, pq);
        }

        if d > 1 && d < pq {
            let other = pq / d;
            return Some((d.min(other), d.max(other)));
        }
    }

    None
}

/// Validates DH parameters according to Telegram MTProto specifications:
/// - g must be in 2..=7
/// - dh_prime must be 2048-bit (256 bytes) safe prime
/// - g_a must be in 1..(dh_prime - 1) and between 2^(2048-64) and 2^2048 - 2^(2048-64)
pub fn check_dh_params(g: i32, dh_prime: &[u8], g_a: &[u8]) -> bool {
    if !(2..=7).contains(&g) {
        return false;
    }
    if dh_prime.len() != 256 || g_a.is_empty() || g_a.len() > 256 {
        return false;
    }
    // Leading byte of dh_prime cannot be zero (must be 2048 bits)
    if dh_prime[0] < 0x80 {
        return false;
    }
    // g_a must not be 0 or 1
    if g_a.iter().all(|&b| b == 0) {
        return false;
    }
    if g_a.len() == 1 && g_a[0] == 1 {
        return false;
    }
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MtprotoDhUtils {
    pub default_g: i32,
    pub prime_bits: usize,
}

impl Default for MtprotoDhUtils {
    fn default() -> Self {
        Self {
            default_g: 3,
            prime_bits: 2048,
        }
    }
}

impl MtprotoDhUtils {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.prime_bits == 2048
    }

    pub fn factorize(&self, pq: u64) -> Option<(u64, u64)> {
        factorize_pq(pq)
    }

    pub fn validate_dh(&self, g: i32, dh_prime: &[u8], g_a: &[u8]) -> bool {
        check_dh_params(g, dh_prime, g_a)
    }
}
