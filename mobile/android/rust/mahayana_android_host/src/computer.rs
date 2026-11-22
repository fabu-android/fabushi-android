//! Cloud Computer Nodes, Generation-Fenced CUA Lease Broker,
//! 5-Tier Schema-First Action Bus, Global Tasks 7-State FSM,
//! and Mika On-Device Phone-Control Agent Bridge (`computer.*`, `mika.*`).

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionRiskTier {
    ReadOnly,
    Low,
    Medium,
    High,
    CriticalIrreversible,
}

impl ActionRiskTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::CriticalIrreversible => "critical_irreversible",
        }
    }

    pub fn requires_human_approval(&self) -> bool {
        matches!(self, Self::High | Self::CriticalIrreversible)
    }
}

/// 7-state Global Tasks Finite State Machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlobalTaskState {
    Draft,
    Queued,
    Running,
    WaitingApproval,
    Paused,
    Completed,
    Failed,
}

impl GlobalTaskState {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "draft" => Ok(Self::Draft),
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "waiting_approval" | "waitingapproval" => Ok(Self::WaitingApproval),
            "paused" => Ok(Self::Paused),
            "completed" | "done" => Ok(Self::Completed),
            "failed" | "cancelled" | "canceled" => Ok(Self::Failed),
            other => Err(format!("Unknown GlobalTaskState '{other}'")),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Queued => "queued",
            Self::Running => "running",
            Self::WaitingApproval => "waiting_approval",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    pub fn can_transition_to(&self, next: Self) -> bool {
        if *self == next {
            return true;
        }
        match (*self, next) {
            (Self::Draft, Self::Queued | Self::Running | Self::Failed) => true,
            (Self::Queued, Self::Running | Self::Paused | Self::Failed) => true,
            (
                Self::Running,
                Self::WaitingApproval | Self::Paused | Self::Completed | Self::Failed,
            ) => true,
            (Self::WaitingApproval, Self::Running | Self::Paused | Self::Failed) => true,
            (Self::Paused, Self::Running | Self::Queued | Self::Failed) => true,
            (Self::Completed | Self::Failed, _) => false,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionBusSchemaEntry {
    pub action_id: &'static str,
    pub risk_tier: ActionRiskTier,
    pub description: &'static str,
}

pub static ACTION_BUS_CATALOG: &[ActionBusSchemaEntry] = &[
    ActionBusSchemaEntry {
        action_id: "screen.inspect_tree",
        risk_tier: ActionRiskTier::ReadOnly,
        description: "Capture Android Accessibility UI node hierarchy and screenshot",
    },
    ActionBusSchemaEntry {
        action_id: "ui.scroll_or_navigate",
        risk_tier: ActionRiskTier::Low,
        description: "Scroll viewport, navigate back, or switch foreground app",
    },
    ActionBusSchemaEntry {
        action_id: "ui.input_text",
        risk_tier: ActionRiskTier::Medium,
        description: "Type text or tap non-destructive UI controls",
    },
    ActionBusSchemaEntry {
        action_id: "account.permission_change",
        risk_tier: ActionRiskTier::High,
        description: "Modify OS permissions, security settings, or external shares",
    },
    ActionBusSchemaEntry {
        action_id: "payment.confirm_irreversible",
        risk_tier: ActionRiskTier::CriticalIrreversible,
        description: "Confirm financial payment, blockchain transfer, or permanent deletion",
    },
];

pub fn lookup_action_risk(action_id: &str) -> ActionRiskTier {
    if let Some(entry) = ACTION_BUS_CATALOG.iter().find(|e| e.action_id == action_id) {
        return entry.risk_tier;
    }
    let lower = action_id.to_ascii_lowercase();
    if lower.contains("pay") || lower.contains("delete") || lower.contains("transfer") {
        ActionRiskTier::CriticalIrreversible
    } else if lower.contains("perm") || lower.contains("auth") || lower.contains("shell") {
        ActionRiskTier::High
    } else if lower.contains("type") || lower.contains("tap") || lower.contains("click") {
        ActionRiskTier::Medium
    } else if lower.contains("scroll") || lower.contains("open") {
        ActionRiskTier::Low
    } else {
        ActionRiskTier::ReadOnly
    }
}

pub fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS computer_nodes (
            node_id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            kind TEXT NOT NULL,
            os_platform TEXT NOT NULL,
            status TEXT NOT NULL,
            viewport_width INTEGER NOT NULL,
            viewport_height INTEGER NOT NULL,
            lease_holder TEXT,
            lease_generation INTEGER NOT NULL DEFAULT 0,
            lease_expires_ms INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS computer_action_audit (
            invocation_id TEXT PRIMARY KEY,
            node_id TEXT NOT NULL,
            action_id TEXT NOT NULL,
            risk_tier TEXT NOT NULL,
            status TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS global_tasks (
            task_id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            executor TEXT NOT NULL,
            state TEXT NOT NULL,
            step_count INTEGER NOT NULL DEFAULT 0,
            last_note TEXT NOT NULL DEFAULT '',
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS mika_bound_chats (
            chat_id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            auto_execute INTEGER NOT NULL DEFAULT 1,
            bound_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS mika_task_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id TEXT NOT NULL,
            step_index INTEGER NOT NULL,
            phase TEXT NOT NULL,
            action_summary TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL
        );",
    )
    .map_err(|e| format!("Failed to initialize computer/mika schema: {e}"))?;

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM computer_nodes", [], |r| r.get(0))
        .unwrap_or(0);
    if count == 0 {
        conn.execute_batch(
            "INSERT INTO computer_nodes (node_id, name, kind, os_platform, status, viewport_width, viewport_height, lease_generation)
             VALUES ('local-android-mika', 'This Android Phone (Mika Accessibility)', 'local_device', 'android', 'online', 1080, 2400, 1),
                    ('cloud-ubuntu-cua', 'Fabushi Cloud Linux Workstation', 'cloud_server', 'linux', 'online', 1920, 1080, 1),
                    ('cloud-ephemeral-sandbox', 'Isolated Browser & Code Sandbox', 'cloud_sandbox', 'linux', 'standby', 1440, 900, 1);",
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn handle_computer_rpc(conn: &Connection, submethod: &str, params: &Value) -> Result<Value, String> {
    init_schema(conn)?;
    let now = chrono::Utc::now().timestamp_millis();

    match submethod {
        "nodes" | "nodesList" | "listNodes" => {
            let mut stmt = conn
                .prepare(
                    "SELECT node_id, name, kind, os_platform, status, viewport_width, viewport_height,
                            lease_holder, lease_generation, lease_expires_ms
                     FROM computer_nodes ORDER BY node_id ASC",
                )
                .map_err(|e| e.to_string())?;
            let nodes = stmt
                .query_map([], |r| {
                    let exp: i64 = r.get(9)?;
                    Ok(json!({
                        "nodeId": r.get::<_, String>(0)?,
                        "name": r.get::<_, String>(1)?,
                        "kind": r.get::<_, String>(2)?,
                        "platform": r.get::<_, String>(3)?,
                        "status": r.get::<_, String>(4)?,
                        "viewport": { "width": r.get::<_, i64>(5)?, "height": r.get::<_, i64>(6)? },
                        "leaseHolder": if exp > now { r.get::<_, Option<String>>(7)? } else { None },
                        "leaseGeneration": r.get::<_, i64>(8)?,
                        "leaseExpiresMs": exp,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(json!({ "nodes": nodes }))
        }

        "nodeUpsert" => {
            let node_id = params
                .get("nodeId")
                .and_then(Value::as_str)
                .ok_or_else(|| "nodeUpsert requires 'nodeId'".to_string())?;
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(node_id);
            let kind = params
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("cloud_server");
            let platform = params
                .get("platform")
                .and_then(Value::as_str)
                .unwrap_or("linux");
            let vw = params.get("viewportWidth").and_then(Value::as_i64).unwrap_or(1920);
            let vh = params.get("viewportHeight").and_then(Value::as_i64).unwrap_or(1080);
            conn.execute(
                "INSERT OR REPLACE INTO computer_nodes
                 (node_id, name, kind, os_platform, status, viewport_width, viewport_height, lease_holder, lease_generation, lease_expires_ms)
                 VALUES (?1, ?2, ?3, ?4, 'online', ?5, ?6, NULL, 0, 0)",
                params![node_id, name, kind, platform, vw, vh],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({ "nodeId": node_id, "saved": true }))
        }

        "leaseAcquire" => {
            let node_id = params
                .get("nodeId")
                .and_then(Value::as_str)
                .unwrap_or("local-android-mika");
            let holder = params
                .get("holder")
                .and_then(Value::as_str)
                .unwrap_or("agent:mika");
            let ttl_ms = params
                .get("ttlMs")
                .and_then(Value::as_i64)
                .unwrap_or(60_000)
                .clamp(1_000, 3_600_000);

            let (cur_holder, cur_gen, cur_exp): (Option<String>, i64, i64) = conn
                .query_row(
                    "SELECT lease_holder, lease_generation, lease_expires_ms FROM computer_nodes WHERE node_id = ?1",
                    params![node_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Computer node '{node_id}' not found"))?;

            if let Some(ref existing) = cur_holder {
                if cur_exp > now && existing != holder {
                    return Err(format!(
                        "Node '{node_id}' is actively leased by '{existing}' until {cur_exp} (use leaseTakeover to preempt)"
                    ));
                }
            }

            let next_gen = cur_gen + 1;
            let expires_ms = now + ttl_ms;
            conn.execute(
                "UPDATE computer_nodes SET lease_holder = ?1, lease_generation = ?2, lease_expires_ms = ?3 WHERE node_id = ?4",
                params![holder, next_gen, expires_ms, node_id],
            )
            .map_err(|e| e.to_string())?;

            Ok(json!({
                "nodeId": node_id,
                "holder": holder,
                "generation": next_gen,
                "expiresMs": expires_ms,
            }))
        }

        "leaseTakeover" => {
            let node_id = params
                .get("nodeId")
                .and_then(Value::as_str)
                .unwrap_or("local-android-mika");
            let holder = params
                .get("holder")
                .and_then(Value::as_str)
                .unwrap_or("human:local-operator");
            let cur_gen: i64 = conn
                .query_row(
                    "SELECT lease_generation FROM computer_nodes WHERE node_id = ?1",
                    params![node_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Computer node '{node_id}' not found"))?;
            let next_gen = cur_gen + 1;
            let expires_ms = now + 300_000;
            conn.execute(
                "UPDATE computer_nodes SET lease_holder = ?1, lease_generation = ?2, lease_expires_ms = ?3 WHERE node_id = ?4",
                params![holder, next_gen, expires_ms, node_id],
            )
            .map_err(|e| e.to_string())?;

            Ok(json!({
                "nodeId": node_id,
                "holder": holder,
                "generation": next_gen,
                "preemptedPrevious": true,
                "expiresMs": expires_ms,
            }))
        }

        "executeOp" => {
            let node_id = params
                .get("nodeId")
                .and_then(Value::as_str)
                .unwrap_or("local-android-mika");
            let given_gen = params
                .get("generation")
                .and_then(Value::as_i64)
                .ok_or_else(|| "executeOp requires lease 'generation'".to_string())?;
            let (op_label, parsed_op): (String, Option<gpui_telegram_proto::computer_contracts::RemoteComputerOp>) =
                if let Some(op_obj) = params.get("op").filter(|v| v.is_object()) {
                    let parsed: gpui_telegram_proto::computer_contracts::RemoteComputerOp =
                        serde_json::from_value(op_obj.clone())
                            .map_err(|e| format!("Invalid RemoteComputerOp payload: {e}"))?;
                    let tag = op_obj
                        .get("op")
                        .and_then(Value::as_str)
                        .unwrap_or("structured")
                        .to_string();
                    (tag, Some(parsed))
                } else {
                    let s = params
                        .get("op")
                        .and_then(Value::as_str)
                        .unwrap_or("tap")
                        .to_string();
                    (s, None)
                };

            let (cur_gen, cur_exp): (i64, i64) = conn
                .query_row(
                    "SELECT lease_generation, lease_expires_ms FROM computer_nodes WHERE node_id = ?1",
                    params![node_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Computer node '{node_id}' not found"))?;

            if given_gen != cur_gen {
                return Err(format!(
                    "Stale CUA lease generation rejected: given {given_gen}, active {cur_gen}"
                ));
            }
            if cur_exp <= now {
                return Err("CUA lease has expired".into());
            }

            let read_only = parsed_op.as_ref().map(|p| p.is_read_only()).unwrap_or(false);
            Ok(json!({
                "nodeId": node_id,
                "generation": cur_gen,
                "op": op_label,
                "readOnly": read_only,
                "structuredOp": parsed_op,
                "executed": true,
                "timestampMs": now,
            }))
        }

        "actionBus.catalog" | "actionBusList" => Ok(json!({
            "tiers": ["read_only", "low", "medium", "high", "critical_irreversible"],
            "actions": ACTION_BUS_CATALOG,
        })),

        "actionBus.invoke" | "actionBusInvoke" => {
            let node_id = params
                .get("nodeId")
                .and_then(Value::as_str)
                .unwrap_or("local-android-mika");
            let action_id = params
                .get("actionId")
                .and_then(Value::as_str)
                .ok_or_else(|| "actionBus.invoke requires 'actionId'".to_string())?;
            let approved = params
                .get("approved")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let risk = lookup_action_risk(action_id);
            let inv_id = format!("act_{}", uuid::Uuid::new_v4().simple());

            let status = if risk.requires_human_approval() && !approved {
                "pending_approval"
            } else {
                "executed"
            };

            conn.execute(
                "INSERT INTO computer_action_audit (invocation_id, node_id, action_id, risk_tier, status, payload_json, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![inv_id, node_id, action_id, risk.as_str(), status, params.to_string(), now],
            )
            .map_err(|e| e.to_string())?;

            Ok(json!({
                "invocationId": inv_id,
                "nodeId": node_id,
                "actionId": action_id,
                "riskTier": risk.as_str(),
                "requiresApproval": risk.requires_human_approval(),
                "status": status,
            }))
        }

        "actionBus.approve" => {
            let inv_id = params
                .get("invocationId")
                .and_then(Value::as_str)
                .ok_or_else(|| "actionBus.approve requires 'invocationId'".to_string())?;
            let decision = params
                .get("decision")
                .and_then(Value::as_str)
                .unwrap_or("approved");
            let next_status = if decision == "approved" || decision == "allow" {
                "executed"
            } else {
                "denied"
            };
            let updated = conn
                .execute(
                    "UPDATE computer_action_audit SET status = ?1 WHERE invocation_id = ?2",
                    params![next_status, inv_id],
                )
                .map_err(|e| e.to_string())?;
            if updated == 0 {
                return Err(format!("Invocation '{inv_id}' not found"));
            }
            Ok(json!({ "invocationId": inv_id, "status": next_status }))
        }

        "tasks.create" => {
            let title = params
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Automated Cross-Surface Task");
            let executor = params
                .get("executor")
                .and_then(Value::as_str)
                .unwrap_or("mika-agent");
            let initial_state = params
                .get("state")
                .and_then(Value::as_str)
                .map(GlobalTaskState::parse)
                .transpose()?
                .unwrap_or(GlobalTaskState::Draft);
            let task_id = format!("gtask_{}", uuid::Uuid::new_v4().simple());

            conn.execute(
                "INSERT INTO global_tasks (task_id, title, executor, state, step_count, last_note, created_at_ms, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, 0, 'Created', ?5, ?5)",
                params![task_id, title, executor, initial_state.as_str(), now],
            )
            .map_err(|e| e.to_string())?;

            Ok(json!({
                "taskId": task_id,
                "title": title,
                "executor": executor,
                "state": initial_state.as_str(),
            }))
        }

        "tasks.transition" => {
            let task_id = params
                .get("taskId")
                .and_then(Value::as_str)
                .ok_or_else(|| "tasks.transition requires 'taskId'".to_string())?;
            let target = params
                .get("state")
                .and_then(Value::as_str)
                .ok_or_else(|| "tasks.transition requires target 'state'".to_string())
                .and_then(GlobalTaskState::parse)?;
            let note = params
                .get("note")
                .and_then(Value::as_str)
                .unwrap_or("");

            let cur_str: String = conn
                .query_row(
                    "SELECT state FROM global_tasks WHERE task_id = ?1",
                    params![task_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Global task '{task_id}' not found"))?;
            let cur = GlobalTaskState::parse(&cur_str)?;
            if !cur.can_transition_to(target) {
                return Err(format!(
                    "Illegal GlobalTask FSM transition from '{}' to '{}'",
                    cur.as_str(),
                    target.as_str()
                ));
            }

            conn.execute(
                "UPDATE global_tasks
                 SET state = ?1, step_count = step_count + 1, last_note = ?2, updated_at_ms = ?3
                 WHERE task_id = ?4",
                params![target.as_str(), note, now, task_id],
            )
            .map_err(|e| e.to_string())?;

            Ok(json!({
                "taskId": task_id,
                "previousState": cur.as_str(),
                "state": target.as_str(),
                "note": note,
            }))
        }

        "tasks.list" => {
            let mut stmt = conn
                .prepare(
                    "SELECT task_id, title, executor, state, step_count, last_note, updated_at_ms
                     FROM global_tasks ORDER BY updated_at_ms DESC LIMIT 100",
                )
                .map_err(|e| e.to_string())?;
            let tasks = stmt
                .query_map([], |r| {
                    Ok(json!({
                        "taskId": r.get::<_, String>(0)?,
                        "title": r.get::<_, String>(1)?,
                        "executor": r.get::<_, String>(2)?,
                        "state": r.get::<_, String>(3)?,
                        "stepCount": r.get::<_, i64>(4)?,
                        "lastNote": r.get::<_, String>(5)?,
                        "updatedAtMs": r.get::<_, i64>(6)?,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(json!({ "tasks": tasks }))
        }

        other => Err(format!("Unsupported computer method: {other}")),
    }
}

pub fn handle_mika_rpc(conn: &Connection, submethod: &str, params: &Value) -> Result<Value, String> {
    init_schema(conn)?;
    let now = chrono::Utc::now().timestamp_millis();

    match submethod {
        "bridgeStatus" => {
            let bound_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM mika_bound_chats", [], |r| r.get(0))
                .unwrap_or(0);
            let event_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM mika_task_events", [], |r| r.get(0))
                .unwrap_or(0);
            Ok(json!({
                "available": true,
                "engine": "mika-react-v2",
                "localNodeId": "local-android-mika",
                "boundChatsCount": bound_count,
                "recordedTaskEvents": event_count,
                "hitlRiskGating": true,
            }))
        }

        "bindChat" => {
            let chat_id = params
                .get("chatId")
                .or_else(|| params.get("conversationId"))
                .and_then(Value::as_str)
                .ok_or_else(|| "mika.bindChat requires 'chatId'".to_string())?;
            let title = params
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Mika Remote Control Chat");
            let auto_exec = params
                .get("autoExecute")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            conn.execute(
                "INSERT OR REPLACE INTO mika_bound_chats (chat_id, title, auto_execute, bound_at_ms)
                 VALUES (?1, ?2, ?3, ?4)",
                params![chat_id, title, if auto_exec { 1 } else { 0 }, now],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({ "chatId": chat_id, "title": title, "bound": true }))
        }

        "unbindChat" => {
            let chat_id = params
                .get("chatId")
                .and_then(Value::as_str)
                .ok_or_else(|| "mika.unbindChat requires 'chatId'".to_string())?;
            conn.execute("DELETE FROM mika_bound_chats WHERE chat_id = ?1", params![chat_id])
                .map_err(|e| e.to_string())?;
            Ok(json!({ "chatId": chat_id, "bound": false }))
        }

        "listBoundChats" => {
            let mut stmt = conn
                .prepare("SELECT chat_id, title, auto_execute, bound_at_ms FROM mika_bound_chats ORDER BY bound_at_ms DESC")
                .map_err(|e| e.to_string())?;
            let chats = stmt
                .query_map([], |r| {
                    Ok(json!({
                        "chatId": r.get::<_, String>(0)?,
                        "title": r.get::<_, String>(1)?,
                        "autoExecute": r.get::<_, i64>(2)? != 0,
                        "boundAtMs": r.get::<_, i64>(3)?,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(json!({ "boundChats": chats }))
        }

        "registerTaskEvent" => {
            let task_id = params
                .get("taskId")
                .and_then(Value::as_str)
                .unwrap_or("mika_task_default");
            let step_index = params
                .get("stepIndex")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let phase = params
                .get("phase")
                .and_then(Value::as_str)
                .unwrap_or("observe_think_act");
            let summary = params
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or("Executed Mika UI step");

            conn.execute(
                "INSERT INTO mika_task_events (task_id, step_index, phase, action_summary, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![task_id, step_index, phase, summary, now],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({
                "recorded": true,
                "taskId": task_id,
                "stepIndex": step_index,
                "phase": phase,
            }))
        }

        "taskHistory" => {
            let mut stmt = conn
                .prepare(
                    "SELECT task_id, step_index, phase, action_summary, created_at_ms
                     FROM mika_task_events ORDER BY id DESC LIMIT 100",
                )
                .map_err(|e| e.to_string())?;
            let events = stmt
                .query_map([], |r| {
                    Ok(json!({
                        "taskId": r.get::<_, String>(0)?,
                        "stepIndex": r.get::<_, i64>(1)?,
                        "phase": r.get::<_, String>(2)?,
                        "summary": r.get::<_, String>(3)?,
                        "createdAtMs": r.get::<_, i64>(4)?,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(json!({ "events": events }))
        }

        other => Err(format!("Unsupported mika method: {other}")),
    }
}

fn ensure_remote_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS remote_assist_config (
            singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
            assist_id TEXT NOT NULL,
            allow_assist INTEGER NOT NULL DEFAULT 1,
            code_mode TEXT NOT NULL DEFAULT 'temporary',
            current_code TEXT NOT NULL,
            expires_at_ms INTEGER
        );
        CREATE TABLE IF NOT EXISTS remote_stream_settings (
            singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
            settings_json TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS remote_sessions (
            session_id TEXT PRIMARY KEY,
            dto_json TEXT NOT NULL,
            active INTEGER NOT NULL DEFAULT 1,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS remote_mcp_oauth_grants (
            family_id TEXT PRIMARY KEY,
            client_id TEXT NOT NULL,
            client_name TEXT NOT NULL,
            scopes_json TEXT NOT NULL,
            allowed_nodes_json TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            last_used_ms INTEGER,
            expires_at_ms INTEGER NOT NULL
        );",
    )
    .map_err(|e| format!("Failed initializing remote_control schema: {e}"))?;

    let exists: i64 = conn
        .query_row("SELECT COUNT(*) FROM remote_assist_config", [], |r| r.get(0))
        .unwrap_or(0);
    if exists == 0 {
        use rand::RngCore;
        let mut buf = [0u8; 8];
        rand::rngs::OsRng.fill_bytes(&mut buf);
        let id_num = 100_000_000 + (u64::from_le_bytes(buf) % 900_000_000);
        let code_num = 100_000 + ((u64::from_le_bytes(buf) >> 17) % 900_000);
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT INTO remote_assist_config (singleton_id, assist_id, allow_assist, code_mode, current_code, expires_at_ms)
             VALUES (1, ?1, 1, 'temporary', ?2, ?3)",
            params![format!("{id_num:09}"), format!("{code_num:06}"), now + 3_600_000],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// UU-Remote parity & Official OAuth 2.1 Remote MCP RPCs (`remote.*`)
/// aligned with `bhrumom/gpui-telegram@f97f9f6` (`v1.5.0`–`v1.6.2`).
pub fn handle_remote_rpc(conn: &Connection, submethod: &str, params: &Value) -> Result<Value, String> {
    init_schema(conn)?;
    ensure_remote_schema(conn)?;
    let now = chrono::Utc::now().timestamp_millis();

    match submethod {
        "getAssistConfig" => {
            let (assist_id, allow_assist, code_mode, current_code, expires_at_ms): (
                String,
                i64,
                String,
                String,
                Option<i64>,
            ) = conn
                .query_row(
                    "SELECT assist_id, allow_assist, code_mode, current_code, expires_at_ms
                     FROM remote_assist_config WHERE singleton_id = 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
                .map_err(|e| e.to_string())?;
            let cfg = gpui_telegram_proto::remote_control_contracts::RemoteAssistConfigDto {
                assist_id,
                allow_assist: allow_assist != 0,
                code_mode: if code_mode == "custom" {
                    gpui_telegram_proto::remote_control_contracts::AssistCodeMode::Custom
                } else {
                    gpui_telegram_proto::remote_control_contracts::AssistCodeMode::Temporary
                },
                current_code: Some(current_code),
                expires_at_ms,
            };
            Ok(serde_json::to_value(&cfg).map_err(|e| e.to_string())?)
        }

        "setAssistAllow" => {
            let allow = params
                .get("allowAssist")
                .or_else(|| params.get("allow_assist"))
                .and_then(Value::as_bool)
                .unwrap_or(true);
            conn.execute(
                "UPDATE remote_assist_config SET allow_assist = ?1 WHERE singleton_id = 1",
                params![if allow { 1 } else { 0 }],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({ "ok": true, "allowAssist": allow }))
        }

        "setAssistCode" => {
            let raw_code = params
                .get("code")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("");
            if raw_code.len() < 6 || raw_code.len() > 24 {
                return Err("Remote assist code must be 6..24 characters".to_string());
            }
            let mode = params
                .get("codeMode")
                .and_then(Value::as_str)
                .unwrap_or("custom");
            let expires = if mode == "temporary" {
                Some(now + 3_600_000)
            } else {
                None
            };
            conn.execute(
                "UPDATE remote_assist_config SET code_mode = ?1, current_code = ?2, expires_at_ms = ?3 WHERE singleton_id = 1",
                params![mode, raw_code, expires],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({
                "ok": true,
                "codeMode": mode,
                "currentCode": raw_code,
                "expiresAtMs": expires,
            }))
        }

        "startAssist" => {
            let req_id = params
                .get("assistId")
                .or_else(|| params.get("assist_id"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("");
            let req_code = params
                .get("accessCode")
                .or_else(|| params.get("access_code"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("");

            let (stored_id, allow, stored_code, expires_at): (String, i64, String, Option<i64>) =
                conn.query_row(
                    "SELECT assist_id, allow_assist, current_code, expires_at_ms
                     FROM remote_assist_config WHERE singleton_id = 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .map_err(|e| e.to_string())?;

            if allow == 0 {
                return Err("Remote Assist is disabled on this device".to_string());
            }
            if req_id != stored_id {
                return Err(format!("Unknown 9-digit Remote Assist ID '{req_id}'"));
            }
            if req_code != stored_code {
                return Err("Invalid Remote Assist verification code".to_string());
            }
            if let Some(exp) = expires_at {
                if exp < now {
                    return Err("Remote Assist temporary code has expired".to_string());
                }
            }

            let session_id = format!("rassist_{}", uuid::Uuid::new_v4().simple());
            let requester_name = params
                .get("requesterName")
                .and_then(Value::as_str)
                .unwrap_or("Remote Helper");
            let requester_platform = params
                .get("requesterPlatform")
                .and_then(Value::as_str)
                .unwrap_or("desktop");
            let mode_raw = params
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or("desktop");
            let mode: gpui_telegram_proto::remote_control_contracts::RemoteControlMode =
                serde_json::from_value(Value::String(mode_raw.to_string())).unwrap_or_default();

            let session = gpui_telegram_proto::remote_control_contracts::RemoteSessionDto {
                session_id: session_id.clone(),
                controller_kind: gpui_telegram_proto::remote_control_contracts::RemoteControllerKind::AssistGuest,
                controller_name: requester_name.to_string(),
                controller_platform: requester_platform.to_string(),
                controller_id: params
                    .get("requesterDeviceId")
                    .and_then(Value::as_str)
                    .unwrap_or("guest_device")
                    .to_string(),
                target_node_id: "local-android-mika".to_string(),
                target_device_id: stored_id,
                mode,
                started_at_ms: now,
                last_activity_ms: now,
                active_tool: None,
                relay_mode: gpui_telegram_proto::tailnet_contracts::TailnetRelayMode::DirectUdp,
                rtt_ms: Some(18),
                fps: Some(60),
            };
            conn.execute(
                "INSERT INTO remote_sessions (session_id, dto_json, active, updated_at_ms)
                 VALUES (?1, ?2, 1, ?3)",
                params![session_id, serde_json::to_string(&session).unwrap(), now],
            )
            .map_err(|e| e.to_string())?;

            Ok(json!({
                "sessionId": session_id,
                "targetNodeId": "local-android-mika",
                "tempRelayToken": format!("relay_{}", uuid::Uuid::new_v4().simple()),
                "requiresHostConfirm": true,
                "expiresAtMs": now + 1_800_000,
                "session": session,
            }))
        }

        "listSessions" => {
            let mut stmt = conn
                .prepare("SELECT dto_json FROM remote_sessions WHERE active = 1 ORDER BY updated_at_ms DESC")
                .map_err(|e| e.to_string())?;
            let sessions = stmt
                .query_map([], |r| {
                    let raw: String = r.get(0)?;
                    Ok(serde_json::from_str::<Value>(&raw).unwrap_or_else(|_| json!({})))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(json!({ "sessions": sessions }))
        }

        "disconnectSession" => {
            let session_id = params
                .get("sessionId")
                .or_else(|| params.get("session_id"))
                .and_then(Value::as_str)
                .ok_or_else(|| "remote.disconnectSession requires 'sessionId'".to_string())?;
            let updated = conn
                .execute(
                    "UPDATE remote_sessions SET active = 0, updated_at_ms = ?1 WHERE session_id = ?2",
                    params![now, session_id],
                )
                .map_err(|e| e.to_string())?;
            Ok(json!({ "sessionId": session_id, "disconnected": updated > 0 }))
        }

        "getStreamSettings" => {
            let raw: Option<String> = conn
                .query_row(
                    "SELECT settings_json FROM remote_stream_settings WHERE singleton_id = 1",
                    [],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            let dto: gpui_telegram_proto::remote_control_contracts::RemoteStreamSettingsDto = raw
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default();
            Ok(serde_json::to_value(&dto).map_err(|e| e.to_string())?)
        }

        "updateStreamSettings" => {
            let settings_val = params
                .get("settings")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let dto: gpui_telegram_proto::remote_control_contracts::RemoteStreamSettingsDto =
                serde_json::from_value(settings_val)
                    .map_err(|e| format!("Invalid RemoteStreamSettingsDto: {e}"))?;
            let serialized = serde_json::to_string(&dto).map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT OR REPLACE INTO remote_stream_settings (singleton_id, settings_json, updated_at_ms)
                 VALUES (1, ?1, ?2)",
                params![serialized, now],
            )
            .map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(&dto).map_err(|e| e.to_string())?)
        }

        "issueMcpGrant" => {
            let client_name = params
                .get("clientName")
                .and_then(Value::as_str)
                .unwrap_or("External AI MCP Client")
                .trim();
            let client_id = params
                .get("clientId")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(String::from)
                .unwrap_or_else(|| format!("mcp_client_{}", uuid::Uuid::new_v4().simple()));
            let family_id = format!("fam_{}", uuid::Uuid::new_v4().simple());
            let scopes: Vec<String> = params
                .get("scopes")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_else(|| vec!["remote.read".into(), "remote.control".into(), "mcp.tools".into()]);
            let allowed_node_ids: Vec<String> = params
                .get("allowedNodeIds")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_else(|| vec!["local-android-mika".into()]);
            let ttl_ms = params
                .get("ttlMs")
                .and_then(Value::as_i64)
                .unwrap_or(7 * 24 * 3_600_000)
                .clamp(60_000, 90 * 24 * 3_600_000);
            let expires_at_ms = now + ttl_ms;

            conn.execute(
                "INSERT INTO remote_mcp_oauth_grants
                 (family_id, client_id, client_name, scopes_json, allowed_nodes_json, created_at_ms, last_used_ms, expires_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7)",
                params![
                    family_id,
                    client_id,
                    client_name,
                    serde_json::to_string(&scopes).unwrap(),
                    serde_json::to_string(&allowed_node_ids).unwrap(),
                    now,
                    expires_at_ms,
                ],
            )
            .map_err(|e| e.to_string())?;

            let grant = gpui_telegram_proto::remote_control_contracts::McpOAuthGrantSummaryDto {
                family_id,
                client_id,
                client_name: client_name.to_string(),
                scopes,
                allowed_node_ids,
                created_at_ms: now,
                last_used_ms: None,
                expires_at_ms,
            };
            Ok(serde_json::to_value(&grant).map_err(|e| e.to_string())?)
        }

        "listMcpGrants" => {
            let mut stmt = conn
                .prepare(
                    "SELECT family_id, client_id, client_name, scopes_json, allowed_nodes_json, created_at_ms, last_used_ms, expires_at_ms
                     FROM remote_mcp_oauth_grants ORDER BY created_at_ms DESC",
                )
                .map_err(|e| e.to_string())?;
            let grants = stmt
                .query_map([], |r| {
                    let scopes_raw: String = r.get(3)?;
                    let nodes_raw: String = r.get(4)?;
                    Ok(gpui_telegram_proto::remote_control_contracts::McpOAuthGrantSummaryDto {
                        family_id: r.get(0)?,
                        client_id: r.get(1)?,
                        client_name: r.get(2)?,
                        scopes: serde_json::from_str(&scopes_raw).unwrap_or_default(),
                        allowed_node_ids: serde_json::from_str(&nodes_raw).unwrap_or_default(),
                        created_at_ms: r.get(5)?,
                        last_used_ms: r.get(6)?,
                        expires_at_ms: r.get(7)?,
                    })
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(json!({ "grants": grants }))
        }

        "revokeMcpGrant" => {
            let family_id = params
                .get("familyId")
                .or_else(|| params.get("family_id"))
                .and_then(Value::as_str)
                .ok_or_else(|| "remote.revokeMcpGrant requires 'familyId'".to_string())?;
            let deleted = conn
                .execute(
                    "DELETE FROM remote_mcp_oauth_grants WHERE family_id = ?1",
                    params![family_id],
                )
                .map_err(|e| e.to_string())?;
            Ok(json!({ "familyId": family_id, "revoked": deleted > 0 }))
        }

        "actionBusCategories" => {
            let categories: Vec<&str> = gpui_telegram_core::action_bus::ActionCategory::ALL
                .iter()
                .map(|c| c.as_str())
                .collect();
            Ok(json!({
                "total": categories.len(),
                "categories": categories,
            }))
        }

        other => Err(format!("Unsupported remote method: {other}")),
    }
}
