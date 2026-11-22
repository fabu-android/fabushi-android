//! Live two-tier dynamic model discovery:
//! 1. **Upstream Provider `/models` APIs**: directly queries Vertex AI (`publishers/google/models`),
//!    Codex (`/backend-api/codex/models`), Gemini (`/v1beta/models`), Anthropic (`/v1/models`),
//!    OpenAI-compatible (`/v1/models`), and Ollama (`/api/tags`) using the active credential.
//! 2. **Live `models.dev` catalog (`https://models.dev/api.json`)**: downloads JSON,
//!    compacts it into a lightweight provider->models map sorted by newest `release_date`, and
//!    persists a small local disk cache (~15 KB, 6h TTL) so newly released models appear
//!    automatically without hardcoding or app updates.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::time::Duration;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::subscriptions::SubscriptionKind;

pub const MODELS_DEV_API_URL: &str = "https://models.dev/api.json";
pub const MODELS_DEV_CACHE_TTL_MS: i64 = 6 * 60 * 60 * 1_000; // 6 hours

/// Compact cached snapshot of `https://models.dev/api.json` mapping provider IDs to sorted model IDs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactModelsCatalog {
    #[serde(default)]
    pub fetched_at_ms: i64,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub providers: HashMap<String, Vec<String>>,
}

impl CompactModelsCatalog {
    pub fn is_fresh(&self, now_ms: i64) -> bool {
        self.fetched_at_ms > 0
            && !self.providers.is_empty()
            && now_ms.saturating_sub(self.fetched_at_ms) < MODELS_DEV_CACHE_TTL_MS
    }

    pub fn models_for_subscription(&self, kind: SubscriptionKind) -> Vec<String> {
        let primary_key = models_dev_provider_id_for_subscription(kind);
        let mut out = self.providers.get(primary_key).cloned().unwrap_or_default();
        if kind == SubscriptionKind::GoogleVertex {
            let mut gemini_only: Vec<String> = out
                .iter()
                .filter(|m| m.starts_with("gemini-") && !m.contains('/'))
                .cloned()
                .collect();
            if gemini_only.is_empty() {
                if let Some(google_models) = self.providers.get("google") {
                    gemini_only = google_models
                        .iter()
                        .filter(|m| m.starts_with("gemini-") && !m.contains('/'))
                        .cloned()
                        .collect();
                }
            }
            out = gemini_only;
        }
        out
    }
}

pub fn models_dev_provider_id_for_subscription(kind: SubscriptionKind) -> &'static str {
    match kind {
        SubscriptionKind::GoogleVertex => "google-vertex",
        SubscriptionKind::CodexChatGpt => "openai",
        SubscriptionKind::GeminiCodeAssist => "google",
        SubscriptionKind::ClaudeCode => "anthropic",
        SubscriptionKind::GitHubCopilot => "github-copilot",
        SubscriptionKind::Antigravity => "google",
        SubscriptionKind::GrokSuperGrok => "xai",
    }
}

fn is_non_chat_utility_model(id: &str) -> bool {
    let lo = id.to_ascii_lowercase();
    lo.contains("embedding")
        || lo.contains("embed-")
        || lo.starts_with("text-embedding")
        || lo.contains("whisper")
        || lo.contains("tts-")
        || lo.starts_with("tts")
        || lo.contains("dall-e")
        || lo.contains("imagen")
        || lo.contains("veo-")
        || lo.contains("lyria-")
        || lo.contains("gpt-image")
        || lo.contains("imagine-video")
        || lo.contains("imagine-image")
        || lo.ends_with("-image")
        || lo.contains("moderation")
        || lo.contains("transcribe")
        || lo.contains("realtime")
        || lo.contains("audio-preview")
        || lo.contains("babbage")
        || lo.contains("davinci")
        || lo.contains("aqa")
}

