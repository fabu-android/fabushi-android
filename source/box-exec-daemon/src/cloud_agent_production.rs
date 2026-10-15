use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use fabushi_android_shared::cloud_agent_model_catalog_wire::{
    decode_cloud_agent_available_models_response, encode_cloud_agent_available_models_request,
};
use fabushi_android_shared::model_catalog::{
    find_catalog_entry, validate_model_params, SandModelCatalogEntry, SandModelCatalogParameterType,
};
use fabushi_android_shared::ExecutionError;
use prost::Message;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use url::Url;

use crate::cloud_agent_backend::{CloudAgentAction, CloudAgentExecutor, CloudAgentRequest};
use crate::cloud_agent_generated_trace::{decode_generated_conversation_trace, trace_to_jsonl};
use crate::cloud_agent_images::ScopedCloudAgentImageLoader;
use crate::cloud_agent_transcript_dump::cloud_agent_transcript_dump_path;
use crate::cloud_agent_wire::*;
use crate::server::{
    RemoteBackendCancelOutcome, RemoteBackendReconcileOutcome, RemoteBackendStartOutcome,
    RemoteRequestIdentity,
};

pub const AVAILABLE_MODELS_PATH: &str = "/aiserver.v1.AiService/AvailableModels";
pub const START_BACKGROUND_COMPOSER_PATH: &str =
    "/aiserver.v1.BackgroundComposerService/StartBackgroundComposerFromSnapshot";
pub const GET_BACKGROUND_COMPOSER_INFO_PATH: &str =
    "/aiserver.v1.BackgroundComposerService/GetBackgroundComposerInfo";
pub const LIST_BACKGROUND_COMPOSERS_PATH: &str =
    "/aiserver.v1.BackgroundComposerService/ListBackgroundComposers";
pub const ADD_ASYNC_FOLLOWUP_PATH: &str =
    "/aiserver.v1.BackgroundComposerService/AddAsyncFollowupBackgroundComposer";
pub const PAUSE_BACKGROUND_COMPOSER_PATH: &str =
    "/aiserver.v1.BackgroundComposerService/PauseBackgroundComposer";
pub const RENAME_BACKGROUND_COMPOSER_PATH: &str =
    "/aiserver.v1.BackgroundComposerService/RenameBackgroundComposer";
pub const ARCHIVE_BACKGROUND_COMPOSER_PATH: &str =
    "/aiserver.v1.BackgroundComposerService/ArchiveBackgroundComposer";
pub const DELETE_BACKGROUND_COMPOSER_PATH: &str =
    "/aiserver.v1.BackgroundComposerService/DeleteBackgroundComposer";
pub const LIST_BACKGROUND_COMPOSER_ARTIFACTS_PATH: &str =
    "/aiserver.v1.BackgroundComposerService/ListBackgroundComposerArtifacts";
pub const GET_BACKGROUND_COMPOSER_CONVERSATION_PATH: &str =
    "/aiserver.v1.BackgroundComposerService/GetBackgroundComposerConversation";
pub const CLOUD_AGENT_URL_PREFIX: &str = "https://cursor.com/agents/";

const RPC_TIMEOUT_MS: u64 = 30_000;
const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct ProductionCloudAgentConfig {
    pub backend_url: String,
    pub access_token: String,
    pub machine_id: String,
    pub client_version: String,
    pub box_namespace: String,
    pub workspace_root: PathBuf,
    pub media_roots: Vec<PathBuf>,
}

