//! Third-Party Coding Agents (Claude Code, Codex, OpenCode, Cursor CLI, Antigravity, Custom ACP)
//! as Native Telegram Bots (`codingAgent.*`).
//! Aligned with `bhrumom/gpui-telegram@9a5e90e` (`gpui_telegram_proto::coding_agent_contracts`).

use gpui_telegram_proto::coding_agent_contracts::{
    CodingAgentBotConfig, CodingApprovalDecision, CodingControlAction, CodingDiffStat,
    CodingPermissionMode, CodingProviderKind, CodingTokenUsage, DetectedCodingProvider,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

pub fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS coding_agent_bots (
            bot_id TEXT PRIMARY KEY,
            display_name TEXT NOT NULL,
            username TEXT NOT NULL,
            config_json TEXT NOT NULL,
            active_model TEXT,
            active_branch TEXT NOT NULL DEFAULT 'main',
            state TEXT NOT NULL DEFAULT 'idle',
            total_turns INTEGER NOT NULL DEFAULT 0,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS coding_agent_checkpoints (
            checkpoint_id TEXT PRIMARY KEY,
            bot_id TEXT NOT NULL,
            turn_idx INTEGER NOT NULL,
            commit_hash TEXT NOT NULL,
            summary TEXT NOT NULL,
            diff_stat_json TEXT NOT NULL,
            usage_json TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS coding_agent_approvals (
            approval_id TEXT PRIMARY KEY,
            bot_id TEXT NOT NULL,
            tool_name TEXT NOT NULL,
            command_preview TEXT NOT NULL,
            decision TEXT NOT NULL DEFAULT 'pending',
            created_at_ms INTEGER NOT NULL,
            decided_at_ms INTEGER
        );",
    )
    .map_err(|e| format!("Failed initializing coding_agent schema: {e}"))
}

fn provider_login_hint(kind: CodingProviderKind) -> &'static str {
    match kind {
        CodingProviderKind::ClaudeCode => "Run `claude login` on bound host node or configure Anthropic key",
        CodingProviderKind::Codex => "Run `codex login` on bound host node or sync OpenAI OAuth",
        CodingProviderKind::OpenCode => "Install `opencode` CLI on bound host node (ACP native)",
        CodingProviderKind::CursorCli => "Run `cursor-agent login` on bound host node (ACP native)",
        CodingProviderKind::Antigravity => "Bind desktop Antigravity CLI (`agy`) node",
        CodingProviderKind::AcpCustom => "Configure custom ACP executable path & args on bound node",
    }
}

pub fn list_supported_providers() -> Vec<DetectedCodingProvider> {
    CodingProviderKind::ALL
        .iter()
        .map(|kind| DetectedCodingProvider {
            kind: *kind,
            display_name: kind.display_name().to_string(),
            binary_path: kind.default_binary().map(String::from),
            version: None,
            installed: false,
            recommended_username: kind.recommended_username().to_string(),
            supports_acp: kind.supports_acp(),
            login_hint: provider_login_hint(*kind).to_string(),
        })
        .collect()
}

