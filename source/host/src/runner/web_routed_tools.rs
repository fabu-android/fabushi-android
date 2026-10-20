use super::AndroidRoutedToolBridge;
use crate::mcp_auth::CursorDashboardMcpAuthBackend;
use serde_json::{json, Value};
use std::{
    net::IpAddr,
    sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex},
};
use url::Url;

pub const WEB_SEARCH_TOOL_NAME: &str = "WebSearch";
pub const WEB_FETCH_TOOL_NAME: &str = "WebFetch";
const WEB_SEARCH_MAX_INLINE_CHARS: usize = 30_000;
const WEB_FETCH_MAX_CONTENT_CHARS: usize = 100_000;

pub(crate) trait AndroidWebToolBackend: Send + Sync {
    fn search(&self, search_term: &str, explanation: Option<&str>, model_id: &str) -> Result<Value, String>;
    fn fetch(&self, url: &str) -> Result<Value, String>;
}

impl AndroidWebToolBackend for CursorDashboardMcpAuthBackend {
    fn search(&self, search_term: &str, explanation: Option<&str>, model_id: &str) -> Result<Value, String> {
        self.run_web_search(search_term, explanation, model_id)
    }
    fn fetch(&self, url: &str) -> Result<Value, String> {
        self.run_web_fetch(url)
    }
}

struct WebRoutedTools {
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    backend: Arc<dyn AndroidWebToolBackend>,
    model_id: String,
    account_fence: String,
    live_account_fence: Arc<Mutex<Option<String>>>,
    cancelled: Arc<AtomicBool>,
}

impl WebRoutedTools {
    fn assert_live(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err("web tool call cancelled".into());
        }
        let live = self.live_account_fence.lock().map_err(|_| "web tool account fence lock poisoned".to_string())?;
        if live.as_deref() != Some(self.account_fence.as_str()) {
            return Err("web tool result belongs to a stale account epoch".into());
        }
        Ok(())
    }
}

impl AndroidRoutedToolBridge for WebRoutedTools {
    fn list_tools(&self) -> Result<Vec<Value>, String> {
        let mut tools = self.delegate.list_tools()?;
        tools.retain(|tool| !matches!(tool.get("name").and_then(Value::as_str), Some(WEB_SEARCH_TOOL_NAME | WEB_FETCH_TOOL_NAME)));
        tools.push(web_search_definition());
        tools.push(web_fetch_definition());
        Ok(tools)
    }

    fn call_tool(&self, name: &str, args: Value, tool_call_id: &str) -> Result<Value, String> {
        match name {
            WEB_SEARCH_TOOL_NAME => {
                self.assert_live()?;
                require_tool_call_id(tool_call_id, WEB_SEARCH_TOOL_NAME)?;
                let object = require_object(&args, WEB_SEARCH_TOOL_NAME)?;
                reject_unknown(object, &["search_term", "query", "searchTerm", "explanation"], WEB_SEARCH_TOOL_NAME)?;
                let search_term = ["search_term", "query", "searchTerm"].iter()
                    .find_map(|key| object.get(*key).and_then(Value::as_str).map(str::trim).filter(|value| !value.is_empty()))
                    .ok_or("WebSearch requires search_term")?;
                let explanation = object.get("explanation").and_then(Value::as_str).map(str::trim).filter(|value| !value.is_empty());
                let response = self.backend.search(search_term, explanation, &self.model_id)?;
                self.assert_live()?;
                Ok(render_web_search_result(&response))
            }
            WEB_FETCH_TOOL_NAME => {
                self.assert_live()?;
                require_tool_call_id(tool_call_id, WEB_FETCH_TOOL_NAME)?;
                let object = require_object(&args, WEB_FETCH_TOOL_NAME)?;
                reject_unknown(object, &["url"], WEB_FETCH_TOOL_NAME)?;
                let raw = object.get("url").and_then(Value::as_str).map(str::trim).filter(|value| !value.is_empty()).ok_or("WebFetch requires url")?;
                let url = validate_web_fetch_url(raw)?;
                let response = self.backend.fetch(url.as_str())?;
                self.assert_live()?;
                Ok(render_web_fetch_result(url.as_str(), &response))
            }
            _ => self.delegate.call_tool(name, args, tool_call_id),
        }
    }
}

