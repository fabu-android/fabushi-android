pub const SHIPPED_CLIENT_SIDE_TOOL_V2_UNION: &[&str] = &[
    "READ_SEMSEARCH_FILES","RIPGREP_SEARCH","READ_FILE","LIST_DIR","EDIT_FILE",
    "FILE_SEARCH","SEMANTIC_SEARCH_FULL","DELETE_FILE","REAPPLY",
    "RUN_TERMINAL_COMMAND_V2","FETCH_RULES","WEB_SEARCH","MCP","SEARCH_SYMBOLS",
    "BACKGROUND_COMPOSER_FOLLOWUP","KNOWLEDGE_BASE","FETCH_PULL_REQUEST","DEEP_SEARCH",
    "CREATE_DIAGRAM","FIX_LINTS","READ_LINTS","GO_TO_DEFINITION","TASK","AWAIT_TASK",
    "TODO_READ","TODO_WRITE","EDIT_FILE_V2","LIST_DIR_V2","READ_FILE_V2",
    "RIPGREP_RAW_SEARCH","GLOB_FILE_SEARCH","CREATE_PLAN","LIST_MCP_RESOURCES",
    "READ_MCP_RESOURCE","READ_PROJECT","UPDATE_PROJECT","TASK_V2","CALL_MCP_TOOL",
    "APPLY_AGENT_DIFF","ASK_QUESTION","SWITCH_MODE","GENERATE_IMAGE","COMPUTER_USE",
    "WRITE_SHELL_STDIN","RECORD_SCREEN","WEB_FETCH","REPORT_BUGFIX_RESULTS",
    "AI_ATTRIBUTION","MCP_AUTH","REFLECT","AWAIT","GET_MCP_TOOLS","SEND_TO_USER",
    "CONNECT_SCM",
];

pub const SHIPPED_AGENT_TOOL_CALL_UNION: &[&str] = &[
    "shellToolCall","deleteToolCall","globToolCall","grepToolCall","readToolCall",
    "updateTodosToolCall","readTodosToolCall","editToolCall","lsToolCall",
    "readLintsToolCall","mcpToolCall","semSearchToolCall","createPlanToolCall",
    "webSearchToolCall","taskToolCall","listMcpResourcesToolCall",
    "readMcpResourceToolCall","applyAgentDiffToolCall","askQuestionToolCall",
    "fetchToolCall","switchModeToolCall","generateImageToolCall",
    "recordScreenToolCall","computerUseToolCall","writeShellStdinToolCall",
    "reflectToolCall","setupVmEnvironmentToolCall","truncatedToolCall",
    "startGrindExecutionToolCall","startGrindPlanningToolCall","webFetchToolCall",
    "reportBugfixResultsToolCall","aiAttributionToolCall","prManagementToolCall",
    "mcpAuthToolCall","awaitToolCall","blameByFilePathToolCall",
    "getMcpToolsToolCall","reportBugToolCall","setActiveBranchToolCall",
    "communicateUpdateToolCall","sendFinalSummaryToolCall","updatePrCodeTourToolCall",
    "replaceEnvToolCall","editPrLabelsToolCall","recordCiInvestigationFindingsToolCall",
    "sendMessageToolCall","fetchCloudAgentDataToolCall","sendToUserToolCall",
    "piReadToolCall","piBashToolCall","piEditToolCall","piWriteToolCall",
    "piGrepToolCall","piFindToolCall","piLsToolCall","connectScmToolCall",
    "searchConversationsToolCall","createGoalToolCall","updateGoalToolCall",
    "adoptToolCall","getAgentStatusToolCall","sendToAgentToolCall",
    "readAgentTranscriptToolCall","createAgentToolCall","stopAgentToolCall",
];

pub const UNPROJECTED_AGENT_TOOL_CALL_ONEOFS: &[&str] = &[
    "deleteToolCall","globToolCall","grepToolCall","updateTodosToolCall",
    "readTodosToolCall","lsToolCall","readLintsToolCall","semSearchToolCall",
    "createPlanToolCall","applyAgentDiffToolCall","fetchToolCall","switchModeToolCall",
    "writeShellStdinToolCall","reflectToolCall","setupVmEnvironmentToolCall",
    "truncatedToolCall","startGrindExecutionToolCall","startGrindPlanningToolCall",
    "reportBugfixResultsToolCall","aiAttributionToolCall","prManagementToolCall",
    "blameByFilePathToolCall","reportBugToolCall","setActiveBranchToolCall",
    "communicateUpdateToolCall","sendFinalSummaryToolCall","updatePrCodeTourToolCall",
    "replaceEnvToolCall","editPrLabelsToolCall","recordCiInvestigationFindingsToolCall",
    "fetchCloudAgentDataToolCall","sendToUserToolCall","piReadToolCall",
    "piBashToolCall","piEditToolCall","piWriteToolCall","piGrepToolCall",
    "piFindToolCall","piLsToolCall","connectScmToolCall","searchConversationsToolCall",
    "createGoalToolCall","updateGoalToolCall","adoptToolCall","getAgentStatusToolCall",
    "sendToAgentToolCall","readAgentTranscriptToolCall","createAgentToolCall",
    "stopAgentToolCall",
];

