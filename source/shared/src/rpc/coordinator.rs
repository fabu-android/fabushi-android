#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoordinatorMethodKind {
    Transcript,
    Agent,
    Tool,
    Mcp,
    Automation,
    Workflow,
    Sharing,
    Box,
    Other,
}

pub fn classify_coordinator_method(method: &str) -> Option<CoordinatorMethodKind> {
    let kind = match method {
        "getAgentTranscriptWindow" | "getAgentThread" | "getAgentTranscriptTail" | "openAgentTail" => CoordinatorMethodKind::Transcript,
        "listAgents" | "countAgents" | "searchAgents" | "createAgent" | "updateAgent" | "deleteAgents" => CoordinatorMethodKind::Agent,
        "sendPrompt" | "respondToWidget" | "resolveLocalToolPermission" => CoordinatorMethodKind::Tool,
        "listRoutedMcpTools" | "executeRoutedMcpTool" | "syncPluginSkills" | "getPluginSyncStatus" => CoordinatorMethodKind::Mcp,
        "getAgentAutomations" | "listAllAutomations" | "createAgentAutomation" | "updateAgentAutomation" | "deleteAgentAutomation" | "runAgentAutomationNow" => CoordinatorMethodKind::Automation,
        "getAgentWorkflows" | "createAgentWorkflow" | "updateAgentWorkflow" | "deleteAgentWorkflow" | "runAgentWorkflowNow" => CoordinatorMethodKind::Workflow,
        "getSharingState" | "createRoomFromAgent" | "createRoomInvite" | "joinSharedRoom" | "leaveSharedRoom" => CoordinatorMethodKind::Sharing,
        "getForeverBoxStatus" | "ensureForeverBox" | "handBackForeverBox" => CoordinatorMethodKind::Box,
        "reactToMessage" | "searchMedia" | "getTrays" | "dismissTray" | "clearTrays" => CoordinatorMethodKind::Other,
        _ => return None,
    };
    Some(kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_methods_fail_closed() {
        assert_eq!(classify_coordinator_method("totallyUnknown"), None);
        assert_eq!(classify_coordinator_method("sendPrompt"), Some(CoordinatorMethodKind::Tool));
    }
}
