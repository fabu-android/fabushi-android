use super::{
    AndroidRoutedToolBridge, DurableSubagentOwner, ProductionTurnAgentBuildBindings,
    ProductionTurnAgentOwner,
    ProductionTurnAgentStaticConfig, ProductionTurnEvent, ProductionTurnInput,
    ProductionTurnPrivacyMode, SubagentLaunch, SubagentRunOutcome,
    SubagentSessionSnapshot, SubagentToolBridge, SubagentToolContext, SAND_AGENT_TOKEN_LIMIT,
};
use crate::android_json_runtime::{AndroidHostMode, now_ms};
use super::{AndroidHostInferenceProvider, AndroidInferenceMode};
use serde_json::{json, Value};
use std::collections::{BTreeSet, VecDeque};
use std::sync::{
    atomic::AtomicBool,
    Arc, Mutex,
};
use std::thread;

struct ParentSubagentRoutedTools {
    mode: AndroidHostMode,
    bearer_token: Option<String>,
    owner: Arc<Mutex<DurableSubagentOwner>>,
    bridge: SubagentToolBridge,
    context: SubagentToolContext,
    events: Arc<Mutex<VecDeque<Value>>>,
    allowed_names: BTreeSet<String>,
}

impl AndroidRoutedToolBridge for ParentSubagentRoutedTools {
    fn list_tools(&self) -> Result<Vec<Value>, String> {
        Ok(self
            .bridge
            .tool_definitions(&self.context)
            .into_iter()
            .filter(|definition| {
                definition
                    .get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| self.allowed_names.contains(name))
            })
            .map(|definition| {
                json!({
                    "type":"function",
                    "name":definition.get("name").cloned().unwrap_or(Value::Null),
                    "description":definition.get("description").cloned().unwrap_or_else(|| Value::String(String::new())),
                    "parameters":definition.get("inputSchema").cloned().unwrap_or_else(|| json!({"type":"object"})),
                })
            })
            .collect())
    }

    fn call_tool(&self, name: &str, args: Value, tool_call_id: &str) -> Result<Value, String> {
        if !self.allowed_names.contains(name) {
            return Err(format!("parent subagent routed tool is unavailable for this turn: {name}"));
        }
        let result = self
            .bridge
            .call(name, &args, tool_call_id, &self.context, now_ms())?;
        let Some(launch) = result.launch else {
            return Ok(result.value);
        };
        if let Err(error) = spawn_generated_subagent(
            self.mode,
            self.bearer_token.clone(),
            Arc::clone(&self.owner),
            self.bridge.clone(),
            Arc::clone(&self.events),
            launch.clone(),
        ) {
            let epoch = self
                .owner
                .lock()
                .map_err(|_| "subagent owner lock poisoned".to_string())?
                .process_epoch();
            let _ = self
                .owner
                .lock()
                .map_err(|_| "subagent owner lock poisoned".to_string())?
                .settle(
                    &launch.record.subagent_id,
                    &self.context.account_fence,
                    epoch,
                    SubagentRunOutcome::Failed(error.clone()),
                    now_ms(),
                );
            return Err(error);
        }
        Ok(result.value)
    }
}

pub fn build_parent_subagent_routed_tools(
    mode: AndroidHostMode,
    bearer_token: Option<String>,
    owner: Arc<Mutex<DurableSubagentOwner>>,
    bridge: SubagentToolBridge,
    events: Arc<Mutex<VecDeque<Value>>>,
    context: SubagentToolContext,
) -> Arc<dyn AndroidRoutedToolBridge> {
    let allowed_names = context
        .frozen_turn
        .tool_names
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    Arc::new(ParentSubagentRoutedTools {
        mode,
        bearer_token,
        owner,
        bridge,
        context,
        events,
        allowed_names,
    })
}

struct GeneratedSubagentRoutedTools {
    bridge: SubagentToolBridge,
    context: SubagentToolContext,
    allowed_names: BTreeSet<String>,
}

impl AndroidRoutedToolBridge for GeneratedSubagentRoutedTools {
    fn list_tools(&self) -> Result<Vec<Value>, String> {
        Ok(self
            .bridge
            .tool_definitions(&self.context)
            .into_iter()
            .filter(|definition| {
                definition
                    .get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| self.allowed_names.contains(name))
            })
            .map(|definition| {
                json!({
                    "type":"function",
                    "name":definition.get("name").cloned().unwrap_or(Value::Null),
                    "description":definition.get("description").cloned().unwrap_or_else(|| Value::String(String::new())),
                    "parameters":definition.get("inputSchema").cloned().unwrap_or_else(|| json!({"type":"object"})),
                })
            })
            .collect())
    }

    fn call_tool(&self, name: &str, args: Value, tool_call_id: &str) -> Result<Value, String> {
        if !self.allowed_names.contains(name) {
            return Err(format!("generated subagent routed tool is unavailable for this turn: {name}"));
        }
        let result = self
            .bridge
            .call(name, &args, tool_call_id, &self.context, now_ms())?;
        if result.launch.is_some() {
            return Err("generated subagent routed tool attempted an unprojected child launch".into());
        }
        Ok(result.value)
    }
}

