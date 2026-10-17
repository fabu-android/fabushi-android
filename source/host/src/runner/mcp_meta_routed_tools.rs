use super::AndroidRoutedToolBridge;
use crate::capability_broker::{PendingCapabilityCall, SharedCapabilityBroker};
use crate::mcp_auth::dashboard_mcp_auth_backend::{
    CursorDashboardMcpAuthBackend, McpMetaServerDescriptor, McpToolExecutionBackendResult,
};
use crate::mcp_server_tool_settings::McpServerToolSettings;
use fabushi_android_mcp_core::config::mcp_tool_call_timeout::DEFAULT_MCP_TOOL_CALL_MAX_TOTAL_TIMEOUT_MS;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex};

pub const GET_MCP_TOOLS_TOOL_NAME: &str = "GetMcpTools";
pub const CALL_MCP_TOOL_NAME: &str = "CallMcpTool";
pub const MCP_META_TOOL_PROVIDER: &str = "mahayana-mcp-meta";
pub const EXTRA_SHORT_TOOL_TIMEOUT_MS: u64 = 5 * 60 * 1_000;
pub const SHORT_TOOL_TIMEOUT_MS: u64 = 15 * 60 * 1_000;
pub const MEDIUM_TOOL_TIMEOUT_MS: u64 = 30 * 60 * 1_000;
pub const LONG_TOOL_TIMEOUT_MS: u64 = DEFAULT_MCP_TOOL_CALL_MAX_TOTAL_TIMEOUT_MS;
pub const EXTRA_LONG_TOOL_TIMEOUT_MS: u64 = 2 * 60 * 60 * 1_000;
const TIMEOUT_BUFFER_MS: u64 = 60 * 1_000;
pub const TOOL_CALL_TIMEOUT_TIERS_MS: [u64; 5] = [
    EXTRA_SHORT_TOOL_TIMEOUT_MS,
    SHORT_TOOL_TIMEOUT_MS,
    MEDIUM_TOOL_TIMEOUT_MS,
    LONG_TOOL_TIMEOUT_MS,
    EXTRA_LONG_TOOL_TIMEOUT_MS,
];

#[derive(Clone, Debug)]
pub struct McpMetaCallTarget {
    pub server_id: String,
    pub server_identifier: String,
    pub server_name: String,
    pub tool_name: String,
    pub arguments: Value,
}

pub type McpMetaReviewPreflight =
    Arc<dyn Fn(&McpMetaCallTarget, &str) -> Result<(), String> + Send + Sync>;

trait McpMetaBackend: Send + Sync {
    fn list_descriptors(&self) -> Result<Vec<McpMetaServerDescriptor>, String>;
    fn execute(
        &self,
        target: &McpMetaCallTarget,
        tool_call_id: &str,
        agent_id: &str,
        timeout_ms: u64,
    ) -> Result<McpToolExecutionBackendResult, String>;
}

impl McpMetaBackend for CursorDashboardMcpAuthBackend {
    fn list_descriptors(&self) -> Result<Vec<McpMetaServerDescriptor>, String> {
        self.list_meta_server_descriptors()
    }

    fn execute(
        &self,
        target: &McpMetaCallTarget,
        tool_call_id: &str,
        agent_id: &str,
        timeout_ms: u64,
    ) -> Result<McpToolExecutionBackendResult, String> {
        self.execute_server_tool_with_timeout(
            &target.server_id,
            &target.tool_name,
            &target.arguments,
            tool_call_id,
            Some(agent_id),
            timeout_ms,
        )
    }
}

struct McpMetaRoutedTools {
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    backend: Arc<dyn McpMetaBackend>,
    settings: Arc<Mutex<McpServerToolSettings>>,
    broker: SharedCapabilityBroker,
    account_fence: String,
    live_account_fence: Arc<Mutex<Option<String>>>,
    agent_id: String,
    cancelled: Arc<AtomicBool>,
    review_preflight: Option<McpMetaReviewPreflight>,
}

