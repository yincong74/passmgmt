//! 密码生成器：系统级 CSPRNG（OsRng），拒绝采样保证每个选中字符集至少一个字符。

use rand::Rng;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
pub const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
pub const DIGITS: &str = "0123456789";
pub const SYMBOLS: &str = "!@#$%^&*()-_=+[]{};:,.<>?";
pub const AMBIGUOUS: &str = "0O1lI|";
pub const MIN_LENGTH: usize = 8;
pub const MAX_LENGTH: usize = 64;

#[derive(Debug, Error)]
pub enum GenError {
    #[error("长度必须在 {MIN_LENGTH}-{MAX_LENGTH} 之间")]
    InvalidLength,
    #[error("至少选择一个字符集")]
    NoCharSets,
    #[error("某字符集过滤后为空")]
    EmptySet,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenOptions {
    pub length: usize,
    pub lowercase: bool,
    pub uppercase: bool,
    pub digits: bool,
    pub symbols: bool,
    pub exclude_ambiguous: bool,
}

impl Default for GenOptions {
    fn default() -> Self {
        Self {
            length: 20,
            lowercase: true,
            uppercase: true,
            digits: true,
            symbols: true,
            exclude_ambiguous: true,
        }
    }
}

fn filter_set(set: &str, exclude_ambiguous: bool) -> Vec<char> {
    set.chars()
        .filter(|c| !exclude_ambiguous || !AMBIGUOUS.contains(*c))
        .collect()
}

pub fn generate(opts: &GenOptions) -> Result<String, GenError> {
    if opts.length < MIN_LENGTH || opts.length > MAX_LENGTH {
        return Err(GenError::InvalidLength);
    }
    let mut sets: Vec<Vec<char>> = Vec::new();
    if opts.lowercase { sets.push(filter_set(LOWER, opts.exclude_ambiguous)); }
    if opts.uppercase { sets.push(filter_set(UPPER, opts.exclude_ambiguous)); }
    if opts.digits { sets.push(filter_set(DIGITS, opts.exclude_ambiguous)); }
    if opts.symbols { sets.push(filter_set(SYMBOLS, opts.exclude_ambiguous)); }
    if sets.is_empty() {
        return Err(GenError::NoCharSets);
    }
    if sets.iter().any(|s| s.is_empty()) {
        return Err(GenError::EmptySet);
    }

    let mut rng = rand::rngs::OsRng;
    let mut chars: Vec<char> = Vec::with_capacity(opts.length);
    // 每个选中字符集先保证一个字符
    for set in &sets {
        chars.push(set[rng.gen_range(0..set.len())]);
    }
    // 其余从合并池抽取
    let pool: Vec<char> = sets.concat();
    while chars.len() < opts.length {
        chars.push(pool[rng.gen_range(0..pool.len())]);
    }
    // Fisher-Yates 洗牌，消除"前几位必来自各字符集"的位置规律
    for i in (1..chars.len()).rev() {
        let j = rng.gen_range(0..=i);
        chars.swap(i, j);
    }
    Ok(chars.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_options_produce_20_chars() {
        let pw = generate(&GenOptions::default()).unwrap();
        assert_eq!(pw.chars().count(), 20);
    }

    #[test]
    fn length_is_exact() {
        for len in [MIN_LENGTH, 16, MAX_LENGTH] {
            let o = GenOptions { length: len, ..Default::default() };
            assert_eq!(generate(&o).unwrap().chars().count(), len);
        }
    }

    #[test]
    fn invalid_length_rejected() {
        let o = GenOptions { length: 7, ..Default::default() };
        assert!(matches!(generate(&o), Err(GenError::InvalidLength)));
        let o = GenOptions { length: 65, ..Default::default() };
        assert!(matches!(generate(&o), Err(GenError::InvalidLength)));
    }

    #[test]
    fn no_charsets_rejected() {
        let o = GenOptions {
            lowercase: false, uppercase: false, digits: false, symbols: false,
            ..Default::default()
        };
        assert!(matches!(generate(&o), Err(GenError::NoCharSets)));
    }

    #[test]
    fn only_selected_sets_appear() {
        let o = GenOptions {
            lowercase: false, uppercase: false, digits: true, symbols: false,
            ..Default::default()
        };
        let pw = generate(&o).unwrap();
        assert!(pw.chars().all(|c| DIGITS.contains(c)));
    }

    #[test]
    fn ambiguous_excluded_when_requested() {
        let o = GenOptions { exclude_ambiguous: true, ..Default::default() };
        for _ in 0..20 {
            let pw = generate(&o).unwrap();
            assert!(!pw.chars().any(|c| AMBIGUOUS.contains(c)));
        }
    }

    #[test]
    fn ambiguous_allowed_when_not_excluded() {
        // 仅数字、不排除混淆：大量抽样应见到 '0' 和 '1'
        let o = GenOptions {
            length: 64, lowercase: false, uppercase: false, digits: true,
            symbols: false, exclude_ambiguous: false,
        };
        let (mut seen0, mut seen1) = (false, false);
        for _ in 0..50 {
            let pw = generate(&o).unwrap();
            seen0 |= pw.contains('0');
            seen1 |= pw.contains('1');
        }
        assert!(seen0 && seen1);
    }

    #[test]
    fn each_selected_set_represented() {
        let o = GenOptions::default();
        for _ in 0..50 {
            let pw = generate(&o).unwrap();
            assert!(pw.chars().any(|c| LOWER.contains(c) && !AMBIGUOUS.contains(c)));
            assert!(pw.chars().any(|c| UPPER.contains(c) && !AMBIGUOUS.contains(c)));
            assert!(pw.chars().any(|c| DIGITS.contains(c) && !AMBIGUOUS.contains(c)));
            assert!(pw.chars().any(|c| SYMBOLS.contains(c)));
        }
    }

    #[test]
    fn two_calls_differ() {
        let a = generate(&GenOptions::default()).unwrap();
        let b = generate(&GenOptions::default()).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn options_json_is_snake_case_with_defaults() {
        let json = serde_json::to_string(&GenOptions::default()).unwrap();
        assert!(json.contains("\"length\":20"));
        assert!(json.contains("\"exclude_ambiguous\":true"));
    }
}