impl ProductionCloudAgentConfig {
    pub fn validate(&self) -> Result<(), ExecutionError> {
        let backend = Url::parse(&self.backend_url).map_err(|error| {
            ExecutionError::InvalidRequest(format!("invalid CloudAgent backend URL: {error}"))
        })?;
        if !matches!(backend.scheme(), "https" | "http") {
            return Err(ExecutionError::InvalidRequest(
                "CloudAgent backend URL must be http(s)".into(),
            ));
        }
        for (label, value) in [
            ("access token", self.access_token.as_str()),
            ("machine id", self.machine_id.as_str()),
            ("client version", self.client_version.as_str()),
            ("box namespace", self.box_namespace.as_str()),
        ] {
            if value.trim().is_empty() || value.chars().any(char::is_control) {
                return Err(ExecutionError::InvalidRequest(format!(
                    "CloudAgent {label} is invalid"
                )));
            }
        }
        if !self.workspace_root.is_absolute()
            || self.media_roots.iter().any(|root| !root.is_absolute())
        {
            return Err(ExecutionError::InvalidRequest(
                "CloudAgent workspace/media roots must be absolute".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TranscriptFileOutcome {
    status: String,
    path: String,
    line_count: usize,
    size_bytes: usize,
}

pub struct ProductionCloudAgentExecutor {
    config: ProductionCloudAgentConfig,
    agent: ureq::Agent,
    image_loader: ScopedCloudAgentImageLoader,
}

impl ProductionCloudAgentExecutor {
    pub fn new(config: ProductionCloudAgentConfig) -> Result<Self, ExecutionError> {
        config.validate()?;
        let timeout = Duration::from_millis(RPC_TIMEOUT_MS);
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout_read(timeout)
            .timeout_write(Duration::from_secs(20))
            .redirects(0)
            .build();
        let image_loader = ScopedCloudAgentImageLoader::new(
            config.workspace_root.clone(),
            config.media_roots.clone(),
        );
        Ok(Self {
            config,
            agent,
            image_loader,
        })
    }

    fn rpc_bytes(
        &self,
        identity: &RemoteRequestIdentity,
        path: &str,
        body: &[u8],
    ) -> Result<Vec<u8>, ExecutionError> {
        let url = Url::parse(&self.config.backend_url)
            .and_then(|base| base.join(path))
            .map_err(|error| {
                ExecutionError::Transport(format!("invalid CloudAgent RPC URL: {error}"))
            })?;
        let authorization = format!("Bearer {}", self.config.access_token);
        let checksum = create_cursor_checksum(&self.config.machine_id, system_now_ms());
        let response = self
            .agent
            .post(url.as_str())
            .set("content-type", "application/proto")
            .set("connect-protocol-version", "1")
            .set("authorization", &authorization)
            .set("x-cursor-checksum", &checksum)
            .set("x-cursor-client-type", "sand")
            .set("x-cursor-client-version", &self.config.client_version)
            .set("x-sand-box-namespace", &self.config.box_namespace)
            .set("x-ghost-mode", "off")
            .set("x-request-id", &identity.request_id)
            .send_bytes(body)
            .map_err(|error| {
                ExecutionError::Transport(format!("CloudAgent RPC failed: {error}"))
            })?;
        let mut output = Vec::new();
        response
            .into_reader()
            .take(MAX_RESPONSE_BYTES as u64 + 1)
            .read_to_end(&mut output)
            .map_err(|error| ExecutionError::Transport(format!("read CloudAgent RPC: {error}")))?;
        if output.len() > MAX_RESPONSE_BYTES {
            return Err(ExecutionError::Transport(
                "CloudAgent RPC response exceeds bounded size".into(),
            ));
        }
        Ok(output)
    }

    fn unary<Req: Message, Resp: Message + Default>(
        &self,
        identity: &RemoteRequestIdentity,
        path: &str,
        request: Req,
    ) -> Result<Resp, ExecutionError> {
        let bytes = self.rpc_bytes(identity, path, &request.encode_to_vec())?;
        Resp::decode(bytes.as_slice()).map_err(|error| {
            ExecutionError::Transport(format!("decode CloudAgent RPC response: {error}"))
        })
    }

    fn unary_empty<Req: Message>(
        &self,
        identity: &RemoteRequestIdentity,
        path: &str,
        request: Req,
    ) -> Result<(), ExecutionError> {
        let _ = self.rpc_bytes(identity, path, &request.encode_to_vec())?;
        Ok(())
    }

    fn model_catalog(
        &self,
        identity: &RemoteRequestIdentity,
    ) -> Result<Vec<SandModelCatalogEntry>, ExecutionError> {
        let bytes = self.rpc_bytes(
            identity,
            AVAILABLE_MODELS_PATH,
            &encode_cloud_agent_available_models_request(),
        )?;
        decode_cloud_agent_available_models_response(&bytes)
            .map_err(|error| ExecutionError::Transport(error.to_string()))
    }

    fn validate_model_selection(
        &self,
        identity: &RemoteRequestIdentity,
        request: &CloudAgentRequest,
    ) -> Result<(), ExecutionError> {
        let Some(model) = request
            .model
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        else {
            return Ok(());
        };
        let catalog = self.model_catalog(identity)?;
        let entry = find_catalog_entry(&catalog, model).ok_or_else(|| {
            ExecutionError::InvalidRequest(format!("CloudAgent model {model:?} is unavailable"))
        })?;
        let params = request
            .model_params
            .iter()
            .map(|(id, value)| (id.clone(), value.clone()))
            .collect::<Vec<_>>();
        let errors = validate_model_params(entry, &params);
        if errors.is_empty() {
            Ok(())
        } else {
            Err(ExecutionError::InvalidRequest(errors.join(" ")))
        }
    }

    fn requested_model(&self, request: &CloudAgentRequest) -> Option<RequestedModelWire> {
        request
            .model
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(|model_id| RequestedModelWire {
                model_id: model_id.to_string(),
                max_mode: true,
                parameters: request
                    .model_params
                    .iter()
                    .map(|(id, value)| RequestedModelParameterWire {
                        id: id.clone(),
                        value: value.clone(),
                    })
                    .collect(),
            })
    }

    fn conversation_action(
        &self,
        identity: &RemoteRequestIdentity,
        request: &CloudAgentRequest,
    ) -> Result<ConversationActionWire, ExecutionError> {
        let selected_images = request
            .images
            .iter()
            .map(|image| {
                self.image_loader
                    .load(&image.url)
                    .map_err(|error| {
                        ExecutionError::InvalidRequest(format!(
                    "CloudAgent image is outside the authorized scope or unreadable: {error:?}"
                ))
                    })
                    .map(|loaded| SelectedImageWire {
                        data: loaded.data,
                        uuid: String::new(),
                        path: loaded.path.to_string_lossy().into_owned(),
                        mime_type: loaded.mime_type.to_string(),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ConversationActionWire {
            user_message_action: Some(UserMessageActionWire {
                user_message: Some(UserMessageWire {
                    text: request.prompt.clone().unwrap_or_default(),
                    message_id: deterministic_id(
                        "message",
                        identity,
                        request.prompt.as_deref().unwrap_or_default(),
                    ),
                    selected_context: (!selected_images.is_empty())
                        .then_some(SelectedContextWire { selected_images }),
                    mode: AGENT_MODE_AGENT,
                }),
                send_to_interaction_listener: Some(true),
            }),
        })
    }

    fn launch(
        &self,
        identity: &RemoteRequestIdentity,
        request: &CloudAgentRequest,
    ) -> Result<String, ExecutionError> {
        self.validate_model_selection(identity, request)?;
        if request
            .environment
            .as_ref()
            .is_some_and(|environment| environment.kind != "cloud")
        {
            return Err(ExecutionError::CapabilityUnavailable(
                "saved/pool/private-worker CloudAgent environments require the external environment resolver".into(),
            ));
        }
        let raw_repo = request
            .repo_url
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                ExecutionError::InvalidRequest(
                    "CloudAgent launch requires repo_url at the external Runner boundary".into(),
                )
            })?;
        let repo = normalize_repo_url(raw_repo)?;
        let bc_id = deterministic_id("bc", identity, &repo);
        let response: StartBackgroundComposerResponseWire = self.unary(
            identity,
            START_BACKGROUND_COMPOSER_PATH,
            StartBackgroundComposerRequestWire {
                snapshot_name_or_id: repo.clone(),
                repository_info: Some(HeadlessRepositoryInfoWire {
                    path_encryption_key: String::new(),
                    should_sync_index: false,
                }),
                snapshot_workspace_root_path: "/workspace".into(),
                auto_branch: true,
                return_immediately: true,
                devcontainer_starting_point: Some(DevcontainerStartingPointWire {
                    url: repo.clone(),
                    r#ref: request.starting_ref.clone().unwrap_or_default(),
                    repo_config: None,
                    environment_name: None,
                    environment_public_id: None,
                }),
                repo_url: Some(repo),
                bc_id: bc_id.clone(),
                source: Some(BACKGROUND_COMPOSER_SOURCE_GROK_BOT),
                add_initial_message_to_responses: Some(true),
                base_branch: request.starting_ref.clone(),
                auto_create_pr: Some(true),
                starting_message_type: Some(STARTING_MESSAGE_TYPE_USER_MESSAGE),
                conversation_action: Some(self.conversation_action(identity, request)?),
                name: request.title.clone(),
                use_private_worker: None,
                requested_models: self.requested_model(request).into_iter().collect(),
                labels: Vec::new(),
                team_id: None,
            },
        )?;
        let id = response
            .composer
            .as_ref()
            .map(|value| value.bc_id.trim())
            .filter(|value| !value.is_empty())
            .unwrap_or(&bc_id)
            .to_string();
        Ok(json!({
            "agentId": id,
            "url": format!("{CLOUD_AGENT_URL_PREFIX}{id}"),
            "initialRunId": response.initial_run_id,
        })
        .to_string())
    }

    fn get_detail(
        &self,
        identity: &RemoteRequestIdentity,
        id: &str,
    ) -> Result<Option<DetailedBackgroundComposerWire>, ExecutionError> {
        let response: GetBackgroundComposerInfoResponseWire = self.unary(
            identity,
            GET_BACKGROUND_COMPOSER_INFO_PATH,
            GetBackgroundComposerInfoRequestWire {
                bc_id: id.into(),
                include_diff: false,
                do_not_throw_if_setup_not_finished: true,
            },
        )?;
        Ok(response.composer)
    }

    fn dump_conversation(
        &self,
        identity: &RemoteRequestIdentity,
        agent_id: &str,
    ) -> Result<TranscriptFileOutcome, ExecutionError> {
        let response: GetBackgroundComposerConversationResponseWire = self.unary(
            identity,
            GET_BACKGROUND_COMPOSER_CONVERSATION_PATH,
            GetBackgroundComposerConversationRequestWire {
                bc_id: agent_id.into(),
            },
        )?;
        let trace =
            decode_generated_conversation_trace(&response.conversation).map_err(|error| {
                ExecutionError::Transport(format!(
                    "decode generated CloudAgent transcript: {error}"
                ))
            })?;
        let jsonl = trace_to_jsonl(&trace).map_err(|error| {
            ExecutionError::Transport(format!("encode CloudAgent transcript: {error}"))
        })?;
        let relative =
            cloud_agent_transcript_dump_path(agent_id).map_err(ExecutionError::InvalidRequest)?;
        let root = fs::canonicalize(&self.config.workspace_root).map_err(|error| {
            ExecutionError::Transport(format!("canonicalize CloudAgent workspace: {error}"))
        })?;
        let target = root.join(&relative);
        let parent = target.parent().ok_or_else(|| {
            ExecutionError::InvalidRequest("CloudAgent transcript path has no parent".into())
        })?;
        fs::create_dir_all(parent).map_err(|error| {
            ExecutionError::Transport(format!("create CloudAgent transcript directory: {error}"))
        })?;
        let parent = fs::canonicalize(parent).map_err(|error| {
            ExecutionError::Transport(format!(
                "canonicalize CloudAgent transcript directory: {error}"
            ))
        })?;
        if !parent.starts_with(&root) || parent == root {
            return Err(ExecutionError::InvalidRequest(
                "CloudAgent transcript directory escapes the authorized workspace".into(),
            ));
        }
        let target = parent.join(target.file_name().ok_or_else(|| {
            ExecutionError::InvalidRequest("CloudAgent transcript path has no file name".into())
        })?);
        if fs::symlink_metadata(&target).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(ExecutionError::InvalidRequest(
                "CloudAgent transcript target must not be a symlink".into(),
            ));
        }
        fs::write(&target, jsonl.as_bytes()).map_err(|error| {
            ExecutionError::Transport(format!("write CloudAgent transcript: {error}"))
        })?;
        let status = self
            .get_detail(identity, agent_id)?
            .map(|detail| status_name(detail.status).to_string())
            .unwrap_or_else(|| "unknown".into());
        Ok(TranscriptFileOutcome {
            status,
            path: relative,
            line_count: trace.len(),
            size_bytes: jsonl.len(),
        })
    }

    fn watch_once(
        &self,
        identity: &RemoteRequestIdentity,
        request: &CloudAgentRequest,
    ) -> Result<Option<String>, ExecutionError> {
        let id = request.agent_id.as_deref().unwrap_or_default();
        let Some(detail) = self.get_detail(identity, id)? else {
            return Ok(Some(
                json!({
                    "status":"error",
                    "text":format!("No cloud agent found for {id}.")
                })
                .to_string(),
            ));
        };
        let status = detail
            .composer
            .as_ref()
            .map(|composer| status_name(composer.status))
            .unwrap_or_else(|| status_name(detail.status));
        if matches!(status, "running" | "creating" | "unknown") {
            return Ok(None);
        }
        let mut text = detail
            .summary
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("Cloud agent {id} finished with status {status}."));
        if let Ok(dump) = self.dump_conversation(identity, id) {
            if dump.line_count > 0 {
                text.push_str(&format!(
                    "\n\nFull transcript dumped to {} ({} bytes, {} message lines). Read it with your shell tools; `tail -n 1 {}` gives the final assistant report.",
                    dump.path, dump.size_bytes, dump.line_count, dump.path
                ));
            }
        }
        Ok(Some(
            json!({
                "status": if status == "finished" { "completed" } else { "error" },
                "text": text,
                "agent": detail_json(detail),
            })
            .to_string(),
        ))
    }

    fn read_action(
        &self,
        identity: &RemoteRequestIdentity,
        request: &CloudAgentRequest,
    ) -> Result<String, ExecutionError> {
        match request.action {
            CloudAgentAction::Models => {
                let models = self.model_catalog(identity)?;
                Ok(json!({"models":models.iter().map(model_json).collect::<Vec<_>>()}).to_string())
            }
            CloudAgentAction::List => {
                if request.scope.as_deref() == Some("launched") {
                    return Err(ExecutionError::CapabilityUnavailable(
                        "CloudAgent launched-scope filtering requires the durable managed-id owner"
                            .into(),
                    ));
                }
                let response: ListBackgroundComposersResponseWire = self.unary(
                    identity,
                    LIST_BACKGROUND_COMPOSERS_PATH,
                    ListBackgroundComposersRequestWire {
                        n: i32::try_from(request.limit.unwrap_or(20)).unwrap_or(i32::MAX),
                        include_archived: None,
                    },
                )?;
                let include_archived = request.include_archived.unwrap_or(false);
                Ok(json!({"agents":response.composers.into_iter()
                    .filter(|item| include_archived || !item.is_archived)
                    .map(composer_json).collect::<Vec<_>>()})
                .to_string())
            }
            CloudAgentAction::Get => {
                let detail =
                    self.get_detail(identity, request.agent_id.as_deref().unwrap_or_default())?;
                Ok(json!({"agent":detail.map(detail_json)}).to_string())
            }
            CloudAgentAction::Dump => {
                let id = request.agent_id.as_deref().unwrap_or_default();
                let dump = self.dump_conversation(identity, id)?;
                Ok(json!({
                    "status":dump.status,
                    "format":"no-preamble-jsonl",
                    "lineCount":dump.line_count,
                    "path":dump.path,
                    "sizeBytes":dump.size_bytes,
                    "traceDecoded":true,
                })
                .to_string())
            }
            CloudAgentAction::ListArtifacts => {
                let response: ListBackgroundComposerArtifactsResponseWire = self.unary(
                    identity,
                    LIST_BACKGROUND_COMPOSER_ARTIFACTS_PATH,
                    ListBackgroundComposerArtifactsRequestWire {
                        bc_id: request.agent_id.clone().unwrap_or_default(),
                    },
                )?;
                Ok(
                    json!({"artifacts":response.artifacts.into_iter().map(|artifact| json!({
                    "path":artifact.absolute_path,
                    "sizeBytes":u64::try_from(artifact.size_bytes).unwrap_or(0),
                })).collect::<Vec<_>>()})
                    .to_string(),
                )
            }
            _ => Err(ExecutionError::InvalidRequest(
                "not a CloudAgent read action".into(),
            )),
        }
    }

    fn mutate_action(
        &self,
        identity: &RemoteRequestIdentity,
        request: &CloudAgentRequest,
    ) -> Result<String, ExecutionError> {
        match request.action {
            CloudAgentAction::Launch => self.launch(identity, request),
            CloudAgentAction::Reply => {
                self.validate_model_selection(identity, request)?;
                let response: AddAsyncFollowupBackgroundComposerResponseWire = self.unary(
                    identity,
                    ADD_ASYNC_FOLLOWUP_PATH,
                    AddAsyncFollowupBackgroundComposerRequestWire {
                        bc_id: request.agent_id.clone().unwrap_or_default(),
                        synchronous: request.interrupt.unwrap_or(false),
                        followup_source: Some(BACKGROUND_COMPOSER_SOURCE_GROK_BOT),
                        followup_conversation_action: Some(
                            self.conversation_action(identity, request)?,
                        ),
                        requested_model: self.requested_model(request),
                    },
                )?;
                Ok(json!({"runId":response.run_id}).to_string())
            }
            CloudAgentAction::Cancel => {
                self.unary_empty(
                    identity,
                    PAUSE_BACKGROUND_COMPOSER_PATH,
                    PauseBackgroundComposerRequestWire {
                        bc_id: request.agent_id.clone().unwrap_or_default(),
                        source: BACKGROUND_COMPOSER_SOURCE_GROK_BOT,
                    },
                )?;
                Ok(json!({"cancelRequested":true}).to_string())
            }
            CloudAgentAction::Rename => {
                #[derive(Clone, PartialEq, Message)]
                struct RenameResponseWire {
                    #[prost(string, tag = "1")]
                    name: String,
                }
                let response: RenameResponseWire = self.unary(
                    identity,
                    RENAME_BACKGROUND_COMPOSER_PATH,
                    RenameBackgroundComposerRequestWire {
                        bc_id: request.agent_id.clone().unwrap_or_default(),
                        new_name: request.title.clone().unwrap_or_default(),
                    },
                )?;
                Ok(json!({"name":response.name}).to_string())
            }
            CloudAgentAction::Archive | CloudAgentAction::Unarchive => {
                #[derive(Clone, PartialEq, Message)]
                struct ArchiveResponseWire {
                    #[prost(bool, tag = "1")]
                    closed_pull_request: bool,
                }
                let archived = matches!(request.action, CloudAgentAction::Archive);
                let response: ArchiveResponseWire = self.unary(
                    identity,
                    ARCHIVE_BACKGROUND_COMPOSER_PATH,
                    ArchiveBackgroundComposerRequestWire {
                        bc_id: request.agent_id.clone().unwrap_or_default(),
                        unarchive: !archived,
                        source: BACKGROUND_COMPOSER_SOURCE_GROK_BOT,
                    },
                )?;
                Ok(json!({
                    "archived":archived,
                    "closedPullRequest":response.closed_pull_request,
                })
                .to_string())
            }
            CloudAgentAction::Delete => {
                self.unary_empty(
                    identity,
                    DELETE_BACKGROUND_COMPOSER_PATH,
                    DeleteBackgroundComposerRequestWire {
                        bc_id: request.agent_id.clone().unwrap_or_default(),
                    },
                )?;
                Ok(json!({"deleted":true}).to_string())
            }
            _ => Err(ExecutionError::InvalidRequest(
                "not a CloudAgent mutation".into(),
            )),
        }
    }
}

impl CloudAgentExecutor for ProductionCloudAgentExecutor {
    fn start(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &CloudAgentRequest,
    ) -> Result<RemoteBackendStartOutcome, ExecutionError> {
        if matches!(request.action, CloudAgentAction::Watch) {
            return Ok(match self.watch_once(identity, request)? {
                Some(output_json) => RemoteBackendStartOutcome::Completed { output_json },
                None => RemoteBackendStartOutcome::Accepted,
            });
        }
        let output_json = if matches!(
            request.action,
            CloudAgentAction::Models
                | CloudAgentAction::List
                | CloudAgentAction::Get
                | CloudAgentAction::Dump
                | CloudAgentAction::ListArtifacts
        ) {
            self.read_action(identity, request)?
        } else {
            self.mutate_action(identity, request)?
        };
        Ok(RemoteBackendStartOutcome::Completed { output_json })
    }

    fn reconcile(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &CloudAgentRequest,
    ) -> Result<RemoteBackendReconcileOutcome, ExecutionError> {
        if matches!(request.action, CloudAgentAction::Watch) {
            return Ok(match self.watch_once(identity, request)? {
                Some(output_json) => RemoteBackendReconcileOutcome::Completed { output_json },
                None => RemoteBackendReconcileOutcome::Pending,
            });
        }
        if matches!(
            request.action,
            CloudAgentAction::Models
                | CloudAgentAction::List
                | CloudAgentAction::Get
                | CloudAgentAction::Dump
                | CloudAgentAction::ListArtifacts
        ) {
            return Ok(RemoteBackendReconcileOutcome::Completed {
                output_json: self.read_action(identity, request)?,
            });
        }
        // Never replay launch/reply/rename/cancel/archive/delete after an ambiguous side effect.
        Ok(RemoteBackendReconcileOutcome::Pending)
    }

    fn cancel(
        &mut self,
        identity: &RemoteRequestIdentity,
        request: &CloudAgentRequest,
    ) -> Result<RemoteBackendCancelOutcome, ExecutionError> {
        if matches!(request.action, CloudAgentAction::Watch) {
            let cancel = CloudAgentRequest {
                action: CloudAgentAction::Cancel,
                prompt: None,
                images: Vec::new(),
                repo_url: None,
                starting_ref: None,
                model: None,
                model_params: BTreeMap::new(),
                title: None,
                environment: None,
                interrupt: None,
                agent_id: request.agent_id.clone(),
                scope: None,
                include_archived: None,
                limit: None,
                confirm: None,
            };
            return match self.mutate_action(identity, &cancel) {
                Ok(_) => Ok(RemoteBackendCancelOutcome::Confirmed),
                Err(ExecutionError::Transport(_)) => {
                    Ok(RemoteBackendCancelOutcome::OutcomeUnknown {
                        reason:
                            "CloudAgent cancellation transport failed; remote run state is unknown"
                                .into(),
                    })
                }
                Err(error) => Err(error),
            };
        }
        Ok(RemoteBackendCancelOutcome::OutcomeUnknown {
            reason: "CloudAgent mutation may already have crossed the backend side-effect boundary"
                .into(),
        })
    }
}

pub fn open_production_cloud_agent_service<A: crate::server::RemoteExecutionAuthorizer>(
    journal_path: impl Into<std::path::PathBuf>,
    authorizer: A,
    config: ProductionCloudAgentConfig,
    now_ms: u64,
) -> Result<
    crate::server::RemoteExecutionService<
        A,
        crate::cloud_agent_backend::CloudAgentBackend<ProductionCloudAgentExecutor>,
    >,
    ExecutionError,
> {
    crate::cloud_agent_backend::open_cloud_agent_service(
        journal_path,
        authorizer,
        ProductionCloudAgentExecutor::new(config)?,
        now_ms,
    )
}

fn deterministic_id(prefix: &str, identity: &RemoteRequestIdentity, extra: &str) -> String {
    let material = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        identity.account_fence,
        identity.account_epoch,
        identity.device_id,
        identity.operation_id,
        identity.request_id,
        extra,
    );
    let digest = Sha256::digest(material.as_bytes());
    format!(
        "{prefix}-{}",
        digest
            .iter()
            .take(16)
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn normalize_repo_url(raw: &str) -> Result<String, ExecutionError> {
    let raw = raw.trim();
    let candidate = if raw.contains("://") {
        raw.to_string()
    } else if let Some((head, tail)) = raw.split_once(':') {
        if !head.contains('/') && !head.chars().any(char::is_whitespace) && !tail.is_empty() {
            format!("ssh://{head}/{tail}")
        } else {
            raw.to_string()
        }
    } else {
        raw.to_string()
    };
    let mut url = Url::parse(&candidate).map_err(|error| {
        ExecutionError::InvalidRequest(format!("invalid CloudAgent repo URL: {error}"))
    })?;
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_query(None);
    url.set_fragment(None);
    Ok(url
        .to_string()
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .to_string())
}

fn system_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn create_cursor_checksum(machine_id: &str, now_ms: u64) -> String {
    let kilo_seconds = now_ms / 1_000_000;
    let mut bytes = [
        ((kilo_seconds >> 40) & 0xff) as u8,
        ((kilo_seconds >> 32) & 0xff) as u8,
        ((kilo_seconds >> 24) & 0xff) as u8,
        ((kilo_seconds >> 16) & 0xff) as u8,
        ((kilo_seconds >> 8) & 0xff) as u8,
        (kilo_seconds & 0xff) as u8,
    ];
    let mut last = 165_u8;
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = ((*byte ^ last).wrapping_add((index % 256) as u8)) & 0xff;
        last = *byte;
    }
    format!("{}{}", URL_SAFE_NO_PAD.encode(bytes), machine_id)
}

fn status_name(status: i32) -> &'static str {
    match status {
        1 => "running",
        2 => "finished",
        3 => "error",
        4 => "creating",
        5 => "expired",
        _ => "unknown",
    }
}

fn composer_json(value: BackgroundComposerWire) -> Value {
    json!({
        "agentId":value.bc_id,
        "name":value.name,
        "status":status_name(value.status),
        "branchName":value.branch_name,
        "prUrl":value.pr_url,
        "archived":value.is_archived,
        "filesChanged":value.files_changed,
        "linesAdded":value.lines_added,
        "linesRemoved":value.lines_removed,
        "commitCount":value.commit_count,
    })
}

fn detail_json(value: DetailedBackgroundComposerWire) -> Value {
    let error = value
        .permanent_error
        .and_then(|error| error.details)
        .map(|details| {
            json!({
                "title":details.title,
                "detail":details.detail,
                "additionalInfo":details.additional_info,
            })
        });
    json!({
        "status":status_name(value.status),
        "summary":value.summary,
        "composer":value.composer.map(composer_json),
        "error":error,
        "pullRequests":value.prs.into_iter().map(|pr| json!({
            "branchName":pr.branch_name,
            "pullNumber":pr.pull_number,
            "status":pr.pr_status,
            "url":pr.pr_url,
        })).collect::<Vec<_>>(),
    })
}

fn model_json(model: &SandModelCatalogEntry) -> Value {
    json!({
        "id":model.id,
        "displayName":model.display_name,
        "aliases":model.aliases,
        "params":model.params.iter().map(|parameter| json!({
            "id":parameter.id,
            "name":parameter.name,
            "type":match parameter.parameter_type {
                SandModelCatalogParameterType::Boolean => "boolean",
                SandModelCatalogParameterType::Enum => "enum",
            },
            "values":parameter.values.iter().map(|value| json!({
                "value":value.value,
                "displayName":value.display_name,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "variants":model.variants.iter().map(|variant| variant.iter().map(|value| json!({
            "id":value.id,
            "value":value.value,
        })).collect::<Vec<_>>()).collect::<Vec<_>>(),
    })
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_credentials_are_removed_before_cloud_launch() {
        assert_eq!(
            normalize_repo_url("https://user:secret@example.com/acme/repo.git?token=x#frag")
                .unwrap(),
            "https://example.com/acme/repo"
        );
    }

    #[test]
    fn checksum_is_stable_for_same_machine_and_time_bucket() {
        assert_eq!(
            create_cursor_checksum("machine-a", 1_728_000_000_000),
            create_cursor_checksum("machine-a", 1_728_000_000_999)
        );
        assert_ne!(
            create_cursor_checksum("machine-a", 1_728_000_000_000),
            create_cursor_checksum("machine-b", 1_728_000_000_000)
        );
    }
}