pub fn spawn_generated_subagent(
    mode: AndroidHostMode,
    bearer_token: Option<String>,
    owner: Arc<Mutex<DurableSubagentOwner>>,
    subagent_tools: SubagentToolBridge,
    events: Arc<Mutex<VecDeque<Value>>>,
    launch: SubagentLaunch,
) -> Result<(), String> {
    let record = launch.record;
    let subagent_id = record.subagent_id.clone();
    let request_id = record.subagent_request_id.clone();
    let account_fence = record.account_fence.clone();
    let Some(frozen_turn) = record.frozen_turn.clone() else {
        return Err("generated subagent launch is missing frozen turn configuration".into());
    };
    if frozen_turn.provider_id != "android-host-inference"
        || frozen_turn.model_id.trim().is_empty()
        || frozen_turn.summarization_binding_id != "android-host-inference:same-provider"
    {
        return Err("generated subagent frozen provider configuration is unsupported".into());
    }
    let model = frozen_turn.model_id.clone();
    let mut child_projection = frozen_turn.clone();
    child_projection.allowed_subagent_types.clear();
    let routed_tools: Arc<dyn AndroidRoutedToolBridge> = Arc::new(GeneratedSubagentRoutedTools {
        bridge: subagent_tools,
        context: SubagentToolContext {
            parent_agent_id: record.parent_agent_id.clone(),
            parent_request_id: request_id.clone(),
            root_parent_request_id: record.lineage.root_parent_request_id.clone(),
            account_fence: account_fence.clone(),
            box_id: record.box_id.clone(),
            quiet_origin: record.quiet_origin.clone(),
            frozen_turn: child_projection,
        },
        allowed_names: frozen_turn.tool_names.iter().cloned().collect(),
    });

    thread::Builder::new()
        .name(format!(
            "fabushi-subagent-{}",
            subagent_id.chars().filter(|value| value.is_ascii_alphanumeric()).take(24).collect::<String>()
        ))
        .spawn(move || {
            let mut prompt = record.prompt.clone();
            loop {
                let callback_epoch = match owner.lock() {
                    Ok(runtime) => runtime.process_epoch(),
                    Err(_) => return,
                };
                let cancelled = Arc::new(AtomicBool::new(false));
                if owner
                    .lock()
                    .ok()
                    .and_then(|mut runtime| runtime.attach_control(&subagent_id, Arc::clone(&cancelled)).ok())
                    .is_none()
                {
                    return;
                }

                let provider = match mode {
                    AndroidHostMode::Test => AndroidHostInferenceProvider::new(AndroidInferenceMode::Test)
                        .with_routed_tools(Arc::clone(&routed_tools)),
                    AndroidHostMode::Production => {
                        let Some(token) = bearer_token.clone() else {
                            settle_and_publish(
                                &owner,
                                &events,
                                &subagent_id,
                                &account_fence,
                                callback_epoch,
                                SubagentRunOutcome::Failed("provider_credentials_unavailable".into()),
                            );
                            return;
                        };
                        match AndroidHostInferenceProvider::production(token, Arc::clone(&cancelled)) {
                            Ok(provider) => provider.with_routed_tools(Arc::clone(&routed_tools)),
                            Err(error) => {
                                settle_and_publish(
                                    &owner,
                                    &events,
                                    &subagent_id,
                                    &account_fence,
                                    callback_epoch,
                                    SubagentRunOutcome::Failed(error.message),
                                );
                                return;
                            }
                        }
                    }
                };

                let summarization_token = bearer_token.clone();
                let summarization_cancelled = Arc::clone(&cancelled);
                let summarization_mode = match mode {
                    AndroidHostMode::Test => AndroidInferenceMode::Test,
                    AndroidHostMode::Production => AndroidInferenceMode::Production,
                };
                let summarization: super::ProductionTurnSummarizationPrompt = Arc::new(
                    move |system, user, should_cancel| {
                        AndroidHostInferenceProvider::run_summarization_prompt_with_model(
                            summarization_mode,
                            summarization_token.clone(),
                            Arc::clone(&summarization_cancelled),
                            &model,
                            system,
                            user,
                            should_cancel,
                        )
                    },
                );
                let frozen_privacy = frozen_turn.privacy_mode.clone();
                let privacy = Arc::new(move || {
                    Some(match frozen_privacy.as_str() {
                        "no-storage" => ProductionTurnPrivacyMode::NoStorage,
                        "no-training" => ProductionTurnPrivacyMode::NoTraining,
                        "usage-data-training-allowed" => {
                            ProductionTurnPrivacyMode::UsageDataTrainingAllowed
                        }
                        "usage-codebase-training-allowed" => {
                            ProductionTurnPrivacyMode::UsageCodebaseTrainingAllowed
                        }
                        _ => ProductionTurnPrivacyMode::Unspecified,
                    })
                });
                let build = ProductionTurnAgentBuildBindings::new(
                    ProductionTurnAgentStaticConfig {
                        model_id: model.clone(),
                        agent_token_limit: SAND_AGENT_TOKEN_LIMIT,
                        conversation_id: subagent_id.clone(),
                        is_box_scoped_subagent: true,
                        is_subagent_runner: true,
                        is_shared_room_runner: false,
                        sand_send_message_delivery_owed: false,
                        transcripts_folder_available: false,
                    },
                    privacy,
                    summarization,
                );

                let mut turn = ProductionTurnAgentOwner::new(provider).with_build_bindings(build);
                let mut output = String::new();
                let result = turn.run_with_event_sink(
                    ProductionTurnInput {
                        operation_id: subagent_id.clone(),
                        request_id: request_id.clone(),
                        agent_id: subagent_id.clone(),
                        model: model.clone(),
                        prompt: prompt.clone(),
                        resume_checkpoint_available: false,
                    },
                    &mut |event| {
                        if let ProductionTurnEvent::Delta(delta) = event {
                            output.push_str(&delta);
                        }
                        Ok(())
                    },
                );

                if let Ok(mut runtime) = owner.lock() {
                    let mut snapshot = runtime
                        .get(&subagent_id)
                        .map(|record| record.snapshot.clone())
                        .unwrap_or_default();
                    snapshot.recent_activity.push(match &result {
                        Ok(_) => "Generated subagent inference completed.".into(),
                        Err(error) => format!("Generated subagent inference stopped: {}", error.message),
                    });
                    if snapshot.recent_activity.len() > 20 {
                        let drain = snapshot.recent_activity.len() - 20;
                        snapshot.recent_activity.drain(0..drain);
                    }
                    let _ = runtime.update_snapshot(
                        &subagent_id,
                        &account_fence,
                        callback_epoch,
                        SubagentSessionSnapshot { ..snapshot },
                        now_ms(),
                    );
                }

                let outcome = match result {
                    Ok(_) => SubagentRunOutcome::Completed(output),
                    Err(error) => SubagentRunOutcome::Failed(error.message),
                };
                let settlement = match owner.lock() {
                    Ok(mut runtime) => runtime.settle(
                        &subagent_id,
                        &account_fence,
                        callback_epoch,
                        outcome,
                        now_ms(),
                    ),
                    Err(_) => return,
                };
                let Ok(settlement) = settlement else { return; };
                if settlement.ignored_stale_callback {
                    return;
                }
                if let Some(continuation) = settlement.continuation {
                    prompt = continuation.prompt;
                    continue;
                }
                publish_settlement(&events, settlement);
                return;
            }
        })
        .map(|_| ())
        .map_err(|error| format!("failed to spawn generated subagent worker: {error}"))
}

