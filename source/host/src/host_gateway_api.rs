use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use serde_json::Value;

pub const HOST_CAPABILITIES: [&str; 2] = ["orderedReplicasV1", "sendAcceptanceV1"];
pub const CREATE_AGENT_NONCE_LEDGER_CAP: usize = 64;
pub const DISABLE_SEND_ACCEPT_RETURN_ENV: &str = "SAND_DISABLE_SEND_ACCEPT_RETURN";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostGatewayOwner {
    TranscriptManager,
    AutoReview,
    LocalToolPermission,
    AgentLifecycle,
    Session,
    HostControl,
    FeatureGate,
    CrossUserSharing,
    Automations,
    Workflow,
    ManagedSetup,
    Mcp,
    ForeverBox,
    CloudAgents,
    BoxStore,
    HostUpgrade,
    TeachRecording,
    Trays,
    Attachments,
    Settings,
    WebAuthn,
    Secrets,
}

macro_rules! define_host_gateway_registry {
    ($(($method:literal, $owner:ident)),+ $(,)?) => {
        pub const FROZEN_HOST_GATEWAY_METHODS: &[&str] = &[$($method),+];
        pub fn host_gateway_owner(method: &str) -> Option<HostGatewayOwner> {
            match method {
                $($method => Some(HostGatewayOwner::$owner),)+
                _ => None,
            }
        }
    };
}

define_host_gateway_registry!(
    ("getTranscript", TranscriptManager),
    ("getAgentTranscript", TranscriptManager),
    ("getAgentTranscriptPage", TranscriptManager),
    ("getAgentTranscriptWindow", TranscriptManager),
    ("getAgentTranscriptTail", TranscriptManager),
    ("getAgentThread", TranscriptManager),
    ("sendPrompt", TranscriptManager),
    ("promptAcceptanceStatus", TranscriptManager),
    ("respondToWidget", TranscriptManager),
    ("resolveAutoReviewApproval", AutoReview),
    ("resolveLocalToolPermission", LocalToolPermission),
    ("dismissWidget", TranscriptManager),
    ("submitSecret", TranscriptManager),
    ("reactToMessage", TranscriptManager),
    ("appendConnectorCard", TranscriptManager),
    ("listAgents", TranscriptManager),
    ("countAgents", TranscriptManager),
    ("searchAgents", TranscriptManager),
    ("searchMedia", TranscriptManager),
    ("createAgent", AgentLifecycle),
    ("kickstartAgent", AgentLifecycle),
    ("requestDiskSaverAudit", HostControl),
    ("createGroup", AgentLifecycle),
    ("setGroupMembers", AgentLifecycle),
    ("updateAgent", AgentLifecycle),
    ("deleteAgent", AgentLifecycle),
    ("deleteAgents", AgentLifecycle),
    ("duplicateAgent", AgentLifecycle),
    ("setAgentUnread", Session),
    ("setAgentNotificationsEnabled", Session),
    ("setAgentNotifyOnUpdates", Session),
    ("setAgentHiddenFromSidebar", Session),
    ("openAgent", Session),
    ("openAgentWindowed", Session),
    ("openAgentTail", Session),
    ("setWindowFocused", Session),
    ("getAgentMemories", TranscriptManager),
    ("deleteAgentMemory", TranscriptManager),
    ("clearAgentMemories", TranscriptManager),
    ("getAgentAutomations", Automations),
    ("listAllAutomations", Automations),
    ("isAgentNetworkEnabled", FeatureGate),
    ("isGlobalSearchEnabled", FeatureGate),
    ("isEgressTunnelAvailable", FeatureGate),
    ("getSharingState", CrossUserSharing),
    ("createRoomFromAgent", CrossUserSharing),
    ("createRoomInvite", CrossUserSharing),
    ("joinSharedRoom", CrossUserSharing),
    ("respondToRoomJoinRequest", CrossUserSharing),
    ("createSharedRoom", CrossUserSharing),
    ("addOwnAgentToSharedRoom", CrossUserSharing),
    ("removeOwnAgentFromSharedRoom", CrossUserSharing),
    ("setSharedRoomTyping", CrossUserSharing),
    ("leaveSharedRoom", CrossUserSharing),
    ("setAgentAutomationEnabled", Automations),
    ("createAgentAutomation", Automations),
    ("updateAgentAutomation", Automations),
    ("deleteAgentAutomation", Automations),
    ("runAgentAutomationNow", Automations),
    ("broadcastToAgents", HostControl),
    ("getAgentWorkflows", Workflow),
    ("createAgentWorkflow", Workflow),
    ("updateAgentWorkflow", Workflow),
    ("setAgentWorkflowEnabled", Workflow),
    ("deleteAgentWorkflow", Workflow),
    ("runAgentWorkflowNow", Workflow),
    ("importAgentWorkflowText", Workflow),
    ("importAgentWorkflowUrl", Workflow),
    ("portAgentLocalSkills", Workflow),
    ("getConversationOutline", TranscriptManager),
    ("skillsCatalog", ManagedSetup),
    ("syncPluginSkills", Mcp),
    ("getPluginSyncStatus", Mcp),
    ("getSkillPublishTargets", Mcp),
    ("publishSkill", Mcp),
    ("resyncPublishedSkill", Mcp),
    ("unpublishSkill", Mcp),
    ("getAgentChannels", Automations),
    ("connectChannel", Automations),
    ("disconnectChannel", Automations),
    ("refreshChannel", Automations),
    ("getListenerIntegrations", Automations),
    ("getListenerConnectUrl", Automations),
    ("getSubagents", TranscriptManager),
    ("getAsyncTasks", TranscriptManager),
    ("setAgentAvatarBytes", Session),
    ("getAgentAvatar", Session),
    ("getForeverBoxStatus", ForeverBox),
    ("getCloudAgentInfo", CloudAgents),
    ("ensureForeverBox", ForeverBox),
    ("resetForeverBox", ForeverBox),
    ("updateForeverBox", ForeverBox),
    ("autoUpdateBoxNow", ForeverBox),
    ("snapshotBoxStoreNow", BoxStore),
    ("getBoxStoreStatus", BoxStore),
    ("clearBoxStoreNow", BoxStore),
    ("updateHostNow", HostUpgrade),
    ("getHostStatus", HostUpgrade),
    ("setBoxMigrating", ForeverBox),
    ("prepareBoxForRecreate", HostControl),
    ("resumeBoxAfterRecreate", HostControl),
    ("handBackForeverBox", Session),
    ("startTeachRecording", TeachRecording),
    ("stopTeachRecording", TeachRecording),
    ("getTeachRecordingStatus", TeachRecording),
    ("getTrays", Trays),
    ("dismissTray", Trays),
    ("clearTrays", Trays),
    ("uploadAttachment", Attachments),
    ("readAttachmentImage", Attachments),
    ("readAttachmentText", Attachments),
    ("readAttachmentChunk", Attachments),
    ("getHostSettings", Settings),
    ("setHostSettings", Settings),
    ("refreshMcp", Mcp),
    ("listRoutedMcpTools", Mcp),
    ("executeRoutedMcpTool", Mcp),
    ("listBoxMcpServers", Mcp),
    ("completeMcpOAuth", Mcp),
    ("requestWebAuthnCeremony", WebAuthn),
    ("setBoxSecrets", Secrets),
    ("getBoxSecretsStatus", Secrets),
);

