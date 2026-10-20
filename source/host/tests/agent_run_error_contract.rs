use fabushi_mahayana_host::extensions::transcript::agent_run_error::{
    AgentRunErrorClass, AgentRunErrorDescription, BackendAdditionalInfo, BackendDetail,
    backend_detail_from_error_message, checkout_deep_control_url, classify_agent_run_error,
    describe_agent_run_error, find_backend_connect_error, format_agent_run_error,
    format_sand_usage_reset_in, get_backend_error_detail_message, message_matches_agent_transport,
    map_error_detail_buttons, parse_backend_connect_error, provider_failure_tray,
    provider_failure_tray_for_request, with_relative_sand_included_limit_reset,
};
use serde_json::json;

const NOW: i64 = 1_700_000_000_000;

#[test]
fn serialized_backend_connect_error_walk_prefers_nested_detailed_error() {
    let reset = chrono::DateTime::from_timestamp_millis(NOW + 3_600_000)
        .expect("reset")
        .to_rfc3339();
    let message = format!(
        "OpenRouter request failed (429): {{\"error\":{{\"message\":\"outer\",\"code\":429,\"cause\":{{\"message\":\"cause without details\"}},\"errors\":[{{\"message\":\"detailed backend\",\"details\":[{{\"details\":{{\"title\":\"Limit reached\",\"detail\":\"Included usage exhausted. It resets at tomorrow.\",\"buttons\":[{{\"label\":\"\",\"action\":{{\"case\":\"switchModel\",\"value\":{{}}}}}}],\"additionalInfo\":{{\"rateLimitReason\":\"sand_included_limit\",\"nextResetAt\":\"{}\"}}}}}}]}}]}}}}.",
        reset
    );
    let root = parse_backend_connect_error(&message).expect("serialized connect error");
    assert_eq!(root.code, Some(429));
    assert_eq!(
        find_backend_connect_error(&root, true)
            .expect("detailed nested error")
            .message,
        "detailed backend"
    );
    assert_eq!(
        get_backend_error_detail_message(&message).as_deref(),
        Some("Limit reached\n\nIncluded usage exhausted. It resets at tomorrow.")
    );
    assert_eq!(
        format_agent_run_error(&message),
        "Limit reached\n\nIncluded usage exhausted. It resets at tomorrow."
    );
    let detail = backend_detail_from_error_message(&message).expect("backend detail");
    assert_eq!(detail.title.as_deref(), Some("Limit reached"));

    let tray = provider_failure_tray("agent-a", &message, NOW);
    assert_eq!(tray.title, "Limit reached");
    assert_eq!(tray.detail, "Included usage exhausted. It resets in 1 hour.");
    assert_eq!(tray.actions, vec![json!({"kind":"switch-model"})]);
    assert_eq!(tray.raw_detail.as_deref(), Some(message.as_str()));
}

#[test]
fn serialized_backend_connect_error_can_fall_back_to_first_detail_less_error() {
    let message = "provider failed: {\"error\":{\"message\":\"outer\",\"code\":503,\"cause\":{\"message\":\"inner\"}}}.";
    let root = parse_backend_connect_error(message).expect("connect error");
    assert!(find_backend_connect_error(&root, true).is_none());
    assert_eq!(
        find_backend_connect_error(&root, false)
            .expect("first connect error")
            .message,
        "outer"
    );
    assert_eq!(format_agent_run_error(message), message);
}

#[test]
fn usage_reset_formatting_matches_frozen_thresholds() {
    let now = chrono::DateTime::from_timestamp_millis(NOW).expect("now");
    let iso = |delta: i64| (now + chrono::Duration::milliseconds(delta)).to_rfc3339();
    assert_eq!(format_sand_usage_reset_in(&iso(-1), NOW).as_deref(), Some("less than a minute"));
    assert_eq!(format_sand_usage_reset_in(&iso(61_000), NOW).as_deref(), Some("2 minutes"));
    assert_eq!(format_sand_usage_reset_in(&iso(3_600_001), NOW).as_deref(), Some("2 hours"));
    assert_eq!(format_sand_usage_reset_in(&iso(86_400_001), NOW).as_deref(), Some("2 days"));
    assert!(format_sand_usage_reset_in("not-a-date", NOW).is_none());
}

#[test]
fn included_limit_reset_rewrites_absolute_and_relative_clauses() {
    let reset = chrono::DateTime::from_timestamp_millis(NOW + 7_200_000)
        .expect("reset")
        .to_rfc3339();
    assert_eq!(
        with_relative_sand_included_limit_reset(
            "Quota reached. It resets at 2026-09-24T12:00:00Z. Try later.",
            Some(&reset),
            NOW,
        ),
        "Quota reached. It resets in 2 hours. Try later."
    );
    assert_eq!(
        with_relative_sand_included_limit_reset(
            "Quota reached. It resets in tomorrow. Try later.",
            Some(&reset),
            NOW,
        ),
        "Quota reached. It resets in 2 hours. Try later."
    );
}

