use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// High-level category grouping for a Magpie provider preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresetKind {
    Vendor,
    Relay,
    Local,
    Remote,
    Custom,
}

impl PresetKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Vendor => "vendor",
            Self::Relay => "relay",
            Self::Local => "local",
            Self::Remote => "remote",
            Self::Custom => "custom",
        }
    }
}

/// Wire protocol family spoken by an upstream provider endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "chat_completions" | "chat" | "openai" => Some(Self::ChatCompletions),
            "codex_responses" | "responses" | "codex" => Some(Self::CodexResponses),
            "anthropic_messages" | "anthropic" | "messages" | "claude" => {
                Some(Self::AnthropicMessages)
            }
            "gemini_generate_content" | "gemini" | "google_gemini" | "generate_content" => {
                Some(Self::GeminiGenerateContent)
            }
            _ => None,
        }
    }
}

/// Regional or plan-specific base-URL override option for a preset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresetRegion {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub responses_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anthropic_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gemini_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keys_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
    #[serde(default)]
    pub models: Vec<String>,
}

/// Built-in Magpie provider preset descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MagpiePreset {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short_name: Option<String>,
    pub icon: String,
    pub kind: PresetKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub responses_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anthropic_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gemini_url: Option<String>,
    pub default_protocol: ProtocolKind,
    pub supported_protocols: Vec<ProtocolKind>,
    pub default_model: String,
    #[serde(default)]
    pub suggested_models: Vec<String>,
    #[serde(default)]
    pub no_key: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region_label: Option<String>,
    #[serde(default)]
    pub key_plans: bool,
    #[serde(default)]
    pub split_accounts: bool,
    #[serde(default)]
    pub regions: Vec<PresetRegion>,
    #[serde(default)]
    pub header_hints: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keys_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog: Option<String>,
    #[serde(default)]
    pub env_var_hints: Vec<String>,
}

/// Resolved base URL, protocol, and model metadata after applying region/protocol choices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedPresetEndpoint {
    pub preset_id: String,
    pub display_name: String,
    pub region_id: Option<String>,
    pub protocol: ProtocolKind,
    pub base_url: String,
    pub default_model: String,
    pub suggested_models: Vec<String>,
    pub no_key: bool,
    pub website: Option<String>,
    pub keys_url: Option<String>,
}

impl MagpiePreset {
    /// Resolve the best endpoint URL and protocol for an optional region and protocol preference.
    pub fn resolve_endpoint(
        &self,
        region_id: Option<&str>,
        preferred_protocol: Option<ProtocolKind>,
    ) -> Option<ResolvedPresetEndpoint> {
        let region = region_id
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .and_then(|rid| {
                let mapped = canonical_region_id(&self.id, rid);
                self.regions
                    .iter()
                    .find(|r| r.id.eq_ignore_ascii_case(mapped))
            })
            .or_else(|| self.regions.first());

        let chat = region
            .and_then(|r| r.chat_url.as_ref())
            .or(self.chat_url.as_ref());
        let responses = region
            .and_then(|r| r.responses_url.as_ref())
            .or(self.responses_url.as_ref());
        let anthropic = region
            .and_then(|r| r.anthropic_url.as_ref())
            .or(self.anthropic_url.as_ref());
        let gemini = region
            .and_then(|r| r.gemini_url.as_ref())
            .or(self.gemini_url.as_ref());

        let pick = |proto: ProtocolKind| -> Option<(ProtocolKind, String)> {
            match proto {
                ProtocolKind::ChatCompletions => chat.cloned().map(|u| (proto, u)),
                ProtocolKind::CodexResponses => responses.cloned().map(|u| (proto, u)),
                ProtocolKind::AnthropicMessages => anthropic.cloned().map(|u| (proto, u)),
                ProtocolKind::GeminiGenerateContent => gemini.cloned().map(|u| (proto, u)),
            }
        };

        let (protocol, base_url) = preferred_protocol
            .and_then(pick)
            .or_else(|| pick(self.default_protocol))
            .or_else(|| pick(ProtocolKind::ChatCompletions))
            .or_else(|| pick(ProtocolKind::AnthropicMessages))
            .or_else(|| pick(ProtocolKind::GeminiGenerateContent))
            .or_else(|| pick(ProtocolKind::CodexResponses))?;

        let mut models = region
            .map(|r| r.models.clone())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| self.suggested_models.clone());
        let default_model = models
            .first()
            .cloned()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.default_model.clone());
        if !default_model.is_empty() && !models.iter().any(|m| m == &default_model) {
            models.insert(0, default_model.clone());
        }

        Some(ResolvedPresetEndpoint {
            preset_id: self.id.clone(),
            display_name: self.name.clone(),
            region_id: region.map(|r| r.id.clone()),
            protocol,
            base_url,
            default_model,
            suggested_models: models,
            no_key: self.no_key,
            website: region
                .and_then(|r| r.website.clone())
                .or_else(|| self.website.clone()),
            keys_url: region
                .and_then(|r| r.keys_url.clone())
                .or_else(|| self.keys_url.clone()),
        })
    }
}

struct PresetBuilder(MagpiePreset);

impl PresetBuilder {
    fn new(id: &str, name: &str, icon: &str, kind: PresetKind, default_model: &str) -> Self {
        Self(MagpiePreset {
            id: id.to_string(),
            name: name.to_string(),
            short_name: None,
            icon: icon.to_string(),
            kind,
            chat_url: None,
            responses_url: None,
            anthropic_url: None,
            gemini_url: None,
            default_protocol: ProtocolKind::ChatCompletions,
            supported_protocols: Vec::new(),
            default_model: default_model.to_string(),
            suggested_models: vec![default_model.to_string()],
            no_key: false,
            key_hint: None,
            region_label: None,
            key_plans: false,
            split_accounts: false,
            regions: Vec::new(),
            header_hints: Vec::new(),
            website: None,
            keys_url: None,
            note: None,
            catalog: None,
            env_var_hints: Vec::new(),
        })
    }