impl McpMetaRoutedTools {
    fn assert_live(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err("MCP meta tool call cancelled".into());
        }
        let live = self.live_account_fence.lock()
            .map_err(|_| "MCP meta account fence lock poisoned".to_string())?;
        if live.as_deref() != Some(self.account_fence.as_str()) {
            return Err("MCP meta result belongs to a stale account epoch".into());
        }
        Ok(())
    }

    fn live_descriptors(&self) -> Result<Vec<McpMetaServerDescriptor>, String> {
        self.assert_live()?;
        let descriptors = self.backend.list_descriptors()?;
        self.assert_live()?;
        Ok(descriptors)
    }

    fn disabled_for(&self, server_id: &str) -> Result<BTreeSet<String>, String> {
        self.settings.lock()
            .map_err(|_| "MCP server tool settings lock poisoned".to_string())?
            .disabled_for(&self.account_fence, server_id)
    }

    fn discovery(&self, args: &Value) -> Result<Value, String> {
        let descriptors = self.live_descriptors()?;
        let server_filter = optional_trimmed(args, "server");
        let tool_filter = optional_trimmed(args, "toolName");
        let pattern = optional_trimmed(args, "pattern").map(str::to_ascii_lowercase);
        let mut rendered = Vec::new();
        for descriptor in descriptors {
            if server_filter.is_some_and(|server| server != descriptor.server_identifier) {
                continue;
            }
            let disabled = self.disabled_for(&descriptor.server_id)?;
            let mut tools = descriptor.tools.into_iter()
                .filter(|tool| !disabled.contains(&tool.name))
                .filter(|tool| tool_filter.map_or(true, |name| name == tool.name))
                .filter(|tool| match pattern.as_ref() {
                    None => true,
                    Some(needle) => {
                        descriptor.server_identifier.to_ascii_lowercase().contains(needle)
                            || descriptor.server_name.to_ascii_lowercase().contains(needle)
                            || tool.name.to_ascii_lowercase().contains(needle)
                            || tool.description.as_deref().unwrap_or_default().to_ascii_lowercase().contains(needle)
                    }
                })
                .map(|tool| json!({
                    "toolName":tool.name,
                    "description":tool.description,
                    "inputSchema":tool.input_schema,
                }))
                .collect::<Vec<_>>();
            tools.sort_by(|left, right| left["toolName"].as_str().cmp(&right["toolName"].as_str()));
            if tools.is_empty() && (tool_filter.is_some() || pattern.is_some()) {
                continue;
            }
            rendered.push(json!({
                "serverIdentifier":descriptor.server_identifier,
                "serverName":descriptor.server_name,
                "plugin":Value::Null,
                "marketplace":Value::Null,
                "pluginDbId":Value::Null,
                "marketplaceId":Value::Null,
                "tools":tools,
            }));
        }
        rendered.sort_by(|left, right| left["serverIdentifier"].as_str().cmp(&right["serverIdentifier"].as_str()));
        Ok(json!({"enabled":true,"mcpDescriptors":rendered}))
    }

    fn call_mcp(&self, args: Value, tool_call_id: &str) -> Result<Value, String> {
        self.assert_live()?;
        if tool_call_id.trim().is_empty() {
            return Err("CallMcpTool tool_call_id is required".into());
        }
        let server_identifier = required_trimmed(&args, "server")?;
        let tool_name = required_trimmed(&args, "toolName")?;
        let arguments = args.get("arguments").cloned().unwrap_or_else(|| json!({}));
        if !arguments.is_object() {
            return Err("CallMcpTool arguments must be an object".into());
        }
        let descriptors = self.live_descriptors()?;
        let descriptor = descriptors.into_iter()
            .find(|candidate| candidate.server_identifier == server_identifier)
            .ok_or_else(|| format!("MCP server {server_identifier} was not found in the current catalog"))?;
        if self.disabled_for(&descriptor.server_id)?.contains(tool_name) {
            return Err(format!("MCP tool {tool_name} is disabled for the current account"));
        }
        let tool = descriptor.tools.iter().find(|tool| tool.name == tool_name)
            .ok_or_else(|| format!("MCP tool {tool_name} was not found on server {server_identifier}"))?;
        let target = McpMetaCallTarget {
            server_id: descriptor.server_id.clone(),
            server_identifier: descriptor.server_identifier.clone(),
            server_name: descriptor.server_name.clone(),
            tool_name: tool.name.clone(),
            arguments: arguments.clone(),
        };
        if let Some(preflight) = self.review_preflight.as_ref() {
            preflight(&target, tool_call_id)?;
        }
        self.assert_live()?;

        let fingerprint = crate::sha256::sha256_hex(format!(
            "{}\n{}\n{}\n{}",
            self.account_fence, tool_call_id, target.server_id, target.tool_name,
        ).as_bytes());
        let request_id = format!("mcp-meta-exec:{}", &fingerprint[..32]);
        let plugin_id = format!("mcp:{}", target.server_id);
        let generation = account_fence_generation(&self.account_fence);
        let started = unix_time_ms();
        let timeout_ms = pick_tool_call_timeout_tier_ms(suggested_tool_timeout_ms(&target.tool_name, &arguments));
        self.broker.begin(PendingCapabilityCall {
            request_id: request_id.clone(),
            plugin_id: plugin_id.clone(),
            capability: "mcp.execute".into(),
            tool: target.tool_name.clone(),
            arguments: arguments.clone(),
            required_permissions: BTreeSet::new(),
            account_fence: self.account_fence.clone(),
            runtime_generation: generation,
            started_at_ms: started,
            deadline_at_ms: started.saturating_add(timeout_ms),
            state: "pending".into(),
        })?;
        self.broker.assert_current(
            &request_id, &plugin_id, &self.account_fence, generation, unix_time_ms(),
        )?;

        let execution = self.backend.execute(&target, tool_call_id, &self.agent_id, timeout_ms);
        let finished = unix_time_ms();
        if self.assert_live().is_err() {
            if self.broker.assert_current(
                &request_id, &plugin_id, &self.account_fence, generation, finished,
            ).is_ok() {
                let _ = self.broker.settle(
                    &request_id,
                    "outcome_unknown",
                    Some("account switch or cancellation occurred while MCP tool execution was in flight; reconcile before replay".into()),
                    finished,
                );
            }
            return Err("MCP meta execution result is stale; reconciliation required before retry".into());
        }
        match execution {
            Ok(McpToolExecutionBackendResult::Settled(value)) => {
                self.broker.assert_current(
                    &request_id, &plugin_id, &self.account_fence, generation, finished,
                )?;
                self.broker.settle(&request_id, "completed", None, finished)?;
                Ok(value)
            }
            Ok(McpToolExecutionBackendResult::OutcomeUnknown { message }) => {
                self.broker.settle(
                    &request_id, "outcome_unknown", Some(message.clone()), finished,
                )?;
                Err(format!("{message} Do not retry automatically; reconcile first."))
            }
            Err(error) => {
                self.broker.settle(
                    &request_id, "outcome_unknown", Some(format!("MCP execution transport failed: {error}")), finished,
                )?;
                Err(format!("MCP execution outcome is unknown: {error}; do not retry automatically"))
            }
        }
    }
}

