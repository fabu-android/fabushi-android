use mahayana_miniapp_protocol::CreatePaymentRequest;
use serde_json::Value;

/// Enforces the Mini App protocol at the canonical Host boundary before a platform request
/// crosses the authenticated network adapter. Presentation/Kotlin may provide early UX
/// validation, but it is not authoritative for payment request shape.
pub fn validate_miniapp_platform_request(
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> Result<(), String> {
    if !is_plugin_purchase_path(path) {
        return Ok(());
    }
    if method != "POST" {
        return Err("Mini App purchase requires POST".into());
    }
    let body = body.ok_or("Mini App purchase requires a request body")?;
    let request: CreatePaymentRequest = serde_json::from_value(body.clone())
        .map_err(|error| format!("invalid Mini App purchase contract: {error}"))?;
    request
        .validate()
        .map_err(|error| format!("invalid Mini App purchase contract: {error}"))
}

fn is_plugin_purchase_path(path: &str) -> bool {
    let parts = path
        .split('?')
        .next()
        .unwrap_or(path)
        .trim_matches('/')
        .split('/')
        .collect::<Vec<_>>();
    parts.len() == 5
        && parts[0] == "v1"
        && parts[1] == "plugins"
        && !parts[2].is_empty()
        && parts[3] == "commerce"
        && parts[4] == "purchase"
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn purchase_accepts_only_server_priced_contract() {
        validate_miniapp_platform_request(
            "POST",
            "/v1/plugins/global-dharma/commerce/purchase",
            Some(&json!({"sku":"lifetime","idempotencyKey":"purchase-123"})),
        )
        .unwrap();
    }

    #[test]
    fn purchase_rejects_client_controlled_price_or_currency() {
        let error = validate_miniapp_platform_request(
            "POST",
            "/v1/plugins/global-dharma/commerce/purchase",
            Some(&json!({
                "sku":"lifetime",
                "idempotencyKey":"purchase-123",
                "amount":1,
                "currency":"USD"
            })),
        )
        .unwrap_err();
        assert!(error.contains("invalid Mini App purchase contract"));
    }

    #[test]
    fn unrelated_platform_requests_are_not_reinterpreted_as_miniapp_payment() {
        validate_miniapp_platform_request(
            "POST",
            "/v1/purchases/restore",
            Some(&json!({})),
        )
        .unwrap();
    }
}