    fn short(mut self, s: &str) -> Self {
        self.0.short_name = Some(s.to_string());
        self
    }

    fn chat(mut self, url: &str) -> Self {
        self.0.chat_url = Some(url.to_string());
        self.push_proto(ProtocolKind::ChatCompletions);
        self
    }

    fn responses(mut self, url: &str) -> Self {
        self.0.responses_url = Some(url.to_string());
        self.push_proto(ProtocolKind::CodexResponses);
        self
    }

    fn anthropic(mut self, url: &str) -> Self {
        self.0.anthropic_url = Some(url.to_string());
        self.push_proto(ProtocolKind::AnthropicMessages);
        self
    }

    fn gemini(mut self, url: &str) -> Self {
        self.0.gemini_url = Some(url.to_string());
        self.push_proto(ProtocolKind::GeminiGenerateContent);
        self
    }

    fn default_proto(mut self, proto: ProtocolKind) -> Self {
        self.0.default_protocol = proto;
        self.push_proto(proto);
        self
    }

    fn push_proto(&mut self, proto: ProtocolKind) {
        if !self.0.supported_protocols.contains(&proto) {
            self.0.supported_protocols.push(proto);
        }
    }

    fn models(mut self, list: &[&str]) -> Self {
        self.0.suggested_models = list.iter().map(|s| (*s).to_string()).collect();
        self
    }

    fn no_key(mut self, hint: Option<&str>) -> Self {
        self.0.no_key = true;
        self.0.key_hint = hint.map(ToString::to_string);
        self
    }

    fn region_label(mut self, label: &str) -> Self {
        self.0.region_label = Some(label.to_string());
        self
    }

    fn key_plans(mut self) -> Self {
        self.0.key_plans = true;
        self
    }

    fn split_accounts(mut self) -> Self {
        self.0.split_accounts = true;
        self
    }

    fn regions(mut self, regs: Vec<PresetRegion>) -> Self {
        self.0.regions = regs;
        self
    }

    fn headers(mut self, hints: &[&str]) -> Self {
        self.0.header_hints = hints.iter().map(|s| (*s).to_string()).collect();
        self
    }

    fn links(mut self, website: &str, keys_url: &str) -> Self {
        self.0.website = Some(website.to_string());
        if !keys_url.is_empty() {
            self.0.keys_url = Some(keys_url.to_string());
        }
        self
    }

    fn note(mut self, note: &str) -> Self {
        self.0.note = Some(note.to_string());
        self
    }

    fn catalog(mut self, catalog: &str) -> Self {
        self.0.catalog = Some(catalog.to_string());
        self
    }

    fn envs(mut self, envs: &[&str]) -> Self {
        self.0.env_var_hints = envs.iter().map(|s| (*s).to_string()).collect();
        self
    }

    fn build(self) -> MagpiePreset {
        self.0
    }
}

fn region(
    id: &str,
    name: &str,
    chat: Option<&str>,
    responses: Option<&str>,
    anthropic: Option<&str>,
) -> PresetRegion {
    PresetRegion {
        id: id.to_string(),
        name: name.to_string(),
        chat_url: chat.map(ToString::to_string),
        responses_url: responses.map(ToString::to_string),
        anthropic_url: anthropic.map(ToString::to_string),
        gemini_url: None,
        keys_url: None,
        website: None,
        models: Vec::new(),
    }
}