pub fn handle_rpc(conn: &Connection, submethod: &str, params: &Value) -> Result<Value, String> {
    init_schema(conn)?;
    let now = chrono::Utc::now().timestamp_millis();

    match submethod {
        "listProviders" => {
            let providers = list_supported_providers();
            Ok(json!({
                "providers": providers,
                "total": providers.len(),
                "permissionModes": [
                    { "id": "ask", "labelZh": CodingPermissionMode::Ask.label_zh(), "labelEn": CodingPermissionMode::Ask.label_en() },
                    { "id": "auto_edit", "labelZh": CodingPermissionMode::AutoEdit.label_zh(), "labelEn": CodingPermissionMode::AutoEdit.label_en() },
                    { "id": "full_access", "labelZh": CodingPermissionMode::FullAccess.label_zh(), "labelEn": CodingPermissionMode::FullAccess.label_en() }
                ]
            }))
        }

        "listBots" => {
            let mut stmt = conn
                .prepare(
                    "SELECT bot_id, display_name, username, config_json, active_model, active_branch, state, total_turns, updated_at_ms
                     FROM coding_agent_bots ORDER BY updated_at_ms DESC",
                )
                .map_err(|e| e.to_string())?;
            let bots = stmt
                .query_map([], |r| {
                    let cfg_raw: String = r.get(3)?;
                    let cfg: CodingAgentBotConfig =
                        serde_json::from_str(&cfg_raw).unwrap_or_default();
                    Ok(json!({
                        "botId": r.get::<_, String>(0)?,
                        "displayName": r.get::<_, String>(1)?,
                        "username": r.get::<_, String>(2)?,
                        "config": cfg,
                        "activeModel": r.get::<_, Option<String>>(4)?,
                        "activeBranch": r.get::<_, String>(5)?,
                        "state": r.get::<_, String>(6)?,
                        "totalTurns": r.get::<_, i64>(7)?,
                        "updatedAtMs": r.get::<_, i64>(8)?,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(json!({ "bots": bots }))
        }

        "createBot" => {
            let provider_raw = params
                .get("providerKind")
                .or_else(|| params.get("provider_kind"))
                .and_then(Value::as_str)
                .unwrap_or("claude_code");
            let provider = CodingProviderKind::parse(provider_raw)
                .ok_or_else(|| format!("Unsupported coding provider '{provider_raw}'"))?;
            let perm_raw = params
                .get("permissionMode")
                .or_else(|| params.get("permission_mode"))
                .and_then(Value::as_str)
                .unwrap_or("ask");
            let permission_mode = CodingPermissionMode::parse(perm_raw)
                .ok_or_else(|| format!("Unsupported coding permission mode '{perm_raw}'"))?;

            let workspace_path = params
                .get("workspacePath")
                .or_else(|| params.get("workspace_path"))
                .and_then(Value::as_str)
                .unwrap_or("~/workspace")
                .trim()
                .to_string();
            let display_name = params
                .get("displayName")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .unwrap_or_else(|| format!("{} Coding Bot", provider.display_name()));
            let raw_username = params
                .get("username")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(provider.recommended_username());
            let clean_handle = raw_username.trim_start_matches('@');
            let username = if clean_handle.ends_with("_bot") {
                clean_handle.to_string()
            } else {
                format!("{clean_handle}_bot")
            };

            let config = CodingAgentBotConfig {
                provider_kind: provider,
                workspace_path,
                use_worktree: params
                    .get("useWorktree")
                    .or_else(|| params.get("use_worktree"))
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                permission_mode,
                executable_override: params
                    .get("executableOverride")
                    .and_then(Value::as_str)
                    .map(String::from),
                extra_args: params
                    .get("extraArgs")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(Value::as_str)
                            .map(String::from)
                            .collect()
                    })
                    .unwrap_or_default(),
                bound_device_id: params
                    .get("boundDeviceId")
                    .and_then(Value::as_str)
                    .map(String::from),
                bound_node_id: params
                    .get("boundNodeId")
                    .and_then(Value::as_str)
                    .map(String::from),
                model_override: params
                    .get("modelOverride")
                    .and_then(Value::as_str)
                    .map(String::from),
                allow_group_members: params
                    .get("allowGroupMembers")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            };
            let bot_id = params
                .get("botId")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(String::from)
                .unwrap_or_else(|| format!("cbot_{}", uuid::Uuid::new_v4().simple()));
            let cfg_json = serde_json::to_string(&config).map_err(|e| e.to_string())?;
            let active_branch = if config.use_worktree {
                format!("coding/{}", provider.as_str())
            } else {
                "main".to_string()
            };

            conn.execute(
                "INSERT OR REPLACE INTO coding_agent_bots
                 (bot_id, display_name, username, config_json, active_model, active_branch, state, total_turns, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'idle', 0, ?7)",
                params![
                    bot_id,
                    display_name,
                    username,
                    cfg_json,
                    config.model_override,
                    active_branch,
                    now,
                ],
            )
            .map_err(|e| e.to_string())?;

            Ok(json!({
                "botId": bot_id,
                "displayName": display_name,
                "username": username,
                "config": config,
                "activeBranch": active_branch,
                "state": "idle",
            }))
        }

        "patchBot" => {
            let bot_id = params
                .get("botId")
                .and_then(Value::as_str)
                .ok_or_else(|| "codingAgent.patchBot requires 'botId'".to_string())?;
            let existing_cfg_raw: String = conn
                .query_row(
                    "SELECT config_json FROM coding_agent_bots WHERE bot_id = ?1",
                    params![bot_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Coding bot '{bot_id}' not found"))?;
            let mut cfg: CodingAgentBotConfig =
                serde_json::from_str(&existing_cfg_raw).unwrap_or_default();

            if let Some(p) = params.get("workspacePath").and_then(Value::as_str) {
                cfg.workspace_path = p.trim().to_string();
            }
            if let Some(wt) = params.get("useWorktree").and_then(Value::as_bool) {
                cfg.use_worktree = wt;
            }
            if let Some(pm) = params.get("permissionMode").and_then(Value::as_str) {
                cfg.permission_mode = CodingPermissionMode::parse(pm)
                    .ok_or_else(|| format!("Unsupported permissionMode '{pm}'"))?;
            }
            if let Some(node) = params.get("boundNodeId").and_then(Value::as_str) {
                cfg.bound_node_id = if node.trim().is_empty() {
                    None
                } else {
                    Some(node.trim().to_string())
                };
            }
            if let Some(model) = params.get("modelOverride").and_then(Value::as_str) {
                cfg.model_override = if model.trim().is_empty() {
                    None
                } else {
                    Some(model.trim().to_string())
                };
            }
            let cfg_json = serde_json::to_string(&cfg).map_err(|e| e.to_string())?;
            conn.execute(
                "UPDATE coding_agent_bots SET config_json = ?1, active_model = ?2, updated_at_ms = ?3 WHERE bot_id = ?4",
                params![cfg_json, cfg.model_override, now, bot_id],
            )
            .map_err(|e| e.to_string())?;

            Ok(json!({ "botId": bot_id, "config": cfg, "updated": true }))
        }

        "deleteBot" => {
            let bot_id = params
                .get("botId")
                .and_then(Value::as_str)
                .ok_or_else(|| "codingAgent.deleteBot requires 'botId'".to_string())?;
            conn.execute("DELETE FROM coding_agent_bots WHERE bot_id = ?1", params![bot_id])
                .map_err(|e| e.to_string())?;
            Ok(json!({ "botId": bot_id, "deleted": true }))
        }

        "dispatchControl" => {
            let bot_id = params
                .get("botId")
                .and_then(Value::as_str)
                .ok_or_else(|| "codingAgent.dispatchControl requires 'botId'".to_string())?;
            let action_val = params
                .get("control")
                .or_else(|| params.get("actionPayload"))
                .cloned()
                .ok_or_else(|| "codingAgent.dispatchControl requires 'control' CodingControlAction".to_string())?;
            let action: CodingControlAction = serde_json::from_value(action_val)
                .map_err(|e| format!("Invalid CodingControlAction: {e}"))?;

            let existing_cfg_raw: String = conn
                .query_row(
                    "SELECT config_json FROM coding_agent_bots WHERE bot_id = ?1",
                    params![bot_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Coding bot '{bot_id}' not found"))?;
            let mut cfg: CodingAgentBotConfig =
                serde_json::from_str(&existing_cfg_raw).unwrap_or_default();

            let summary = match &action {
                CodingControlAction::SetWorkspace { path } => {
                    cfg.workspace_path = path.clone();
                    conn.execute(
                        "UPDATE coding_agent_bots SET config_json = ?1, updated_at_ms = ?2 WHERE bot_id = ?3",
                        params![serde_json::to_string(&cfg).unwrap(), now, bot_id],
                    )
                    .map_err(|e| e.to_string())?;
                    format!("Workspace updated to {path}")
                }
                CodingControlAction::SetModel { model } => {
                    cfg.model_override = Some(model.clone());
                    conn.execute(
                        "UPDATE coding_agent_bots SET config_json = ?1, active_model = ?2, updated_at_ms = ?3 WHERE bot_id = ?4",
                        params![serde_json::to_string(&cfg).unwrap(), model, now, bot_id],
                    )
                    .map_err(|e| e.to_string())?;
                    format!("Active coding model set to {model}")
                }
                CodingControlAction::SetPermissionMode { mode } => {
                    cfg.permission_mode = *mode;
                    conn.execute(
                        "UPDATE coding_agent_bots SET config_json = ?1, updated_at_ms = ?2 WHERE bot_id = ?3",
                        params![serde_json::to_string(&cfg).unwrap(), now, bot_id],
                    )
                    .map_err(|e| e.to_string())?;
                    format!("Permission mode updated to {}", mode.as_str())
                }
                CodingControlAction::WorktreeOp { op, branch } => {
                    let target_branch = branch
                        .clone()
                        .unwrap_or_else(|| format!("coding/{}-wt", cfg.provider_kind.as_str()));
                    if op == "create" || op == "switch" {
                        conn.execute(
                            "UPDATE coding_agent_bots SET active_branch = ?1, updated_at_ms = ?2 WHERE bot_id = ?3",
                            params![target_branch, now, bot_id],
                        )
                        .map_err(|e| e.to_string())?;
                    }
                    format!("Worktree op '{op}' applied on branch '{target_branch}'")
                }
                CodingControlAction::RollbackCheckpoint { turn_idx } => {
                    let idx = turn_idx.unwrap_or(0) as i64;
                    conn.execute(
                        "DELETE FROM coding_agent_checkpoints WHERE bot_id = ?1 AND turn_idx > ?2",
                        params![bot_id, idx],
                    )
                    .map_err(|e| e.to_string())?;
                    format!("Rolled back checkpoints after turn #{idx}")
                }
                other => format!("Dispatched coding control action: {other:?}"),
            };

            Ok(json!({
                "botId": bot_id,
                "ok": true,
                "summary": summary,
                "config": cfg,
            }))
        }

        "recordTurn" => {
            let bot_id = params
                .get("botId")
                .and_then(Value::as_str)
                .ok_or_else(|| "codingAgent.recordTurn requires 'botId'".to_string())?;
            let summary = params
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or("Applied coding turn");
            let diff_stat: CodingDiffStat = params
                .get("diffStat")
                .cloned()
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();
            let usage: CodingTokenUsage = params
                .get("usage")
                .cloned()
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();

            let next_turn: i64 = conn
                .query_row(
                    "SELECT total_turns FROM coding_agent_bots WHERE bot_id = ?1",
                    params![bot_id],
                    |r| r.get::<_, i64>(0),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Coding bot '{bot_id}' not found"))?
                + 1;
            let checkpoint_id = format!("cp_{}", uuid::Uuid::new_v4().simple());
            let commit_hash = format!("{:07x}", (now as u64 ^ (next_turn as u64 * 2654435761)) & 0x0fff_ffff);

            conn.execute(
                "INSERT INTO coding_agent_checkpoints
                 (checkpoint_id, bot_id, turn_idx, commit_hash, summary, diff_stat_json, usage_json, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    checkpoint_id,
                    bot_id,
                    next_turn,
                    commit_hash,
                    summary,
                    serde_json::to_string(&diff_stat).unwrap(),
                    serde_json::to_string(&usage).unwrap(),
                    now,
                ],
            )
            .map_err(|e| e.to_string())?;
            conn.execute(
                "UPDATE coding_agent_bots SET total_turns = ?1, updated_at_ms = ?2 WHERE bot_id = ?3",
                params![next_turn, now, bot_id],
            )
            .map_err(|e| e.to_string())?;

            Ok(json!({
                "checkpointId": checkpoint_id,
                "botId": bot_id,
                "turnIdx": next_turn,
                "commitHash": commit_hash,
                "diffStat": diff_stat,
                "usage": usage,
            }))
        }

        "listCheckpoints" => {
            let bot_id = params
                .get("botId")
                .and_then(Value::as_str)
                .ok_or_else(|| "codingAgent.listCheckpoints requires 'botId'".to_string())?;
            let mut stmt = conn
                .prepare(
                    "SELECT checkpoint_id, turn_idx, commit_hash, summary, diff_stat_json, usage_json, created_at_ms
                     FROM coding_agent_checkpoints WHERE bot_id = ?1 ORDER BY turn_idx DESC LIMIT 50",
                )
                .map_err(|e| e.to_string())?;
            let items = stmt
                .query_map(params![bot_id], |r| {
                    let diff_raw: String = r.get(4)?;
                    let usage_raw: String = r.get(5)?;
                    Ok(json!({
                        "checkpointId": r.get::<_, String>(0)?,
                        "turnIdx": r.get::<_, i64>(1)?,
                        "commitHash": r.get::<_, String>(2)?,
                        "summary": r.get::<_, String>(3)?,
                        "diffStat": serde_json::from_str::<CodingDiffStat>(&diff_raw).unwrap_or_default(),
                        "usage": serde_json::from_str::<CodingTokenUsage>(&usage_raw).unwrap_or_default(),
                        "createdAtMs": r.get::<_, i64>(6)?,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(json!({ "botId": bot_id, "checkpoints": items }))
        }

        "decideApproval" => {
            let bot_id = params
                .get("botId")
                .and_then(Value::as_str)
                .unwrap_or("default");
            let approval_id = params
                .get("approvalId")
                .and_then(Value::as_str)
                .map(String::from)
                .unwrap_or_else(|| format!("cappr_{}", uuid::Uuid::new_v4().simple()));
            let decision_raw = params
                .get("decision")
                .and_then(Value::as_str)
                .unwrap_or("allow_once");
            let decision: CodingApprovalDecision =
                serde_json::from_value(Value::String(decision_raw.to_string()))
                    .map_err(|e| format!("Invalid CodingApprovalDecision '{decision_raw}': {e}"))?;
            let tool_name = params
                .get("toolName")
                .and_then(Value::as_str)
                .unwrap_or("shell_exec");
            let preview = params
                .get("commandPreview")
                .and_then(Value::as_str)
                .unwrap_or("");
            conn.execute(
                "INSERT OR REPLACE INTO coding_agent_approvals
                 (approval_id, bot_id, tool_name, command_preview, decision, created_at_ms, decided_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                params![approval_id, bot_id, tool_name, preview, decision_raw, now],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({
                "approvalId": approval_id,
                "botId": bot_id,
                "decision": decision,
                "decidedAtMs": now,
            }))
        }

        other => Err(format!("Unsupported codingAgent method: {other}")),
    }
}
