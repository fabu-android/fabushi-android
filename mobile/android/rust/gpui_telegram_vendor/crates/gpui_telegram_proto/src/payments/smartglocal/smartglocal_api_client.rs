//! Telegram Desktop 1:1 Parity Submodule: `SmartglocalApiClientSmartglocal`
//! Subsystem: `payments` | Feature ID: `CAP-06`
//! Upstream Sources: `Telegram/SourceFiles/payments/smartglocal/smartglocal_api_client.cpp`, `Telegram/SourceFiles/payments/smartglocal/smartglocal_api_client.h`

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PaymentConfiguration {
    pub public_token: String,
    pub tokenize_url: String,
    pub is_test: bool,
}

pub fn api_url_base(is_test: bool) -> &'static str {
    if is_test {
        "tgb-playground.smart-glocal.com/cds/v1"
    } else {
        "tgb.smart-glocal.com/cds/v1"
    }
}

pub fn token_endpoint() -> &'static str {
    "tokenize/card"
}

pub fn compute_api_url(config: &PaymentConfiguration) -> String {
    let url = &config.tokenize_url;
    let host_suffix = ".smart-glocal.com";
    if url.starts_with("https://")
        && url.contains(host_suffix)
        && url.contains("/cds/v1/tokenize/card")
        && !url.contains('?')
        && !url.contains('#')
    {
        return url.clone();
    }
    format!("https://{}/{}", api_url_base(config.is_test), token_endpoint())
}

pub fn to_card_json(number: &str, exp_month: u32, exp_year: u32, cvc: &str) -> String {
    let month_str = format!("{:02}", exp_month);
    let year_str = format!("{:02}", exp_year % 100);
    format!(
        r#"{{"card":{{"number":"{}","expiration_month":"{}","expiration_year":"{}","security_code":"{}"}}}}"#,
        number, month_str, year_str, cvc
    )
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmartglocalApiClientSmartglocal {
    pub is_enabled: bool,
    pub api_url: String,
    pub configuration: PaymentConfiguration,
    pub additional_headers: HashMap<String, String>,
    pub updated_at: i64,
}

impl Default for SmartglocalApiClientSmartglocal {
    fn default() -> Self {
        Self::new(PaymentConfiguration::default())
    }
}

impl SmartglocalApiClientSmartglocal {
    pub fn new(configuration: PaymentConfiguration) -> Self {
        let api_url = compute_api_url(&configuration);
        let mut headers = HashMap::new();
        headers.insert("X-PUBLIC-TOKEN".to_string(), configuration.public_token.clone());
        headers.insert("Content-Type".to_string(), "application/json".to_string());

        Self {
            is_enabled: true,
            api_url,
            configuration,
            additional_headers: headers,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.is_enabled && !self.configuration.public_token.is_empty()
    }

    pub fn create_token_with_card(
        &mut self,
        number: &str,
        exp_month: u32,
        exp_year: u32,
        cvc: &str,
    ) -> (String, String, HashMap<String, String>) {
        let body = to_card_json(number, exp_month, exp_year, cvc);
        self.updated_at = chrono::Utc::now().timestamp();
        (self.api_url.clone(), body, self.additional_headers.clone())
    }

    pub fn create_token_with_data(&mut self, data: &str) -> (String, String, HashMap<String, String>) {
        self.updated_at = chrono::Utc::now().timestamp();
        (self.api_url.clone(), data.to_string(), self.additional_headers.clone())
    }
}
