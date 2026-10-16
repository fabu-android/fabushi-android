#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinishReason { Stop, Length, ToolCall, Cancelled, Error, Unknown }

pub fn normalize_finish_reason(value: &str) -> FinishReason {
    match value {
        "stop" | "completed" => FinishReason::Stop,
        "length" | "max_tokens" => FinishReason::Length,
        "tool_call" | "tool_calls" => FinishReason::ToolCall,
        "cancelled" | "canceled" => FinishReason::Cancelled,
        "error" | "failed" => FinishReason::Error,
        _ => FinishReason::Unknown,
    }
}
