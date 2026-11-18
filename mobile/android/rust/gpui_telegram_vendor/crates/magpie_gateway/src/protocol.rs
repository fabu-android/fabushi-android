use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

/// Canonical message role used across Rosetta wire adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnifiedRole {
    System,
    User,
    Assistant,
    Tool,
}

/// Canonical tool schema definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnifiedToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// Canonical model-emitted function/tool invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnifiedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// Canonical chat message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnifiedMessage {
    pub role: UnifiedRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<UnifiedToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
}

/// Canonical token usage summary.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnifiedUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub total_tokens: u64,
}

/// Canonical inbound/outbound request IR for Rosetta protocol conversion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnifiedRequest {
    pub model: String,
    #[serde(default)]
    pub system_prompt: String,
    #[serde(default)]
    pub messages: Vec<UnifiedMessage>,
    #[serde(default)]
    pub tools: Vec<UnifiedToolSpec>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub thinking_budget_tokens: Option<usize>,
    #[serde(default)]
    pub stream: bool,
}

/// Canonical parsed response from any upstream wire protocol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnifiedResponse {
    pub provider_id: String,
    pub model: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<UnifiedToolCall>,
    pub finish_reason: String,
    pub usage: UnifiedUsage,
}

/// Incremental SSE event extracted from a Gemini `streamGenerateContent?alt=sse` payload.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GeminiSseDelta {
    pub text_delta: String,
    pub thinking_delta: String,
    pub tool_calls: Vec<UnifiedToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<UnifiedUsage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProtocolError {
    #[error("Invalid protocol payload JSON: {0}")]
    InvalidJson(String),
    #[error("Upstream protocol error: {0}")]
    Upstream(String),
    #[error("Missing required field: {0}")]
    MissingField(String),
}

/// Normalize an Anthropic Messages base URL into `{base}/v1/messages` or `{base}/messages`.
pub fn resolve_anthropic_messages_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/v1/messages") || trimmed.ends_with("/messages") {
        return trimmed.to_string();
    }
    if trimmed.ends_with("/v1") {
        return format!("{trimmed}/messages");
    }
    format!("{trimmed}/v1/messages")
}

/// Normalize a Gemini API base URL and resolve `{base}/models/{model}:(stream)generateContent`.
pub fn resolve_gemini_endpoint_url(base_url: &str, model: &str, stream: bool) -> String {
    let mut base = base_url.trim().trim_end_matches('/').to_string();
    if !base.is_empty() && !base.contains("://") {
        base = format!("https://{base}");
    }
    if let Some(stripped) = base.strip_suffix("/openai") {
        base = stripped.trim_end_matches('/').to_string();
    }
    if let Some(idx) = base.find("/models") {
        base = base[..idx].trim_end_matches('/').to_string();
    }
    // If bare host with empty path (e.g. https://generativelanguage.googleapis.com or bare proxy host), add /v1beta.
    if let Some(scheme_idx) = base.find("://") {
        let after_scheme = &base[scheme_idx + 3..];
        if !after_scheme.contains('/') {
            base.push_str("/v1beta");
        }
    }
    let clean_model = model.trim().strip_prefix("models/").unwrap_or(model.trim());
    if stream {
        format!("{base}/models/{clean_model}:streamGenerateContent?alt=sse")
    } else {
        format!("{base}/models/{clean_model}:generateContent")
    }
}

/// Build a native Google Gemini `generateContent` JSON request body from a `UnifiedRequest`.
pub fn build_gemini_generate_content_payload(req: &UnifiedRequest) -> Value {
    let mut system_parts = Vec::new();
    if !req.system_prompt.trim().is_empty() {
        system_parts.push(json!({ "text": req.system_prompt.trim() }));
    }

    let mut contents = Vec::new();
    for msg in &req.messages {
        match msg.role {
            UnifiedRole::System => {
                if !msg.content.trim().is_empty() {
                    system_parts.push(json!({ "text": msg.content.trim() }));
                }
            }
            UnifiedRole::User => {
                contents.push(json!({
                    "role": "user",
                    "parts": [{ "text": msg.content }],
                }));
            }
            UnifiedRole::Assistant => {
                let mut parts = Vec::new();
                if !msg.content.is_empty() {
                    parts.push(json!({ "text": msg.content }));
                }
                for tc in &msg.tool_calls {
                    let args_val: Value =
                        serde_json::from_str(&tc.arguments).unwrap_or_else(|_| json!({}));
                    parts.push(json!({
                        "functionCall": {
                            "name": tc.name,
                            "args": args_val,
                        }
                    }));
                }
                if !parts.is_empty() {
                    contents.push(json!({
                        "role": "model",
                        "parts": parts,
                    }));
                }
            }
            UnifiedRole::Tool => {
                let fn_name = msg
                    .name
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("tool_result");
                let response_val: Value = serde_json::from_str(&msg.content)
                    .unwrap_or_else(|_| json!({ "result": msg.content }));
                let response_obj = if response_val.is_object() {
                    response_val
                } else {
                    json!({ "result": response_val })
                };
                contents.push(json!({
                    "role": "user",
                    "parts": [{
                        "functionResponse": {
                            "name": fn_name,
                            "response": response_obj,
                        }
                    }]
                }));
            }
        }
    }

    let mut payload = json!({
        "contents": contents,
    });

    if !system_parts.is_empty() {
        payload["systemInstruction"] = json!({ "parts": system_parts });
    }

    if !req.tools.is_empty() {
        let decls: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                })
            })
            .collect();
        payload["tools"] = json!([{ "functionDeclarations": decls }]);
    }

    let mut gen_cfg = serde_json::Map::new();
    if let Some(temp) = req.temperature {
        gen_cfg.insert("temperature".to_string(), json!(temp));
    }
    if let Some(max_tok) = req.max_tokens {
        gen_cfg.insert("maxOutputTokens".to_string(), json!(max_tok));
    }
    if let Some(budget) = req.thinking_budget_tokens {
        if budget > 0 {
            gen_cfg.insert(
                "thinkingConfig".to_string(),
                json!({
                    "thinkingBudget": budget,
                    "includeThoughts": true,
                }),
            );
        }
    }
    if !gen_cfg.is_empty() {
        payload["generationConfig"] = Value::Object(gen_cfg);
    }

    payload
}

fn extract_gemini_error(val: &Value) -> Option<String> {
    let err = val.get("error").filter(|e| !e.is_null())?;
    err.get("message")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| err.as_str().map(ToString::to_string))
        .or_else(|| Some(err.to_string()))
}