#[test]
fn action_mapping_enforces_protocols_dedupes_switch_and_caps_at_three() {
    let buttons = vec![
        json!({"label":"Docs","action":{"case":"url","value":{"url":"https://example.com/help"}}}),
        json!({"label":"Bad","action":{"case":"url","value":{"url":"file:///tmp/x"}}}),
        json!({"label":"","action":{"case":"switchModel","value":{}}}),
        json!({"label":"ignored duplicate","action":{"case":"switchModel","value":{}}}),
        json!({"label":"Raise","action":{"case":"dashboardAction","value":{"action":"requestLimitIncrease","args":{"team":"a"},"successMessage":"sent"}}}),
        json!({"label":"Pricing","action":{"case":"upgradeChoice","value":{}}}),
    ];
    assert_eq!(
        map_error_detail_buttons(&buttons),
        vec![
            json!({"kind":"open-url","label":"Docs","url":"https://example.com/help"}),
            json!({"kind":"switch-model"}),
            json!({"kind":"dashboard-action","label":"Raise","action":"requestLimitIncrease","args":{"team":"a"},"successMessage":"sent"}),
        ]
    );
}

#[test]
fn checkout_url_normalizes_tier_and_trial_flag() {
    assert_eq!(
        checkout_deep_control_url(&json!({"membershipToUpgradeTo":"ultra","allowTrial":true})),
        "https://cursor.com/api/auth/checkoutDeepControl?tier=ultra&allowTrial=true"
    );
    assert_eq!(
        checkout_deep_control_url(&json!({"membershipToUpgradeTo":"enterprise","allowTrial":false})),
        "https://cursor.com/api/auth/checkoutDeepControl?tier=pro&allowTrial=false"
    );
}

#[test]
fn structured_description_uses_backend_title_limit_reset_and_actions() {
    let reset = chrono::DateTime::from_timestamp_millis(NOW + 3_600_000)
        .expect("reset")
        .to_rfc3339();
    let detail = BackendDetail {
        title: Some("Limit reached".into()),
        detail: Some("Included usage exhausted. It resets at tomorrow.".into()),
        buttons: vec![json!({"label":"","action":{"case":"switchModel","value":{}}})],
        additional_info: Some(BackendAdditionalInfo {
            rate_limit_reason: Some("sand_included_limit".into()),
            next_reset_at: Some(reset),
        }),
    };
    assert_eq!(
        describe_agent_run_error("fallback", Some(&detail), NOW),
        AgentRunErrorDescription {
            title: Some("Limit reached".into()),
            detail: "Included usage exhausted. It resets in 1 hour.".into(),
            actions: vec![json!({"kind":"switch-model"})],
        }
    );
}

#[test]
fn provider_failure_projection_is_ready_for_shipping_trays() {
    let tray = provider_failure_tray("agent-a", "provider disconnected", NOW);
    assert_eq!(tray.agent_id.as_deref(), Some("agent-a"));
    assert_eq!(tray.title, "Agent run failed");
    assert_eq!(tray.detail, "provider disconnected");
    assert_eq!(tray.raw_detail.as_deref(), Some("provider disconnected"));
    assert_eq!(tray.error_kind.as_deref(), Some("agent_run_error"));
    assert_eq!(tray.dedupe_key.as_deref(), Some("agent-run-error:agent-a"));
}