pub const UNPROJECTED_CLIENT_SIDE_TOOL_V2_VARIANTS: &[&str] = &[
    "READ_SEMSEARCH_FILES","RIPGREP_SEARCH","READ_FILE","LIST_DIR","EDIT_FILE",
    "FILE_SEARCH","SEMANTIC_SEARCH_FULL","DELETE_FILE","REAPPLY","FETCH_RULES",
    "MCP","SEARCH_SYMBOLS","BACKGROUND_COMPOSER_FOLLOWUP","KNOWLEDGE_BASE",
    "FETCH_PULL_REQUEST","DEEP_SEARCH","CREATE_DIAGRAM","FIX_LINTS","READ_LINTS",
    "GO_TO_DEFINITION","TASK","AWAIT_TASK","TODO_READ","TODO_WRITE","LIST_DIR_V2",
    "RIPGREP_RAW_SEARCH","GLOB_FILE_SEARCH","CREATE_PLAN","READ_PROJECT",
    "UPDATE_PROJECT","APPLY_AGENT_DIFF","SWITCH_MODE","WRITE_SHELL_STDIN",
    "REPORT_BUGFIX_RESULTS","AI_ATTRIBUTION","REFLECT","AWAIT","SEND_TO_USER",
    "CONNECT_SCM",
];

pub const CLIENT_SIDE_TOOL_V2_SUPPORTED: &[(&str, &str)] = &[
    ("shellToolCall","RUN_TERMINAL_COMMAND_V2"),
    ("editToolCall","EDIT_FILE_V2"),
    ("readToolCall","READ_FILE_V2 (Read and ExternalRead share this generated agent oneof)"),
    ("mcpToolCall","CALL_MCP_TOOL"),
    ("taskToolCall","TASK_V2"),
    ("listMcpResourcesToolCall","LIST_MCP_RESOURCES"),
    ("readMcpResourceToolCall","READ_MCP_RESOURCE"),
    ("askQuestionToolCall","ASK_QUESTION"),
    ("mcpAuthToolCall","MCP_AUTH"),
    ("webSearchToolCall","WEB_SEARCH"),
    ("webFetchToolCall","WEB_FETCH"),
    ("computerUseToolCall","COMPUTER_USE"),
    ("generateImageToolCall","GENERATE_IMAGE (the shipped call union has no params arm; rawArgs is preserved)"),
    ("recordScreenToolCall","RECORD_SCREEN"),
    ("getMcpToolsToolCall","GET_MCP_TOOLS"),
];

pub const CLIENT_SIDE_TOOL_V2_ORDINARY_TRANSCRIPT_ONLY: &[(&str, &str)] = &[
    ("sendMessageToolCall","the shipped ClientSideToolV2 enum has no SendMessage variant"),
    ("approvalInteractions","host permission queries/cards are lifecycle transcript records, not ClientSideToolV2 call/result variants"),
    ("awaitToolCall","the enum has AWAIT but the shipped call/result unions have no await arms"),
];

pub const CLIENT_SIDE_TOOL_V2_UNRECOVERED_POLICY: &str =
    "not projected until each agent result can be losslessly matched to a generated ClientSideToolV2 result";
pub const CLIENT_SIDE_TOOL_V2_READ_BINARY_OUTPUTS_POLICY: &str =
    "READ_FILE_V2 carries text; binary/blob identifiers remain in ordinary transcript";

pub fn supported_projection(agent_tool_oneof: &str) -> Option<&'static str> {
    CLIENT_SIDE_TOOL_V2_SUPPORTED.iter()
        .find_map(|(source,target)| (*source==agent_tool_oneof).then_some(*target))
}

pub fn is_ordinary_transcript_only(name: &str) -> bool {
    CLIENT_SIDE_TOOL_V2_ORDINARY_TRANSCRIPT_ONLY.iter().any(|(entry,_)| *entry==name)
}

pub fn is_unprojected_agent_tool(name: &str) -> bool {
    UNPROJECTED_AGENT_TOOL_CALL_ONEOFS.contains(&name)
}

pub fn is_unprojected_client_side_variant(name: &str) -> bool {
    UNPROJECTED_CLIENT_SIDE_TOOL_V2_VARIANTS.contains(&name)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_projection_is_explicit_and_never_promotes_unrecovered_variants() {
        assert_eq!(supported_projection("shellToolCall"), Some("RUN_TERMINAL_COMMAND_V2"));
        assert_eq!(
            supported_projection("readToolCall"),
            Some("READ_FILE_V2 (Read and ExternalRead share this generated agent oneof)")
        );
        assert_eq!(supported_projection("sendMessageToolCall"), None);
        assert!(is_ordinary_transcript_only("sendMessageToolCall"));
        assert!(is_unprojected_agent_tool("connectScmToolCall"));
        assert!(is_unprojected_client_side_variant("CONNECT_SCM"));
    }

    #[test]
    fn every_supported_projection_names_a_shipped_client_side_variant() {
        for (source, target) in CLIENT_SIDE_TOOL_V2_SUPPORTED {
            assert!(SHIPPED_AGENT_TOOL_CALL_UNION.contains(source));
            let target_name = target.split_once(' ').map(|(name, _)| name).unwrap_or(target);
            assert!(
                SHIPPED_CLIENT_SIDE_TOOL_V2_UNION.contains(&target_name),
                "{source} points at unknown ClientSideToolV2 variant {target_name}"
            );
        }
    }
}