/// Parses `https://models.dev/api.json` into a compact provider->models map sorted by newest
/// `release_date` first (up to 32 chat-capable models per provider).
pub fn parse_models_dev_json(raw: &str, now_ms: i64) -> Result<CompactModelsCatalog, String> {
    let root: Value =
        serde_json::from_str(raw).map_err(|e| format!("Failed to parse models.dev JSON: {e}"))?;
    let Some(obj) = root.as_object() else {
        return Err("models.dev JSON root is not an object".to_string());
    };

    let mut providers = HashMap::new();
    for (prov_id, prov_val) in obj {
        let Some(models_obj) = prov_val.get("models").and_then(Value::as_object) else {
            continue;
        };
        let mut entries: Vec<(String, String)> = Vec::new();
        for (model_key, model_val) in models_obj {
            let model_id = model_val
                .get("id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(model_key.as_str());
            if is_non_chat_utility_model(model_id) {
                continue;
            }
            let release_date = model_val
                .get("release_date")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            entries.push((release_date, model_id.to_string()));
        }
        entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
        let mut unique = Vec::new();
        for (_, mid) in entries {
            if !unique.contains(&mid) {
                unique.push(mid);
            }
            if unique.len() >= 32 {
                break;
            }
        }
        if !unique.is_empty() {
            providers.insert(prov_id.clone(), unique);
        }
    }

    Ok(CompactModelsCatalog {
        fetched_at_ms: now_ms,
        source: "models.dev".to_string(),
        providers,
    })
}

/// Loads a fresh `CompactModelsCatalog` from disk cache if valid, otherwise fetches live from
/// `https://models.dev/api.json` (or `override_url` for local/test endpoints) and updates the disk cache.
pub fn fetch_models_dev_catalog(
    agent: &ureq::Agent,
    cache_path: Option<&Path>,
    force_refresh: bool,
) -> Result<CompactModelsCatalog, String> {
    fetch_models_dev_catalog_from_url(agent, MODELS_DEV_API_URL, cache_path, force_refresh)
}

pub fn fetch_models_dev_catalog_from_url(
    agent: &ureq::Agent,
    url: &str,
    cache_path: Option<&Path>,
    force_refresh: bool,
) -> Result<CompactModelsCatalog, String> {
    let now_ms = Utc::now().timestamp_millis();

    let stale_cache = cache_path.and_then(|p| {
        std::fs::read_to_string(p)
            .ok()
            .and_then(|raw| serde_json::from_str::<CompactModelsCatalog>(&raw).ok())
    });

    if !force_refresh {
        if let Some(ref cached) = stale_cache {
            if cached.is_fresh(now_ms) {
                return Ok(cached.clone());
            }
        }
    }

    let req = agent
        .get(url)
        .set("User-Agent", "fabushi-gateway/1.6")
        .timeout(Duration::from_secs(20));

    match req.call() {
        Ok(resp) => {
            let body = resp
                .into_string()
                .map_err(|e| format!("Failed reading models.dev body: {e}"))?;
            let catalog = parse_models_dev_json(&body, now_ms)?;
            if let Some(p) = cache_path {
                if let Some(parent) = p.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Ok(serialized) = serde_json::to_string(&catalog) {
                    let _ = std::fs::write(p, serialized);
                }
            }
            Ok(catalog)
        }
        Err(ureq::Error::Status(code, _)) => {
            if let Some(cached) = stale_cache {
                return Ok(cached);
            }
            Err(format!("models.dev returned HTTP {code}"))
        }
        Err(ureq::Error::Transport(e)) => {
            if let Some(cached) = stale_cache {
                return Ok(cached);
            }
            Err(format!("models.dev request failed: {e}"))
        }
    }
}

/// Parses Google Vertex AI / Gemini models list JSON responses:
/// - Vertex AI `{"publisherModels": [{"name": "publishers/google/models/gemini-2.5-pro"}]}`
/// - Gemini API `{"models": [{"name": "models/gemini-2.5-pro"}]}`
pub fn parse_vertex_or_gemini_models_response(val: &Value) -> Vec<String> {
    let arr = val
        .get("publisherModels")
        .and_then(Value::as_array)
        .or_else(|| val.get("models").and_then(Value::as_array));
    let Some(items) = arr else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in items {
        let raw_name = item
            .get("name")
            .and_then(Value::as_str)
            .or_else(|| item.get("id").and_then(Value::as_str))
            .unwrap_or("");
        let clean = raw_name.rsplit('/').next().unwrap_or(raw_name).trim();
        if clean.is_empty() || !clean.starts_with("gemini-") || is_non_chat_utility_model(clean) {
            continue;
        }
        if !out.iter().any(|x| x == clean) {
            out.push(clean.to_string());
        }
    }
    out.sort_by(|a, b| b.cmp(a));
    out
}

/// Parses OpenAI `/v1/models`, Anthropic `/v1/models`, Codex `/backend-api/codex/models`,
/// and Ollama `/api/tags` JSON responses.
pub fn parse_openai_or_codex_models_response(val: &Value) -> Vec<String> {
    let mut out = Vec::new();

    if let Some(data) = val.get("data").and_then(Value::as_array) {
        for item in data {
            if let Some(id) = item.get("id").and_then(Value::as_str).map(str::trim) {
                if !id.is_empty() && !is_non_chat_utility_model(id) && !out.iter().any(|x| x == id)
                {
                    out.push(id.to_string());
                }
            }
        }
    }

    if let Some(models) = val.get("models").and_then(Value::as_array) {
        for item in models {
            let hidden = item
                .get("visibility")
                .and_then(Value::as_str)
                .is_some_and(|v| v.eq_ignore_ascii_case("hide"));
            if hidden {
                continue;
            }
            let slug = item
                .get("slug")
                .and_then(Value::as_str)
                .or_else(|| item.get("id").and_then(Value::as_str))
                .or_else(|| item.get("name").and_then(Value::as_str))
                .map(str::trim)
                .unwrap_or("");
            if !slug.is_empty()
                && slug != "gpt-reserve"
                && !is_non_chat_utility_model(slug)
                && !out.iter().any(|x| x == slug)
            {
                out.push(slug.to_string());
            }
        }
    }

    out
}

/// Queries the upstream provider's live `/models` endpoint directly using the provided credential.
pub fn fetch_live_upstream_models(
    agent: &ureq::Agent,
    kind: SubscriptionKind,
    base_url: Option<&str>,
    bearer_token: Option<&str>,
    headers: &BTreeMap<String, String>,
    project_id: Option<&str>,
    location: Option<&str>,
) -> Result<Vec<String>, String> {
    let tok = bearer_token.map(str::trim).filter(|s| !s.is_empty());

    match kind {
        SubscriptionKind::GoogleVertex => {
            let Some(bearer) = tok else {
                return Err("Missing Google Vertex OAuth2 bearer token".to_string());
            };
            let loc = location.unwrap_or("global");
            let host = if let Some(b) = base_url.map(|s| s.trim_end_matches('/')).filter(|s| !s.is_empty()) {
                b.to_string()
            } else if loc == "global" {
                "https://aiplatform.googleapis.com".to_string()
            } else {
                format!("https://{loc}-aiplatform.googleapis.com")
            };
            let url = format!("{host}/v1beta1/publishers/google/models?pageSize=100");
            let mut req = agent
                .get(&url)
                .set("Authorization", &format!("Bearer {bearer}"))
                .timeout(Duration::from_secs(12));
            if let Some(proj) = project_id
                .or_else(|| headers.get("x-goog-user-project").map(String::as_str))
                .filter(|s| !s.trim().is_empty())
            {
                req = req.set("x-goog-user-project", proj);
            }
            let val: Value = req
                .call()
                .map_err(|e| format!("Vertex AI models request failed: {e}"))?
                .into_json()
                .map_err(|e| format!("Failed parsing Vertex AI models JSON: {e}"))?;
            let models = parse_vertex_or_gemini_models_response(&val);
            if models.is_empty() {
                Err("Vertex AI returned no publisher Gemini models".to_string())
            } else {
                Ok(models)
            }
        }
        SubscriptionKind::CodexChatGpt => {
            let Some(bearer) = tok else {
                return Err("Missing Codex OAuth bearer token".to_string());
            };
            let base = base_url
                .map(|s| s.trim_end_matches('/'))
                .filter(|s| !s.is_empty())
                .unwrap_or("https://chatgpt.com/backend-api/codex");
            let url = format!("{base}/models?client_version=0.1.0");
            let mut req = agent
                .get(&url)
                .set("Authorization", &format!("Bearer {bearer}"))
                .set("Accept", "application/json")
                .timeout(Duration::from_secs(12));
            let mut has_originator = false;
            for (k, v) in headers {
                let lo = k.to_ascii_lowercase();
                if lo == "originator" {
                    has_originator = true;
                }
                if !lo.starts_with("x-codex-") && !lo.starts_with("x-adc-") {
                    req = req.set(k.as_str(), v.as_str());
                }
            }
            if !has_originator {
                req = req.set("originator", "codex_cli_rs");
            }
            let val: Value = req
                .call()
                .map_err(|e| format!("Codex models request failed: {e}"))?
                .into_json()
                .map_err(|e| format!("Failed parsing Codex models JSON: {e}"))?;
            let models = parse_openai_or_codex_models_response(&val);
            if models.is_empty() {
                Err("Codex returned empty models list".to_string())
            } else {
                Ok(models)
            }
        }
        SubscriptionKind::ClaudeCode => {
            let Some(bearer) = tok else {
                return Err("Missing Anthropic token".to_string());
            };
            let base = base_url
                .map(|s| s.trim_end_matches('/'))
                .filter(|s| !s.is_empty())
                .unwrap_or("https://api.anthropic.com/v1");
            let url = format!("{base}/models");
            let mut req = agent
                .get(&url)
                .set("anthropic-version", "2023-06-01")
                .timeout(Duration::from_secs(12));
            if bearer.starts_with("sk-ant-") {
                req = req.set("x-api-key", bearer);
            } else {
                req = req.set("Authorization", &format!("Bearer {bearer}"));
            }
            let val: Value = req
                .call()
                .map_err(|e| format!("Anthropic models request failed: {e}"))?
                .into_json()
                .map_err(|e| format!("Failed parsing Anthropic models JSON: {e}"))?;
            let models = parse_openai_or_codex_models_response(&val);
            if models.is_empty() {
                Err("Anthropic returned empty models list".to_string())
            } else {
                Ok(models)
            }
        }
        SubscriptionKind::GeminiCodeAssist | SubscriptionKind::Antigravity => {
            let Some(bearer) = tok else {
                return Err("Missing Google OAuth token".to_string());
            };
            let base = base_url
                .map(|s| s.trim_end_matches('/'))
                .filter(|s| !s.is_empty())
                .unwrap_or("https://generativelanguage.googleapis.com");
            let url = format!("{base}/v1beta/models?pageSize=100");
            let val: Value = agent
                .get(&url)
                .set("Authorization", &format!("Bearer {bearer}"))
                .timeout(Duration::from_secs(12))
                .call()
                .map_err(|e| format!("Gemini models request failed: {e}"))?
                .into_json()
                .map_err(|e| format!("Failed parsing Gemini models JSON: {e}"))?;
            let models = parse_vertex_or_gemini_models_response(&val);
            if models.is_empty() {
                Err("Gemini returned empty models list".to_string())
            } else {
                Ok(models)
            }
        }
        _ => {
            let base = base_url
                .map(|s| s.trim_end_matches('/'))
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "Missing base_url for live models fetch".to_string())?;
            let url = format!("{base}/models");
            let mut req = agent.get(&url).timeout(Duration::from_secs(12));
            if let Some(bearer) = tok {
                req = req.set("Authorization", &format!("Bearer {bearer}"));
            }
            let val: Value = req
                .call()
                .map_err(|e| format!("Upstream models request failed: {e}"))?
                .into_json()
                .map_err(|e| format!("Failed parsing upstream models JSON: {e}"))?;
            let models = parse_openai_or_codex_models_response(&val);
            if models.is_empty() {
                Err("Upstream returned empty models list".to_string())
            } else {
                Ok(models)
            }
        }
    }
}

