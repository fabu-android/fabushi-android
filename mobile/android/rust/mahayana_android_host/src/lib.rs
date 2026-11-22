//! `mahayana_android_host` (`libmahayana_app_host.so`)
//!
//! Unified Native Rust Host & GPUI-Telegram + Mika Engine for Fabushi Android.
//! Exports unconditional JNI symbols for `com.ombhrum.fabushi.core.MahayanaHost` on both
//! Android (`aarch64-linux-android`, `x86_64-linux-android`) and Linux host JVM
//! (`x86_64-unknown-linux-gnu`), plus C FFI entrypoints.

pub mod coding_agent;
pub mod computer;
pub mod e2ee;
pub mod fabushi_miniapp;
pub mod hermes;
pub mod magpie;
pub mod messaging;
pub mod telegram_ext;
pub mod user_vault;
pub mod wallet;
pub mod waveform;

use crate::waveform::VadBargeInDetector;
use jni::objects::{JObject, JString};
use jni::sys::{jlong, jstring};
use jni::JNIEnv;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::ffi::{CStr, CString};
use std::fs;
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

static NEXT_HANDLE: AtomicI64 = AtomicI64::new(1);
static HOST_REGISTRY: LazyLock<Mutex<HashMap<i64, Arc<MahayanaAndroidHost>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub struct MahayanaAndroidHost {
    app_data_dir: PathBuf,
    test_mode: bool,
    db: Mutex<Connection>,
    events: Mutex<VecDeque<Value>>,
    vad: Mutex<VadBargeInDetector>,
}

impl MahayanaAndroidHost {
    pub fn new(app_data_dir: impl AsRef<Path>, test_mode: bool) -> Result<Self, String> {
        let root = app_data_dir.as_ref().to_path_buf();
        fs::create_dir_all(&root)
            .map_err(|e| format!("Failed creating app_data_dir '{}': {e}", root.display()))?;

        let db_file = if test_mode {
            root.join(format!(
                "runtime_test_{}.sqlite3",
                uuid::Uuid::new_v4().simple()
            ))
        } else {
            root.join("runtime.sqlite3")
        };

        let conn = Connection::open(&db_file)
            .map_err(|e| format!("Failed opening SQLite '{}': {e}", db_file.display()))?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )
        .map_err(|e| format!("Failed configuring SQLite WAL pragmas: {e}"))?;

        init_host_schema(&conn, test_mode)?;
        messaging::init_schema(&conn, test_mode)?;
        e2ee::init_schema(&conn)?;
        wallet::init_schema(&conn)?;
        fabushi_miniapp::init_schema(&conn)?;
        hermes::init_schema(&conn)?;
        magpie::init_schema(&conn)?;
        computer::init_schema(&conn)?;
        telegram_ext::init_schema(&conn)?;
        user_vault::init_schema(&conn)?;
        coding_agent::init_schema(&conn)?;

