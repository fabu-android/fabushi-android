use crate::presets::{find_preset, resolve_preset_endpoints, ProtocolKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

/// Parsed provider configuration produced from an import link or `.env` paste.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedProviderSpec {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    pub protocol: ProtocolKind,
    pub base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub responses_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anthropic_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gemini_url: Option<String>,
    pub default_model: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keys_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ImportError {
    #[error("Unsupported import link scheme or host (expected magpie://import?... or https://usemagpie.ai/import?...)")]
    UnsupportedLink,
    #[error("Unknown preset '{0}'")]
    UnknownPreset(String),
    #[error("Region '{region}' not found for preset '{preset}'")]
    UnknownRegion { preset: String, region: String },
    #[error("Import link requires either preset= or name=")]
    MissingNameOrPreset,
    #[error("Import link requires at least one http(s) endpoint URL")]
    MissingEndpointUrl,
    #[error("Unsafe or invalid endpoint URL '{0}'")]
    InvalidEndpointUrl(String),
}

fn decode_percent_query(raw: &str) -> String {
    let mut bytes = Vec::with_capacity(raw.len());
    let raw_bytes = raw.as_bytes();
    let mut i = 0usize;
    while i < raw_bytes.len() {
        match raw_bytes[i] {
            b'+' => {
                bytes.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < raw_bytes.len() => {
                if let (Some(hi), Some(lo)) = (hex_nibble(raw_bytes[i + 1]), hex_nibble(raw_bytes[i + 2])) {
                    bytes.push((hi << 4) | lo);
                    i += 3;
                } else {
                    bytes.push(b'%');
                    i += 1;
                }
            }
            other => {
                bytes.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn parse_query_params(query: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let dk = decode_percent_query(k).trim().to_ascii_lowercase();
        let dv = decode_percent_query(v).trim().to_string();
        if !dk.is_empty() && !dv.is_empty() {
            map.insert(dk, dv);
        }
    }
    map
}

fn validate_http_endpoint(raw: &str) -> Result<String, ImportError> {
    let trimmed = raw.trim().trim_end_matches('/');
    let lower = trimmed.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return Err(ImportError::InvalidEndpointUrl(raw.to_string()));
    }
    let after = if lower.starts_with("https://") {
        &trimmed[8..]
    } else {
        &trimmed[7..]
    };
    if after.is_empty() || after.starts_with('/') {
        return Err(ImportError::InvalidEndpointUrl(raw.to_string()));
    }
    Ok(trimmed.to_string())
}

fn slugify_id(raw: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in raw.trim().to_ascii_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

/// Parse a `magpie://import?...` or `https://usemagpie.ai/import?...` deep-link into an `ImportedProviderSpec`.
pub fn parse_magpie_import_link(link: &str) -> Result<ImportedProviderSpec, ImportError> {
    let trimmed = link.trim();
    let query = if let Some(rest) = trimmed
        .strip_prefix("magpie://import")
        .or_else(|| trimmed.strip_prefix("magpie:///import"))
        .or_else(|| trimmed.strip_prefix("magpie:import"))
    {
        rest.strip_prefix('?').ok_or(ImportError::UnsupportedLink)?
    } else if let Some(rest) = trimmed
        .strip_prefix("https://usemagpie.ai/import")
        .or_else(|| trimmed.strip_prefix("https://www.usemagpie.ai/import"))
        .or_else(|| trimmed.strip_prefix("http://usemagpie.ai/import"))
    {
        rest.strip_prefix('?').ok_or(ImportError::UnsupportedLink)?
    } else {
        return Err(ImportError::UnsupportedLink);
    };

    let params = parse_query_params(query);
    let get = |k: &str| params.get(k).map(String::as_str).map(str::trim).filter(|s| !s.is_empty());

    let explicit_models: Vec<String> = get("models")
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default();

    let api_key = get("key").map(ToString::to_string);

    if let Some(preset_raw) = get("preset") {
        let preset = find_preset(preset_raw)
            .ok_or_else(|| ImportError::UnknownPreset(preset_raw.to_string()))?;
        let region_arg = get("region");
        if let Some(r) = region_arg {
            let valid_region = preset
                .regions
                .iter()
                .any(|reg| reg.id.eq_ignore_ascii_case(r) || (preset.id == "tencent-cloud" && r.eq_ignore_ascii_case("enterprise")));
            if !valid_region {
                return Err(ImportError::UnknownRegion {
                    preset: preset.id.clone(),
                    region: r.to_string(),
                });
            }
        }
        let resolved = resolve_preset_endpoints(&preset.id, region_arg, None)
            .ok_or(ImportError::MissingEndpointUrl)?;
        let name = get("name")
            .map(ToString::to_string)
            .unwrap_or_else(|| resolved.display_name.clone());
        let id = get("id")
            .map(slugify_id)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| preset.id.clone());
        let mut models = if explicit_models.is_empty() {
            resolved.suggested_models.clone()
        } else {
            explicit_models
        };
        let default_model = get("model")
            .map(ToString::to_string)
            .or_else(|| models.first().cloned())
            .unwrap_or_else(|| resolved.default_model.clone());
        if !default_model.is_empty() && !models.contains(&default_model) {
            models.insert(0, default_model.clone());
        }

        return Ok(ImportedProviderSpec {
            id,
            name,
            preset_id: Some(preset.id.clone()),
            region_id: resolved.region_id,
            api_key,
            protocol: resolved.protocol,
            base_url: resolved.base_url,
            chat_url: preset.chat_url.clone(),
            responses_url: preset.responses_url.clone(),
            anthropic_url: preset.anthropic_url.clone(),
            gemini_url: preset.gemini_url.clone(),
            default_model,
            models,
            catalog: get("catalog").map(ToString::to_string).or_else(|| preset.catalog.clone()),
            website: get("website").map(ToString::to_string).or(resolved.website),
            keys_url: get("keys").map(ToString::to_string).or(resolved.keys_url),
        });
    }

    let name = get("name")
        .ok_or(ImportError::MissingNameOrPreset)?
        .to_string();
    let id = get("id")
        .map(slugify_id)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            let slug = slugify_id(&name);
            if slug.is_empty() {
                "custom-import".to_string()
            } else {
                slug
            }
        });

    let chat_url = get("chat").map(validate_http_endpoint).transpose()?;
    let responses_url = get("responses").map(validate_http_endpoint).transpose()?;
    let anthropic_url = get("anthropic").map(validate_http_endpoint).transpose()?;
    let gemini_url = get("gemini").map(validate_http_endpoint).transpose()?;

    let (protocol, base_url) = if let Some(ref u) = chat_url {
        (ProtocolKind::ChatCompletions, u.clone())
    } else if let Some(ref u) = anthropic_url {
        (ProtocolKind::AnthropicMessages, u.clone())
    } else if let Some(ref u) = gemini_url {
        (ProtocolKind::GeminiGenerateContent, u.clone())
    } else if let Some(ref u) = responses_url {
        (ProtocolKind::CodexResponses, u.clone())
    } else {
        return Err(ImportError::MissingEndpointUrl);
    };

    let mut models = explicit_models;
    let default_model = get("model")
        .map(ToString::to_string)
        .or_else(|| models.first().cloned())
        .unwrap_or_else(|| match protocol {
            ProtocolKind::AnthropicMessages => "claude-sonnet-4-20250514".to_string(),
            ProtocolKind::GeminiGenerateContent => "gemini-2.5-flash".to_string(),
            ProtocolKind::CodexResponses => "codex-mini-latest".to_string(),
            ProtocolKind::ChatCompletions => "gpt-4o".to_string(),
        });
    if !models.contains(&default_model) {
        models.insert(0, default_model.clone());
    }

    Ok(ImportedProviderSpec {
        id,
        name,
        preset_id: None,
        region_id: None,
        api_key,
        protocol,
        base_url,
        chat_url,
        responses_url,
        anthropic_url,
        gemini_url,
        default_model,
        models,
        catalog: get("catalog").map(ToString::to_string),
        website: get("website").map(ToString::to_string),
        keys_url: get("keys").map(ToString::to_string),
    })
}

struct EnvVendorMapping {
    preset_id: &'static str,
    key_vars: &'static [&'static str],
    base_url_vars: &'static [&'static str],
    model_vars: &'static [&'static str],
}