pub fn is_sand_agent_purpose(value: &str) -> bool {
    matches!(value, "disk-saver" | "plugin-auth")
}

pub fn sanitize_template_id(value: &str) -> Option<&str> {
    let valid_len = (1..=64).contains(&value.len());
    let valid_chars = value
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    (valid_len && valid_chars).then_some(value)
}

pub fn sanitize_create_agent_args(args: &Value) -> Value {
    let mut projected = args.clone();
    let Some(object) = projected.as_object_mut() else { return projected; };
    if object.get("purpose").and_then(Value::as_str).is_some_and(|purpose| !is_sand_agent_purpose(purpose)) {
        object.remove("purpose");
    }
    if object.get("templateId").and_then(Value::as_str).is_some_and(|id| sanitize_template_id(id).is_none()) {
        object.remove("templateId");
    }
    projected
}

#[derive(Debug, Default)]
pub struct CreateAgentNonceLedger<T> {
    order: VecDeque<String>,
    values: HashMap<String, T>,
}

impl<T> CreateAgentNonceLedger<T> {
    pub fn insert(&mut self, nonce: impl Into<String>, value: T) {
        let nonce = nonce.into();
        if !self.values.contains_key(&nonce) { self.order.push_back(nonce.clone()); }
        self.values.insert(nonce, value);
        while self.order.len() > CREATE_AGENT_NONCE_LEDGER_CAP {
            if let Some(expired) = self.order.pop_front() { self.values.remove(&expired); }
        }
    }
    pub fn get(&self, nonce: &str) -> Option<&T> { self.values.get(nonce) }
    pub fn len(&self) -> usize { self.values.len() }
    pub fn is_empty(&self) -> bool { self.values.is_empty() }
}

/// Existing Android capability grouping retained for native composition callers.
pub fn host_capabilities() -> BTreeSet<&'static str> {
    ["agents","transcript","mcp","automations","workflows","sharing","box","media","trays"]
        .into_iter()
        .collect()
}

pub struct DynamicGatewayApi {
    handlers: BTreeMap<String, Box<dyn Fn(&str)->Result<String,String> + Send + Sync>>,
}

impl DynamicGatewayApi {
    pub fn new() -> Self { Self { handlers:BTreeMap::new() } }
    pub fn register(&mut self, method: impl Into<String>, handler: Box<dyn Fn(&str)->Result<String,String> + Send + Sync>) -> Result<(), &'static str> {
        let method=method.into();
        if method.trim().is_empty() { return Err("gateway method is empty"); }
        if self.handlers.insert(method,handler).is_some() { return Err("duplicate gateway method"); }
        Ok(())
    }
    pub fn call(&self, method:&str, payload:&str)->Result<String,String> {
        self.handlers.get(method).ok_or_else(||"unknown-method".to_string())?(payload)
    }
}
impl Default for DynamicGatewayApi { fn default()->Self{Self::new()} }