        Ok(Self {
            app_data_dir: root,
            test_mode,
            db: Mutex::new(conn),
            events: Mutex::new(VecDeque::with_capacity(128)),
            vad: Mutex::new(VadBargeInDetector::default()),
        })
    }

    pub fn app_data_dir(&self) -> &Path {
        &self.app_data_dir
    }

    pub fn is_test_mode(&self) -> bool {
        self.test_mode
    }

    /// Dispatches a serialized JSON request (`{"method": "...", "params": {...}}`)
    /// and returns a serialized JSON envelope (`{"ok": true, "result": ...}` or `{"ok": false, "error": ...}`).
    pub fn dispatch_json(&self, request_json: &str) -> String {
        let parsed: Value = match serde_json::from_str(request_json) {
            Ok(v) => v,
            Err(e) => {
                return json!({
                    "ok": false,
                    "error": format!("Invalid request JSON: {e}")
                })
                .to_string();
            }
        };
        let method = match parsed.get("method").and_then(Value::as_str) {
            Some(m) if !m.trim().is_empty() => m.trim(),
            _ => {
                return json!({
                    "ok": false,
                    "error": "Request missing non-empty 'method'"
                })
                .to_string();
            }
        };
        let params = parsed.get("params").cloned().unwrap_or_else(|| json!({}));

        match self.dispatch(method, &params) {
            Ok(result) => json!({
                "ok": true,
                "result": result
            })
            .to_string(),
            Err(err) => json!({
                "ok": false,
                "error": err
            })
            .to_string(),
        }
    }

    pub fn dispatch(&self, method: &str, params: &Value) -> Result<Value, String> {
        let conn = self
            .db
            .lock()
            .map_err(|_| "Host SQLite mutex poisoned".to_string())?;

        if let Some(sub) = method
            .strip_prefix("telegram.e2ee.")
            .or_else(|| method.strip_prefix("e2ee."))
        {
            return e2ee::handle_rpc(&conn, sub, params);
        }
        if let Some(sub) = method.strip_prefix("telegram.auth.") {
            return telegram_ext::handle_auth_rpc(&conn, sub, params);
        }
        if let Some(sub) = method.strip_prefix("telegram.messaging.") {
            return telegram_ext::handle_messaging_ext_rpc(&conn, sub, params);
        }
        if let Some(sub) = method
            .strip_prefix("telegram.calls.")
            .or_else(|| method.strip_prefix("calls."))
        {
            return telegram_ext::handle_calls_rpc(&conn, &self.vad, sub, params);
        }
        if let Some(sub) = method.strip_prefix("telegram.server.") {
            return telegram_ext::handle_server_rpc(&conn, sub, params);
        }
        if let Some(sub) = method.strip_prefix("wallet.") {
            return wallet::handle_rpc(&conn, sub, params);
        }
        if let Some(sub) = method.strip_prefix("fabushiMiniapp.") {
            return fabushi_miniapp::handle_rpc(&conn, sub, params);
        }
        if let Some(sub) = method.strip_prefix("miniapp.webmcp.") {
            return fabushi_miniapp::handle_webmcp_rpc(&conn, sub, params);
        }
        if let Some(sub) = method.strip_prefix("hermes.") {
            return hermes::handle_rpc(&conn, sub, params);
        }
        if let Some(sub) = method.strip_prefix("magpie.") {
            return magpie::handle_rpc(&conn, sub, params);
        }
        if let Some(sub) = method.strip_prefix("computer.") {
            return computer::handle_computer_rpc(&conn, sub, params);
        }
        if let Some(sub) = method.strip_prefix("remote.") {
            return computer::handle_remote_rpc(&conn, sub, params);
        }
        if let Some(sub) = method.strip_prefix("userVault.") {
            return user_vault::handle_rpc(&conn, sub, params);
        }
        if let Some(sub) = method.strip_prefix("codingAgent.") {
            return coding_agent::handle_rpc(&conn, sub, params);
        }
        if let Some(sub) = method.strip_prefix("mika.") {
            return computer::handle_mika_rpc(&conn, sub, params);
        }

        match method {
            "feature.info" => Ok(json!({
                "platform": "android",
                "protocolVersion": "2",
                "runtimeVersion": if self.test_mode { "2.0.0-test" } else { "2.0.0" },
                "desktopParityVersion": "1.6.2",
                "desktopParityCommit": "9542524",
                "engine": "mahayana-gpui-telegram-mika",
                "sqliteWal": true
            })),

            // Auth family
            "feature.auth.status" => auth_status(&conn),
            "feature.auth.providers" => Ok(auth_providers()),
            "feature.auth.oauthStart" => auth_oauth_start(&conn, params),
            "feature.auth.oauthPoll" => auth_oauth_poll(&conn, params),
            "feature.auth.browserStart" => auth_browser_start(&conn, params),
            "feature.auth.browserReopen" => auth_browser_reopen(&conn, params),
            "feature.auth.browserCancel" => auth_browser_cancel(&conn, params),
            "feature.auth.browserPoll" => auth_browser_poll(&conn, params),
            "feature.auth.passwordLogin" => auth_password_login(&conn, params),
            "feature.auth.logout" => {
                if let Ok(mut q) = self.events.lock() {
                    q.clear();
                }
                auth_logout(&conn)
            }
            "feature.auth.deviceAgentSession" => auth_device_agent_session(&conn),

            // Feature execution / streaming event bus / approvals / interrupts
            "feature.execute" => self.handle_feature_execute(&conn, params),
            "feature.receive" => self.handle_feature_receive(),
            "feature.interrupt" => self.handle_feature_interrupt(&conn, params),
            "feature.approval.resolve" => self.handle_approval_resolve(&conn, params),

            // Persistence KV store (SQLite WAL)
            "feature.persistence.set" => persistence_set(&conn, params),
            "feature.persistence.get" => persistence_get(&conn, params),
            "feature.persistence.remove" => persistence_remove(&conn, params),
            "feature.persistence.list" => persistence_list(&conn, params),

            // Marketplace, plugins, portable runtime & platform control plane
            "feature.marketplace.browse" => marketplace_browse(&conn, params),
            "feature.marketplace.release" => marketplace_release(params),
            "feature.marketplace.add" => marketplace_add_plugin(&conn, params),
            "feature.plugin.install" => plugin_install(&conn, params),
            "feature.plugin.uninstall" => plugin_uninstall(&conn, params),
            "feature.plugin.listInstalled" => plugin_list_installed(&conn),
            "feature.plugin.uiDocument" => plugin_ui_document(params),
            "plugin.permission.grant" => plugin_permission_grant(&conn, params),
            "plugin.compatibility" => plugin_compatibility(params),
            "runtime.start" => runtime_start(&conn, params),
            "runtime.call" => runtime_call(&conn, params),
            "platform.request" => platform_request(&conn, params),

            // Messaging Protocol v2
            "feature.messaging.access.issue" => {
                messaging::issue_access(&conn, self.test_mode, params)
            }
            "feature.messaging.execute" => {
                messaging::execute_command(&conn, &self.app_data_dir, self.test_mode, params)
            }
            "feature.messaging.blob.read" => {
                messaging::read_blob(&conn, &self.app_data_dir, params)
            }

            other => Err(format!("Unsupported MahayanaHost method: {other}")),
        }
    }

    fn push_event(&self, event: Value) {
        if let Ok(mut q) = self.events.lock() {
            if q.len() >= 1024 {
                q.pop_front();
            }
            q.push_back(event);
        }
    }

    fn handle_feature_receive(&self) -> Result<Value, String> {
        let mut q = self
            .events
            .lock()
            .map_err(|_| "Event queue mutex poisoned".to_string())?;
        Ok(q.pop_front().unwrap_or_else(|| json!({ "type": "" })))
    }

    fn handle_feature_execute(&self, conn: &Connection, params: &Value) -> Result<Value, String> {
        let now = chrono::Utc::now().timestamp_millis();
        let command = params.get("command").unwrap_or(params);
        let cmd_type = command
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| "feature.execute command missing 'type'".to_string())?;
        let request_id = command
            .get("requestId")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .map(String::from)
            .unwrap_or_else(|| format!("req_{}", uuid::Uuid::new_v4().simple()));
        let operation_id = format!("op_{request_id}");

        match cmd_type {
            "chat.send" => {
                let text = command
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim();
                let agent_id = command
                    .get("agentId")
                    .and_then(Value::as_str)
                    .unwrap_or("mahayana-assistant");
                let mode = command
                    .get("mode")
                    .and_then(Value::as_str)
                    .unwrap_or("agent");

                if text.is_empty() {
                    return Err("chat.send requires non-empty 'text'".into());
                }

                if text.starts_with('/') {
                    let slash = hermes::handle_rpc(
                        conn,
                        "runSlashCommand",
                        &json!({ "input": text }),
                    )?;
                    let reply_text = slash["summary"]
                        .as_str()
                        .unwrap_or("Executed slash command")
                        .to_string();
                    conn.execute(
                        "INSERT OR REPLACE INTO host_operations (operation_id, request_id, kind, status, updated_at_ms)
                         VALUES (?1, ?2, 'chat.send', 'completed', ?3)",
                        params![operation_id, request_id, now],
                    )
                    .map_err(|e| e.to_string())?;
                    self.push_event(json!({
                        "type": "operation.started",
                        "operationId": operation_id,
                        "requestId": request_id,
                        "label": "执行斜杠指令"
                    }));
                    self.push_event(json!({
                        "type": "model.routed",
                        "operationId": operation_id,
                        "provider": "hermes-slash",
                        "model": "slash-command",
                        "mode": mode
                    }));
                    self.push_event(json!({
                        "type": "agent.step",
                        "operationId": operation_id,
                        "stepId": "step-slash-command",
                        "title": slash["command"],
                        "detail": slash["category"],
                        "status": "completed"
                    }));
                    self.push_event(json!({
                        "type": "chat.delta",
                        "operationId": operation_id,
                        "delta": reply_text
                    }));
                    self.push_event(json!({
                        "type": "chat.message",
                        "operationId": operation_id,
                        "role": "assistant",
                        "text": reply_text
                    }));
                    self.push_event(json!({
                        "type": "operation.completed",
                        "operationId": operation_id,
                        "requestId": request_id
                    }));
                } else {
                    let system_prompt = if agent_id.contains("mika") {
                        "You are Mika, the Fabushi Android on-device accessibility & task assistant."
                    } else {
                        "You are the Fabushi Android assistant powered by Magpie & Hermes."
                    };
                    let req = magpie::UnifiedRequest {
                        model: command
                            .get("model")
                            .and_then(Value::as_str)
                            .unwrap_or("auto")
                            .to_string(),
                        system_prompt: system_prompt.to_string(),
                        messages: vec![magpie::UnifiedMessage {
                            role: magpie::UnifiedRole::User,
                            content: text.to_string(),
                            name: None,
                            tool_call_id: None,
                            tool_calls: Vec::new(),
                            reasoning: None,
                        }],
                        tools: Vec::new(),
                        temperature: None,
                        max_tokens: None,
                        thinking_budget_tokens: None,
                        stream: true,
                    };

                    let mut streamed_deltas = Vec::<String>::new();
                    match magpie::execute_live_request(
                        conn,
                        &req,
                        None,
                        command.get("providerId").and_then(Value::as_str),
                        Some(&request_id),
                        |delta| {
                            if !delta.is_empty() {
                                streamed_deltas.push(delta.to_string());
                            }
                        },
                    ) {
                        Ok(outcome) => {
                            conn.execute(
                                "INSERT OR REPLACE INTO host_operations (operation_id, request_id, kind, status, updated_at_ms)
                                 VALUES (?1, ?2, 'chat.send', 'completed', ?3)",
                                params![operation_id, request_id, now],
                            )
                            .map_err(|e| e.to_string())?;

                            self.push_event(json!({
                                "type": "operation.started",
                                "operationId": operation_id,
                                "requestId": request_id,
                                "label": "正在流式生成"
                            }));
                            self.push_event(json!({
                                "type": "model.routed",
                                "operationId": operation_id,
                                "provider": outcome.provider_id,
                                "model": outcome.model,
                                "protocol": outcome.protocol,
                                "mode": mode
                            }));
                            self.push_event(json!({
                                "type": "agent.step",
                                "operationId": operation_id,
                                "stepId": "step-magpie-live",
                                "title": format!("Magpie live completion via {}", outcome.provider_id),
                                "detail": format!("{} ({} ms)", outcome.model, outcome.latency_ms),
                                "status": "completed"
                            }));
                            for d in &streamed_deltas {
                                self.push_event(json!({
                                    "type": "chat.delta",
                                    "operationId": operation_id,
                                    "delta": d
                                }));
                            }
                            self.push_event(json!({
                                "type": "chat.message",
                                "operationId": operation_id,
                                "role": "assistant",
                                "text": outcome.reply,
                                "provider": outcome.provider_id,
                                "model": outcome.model,
                                "inputTokens": outcome.usage.input_tokens,
                                "outputTokens": outcome.usage.output_tokens,
                                "costUsd": outcome.cost_usd
                            }));
                            self.push_event(json!({
                                "type": "operation.completed",
                                "operationId": operation_id,
                                "requestId": request_id
                            }));
                        }
                        Err(err) => {
                            let code = if err.starts_with("PROVIDER_NOT_CONFIGURED") {
                                "PROVIDER_NOT_CONFIGURED"
                            } else {
                                "UPSTREAM_UNAVAILABLE"
                            };
                            conn.execute(
                                "INSERT OR REPLACE INTO host_operations (operation_id, request_id, kind, status, updated_at_ms)
                                 VALUES (?1, ?2, 'chat.send', 'failed', ?3)",
                                params![operation_id, request_id, now],
                            )
                            .map_err(|e| e.to_string())?;

                            self.push_event(json!({
                                "type": "operation.started",
                                "operationId": operation_id,
                                "requestId": request_id,
                                "label": "模型调用失败"
                            }));
                            self.push_event(json!({
                                "type": "operation.failed",
                                "operationId": operation_id,
                                "requestId": request_id,
                                "code": code,
                                "error": err
                            }));
                            return Err(err);
                        }
                    }
                }
            }

            "bot.list" => {
                let bots = collect_surface_bots(conn)?;
                self.push_event(json!({
                    "type": "bot.listed",
                    "operationId": operation_id,
                    "requestId": request_id,
                    "bots": bots
                }));
            }

            "bot.create" => {
                let name = command
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("Custom Bot")
                    .trim();
                let description = command
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim();
                let bot_id = format!("bot-{}", uuid::Uuid::new_v4().simple());
                conn.execute(
                    "INSERT INTO host_bots (bot_id, name, description, mini_app_id, created_at_ms)
                     VALUES (?1, ?2, ?3, NULL, ?4)",
                    params![bot_id, name, description, now],
                )
                .map_err(|e| e.to_string())?;
                self.push_event(json!({
                    "type": "bot.created",
                    "operationId": operation_id,
                    "requestId": request_id,
                    "bot": {
                        "id": bot_id,
                        "name": name,
                        "description": description
                    }
                }));
                self.push_event(json!({
                    "type": "operation.completed",
                    "operationId": operation_id,
                    "requestId": request_id
                }));
            }

            "marketplace.install" => {
                let plugin_id = command
                    .get("miniAppId")
                    .or_else(|| command.get("pluginId"))
                    .and_then(Value::as_str)
                    .unwrap_or("global-dharma");
                record_plugin_install(conn, plugin_id, "2.0.0", "cordis-js", now)?;
                self.push_event(json!({
                    "type": "operation.started",
                    "operationId": operation_id,
                    "requestId": request_id,
                    "label": format!("Installing {plugin_id}")
                }));
                self.push_event(json!({
                    "type": "miniapp.installed",
                    "operationId": operation_id,
                    "requestId": request_id,
                    "miniAppId": plugin_id,
                    "pluginId": plugin_id
                }));
                self.push_event(json!({
                    "type": "operation.completed",
                    "operationId": operation_id,
                    "requestId": request_id
                }));
            }

            "miniapp.open" => {
                let plugin_id = command
                    .get("miniAppId")
                    .or_else(|| command.get("pluginId"))
                    .and_then(Value::as_str)
                    .unwrap_or("global-dharma");
                self.push_event(json!({
                    "type": "miniapp.opened",
                    "operationId": operation_id,
                    "requestId": request_id,
                    "miniAppId": plugin_id
                }));
                self.push_event(json!({
                    "type": "operation.completed",
                    "operationId": operation_id,
                    "requestId": request_id
                }));
            }

            "capability.request" => {
                let mini_app_id = command
                    .get("miniAppId")
                    .and_then(Value::as_str)
                    .unwrap_or("global-dharma");
                let capability = command
                    .get("capability")
                    .and_then(Value::as_str)
                    .unwrap_or("camera");
                let reason = command
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let approval_id = format!("apr_{}", uuid::Uuid::new_v4().simple());

                conn.execute(
                    "INSERT INTO host_approvals
                     (approval_id, operation_id, mini_app_id, capability, reason, decision, updated_at_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6)",
                    params![approval_id, operation_id, mini_app_id, capability, reason, now],
                )
                .map_err(|e| e.to_string())?;

                self.push_event(json!({
                    "type": "approval.requested",
                    "approvalId": approval_id,
                    "operationId": operation_id,
                    "requestId": request_id,
                    "miniAppId": mini_app_id,
                    "capability": capability,
                    "reason": reason
                }));
            }

            "runtime.longTask" => {
                let label = command
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or("Long background operation");
                conn.execute(
                    "INSERT OR REPLACE INTO host_operations
                     (operation_id, request_id, kind, status, updated_at_ms)
                     VALUES (?1, ?2, 'runtime.longTask', 'running', ?3)",
                    params![operation_id, request_id, now],
                )
                .map_err(|e| e.to_string())?;
                self.push_event(json!({
                    "type": "operation.started",
                    "operationId": operation_id,
                    "requestId": request_id,
                    "label": label
                }));
            }

            "session.clear" => {
                if let Ok(mut q) = self.events.lock() {
                    q.clear();
                }
                self.push_event(json!({
                    "type": "session.cleared",
                    "operationId": operation_id,
                    "requestId": request_id
                }));
                self.push_event(json!({
                    "type": "operation.completed",
                    "operationId": operation_id,
                    "requestId": request_id
                }));
            }

            other => {
                return Err(format!("Unsupported feature.execute command type: {other}"));
            }
        }

        Ok(json!({
            "accepted": true,
            "requestId": request_id,
            "operationId": operation_id
        }))
    }

    fn handle_feature_interrupt(&self, conn: &Connection, params: &Value) -> Result<Value, String> {
        let now = chrono::Utc::now().timestamp_millis();
        let operation_id = params
            .get("operationId")
            .and_then(Value::as_str)
            .ok_or_else(|| "feature.interrupt requires 'operationId'".to_string())?;

        conn.execute(
            "UPDATE host_operations SET status = 'interrupted', updated_at_ms = ?1 WHERE operation_id = ?2",
            params![now, operation_id],
        )
        .map_err(|e| e.to_string())?;

        if let Ok(mut q) = self.events.lock() {
            q.retain(|ev| {
                let same_op = ev.get("operationId").and_then(Value::as_str) == Some(operation_id);
                let is_delta_or_done = matches!(
                    ev.get("type").and_then(Value::as_str),
                    Some("chat.delta" | "operation.completed")
                );
                !(same_op && is_delta_or_done)
            });
            q.push_back(json!({
                "type": "operation.interrupted",
                "operationId": operation_id
            }));
        }

        Ok(json!({
            "interrupted": true,
            "operationId": operation_id
        }))
    }

    fn handle_approval_resolve(&self, conn: &Connection, params: &Value) -> Result<Value, String> {
        let now = chrono::Utc::now().timestamp_millis();
        let resolution = params.get("resolution").unwrap_or(params);
        let approval_id = resolution
            .get("approvalId")
            .and_then(Value::as_str)
            .ok_or_else(|| "feature.approval.resolve requires 'approvalId'".to_string())?;
        let decision = resolution
            .get("decision")
            .and_then(Value::as_str)
            .unwrap_or("allow-once");

        let op_id: Option<String> = conn
            .query_row(
                "SELECT operation_id FROM host_approvals WHERE approval_id = ?1",
                params![approval_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;

        conn.execute(
            "UPDATE host_approvals SET decision = ?1, updated_at_ms = ?2 WHERE approval_id = ?3",
            params![decision, now, approval_id],
        )
        .map_err(|e| e.to_string())?;

        let operation_id = op_id.unwrap_or_else(|| format!("op_{approval_id}"));
        self.push_event(json!({
            "type": "approval.resolved",
            "approvalId": approval_id,
            "operationId": operation_id,
            "decision": decision
        }));
        self.push_event(json!({
            "type": "operation.completed",
            "operationId": operation_id
        }));

        Ok(json!({
            "resolved": true,
            "approvalId": approval_id,
            "operationId": operation_id,
            "decision": decision
        }))
    }
}

// ============================================================================
// Handle Registry, JNI Entrypoints & C ABI Entrypoints
// ============================================================================

fn register_host(app_data_dir: &str, test_mode: bool) -> i64 {
    let dir = if app_data_dir.trim().is_empty() {
        std::env::temp_dir()
            .join("mahayana_android_host_default")
            .to_string_lossy()
            .to_string()
    } else {
        app_data_dir.to_string()
    };

    match MahayanaAndroidHost::new(&dir, test_mode) {
        Ok(host) => {
            let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
            if let Ok(mut reg) = HOST_REGISTRY.lock() {
                reg.insert(handle, Arc::new(host));
                handle
            } else {
                0
            }
        }
        Err(_) => 0,
    }
}

fn dispatch_by_handle(handle: i64, request_json: &str) -> String {
    let host = HOST_REGISTRY
        .lock()
        .ok()
        .and_then(|reg| reg.get(&handle).cloned());
    match host {
        Some(h) => h.dispatch_json(request_json),
        None => json!({
            "ok": false,
            "error": format!("Invalid or closed MahayanaHost handle: {handle}")
        })
        .to_string(),
    }
}

fn destroy_by_handle(handle: i64) {
    if let Ok(mut reg) = HOST_REGISTRY.lock() {
        reg.remove(&handle);
    }
}

#[no_mangle]
pub extern "system" fn Java_com_ombhrum_fabushi_core_MahayanaHost_nativeCreate(
    mut env: JNIEnv,
    _obj: JObject,
    app_data_dir: JString,
) -> jlong {
    catch_unwind(AssertUnwindSafe(|| {
        let dir: String = match env.get_string(&app_data_dir) {
            Ok(s) => s.into(),
            Err(_) => return 0,
        };
        register_host(&dir, false)
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub extern "system" fn Java_com_ombhrum_fabushi_core_MahayanaHost_nativeCreateTest(
    mut env: JNIEnv,
    _obj: JObject,
    app_data_dir: JString,
) -> jlong {
    catch_unwind(AssertUnwindSafe(|| {
        let dir: String = match env.get_string(&app_data_dir) {
            Ok(s) => s.into(),
            Err(_) => return 0,
        };
        register_host(&dir, true)
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub extern "system" fn Java_com_ombhrum_fabushi_core_MahayanaHost_nativeDispatch(
    mut env: JNIEnv,
    _obj: JObject,
    handle: jlong,
    request_json: JString,
) -> jstring {
    let response = catch_unwind(AssertUnwindSafe(|| {
        let req: String = match env.get_string(&request_json) {
            Ok(s) => s.into(),
            Err(e) => {
                return json!({
                    "ok": false,
                    "error": format!("JNI invalid request_json string: {e}")
                })
                .to_string();
            }
        };
        dispatch_by_handle(handle, &req)
    }))
    .unwrap_or_else(|_| {
        json!({
            "ok": false,
            "error": "Native host panic caught across JNI boundary"
        })
        .to_string()
    });

    match env.new_string(&response) {
        Ok(js) => js.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_com_ombhrum_fabushi_core_MahayanaHost_nativeDestroy(
    _env: JNIEnv,
    _obj: JObject,
    handle: jlong,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        destroy_by_handle(handle);
    }));
}

// C ABI Entrypoints
#[no_mangle]
pub unsafe extern "C" fn mahayana_app_host_create(app_data_dir: *const c_char) -> i64 {
    catch_unwind(AssertUnwindSafe(|| {
        let dir = if app_data_dir.is_null() {
            ""
        } else {
            CStr::from_ptr(app_data_dir).to_str().unwrap_or("")
        };
        register_host(dir, false)
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "C" fn mahayana_app_host_create_test(app_data_dir: *const c_char) -> i64 {
    catch_unwind(AssertUnwindSafe(|| {
        let dir = if app_data_dir.is_null() {
            ""
        } else {
            CStr::from_ptr(app_data_dir).to_str().unwrap_or("")
        };
        register_host(dir, true)
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "C" fn mahayana_app_host_dispatch_with_handle(
    handle: i64,
    request_json: *const c_char,
) -> *mut c_char {
    let resp = catch_unwind(AssertUnwindSafe(|| {
        if request_json.is_null() {
            return json!({ "ok": false, "error": "Null request_json pointer" }).to_string();
        }
        let req = match CStr::from_ptr(request_json).to_str() {
            Ok(s) => s,
            Err(e) => {
                return json!({ "ok": false, "error": format!("Invalid UTF-8: {e}") }).to_string()
            }
        };
        dispatch_by_handle(handle, req)
    }))
    .unwrap_or_else(|_| json!({ "ok": false, "error": "C FFI panic caught" }).to_string());

    CString::new(resp)
        .unwrap_or_else(|_| CString::new("{\"ok\":false,\"error\":\"NUL byte\"}").unwrap())
        .into_raw()
}

#[no_mangle]
pub extern "C" fn mahayana_app_host_destroy(handle: i64) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        destroy_by_handle(handle);
    }));
}

#[no_mangle]
pub unsafe extern "C" fn mahayana_app_host_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            drop(CString::from_raw(ptr));
        }));
    }
}

// ============================================================================
// Baseline Host Schema & Implementations
// ============================================================================

fn init_host_schema(conn: &Connection, test_mode: bool) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS host_auth_state (
            singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
            logged_in INTEGER NOT NULL DEFAULT 0,
            provider TEXT NOT NULL DEFAULT 'local',
            username TEXT NOT NULL DEFAULT 'fabushi',
            nickname TEXT NOT NULL DEFAULT 'Fabushi 行者',
            email TEXT NOT NULL DEFAULT 'user@fabushi.ombhrum.com',
            access_token TEXT NOT NULL DEFAULT '',
            device_id TEXT NOT NULL DEFAULT 'android:fabushi-native',
            session_id TEXT NOT NULL DEFAULT 'session:android-native',
            expires_at_sec INTEGER NOT NULL DEFAULT 0
        );
        INSERT OR IGNORE INTO host_auth_state (singleton_id, logged_in) VALUES (1, 0);
        CREATE TABLE IF NOT EXISTS host_auth_attempts (
            attempt_id TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            provider TEXT NOT NULL,
            login_url TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS host_kv (
            namespace TEXT NOT NULL,
            key TEXT NOT NULL,
            value_json TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            PRIMARY KEY (namespace, key)
        );
        CREATE TABLE IF NOT EXISTS host_plugins (
            plugin_id TEXT PRIMARY KEY,
            version TEXT NOT NULL,
            runtime TEXT NOT NULL,
            added_to_account INTEGER NOT NULL DEFAULT 1,
            started INTEGER NOT NULL DEFAULT 0,
            installed_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS host_plugin_permissions (
            plugin_id TEXT NOT NULL,
            permission TEXT NOT NULL,
            granted_at_ms INTEGER NOT NULL,
            PRIMARY KEY (plugin_id, permission)
        );
        CREATE TABLE IF NOT EXISTS host_entitlements (
            plugin_id TEXT NOT NULL,
            capability TEXT NOT NULL,
            sku TEXT NOT NULL,
            allowed INTEGER NOT NULL DEFAULT 0,
            idempotency_key TEXT,
            updated_at_ms INTEGER NOT NULL,
            PRIMARY KEY (plugin_id, capability)
        );
        CREATE TABLE IF NOT EXISTS host_bots (
            bot_id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT NOT NULL,
            mini_app_id TEXT,
            created_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS host_operations (
            operation_id TEXT PRIMARY KEY,
            request_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            status TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS host_approvals (
            approval_id TEXT PRIMARY KEY,
            operation_id TEXT NOT NULL,
            mini_app_id TEXT NOT NULL,
            capability TEXT NOT NULL,
            reason TEXT NOT NULL,
            decision TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );",
    )
    .map_err(|e| format!("Failed initializing host schema: {e}"))?;

    if !test_mode {
        // Seed canonical Global Dharma & Mika Phone Agent as ready-to-use account apps on first launch
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM host_plugins", [], |r| r.get(0))
            .unwrap_or(0);
        if count == 0 {
            let now = chrono::Utc::now().timestamp_millis();
            record_plugin_install(conn, "global-dharma", "2.0.0", "cordis-js", now)?;
            record_plugin_install(conn, "mika-phone-agent", "2.0.0", "cordis-js", now)?;
        }
    }
    Ok(())
}

fn try_sync_ci_session_file(conn: &Connection) -> Result<(), String> {
    let Ok(ci_path) = std::env::var("FABUSHI_CI_ACCOUNT_SESSION_FILE") else {
        return Ok(());
    };
    let trimmed = ci_path.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let raw = fs::read_to_string(trimmed)
        .map_err(|e| format!("Failed reading FABUSHI_CI_ACCOUNT_SESSION_FILE: {e}"))?;
    let v: Value = serde_json::from_str(&raw)
        .map_err(|e| format!("Invalid JSON in FABUSHI_CI_ACCOUNT_SESSION_FILE: {e}"))?;

    let provider = v.get("provider").and_then(Value::as_str).unwrap_or("");
    let ci_runner = v.get("ciRunner").and_then(Value::as_bool).unwrap_or(false);
    let access_token = v
        .get("accessToken")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let device_id = v
        .get("deviceId")
        .and_then(Value::as_str)
        .unwrap_or("ci:android-runner")
        .trim();
    let session_id = v
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or("ci-session")
        .trim();
    let expires_at = v
        .get("accessTokenExpiresAt")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let now_sec = chrono::Utc::now().timestamp();

    if provider != "github-actions"
        || !ci_runner
        || access_token.len() < 24
        || access_token.chars().any(char::is_whitespace)
        || expires_at <= now_sec
    {
        return Err("Invalid or expired CI account session file".into());
    }

    let username = v
        .get("username")
        .or_else(|| v.pointer("/user/username"))
        .and_then(Value::as_str)
        .unwrap_or("ci-runner");
    let nickname = v
        .pointer("/user/nickname")
        .and_then(Value::as_str)
        .unwrap_or(username);
    let email = v
        .pointer("/user/email")
        .and_then(Value::as_str)
        .unwrap_or("ci@fabushi.ombhrum.com");

    conn.execute(
        "UPDATE host_auth_state
         SET logged_in = 1, provider = 'github-actions', username = ?1, nickname = ?2,
             email = ?3, access_token = ?4, device_id = ?5, session_id = ?6, expires_at_sec = ?7
         WHERE singleton_id = 1",
        params![
            username,
            nickname,
            email,
            access_token,
            device_id,
            session_id,
            expires_at
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn auth_status(conn: &Connection) -> Result<Value, String> {
    try_sync_ci_session_file(conn)?;
    let (logged_in, provider, username, nickname, email, device_id, session_id): (
        i64,
        String,
        String,
        String,
        String,
        String,
        String,
    ) = conn
        .query_row(
            "SELECT logged_in, provider, username, nickname, email, device_id, session_id
             FROM host_auth_state WHERE singleton_id = 1",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            },
        )
        .map_err(|e| e.to_string())?;

    let is_logged = logged_in != 0;
    Ok(json!({
        "loggedIn": is_logged,
        "provider": if is_logged { Value::String(provider) } else { Value::Null },
        "deviceId": device_id,
        "sessionId": session_id,
        "user": if is_logged {
            json!({
                "id": messaging::DEFAULT_ACTOR_ID,
                "username": username,
                "nickname": nickname,
                "email": email
            })
        } else {
            Value::Null
        }
    }))
}

fn auth_providers() -> Value {
    json!([
        { "id": "google", "displayName": "Google", "enabled": true },
        { "id": "apple", "displayName": "Apple", "enabled": true },
        { "id": "github", "displayName": "GitHub", "enabled": true },
        { "id": "microsoft", "displayName": "Microsoft", "enabled": true },
        { "id": "telegram", "displayName": "Telegram Phone / QR", "enabled": true },
        { "id": "passkey", "displayName": "Passkey (WebAuthn)", "enabled": true }
    ])
}

fn mark_logged_in(
    conn: &Connection,
    provider: &str,
    username: &str,
    nickname: &str,
    email: &str,
) -> Result<Value, String> {
    let now_sec = chrono::Utc::now().timestamp();
    let token = format!(
        "fabushi_agent_tok_{}_{}",
        uuid::Uuid::new_v4().simple(),
        now_sec
    );
    let session_id = format!("session:android:{}", &uuid::Uuid::new_v4().simple().to_string()[..16]);
    conn.execute(
        "UPDATE host_auth_state
         SET logged_in = 1, provider = ?1, username = ?2, nickname = ?3, email = ?4,
             access_token = ?5, device_id = 'android:fabushi-native', session_id = ?6,
             expires_at_sec = ?7
         WHERE singleton_id = 1",
        params![
            provider,
            username,
            nickname,
            email,
            token,
            session_id,
            now_sec + 14_400
        ],
    )
    .map_err(|e| e.to_string())?;
    auth_status(conn)
}

fn auth_oauth_start(conn: &Connection, params: &Value) -> Result<Value, String> {
    let now = chrono::Utc::now().timestamp_millis();
    let provider = params
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or("google");
    let attempt_id = format!("oauth_{}", &uuid::Uuid::new_v4().simple().to_string()[..20]);
    let url = format!("about:blank#fabushi-test-oauth&provider={provider}&attemptId={attempt_id}");
    conn.execute(
        "INSERT INTO host_auth_attempts (attempt_id, kind, provider, login_url, status, created_at_ms)
         VALUES (?1, 'oauth', ?2, ?3, 'pending', ?4)",
        params![attempt_id, provider, url, now],
    )
    .map_err(|e| e.to_string())?;
    Ok(json!({
        "attemptId": attempt_id,
        "provider": provider,
        "authorizationUrl": url,
        "loginUrl": url,
        "status": "pending"
    }))
}

fn auth_oauth_poll(conn: &Connection, params: &Value) -> Result<Value, String> {
    let attempt_id = params
        .get("attemptId")
        .and_then(Value::as_str)
        .ok_or_else(|| "oauthPoll requires 'attemptId'".to_string())?;
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT provider, status FROM host_auth_attempts WHERE attempt_id = ?1",
            params![attempt_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let (provider, prev_status) =
        row.ok_or_else(|| format!("Unknown OAuth attemptId '{attempt_id}'"))?;
    if prev_status == "cancelled" {
        return Ok(json!({ "attemptId": attempt_id, "status": "cancelled" }));
    }
    conn.execute(
        "UPDATE host_auth_attempts SET status = 'completed' WHERE attempt_id = ?1",
        params![attempt_id],
    )
    .map_err(|e| e.to_string())?;
    let auth = mark_logged_in(
        conn,
        &provider,
        "fabushi",
        "Fabushi 行者",
        "user@fabushi.ombhrum.com",
    )?;
    Ok(json!({
        "attemptId": attempt_id,
        "provider": provider,
        "status": "completed",
        "auth": auth
    }))
}

fn auth_browser_start(conn: &Connection, _params: &Value) -> Result<Value, String> {
    let now = chrono::Utc::now().timestamp_millis();
    let attempt_id = format!("browser_{}", &uuid::Uuid::new_v4().simple().to_string()[..20]);
    let base = std::env::var("FABUSHI_LIVE_AUTH_URL")
        .ok()
        .filter(|s| !s.trim().is_empty());
    let login_url = match base {
        Some(b) => format!("{}?attemptId={attempt_id}", b.trim_end_matches('/')),
        None => format!("about:blank#fabushi-test-browser-login&attemptId={attempt_id}"),
    };
    conn.execute(
        "INSERT INTO host_auth_attempts (attempt_id, kind, provider, login_url, status, created_at_ms)
         VALUES (?1, 'browser', 'fabushi-browser', ?2, 'pending', ?3)",
        params![attempt_id, login_url, now],
    )
    .map_err(|e| e.to_string())?;
    Ok(json!({
        "attemptId": attempt_id,
        "loginUrl": login_url,
        "authorizationUrl": login_url,
        "status": "pending"
    }))
}

fn auth_browser_reopen(conn: &Connection, params: &Value) -> Result<Value, String> {
    let attempt_id = params
        .get("attemptId")
        .and_then(Value::as_str)
        .ok_or_else(|| "browserReopen requires 'attemptId'".to_string())?;
    let url: String = conn
        .query_row(
            "SELECT login_url FROM host_auth_attempts WHERE attempt_id = ?1",
            params![attempt_id],
            |r| r.get(0),
        )
        .unwrap_or_else(|_| {
            format!("about:blank#fabushi-test-browser-login&attemptId={attempt_id}")
        });
    Ok(json!({
        "attemptId": attempt_id,
        "loginUrl": url,
        "authorizationUrl": url
    }))
}

fn auth_browser_cancel(conn: &Connection, params: &Value) -> Result<Value, String> {
    let attempt_id = params
        .get("attemptId")
        .and_then(Value::as_str)
        .unwrap_or("");
    conn.execute(
        "UPDATE host_auth_attempts SET status = 'cancelled' WHERE attempt_id = ?1",
        params![attempt_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(json!({
        "attemptId": attempt_id,
        "status": "cancelled"
    }))
}

fn auth_browser_poll(conn: &Connection, params: &Value) -> Result<Value, String> {
    let attempt_id = params
        .get("attemptId")
        .and_then(Value::as_str)
        .ok_or_else(|| "browserPoll requires 'attemptId'".to_string())?;
    let status: Option<String> = conn
        .query_row(
            "SELECT status FROM host_auth_attempts WHERE attempt_id = ?1",
            params![attempt_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;

    if status.as_deref() == Some("cancelled") {
        return Ok(json!({
            "attemptId": attempt_id,
            "status": "cancelled"
        }));
    }

    conn.execute(
        "UPDATE host_auth_attempts SET status = 'completed' WHERE attempt_id = ?1",
        params![attempt_id],
    )
    .map_err(|e| e.to_string())?;
    let auth = mark_logged_in(
        conn,
        "fabushi-browser",
        "fabushi",
        "Fabushi 行者",
        "user@fabushi.ombhrum.com",
    )?;
    Ok(json!({
        "attemptId": attempt_id,
        "status": "completed",
        "auth": auth
    }))
}

fn auth_password_login(conn: &Connection, params: &Value) -> Result<Value, String> {
    let identity = params
        .get("username")
        .or_else(|| params.get("email"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let password = params
        .get("password")
        .and_then(Value::as_str)
        .unwrap_or("");
    if identity.is_empty() || password.len() < 4 {
        return Err("Invalid credentials: username and password (>=4 chars) required".into());
    }
    let email = if identity.contains('@') {
        identity.to_string()
    } else {
        format!("{identity}@fabushi.ombhrum.com")
    };
    mark_logged_in(conn, "password", identity, identity, &email)
}

fn auth_logout(conn: &Connection) -> Result<Value, String> {
    conn.execute(
        "UPDATE host_auth_state
         SET logged_in = 0, access_token = '', expires_at_sec = 0
         WHERE singleton_id = 1",
        [],
    )
    .map_err(|e| e.to_string())?;
    Ok(json!({
        "loggedIn": false,
        "user": {
            "nickname": "Fabushi",
            "email": ""
        }
    }))
}

fn auth_device_agent_session(conn: &Connection) -> Result<Value, String> {
    try_sync_ci_session_file(conn)?;
    let now_sec = chrono::Utc::now().timestamp();
    let (logged_in, username, mut token, device_id, session_id, mut expires_at): (
        i64,
        String,
        String,
        String,
        String,
        i64,
    ) = conn
        .query_row(
            "SELECT logged_in, username, access_token, device_id, session_id, expires_at_sec
             FROM host_auth_state WHERE singleton_id = 1",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .map_err(|e| e.to_string())?;

    if logged_in == 0 {
        return Err("Not logged in for deviceAgentSession".into());
    }
    if token.len() < 24 || expires_at <= now_sec {
        token = format!("fabushi_agent_tok_{}_{}", uuid::Uuid::new_v4().simple(), now_sec);
        expires_at = now_sec + 14_400;
        conn.execute(
            "UPDATE host_auth_state SET access_token = ?1, expires_at_sec = ?2 WHERE singleton_id = 1",
            params![token, expires_at],
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(json!({
        "accessToken": token,
        "deviceId": device_id,
        "sessionId": session_id,
        "username": username,
        "accessTokenExpiresAt": expires_at
    }))
}

// ============================================================================
// Persistence KV Store
// ============================================================================

fn persistence_set(conn: &Connection, params: &Value) -> Result<Value, String> {
    let now = chrono::Utc::now().timestamp_millis();
    let ns = params
        .get("namespace")
        .and_then(Value::as_str)
        .unwrap_or("default");
    let key = params
        .get("key")
        .and_then(Value::as_str)
        .ok_or_else(|| "feature.persistence.set requires 'key'".to_string())?;
    let val = params.get("value").cloned().unwrap_or(Value::Null);
    conn.execute(
        "INSERT OR REPLACE INTO host_kv (namespace, key, value_json, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4)",
        params![ns, key, val.to_string(), now],
    )
    .map_err(|e| e.to_string())?;
    Ok(json!({
        "namespace": ns,
        "key": key,
        "stored": true,
        "updatedAtMs": now
    }))
}

fn persistence_get(conn: &Connection, params: &Value) -> Result<Value, String> {
    let ns = params
        .get("namespace")
        .and_then(Value::as_str)
        .unwrap_or("default");
    let key = params
        .get("key")
        .and_then(Value::as_str)
        .ok_or_else(|| "feature.persistence.get requires 'key'".to_string())?;
    let raw: Option<(String, i64)> = conn
        .query_row(
            "SELECT value_json, updated_at_ms FROM host_kv WHERE namespace = ?1 AND key = ?2",
            params![ns, key],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    match raw {
        Some((s, ts)) => {
            let v: Value = serde_json::from_str(&s).unwrap_or(Value::String(s));
            Ok(json!({
                "namespace": ns,
                "key": key,
                "found": true,
                "value": v,
                "updatedAtMs": ts
            }))
        }
        None => Ok(json!({
            "namespace": ns,
            "key": key,
            "found": false,
            "value": Value::Null
        })),
    }
}

fn persistence_remove(conn: &Connection, params: &Value) -> Result<Value, String> {
    let ns = params
        .get("namespace")
        .and_then(Value::as_str)
        .unwrap_or("default");
    let key = params
        .get("key")
        .and_then(Value::as_str)
        .ok_or_else(|| "feature.persistence.remove requires 'key'".to_string())?;
    let affected = conn
        .execute(
            "DELETE FROM host_kv WHERE namespace = ?1 AND key = ?2",
            params![ns, key],
        )
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "namespace": ns,
        "key": key,
        "removed": affected > 0
    }))
}

fn persistence_list(conn: &Connection, params: &Value) -> Result<Value, String> {
    let ns = params
        .get("namespace")
        .and_then(Value::as_str)
        .unwrap_or("default");
    let prefix = params.get("prefix").and_then(Value::as_str).unwrap_or("");
    let like_pat = format!("{prefix}%");
    let mut stmt = conn
        .prepare(
            "SELECT key, value_json, updated_at_ms FROM host_kv
             WHERE namespace = ?1 AND key LIKE ?2
             ORDER BY key ASC",
        )
        .map_err(|e| e.to_string())?;
    let entries: Vec<Value> = stmt
        .query_map(params![ns, like_pat], |r| {
            let k: String = r.get(0)?;
            let raw: String = r.get(1)?;
            let ts: i64 = r.get(2)?;
            let v: Value = serde_json::from_str(&raw).unwrap_or(Value::String(raw));
            Ok(json!({
                "key": k,
                "value": v,
                "updatedAtMs": ts
            }))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "namespace": ns,
        "entries": entries
    }))
}

// ============================================================================
// Marketplace, Plugins, Portable Runtime & Platform Control Plane
// ============================================================================

struct CatalogPlugin {
    id: &'static str,
    display_name: &'static str,
    description: &'static str,
    version: &'static str,
    runtime: &'static str,
    source_ref: &'static str,
    permissions: &'static [&'static str],
    commands: Value,
}

fn catalog_plugins() -> Vec<CatalogPlugin> {
    vec![
        CatalogPlugin {
            id: "global-dharma",
            display_name: "全球法布施",
            description: "全球 249 国与地区法布施地图、禅堂转经轮、八大经典共修、功德榜与 AI 法义开示。",
            version: "2.0.0",
            runtime: "cordis-js",
            source_ref: "github:ombhrum/global-dharma@cb8f4a2d9e1047a1b3c5d7e9f102938475612345",
            permissions: &["storage", "notifications"],
            commands: json!([
                { "tool": "dharma.sutras", "slash": "/sutras", "description": "浏览经典藏经阁", "approval": "none" },
                { "tool": "dharma.offer", "slash": "/offer", "description": "向全球国家节点发起法布施", "approval": "none" },
                { "tool": "local.prayer-wheel.start", "slash": "/wheel", "description": "启动本地禅堂转经轮", "approval": "none" },
                { "tool": "dharma.counsel", "slash": "/ask", "description": "AI 禅修与法义开示", "approval": "none" }
            ]),
        },
        CatalogPlugin {
            id: "mika-phone-agent",
            display_name: "Mika 手机操控助手",
            description: "端侧 Android 无障碍 + 多模态屏幕推理智能体，支持跨应用自动化与支付安全护栏。",
            version: "2.0.0",
            runtime: "cordis-js",
            source_ref: "github:fabu-android/mika-phone-agent@9a1b2c3d4e5f60718293a4b5c6d7e8f901234567",
            permissions: &["accessibility", "overlay"],
            commands: json!([
                { "tool": "mika.status", "slash": "/status", "description": "检查无障碍服务与悬浮窗权限", "approval": "none" },
                { "tool": "mika.runTask", "slash": "/run", "description": "下发手机自动操控任务", "approval": "confirm" }
            ]),
        },
        CatalogPlugin {
            id: "ton-wallet",
            display_name: "TON & Stars 钱包",
            description: "BIP-39 24 词助记词、TON Wallet v4r2 自托管钱包、Telegram Stars 账本、NFT 藏品与 TonConnect 2.0。",
            version: "2.0.0",
            runtime: "javascript",
            source_ref: "github:ombhrum/ton-stars-miniapp@7f6e5d4c3b2a1908f7e6d5c4b3a2918071625344",
            permissions: &["biometric", "storage"],
            commands: json!([
                { "tool": "wallet.state", "slash": "/balance", "description": "查看 TON 与 Stars 余额", "approval": "none" },
                { "tool": "wallet.emulate", "slash": "/emulate", "description": "预演 TON 交易手续费与风险", "approval": "none" }
            ]),
        },
        CatalogPlugin {
            id: "hermes-studio",
            display_name: "Hermes AI Studio",
            description: "随身 AI 智能体工作台：会话分支树、MEMORY/USER/SOUL 持久记忆、Cron 定时任务与 Kanban 看板。",
            version: "2.0.0",
            runtime: "deepseek-js",
            source_ref: "github:ombhrum/hermes-studio-miniapp@11223344556677889900aabbccddeeff00112233",
            permissions: &["storage"],
            commands: json!([
                { "tool": "hermes.sessions", "slash": "/sessions", "description": "查看与分支推理会话", "approval": "none" },
                { "tool": "hermes.kanban", "slash": "/kanban", "description": "查看智能体任务看板", "approval": "none" }
            ]),
        },
        CatalogPlugin {
            id: "magpie-gateway",
            display_name: "Magpie AI 路由网关",
            description: "51 家大模型预设、Rosetta 跨协议互译、熔断健康状态机、PII 敏感密钥自动脱敏与成本账本。",
            version: "2.0.0",
            runtime: "javascript",
            source_ref: "github:ombhrum/magpie-gateway-miniapp@fedcba9876543210fedcba9876543210fedcba98",
            permissions: &["network"],
            commands: json!([
                { "tool": "magpie.presets", "slash": "/providers", "description": "浏览 51 家模型供应商预设", "approval": "none" },
                { "tool": "magpie.usage", "slash": "/usage", "description": "查看 Token 消耗与成本统计", "approval": "none" }
            ]),
        },
        CatalogPlugin {
            id: "computer-control",
            display_name: "Cloud Computer 云电脑操控",
            description: "云端桌面与本地设备统一 CUA 租约仲裁、5 级风险动作总线与 7 状态全局任务编排。",
            version: "2.0.0",
            runtime: "cordis-js",
            source_ref: "github:ombhrum/computer-control-miniapp@0123456789abcdef0123456789abcdef01234567",
            permissions: &["network"],
            commands: json!([
                { "tool": "computer.nodes", "slash": "/nodes", "description": "列出在线计算节点与租约代次", "approval": "none" },
                { "tool": "computer.tasks", "slash": "/tasks", "description": "查看全局任务状态机", "approval": "none" }
            ]),
        },
    ]
}

fn build_install_contract(source_ref: &str) -> Value {
    json!({
        "protocol": "fabushi.marketplace.install.v1",
        "strategy": "github-immutable",
        "source": {
            "provider": "github",
            "sourceRef": source_ref,
            "marketplaceHostsPackage": false
        }
    })
}

fn build_release_manifest(p: &CatalogPlugin) -> Value {
    let install = build_install_contract(p.source_ref);
    json!({
        "pluginId": p.id,
        "displayName": p.display_name,
        "description": p.description,
        "version": p.version,
        "runtime": p.runtime,
        "portableCompatible": true,
        "requestedPermissions": p.permissions,
        "commands": p.commands,
        "install": install
    })
}

fn marketplace_browse(_conn: &Connection, params: &Value) -> Result<Value, String> {
    let query = params
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_lowercase();

    let plugins: Vec<Value> = catalog_plugins()
        .into_iter()
        .filter(|p| {
            query.is_empty()
                || p.id.to_lowercase().contains(&query)
                || p.display_name.to_lowercase().contains(&query)
                || p.description.to_lowercase().contains(&query)
        })
        .map(|p| {
            let install = build_install_contract(p.source_ref);
            let manifest = build_release_manifest(&p);
            json!({
                "pluginId": p.id,
                "displayName": p.display_name,
                "description": p.description,
                "latestVersion": p.version,
                "runtime": p.runtime,
                "commands": p.commands,
                "install": install,
                "releaseManifest": manifest
            })
        })
        .collect();

    Ok(json!({ "plugins": plugins }))
}

fn marketplace_release(params: &Value) -> Result<Value, String> {
    let plugin_id = params
        .get("pluginId")
        .and_then(Value::as_str)
        .ok_or_else(|| "feature.marketplace.release requires 'pluginId'".to_string())?;
    let catalog = catalog_plugins();
    let found = catalog
        .iter()
        .find(|p| p.id == plugin_id)
        .ok_or_else(|| format!("Unknown marketplace plugin '{plugin_id}'"))?;
    let install = build_install_contract(found.source_ref);
    let release_manifest = build_release_manifest(found);
    Ok(json!({
        "pluginId": found.id,
        "version": found.version,
        "install": install,
        "releaseManifest": release_manifest
    }))
}

fn record_plugin_install(
    conn: &Connection,
    plugin_id: &str,
    version: &str,
    runtime: &str,
    now: i64,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO host_plugins (plugin_id, version, runtime, added_to_account, started, installed_at_ms)
         VALUES (?1, ?2, ?3, 1, 1, ?4)
         ON CONFLICT(plugin_id) DO UPDATE SET
            version = excluded.version,
            runtime = excluded.runtime,
            added_to_account = 1",
        params![plugin_id, version, runtime, now],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn marketplace_add_plugin(conn: &Connection, params: &Value) -> Result<Value, String> {
    let now = chrono::Utc::now().timestamp_millis();
    let plugin_id = params
        .get("pluginId")
        .and_then(Value::as_str)
        .ok_or_else(|| "marketplace.add requires 'pluginId'".to_string())?;
    record_plugin_install(conn, plugin_id, "2.0.0", "cordis-js", now)?;
    Ok(json!({ "pluginId": plugin_id, "added": true }))
}

fn plugin_install(conn: &Connection, params: &Value) -> Result<Value, String> {
    let now = chrono::Utc::now().timestamp_millis();
    let release = params.get("release").unwrap_or(params);
    let plugin_id = release
        .get("pluginId")
        .and_then(Value::as_str)
        .ok_or_else(|| "feature.plugin.install requires 'release.pluginId'".to_string())?;
    let version = release
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("2.0.0");
    let runtime = release
        .get("runtime")
        .and_then(Value::as_str)
        .unwrap_or("cordis-js");
    let perms = release
        .get("requestedPermissions")
        .cloned()
        .unwrap_or_else(|| json!([]));

    record_plugin_install(conn, plugin_id, version, runtime, now)?;
    Ok(json!({
        "pluginId": plugin_id,
        "version": version,
        "runtime": runtime,
        "requestedPermissions": perms,
        "installed": true
    }))
}

fn plugin_uninstall(conn: &Connection, params: &Value) -> Result<Value, String> {
    let plugin_id = params
        .get("pluginId")
        .and_then(Value::as_str)
        .ok_or_else(|| "feature.plugin.uninstall requires 'pluginId'".to_string())?;
    conn.execute(
        "DELETE FROM host_plugins WHERE plugin_id = ?1",
        params![plugin_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(json!({ "pluginId": plugin_id, "uninstalled": true }))
}

fn plugin_list_installed(conn: &Connection) -> Result<Value, String> {
    let mut stmt = conn
        .prepare("SELECT plugin_id, version, runtime, started, installed_at_ms FROM host_plugins ORDER BY installed_at_ms ASC")
        .map_err(|e| e.to_string())?;
    let list: Vec<Value> = stmt
        .query_map([], |r| {
            Ok(json!({
                "pluginId": r.get::<_, String>(0)?,
                "version": r.get::<_, String>(1)?,
                "runtime": r.get::<_, String>(2)?,
                "started": r.get::<_, i64>(3)? != 0,
                "installedAtMs": r.get::<_, i64>(4)?
            }))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(json!({ "plugins": list }))
}

fn plugin_permission_grant(conn: &Connection, params: &Value) -> Result<Value, String> {
    let now = chrono::Utc::now().timestamp_millis();
    let plugin_id = params
        .get("pluginId")
        .and_then(Value::as_str)
        .ok_or_else(|| "plugin.permission.grant requires 'pluginId'".to_string())?;
    let permission = params
        .get("permission")
        .and_then(Value::as_str)
        .ok_or_else(|| "plugin.permission.grant requires 'permission'".to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO host_plugin_permissions (plugin_id, permission, granted_at_ms)
         VALUES (?1, ?2, ?3)",
        params![plugin_id, permission, now],
    )
    .map_err(|e| e.to_string())?;
    Ok(json!({
        "pluginId": plugin_id,
        "permission": permission,
        "granted": true
    }))
}

fn plugin_compatibility(params: &Value) -> Result<Value, String> {
    let plugin_id = params
        .get("pluginId")
        .and_then(Value::as_str)
        .unwrap_or("global-dharma");
    Ok(json!({
        "pluginId": plugin_id,
        "platform": "android",
        "portableCompatible": true,
        "engine": "mahayana-portable-v2"
    }))
}

fn runtime_start(conn: &Connection, params: &Value) -> Result<Value, String> {
    let plugin_id = params
        .get("pluginId")
        .and_then(Value::as_str)
        .ok_or_else(|| "runtime.start requires 'pluginId'".to_string())?;
    conn.execute(
        "UPDATE host_plugins SET started = 1 WHERE plugin_id = ?1",
        params![plugin_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(json!({
        "pluginId": plugin_id,
        "started": true,
        "status": "running"
    }))
}

fn runtime_call(conn: &Connection, params: &Value) -> Result<Value, String> {
    let plugin_id = params
        .get("pluginId")
        .and_then(Value::as_str)
        .unwrap_or("global-dharma");
    let tool = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "runtime.call requires 'name'".to_string())?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let structured = match tool {
        "dharma.sutras" => fabushi_miniapp::handle_rpc(conn, "sutras", &args)?,
        "dharma.offer" => fabushi_miniapp::handle_rpc(conn, "offerDharma", &args)?,
        "dharma.counsel" => fabushi_miniapp::handle_rpc(conn, "aiDharmaChat", &args)?,
        "local.prayer-wheel.start" => fabushi_miniapp::handle_rpc(conn, "spinPrayerWheel", &args)?,
        "wallet.state" => wallet::handle_rpc(conn, "status", &args)?,
        "wallet.emulate" => wallet::handle_rpc(conn, "tonConnectEmulate", &args)?,
        "hermes.sessions" => hermes::handle_rpc(conn, "sessions", &args)?,
        "hermes.kanban" => hermes::handle_rpc(conn, "kanbanBoard", &args)?,
        "magpie.presets" => magpie::handle_rpc(conn, "presets", &args)?,
        "magpie.usage" => magpie::handle_rpc(conn, "usageSummary", &args)?,
        "computer.nodes" => computer::handle_computer_rpc(conn, "nodes", &args)?,
        "computer.tasks" => computer::handle_computer_rpc(conn, "tasks.list", &args)?,
        "mika.status" => computer::handle_mika_rpc(conn, "bridgeStatus", &args)?,
        _ => json!({
            "pluginId": plugin_id,
            "tool": tool,
            "executed": true,
            "arguments": args
        }),
    };

    Ok(json!({
        "pluginId": plugin_id,
        "tool": tool,
        "structuredContent": structured,
        "content": [
            { "type": "text", "text": structured.to_string() }
        ]
    }))
}

fn plugin_ui_document(params: &Value) -> Result<Value, String> {
    let plugin_id = params
        .get("pluginId")
        .and_then(Value::as_str)
        .unwrap_or("global-dharma");
    let catalog = catalog_plugins();
    let (title, desc) = catalog
        .iter()
        .find(|p| p.id == plugin_id)
        .map(|p| (p.display_name, p.description))
        .unwrap_or((plugin_id, "Fabushi Native Mini App"));

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width,initial-scale=1,maximum-scale=1" />
<title>{title}</title>
<style>
  body {{ margin:0; font-family:system-ui,-apple-system,sans-serif; background:#0F141C; color:#F3EFE6; padding:20px; }}
  .card {{ background:#18212E; border:1px solid #2B394E; border-radius:16px; padding:18px; margin-bottom:14px; }}
  h1 {{ font-size:20px; margin:0 0 8px; color:#F5C96B; }}
  p {{ font-size:14px; line-height:1.55; color:#C4CFDE; margin:0; }}
  .badge {{ display:inline-block; padding:4px 10px; border-radius:999px; background:#26354A; color:#8EC5FC; font-size:12px; margin-bottom:10px; }}
</style>
</head>
<body>
  <div class="card">
    <span class="badge">Mahayana WebMCP · {plugin_id}</span>
    <h1>{title}</h1>
    <p>{desc}</p>
  </div>
</body>
</html>"#
    );

    Ok(json!({
        "pluginId": plugin_id,
        "title": title,
        "html": html
    }))
}

fn platform_request(conn: &Connection, params: &Value) -> Result<Value, String> {
    let now = chrono::Utc::now().timestamp_millis();
    let method = params
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET")
        .to_ascii_uppercase();
    let path = params
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or("/")
        .trim();
    let body = params.get("body").cloned().unwrap_or_else(|| json!({}));

    // 1. GET /v1/marketplace/added
    if method == "GET" && path == "/v1/marketplace/added" {
        let mut stmt = conn
            .prepare("SELECT plugin_id, version FROM host_plugins WHERE added_to_account = 1 ORDER BY installed_at_ms ASC")
            .map_err(|e| e.to_string())?;
        let installed_ids: Vec<(String, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        let catalog = catalog_plugins();
        let apps: Vec<Value> = installed_ids
            .into_iter()
            .map(|(pid, ver)| {
                let (title, desc) = catalog
                    .iter()
                    .find(|c| c.id == pid)
                    .map(|c| (c.display_name.to_string(), c.description.to_string()))
                    .unwrap_or_else(|| (pid.clone(), "Installed Fabushi Mini App".to_string()));
                json!({
                    "id": pid,
                    "pluginId": pid,
                    "title": title,
                    "description": desc,
                    "version": ver,
                    "bot": {
                        "id": format!("{pid}-bot"),
                        "displayName": title,
                        "description": desc,
                        "menuButton": {
                            "action": "open-miniapp",
                            "miniAppId": pid,
                            "text": "打开应用"
                        }
                    }
                })
            })
            .collect();

        return Ok(json!({
            "ok": true,
            "statusCode": 200,
            "data": { "apps": apps }
        }));
    }

    // 2. POST /v1/marketplace/plugins/{id}/add
    if method == "POST" && path.starts_with("/v1/marketplace/plugins/") && path.ends_with("/add") {
        let plugin_id = path
            .trim_start_matches("/v1/marketplace/plugins/")
            .trim_end_matches("/add")
            .trim_matches('/');
        record_plugin_install(conn, plugin_id, "2.0.0", "cordis-js", now)?;
        return Ok(json!({
            "ok": true,
            "statusCode": 200,
            "data": {
                "added": true,
                "pluginId": plugin_id
            }
        }));
    }

    // 3. POST /v1/marketplace/plugins/{id}/route
    if method == "POST" && path.starts_with("/v1/marketplace/plugins/") && path.ends_with("/route")
    {
        let plugin_id = path
            .trim_start_matches("/v1/marketplace/plugins/")
            .trim_end_matches("/route")
            .trim_matches('/');
        let input = body.get("input").and_then(Value::as_str).unwrap_or("").trim();
        let prefer_remote = body
            .get("preferRemoteMcp")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let (slash, tool, message, structured) = route_miniapp_input(conn, plugin_id, input)?;
        let mut data = json!({
            "pluginId": plugin_id,
            "command": { "slash": slash, "tool": tool },
            "arguments": { "input": input },
            "requiresApproval": false,
            "message": message,
            "structuredContent": structured
        });
        if prefer_remote {
            if let Some(obj) = data.as_object_mut() {
                obj.insert(
                    "execution".into(),
                    json!({ "kind": "mcp-http", "tool": tool }),
                );
            }
        }
        return Ok(json!({
            "ok": true,
            "statusCode": 200,
            "data": data
        }));
    }

    // 4. POST /v1/auth/plugin-token
    if method == "POST" && path == "/v1/auth/plugin-token" {
        let plugin_id = body
            .get("pluginId")
            .and_then(Value::as_str)
            .unwrap_or("global-dharma");
        let token = format!(
            "fabushi_plugin_tok_{}_{}",
            plugin_id.replace('-', "_"),
            uuid::Uuid::new_v4().simple()
        );
        return Ok(json!({
            "ok": true,
            "statusCode": 200,
            "data": {
                "pluginId": plugin_id,
                "accessToken": token,
                "expiresIn": 300
            }
        }));
    }

    // 5. GET /v1/plugins/{id}/entitlements/{capability}
    if method == "GET" && path.starts_with("/v1/plugins/") && path.contains("/entitlements/") {
        let rest = path.trim_start_matches("/v1/plugins/");
        let mut parts = rest.splitn(2, "/entitlements/");
        let plugin_id = parts.next().unwrap_or("global-dharma");
        let capability = parts.next().unwrap_or("local.prayer-wheel.start");

        let allowed: bool = conn
            .query_row(
                "SELECT allowed FROM host_entitlements WHERE plugin_id = ?1 AND capability = ?2",
                params![plugin_id, capability],
                |r| Ok(r.get::<_, i64>(0)? != 0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .unwrap_or(false);

        return Ok(json!({
            "ok": true,
            "statusCode": 200,
            "data": {
                "pluginId": plugin_id,
                "capability": capability,
                "access": {
                    "protected": true,
                    "allowed": allowed,
                    "reason": if allowed { "entitled" } else { "not_entitled" }
                },
                "purchaseOptions": [
                    {
                        "sku": "local-prayer-wheel.lifetime",
                        "productId": "prod.global-dharma.local-prayer-wheel.lifetime",
                        "currency": "CNY",
                        "amount": 108_000,
                        "displayPrice": "¥1,080.00",
                        "activeRails": ["fabushi_pay_test", "google_play", "ton_stars"]
                    }
                ]
            }
        }));
    }

    // 6. POST /v1/plugins/{id}/commerce/purchase
    if method == "POST"
        && path.starts_with("/v1/plugins/")
        && path.ends_with("/commerce/purchase")
    {
        let plugin_id = path
            .trim_start_matches("/v1/plugins/")
            .trim_end_matches("/commerce/purchase")
            .trim_matches('/');
        let sku = body
            .get("sku")
            .and_then(Value::as_str)
            .unwrap_or("local-prayer-wheel.lifetime");
        let idem = body
            .get("idempotencyKey")
            .and_then(Value::as_str)
            .unwrap_or("");
        let capability = if sku.contains("prayer-wheel") {
            "local.prayer-wheel.start"
        } else {
            sku
        };

        conn.execute(
            "INSERT OR REPLACE INTO host_entitlements
             (plugin_id, capability, sku, allowed, idempotency_key, updated_at_ms)
             VALUES (?1, ?2, ?3, 1, ?4, ?5)",
            params![plugin_id, capability, sku, idem, now],
        )
        .map_err(|e| e.to_string())?;

        return Ok(json!({
            "ok": true,
            "statusCode": 200,
            "data": {
                "purchased": true,
                "pluginId": plugin_id,
                "sku": sku,
                "capability": capability,
                "allowed": true,
                "orderId": format!("ord_{}", &uuid::Uuid::new_v4().simple().to_string()[..16])
            }
        }));
    }

    // 7. POST /v1/purchases/restore
    if method == "POST" && path == "/v1/purchases/restore" {
        let mut stmt = conn
            .prepare("SELECT plugin_id, capability, sku FROM host_entitlements WHERE allowed = 1")
            .map_err(|e| e.to_string())?;
        let restored: Vec<Value> = stmt
            .query_map([], |r| {
                Ok(json!({
                    "pluginId": r.get::<_, String>(0)?,
                    "capability": r.get::<_, String>(1)?,
                    "sku": r.get::<_, String>(2)?
                }))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        return Ok(json!({
            "ok": true,
            "statusCode": 200,
            "data": {
                "restored": true,
                "entitlements": restored
            }
        }));
    }

    Ok(json!({
        "ok": false,
        "statusCode": 404,
        "data": { "error": format!("Unsupported platform route: {method} {path}") }
    }))
}

fn route_miniapp_input(
    conn: &Connection,
    plugin_id: &str,
    input: &str,
) -> Result<(String, String, String, Value), String> {
    match plugin_id {
        "global-dharma" => {
            if input.starts_with("/wheel") || input.contains("转经轮") {
                let res = fabushi_miniapp::handle_rpc(
                    conn,
                    "spinPrayerWheel",
                    &json!({ "rotations": 108 }),
                )?;
                let total = res
                    .get("wheelRotations")
                    .and_then(Value::as_u64)
                    .unwrap_or(108);
                let merit = res.get("totalMerit").and_then(Value::as_u64).unwrap_or(972);
                Ok((
                    "/wheel".into(),
                    "local.prayer-wheel.start".into(),
                    format!("🙏 已转动本地禅堂转经轮 108 匝（累计 {total} 匝 · 功德 {merit}）。"),
                    res,
                ))
            } else if input.starts_with("/sutras") || input.contains("经") {
                let res = fabushi_miniapp::handle_rpc(conn, "sutras", &json!({}))?;
                Ok((
                    "/sutras".into(),
                    "dharma.sutras".into(),
                    "📖 全球法布施藏经阁已就绪：《般若波罗蜜多心经》《金刚经》《普贤行愿品》等 8 部经典可供修持。".into(),
                    res,
                ))
            } else {
                let res = fabushi_miniapp::handle_rpc(
                    conn,
                    "aiDharmaChat",
                    &json!({ "prompt": input }),
                )?;
                let reply = res
                    .get("reply")
                    .and_then(Value::as_str)
                    .unwrap_or("愿心清净，福慧双修。")
                    .to_string();
                Ok(("/ask".into(), "dharma.counsel".into(), reply, res))
            }
        }
        "ton-wallet" => {
            let res = wallet::handle_rpc(conn, "status", &json!({}))?;
            let addr = res
                .get("address")
                .and_then(Value::as_str)
                .unwrap_or("UQ...");
            let stars = res.get("starsBalance").and_then(Value::as_i64).unwrap_or(0);
            Ok((
                "/balance".into(),
                "wallet.state".into(),
                format!("💎 TON Wallet v4r2 地址：`{addr}` · Stars 余额：{stars} ⭐"),
                res,
            ))
        }
        "hermes-studio" => {
            let res = hermes::handle_rpc(conn, "listCommands", &json!({}))?;
            let count = res.get("total").and_then(Value::as_u64).unwrap_or(61);
            Ok((
                "/help".into(),
                "hermes.sessions".into(),
                format!("⚡ Hermes AI Studio 在线（已加载 {count} 条斜杠指令与持久记忆仓）。"),
                res,
            ))
        }
        _ => Ok((
            "/run".into(),
            format!("{plugin_id}.invoke"),
            format!("✅ 已通过 {plugin_id} 本地引擎处理输入：「{input}」"),
            json!({ "pluginId": plugin_id, "input": input }),
        )),
    }
}

fn collect_surface_bots(conn: &Connection) -> Result<Vec<Value>, String> {
    let mut bots = vec![
        json!({
            "id": "mika-companion-agent",
            "name": "Mika 手机操控智能体",
            "description": "端侧 Android 无障碍屏幕感知、ReAct 规划与自动化操控",
            "miniAppId": null,
            "menuButtonText": "操控面板"
        }),
        json!({
            "id": "hermes-companion-agent",
            "name": "Hermes 深度推理助手",
            "description": "支持 61 条斜杠命令、持久记忆与多分支任务规划",
            "miniAppId": null,
            "menuButtonText": "打开工作台"
        }),
        json!({
            "id": "magpie-router-agent",
            "name": "Magpie 多模型智能路由",
            "description": "51 家大模型最优成本/低延迟自适应路由与隐私脱敏",
            "miniAppId": null,
            "menuButtonText": "路由状态"
        }),
    ];

    let mut stmt = conn
        .prepare("SELECT bot_id, name, description, mini_app_id FROM host_bots ORDER BY created_at_ms DESC")
        .map_err(|e| e.to_string())?;
    let custom: Vec<Value> = stmt
        .query_map([], |r| {
            Ok(json!({
                "id": r.get::<_, String>(0)?,
                "name": r.get::<_, String>(1)?,
                "description": r.get::<_, String>(2)?,
                "miniAppId": r.get::<_, Option<String>>(3)?,
                "menuButtonText": null
            }))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    bots.extend(custom);
    Ok(bots)
}

