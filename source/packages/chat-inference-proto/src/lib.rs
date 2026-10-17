pub mod client;
pub mod converters;
#[allow(non_snake_case)]
#[path = "cursorModelProviderOptions.rs"]
pub mod cursor_model_provider_options;
pub mod index;

#[cfg(test)]
mod tests {
    use super::{
        client::{InferenceRequest, InferenceTransport},
        converters::{normalize_finish_reason, FinishReason},
        cursor_model_provider_options::CursorModelProviderOptions,
    };

    struct Echo;
    impl InferenceTransport for Echo {
        fn send(&mut self, request: &InferenceRequest) -> Result<String, String> { Ok(request.prompt.clone()) }
    }

    #[test]
    fn request_and_provider_options_are_validated() {
        let request = InferenceRequest { request_id: "r".into(), model: "m".into(), prompt: "hello".into() };
        assert!(request.validate().is_ok());
        assert_eq!(Echo.send(&request).unwrap(), "hello");
        assert!(CursorModelProviderOptions { model: "".into(), max_output_tokens: 1 }.validate().is_err());
        assert_eq!(normalize_finish_reason("tool_calls"), FinishReason::ToolCall);
    }
}