fn build_presets() -> Vec<MagpiePreset> {
    let ark_models = &[
        "ark-code-latest",
        "doubao-seed-evolving",
        "doubao-seed-2.1-pro",
        "doubao-seed-2.1-lite",
        "minimax-m3",
        "glm-5.3",
        "deepseek-v4.1-flash",
        "deepseek-v4-pro",
        "kimi-k3",
    ];
    let alibaba_plan_models = &[
        "auto",
        "qwen3.8-max",
        "qwen3.8-flash",
        "qwen3.7-max",
        "qwen3.7-plus",
        "deepseek-v4.1-flash",
        "deepseek-v4-pro",
        "glm-5.3",
    ];

    vec![
        // --- Vendors ---
        PresetBuilder::new("deepseek", "DeepSeek", "deepseek-color", PresetKind::Vendor, "deepseek-chat")
            .chat("https://api.deepseek.com/v1")
            .responses("https://api.deepseek.com/v1")
            .anthropic("https://api.deepseek.com/anthropic")
            .models(&["deepseek-chat", "deepseek-reasoner"])
            .links("https://platform.deepseek.com", "https://platform.deepseek.com/api_keys")
            .catalog("deepseek")
            .envs(&["DEEPSEEK_API_KEY"])
            .build(),
        PresetBuilder::new("moonshot", "Kimi (Moonshot Global)", "kimi", PresetKind::Vendor, "kimi-k2.5")
            .short("Kimi")
            .chat("https://api.moonshot.ai/v1")
            .anthropic("https://api.moonshot.ai/anthropic")
            .models(&["kimi-k2.5", "kimi-k2-0905-preview", "moonshot-v1-128k"])
            .links("https://platform.kimi.ai?aff=magpie", "https://platform.kimi.ai/console/api-keys?aff=magpie")
            .catalog("moonshotai")
            .envs(&["MOONSHOT_API_KEY", "KIMI_API_KEY"])
            .build(),
        PresetBuilder::new("moonshot-cn", "Kimi (China)", "kimi", PresetKind::Vendor, "kimi-k2.5")
            .short("Kimi CN")
            .chat("https://api.moonshot.cn/v1")
            .anthropic("https://api.moonshot.cn/anthropic")
            .models(&["kimi-k2.5", "kimi-k2-0905-preview", "moonshot-v1-128k"])
            .links("https://platform.kimi.com?aff=magpie", "https://platform.kimi.com/console/api-keys?aff=magpie")
            .catalog("moonshotai")
            .envs(&["MOONSHOT_API_KEY", "KIMI_API_KEY"])
            .build(),
        PresetBuilder::new("kimi-code", "Kimi Code", "kimi", PresetKind::Vendor, "kimi-for-coding")
            .chat("https://api.kimi.ai/coding/v1")
            .anthropic("https://api.kimi.ai/coding")
            .models(&["kimi-for-coding", "k3"])
            .split_accounts()
            .note("Membership")
            .links("https://www.kimi.ai/code?aff=magpie", "https://www.kimi.ai/code/console?aff=magpie")
            .catalog("kimi-code-plan-global")
            .build(),
        PresetBuilder::new("kimi-code-cn", "Kimi Code (China)", "kimi", PresetKind::Vendor, "kimi-for-coding")
            .short("Kimi Code CN")
            .chat("https://api.kimi.com/coding/v1")
            .anthropic("https://api.kimi.com/coding")
            .models(&["kimi-for-coding", "k3"])
            .split_accounts()
            .note("Membership · China")
            .links("https://www.kimi.com/code?aff=magpie", "https://www.kimi.com/code/console?aff=magpie")
            .catalog("kimi-code-plan-cn")
            .build(),
        PresetBuilder::new("zhipu", "Zhipu GLM (智谱清言)", "zhipu-color", PresetKind::Vendor, "glm-5")
            .short("Zhipu GLM")
            .chat("https://open.bigmodel.cn/api/paas/v4")
            .responses("https://open.bigmodel.cn/api/v1")
            .anthropic("https://open.bigmodel.cn/api/anthropic")
            .models(&["glm-5", "glm-4.7", "glm-4.5-air", "glm-4-plus"])
            .region_label("Plan")
            .key_plans()
            .regions(vec![
                region("api", "Pay as you go", Some("https://open.bigmodel.cn/api/paas/v4"), None, Some("https://open.bigmodel.cn/api/anthropic")),
                region("coding", "Coding Plan", Some("https://open.bigmodel.cn/api/coding/paas/v4"), Some("https://open.bigmodel.cn/api/v1"), Some("https://open.bigmodel.cn/api/anthropic")),
            ])
            .links("https://open.bigmodel.cn", "https://open.bigmodel.cn/usercenter/proj-mgmt/apikeys")
            .catalog("zhipuai")
            .envs(&["ZHIPU_API_KEY", "ZHIPUAI_API_KEY"])
            .build(),
        PresetBuilder::new("zai", "Z.ai", "zai", PresetKind::Vendor, "glm-5")
            .chat("https://api.z.ai/api/paas/v4")
            .responses("https://api.z.ai/api/v1")
            .anthropic("https://api.z.ai/api/anthropic")
            .models(&["glm-5", "glm-4.7"])
            .region_label("Plan")
            .key_plans()
            .regions(vec![
                region("api", "Pay as you go", Some("https://api.z.ai/api/paas/v4"), None, Some("https://api.z.ai/api/anthropic")),
                region("coding", "Coding Plan", Some("https://api.z.ai/api/coding/paas/v4"), Some("https://api.z.ai/api/v1"), Some("https://api.z.ai/api/anthropic")),
            ])
            .links("https://z.ai", "https://z.ai/manage-apikey/apikey-list")
            .catalog("zhipuai")
            .build(),
        PresetBuilder::new("minimax", "MiniMax", "minimax-color", PresetKind::Vendor, "MiniMax-M2.5")
            .chat("https://api.minimax.io/v1")
            .anthropic("https://api.minimax.io/anthropic")
            .models(&["MiniMax-M2.5", "MiniMax-Text-01"])
            .links("https://platform.minimax.io", "https://platform.minimax.io/user-center/basic-information/interface-key")
            .catalog("minimax")
            .envs(&["MINIMAX_API_KEY"])
            .build(),
        PresetBuilder::new("minimax-cn", "MiniMax (China)", "minimax-color", PresetKind::Vendor, "MiniMax-M2.5")
            .short("MiniMax CN")
            .chat("https://api.minimaxi.com/v1")
            .anthropic("https://api.minimaxi.com/anthropic")
            .models(&["MiniMax-M2.5", "MiniMax-Text-01"])
            .links("https://platform.minimaxi.com", "https://platform.minimaxi.com/user-center/basic-information/interface-key")
            .catalog("minimax")
            .envs(&["MINIMAX_API_KEY"])
            .build(),
        PresetBuilder::new("stepfun", "StepFun", "stepfun", PresetKind::Vendor, "step-3")
            .chat("https://api.stepfun.ai/step_plan/v1")
            .anthropic("https://api.stepfun.ai/step_plan")
            .models(&["step-3", "step-2-16k"])
            .region_label("Plan")
            .key_plans()
            .regions(vec![
                region("plan", "Step Plan", Some("https://api.stepfun.ai/step_plan/v1"), None, Some("https://api.stepfun.ai/step_plan")),
                region("api", "Pay as you go", Some("https://api.stepfun.ai/v1"), None, Some("https://api.stepfun.ai")),
            ])
            .links("https://platform.stepfun.ai", "https://platform.stepfun.ai/interface-key")
            .catalog("stepfun-ai-step-plan, stepfun-ai")
            .envs(&["STEPFUN_API_KEY"])
            .build(),
        PresetBuilder::new("stepfun-cn", "StepFun (China)", "stepfun", PresetKind::Vendor, "step-3")
            .short("StepFun CN")
            .chat("https://api.stepfun.com/step_plan/v1")
            .anthropic("https://api.stepfun.com/step_plan")
            .models(&["step-3", "step-2-16k"])
            .region_label("Plan")
            .key_plans()
            .regions(vec![
                region("plan", "Step Plan", Some("https://api.stepfun.com/step_plan/v1"), None, Some("https://api.stepfun.com/step_plan")),
                region("api", "Pay as you go", Some("https://api.stepfun.com/v1"), None, Some("https://api.stepfun.com")),
            ])
            .links("https://platform.stepfun.com", "https://platform.stepfun.com/interface-key")
            .catalog("stepfun-step-plan, stepfun")
            .envs(&["STEPFUN_API_KEY"])
            .build(),
        PresetBuilder::new("xiaomi", "Xiaomi MiMo", "mimocode", PresetKind::Vendor, "mimo-v2.6-pro")
            .chat("https://token-plan-cn.xiaomimimo.com/v1")
            .responses("https://token-plan-cn.xiaomimimo.com/v1")
            .anthropic("https://token-plan-cn.xiaomimimo.com/anthropic")
            .models(&["mimo-v2.6-pro", "mimo-v2.6-flash"])
            .key_plans()
            .note("Token Plan · pay as you go")
            .regions(vec![
                region("plan-cn", "Plan · China", Some("https://token-plan-cn.xiaomimimo.com/v1"), Some("https://token-plan-cn.xiaomimimo.com/v1"), Some("https://token-plan-cn.xiaomimimo.com/anthropic")),
                region("plan-sgp", "Plan · Singapore", Some("https://token-plan-sgp.xiaomimimo.com/v1"), Some("https://token-plan-sgp.xiaomimimo.com/v1"), Some("https://token-plan-sgp.xiaomimimo.com/anthropic")),
                region("plan-ams", "Plan · Europe", Some("https://token-plan-ams.xiaomimimo.com/v1"), Some("https://token-plan-ams.xiaomimimo.com/v1"), Some("https://token-plan-ams.xiaomimimo.com/anthropic")),
                region("api", "Pay as you go", Some("https://api.xiaomimimo.com/v1"), Some("https://api.xiaomimimo.com/v1"), Some("https://api.xiaomimimo.com/anthropic")),
            ])
            .links("https://platform.xiaomimimo.com", "https://platform.xiaomimimo.com/token-plan")
            .catalog("xiaomi")
            .build(),
        PresetBuilder::new("qwen", "Qwen (DashScope Intl)", "qwen-color", PresetKind::Vendor, "qwen-plus")
            .short("Qwen")
            .chat("https://dashscope-intl.aliyuncs.com/compatible-mode/v1")
            .anthropic("https://dashscope-intl.aliyuncs.com/apps/anthropic")
            .models(&["qwen-plus", "qwen3-max", "qwen3-coder-plus", "qwen-turbo"])
            .note("DashScope · intl")
            .links("https://modelstudio.console.alibabacloud.com", "https://modelstudio.console.alibabacloud.com/?tab=playground#/api-key")
            .catalog("alibaba")
            .envs(&["DASHSCOPE_API_KEY", "QWEN_API_KEY"])
            .build(),
        PresetBuilder::new("qwen-cn", "Qwen (China)", "qwen-color", PresetKind::Vendor, "qwen-plus")
            .short("Qwen CN")
            .chat("https://dashscope.aliyuncs.com/compatible-mode/v1")
            .anthropic("https://dashscope.aliyuncs.com/apps/anthropic")
            .models(&["qwen-plus", "qwen3-max", "qwen3-coder-plus", "qwen-turbo"])
            .note("DashScope · China")
            .links("https://bailian.console.aliyun.com", "https://bailian.console.aliyun.com/?tab=model#/api-key")
            .catalog("alibaba")
            .envs(&["DASHSCOPE_API_KEY", "QWEN_API_KEY"])
            .build(),
        PresetBuilder::new("qwen-token-plan", "Qwen Token Plan", "qwen-color", PresetKind::Vendor, "auto")
            .short("Qwen Plan")
            .chat("https://token-plan.maas.qianwenaiapi.com/compatible-mode/v1")
            .anthropic("https://token-plan.maas.qianwenaiapi.com/apps/anthropic")
            .models(alibaba_plan_models)
            .note("Qwen AI platform · subscription")
            .links("https://platform.qianwenai.com/docs/token-plan/overview", "https://platform.qianwenai.com/home/analytics/token-plan/individual")
            .build(),
        PresetBuilder::new("bailian-token-plan", "Bailian Token Plan", "bailian-color", PresetKind::Vendor, "auto")
            .short("Bailian Plan")
            .chat("https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1")
            .anthropic("https://token-plan.cn-beijing.maas.aliyuncs.com/apps/anthropic")
            .models(alibaba_plan_models)
            .note("Alibaba Cloud Bailian · subscription")
            .links("https://help.aliyun.com/zh/model-studio/token-plan-overview", "https://bailian.console.aliyun.com/cn-beijing/subscription/token-plan/personal")
            .build(),
        PresetBuilder::new("baidu-qianfan", "Baidu Qianfan", "baiducloud-color", PresetKind::Vendor, "qianfan-code-latest")
            .chat("https://qianfan.baidubce.com/v2/tokenplan/personal")
            .responses("https://qianfan.baidubce.com/v2/tokenplan/personal")
            .anthropic("https://qianfan.baidubce.com/anthropic/tokenplan/personal")
            .models(&["qianfan-code-latest", "deepseek-v4.1-flash", "deepseek-v4-pro", "glm-5.3"])
            .region_label("Plan")
            .key_plans()
            .regions(vec![
                region("personal", "Token Plan Personal", Some("https://qianfan.baidubce.com/v2/tokenplan/personal"), Some("https://qianfan.baidubce.com/v2/tokenplan/personal"), Some("https://qianfan.baidubce.com/anthropic/tokenplan/personal")),
                region("team", "Token Plan Enterprise", Some("https://qianfan.baidubce.com/v2/tokenplan/team"), Some("https://qianfan.baidubce.com/v2/tokenplan/team"), Some("https://qianfan.baidubce.com/anthropic/tokenplan/team")),
                region("api", "Pay as you go", Some("https://qianfan.baidubce.com/v2"), Some("https://qianfan.baidubce.com/v2"), Some("https://qianfan.baidubce.com/anthropic")),
            ])
            .links("https://cloud.baidu.com/doc/qianfan/s/Dmrabu8b6", "https://console.bce.baidu.com/qianfan/resource/token-plan")
            .build(),
        PresetBuilder::new("tencent-cloud", "Tencent Cloud", "tencentcloud-color", PresetKind::Vendor, "tc-code-latest")
            .chat("https://tokenhub.tencentmaas.com/plan/v3")
            .responses("https://tokenhub.tencentmaas.com/plan/v3")
            .anthropic("https://tokenhub.tencentmaas.com/plan/anthropic")
            .models(&["tc-code-latest", "auto", "hy4-preview", "glm-5.3", "kimi-k3", "deepseek-v4-pro"])
            .key_plans()
            .regions(vec![
                region("plan", "Plan · China", Some("https://tokenhub.tencentmaas.com/plan/v3"), Some("https://tokenhub.tencentmaas.com/plan/v3"), Some("https://tokenhub.tencentmaas.com/plan/anthropic")),
                region("cn", "Pay as you go · China", Some("https://tokenhub.tencentmaas.com/v1"), Some("https://tokenhub.tencentmaas.com/v1"), Some("https://tokenhub.tencentmaas.com")),
                region("intl", "Pay as you go · Global", Some("https://tokenhub-intl.tencentmaas.com/v1"), Some("https://tokenhub-intl.tencentmaas.com/v1"), Some("https://tokenhub-intl.tencentmaas.com")),
                region("enterprise-intl", "Enterprise plan · Global", Some("https://tokenhub-intl.tencentmaas.com/plan/v3"), Some("https://tokenhub-intl.tencentmaas.com/plan/v3"), Some("https://tokenhub-intl.tencentmaas.com/plan/anthropic")),
            ])
            .links("https://cloud.tencent.com/document/product/1823/130060", "https://console.cloud.tencent.com/tokenhub/tokenplan")
            .build(),
        PresetBuilder::new("volcengine", "Volcengine Ark (火山方舟)", "volcengine-color", PresetKind::Vendor, "ark-code-latest")
            .short("Volcengine Ark")
            .chat("https://ark.cn-beijing.volces.com/api/coding/v3")
            .responses("https://ark.cn-beijing.volces.com/api/coding/v3")
            .anthropic("https://ark.cn-beijing.volces.com/api/coding")
            .models(ark_models)
            .region_label("Plan")
            .key_plans()
            .regions(vec![
                region("coding", "Coding Plan", Some("https://ark.cn-beijing.volces.com/api/coding/v3"), Some("https://ark.cn-beijing.volces.com/api/coding/v3"), Some("https://ark.cn-beijing.volces.com/api/coding")),
                region("agent", "Agent Plan", Some("https://ark.cn-beijing.volces.com/api/plan/v3"), Some("https://ark.cn-beijing.volces.com/api/plan/v3"), Some("https://ark.cn-beijing.volces.com/api/plan")),
                region("api", "Pay as you go", Some("https://ark.cn-beijing.volces.com/api/v3"), Some("https://ark.cn-beijing.volces.com/api/v3"), None),
            ])
            .note("火山方舟 · Coding / Agent Plan")
            .links("https://www.volcengine.com/docs/82379/1925114", "https://ark.volcengine.com/region:cn-beijing/apikey")
            .build(),
        PresetBuilder::new("longcat", "LongCat", "longcat", PresetKind::Vendor, "LongCat-Flash-Chat")
            .chat("https://api.longcat.chat/openai/v1")
            .responses("https://api.longcat.chat/openai/v1")
            .anthropic("https://api.longcat.chat/anthropic")
            .models(&["LongCat-Flash-Chat", "LongCat-Flash-Thinking"])
            .links("https://longcat.chat/platform/docs/zh/api-docs", "https://longcat.chat/platform/api_keys")
            .catalog("longcat")
            .build(),
        PresetBuilder::new("anthropic", "Anthropic Claude", "claude-color", PresetKind::Vendor, "claude-sonnet-4-20250514")
            .short("Anthropic")
            .anthropic("https://api.anthropic.com/v1")
            .default_proto(ProtocolKind::AnthropicMessages)
            .models(&["claude-sonnet-4-20250514", "claude-opus-4-20250514", "claude-3-7-sonnet-latest"])
            .headers(&["anthropic-workspace-id"])
            .links("https://console.anthropic.com", "https://console.anthropic.com/settings/keys")
            .catalog("anthropic")
            .envs(&["ANTHROPIC_API_KEY"])
            .build(),
        PresetBuilder::new("openai", "OpenAI", "openai", PresetKind::Vendor, "gpt-4o")
            .chat("https://api.openai.com/v1")
            .responses("https://api.openai.com/v1")
            .models(&["gpt-4o", "gpt-4.1", "o3-mini", "codex-mini-latest"])
            .links("https://platform.openai.com", "https://platform.openai.com/api-keys")
            .catalog("openai")
            .envs(&["OPENAI_API_KEY"])
            .build(),
        PresetBuilder::new("google", "Google Gemini", "gemini-color", PresetKind::Vendor, "gemini-2.5-flash")
            .gemini("https://generativelanguage.googleapis.com/v1beta")
            .chat("https://generativelanguage.googleapis.com/v1beta/openai")
            .default_proto(ProtocolKind::GeminiGenerateContent)
            .models(&["gemini-2.5-flash", "gemini-2.5-pro", "gemini-3.1-pro-preview"])
            .note("Gemini Developer API")
            .links("https://aistudio.google.com", "https://aistudio.google.com/apikey")
            .catalog("google")
            .envs(&["GEMINI_API_KEY", "GOOGLE_API_KEY"])
            .build(),
        PresetBuilder::new("google-vertex", "Google Vertex AI", "vertexai-color", PresetKind::Vendor, "gemini-2.5-flash")
            .short("Vertex AI")
            .gemini("https://aiplatform.googleapis.com/v1")
            .chat("https://aiplatform.googleapis.com/v1beta1")
            .default_proto(ProtocolKind::GeminiGenerateContent)
            .models(&[
                "gemini-2.5-flash",
                "gemini-2.5-pro",
                "gemini-1.5-pro",
                "gemini-1.5-flash",
                "gemini-3.1-pro-preview",
            ])
            .note("Vertex AI Model Garden & ADC")
            .headers(&["x-goog-user-project"])
            .links("https://cloud.google.com/vertex-ai", "https://console.cloud.google.com/vertex-ai")
            .catalog("google-vertex")
            .envs(&["GOOGLE_APPLICATION_CREDENTIALS", "CLOUDSDK_CORE_PROJECT", "VERTEX_PROJECT_ID"])
            .regions(vec![
                PresetRegion {
                    id: "us-central1".to_string(),
                    name: "us-central1 (Iowa)".to_string(),
                    chat_url: Some("https://us-central1-aiplatform.googleapis.com/v1beta1".to_string()),
                    responses_url: None,
                    anthropic_url: None,
                    gemini_url: Some("https://us-central1-aiplatform.googleapis.com/v1".to_string()),
                    keys_url: None,
                    website: None,
                    models: vec![
                        "gemini-2.5-flash".to_string(),
                        "gemini-2.5-pro".to_string(),
                        "gemini-1.5-pro".to_string(),
                    ],
                },
                PresetRegion {
                    id: "global".to_string(),
                    name: "Global (Auto)".to_string(),
                    chat_url: Some("https://aiplatform.googleapis.com/v1beta1".to_string()),
                    responses_url: None,
                    anthropic_url: None,
                    gemini_url: Some("https://aiplatform.googleapis.com/v1".to_string()),
                    keys_url: None,
                    website: None,
                    models: vec![
                        "gemini-2.5-flash".to_string(),
                        "gemini-2.5-pro".to_string(),
                        "gemini-1.5-pro".to_string(),
                    ],
                },
            ])
            .build(),
        PresetBuilder::new("xai", "xAI Grok", "xai", PresetKind::Vendor, "grok-3-beta")
            .short("xAI")
            .chat("https://api.x.ai/v1")
            .responses("https://api.x.ai/v1")
            .anthropic("https://api.x.ai")
            .models(&["grok-3-beta", "grok-3-mini-beta"])
            .links("https://console.x.ai", "https://console.x.ai")
            .catalog("xai")
            .envs(&["XAI_API_KEY"])
            .build(),
        PresetBuilder::new("mistral", "Mistral AI", "mistral-color", PresetKind::Vendor, "mistral-large-latest")
            .short("Mistral")
            .chat("https://api.mistral.ai/v1")
            .models(&["mistral-large-latest", "codestral-latest"])
            .links("https://console.mistral.ai", "https://console.mistral.ai/api-keys")
            .catalog("mistral")
            .envs(&["MISTRAL_API_KEY"])
            .build(),
        PresetBuilder::new("groq", "Groq", "groq", PresetKind::Vendor, "llama-3.3-70b-versatile")
            .chat("https://api.groq.com/openai/v1")
            .responses("https://api.groq.com/openai/v1")
            .models(&["llama-3.3-70b-versatile", "deepseek-r1-distill-llama-70b"])
            .links("https://console.groq.com", "https://console.groq.com/keys")
            .catalog("groq")
            .envs(&["GROQ_API_KEY"])
            .build(),
        PresetBuilder::new("nous", "Nous Portal", "nousresearch", PresetKind::Vendor, "Hermes-3-Llama-3.1-70B")
            .chat("https://inference-api.nousresearch.com/v1")
            .models(&["Hermes-3-Llama-3.1-70B", "DeepHermes-3-Llama-3-8B-Preview"])
            .note("Nous Research · Hermes models")
            .links("https://portal.nousresearch.com", "https://portal.nousresearch.com")
            .envs(&["NOUS_API_KEY"])
            .build(),

        // --- Relays ---
        PresetBuilder::new("openrouter", "OpenRouter", "openrouter", PresetKind::Relay, "anthropic/claude-sonnet-4")
            .chat("https://openrouter.ai/api/v1")
            .anthropic("https://openrouter.ai/api")
            .models(&["anthropic/claude-sonnet-4", "openai/gpt-4o", "deepseek/deepseek-chat-v3-0324"])
            .headers(&["HTTP-Referer", "X-OpenRouter-Title"])
            .links("https://openrouter.ai", "https://openrouter.ai/keys")
            .catalog("openrouter")
            .envs(&["OPENROUTER_API_KEY"])
            .build(),
        PresetBuilder::new("siliconflow", "SiliconFlow (硅基流动)", "siliconcloud-color", PresetKind::Relay, "deepseek-ai/DeepSeek-V3")
            .short("SiliconFlow")
            .chat("https://api.siliconflow.cn/v1")
            .models(&["deepseek-ai/DeepSeek-V3", "deepseek-ai/DeepSeek-R1", "Qwen/Qwen2.5-72B-Instruct"])
            .key_plans()
            .regions(vec![
                region("cn", "China", Some("https://api.siliconflow.cn/v1"), None, None),
                region("intl", "Global", Some("https://api.siliconflow.com/v1"), None, None),
            ])
            .links("https://cloud.siliconflow.cn", "https://cloud.siliconflow.cn/account/ak")
            .catalog("siliconflow")
            .envs(&["SILICONFLOW_API_KEY"])
            .build(),
        PresetBuilder::new("vercel", "Vercel AI Gateway", "vercel", PresetKind::Relay, "anthropic/claude-sonnet-4")
            .short("Vercel")
            .chat("https://ai-gateway.vercel.sh/v1")
            .responses("https://ai-gateway.vercel.sh/v1")
            .anthropic("https://ai-gateway.vercel.sh")
            .models(&["anthropic/claude-sonnet-4", "openai/gpt-4o"])
            .links("https://vercel.com/docs/ai-gateway", "https://vercel.com/d?to=%2F%5Bteam%5D%2F%7E%2Fai-gateway%2Fapi-keys")
            .catalog("vercel")
            .build(),
        PresetBuilder::new("together", "Together AI", "together-color", PresetKind::Relay, "meta-llama/Llama-3.3-70B-Instruct-Turbo")
            .chat("https://api.together.xyz/v1")
            .models(&["meta-llama/Llama-3.3-70B-Instruct-Turbo", "deepseek-ai/DeepSeek-V3"])
            .links("https://api.together.ai", "https://api.together.ai/settings/api-keys")
            .catalog("togetherai")
            .envs(&["TOGETHER_API_KEY"])
            .build(),
        PresetBuilder::new("fireworks", "Fireworks AI", "fireworks-color", PresetKind::Relay, "accounts/fireworks/models/deepseek-v3")
            .short("Fireworks")
            .chat("https://api.fireworks.ai/inference/v1")
            .models(&["accounts/fireworks/models/deepseek-v3", "accounts/fireworks/models/llama-v3p3-70b-instruct"])
            .links("https://fireworks.ai", "https://app.fireworks.ai/settings/users/api-keys")
            .catalog("fireworks-ai")
            .envs(&["FIREWORKS_API_KEY"])
            .build(),
        PresetBuilder::new("fireworks-nexus", "Fireworks Nexus", "fireworks-color", PresetKind::Relay, "auto")
            .short("Nexus")
            .chat("https://api.fireworks.ai/inference/v1")
            .responses("https://api.fireworks.ai/inference/v1")
            .anthropic("https://api.fireworks.ai/inference")
            .models(&["auto"])
            .headers(&["x-anthropic-api-key", "x-openai-api-key"])
            .note("Nexus Auto · Pro, Team, Enterprise")
            .links("https://docs.fireworks.ai/nexus", "https://nexus.fireworks.ai/settings/api-keys")
            .build(),
        PresetBuilder::new("nvidia", "NVIDIA NIM", "nvidia-color", PresetKind::Relay, "meta/llama-3.3-70b-instruct")
            .chat("https://integrate.api.nvidia.com/v1")
            .models(&["meta/llama-3.3-70b-instruct", "deepseek-ai/deepseek-r1"])
            .links("https://build.nvidia.com", "https://build.nvidia.com/settings/api-keys")
            .catalog("nvidia")
            .envs(&["NVIDIA_API_KEY"])
            .build(),
        PresetBuilder::new("modelscope", "ModelScope (魔搭)", "modelscope-color", PresetKind::Relay, "Qwen/Qwen2.5-Coder-32B-Instruct")
            .short("ModelScope")
            .chat("https://api-inference.modelscope.cn/v1")
            .responses("https://api-inference.modelscope.cn/v1")
            .models(&["Qwen/Qwen2.5-Coder-32B-Instruct", "deepseek-ai/DeepSeek-V3"])
            .note("魔搭 · API-Inference")
            .links("https://modelscope.cn/docs/model-service/API-Inference/intro", "https://modelscope.cn/my/myaccesstoken")
            .catalog("modelscope")
            .envs(&["MODELSCOPE_API_KEY"])
            .build(),
        PresetBuilder::new("opencode-zen", "OpenCode Zen", "opencode", PresetKind::Relay, "big-pickle")
            .chat("https://opencode.ai/zen/v1")
            .responses("https://opencode.ai/zen/v1")
            .anthropic("https://opencode.ai/zen")
            .models(&["big-pickle", "glm-5-free"])
            .no_key(Some("optional: free models need no key"))
            .note("free coding models with optional key")
            .links("https://opencode.ai/docs/zen", "https://opencode.ai/auth")
            .catalog("opencode")
            .build(),
        PresetBuilder::new("kilo", "Kilo Gateway", "kilo", PresetKind::Relay, "kilo/auto:free")
            .chat("https://api.kilo.ai/api/openrouter")
            .models(&["kilo/auto:free"])
            .no_key(Some("optional: free models need no key"))
            .note("free models with no key")
            .links("https://kilo.ai/docs/gateway", "https://app.kilo.ai")
            .build(),
        PresetBuilder::new("aihubmix", "AiHubMix", "aihubmix-color", PresetKind::Relay, "gpt-4o")
            .chat("https://aihubmix.com/v1")
            .anthropic("https://aihubmix.com")
            .models(&["gpt-4o", "claude-sonnet-4-20250514", "deepseek-chat"])
            .links("https://aihubmix.com", "https://console.aihubmix.com/token")
            .build(),
        PresetBuilder::new("302ai", "302.AI", "ai302-color", PresetKind::Relay, "gpt-4o")
            .chat("https://api.302.ai/v1")
            .anthropic("https://api.302.ai")
            .models(&["gpt-4o", "claude-sonnet-4-20250514", "deepseek-chat"])
            .links("https://302.ai", "https://302.ai/api-keys/list")
            .build(),
        PresetBuilder::new("aigocode", "AIGoCode", "aigocode", PresetKind::Relay, "claude-sonnet-4-20250514")
            .chat("https://api.aigocode.dev/v1")
            .responses("https://api.aigocode.dev/v1")
            .anthropic("https://api.aigocode.dev")
            .models(&["claude-sonnet-4-20250514", "gpt-4o"])
            .links("https://www.aigocode.net", "https://www.aigocode.net/dashboard/keys")
            .build(),

        // --- Local ---
        PresetBuilder::new("ollama", "Ollama (Local)", "ollama", PresetKind::Local, "qwen3:8b")
            .short("Ollama")
            .chat("http://127.0.0.1:11434/v1")
            .anthropic("http://127.0.0.1:11434")
            .models(&["qwen3:8b", "llama3.2", "deepseek-r1:8b"])
            .no_key(Some("No API key required for local Ollama"))
            .note("your local models (127.0.0.1:11434)")
            .links("https://ollama.com", "")
            .build(),
        PresetBuilder::new("lmstudio", "LM Studio", "lmstudio", PresetKind::Local, "local-model")
            .chat("http://127.0.0.1:1234/v1")
            .models(&["local-model"])
            .no_key(Some("No API key required for local LM Studio"))
            .note("local server, port 1234 by default")
            .links("https://lmstudio.ai", "")
            .build(),
        PresetBuilder::new("omlx", "oMLX", "omlx", PresetKind::Local, "mlx-community/Qwen3-8B-4bit")
            .chat("http://127.0.0.1:8000/v1")
            .responses("http://127.0.0.1:8000/v1")
            .anthropic("http://127.0.0.1:8000")
            .models(&["mlx-community/Qwen3-8B-4bit"])
            .no_key(Some("No API key required for local oMLX"))
            .note("local server, port 8000 by default")
            .links("https://omlx.ai", "")
            .build(),
        PresetBuilder::new("mlx-serve", "MLX-Serve", "mlx-serve", PresetKind::Local, "mlx-default")
            .chat("http://127.0.0.1:8080/v1")
            .responses("http://127.0.0.1:8080/v1")
            .anthropic("http://127.0.0.1:8080")
            .models(&["mlx-default"])
            .no_key(Some("No API key required for local MLX-Serve"))
            .note("local server, port 8080 by default")
            .links("https://github.com/ddalcu/mlx-serve", "")
            .build(),
        PresetBuilder::new("gpustack", "GPUStack", "gpustack", PresetKind::Local, "qwen3-8b")
            .chat("http://127.0.0.1/v1")
            .models(&["qwen3-8b"])
            .no_key(Some("Optional API key for local GPUStack"))
            .note("self-hosted GPU cluster server")
            .links("https://github.com/gpustack/gpustack", "")
            .build(),

        // --- Remote & Custom ---
        PresetBuilder::new("remote-magpie", "Remote Magpie Gateway", "magpie", PresetKind::Remote, "auto/gemini-3.8-flash")
            .short("Remote Magpie")
            .chat("http://127.0.0.1:3425/v1")
            .responses("http://127.0.0.1:3425/backend-api/codex")
            .anthropic("http://127.0.0.1:3425")
            .models(&["auto/gemini-3.8-flash", "auto/gemini-3.1-pro-preview"])
            .no_key(Some("Optional gateway access key"))
            .note("another computer's Magpie gateway (default 127.0.0.1:3425)")
            .links("https://github.com/yetone/magpie", "")
            .build(),
        PresetBuilder::new("custom-openai", "Custom OpenAI-Compatible", "openai", PresetKind::Custom, "gpt-4o")
            .chat("https://api.example.com/v1")
            .models(&["gpt-4o"])
            .note("OpenAI Chat Completions (/chat/completions)")
            .build(),
        PresetBuilder::new("custom-anthropic", "Custom Anthropic-Compatible", "claude-color", PresetKind::Custom, "claude-sonnet-4-20250514")
            .anthropic("https://api.example.com")
            .default_proto(ProtocolKind::AnthropicMessages)
            .models(&["claude-sonnet-4-20250514"])
            .note("Anthropic Messages (/v1/messages)")
            .build(),
        PresetBuilder::new("custom-gemini", "Custom Gemini-Compatible", "gemini-color", PresetKind::Custom, "gemini-2.5-flash")
            .gemini("https://generativelanguage.googleapis.com/v1beta")
            .default_proto(ProtocolKind::GeminiGenerateContent)
            .models(&["gemini-2.5-flash"])
            .note("Google Gemini API (:generateContent)")
            .build(),
        PresetBuilder::new("custom-responses", "Custom Codex Responses", "openai", PresetKind::Custom, "codex-mini-latest")
            .responses("https://api.example.com/v1")
            .default_proto(ProtocolKind::CodexResponses)
            .models(&["codex-mini-latest"])
            .note("OpenAI Responses API (/responses)")
            .build(),
    ]
}

