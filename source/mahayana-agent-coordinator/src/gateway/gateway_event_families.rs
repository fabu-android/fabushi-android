pub fn sse_channel_for_family(family: &str) -> Option<&'static str> {
    Some(match family {
        "transcript" => "transcript",
        "client-side-tool-v2" => "client-side-tool-v2",
        "agents" => "agents",
        "agent-upserted" => "agent-upserted",
        "tray" => "tray",
        "agents-workflow" => "workflows",
        "subagents" => "subagents",
        "async-tasks" => "async-tasks",
        "agents-automation" => "automations",
        "mcp-servers-updated" => "mcp-servers",
        "forever-box" => "forever-box",
        "teach-recording" => "teach-recording",
        "box-disk-pressure" => "box-disk-pressure",
        "computer-action" => "computer-action",
        "outline" => "outline",
        "sharing" => "sharing",
        "host-settings" => "host-settings",
        _ => return None,
    })
}

pub fn family_for_sse_channel(channel: &str) -> Option<&'static str> {
    const FAMILIES: &[&str] = &[
        "transcript","client-side-tool-v2","agents","agent-upserted","tray","agents-workflow",
        "subagents","async-tasks","agents-automation","mcp-servers-updated","forever-box",
        "teach-recording","box-disk-pressure","computer-action","outline","sharing","host-settings",
    ];
    FAMILIES.iter().copied().find(|family| sse_channel_for_family(family) == Some(channel))
}