impl AndroidRoutedToolBridge for McpMetaRoutedTools {
    fn list_tools(&self) -> Result<Vec<Value>, String> {
        // Desktop meta mode intentionally exposes only the two meta tools instead of thousands of
        // direct MCP tools. The delegate still owns non-MCP first-party tools.
        let mut tools = self.delegate.list_tools()?;
        tools.retain(|tool| !matches!(tool.get("name").and_then(Value::as_str), Some(GET_MCP_TOOLS_TOOL_NAME | CALL_MCP_TOOL_NAME)));
        tools.push(get_tool_definition());
        tools.push(call_tool_definition());
        Ok(tools)
    }

    fn call_tool(&self, name: &str, args: Value, tool_call_id: &str) -> Result<Value, String> {
        match name {
            GET_MCP_TOOLS_TOOL_NAME => self.discovery(&args),
            CALL_MCP_TOOL_NAME => self.call_mcp(args, tool_call_id),
            _ => self.delegate.call_tool(name, args, tool_call_id),
        }
    }
}

pub(crate) fn with_mcp_meta_routed_tools(
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    backend: Arc<CursorDashboardMcpAuthBackend>,
    settings: Arc<Mutex<McpServerToolSettings>>,
    broker: SharedCapabilityBroker,
    account_fence: &str,
    live_account_fence: Arc<Mutex<Option<String>>>,
    agent_id: &str,
    cancelled: Arc<AtomicBool>,
    review_preflight: Option<McpMetaReviewPreflight>,
) -> Arc<dyn AndroidRoutedToolBridge> {
    Arc::new(McpMetaRoutedTools {
        delegate,
        backend,
        settings,
        broker,
        account_fence: account_fence.to_string(),
        live_account_fence,
        agent_id: agent_id.to_string(),
        cancelled,
        review_preflight,
    })
}

fn get_tool_definition() -> Value {
    json!({
        "type":"function",
        "name":GET_MCP_TOOLS_TOOL_NAME,
        "description":"Discover available MCP servers and tool schemas before invoking them.",
        "parameters":{"type":"object","additionalProperties":false,"properties":{
            "server":{"type":"string"},"toolName":{"type":"string"},"pattern":{"type":"string"}
        }}
    })
}

fn call_tool_definition() -> Value {
    json!({
        "type":"function",
        "name":CALL_MCP_TOOL_NAME,
        "description":"Invoke one MCP tool after discovering its schema with GetMcpTools.",
        "parameters":{"type":"object","additionalProperties":false,"required":["server","toolName"],"properties":{
            "server":{"type":"string"},"toolName":{"type":"string"},"arguments":{"type":"object"}
        }}
    })
}

