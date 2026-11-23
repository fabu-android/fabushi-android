//! Cloud Computer Nodes, Generation-Fenced CUA Lease Broker,
//! 5-Tier Schema-First Action Bus, Global Tasks 7-State FSM,
//! and Mika On-Device Phone-Control Agent Bridge (`computer.*`, `mika.*`).

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Mutex;

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
        conn.execute(
            "INSERT INTO computer_nodes (node_id, name, kind, os_platform, status, viewport_width, viewport_height, lease_generation)
             VALUES ('local-android-mika', 'This Android Phone (Mika Accessibility)', 'local_device', 'android', 'standby', 1080, 2400, 1)",
            [],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn handle_computer_rpc(
    db: &Mutex<Connection>,
    submethod: &str,
    params: &Value,
) -> Result<Value, String> {
    if submethod == "executeOp" {
        return handle_execute_op(db, params);
    }
    crate::with_db(db, |conn| handle_computer_rpc_local(conn, submethod, params))
}

fn handle_execute_op(db: &Mutex<Connection>, params: &Value) -> Result<Value, String> {
    let now = chrono::Utc::now().timestamp_millis();
    let node_id = params
        .get("nodeId")
        .or_else(|| params.get("node_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("local-android-mika");
    let given_gen = params
        .get("generation")
        .and_then(Value::as_i64)
        .ok_or_else(|| "executeOp requires lease 'generation'".to_string())?;
    let op_raw = params
        .get("op")
        .or_else(|| params.get("operation"))
        .ok_or_else(|| "executeOp requires non-empty 'op'".to_string())?;

    let (op_label, parsed_op): (
        String,
        Option<gpui_telegram_proto::computer_contracts::RemoteComputerOp>,
    ) = if op_raw.is_object() {
        let parsed: gpui_telegram_proto::computer_contracts::RemoteComputerOp =
            serde_json::from_value(op_raw.clone())
                .map_err(|e| format!("Invalid RemoteComputerOp payload: {e}"))?;
        let tag = op_raw
            .get("op")
            .and_then(Value::as_str)
            .unwrap_or("structured")
            .to_string();
        (tag, Some(parsed))
    } else if let Some(s) = op_raw.as_str().map(str::trim).filter(|s| !s.is_empty()) {
        (s.to_string(), None)
    } else {
        return Err("executeOp requires non-empty 'op'".to_string());
    };

    let has_server_auth = crate::with_db(db, |conn| {
        init_schema(conn)?;
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
        Ok(crate::telegram_ext::has_authenticated_server_session(conn))
    })?;

    if node_id == "local-android-mika" {
        return Err(format!(
            "LOCAL_EXECUTOR_REQUIRED: local-android-mika operation '{op_label}' must execute through in-process Mika TaskService/AndroidSandbox (MikaApp.mcp)"
        ));
    }

    if has_server_auth {
        if let Some(ref structured) = parsed_op {
            let op_json = serde_json::to_value(structured).map_err(|e| e.to_string())?;
            return crate::telegram_ext::handle_server_rpc(
                db,
                "execComputer",
                &json!({
                    "nodeId": node_id,
                    "operation": op_json,
                    "callerLabel": params
                        .get("callerLabel")
                        .or_else(|| params.get("caller_label"))
                        .and_then(Value::as_str)
                        .unwrap_or("Fabushi Android"),
                }),
            );
        }
    }

    Err(format!(
        "REMOTE_EXECUTOR_UNAVAILABLE: Execute cloud computer operation '{op_label}' on '{node_id}' via telegram.server.execComputer (POST /api/v1/computers/exec) with an authenticated server session"
    ))
}

fn handle_computer_rpc_local(
    conn: &Connection,
    submethod: &str,
    params: &Value,
) -> Result<Value, String> {
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
            let status = params
                .get("status")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("standby");
            conn.execute(
                "INSERT OR REPLACE INTO computer_nodes
                 (node_id, name, kind, os_platform, status, viewport_width, viewport_height, lease_holder, lease_generation, lease_expires_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, 0, 0)",
                params![node_id, name, kind, platform, status, vw, vh],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({ "nodeId": node_id, "status": status, "saved": true }))
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
            let current_status: Option<String> = conn
                .query_row(
                    "SELECT status FROM computer_action_audit WHERE invocation_id = ?1",
                    params![inv_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            let Some(existing_status) = current_status else {
                return Err(format!("Invocation '{inv_id}' not found"));
            };
            let updated = conn
                .execute(
                    "UPDATE computer_action_audit SET status = ?1 WHERE invocation_id = ?2 AND status = 'pending_approval'",
                    params![next_status, inv_id],
                )
                .map_err(|e| e.to_string())?;
            if updated == 0 {
                return Err(format!(
                    "Invocation '{inv_id}' already decided (current status: '{existing_status}')"
                ));
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
            let local_status: String = conn
                .query_row(
                    "SELECT status FROM computer_nodes WHERE node_id = 'local-android-mika'",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or_else(|_| "standby".into());
            Ok(json!({
                "available": local_status == "online",
                "status": local_status,
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
        "CREATE TABLE IF NOT EXISTS remote_stream_settings (
            singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
            settings_json TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );",
    )
    .map_err(|e| format!("Failed initializing remote_control schema: {e}"))?;
    Ok(())
}

/// UU-Remote parity & Official OAuth 2.1 Remote MCP RPCs (`remote.*`)
/// Proxies server-backed assist, sessions, and MCP grants to `gpui_telegram_server`
/// (`/api/v1/remote/*`, `/api/v1/mcp/grants*`) when authenticated and fails closed offline.
pub fn handle_remote_rpc(
    db: &Mutex<Connection>,
    submethod: &str,
    params: &Value,
) -> Result<Value, String> {
    match submethod {
        "issueMcpGrant" => Err(
            "OAUTH_FLOW_REQUIRED: MCP OAuth 2.1 grants must be authorized via the server OAuth 2.1 flow and cannot be minted locally"
                .to_string(),
        ),

        "getAssistConfig" => {
            if !crate::with_db(db, |c| Ok(crate::telegram_ext::has_authenticated_server_session(c)))? {
                return Err(
                    "REMOTE_ASSIST_UNAVAILABLE: Sign in to server for Remote Assist (/api/v1/remote/assist/info)"
                        .to_string(),
                );
            }
            crate::telegram_ext::handle_server_rpc(db, "assistInfo", params)
        }

        "setAssistAllow" => {
            if !crate::with_db(db, |c| Ok(crate::telegram_ext::has_authenticated_server_session(c)))? {
                return Err(
                    "REMOTE_ASSIST_UNAVAILABLE: Sign in to server for Remote Assist (/api/v1/remote/assist/allow)"
                        .to_string(),
                );
            }
            crate::telegram_ext::handle_server_rpc(db, "assistAllow", params)
        }

        "setAssistCode" => {
            if !crate::with_db(db, |c| Ok(crate::telegram_ext::has_authenticated_server_session(c)))? {
                return Err(
                    "REMOTE_ASSIST_UNAVAILABLE: Sign in to server for Remote Assist (/api/v1/remote/assist/set-code)"
                        .to_string(),
                );
            }
            crate::telegram_ext::handle_server_rpc(db, "assistSetCode", params)
        }

        "startAssist" => {
            if !crate::with_db(db, |c| Ok(crate::telegram_ext::has_authenticated_server_session(c)))? {
                return Err(
                    "REMOTE_ASSIST_UNAVAILABLE: Sign in to server for Remote Assist (/api/v1/remote/assist/start)"
                        .to_string(),
                );
            }
            crate::telegram_ext::handle_server_rpc(db, "assistStart", params)
        }

        "listSessions" => {
            if !crate::with_db(db, |c| Ok(crate::telegram_ext::has_authenticated_server_session(c)))? {
                return Err(
                    "REMOTE_ASSIST_UNAVAILABLE: Sign in to server for Remote Sessions (/api/v1/remote/sessions)"
                        .to_string(),
                );
            }
            crate::telegram_ext::handle_server_rpc(db, "listRemoteSessions", params)
        }

        "disconnectSession" => {
            if !crate::with_db(db, |c| Ok(crate::telegram_ext::has_authenticated_server_session(c)))? {
                return Err(
                    "REMOTE_ASSIST_UNAVAILABLE: Sign in to server for Remote Sessions (/api/v1/remote/sessions/disconnect)"
                        .to_string(),
                );
            }
            crate::telegram_ext::handle_server_rpc(db, "disconnectRemoteSession", params)
        }

        "listMcpGrants" => {
            if !crate::with_db(db, |c| Ok(crate::telegram_ext::has_authenticated_server_session(c)))? {
                return Err(
                    "AUTH_REQUIRED: Sign in to list server MCP OAuth grants (/api/v1/mcp/grants)"
                        .to_string(),
                );
            }
            crate::telegram_ext::handle_server_rpc(db, "listMcpGrants", params)
        }

        "revokeMcpGrant" => {
            if !crate::with_db(db, |c| Ok(crate::telegram_ext::has_authenticated_server_session(c)))? {
                return Err(
                    "AUTH_REQUIRED: Sign in to revoke server MCP OAuth grants (/api/v1/mcp/grants)"
                        .to_string(),
                );
            }
            crate::telegram_ext::handle_server_rpc(db, "revokeMcpGrant", params)
        }

        "getStreamSettings" => crate::with_db(db, |conn| {
            init_schema(conn)?;
            ensure_remote_schema(conn)?;
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
            serde_json::to_value(&dto).map_err(|e| e.to_string())
        }),

        "updateStreamSettings" => crate::with_db(db, |conn| {
            init_schema(conn)?;
            ensure_remote_schema(conn)?;
            let now = chrono::Utc::now().timestamp_millis();
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
            serde_json::to_value(&dto).map_err(|e| e.to_string())
        }),

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
