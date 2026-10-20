// @generated from bhrumom/fabushi-desktop@371b72f1cd20de03dc4dff9a15ed761054968093.
// Desktop source/host/build.rs blob: c9ae9d84176310741c75416524d4658fa56d050b.
// Frozen source/packages/proto/generated/agent/v1 tree: 451b725829c31d3e2eb7115ee7957c444260ed8e.
// Regenerate from Desktop canonical schema authority; do not hand-edit descriptors.

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
struct BlobReferenceFieldDescriptor { field_number: u64, blob_reference_type: &'static str, map_value: bool, skip: bool }
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
struct NestedMessageFieldDescriptor { field_number: u64, child_type: &'static str, map_value: bool }
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
struct MessageDescriptor { blob_fields: &'static [BlobReferenceFieldDescriptor], nested_fields: &'static [NestedMessageFieldDescriptor] }

fn is_proto_blob_reference_type(type_name: &str) -> bool {
    matches!(type_name,
        "agent.v1.ConversationPlan" |
        "agent.v1.ConversationStateStructure" |
        "agent.v1.ConversationStep" |
        "agent.v1.ConversationSummary" |
        "agent.v1.ConversationSummaryArchive" |
        "agent.v1.ConversationTurnStructure" |
        "agent.v1.FileState" |
        "agent.v1.InvocationContext" |
        "agent.v1.RequestContextMcpsPart" |
        "agent.v1.RequestContextRulesPart" |
        "agent.v1.RequestContextSkillsPart" |
        "agent.v1.RequestContextSubagentsPart" |
        "agent.v1.SelectedGitPRDiffSelection" |
        "agent.v1.SelectedPullRequest" |
        "agent.v1.ShellCommand" |
        "agent.v1.ShellOutput" |
        "agent.v1.SubagentPersistedState" |
        "agent.v1.TodoItem" |
        "agent.v1.UserMessage" |
        "__never__"
    )
}

fn message_descriptor(type_name: &str) -> Option<MessageDescriptor> {
    match type_name {
        "agent.v1.AdoptArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.AdoptResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.AdoptToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.AdoptArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.AdoptResult", map_value: false },
            ],
        }),
        "agent.v1.AgentConversationTurnStructure" => Some(MessageDescriptor {
            blob_fields: &[
                BlobReferenceFieldDescriptor { field_number: 1, blob_reference_type: "agent.v1.UserMessage", map_value: false, skip: false },
                BlobReferenceFieldDescriptor { field_number: 2, blob_reference_type: "agent.v1.ConversationStep", map_value: false, skip: false },
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.AgentSkill" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.AiAttributionArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.AiAttributionError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.AiAttributionResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.AiAttributionSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.AiAttributionError", map_value: false },
            ],
        }),
        "agent.v1.AiAttributionSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.OutputLocation", map_value: false },
            ],
        }),
        "agent.v1.AiAttributionToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.AiAttributionArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.AiAttributionResult", map_value: false },
            ],
        }),
        "agent.v1.AppliedAgentChange" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ApplyAgentDiffArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ApplyAgentDiffError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.AppliedAgentChange", map_value: false },
            ],
        }),
        "agent.v1.ApplyAgentDiffResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ApplyAgentDiffSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ApplyAgentDiffError", map_value: false },
            ],
        }),
        "agent.v1.ApplyAgentDiffSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.AppliedAgentChange", map_value: false },
            ],
        }),
        "agent.v1.ApplyAgentDiffToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ApplyAgentDiffArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ApplyAgentDiffResult", map_value: false },
            ],
        }),
        "agent.v1.AskQuestionArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.AskQuestionArgs.Question", map_value: false },
            ],
        }),
        "agent.v1.AskQuestionArgs.Option" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.AskQuestionArgs.Question" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.AskQuestionArgs.Option", map_value: false },
            ],
        }),
        "agent.v1.AskQuestionAsync" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.AskQuestionError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.AskQuestionRejected" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.AskQuestionResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.AskQuestionSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.AskQuestionError", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.AskQuestionRejected", map_value: false },
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.AskQuestionAsync", map_value: false },
            ],
        }),
        "agent.v1.AskQuestionSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.AskQuestionSuccess.Answer", map_value: false },
            ],
        }),
        "agent.v1.AskQuestionSuccess.Answer" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.AskQuestionToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.AskQuestionArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.AskQuestionResult", map_value: false },
            ],
        }),
        "agent.v1.AssistantMessage" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.AwaitArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.AwaitError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.AwaitResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.AwaitTaskComplete", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.AwaitTaskStillRunning", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.AwaitError", map_value: false },
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.AwaitSuccess", map_value: false },
            ],
        }),
        "agent.v1.AwaitSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.AwaitTaskComplete", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.AwaitTaskStillRunning", map_value: false },
            ],
        }),
        "agent.v1.AwaitTaskComplete" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.AwaitTaskStillRunning" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.AwaitToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.AwaitArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.AwaitResult", map_value: false },
            ],
        }),
        "agent.v1.BlameByFilePathArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.BlameByFilePathError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.BlameByFilePathResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.BlameByFilePathSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.BlameByFilePathError", map_value: false },
            ],
        }),
        "agent.v1.BlameByFilePathSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.BlameByFilePathToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.BlameByFilePathArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.BlameByFilePathResult", map_value: false },
            ],
        }),
        "agent.v1.BugfixResultItem" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.CallFrame" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ClickAction" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.Coordinate", map_value: false },
            ],
        }),
        "agent.v1.CloudSubagentReference" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.CommandClassifierResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.CommandClassifierResult.ClassifiedCommand", map_value: false },
            ],
        }),
        "agent.v1.CommandClassifierResult.ClassifiedCommand" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.CommunicateUpdateArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.CommunicateUpdateError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.CommunicateUpdateHistoryEntry" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.CommunicateUpdateResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.CommunicateUpdateSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.CommunicateUpdateError", map_value: false },
            ],
        }),
        "agent.v1.CommunicateUpdateSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.CommunicateUpdateToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.CommunicateUpdateArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.CommunicateUpdateResult", map_value: false },
            ],
        }),
        "agent.v1.CommunicateUpdateTurnState" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.CommunicateUpdateHistoryEntry", map_value: false },
            ],
        }),
        "agent.v1.ComputerUseAction" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.MouseMoveAction", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ClickAction", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.MouseDownAction", map_value: false },
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.MouseUpAction", map_value: false },
                NestedMessageFieldDescriptor { field_number: 5, child_type: "agent.v1.DragAction", map_value: false },
                NestedMessageFieldDescriptor { field_number: 6, child_type: "agent.v1.ScrollAction", map_value: false },
                NestedMessageFieldDescriptor { field_number: 7, child_type: "agent.v1.TypeAction", map_value: false },
                NestedMessageFieldDescriptor { field_number: 8, child_type: "agent.v1.KeyAction", map_value: false },
                NestedMessageFieldDescriptor { field_number: 9, child_type: "agent.v1.WaitAction", map_value: false },
                NestedMessageFieldDescriptor { field_number: 10, child_type: "agent.v1.ScreenshotAction", map_value: false },
                NestedMessageFieldDescriptor { field_number: 11, child_type: "agent.v1.CursorPositionAction", map_value: false },
            ],
        }),
        "agent.v1.ComputerUseArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ComputerUseAction", map_value: false },
            ],
        }),
        "agent.v1.ComputerUseError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ComputerUseResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ComputerUseSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ComputerUseError", map_value: false },
            ],
        }),
        "agent.v1.ComputerUseSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 6, child_type: "agent.v1.Coordinate", map_value: false },
            ],
        }),
        "agent.v1.ComputerUseToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ComputerUseArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ComputerUseResult", map_value: false },
            ],
        }),
        "agent.v1.ConnectScmArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ConnectScmGithub", map_value: false },
            ],
        }),
        "agent.v1.ConnectScmError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ConnectScmGithub" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ConnectScmGithubRepository", map_value: false },
            ],
        }),
        "agent.v1.ConnectScmGithubRepository" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ConnectScmRejected" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ConnectScmResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ConnectScmSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ConnectScmError", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.ConnectScmRejected", map_value: false },
            ],
        }),
        "agent.v1.ConnectScmSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ConnectScmToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ConnectScmArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ConnectScmResult", map_value: false },
            ],
        }),
        "agent.v1.ConversationPlan" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ConversationSearchArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ConversationSearchError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ConversationSearchHit" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ConversationSearchResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ConversationSearchSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ConversationSearchError", map_value: false },
            ],
        }),
        "agent.v1.ConversationSearchSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ConversationSearchHit", map_value: false },
            ],
        }),
        "agent.v1.ConversationStateStructure" => Some(MessageDescriptor {
            blob_fields: &[
                BlobReferenceFieldDescriptor { field_number: 1, blob_reference_type: "json", map_value: false, skip: false },
                BlobReferenceFieldDescriptor { field_number: 3, blob_reference_type: "agent.v1.TodoItem", map_value: false, skip: false },
                BlobReferenceFieldDescriptor { field_number: 6, blob_reference_type: "agent.v1.ConversationSummary", map_value: false, skip: false },
                BlobReferenceFieldDescriptor { field_number: 7, blob_reference_type: "agent.v1.ConversationPlan", map_value: false, skip: false },
                BlobReferenceFieldDescriptor { field_number: 8, blob_reference_type: "agent.v1.ConversationTurnStructure", map_value: false, skip: false },
                BlobReferenceFieldDescriptor { field_number: 11, blob_reference_type: "agent.v1.ConversationSummaryArchive", map_value: false, skip: true },
                BlobReferenceFieldDescriptor { field_number: 12, blob_reference_type: "agent.v1.FileState", map_value: true, skip: false },
                BlobReferenceFieldDescriptor { field_number: 13, blob_reference_type: "agent.v1.ConversationSummaryArchive", map_value: false, skip: true },
                BlobReferenceFieldDescriptor { field_number: 31, blob_reference_type: "agent.v1.SubagentPersistedState", map_value: true, skip: false },
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 5, child_type: "agent.v1.ConversationTokenDetails", map_value: false },
                NestedMessageFieldDescriptor { field_number: 15, child_type: "agent.v1.FileStateStructure", map_value: true },
                NestedMessageFieldDescriptor { field_number: 14, child_type: "agent.v1.StepTiming", map_value: false },
                NestedMessageFieldDescriptor { field_number: 16, child_type: "agent.v1.SubagentPersistedState", map_value: true },
                NestedMessageFieldDescriptor { field_number: 20, child_type: "agent.v1.PlanRegistryEntry", map_value: true },
                NestedMessageFieldDescriptor { field_number: 21, child_type: "agent.v1.TrackedGitRepo", map_value: false },
                NestedMessageFieldDescriptor { field_number: 23, child_type: "agent.v1.CommunicateUpdateHistoryEntry", map_value: false },
                NestedMessageFieldDescriptor { field_number: 29, child_type: "agent.v1.CommunicateUpdateTurnState", map_value: true },
                NestedMessageFieldDescriptor { field_number: 30, child_type: "agent.v1.SubagentRunState", map_value: true },
                NestedMessageFieldDescriptor { field_number: 32, child_type: "agent.v1.GoalState", map_value: false },
            ],
        }),
        "agent.v1.ConversationStep" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.AssistantMessage", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.ThinkingMessage", map_value: false },
            ],
        }),
        "agent.v1.ConversationSummary" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ConversationSummaryArchive" => Some(MessageDescriptor {
            blob_fields: &[
                BlobReferenceFieldDescriptor { field_number: 1, blob_reference_type: "json", map_value: false, skip: false },
                BlobReferenceFieldDescriptor { field_number: 4, blob_reference_type: "json", map_value: false, skip: false },
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ConversationTokenDetails" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.PromptTokenBreakdownSnapshot", map_value: false },
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.PromptContextUsageTree", map_value: false },
            ],
        }),
        "agent.v1.ConversationTurnStructure" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.AgentConversationTurnStructure", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ShellConversationTurnStructure", map_value: false },
            ],
        }),
        "agent.v1.Coordinate" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.CreateAgentArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.CreateAgentError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.CreateAgentResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.CreateAgentSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.CreateAgentError", map_value: false },
            ],
        }),
        "agent.v1.CreateAgentSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.CreateAgentToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.CreateAgentArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.CreateAgentResult", map_value: false },
            ],
        }),
        "agent.v1.CreateGoalArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.CreateGoalResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.CreateGoalSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.GoalError", map_value: false },
            ],
        }),
        "agent.v1.CreateGoalSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.CreateGoalToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.CreateGoalArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.CreateGoalResult", map_value: false },
            ],
        }),
        "agent.v1.CreatePlanArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.TodoItem", map_value: false },
                NestedMessageFieldDescriptor { field_number: 6, child_type: "agent.v1.Phase", map_value: false },
            ],
        }),
        "agent.v1.CreatePlanError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.CreatePlanResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.CreatePlanSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.CreatePlanError", map_value: false },
            ],
        }),
        "agent.v1.CreatePlanSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.CreatePlanToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.CreatePlanArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.CreatePlanResult", map_value: false },
            ],
        }),
        "agent.v1.CreatePrAction" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.CursorPositionAction" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.CursorRule" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.CursorRuleType", map_value: false },
            ],
        }),
        "agent.v1.CursorRuleType" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.CursorRuleTypeGlobal", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.CursorRuleTypeFileGlobs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.CursorRuleTypeAgentFetched", map_value: false },
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.CursorRuleTypeManuallyAttached", map_value: false },
            ],
        }),
        "agent.v1.CursorRuleTypeAgentFetched" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.CursorRuleTypeFileGlobs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.CursorRuleTypeGlobal" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.CursorRuleTypeManuallyAttached" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.CustomModeExitIntent" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.SubmittedExitedCustomMode", map_value: false },
            ],
        }),
        "agent.v1.CustomModeIntent" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.SubmittedCustomMode", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.CustomModeExitIntent", map_value: false },
            ],
        }),
        "agent.v1.CustomSubagent" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.DeleteArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.DeleteError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.DeleteFileBusy" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.DeleteFileNotFound" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.DeleteNotFile" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.DeletePermissionDenied" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.DeleteRejected" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.DeleteResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.DeleteSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.DeleteFileNotFound", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.DeleteNotFile", map_value: false },
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.DeletePermissionDenied", map_value: false },
                NestedMessageFieldDescriptor { field_number: 5, child_type: "agent.v1.DeleteFileBusy", map_value: false },
                NestedMessageFieldDescriptor { field_number: 6, child_type: "agent.v1.DeleteRejected", map_value: false },
                NestedMessageFieldDescriptor { field_number: 7, child_type: "agent.v1.DeleteError", map_value: false },
            ],
        }),
        "agent.v1.DeleteSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.DeleteToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.DeleteArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.DeleteResult", map_value: false },
            ],
        }),
        "agent.v1.DiagnosticItem" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.DiagnosticRange", map_value: false },
            ],
        }),
        "agent.v1.DiagnosticRange" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.Position", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.Position", map_value: false },
            ],
        }),
        "agent.v1.DragAction" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.Coordinate", map_value: false },
            ],
        }),
        "agent.v1.EditArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.EditError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.EditFileNotFound" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.EditPrLabelsArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.EditPrLabelsError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.EditPrLabelsResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.EditPrLabelsSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.EditPrLabelsError", map_value: false },
            ],
        }),
        "agent.v1.EditPrLabelsSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.EditPrLabelsToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.EditPrLabelsArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.EditPrLabelsResult", map_value: false },
            ],
        }),
        "agent.v1.EditReadPermissionDenied" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.EditRejected" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.EditResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.EditSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.EditFileNotFound", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.EditReadPermissionDenied", map_value: false },
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.EditWritePermissionDenied", map_value: false },
                NestedMessageFieldDescriptor { field_number: 6, child_type: "agent.v1.EditRejected", map_value: false },
                NestedMessageFieldDescriptor { field_number: 7, child_type: "agent.v1.EditError", map_value: false },
            ],
        }),
        "agent.v1.EditSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.EditToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.EditArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.EditResult", map_value: false },
            ],
        }),
        "agent.v1.EditWritePermissionDenied" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ExecutePlanInfo" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ExtraContextEntry" => Some(MessageDescriptor {
            blob_fields: &[
                BlobReferenceFieldDescriptor { field_number: 2, blob_reference_type: "string", map_value: false, skip: false },
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.FetchArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.FetchCloudAgentDataArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.FetchCloudAgentDataError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.FetchCloudAgentDataResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.FetchCloudAgentDataSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.FetchCloudAgentDataError", map_value: false },
            ],
        }),
        "agent.v1.FetchCloudAgentDataSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.FetchCloudAgentDataToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.FetchCloudAgentDataArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.FetchCloudAgentDataResult", map_value: false },
            ],
        }),
        "agent.v1.FetchError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.FetchResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.FetchSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.FetchError", map_value: false },
            ],
        }),
        "agent.v1.FetchSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.FetchToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.FetchArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.FetchResult", map_value: false },
            ],
        }),
        "agent.v1.FileDiagnostics" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.DiagnosticItem", map_value: false },
            ],
        }),
        "agent.v1.FileState" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.FileStateStructure" => Some(MessageDescriptor {
            blob_fields: &[
                BlobReferenceFieldDescriptor { field_number: 1, blob_reference_type: "string", map_value: false, skip: false },
                BlobReferenceFieldDescriptor { field_number: 2, blob_reference_type: "string", map_value: false, skip: false },
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.GenerateImageArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.GenerateImageError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.GenerateImageResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.GenerateImageSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.GenerateImageError", map_value: false },
            ],
        }),
        "agent.v1.GenerateImageSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.GenerateImageToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.GenerateImageArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.GenerateImageResult", map_value: false },
            ],
        }),
        "agent.v1.GetAgentStatusArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.GetAgentStatusError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.GetAgentStatusResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.GetAgentStatusSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.GetAgentStatusError", map_value: false },
            ],
        }),
        "agent.v1.GetAgentStatusSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.GetAgentStatusWorker", map_value: false },
            ],
        }),
        "agent.v1.GetAgentStatusToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.GetAgentStatusArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.GetAgentStatusResult", map_value: false },
            ],
        }),
        "agent.v1.GetAgentStatusWorker" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.GetCiStatusAction" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.GetMcpToolsAgentResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.GetMcpToolsSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.GetMcpToolsError", map_value: false },
            ],
        }),
        "agent.v1.GetMcpToolsArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.GetMcpToolsError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.GetMcpToolsSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.GetMcpToolsToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.GetMcpToolsArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.GetMcpToolsAgentResult", map_value: false },
            ],
        }),
        "agent.v1.GlobToolArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.GlobToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.GlobToolArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.GlobToolResult", map_value: false },
            ],
        }),
        "agent.v1.GlobToolError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.GlobToolResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.GlobToolSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.GlobToolError", map_value: false },
            ],
        }),
        "agent.v1.GlobToolSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.GoalError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.GoalState" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.GrepArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 15, child_type: "agent.v1.SandboxPolicy", map_value: false },
            ],
        }),
        "agent.v1.GrepContentMatch" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.GrepContentResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.GrepFileMatch", map_value: false },
            ],
        }),
        "agent.v1.GrepCountResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.GrepFileCount", map_value: false },
            ],
        }),
        "agent.v1.GrepError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.GrepFileCount" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.GrepFileMatch" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.GrepContentMatch", map_value: false },
            ],
        }),
        "agent.v1.GrepFilesResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.GrepResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.GrepSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.GrepError", map_value: false },
            ],
        }),
        "agent.v1.GrepSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.GrepUnionResult", map_value: true },
                NestedMessageFieldDescriptor { field_number: 5, child_type: "agent.v1.GrepUnionResult", map_value: false },
            ],
        }),
        "agent.v1.GrepToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.GrepArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.GrepResult", map_value: false },
            ],
        }),
        "agent.v1.GrepUnionResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.GrepCountResult", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.GrepFilesResult", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.GrepContentResult", map_value: false },
            ],
        }),
        "agent.v1.HookAdditionalContext" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.InvocationContext" => Some(MessageDescriptor {
            blob_fields: &[
                BlobReferenceFieldDescriptor { field_number: 10, blob_reference_type: "agent.v1.InvocationContext", map_value: false, skip: false },
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.InvocationContext.SlackThread", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.InvocationContext.GithubPR", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.InvocationContext.IdeState", map_value: false },
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.InvocationContext.MicrosoftTeamsThread", map_value: false },
            ],
        }),
        "agent.v1.InvocationContext.GithubPR" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.InvocationContext.IdeState" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.InvocationContext.IdeState.File", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.InvocationContext.IdeState.File", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.InvocationContext.IdeState.ViewedPullRequest", map_value: false },
            ],
        }),
        "agent.v1.InvocationContext.IdeState.File" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.InvocationContext.IdeState.File.CursorPosition", map_value: false },
            ],
        }),
        "agent.v1.InvocationContext.IdeState.File.CursorPosition" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.InvocationContext.IdeState.ViewedPullRequest" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.InvocationContext.MicrosoftTeamsThread" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.InvocationContext.SlackThread" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.KeyAction" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ListMcpResourcesError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ListMcpResourcesExecArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ListMcpResourcesExecResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ListMcpResourcesSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ListMcpResourcesError", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.ListMcpResourcesRejected", map_value: false },
            ],
        }),
        "agent.v1.ListMcpResourcesExecResult.McpResource" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ListMcpResourcesRejected" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ListMcpResourcesSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ListMcpResourcesExecResult.McpResource", map_value: false },
            ],
        }),
        "agent.v1.ListMcpResourcesToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ListMcpResourcesExecArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ListMcpResourcesExecResult", map_value: false },
            ],
        }),
        "agent.v1.LsArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.SandboxPolicy", map_value: false },
            ],
        }),
        "agent.v1.LsDirectoryTreeNode" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.LsDirectoryTreeNode", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.LsDirectoryTreeNode.File", map_value: false },
            ],
        }),
        "agent.v1.LsDirectoryTreeNode.File" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.TerminalMetadata", map_value: false },
            ],
        }),
        "agent.v1.LsError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.LsRejected" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.LsResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.LsSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.LsError", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.LsRejected", map_value: false },
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.LsTimeout", map_value: false },
            ],
        }),
        "agent.v1.LsSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.LsDirectoryTreeNode", map_value: false },
            ],
        }),
        "agent.v1.LsTimeout" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.LsDirectoryTreeNode", map_value: false },
            ],
        }),
        "agent.v1.LsToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.LsArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.LsResult", map_value: false },
            ],
        }),
        "agent.v1.McpArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 6, child_type: "agent.v1.SmartModeApproval", map_value: false },
            ],
        }),
        "agent.v1.McpAuthArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.McpAuthError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.McpAuthRejected" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.McpAuthResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.McpAuthSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.McpAuthError", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.McpAuthRejected", map_value: false },
            ],
        }),
        "agent.v1.McpAuthSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.McpAuthToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.McpAuthArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.McpAuthResult", map_value: false },
            ],
        }),
        "agent.v1.McpDescriptor" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 5, child_type: "agent.v1.McpToolDescriptor", map_value: false },
            ],
        }),
        "agent.v1.McpFileSystemOptions" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.McpDescriptor", map_value: false },
            ],
        }),
        "agent.v1.McpImageContent" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.McpInstructions" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.McpMetaToolOptions" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.McpDescriptor", map_value: false },
            ],
        }),
        "agent.v1.McpPermissionDenied" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.McpRejected" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.McpSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.McpToolResultContentItem", map_value: false },
            ],
        }),
        "agent.v1.McpTextContent" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.OutputLocation", map_value: false },
            ],
        }),
        "agent.v1.McpToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.McpArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.McpToolResult", map_value: false },
            ],
        }),
        "agent.v1.McpToolDefinition" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.McpToolDescriptor" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.McpToolError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.McpToolResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.McpSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.McpToolError", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.McpRejected", map_value: false },
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.McpPermissionDenied", map_value: false },
            ],
        }),
        "agent.v1.McpToolResultContentItem" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.McpTextContent", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.McpImageContent", map_value: false },
            ],
        }),
        "agent.v1.MouseDownAction" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.MouseMoveAction" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.Coordinate", map_value: false },
            ],
        }),
        "agent.v1.MouseUpAction" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.NetworkPolicy" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 5, child_type: "agent.v1.NetworkPolicyLoggingConfig", map_value: false },
            ],
        }),
        "agent.v1.NetworkPolicyLoggingConfig" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.NewCloudVmTarget" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.OutputLocation" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.Phase" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.TodoItem", map_value: false },
            ],
        }),
        "agent.v1.PiBashToolArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PiBashToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.PiBashToolArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.PiBashToolResult", map_value: false },
            ],
        }),
        "agent.v1.PiBashToolError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.PiTruncation", map_value: false },
            ],
        }),
        "agent.v1.PiBashToolResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.PiBashToolSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.PiBashToolError", map_value: false },
            ],
        }),
        "agent.v1.PiBashToolSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.PiTruncation", map_value: false },
            ],
        }),
        "agent.v1.PiEditReplacement" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PiEditToolArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.PiEditReplacement", map_value: false },
            ],
        }),
        "agent.v1.PiEditToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.PiEditToolArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.PiEditToolResult", map_value: false },
            ],
        }),
        "agent.v1.PiEditToolError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PiEditToolRejected" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PiEditToolResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.PiEditToolSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.PiEditToolError", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.PiEditToolRejected", map_value: false },
            ],
        }),
        "agent.v1.PiEditToolSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PiFindToolArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PiFindToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.PiFindToolArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.PiFindToolResult", map_value: false },
            ],
        }),
        "agent.v1.PiFindToolError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PiFindToolResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.PiFindToolSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.PiFindToolError", map_value: false },
            ],
        }),
        "agent.v1.PiFindToolSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.PiTruncation", map_value: false },
            ],
        }),
        "agent.v1.PiGrepToolArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PiGrepToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.PiGrepToolArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.PiGrepToolResult", map_value: false },
            ],
        }),
        "agent.v1.PiGrepToolError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PiGrepToolResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.PiGrepToolSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.PiGrepToolError", map_value: false },
            ],
        }),
        "agent.v1.PiGrepToolSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.PiTruncation", map_value: false },
            ],
        }),
        "agent.v1.PiLsToolArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PiLsToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.PiLsToolArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.PiLsToolResult", map_value: false },
            ],
        }),
        "agent.v1.PiLsToolError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PiLsToolResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.PiLsToolSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.PiLsToolError", map_value: false },
            ],
        }),
        "agent.v1.PiLsToolSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.PiTruncation", map_value: false },
            ],
        }),
        "agent.v1.PiReadToolArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PiReadToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.PiReadToolArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.PiReadToolResult", map_value: false },
            ],
        }),
        "agent.v1.PiReadToolError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PiReadToolResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.PiReadToolSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.PiReadToolError", map_value: false },
            ],
        }),
        "agent.v1.PiReadToolSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.PiTruncation", map_value: false },
            ],
        }),
        "agent.v1.PiTruncation" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PiWriteToolArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PiWriteToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.PiWriteToolArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.PiWriteToolResult", map_value: false },
            ],
        }),
        "agent.v1.PiWriteToolError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PiWriteToolRejected" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PiWriteToolResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.PiWriteToolSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.PiWriteToolError", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.PiWriteToolRejected", map_value: false },
            ],
        }),
        "agent.v1.PiWriteToolSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PlanRegistryEntry" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.Position" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PostCommentAction" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PrManagementArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.CreatePrAction", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.UpdatePrAction", map_value: false },
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.PostCommentAction", map_value: false },
                NestedMessageFieldDescriptor { field_number: 5, child_type: "agent.v1.ResolveCommentAction", map_value: false },
                NestedMessageFieldDescriptor { field_number: 6, child_type: "agent.v1.GetCiStatusAction", map_value: false },
                NestedMessageFieldDescriptor { field_number: 7, child_type: "agent.v1.SetPrStatusAction", map_value: false },
            ],
        }),
        "agent.v1.PrManagementError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PrManagementNeedsConfirmation" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PrManagementRegistered" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PrManagementRejected" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PrManagementResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.PrManagementSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.PrManagementError", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.PrManagementRejected", map_value: false },
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.PrManagementRegistered", map_value: false },
                NestedMessageFieldDescriptor { field_number: 5, child_type: "agent.v1.PrManagementNeedsConfirmation", map_value: false },
            ],
        }),
        "agent.v1.PrManagementSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PrManagementToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.PrManagementArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.PrManagementResult", map_value: false },
            ],
        }),
        "agent.v1.ProjectDetails" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ProjectSubagentDetails", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.ProjectSideChatDetails", map_value: false },
            ],
        }),
        "agent.v1.ProjectSideChatDetails" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ProjectSubagentDetails" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PromptContextNode" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 11, child_type: "agent.v1.PromptContextSourceRef", map_value: false },
            ],
        }),
        "agent.v1.PromptContextSourceRef" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PromptContextUsageTree" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.PromptContextNode", map_value: false },
            ],
        }),
        "agent.v1.PromptTokenBreakdownCategory" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.PromptTokenBreakdownSnapshot" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.PromptTokenBreakdownCategory", map_value: false },
            ],
        }),
        "agent.v1.PromptUploadRef" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.Range" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.Position", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.Position", map_value: false },
            ],
        }),
        "agent.v1.ReadAgentTranscriptArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ReadAgentTranscriptError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ReadAgentTranscriptResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ReadAgentTranscriptSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ReadAgentTranscriptError", map_value: false },
            ],
        }),
        "agent.v1.ReadAgentTranscriptSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ReadAgentTranscriptToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ReadAgentTranscriptArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ReadAgentTranscriptResult", map_value: false },
            ],
        }),
        "agent.v1.ReadLintsToolArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ReadLintsToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ReadLintsToolArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ReadLintsToolResult", map_value: false },
            ],
        }),
        "agent.v1.ReadLintsToolError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ReadLintsToolResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ReadLintsToolSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ReadLintsToolError", map_value: false },
            ],
        }),
        "agent.v1.ReadLintsToolSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.FileDiagnostics", map_value: false },
            ],
        }),
        "agent.v1.ReadMcpResourceError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ReadMcpResourceExecArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 5, child_type: "agent.v1.SmartModeApproval", map_value: false },
            ],
        }),
        "agent.v1.ReadMcpResourceExecResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ReadMcpResourceSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ReadMcpResourceError", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.ReadMcpResourceRejected", map_value: false },
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.ReadMcpResourceNotFound", map_value: false },
            ],
        }),
        "agent.v1.ReadMcpResourceNotFound" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ReadMcpResourceRejected" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ReadMcpResourceSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 9, child_type: "agent.v1.OutputLocation", map_value: false },
            ],
        }),
        "agent.v1.ReadMcpResourceToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ReadMcpResourceExecArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ReadMcpResourceExecResult", map_value: false },
            ],
        }),
        "agent.v1.ReadRange" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ReadTodosArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ReadTodosError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ReadTodosResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ReadTodosSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ReadTodosError", map_value: false },
            ],
        }),
        "agent.v1.ReadTodosSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.TodoItem", map_value: false },
            ],
        }),
        "agent.v1.ReadTodosToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ReadTodosArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ReadTodosResult", map_value: false },
            ],
        }),
        "agent.v1.ReadToolArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ReadToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ReadToolArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ReadToolResult", map_value: false },
            ],
        }),
        "agent.v1.ReadToolError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ReadToolResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ReadToolSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ReadToolError", map_value: false },
            ],
        }),
        "agent.v1.ReadToolSuccess" => Some(MessageDescriptor {
            blob_fields: &[
                BlobReferenceFieldDescriptor { field_number: 9, blob_reference_type: "bytes", map_value: false, skip: false },
                BlobReferenceFieldDescriptor { field_number: 10, blob_reference_type: "string", map_value: false, skip: false },
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 8, child_type: "agent.v1.ReadRange", map_value: false },
                NestedMessageFieldDescriptor { field_number: 13, child_type: "agent.v1.CursorRule", map_value: false },
            ],
        }),
        "agent.v1.RecentAgent" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.RecentAgentsContext" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.RecentAgent", map_value: false },
            ],
        }),
        "agent.v1.RecordCiInvestigationFinding" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.RecordCiInvestigationFindingsArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.RecordCiInvestigationFinding", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.RecordCiInvestigationOverall", map_value: false },
            ],
        }),
        "agent.v1.RecordCiInvestigationFindingsError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.RecordCiInvestigationFindingsResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.RecordCiInvestigationFindingsSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.RecordCiInvestigationFindingsError", map_value: false },
            ],
        }),
        "agent.v1.RecordCiInvestigationFindingsSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.RecordCiInvestigationFindingsToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.RecordCiInvestigationFindingsArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.RecordCiInvestigationFindingsResult", map_value: false },
            ],
        }),
        "agent.v1.RecordCiInvestigationOverall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.RecordScreenArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.RecordScreenDiscardSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.RecordScreenFailure" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.RecordScreenResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.RecordScreenStartSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.RecordScreenSaveSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.RecordScreenDiscardSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.RecordScreenFailure", map_value: false },
            ],
        }),
        "agent.v1.RecordScreenSaveSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.RecordScreenStartSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.RecordScreenToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.RecordScreenArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.RecordScreenResult", map_value: false },
            ],
        }),
        "agent.v1.ReflectArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ReflectError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ReflectResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ReflectSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ReflectError", map_value: false },
            ],
        }),
        "agent.v1.ReflectSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ReflectToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ReflectArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ReflectResult", map_value: false },
            ],
        }),
        "agent.v1.ReplaceEnvArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ReplaceEnvConfig", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.RepoCheckoutRefOverride", map_value: false },
            ],
        }),
        "agent.v1.ReplaceEnvConfig" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ReplaceEnvFailure" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ReplaceEnvResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ReplaceEnvSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ReplaceEnvFailure", map_value: false },
            ],
        }),
        "agent.v1.ReplaceEnvSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ReplaceEnvToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ReplaceEnvArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ReplaceEnvResult", map_value: false },
            ],
        }),
        "agent.v1.RepoCheckoutRefOverride" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ReportBugArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ReportBugError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ReportBugResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ReportBugSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ReportBugError", map_value: false },
            ],
        }),
        "agent.v1.ReportBugSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ReportBugToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ReportBugArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ReportBugResult", map_value: false },
            ],
        }),
        "agent.v1.ReportBugfixResultsArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.BugfixResultItem", map_value: false },
            ],
        }),
        "agent.v1.ReportBugfixResultsError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ReportBugfixResultsResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ReportBugfixResultsSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ReportBugfixResultsError", map_value: false },
            ],
        }),
        "agent.v1.ReportBugfixResultsSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.BugfixResultItem", map_value: false },
            ],
        }),
        "agent.v1.ReportBugfixResultsToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ReportBugfixResultsArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ReportBugfixResultsResult", map_value: false },
            ],
        }),
        "agent.v1.RequestContextMcpsPart" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.McpToolDefinition", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.McpInstructions", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.McpFileSystemOptions", map_value: false },
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.McpMetaToolOptions", map_value: false },
            ],
        }),
        "agent.v1.RequestContextRulesPart" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.CursorRule", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.CursorRule", map_value: false },
            ],
        }),
        "agent.v1.RequestContextSkillsPart" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.AgentSkill", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.SkillOptions", map_value: false },
            ],
        }),
        "agent.v1.RequestContextSubagentsPart" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.CustomSubagent", map_value: false },
            ],
        }),
        "agent.v1.ResolveCommentAction" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SameMachineTarget" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SandboxPolicy" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 10, child_type: "agent.v1.NetworkPolicy", map_value: false },
            ],
        }),
        "agent.v1.ScreenshotAction" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ScrollAction" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.Coordinate", map_value: false },
            ],
        }),
        "agent.v1.SearchConversationsToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ConversationSearchArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ConversationSearchResult", map_value: false },
            ],
        }),
        "agent.v1.SelectedAgenticGitAction" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.SelectedAgenticGitActionCommitParams", map_value: false },
                NestedMessageFieldDescriptor { field_number: 6, child_type: "agent.v1.SelectedAgenticGitActionCommitParams", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.SelectedAgenticGitActionPushParams", map_value: false },
                NestedMessageFieldDescriptor { field_number: 7, child_type: "agent.v1.SelectedAgenticGitActionPushParams", map_value: false },
                NestedMessageFieldDescriptor { field_number: 8, child_type: "agent.v1.SelectedAgenticGitActionCommitParams", map_value: false },
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.SelectedAgenticGitActionFixMergeConflictsParams", map_value: false },
                NestedMessageFieldDescriptor { field_number: 11, child_type: "agent.v1.SelectedAgenticGitActionBabysitPrInCloudParams", map_value: false },
                NestedMessageFieldDescriptor { field_number: 14, child_type: "agent.v1.SelectedAgenticGitActionPullLocallyParams", map_value: false },
                NestedMessageFieldDescriptor { field_number: 15, child_type: "agent.v1.SelectedAgenticGitActionPullLocallyParams", map_value: false },
                NestedMessageFieldDescriptor { field_number: 12, child_type: "agent.v1.SelectedAgenticGitActionCommitParams", map_value: false },
                NestedMessageFieldDescriptor { field_number: 13, child_type: "agent.v1.SelectedAgenticGitActionCommitParams", map_value: false },
                NestedMessageFieldDescriptor { field_number: 16, child_type: "agent.v1.SelectedAgenticGitActionUpdateBranchParams", map_value: false },
                NestedMessageFieldDescriptor { field_number: 17, child_type: "agent.v1.SelectedAgenticGitActionCreateBranchParams", map_value: false },
                NestedMessageFieldDescriptor { field_number: 5, child_type: "agent.v1.SelectedGitBranchContext", map_value: false },
            ],
        }),
        "agent.v1.SelectedAgenticGitActionBabysitPrInCloudParams" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelectedAgenticGitActionCommitParams" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 5, child_type: "agent.v1.SelectedAgenticGitFileWithStatus", map_value: false },
                NestedMessageFieldDescriptor { field_number: 6, child_type: "agent.v1.SelectedAgenticGitFileWithStatus", map_value: false },
            ],
        }),
        "agent.v1.SelectedAgenticGitActionCreateBranchParams" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelectedAgenticGitActionFixMergeConflictsParams" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelectedAgenticGitActionPullLocallyParams" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelectedAgenticGitActionPushParams" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelectedAgenticGitActionUpdateBranchParams" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelectedAgenticGitFileWithStatus" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelectedBrowser" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelectedCodeSelection" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.Range", map_value: false },
            ],
        }),
        "agent.v1.SelectedConsoleLog" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 6, child_type: "agent.v1.StackTrace", map_value: false },
            ],
        }),
        "agent.v1.SelectedContext" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.SelectedImage", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.InvocationContext", map_value: false },
                NestedMessageFieldDescriptor { field_number: 16, child_type: "agent.v1.ExtraContextEntry", map_value: false },
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.SelectedFile", map_value: false },
                NestedMessageFieldDescriptor { field_number: 5, child_type: "agent.v1.SelectedCodeSelection", map_value: false },
                NestedMessageFieldDescriptor { field_number: 6, child_type: "agent.v1.SelectedTerminal", map_value: false },
                NestedMessageFieldDescriptor { field_number: 7, child_type: "agent.v1.SelectedTerminalSelection", map_value: false },
                NestedMessageFieldDescriptor { field_number: 8, child_type: "agent.v1.SelectedFolder", map_value: false },
                NestedMessageFieldDescriptor { field_number: 9, child_type: "agent.v1.SelectedExternalLink", map_value: false },
                NestedMessageFieldDescriptor { field_number: 10, child_type: "agent.v1.SelectedCursorRule", map_value: false },
                NestedMessageFieldDescriptor { field_number: 18, child_type: "agent.v1.SelectedGitDiff", map_value: false },
                NestedMessageFieldDescriptor { field_number: 11, child_type: "agent.v1.SelectedGitDiffFromBranchToMain", map_value: false },
                NestedMessageFieldDescriptor { field_number: 12, child_type: "agent.v1.SelectedCursorCommand", map_value: false },
                NestedMessageFieldDescriptor { field_number: 13, child_type: "agent.v1.SelectedDocumentation", map_value: false },
                NestedMessageFieldDescriptor { field_number: 14, child_type: "agent.v1.SelectedUIElement", map_value: false },
                NestedMessageFieldDescriptor { field_number: 15, child_type: "agent.v1.SelectedConsoleLog", map_value: false },
                NestedMessageFieldDescriptor { field_number: 17, child_type: "agent.v1.SelectedGitCommit", map_value: false },
                NestedMessageFieldDescriptor { field_number: 19, child_type: "agent.v1.SelectedPastChat", map_value: false },
                NestedMessageFieldDescriptor { field_number: 20, child_type: "agent.v1.SelectedGitPRDiffSelection", map_value: false },
                NestedMessageFieldDescriptor { field_number: 21, child_type: "agent.v1.SelectedPullRequest", map_value: false },
                NestedMessageFieldDescriptor { field_number: 22, child_type: "agent.v1.SelectedSubagent", map_value: false },
                NestedMessageFieldDescriptor { field_number: 23, child_type: "agent.v1.SelectedVideo", map_value: false },
                NestedMessageFieldDescriptor { field_number: 24, child_type: "agent.v1.SelectedBrowser", map_value: false },
                NestedMessageFieldDescriptor { field_number: 25, child_type: "agent.v1.SelectedDocument", map_value: false },
                NestedMessageFieldDescriptor { field_number: 26, child_type: "agent.v1.AgentSkill", map_value: false },
                NestedMessageFieldDescriptor { field_number: 27, child_type: "agent.v1.RecentAgentsContext", map_value: false },
                NestedMessageFieldDescriptor { field_number: 34, child_type: "agent.v1.SelectedAgenticGitAction", map_value: false },
            ],
        }),
        "agent.v1.SelectedCursorCommand" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.SelectedPluginCapabilityRef", map_value: false },
            ],
        }),
        "agent.v1.SelectedCursorRule" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.CursorRule", map_value: false },
            ],
        }),
        "agent.v1.SelectedDocument" => Some(MessageDescriptor {
            blob_fields: &[
                BlobReferenceFieldDescriptor { field_number: 1, blob_reference_type: "bytes", map_value: false, skip: false },
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 9, child_type: "agent.v1.SelectedDocument.BlobIdWithData", map_value: false },
                NestedMessageFieldDescriptor { field_number: 10, child_type: "agent.v1.PromptUploadRef", map_value: false },
            ],
        }),
        "agent.v1.SelectedDocument.BlobIdWithData" => Some(MessageDescriptor {
            blob_fields: &[
                BlobReferenceFieldDescriptor { field_number: 1, blob_reference_type: "bytes", map_value: false, skip: false },
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelectedDocumentation" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelectedExternalLink" => Some(MessageDescriptor {
            blob_fields: &[
                BlobReferenceFieldDescriptor { field_number: 6, blob_reference_type: "string", map_value: false, skip: false },
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelectedFile" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelectedFolder" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.LsDirectoryTreeNode", map_value: false },
            ],
        }),
        "agent.v1.SelectedGitBranchContext" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelectedGitCommit" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelectedGitDiff" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelectedGitDiffFromBranchToMain" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelectedGitPRDiffSelection" => Some(MessageDescriptor {
            blob_fields: &[
                BlobReferenceFieldDescriptor { field_number: 6, blob_reference_type: "agent.v1.SelectedGitPRDiffSelection", map_value: false, skip: false },
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelectedImage" => Some(MessageDescriptor {
            blob_fields: &[
                BlobReferenceFieldDescriptor { field_number: 1, blob_reference_type: "bytes", map_value: false, skip: false },
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 9, child_type: "agent.v1.SelectedImage.BlobIdWithData", map_value: false },
                NestedMessageFieldDescriptor { field_number: 10, child_type: "agent.v1.PromptUploadRef", map_value: false },
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.SelectedImage.Dimension", map_value: false },
            ],
        }),
        "agent.v1.SelectedImage.BlobIdWithData" => Some(MessageDescriptor {
            blob_fields: &[
                BlobReferenceFieldDescriptor { field_number: 1, blob_reference_type: "bytes", map_value: false, skip: false },
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelectedImage.Dimension" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelectedPastChat" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelectedPluginCapabilityRef" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelectedPullRequest" => Some(MessageDescriptor {
            blob_fields: &[
                BlobReferenceFieldDescriptor { field_number: 7, blob_reference_type: "agent.v1.SelectedPullRequest", map_value: false, skip: false },
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelectedSubagent" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelectedTerminal" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelectedTerminalSelection" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.Range", map_value: false },
            ],
        }),
        "agent.v1.SelectedUIElement" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelectedVideo" => Some(MessageDescriptor {
            blob_fields: &[
                BlobReferenceFieldDescriptor { field_number: 1, blob_reference_type: "bytes", map_value: false, skip: false },
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 9, child_type: "agent.v1.SelectedVideo.BlobIdWithData", map_value: false },
                NestedMessageFieldDescriptor { field_number: 11, child_type: "agent.v1.SelectedVideo.SignedUrl", map_value: false },
            ],
        }),
        "agent.v1.SelectedVideo.BlobIdWithData" => Some(MessageDescriptor {
            blob_fields: &[
                BlobReferenceFieldDescriptor { field_number: 1, blob_reference_type: "bytes", map_value: false, skip: false },
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelectedVideo.SignedUrl" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelfHostedPoolTarget" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.SelfHostedWorkerLabel", map_value: false },
            ],
        }),
        "agent.v1.SelfHostedWorkerLabel" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SelfHostedWorkerTarget" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SemSearchToolArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SemSearchToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.SemSearchToolArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.SemSearchToolResult", map_value: false },
            ],
        }),
        "agent.v1.SemSearchToolError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SemSearchToolResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.SemSearchToolSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.SemSearchToolError", map_value: false },
            ],
        }),
        "agent.v1.SemSearchToolSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SendFinalSummaryArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SendFinalSummaryError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SendFinalSummaryResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.SendFinalSummarySuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.SendFinalSummaryError", map_value: false },
            ],
        }),
        "agent.v1.SendFinalSummarySuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SendFinalSummaryToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.SendFinalSummaryArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.SendFinalSummaryResult", map_value: false },
            ],
        }),
        "agent.v1.SendMessageArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.SendMessageText", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.SendMessageAttachment", map_value: false },
            ],
        }),
        "agent.v1.SendMessageAttachment" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SendMessageError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SendMessageResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.SendMessageSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.SendMessageError", map_value: false },
            ],
        }),
        "agent.v1.SendMessageSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SendMessageText" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SendMessageToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.SendMessageArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.SendMessageResult", map_value: false },
            ],
        }),
        "agent.v1.SendToAgentArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SendToAgentError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SendToAgentResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.SendToAgentSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.SendToAgentError", map_value: false },
            ],
        }),
        "agent.v1.SendToAgentSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SendToAgentToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.SendToAgentArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.SendToAgentResult", map_value: false },
            ],
        }),
        "agent.v1.SendToUserArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SendToUserError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SendToUserResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.SendToUserSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.SendToUserError", map_value: false },
            ],
        }),
        "agent.v1.SendToUserSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SendToUserToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.SendToUserArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.SendToUserResult", map_value: false },
            ],
        }),
        "agent.v1.SetActiveBranchArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SetActiveBranchError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SetActiveBranchResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.SetActiveBranchSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.SetActiveBranchError", map_value: false },
            ],
        }),
        "agent.v1.SetActiveBranchSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SetActiveBranchToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.SetActiveBranchArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.SetActiveBranchResult", map_value: false },
            ],
        }),
        "agent.v1.SetPrStatusAction" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SetupVmEnvironmentArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SetupVmEnvironmentResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.SetupVmEnvironmentSuccess", map_value: false },
            ],
        }),
        "agent.v1.SetupVmEnvironmentSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SetupVmEnvironmentToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.SetupVmEnvironmentArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.SetupVmEnvironmentResult", map_value: false },
            ],
        }),
        "agent.v1.ShellArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 8, child_type: "agent.v1.ShellCommandParsingResult", map_value: false },
                NestedMessageFieldDescriptor { field_number: 9, child_type: "agent.v1.SandboxPolicy", map_value: false },
                NestedMessageFieldDescriptor { field_number: 16, child_type: "agent.v1.CommandClassifierResult", map_value: false },
                NestedMessageFieldDescriptor { field_number: 18, child_type: "agent.v1.ShellOutputNotificationConfig", map_value: false },
                NestedMessageFieldDescriptor { field_number: 19, child_type: "agent.v1.SmartModeApproval", map_value: false },
                NestedMessageFieldDescriptor { field_number: 20, child_type: "agent.v1.ShellHookApprovalRequirement", map_value: false },
            ],
        }),
        "agent.v1.ShellCommand" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ShellCommandParsingResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ShellCommandParsingResult.ExecutableCommand", map_value: false },
                NestedMessageFieldDescriptor { field_number: 6, child_type: "agent.v1.ShellCommandParsingResult.Redirect", map_value: false },
            ],
        }),
        "agent.v1.ShellCommandParsingResult.ExecutableCommand" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ShellCommandParsingResult.ExecutableCommandArg", map_value: false },
            ],
        }),
        "agent.v1.ShellCommandParsingResult.ExecutableCommandArg" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ShellCommandParsingResult.Redirect" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ShellConversationTurnStructure" => Some(MessageDescriptor {
            blob_fields: &[
                BlobReferenceFieldDescriptor { field_number: 1, blob_reference_type: "agent.v1.ShellCommand", map_value: false, skip: false },
                BlobReferenceFieldDescriptor { field_number: 2, blob_reference_type: "agent.v1.ShellOutput", map_value: false, skip: false },
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ShellFailure" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 8, child_type: "agent.v1.OutputLocation", map_value: false },
            ],
        }),
        "agent.v1.ShellHookApprovalRequirement" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ShellOutput" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ShellOutputNotificationConfig" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ShellPermissionDenied" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ShellRejected" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ShellResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ShellSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ShellFailure", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.ShellTimeout", map_value: false },
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.ShellRejected", map_value: false },
                NestedMessageFieldDescriptor { field_number: 5, child_type: "agent.v1.ShellSpawnError", map_value: false },
                NestedMessageFieldDescriptor { field_number: 7, child_type: "agent.v1.ShellPermissionDenied", map_value: false },
                NestedMessageFieldDescriptor { field_number: 101, child_type: "agent.v1.SandboxPolicy", map_value: false },
            ],
        }),
        "agent.v1.ShellSpawnError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ShellSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 8, child_type: "agent.v1.OutputLocation", map_value: false },
            ],
        }),
        "agent.v1.ShellTimeout" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ShellToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ShellArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.ShellResult", map_value: false },
            ],
        }),
        "agent.v1.SkillDescriptor" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SkillOptions" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.SkillDescriptor", map_value: false },
            ],
        }),
        "agent.v1.SmartModeApproval" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.StackTrace" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.CallFrame", map_value: false },
            ],
        }),
        "agent.v1.StartGrindExecutionArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.StartGrindExecutionError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.StartGrindExecutionResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.StartGrindExecutionSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.StartGrindExecutionError", map_value: false },
            ],
        }),
        "agent.v1.StartGrindExecutionSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.StartGrindExecutionToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.StartGrindExecutionArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.StartGrindExecutionResult", map_value: false },
            ],
        }),
        "agent.v1.StartGrindPlanningArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.StartGrindPlanningError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.StartGrindPlanningResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.StartGrindPlanningSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.StartGrindPlanningError", map_value: false },
            ],
        }),
        "agent.v1.StartGrindPlanningSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.StartGrindPlanningToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.StartGrindPlanningArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.StartGrindPlanningResult", map_value: false },
            ],
        }),
        "agent.v1.StepTiming" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.StopAgentArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.StopAgentError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.StopAgentResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.StopAgentSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.StopAgentError", map_value: false },
            ],
        }),
        "agent.v1.StopAgentSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.StopAgentToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.StopAgentArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.StopAgentResult", map_value: false },
            ],
        }),
        "agent.v1.SubagentPersistedState" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ConversationStateStructure", map_value: false },
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.SubagentType", map_value: false },
                NestedMessageFieldDescriptor { field_number: 7, child_type: "agent.v1.CloudSubagentReference", map_value: false },
                NestedMessageFieldDescriptor { field_number: 10, child_type: "agent.v1.TargetMachine", map_value: false },
            ],
        }),
        "agent.v1.SubagentRunState" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SubagentType" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.SubagentTypeUnspecified", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.SubagentTypeComputerUse", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.SubagentTypeCustom", map_value: false },
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.SubagentTypeExplore", map_value: false },
                NestedMessageFieldDescriptor { field_number: 5, child_type: "agent.v1.SubagentTypeMediaReview", map_value: false },
                NestedMessageFieldDescriptor { field_number: 6, child_type: "agent.v1.SubagentTypeBash", map_value: false },
                NestedMessageFieldDescriptor { field_number: 7, child_type: "agent.v1.SubagentTypeBrowserUse", map_value: false },
                NestedMessageFieldDescriptor { field_number: 8, child_type: "agent.v1.SubagentTypeShell", map_value: false },
                NestedMessageFieldDescriptor { field_number: 9, child_type: "agent.v1.SubagentTypeVmSetupHelper", map_value: false },
                NestedMessageFieldDescriptor { field_number: 10, child_type: "agent.v1.SubagentTypeDebug", map_value: false },
                NestedMessageFieldDescriptor { field_number: 11, child_type: "agent.v1.SubagentTypeCursorGuide", map_value: false },
                NestedMessageFieldDescriptor { field_number: 12, child_type: "agent.v1.SubagentTypeWatchVideo", map_value: false },
            ],
        }),
        "agent.v1.SubagentTypeBash" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SubagentTypeBrowserUse" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SubagentTypeComputerUse" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SubagentTypeCursorGuide" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SubagentTypeCustom" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SubagentTypeDebug" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SubagentTypeExplore" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SubagentTypeMediaReview" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SubagentTypeShell" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SubagentTypeUnspecified" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SubagentTypeVmSetupHelper" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SubagentTypeWatchVideo" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SubmittedCustomMode" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SubmittedExitedCustomMode" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SubscriptionEventDisplay" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SwitchModeArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SwitchModeError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SwitchModeRejected" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SwitchModeResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.SwitchModeSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.SwitchModeError", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.SwitchModeRejected", map_value: false },
            ],
        }),
        "agent.v1.SwitchModeSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.SwitchModeToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.SwitchModeArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.SwitchModeResult", map_value: false },
            ],
        }),
        "agent.v1.TargetMachine" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.SameMachineTarget", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.NewCloudVmTarget", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.SelfHostedWorkerTarget", map_value: false },
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.SelfHostedPoolTarget", map_value: false },
            ],
        }),
        "agent.v1.TaskArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.SubagentType", map_value: false },
                NestedMessageFieldDescriptor { field_number: 11, child_type: "agent.v1.TargetMachine", map_value: false },
            ],
        }),
        "agent.v1.TaskError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.TaskResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.TaskSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.TaskError", map_value: false },
            ],
        }),
        "agent.v1.TaskSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ConversationStep", map_value: false },
            ],
        }),
        "agent.v1.TaskToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.TaskArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.TaskResult", map_value: false },
            ],
        }),
        "agent.v1.TerminalMetadata" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.TerminalMetadata.Command", map_value: false },
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.TerminalMetadata.Command", map_value: false },
            ],
        }),
        "agent.v1.TerminalMetadata.Command" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ThinkingMessage" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.TodoItem" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.ToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.ShellToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.DeleteToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 4, child_type: "agent.v1.GlobToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 5, child_type: "agent.v1.GrepToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 8, child_type: "agent.v1.ReadToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 9, child_type: "agent.v1.UpdateTodosToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 10, child_type: "agent.v1.ReadTodosToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 12, child_type: "agent.v1.EditToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 13, child_type: "agent.v1.LsToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 14, child_type: "agent.v1.ReadLintsToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 15, child_type: "agent.v1.McpToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 16, child_type: "agent.v1.SemSearchToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 17, child_type: "agent.v1.CreatePlanToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 18, child_type: "agent.v1.WebSearchToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 19, child_type: "agent.v1.TaskToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 20, child_type: "agent.v1.ListMcpResourcesToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 21, child_type: "agent.v1.ReadMcpResourceToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 22, child_type: "agent.v1.ApplyAgentDiffToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 23, child_type: "agent.v1.AskQuestionToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 24, child_type: "agent.v1.FetchToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 25, child_type: "agent.v1.SwitchModeToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 28, child_type: "agent.v1.GenerateImageToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 29, child_type: "agent.v1.RecordScreenToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 30, child_type: "agent.v1.ComputerUseToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 31, child_type: "agent.v1.WriteShellStdinToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 32, child_type: "agent.v1.ReflectToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 33, child_type: "agent.v1.SetupVmEnvironmentToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 34, child_type: "agent.v1.TruncatedToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 35, child_type: "agent.v1.StartGrindExecutionToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 36, child_type: "agent.v1.StartGrindPlanningToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 37, child_type: "agent.v1.WebFetchToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 38, child_type: "agent.v1.ReportBugfixResultsToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 39, child_type: "agent.v1.AiAttributionToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 40, child_type: "agent.v1.PrManagementToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 41, child_type: "agent.v1.McpAuthToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 42, child_type: "agent.v1.AwaitToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 43, child_type: "agent.v1.BlameByFilePathToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 44, child_type: "agent.v1.GetMcpToolsToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 45, child_type: "agent.v1.ReportBugToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 46, child_type: "agent.v1.SetActiveBranchToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 48, child_type: "agent.v1.CommunicateUpdateToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 49, child_type: "agent.v1.SendFinalSummaryToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 50, child_type: "agent.v1.UpdatePrCodeTourToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 51, child_type: "agent.v1.ReplaceEnvToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 52, child_type: "agent.v1.EditPrLabelsToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 53, child_type: "agent.v1.RecordCiInvestigationFindingsToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 55, child_type: "agent.v1.SendMessageToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 56, child_type: "agent.v1.FetchCloudAgentDataToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 58, child_type: "agent.v1.SendToUserToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 61, child_type: "agent.v1.PiReadToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 62, child_type: "agent.v1.PiBashToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 63, child_type: "agent.v1.PiEditToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 64, child_type: "agent.v1.PiWriteToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 65, child_type: "agent.v1.PiGrepToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 66, child_type: "agent.v1.PiFindToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 67, child_type: "agent.v1.PiLsToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 68, child_type: "agent.v1.ConnectScmToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 69, child_type: "agent.v1.SearchConversationsToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 70, child_type: "agent.v1.CreateGoalToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 71, child_type: "agent.v1.UpdateGoalToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 72, child_type: "agent.v1.AdoptToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 73, child_type: "agent.v1.GetAgentStatusToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 74, child_type: "agent.v1.SendToAgentToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 75, child_type: "agent.v1.ReadAgentTranscriptToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 76, child_type: "agent.v1.CreateAgentToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 77, child_type: "agent.v1.StopAgentToolCall", map_value: false },
                NestedMessageFieldDescriptor { field_number: 54, child_type: "agent.v1.HookAdditionalContext", map_value: false },
            ],
        }),
        "agent.v1.TrackedGitRepo" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.TriggeringUserInfo" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.TruncatedToolCall" => Some(MessageDescriptor {
            blob_fields: &[
                BlobReferenceFieldDescriptor { field_number: 1, blob_reference_type: "agent.v1.ConversationStep", map_value: false, skip: false },
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.TruncatedToolCallArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.TruncatedToolCallResult", map_value: false },
            ],
        }),
        "agent.v1.TruncatedToolCallArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.TruncatedToolCallError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.TruncatedToolCallResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.TruncatedToolCallSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.TruncatedToolCallError", map_value: false },
            ],
        }),
        "agent.v1.TruncatedToolCallSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.TypeAction" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.UpdateGoalArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.UpdateGoalResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.UpdateGoalSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.GoalError", map_value: false },
            ],
        }),
        "agent.v1.UpdateGoalSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.UpdateGoalToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.UpdateGoalArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.UpdateGoalResult", map_value: false },
            ],
        }),
        "agent.v1.UpdatePrAction" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.UpdatePrCodeTourArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.UpdatePrCodeTourError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.UpdatePrCodeTourResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.UpdatePrCodeTourSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.UpdatePrCodeTourError", map_value: false },
            ],
        }),
        "agent.v1.UpdatePrCodeTourSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.UpdatePrCodeTourToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.UpdatePrCodeTourArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.UpdatePrCodeTourResult", map_value: false },
            ],
        }),
        "agent.v1.UpdateTodosArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.TodoItem", map_value: false },
            ],
        }),
        "agent.v1.UpdateTodosError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.UpdateTodosResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.UpdateTodosSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.UpdateTodosError", map_value: false },
            ],
        }),
        "agent.v1.UpdateTodosSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.TodoItem", map_value: false },
            ],
        }),
        "agent.v1.UpdateTodosToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.UpdateTodosArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.UpdateTodosResult", map_value: false },
            ],
        }),
        "agent.v1.UserMessage" => Some(MessageDescriptor {
            blob_fields: &[
                BlobReferenceFieldDescriptor { field_number: 10, blob_reference_type: "agent.v1.ConversationStateStructure", map_value: false, skip: true },
                BlobReferenceFieldDescriptor { field_number: 18, blob_reference_type: "string", map_value: false, skip: false },
                BlobReferenceFieldDescriptor { field_number: 19, blob_reference_type: "string", map_value: false, skip: false },
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.SelectedContext", map_value: false },
                NestedMessageFieldDescriptor { field_number: 13, child_type: "agent.v1.TriggeringUserInfo", map_value: false },
                NestedMessageFieldDescriptor { field_number: 14, child_type: "agent.v1.ExecutePlanInfo", map_value: false },
                NestedMessageFieldDescriptor { field_number: 15, child_type: "agent.v1.UserMessage.SimulatedMessageMetadata", map_value: false },
                NestedMessageFieldDescriptor { field_number: 21, child_type: "agent.v1.HookAdditionalContext", map_value: false },
                NestedMessageFieldDescriptor { field_number: 22, child_type: "agent.v1.CustomModeIntent", map_value: false },
                NestedMessageFieldDescriptor { field_number: 23, child_type: "agent.v1.ProjectDetails", map_value: false },
            ],
        }),
        "agent.v1.UserMessage.SimulatedMessageMetadata" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 6, child_type: "agent.v1.SubscriptionEventDisplay", map_value: false },
            ],
        }),
        "agent.v1.WaitAction" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.WebFetchArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.WebFetchError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.WebFetchRejected" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.WebFetchResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.WebFetchSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.WebFetchError", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.WebFetchRejected", map_value: false },
            ],
        }),
        "agent.v1.WebFetchSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.OutputLocation", map_value: false },
            ],
        }),
        "agent.v1.WebFetchToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.WebFetchArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.WebFetchResult", map_value: false },
            ],
        }),
        "agent.v1.WebSearchArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.WebSearchError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.WebSearchReference" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.WebSearchRejected" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.WebSearchResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.WebSearchSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.WebSearchError", map_value: false },
                NestedMessageFieldDescriptor { field_number: 3, child_type: "agent.v1.WebSearchRejected", map_value: false },
            ],
        }),
        "agent.v1.WebSearchSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.WebSearchReference", map_value: false },
            ],
        }),
        "agent.v1.WebSearchToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.WebSearchArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.WebSearchResult", map_value: false },
            ],
        }),
        "agent.v1.WriteShellStdinArgs" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.WriteShellStdinError" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.WriteShellStdinResult" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.WriteShellStdinSuccess", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.WriteShellStdinError", map_value: false },
            ],
        }),
        "agent.v1.WriteShellStdinSuccess" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
            ],
        }),
        "agent.v1.WriteShellStdinToolCall" => Some(MessageDescriptor {
            blob_fields: &[
            ],
            nested_fields: &[
                NestedMessageFieldDescriptor { field_number: 1, child_type: "agent.v1.WriteShellStdinArgs", map_value: false },
                NestedMessageFieldDescriptor { field_number: 2, child_type: "agent.v1.WriteShellStdinResult", map_value: false },
            ],
        }),
        _ => None,
    }
}