/// Merges multiple model sources in priority order (upstream live -> models.dev -> local cache),
/// preserving order and deduplicating.
pub fn merge_discovered_models(sources: &[&[String]]) -> Vec<String> {
    let mut out = Vec::new();
    for src in sources {
        for m in *src {
            let trimmed = m.trim();
            if !trimmed.is_empty() && !out.iter().any(|x| x == trimmed) {
                out.push(trimmed.to_string());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_models_dev_json_sorts_newest_first_and_filters_utility_models() {
        let sample = json!({
            "google-vertex": {
                "id": "google-vertex",
                "models": {
                    "gemini-2.5-flash": { "id": "gemini-2.5-flash", "release_date": "2025-04-17" },
                    "gemini-3.8-flash": { "id": "gemini-3.8-flash", "release_date": "2026-09-01" },
                    "text-embedding-005": { "id": "text-embedding-005", "release_date": "2025-01-01" },
                    "imagen-3.0-generate": { "id": "imagen-3.0-generate", "release_date": "2025-02-01" }
                }
            },
            "openai": {
                "id": "openai",
                "models": {
                    "gpt-4o": { "id": "gpt-4o", "release_date": "2024-05-13" },
                    "gpt-6.1-sol": { "id": "gpt-6.1-sol", "release_date": "2026-10-15" },
                    "whisper-1": { "id": "whisper-1", "release_date": "2023-01-01" }
                }
            }
        })
        .to_string();

        let catalog = parse_models_dev_json(&sample, 1_700_000_000_000).unwrap();
        assert_eq!(
            catalog.models_for_subscription(SubscriptionKind::GoogleVertex),
            vec!["gemini-3.8-flash".to_string(), "gemini-2.5-flash".to_string()]
        );
        assert_eq!(
            catalog.models_for_subscription(SubscriptionKind::CodexChatGpt),
            vec!["gpt-6.1-sol".to_string(), "gpt-4o".to_string()]
        );
    }

    #[test]
    fn test_parse_vertex_and_codex_live_model_responses() {
        let vertex_json = json!({
            "publisherModels": [
                { "name": "publishers/google/models/gemini-2.5-pro" },
                { "name": "publishers/google/models/gemini-3.8-flash" },
                { "name": "publishers/google/models/text-embedding-004" }
            ]
        });
        assert_eq!(
            parse_vertex_or_gemini_models_response(&vertex_json),
            vec!["gemini-3.8-flash".to_string(), "gemini-2.5-pro".to_string()]
        );

        let codex_json = json!({
            "models": [
                { "slug": "gpt-6.1-sol", "visibility": "list" },
                { "slug": "internal-hidden", "visibility": "hide" },
                { "slug": "gpt-reserve", "visibility": "list" },
                { "slug": "gpt-5.6-luna", "visibility": "list" }
            ]
        });
        assert_eq!(
            parse_openai_or_codex_models_response(&codex_json),
            vec!["gpt-6.1-sol".to_string(), "gpt-5.6-luna".to_string()]
        );
    }
}