fn required_trimmed<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    optional_trimmed(args, key).ok_or_else(|| format!("{key} is required"))
}

fn optional_trimmed<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str).map(str::trim).filter(|value| !value.is_empty())
}

pub fn parse_block_until_ms(args: &Value) -> Option<u64> {
    let raw = args.get("block_until_ms")?.as_f64()?;
    if !raw.is_finite() || raw < 0.0 { return None; }
    if raw >= u64::MAX as f64 { Some(u64::MAX) } else { Some(raw.floor() as u64) }
}

pub fn is_subagent_tool_name(tool_name: &str) -> bool {
    matches!(tool_name.trim().to_ascii_lowercase().as_str(), "task" | "mcp_task" | "subagent")
}

pub fn suggested_tool_timeout_ms(tool_name: &str, args: &Value) -> u64 {
    if is_subagent_tool_name(tool_name) { return LONG_TOOL_TIMEOUT_MS; }
    parse_block_until_ms(args)
        .map(|block| block.saturating_add(TIMEOUT_BUFFER_MS))
        .unwrap_or(SHORT_TOOL_TIMEOUT_MS)
}

pub fn pick_tool_call_timeout_tier_ms(suggested_ms: u64) -> u64 {
    TOOL_CALL_TIMEOUT_TIERS_MS.into_iter().find(|tier| *tier >= suggested_ms).unwrap_or(EXTRA_LONG_TOOL_TIMEOUT_MS)
}

fn account_fence_generation(account_fence: &str) -> u64 {
    let digest = crate::sha256::sha256_hex(account_fence.as_bytes());
    u64::from_str_radix(&digest[..16], 16).unwrap_or(1).max(1)
}

