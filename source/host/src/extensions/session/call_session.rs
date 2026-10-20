use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use rusqlite::types::Type;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

const CALL_SESSION_DB_FILENAME: &str = ".call-sessions.db";
const SCHEMA: &str = r#"
PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS call_sessions (
    id TEXT PRIMARY KEY,
    scope_id TEXT NOT NULL,
    creator_id TEXT NOT NULL,
    state TEXT NOT NULL,
    generation INTEGER NOT NULL DEFAULT 0,
    signal_seq INTEGER NOT NULL DEFAULT 0,
    participant_ids_json TEXT NOT NULL,
    media_capabilities_json TEXT NOT NULL DEFAULT '{}',
    device_selection_json TEXT NOT NULL DEFAULT '{}',
    terminal_reason TEXT,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_call_sessions_scope_updated
    ON call_sessions(scope_id, updated_at_ms DESC, id DESC);
CREATE TABLE IF NOT EXISTS call_remote_sync (
    call_id TEXT PRIMARY KEY,
    event_seq INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (call_id) REFERENCES call_sessions(id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS call_signals (
    call_id TEXT NOT NULL,
    generation INTEGER NOT NULL,
    seq INTEGER NOT NULL,
    sender_device_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY (call_id, generation, seq),
    FOREIGN KEY (call_id) REFERENCES call_sessions(id) ON DELETE CASCADE
);
"#;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CallSession {
    pub id: String,
    pub scope_id: String,
    pub creator_id: String,
    pub state: String,
    pub generation: u64,
    pub signal_seq: u64,
    pub participant_ids: Vec<String>,
    pub media_capabilities: Value,
    pub device_selection: Value,
    pub terminal_reason: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CallSignal {
    pub call_id: String,
    pub generation: u64,
    pub seq: u64,
    pub sender_device_id: String,
    pub kind: String,
    pub payload: Value,
    pub created_at_ms: i64,
}

pub struct CallSessionStore {
    path: PathBuf,
    connection: Mutex<Connection>,
}

impl CallSessionStore {
    pub fn open(agents_root: &Path, busy_timeout_ms: u64) -> Result<Self, String> {
        std::fs::create_dir_all(agents_root).map_err(|error| error.to_string())?;
        let path = agents_root.join(CALL_SESSION_DB_FILENAME);
        let connection = Connection::open(&path).map_err(|error| error.to_string())?;
        connection
            .busy_timeout(std::time::Duration::from_millis(busy_timeout_ms))
            .map_err(|error| error.to_string())?;
        connection.execute_batch(SCHEMA).map_err(|error| error.to_string())?;
        Ok(Self {
            path,
            connection: Mutex::new(connection),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn create(
        &self,
        scope_id: &str,
        creator_id: &str,
        participant_ids: &[String],
    ) -> Result<CallSession, String> {
        self.create_with_id(&Uuid::new_v4().to_string(), scope_id, creator_id, participant_ids)
    }

    pub fn create_with_id(
        &self,
        call_id: &str,
        scope_id: &str,
        creator_id: &str,
        participant_ids: &[String],
    ) -> Result<CallSession, String> {
        let call_id = required(call_id, "call id")?;
        let scope_id = required(scope_id, "call scope id")?;
        let creator_id = required(creator_id, "call creator id")?;
        let mut participants = participant_ids
            .iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        participants.sort();
        participants.dedup();
        if participants.len() < 2 {
            return Err("call session requires at least two participants".into());
        }
        if !participants.iter().any(|value| value == creator_id) {
            return Err("call creator must be a call participant".into());
        }
        let id = call_id.to_string();
        let now = now_ms();
        let participants_json = serde_json::to_string(&participants).map_err(|error| error.to_string())?;
        let connection = self.connection.lock().map_err(|_| "call session store poisoned".to_string())?;
        connection.execute(
            "INSERT INTO call_sessions (id, scope_id, creator_id, state, generation, signal_seq, participant_ids_json, media_capabilities_json, device_selection_json, created_at_ms, updated_at_ms) VALUES (?, ?, ?, 'invited', 0, 0, ?, '{}', '{}', ?, ?)",
            params![id, scope_id, creator_id, participants_json, now, now],
        ).map_err(|error| error.to_string())?;
        drop(connection);
        self.get(&id)?.ok_or_else(|| "created call session disappeared".to_string())
    }

    pub fn get(&self, call_id: &str) -> Result<Option<CallSession>, String> {
        let call_id = required(call_id, "call id")?;
        let connection = self.connection.lock().map_err(|_| "call session store poisoned".to_string())?;
        connection
            .query_row(
                "SELECT id, scope_id, creator_id, state, generation, signal_seq, participant_ids_json, media_capabilities_json, device_selection_json, terminal_reason, created_at_ms, updated_at_ms FROM call_sessions WHERE id = ?",
                params![call_id],
                map_session,
            )
            .optional()
            .map_err(|error| error.to_string())
    }

    pub fn remote_event_seq(&self, call_id: &str) -> Result<u64, String> {
        let call_id = required(call_id, "call id")?;
        let connection = self.connection.lock().map_err(|_| "call session store poisoned".to_string())?;
        connection
            .query_row(
                "SELECT event_seq FROM call_remote_sync WHERE call_id = ?",
                params![call_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map(|value| value.unwrap_or(0) as u64)
            .map_err(|error| error.to_string())
    }

    pub fn set_remote_event_seq(&self, call_id: &str, event_seq: u64) -> Result<(), String> {
        let call_id = required(call_id, "call id")?;
        let connection = self.connection.lock().map_err(|_| "call session store poisoned".to_string())?;
        connection
            .execute(
                "INSERT INTO call_remote_sync (call_id, event_seq) VALUES (?, ?) ON CONFLICT(call_id) DO UPDATE SET event_seq = MAX(call_remote_sync.event_seq, excluded.event_seq)",
                params![call_id, event_seq as i64],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn list_for_scope(&self, scope_id: &str, limit: usize) -> Result<Vec<CallSession>, String> {
        let scope_id = required(scope_id, "call scope id")?;
        let limit = limit.clamp(1, 200);
        let connection = self.connection.lock().map_err(|_| "call session store poisoned".to_string())?;
        let mut statement = connection.prepare(
            "SELECT id, scope_id, creator_id, state, generation, signal_seq, participant_ids_json, media_capabilities_json, device_selection_json, terminal_reason, created_at_ms, updated_at_ms FROM call_sessions WHERE scope_id = ? ORDER BY updated_at_ms DESC, id DESC LIMIT ?"
        ).map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![scope_id, limit as i64], map_session)
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|error| error.to_string())
    }

    pub fn transition(
        &self,
        call_id: &str,
        expected_generation: u64,
        action: &str,
        terminal_reason: Option<&str>,
    ) -> Result<CallSession, String> {
        let call_id = required(call_id, "call id")?;
        let action = required(action, "call action")?;
        let mut connection = self.connection.lock().map_err(|_| "call session store poisoned".to_string())?;
        let transaction = connection.transaction().map_err(|error| error.to_string())?;
        let current = transaction
            .query_row(
                "SELECT id, scope_id, creator_id, state, generation, signal_seq, participant_ids_json, media_capabilities_json, device_selection_json, terminal_reason, created_at_ms, updated_at_ms FROM call_sessions WHERE id = ?",
                params![call_id],
                map_session,
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "call session not found".to_string())?;
        if current.generation != expected_generation {
            return Err(format!(
                "stale call generation: expected {}, current {}",
                expected_generation, current.generation
            ));
        }
        let (next_state, next_generation, reset_signal_seq, reason) =
            transition_target(&current, action, terminal_reason)?;
        let now = now_ms();
        let changed = transaction.execute(
            "UPDATE call_sessions SET state = ?, generation = ?, signal_seq = CASE WHEN ? THEN 0 ELSE signal_seq END, terminal_reason = ?, updated_at_ms = ? WHERE id = ? AND generation = ?",
            params![
                next_state,
                next_generation as i64,
                reset_signal_seq,
                reason,
                now,
                call_id,
                expected_generation as i64
            ],
        ).map_err(|error| error.to_string())?;
        if changed != 1 {
            return Err("call session transition lost its generation fence".into());
        }
        transaction.commit().map_err(|error| error.to_string())?;
        drop(connection);
        self.get(call_id)?.ok_or_else(|| "call session disappeared after transition".to_string())
    }

    pub fn update_media(
        &self,
        call_id: &str,
        expected_generation: u64,
        media_capabilities: Option<&Value>,
        device_selection: Option<&Value>,
    ) -> Result<CallSession, String> {
        if media_capabilities.is_none() && device_selection.is_none() {
            return Err("call media update requires capabilities or device selection".into());
        }
        let call_id = required(call_id, "call id")?;
        let current = self.get(call_id)?.ok_or_else(|| "call session not found".to_string())?;
        if current.generation != expected_generation {
            return Err(format!(
                "stale call generation: expected {}, current {}",
                expected_generation, current.generation
            ));
        }
        if is_terminal(&current.state) {
            return Err("terminal call session cannot update media state".into());
        }
        let capabilities = media_capabilities.unwrap_or(&current.media_capabilities);
        let devices = device_selection.unwrap_or(&current.device_selection);
        if !capabilities.is_object() || !devices.is_object() {
            return Err("call media capabilities and device selection must be objects".into());
        }
        let capabilities_json = serde_json::to_string(capabilities).map_err(|error| error.to_string())?;
        let devices_json = serde_json::to_string(devices).map_err(|error| error.to_string())?;
        let now = now_ms();
        let connection = self.connection.lock().map_err(|_| "call session store poisoned".to_string())?;
        let changed = connection.execute(
            "UPDATE call_sessions SET media_capabilities_json = ?, device_selection_json = ?, updated_at_ms = ? WHERE id = ? AND generation = ? AND state NOT IN ('ended', 'failed')",
            params![capabilities_json, devices_json, now, call_id, expected_generation as i64],
        ).map_err(|error| error.to_string())?;
        if changed != 1 {
            return Err("call media update lost its generation fence".into());
        }
        drop(connection);
        self.get(call_id)?.ok_or_else(|| "call session disappeared after media update".to_string())
    }

    pub fn append_signal(
        &self,
        call_id: &str,
        expected_generation: u64,
        seq: u64,
        sender_device_id: &str,
        kind: &str,
        payload: &Value,
    ) -> Result<CallSignal, String> {
        self.append_signal_ordered(
            call_id,
            expected_generation,
            seq,
            sender_device_id,
            kind,
            payload,
            false,
        )
    }

    pub fn append_remote_signal(
        &self,
        call_id: &str,
        expected_generation: u64,
        event_seq: u64,
        sender_device_id: &str,
        kind: &str,
        payload: &Value,
    ) -> Result<CallSignal, String> {
        self.append_signal_ordered(
            call_id,
            expected_generation,
            event_seq,
            sender_device_id,
            kind,
            payload,
            true,
        )
    }

    fn append_signal_ordered(
        &self,
        call_id: &str,
        expected_generation: u64,
        seq: u64,
        sender_device_id: &str,
        kind: &str,
        payload: &Value,
        allow_monotonic_gap: bool,
    ) -> Result<CallSignal, String> {
        let call_id = required(call_id, "call id")?;
        let sender_device_id = required(sender_device_id, "sender device id")?;
        let kind = required(kind, "call signal kind")?;
        if !payload.is_object() {
            return Err("call signal payload must be an object".into());
        }
        let payload_json = serde_json::to_string(payload).map_err(|error| error.to_string())?;
        let mut connection = self.connection.lock().map_err(|_| "call session store poisoned".to_string())?;
        let transaction = connection.transaction().map_err(|error| error.to_string())?;
        let current: (String, u64, u64) = transaction
            .query_row(
                "SELECT state, generation, signal_seq FROM call_sessions WHERE id = ?",
                params![call_id],
                |row| Ok((row.get(0)?, row.get::<_, i64>(1)? as u64, row.get::<_, i64>(2)? as u64)),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "call session not found".to_string())?;
        if current.1 != expected_generation {
            return Err(format!(
                "stale call generation: expected {}, current {}",
                expected_generation, current.1
            ));
        }
        if is_terminal(&current.0) {
            return Err("terminal call session cannot accept signaling".into());
        }
        if seq <= current.2 {
            let existing = transaction
                .query_row(
                    "SELECT call_id, generation, seq, sender_device_id, kind, payload_json, created_at_ms FROM call_signals WHERE call_id = ? AND generation = ? AND seq = ?",
                    params![call_id, expected_generation as i64, seq as i64],
                    map_signal,
                )
                .optional()
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "call signal sequence is stale".to_string())?;
            if existing.sender_device_id == sender_device_id
                && existing.kind == kind
                && existing.payload == *payload
            {
                return Ok(existing);
            }
            return Err("call signal sequence conflicts with previously accepted payload".into());
        }
        if !allow_monotonic_gap && seq != current.2 + 1 {
            return Err(format!(
                "call signal sequence gap: expected {}, received {}",
                current.2 + 1,
                seq
            ));
        }
        let now = now_ms();
        transaction.execute(
            "INSERT INTO call_signals (call_id, generation, seq, sender_device_id, kind, payload_json, created_at_ms) VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![call_id, expected_generation as i64, seq as i64, sender_device_id, kind, payload_json, now],
        ).map_err(|error| error.to_string())?;
        let changed = transaction.execute(
            "UPDATE call_sessions SET signal_seq = ?, updated_at_ms = ? WHERE id = ? AND generation = ? AND signal_seq = ?",
            params![seq as i64, now, call_id, expected_generation as i64, current.2 as i64],
        ).map_err(|error| error.to_string())?;
        if changed != 1 {
            return Err("call signaling lost its sequence fence".into());
        }
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(CallSignal {
            call_id: call_id.to_string(),
            generation: expected_generation,
            seq,
            sender_device_id: sender_device_id.to_string(),
            kind: kind.to_string(),
            payload: payload.clone(),
            created_at_ms: now,
        })
    }

    pub fn list_signals(
        &self,
        call_id: &str,
        generation: u64,
        after_seq: u64,
        limit: usize,
    ) -> Result<Vec<CallSignal>, String> {
        let call_id = required(call_id, "call id")?;
        let limit = limit.clamp(1, 500);
        let connection = self.connection.lock().map_err(|_| "call session store poisoned".to_string())?;
        let mut statement = connection.prepare(
            "SELECT call_id, generation, seq, sender_device_id, kind, payload_json, created_at_ms FROM call_signals WHERE call_id = ? AND generation = ? AND seq > ? ORDER BY seq ASC LIMIT ?"
        ).map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(
                params![call_id, generation as i64, after_seq as i64, limit as i64],
                map_signal,
            )
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|error| error.to_string())
    }
}

pub(super) fn transition_target(
    current: &CallSession,
    action: &str,
    terminal_reason: Option<&str>,
) -> Result<(&'static str, u64, bool, Option<String>), String> {
    if is_terminal(&current.state) {
        return Err("terminal call session cannot transition".into());
    }
    let reason = terminal_reason
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    match (current.state.as_str(), action) {
        ("invited" | "ringing", "accept") => Ok(("negotiating", current.generation, false, None)),
        ("negotiating" | "reconnecting", "connected") => Ok(("connected", current.generation, false, None)),
        ("invited" | "ringing", "decline") => Ok(("ended", current.generation, false, Some(reason.unwrap_or_else(|| "declined".into())))),
        ("invited" | "ringing" | "negotiating" | "connected" | "reconnecting", "hangup") => {
            Ok(("ended", current.generation, false, Some(reason.unwrap_or_else(|| "hangup".into()))))
        }
        ("invited" | "ringing" | "negotiating" | "connected" | "reconnecting", "fail") => {
            Ok(("failed", current.generation, false, Some(reason.unwrap_or_else(|| "failed".into()))))
        }
        ("negotiating" | "connected" | "reconnecting", "reconnect") => {
            Ok(("reconnecting", current.generation + 1, true, None))
        }
        ("reconnecting", "resume") => Ok(("negotiating", current.generation, false, None)),
        (_, "ring") if current.state == "invited" => Ok(("ringing", current.generation, false, None)),
        _ => Err(format!("invalid call transition: {} -> {action}", current.state)),
    }
}

fn is_terminal(state: &str) -> bool {
    matches!(state, "ended" | "failed")
}

fn required<'a>(value: &'a str, label: &str) -> Result<&'a str, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("{label} is required"))
    } else {
        Ok(value)
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn parse_json_column<T: serde::de::DeserializeOwned>(
    raw: &str,
    column: usize,
) -> rusqlite::Result<T> {
    serde_json::from_str(raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(column, Type::Text, Box::new(error))
    })
}

fn map_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<CallSession> {
    let participant_ids_json: String = row.get(6)?;
    let media_capabilities_json: String = row.get(7)?;
    let device_selection_json: String = row.get(8)?;
    Ok(CallSession {
        id: row.get(0)?,
        scope_id: row.get(1)?,
        creator_id: row.get(2)?,
        state: row.get(3)?,
        generation: row.get::<_, i64>(4)? as u64,
        signal_seq: row.get::<_, i64>(5)? as u64,
        participant_ids: parse_json_column(&participant_ids_json, 6)?,
        media_capabilities: parse_json_column(&media_capabilities_json, 7)?,
        device_selection: parse_json_column(&device_selection_json, 8)?,
        terminal_reason: row.get(9)?,
        created_at_ms: row.get(10)?,
        updated_at_ms: row.get(11)?,
    })
}

fn map_signal(row: &rusqlite::Row<'_>) -> rusqlite::Result<CallSignal> {
    let payload_json: String = row.get(5)?;
    Ok(CallSignal {
        call_id: row.get(0)?,
        generation: row.get::<_, i64>(1)? as u64,
        seq: row.get::<_, i64>(2)? as u64,
        sender_device_id: row.get(3)?,
        kind: row.get(4)?,
        payload: parse_json_column(&payload_json, 5)?,
        created_at_ms: row.get(6)?,
    })
}