const ENV_VENDOR_MAPPINGS: &[EnvVendorMapping] = &[
    EnvVendorMapping {
        preset_id: "anthropic",
        key_vars: &["ANTHROPIC_API_KEY"],
        base_url_vars: &["ANTHROPIC_BASE_URL"],
        model_vars: &["ANTHROPIC_MODEL"],
    },
    EnvVendorMapping {
        preset_id: "openai",
        key_vars: &["OPENAI_API_KEY"],
        base_url_vars: &["OPENAI_BASE_URL"],
        model_vars: &["OPENAI_MODEL"],
    },
    EnvVendorMapping {
        preset_id: "google",
        key_vars: &["GEMINI_API_KEY", "GOOGLE_API_KEY"],
        base_url_vars: &["GEMINI_BASE_URL", "GOOGLE_GEMINI_BASE_URL"],
        model_vars: &["GEMINI_MODEL"],
    },
    EnvVendorMapping {
        preset_id: "deepseek",
        key_vars: &["DEEPSEEK_API_KEY"],
        base_url_vars: &["DEEPSEEK_BASE_URL"],
        model_vars: &["DEEPSEEK_MODEL"],
    },
    EnvVendorMapping {
        preset_id: "moonshot",
        key_vars: &["MOONSHOT_API_KEY", "KIMI_API_KEY"],
        base_url_vars: &["MOONSHOT_BASE_URL", "KIMI_BASE_URL"],
        model_vars: &["MOONSHOT_MODEL", "KIMI_MODEL"],
    },
    EnvVendorMapping {
        preset_id: "zhipu",
        key_vars: &["ZHIPU_API_KEY", "ZHIPUAI_API_KEY"],
        base_url_vars: &["ZHIPU_BASE_URL", "ZHIPUAI_BASE_URL"],
        model_vars: &["ZHIPU_MODEL"],
    },
    EnvVendorMapping {
        preset_id: "minimax",
        key_vars: &["MINIMAX_API_KEY"],
        base_url_vars: &["MINIMAX_BASE_URL"],
        model_vars: &["MINIMAX_MODEL"],
    },
    EnvVendorMapping {
        preset_id: "stepfun",
        key_vars: &["STEPFUN_API_KEY"],
        base_url_vars: &["STEPFUN_BASE_URL"],
        model_vars: &["STEPFUN_MODEL"],
    },
    EnvVendorMapping {
        preset_id: "qwen",
        key_vars: &["DASHSCOPE_API_KEY", "QWEN_API_KEY"],
        base_url_vars: &["DASHSCOPE_BASE_URL", "QWEN_BASE_URL"],
        model_vars: &["QWEN_MODEL", "DASHSCOPE_MODEL"],
    },
    EnvVendorMapping {
        preset_id: "openrouter",
        key_vars: &["OPENROUTER_API_KEY"],
        base_url_vars: &["OPENROUTER_BASE_URL"],
        model_vars: &["OPENROUTER_MODEL"],
    },
    EnvVendorMapping {
        preset_id: "siliconflow",
        key_vars: &["SILICONFLOW_API_KEY"],
        base_url_vars: &["SILICONFLOW_BASE_URL"],
        model_vars: &["SILICONFLOW_MODEL"],
    },
    EnvVendorMapping {
        preset_id: "groq",
        key_vars: &["GROQ_API_KEY"],
        base_url_vars: &["GROQ_BASE_URL"],
        model_vars: &["GROQ_MODEL"],
    },
    EnvVendorMapping {
        preset_id: "xai",
        key_vars: &["XAI_API_KEY"],
        base_url_vars: &["XAI_BASE_URL"],
        model_vars: &["XAI_MODEL"],
    },
    EnvVendorMapping {
        preset_id: "mistral",
        key_vars: &["MISTRAL_API_KEY"],
        base_url_vars: &["MISTRAL_BASE_URL"],
        model_vars: &["MISTRAL_MODEL"],
    },
    EnvVendorMapping {
        preset_id: "together",
        key_vars: &["TOGETHER_API_KEY"],
        base_url_vars: &["TOGETHER_BASE_URL"],
        model_vars: &["TOGETHER_MODEL"],
    },
    EnvVendorMapping {
        preset_id: "fireworks",
        key_vars: &["FIREWORKS_API_KEY"],
        base_url_vars: &["FIREWORKS_BASE_URL"],
        model_vars: &["FIREWORKS_MODEL"],
    },
    EnvVendorMapping {
        preset_id: "nvidia",
        key_vars: &["NVIDIA_API_KEY"],
        base_url_vars: &["NVIDIA_BASE_URL"],
        model_vars: &["NVIDIA_MODEL"],
    },
    EnvVendorMapping {
        preset_id: "modelscope",
        key_vars: &["MODELSCOPE_API_KEY"],
        base_url_vars: &["MODELSCOPE_BASE_URL"],
        model_vars: &["MODELSCOPE_MODEL"],
    },
    EnvVendorMapping {
        preset_id: "nous",
        key_vars: &["NOUS_API_KEY"],
        base_url_vars: &["NOUS_BASE_URL"],
        model_vars: &["NOUS_MODEL"],
    },
];