fn settle_and_publish(
    owner: &Arc<Mutex<DurableSubagentOwner>>,
    events: &Arc<Mutex<VecDeque<Value>>>,
    subagent_id: &str,
    account_fence: &str,
    callback_epoch: u64,
    outcome: SubagentRunOutcome,
) {
    let settlement = owner.lock().ok().and_then(|mut runtime| {
        runtime
            .settle(subagent_id, account_fence, callback_epoch, outcome, now_ms())
            .ok()
    });
    if let Some(settlement) = settlement {
        publish_settlement(events, settlement);
    }
}

fn publish_settlement(
    events: &Arc<Mutex<VecDeque<Value>>>,
    settlement: super::SubagentSettlement,
) {
    let Some(record) = settlement.completion else {
        return;
    };
    let family = match record.status {
        super::SubagentStatus::Completed => "subagent.completed",
        super::SubagentStatus::Failed => "subagent.failed",
        super::SubagentStatus::Aborted => "subagent.aborted",
        super::SubagentStatus::OutcomeUnknown => "subagent.outcome-unknown",
        super::SubagentStatus::Running => "subagent.running",
    };
    if let Ok(mut queue) = events.lock() {
        queue.push_back(json!({
            "type": family,
            "subagentId": record.subagent_id,
            "subagentRequestId": record.subagent_request_id,
            "parentAgentId": record.parent_agent_id,
            "parentRequestId": record.lineage.parent_request_id,
            "rootParentRequestId": record.lineage.root_parent_request_id,
            "toolCallId": record.tool_call_id,
            "subagentType": record.subagent_type,
            "status": super::status_label(record.status),
            "result": record.completion_result,
            "error": record.completion_error,
        }));
        if let Some(usage) = settlement.computer_use_usage {
            queue.push_back(json!({
                "type":"subagent.computer-use.usage",
                "usage":usage,
            }));
        }
        if let Some(audit) = settlement.computer_use_audit {
            queue.push_back(json!({
                "type":"subagent.computer-use.audit",
                "audit":audit,
            }));
        }
    }
}