fn unix_time_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis().min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_auth::dashboard_mcp_auth_backend::McpServerToolSummary;

    #[test]
    fn long_timeout_tier_uses_desktop_mcp_default_contract() {
        assert_eq!(LONG_TOOL_TIMEOUT_MS, DEFAULT_MCP_TOOL_CALL_MAX_TOTAL_TIMEOUT_MS);
    }

    #[test]
    fn timeout_tiers_match_desktop_meta_contract() {
        assert_eq!(suggested_tool_timeout_ms("read", &json!({})), SHORT_TOOL_TIMEOUT_MS);
        assert_eq!(suggested_tool_timeout_ms("Task", &json!({})), LONG_TOOL_TIMEOUT_MS);
        assert_eq!(suggested_tool_timeout_ms("shell", &json!({"block_until_ms":1_000})), 61_000);
        assert_eq!(pick_tool_call_timeout_tier_ms(61_000), EXTRA_SHORT_TOOL_TIMEOUT_MS);
        assert_eq!(pick_tool_call_timeout_tier_ms(EXTRA_LONG_TOOL_TIMEOUT_MS + 1), EXTRA_LONG_TOOL_TIMEOUT_MS);
    }

    struct EmptyBridge;
    impl AndroidRoutedToolBridge for EmptyBridge {
        fn list_tools(&self) -> Result<Vec<Value>, String> { Ok(Vec::new()) }
        fn call_tool(&self, name: &str, _args: Value, _tool_call_id: &str) -> Result<Value, String> {
            Err(format!("unexpected delegate {name}"))
        }
    }

    #[derive(Clone)]
    struct FakeBackend {
        calls: Arc<std::sync::atomic::AtomicUsize>,
        outcome_unknown: bool,
    }
    impl McpMetaBackend for FakeBackend {
        fn list_descriptors(&self) -> Result<Vec<McpMetaServerDescriptor>, String> {
            Ok(vec![McpMetaServerDescriptor {
                server_id: "17".into(),
                server_identifier: "calendar/default".into(),
                server_name: "Calendar".into(),
                tools: vec![
                    McpServerToolSummary {
                        name: "list".into(),
                        title: Some("List events".into()),
                        description: Some("List calendar events".into()),
                        provider_identifier: "calendar/default".into(),
                        input_schema: json!({"type":"object","properties":{"limit":{"type":"integer"}}}),
                    },
                    McpServerToolSummary {
                        name: "create".into(),
                        title: Some("Create event".into()),
                        description: Some("Create an event".into()),
                        provider_identifier: "calendar/default".into(),
                        input_schema: json!({"type":"object"}),
                    },
                ],
            }])
        }

        fn execute(
            &self,
            target: &McpMetaCallTarget,
            _tool_call_id: &str,
            _agent_id: &str,
            timeout_ms: u64,
        ) -> Result<McpToolExecutionBackendResult, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(target.server_identifier, "calendar/default");
            assert_eq!(timeout_ms, SHORT_TOOL_TIMEOUT_MS);
            if self.outcome_unknown {
                Ok(McpToolExecutionBackendResult::OutcomeUnknown { message: "timed out".into() })
            } else {
                Ok(McpToolExecutionBackendResult::Settled(json!({"case":"success","content":[]})))
            }
        }
    }

    fn test_bridge(
        root: &std::path::Path,
        live: Arc<Mutex<Option<String>>>,
        outcome_unknown: bool,
    ) -> (McpMetaRoutedTools, Arc<std::sync::atomic::AtomicUsize>) {
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let backend = Arc::new(FakeBackend { calls: Arc::clone(&calls), outcome_unknown });
        let settings = Arc::new(Mutex::new(
            McpServerToolSettings::open(root.join("mcp-settings.json")).unwrap(),
        ));
        let broker = SharedCapabilityBroker::open(root.join("capability-broker.json"), unix_time_ms()).unwrap();
        (McpMetaRoutedTools {
            delegate: Arc::new(EmptyBridge),
            backend,
            settings,
            broker,
            account_fence: "account-a".into(),
            live_account_fence: live,
            agent_id: "agent-a".into(),
            cancelled: Arc::new(AtomicBool::new(false)),
            review_preflight: Some(Arc::new(|_, _| Ok(()))),
        }, calls)
    }

    #[test]
    fn discovery_preserves_input_schema_and_hides_disabled_tools() {
        let root = tempfile::tempdir().unwrap();
        let live = Arc::new(Mutex::new(Some("account-a".into())));
        let (bridge, _) = test_bridge(root.path(), live, false);
        bridge.settings.lock().unwrap().toggle("account-a", "17", "create").unwrap();
        let found = bridge.call_tool(GET_MCP_TOOLS_TOOL_NAME, json!({"server":"calendar/default"}), "discover").unwrap();
        assert_eq!(found["mcpDescriptors"][0]["serverIdentifier"], "calendar/default");
        assert_eq!(found["mcpDescriptors"][0]["tools"].as_array().unwrap().len(), 1);
        assert_eq!(found["mcpDescriptors"][0]["tools"][0]["toolName"], "list");
        assert_eq!(found["mcpDescriptors"][0]["tools"][0]["inputSchema"]["properties"]["limit"]["type"], "integer");
    }

    #[test]
    fn call_mcp_revalidates_catalog_records_broker_and_rejects_duplicate_replay() {
        let root = tempfile::tempdir().unwrap();
        let live = Arc::new(Mutex::new(Some("account-a".into())));
        let (bridge, calls) = test_bridge(root.path(), live, false);
        let args = json!({"server":"calendar/default","toolName":"list","arguments":{"limit":2}});
        let result = bridge.call_tool(CALL_MCP_TOOL_NAME, args.clone(), "tool-call-1").unwrap();
        assert_eq!(result["case"], "success");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let duplicate = bridge.call_tool(CALL_MCP_TOOL_NAME, args, "tool-call-1").unwrap_err();
        assert!(duplicate.contains("duplicate") || duplicate.contains("terminal"));
        assert_eq!(calls.load(Ordering::SeqCst), 1, "duplicate tool call must not replay side effects");
    }

    #[test]
    fn outcome_unknown_and_account_switch_are_fail_closed_without_retry() {
        let root = tempfile::tempdir().unwrap();
        let live = Arc::new(Mutex::new(Some("account-a".into())));
        let (bridge, calls) = test_bridge(root.path(), Arc::clone(&live), true);
        let error = bridge.call_tool(CALL_MCP_TOOL_NAME, json!({
            "server":"calendar/default","toolName":"list","arguments":{}
        }), "tool-call-unknown").unwrap_err();
        assert!(error.contains("Do not retry automatically"));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        *live.lock().unwrap() = Some("account-b".into());
        let stale = bridge.call_tool(GET_MCP_TOOLS_TOOL_NAME, json!({}), "discover-stale").unwrap_err();
        assert!(stale.contains("stale account"));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn meta_tool_definitions_match_desktop_names_and_required_arguments() {
        let get = get_tool_definition();
        let call = call_tool_definition();
        assert_eq!(get["name"], GET_MCP_TOOLS_TOOL_NAME);
        assert_eq!(call["name"], CALL_MCP_TOOL_NAME);
        assert_eq!(call["parameters"]["required"], json!(["server","toolName"]));
    }
}
