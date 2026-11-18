//! Telegram Desktop Core Mathematical, Digest & String Utilities
//! Upstream sources: `Telegram/SourceFiles/core/utils.h`, `utils.cpp`

use sha2::{Digest, Sha256};

pub fn in_range<T: PartialOrd>(value: T, from: T, till: T) -> bool {
    value >= from && value < till
}

pub fn rowscount(full_count: usize, count_per_row: usize) -> usize {
    if count_per_row == 0 {
        return 0;
    }
    (full_count + count_per_row - 1) / count_per_row
}

pub fn hash_sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

pub fn translit_rus_eng(rus: &str) -> String {
    let mut out = String::with_capacity(rus.len());
    for ch in rus.chars() {
        let mapped = match ch {
            'а' | 'А' => "a",
            'б' | 'Б' => "b",
            'в' | 'В' => "v",
            'г' | 'Г' => "g",
            'д' | 'Д' => "d",
            'е' | 'Е' | 'ё' | 'Ё' => "e",
            'ж' | 'Ж' => "zh",
            'з' | 'З' => "z",
            'и' | 'И' => "i",
            'й' | 'Й' => "y",
            'к' | 'К' => "k",
            'л' | 'Л' => "l",
            'м' | 'М' => "m",
            'н' | 'Н' => "n",
            'о' | 'О' => "o",
            'п' | 'П' => "p",
            'р' | 'Р' => "r",
            'с' | 'С' => "s",
            'т' | 'Т' => "t",
            'у' | 'У' => "u",
            'ф' | 'Ф' => "f",
            'х' | 'Х' => "kh",
            'ц' | 'Ц' => "ts",
            'ч' | 'Ч' => "ch",
            'ш' | 'Ш' => "sh",
            'щ' | 'Щ' => "shch",
            'ы' | 'Ы' => "y",
            'э' | 'Э' => "e",
            'ю' | 'Ю' => "yu",
            'я' | 'Я' => "ya",
            other => {
                out.push(other);
                continue;
            }
        };
        out.push_str(mapped);
    }
    out
}

/// Core utilities struct container.
#[derive(Debug, Clone, Copy, Default)]
pub struct Utils;

impl Utils {
    pub fn in_range<T: PartialOrd>(value: T, from: T, till: T) -> bool {
        in_range(value, from, till)
    }

    pub fn rowscount(full_count: usize, count_per_row: usize) -> usize {
        rowscount(full_count, count_per_row)
    }

    pub fn hash_sha256(data: &[u8]) -> [u8; 32] {
        hash_sha256(data)
    }

    pub fn translit(s: &str) -> String {
        translit_rus_eng(s)
    }
}
