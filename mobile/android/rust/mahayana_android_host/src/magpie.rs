//! Magpie Multi-Provider Streaming AI Gateway (51+ Presets, 8 Smart Routing Strategies,
//! Circuit-Breaker Cooldowns & Seat Failover, 4-Way Rosetta Wire Translation,
//! Runtime-Salted Reversible PII/Secret Masking + `StreamingUnmasker`, Deep-Link / `.env` Import,
//! Local Subscription Discovery, and SQLite Token/Cost Usage Ledger).
//!
//! Zero fake echo policy: live completions (`chatComplete`, `chatStream`, `execute_live_request`)
//! strictly execute real HTTP/HTTPS + SSE requests against configured upstream endpoints and
//! return structured `PROVIDER_NOT_CONFIGURED` / `UPSTREAM_UNAVAILABLE` errors when unconfigured
//! or unreachable.

use base64::Engine;
pub use magpie_gateway::{
    estimate_cost_usd, parse_anthropic_response_json, parse_anthropic_sse_event,
    parse_chat_completions_response_json, parse_dotenv_providers, parse_gemini_response_json,
    parse_gemini_sse_chunk, parse_magpie_import_link, parse_openai_sse_chunk,
    parse_responses_response_json, parse_retry_after_header, resolve_anthropic_messages_url,
    resolve_gemini_endpoint_url, unified_to_anthropic_request_json,
    unified_to_chat_completions_request_json, unified_to_responses_request_json,
    build_gemini_generate_content_payload, MaskingAction, MaskingConfig, MaskingEngine,
    StreamingUnmasker, UnifiedMessage, UnifiedRequest, UnifiedResponse, UnifiedRole,
    UnifiedStreamChunk, UnifiedToolCall, UnifiedToolSpec, UnifiedUsage,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireProtocol {
    ChatCompletions,
    CodexResponses,
    AnthropicMessages,
    GeminiGenerateContent,
}

impl WireProtocol {
    pub fn from_str_lossy(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "responses" | "codex_responses" | "codexresponses" | "openai_responses" => {
                Self::CodexResponses
            }
            "anthropic" | "anthropic_messages" | "anthropicmessages" | "messages" => {
                Self::AnthropicMessages
            }
            "gemini" | "gemini_generate_content" | "geminigeneratecontent" => {
                Self::GeminiGenerateContent
            }
            _ => Self::ChatCompletions,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoutingStrategy {
    Smart,
    Order,
    Rotate,
    Cheapest,
    Drain,
    Usage,
    Pace,
    Fastest,
}

impl RoutingStrategy {
    pub fn from_str_lossy(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "order" => Self::Order,
            "rotate" | "round_robin" => Self::Rotate,
            "cheapest" | "lowest_cost" => Self::Cheapest,
            "drain" => Self::Drain,
            "usage" => Self::Usage,
            "pace" => Self::Pace,
            "fastest" | "lowest_latency" => Self::Fastest,
            _ => Self::Smart,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub protocol: WireProtocol,
    pub base_url: &'static str,
    pub default_model: &'static str,
    pub input_cost_per_1m_usd: f64,
    pub output_cost_per_1m_usd: f64,
}

macro_rules! preset {
    ($id:expr, $name:expr, $cat:expr, $proto:ident, $url:expr, $model:expr, $in_c:expr, $out_c:expr) => {
        ProviderPreset {
            id: $id,
            name: $name,
            category: $cat,
            protocol: WireProtocol::$proto,
            base_url: $url,
            default_model: $model,
            input_cost_per_1m_usd: $in_c,
            output_cost_per_1m_usd: $out_c,
        }
    };
}

/// Canonical 51+ Magpie provider presets spanning Direct Cloud, CN AI, Aggregators, Local Engines, and Custom Wire Endpoints.
pub static MAGPIE_PRESETS: &[ProviderPreset] = &[
    // Global Frontier Direct (12)
    preset!("openai", "OpenAI", "frontier", ChatCompletions, "https://api.openai.com/v1", "gpt-4o", 2.50, 10.00),
    preset!("openai-codex", "OpenAI Codex Responses", "frontier", CodexResponses, "https://api.openai.com/v1", "o3-mini", 1.10, 4.40),
    preset!("anthropic", "Anthropic Claude", "frontier", AnthropicMessages, "https://api.anthropic.com", "claude-sonnet-4-5", 3.00, 15.00),
    preset!("google", "Google Gemini", "frontier", GeminiGenerateContent, "https://generativelanguage.googleapis.com/v1beta", "gemini-2.5-flash", 0.15, 0.60),
    preset!("google-pro", "Google Gemini Pro", "frontier", GeminiGenerateContent, "https://generativelanguage.googleapis.com/v1beta", "gemini-2.5-pro", 1.25, 10.00),
    preset!("xai", "xAI Grok", "frontier", ChatCompletions, "https://api.x.ai/v1", "grok-3", 3.00, 15.00),
    preset!("mistral", "Mistral AI", "frontier", ChatCompletions, "https://api.mistral.ai/v1", "mistral-large-latest", 2.00, 6.00),
    preset!("cohere", "Cohere", "frontier", ChatCompletions, "https://api.cohere.ai/compatibility/v1", "command-r-plus", 2.50, 10.00),
    preset!("ai21", "AI21 Labs", "frontier", ChatCompletions, "https://api.ai21.com/studio/v1", "jamba-large", 2.00, 8.00),
    preset!("perplexity", "Perplexity Sonar", "frontier", ChatCompletions, "https://api.perplexity.ai", "sonar-pro", 3.00, 15.00),
    preset!("nous", "Nous Research", "frontier", ChatCompletions, "https://inference-api.nousresearch.com/v1", "Hermes-3-Llama-3.1-405B", 0.80, 0.80),
    preset!("amazon-bedrock-openai", "AWS Bedrock Gateway", "frontier", ChatCompletions, "https://bedrock-runtime.us-east-1.amazonaws.com/openai/v1", "anthropic.claude-3-5-sonnet", 3.00, 15.00),
    // High-Speed Inference Cloud (11)
    preset!("groq", "Groq LPU", "fast_cloud", ChatCompletions, "https://api.groq.com/openai/v1", "llama-3.3-70b-versatile", 0.59, 0.79),
    preset!("cerebras", "Cerebras Wafer-Scale", "fast_cloud", ChatCompletions, "https://api.cerebras.ai/v1", "llama-3.3-70b", 0.60, 0.60),
    preset!("sambanova", "SambaNova Cloud", "fast_cloud", ChatCompletions, "https://api.sambanova.ai/v1", "Meta-Llama-3.3-70B-Instruct", 0.60, 0.60),
    preset!("together", "Together AI", "fast_cloud", ChatCompletions, "https://api.together.xyz/v1", "meta-llama/Llama-3.3-70B-Instruct-Turbo", 0.88, 0.88),
    preset!("fireworks", "Fireworks AI", "fast_cloud", ChatCompletions, "https://api.fireworks.ai/inference/v1", "accounts/fireworks/models/deepseek-v3", 0.90, 0.90),
    preset!("deepinfra", "DeepInfra", "fast_cloud", ChatCompletions, "https://api.deepinfra.com/v1/openai", "deepseek-ai/DeepSeek-V3", 0.49, 0.89),
    preset!("hyperbolic", "Hyperbolic", "fast_cloud", ChatCompletions, "https://api.hyperbolic.xyz/v1", "Qwen/Qwen2.5-Coder-32B-Instruct", 0.20, 0.20),
    preset!("novita", "Novita AI", "fast_cloud", ChatCompletions, "https://api.novita.ai/v3/openai", "deepseek/deepseek-r1", 0.55, 2.19),
    preset!("parasail", "Parasail", "fast_cloud", ChatCompletions, "https://api.parasail.io/v1", "meta-llama/Llama-3.3-70B-Instruct", 0.40, 0.40),
    preset!("nebius", "Nebius AI Studio", "fast_cloud", ChatCompletions, "https://api.studio.nebius.ai/v1", "deepseek-ai/DeepSeek-R1", 0.80, 2.40),
    preset!("lambda", "Lambda Cloud Inference", "fast_cloud", ChatCompletions, "https://api.lambdalabs.com/v1", "llama3.3-70b-instruct-fp8", 0.20, 0.20),
    // Asia / CN Frontier Providers (14)
    preset!("deepseek", "DeepSeek (深度求索)", "cn_frontier", ChatCompletions, "https://api.deepseek.com/v1", "deepseek-chat", 0.27, 1.10),
    preset!("deepseek-reasoner", "DeepSeek R1 Reasoner", "cn_frontier", ChatCompletions, "https://api.deepseek.com/v1", "deepseek-reasoner", 0.55, 2.19),
    preset!("qwen", "Alibaba Qwen (通义千问)", "cn_frontier", ChatCompletions, "https://dashscope.aliyuncs.com/compatible-mode/v1", "qwen-max", 0.40, 1.20),
    preset!("moonshot", "Moonshot Kimi (月之暗面)", "cn_frontier", ChatCompletions, "https://api.moonshot.cn/v1", "kimi-k2", 0.60, 2.50),
    preset!("zhipu", "Zhipu GLM (智谱清言)", "cn_frontier", ChatCompletions, "https://open.bigmodel.cn/api/paas/v4", "glm-4-plus", 0.50, 0.50),
    preset!("zai", "Z.ai International", "cn_frontier", ChatCompletions, "https://api.z.ai/api/paas/v4", "glm-4.5", 0.60, 2.20),
    preset!("yi", "01.AI Yi (零一万物)", "cn_frontier", ChatCompletions, "https://api.lingyiwanwu.com/v1", "yi-lightning", 0.14, 0.14),
    preset!("baichuan", "Baichuan (百川智能)", "cn_frontier", ChatCompletions, "https://api.baichuan-ai.com/v1", "Baichuan4-Turbo", 0.50, 0.50),
    preset!("minimax", "MiniMax (稀宇科技)", "cn_frontier", ChatCompletions, "https://api.minimax.chat/v1", "MiniMax-Text-01", 0.15, 1.10),
    preset!("stepfun", "StepFun (阶跃星辰)", "cn_frontier", ChatCompletions, "https://api.stepfun.com/v1", "step-2-16k", 0.50, 1.20),
    preset!("doubao", "ByteDance Volcengine Doubao", "cn_frontier", ChatCompletions, "https://ark.cn-beijing.volces.com/api/v3", "doubao-1-5-pro-32k", 0.11, 0.28),
    preset!("hunyuan", "Tencent Hunyuan (混元)", "cn_frontier", ChatCompletions, "https://api.hunyuan.cloud.tencent.com/v1", "hunyuan-turbo", 0.20, 0.60),
    preset!("spark", "iFlytek Spark (讯飞星火)", "cn_frontier", ChatCompletions, "https://spark-api-open.xf-yun.com/v1", "4.0Ultra", 0.40, 0.80),
    preset!("siliconflow", "SiliconFlow (硅基流动)", "cn_frontier", ChatCompletions, "https://api.siliconflow.cn/v1", "deepseek-ai/DeepSeek-V3", 0.27, 1.10),
    // Routers, Gateways & Developer Platforms (9)
    preset!("openrouter", "OpenRouter", "gateway", ChatCompletions, "https://openrouter.ai/api/v1", "anthropic/claude-3.5-sonnet", 3.00, 15.00),
    preset!("github-models", "GitHub Models", "gateway", ChatCompletions, "https://models.inference.ai.azure.com", "gpt-4o", 0.00, 0.00),
    preset!("azure-openai", "Azure OpenAI", "gateway", ChatCompletions, "https://YOUR_RESOURCE.openai.azure.com/openai/deployments/gpt-4o", "gpt-4o", 2.50, 10.00),
    preset!("cloudflare-ai", "Cloudflare Workers AI", "gateway", ChatCompletions, "https://api.cloudflare.com/client/v4/accounts/ACCOUNT/ai/v1", "@cf/meta/llama-3.3-70b-instruct-fp8-fast", 0.29, 2.25),
    preset!("huggingface", "HuggingFace Inference", "gateway", ChatCompletions, "https://router.huggingface.co/v1", "Qwen/Qwen2.5-72B-Instruct", 0.30, 0.30),
    preset!("nvidia-nim", "NVIDIA NIM", "gateway", ChatCompletions, "https://integrate.api.nvidia.com/v1", "nvidia/llama-3.1-nemotron-70b-instruct", 0.35, 0.40),
    preset!("replicate-openai", "Replicate Gateway", "gateway", ChatCompletions, "https://openai-proxy.replicate.com/v1", "meta/meta-llama-3-70b-instruct", 0.65, 2.75),
    preset!("vercel-ai", "Vercel AI Gateway", "gateway", ChatCompletions, "https://ai-gateway.vercel.sh/v1", "openai/gpt-4o-mini", 0.15, 0.60),
    preset!("litellm", "LiteLLM Proxy", "gateway", ChatCompletions, "http://127.0.0.1:4000/v1", "gpt-4o-mini", 0.15, 0.60),
    // Local & On-Device Engines + Custom Wire Adapters (9)
    preset!("ollama", "Ollama Local", "local", ChatCompletions, "http://127.0.0.1:11434/v1", "qwen2.5:7b", 0.00, 0.00),
    preset!("lmstudio", "LM Studio Local", "local", ChatCompletions, "http://127.0.0.1:1234/v1", "local-model", 0.00, 0.00),
    preset!("vllm", "vLLM Server", "local", ChatCompletions, "http://127.0.0.1:8000/v1", "Qwen/Qwen2.5-14B-Instruct", 0.00, 0.00),
    preset!("llamacpp", "llama.cpp Server", "local", ChatCompletions, "http://127.0.0.1:8080/v1", "local-gguf", 0.00, 0.00),
    preset!("mlx-lm", "Apple/Local MLX Server", "local", ChatCompletions, "http://127.0.0.1:8088/v1", "mlx-community/Qwen2.5-7B-Instruct-4bit", 0.00, 0.00),
    preset!("custom-openai", "Custom OpenAI-Compatible", "custom", ChatCompletions, "https://api.example.com/v1", "custom-model", 0.50, 1.50),
    preset!("custom-responses", "Custom Codex Responses", "custom", CodexResponses, "https://api.example.com/v1", "custom-responses-model", 0.50, 1.50),
    preset!("custom-anthropic", "Custom Anthropic Messages", "custom", AnthropicMessages, "https://api.example.com", "claude-sonnet-4-5", 3.00, 15.00),
    preset!("custom-gemini", "Custom Gemini Endpoint", "custom", GeminiGenerateContent, "https://api.example.com/v1beta", "gemini-2.5-flash", 0.15, 0.60),
];

static ROTATE_COUNTER: AtomicUsize = AtomicUsize::new(0);
static MASKING_ENGINE: OnceLock<MaskingEngine> = OnceLock::new();

pub fn shared_masking_engine() -> &'static MaskingEngine {
    MASKING_ENGINE.get_or_init(|| {
        MaskingEngine::new(MaskingConfig {
            enabled: true,
            mask_secrets: true,
            mask_personal: true,
            default_action: MaskingAction::Mask,
            ..MaskingConfig::default()
        })
    })
}

/// Reversible runtime-salted masking wrapper delegating to shared `magpie_gateway::MaskingEngine`.
pub struct MaskingVault {
    engine: MaskingEngine,
}

impl Default for MaskingVault {
    fn default() -> Self {
        Self::new()
    }
}

impl MaskingVault {
    pub fn new() -> Self {
        Self {
            engine: MaskingEngine::new(MaskingConfig {
                enabled: true,
                mask_secrets: true,
                mask_personal: true,
                default_action: MaskingAction::Mask,
                ..MaskingConfig::default()
            }),
        }
    }

    pub fn mask(&mut self, input: &str) -> (String, usize) {
        let res = self.engine.mask_text(input);
        (res.masked_text, res.hits.len())
    }

    pub fn unmask(&self, text: &str) -> String {
        self.engine.unmask_text(text)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfiguredProvider {
    pub id: String,
    pub preset_id: String,
    pub name: String,
    pub protocol: WireProtocol,
    pub base_url: String,
    pub api_key: Option<String>,
    pub default_model: String,
    pub priority: i32,
    pub weight: u32,
    pub enabled: bool,
    pub allow_no_key: bool,
    pub input_cost_per_1m_usd: f64,
    pub output_cost_per_1m_usd: f64,
    pub cooldown_until_ms: i64,
    pub consecutive_failures: u32,
    pub last_latency_ms: u64,
    pub total_requests: u64,
}

impl ConfiguredProvider {
    pub fn has_live_credentials(&self) -> bool {
        if self
            .api_key
            .as_deref()
            .map(|k| !k.trim().is_empty())
            .unwrap_or(false)
        {
            return true;
        }
        if self.allow_no_key {
            return true;
        }
        is_local_loopback_url(&self.base_url)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveTurnResult {
    pub provider_id: String,
    pub preset_id: String,
    pub model: String,
    pub protocol: WireProtocol,
    pub reply: String,
    pub reasoning: Option<String>,
    pub tool_calls: Vec<UnifiedToolCall>,
    pub finish_reason: String,
    pub masked_secrets_count: usize,
    pub usage: UnifiedUsage,
    pub cost_usd: f64,
    pub latency_ms: u64,
    pub failed_over_from: Vec<String>,
}

pub fn is_local_loopback_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    let rest = if let Some(r) = lower.strip_prefix("http://") {
        r
    } else if let Some(r) = lower.strip_prefix("https://") {
        r
    } else {
        return false;
    };
    for host in ["127.0.0.1", "localhost", "[::1]"] {
        if let Some(after) = rest.strip_prefix(host) {
            if after.is_empty()
                || after.starts_with(':')
                || after.starts_with('/')
                || after.starts_with('?')
                || after.starts_with('#')
            {
                return true;
            }
        }
    }
    false
}

/// Circuit-breaker rest window computation matching Magpie exponential backoff.
pub fn compute_cooldown_ms(
    status_code: u16,
    consecutive_failures: u32,
    retry_after_ms: Option<u64>,
) -> i64 {
    if let Some(ra) = retry_after_ms.filter(|&ms| ms > 0) {
        return (ra.min(3_600_000)) as i64;
    }
    let exp = consecutive_failures.saturating_sub(1).min(5);
    match status_code {
        429 => (15_000i64 * (1i64 << exp)).min(300_000),
        401 | 402 | 403 => (300_000i64 * (1i64 << exp.min(3))).min(1_800_000),
        500..=599 => (5_000i64 * (1i64 << exp)).min(120_000),
        _ => 10_000,
    }
}

/// Rosetta 4-way wire translation delegates directly to shared `magpie_gateway::protocol`.
pub fn translate_rosetta_request(
    target: WireProtocol,
    model: &str,
    system_prompt: Option<&str>,
    messages: &[(String, String)],
    max_tokens: u32,
    stream: bool,
) -> Value {
    let unified_messages = messages
        .iter()
        .map(|(role, content)| UnifiedMessage {
            role: match role.as_str() {
                "system" => UnifiedRole::System,
                "assistant" | "model" => UnifiedRole::Assistant,
                "tool" => UnifiedRole::Tool,
                _ => UnifiedRole::User,
            },
            content: content.clone(),
            name: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            reasoning: None,
        })
        .collect::<Vec<_>>();

    let req = UnifiedRequest {
        model: model.to_string(),
        system_prompt: system_prompt.unwrap_or_default().to_string(),
        messages: unified_messages,
        tools: Vec::new(),
        temperature: None,
        max_tokens: Some(max_tokens),
        thinking_budget_tokens: None,
        stream,
    };

    match target {
        WireProtocol::ChatCompletions => unified_to_chat_completions_request_json(&req),
        WireProtocol::CodexResponses => unified_to_responses_request_json(&req),
        WireProtocol::AnthropicMessages => unified_to_anthropic_request_json(&req),
        WireProtocol::GeminiGenerateContent => build_gemini_generate_content_payload(&req),
    }
}

/// Parses a `magpie://import?...` deep-link or Base64 JSON bundle.
pub fn parse_import_uri(uri: &str) -> Result<Value, String> {
    let trimmed = uri.trim();
    if let Ok(spec) = parse_magpie_import_link(trimmed) {
        return Ok(json!({
            "preset": spec.preset_id.unwrap_or_else(|| "openai".to_string()),
            "name": spec.name,
            "base_url": spec.base_url,
            "api_key": spec.api_key,
            "model": spec.models.first().cloned(),
        }));
    }

    if let Some(rest) = trimmed.strip_prefix("magpie://import?") {
        let mut map = BTreeMap::new();
        for kv in rest.split('&') {
            if let Some((k, v)) = kv.split_once('=') {
                map.insert(k.to_string(), url_decode(v));
            }
        }
        if let Some(b64) = map.get("data") {
            let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(b64)
                .or_else(|_| base64::engine::general_purpose::STANDARD.decode(b64))
                .map_err(|e| format!("Invalid base64 in magpie://import?data=: {e}"))?;
            return serde_json::from_slice(&bytes)
                .map_err(|e| format!("Invalid JSON in magpie import payload: {e}"));
        }
        return Ok(serde_json::to_value(map).map_err(|e| e.to_string())?);
    }
    Err("Expected magpie://import?... deep link".into())
}

/// Parses `.env` text for well-known LLM API keys (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, etc.).
pub fn parse_dotenv_keys(env_text: &str) -> Vec<(&'static str, String)> {
    let env_vars = [
        ("OPENAI_API_KEY", "openai"),
        ("ANTHROPIC_API_KEY", "anthropic"),
        ("GEMINI_API_KEY", "google"),
        ("GOOGLE_API_KEY", "google"),
        ("DEEPSEEK_API_KEY", "deepseek"),
        ("GROQ_API_KEY", "groq"),
        ("OPENROUTER_API_KEY", "openrouter"),
        ("XAI_API_KEY", "xai"),
        ("MISTRAL_API_KEY", "mistral"),
        ("MOONSHOT_API_KEY", "moonshot"),
        ("DASHSCOPE_API_KEY", "qwen"),
        ("ZHIPU_API_KEY", "zhipu"),
        ("SILICONFLOW_API_KEY", "siliconflow"),
        ("TOGETHER_API_KEY", "together"),
        ("FIREWORKS_API_KEY", "fireworks"),
    ];
    let mut found = Vec::new();
    for raw_line in env_text.lines() {
        let line = raw_line.trim().trim_start_matches("export ").trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let key_name = k.trim();
            let val = v.trim().trim_matches('"').trim_matches('\'').to_string();
            if val.is_empty() {
                continue;
            }
            for &(env_k, preset_id) in &env_vars {
                if key_name == env_k {
                    found.push((preset_id, val.clone()));
                }
            }
        }
    }
    found
}

fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(
                std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""),
                16,
            ) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS magpie_providers (
            id TEXT PRIMARY KEY,
            preset_id TEXT NOT NULL,
            name TEXT NOT NULL,
            protocol TEXT NOT NULL,
            base_url TEXT NOT NULL,
            api_key TEXT,
            default_model TEXT NOT NULL,
            priority INTEGER NOT NULL DEFAULT 10,
            weight INTEGER NOT NULL DEFAULT 100,
            enabled INTEGER NOT NULL DEFAULT 1,
            allow_no_key INTEGER NOT NULL DEFAULT 0,
            input_cost_per_1m_usd REAL NOT NULL DEFAULT 1.0,
            output_cost_per_1m_usd REAL NOT NULL DEFAULT 3.0,
            cooldown_until_ms INTEGER NOT NULL DEFAULT 0,
            consecutive_failures INTEGER NOT NULL DEFAULT 0,
            last_latency_ms INTEGER NOT NULL DEFAULT 0,
            total_requests INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS magpie_config (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS magpie_usage_ledger (
            entry_id TEXT PRIMARY KEY,
            provider_id TEXT NOT NULL,
            model TEXT NOT NULL,
            input_tokens INTEGER NOT NULL,
            output_tokens INTEGER NOT NULL,
            cost_usd REAL NOT NULL,
            latency_ms INTEGER NOT NULL,
            created_at_ms INTEGER NOT NULL
        );",
    )
    .map_err(|e| format!("Failed to initialize Magpie schema: {e}"))?;

    // Safe additive column migration for existing SQLite databases
    let _ = conn.execute(
        "ALTER TABLE magpie_providers ADD COLUMN allow_no_key INTEGER NOT NULL DEFAULT 0",
        [],
    );

    conn.execute(
        "INSERT OR IGNORE INTO magpie_config (key, value) VALUES ('routing_strategy', 'Smart')",
        [],
    )
    .map_err(|e| e.to_string())?;

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM magpie_providers", [], |r| r.get(0))
        .unwrap_or(0);
    if count == 0 {
        // Seed unauthenticated catalog templates (api_key = NULL -> never selected for live calls
        // until the user supplies an API key, imports .env/URI, or configures a local endpoint).
        for (idx, preset_id) in ["google", "deepseek", "anthropic", "openai"]
            .iter()
            .enumerate()
        {
            if let Some(p) = MAGPIE_PRESETS.iter().find(|x| x.id == *preset_id) {
                conn.execute(
                    "INSERT INTO magpie_providers
                    (id, preset_id, name, protocol, base_url, api_key, default_model, priority, weight, enabled, allow_no_key, input_cost_per_1m_usd, output_cost_per_1m_usd, last_latency_ms)
                    VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7, 100, 1, 0, ?8, ?9, ?10)",
                    params![
                        p.id,
                        p.id,
                        p.name,
                        format!("{:?}", p.protocol),
                        p.base_url,
                        p.default_model,
                        100 + (idx as i32),
                        p.input_cost_per_1m_usd,
                        p.output_cost_per_1m_usd,
                        0i64,
                    ],
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }

    // Hydrate from process environment keys if explicitly provided at launch
    hydrate_env_providers_if_present(conn)?;
    Ok(())
}

fn hydrate_env_providers_if_present(conn: &Connection) -> Result<(), String> {
    let env_vars = [
        ("OPENAI_API_KEY", "openai", "OPENAI_BASE_URL"),
        ("ANTHROPIC_API_KEY", "anthropic", "ANTHROPIC_BASE_URL"),
        ("GEMINI_API_KEY", "google", "GEMINI_BASE_URL"),
        ("GOOGLE_API_KEY", "google", "GEMINI_BASE_URL"),
        ("DEEPSEEK_API_KEY", "deepseek", "DEEPSEEK_BASE_URL"),
        ("OPENROUTER_API_KEY", "openrouter", "OPENROUTER_BASE_URL"),
    ];
    for (key_var, preset_id, url_var) in env_vars {
        if let Ok(raw_key) = std::env::var(key_var) {
            let trimmed = raw_key.trim();
            if !trimmed.is_empty() {
                if let Some(preset) = MAGPIE_PRESETS.iter().find(|p| p.id == preset_id) {
                    let custom_url = std::env::var(url_var)
                        .ok()
                        .map(|u| u.trim().to_string())
                        .filter(|u| !u.is_empty())
                        .unwrap_or_else(|| preset.base_url.to_string());
                    conn.execute(
                        "INSERT INTO magpie_providers
                        (id, preset_id, name, protocol, base_url, api_key, default_model, priority, weight, enabled, allow_no_key, input_cost_per_1m_usd, output_cost_per_1m_usd)
                        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, 100, 1, 0, ?8, ?9)
                        ON CONFLICT(id) DO UPDATE SET
                            api_key = COALESCE(magpie_providers.api_key, excluded.api_key),
                            base_url = CASE WHEN magpie_providers.api_key IS NULL THEN excluded.base_url ELSE magpie_providers.base_url END",
                        params![
                            preset.id,
                            preset.id,
                            preset.name,
                            format!("{:?}", preset.protocol),
                            custom_url,
                            trimmed,
                            preset.default_model,
                            preset.input_cost_per_1m_usd,
                            preset.output_cost_per_1m_usd,
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                }
            }
        }
    }
    Ok(())
}

fn load_providers(conn: &Connection) -> Result<Vec<ConfiguredProvider>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, preset_id, name, protocol, base_url, api_key, default_model,
                    priority, weight, enabled, allow_no_key, input_cost_per_1m_usd, output_cost_per_1m_usd,
                    cooldown_until_ms, consecutive_failures, last_latency_ms, total_requests
             FROM magpie_providers ORDER BY priority ASC, id ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let proto_s: String = row.get(3)?;
            Ok(ConfiguredProvider {
                id: row.get(0)?,
                preset_id: row.get(1)?,
                name: row.get(2)?,
                protocol: WireProtocol::from_str_lossy(&proto_s),
                base_url: row.get(4)?,
                api_key: row.get(5)?,
                default_model: row.get(6)?,
                priority: row.get(7)?,
                weight: row.get::<_, i64>(8)?.max(1) as u32,
                enabled: row.get::<_, i64>(9)? != 0,
                allow_no_key: row.get::<_, i64>(10).unwrap_or(0) != 0,
                input_cost_per_1m_usd: row.get(11)?,
                output_cost_per_1m_usd: row.get(12)?,
                cooldown_until_ms: row.get(13)?,
                consecutive_failures: row.get::<_, i64>(14)?.max(0) as u32,
                last_latency_ms: row.get::<_, i64>(15)?.max(0) as u64,
                total_requests: row.get::<_, i64>(16)?.max(0) as u64,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

pub fn current_routing_strategy(conn: &Connection) -> RoutingStrategy {
    let strat_str: String = conn
        .query_row(
            "SELECT value FROM magpie_config WHERE key = 'routing_strategy'",
            [],
            |r| r.get(0),
        )
        .unwrap_or_else(|_| "Smart".into());
    RoutingStrategy::from_str_lossy(&strat_str)
}

pub fn has_configured_live_seat(conn: &Connection) -> bool {
    if init_schema(conn).is_err() {
        return false;
    }
    load_providers(conn)
        .map(|all| all.into_iter().any(|p| p.enabled && p.has_live_credentials()))
        .unwrap_or(false)
}

fn order_candidates(mut healthy: Vec<ConfiguredProvider>, strategy: RoutingStrategy) -> Vec<ConfiguredProvider> {
    if healthy.len() <= 1 {
        return healthy;
    }
    match strategy {
        RoutingStrategy::Order | RoutingStrategy::Drain => {
            healthy.sort_by_key(|p| (p.priority, p.id.clone()));
        }
        RoutingStrategy::Rotate => {
            healthy.sort_by_key(|p| (p.priority, p.id.clone()));
            let start = ROTATE_COUNTER.fetch_add(1, Ordering::Relaxed) % healthy.len();
            healthy.rotate_left(start);
        }
        RoutingStrategy::Cheapest => {
            healthy.sort_by(|a, b| {
                let ca = a.input_cost_per_1m_usd + a.output_cost_per_1m_usd;
                let cb = b.input_cost_per_1m_usd + b.output_cost_per_1m_usd;
                ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        RoutingStrategy::Fastest => {
            healthy.sort_by_key(|p| {
                let effective_latency = if p.last_latency_ms == 0 {
                    u64::MAX
                } else {
                    p.last_latency_ms
                };
                (effective_latency, p.priority)
            });
        }
        RoutingStrategy::Usage | RoutingStrategy::Pace => {
            healthy.sort_by_key(|p| (p.total_requests, p.priority));
        }
        RoutingStrategy::Smart => {
            healthy.sort_by(|a, b| {
                let score_a = (a.priority as f64) * 10.0
                    + (a.input_cost_per_1m_usd + a.output_cost_per_1m_usd) * 5.0
                    + (a.last_latency_ms as f64) * 0.05
                    + (a.consecutive_failures as f64) * 40.0;
                let score_b = (b.priority as f64) * 10.0
                    + (b.input_cost_per_1m_usd + b.output_cost_per_1m_usd) * 5.0
                    + (b.last_latency_ms as f64) * 0.05
                    + (b.consecutive_failures as f64) * 40.0;
                score_a
                    .partial_cmp(&score_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
    }
    healthy
}

fn select_route_preview(
    conn: &Connection,
    strategy: RoutingStrategy,
    now_ms: i64,
) -> Result<ConfiguredProvider, String> {
    let all = load_providers(conn)?;
    let healthy: Vec<ConfiguredProvider> = all
        .into_iter()
        .filter(|p| p.enabled && p.cooldown_until_ms <= now_ms)
        .collect();
    if healthy.is_empty() {
        return Err("All Magpie providers are disabled or in circuit-breaker cooldown".into());
    }
    Ok(order_candidates(healthy, strategy).remove(0))
}

fn select_live_candidates(
    conn: &Connection,
    strategy: RoutingStrategy,
    preferred_provider_id: Option<&str>,
    now_ms: i64,
) -> Result<Vec<ConfiguredProvider>, String> {
    let all = load_providers(conn)?;
    let credentialed: Vec<ConfiguredProvider> = all
        .into_iter()
        .filter(|p| p.enabled && p.has_live_credentials())
        .collect();
    if credentialed.is_empty() {
        return Err(
            "PROVIDER_NOT_CONFIGURED: No LLM provider API key or local endpoint is configured in Magpie Gateway"
                .into(),
        );
    }

    let healthy: Vec<ConfiguredProvider> = credentialed
        .into_iter()
        .filter(|p| p.cooldown_until_ms <= now_ms)
        .collect();
    if healthy.is_empty() {
        return Err(
            "UPSTREAM_UNAVAILABLE: All configured Magpie seats are currently in circuit-breaker cooldown"
                .into(),
        );
    }

    let mut ordered = order_candidates(healthy, strategy);
    if let Some(pref) = preferred_provider_id.filter(|s| !s.trim().is_empty()) {
        if let Some(pos) = ordered
            .iter()
            .position(|p| p.id == pref || p.preset_id == pref)
        {
            let chosen = ordered.remove(pos);
            ordered.insert(0, chosen);
        }
    }
    Ok(ordered)
}

fn record_provider_failure(
    conn: &Connection,
    provider_id: &str,
    status_code: u16,
    retry_after_ms: Option<u64>,
) -> Result<i64, String> {
    let cur_failures: u32 = conn
        .query_row(
            "SELECT consecutive_failures FROM magpie_providers WHERE id = ?1",
            params![provider_id],
            |r| r.get::<_, i64>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .unwrap_or(0)
        .max(0) as u32;
    let next_failures = cur_failures + 1;
    let cooldown_ms = compute_cooldown_ms(status_code, next_failures, retry_after_ms);
    let until = chrono::Utc::now().timestamp_millis() + cooldown_ms;
    conn.execute(
        "UPDATE magpie_providers
         SET consecutive_failures = ?1, cooldown_until_ms = ?2
         WHERE id = ?3",
        params![next_failures as i64, until, provider_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(cooldown_ms)
}

fn mask_unified_request(req: &UnifiedRequest, engine: &MaskingEngine) -> (UnifiedRequest, usize) {
    let mut masked_count = 0usize;
    let sys_res = engine.mask_text(&req.system_prompt);
    masked_count += sys_res.hits.len();

    let mut masked_messages = Vec::with_capacity(req.messages.len());
    for msg in &req.messages {
        let content_res = engine.mask_text(&msg.content);
        masked_count += content_res.hits.len();
        let mut tcs = Vec::with_capacity(msg.tool_calls.len());
        for tc in &msg.tool_calls {
            let arg_res = engine.mask_text(&tc.arguments);
            masked_count += arg_res.hits.len();
            tcs.push(UnifiedToolCall {
                id: tc.id.clone(),
                name: tc.name.clone(),
                arguments: arg_res.masked_text,
                thought_signature: tc.thought_signature.clone(),
            });
        }
        masked_messages.push(UnifiedMessage {
            role: msg.role,
            content: content_res.masked_text,
            name: msg.name.clone(),
            tool_call_id: msg.tool_call_id.clone(),
            tool_calls: tcs,
            reasoning: msg.reasoning.clone(),
        });
    }

    (
        UnifiedRequest {
            model: req.model.clone(),
            system_prompt: sys_res.masked_text,
            messages: masked_messages,
            tools: req.tools.clone(),
            temperature: req.temperature,
            max_tokens: req.max_tokens,
            thinking_budget_tokens: req.thinking_budget_tokens,
            stream: req.stream,
        },
        masked_count,
    )
}

#[derive(Debug)]
struct SeatHttpError {
    status_code: u16,
    retry_after_ms: Option<u64>,
    message: String,
    partial_emitted: bool,
}

fn build_seat_endpoint_and_body(
    seat: &ConfiguredProvider,
    req: &UnifiedRequest,
) -> (String, Value) {
    let base = seat.base_url.trim_end_matches('/');
    match seat.protocol {
        WireProtocol::ChatCompletions => {
            let url = if base.ends_with("/chat/completions") {
                base.to_string()
            } else {
                format!("{base}/chat/completions")
            };
            (url, unified_to_chat_completions_request_json(req))
        }
        WireProtocol::CodexResponses => {
            let url = if base.ends_with("/responses") {
                base.to_string()
            } else {
                format!("{base}/responses")
            };
            (url, unified_to_responses_request_json(req))
        }
        WireProtocol::AnthropicMessages => {
            let url = resolve_anthropic_messages_url(base);
            (url, unified_to_anthropic_request_json(req))
        }
        WireProtocol::GeminiGenerateContent => {
            let url = resolve_gemini_endpoint_url(base, &req.model, req.stream);
            (url, build_gemini_generate_content_payload(req))
        }
    }
}

fn merge_stream_tool_calls(accum: &mut Vec<UnifiedToolCall>, deltas: Vec<UnifiedToolCall>) {
    for tc in deltas {
        if !tc.name.is_empty() && (accum.is_empty() || !tc.id.starts_with("call_") && !tc.id.starts_with("toolu_")) {
            if let Some(existing) = accum.iter_mut().find(|e| e.id == tc.id) {
                if existing.name.is_empty() {
                    existing.name = tc.name;
                }
                existing.arguments.push_str(&tc.arguments);
                continue;
            }
            accum.push(tc);
            continue;
        }
        if let Some(existing) = accum.iter_mut().find(|e| e.id == tc.id) {
            if !tc.name.is_empty() && existing.name.is_empty() {
                existing.name = tc.name;
            }
            existing.arguments.push_str(&tc.arguments);
        } else if tc.name.is_empty() && !accum.is_empty() {
            // Continuation delta for the most recent tool call block
            if let Some(last) = accum.last_mut() {
                last.arguments.push_str(&tc.arguments);
            }
        } else {
            accum.push(tc);
        }
    }
}

fn parse_codex_responses_sse_line(
    event_type: &str,
    line_or_json: &str,
) -> Result<Option<UnifiedStreamChunk>, String> {
    let trimmed = line_or_json.trim();
    let data = trimmed.strip_prefix("data:").map(str::trim).unwrap_or(trimmed);
    if data.is_empty() || data == "[DONE]" || data.starts_with(':') {
        return Ok(None);
    }
    let val: Value = serde_json::from_str(data)
        .map_err(|e| format!("Codex Responses SSE JSON parse error: {e}"))?;
    let etype = if !event_type.is_empty() {
        event_type
    } else {
        val.get("type").and_then(Value::as_str).unwrap_or("")
    };

    if etype == "error" || etype == "response.failed" {
        let msg = val
            .pointer("/error/message")
            .or_else(|| val.pointer("/response/error/message"))
            .or_else(|| val.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("Codex Responses upstream SSE error");
        return Err(msg.to_string());
    }

    match etype {
        "response.output_text.delta" => {
            let delta = val
                .get("delta")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            Ok(Some(UnifiedStreamChunk {
                text_delta: delta,
                ..Default::default()
            }))
        }
        "response.reasoning_summary_text.delta" | "response.reasoning.delta" => {
            let delta = val
                .get("delta")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            Ok(Some(UnifiedStreamChunk {
                reasoning_delta: Some(delta),
                ..Default::default()
            }))
        }
        "response.output_item.done" => {
            if let Some(item) = val.get("item") {
                if item.get("type").and_then(Value::as_str) == Some("function_call") {
                    let id = item
                        .get("call_id")
                        .or_else(|| item.get("id"))
                        .and_then(Value::as_str)
                        .unwrap_or("call_0")
                        .to_string();
                    let name = item
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let arguments = item
                        .get("arguments")
                        .and_then(Value::as_str)
                        .unwrap_or("{}")
                        .to_string();
                    return Ok(Some(UnifiedStreamChunk {
                        tool_calls: vec![UnifiedToolCall {
                            id,
                            name,
                            arguments,
                            thought_signature: None,
                        }],
                        ..Default::default()
                    }));
                }
            }
            Ok(None)
        }
        "response.completed" | "response.done" => {
            let resp_obj = val.get("response").unwrap_or(&val);
            let usage = resp_obj.get("usage").map(|u| {
                let input_tokens = u.get("input_tokens").and_then(Value::as_u64).unwrap_or(0);
                let output_tokens = u.get("output_tokens").and_then(Value::as_u64).unwrap_or(0);
                let cache_read_tokens = u
                    .pointer("/input_tokens_details/cached_tokens")
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
                finish_reason: Some("stop".into()),
                usage,
                ..Default::default()
            }))
        }
        _ => Ok(None),
    }
}

fn call_single_seat<F>(
    seat: &ConfiguredProvider,
    masked_req: &UnifiedRequest,
    engine: &MaskingEngine,
    on_delta: &mut F,
) -> Result<(UnifiedResponse, u64), SeatHttpError>
where
    F: FnMut(&str),
{
    let started = Instant::now();
    let (url, body) = build_seat_endpoint_and_body(seat, masked_req);

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(60))
        .build();

    let mut request = agent.post(&url).set("Content-Type", "application/json");
    if masked_req.stream {
        request = request.set("Accept", "text/event-stream");
    }
    if let Some(key) = seat
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty())
    {
        match seat.protocol {
            WireProtocol::AnthropicMessages => {
                request = request
                    .set("x-api-key", key)
                    .set("anthropic-version", "2023-06-01");
            }
            WireProtocol::GeminiGenerateContent => {
                request = request.set("x-goog-api-key", key);
            }
            WireProtocol::ChatCompletions | WireProtocol::CodexResponses => {
                request = request.set("Authorization", &format!("Bearer {key}"));
            }
        }
    }

    let response = match request.send_json(body) {
        Ok(resp) => resp,
        Err(ureq::Error::Status(code, resp)) => {
            let retry_after_ms = resp
                .header("Retry-After")
                .or_else(|| resp.header("retry-after"))
                .and_then(parse_retry_after_header);
            let err_body = resp.into_string().unwrap_or_default();
            let preview: String = err_body.chars().take(240).collect();
            return Err(SeatHttpError {
                status_code: code,
                retry_after_ms,
                message: format!("HTTP {code} from {}: {preview}", seat.id),
                partial_emitted: false,
            });
        }
        Err(ureq::Error::Transport(tr)) => {
            return Err(SeatHttpError {
                status_code: 503,
                retry_after_ms: None,
                message: format!("Transport error contacting {}: {tr}", seat.id),
                partial_emitted: false,
            });
        }
    };

    let content_type = response
        .header("Content-Type")
        .or_else(|| response.header("content-type"))
        .unwrap_or("")
        .to_ascii_lowercase();
    let is_sse = content_type.contains("text/event-stream");

    const MAX_NON_SSE_BODY_BYTES: u64 = 16 * 1024 * 1024;
    const MAX_SSE_TOTAL_BYTES: u64 = 32 * 1024 * 1024;

    if !is_sse && !masked_req.stream {
        let mut raw_bytes = Vec::new();
        let mut limited = std::io::Read::take(response.into_reader(), MAX_NON_SSE_BODY_BYTES + 1);
        std::io::Read::read_to_end(&mut limited, &mut raw_bytes).map_err(|e| SeatHttpError {
            status_code: 502,
            retry_after_ms: None,
            message: format!("Failed reading response body from {}: {e}", seat.id),
            partial_emitted: false,
        })?;
        if raw_bytes.len() as u64 > MAX_NON_SSE_BODY_BYTES {
            return Err(SeatHttpError {
                status_code: 502,
                retry_after_ms: None,
                message: format!(
                    "Response body from {} exceeded maximum allowed size ({MAX_NON_SSE_BODY_BYTES} bytes)",
                    seat.id
                ),
                partial_emitted: false,
            });
        }
        let raw_body = String::from_utf8(raw_bytes).map_err(|e| SeatHttpError {
            status_code: 502,
            retry_after_ms: None,
            message: format!("Response body from {} was not valid UTF-8: {e}", seat.id),
            partial_emitted: false,
        })?;
        let parsed = parse_non_stream_body(seat, &masked_req.model, &raw_body)?;
        let unmasked = unmask_unified_response(parsed, engine);
        if !unmasked.content.is_empty() {
            on_delta(&unmasked.content);
        }
        let latency_ms = started.elapsed().as_millis().max(1) as u64;
        return Ok((unmasked, latency_ms));
    }

    let mut reader = BufReader::new(std::io::Read::take(
        response.into_reader(),
        MAX_SSE_TOTAL_BYTES + 1,
    ));
    if !is_sse {
        // Provider returned JSON even though stream=true was requested; support seamlessly.
        let mut raw_bytes = Vec::new();
        let mut limited = std::io::Read::take(&mut reader, MAX_NON_SSE_BODY_BYTES + 1);
        std::io::Read::read_to_end(&mut limited, &mut raw_bytes).map_err(|e| SeatHttpError {
            status_code: 502,
            retry_after_ms: None,
            message: format!("Failed reading response body from {}: {e}", seat.id),
            partial_emitted: false,
        })?;
        if raw_bytes.len() as u64 > MAX_NON_SSE_BODY_BYTES {
            return Err(SeatHttpError {
                status_code: 502,
                retry_after_ms: None,
                message: format!(
                    "Response body from {} exceeded maximum allowed size ({MAX_NON_SSE_BODY_BYTES} bytes)",
                    seat.id
                ),
                partial_emitted: false,
            });
        }
        let full_buf = String::from_utf8(raw_bytes).map_err(|e| SeatHttpError {
            status_code: 502,
            retry_after_ms: None,
            message: format!("Response body from {} was not valid UTF-8: {e}", seat.id),
            partial_emitted: false,
        })?;
        let trimmed = full_buf.trim();
        if trimmed.starts_with('{') {
            let parsed = parse_non_stream_body(seat, &masked_req.model, trimmed)?;
            let unmasked = unmask_unified_response(parsed, engine);
            if !unmasked.content.is_empty() {
                on_delta(&unmasked.content);
            }
            let latency_ms = started.elapsed().as_millis().max(1) as u64;
            return Ok((unmasked, latency_ms));
        }
        // Otherwise process buffered text as SSE lines
        let parsed = consume_sse_reader(
            BufReader::new(full_buf.as_bytes()),
            seat,
            &masked_req.model,
            engine,
            on_delta,
        )?;
        let latency_ms = started.elapsed().as_millis().max(1) as u64;
        return Ok((parsed, latency_ms));
    }

    let parsed = consume_sse_reader(reader, seat, &masked_req.model, engine, on_delta)?;
    let latency_ms = started.elapsed().as_millis().max(1) as u64;
    Ok((parsed, latency_ms))
}

fn parse_non_stream_body(
    seat: &ConfiguredProvider,
    fallback_model: &str,
    raw_json: &str,
) -> Result<UnifiedResponse, SeatHttpError> {
    let res = match seat.protocol {
        WireProtocol::ChatCompletions => {
            parse_chat_completions_response_json(&seat.id, fallback_model, raw_json)
        }
        WireProtocol::CodexResponses => {
            parse_responses_response_json(&seat.id, fallback_model, raw_json)
        }
        WireProtocol::AnthropicMessages => {
            parse_anthropic_response_json(&seat.id, fallback_model, raw_json)
        }
        WireProtocol::GeminiGenerateContent => {
            parse_gemini_response_json(&seat.id, fallback_model, raw_json)
        }
    };
    res.map_err(|e| SeatHttpError {
        status_code: 502,
        retry_after_ms: None,
        message: format!("Protocol parse failure from {}: {e}", seat.id),
        partial_emitted: false,
    })
}

fn consume_sse_reader<R, F>(
    mut reader: R,
    seat: &ConfiguredProvider,
    model: &str,
    engine: &MaskingEngine,
    on_delta: &mut F,
) -> Result<UnifiedResponse, SeatHttpError>
where
    R: BufRead,
    F: FnMut(&str),
{
    let mut unmasker = StreamingUnmasker::new();
    let mut full_content = String::new();
    let mut full_reasoning = String::new();
    let mut tool_calls: Vec<UnifiedToolCall> = Vec::new();
    let mut finish_reason = "stop".to_string();
    let mut usage = UnifiedUsage::default();
    let mut current_event = String::new();
    let mut gemini_call_counter = 0usize;
    const MAX_SSE_TOTAL_BYTES: usize = 32 * 1024 * 1024;
    const MAX_SSE_LINES: usize = 200_000;
    let mut partial_emitted = false;
    let mut total_bytes_read = 0usize;
    let mut lines_read = 0usize;
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).map_err(|e| SeatHttpError {
            status_code: 502,
            retry_after_ms: None,
            message: format!("SSE stream read error from {}: {e}", seat.id),
            partial_emitted,
        })?;
        if n == 0 {
            break;
        }
        total_bytes_read = total_bytes_read.saturating_add(n);
        lines_read = lines_read.saturating_add(1);
        if total_bytes_read > MAX_SSE_TOTAL_BYTES || lines_read > MAX_SSE_LINES {
            return Err(SeatHttpError {
                status_code: 502,
                retry_after_ms: None,
                message: format!(
                    "SSE stream from {} exceeded maximum allowed bounds ({MAX_SSE_TOTAL_BYTES} bytes / {MAX_SSE_LINES} lines)",
                    seat.id
                ),
                partial_emitted,
            });
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            current_event.clear();
            continue;
        }
        if let Some(ev) = trimmed.strip_prefix("event:") {
            current_event = ev.trim().to_string();
            continue;
        }
        if trimmed.starts_with(':') {
            continue;
        }
        if !trimmed.starts_with("data:") {
            continue;
        }

        let chunk_opt = match seat.protocol {
            WireProtocol::ChatCompletions => {
                parse_openai_sse_chunk(trimmed).map_err(|e| SeatHttpError {
                    status_code: 502,
                    retry_after_ms: None,
                    message: format!("{e}"),
                    partial_emitted,
                })?
            }
            WireProtocol::AnthropicMessages => {
                parse_anthropic_sse_event(&current_event, trimmed).map_err(|e| SeatHttpError {
                    status_code: 502,
                    retry_after_ms: None,
                    message: format!("{e}"),
                    partial_emitted,
                })?
            }
            WireProtocol::GeminiGenerateContent => {
                let c = parse_gemini_sse_chunk(trimmed, gemini_call_counter).map_err(|e| {
                    SeatHttpError {
                        status_code: 502,
                        retry_after_ms: None,
                        message: format!("{e}"),
                        partial_emitted,
                    }
                })?;
                if let Some(ref ch) = c {
                    gemini_call_counter += ch.tool_calls.len();
                }
                c
            }
            WireProtocol::CodexResponses => parse_codex_responses_sse_line(&current_event, trimmed)
                .map_err(|msg| SeatHttpError {
                    status_code: 502,
                    retry_after_ms: None,
                    message: msg,
                    partial_emitted,
                })?,
        };

        if let Some(chunk) = chunk_opt {
            if !chunk.text_delta.is_empty() {
                let ready = unmasker.push(&chunk.text_delta, engine);
                if !ready.is_empty() {
                    partial_emitted = true;
                    full_content.push_str(&ready);
                    on_delta(&ready);
                }
            }
            if let Some(r_delta) = chunk.reasoning_delta {
                if !r_delta.is_empty() {
                    full_reasoning.push_str(&engine.unmask_text(&r_delta));
                }
            }
            if !chunk.tool_calls.is_empty() {
                merge_stream_tool_calls(&mut tool_calls, chunk.tool_calls);
            }
            if let Some(fr) = chunk.finish_reason {
                finish_reason = fr;
            }
            if let Some(u) = chunk.usage {
                if u.input_tokens > 0 {
                    usage.input_tokens = u.input_tokens;
                }
                if u.output_tokens > 0 {
                    usage.output_tokens = u.output_tokens;
                }
                if u.cache_read_tokens > 0 {
                    usage.cache_read_tokens = u.cache_read_tokens;
                }
                usage.total_tokens = usage.input_tokens + usage.output_tokens;
            }
        }
    }

    let tail = unmasker.finish(engine);
    if !tail.is_empty() {
        full_content.push_str(&tail);
        on_delta(&tail);
    }

    for tc in &mut tool_calls {
        tc.arguments = engine.unmask_text(&tc.arguments);
    }
    if !tool_calls.is_empty() && finish_reason == "stop" {
        finish_reason = "tool_calls".to_string();
    }

    if usage.input_tokens == 0 && usage.output_tokens == 0 {
        usage.output_tokens = ((full_content.len() as u64) / 4).max(1);
        usage.total_tokens = usage.output_tokens;
    }

    Ok(UnifiedResponse {
        provider_id: seat.id.clone(),
        model: model.to_string(),
        content: full_content,
        reasoning: (!full_reasoning.is_empty()).then_some(full_reasoning),
        tool_calls,
        finish_reason,
        usage,
    })
}

fn unmask_unified_response(mut resp: UnifiedResponse, engine: &MaskingEngine) -> UnifiedResponse {
    resp.content = engine.unmask_text(&resp.content);
    if let Some(r) = resp.reasoning {
        resp.reasoning = Some(engine.unmask_text(&r));
    }
    for tc in &mut resp.tool_calls {
        tc.arguments = engine.unmask_text(&tc.arguments);
    }
    resp
}

/// Executes a genuine Magpie multi-provider LLM turn with reversible PII pre-masking,
/// SSE chunk unmasking, circuit-breaker seat failover, and real SQLite usage accounting.
/// Releases `db` mutex across all blocking HTTP/SSE upstream calls.
pub fn execute_live_request<F>(
    db: &Mutex<Connection>,
    req: &UnifiedRequest,
    strategy_override: Option<RoutingStrategy>,
    preferred_provider_id: Option<&str>,
    _session_id: Option<&str>,
    mut on_delta: F,
) -> Result<LiveTurnResult, String>
where
    F: FnMut(&str),
{
    let candidates = crate::with_db(db, |conn| {
        init_schema(conn)?;
        let now_ms = chrono::Utc::now().timestamp_millis();
        let strategy = strategy_override.unwrap_or_else(|| current_routing_strategy(conn));
        select_live_candidates(conn, strategy, preferred_provider_id, now_ms)
    })?;

    let engine = shared_masking_engine();
    let (mut masked_req, masked_secrets_count) = mask_unified_request(req, engine);

    let mut failed_over_from = Vec::new();
    let mut last_errors = Vec::new();

    for seat in &candidates {
        let effective_model = if req.model.trim().is_empty() || req.model == "auto" {
            seat.default_model.clone()
        } else {
            req.model.clone()
        };
        masked_req.model = effective_model.clone();

        match call_single_seat(seat, &masked_req, engine, &mut on_delta) {
            Ok((mut resp, latency_ms)) => {
                if resp.usage.input_tokens == 0 {
                    let approx_in = masked_req
                        .messages
                        .iter()
                        .map(|m| (m.content.len() as u64) / 4 + 1)
                        .sum::<u64>()
                        + (masked_req.system_prompt.len() as u64) / 4;
                    resp.usage.input_tokens = approx_in.max(1);
                    resp.usage.total_tokens = resp.usage.input_tokens + resp.usage.output_tokens;
                }

                let cost_usd = (resp.usage.input_tokens as f64 / 1_000_000.0)
                    * seat.input_cost_per_1m_usd
                    + (resp.usage.output_tokens as f64 / 1_000_000.0)
                        * seat.output_cost_per_1m_usd;

                let entry_id = format!("use_{}", uuid::Uuid::new_v4().simple());
                let recorded_at = chrono::Utc::now().timestamp_millis();
                crate::with_db(db, |conn| {
                    conn.execute(
                        "INSERT INTO magpie_usage_ledger
                        (entry_id, provider_id, model, input_tokens, output_tokens, cost_usd, latency_ms, created_at_ms)
                        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                        params![
                            entry_id,
                            seat.id,
                            effective_model,
                            resp.usage.input_tokens as i64,
                            resp.usage.output_tokens as i64,
                            cost_usd,
                            latency_ms as i64,
                            recorded_at,
                        ],
                    )
                    .map_err(|e| e.to_string())?;

                    conn.execute(
                        "UPDATE magpie_providers
                         SET total_requests = total_requests + 1,
                             consecutive_failures = 0,
                             cooldown_until_ms = 0,
                             last_latency_ms = ?1
                         WHERE id = ?2",
                        params![latency_ms as i64, seat.id],
                    )
                    .map_err(|e| e.to_string())?;
                    Ok(())
                })?;

                return Ok(LiveTurnResult {
                    provider_id: seat.id.clone(),
                    preset_id: seat.preset_id.clone(),
                    model: effective_model,
                    protocol: seat.protocol,
                    reply: resp.content,
                    reasoning: resp.reasoning,
                    tool_calls: resp.tool_calls,
                    finish_reason: resp.finish_reason,
                    masked_secrets_count,
                    usage: resp.usage,
                    cost_usd,
                    latency_ms,
                    failed_over_from,
                });
            }
            Err(err) => {
                let _ = crate::with_db(db, |conn| {
                    record_provider_failure(conn, &seat.id, err.status_code, err.retry_after_ms)
                });
                failed_over_from.push(seat.id.clone());
                last_errors.push(err.message.clone());
                if err.partial_emitted {
                    break;
                }
            }
        }
    }

    Err(format!(
        "UPSTREAM_UNAVAILABLE: All configured Magpie seats failed ({})",
        last_errors.join("; ")
    ))
}

pub fn handle_rpc(db: &Mutex<Connection>, submethod: &str, params: &Value) -> Result<Value, String> {
    match submethod {
        "presets" => Ok(json!({
            "presets": MAGPIE_PRESETS,
            "total": MAGPIE_PRESETS.len(),
            "sharedGatewayPresetsCount": magpie_gateway::all_presets().len(),
        })),

        "chatComplete" | "chatStream" => handle_chat_rpc(db, submethod, params),

        "parseModelsDevCatalog" => {
            let raw_json = params
                .get("json")
                .or_else(|| params.get("raw"))
                .and_then(Value::as_str)
                .ok_or_else(|| "magpie.parseModelsDevCatalog requires 'json' string".to_string())?;
            let now_ms = chrono::Utc::now().timestamp_millis();
            let catalog = magpie_gateway::parse_models_dev_json(raw_json, now_ms)?;
            Ok(json!({
                "source": catalog.source,
                "fetchedAtMs": catalog.fetched_at_ms,
                "providerCount": catalog.providers.len(),
                "providers": catalog.providers,
            }))
        }

        "fetchModelsDevCatalog" => {
            let force_refresh = params
                .get("forceRefresh")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let url = params
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or(magpie_gateway::MODELS_DEV_API_URL);
            let cache_path = params
                .get("cachePath")
                .and_then(Value::as_str)
                .map(std::path::PathBuf::from);
            let agent = ureq::AgentBuilder::new()
                .timeout_connect(std::time::Duration::from_secs(10))
                .timeout_read(std::time::Duration::from_secs(20))
                .build();
            let catalog = magpie_gateway::fetch_models_dev_catalog_from_url(
                &agent,
                url,
                cache_path.as_deref(),
                force_refresh,
            )?;
            Ok(json!({
                "source": catalog.source,
                "fetchedAtMs": catalog.fetched_at_ms,
                "providerCount": catalog.providers.len(),
                "providers": catalog.providers,
            }))
        }

        "discoverModels" => {
            let kind_raw = params
                .get("subscriptionKind")
                .or_else(|| params.get("providerId"))
                .and_then(Value::as_str)
                .unwrap_or("codex");
            let kind = match kind_raw.trim().to_ascii_lowercase().as_str() {
                "google-vertex" | "vertex" | "google_vertex" => {
                    magpie_gateway::SubscriptionKind::GoogleVertex
                }
                "gemini" | "google" | "gemini_code_assist" => {
                    magpie_gateway::SubscriptionKind::GeminiCodeAssist
                }
                "anthropic" | "claude" | "claude_code" => {
                    magpie_gateway::SubscriptionKind::ClaudeCode
                }
                "github_copilot" | "copilot" => magpie_gateway::SubscriptionKind::GitHubCopilot,
                "antigravity" => magpie_gateway::SubscriptionKind::Antigravity,
                "grok" | "xai" => magpie_gateway::SubscriptionKind::GrokSuperGrok,
                _ => magpie_gateway::SubscriptionKind::CodexChatGpt,
            };

            let base_url = params.get("baseUrl").and_then(Value::as_str);
            let bearer = params
                .get("apiKey")
                .or_else(|| params.get("bearerToken"))
                .and_then(Value::as_str);
            let project_id = params.get("projectId").and_then(Value::as_str);
            let location = params.get("location").and_then(Value::as_str);
            let headers = std::collections::BTreeMap::new();

            let mut live_models: Vec<String> = Vec::new();
            let mut live_error: Option<String> = None;
            if bearer.is_some() || base_url.is_some() {
                let agent = ureq::AgentBuilder::new()
                    .timeout_connect(std::time::Duration::from_secs(10))
                    .timeout_read(std::time::Duration::from_secs(20))
                    .build();
                match magpie_gateway::fetch_live_upstream_models(
                    &agent,
                    kind,
                    base_url,
                    bearer,
                    &headers,
                    project_id,
                    location,
                ) {
                    Ok(m) => live_models = m,
                    Err(e) => live_error = Some(e),
                }
            }

            let defaults: Vec<String> = magpie_gateway::default_models_for_subscription_kind(kind)
                .iter()
                .map(|s| (*s).to_string())
                .collect();
            let merged = magpie_gateway::merge_discovered_models(&[&live_models, &defaults]);
            Ok(json!({
                "subscriptionKind": format!("{kind:?}"),
                "modelsDevProvider": magpie_gateway::models_dev_provider_id_for_subscription(kind),
                "liveCount": live_models.len(),
                "liveError": live_error,
                "models": merged,
            }))
        }

        "discoverSyncableSubscriptions" => {
            let home_dir = params
                .get("homeDir")
                .and_then(Value::as_str)
                .map(std::path::PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(std::path::PathBuf::from))
                .unwrap_or_else(|| std::path::PathBuf::from("/"));
            let target_is_remote = params
                .get("targetIsRemote")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let items = magpie_gateway::discover_syncable_subscriptions_in_home(
                &home_dir,
                None,
                target_is_remote,
            );
            let redacted: Vec<Value> = items
                .iter()
                .map(|item| {
                    json!({
                        "id": item.account.id,
                        "kind": format!("{:?}", item.account.kind),
                        "label": item.account.label,
                        "email": item.account.email,
                        "plan": item.account.plan,
                        "active": item.account.active,
                        "baseUrl": item.account.base_url,
                        "defaultModel": item.account.default_model,
                        "regionId": item.region_id,
                        "hasAccessToken": item.access_token.is_some(),
                        "hasVertexAdc": item.vertex_adc.is_some(),
                        "hasCodexOAuth": item.codex_oauth.is_some(),
                        "availableModels": item.available_models,
                    })
                })
                .collect();
            Ok(json!({
                "total": redacted.len(),
                "subscriptions": redacted,
            }))
        }

        "sanitizeVertexSchema" => {
            let schema = params
                .get("schema")
                .ok_or_else(|| "magpie.sanitizeVertexSchema requires 'schema'".to_string())?;
            Ok(json!({
                "sanitized": magpie_gateway::sanitize_schema_for_gemini(schema),
            }))
        }

        "mergeGeminiFunctionResponses" => {
            let req_val = params
                .get("request")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let unified: UnifiedRequest = serde_json::from_value(req_val)
                .map_err(|e| format!("Invalid UnifiedRequest JSON: {e}"))?;
            let payload = magpie_gateway::build_gemini_generate_content_payload(&unified);
            Ok(json!({ "payload": payload }))
        }

        _ => crate::with_db(db, |conn| handle_rpc_local(conn, submethod, params)),
    }
}

fn handle_chat_rpc(
    db: &Mutex<Connection>,
    submethod: &str,
    params: &Value,
) -> Result<Value, String> {
    let stream = submethod == "chatStream"
        || params
            .get("stream")
            .and_then(Value::as_bool)
            .unwrap_or(true);
    let system_prompt = params
        .get("system")
        .or_else(|| params.get("systemPrompt"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let model = params
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("auto")
        .to_string();
    let preferred_provider = params.get("providerId").and_then(Value::as_str);
    let strategy_override = params
        .get("strategy")
        .and_then(Value::as_str)
        .map(RoutingStrategy::from_str_lossy);

    let mut messages = Vec::new();
    if let Some(arr) = params.get("messages").and_then(Value::as_array) {
        for m in arr {
            let role = match m.get("role").and_then(Value::as_str).unwrap_or("user") {
                "system" => UnifiedRole::System,
                "assistant" | "model" => UnifiedRole::Assistant,
                "tool" => UnifiedRole::Tool,
                _ => UnifiedRole::User,
            };
            let content = m
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            messages.push(UnifiedMessage {
                role,
                content,
                name: m.get("name").and_then(Value::as_str).map(ToString::to_string),
                tool_call_id: m
                    .get("toolCallId")
                    .or_else(|| m.get("tool_call_id"))
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                tool_calls: Vec::new(),
                reasoning: None,
            });
        }
    } else {
        let prompt = params
            .get("prompt")
            .or_else(|| params.get("text"))
            .and_then(Value::as_str)
            .ok_or_else(|| "chatComplete requires 'prompt', 'text', or 'messages'".to_string())?;
        messages.push(UnifiedMessage {
            role: UnifiedRole::User,
            content: prompt.to_string(),
            name: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
            reasoning: None,
        });
    }

    let req = UnifiedRequest {
        model,
        system_prompt,
        messages,
        tools: Vec::new(),
        temperature: params
            .get("temperature")
            .and_then(Value::as_f64)
            .map(|v| v as f32),
        max_tokens: params
            .get("maxTokens")
            .and_then(Value::as_u64)
            .map(|v| v as u32),
        thinking_budget_tokens: None,
        stream,
    };

    let mut deltas = Vec::new();
    let outcome = execute_live_request(
        db,
        &req,
        strategy_override,
        preferred_provider,
        params.get("sessionId").and_then(Value::as_str),
        |delta| {
            if !delta.is_empty() {
                deltas.push(delta.to_string());
            }
        },
    )?;

    Ok(json!({
        "providerId": outcome.provider_id,
        "presetId": outcome.preset_id,
        "model": outcome.model,
        "protocol": outcome.protocol,
        "reply": outcome.reply,
        "reasoning": outcome.reasoning,
        "toolCalls": outcome.tool_calls,
        "finishReason": outcome.finish_reason,
        "deltas": deltas,
        "maskedSecretsCount": outcome.masked_secrets_count,
        "inputTokens": outcome.usage.input_tokens,
        "outputTokens": outcome.usage.output_tokens,
        "costUsd": outcome.cost_usd,
        "latencyMs": outcome.latency_ms,
        "failedOverFrom": outcome.failed_over_from,
    }))
}

fn handle_rpc_local(conn: &Connection, submethod: &str, params: &Value) -> Result<Value, String> {
    init_schema(conn)?;
    match submethod {
        "presets" => Ok(json!({
            "presets": MAGPIE_PRESETS,
            "total": MAGPIE_PRESETS.len(),
            "sharedGatewayPresetsCount": magpie_gateway::all_presets().len(),
        })),

        "providers" | "status" => {
            let providers = load_providers(conn)?;
            let redacted: Vec<Value> = providers
                .iter()
                .map(|p| {
                    json!({
                        "id": p.id,
                        "presetId": p.preset_id,
                        "name": p.name,
                        "protocol": p.protocol,
                        "baseUrl": p.base_url,
                        "hasApiKey": p.api_key.as_ref().map(|k| !k.is_empty()).unwrap_or(false),
                        "configuredForLive": p.has_live_credentials(),
                        "defaultModel": p.default_model,
                        "priority": p.priority,
                        "weight": p.weight,
                        "enabled": p.enabled,
                        "allowNoKey": p.allow_no_key,
                        "inputCostPer1M": p.input_cost_per_1m_usd,
                        "outputCostPer1M": p.output_cost_per_1m_usd,
                        "cooldownUntilMs": p.cooldown_until_ms,
                        "consecutiveFailures": p.consecutive_failures,
                        "lastLatencyMs": p.last_latency_ms,
                        "totalRequests": p.total_requests,
                    })
                })
                .collect();
            let strat_str: String = conn
                .query_row(
                    "SELECT value FROM magpie_config WHERE key = 'routing_strategy'",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or_else(|_| "Smart".into());
            let live_ready = providers.iter().any(|p| p.enabled && p.has_live_credentials());
            Ok(json!({
                "routingStrategy": strat_str,
                "liveReady": live_ready,
                "presetCount": MAGPIE_PRESETS.len(),
                "providers": redacted,
            }))
        }

        "upsertProvider" => {
            let preset_id = params
                .get("presetId")
                .and_then(Value::as_str)
                .unwrap_or("custom-openai");
            let preset = MAGPIE_PRESETS
                .iter()
                .find(|p| p.id == preset_id)
                .unwrap_or_else(|| MAGPIE_PRESETS.last().expect("non-empty presets"));
            let id = params
                .get("id")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(ToString::to_string)
                .unwrap_or_else(|| preset.id.to_string());
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(preset.name);
            let proto = params
                .get("protocol")
                .and_then(Value::as_str)
                .map(WireProtocol::from_str_lossy)
                .unwrap_or(preset.protocol);
            let region_id = params.get("regionId").and_then(Value::as_str);
            let default_base_url = if let Some(reg) = region_id {
                magpie_gateway::resolve_preset_endpoints(preset_id, Some(reg), None)
                    .map(|ep| ep.base_url)
                    .unwrap_or_else(|| preset.base_url.to_string())
            } else {
                preset.base_url.to_string()
            };
            let base_url = params
                .get("baseUrl")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(&default_base_url);
            let api_key = params
                .get("apiKey")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let model = params
                .get("defaultModel")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(preset.default_model);
            let priority = params.get("priority").and_then(Value::as_i64).unwrap_or(5) as i32;
            let enabled = params
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let allow_no_key = params
                .get("allowNoKey")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| preset.category == "local" || is_local_loopback_url(base_url));
            let clear_api_key = params
                .get("clearApiKey")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let in_cost = params
                .get("inputCostPer1M")
                .and_then(Value::as_f64)
                .unwrap_or(preset.input_cost_per_1m_usd);
            let out_cost = params
                .get("outputCostPer1M")
                .and_then(Value::as_f64)
                .unwrap_or(preset.output_cost_per_1m_usd);

            if clear_api_key {
                conn.execute(
                    "INSERT INTO magpie_providers
                    (id, preset_id, name, protocol, base_url, api_key, default_model, priority, weight, enabled, allow_no_key, input_cost_per_1m_usd, output_cost_per_1m_usd, cooldown_until_ms, consecutive_failures)
                    VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7, 100, ?8, ?9, ?10, ?11, 0, 0)
                    ON CONFLICT(id) DO UPDATE SET
                        preset_id = excluded.preset_id,
                        name = excluded.name,
                        protocol = excluded.protocol,
                        base_url = excluded.base_url,
                        api_key = NULL,
                        default_model = excluded.default_model,
                        priority = excluded.priority,
                        enabled = excluded.enabled,
                        allow_no_key = excluded.allow_no_key,
                        input_cost_per_1m_usd = excluded.input_cost_per_1m_usd,
                        output_cost_per_1m_usd = excluded.output_cost_per_1m_usd,
                        cooldown_until_ms = 0,
                        consecutive_failures = 0",
                    params![
                        id,
                        preset.id,
                        name,
                        format!("{proto:?}"),
                        base_url,
                        model,
                        priority,
                        if enabled { 1 } else { 0 },
                        if allow_no_key { 1 } else { 0 },
                        in_cost,
                        out_cost,
                    ],
                )
                .map_err(|e| e.to_string())?;
            } else {
                conn.execute(
                    "INSERT INTO magpie_providers
                    (id, preset_id, name, protocol, base_url, api_key, default_model, priority, weight, enabled, allow_no_key, input_cost_per_1m_usd, output_cost_per_1m_usd, cooldown_until_ms, consecutive_failures)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 100, ?9, ?10, ?11, ?12, 0, 0)
                    ON CONFLICT(id) DO UPDATE SET
                        preset_id = excluded.preset_id,
                        name = excluded.name,
                        protocol = excluded.protocol,
                        base_url = excluded.base_url,
                        api_key = COALESCE(excluded.api_key, magpie_providers.api_key),
                        default_model = excluded.default_model,
                        priority = excluded.priority,
                        enabled = excluded.enabled,
                        allow_no_key = excluded.allow_no_key,
                        input_cost_per_1m_usd = excluded.input_cost_per_1m_usd,
                        output_cost_per_1m_usd = excluded.output_cost_per_1m_usd,
                        cooldown_until_ms = 0,
                        consecutive_failures = 0",
                    params![
                        id,
                        preset.id,
                        name,
                        format!("{proto:?}"),
                        base_url,
                        api_key,
                        model,
                        priority,
                        if enabled { 1 } else { 0 },
                        if allow_no_key { 1 } else { 0 },
                        in_cost,
                        out_cost,
                    ],
                )
                .map_err(|e| e.to_string())?;
            }

            let persisted_has_key: bool = conn
                .query_row(
                    "SELECT api_key FROM magpie_providers WHERE id = ?1",
                    params![&id],
                    |r| r.get::<_, Option<String>>(0),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .flatten()
                .map(|k| !k.trim().is_empty())
                .unwrap_or(false);

            Ok(json!({
                "id": id,
                "presetId": preset.id,
                "protocol": proto,
                "defaultModel": model,
                "configuredForLive": persisted_has_key || allow_no_key || is_local_loopback_url(base_url),
            }))
        }

        "removeProvider" => {
            let id = params
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| "removeProvider requires 'id'".to_string())?;
            let deleted = conn
                .execute("DELETE FROM magpie_providers WHERE id = ?1", params![id])
                .map_err(|e| e.to_string())?;
            Ok(json!({ "id": id, "removed": deleted > 0 }))
        }

        "setStrategy" => {
            let s = params
                .get("strategy")
                .and_then(Value::as_str)
                .unwrap_or("Smart");
            let parsed = RoutingStrategy::from_str_lossy(s);
            let label = format!("{parsed:?}");
            conn.execute(
                "INSERT OR REPLACE INTO magpie_config (key, value) VALUES ('routing_strategy', ?1)",
                params![label],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({ "strategy": label }))
        }

        "routePreview" => {
            let strat_str: String = params
                .get("strategy")
                .and_then(Value::as_str)
                .map(ToString::to_string)
                .unwrap_or_else(|| {
                    conn.query_row(
                        "SELECT value FROM magpie_config WHERE key = 'routing_strategy'",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap_or_else(|_| "Smart".into())
                });
            let strat = RoutingStrategy::from_str_lossy(&strat_str);
            let chosen = select_route_preview(conn, strat, chrono::Utc::now().timestamp_millis())?;
            Ok(json!({
                "strategy": format!("{strat:?}"),
                "providerId": chosen.id,
                "name": chosen.name,
                "protocol": chosen.protocol,
                "model": chosen.default_model,
                "baseUrl": chosen.base_url,
                "configuredForLive": chosen.has_live_credentials(),
            }))
        }

        "recordFailure" => {
            let id = params
                .get("providerId")
                .and_then(Value::as_str)
                .ok_or_else(|| "recordFailure requires 'providerId'".to_string())?;
            let status_code = params
                .get("statusCode")
                .and_then(Value::as_u64)
                .unwrap_or(429) as u16;
            let retry_after_ms = params
                .get("retryAfterHeader")
                .and_then(Value::as_str)
                .and_then(parse_retry_after_header);
            let cooldown_ms = record_provider_failure(conn, id, status_code, retry_after_ms)?;
            let row: Option<(i64, i64)> = conn
                .query_row(
                    "SELECT consecutive_failures, cooldown_until_ms FROM magpie_providers WHERE id = ?1",
                    params![id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            let (consecutive_failures, until) = row.unwrap_or((1, chrono::Utc::now().timestamp_millis() + cooldown_ms));
            Ok(json!({
                "providerId": id,
                "statusCode": status_code,
                "consecutiveFailures": consecutive_failures,
                "cooldownMs": cooldown_ms,
                "cooldownUntilMs": until,
            }))
        }

        "rosettaTranslate" => {
            let target = params
                .get("targetProtocol")
                .and_then(Value::as_str)
                .map(WireProtocol::from_str_lossy)
                .unwrap_or(WireProtocol::AnthropicMessages);
            let model = params
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or("claude-sonnet-4-5");
            let system = params.get("system").and_then(Value::as_str);
            let max_tokens = params
                .get("maxTokens")
                .and_then(Value::as_u64)
                .unwrap_or(1024) as u32;
            let stream = params
                .get("stream")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let mut msgs = Vec::new();
            if let Some(arr) = params.get("messages").and_then(Value::as_array) {
                for m in arr {
                    let role = m
                        .get("role")
                        .and_then(Value::as_str)
                        .unwrap_or("user")
                        .to_string();
                    let content = m
                        .get("content")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    msgs.push((role, content));
                }
            } else if let Some(text) = params.get("text").and_then(Value::as_str) {
                msgs.push(("user".into(), text.to_string()));
            }
            let payload =
                translate_rosetta_request(target, model, system, &msgs, max_tokens, stream);
            Ok(json!({
                "targetProtocol": target,
                "payload": payload,
            }))
        }

        "maskPreview" => {
            let text = params
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let engine = shared_masking_engine();
            let res = engine.mask_text(text);
            let restored = engine.unmask_text(&res.masked_text);
            Ok(json!({
                "maskedText": res.masked_text,
                "maskedCount": res.hits.len(),
                "hits": res.hits,
                "restoredMatchesInput": restored == text,
            }))
        }

        "importUri" => {
            let uri = params
                .get("uri")
                .and_then(Value::as_str)
                .ok_or_else(|| "importUri requires 'uri'".to_string())?;
            let parsed = parse_import_uri(uri)?;
            let preset_id = parsed
                .get("preset")
                .or_else(|| parsed.get("presetId"))
                .and_then(Value::as_str)
                .unwrap_or("openai");
            let upserted = handle_rpc_local(
                conn,
                "upsertProvider",
                &json!({
                    "id": parsed.get("id").and_then(Value::as_str).unwrap_or(preset_id),
                    "presetId": preset_id,
                    "name": parsed.get("name").and_then(Value::as_str),
                    "baseUrl": parsed.get("base_url").or_else(|| parsed.get("baseUrl")).and_then(Value::as_str),
                    "apiKey": parsed.get("api_key").or_else(|| parsed.get("apiKey")).and_then(Value::as_str),
                    "defaultModel": parsed.get("model").or_else(|| parsed.get("defaultModel")).and_then(Value::as_str),
                }),
            )?;
            Ok(json!({
                "imported": parsed,
                "provider": upserted,
            }))
        }

        "importEnv" => {
            let env_text = params
                .get("envText")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let pairs = parse_dotenv_keys(env_text);
            let mut imported_ids = Vec::new();
            for (preset_id, api_key) in &pairs {
                if let Some(p) = MAGPIE_PRESETS.iter().find(|x| x.id == *preset_id) {
                    conn.execute(
                        "INSERT INTO magpie_providers
                        (id, preset_id, name, protocol, base_url, api_key, default_model, priority, weight, enabled, allow_no_key, input_cost_per_1m_usd, output_cost_per_1m_usd)
                        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 3, 100, 1, 0, ?8, ?9)
                        ON CONFLICT(id) DO UPDATE SET api_key = excluded.api_key, enabled = 1, cooldown_until_ms = 0, consecutive_failures = 0",
                        params![
                            p.id,
                            p.id,
                            p.name,
                            format!("{:?}", p.protocol),
                            p.base_url,
                            api_key,
                            p.default_model,
                            p.input_cost_per_1m_usd,
                            p.output_cost_per_1m_usd,
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                    imported_ids.push(p.id.to_string());
                }
            }
            Ok(json!({
                "importedCount": imported_ids.len(),
                "providerIds": imported_ids,
            }))
        }

        "discoverSubscriptions" => {
            let home_dir = params
                .get("homeDir")
                .and_then(Value::as_str)
                .map(std::path::PathBuf::from)
                .or_else(|| std::env::var("HOME").ok().map(std::path::PathBuf::from));
            if let Some(home) = home_dir {
                let reg = magpie_gateway::discover_subscriptions_in_home(&home, None);
                Ok(serde_json::to_value(reg).map_err(|e| e.to_string())?)
            } else {
                Ok(json!({ "accounts": [] }))
            }
        }

        "usageSummary" => {
            let (total_turns, in_tok, out_tok, total_cost): (i64, i64, i64, f64) = conn
                .query_row(
                    "SELECT COUNT(*), COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0), COALESCE(SUM(cost_usd), 0.0)
                     FROM magpie_usage_ledger",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .map_err(|e| e.to_string())?;
            Ok(json!({
                "totalTurns": total_turns,
                "inputTokens": in_tok,
                "outputTokens": out_tok,
                "totalCostUsd": (total_cost * 1_000_000.0).round() / 1_000_000.0,
            }))
        }

        other => Err(format!("Unsupported magpie method: {other}")),
    }
}