pub(crate) fn with_web_routed_tools(
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    backend: Arc<CursorDashboardMcpAuthBackend>,
    model_id: &str,
    account_fence: &str,
    live_account_fence: Arc<Mutex<Option<String>>>,
    cancelled: Arc<AtomicBool>,
) -> Arc<dyn AndroidRoutedToolBridge> {
    Arc::new(WebRoutedTools {
        delegate,
        backend,
        model_id: model_id.to_string(),
        account_fence: account_fence.to_string(),
        live_account_fence,
        cancelled,
    })
}

fn web_search_definition() -> Value {
    json!({
        "type":"function",
        "name":WEB_SEARCH_TOOL_NAME,
        "description":"Search the web for up-to-date information and return snippets and URLs.",
        "parameters":{
            "type":"object","additionalProperties":false,"required":["search_term"],
            "properties":{
                "search_term":{"type":"string","description":"The search term to look up on the web."},
                "explanation":{"type":"string","description":"One sentence explaining why this search contributes to the goal."}
            }
        }
    })
}

fn web_fetch_definition() -> Value {
    json!({
        "type":"function",
        "name":WEB_FETCH_TOOL_NAME,
        "description":"Fetch a public http(s) URL and return readable content. Localhost and private-network hosts are rejected.",
        "parameters":{
            "type":"object","additionalProperties":false,"required":["url"],
            "properties":{"url":{"type":"string","description":"A fully-formed http:// or https:// URL."}}
        }
    })
}

fn render_web_search_result(response: &Value) -> Value {
    let answer = response.get("answer").and_then(Value::as_str).filter(|value| !value.is_empty());
    let mut references = Vec::new();
    if let Some(answer) = answer {
        references.push(json!({"title":"Web search results","url":"","chunk":answer}));
    } else if let Some(documents) = response.get("documents").and_then(Value::as_array) {
        for document in documents {
            let text = document.get("text").and_then(Value::as_str).unwrap_or_default();
            let chunk = text.chars().take(WEB_SEARCH_MAX_INLINE_CHARS).collect::<String>();
            references.push(json!({
                "title":document.get("title").and_then(Value::as_str).unwrap_or_default(),
                "url":document.get("url").and_then(Value::as_str).unwrap_or_default(),
                "chunk":chunk,
            }));
        }
    }
    json!({"success":{"references":references}})
}

fn render_web_fetch_result(url: &str, response: &Value) -> Value {
    if let Some(error) = response.get("error").and_then(Value::as_str).filter(|value| !value.is_empty()) {
        return json!({"error":{"url":url,"error":error,"isTimeout":response.get("isTimeout").and_then(Value::as_bool).unwrap_or(false)}});
    }
    let content = response.get("content").and_then(Value::as_str).unwrap_or_default();
    let markdown = content.chars().take(WEB_FETCH_MAX_CONTENT_CHARS).collect::<String>();
    json!({"success":{"url":url,"markdown":markdown}})
}

fn validate_web_fetch_url(raw: &str) -> Result<Url, String> {
    let mut url = Url::parse(raw).map_err(|_| "Invalid URL: must include http:// or https://".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(format!("Invalid URL protocol: {} (must be http or https)", url.scheme()));
    }
    url.set_username("").map_err(|_| "could not strip URL username".to_string())?;
    url.set_password(None).map_err(|_| "could not strip URL password".to_string())?;
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if host.is_empty() {
        return Err("WebFetch URL requires a host".into());
    }
    if host == "localhost" || host.ends_with(".localhost") {
        return Err(format!("Cannot fetch from localhost ({host}) because this tool runs from an isolated server."));
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_non_public_ip(ip) {
            return Err(format!("Cannot fetch from private or loopback IP ({host}) because this tool runs from an isolated server."));
        }
    }
    Ok(url)
}

fn is_non_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_loopback() || ip.is_private() || ip.is_link_local() || ip.is_unspecified() || ip.is_broadcast(),
        IpAddr::V6(ip) => ip.is_loopback() || ip.is_unspecified() || ip.is_unique_local(),
    }
}

