pub const MAX_SAFE_TOKEN_COUNT: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub reasoning_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TurnEndedUsage {
    pub input_tokens: Option<i128>,
    pub output_tokens: Option<i128>,
    pub cache_read_tokens: Option<i128>,
    pub cache_write_tokens: Option<i128>,
    pub reasoning_tokens: Option<i128>,
}

pub fn to_safe_token_count(value: i128) -> u64 {
    if value <= 0 { 0 } else { (value as u128).min(MAX_SAFE_TOKEN_COUNT as u128) as u64 }
}

pub fn safe_token_count_from_f64(value: f64) -> u64 {
    if !value.is_finite() || value <= 0.0 { 0 } else { value.floor().min(MAX_SAFE_TOKEN_COUNT as f64) as u64 }
}

pub fn turn_usage_from_turn_ended(fields: &TurnEndedUsage) -> Option<TurnUsage> {
    if fields.input_tokens.is_none() && fields.output_tokens.is_none() && fields.cache_read_tokens.is_none() && fields.cache_write_tokens.is_none() && fields.reasoning_tokens.is_none() { return None; }
    Some(TurnUsage {
        input_tokens: to_safe_token_count(fields.input_tokens.unwrap_or(0)),
        output_tokens: to_safe_token_count(fields.output_tokens.unwrap_or(0)),
        cache_read_tokens: to_safe_token_count(fields.cache_read_tokens.unwrap_or(0)),
        cache_write_tokens: to_safe_token_count(fields.cache_write_tokens.unwrap_or(0)),
        reasoning_tokens: fields.reasoning_tokens.map(to_safe_token_count),
    })
}

pub fn total_input_tokens(usage: &TurnUsage) -> u64 { usage.input_tokens }
pub fn add_token_counts(a: u64, b: u64) -> u64 { a.saturating_add(b).min(MAX_SAFE_TOKEN_COUNT) }

pub fn merge_turn_usage(a: Option<TurnUsage>, b: Option<TurnUsage>) -> Option<TurnUsage> {
    match (a, b) {
        (None, value) | (value, None) => value,
        (Some(a), Some(b)) => Some(TurnUsage {
            input_tokens: add_token_counts(a.input_tokens, b.input_tokens),
            output_tokens: add_token_counts(a.output_tokens, b.output_tokens),
            cache_read_tokens: add_token_counts(a.cache_read_tokens, b.cache_read_tokens),
            cache_write_tokens: add_token_counts(a.cache_write_tokens, b.cache_write_tokens),
            reasoning_tokens: match (a.reasoning_tokens, b.reasoning_tokens) { (None, None) => None, (left, right) => Some(add_token_counts(left.unwrap_or(0), right.unwrap_or(0))) },
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clamps_to_desktop_safe_integer_contract() {
        assert_eq!(to_safe_token_count(-1), 0);
        assert_eq!(to_safe_token_count(MAX_SAFE_TOKEN_COUNT as i128 + 100), MAX_SAFE_TOKEN_COUNT);
        assert_eq!(safe_token_count_from_f64(f64::NAN), 0);
        assert_eq!(safe_token_count_from_f64((MAX_SAFE_TOKEN_COUNT as f64) * 2.0), MAX_SAFE_TOKEN_COUNT);
    }
    #[test]
    fn merge_saturates_and_preserves_reasoning_presence() {
        let a=TurnUsage{input_tokens:MAX_SAFE_TOKEN_COUNT,output_tokens:2,cache_read_tokens:3,cache_write_tokens:4,reasoning_tokens:None};
        let b=TurnUsage{input_tokens:10,output_tokens:5,cache_read_tokens:6,cache_write_tokens:7,reasoning_tokens:Some(8)};
        let m=merge_turn_usage(Some(a),Some(b)).unwrap();
        assert_eq!(m.input_tokens,MAX_SAFE_TOKEN_COUNT);
        assert_eq!(m.output_tokens,7);
        assert_eq!(m.reasoning_tokens,Some(8));
    }
}
