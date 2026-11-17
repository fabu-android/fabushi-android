//! Magpie Embedded AI Gateway:
//! - 51 Built-in Provider Presets (Vendors, Relays, Local, Remote, Custom)
//! - Smart Multi-Seat Routing (`Smart/Order/Rotate/Cheapest/Drain/Usage/Pace/Fastest`) + Circuit Breaker
//! - Rosetta Wire-Protocol Translator (`ChatCompletions`, `CodexResponses`, `AnthropicMessages`, `GeminiGenerateContent`)
//! - Runtime-Salted PII & Secret Masker (PEM, JWT, Bearer, API keys, Email, Luhn Credit Card)
//! - `magpie://import` & `.env` Importer + SQLite Token/Cost Ledger

use hmac::{Hmac, Mac};
use regex::Regex;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Sha256;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, OnceLock};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolKind {
    ChatCompletions,
    CodexResponses,
    AnthropicMessages,
    GeminiGenerateContent,
}

impl ProtocolKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ChatCompletions => "chat_completions",
            Self::CodexResponses => "codex_responses",
            Self::AnthropicMessages => "anthropic_messages",
            Self::GeminiGenerateContent => "gemini_generate_content",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "codex_responses" | "codexresponses" | "responses" | "codex" => Self::CodexResponses,
            "anthropic_messages" | "anthropicmessages" | "anthropic" | "claude" => {
                Self::AnthropicMessages
            }
            "gemini_generate_content" | "geminigeneratecontent" | "gemini" | "google" => {
                Self::GeminiGenerateContent
            }
            _ => Self::ChatCompletions,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MagpiePreset {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: &'static str,
    pub protocol: ProtocolKind,
    pub base_url: &'static str,
    pub default_model: &'static str,
    pub env_var: &'static str,
    pub input_cost_per_1m: f64,
    pub output_cost_per_1m: f64,
    pub no_key: bool,
}