fn parse_gemini_usage(val: &Value) -> Option<UnifiedUsage> {
    let u = val
        .get("usageMetadata")
        .or_else(|| val.get("usage_metadata"))?;
    let input_tokens = u
        .get("promptTokenCount")
        .or_else(|| u.get("prompt_token_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output_tokens = u
        .get("candidatesTokenCount")
        .or_else(|| u.get("candidates_token_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let cache_read_tokens = u
        .get("cachedContentTokenCount")
        .or_else(|| u.get("cached_content_token_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let total_tokens = u
        .get("totalTokenCount")
        .or_else(|| u.get("total_token_count"))
        .and_then(Value::as_u64)
        .unwrap_or(input_tokens.saturating_add(output_tokens));
    Some(UnifiedUsage {
        input_tokens,
        output_tokens,
        cache_read_tokens,
        total_tokens,
    })
}

fn extract_gemini_parts(val: &Value, call_counter: usize) -> (String, String, Vec<UnifiedToolCall>, Option<String>) {
    let mut text = String::new();
    let mut thinking = String::new();
    let mut tool_calls = Vec::new();

    let candidate = val
        .get("candidates")
        .and_then(Value::as_array)
        .and_then(|c| c.first());
    let finish_reason = candidate
        .and_then(|c| c.get("finishReason").or_else(|| c.get("finish_reason")))
        .and_then(Value::as_str)
        .map(|s| match s {
            "STOP" => "stop".to_string(),
            "MAX_TOKENS" => "length".to_string(),
            "SAFETY" | "RECITATION" => "content_filter".to_string(),
            other => other.to_ascii_lowercase(),
        });

    if let Some(parts) = candidate
        .and_then(|c| c.get("content"))
        .and_then(|c| c.get("parts"))
        .and_then(Value::as_array)
    {
        for (idx, part) in parts.iter().enumerate() {
            let is_thought = part
                .get("thought")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if let Some(t) = part.get("text").and_then(Value::as_str) {
                if is_thought {
                    thinking.push_str(t);
                } else {
                    text.push_str(t);
                }
            }
            if let Some(fc) = part.get("functionCall").or_else(|| part.get("function_call")) {
                let name = fc
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let args = fc
                    .get("args")
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "{}".to_string());
                let id = fc
                    .get("id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string)
                    .unwrap_or_else(|| format!("call_gemini_{}", call_counter + idx + 1));
                tool_calls.push(UnifiedToolCall {
                    id,
                    name,
                    arguments: args,
                });
            }
        }
    }

    (text, thinking, tool_calls, finish_reason)
}

/// Parse a non-streaming Gemini `generateContent` JSON response into `UnifiedResponse`.
pub fn parse_gemini_response_json(
    provider_id: &str,
    model: &str,
    body: &str,
) -> Result<UnifiedResponse, ProtocolError> {
    let val: Value = serde_json::from_str(body)
        .map_err(|e| ProtocolError::InvalidJson(format!("Gemini response JSON: {e}")))?;
    if let Some(msg) = extract_gemini_error(&val) {
        return Err(ProtocolError::Upstream(msg));
    }
    let (content, thinking, tool_calls, finish_opt) = extract_gemini_parts(&val, 0);
    let usage = parse_gemini_usage(&val).unwrap_or_default();
    let finish_reason = if !tool_calls.is_empty() {
        "tool_calls".to_string()
    } else {
        finish_opt.unwrap_or_else(|| "stop".to_string())
    };
    Ok(UnifiedResponse {
        provider_id: provider_id.to_string(),
        model: model.to_string(),
        content,
        reasoning: (!thinking.is_empty()).then_some(thinking),
        tool_calls,
        finish_reason,
        usage,
    })
}

/// Parse a single SSE line/data payload from Gemini `streamGenerateContent?alt=sse`.
pub fn parse_gemini_sse_data(
    line_or_json: &str,
    call_counter: usize,
) -> Result<Option<GeminiSseDelta>, ProtocolError> {
    let trimmed = line_or_json.trim();
    if trimmed.is_empty()
        || trimmed == "[DONE]"
        || trimmed.starts_with(':')
        || trimmed.starts_with("event:")
        || trimmed.starts_with("id:")
        || trimmed.starts_with("retry:")
    {
        return Ok(None);
    }
    let data = trimmed.strip_prefix("data:").map(str::trim).unwrap_or(trimmed);
    if data.is_empty()
        || data == "[DONE]"
        || data.starts_with("event:")
        || data.starts_with(':')
        || data.starts_with("id:")
        || data.starts_with("retry:")
    {
        return Ok(None);
    }
    let val: Value = serde_json::from_str(data)
        .map_err(|e| ProtocolError::InvalidJson(format!("Gemini SSE JSON: {e}")))?;
    if let Some(msg) = extract_gemini_error(&val) {
        return Err(ProtocolError::Upstream(msg));
    }
    let (text_delta, thinking_delta, tool_calls, finish_reason) =
        extract_gemini_parts(&val, call_counter);
    let usage = parse_gemini_usage(&val);
    Ok(Some(GeminiSseDelta {
        text_delta,
        thinking_delta,
        tool_calls,
        finish_reason,
        usage,
    }))
}

fn extract_text_from_content_value(val: Option<&Value>) -> String {
    let Some(v) = val else {
        return String::new();
    };
    if let Some(s) = v.as_str() {
        return s.to_string();
    }
    if let Some(arr) = v.as_array() {
        return arr
            .iter()
            .filter_map(|part| {
                part.get("text")
                    .and_then(Value::as_str)
                    .or_else(|| part.get("input_text").and_then(Value::as_str))
                    .or_else(|| part.as_str())
            })
            .collect::<Vec<_>>()
            .join("")
    }
    String::new()
}

fn push_unique_reasoning_part(parts: &mut Vec<String>, candidate: &str) {
    if !candidate.trim().is_empty() && !parts.iter().any(|existing| existing == candidate) {
        parts.push(candidate.to_string());
    }
}

fn extract_message_reasoning_parts(msg: &Value) -> Vec<String> {
    let mut parts = Vec::new();
    for key in ["reasoning_content", "reasoning", "thought"] {
        if let Some(s) = msg.get(key).and_then(Value::as_str) {
            push_unique_reasoning_part(&mut parts, s);
        }
    }
    if let Some(arr) = msg.get("content").and_then(Value::as_array) {
        for part in arr {
            let ptype = part.get("type").and_then(Value::as_str).unwrap_or("");
            if matches!(
                ptype,
                "thinking" | "reasoning" | "reasoning_text" | "summary_text"
            ) {
                for key in ["thinking", "text", "reasoning", "reasoning_content", "thought"] {
                    if let Some(s) = part.get(key).and_then(Value::as_str) {
                        push_unique_reasoning_part(&mut parts, s);
                    }
                }
            }
        }
    }
    parts
}

fn extract_message_reasoning(msg: &Value) -> Option<String> {
    let parts = extract_message_reasoning_parts(msg);
    (!parts.is_empty()).then(|| parts.join("\n"))
}

/// Inbound OpenAI `/v1/chat/completions` JSON request -> `UnifiedRequest`.
pub fn inbound_chat_completions_to_unified(body: &Value) -> Result<UnifiedRequest, ProtocolError> {
    let model = body
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ProtocolError::MissingField("model".to_string()))?
        .to_string();

    let mut system_parts = Vec::new();
    let mut messages = Vec::new();
    if let Some(arr) = body.get("messages").and_then(Value::as_array) {
        for msg in arr {
            let role_str = msg.get("role").and_then(Value::as_str).unwrap_or("user");
            let text = extract_text_from_content_value(msg.get("content"));
            match role_str {
                "system" | "developer" => {
                    if !text.is_empty() {
                        system_parts.push(text);
                    }
                }
                "assistant" => {
                    let mut tool_calls = Vec::new();
                    if let Some(tcs) = msg.get("tool_calls").and_then(Value::as_array) {
                        for (idx, tc) in tcs.iter().enumerate() {
                            let id = tc
                                .get("id")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            let name = tc
                                .get("function")
                                .and_then(|f| f.get("name"))
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            let arguments = tc
                                .get("function")
                                .and_then(|f| f.get("arguments"))
                                .and_then(Value::as_str)
                                .unwrap_or("{}")
                                .to_string();
                            tool_calls.push(UnifiedToolCall {
                                id: if id.is_empty() { format!("call_{idx}") } else { id },
                                name,
                                arguments,
                            });
                        }
                    }
                    messages.push(UnifiedMessage {
                        role: UnifiedRole::Assistant,
                        content: text,
                        name: None,
                        tool_call_id: None,
                        tool_calls,
                        reasoning: extract_message_reasoning(msg),
                    });
                }
                "tool" => {
                    messages.push(UnifiedMessage {
                        role: UnifiedRole::Tool,
                        content: text,
                        name: msg.get("name").and_then(Value::as_str).map(ToString::to_string),
                        tool_call_id: msg
                            .get("tool_call_id")
                            .and_then(Value::as_str)
                            .map(ToString::to_string),
                        tool_calls: Vec::new(),
                        reasoning: extract_message_reasoning(msg),
                    });
                }
                _ => {
                    messages.push(UnifiedMessage {
                        role: UnifiedRole::User,
                        content: text,
                        name: None,
                        tool_call_id: None,
                        tool_calls: Vec::new(),
                        reasoning: extract_message_reasoning(msg),
                    });
                }
            }
        }
    }

    let tools = body
        .get("tools")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|t| {
                    let f = t.get("function").unwrap_or(t);
                    let name = f.get("name").and_then(Value::as_str)?.to_string();
                    Some(UnifiedToolSpec {
                        name,
                        description: f
                            .get("description")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        parameters: f.get("parameters").cloned().unwrap_or_else(|| json!({})),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(UnifiedRequest {
        model,
        system_prompt: system_parts.join("\n\n"),
        messages,
        tools,
        temperature: body.get("temperature").and_then(Value::as_f64).map(|f| f as f32),
        max_tokens: body
            .get("max_tokens")
            .or_else(|| body.get("max_completion_tokens"))
            .and_then(Value::as_u64)
            .map(|n| n as u32),
        thinking_budget_tokens: None,
        stream: body.get("stream").and_then(Value::as_bool).unwrap_or(false),
    })
}

/// Inbound OpenAI `/v1/responses` JSON request -> `UnifiedRequest`.
pub fn inbound_responses_to_unified(body: &Value) -> Result<UnifiedRequest, ProtocolError> {
    let model = body
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ProtocolError::MissingField("model".to_string()))?
        .to_string();

    let system_prompt = body
        .get("instructions")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let mut messages = Vec::new();
    if let Some(input_str) = body.get("input").and_then(Value::as_str) {
        messages.push(UnifiedMessage {
            role: UnifiedRole::User,
            content: input_str.to_string(),
            name: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            reasoning: None,
        });
    } else if let Some(items) = body.get("input").and_then(Value::as_array) {
        for item in items {
            let itype = item.get("type").and_then(Value::as_str).unwrap_or("message");
            match itype {
                "function_call" => {
                    messages.push(UnifiedMessage {
                        role: UnifiedRole::Assistant,
                        content: String::new(),
                        name: None,
                        tool_call_id: None,
                        tool_calls: vec![UnifiedToolCall {
                            id: item
                                .get("call_id")
                                .and_then(Value::as_str)
                                .unwrap_or("call_1")
                                .to_string(),
                            name: item
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            arguments: item
                                .get("arguments")
                                .and_then(Value::as_str)
                                .unwrap_or("{}")
                                .to_string(),
                        }],
                        reasoning: None,
                    });
                }
                "function_call_output" => {
                    messages.push(UnifiedMessage {
                        role: UnifiedRole::Tool,
                        content: item
                            .get("output")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        name: None,
                        tool_call_id: item
                            .get("call_id")
                            .and_then(Value::as_str)
                            .map(ToString::to_string),
                        tool_calls: Vec::new(),
                        reasoning: None,
                    });
                }
                "reasoning" => {
                    let mut parts = Vec::new();

                    if let Some(summary) = item.get("summary") {
                        if let Some(s) = summary.as_str() {
                            push_unique_reasoning_part(&mut parts, s);
                        } else if let Some(arr) = summary.as_array() {
                            for entry in arr {
                                if let Some(s) = entry.as_str() {
                                    push_unique_reasoning_part(&mut parts, s);
                                } else {
                                    for key in ["text", "thinking", "reasoning", "thought"] {
                                        if let Some(s) = entry.get(key).and_then(Value::as_str) {
                                            push_unique_reasoning_part(&mut parts, s);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if let Some(content) = item.get("content") {
                        if let Some(s) = content.as_str() {
                            push_unique_reasoning_part(&mut parts, s);
                        } else if let Some(arr) = content.as_array() {
                            for entry in arr {
                                if let Some(s) = entry.as_str() {
                                    push_unique_reasoning_part(&mut parts, s);
                                } else {
                                    for key in [
                                        "text",
                                        "input_text",
                                        "output_text",
                                        "thinking",
                                        "reasoning",
                                        "reasoning_content",
                                        "thought",
                                        "reasoning_text",
                                    ] {
                                        if let Some(s) = entry.get(key).and_then(Value::as_str) {
                                            push_unique_reasoning_part(&mut parts, s);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if let Some(s) = item.get("text").and_then(Value::as_str) {
                        push_unique_reasoning_part(&mut parts, s);
                    }

                    for extra in extract_message_reasoning_parts(item) {
                        push_unique_reasoning_part(&mut parts, &extra);
                    }

                    if !parts.is_empty() {
                        messages.push(UnifiedMessage {
                            role: UnifiedRole::Assistant,
                            content: String::new(),
                            name: None,
                            tool_call_id: None,
                            tool_calls: Vec::new(),
                            reasoning: Some(parts.join("\n")),
                        });
                    }
                }
                _ => {
                    let role = match item.get("role").and_then(Value::as_str).unwrap_or("user") {
                        "developer" | "system" => UnifiedRole::System,
                        "assistant" => UnifiedRole::Assistant,
                        _ => UnifiedRole::User,
                    };
                    messages.push(UnifiedMessage {
                        role,
                        content: extract_text_from_content_value(item.get("content")),
                        name: None,
                        tool_call_id: None,
                        tool_calls: Vec::new(),
                        reasoning: extract_message_reasoning(item),
                    });
                }
            }
        }
    }

    let tools = body
        .get("tools")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|t| {
                    let name = t.get("name").and_then(Value::as_str)?.to_string();
                    Some(UnifiedToolSpec {
                        name,
                        description: t
                            .get("description")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        parameters: t.get("parameters").cloned().unwrap_or_else(|| json!({})),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(UnifiedRequest {
        model,
        system_prompt,
        messages,
        tools,
        temperature: body.get("temperature").and_then(Value::as_f64).map(|f| f as f32),
        max_tokens: body
            .get("max_output_tokens")
            .and_then(Value::as_u64)
            .map(|n| n as u32),
        thinking_budget_tokens: None,
        stream: body.get("stream").and_then(Value::as_bool).unwrap_or(false),
    })
}

/// Inbound Anthropic `/v1/messages` JSON request -> `UnifiedRequest`.
pub fn inbound_anthropic_to_unified(body: &Value) -> Result<UnifiedRequest, ProtocolError> {
    let model = body
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ProtocolError::MissingField("model".to_string()))?
        .to_string();

    let system_prompt = extract_text_from_content_value(body.get("system"));
    let mut messages = Vec::new();
    if let Some(arr) = body.get("messages").and_then(Value::as_array) {
        for msg in arr {
            let role_str = msg.get("role").and_then(Value::as_str).unwrap_or("user");
            if let Some(blocks) = msg.get("content").and_then(Value::as_array) {
                let mut text_parts = Vec::new();
                let mut tool_calls = Vec::new();
                let mut reasoning_parts = Vec::new();
                for block in blocks {
                    match block.get("type").and_then(Value::as_str).unwrap_or("") {
                        "text" => {
                            if let Some(t) = block.get("text").and_then(Value::as_str) {
                                text_parts.push(t.to_string());
                            }
                        }
                        "thinking" | "reasoning" => {
                            for key in ["thinking", "text", "reasoning", "reasoning_content", "thought"] {
                                if let Some(t) = block.get(key).and_then(Value::as_str) {
                                    push_unique_reasoning_part(&mut reasoning_parts, t);
                                }
                            }
                        }
                        "tool_use" => {
                            tool_calls.push(UnifiedToolCall {
                                id: block
                                    .get("id")
                                    .and_then(Value::as_str)
                                    .unwrap_or("toolu_1")
                                    .to_string(),
                                name: block
                                    .get("name")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string(),
                                arguments: block
                                    .get("input")
                                    .map(|v| v.to_string())
                                    .unwrap_or_else(|| "{}".to_string()),
                            });
                        }
                        "tool_result" => {
                            messages.push(UnifiedMessage {
                                role: UnifiedRole::Tool,
                                content: extract_text_from_content_value(block.get("content")),
                                name: None,
                                tool_call_id: block
                                    .get("tool_use_id")
                                    .and_then(Value::as_str)
                                    .map(ToString::to_string),
                                tool_calls: Vec::new(),
                                reasoning: None,
                            });
                        }
                        _ => {}
                    }
                }
                for key in ["reasoning_content", "reasoning", "thought"] {
                    if let Some(s) = msg.get(key).and_then(Value::as_str) {
                        push_unique_reasoning_part(&mut reasoning_parts, s);
                    }
                }
                if !text_parts.is_empty() || !tool_calls.is_empty() || !reasoning_parts.is_empty() {
                    messages.push(UnifiedMessage {
                        role: if role_str == "assistant" {
                            UnifiedRole::Assistant
                        } else {
                            UnifiedRole::User
                        },
                        content: text_parts.join("\n"),
                        name: None,
                        tool_call_id: None,
                        tool_calls,
                        reasoning: (!reasoning_parts.is_empty())
                            .then(|| reasoning_parts.join("\n")),
                    });
                }
            } else {
                let text = extract_text_from_content_value(msg.get("content"));
                messages.push(UnifiedMessage {
                    role: if role_str == "assistant" {
                        UnifiedRole::Assistant
                    } else {
                        UnifiedRole::User
                    },
                    content: text,
                    name: None,
                    tool_call_id: None,
                    tool_calls: Vec::new(),
                    reasoning: extract_message_reasoning(msg),
                });
            }
        }
    }

    let tools = body
        .get("tools")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|t| {
                    let name = t.get("name").and_then(Value::as_str)?.to_string();
                    Some(UnifiedToolSpec {
                        name,
                        description: t
                            .get("description")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        parameters: t
                            .get("input_schema")
                            .or_else(|| t.get("parameters"))
                            .cloned()
                            .unwrap_or_else(|| json!({})),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let thinking_budget_tokens = body
        .get("thinking")
        .and_then(|t| t.get("budget_tokens"))
        .and_then(Value::as_u64)
        .map(|n| n as usize);

    Ok(UnifiedRequest {
        model,
        system_prompt,
        messages,
        tools,
        temperature: body.get("temperature").and_then(Value::as_f64).map(|f| f as f32),
        max_tokens: body
            .get("max_tokens")
            .and_then(Value::as_u64)
            .map(|n| n as u32),
        thinking_budget_tokens,
        stream: body.get("stream").and_then(Value::as_bool).unwrap_or(false),
    })
}

/// Outbound `UnifiedResponse` -> OpenAI `/v1/chat/completions` JSON.
pub fn unified_to_chat_completions_json(resp: &UnifiedResponse) -> Value {
    let mut msg = json!({
        "role": "assistant",
        "content": resp.content,
    });
    if let Some(ref reason) = resp.reasoning {
        msg["reasoning_content"] = Value::String(reason.clone());
    }
    if !resp.tool_calls.is_empty() {
        let tcs: Vec<Value> = resp
            .tool_calls
            .iter()
            .map(|tc| {
                json!({
                    "id": tc.id,
                    "type": "function",
                    "function": {
                        "name": tc.name,
                        "arguments": tc.arguments,
                    }
                })
            })
            .collect();
        msg["tool_calls"] = Value::Array(tcs);
    }
    json!({
        "id": format!("chatcmpl-magpie-{}", resp.provider_id),
        "object": "chat.completion",
        "model": resp.model,
        "choices": [{
            "index": 0,
            "message": msg,
            "finish_reason": resp.finish_reason,
        }],
        "usage": {
            "prompt_tokens": resp.usage.input_tokens,
            "completion_tokens": resp.usage.output_tokens,
            "total_tokens": resp.usage.total_tokens,
            "prompt_tokens_details": {
                "cached_tokens": resp.usage.cache_read_tokens,
            }
        }
    })
}

/// Outbound `UnifiedResponse` -> OpenAI `/v1/responses` JSON.
pub fn unified_to_responses_json(resp: &UnifiedResponse) -> Value {
    let mut output = Vec::new();
    if let Some(ref reason) = resp.reasoning {
        output.push(json!({
            "type": "reasoning",
            "summary": [{ "type": "summary_text", "text": reason }],
        }));
    }
    if !resp.content.is_empty() || resp.tool_calls.is_empty() {
        output.push(json!({
            "type": "message",
            "role": "assistant",
            "content": [{ "type": "output_text", "text": resp.content }],
        }));
    }
    for tc in &resp.tool_calls {
        output.push(json!({
            "type": "function_call",
            "call_id": tc.id,
            "name": tc.name,
            "arguments": tc.arguments,
        }));
    }
    json!({
        "id": format!("resp_magpie_{}", resp.provider_id),
        "object": "response",
        "status": "completed",
        "model": resp.model,
        "output_text": resp.content,
        "output": output,
        "usage": {
            "input_tokens": resp.usage.input_tokens,
            "output_tokens": resp.usage.output_tokens,
            "total_tokens": resp.usage.total_tokens,
            "input_tokens_details": {
                "cached_tokens": resp.usage.cache_read_tokens,
            }
        }
    })
}

/// Outbound `UnifiedResponse` -> Anthropic `/v1/messages` JSON.
pub fn unified_to_anthropic_json(resp: &UnifiedResponse) -> Value {
    let mut blocks = Vec::new();
    if let Some(ref reason) = resp.reasoning {
        blocks.push(json!({
            "type": "thinking",
            "thinking": reason,
        }));
    }
    if !resp.content.is_empty() || resp.tool_calls.is_empty() {
        blocks.push(json!({
            "type": "text",
            "text": resp.content,
        }));
    }
    for tc in &resp.tool_calls {
        let input_val: Value = serde_json::from_str(&tc.arguments).unwrap_or_else(|_| json!({}));
        blocks.push(json!({
            "type": "tool_use",
            "id": tc.id,
            "name": tc.name,
            "input": input_val,
        }));
    }
    json!({
        "id": format!("msg_magpie_{}", resp.provider_id),
        "type": "message",
        "role": "assistant",
        "model": resp.model,
        "content": blocks,
        "stop_reason": if resp.tool_calls.is_empty() { "end_turn" } else { "tool_use" },
        "usage": {
            "input_tokens": resp.usage.input_tokens,
            "output_tokens": resp.usage.output_tokens,
            "cache_read_input_tokens": resp.usage.cache_read_tokens,
        }
    })
}

/// Unified streaming SSE chunk exchanged across providers and clients.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UnifiedStreamChunk {
    pub text_delta: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_delta: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<UnifiedToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<UnifiedUsage>,
}

/// Outbound `UnifiedRequest` -> OpenAI `/v1/chat/completions` request JSON.
pub fn unified_to_chat_completions_request_json(req: &UnifiedRequest) -> Value {
    let mut messages = Vec::new();
    if !req.system_prompt.trim().is_empty() {
        messages.push(json!({
            "role": "system",
            "content": req.system_prompt.trim(),
        }));
    }
    for msg in &req.messages {
        let role_str = match msg.role {
            UnifiedRole::System => "system",
            UnifiedRole::User => "user",
            UnifiedRole::Assistant => "assistant",
            UnifiedRole::Tool => "tool",
        };
        let mut obj = json!({
            "role": role_str,
            "content": msg.content,
        });
        if let Some(ref name) = msg.name {
            obj["name"] = Value::String(name.clone());
        }
        if let Some(ref tid) = msg.tool_call_id {
            obj["tool_call_id"] = Value::String(tid.clone());
        }
        if !msg.tool_calls.is_empty() {
            let tcs: Vec<Value> = msg
                .tool_calls
                .iter()
                .map(|tc| {
                    json!({
                        "id": tc.id,
                        "type": "function",
                        "function": {
                            "name": tc.name,
                            "arguments": tc.arguments,
                        }
                    })
                })
                .collect();
            obj["tool_calls"] = Value::Array(tcs);
        }
        messages.push(obj);
    }

    let mut body = json!({
        "model": req.model,
        "messages": messages,
        "stream": req.stream,
    });
    if let Some(temp) = req.temperature {
        body["temperature"] = json!(temp);
    }
    if let Some(max_tok) = req.max_tokens {
        body["max_tokens"] = json!(max_tok);
    }
    if !req.tools.is_empty() {
        let tools_val: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            })
            .collect();
        body["tools"] = Value::Array(tools_val);
    }
    body
}

/// Outbound `UnifiedRequest` -> OpenAI `/v1/responses` request JSON.
pub fn unified_to_responses_request_json(req: &UnifiedRequest) -> Value {
    let mut input = Vec::new();
    for msg in &req.messages {
        match msg.role {
            UnifiedRole::Tool => {
                input.push(json!({
                    "type": "function_call_output",
                    "call_id": msg.tool_call_id.clone().unwrap_or_default(),
                    "output": msg.content,
                }));
            }
            UnifiedRole::Assistant if !msg.tool_calls.is_empty() => {
                for tc in &msg.tool_calls {
                    input.push(json!({
                        "type": "function_call",
                        "call_id": tc.id,
                        "name": tc.name,
                        "arguments": tc.arguments,
                    }));
                }
            }
            UnifiedRole::Assistant => {
                input.push(json!({
                    "type": "message",
                    "role": "assistant",
                    "content": msg.content,
                }));
            }
            UnifiedRole::System => {
                input.push(json!({
                    "type": "message",
                    "role": "developer",
                    "content": msg.content,
                }));
            }
            UnifiedRole::User => {
                input.push(json!({
                    "type": "message",
                    "role": "user",
                    "content": msg.content,
                }));
            }
        }
    }

    let mut body = json!({
        "model": req.model,
        "input": input,
        "stream": req.stream,
    });
    if !req.system_prompt.is_empty() {
        body["instructions"] = json!(req.system_prompt);
    }
    if let Some(temp) = req.temperature {
        body["temperature"] = json!(temp);
    }
    if let Some(max_tok) = req.max_tokens {
        body["max_output_tokens"] = json!(max_tok);
    }
    if !req.tools.is_empty() {
        let tools_val: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                })
            })
            .collect();
        body["tools"] = Value::Array(tools_val);
    }
    body
}

/// Outbound `UnifiedRequest` -> Anthropic `/v1/messages` request JSON.
pub fn unified_to_anthropic_request_json(req: &UnifiedRequest) -> Value {
    let mut messages = Vec::new();
    for msg in &req.messages {
        match msg.role {
            UnifiedRole::System => {}
            UnifiedRole::Tool => {
                messages.push(json!({
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": msg.tool_call_id.clone().unwrap_or_default(),
                        "content": msg.content,
                    }]
                }));
            }
            UnifiedRole::Assistant => {
                let mut blocks = Vec::new();
                if !msg.content.is_empty() {
                    blocks.push(json!({
                        "type": "text",
                        "text": msg.content,
                    }));
                }
                for tc in &msg.tool_calls {
                    let input_val: Value =
                        serde_json::from_str(&tc.arguments).unwrap_or_else(|_| json!({}));
                    blocks.push(json!({
                        "type": "tool_use",
                        "id": tc.id,
                        "name": tc.name,
                        "input": input_val,
                    }));
                }
                if blocks.is_empty() {
                    blocks.push(json!({
                        "type": "text",
                        "text": "",
                    }));
                }
                messages.push(json!({
                    "role": "assistant",
                    "content": blocks,
                }));
            }
            UnifiedRole::User => {
                messages.push(json!({
                    "role": "user",
                    "content": [{
                        "type": "text",
                        "text": msg.content,
                    }]
                }));
            }
        }
    }

    let max_tokens = req.max_tokens.unwrap_or(4096);
    let mut body = json!({
        "model": req.model,
        "messages": messages,
        "max_tokens": max_tokens,
        "stream": req.stream,
    });
    if !req.system_prompt.is_empty() {
        body["system"] = json!(req.system_prompt);
    }
    if let Some(temp) = req.temperature {
        body["temperature"] = json!(temp);
    }
    if !req.tools.is_empty() {
        let tools_val: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters,
                })
            })
            .collect();
        body["tools"] = Value::Array(tools_val);
    }
    if let Some(budget) = req.thinking_budget_tokens {
        if budget > 0 {
            body["thinking"] = json!({
                "type": "enabled",
                "budget_tokens": budget,
            });
        }
    }
    body
}

/// Parse an OpenAI `/v1/chat/completions` JSON response into `UnifiedResponse`.
pub fn parse_chat_completions_response_json(
    provider_id: &str,
    model: &str,
    body: &str,
) -> Result<UnifiedResponse, ProtocolError> {
    let val: Value = serde_json::from_str(body)
        .map_err(|e| ProtocolError::InvalidJson(format!("OpenAI response JSON: {e}")))?;
    if let Some(err) = val.get("error").filter(|e| !e.is_null()) {
        let msg = err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Unknown upstream OpenAI error");
        return Err(ProtocolError::Upstream(msg.to_string()));
    }
    let choice = val
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .ok_or_else(|| ProtocolError::InvalidJson("Missing choices in response".to_string()))?;

    let message = choice
        .get("message")
        .ok_or_else(|| ProtocolError::InvalidJson("Missing message in choice".to_string()))?;
    let content = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let reasoning = message
        .get("reasoning_content")
        .or_else(|| message.get("thought"))
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let finish_reason = choice
        .get("finish_reason")
        .and_then(Value::as_str)
        .unwrap_or("stop")
        .to_string();

    let mut tool_calls = Vec::new();
    if let Some(tcs) = message.get("tool_calls").and_then(Value::as_array) {
        for (idx, tc) in tcs.iter().enumerate() {
            let id = tc
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let name = tc
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let arguments = tc
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(Value::as_str)
                .unwrap_or("{}")
                .to_string();
            tool_calls.push(UnifiedToolCall {
                id: if id.is_empty() { format!("call_{idx}") } else { id },
                name,
                arguments,
            });
        }
    }

    let usage = if let Some(u) = val.get("usage") {
        let input_tokens = u.get("prompt_tokens").and_then(Value::as_u64).unwrap_or(0);
        let output_tokens = u.get("completion_tokens").and_then(Value::as_u64).unwrap_or(0);
        let cache_read_tokens = u
            .get("prompt_tokens_details")
            .and_then(|d| d.get("cached_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let total_tokens = u
            .get("total_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(input_tokens + output_tokens);
        UnifiedUsage {
            input_tokens,
            output_tokens,
            cache_read_tokens,
            total_tokens,
        }
    } else {
        UnifiedUsage::default()
    };

    Ok(UnifiedResponse {
        provider_id: provider_id.to_string(),
        model: model.to_string(),
        content,
        reasoning,
        tool_calls,
        finish_reason,
        usage,
    })
}

/// Parse an OpenAI `/v1/responses` JSON response into `UnifiedResponse`.
pub fn parse_responses_response_json(
    provider_id: &str,
    model: &str,
    body: &str,
) -> Result<UnifiedResponse, ProtocolError> {
    let val: Value = serde_json::from_str(body)
        .map_err(|e| ProtocolError::InvalidJson(format!("OpenAI responses JSON: {e}")))?;
    if let Some(err) = val.get("error").filter(|e| !e.is_null()) {
        let msg = err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Unknown upstream error");
        return Err(ProtocolError::Upstream(msg.to_string()));
    }

    let mut content = val
        .get("output_text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let mut reasoning = None;
    let mut tool_calls = Vec::new();

    if let Some(output) = val.get("output").and_then(Value::as_array) {
        for item in output {
            let itype = item.get("type").and_then(Value::as_str).unwrap_or("");
            match itype {
                "message" => {
                    if content.is_empty() {
                        content = extract_text_from_content_value(item.get("content"));
                    }
                }
                "reasoning" => {
                    if let Some(summary) = item.get("summary").and_then(Value::as_array) {
                        for s in summary {
                            if let Some(t) = s.get("text").and_then(Value::as_str) {
                                reasoning = Some(t.to_string());
                            }
                        }
                    }
                }
                "function_call" => {
                    tool_calls.push(UnifiedToolCall {
                        id: item
                            .get("call_id")
                            .and_then(Value::as_str)
                            .unwrap_or("call_1")
                            .to_string(),
                        name: item
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        arguments: item
                            .get("arguments")
                            .and_then(Value::as_str)
                            .unwrap_or("{}")
                            .to_string(),
                    });
                }
                _ => {}
            }
        }
    }

    let usage = if let Some(u) = val.get("usage") {
        let input_tokens = u.get("input_tokens").and_then(Value::as_u64).unwrap_or(0);
        let output_tokens = u.get("output_tokens").and_then(Value::as_u64).unwrap_or(0);
        let cache_read_tokens = u
            .get("input_tokens_details")
            .and_then(|d| d.get("cached_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let total_tokens = u
            .get("total_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(input_tokens + output_tokens);
        UnifiedUsage {
            input_tokens,
            output_tokens,
            cache_read_tokens,
            total_tokens,
        }
    } else {
        UnifiedUsage::default()
    };

    Ok(UnifiedResponse {
        provider_id: provider_id.to_string(),
        model: model.to_string(),
        content,
        reasoning,
        finish_reason: if tool_calls.is_empty() { "stop".to_string() } else { "tool_calls".to_string() },
        tool_calls,
        usage,
    })
}

/// Parse an Anthropic `/v1/messages` JSON response into `UnifiedResponse`.
pub fn parse_anthropic_response_json(
    provider_id: &str,
    model: &str,
    body: &str,
) -> Result<UnifiedResponse, ProtocolError> {
    let val: Value = serde_json::from_str(body)
        .map_err(|e| ProtocolError::InvalidJson(format!("Anthropic response JSON: {e}")))?;
    if let Some(err) = val.get("error").filter(|e| !e.is_null()) {
        let msg = err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Unknown upstream Anthropic error");
        return Err(ProtocolError::Upstream(msg.to_string()));
    }

    let mut text_parts = Vec::new();
    let mut reasoning = None;
    let mut tool_calls = Vec::new();

    if let Some(content) = val.get("content").and_then(Value::as_array) {
        for block in content {
            match block.get("type").and_then(Value::as_str).unwrap_or("") {
                "text" => {
                    if let Some(t) = block.get("text").and_then(Value::as_str) {
                        text_parts.push(t.to_string());
                    }
                }
                "thinking" => {
                    if let Some(t) = block.get("thinking").and_then(Value::as_str) {
                        reasoning = Some(t.to_string());
                    }
                }
                "tool_use" => {
                    tool_calls.push(UnifiedToolCall {
                        id: block
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or("toolu_1")
                            .to_string(),
                        name: block
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        arguments: block
                            .get("input")
                            .map(|v| v.to_string())
                            .unwrap_or_else(|| "{}".to_string()),
                    });
                }
                _ => {}
            }
        }
    }

    let finish_reason = val
        .get("stop_reason")
        .and_then(Value::as_str)
        .map(|s| match s {
            "end_turn" | "stop_sequence" => "stop".to_string(),
            "max_tokens" => "length".to_string(),
            "tool_use" => "tool_calls".to_string(),
            other => other.to_string(),
        })
        .unwrap_or_else(|| "stop".to_string());

    let usage = if let Some(u) = val.get("usage") {
        let input_tokens = u.get("input_tokens").and_then(Value::as_u64).unwrap_or(0);
        let output_tokens = u.get("output_tokens").and_then(Value::as_u64).unwrap_or(0);
        let cache_read_tokens = u
            .get("cache_read_input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        UnifiedUsage {
            input_tokens,
            output_tokens,
            cache_read_tokens,
            total_tokens: input_tokens + output_tokens,
        }
    } else {
        UnifiedUsage::default()
    };

    Ok(UnifiedResponse {
        provider_id: provider_id.to_string(),
        model: model.to_string(),
        content: text_parts.join("\n"),
        reasoning,
        tool_calls,
        finish_reason,
        usage,
    })
}

/// Parse a single OpenAI streaming SSE chunk line (e.g. `data: {"choices":[{"delta":{"content":"..."}}]}`).
pub fn parse_openai_sse_chunk(line_or_json: &str) -> Result<Option<UnifiedStreamChunk>, ProtocolError> {
    let trimmed = line_or_json.trim();
    let data = trimmed.strip_prefix("data:").map(str::trim).unwrap_or(trimmed);
    if data.is_empty() || data.starts_with(':') || data.starts_with("event:") {
        return Ok(None);
    }
    if data == "[DONE]" {
        return Ok(None);
    }

    let val: Value = serde_json::from_str(data)
        .map_err(|e| ProtocolError::InvalidJson(format!("OpenAI SSE JSON: {e}")))?;
    if let Some(err) = val.get("error").filter(|e| !e.is_null()) {
        let msg = err
            .get("message")
            .and_then(Value::as_str)
            .or_else(|| err.as_str())
            .unwrap_or("SSE upstream error");
        return Err(ProtocolError::Upstream(msg.to_string()));
    }
    if val.get("type").and_then(Value::as_str) == Some("error") {
        let msg = val
            .get("message")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .or_else(|| val.get("code").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()))
            .unwrap_or("SSE upstream error");
        return Err(ProtocolError::Upstream(msg.to_string()));
    }

    let choice = val
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first());

    let text_delta = choice
        .and_then(|c| c.get("delta"))
        .and_then(|d| d.get("content"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let reasoning_delta = choice
        .and_then(|c| c.get("delta"))
        .and_then(|d| d.get("reasoning_content").or_else(|| d.get("thought")))
        .and_then(Value::as_str)
        .map(ToString::to_string);

    let finish_reason = choice
        .and_then(|c| c.get("finish_reason"))
        .and_then(Value::as_str)
        .map(ToString::to_string);

    let mut tool_calls = Vec::new();
    if let Some(tcs) = choice
        .and_then(|c| c.get("delta"))
        .and_then(|d| d.get("tool_calls"))
        .and_then(Value::as_array)
    {
        for (idx, tc) in tcs.iter().enumerate() {
            let id = tc
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let name = tc
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let arguments = tc
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            tool_calls.push(UnifiedToolCall {
                id: if id.is_empty() { format!("call_{idx}") } else { id },
                name,
                arguments,
            });
        }
    }

    let usage = val.get("usage").map(|u| {
        let input_tokens = u.get("prompt_tokens").and_then(Value::as_u64).unwrap_or(0);
        let output_tokens = u.get("completion_tokens").and_then(Value::as_u64).unwrap_or(0);
        let cache_read_tokens = u
            .get("prompt_tokens_details")
            .and_then(|d| d.get("cached_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let total_tokens = u
            .get("total_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(input_tokens + output_tokens);
        UnifiedUsage {
            input_tokens,
            output_tokens,
            cache_read_tokens,
            total_tokens,
        }
    });

    Ok(Some(UnifiedStreamChunk {
        text_delta,
        reasoning_delta,
        tool_calls,
        finish_reason,
        usage,
    }))
}

/// Parse an Anthropic streaming SSE event (`content_block_delta`, `message_delta`, etc.).
pub fn parse_anthropic_sse_event(
    event_type: &str,
    data: &str,
) -> Result<Option<UnifiedStreamChunk>, ProtocolError> {
    let trimmed = data.trim();
    let json_str = trimmed.strip_prefix("data:").map(str::trim).unwrap_or(trimmed);
    if json_str.is_empty() || json_str == "[DONE]" || json_str.starts_with(':') {
        return Ok(None);
    }

    let val: Value = serde_json::from_str(json_str)
        .map_err(|e| ProtocolError::InvalidJson(format!("Anthropic SSE JSON: {e}")))?;
    if let Some(err) = val.get("error").filter(|e| !e.is_null()) {
        let msg = err
            .get("message")
            .and_then(Value::as_str)
            .or_else(|| err.as_str())
            .unwrap_or("Anthropic SSE upstream error");
        return Err(ProtocolError::Upstream(msg.to_string()));
    }

    let etype = if event_type.is_empty() {
        val.get("type").and_then(Value::as_str).unwrap_or("")
    } else {
        event_type
    };

    if etype == "error" || val.get("type").and_then(Value::as_str) == Some("error") {
        let msg = val
            .get("message")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .or_else(|| {
                val.get("code")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or("Anthropic SSE upstream error");
        return Err(ProtocolError::Upstream(msg.to_string()));
    }

    match etype {
        "error" => unreachable!("handled above"),
        "content_block_delta" => {
            let delta = val.get("delta");
            let dtype = delta.and_then(|d| d.get("type")).and_then(Value::as_str).unwrap_or("");
            match dtype {
                "text_delta" => {
                    let text = delta
                        .and_then(|d| d.get("text"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    Ok(Some(UnifiedStreamChunk {
                        text_delta: text,
                        ..Default::default()
                    }))
                }
                "thinking_delta" => {
                    let thinking = delta
                        .and_then(|d| d.get("thinking"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    Ok(Some(UnifiedStreamChunk {
                        reasoning_delta: Some(thinking),
                        ..Default::default()
                    }))
                }
                "input_json_delta" => {
                    let partial = delta
                        .and_then(|d| d.get("partial_json"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let idx = val.get("index").and_then(Value::as_u64).unwrap_or(0);
                    Ok(Some(UnifiedStreamChunk {
                        tool_calls: vec![UnifiedToolCall {
                            id: format!("toolu_{idx}"),
                            name: String::new(),
                            arguments: partial,
                        }],
                        ..Default::default()
                    }))
                }
                _ => Ok(None),
            }
        }
        "content_block_start" => {
            let block = val.get("content_block");
            if block.and_then(|b| b.get("type")).and_then(Value::as_str) == Some("tool_use") {
                let id = block
                    .and_then(|b| b.get("id"))
                    .and_then(Value::as_str)
                    .unwrap_or("toolu_1")
                    .to_string();
                let name = block
                    .and_then(|b| b.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                Ok(Some(UnifiedStreamChunk {
                    tool_calls: vec![UnifiedToolCall {
                        id,
                        name,
                        arguments: String::new(),
                    }],
                    ..Default::default()
                }))
            } else {
                Ok(None)
            }
        }
        "message_delta" => {
            let finish_reason = val
                .get("delta")
                .and_then(|d| d.get("stop_reason"))
                .and_then(Value::as_str)
                .map(|s| match s {
                    "end_turn" | "stop_sequence" => "stop".to_string(),
                    "tool_use" => "tool_calls".to_string(),
                    "max_tokens" => "length".to_string(),
                    other => other.to_string(),
                });
            let usage = val.get("usage").map(|u| {
                let output_tokens = u.get("output_tokens").and_then(Value::as_u64).unwrap_or(0);
                UnifiedUsage {
                    output_tokens,
                    total_tokens: output_tokens,
                    ..Default::default()
                }
            });
            Ok(Some(UnifiedStreamChunk {
                finish_reason,
                usage,
                ..Default::default()
            }))
        }
        "message_start" => {
            let usage = val.get("message").and_then(|m| m.get("usage")).map(|u| {
                let input_tokens = u.get("input_tokens").and_then(Value::as_u64).unwrap_or(0);
                let cache_read = u
                    .get("cache_read_input_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                UnifiedUsage {
                    input_tokens,
                    cache_read_tokens: cache_read,
                    total_tokens: input_tokens,
                    ..Default::default()
                }
            });
            Ok(Some(UnifiedStreamChunk {
                usage,
                ..Default::default()
            }))
        }
        _ => Ok(None),
    }
}

/// Parse a Gemini SSE delta line into `UnifiedStreamChunk`.
pub fn parse_gemini_sse_chunk(
    line_or_json: &str,
    call_counter: usize,
) -> Result<Option<UnifiedStreamChunk>, ProtocolError> {
    let delta = parse_gemini_sse_data(line_or_json, call_counter)?;
    let Some(d) = delta else {
        return Ok(None);
    };
    Ok(Some(UnifiedStreamChunk {
        text_delta: d.text_delta,
        reasoning_delta: (!d.thinking_delta.is_empty()).then_some(d.thinking_delta),
        tool_calls: d.tool_calls,
        finish_reason: d.finish_reason,
        usage: d.usage,
    }))
}

/// Encode a `UnifiedStreamChunk` into an OpenAI-compatible `data: {...}\n\n` SSE line.
pub fn encode_openai_chat_chunk(
    chunk_id: &str,
    model: &str,
    chunk: &UnifiedStreamChunk,
) -> String {
    let mut delta_obj = serde_json::Map::new();
    if !chunk.text_delta.is_empty() {
        delta_obj.insert("content".to_string(), json!(chunk.text_delta));
    }
    if let Some(ref reason) = chunk.reasoning_delta {
        delta_obj.insert("reasoning_content".to_string(), json!(reason));
    }
    if !chunk.tool_calls.is_empty() {
        let tcs: Vec<Value> = chunk
            .tool_calls
            .iter()
            .enumerate()
            .map(|(idx, tc)| {
                json!({
                    "index": idx,
                    "id": tc.id,
                    "type": "function",
                    "function": {
                        "name": tc.name,
                        "arguments": tc.arguments,
                    }
                })
            })
            .collect();
        delta_obj.insert("tool_calls".to_string(), Value::Array(tcs));
    }

    let payload = json!({
        "id": chunk_id,
        "object": "chat.completion.chunk",
        "model": model,
        "choices": [{
            "index": 0,
            "delta": Value::Object(delta_obj),
            "finish_reason": chunk.finish_reason,
        }],
        "usage": chunk.usage.as_ref().map(|u| json!({
            "prompt_tokens": u.input_tokens,
            "completion_tokens": u.output_tokens,
            "total_tokens": u.total_tokens,
        }))
    });

    format!("data: {payload}\n\n")
}

/// Standard OpenAI SSE terminal sequence.
pub fn encode_openai_done() -> &'static str {
    "data: [DONE]\n\n"
}

/// Encode a `UnifiedStreamChunk` into Anthropic SSE event strings.
pub fn encode_anthropic_sse_events(
    _msg_id: &str,
    _model: &str,
    chunk: &UnifiedStreamChunk,
    block_index: usize,
) -> Vec<String> {
    let mut out = Vec::new();

    if let Some(ref reason) = chunk.reasoning_delta {
        out.push(format!(
            "event: content_block_delta\ndata: {}\n\n",
            json!({
                "type": "content_block_delta",
                "index": block_index,
                "delta": {
                    "type": "thinking_delta",
                    "thinking": reason,
                }
            })
        ));
    }

    if !chunk.text_delta.is_empty() {
        out.push(format!(
            "event: content_block_delta\ndata: {}\n\n",
            json!({
                "type": "content_block_delta",
                "index": block_index,
                "delta": {
                    "type": "text_delta",
                    "text": chunk.text_delta,
                }
            })
        ));
    }

    for tc in &chunk.tool_calls {
        out.push(format!(
            "event: content_block_delta\ndata: {}\n\n",
            json!({
                "type": "content_block_delta",
                "index": block_index,
                "delta": {
                    "type": "input_json_delta",
                    "partial_json": tc.arguments,
                }
            })
        ));
    }

    if let Some(ref finish) = chunk.finish_reason {
        let stop_reason = match finish.as_str() {
            "stop" => "end_turn",
            "tool_calls" => "tool_use",
            "length" => "max_tokens",
            other => other,
        };
        out.push(format!(
            "event: message_delta\ndata: {}\n\n",
            json!({
                "type": "message_delta",
                "delta": {
                    "stop_reason": stop_reason,
                    "stop_sequence": null,
                },
                "usage": chunk.usage.as_ref().map(|u| json!({
                    "output_tokens": u.output_tokens,
                }))
            })
        ));
        out.push("event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".to_string());
    }

    out
}

/// Encode a `UnifiedStreamChunk` into a Google Gemini SSE event line.
pub fn encode_gemini_sse_chunk(chunk: &UnifiedStreamChunk) -> String {
    let mut parts = Vec::new();
    if let Some(ref reason) = chunk.reasoning_delta {
        parts.push(json!({
            "thought": true,
            "text": reason,
        }));
    }
    if !chunk.text_delta.is_empty() {
        parts.push(json!({
            "text": chunk.text_delta,
        }));
    }
    for tc in &chunk.tool_calls {
        let args_val: Value = serde_json::from_str(&tc.arguments).unwrap_or_else(|_| json!({}));
        parts.push(json!({
            "functionCall": {
                "name": tc.name,
                "args": args_val,
            }
        }));
    }

    let finish_reason = chunk.finish_reason.as_deref().map(|s| match s {
        "stop" => "STOP",
        "length" => "MAX_TOKENS",
        "content_filter" => "SAFETY",
        other => other,
    });

    let payload = json!({
        "candidates": [{
            "content": {
                "role": "model",
                "parts": parts,
            },
            "finishReason": finish_reason,
        }],
        "usageMetadata": chunk.usage.as_ref().map(|u| json!({
            "promptTokenCount": u.input_tokens,
            "candidatesTokenCount": u.output_tokens,
            "totalTokenCount": u.total_tokens,
        }))
    });

    format!("data: {payload}\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gemini_sse_data_skips_control_lines() {
        for line in [
            "",
            "   ",
            ": ping",
            "id: gem_1",
            "id:123",
            "retry: 1000",
            "retry:500",
            "event: message",
            "data:   ",
            "data: [DONE]",
            "[DONE]",
        ] {
            assert!(
                parse_gemini_sse_data(line, 0).unwrap().is_none(),
                "expected None for SSE control line: {line:?}"
            );
        }

        let delta = parse_gemini_sse_data(
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"ok\"}]}}]}",
            0,
        )
        .unwrap()
        .expect("expected parsed delta");
        assert_eq!(delta.text_delta, "ok");

        let err = parse_gemini_sse_data("data: {broken", 0).unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidJson(_)));
    }

    #[test]
    fn test_inbound_rosetta_adapters_retain_reasoning() {
        // 1. Chat Completions
        let cc = inbound_chat_completions_to_unified(&json!({
            "model": "deepseek-reasoner",
            "messages": [
                {"role": "user", "content": "Hi"},
                {"role": "assistant", "content": "Hello", "reasoning_content": "secret-thought-cc"}
            ]
        }))
        .unwrap();
        assert_eq!(cc.messages[1].reasoning.as_deref(), Some("secret-thought-cc"));

        // 2. Responses
        let resp = inbound_responses_to_unified(&json!({
            "model": "o3",
            "input": [
                {"type": "reasoning", "summary": [{"type": "summary_text", "text": "secret-thought-resp"}]},
                {"type": "message", "role": "assistant", "content": "Answer"}
            ]
        }))
        .unwrap();
        assert_eq!(resp.messages.len(), 2);
        assert_eq!(resp.messages[0].role, UnifiedRole::Assistant);
        assert_eq!(resp.messages[0].reasoning.as_deref(), Some("secret-thought-resp"));
        assert_eq!(resp.messages[1].content, "Answer");

        // 3. Anthropic (with text + thinking-only turn)
        let ant = inbound_anthropic_to_unified(&json!({
            "model": "claude-sonnet-4-20250514",
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        {"type": "thinking", "thinking": "secret-thought-ant"},
                        {"type": "text", "text": "Done"}
                    ]
                },
                {
                    "role": "assistant",
                    "content": [
                        {"type": "thinking", "thinking": "thinking-only-turn"}
                    ]
                }
            ]
        }))
        .unwrap();
        assert_eq!(ant.messages.len(), 2);
        assert_eq!(ant.messages[0].content, "Done");
        assert_eq!(ant.messages[0].reasoning.as_deref(), Some("secret-thought-ant"));
        assert_eq!(ant.messages[1].content, "");
        assert_eq!(ant.messages[1].reasoning.as_deref(), Some("thinking-only-turn"));

        // 4. Case 2a: Empty/whitespace reasoning_content must not shadow populated reasoning/thought
        let cc_shadow = inbound_chat_completions_to_unified(&json!({
            "model": "deepseek-reasoner",
            "messages": [
                {
                    "role": "assistant",
                    "content": "hi",
                    "reasoning_content": "",
                    "reasoning": "sk-ant-api03-shadowsecret"
                },
                {
                    "role": "assistant",
                    "content": "hi2",
                    "reasoning_content": "   ",
                    "thought": "sk-ant-api03-thoughtsecret"
                }
            ]
        }))
        .unwrap();
        assert_eq!(
            cc_shadow.messages[0].reasoning.as_deref(),
            Some("sk-ant-api03-shadowsecret")
        );
        assert_eq!(
            cc_shadow.messages[1].reasoning.as_deref(),
            Some("sk-ant-api03-thoughtsecret")
        );

        // 5. Case 2b: Responses reasoning item retains BOTH summary and content
        let resp_summary_and_content = inbound_responses_to_unified(&json!({
            "model": "o3",
            "input": [
                {
                    "type": "reasoning",
                    "summary": [{"type": "summary_text", "text": "safe summary"}],
                    "content": [{"type": "reasoning_text", "text": "sk-ant-api03-contentsecret"}]
                }
            ]
        }))
        .unwrap();
        assert_eq!(resp_summary_and_content.messages.len(), 1);
        assert_eq!(
            resp_summary_and_content.messages[0].reasoning.as_deref(),
            Some("safe summary\nsk-ant-api03-contentsecret")
        );

        // 6. Case 2c: Responses reasoning item retains content blocks using "thinking" key
        let resp_thinking_content = inbound_responses_to_unified(&json!({
            "model": "o3",
            "input": [
                {
                    "type": "reasoning",
                    "content": [{"type": "thinking", "thinking": "sk-ant-api03-thinkingsecret"}]
                }
            ]
        }))
        .unwrap();
        assert_eq!(resp_thinking_content.messages.len(), 1);
        assert_eq!(
            resp_thinking_content.messages[0].reasoning.as_deref(),
            Some("sk-ant-api03-thinkingsecret")
        );

        // 7. Case 2d: Responses reasoning item retains top-level "reasoning" / "thought" (standalone and with summary)
        let resp_top_level = inbound_responses_to_unified(&json!({
            "model": "o3",
            "input": [
                {
                    "type": "reasoning",
                    "reasoning": "sk-ant-api03-topsecret"
                },
                {
                    "type": "reasoning",
                    "summary": [{"type": "summary_text", "text": "summary-only"}],
                    "thought": "sk-ant-api03-thought-top"
                }
            ]
        }))
        .unwrap();
        assert_eq!(resp_top_level.messages.len(), 2);
        assert_eq!(
            resp_top_level.messages[0].reasoning.as_deref(),
            Some("sk-ant-api03-topsecret")
        );
        assert_eq!(
            resp_top_level.messages[1].reasoning.as_deref(),
            Some("summary-only\nsk-ant-api03-thought-top")
        );

        // 8. Anthropic empty thinking must not shadow text inside thinking/reasoning block
        let ant_shadow = inbound_anthropic_to_unified(&json!({
            "model": "claude-sonnet-4-20250514",
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        {"type": "thinking", "thinking": "", "text": "sk-ant-api03-antshadow"},
                        {"type": "text", "text": "Answer"}
                    ]
                }
            ]
        }))
        .unwrap();
        assert_eq!(ant_shadow.messages.len(), 1);
        assert_eq!(ant_shadow.messages[0].content, "Answer");
        assert_eq!(
            ant_shadow.messages[0].reasoning.as_deref(),
            Some("sk-ant-api03-antshadow")
        );
    }

    #[test]
    fn test_parse_anthropic_and_openai_flat_sse_error_events() {
        let err_flat_anth = parse_anthropic_sse_event(
            "error",
            r#"data: {"type":"error","message":"anth_top_msg_no_nested_obj"}"#,
        )
        .unwrap_err();
        match err_flat_anth {
            ProtocolError::Upstream(msg) => assert!(msg.contains("anth_top_msg_no_nested_obj"), "{msg}"),
            other => panic!("expected Upstream error, got {other:?}"),
        }

        let err_flat_anth_no_ev = parse_anthropic_sse_event(
            "",
            r#"data: {"type":"error","message":"anth_flat_without_event_header"}"#,
        )
        .unwrap_err();
        match err_flat_anth_no_ev {
            ProtocolError::Upstream(msg) => {
                assert!(msg.contains("anth_flat_without_event_header"), "{msg}")
            }
            other => panic!("expected Upstream error, got {other:?}"),
        }

        let err_nested_anth = parse_anthropic_sse_event(
            "error",
            r#"data: {"type":"error","error":{"type":"overloaded_error","message":"overloaded"}}"#,
        )
        .unwrap_err();
        match err_nested_anth {
            ProtocolError::Upstream(msg) => assert_eq!(msg, "overloaded"),
            other => panic!("expected Upstream error, got {other:?}"),
        }

        let err_flat_openai = parse_openai_sse_chunk(
            r#"data: {"type":"error","message":"openai_flat_err"}"#,
        )
        .unwrap_err();
        match err_flat_openai {
            ProtocolError::Upstream(msg) => assert_eq!(msg, "openai_flat_err"),
            other => panic!("expected Upstream error, got {other:?}"),
        }
    }
}
