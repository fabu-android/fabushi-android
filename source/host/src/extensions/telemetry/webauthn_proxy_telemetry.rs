use crate::extensions::webauthn_proxy::webauthn_proxy_bridge::WebAuthnBridgeReport;
use std::collections::BTreeMap;

pub const WEBAUTHN_PROXY_EVENT: &str = "sand.webauthn_proxy";

pub const KNOWN_DOM_ERROR_NAMES: &[&str] = &[
    "NotAllowedError",
    "InvalidStateError",
    "NotSupportedError",
    "SecurityError",
    "AbortError",
    "ConstraintError",
    "DataError",
    "TimeoutError",
    "NetworkError",
    "OperationError",
    "UnknownError",
];

pub fn branded_dom_error(value: Option<&str>) -> &'static str {
    value
        .and_then(|candidate| KNOWN_DOM_ERROR_NAMES.iter().copied().find(|known| *known == candidate))
        .unwrap_or("OtherError")
}

pub fn failure_code(report: &WebAuthnBridgeReport) -> Option<&'static str> {
    match report.cause.as_deref() {
        Some("no_provider") => Some("webauthn_no_provider"),
        Some("provider_stale") => Some("webauthn_provider_stale"),
        Some("dispatch_failed") => Some("webauthn_dispatch_failed"),
        Some("timeout") => Some("webauthn_ceremony_timed_out"),
        Some("consent_declined") => Some("webauthn_consent_declined"),
        Some("sign_failed") => Some("webauthn_sign_failed"),
        Some("desktop_failed") => Some("webauthn_desktop_failed"),
        _ => None,
    }
}

pub fn webauthn_proxy_telemetry(report: &WebAuthnBridgeReport) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::from([
        ("event".into(), WEBAUTHN_PROXY_EVENT.into()),
        ("stage".into(), report.stage.clone()),
        ("outcome".into(), report.outcome.clone()),
        ("ceremony_kind".into(), report.ceremony_kind.clone()),
        ("request_id".into(), report.request_id.clone()),
        ("elapsed_ms".into(), report.elapsed_ms.to_string()),
        (
            "level".into(),
            if report.outcome == "failed" || report.outcome == "timeout" {
                "warn"
            } else {
                "info"
            }
            .into(),
        ),
    ]);
    if let Some(value) = report.provider_count {
        fields.insert("provider_count".into(), value.to_string());
    }
    if let Some(value) = report.live_provider_count {
        fields.insert("live_provider_count".into(), value.to_string());
    }
    if let Some(cause) = &report.cause {
        fields.insert("cause".into(), cause.clone());
    }
    if let Some(raw) = report.raw_dom_error_name.as_deref() {
        fields.insert("dom_error".into(), branded_dom_error(Some(raw)).into());
    }
    if let Some(code) = failure_code(report) {
        fields.insert("error_code".into(), code.into());
    }
    fields
}

#[cfg(test)]
mod tests {
    use super::*;
    use fabushi_android_shared::webauthn_gateway::WebAuthnOriginClass;

    #[test]
    fn telemetry_bounds_dom_errors_and_maps_failure_cause() {
        let report = WebAuthnBridgeReport {
            request_id: "r1".into(),
            origin_class: WebAuthnOriginClass::External,
            ceremony_kind: "get".into(),
            stage: "complete".into(),
            outcome: "failed".into(),
            cause: Some("sign_failed".into()),
            provider_count: Some(1),
            live_provider_count: Some(1),
            raw_dom_error_name: Some("VendorPrivateError".into()),
            raw_sign_error_class: Some("vendor".into()),
            elapsed_ms: 12,
        };
        let fields = webauthn_proxy_telemetry(&report);
        assert_eq!(fields.get("dom_error").map(String::as_str), Some("OtherError"));
        assert_eq!(fields.get("error_code").map(String::as_str), Some("webauthn_sign_failed"));
        assert_eq!(fields.get("level").map(String::as_str), Some("warn"));
    }
}