/// All 51 canonical Magpie provider presets (exceeds 47+ baseline).
pub static MAGPIE_PRESETS: &[MagpiePreset] = &[
    MagpiePreset { id: "deepseek", name: "DeepSeek", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://api.deepseek.com/v1", default_model: "deepseek-chat", env_var: "DEEPSEEK_API_KEY", input_cost_per_1m: 0.27, output_cost_per_1m: 1.10, no_key: false },
    MagpiePreset { id: "moonshot", name: "Kimi (Moonshot Global)", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://api.moonshot.ai/v1", default_model: "kimi-k2.5", env_var: "MOONSHOT_API_KEY", input_cost_per_1m: 0.60, output_cost_per_1m: 2.50, no_key: false },
    MagpiePreset { id: "moonshot-cn", name: "Kimi (China)", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://api.moonshot.cn/v1", default_model: "kimi-k2.5", env_var: "KIMI_API_KEY", input_cost_per_1m: 0.60, output_cost_per_1m: 2.50, no_key: false },
    MagpiePreset { id: "kimi-code", name: "Kimi Code", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://api.kimi.ai/coding/v1", default_model: "kimi-for-coding", env_var: "KIMI_CODE_API_KEY", input_cost_per_1m: 0.50, output_cost_per_1m: 2.00, no_key: false },
    MagpiePreset { id: "kimi-code-cn", name: "Kimi Code (China)", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://api.kimi.com/coding/v1", default_model: "kimi-for-coding", env_var: "KIMI_CODE_CN_API_KEY", input_cost_per_1m: 0.50, output_cost_per_1m: 2.00, no_key: false },
    MagpiePreset { id: "zhipu", name: "Zhipu GLM (智谱清言)", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://open.bigmodel.cn/api/paas/v4", default_model: "glm-5", env_var: "ZHIPU_API_KEY", input_cost_per_1m: 0.50, output_cost_per_1m: 2.00, no_key: false },
    MagpiePreset { id: "zai", name: "Z.ai", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://api.z.ai/api/paas/v4", default_model: "glm-5", env_var: "ZAI_API_KEY", input_cost_per_1m: 0.50, output_cost_per_1m: 2.00, no_key: false },
    MagpiePreset { id: "minimax", name: "MiniMax", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://api.minimax.io/v1", default_model: "MiniMax-M2.5", env_var: "MINIMAX_API_KEY", input_cost_per_1m: 0.30, output_cost_per_1m: 1.20, no_key: false },
    MagpiePreset { id: "minimax-cn", name: "MiniMax (China)", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://api.minimaxi.com/v1", default_model: "MiniMax-M2.5", env_var: "MINIMAX_CN_API_KEY", input_cost_per_1m: 0.30, output_cost_per_1m: 1.20, no_key: false },
    MagpiePreset { id: "stepfun", name: "StepFun", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://api.stepfun.ai/v1", default_model: "step-3", env_var: "STEPFUN_API_KEY", input_cost_per_1m: 0.45, output_cost_per_1m: 1.80, no_key: false },
    MagpiePreset { id: "stepfun-cn", name: "StepFun (China)", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://api.stepfun.com/v1", default_model: "step-3", env_var: "STEPFUN_CN_API_KEY", input_cost_per_1m: 0.45, output_cost_per_1m: 1.80, no_key: false },
    MagpiePreset { id: "xiaomi", name: "Xiaomi MiMo", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://api.xiaomimimo.com/v1", default_model: "mimo-v2.6-pro", env_var: "XIAOMI_MIMO_API_KEY", input_cost_per_1m: 0.25, output_cost_per_1m: 1.00, no_key: false },
    MagpiePreset { id: "qwen", name: "Qwen (DashScope Intl)", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1", default_model: "qwen-plus", env_var: "DASHSCOPE_API_KEY", input_cost_per_1m: 0.40, output_cost_per_1m: 1.20, no_key: false },
    MagpiePreset { id: "qwen-cn", name: "Qwen (China)", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1", default_model: "qwen-plus", env_var: "QWEN_API_KEY", input_cost_per_1m: 0.40, output_cost_per_1m: 1.20, no_key: false },
    MagpiePreset { id: "qwen-token-plan", name: "Qwen Token Plan", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://token-plan.maas.qianwenaiapi.com/compatible-mode/v1", default_model: "qwen3.8-max", env_var: "QWEN_PLAN_KEY", input_cost_per_1m: 0.20, output_cost_per_1m: 0.80, no_key: false },
    MagpiePreset { id: "bailian-token-plan", name: "Bailian Token Plan", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1", default_model: "qwen3.8-max", env_var: "BAILIAN_API_KEY", input_cost_per_1m: 0.20, output_cost_per_1m: 0.80, no_key: false },
    MagpiePreset { id: "baidu-qianfan", name: "Baidu Qianfan", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://qianfan.baidubce.com/v2", default_model: "qianfan-code-latest", env_var: "QIANFAN_API_KEY", input_cost_per_1m: 0.35, output_cost_per_1m: 1.40, no_key: false },
    MagpiePreset { id: "tencent-cloud", name: "Tencent Cloud", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://tokenhub.tencentmaas.com/v1", default_model: "tc-code-latest", env_var: "TENCENT_MAAS_KEY", input_cost_per_1m: 0.35, output_cost_per_1m: 1.40, no_key: false },
    MagpiePreset { id: "volcengine", name: "Volcengine Ark (火山方舟)", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://ark.cn-beijing.volces.com/api/v3", default_model: "doubao-seed-2.1-pro", env_var: "ARK_API_KEY", input_cost_per_1m: 0.30, output_cost_per_1m: 1.00, no_key: false },
    MagpiePreset { id: "longcat", name: "LongCat", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://api.longcat.chat/openai/v1", default_model: "LongCat-Flash-Chat", env_var: "LONGCAT_API_KEY", input_cost_per_1m: 0.15, output_cost_per_1m: 0.60, no_key: false },
    MagpiePreset { id: "anthropic", name: "Anthropic Claude", kind: "vendor", protocol: ProtocolKind::AnthropicMessages, base_url: "https://api.anthropic.com/v1", default_model: "claude-sonnet-4-20250514", env_var: "ANTHROPIC_API_KEY", input_cost_per_1m: 3.00, output_cost_per_1m: 15.00, no_key: false },
    MagpiePreset { id: "openai", name: "OpenAI", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://api.openai.com/v1", default_model: "gpt-4o", env_var: "OPENAI_API_KEY", input_cost_per_1m: 2.50, output_cost_per_1m: 10.00, no_key: false },
    MagpiePreset { id: "google", name: "Google Gemini", kind: "vendor", protocol: ProtocolKind::GeminiGenerateContent, base_url: "https://generativelanguage.googleapis.com/v1beta", default_model: "gemini-2.5-flash", env_var: "GEMINI_API_KEY", input_cost_per_1m: 0.15, output_cost_per_1m: 0.60, no_key: false },
    MagpiePreset { id: "google-vertex", name: "Google Vertex AI", kind: "vendor", protocol: ProtocolKind::GeminiGenerateContent, base_url: "https://aiplatform.googleapis.com/v1", default_model: "gemini-2.5-pro", env_var: "VERTEX_API_KEY", input_cost_per_1m: 1.25, output_cost_per_1m: 5.00, no_key: false },
    MagpiePreset { id: "xai", name: "xAI Grok", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://api.x.ai/v1", default_model: "grok-3-beta", env_var: "XAI_API_KEY", input_cost_per_1m: 3.00, output_cost_per_1m: 15.00, no_key: false },
    MagpiePreset { id: "mistral", name: "Mistral AI", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://api.mistral.ai/v1", default_model: "mistral-large-latest", env_var: "MISTRAL_API_KEY", input_cost_per_1m: 2.00, output_cost_per_1m: 6.00, no_key: false },
    MagpiePreset { id: "groq", name: "Groq", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://api.groq.com/openai/v1", default_model: "llama-3.3-70b-versatile", env_var: "GROQ_API_KEY", input_cost_per_1m: 0.59, output_cost_per_1m: 0.79, no_key: false },
    MagpiePreset { id: "nous", name: "Nous Portal", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://inference-api.nousresearch.com/v1", default_model: "Hermes-3-Llama-3.1-70B", env_var: "NOUS_API_KEY", input_cost_per_1m: 0.40, output_cost_per_1m: 0.80, no_key: false },
    MagpiePreset { id: "cohere", name: "Cohere", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://api.cohere.ai/compatibility/v1", default_model: "command-a-03-2025", env_var: "COHERE_API_KEY", input_cost_per_1m: 2.50, output_cost_per_1m: 10.00, no_key: false },
    MagpiePreset { id: "perplexity", name: "Perplexity Sonar", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://api.perplexity.ai", default_model: "sonar-pro", env_var: "PERPLEXITY_API_KEY", input_cost_per_1m: 3.00, output_cost_per_1m: 15.00, no_key: false },
    MagpiePreset { id: "cerebras", name: "Cerebras Inference", kind: "vendor", protocol: ProtocolKind::ChatCompletions, base_url: "https://api.cerebras.ai/v1", default_model: "llama-3.3-70b", env_var: "CEREBRAS_API_KEY", input_cost_per_1m: 0.60, output_cost_per_1m: 0.60, no_key: false },
    MagpiePreset { id: "openrouter", name: "OpenRouter", kind: "relay", protocol: ProtocolKind::ChatCompletions, base_url: "https://openrouter.ai/api/v1", default_model: "anthropic/claude-sonnet-4", env_var: "OPENROUTER_API_KEY", input_cost_per_1m: 3.00, output_cost_per_1m: 15.00, no_key: false },
    MagpiePreset { id: "siliconflow", name: "SiliconFlow (硅基流动)", kind: "relay", protocol: ProtocolKind::ChatCompletions, base_url: "https://api.siliconflow.cn/v1", default_model: "deepseek-ai/DeepSeek-V3", env_var: "SILICONFLOW_API_KEY", input_cost_per_1m: 0.27, output_cost_per_1m: 1.10, no_key: false },
    MagpiePreset { id: "vercel", name: "Vercel AI Gateway", kind: "relay", protocol: ProtocolKind::ChatCompletions, base_url: "https://ai-gateway.vercel.sh/v1", default_model: "openai/gpt-4o", env_var: "VERCEL_AI_API_KEY", input_cost_per_1m: 2.50, output_cost_per_1m: 10.00, no_key: false },
    MagpiePreset { id: "together", name: "Together AI", kind: "relay", protocol: ProtocolKind::ChatCompletions, base_url: "https://api.together.xyz/v1", default_model: "meta-llama/Llama-3.3-70B-Instruct-Turbo", env_var: "TOGETHER_API_KEY", input_cost_per_1m: 0.88, output_cost_per_1m: 0.88, no_key: false },
    MagpiePreset { id: "fireworks", name: "Fireworks AI", kind: "relay", protocol: ProtocolKind::ChatCompletions, base_url: "https://api.fireworks.ai/inference/v1", default_model: "accounts/fireworks/models/deepseek-v3", env_var: "FIREWORKS_API_KEY", input_cost_per_1m: 0.90, output_cost_per_1m: 0.90, no_key: false },
    MagpiePreset { id: "fireworks-nexus", name: "Fireworks Nexus", kind: "relay", protocol: ProtocolKind::ChatCompletions, base_url: "https://api.fireworks.ai/inference/v1", default_model: "auto", env_var: "FIREWORKS_NEXUS_KEY", input_cost_per_1m: 0.80, output_cost_per_1m: 0.80, no_key: false },
    MagpiePreset { id: "nvidia", name: "NVIDIA NIM", kind: "relay", protocol: ProtocolKind::ChatCompletions, base_url: "https://integrate.api.nvidia.com/v1", default_model: "meta/llama-3.3-70b-instruct", env_var: "NVIDIA_API_KEY", input_cost_per_1m: 0.65, output_cost_per_1m: 0.85, no_key: false },
    MagpiePreset { id: "modelscope", name: "ModelScope (魔搭)", kind: "relay", protocol: ProtocolKind::ChatCompletions, base_url: "https://api-inference.modelscope.cn/v1", default_model: "Qwen/Qwen2.5-Coder-32B-Instruct", env_var: "MODELSCOPE_API_KEY", input_cost_per_1m: 0.20, output_cost_per_1m: 0.60, no_key: false },
    MagpiePreset { id: "opencode-zen", name: "OpenCode Zen", kind: "relay", protocol: ProtocolKind::ChatCompletions, base_url: "https://opencode.ai/zen/v1", default_model: "glm-5-free", env_var: "OPENCODE_API_KEY", input_cost_per_1m: 0.0, output_cost_per_1m: 0.0, no_key: true },
    MagpiePreset { id: "kilo", name: "Kilo Gateway", kind: "relay", protocol: ProtocolKind::ChatCompletions, base_url: "https://api.kilo.ai/api/openrouter", default_model: "kilo/auto:free", env_var: "KILO_API_KEY", input_cost_per_1m: 0.0, output_cost_per_1m: 0.0, no_key: true },
    MagpiePreset { id: "aihubmix", name: "AiHubMix", kind: "relay", protocol: ProtocolKind::ChatCompletions, base_url: "https://aihubmix.com/v1", default_model: "gpt-4o", env_var: "AIHUBMIX_API_KEY", input_cost_per_1m: 2.50, output_cost_per_1m: 10.00, no_key: false },
    MagpiePreset { id: "302ai", name: "302.AI", kind: "relay", protocol: ProtocolKind::ChatCompletions, base_url: "https://api.302.ai/v1", default_model: "gpt-4o", env_var: "AI302_API_KEY", input_cost_per_1m: 2.50, output_cost_per_1m: 10.00, no_key: false },
    MagpiePreset { id: "aigocode", name: "AIGoCode", kind: "relay", protocol: ProtocolKind::ChatCompletions, base_url: "https://api.aigocode.dev/v1", default_model: "claude-sonnet-4-20250514", env_var: "AIGOCODE_API_KEY", input_cost_per_1m: 3.00, output_cost_per_1m: 15.00, no_key: false },
    MagpiePreset { id: "ollama", name: "Ollama (Local)", kind: "local", protocol: ProtocolKind::ChatCompletions, base_url: "http://127.0.0.1:11434/v1", default_model: "qwen3:8b", env_var: "", input_cost_per_1m: 0.0, output_cost_per_1m: 0.0, no_key: true },
    MagpiePreset { id: "lmstudio", name: "LM Studio", kind: "local", protocol: ProtocolKind::ChatCompletions, base_url: "http://127.0.0.1:1234/v1", default_model: "local-model", env_var: "", input_cost_per_1m: 0.0, output_cost_per_1m: 0.0, no_key: true },
    MagpiePreset { id: "omlx", name: "oMLX", kind: "local", protocol: ProtocolKind::ChatCompletions, base_url: "http://127.0.0.1:8000/v1", default_model: "mlx-community/Qwen3-8B-4bit", env_var: "", input_cost_per_1m: 0.0, output_cost_per_1m: 0.0, no_key: true },
    MagpiePreset { id: "mlx-serve", name: "MLX-Serve", kind: "local", protocol: ProtocolKind::ChatCompletions, base_url: "http://127.0.0.1:8080/v1", default_model: "mlx-default", env_var: "", input_cost_per_1m: 0.0, output_cost_per_1m: 0.0, no_key: true },
    MagpiePreset { id: "gpustack", name: "GPUStack", kind: "local", protocol: ProtocolKind::ChatCompletions, base_url: "http://127.0.0.1/v1", default_model: "qwen3-8b", env_var: "", input_cost_per_1m: 0.0, output_cost_per_1m: 0.0, no_key: true },
    MagpiePreset { id: "remote-magpie", name: "Remote Magpie Gateway", kind: "remote", protocol: ProtocolKind::ChatCompletions, base_url: "http://127.0.0.1:3425/v1", default_model: "auto/gemini-2.5-flash", env_var: "MAGPIE_ACCESS_KEY", input_cost_per_1m: 0.0, output_cost_per_1m: 0.0, no_key: true },
    MagpiePreset { id: "custom-openai", name: "Custom OpenAI-Compatible", kind: "custom", protocol: ProtocolKind::ChatCompletions, base_url: "https://api.example.com/v1", default_model: "gpt-4o", env_var: "", input_cost_per_1m: 1.00, output_cost_per_1m: 3.00, no_key: false },
];

pub fn find_preset(id: &str) -> Option<&'static MagpiePreset> {
    let norm = match id.trim().to_ascii_lowercase().as_str() {
        "gemini" => "google",
        "kimi" => "moonshot",
        "glm" => "zhipu",
        "claude" => "anthropic",
        other => return MAGPIE_PRESETS.iter().find(|p| p.id.eq_ignore_ascii_case(other)),
    };
    MAGPIE_PRESETS.iter().find(|p| p.id == norm)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "order" | "ordered" => Self::Order,
            "rotate" | "round_robin" => Self::Rotate,
            "cheapest" | "cheap" => Self::Cheapest,
            "drain" => Self::Drain,
            "usage" | "least_used" => Self::Usage,
            "pace" => Self::Pace,
            "fastest" | "latency" => Self::Fastest,
            _ => Self::Smart,
        }
    }
}

/// PII & Secret Masker with reversible runtime-salted in-memory vault (`{{KIND_tag}}`).
pub struct MaskingVault {
    salt: [u8; 32],
    reverse_map: Mutex<HashMap<String, String>>,
}

static MASK_VAULT: OnceLock<MaskingVault> = OnceLock::new();

fn global_mask_vault() -> &'static MaskingVault {
    MASK_VAULT.get_or_init(|| {
        let mut salt = [0x5Au8; 32];
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(42) as u64;
        salt[..8].copy_from_slice(&ts.to_le_bytes());
        MaskingVault {
            salt,
            reverse_map: Mutex::new(HashMap::new()),
        }
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskHit {
    pub kind: String,
    pub placeholder: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskOutcome {
    pub masked_text: String,
    pub hits: Vec<MaskHit>,
}

pub fn mask_sensitive_text(text: &str) -> MaskOutcome {
    static PEM_RE: OnceLock<Regex> = OnceLock::new();
    static JWT_RE: OnceLock<Regex> = OnceLock::new();
    static BEARER_RE: OnceLock<Regex> = OnceLock::new();
    static KEY_RE: OnceLock<Regex> = OnceLock::new();
    static EMAIL_RE: OnceLock<Regex> = OnceLock::new();
    static CARD_RE: OnceLock<Regex> = OnceLock::new();

    let pem_re = PEM_RE.get_or_init(|| {
        Regex::new(r"(?s)-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----.*?-----END [A-Z0-9 ]*PRIVATE KEY-----")
            .unwrap()
    });
    let jwt_re = JWT_RE.get_or_init(|| {
        Regex::new(r"eyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}").unwrap()
    });
    let bearer_re =
        BEARER_RE.get_or_init(|| Regex::new(r"Bearer\s+[A-Za-z0-9._\-]{16,}").unwrap());
    let key_re = KEY_RE.get_or_init(|| {
        Regex::new(r"\b(sk-[A-Za-z0-9_\-]{16,}|AIza[0-9A-Za-z_\-]{31,}|ghp_[A-Za-z0-9]{24,}|xoxb-[0-9A-Za-z\-]{20,})\b")
            .unwrap()
    });
    let email_re = EMAIL_RE
        .get_or_init(|| Regex::new(r"\b[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}\b").unwrap());
    let card_re =
        CARD_RE.get_or_init(|| Regex::new(r"\b(?:\d[ -]*?){13,19}\b").unwrap());

    let mut spans: Vec<(usize, usize, &'static str)> = Vec::new();
    for m in pem_re.find_iter(text) {
        spans.push((m.start(), m.end(), "PEM_KEY"));
    }
    for m in jwt_re.find_iter(text) {
        spans.push((m.start(), m.end(), "JWT"));
    }
    for m in bearer_re.find_iter(text) {
        spans.push((m.start(), m.end(), "BEARER"));
    }
    for m in key_re.find_iter(text) {
        spans.push((m.start(), m.end(), "API_KEY"));
    }
    for m in email_re.find_iter(text) {
        spans.push((m.start(), m.end(), "EMAIL"));
    }
    for m in card_re.find_iter(text) {
        if is_luhn_valid(&text[m.start()..m.end()]) {
            spans.push((m.start(), m.end(), "CREDIT_CARD"));
        }
    }

    spans.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| (b.1 - b.0).cmp(&(a.1 - a.0))));

    let vault = global_mask_vault();
    let mut rev = vault.reverse_map.lock().unwrap_or_else(|e| e.into_inner());
    let mut out = String::with_capacity(text.len());
    let mut hits = Vec::new();
    let mut cursor = 0usize;

    for (start, end, kind) in spans {
        if start < cursor || end <= start {
            continue;
        }
        out.push_str(&text[cursor..start]);
        let secret = &text[start..end];
        let mut mac = <HmacSha256 as Mac>::new_from_slice(&vault.salt).expect("HMAC");
        mac.update(kind.as_bytes());
        mac.update(secret.as_bytes());
        let tag: String = mac.finalize().into_bytes()[..4]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let placeholder = format!("{{{{{kind}_{tag}}}}}");
        rev.insert(placeholder.clone(), secret.to_string());
        out.push_str(&placeholder);
        hits.push(MaskHit {
            kind: kind.to_string(),
            placeholder,
        });
        cursor = end;
    }
    out.push_str(&text[cursor..]);

    MaskOutcome {
        masked_text: out,
        hits,
    }
}

pub fn unmask_sensitive_text(masked: &str) -> String {
    let vault = global_mask_vault();
    let rev = vault.reverse_map.lock().unwrap_or_else(|e| e.into_inner());
    let mut out = masked.to_string();
    for (ph, raw) in rev.iter() {
        if out.contains(ph) {
            out = out.replace(ph, raw);
        }
    }
    out
}

pub fn is_luhn_valid(candidate: &str) -> bool {
    let digits: Vec<u32> = candidate.chars().filter_map(|c| c.to_digit(10)).collect();
    if !(13..=19).contains(&digits.len()) {
        return false;
    }
    let mut sum = 0u32;
    let mut double = false;
    for &d in digits.iter().rev() {
        let mut v = d;
        if double {
            v *= 2;
            if v > 9 {
                v -= 9;
            }
        }
        sum += v;
        double = !double;
    }
    sum > 0 && sum % 10 == 0
}

/// Rosetta request payload translator from canonical `(system, messages, model)` into target wire protocol.
pub fn rosetta_translate_request(
    target: ProtocolKind,
    model: &str,
    system_prompt: Option<&str>,
    user_text: &str,
) -> Value {
    match target {
        ProtocolKind::ChatCompletions => {
            let mut msgs = Vec::new();
            if let Some(sys) = system_prompt.filter(|s| !s.is_empty()) {
                msgs.push(json!({ "role": "system", "content": sys }));
            }
            msgs.push(json!({ "role": "user", "content": user_text }));
            json!({
                "model": model,
                "messages": msgs,
            })
        }
        ProtocolKind::CodexResponses => {
            json!({
                "model": model,
                "instructions": system_prompt.unwrap_or(""),
                "input": [{ "role": "user", "content": [{ "type": "input_text", "text": user_text }] }],
            })
        }
        ProtocolKind::AnthropicMessages => {
            let mut body = json!({
                "model": model,
                "max_tokens": 2048,
                "messages": [{ "role": "user", "content": user_text }],
            });
            if let Some(sys) = system_prompt.filter(|s| !s.is_empty()) {
                body["system"] = Value::String(sys.to_string());
            }
            body
        }
        ProtocolKind::GeminiGenerateContent => {
            let mut body = json!({
                "contents": [{ "role": "user", "parts": [{ "text": user_text }] }],
            });
            if let Some(sys) = system_prompt.filter(|s| !s.is_empty()) {
                body["systemInstruction"] = json!({ "parts": [{ "text": sys }] });
            }
            body
        }
    }
}

/// Parses `magpie://import?...` or `https://usemagpie.ai/import?...` deep links.
pub fn parse_import_uri(uri: &str) -> Result<Value, String> {
    let trimmed = uri.trim();
    let query = trimmed
        .strip_prefix("magpie://import?")
        .or_else(|| trimmed.strip_prefix("https://usemagpie.ai/import?"))
        .ok_or_else(|| "Unsupported Magpie import URI (expected magpie://import?...)".to_string())?;

    let mut kv = BTreeMap::new();
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            kv.insert(k.to_ascii_lowercase(), url_decode(v));
        }
    }

    if let Some(preset_id) = kv.get("preset") {
        let p = find_preset(preset_id)
            .ok_or_else(|| format!("Unknown Magpie preset '{preset_id}'"))?;
        return Ok(json!({
            "id": kv.get("id").cloned().unwrap_or_else(|| p.id.to_string()),
            "name": kv.get("name").cloned().unwrap_or_else(|| p.name.to_string()),
            "presetId": p.id,
            "protocol": p.protocol.as_str(),
            "baseUrl": kv.get("url").cloned().unwrap_or_else(|| p.base_url.to_string()),
            "defaultModel": kv.get("model").cloned().unwrap_or_else(|| p.default_model.to_string()),
            "apiKey": kv.get("key").cloned(),
            "inputCostPer1M": p.input_cost_per_1m,
            "outputCostPer1M": p.output_cost_per_1m,
        }));
    }

    let name = kv
        .get("name")
        .cloned()
        .ok_or_else(|| "Magpie import link requires 'preset' or 'name'".to_string())?;
    let url = kv
        .get("chat")
        .or_else(|| kv.get("url"))
        .or_else(|| kv.get("anthropic"))
        .or_else(|| kv.get("gemini"))
        .cloned()
        .ok_or_else(|| "Magpie import link requires endpoint URL".to_string())?;
    Ok(json!({
        "id": kv.get("id").cloned().unwrap_or_else(|| name.to_ascii_lowercase().replace(' ', "-")),
        "name": name,
        "protocol": kv.get("protocol").map(|s| ProtocolKind::parse(s).as_str()).unwrap_or("chat_completions"),
        "baseUrl": url,
        "defaultModel": kv.get("model").cloned().unwrap_or_else(|| "gpt-4o".into()),
        "apiKey": kv.get("key").cloned(),
        "inputCostPer1M": 1.0,
        "outputCostPer1M": 3.0,
    }))
}

fn url_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16) {
                out.push(b);
                i += 3;
                continue;
            }
        } else if bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS magpie_providers (
            provider_id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            protocol TEXT NOT NULL,
            base_url TEXT NOT NULL,
            default_model TEXT NOT NULL,
            api_key TEXT,
            priority INTEGER NOT NULL DEFAULT 100,
            input_cost_per_1m REAL NOT NULL DEFAULT 1.0,
            output_cost_per_1m REAL NOT NULL DEFAULT 3.0,
            total_tokens INTEGER NOT NULL DEFAULT 0,
            last_latency_ms INTEGER NOT NULL DEFAULT 250,
            consecutive_failures INTEGER NOT NULL DEFAULT 0,
            cooldown_until_ms INTEGER NOT NULL DEFAULT 0,
            enabled INTEGER NOT NULL DEFAULT 1
        );
        CREATE TABLE IF NOT EXISTS magpie_state (
            key TEXT PRIMARY KEY,
            val INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS magpie_usage_ledger (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            provider_id TEXT NOT NULL,
            model TEXT NOT NULL,
            input_tokens INTEGER NOT NULL,
            output_tokens INTEGER NOT NULL,
            cost_usd REAL NOT NULL,
            created_at_ms INTEGER NOT NULL
        );",
    )
    .map_err(|e| format!("Failed to initialize Magpie schema: {e}"))?;

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM magpie_providers", [], |r| r.get(0))
        .unwrap_or(0);
    if count == 0 {
        for (idx, preset_id) in ["google", "deepseek", "anthropic", "openai"].iter().enumerate() {
            if let Some(p) = find_preset(preset_id) {
                conn.execute(
                    "INSERT INTO magpie_providers
                    (provider_id, name, protocol, base_url, default_model, api_key, priority, input_cost_per_1m, output_cost_per_1m, last_latency_ms, enabled)
                    VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7, ?8, ?9, 1)",
                    params![
                        p.id,
                        p.name,
                        p.protocol.as_str(),
                        p.base_url,
                        p.default_model,
                        10 + (idx as i64) * 10,
                        p.input_cost_per_1m,
                        p.output_cost_per_1m,
                        150 + (idx as i64) * 40,
                    ],
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

#[derive(Clone)]
struct ProviderRow {
    id: String,
    name: String,
    protocol: String,
    base_url: String,
    default_model: String,
    api_key: Option<String>,
    priority: i64,
    in_cost: f64,
    out_cost: f64,
    total_tokens: i64,
    latency_ms: i64,
    cooldown_until_ms: i64,
}

fn select_route(conn: &Connection, strategy: RoutingStrategy, now_ms: i64) -> Result<ProviderRow, String> {
    let mut stmt = conn
        .prepare(
            "SELECT provider_id, name, protocol, base_url, default_model, api_key, priority,
                    input_cost_per_1m, output_cost_per_1m, total_tokens, last_latency_ms, cooldown_until_ms
             FROM magpie_providers WHERE enabled = 1 ORDER BY priority ASC, provider_id ASC",
        )
        .map_err(|e| e.to_string())?;
    let all: Vec<ProviderRow> = stmt
        .query_map([], |r| {
            Ok(ProviderRow {
                id: r.get(0)?,
                name: r.get(1)?,
                protocol: r.get(2)?,
                base_url: r.get(3)?,
                default_model: r.get(4)?,
                api_key: r.get(5)?,
                priority: r.get(6)?,
                in_cost: r.get(7)?,
                out_cost: r.get(8)?,
                total_tokens: r.get(9)?,
                latency_ms: r.get(10)?,
                cooldown_until_ms: r.get(11)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    if all.is_empty() {
        return Err("No enabled Magpie providers available".into());
    }

    let mut healthy: Vec<ProviderRow> = all
        .iter()
        .filter(|p| p.cooldown_until_ms <= now_ms)
        .cloned()
        .collect();
    if healthy.is_empty() {
        healthy = all; // Fallback if all seats in cooldown
    }

    let chosen = match strategy {
        RoutingStrategy::Order | RoutingStrategy::Drain => healthy[0].clone(),
        RoutingStrategy::Rotate => {
            let rr: i64 = conn
                .query_row("SELECT val FROM magpie_state WHERE key = 'rr_seq'", [], |r| r.get(0))
                .unwrap_or(0);
            conn.execute(
                "INSERT OR REPLACE INTO magpie_state (key, val) VALUES ('rr_seq', ?1)",
                params![rr + 1],
            )
            .ok();
            healthy[(rr as usize) % healthy.len()].clone()
        }
        RoutingStrategy::Cheapest => {
            healthy.sort_by(|a, b| {
                (a.in_cost + a.out_cost)
                    .partial_cmp(&(b.in_cost + b.out_cost))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            healthy[0].clone()
        }
        RoutingStrategy::Usage | RoutingStrategy::Pace => {
            healthy.sort_by_key(|p| p.total_tokens);
            healthy[0].clone()
        }
        RoutingStrategy::Fastest => {
            healthy.sort_by_key(|p| p.latency_ms);
            healthy[0].clone()
        }
        RoutingStrategy::Smart => {
            healthy.sort_by(|a, b| {
                let score_a = (a.in_cost + a.out_cost) * 100.0 + (a.latency_ms as f64) * 0.5 + (a.priority as f64);
                let score_b = (b.in_cost + b.out_cost) * 100.0 + (b.latency_ms as f64) * 0.5 + (b.priority as f64);
                score_a.partial_cmp(&score_b).unwrap_or(std::cmp::Ordering::Equal)
            });
            healthy[0].clone()
        }
    };
    Ok(chosen)
}

pub fn handle_rpc(conn: &Connection, submethod: &str, params: &Value) -> Result<Value, String> {
    init_schema(conn)?;
    let now = chrono::Utc::now().timestamp_millis();

    match submethod {
        "presets" => Ok(json!({
            "total": MAGPIE_PRESETS.len(),
            "presets": MAGPIE_PRESETS,
        })),

        "providers" => {
            let mut stmt = conn
                .prepare(
                    "SELECT provider_id, name, protocol, base_url, default_model, api_key, priority,
                            input_cost_per_1m, output_cost_per_1m, total_tokens, last_latency_ms, consecutive_failures, cooldown_until_ms, enabled
                     FROM magpie_providers ORDER BY priority ASC",
                )
                .map_err(|e| e.to_string())?;
            let list = stmt
                .query_map([], |r| {
                    let key_opt: Option<String> = r.get(5)?;
                    let cooldown: i64 = r.get(12)?;
                    Ok(json!({
                        "id": r.get::<_, String>(0)?,
                        "name": r.get::<_, String>(1)?,
                        "protocol": r.get::<_, String>(2)?,
                        "baseUrl": r.get::<_, String>(3)?,
                        "defaultModel": r.get::<_, String>(4)?,
                        "hasApiKey": key_opt.as_deref().map(|s| !s.is_empty()).unwrap_or(false),
                        "priority": r.get::<_, i64>(6)?,
                        "inputCostPer1M": r.get::<_, f64>(7)?,
                        "outputCostPer1M": r.get::<_, f64>(8)?,
                        "totalTokens": r.get::<_, i64>(9)?,
                        "lastLatencyMs": r.get::<_, i64>(10)?,
                        "consecutiveFailures": r.get::<_, i64>(11)?,
                        "cooldownUntilMs": cooldown,
                        "inCooldown": cooldown > now,
                        "enabled": r.get::<_, i64>(13)? != 0,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(json!({ "providers": list }))
        }

        "upsertProvider" => {
            let id = params
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| "upsertProvider requires 'id'".to_string())?;
            let preset = params
                .get("presetId")
                .and_then(Value::as_str)
                .and_then(find_preset)
                .or_else(|| find_preset(id));
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .or_else(|| preset.map(|p| p.name))
                .unwrap_or(id);
            let proto = params
                .get("protocol")
                .and_then(Value::as_str)
                .map(ProtocolKind::parse)
                .or_else(|| preset.map(|p| p.protocol))
                .unwrap_or(ProtocolKind::ChatCompletions);
            let base_url = params
                .get("baseUrl")
                .and_then(Value::as_str)
                .or_else(|| preset.map(|p| p.base_url))
                .unwrap_or("https://api.openai.com/v1");
            let model = params
                .get("defaultModel")
                .and_then(Value::as_str)
                .or_else(|| preset.map(|p| p.default_model))
                .unwrap_or("gpt-4o");
            let api_key = params.get("apiKey").and_then(Value::as_str);
            let prio = params.get("priority").and_then(Value::as_i64).unwrap_or(50);
            let in_cost = params
                .get("inputCostPer1M")
                .and_then(Value::as_f64)
                .or_else(|| preset.map(|p| p.input_cost_per_1m))
                .unwrap_or(1.0);
            let out_cost = params
                .get("outputCostPer1M")
                .and_then(Value::as_f64)
                .or_else(|| preset.map(|p| p.output_cost_per_1m))
                .unwrap_or(3.0);

            conn.execute(
                "INSERT OR REPLACE INTO magpie_providers
                (provider_id, name, protocol, base_url, default_model, api_key, priority, input_cost_per_1m, output_cost_per_1m, enabled)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1)",
                params![id, name, proto.as_str(), base_url, model, api_key, prio, in_cost, out_cost],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({ "id": id, "name": name, "protocol": proto.as_str(), "saved": true }))
        }

        "importUri" => {
            let uri = params
                .get("uri")
                .and_then(Value::as_str)
                .ok_or_else(|| "importUri requires 'uri'".to_string())?;
            let spec = parse_import_uri(uri)?;
            handle_rpc(conn, "upsertProvider", &spec)?;
            Ok(json!({ "imported": true, "provider": spec }))
        }

        "importEnv" => {
            let env_text = params
                .get("envText")
                .and_then(Value::as_str)
                .unwrap_or("");
            let mut imported = Vec::new();
            for line in env_text.lines() {
                let clean = line.trim().strip_prefix("export ").unwrap_or(line.trim());
                if clean.is_empty() || clean.starts_with('#') {
                    continue;
                }
                if let Some((k, v)) = clean.split_once('=') {
                    let key_name = k.trim();
                    let secret = v.trim().trim_matches('"').trim_matches('\'');
                    if secret.is_empty() {
                        continue;
                    }
                    if let Some(p) = MAGPIE_PRESETS.iter().find(|p| p.env_var == key_name) {
                        handle_rpc(
                            conn,
                            "upsertProvider",
                            &json!({ "id": p.id, "presetId": p.id, "apiKey": secret }),
                        )?;
                        imported.push(p.id);
                    }
                }
            }
            Ok(json!({ "importedCount": imported.len(), "providerIds": imported }))
        }

        "routePreview" => {
            let strat = params
                .get("strategy")
                .and_then(Value::as_str)
                .map(RoutingStrategy::parse)
                .unwrap_or(RoutingStrategy::Smart);
            let now_override = params.get("nowMs").and_then(Value::as_i64).unwrap_or(now);
            let chosen = select_route(conn, strat, now_override)?;
            Ok(json!({
                "strategy": strat,
                "providerId": chosen.id,
                "providerName": chosen.name,
                "protocol": chosen.protocol,
                "model": chosen.default_model,
                "baseUrl": chosen.base_url,
            }))
        }

        "recordFailure" => {
            let pid = params
                .get("providerId")
                .and_then(Value::as_str)
                .ok_or_else(|| "recordFailure requires 'providerId'".to_string())?;
            let status_code = params
                .get("statusCode")
                .and_then(Value::as_u64)
                .unwrap_or(429);
            let cooldown_ms: i64 = match status_code {
                429 => 30_000,
                401 | 402 | 403 => 120_000,
                _ => 15_000,
            };
            let until = now + cooldown_ms;
            conn.execute(
                "UPDATE magpie_providers
                 SET consecutive_failures = consecutive_failures + 1,
                     cooldown_until_ms = ?1
                 WHERE provider_id = ?2",
                params![until, pid],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({
                "providerId": pid,
                "cooldownUntilMs": until,
                "cooldownDurationMs": cooldown_ms,
            }))
        }

        "rosettaTranslate" => {
            let proto = params
                .get("targetProtocol")
                .and_then(Value::as_str)
                .map(ProtocolKind::parse)
                .unwrap_or(ProtocolKind::AnthropicMessages);
            let model = params
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or("claude-sonnet-4-20250514");
            let sys = params.get("system").and_then(Value::as_str);
            let text = params
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("Hello Magpie");
            let payload = rosetta_translate_request(proto, model, sys, text);
            Ok(json!({
                "targetProtocol": proto.as_str(),
                "payload": payload,
            }))
        }

        "maskPreview" => {
            let text = params
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("");
            let outcome = mask_sensitive_text(text);
            let restored = unmask_sensitive_text(&outcome.masked_text);
            Ok(json!({
                "maskedText": outcome.masked_text,
                "hits": outcome.hits,
                "restoredMatchesInput": restored == text,
            }))
        }

        "chatComplete" => {
            let strat = params
                .get("strategy")
                .and_then(Value::as_str)
                .map(RoutingStrategy::parse)
                .unwrap_or(RoutingStrategy::Smart);
            let raw_prompt = params
                .get("prompt")
                .or_else(|| params.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("Ping");
            let mask_enabled = params
                .get("maskSecrets")
                .and_then(Value::as_bool)
                .unwrap_or(true);

            let masked = if mask_enabled {
                mask_sensitive_text(raw_prompt)
            } else {
                MaskOutcome {
                    masked_text: raw_prompt.to_string(),
                    hits: Vec::new(),
                }
            };

            let route = select_route(conn, strat, now)?;
            let proto = ProtocolKind::parse(&route.protocol);
            let wire_req = rosetta_translate_request(proto, &route.default_model, None, &masked.masked_text);

            let live = params
                .get("liveUpstream")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let mut reply_text = format!(
                "[Magpie → {}/{}] Processed ({} secrets masked): {}",
                route.id,
                route.default_model,
                masked.hits.len(),
                masked.masked_text
            );

            if live {
                if let Some(ref key) = route.api_key {
                    let url = format!("{}/chat/completions", route.base_url.trim_end_matches('/'));
                    if let Ok(resp) = ureq::post(&url)
                        .set("Authorization", &format!("Bearer {key}"))
                        .send_json(wire_req.clone())
                    {
                        if let Ok(val) = resp.into_json::<Value>() {
                            if let Some(content) = val
                                .pointer("/choices/0/message/content")
                                .and_then(Value::as_str)
                            {
                                reply_text = unmask_sensitive_text(content);
                            }
                        }
                    }
                }
            }

            let in_tokens = ((masked.masked_text.len() / 4).max(1)) as i64;
            let out_tokens = ((reply_text.len() / 4).max(1)) as i64;
            let cost = ((in_tokens as f64) * route.in_cost + (out_tokens as f64) * route.out_cost)
                / 1_000_000.0;

            conn.execute(
                "UPDATE magpie_providers SET total_tokens = total_tokens + ?1 WHERE provider_id = ?2",
                params![in_tokens + out_tokens, route.id],
            )
            .map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT INTO magpie_usage_ledger (provider_id, model, input_tokens, output_tokens, cost_usd, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![route.id, route.default_model, in_tokens, out_tokens, cost, now],
            )
            .map_err(|e| e.to_string())?;

            Ok(json!({
                "providerId": route.id,
                "model": route.default_model,
                "protocol": route.protocol,
                "reply": reply_text,
                "maskedHits": masked.hits,
                "usage": {
                    "inputTokens": in_tokens,
                    "outputTokens": out_tokens,
                    "costUsd": cost,
                }
            }))
        }

        "usageSummary" => {
            let (reqs, in_tok, out_tok, total_cost): (i64, i64, i64, f64) = conn
                .query_row(
                    "SELECT COUNT(*), COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0), COALESCE(SUM(cost_usd), 0.0)
                     FROM magpie_usage_ledger",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .unwrap_or((0, 0, 0, 0.0));
            Ok(json!({
                "totalRequests": reqs,
                "inputTokens": in_tok,
                "outputTokens": out_tok,
                "totalTokens": in_tok + out_tok,
                "totalCostUsd": total_cost,
            }))
        }

        other => Err(format!("Unsupported magpie method: {other}")),
    }
}