fn require_object<'a>(args: &'a Value, tool: &str) -> Result<&'a serde_json::Map<String, Value>, String> {
    args.as_object().ok_or_else(|| format!("{tool} arguments must be an object"))
}
fn reject_unknown(object: &serde_json::Map<String, Value>, allowed: &[&str], tool: &str) -> Result<(), String> {
    if let Some(key) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(format!("{tool} received unsupported argument {key}"));
    }
    Ok(())
}
fn require_tool_call_id<'a>(value: &'a str, tool: &str) -> Result<&'a str, String> {
    let value = value.trim();
    if value.is_empty() { Err(format!("{tool} tool_call_id is required")) } else { Ok(value) }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Empty;
    impl AndroidRoutedToolBridge for Empty {
        fn list_tools(&self) -> Result<Vec<Value>, String> { Ok(Vec::new()) }
        fn call_tool(&self, name: &str, _args: Value, _tool_call_id: &str) -> Result<Value, String> { Err(format!("unexpected delegate {name}")) }
    }
    struct Fake;
    impl AndroidWebToolBackend for Fake {
        fn search(&self, search_term: &str, explanation: Option<&str>, model_id: &str) -> Result<Value, String> {
            Ok(json!({"answer":Value::Null,"documents":[{"url":"https://example.com","title":model_id,"text":format!("{search_term}:{}",explanation.unwrap_or(""))}]}))
        }
        fn fetch(&self, url: &str) -> Result<Value, String> { Ok(json!({"content":format!("fetched {url}"),"error":Value::Null,"isTimeout":Value::Null})) }
    }
    fn bridge() -> Arc<dyn AndroidRoutedToolBridge> {
        Arc::new(WebRoutedTools {
            delegate: Arc::new(Empty), backend: Arc::new(Fake), model_id:"grok-4.5".into(),
            account_fence:"acct:a".into(), live_account_fence:Arc::new(Mutex::new(Some("acct:a".into()))), cancelled:Arc::new(AtomicBool::new(false)),
        })
    }
    #[test]
    fn exposes_desktop_web_tool_contract_and_aliases() {
        let tools=bridge(); let defs=tools.list_tools().unwrap();
        assert!(defs.iter().any(|tool| tool["name"]==WEB_SEARCH_TOOL_NAME));
        assert!(defs.iter().any(|tool| tool["name"]==WEB_FETCH_TOOL_NAME));
        let result=tools.call_tool(WEB_SEARCH_TOOL_NAME,json!({"query":"rust 2026","explanation":"fresh"}),"call-1").unwrap();
        assert_eq!(result["success"]["references"][0]["url"],"https://example.com");
    }
    #[test]
    fn fetch_strips_credentials_and_rejects_private_hosts() {
        let tools=bridge();
        assert!(tools.call_tool(WEB_FETCH_TOOL_NAME,json!({"url":"http://127.0.0.1/private"}),"call-2").is_err());
        let result=tools.call_tool(WEB_FETCH_TOOL_NAME,json!({"url":"https://user:pass@example.com/page"}),"call-3").unwrap();
        assert_eq!(result["success"]["url"],"https://example.com/page");
    }
    #[test]
    fn stale_account_and_cancelled_turns_fail_closed() {
        let live=Arc::new(Mutex::new(Some("acct:b".into())));
        let cancelled=Arc::new(AtomicBool::new(false));
        let tools:Arc<dyn AndroidRoutedToolBridge>=Arc::new(WebRoutedTools{delegate:Arc::new(Empty),backend:Arc::new(Fake),model_id:"grok-4.5".into(),account_fence:"acct:a".into(),live_account_fence:Arc::clone(&live),cancelled:Arc::clone(&cancelled)});
        assert!(tools.call_tool(WEB_SEARCH_TOOL_NAME,json!({"search_term":"x"}),"call-4").is_err());
        *live.lock().unwrap()=Some("acct:a".into()); cancelled.store(true,Ordering::Release);
        assert!(tools.call_tool(WEB_SEARCH_TOOL_NAME,json!({"search_term":"x"}),"call-5").is_err());
    }
}
