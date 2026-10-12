use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoutedMcpTool {
    pub name: String,
    pub provider: String,
    pub remote_name: String,
    pub read_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoutedMcpCall {
    pub call_id: String,
    pub tool_name: String,
    pub arguments_json: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoutedMcpAuthChallenge {
    pub provider: String,
    pub authorization_url: String,
    pub state: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoutedMcpFailureCode {
    InvalidCall,
    UnknownTool,
    AuthorizationRequired,
    ExecutionFailed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoutedMcpFailure {
    pub code: RoutedMcpFailureCode,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoutedMcpOutcome {
    Result { call_id: String, payload_json: String },
    AuthorizationRequired { call_id: String, challenge: RoutedMcpAuthChallenge },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoutedMcpProviderError {
    AuthorizationRequired { authorization_url: String, state: String },
    Failed,
}

pub trait RoutedMcpExecutor {
    fn execute(
        &mut self,
        provider: &str,
        remote_name: &str,
        arguments_json: &str,
    ) -> Result<String, RoutedMcpProviderError>;
}

#[derive(Default)]
pub struct RoutedMcpBridge {
    tools: BTreeMap<String, RoutedMcpTool>,
}

impl RoutedMcpBridge {
    pub fn replace_tools(
        &mut self,
        tools: impl IntoIterator<Item = RoutedMcpTool>,
    ) -> Result<(), &'static str> {
        let mut next = BTreeMap::new();
        for tool in tools {
            if tool.name.trim().is_empty()
                || tool.provider.trim().is_empty()
                || tool.remote_name.trim().is_empty()
            {
                return Err("MCP tool identity fields are required");
            }
            if next.insert(tool.name.clone(), tool).is_some() {
                return Err("duplicate routed MCP tool name");
            }
        }
        self.tools = next;
        Ok(())
    }

    pub fn tool(&self, name: &str) -> Option<&RoutedMcpTool> {
        self.tools.get(name)
    }

    pub fn list(&self) -> Vec<&RoutedMcpTool> {
        self.tools.values().collect()
    }

    pub fn execute<E: RoutedMcpExecutor>(
        &self,
        executor: &mut E,
        call: RoutedMcpCall,
    ) -> Result<RoutedMcpOutcome, RoutedMcpFailure> {
        if call.call_id.trim().is_empty() || call.tool_name.trim().is_empty() {
            return Err(RoutedMcpFailure {
                code: RoutedMcpFailureCode::InvalidCall,
                message: "call_id and tool_name are required".into(),
            });
        }
        let tool = self.tools.get(&call.tool_name).ok_or_else(|| RoutedMcpFailure {
            code: RoutedMcpFailureCode::UnknownTool,
            message: "routed MCP tool is not registered".into(),
        })?;
        match executor.execute(&tool.provider, &tool.remote_name, &call.arguments_json) {
            Ok(payload_json) => Ok(RoutedMcpOutcome::Result {
                call_id: call.call_id,
                payload_json,
            }),
            Err(RoutedMcpProviderError::AuthorizationRequired { authorization_url, state }) => {
                if authorization_url.trim().is_empty() || state.trim().is_empty() {
                    return Err(RoutedMcpFailure {
                        code: RoutedMcpFailureCode::ExecutionFailed,
                        message: "provider returned an invalid authorization challenge".into(),
                    });
                }
                Ok(RoutedMcpOutcome::AuthorizationRequired {
                    call_id: call.call_id,
                    challenge: RoutedMcpAuthChallenge {
                        provider: tool.provider.clone(),
                        authorization_url,
                        state,
                    },
                })
            }
            Err(RoutedMcpProviderError::Failed) => Err(RoutedMcpFailure {
                code: RoutedMcpFailureCode::ExecutionFailed,
                message: "routed MCP provider execution failed".into(),
            }),
        }
    }
}