fn parse_dotenv_lines(raw: &str) -> BTreeMap<String, String> {
    let mut vars = BTreeMap::new();
    for raw_line in raw.lines() {
        let mut line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("export ") {
            line = rest.trim_start();
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let key = k.trim();
        if key.is_empty() {
            continue;
        }
        let val_trimmed = v.trim();
        let val = if (val_trimmed.starts_with('"') && val_trimmed.ends_with('"') && val_trimmed.len() >= 2)
            || (val_trimmed.starts_with('\'') && val_trimmed.ends_with('\'') && val_trimmed.len() >= 2)
        {
            val_trimmed[1..val_trimmed.len() - 1].to_string()
        } else {
            // Strip unquoted trailing ` # comment`
            let mut prev_ws = false;
            let mut cut = val_trimmed.len();
            for (idx, ch) in val_trimmed.char_indices() {
                if ch == '#' && prev_ws {
                    cut = idx;
                    break;
                }
                prev_ws = ch.is_whitespace();
            }
            val_trimmed[..cut].trim().to_string()
        };
        if !val.is_empty() {
            vars.entry(key.to_string()).or_insert(val);
        }
    }
    vars
}

/// Parse pasted `.env` content into ready-to-save `ImportedProviderSpec` entries.
pub fn parse_dotenv_providers(raw: &str) -> Vec<ImportedProviderSpec> {
    let vars = parse_dotenv_lines(raw);
    let mut out = Vec::new();
    for mapping in ENV_VENDOR_MAPPINGS {
        let Some(api_key) = mapping
            .key_vars
            .iter()
            .find_map(|k| vars.get(*k))
            .cloned()
        else {
            continue;
        };
        let Some(preset) = find_preset(mapping.preset_id) else {
            continue;
        };
        let Some(resolved) = preset.resolve_endpoint(None, None) else {
            continue;
        };
        let base_url = mapping
            .base_url_vars
            .iter()
            .find_map(|k| vars.get(*k))
            .cloned()
            .unwrap_or(resolved.base_url);
        let default_model = mapping
            .model_vars
            .iter()
            .find_map(|k| vars.get(*k))
            .cloned()
            .unwrap_or(resolved.default_model);
        let mut models = resolved.suggested_models;
        if !models.contains(&default_model) {
            models.insert(0, default_model.clone());
        }
        out.push(ImportedProviderSpec {
            id: preset.id.clone(),
            name: preset.name.clone(),
            preset_id: Some(preset.id.clone()),
            region_id: resolved.region_id,
            api_key: Some(api_key),
            protocol: resolved.protocol,
            base_url,
            chat_url: preset.chat_url.clone(),
            responses_url: preset.responses_url.clone(),
            anthropic_url: preset.anthropic_url.clone(),
            gemini_url: preset.gemini_url.clone(),
            default_model,
            models,
            catalog: preset.catalog.clone(),
            website: resolved.website,
            keys_url: resolved.keys_url,
        });
    }
    out
}
