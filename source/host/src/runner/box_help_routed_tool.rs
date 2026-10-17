use super::AndroidRoutedToolBridge;
use crate::android_box_handoff::{AndroidBoxHandoffOwner, AndroidHandoffStart};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

pub(crate) const REQUEST_BOX_HELP_TOOL_NAME: &str = "request_box_help";
pub(crate) const WAITING_USER_CANCELLATION_PREFIX: &str = "waiting-user:box-help:";
const WAITING_USER_RESULT_KEY: &str = "__fabushiWaitingUserBoxHelp";

pub(crate) fn waiting_user_box_help_request_id(value: &Value) -> Option<&str> {
    value
        .get(WAITING_USER_RESULT_KEY)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

pub(crate) fn with_box_help_routed_tool(
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    owner: Arc<Mutex<AndroidBoxHandoffOwner>>,
    account_fence: &str,
    agent_id: &str,
) -> Arc<dyn AndroidRoutedToolBridge> {
    Arc::new(BoxHelpRoutedTool {
        delegate,
        owner,
        account_fence: account_fence.to_string(),
        agent_id: agent_id.to_string(),
    })
}

struct BoxHelpRoutedTool {
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    owner: Arc<Mutex<AndroidBoxHandoffOwner>>,
    account_fence: String,
    agent_id: String,
}

impl AndroidRoutedToolBridge for BoxHelpRoutedTool {
    fn list_tools(&self) -> Result<Vec<Value>, String> {
        let mut tools = self.delegate.list_tools()?;
        tools.retain(|tool| tool.get("name").and_then(Value::as_str) != Some(REQUEST_BOX_HELP_TOOL_NAME));
        tools.push(json!({
            "type":"function",
            "name":REQUEST_BOX_HELP_TOOL_NAME,
            "description":"Ask the user to temporarily take over the current Fabushi box/session for an action that requires a human, such as authentication, CAPTCHA, or payment. The current Agent turn ends after the handoff starts and resumes only after the user hands control back.",
            "parameters":{
                "type":"object",
                "additionalProperties":false,
                "required":["instruction"],
                "properties":{
                    "instruction":{"type":"string","minLength":1,"maxLength":16000},
                    "reason":{"type":"string","enum":["auth","captcha","payment","other"]},
                    "domain":{"type":"string","maxLength":512},
                    "idp_domain":{"type":"string","maxLength":512}
                }
            }
        }));
        Ok(tools)
    }

    fn call_tool(&self, name: &str, args: Value, tool_call_id: &str) -> Result<Value, String> {
        if name != REQUEST_BOX_HELP_TOOL_NAME {
            return self.delegate.call_tool(name, args, tool_call_id);
        }
        let request = normalize_request(&args)?;
        let started = self
            .owner
            .lock()
            .map_err(|_| "Android box handoff lock poisoned".to_string())?
            .start_with_context(
                &self.account_fence,
                &self.agent_id,
                &request.instruction,
                request.reason.as_deref(),
                request.domain.as_deref(),
                request.idp_domain.as_deref(),
                now_ms(),
            )?;
        let (status, pending) = match started {
            AndroidHandoffStart::Started(pending) => ("started", pending),
            AndroidHandoffStart::AlreadyPending(pending) => ("already-pending", pending),
        };
        Ok(json!({
            "__fabushiWaitingUserBoxHelp":pending.request_id,
            "status":status,
            "instruction":pending.instruction,
            "reason":pending.reason,
            "domain":pending.domain,
            "idp_domain":pending.idp_domain
        }))
    }
}

struct NormalizedBoxHelpRequest {
    instruction: String,
    reason: Option<String>,
    domain: Option<String>,
    idp_domain: Option<String>,
}

fn normalize_request(args: &Value) -> Result<NormalizedBoxHelpRequest, String> {
    let object = args.as_object().ok_or("request_box_help arguments must be an object")?;
    let instruction = object
        .get("instruction")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or("request_box_help instruction is required")?;
    if instruction.chars().count() > 16_000 {
        return Err("request_box_help instruction exceeds the supported limit".into());
    }
    let reason = object
        .get("reason")
        .and_then(Value::as_str)
        .filter(|value| matches!(*value, "auth" | "captcha" | "payment" | "other"))
        .map(ToOwned::to_owned);
    Ok(NormalizedBoxHelpRequest {
        instruction: instruction.to_string(),
        reason,
        domain: object
            .get("domain")
            .and_then(Value::as_str)
            .and_then(normalize_box_help_domain),
        idp_domain: object
            .get("idp_domain")
            .and_then(Value::as_str)
            .and_then(normalize_box_help_domain),
    })
}

fn normalize_box_help_domain(raw: &str) -> Option<String> {
    let value = raw.trim().to_ascii_lowercase();
    if value.is_empty() {
        return None;
    }
    let candidate = if value.contains("://") {
        value
    } else {
        format!("https://{value}")
    };
    let url = Url::parse(&candidate).ok()?;
    let host = url.host_str()?;
    let host = host.strip_prefix("www.").unwrap_or(host);
    (!host.is_empty()).then(|| host.to_string())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct EmptyBridge;
    impl AndroidRoutedToolBridge for EmptyBridge {
        fn list_tools(&self) -> Result<Vec<Value>, String> {
            Ok(vec![json!({"type":"function","name":"delegate","parameters":{"type":"object"}})])
        }
        fn call_tool(&self, name: &str, _args: Value, _tool_call_id: &str) -> Result<Value, String> {
            Ok(json!({"delegated":name}))
        }
    }

    fn temp_owner() -> (std::path::PathBuf, Arc<Mutex<AndroidBoxHandoffOwner>>) {
        let root = std::env::temp_dir().join(format!(
            "fabushi-box-help-routed-{}-{}",
            std::process::id(),
            now_ms()
        ));
        fs::create_dir_all(&root).unwrap();
        let owner = AndroidBoxHandoffOwner::open(root.join("handoff.json")).unwrap();
        (root, Arc::new(Mutex::new(owner)))
    }

    #[test]
    fn exposes_desktop_box_help_contract_and_delegates_unknown_tools() {
        let (root, owner) = temp_owner();
        let bridge = with_box_help_routed_tool(Arc::new(EmptyBridge), owner, "session:a", "agent-a");
        let tools = bridge.list_tools().unwrap();
        assert!(tools.iter().any(|tool| tool["name"] == REQUEST_BOX_HELP_TOOL_NAME));
        assert_eq!(bridge.call_tool("delegate", json!({}), "call-1").unwrap()["delegated"], "delegate");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn starts_one_durable_account_fenced_handoff_and_normalizes_domains() {
        let (root, owner) = temp_owner();
        let bridge = with_box_help_routed_tool(
            Arc::new(EmptyBridge),
            Arc::clone(&owner),
            "session:a",
            "agent-a",
        );
        let result = bridge.call_tool(
            REQUEST_BOX_HELP_TOOL_NAME,
            json!({
                "instruction":"Please finish sign-in",
                "reason":"auth",
                "domain":"https://www.Example.COM/login?next=1",
                "idp_domain":"https://accounts.example.com/"
            }),
            "call-1",
        ).unwrap();
        let request_id = waiting_user_box_help_request_id(&result).unwrap().to_string();
        let pending = owner.lock().unwrap().pending_for("session:a", "agent-a").unwrap().unwrap();
        assert_eq!(pending.request_id, request_id);
        assert_eq!(pending.reason.as_deref(), Some("auth"));
        assert_eq!(pending.domain.as_deref(), Some("example.com"));
        assert_eq!(pending.idp_domain.as_deref(), Some("accounts.example.com"));
        let replay = bridge.call_tool(
            REQUEST_BOX_HELP_TOOL_NAME,
            json!({"instruction":"Please finish sign-in","reason":"auth"}),
            "call-2",
        ).unwrap();
        assert_eq!(waiting_user_box_help_request_id(&replay), Some(request_id.as_str()));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_optional_fields_follow_desktop_fail_soft_semantics() {
        let (root, owner) = temp_owner();
        let bridge = with_box_help_routed_tool(
            Arc::new(EmptyBridge),
            Arc::clone(&owner),
            "session:a",
            "agent-a",
        );
        let result = bridge.call_tool(
            REQUEST_BOX_HELP_TOOL_NAME,
            json!({
                "instruction":"help",
                "reason":"sudo",
                "domain":"not a host / still invalid",
                "idp_domain":"",
                "extra":true
            }),
            "call-1",
        ).unwrap();
        assert!(waiting_user_box_help_request_id(&result).is_some());
        let pending = owner.lock().unwrap().pending_for("session:a", "agent-a").unwrap().unwrap();
        assert_eq!(pending.reason, None);
        assert_eq!(pending.domain, None);
        assert_eq!(pending.idp_domain, None);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn normalizes_domains_with_url_host_semantics() {
        assert_eq!(
            normalize_box_help_domain("https://www.Example.COM/login?next=1").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            normalize_box_help_domain("accounts.Example.com/path").as_deref(),
            Some("accounts.example.com")
        );
        assert_eq!(normalize_box_help_domain("not a host / still invalid"), None);
    }
}
