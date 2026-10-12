pub const DSV3_TOOL_TOKENS_TO_STRIP: &[&str] = &[
    "<｜tool▁calls▁begin｜>",
    "<｜tool▁calls▁end｜>",
    "<｜tool▁call▁begin｜>",
    "<｜tool▁call▁end｜>",
    "<｜tool▁outputs▁begin｜>",
    "<｜tool▁outputs▁end｜>",
    "<｜tool▁output▁begin｜>",
    "<｜tool▁output▁end｜>",
    "<｜tool▁sep｜>",
    "<|redacted_tool_calls_begin|>",
    "<|redacted_tool_calls_end|>",
    "<|redacted_tool_call_begin|>",
    "<|redacted_tool_call_end|>",
    "<|redacted_tool_outputs_begin|>",
    "<|redacted_tool_outputs_end|>",
    "<|redacted_tool_output_begin|>",
    "<|redacted_tool_output_end|>",
    "<|redacted_tool_sep|>",
];

pub fn is_cursor_big_model(model_name: Option<&str>) -> bool {
    let Some(model_name) = model_name.filter(|value| !value.is_empty()) else {
        return false;
    };
    let lower = model_name.to_ascii_lowercase();
    [
        "cursor-big",
        "dsv3",
        "kimi2p5-uninitialized",
        "kimi-k2p5-rl-",
        "kimi-k2p5-agent-",
        "titanium-0318",
        "composer",
        "genericbase",
    ]
    .iter()
    .any(|part| lower.contains(part))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn big_model_detection_is_case_insensitive() {
        assert!(is_cursor_big_model(Some("Composer-1")));
        assert!(is_cursor_big_model(Some("foo-DSV3-bar")));
        assert!(!is_cursor_big_model(Some("gpt-5")));
        assert!(!is_cursor_big_model(None));
        assert!(DSV3_TOOL_TOKENS_TO_STRIP.len() >= 18);
    }
}