#[test]
fn desktop_agent_client_error_taxonomy_maps_to_single_host_classification_owner() {
    let classified = |error: &str, retryable: Option<bool>| {
        let retryable_json = retryable.map(|value| value.to_string()).unwrap_or_else(|| "null".into());
        let message = format!(
            r#"provider failed: {{"error":{{"message":"classified","details":[{{"details":{{"error":"{error}","details":{{"isRetryable":{retryable_json}}}}}}}]}}}}"#
        );
        classify_agent_run_error(&message, Some("req-taxonomy"), false)
    };
    for code in ["NOT_LOGGED_IN","AGENT_REQUIRES_LOGIN","AUTH_TOKEN_NOT_FOUND","AUTH_TOKEN_EXPIRED",
        "INVALID_AUTH_ID","UNAUTHORIZED","GITHUB_NO_USER_CREDENTIALS","GITHUB_USER_NO_ACCESS"] {
        let value = classified(code, None);
        assert_eq!(value.class, AgentRunErrorClass::ActionRequired, "{code}");
        assert_eq!(value.action.as_deref(), Some("login"), "{code}");
    }
    for code in ["FREE_USER_USAGE_LIMIT","FREE_USER_RATE_LIMIT_EXCEEDED","PRO_USER_ONLY","PRO_USER_USAGE_LIMIT",
        "PRO_USER_RATE_LIMIT_EXCEEDED","RATE_LIMITED","RATE_LIMITED_CHANGEABLE","GENERIC_RATE_LIMIT_EXCEEDED"] {
        assert_eq!(classified(code, None).action.as_deref(), Some("upgrade"), "{code}");
    }
    for code in ["USAGE_PRICING_REQUIRED","USAGE_PRICING_REQUIRED_CHANGEABLE"] {
        assert_eq!(classified(code, None).action.as_deref(), Some("payment"), "{code}");
    }
    for code in ["BAD_API_KEY","BAD_USER_API_KEY","OUTDATED_CLIENT"] {
        assert_eq!(classified(code, None).action.as_deref(), Some("config"), "{code}");
    }
    for code in ["USER_ABORTED_REQUEST","DEBOUNCED"] {
        assert_eq!(classified(code, None).class, AgentRunErrorClass::Cancelled, "{code}");
    }
    assert_eq!(classified("CUSTOM_MESSAGE", Some(false)).class, AgentRunErrorClass::NonRetriable);
    assert_eq!(classified("CUSTOM_MESSAGE", Some(true)).class, AgentRunErrorClass::Retriable);
    assert_eq!(classified("UNRECOGNIZED", Some(false)).class, AgentRunErrorClass::NonRetriable);
}

#[test]
fn action_required_request_display_details_and_inference_type_are_preserved() {
    let message = r#"provider failed: {"error":{"message":"auth required","code":16,"inferenceRequestErrorType":"credential","details":[{"details":{"error":"AUTH_TOKEN_EXPIRED","details":{"isRetryable":false,"title":"Sign in again","detail":"Your session expired.","analyticsMetadata":{"actionRequired":"login"}}}}]}}"#;
    let value = classify_agent_run_error(message, Some("req-42"), false);
    assert_eq!(value.class, AgentRunErrorClass::ActionRequired);
    assert_eq!(value.request_id.as_deref(), Some("req-42"));
    assert_eq!(value.action.as_deref(), Some("login"));
    assert_eq!(value.display_info.connect_code, Some(16));
    assert_eq!(value.display_info.error_code.as_deref(), Some("AUTH_TOKEN_EXPIRED"));
    assert_eq!(value.display_info.title.as_deref(), Some("Sign in again"));
    assert_eq!(value.display_info.detail.as_deref(), Some("Your session expired."));
    assert_eq!(value.display_info.is_retryable, Some(false));
    assert_eq!(value.display_info.inference_request_error_type.as_deref(), Some("credential"));
    assert!(value.display_info.error_details.is_some());
    let tray = provider_failure_tray_for_request("agent-a", Some("req-42"), message, NOW);
    assert_eq!(tray.request_id.as_deref(), Some("req-42"));
    assert_eq!(tray.classification.as_deref(), Some("action-required"));
    assert_eq!(tray.recovery_action.as_deref(), Some("login"));
    assert_eq!(tray.actions[0]["kind"], "recovery-route");
    assert_eq!(tray.actions[0]["route"], "login");
    assert_eq!(tray.display_info.as_ref().and_then(|v| v.get("errorCode")).and_then(Value::as_str), Some("AUTH_TOKEN_EXPIRED"));
}

#[test]
fn transport_cancel_retry_and_blob_not_found_match_desktop_contract() {
    assert!(message_matches_agent_transport("ERR_HTTP2_SESSION_ERROR"));
    assert!(message_matches_agent_transport("ENOTFOUND api.example.test"));
    assert!(message_matches_agent_transport("socket hang up"));
    assert!(!message_matches_agent_transport("BAD_API_KEY"));
    let cancelled = classify_agent_run_error(r#"failure {"error":{"message":"cancelled","code":1}}"#, None, false);
    assert_eq!(cancelled.class, AgentRunErrorClass::Cancelled);
    let transport_cancel = classify_agent_run_error(r#"ECONNRESET {"error":{"message":"cancelled","code":1}}"#, None, false);
    assert_eq!(transport_cancel.class, AgentRunErrorClass::Retriable);
    assert!(transport_cancel.is_transport);
    let missing = classify_agent_run_error("blob not found: sha256:missing", Some("req-blob"), false);
    assert_eq!(missing.class, AgentRunErrorClass::NonRetriable);
    assert_eq!(missing.display_info.error_code.as_deref(), Some("BLOB_NOT_FOUND"));
    assert_eq!(missing.display_info.title.as_deref(), Some("Conversation data missing"));
    assert_eq!(missing.display_info.is_retryable, Some(false));
}
