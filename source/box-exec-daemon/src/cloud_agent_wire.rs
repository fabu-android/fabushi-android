use std::collections::HashMap;

use prost::Message;

pub const BACKGROUND_COMPOSER_SOURCE_GROK_BOT: i32 = 33;
pub const STARTING_MESSAGE_TYPE_USER_MESSAGE: i32 = 1;
pub const AGENT_MODE_AGENT: i32 = 1;

#[derive(Clone, PartialEq, Message)]
pub struct RequestedModelParameterWire {
    #[prost(string, tag = "1")]
    pub id: String,
    #[prost(string, tag = "2")]
    pub value: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct RequestedModelWire {
    #[prost(string, tag = "1")]
    pub model_id: String,
    #[prost(bool, tag = "2")]
    pub max_mode: bool,
    #[prost(message, repeated, tag = "3")]
    pub parameters: Vec<RequestedModelParameterWire>,
}

#[derive(Clone, PartialEq, Message)]
pub struct SelectedImageWire {
    #[prost(bytes = "vec", tag = "8")]
    pub data: Vec<u8>,
    #[prost(string, tag = "2")]
    pub uuid: String,
    #[prost(string, tag = "3")]
    pub path: String,
    #[prost(string, tag = "7")]
    pub mime_type: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct SelectedContextWire {
    #[prost(message, repeated, tag = "1")]
    pub selected_images: Vec<SelectedImageWire>,
}

#[derive(Clone, PartialEq, Message)]
pub struct UserMessageWire {
    #[prost(string, tag = "1")]
    pub text: String,
    #[prost(string, tag = "2")]
    pub message_id: String,
    #[prost(message, optional, tag = "3")]
    pub selected_context: Option<SelectedContextWire>,
    #[prost(enumeration = "AgentModeWire", tag = "4")]
    pub mode: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, prost::Enumeration)]
#[repr(i32)]
pub enum AgentModeWire {
    Unspecified = 0,
    Agent = 1,
    Ask = 2,
    Plan = 3,
    Debug = 4,
    Triage = 5,
    Project = 6,
    Multitask = 7,
    Custom = 8,
}

#[derive(Clone, PartialEq, Message)]
pub struct UserMessageActionWire {
    #[prost(message, optional, tag = "1")]
    pub user_message: Option<UserMessageWire>,
    #[prost(bool, optional, tag = "3")]
    pub send_to_interaction_listener: Option<bool>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ConversationActionWire {
    #[prost(message, optional, tag = "1")]
    pub user_message_action: Option<UserMessageActionWire>,
}

#[derive(Clone, PartialEq, Message)]
pub struct EnvironmentRepoEntryWire {
    #[prost(string, tag = "1")]
    pub repo_url: String,
    #[prost(string, tag = "2")]
    pub scm_repo_node_id: String,
    #[prost(string, optional, tag = "3")]
    pub git_enterprise_uuid: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct EnvironmentRepoConfigWire {
    #[prost(message, repeated, tag = "1")]
    pub repos: Vec<EnvironmentRepoEntryWire>,
}

#[derive(Clone, PartialEq, Message)]
pub struct DevcontainerStartingPointWire {
    #[prost(string, tag = "1")]
    pub url: String,
    #[prost(string, tag = "2")]
    pub r#ref: String,
    #[prost(message, optional, tag = "11")]
    pub repo_config: Option<EnvironmentRepoConfigWire>,
    #[prost(string, optional, tag = "12")]
    pub environment_name: Option<String>,
    #[prost(string, optional, tag = "15")]
    pub environment_public_id: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct HeadlessRepositoryInfoWire {
    #[prost(string, tag = "2")]
    pub path_encryption_key: String,
    #[prost(bool, tag = "6")]
    pub should_sync_index: bool,
}

#[derive(Clone, PartialEq, Message)]
pub struct PrivateWorkerLabelWire {
    #[prost(string, tag = "1")]
    pub key: String,
    #[prost(string, tag = "2")]
    pub value: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct StartBackgroundComposerRequestWire {
    #[prost(string, tag = "1")]
    pub snapshot_name_or_id: String,
    #[prost(message, optional, tag = "5")]
    pub repository_info: Option<HeadlessRepositoryInfoWire>,
    #[prost(string, tag = "6")]
    pub snapshot_workspace_root_path: String,
    #[prost(bool, tag = "9")]
    pub auto_branch: bool,
    #[prost(bool, tag = "10")]
    pub return_immediately: bool,
    #[prost(message, optional, tag = "11")]
    pub devcontainer_starting_point: Option<DevcontainerStartingPointWire>,
    #[prost(string, optional, tag = "14")]
    pub repo_url: Option<String>,
    #[prost(string, tag = "22")]
    pub bc_id: String,
    #[prost(int32, optional, tag = "23")]
    pub source: Option<i32>,
    #[prost(bool, optional, tag = "26")]
    pub add_initial_message_to_responses: Option<bool>,
    #[prost(string, optional, tag = "31")]
    pub base_branch: Option<String>,
    #[prost(bool, optional, tag = "37")]
    pub auto_create_pr: Option<bool>,
    #[prost(int32, optional, tag = "41")]
    pub starting_message_type: Option<i32>,
    #[prost(message, optional, tag = "44")]
    pub conversation_action: Option<ConversationActionWire>,
    #[prost(string, optional, tag = "50")]
    pub name: Option<String>,
    #[prost(bool, optional, tag = "55")]
    pub use_private_worker: Option<bool>,
    #[prost(message, repeated, tag = "58")]
    pub requested_models: Vec<RequestedModelWire>,
    #[prost(message, repeated, tag = "59")]
    pub labels: Vec<PrivateWorkerLabelWire>,
    #[prost(int32, optional, tag = "98")]
    pub team_id: Option<i32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct BackgroundComposerWire {
    #[prost(string, tag = "1")]
    pub bc_id: String,
    #[prost(double, tag = "2")]
    pub created_at_ms: f64,
    #[prost(string, tag = "5")]
    pub name: String,
    #[prost(string, tag = "6")]
    pub branch_name: String,
    #[prost(bool, tag = "9")]
    pub is_archived: bool,
    #[prost(int32, tag = "12")]
    pub status: i32,
    #[prost(string, tag = "22")]
    pub pr_url: String,
    #[prost(int32, optional, tag = "25")]
    pub lines_added: Option<i32>,
    #[prost(int32, optional, tag = "26")]
    pub lines_removed: Option<i32>,
    #[prost(int32, optional, tag = "27")]
    pub files_changed: Option<i32>,
    #[prost(int32, optional, tag = "37")]
    pub commit_count: Option<i32>,
    #[prost(int32, optional, tag = "43")]
    pub pr_status: Option<i32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct StartBackgroundComposerResponseWire {
    #[prost(message, optional, tag = "1")]
    pub composer: Option<BackgroundComposerWire>,
    #[prost(string, optional, tag = "3")]
    pub initial_run_id: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct HeadlessPromptWire {
    #[prost(string, tag = "1")]
    pub text: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct CustomErrorDetailsWire {
    #[prost(string, tag = "1")]
    pub title: String,
    #[prost(string, tag = "2")]
    pub detail: String,
    #[prost(map = "string, string", tag = "7")]
    pub additional_info: HashMap<String, String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ErrorDetailsWire {
    #[prost(message, optional, tag = "2")]
    pub details: Option<CustomErrorDetailsWire>,
}

#[derive(Clone, PartialEq, Message)]
pub struct BackgroundComposerPrWire {
    #[prost(string, tag = "1")]
    pub branch_name: String,
    #[prost(int32, optional, tag = "4")]
    pub pull_number: Option<i32>,
    #[prost(int32, optional, tag = "5")]
    pub pr_status: Option<i32>,
    #[prost(string, optional, tag = "6")]
    pub pr_url: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct DetailedBackgroundComposerWire {
    #[prost(message, optional, tag = "1")]
    pub composer: Option<BackgroundComposerWire>,
    #[prost(message, optional, tag = "4")]
    pub prompt: Option<HeadlessPromptWire>,
    #[prost(int32, tag = "5")]
    pub status: i32,
    #[prost(string, optional, tag = "10")]
    pub summary: Option<String>,
    #[prost(message, optional, tag = "16")]
    pub permanent_error: Option<ErrorDetailsWire>,
    #[prost(message, repeated, tag = "20")]
    pub prs: Vec<BackgroundComposerPrWire>,
}

#[derive(Clone, PartialEq, Message)]
pub struct GetBackgroundComposerInfoRequestWire {
    #[prost(string, tag = "1")]
    pub bc_id: String,
    #[prost(bool, tag = "2")]
    pub include_diff: bool,
    #[prost(bool, tag = "3")]
    pub do_not_throw_if_setup_not_finished: bool,
}

#[derive(Clone, PartialEq, Message)]
pub struct GetBackgroundComposerInfoResponseWire {
    #[prost(message, optional, tag = "1")]
    pub composer: Option<DetailedBackgroundComposerWire>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ListBackgroundComposersRequestWire {
    #[prost(int32, tag = "1")]
    pub n: i32,
    #[prost(bool, optional, tag = "10")]
    pub include_archived: Option<bool>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ListBackgroundComposersResponseWire {
    #[prost(message, repeated, tag = "1")]
    pub composers: Vec<BackgroundComposerWire>,
}

#[derive(Clone, PartialEq, Message)]
pub struct AddAsyncFollowupBackgroundComposerRequestWire {
    #[prost(string, tag = "1")]
    pub bc_id: String,
    #[prost(bool, tag = "3")]
    pub synchronous: bool,
    #[prost(int32, optional, tag = "7")]
    pub followup_source: Option<i32>,
    #[prost(message, optional, tag = "10")]
    pub followup_conversation_action: Option<ConversationActionWire>,
    #[prost(message, optional, tag = "15")]
    pub requested_model: Option<RequestedModelWire>,
}

#[derive(Clone, PartialEq, Message)]
pub struct AddAsyncFollowupBackgroundComposerResponseWire {
    #[prost(string, tag = "1")]
    pub run_id: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct PauseBackgroundComposerRequestWire {
    #[prost(string, tag = "1")]
    pub bc_id: String,
    #[prost(int32, tag = "3")]
    pub source: i32,
}

#[derive(Clone, PartialEq, Message)]
pub struct RenameBackgroundComposerRequestWire {
    #[prost(string, tag = "1")]
    pub bc_id: String,
    #[prost(string, tag = "2")]
    pub new_name: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct ArchiveBackgroundComposerRequestWire {
    #[prost(string, tag = "1")]
    pub bc_id: String,
    #[prost(bool, tag = "2")]
    pub unarchive: bool,
    #[prost(int32, tag = "4")]
    pub source: i32,
}

#[derive(Clone, PartialEq, Message)]
pub struct DeleteBackgroundComposerRequestWire {
    #[prost(string, tag = "1")]
    pub bc_id: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct ListBackgroundComposerArtifactsRequestWire {
    #[prost(string, tag = "1")]
    pub bc_id: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct BackgroundComposerArtifactWire {
    #[prost(string, tag = "1")]
    pub absolute_path: String,
    #[prost(int64, tag = "2")]
    pub size_bytes: i64,
}

#[derive(Clone, PartialEq, Message)]
pub struct ListBackgroundComposerArtifactsResponseWire {
    #[prost(message, repeated, tag = "1")]
    pub artifacts: Vec<BackgroundComposerArtifactWire>,
}

#[derive(Clone, PartialEq, Message)]
pub struct GetBackgroundComposerConversationRequestWire {
    #[prost(string, tag = "1")]
    pub bc_id: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct GetBackgroundComposerConversationResponseWire {
    #[prost(bytes = "vec", repeated, tag = "1")]
    pub conversation: Vec<Vec<u8>>,
}

#[derive(Clone, PartialEq, Message)]
pub struct GetPullRequestMergeStatusRequestWire {
    #[prost(string, tag = "1")]
    pub pr_url: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct GetPullRequestMergeStatusResponseWire {
    #[prost(bool, tag = "1")]
    pub is_merged: bool,
    #[prost(bool, tag = "2")]
    pub is_closed: bool,
    #[prost(string, tag = "4")]
    pub state: String,
    #[prost(bool, tag = "5")]
    pub is_draft: bool,
}

#[derive(Clone, PartialEq, Message)]
pub struct FileDiffWire {
    #[prost(string, tag = "1")]
    pub from: String,
    #[prost(string, tag = "2")]
    pub to: String,
    #[prost(int32, tag = "4")]
    pub added: i32,
    #[prost(int32, tag = "5")]
    pub removed: i32,
}

#[derive(Clone, PartialEq, Message)]
pub struct GitDiffWire {
    #[prost(message, repeated, tag = "1")]
    pub diffs: Vec<FileDiffWire>,
}

#[derive(Clone, PartialEq, Message)]
pub struct GetOptimizedDiffDetailsRequestWire {
    #[prost(string, tag = "1")]
    pub bc_id: String,
    #[prost(bool, tag = "2")]
    pub exclude_before_after_diffs: bool,
}

#[derive(Clone, PartialEq, Message)]
pub struct GetOptimizedDiffDetailsResponseWire {
    #[prost(message, optional, tag = "1")]
    pub diff: Option<GitDiffWire>,
}

#[derive(Clone, PartialEq, Message)]
pub struct LogicalEnvironmentRepoEntryWire {
    #[prost(string, tag = "1")]
    pub repo_url: String,
    #[prost(string, tag = "2")]
    pub scm_repo_node_id: String,
    #[prost(string, optional, tag = "3")]
    pub git_enterprise_uuid: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct LogicalEnvironmentRepoConfigWire {
    #[prost(message, repeated, tag = "1")]
    pub repos: Vec<LogicalEnvironmentRepoEntryWire>,
}

#[derive(Clone, PartialEq, Message)]
pub struct LogicalEnvironmentWire {
    #[prost(int64, tag = "1")]
    pub id: i64,
    #[prost(string, tag = "2")]
    pub public_id: String,
    #[prost(string, tag = "3")]
    pub name: String,
    #[prost(message, optional, tag = "7")]
    pub repo_config: Option<LogicalEnvironmentRepoConfigWire>,
}

#[derive(Clone, PartialEq, Message)]
pub struct GetEnvironmentRequestWire {
    #[prost(string, tag = "1")]
    pub public_id: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct GetEnvironmentResponseWire {
    #[prost(message, optional, tag = "1")]
    pub environment: Option<LogicalEnvironmentWire>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ListEnvironmentsRequestWire {
    #[prost(int32, optional, tag = "1")]
    pub limit: Option<i32>,
    #[prost(bool, optional, tag = "4")]
    pub include_repository_scope_environments: Option<bool>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ListEnvironmentsResponseWire {
    #[prost(message, repeated, tag = "1")]
    pub environments: Vec<LogicalEnvironmentWire>,
}

#[derive(Clone, PartialEq, Message)]
pub struct TeamWire {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(int32, tag = "2")]
    pub id: i32,
    #[prost(bool, tag = "36")]
    pub is_direct_member: bool,
}

#[derive(Clone, PartialEq, Message)]
pub struct GetTeamsRequestWire {
    #[prost(bool, optional, tag = "1")]
    pub active_only: Option<bool>,
}

#[derive(Clone, PartialEq, Message)]
pub struct GetTeamsResponseWire {
    #[prost(message, repeated, tag = "1")]
    pub teams: Vec<TeamWire>,
}

#[derive(Clone, PartialEq, Message)]
pub struct BackgroundAgentSettingsWire {
    #[prost(bool, optional, tag = "24")]
    pub disable_cloud_agents_in_sand: Option<bool>,
}

#[derive(Clone, PartialEq, Message)]
pub struct GetTeamAdminSettingsRequestWire {
    #[prost(int32, optional, tag = "1")]
    pub team_id: Option<i32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct GetTeamAdminSettingsResponseWire {
    #[prost(message, optional, tag = "7")]
    pub background_agent_settings: Option<BackgroundAgentSettingsWire>,
}