static PRESETS_CACHE: OnceLock<Vec<MagpiePreset>> = OnceLock::new();

/// Return the canonical list of all built-in Magpie provider presets.
pub fn all_presets() -> &'static [MagpiePreset] {
    PRESETS_CACHE.get_or_init(build_presets).as_slice()
}

/// Resolve legacy or shorthand preset aliases into `(preset_id, optional_region_id)`.
pub fn canonical_preset_alias(id: &str) -> (&str, Option<&str>) {
    match id.trim().to_ascii_lowercase().as_str() {
        "qianfan-token-plan" => ("baidu-qianfan", None),
        "tencent-token-plan" => ("tencent-cloud", Some("plan")),
        "tencent-tokenhub-cn" => ("tencent-cloud", Some("cn")),
        "tencent-tokenhub" => ("tencent-cloud", Some("intl")),
        "magpie-local" | "magpie_local" => ("remote-magpie", None),
        "gemini" => ("google", None),
        "vertex" | "vertex-ai" | "google-vertex" | "vertex_ai" => ("google-vertex", None),
        "vertex-us" | "vertex-central" => ("google-vertex", Some("us-central1")),
        "kimi" => ("moonshot", None),
        "kimi-cn" => ("moonshot-cn", None),
        "glm" => ("zhipu", None),
        _ => (id.trim(), None),
    }
}

fn canonical_region_id<'a>(preset_id: &str, region_id: &'a str) -> &'a str {
    if preset_id.eq_ignore_ascii_case("tencent-cloud")
        && region_id.eq_ignore_ascii_case("enterprise")
    {
        return "plan";
    }
    region_id
}

/// Find a preset by ID (case-insensitive) or known alias.
pub fn find_preset(id: &str) -> Option<&'static MagpiePreset> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (canonical, _) = canonical_preset_alias(trimmed);
    all_presets()
        .iter()
        .find(|p| p.id.eq_ignore_ascii_case(canonical))
}

/// Resolve preset endpoints applying any alias default region unless `region_id` overrides it.
pub fn resolve_preset_endpoints(
    id: &str,
    region_id: Option<&str>,
    preferred_protocol: Option<ProtocolKind>,
) -> Option<ResolvedPresetEndpoint> {
    let (canonical, alias_region) = canonical_preset_alias(id);
    let preset = all_presets()
        .iter()
        .find(|p| p.id.eq_ignore_ascii_case(canonical))?;
    let effective_region = region_id
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or(alias_region);
    preset.resolve_endpoint(effective_region, preferred_protocol)
}
