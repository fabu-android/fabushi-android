//! Telegram Desktop Password Strength Cyrillic Transliteration
//! Upstream sources: `Telegram/SourceFiles/_other/zxcvbn_translit.cpp`

use crate::core::utils::translit_rus_eng;

pub const MINIMUM_MAPPED_LENGTH: usize = 2;

/// Transliteration preprocessor feeding sanitized Latin tokens to the zxcvbn password scorer.
#[derive(Debug, Clone, Copy, Default)]
pub struct ZxcvbnTranslit;

impl ZxcvbnTranslit {
    pub fn is_dropped_markup(word: &str) -> bool {
        matches!(word, "chffffff" | "fntahoma" | "b1" | "i0")
    }

    pub fn map_word(word: &str) -> Option<String> {
        if Self::is_dropped_markup(word) {
            return None;
        }
        let transliterated = translit_rus_eng(word);
        if transliterated.len() >= MINIMUM_MAPPED_LENGTH {
            Some(transliterated)
        } else {
            None
        }
    }
}
