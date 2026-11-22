//! Pure-Rust Magpie gateway core: provider presets, read-only local subscription discovery,
//! smart routing groups with exponential decay & session affinity, privacy masking, Rosetta Gemini
//! wire translation, and deep-link / `.env` importers.

#[cfg(feature = "axum-server")]
pub mod gateway;
pub mod import;
pub mod masking;
pub mod models_catalog;
pub mod presets;
pub mod protocol;
pub mod routing;
pub mod subscriptions;
pub mod usage;

#[cfg(feature = "axum-server")]
pub use gateway::{
    create_gateway_router, handle_chat_completions, handle_messages, handle_models, handle_quotas,
    handle_responses, handle_usage, GatewayServer, GatewayServerHandle, GatewayState, QuotasView,
    UsageResponseView,
};
pub use import::{parse_dotenv_providers, parse_magpie_import_link, ImportError, ImportedProviderSpec};
pub use masking::{
    mask_text, unmask_text, MaskResult, MaskingAction, MaskingConfig, MaskingEngine, MaskingHit,
    StreamingUnmasker,
};
pub use models_catalog::{
    fetch_live_upstream_models, fetch_models_dev_catalog, fetch_models_dev_catalog_from_url,
    merge_discovered_models, models_dev_provider_id_for_subscription, parse_models_dev_json,
    parse_openai_or_codex_models_response, parse_vertex_or_gemini_models_response,
    CompactModelsCatalog, MODELS_DEV_API_URL, MODELS_DEV_CACHE_TTL_MS,
};
pub use presets::{
    all_presets, canonical_preset_alias, find_preset, resolve_preset_endpoints, MagpiePreset,
    PresetKind, PresetRegion, ProtocolKind, ResolvedPresetEndpoint,
};
pub use protocol::{
    build_gemini_generate_content_payload, encode_anthropic_sse_events, encode_gemini_sse_chunk,
    encode_openai_chat_chunk, encode_openai_done, inbound_anthropic_to_unified,
    inbound_chat_completions_to_unified, inbound_responses_to_unified,
    parse_anthropic_response_json, parse_anthropic_sse_event, parse_chat_completions_response_json,
    parse_gemini_response_json, parse_gemini_sse_chunk, parse_gemini_sse_data,
    parse_openai_sse_chunk, parse_responses_response_json, resolve_anthropic_messages_url,
    resolve_gemini_endpoint_url, sanitize_schema_for_gemini, unified_to_anthropic_json,
    unified_to_anthropic_request_json, unified_to_chat_completions_json,
    unified_to_chat_completions_request_json, unified_to_responses_json,
    unified_to_responses_request_json, GeminiSseDelta, ProtocolError, UnifiedMessage,
    UnifiedRequest, UnifiedResponse, UnifiedRole, UnifiedStreamChunk, UnifiedToolCall,
    UnifiedToolSpec, UnifiedUsage,
};
pub use routing::{
    classify_intent, parse_retry_after_header, resolve_group_targets, split_target_spec,
    CircuitBreaker, IntentTier, ResolvedRouteTarget, RestKind, RestState, RouteDecisionTrace,
    RouteRuntimeMetrics, RouteSeatMetrics, RoutingError, RoutingGroup, RoutingGroupMember,
    RoutingStrategy, SessionAffinityEntry, BASE_RATE_REST_MS, CREDIT_REST_LADDER_MS,
    MAX_RATE_REST_MS, OVERLOAD_REST_MS,
};
pub use subscriptions::{
    decode_jwt_payload, default_models_for_subscription_kind, discover_subscriptions_in_home,
    discover_syncable_subscriptions_in_home, extract_jwt_exp_ms, is_loopback_server_url,
    is_sensitive_header_name, parse_gcloud_config_default, parse_magpie_logins,
    parse_magpie_providers, parse_vertex_adc_credentials, redact_header_value,
    redact_oauth_error_body, scan_claude_dirs, scan_gemini_dirs, scan_magpie_dir, scan_vertex_adc,
    CachedVertexToken, ClientSubscriptionSyncItem, CodexOAuthCredentials, CodexTokenManager,
    GoogleOAuthTokenResponse, SubscriptionAccount, SubscriptionKind, SubscriptionRegistry,
    TokenRefreshError, VertexAdcCredentials, VertexTokenManager, DEFAULT_CODEX_CLIENT_ID,
    DEFAULT_TOKEN_REFRESH_LEAD_TIME_MS, GOOGLE_OAUTH2_TOKEN_URL, OPENAI_OAUTH2_TOKEN_URL,
};
#[cfg(feature = "axum-server")]
pub use usage::normalize_session_header;
pub use usage::{
    estimate_cost_usd, normalize_session_from_map, UsageLedger, UsageRecord, UsageSummary,
    SESSION_HEADERS,
};

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine as _;
    use serde_json::json;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    struct TempDirGuard {
        path: PathBuf,
    }

    impl TempDirGuard {
        fn new(prefix: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "{prefix}_{}_{}",
                std::process::id(),
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or(1)
            ));
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn presets_cover_vendors_relays_local_remote_custom_and_regions() {
        let presets = all_presets();
        assert!(presets.len() >= 40, "expected >=40 presets, got {}", presets.len());

        for required in [
            "deepseek",
            "moonshot",
            "moonshot-cn",
            "kimi-code",
            "kimi-code-cn",
            "zhipu",
            "zai",
            "minimax",
            "minimax-cn",
            "stepfun",
            "stepfun-cn",
            "xiaomi",
            "qwen",
            "qwen-cn",
            "baidu-qianfan",
            "tencent-cloud",
            "volcengine",
            "longcat",
            "anthropic",
            "openai",
            "google",
            "xai",
            "mistral",
            "groq",
            "nous",
            "openrouter",
            "siliconflow",
            "vercel",
            "together",
            "fireworks",
            "fireworks-nexus",
            "nvidia",
            "modelscope",
            "opencode-zen",
            "kilo",
            "aihubmix",
            "302ai",
            "ollama",
            "lmstudio",
            "omlx",
            "mlx-serve",
            "gpustack",
            "remote-magpie",
            "custom-openai",
            "custom-anthropic",
            "custom-gemini",
            "custom-responses",
        ] {
            assert!(find_preset(required).is_some(), "missing preset {required}");
        }

        let zhipu_coding = resolve_preset_endpoints("zhipu", Some("coding"), None).unwrap();
        assert_eq!(zhipu_coding.base_url, "https://open.bigmodel.cn/api/coding/paas/v4");
        assert_eq!(zhipu_coding.region_id.as_deref(), Some("coding"));

        let zai_coding = resolve_preset_endpoints("zai", Some("coding"), None).unwrap();
        assert_eq!(zai_coding.base_url, "https://api.z.ai/api/coding/paas/v4");

        let ollama = find_preset("ollama").unwrap();
        assert!(ollama.no_key);
        assert_eq!(ollama.chat_url.as_deref(), Some("http://127.0.0.1:11434/v1"));

        let sf_intl = resolve_preset_endpoints("siliconflow", Some("intl"), None).unwrap();
        assert_eq!(sf_intl.base_url, "https://api.siliconflow.com/v1");
    }

    #[test]
    fn subscriptions_discovery_parses_local_credentials_readonly_without_secrets_in_json() {
        let dir = TempDirGuard::new("magpie_subs");
        let home = &dir.path;

        // 1. Codex auth + config
        let codex_dir = home.join(".codex");
        std::fs::create_dir_all(&codex_dir).unwrap();
        let jwt_hdr = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
        let jwt_claims = URL_SAFE_NO_PAD.encode(
            br#"{"email":"zen@example.com","https://api.openai.com/auth":{"chatgpt_account_id":"acct_42","chatgpt_plan_type":"pro"}}"#,
        );
        let jwt = format!("{jwt_hdr}.{jwt_claims}.sig");
        std::fs::write(
            codex_dir.join("auth.json"),
            json!({
                "tokens": {
                    "access_token": jwt,
                    "refresh_token": "rt_secret_should_not_serialize",
                    "account_id": "acct_42"
                }
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            codex_dir.join("config.toml"),
            "model = \"gpt-5-codex\"\nopenai_base_url = \"http://127.0.0.1:3425/backend-api/codex\"\n",
        )
        .unwrap();

        // 2. Claude Code credentials
        let claude_dir = home.join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        std::fs::write(
            claude_dir.join(".credentials.json"),
            json!({
                "claudeAiOauth": {
                    "accessToken": "sk-ant-oat01-secret-token",
                    "refreshToken": "rt-claude",
                    "expiresAt": chrono::Utc::now().timestamp_millis() + 3_600_000,
                    "subscriptionType": "max"
                },
                "oauthAccount": { "emailAddress": "claude@example.com" }
            })
            .to_string(),
        )
        .unwrap();

        // 3. GitHub Copilot
        let copilot_dir = home.join(".config").join("github-copilot");
        std::fs::create_dir_all(&copilot_dir).unwrap();
        std::fs::write(
            copilot_dir.join("hosts.json"),
            json!({
                "github.com": {
                    "user": "bhrum-dev",
                    "oauth_token": "ghu_secret_copilot_token"
                }
            })
            .to_string(),
        )
        .unwrap();

        // 4. Gemini Code Assist
        let gemini_dir = home.join(".gemini");
        std::fs::create_dir_all(&gemini_dir).unwrap();
        std::fs::write(
            gemini_dir.join("oauth_creds.json"),
            json!({
                "access_token": "ya29.secret_gemini",
                "refresh_token": "1//refresh_gemini",
                "expiry_date": chrono::Utc::now().timestamp_millis() + 3_600_000
            })
            .to_string(),
        )
        .unwrap();

        let reg = discover_subscriptions_in_home(home, None);
        assert!(reg.has_active_subscription());
        assert_eq!(reg.accounts.len(), 4);

        // Verify raw access tokens are never serialized to JSON!
        let serialized = serde_json::to_string(&reg).unwrap();
        assert!(!serialized.contains("sk-ant-oat01-secret-token"));
        assert!(!serialized.contains("ghu_secret_copilot_token"));
        assert!(!serialized.contains("ya29.secret_gemini"));
    }

    #[test]
    fn routing_strategies_decay_cycle_guard_and_affinity_work() {
        let mut groups = HashMap::new();
        let mut main = RoutingGroup::new("smart-mix", "Smart Mix", RoutingStrategy::Rotate);
        main.members = vec![
            RoutingGroupMember::new("deepseek/deepseek-chat"),
            RoutingGroupMember::new("zhipu/glm-5"),
        ];
        main.intent_fast_model = Some("groq/llama-3.3-70b-versatile".to_string());
        main.intent_deep_model = Some("anthropic/claude-opus-4-20250514".to_string());
        groups.insert("smart-mix".to_string(), main);

        let mut metrics = RouteRuntimeMetrics::default();
        let now = 10_000_000i64;

        // Intent fast overrides first pick when prompt is short simple ask
        let fast_targets = resolve_group_targets(
            "group/smart-mix",
            &groups,
            &mut metrics,
            "deepseek",
            Some("sess1"),
            "hello",
            1,
            0,
            now,
        )
        .unwrap();
        assert_eq!(fast_targets[0].provider_id, "groq");
        assert_eq!(fast_targets[0].reason, "intent_fast");

        // Standard prompt respects session affinity and rotation
        let first_std = resolve_group_targets(
            "smart-mix",
            &groups,
            &mut metrics,
            "deepseek",
            None,
            "Please explain how connection pooling handles backpressure across services cleanly in production.",
            2,
            1,
            now,
        )
        .unwrap();
        assert_eq!(first_std[0].provider_id, "deepseek");
        let second_std = resolve_group_targets(
            "smart-mix",
            &groups,
            &mut metrics,
            "deepseek",
            None,
            "Please explain how connection pooling handles backpressure across services cleanly in production.",
            2,
            1,
            now,
        )
        .unwrap();
        assert_eq!(second_std[0].provider_id, "zhipu");

        // Half-life decay check (3600s -> half)
        metrics.record_usage("deepseek/deepseek-chat", 10_000, Some(120), now);
        let decayed = metrics
            .seats
            .get("deepseek/deepseek-chat")
            .unwrap()
            .decayed_usage(now + 3_600_000);
        assert!((decayed - 5_000.0).abs() < 1e-6, "decayed={decayed}");

        // Cycle protection
        let mut cyc_a = RoutingGroup::new("a", "A", RoutingStrategy::Order);
        cyc_a.members.push(RoutingGroupMember::new("group/b"));
        let mut cyc_b = RoutingGroup::new("b", "B", RoutingStrategy::Order);
        cyc_b.members.push(RoutingGroupMember::new("group/a"));
        groups.insert("a".to_string(), cyc_a);
        groups.insert("b".to_string(), cyc_b);

        let cyc_err = resolve_group_targets(
            "group/a",
            &groups,
            &mut metrics,
            "deepseek",
            None,
            "hello",
            1,
            0,
            now,
        )
        .unwrap_err();
        assert!(matches!(cyc_err, RoutingError::CycleDetected(_)));
    }

    #[test]
    fn masking_secrets_pii_allowlist_and_streaming_unmasker_work() {
        let mut cfg = MaskingConfig {
            enabled: true,
            mask_secrets: true,
            mask_personal: true,
            custom_words: vec!["ProjectPhoenix".to_string()],
            allow_list: vec!["safe@company.com".to_string()],
            default_action: MaskingAction::Mask,
            ..Default::default()
        };
        cfg.kind_actions
            .insert("card_number".to_string(), MaskingAction::Block);

        let engine = MaskingEngine::new(cfg);
        let sample_key = "sk-ant-api03-secret9988776655";
        let sample_jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTYifQ.dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let input = format!(
            "Key={sample_key}, JWT={sample_jwt}, email=alice@realcorp.io, doc=bob@example.com, safe=safe@company.com, word=ProjectPhoenix"
        );

        let masked = engine.mask_text(&input);
        assert!(!masked.blocked);
        assert!(!masked.masked_text.contains(sample_key));
        assert!(!masked.masked_text.contains(sample_jwt));
        assert!(!masked.masked_text.contains("alice@realcorp.io"));
        assert!(!masked.masked_text.contains("ProjectPhoenix"));
        // example.com and allowlisted safe@company.com remain untouched
        assert!(masked.masked_text.contains("bob@example.com"));
        assert!(masked.masked_text.contains("safe@company.com"));
        assert_eq!(engine.unmask_text(&masked.masked_text), input);

        // Split placeholder across streaming SSE chunks
        let mut stream_unmasker = StreamingUnmasker::new();
        let bytes_text = masked.masked_text.as_str();
        let mut reconstructed = String::new();
        for chunk in bytes_text.as_bytes().chunks(5) {
            let s = std::str::from_utf8(chunk).unwrap();
            reconstructed.push_str(&stream_unmasker.push(s, &engine));
        }
        reconstructed.push_str(&stream_unmasker.finish(&engine));
        assert_eq!(reconstructed, input);

        // Luhn card blocking
        let blocked_res = engine.mask_text("charge 4111-1111-1111-1111 now");
        assert!(blocked_res.blocked);
        assert!(blocked_res.block_reason.is_some());
    }

    #[test]
    fn gemini_rosetta_protocol_and_import_links_work() {
        assert_eq!(
            resolve_gemini_endpoint_url("https://generativelanguage.googleapis.com", "gemini-2.5-flash", false),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent"
        );
        assert_eq!(
            resolve_gemini_endpoint_url("https://generativelanguage.googleapis.com/v1beta/openai", "models/gemini-2.5-pro", true),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:streamGenerateContent?alt=sse"
        );

        let openai_body = json!({
            "model": "gemini-2.5-flash",
            "stream": true,
            "messages": [
                {"role": "system", "content": "Be concise."},
                {"role": "user", "content": "Hello Gemini"}
            ]
        });
        let unified = inbound_chat_completions_to_unified(&openai_body).unwrap();
        let g_payload = build_gemini_generate_content_payload(&unified);
        assert_eq!(g_payload["systemInstruction"]["parts"][0]["text"], "Be concise.");
        assert_eq!(g_payload["contents"][0]["parts"][0]["text"], "Hello Gemini");

        let resp_body = json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [
                        {"thought": true, "text": "thinking step"},
                        {"text": "Hello from Gemini!"}
                    ]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 12,
                "candidatesTokenCount": 5,
                "totalTokenCount": 17
            }
        })
        .to_string();
        let parsed = parse_gemini_response_json("google", "gemini-2.5-flash", &resp_body).unwrap();
        assert_eq!(parsed.content, "Hello from Gemini!");
        assert_eq!(parsed.reasoning.as_deref(), Some("thinking step"));
        assert_eq!(parsed.usage.total_tokens, 17);

        let sse_delta = parse_gemini_sse_data(
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"chunk\"}]}}]}",
            0,
        )
        .unwrap()
        .unwrap();
        assert_eq!(sse_delta.text_delta, "chunk");

        let spec = parse_magpie_import_link("magpie://import?preset=zhipu&region=coding&key=sk-glm-123").unwrap();
        assert_eq!(spec.preset_id.as_deref(), Some("zhipu"));
        assert_eq!(spec.region_id.as_deref(), Some("coding"));
        assert_eq!(spec.base_url, "https://open.bigmodel.cn/api/coding/paas/v4");
        assert_eq!(spec.api_key.as_deref(), Some("sk-glm-123"));

        let web_spec = parse_magpie_import_link(
            "https://usemagpie.ai/import?name=Team%20Relay&chat=https://relay.example.com/v1&key=sk-r1&models=deepseek-chat,gpt-4o",
        )
        .unwrap();
        assert_eq!(web_spec.id, "team-relay");
        assert_eq!(web_spec.models, vec!["deepseek-chat", "gpt-4o"]);

        let dotenv_specs = parse_dotenv_providers(
            "export GEMINI_API_KEY=\"AIzaSyDummyTestKey1234567890123456\"\nDEEPSEEK_API_KEY=sk-ds-1234567890123456 # prod\n",
        );
        assert_eq!(dotenv_specs.len(), 2);
        assert!(dotenv_specs.iter().any(|s| s.id == "google"));
        assert!(dotenv_specs.iter().any(|s| s.id == "deepseek"));

        // Responses + Anthropic inbound/outbound adapters
        let resp_req = inbound_responses_to_unified(&json!({
            "model": "gpt-4o",
            "instructions": "Sys",
            "input": [{"type": "message", "role": "user", "content": "Hello"}],
            "max_output_tokens": 512
        }))
        .unwrap();
        assert_eq!(resp_req.system_prompt, "Sys");
        assert_eq!(resp_req.max_tokens, Some(512));

        let ant_req = inbound_anthropic_to_unified(&json!({
            "model": "claude-sonnet-4-20250514",
            "system": "AntSys",
            "max_tokens": 1024,
            "messages": [{"role": "user", "content": [{"type": "text", "text": "Ping"}]}]
        }))
        .unwrap();
        assert_eq!(ant_req.system_prompt, "AntSys");
        assert_eq!(ant_req.messages[0].content, "Ping");

        let chat_out = unified_to_chat_completions_json(&parsed);
        assert_eq!(chat_out["choices"][0]["message"]["content"], "Hello from Gemini!");
        let resp_out = unified_to_responses_json(&parsed);
        assert_eq!(resp_out["output_text"], "Hello from Gemini!");
        let ant_out = unified_to_anthropic_json(&parsed);
        assert_eq!(ant_out["stop_reason"], "end_turn");
    }

    #[test]
    fn routing_cheapest_pace_usage_drain_and_pem_bearer_masking_work() {
        let mut metrics = RouteRuntimeMetrics::default();
        let now = 5_000_000i64;
        let mut groups = HashMap::new();

        // Cheapest strategy prefers free / lowest cost member
        let mut cheap_g = RoutingGroup::new("cost", "Cost", RoutingStrategy::Cheapest);
        cheap_g.members = vec![
            RoutingGroupMember {
                target: "anthropic/claude-opus-4-20250514".to_string(),
                input_cost_per_1m: Some(15.0),
                output_cost_per_1m: Some(75.0),
                ..RoutingGroupMember::new("anthropic/claude-opus-4-20250514")
            },
            RoutingGroupMember {
                target: "deepseek/deepseek-chat".to_string(),
                input_cost_per_1m: Some(0.27),
                output_cost_per_1m: Some(1.10),
                ..RoutingGroupMember::new("deepseek/deepseek-chat")
            },
        ];
        groups.insert("cost".to_string(), cheap_g);
        let cheap_res = resolve_group_targets(
            "group/cost",
            &groups,
            &mut metrics,
            "deepseek",
            None,
            "Compare SQL engines cleanly across read-heavy workloads.",
            2,
            1,
            now,
        )
        .unwrap();
        assert_eq!(cheap_res[0].provider_id, "deepseek");
        assert_eq!(cheap_res[0].reason, "cheapest");

        // Drain stays on first member until in cooldown/quota exhausted
        let mut drain_g = RoutingGroup::new("drain", "Drain", RoutingStrategy::Drain);
        drain_g.members = vec![
            RoutingGroupMember::new("openai/gpt-4o"),
            RoutingGroupMember::new("zhipu/glm-5"),
        ];
        groups.insert("drain".to_string(), drain_g);
        let d1 = resolve_group_targets("drain", &groups, &mut metrics, "openai", None, "Normal task here", 2, 1, now).unwrap();
        assert_eq!(d1[0].provider_id, "openai");
        metrics.record_failure("openai/gpt-4o", Some(429), Some(60), now);
        let d2 = resolve_group_targets("drain", &groups, &mut metrics, "openai", None, "Normal task here", 2, 1, now).unwrap();
        assert_eq!(d2[0].provider_id, "zhipu");

        // Usage + Pace least-used selection
        metrics.record_usage("deepseek/deepseek-chat", 5_000, Some(100), now);
        let mut usage_g = RoutingGroup::new("bal", "Bal", RoutingStrategy::Usage);
        usage_g.members = vec![
            RoutingGroupMember::new("deepseek/deepseek-chat"),
            RoutingGroupMember::new("qwen/qwen-plus"),
        ];
        groups.insert("bal".to_string(), usage_g);
        let u_res = resolve_group_targets("bal", &groups, &mut metrics, "deepseek", None, "Normal task here", 2, 1, now).unwrap();
        assert_eq!(u_res[0].provider_id, "qwen");

        // PEM private key + Bearer token masking
        let eng = MaskingEngine::new(MaskingConfig {
            enabled: true,
            mask_secrets: true,
            mask_personal: true,
            ..Default::default()
        });
        let pem = "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkqhkiG9w0BAQEFAASC\n-----END PRIVATE KEY-----";
        let raw = format!("Auth: Bearer supersecrettoken987654321 and key:\n{pem}");
        let res = eng.mask_text(&raw);
        assert!(!res.masked_text.contains("supersecrettoken987654321"));
        assert!(!res.masked_text.contains("MIIEvQIBADANBgkqhkiG9w0BAQEFAASC"));
        assert_eq!(eng.unmask_text(&res.masked_text), raw);
    }

    #[test]
    fn test_vertex_adc_discovery_and_fallback_parsing() {
        let dir = TempDirGuard::new("vertex_adc_test");
        let home = &dir.path;

        // 1. Full ADC JSON parsing
        let full_adc_json = json!({
            "account": "test-user@example.com",
            "client_id": "mock-client-id-12345.apps.googleusercontent.com",
            "client_secret": "GOCSPX-mockclientsecret12345",
            "quota_project_id": "my-quota-project-999",
            "refresh_token": "1//04mock_refresh_token_abcdef",
            "type": "authorized_user",
            "universe_domain": "googleapis.com"
        })
        .to_string();

        let parsed_creds = parse_vertex_adc_credentials(&full_adc_json).unwrap();
        assert_eq!(
            parsed_creds.client_id,
            "mock-client-id-12345.apps.googleusercontent.com"
        );
        assert_eq!(parsed_creds.client_secret, "GOCSPX-mockclientsecret12345");
        assert_eq!(
            parsed_creds.refresh_token,
            "1//04mock_refresh_token_abcdef"
        );
        assert_eq!(
            parsed_creds.quota_project_id.as_deref(),
            Some("my-quota-project-999")
        );
        assert_eq!(
            parsed_creds.account.as_deref(),
            Some("test-user@example.com")
        );

        // 2. ADC JSON missing quota_project_id and account + config_default fallback
        let partial_adc_json = json!({
            "client_id": "mock-client-id-partial.apps.googleusercontent.com",
            "client_secret": "GOCSPX-partialsecret",
            "refresh_token": "1//04partialrefreshtoken",
            "type": "authorized_user"
        })
        .to_string();

        let gcloud_dir = home.join(".config").join("gcloud");
        std::fs::create_dir_all(&gcloud_dir).unwrap();
        std::fs::write(
            gcloud_dir.join("application_default_credentials.json"),
            &partial_adc_json,
        )
        .unwrap();

        let config_default_content = r#"
[auth]

[core]
project = fallback-project-from-ini
account = fallback-account@example.com
"#;
        let (ini_proj, ini_acct) = parse_gcloud_config_default(config_default_content);
        assert_eq!(ini_proj.as_deref(), Some("fallback-project-from-ini"));
        assert_eq!(ini_acct.as_deref(), Some("fallback-account@example.com"));

        let cfg_dir = gcloud_dir.join("configurations");
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(cfg_dir.join("config_default"), config_default_content).unwrap();

        let scanned = scan_vertex_adc(home, None)
            .expect("scan_vertex_adc should succeed with fallback");
        assert_eq!(scanned.kind, SubscriptionKind::GoogleVertex);
        assert_eq!(
            scanned.email.as_deref(),
            Some("fallback-account@example.com")
        );
        assert_eq!(
            scanned.account_id.as_deref(),
            Some("fallback-project-from-ini")
        );
        assert_eq!(
            scanned
                .headers
                .get("x-goog-user-project")
                .map(String::as_str),
            Some("fallback-project-from-ini")
        );
        assert_eq!(scanned.protocol, ProtocolKind::GeminiGenerateContent);
        assert_eq!(scanned.default_model.as_deref(), Some("gemini-2.5-flash"));
        assert!(scanned.active);

        // 3. Custom path via env parameter
        let custom_dir = home.join("custom_gcloud");
        std::fs::create_dir_all(&custom_dir).unwrap();
        let custom_file = custom_dir.join("custom_adc.json");
        std::fs::write(&custom_file, &full_adc_json).unwrap();
        let scanned_custom = scan_vertex_adc(home, Some(&custom_file)).unwrap();
        assert_eq!(
            scanned_custom.email.as_deref(),
            Some("test-user@example.com")
        );
        assert_eq!(
            scanned_custom.account_id.as_deref(),
            Some("my-quota-project-999")
        );
    }

    #[test]
    fn test_magpie_providers_and_logins_parsing() {
        let dir = TempDirGuard::new("magpie_state_test");
        let home = &dir.path;

        let magpie_dir = home.join(".config").join("magpie");
        std::fs::create_dir_all(&magpie_dir).unwrap();

        // 1. providers.json with google-vertex
        let providers_json = json!({
            "providers": [
                {
                    "id": "gemini",
                    "name": "Google Gemini",
                    "off": true
                },
                {
                    "id": "google-vertex",
                    "name": "Google Vertex AI",
                    "icon": "vertexai-color",
                    "preset": "google-vertex",
                    "key": "",
                    "vertex": {
                        "project": "project-07a836a9-9c7f-46b5-82b",
                        "location": "global",
                        "credentials": "/mock/path/application_default_credentials.json"
                    },
                    "website": "https://docs.cloud.google.com"
                }
            ]
        })
        .to_string();

        let prov_accounts = parse_magpie_providers(&providers_json);
        assert_eq!(prov_accounts.len(), 1);
        let vertex_prov = &prov_accounts[0];
        assert_eq!(vertex_prov.id, "magpie_provider:google-vertex");
        assert_eq!(vertex_prov.kind, SubscriptionKind::GoogleVertex);
        assert_eq!(
            vertex_prov.account_id.as_deref(),
            Some("project-07a836a9-9c7f-46b5-82b")
        );
        assert_eq!(
            vertex_prov
                .headers
                .get("x-goog-user-project")
                .map(String::as_str),
            Some("project-07a836a9-9c7f-46b5-82b")
        );

        // 2. logins.json with multi-account Codex and Gemini sessions
        let jwt_hdr = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
        let jwt_claims_1 = URL_SAFE_NO_PAD.encode(
            br#"{"email":"b@ombhrum.com","exp":1893456000,"https://api.openai.com/auth":{"chatgpt_account_id":"acct_alpha","chatgpt_plan_type":"team"}}"#,
        );
        let jwt_1 = format!("{jwt_hdr}.{jwt_claims_1}.sig");

        let jwt_claims_2 = URL_SAFE_NO_PAD.encode(
            br#"{"email":"rick@example.com","exp":1893456000,"https://api.openai.com/auth":{"chatgpt_account_id":"acct_beta","chatgpt_plan_type":"team"}}"#,
        );
        let jwt_2 = format!("{jwt_hdr}.{jwt_claims_2}.sig");

        let logins_json = json!([
            {
                "id": "53153f1af1c2a3cc",
                "order": 0,
                "agent": "codex",
                "user": "b@ombhrum.com · Team",
                "plan": "team",
                "seen": 1731800000000i64,
                "on": true,
                "auth": {
                    "tokens": {
                        "access_token": jwt_1,
                        "refresh_token": "rt_mock_1",
                        "account_id": "acct_alpha"
                    }
                }
            },
            {
                "id": "890db74b8ec4f574",
                "order": 1,
                "agent": "codex",
                "user": "rick@example.com · Team",
                "plan": "team",
                "seen": 1731800000000i64,
                "on": true,
                "auth": {
                    "tokens": {
                        "access_token": jwt_2,
                        "refresh_token": "rt_mock_2",
                        "account_id": "acct_beta"
                    }
                }
            },
            {
                "id": "017ab2e39f452d1b",
                "order": 2,
                "agent": "gemini",
                "user": "arapacanala@gmail.com",
                "plan": null,
                "on": true,
                "auth": {
                    "access_token": "ya29.mock_gemini_token",
                    "refresh_token": "1//mock_refresh_gemini",
                    "expiry_date": chrono::Utc::now().timestamp_millis() + 3_600_000
                }
            }
        ])
        .to_string();

        let now_ms = chrono::Utc::now().timestamp_millis();
        let parsed_logins = parse_magpie_logins(&logins_json, now_ms);
        assert_eq!(parsed_logins.len(), 3);

        let codex_1 = parsed_logins
            .iter()
            .find(|a| a.id == "magpie_codex:53153f1af1c2a3cc")
            .unwrap();
        assert_eq!(codex_1.kind, SubscriptionKind::CodexChatGpt);
        assert_eq!(codex_1.email.as_deref(), Some("b@ombhrum.com"));
        assert_eq!(codex_1.account_id.as_deref(), Some("acct_alpha"));
        assert_eq!(codex_1.plan.as_deref(), Some("team"));
        assert_eq!(
            codex_1.headers.get("originator").map(String::as_str),
            Some("magpie")
        );
        assert_eq!(
            codex_1
                .headers
                .get("ChatGPT-Account-ID")
                .map(String::as_str),
            Some("acct_alpha")
        );
        assert!(codex_1.active);

        let gemini_1 = parsed_logins
            .iter()
            .find(|a| a.id == "magpie_gemini:017ab2e39f452d1b")
            .unwrap();
        assert_eq!(gemini_1.kind, SubscriptionKind::GeminiCodeAssist);
        assert_eq!(gemini_1.email.as_deref(), Some("arapacanala@gmail.com"));
        assert!(gemini_1.active);

        // 3. scan_magpie_dir integration
        std::fs::write(magpie_dir.join("providers.json"), &providers_json).unwrap();
        std::fs::write(magpie_dir.join("logins.json"), &logins_json).unwrap();
        let mut out = Vec::new();
        scan_magpie_dir(&magpie_dir, now_ms, &mut out);
        assert_eq!(out.len(), 4); // 1 provider + 3 logins
    }

    #[test]
    fn test_claude_settings_and_gemini_jetski_discovery() {
        let dir = TempDirGuard::new("claude_gemini_ext_test");
        let home = &dir.path;

        // 1. ~/.claude/settings.json
        let claude_dir = home.join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        std::fs::write(
            claude_dir.join("settings.json"),
            json!({
                "model": "codex/gpt-6.1-sol",
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "sk-ant-settings-secret-12345",
                    "ANTHROPIC_BASE_URL": "http://127.0.0.1:3425"
                }
            })
            .to_string(),
        )
        .unwrap();

        let mut claude_accounts = Vec::new();
        scan_claude_dirs(home, chrono::Utc::now().timestamp_millis(), &mut claude_accounts);
        let settings_acc = claude_accounts
            .iter()
            .find(|a| a.id == "claude_code:settings")
            .expect("should discover claude_code:settings");
        assert_eq!(settings_acc.kind, SubscriptionKind::ClaudeCode);
        assert_eq!(
            settings_acc.base_url.as_deref(),
            Some("http://127.0.0.1:3425")
        );
        assert_eq!(
            settings_acc.default_model.as_deref(),
            Some("codex/gpt-6.1-sol")
        );
        assert_eq!(
            settings_acc.access_token.as_deref(),
            Some("sk-ant-settings-secret-12345")
        );
        assert!(settings_acc.active);

        // 2. ~/.gemini/jetski-standalone-oauth-token
        let gemini_dir = home.join(".gemini");
        std::fs::create_dir_all(&gemini_dir).unwrap();
        std::fs::write(
            gemini_dir.join("jetski-standalone-oauth-token"),
            json!({
                "token": {
                    "access_token": "ya29.jetski_access_token_mock",
                    "token_type": "Bearer",
                    "refresh_token": "1//jetski_refresh_token_mock",
                    "expiry": "2030-01-01T00:00:00Z"
                },
                "auth_method": "consumer",
                "id_token_email": "bhrum21@gmail.com"
            })
            .to_string(),
        )
        .unwrap();

        let mut gemini_accounts = Vec::new();
        scan_gemini_dirs(
            &gemini_dir,
            chrono::Utc::now().timestamp_millis(),
            &mut gemini_accounts,
        );
        let jetski_acc = gemini_accounts
            .iter()
            .find(|a| a.id == "gemini_jetski:bhrum21@gmail.com")
            .expect("should discover gemini_jetski");
        assert_eq!(jetski_acc.kind, SubscriptionKind::GeminiCodeAssist);
        assert_eq!(jetski_acc.email.as_deref(), Some("bhrum21@gmail.com"));
        assert_eq!(jetski_acc.plan.as_deref(), Some("consumer"));
        assert!(jetski_acc.active);
    }

    #[test]
    fn test_vertex_oauth_token_refresh_and_cache_lead_time() {
        let creds = VertexAdcCredentials {
            client_id: "test-client-id.apps.googleusercontent.com".to_string(),
            client_secret: "test-client-secret-987".to_string(),
            refresh_token: "1//test-refresh-token-xyz".to_string(),
            quota_project_id: Some("my-quota-proj".to_string()),
            account: Some("test@example.com".to_string()),
        };

        let now_ms = 1_000_000_000_000i64;
        let lead_time_ms = DEFAULT_TOKEN_REFRESH_LEAD_TIME_MS;

        let cached = CachedVertexToken::new("ya29.cached_token".to_string(), 3600, now_ms);
        assert_eq!(cached.expires_at_ms, now_ms + 3_600_000);
        assert!(cached.is_fresh(now_ms, lead_time_ms));
        assert!(cached.is_fresh(now_ms + 3_200_000, lead_time_ms));
        assert!(!cached.is_fresh(now_ms + 3_400_000, lead_time_ms));
        assert!(!cached.is_fresh(now_ms + 3_700_000, lead_time_ms));

        let mgr = VertexTokenManager::new(creds.clone());
        assert_eq!(mgr.get_valid_cached_token(now_ms), None);

        mgr.set_cached_token(cached.clone());
        assert_eq!(
            mgr.get_valid_cached_token(now_ms).as_deref(),
            Some("ya29.cached_token")
        );
        assert_eq!(mgr.get_valid_cached_token(now_ms + 3_400_000), None);

        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let mock_token_url = format!("http://127.0.0.1:{port}/token");

        let srv = std::thread::spawn(move || {
            if let Ok((mut socket, _)) = listener.accept() {
                let _ = socket.set_read_timeout(Some(std::time::Duration::from_secs(5)));
                let mut raw = Vec::new();
                let mut tmp = [0u8; 2048];
                let mut header_end: Option<usize> = None;
                let mut content_len = 0usize;
                loop {
                    match socket.read(&mut tmp) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            raw.extend_from_slice(&tmp[..n]);
                            if header_end.is_none() {
                                if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                                    let h_end = pos + 4;
                                    header_end = Some(h_end);
                                    for line in String::from_utf8_lossy(&raw[..h_end]).lines() {
                                        if let Some((k, v)) = line.split_once(':') {
                                            if k.trim().eq_ignore_ascii_case("content-length") {
                                                content_len = v.trim().parse().unwrap_or(0);
                                            }
                                        }
                                    }
                                }
                            }
                            if let Some(h_end) = header_end {
                                if raw.len() >= h_end + content_len {
                                    break;
                                }
                            }
                        }
                    }
                }
                let req_str = String::from_utf8_lossy(&raw);
                assert!(req_str.starts_with("POST "));
                assert!(req_str.contains("grant_type=refresh_token"));
                assert!(req_str.contains("client_id=test-client-id"));
                assert!(req_str.contains("refresh_token=1%2F%2Ftest-refresh-token-xyz"));

                let body = json!({
                    "access_token": "ya29.mock_refreshed_access_token_success",
                    "expires_in": 3600,
                    "token_type": "Bearer",
                    "scope": "https://www.googleapis.com/auth/cloud-platform"
                })
                .to_string();

                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(resp.as_bytes());
            }
        });

        let agent = ureq::Agent::new();
        let fresh_mgr = VertexTokenManager::new(creds).with_token_url(mock_token_url);

        let token = fresh_mgr.get_or_refresh_token(&agent, now_ms).unwrap();
        assert_eq!(token, "ya29.mock_refreshed_access_token_success");

        let cached_entry = fresh_mgr.cached_token().expect("token should be cached");
        assert_eq!(
            cached_entry.access_token,
            "ya29.mock_refreshed_access_token_success"
        );

        let cached_token = fresh_mgr.get_or_refresh_token(&agent, now_ms + 1000).unwrap();
        assert_eq!(cached_token, "ya29.mock_refreshed_access_token_success");
        srv.join().unwrap();
    }

    #[test]
    fn test_google_vertex_presets_catalog_and_resolution() {
        // 1. find_preset by canonical name and aliases
        let p1 = find_preset("google-vertex").expect("google-vertex preset not found");
        assert_eq!(p1.id, "google-vertex");
        assert_eq!(p1.name, "Google Vertex AI");
        assert_eq!(p1.default_protocol, ProtocolKind::GeminiGenerateContent);
        assert_eq!(p1.default_model, "gemini-2.5-flash");
        assert!(p1
            .suggested_models
            .contains(&"gemini-2.5-flash".to_string()));
        assert!(p1.suggested_models.contains(&"gemini-2.5-pro".to_string()));
        assert!(p1.suggested_models.contains(&"gemini-1.5-pro".to_string()));
        assert!(p1
            .header_hints
            .contains(&"x-goog-user-project".to_string()));

        let p_alias1 = find_preset("vertex").expect("vertex alias not found");
        assert_eq!(p_alias1.id, "google-vertex");

        let p_alias2 = find_preset("vertex-ai").expect("vertex-ai alias not found");
        assert_eq!(p_alias2.id, "google-vertex");

        // 2. resolve_preset_endpoints
        let ep_default = resolve_preset_endpoints("google-vertex", None, None).unwrap();
        assert_eq!(ep_default.preset_id, "google-vertex");
        assert_eq!(ep_default.protocol, ProtocolKind::GeminiGenerateContent);
        assert_eq!(ep_default.default_model, "gemini-2.5-flash");

        let ep_central =
            resolve_preset_endpoints("google-vertex", Some("us-central1"), None).unwrap();
        assert_eq!(ep_central.region_id.as_deref(), Some("us-central1"));
        assert!(ep_central.base_url.contains("us-central1"));

        let ep_global =
            resolve_preset_endpoints("google-vertex", Some("global"), None).unwrap();
        assert_eq!(ep_global.region_id.as_deref(), Some("global"));
        assert_eq!(ep_global.base_url, "https://aiplatform.googleapis.com/v1");
    }

    #[test]
    fn test_full_integrated_home_discovery_with_vertex_magpie_and_all_providers() {
        let dir = TempDirGuard::new("full_home_discovery_test");
        let home = &dir.path;

        // 1. Google Vertex ADC + config_default
        let gcloud_dir = home.join(".config").join("gcloud");
        std::fs::create_dir_all(gcloud_dir.join("configurations")).unwrap();
        std::fs::write(
            gcloud_dir.join("application_default_credentials.json"),
            json!({
                "client_id": "cid-vertex-999.apps.googleusercontent.com",
                "client_secret": "csec-vertex-999",
                "refresh_token": "1//rt-vertex-999",
                "type": "authorized_user"
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            gcloud_dir.join("configurations").join("config_default"),
            "[core]\nproject = gcp-prod-vertex\naccount = bhrum@example.com\n",
        )
        .unwrap();

        // 2. Magpie providers.json & logins.json
        let magpie_dir = home.join(".config").join("magpie");
        std::fs::create_dir_all(&magpie_dir).unwrap();
        std::fs::write(
            magpie_dir.join("providers.json"),
            json!({
                "providers": [
                    {
                        "id": "google-vertex",
                        "name": "Google Vertex AI",
                        "preset": "google-vertex",
                        "vertex": {
                            "project": "gcp-prod-vertex",
                            "location": "global"
                        }
                    }
                ]
            })
            .to_string(),
        )
        .unwrap();

        let jwt_hdr = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
        let jwt_claims = URL_SAFE_NO_PAD.encode(
            br#"{"email":"codex-dev@example.com","exp":2000000000,"https://api.openai.com/auth":{"chatgpt_account_id":"acct-codex-1","chatgpt_plan_type":"team"}}"#,
        );
        let jwt = format!("{jwt_hdr}.{jwt_claims}.sig");

        std::fs::write(
            magpie_dir.join("logins.json"),
            json!([
                {
                    "id": "mag_codex_1",
                    "order": 0,
                    "agent": "codex",
                    "user": "codex-dev@example.com · Team",
                    "plan": "team",
                    "on": true,
                    "auth": {
                        "tokens": {
                            "access_token": jwt,
                            "refresh_token": "rt_mag_codex"
                        }
                    }
                },
                {
                    "id": "mag_gem_1",
                    "order": 1,
                    "agent": "gemini",
                    "user": "gem-dev@example.com",
                    "on": true,
                    "auth": {
                        "access_token": "ya29.mag_gem_tok",
                        "refresh_token": "1//mag_gem_rt"
                    }
                }
            ])
            .to_string(),
        )
        .unwrap();

        // 3. Codex CLI
        let codex_dir = home.join(".codex");
        std::fs::create_dir_all(&codex_dir).unwrap();
        std::fs::write(
            codex_dir.join("auth.json"),
            json!({
                "tokens": {
                    "access_token": jwt,
                    "refresh_token": "rt_codex_cli"
                }
            })
            .to_string(),
        )
        .unwrap();

        // 4. Claude Code Settings + Credentials
        let claude_dir = home.join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        std::fs::write(
            claude_dir.join("settings.json"),
            json!({
                "model": "claude-sonnet-4-20250514",
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "sk-ant-tok-full",
                    "ANTHROPIC_BASE_URL": "https://api.anthropic.com/v1"
                }
            })
            .to_string(),
        )
        .unwrap();

        // 5. GitHub Copilot
        let copilot_dir = home.join(".config").join("github-copilot");
        std::fs::create_dir_all(&copilot_dir).unwrap();
        std::fs::write(
            copilot_dir.join("hosts.json"),
            json!({
                "github.com": {
                    "user": "octocat-dev",
                    "oauth_token": "ghu_octocat_tok"
                }
            })
            .to_string(),
        )
        .unwrap();

        // 6. Gemini Jetski Standalone
        let gemini_dir = home.join(".gemini");
        std::fs::create_dir_all(&gemini_dir).unwrap();
        std::fs::write(
            gemini_dir.join("jetski-standalone-oauth-token"),
            json!({
                "token": {
                    "access_token": "ya29.jetski_standalone",
                    "refresh_token": "1//jetski_rt",
                    "expiry": "2030-01-01T00:00:00Z"
                },
                "auth_method": "consumer",
                "id_token_email": "jetski-user@gmail.com"
            })
            .to_string(),
        )
        .unwrap();

        // 7. Antigravity
        let agy_dir = home.join(".antigravity");
        std::fs::create_dir_all(&agy_dir).unwrap();
        std::fs::write(
            agy_dir.join("accounts.json"),
            json!({
                "accounts": [
                    {
                        "email": "agy-user@gmail.com",
                        "refresh_token": "1//agy_rt",
                        "plan": "pro"
                    }
                ]
            })
            .to_string(),
        )
        .unwrap();

        // 8. Grok SuperGrok
        let grok_dir = home.join(".grok");
        std::fs::create_dir_all(&grok_dir).unwrap();
        std::fs::write(
            grok_dir.join("session.json"),
            json!({
                "email": "grok-user@example.com",
                "token": "xai-sso-token-12345"
            })
            .to_string(),
        )
        .unwrap();

        let reg = discover_subscriptions_in_home(home, None);
        assert!(reg.has_active_subscription());

        // Check discovered families
        let kinds: Vec<SubscriptionKind> = reg.accounts.iter().map(|a| a.kind).collect();
        assert!(kinds.contains(&SubscriptionKind::GoogleVertex));
        assert!(kinds.contains(&SubscriptionKind::CodexChatGpt));
        assert!(kinds.contains(&SubscriptionKind::ClaudeCode));
        assert!(kinds.contains(&SubscriptionKind::GitHubCopilot));
        assert!(kinds.contains(&SubscriptionKind::GeminiCodeAssist));
        assert!(kinds.contains(&SubscriptionKind::Antigravity));
        assert!(kinds.contains(&SubscriptionKind::GrokSuperGrok));

        // Check vertex ADC details
        let vertex_acc = reg.accounts.iter().find(|a| a.id.starts_with("google_vertex:")).unwrap();
        assert_eq!(vertex_acc.email.as_deref(), Some("bhrum@example.com"));
        assert_eq!(vertex_acc.account_id.as_deref(), Some("gcp-prod-vertex"));
        assert_eq!(vertex_acc.headers.get("x-goog-user-project").map(String::as_str), Some("gcp-prod-vertex"));

        // Check Magpie provider details
        let mag_prov = reg.accounts.iter().find(|a| a.id == "magpie_provider:google-vertex").unwrap();
        assert_eq!(mag_prov.account_id.as_deref(), Some("gcp-prod-vertex"));

        // Check Magpie logins
        let mag_codex = reg.accounts.iter().find(|a| a.id == "magpie_codex:mag_codex_1").unwrap();
        assert_eq!(mag_codex.email.as_deref(), Some("codex-dev@example.com"));
        assert_eq!(mag_codex.plan.as_deref(), Some("team"));

        // Verify JSON serialization omits secrets
        let serialized = serde_json::to_string(&reg).unwrap();
        assert!(!serialized.contains("csec-vertex-999"));
        assert!(!serialized.contains("1//rt-vertex-999"));
        assert!(!serialized.contains("rt_mag_codex"));
        assert!(!serialized.contains("ya29.mag_gem_tok"));
        assert!(!serialized.contains("sk-ant-tok-full"));
        assert!(!serialized.contains("ghu_octocat_tok"));
        assert!(!serialized.contains("ya29.jetski_standalone"));
        assert!(!serialized.contains("xai-sso-token-12345"));
    }

    #[test]
    fn test_host_discovery_on_real_user_environment() {
        let real_home = std::path::Path::new("/Users/bhrum");
        if real_home.is_dir() {
            let reg = discover_subscriptions_in_home(real_home, None);
            println!("Discovered {} real accounts on host:", reg.accounts.len());
            for acc in &reg.accounts {
                println!(
                    " - [{:?}] {} (id={}, active={}, model={:?})",
                    acc.kind,
                    acc.label,
                    acc.id,
                    acc.active,
                    acc.default_model
                );
            }
            // On user's host, gcloud ADC and Magpie are installed
            assert!(
                reg.accounts.iter().any(|a| a.kind == SubscriptionKind::GoogleVertex),
                "Real host should discover Google Vertex AI ADC"
            );
            assert!(
                reg.accounts.iter().any(|a| a.kind == SubscriptionKind::CodexChatGpt),
                "Real host should discover Codex / ChatGPT"
            );
            assert!(
                reg.accounts.iter().any(|a| a.kind == SubscriptionKind::GeminiCodeAssist),
                "Real host should discover Gemini"
            );
            assert!(
                reg.has_active_subscription(),
                "Real host should have at least one active subscription"
            );
        }
    }

    #[test]
    fn test_routing_strategy_fastest_evaluates_latency_and_tps() {
        let mut groups = HashMap::new();
        let mut group = RoutingGroup::new("latency-pool", "Latency Pool", RoutingStrategy::Fastest);
        group.members = vec![
            RoutingGroupMember::new("provider_slow/model_a"),
            RoutingGroupMember::new("provider_fast/model_b"),
            RoutingGroupMember::new("provider_mid/model_c"),
        ];
        groups.insert("latency-pool".to_string(), group);

        let mut metrics = RouteRuntimeMetrics::default();
        let now = 1_000_000i64;

        // Slow seat: 400ms TTFT, 20 TPS
        metrics.record_metrics("provider_slow/model_a", 100, Some(500), Some(400), Some(20.0), now);
        // Fast seat: 80ms TTFT, 120 TPS
        metrics.record_metrics("provider_fast/model_b", 100, Some(120), Some(80), Some(120.0), now);
        // Mid seat: 200ms TTFT, 50 TPS
        metrics.record_metrics("provider_mid/model_c", 100, Some(250), Some(200), Some(50.0), now);

        let targets = resolve_group_targets(
            "group/latency-pool",
            &groups,
            &mut metrics,
            "default",
            None,
            "Normal query for fastest model",
            1,
            0,
            now,
        )
        .expect("resolve targets");

        assert_eq!(targets[0].provider_id, "provider_fast");
        assert_eq!(targets[0].model, "model_b");
        assert_eq!(targets[0].strategy, RoutingStrategy::Fastest);
        assert_eq!(targets[0].reason, "fastest_latency");

        assert_eq!(targets[1].provider_id, "provider_mid");
        assert_eq!(targets[2].provider_id, "provider_slow");

        // When fastest model enters rest / cooldown, mid model takes priority
        metrics.record_failure("provider_fast/model_b", Some(429), Some(60), now);
        let targets_after_rest = resolve_group_targets(
            "group/latency-pool",
            &groups,
            &mut metrics,
            "default",
            None,
            "Normal query for fastest model",
            1,
            0,
            now,
        )
        .expect("resolve after rest");

        assert_eq!(targets_after_rest[0].provider_id, "provider_mid");
        assert_eq!(targets_after_rest[1].provider_id, "provider_slow");
        // Cooled model placed last
        assert_eq!(targets_after_rest[2].provider_id, "provider_fast");
    }

    #[test]
    fn test_circuit_breaker_rest_ladders() {
        let mut cb = CircuitBreaker::default();
        let now = 10_000_000i64;
        let seat = "provider_a/account_1";

        // 1. CreditRest ladder progression: [30m, 1h, 2h, 4h, 8h, 24h, 7d]
        for (step, &expected_dur) in CREDIT_REST_LADDER_MS.iter().enumerate() {
            let res = cb
                .record_upstream_error(
                    seat,
                    "provider_a",
                    402,
                    "Your balance is insufficient or credit expired",
                    None,
                    None,
                    now,
                )
                .expect("record error")
                .expect("rest state applied");
            assert_eq!(res.kind, RestKind::CreditRest);
            assert_eq!(res.until_ms, now + expected_dur, "failed at step {step}");
            assert!(cb.is_in_rest(seat, now + expected_dur - 100));
            assert!(!cb.is_in_rest(seat, now + expected_dur + 100));
        }

        // 2. OverloadRest: 30s
        let res_overload = cb
            .record_upstream_error(
                "provider_b/account_1",
                "provider_b",
                503,
                "Service unavailable, server is overloaded",
                None,
                None,
                now,
            )
            .expect("record overload")
            .expect("overload rest applied");
        assert_eq!(res_overload.kind, RestKind::OverloadRest);
        assert_eq!(res_overload.until_ms, now + OVERLOAD_REST_MS);

        // 3. RateRest exponential backoff
        let res_rate1 = cb
            .record_upstream_error(
                "provider_c/account_1",
                "provider_c",
                429,
                "Rate limit exceeded. Please slow down.",
                None,
                None,
                now,
            )
            .expect("record rate")
            .expect("rate rest applied");
        assert_eq!(res_rate1.kind, RestKind::RateRest);
        assert_eq!(res_rate1.until_ms, now + BASE_RATE_REST_MS);

        // Second failure doubles backoff
        let res_rate2 = cb
            .record_upstream_error(
                "provider_c/account_1",
                "provider_c",
                429,
                "Rate limit exceeded.",
                None,
                None,
                now,
            )
            .expect("record rate 2")
            .expect("rate rest 2 applied");
        assert_eq!(res_rate2.until_ms, now + BASE_RATE_REST_MS * 2);

        // 4. QuotaRest with explicit reset timestamp
        let reset_target = now + 45 * 60 * 1000;
        let res_quota = cb
            .record_upstream_error(
                "provider_d/account_1",
                "provider_d",
                429,
                "Monthly quota exhausted",
                None,
                Some(reset_target),
                now,
            )
            .expect("record quota")
            .expect("quota rest applied");
        assert_eq!(res_quota.kind, RestKind::QuotaRest);
        assert_eq!(res_quota.until_ms, reset_target);
    }

    #[test]
    fn test_forbidden_alike_disambiguation_multi_account() {
        let mut cb = CircuitBreaker::default();
        let now = 20_000_000i64;

        let acct1 = "deepseek/user_acc_1";
        let acct2 = "deepseek/user_acc_2";
        let acct3 = "deepseek/user_acc_3";

        let error_body = "Forbidden: The prompt triggered our regional safety filter policy.";

        // Inherent request-level rejection check
        assert!(cb.is_forbidden_alike("deepseek", 403, error_body));

        // When Account 1 hits this error:
        let res1 = cb
            .record_upstream_error(acct1, "deepseek", 403, error_body, None, None, now)
            .expect("record error 1");
        // Because error_body matches policy/safety keywords, it is disambiguated immediately without resting the account!
        assert!(res1.is_none(), "should not rest account on request policy rejection");
        assert!(!cb.is_in_rest(acct1, now));

        // Test non-keyword identical 500 error across multiple accounts:
        let internal_err = "Internal upstream cluster 502 gateway error on shard 42";
        let res_a = cb
            .record_upstream_error(acct1, "deepseek", 502, internal_err, None, None, now)
            .expect("record first error");
        assert!(res_a.is_some(), "first account might get temporary rest before pattern emerges");

        // When second distinct account receives the IDENTICAL error text within 60s:
        let res_b = cb
            .record_upstream_error(acct2, "deepseek", 502, internal_err, None, None, now + 1000)
            .expect("record second error");
        // Disambiguated as forbiddenAlike! Rest is prevented.
        assert!(res_b.is_none(), "second account should be protected by forbiddenAlike");
        assert!(!cb.is_in_rest(acct2, now + 1000));
        // Account 1's rest was also automatically cleared upon pattern detection!
        assert!(!cb.is_in_rest(acct1, now + 1000));

        // Third account also protected:
        let res_c = cb
            .record_upstream_error(acct3, "deepseek", 502, internal_err, None, None, now + 2000)
            .expect("record third error");
        assert!(res_c.is_none(), "subsequent accounts protected by known forbiddenAlike");
        assert!(!cb.is_in_rest(acct3, now + 2000));
    }

    #[test]
    fn test_route_runtime_metrics_forbidden_alike_seat_rehabilitation() {
        let mut metrics = RouteRuntimeMetrics::default();
        let now = 100_000i64;
        let acct1 = "prov/acc1";
        let acct2 = "prov/acc2";
        let error_msg = "Upstream cluster connection reset by peer (502)";

        // Step 1: Account 1 fails with 502
        let rested1 = metrics.record_failure_disambiguated(
            acct1, "prov", 502, error_msg, None, None, now,
        );
        assert!(rested1, "first account should be placed in rest");
        assert!(metrics.circuit_breaker.is_in_rest(acct1, now));
        assert!(metrics.seats.get(acct1).unwrap().is_in_cooldown(now));

        // Step 2: Account 2 encounters identical 502 within 60s
        let rested2 = metrics.record_failure_disambiguated(
            acct2, "prov", 502, error_msg, None, None, now + 1_000,
        );
        assert!(!rested2, "forbiddenAlike disambiguation should suppress rest on second account");
        assert!(!metrics.circuit_breaker.is_in_rest(acct2, now + 1_000));
        assert!(!metrics.circuit_breaker.is_in_rest(acct1, now + 1_000));

        // Step 3: Verify Account 1 seat cooldown was rehabilitated in RouteRuntimeMetrics.seats
        let seat1 = metrics.seats.get(acct1).unwrap();
        assert_eq!(seat1.cooldown_until_ms, 0, "seat1 cooldown_until_ms must be reset to 0");
        assert_eq!(seat1.consecutive_failures, 0, "seat1 consecutive_failures must be reset to 0");
        assert!(!seat1.is_in_cooldown(now + 1_000), "seat1 must NOT be in cooldown after forbiddenAlike triggers");
    }

    #[test]
    fn test_protocol_bidirectional_conversions_and_sse() {
        // 1. UnifiedRequest -> OpenAI request JSON
        let req = UnifiedRequest {
            model: "gpt-4o".to_string(),
            system_prompt: "You are a helpful coding assistant.".to_string(),
            messages: vec![
                UnifiedMessage {
                    role: UnifiedRole::User,
                    content: "Can you implement quicksort in Rust?".to_string(),
                    name: None,
                    tool_call_id: None,
                    tool_calls: Vec::new(),
                    reasoning: None,
                },
            ],
            tools: vec![
                UnifiedToolSpec {
                    name: "execute_code".to_string(),
                    description: "Execute snippet".to_string(),
                    parameters: json!({"type": "object", "properties": {"code": {"type": "string"}}}),
                }
            ],
            temperature: Some(0.7),
            max_tokens: Some(1024),
            thinking_budget_tokens: None,
            stream: true,
        };

        let openai_req = unified_to_chat_completions_request_json(&req);
        assert_eq!(openai_req["model"], "gpt-4o");
        assert_eq!(openai_req["messages"][0]["role"], "system");
        assert_eq!(openai_req["messages"][1]["role"], "user");
        assert_eq!(openai_req["tools"][0]["function"]["name"], "execute_code");
        assert_eq!(openai_req["stream"], true);

        // 2. UnifiedRequest -> Anthropic request JSON
        let anthropic_req = unified_to_anthropic_request_json(&req);
        assert_eq!(anthropic_req["model"], "gpt-4o");
        assert_eq!(anthropic_req["system"], "You are a helpful coding assistant.");
        assert_eq!(anthropic_req["messages"][0]["role"], "user");
        assert_eq!(anthropic_req["tools"][0]["name"], "execute_code");

        // 3. Response parsing
        let openai_resp_raw = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Here is quicksort in Rust",
                    "reasoning_content": "Algorithm plan..."
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 15,
                "completion_tokens": 40,
                "total_tokens": 55
            }
        }).to_string();

        let parsed_openai = parse_chat_completions_response_json("openai", "gpt-4o", &openai_resp_raw).unwrap();
        assert_eq!(parsed_openai.content, "Here is quicksort in Rust");
        assert_eq!(parsed_openai.reasoning.as_deref(), Some("Algorithm plan..."));
        assert_eq!(parsed_openai.usage.total_tokens, 55);

        // 4. SSE chunk decoding & encoding
        let sse_line = "data: {\"id\":\"chatcmpl-123\",\"choices\":[{\"delta\":{\"content\":\" quick\",\"reasoning_content\":\"step 1\"},\"finish_reason\":null}]}";
        let chunk = parse_openai_sse_chunk(sse_line).unwrap().expect("parsed chunk");
        assert_eq!(chunk.text_delta, " quick");
        assert_eq!(chunk.reasoning_delta.as_deref(), Some("step 1"));

        let encoded_sse = encode_openai_chat_chunk("chatcmpl-123", "gpt-4o", &chunk);
        assert!(encoded_sse.starts_with("data: "));
        assert!(encoded_sse.contains("\"content\":\" quick\""));

        // Anthropic SSE event decoding & encoding
        let ant_event = parse_anthropic_sse_event(
            "content_block_delta",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hello anthropic\"}}",
        )
        .unwrap()
        .expect("parsed anthropic chunk");
        assert_eq!(ant_event.text_delta, "hello anthropic");

        let ant_encoded = encode_anthropic_sse_events("msg_123", "claude-3-5-sonnet", &ant_event, 0);
        assert!(!ant_encoded.is_empty());
        assert!(ant_encoded[0].contains("event: content_block_delta"));
        assert!(ant_encoded[0].contains("hello anthropic"));
    }

    #[test]
    fn test_session_header_normalization_and_usage_ledger() {
        let dir = TempDirGuard::new("usage_ledger_test");
        let ledger_path = dir.path.join("usage.jsonl");

        // 1. Session header normalization across diverse client variants
        for (header_name, header_val) in [
            ("x-magpie-session", "sess_magpie_alpha"),
            ("x-conversation-id", "sess_conv_beta"),
            ("x-session-affinity", "sess_aff_gamma"),
            ("session_id", "sess_snake_delta"),
            ("session-id", "sess_kebab_epsilon"),
            ("x-codex-window-id", "sess_codex_zeta"),
            ("x-deepseek-harness-session-id", "sess_deepseek_eta"),
        ] {
            let mut map = HashMap::new();
            map.insert(header_name.to_string(), header_val.to_string());
            let normalized = normalize_session_from_map(&map);
            assert_eq!(normalized.as_deref(), Some(header_val));
        }

        // 2. Ledger recording and JSONL persistence
        let ledger = UsageLedger::new(Some(ledger_path.clone()));
        let now = 30_000_000i64;

        let rec1 = UsageRecord::new(
            "rec_1",
            "agent_sess_1",
            "openai",
            "gpt-4o",
            100,
            200,
            20,
            50,
            Some(120),
            Some(800),
            now,
        );
        let rec2 = UsageRecord::new(
            "rec_2",
            "agent_sess_1",
            "deepseek",
            "deepseek-chat",
            500,
            1000,
            100,
            0,
            Some(80),
            Some(1500),
            now + 2000,
        );
        let rec3 = UsageRecord::new(
            "rec_3",
            "agent_sess_2",
            "anthropic",
            "claude-3-5-sonnet",
            300,
            400,
            0,
            0,
            Some(150),
            Some(1200),
            now + 4000,
        );

        ledger.record(rec1).unwrap();
        ledger.record(rec2).unwrap();
        ledger.record(rec3).unwrap();

        assert_eq!(ledger.total_entries(), 3);

        // Summary rollups
        let sum_all = ledger.summary(None);
        assert_eq!(sum_all.total_records, 3);
        assert_eq!(sum_all.total_input_tokens, 900);
        assert_eq!(sum_all.total_output_tokens, 1600);
        assert_eq!(sum_all.total_tokens, 2500);
        assert!(sum_all.total_cost_usd > 0.0);
        assert!(sum_all.avg_ttft_ms.is_some());

        // Session-filtered summary
        let sum_sess1 = ledger.summary_for_session("agent_sess_1");
        assert_eq!(sum_sess1.total_records, 2);
        assert_eq!(sum_sess1.total_tokens, 1800);

        // Verify JSONL file persistence & reload
        assert!(ledger_path.exists());
        let reloaded_ledger = UsageLedger::new(Some(ledger_path));
        assert_eq!(reloaded_ledger.total_entries(), 3);
        let reloaded_sum = reloaded_ledger.summary(None);
        assert_eq!(reloaded_sum.total_records, 3);
    }

    #[cfg(feature = "axum-server")]
    #[tokio::test]
    async fn test_embedded_gateway_server_loopback_and_endpoints() {
        let mut state = GatewayState::new("mock_provider");

        // Set up a virtual routing group
        let mut groups = HashMap::new();
        let mut group = RoutingGroup::new("agent-mix", "Agent Mix", RoutingStrategy::Rotate);
        group.members = vec![
            RoutingGroupMember::new("mock_provider/gpt-mock-1"),
            RoutingGroupMember::new("mock_provider/gpt-mock-2"),
        ];
        groups.insert("agent-mix".to_string(), group);
        state = state.with_groups(groups);

        // Provide custom execution handler to mock upstream completion turns cleanly
        state = state.with_custom_handler(Arc::new(|req, sess| {
            Box::pin(async move {
                let user_msg = req
                    .messages
                    .last()
                    .map(|m| m.content.clone())
                    .unwrap_or_default();
                Ok(UnifiedResponse {
                    provider_id: "mock_provider".to_string(),
                    model: req.model,
                    content: format!("ECHO_REPLY: {user_msg} (sess={:?})", sess),
                    reasoning: Some("mock thinking step".to_string()),
                    tool_calls: Vec::new(),
                    finish_reason: "stop".to_string(),
                    usage: UnifiedUsage {
                        input_tokens: 10,
                        output_tokens: 25,
                        cache_read_tokens: 0,
                        total_tokens: 35,
                    },
                })
            })
        }));

        // Bind loopback on dynamic port
        let state_clone = state.clone();
        let server = GatewayServer::bind_loopback(state, 0)
            .await
            .expect("bind loopback server");
        let local_addr = server.local_addr().expect("local addr");
        let handle = server.run_background();

        let client = reqwest::Client::new();
        let base_url = format!("http://{local_addr}");

        // 1. Test GET /v1/models
        let models_res: Value = client
            .get(format!("{base_url}/v1/models"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(models_res["object"], "list");
        let data = models_res["data"].as_array().unwrap();
        assert!(data.iter().any(|m| m["id"] == "group/agent-mix"));
        assert!(data.iter().any(|m| m["id"] == "agent-mix"));

        // 2. Test POST /v1/chat/completions (synchronous JSON)
        let chat_res: Value = client
            .post(format!("{base_url}/v1/chat/completions"))
            .header("X-Magpie-Session", "test_worker_m2_sess")
            .json(&json!({
                "model": "group/agent-mix",
                "messages": [{"role": "user", "content": "Hello Magpie Gateway!"}],
                "stream": false
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(chat_res["object"], "chat.completion");
        let content = chat_res["choices"][0]["message"]["content"]
            .as_str()
            .unwrap();
        assert!(content.contains("ECHO_REPLY: Hello Magpie Gateway!"));
        assert!(content.contains("test_worker_m2_sess"));

        // 3. Test POST /v1/chat/completions (streaming SSE)
        let sse_res = client
            .post(format!("{base_url}/v1/chat/completions"))
            .header("x-conversation-id", "stream_sess_99")
            .json(&json!({
                "model": "group/agent-mix",
                "messages": [{"role": "user", "content": "Stream me please"}],
                "stream": true
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(sse_res.status(), reqwest::StatusCode::OK);
        let sse_text = sse_res.text().await.unwrap();
        assert!(sse_text.contains("data: "));
        assert!(sse_text.contains("[DONE]"));

        // 4. Test GET /v1/magpie/usage
        let usage_res: Value = client
            .get(format!("{base_url}/v1/magpie/usage"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let summary = &usage_res["summary"];
        assert!(summary["total_records"].as_u64().unwrap() >= 2);
        assert!(summary["total_tokens"].as_u64().unwrap() >= 70);

        // 5. Test GET /v1/magpie/quotas
        let quotas_res: Value = client
            .get(format!("{base_url}/v1/magpie/quotas"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(quotas_res["seats"].is_object());

        // 6. Test GET /v1/magpie/quotas reflects active rests from metrics.circuit_breaker
        {
            let mut m = state_clone.metrics.write();
            m.record_failure_disambiguated(
                "mock_provider/gpt-mock-1",
                "mock_provider",
                502,
                "Internal upstream gateway error",
                None,
                None,
                1_000_000,
            );
        }

        let quotas_with_rest: Value = client
            .get(format!("{base_url}/v1/magpie/quotas"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(
            quotas_with_rest["rests"]["mock_provider/gpt-mock-1"].is_object(),
            "quotas endpoint must reflect live circuit breaker rest state"
        );
        assert_eq!(
            quotas_with_rest["rests"]["mock_provider/gpt-mock-1"]["status"],
            502
        );

        // 7. Test forbiddenAlike unwinding reflected in /v1/magpie/quotas
        {
            let mut m = state_clone.metrics.write();
            m.record_failure_disambiguated(
                "mock_provider/gpt-mock-2",
                "mock_provider",
                502,
                "Internal upstream gateway error",
                None,
                None,
                1_001_000,
            );
        }

        let quotas_after_disambiguation: Value = client
            .get(format!("{base_url}/v1/magpie/quotas"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        // Rest has been removed, and error fingerprint added to forbidden_alike
        let rests_map = quotas_after_disambiguation["rests"].as_object().unwrap();
        assert!(rests_map.is_empty(), "rest should be cleared upon forbiddenAlike disambiguation");
        let fa_arr = quotas_after_disambiguation["forbidden_alike"].as_array().unwrap();
        assert!(
            fa_arr.iter().any(|v| v.as_str().unwrap().contains("mock_provider::502")),
            "forbidden_alike should include the disambiguated error"
        );

        // Graceful shutdown
        handle.shutdown().await;
    }

    #[test]
    fn test_usage_ledger_concurrency_and_session_normalization() {
        use std::thread;

        // 1. Session map normalization precedence
        let mut map = HashMap::new();
        map.insert("x-session-id".to_string(), "lower_priority_sess".to_string());
        map.insert("x-magpie-session".to_string(), "high_priority_magpie".to_string());
        assert_eq!(
            normalize_session_from_map(&map),
            Some("high_priority_magpie".to_string()),
            "x-magpie-session must take precedence over x-session-id"
        );

        // 3. Multi-threaded concurrent JSONL append integrity
        let dir = TempDirGuard::new("usage_ledger_concurrency");
        let jsonl_path = dir.path.join("usage.jsonl");
        let ledger = Arc::new(UsageLedger::new(Some(jsonl_path.clone())));

        let mut handles = Vec::new();
        for t in 0..20 {
            let l = ledger.clone();
            handles.push(thread::spawn(move || {
                for i in 0..50 {
                    let rec = UsageRecord::new(
                        format!("rec_{t}_{i}"),
                        format!("sess_{t}"),
                        "openai",
                        "gpt-4o",
                        100,
                        200,
                        10,
                        20,
                        Some(50),
                        Some(500),
                        1000 + i as i64,
                    );
                    l.record(rec).expect("record usage");
                }
            }));
        }

        for h in handles {
            h.join().expect("thread join");
        }

        assert_eq!(ledger.total_entries(), 1000);

        // Read file line by line and verify 100% of physical lines parse as valid JSON
        let file = std::fs::File::open(&jsonl_path).expect("open usage jsonl");
        let reader = std::io::BufReader::new(file);
        let mut parsed_count = 0;
        let mut total_lines = 0;
        for line in std::io::BufRead::lines(reader) {
            let l = line.expect("read line");
            let trimmed = l.trim();
            if trimmed.is_empty() {
                continue;
            }
            total_lines += 1;
            let parse_res = serde_json::from_str::<UsageRecord>(trimmed);
            assert!(
                parse_res.is_ok(),
                "Corrupted JSON line at #{total_lines}: {trimmed}"
            );
            parsed_count += 1;
        }

        assert_eq!(total_lines, 1000, "Expected exactly 1000 physical JSONL lines");
        assert_eq!(parsed_count, 1000, "Expected 1000 successfully parsed records");
    }
}
