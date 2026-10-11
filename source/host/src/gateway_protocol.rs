use std::collections::BTreeSet;

pub fn sand_gateway_commands() -> BTreeSet<&'static str> {
    [
        "health","listAgents","countAgents","searchAgents","createAgent","updateAgent","deleteAgents",
        "sendPrompt","respondToWidget","getAgentTranscriptWindow","getAgentThread","getAgentTranscriptTail",
        "openAgentTail","listRoutedMcpTools","executeRoutedMcpTool","getAgentAutomations","listAllAutomations",
        "createAgentAutomation","updateAgentAutomation","deleteAgentAutomation","runAgentAutomationNow",
        "getAgentWorkflows","createAgentWorkflow","updateAgentWorkflow","deleteAgentWorkflow","runAgentWorkflowNow",
        "getSharingState","createRoomFromAgent","createRoomInvite","joinSharedRoom","leaveSharedRoom",
        "getForeverBoxStatus","ensureForeverBox","handBackForeverBox","searchMedia","getTrays","dismissTray","clearTrays",
    ].into_iter().collect()
}

pub fn is_known_gateway_command(method: &str) -> bool { sand_gateway_commands().contains(method) }

pub fn validate_command_payload(method: &str, json: &str) -> Result<(), &'static str> {
    if !is_known_gateway_command(method) { return Err("unknown gateway command"); }
    let trimmed = json.trim();
    if !(trimmed.starts_with('{') && trimmed.ends_with('}')) { return Err("gateway command payload must be a JSON object"); }
    Ok(())
}
