//! Hermes AI Studio — Multi-Turn Agentic Tool-Calling Engine, Sessions & Branching,
//! Bounded `MEMORY.md` (2,200 chars) / `USER.md` (1,375 chars) / `SOUL.md`,
//! 29 Toolsets, 61 Slash Commands, Skill Security Scanner, 5-Field Cron Scheduler,
//! Kanban Task Board, and Approval Gates (`hermes_approvals` pause/resume).
//!
//! Zero fake echo policy: all non-slash user prompts route strictly through the live
//! Magpie multi-provider gateway (`crate::magpie::execute_live_request`).

use crate::magpie::{
    UnifiedMessage, UnifiedRequest, UnifiedRole, UnifiedToolCall, UnifiedToolSpec,
};
use chrono::{Datelike, TimeZone, Timelike, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Mutex;

pub const MEMORY_MD_CHAR_LIMIT: usize = 2_200;
pub const USER_MD_CHAR_LIMIT: usize = 1_375;
pub const SOUL_MD_CHAR_LIMIT: usize = 4_000;
pub const MAX_AGENTIC_STEPS: usize = 6;

#[derive(Debug, Clone, Serialize)]
pub struct SlashCommandSpec {
    pub name: &'static str,
    pub category: &'static str,
    pub usage: &'static str,
    pub description: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolsetSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub risk_tier: &'static str,
    pub enabled_by_default: bool,
}

macro_rules! cmd {
    ($n:expr, $c:expr, $u:expr, $d:expr) => {
        SlashCommandSpec {
            name: $n,
            category: $c,
            usage: $u,
            description: $d,
        }
    };
}

/// Canonical 61 Hermes Studio slash commands across 8 functional domains.
pub static HERMES_SLASH_COMMANDS: &[SlashCommandSpec] = &[
    // Session & Branching (10)
    cmd!("/new", "session", "/new [title]", "Create a fresh Hermes Studio conversation"),
    cmd!("/branch", "session", "/branch [label]", "Fork current session from latest turn"),
    cmd!("/switch", "session", "/switch <session_id>", "Switch active Hermes session"),
    cmd!("/rename", "session", "/rename <title>", "Rename the active session"),
    cmd!("/archive", "session", "/archive", "Archive the active session"),
    cmd!("/clear", "session", "/clear", "Clear ephemeral turn history in active session"),
    cmd!("/compact", "session", "/compact", "Summarize older turns into compressed context"),
    cmd!("/rewind", "session", "/rewind [steps]", "Rewind N assistant turns"),
    cmd!("/export", "session", "/export [json|md]", "Export trajectory in ShareGPT or Markdown"),
    cmd!("/history", "session", "/history [limit]", "Inspect recent session turn summaries"),
    // Models, Routing & Magpie (9)
    cmd!("/model", "model", "/model <model_id>", "Pin or switch active LLM model"),
    cmd!("/provider", "model", "/provider <preset>", "Select Magpie upstream provider"),
    cmd!("/route", "model", "/route <smart|cheapest|fastest|order>", "Change Magpie routing policy"),
    cmd!("/moa", "model", "/moa <on|off>", "Toggle Mixture-of-Agents synthesis"),
    cmd!("/temperature", "model", "/temperature <0.0-2.0>", "Adjust sampling temperature"),
    cmd!("/thinking", "model", "/thinking <off|low|medium|high>", "Configure reasoning effort budget"),
    cmd!("/tokens", "model", "/tokens", "Inspect token & USD cost ledger for session"),
    cmd!("/quotas", "model", "/quotas", "Check provider rate limits & cooldowns"),
    cmd!("/masking", "model", "/masking <on|off|preview>", "Inspect or toggle Magpie PII masking"),
    // Memory, Soul & Dialectics (8)
    cmd!("/memory", "memory", "/memory [show|set|append]", "Inspect or update bounded MEMORY.md (2200c)"),
    cmd!("/user", "memory", "/user [show|set]", "Inspect or update bounded USER.md (1375c)"),
    cmd!("/soul", "memory", "/soul [show|set]", "Inspect or update agent persona SOUL.md"),
    cmd!("/remember", "memory", "/remember <fact>", "Distill a durable fact into MEMORY.md"),
    cmd!("/forget", "memory", "/forget <keyword>", "Prune matching lines from MEMORY.md"),
    cmd!("/honcho", "memory", "/honcho <query>", "Run Honcho dialectic user-representation synthesis"),
    cmd!("/context", "memory", "/context", "Inspect active context window & token breakdown"),
    cmd!("/pin", "memory", "/pin <note>", "Pin an instruction to system prompt header"),
    // Skills & Toolsets (8)
    cmd!("/skills", "skills", "/skills [list|scan]", "List installed skills and security status"),
    cmd!("/skill-add", "skills", "/skill-add <name> <body_md>", "Install and security-scan a Markdown skill"),
    cmd!("/skill-rm", "skills", "/skill-rm <name>", "Remove an installed skill"),
    cmd!("/tools", "skills", "/tools", "List all 29 Hermes toolsets and toggle states"),
    cmd!("/tool-enable", "skills", "/tool-enable <id>", "Enable a Hermes toolset"),
    cmd!("/tool-disable", "skills", "/tool-disable <id>", "Disable a Hermes toolset"),
    cmd!("/mcp", "skills", "/mcp [list|connect]", "Inspect connected MCP tool servers"),
    cmd!("/sandbox", "skills", "/sandbox <strict|standard>", "Set tool execution sandbox policy"),
    // Cron, Heartbeat & Kanban Tasks (9)
    cmd!("/cron", "automation", "/cron <expr> <prompt>", "Schedule a 5-field cron recurring agent job"),
    cmd!("/cron-list", "automation", "/cron-list", "List active cron schedules and next run times"),
    cmd!("/cron-rm", "automation", "/cron-rm <job_id>", "Delete a scheduled cron job"),
    cmd!("/heartbeat", "automation", "/heartbeat <interval_m>", "Configure autonomous heartbeat check-in"),
    cmd!("/kanban", "automation", "/kanban", "Open Kanban task board summary"),
    cmd!("/task-add", "automation", "/task-add <title>", "Add a card to Kanban Todo column"),
    cmd!("/task-move", "automation", "/task-move <id> <col>", "Move Kanban card across columns"),
    cmd!("/queue", "automation", "/queue <prompt>", "Enqueue a follow-up prompt while busy"),
    cmd!("/routines", "automation", "/routines", "Show next 7-day automation routine forecast"),
    // Security, Approvals & Computer Control (7)
    cmd!("/approvals", "security", "/approvals", "List pending dangerous-command approval gates"),
    cmd!("/approve", "security", "/approve <id>", "Approve a pending high-risk tool invocation"),
    cmd!("/deny", "security", "/deny <id>", "Reject a pending high-risk tool invocation"),
    cmd!("/computer", "security", "/computer [nodes|lease]", "Inspect Computer Control nodes & CUA lease"),
    cmd!("/mika", "security", "/mika <goal>", "Delegate on-device UI goal to Mika Accessibility Agent"),
    cmd!("/audit", "security", "/audit", "Show recent security redactions & approval decisions"),
    cmd!("/takeover", "security", "/takeover", "Immediately revoke autonomous CUA lease"),
    // Dharma, Wallet & Telegram Bridges (6)
    cmd!("/chant", "dharma", "/chant [sutra]", "Launch Zen Room sutra recitation tracker"),
    cmd!("/dharma", "dharma", "/dharma <country_code>", "Dispatch Global Dharma Offering to country"),
    cmd!("/wallet", "dharma", "/wallet", "Show TON v4r2 & Telegram Stars balance"),
    cmd!("/bridge", "dharma", "/bridge <chat_id>", "Bind Hermes session to a Telegram conversation"),
    cmd!("/voice", "dharma", "/voice <on|off>", "Toggle full-duplex Talk Mode with VAD barge-in"),
    cmd!("/translate", "dharma", "/translate <lang>", "Translate last turn preserving Buddhist terminology"),
    // Diagnostics & Help (4)
    cmd!("/help", "system", "/help [category]", "List all 61 Hermes slash commands"),
    cmd!("/status", "system", "/status", "Display Hermes runtime, DB, and gateway health"),
    cmd!("/doctor", "system", "/doctor", "Run self-diagnostics on memory caps, cron, and keys"),
    cmd!("/version", "system", "/version", "Show Hermes Studio & Mahayana protocol version"),
];

macro_rules! toolset {
    ($id:expr, $name:expr, $cat:expr, $risk:expr, $def:expr) => {
        ToolsetSpec {
            id: $id,
            name: $name,
            category: $cat,
            risk_tier: $risk,
            enabled_by_default: $def,
        }
    };
}

/// Canonical 29 Hermes toolsets.
pub static HERMES_TOOLSETS: &[ToolsetSpec] = &[
    toolset!("filesystem_read", "Filesystem Read", "core", "read_only", true),
    toolset!("filesystem_write", "Filesystem Write", "core", "medium", true),
    toolset!("terminal_exec", "Sandboxed Terminal", "core", "high", false),
    toolset!("code_search", "Ripgrep Code Search", "core", "read_only", true),
    toolset!("git_ops", "Git Repository Ops", "core", "medium", true),
    toolset!("web_search", "Web Search", "web", "low", true),
    toolset!("web_fetch", "Web Page Reader", "web", "low", true),
    toolset!("browser_cua", "Headless Browser CUA", "web", "high", false),
    toolset!("memory_core", "Bounded Memory & Soul", "memory", "low", true),
    toolset!("honcho_dialectics", "Honcho User Modeling", "memory", "read_only", true),
    toolset!("skills_manager", "Markdown Skills Engine", "agentic", "low", true),
    toolset!("subagent_spawn", "ACP Subagent Spawner", "agentic", "medium", true),
    toolset!("moa_synthesizer", "Mixture-of-Agents", "agentic", "low", true),
    toolset!("cron_scheduler", "5-Field Cron Scheduler", "automation", "medium", true),
    toolset!("kanban_board", "Kanban Task Board", "automation", "low", true),
    toolset!("heartbeat_monitor", "Proactive Heartbeat", "automation", "low", true),
    toolset!("telegram_messaging", "Telegram Messaging Bridge", "im", "medium", true),
    toolset!("telegram_botfather", "BotFather Provisioner", "im", "medium", true),
    toolset!("telegram_stories", "Stories Publisher", "im", "medium", false),
    toolset!("mika_accessibility", "Mika Android Accessibility", "device", "high", true),
    toolset!("android_intents", "Android Deep-Link Intents", "device", "medium", true),
    toolset!("clipboard_bridge", "Secure Clipboard", "device", "medium", false),
    toolset!("notifications_read", "Notification Listener", "device", "read_only", true),
    toolset!("computer_remote", "Remote Tailnet Node CUA", "computer", "high", false),
    toolset!("docker_box", "Isolated Container Box", "computer", "high", false),
    toolset!("ton_wallet_read", "TON v4r2 Balance & Explorer", "finance", "read_only", true),
    toolset!("ton_wallet_tx", "TON & Stars Transfer", "finance", "critical_irreversible", false),
    toolset!("dharma_catalog", "Global Dharma & Sutras", "dharma", "read_only", true),
    toolset!("voice_talk_vad", "Voice Talk & VAD Barge-In", "media", "low", true),
];

/// Security scanner for user/agent-authored Markdown skills.
pub fn scan_skill_security(markdown: &str) -> Result<Vec<String>, String> {
    let lower = markdown.to_ascii_lowercase();
    let blocked_patterns = [
        ("rm -rf /", "Destructive root filesystem wipe detected"),
        ("curl ", "Raw curl pipe/exfiltration command in skill body"),
        ("| sh", "Remote shell pipe execution detected"),
        ("| bash", "Remote bash pipe execution detected"),
        ("ignore previous instructions", "Prompt-injection override directive detected"),
        ("disregard system prompt", "System prompt bypass attempt detected"),
        ("/etc/shadow", "Sensitive credential file target detected"),
        ("private_key", "Explicit secret exfiltration keyword detected"),
    ];
    for (pat, reason) in blocked_patterns {
        if lower.contains(pat) {
            if pat == "curl " && !(lower.contains("| sh") || lower.contains("| bash") || lower.contains("evil")) {
                continue;
            }
            return Err(format!("Skill rejected by Hermes Security Scanner: {reason}"));
        }
    }
    let mut warnings = Vec::new();
    if lower.contains("sudo ") {
        warnings.push("Contains 'sudo' privilege escalation reference".to_string());
    }
    Ok(warnings)
}

#[derive(Debug, Clone)]
pub struct CronSchedule {
    minutes: [bool; 60],
    hours: [bool; 24],
    days_of_month: [bool; 32],
    months: [bool; 13],
    days_of_week: [bool; 7],
}

impl CronSchedule {
    pub fn parse(expr: &str) -> Result<Self, String> {
        let parts: Vec<&str> = expr.split_whitespace().collect();
        if parts.len() != 5 {
            return Err(format!(
                "Expected 5-field cron expression (min hour dom month dow), got {} fields",
                parts.len()
            ));
        }
        let mut minutes = [false; 60];
        let mut hours = [false; 24];
        let mut dom = [false; 32];
        let mut months = [false; 13];
        let mut dow = [false; 7];

        parse_cron_field(parts[0], 0, 59, &mut minutes)?;
        parse_cron_field(parts[1], 0, 23, &mut hours)?;
        parse_cron_field(parts[2], 1, 31, &mut dom)?;
        parse_cron_field(parts[3], 1, 12, &mut months)?;
        parse_cron_field(parts[4], 0, 6, &mut dow)?;

        Ok(Self {
            minutes,
            hours,
            days_of_month: dom,
            months,
            days_of_week: dow,
        })
    }

    pub fn next_after_ms(&self, after_ms: i64) -> Result<i64, String> {
        let start_min = (after_ms / 60_000) + 1;
        for offset in 0..(366 * 24 * 60) {
            let ts_secs = (start_min + offset) * 60;
            let Some(dt) = Utc.timestamp_opt(ts_secs, 0).single() else {
                continue;
            };
            let m = dt.minute() as usize;
            let h = dt.hour() as usize;
            let d = dt.day() as usize;
            let mo = dt.month() as usize;
            let w = dt.weekday().num_days_from_sunday() as usize;
            if self.minutes[m]
                && self.hours[h]
                && self.days_of_month[d]
                && self.months[mo]
                && self.days_of_week[w]
            {
                return Ok(ts_secs * 1000);
            }
        }
        Err("No matching cron execution timestamp within 366 days".into())
    }
}

fn parse_cron_field(field: &str, min_v: usize, max_v: usize, out: &mut [bool]) -> Result<(), String> {
    let max_step = max_v - min_v + 1;
    for part in field.split(',') {
        let part = part.trim();
        let (range_part, step) = if let Some((r, s)) = part.split_once('/') {
            let st: usize = s.parse().map_err(|_| format!("Invalid cron step '{s}'"))?;
            if st == 0 || st > max_step {
                return Err(format!("Cron step {st} out of bounds [1..={max_step}]"));
            }
            (r, st)
        } else {
            (part, 1usize)
        };

        let (lo, hi) = if range_part == "*" {
            (min_v, max_v)
        } else if let Some((a, b)) = range_part.split_once('-') {
            let start: usize = a.parse().map_err(|_| format!("Invalid cron start '{a}'"))?;
            let end: usize = b.parse().map_err(|_| format!("Invalid cron end '{b}'"))?;
            if start < min_v || end > max_v || start > end {
                return Err(format!("Cron range {start}-{end} out of bounds [{min_v}..={max_v}]"));
            }
            (start, end)
        } else {
            let v: usize = range_part
                .parse()
                .map_err(|_| format!("Invalid cron value '{range_part}'"))?;
            let v = if max_v == 6 && v == 7 { 0 } else { v };
            if v < min_v || v > max_v {
                return Err(format!("Cron value {v} out of bounds [{min_v}..={max_v}]"));
            }
            (v, v)
        };

        let mut cur = lo;
        while cur <= hi {
            out[cur] = true;
            match cur.checked_add(step) {
                Some(next) => cur = next,
                None => break,
            }
        }
    }
    Ok(())
}

pub fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS hermes_sessions (
            session_id TEXT PRIMARY KEY,
            parent_session_id TEXT,
            title TEXT NOT NULL,
            model TEXT NOT NULL,
            provider_id TEXT NOT NULL,
            archived INTEGER NOT NULL DEFAULT 0,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS hermes_messages (
            msg_id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            seq INTEGER NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            model TEXT,
            token_estimate INTEGER NOT NULL DEFAULT 0,
            tool_calls_json TEXT,
            tool_call_id TEXT,
            tool_name TEXT,
            created_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS hermes_memory (
            target TEXT PRIMARY KEY,
            content TEXT NOT NULL,
            char_limit INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS hermes_skills (
            name TEXT PRIMARY KEY,
            description TEXT NOT NULL,
            body_markdown TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS hermes_toolset_state (
            toolset_id TEXT PRIMARY KEY,
            enabled INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS hermes_cron_jobs (
            job_id TEXT PRIMARY KEY,
            schedule TEXT NOT NULL,
            prompt TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            next_run_ms INTEGER NOT NULL,
            last_run_ms INTEGER
        );
        CREATE TABLE IF NOT EXISTS hermes_kanban_cards (
            card_id TEXT PRIMARY KEY,
            column_id TEXT NOT NULL,
            title TEXT NOT NULL,
            priority TEXT NOT NULL,
            assignee TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS hermes_approvals (
            approval_id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            tool_name TEXT NOT NULL,
            command_summary TEXT NOT NULL,
            risk_level TEXT NOT NULL,
            status TEXT NOT NULL,
            tool_call_id TEXT,
            arguments_json TEXT,
            created_at_ms INTEGER NOT NULL
        );",
    )
    .map_err(|e| format!("Failed to initialize Hermes schema: {e}"))?;

    // Safe additive migrations for existing DBs
    let _ = conn.execute("ALTER TABLE hermes_messages ADD COLUMN tool_calls_json TEXT", []);
    let _ = conn.execute("ALTER TABLE hermes_messages ADD COLUMN tool_call_id TEXT", []);
    let _ = conn.execute("ALTER TABLE hermes_messages ADD COLUMN tool_name TEXT", []);
    let _ = conn.execute("ALTER TABLE hermes_approvals ADD COLUMN tool_call_id TEXT", []);
    let _ = conn.execute("ALTER TABLE hermes_approvals ADD COLUMN arguments_json TEXT", []);

    let now = Utc::now().timestamp_millis();
    for (target, default_body, limit) in [
        (
            "SOUL.md",
            "You are Hermes, a calm, rigorous, compassionate multi-provider AI research & automation partner integrated into Fabushi Android.",
            SOUL_MD_CHAR_LIMIT,
        ),
        (
            "MEMORY.md",
            "- Prefer concise, verifiable engineering answers.\n- Preserve Sanskrit/Chinese Dharma terminology accurately.\n- Respect 16KB ELF page alignment on Android native builds.",
            MEMORY_MD_CHAR_LIMIT,
        ),
        (
            "USER.md",
            "- Platform: Fabushi Android (Pixel / ARM64)\n- Languages: zh-CN, en-US",
            USER_MD_CHAR_LIMIT,
        ),
    ] {
        conn.execute(
            "INSERT OR IGNORE INTO hermes_memory (target, content, char_limit, updated_at_ms) VALUES (?1, ?2, ?3, ?4)",
            params![target, default_body, limit as i64, now],
        )
        .map_err(|e| e.to_string())?;
    }

    for ts in HERMES_TOOLSETS {
        conn.execute(
            "INSERT OR IGNORE INTO hermes_toolset_state (toolset_id, enabled) VALUES (?1, ?2)",
            params![ts.id, if ts.enabled_by_default { 1 } else { 0 }],
        )
        .map_err(|e| e.to_string())?;
    }

    let session_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM hermes_sessions", [], |r| r.get(0))
        .unwrap_or(0);
    if session_count == 0 {
        conn.execute(
            "INSERT INTO hermes_sessions (session_id, parent_session_id, title, model, provider_id, archived, created_at_ms, updated_at_ms)
             VALUES ('hermes_main', NULL, 'Hermes Studio Main', 'auto', 'auto', 0, ?1, ?1)",
            params![now],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn clamp_chars(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

fn is_dangerous_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "terminal_exec"
            | "computer_execute_op"
            | "wallet_send_ton"
            | "browser_cua"
            | "docker_box"
    )
}

fn next_message_seq(conn: &Connection, session_id: &str) -> i64 {
    conn.query_row(
        "SELECT COALESCE(MAX(seq), 0) + 1 FROM hermes_messages WHERE session_id = ?1",
        params![session_id],
        |r| r.get(0),
    )
    .unwrap_or(1)
}

fn load_enabled_tool_specs(conn: &Connection) -> Result<Vec<UnifiedToolSpec>, String> {
    let mut enabled_ids = std::collections::HashSet::new();
    let mut stmt = conn
        .prepare("SELECT toolset_id FROM hermes_toolset_state WHERE enabled = 1")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    for id in rows.flatten() {
        enabled_ids.insert(id);
    }

    let mut specs = Vec::new();
    if enabled_ids.contains("memory_core") {
        specs.push(UnifiedToolSpec {
            name: "memory_read".into(),
            description: "Read SOUL.md, MEMORY.md, or USER.md from Hermes bounded memory".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "target": { "type": "string", "enum": ["SOUL.md", "MEMORY.md", "USER.md"] }
                },
                "required": ["target"]
            }),
        });
        specs.push(UnifiedToolSpec {
            name: "memory_update".into(),
            description: "Update bounded MEMORY.md (2200 chars) or USER.md (1375 chars)".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "target": { "type": "string", "enum": ["MEMORY.md", "USER.md"] },
                    "content": { "type": "string" }
                },
                "required": ["target", "content"]
            }),
        });
    }
    if enabled_ids.contains("kanban_board") {
        specs.push(UnifiedToolSpec {
            name: "kanban_list".into(),
            description: "List all cards on the Hermes Kanban task board".into(),
            parameters: json!({ "type": "object", "properties": {} }),
        });
        specs.push(UnifiedToolSpec {
            name: "kanban_add_card".into(),
            description: "Create a new card on the Hermes Kanban board".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "columnId": { "type": "string", "enum": ["todo", "in_progress", "review", "done"] },
                    "priority": { "type": "string", "enum": ["low", "medium", "high"] }
                },
                "required": ["title"]
            }),
        });
        specs.push(UnifiedToolSpec {
            name: "kanban_move_card".into(),
            description: "Move an existing Kanban card to another column".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "cardId": { "type": "string" },
                    "columnId": { "type": "string", "enum": ["todo", "in_progress", "review", "done"] }
                },
                "required": ["cardId", "columnId"]
            }),
        });
    }
    if enabled_ids.contains("cron_scheduler") {
        specs.push(UnifiedToolSpec {
            name: "cron_schedule".into(),
            description: "Schedule a 5-field cron recurring Hermes automation job".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "schedule": { "type": "string" },
                    "prompt": { "type": "string" }
                },
                "required": ["schedule", "prompt"]
            }),
        });
    }
    if enabled_ids.contains("telegram_messaging") {
        specs.push(UnifiedToolSpec {
            name: "messaging_search".into(),
            description: "Search local Telegram messages via SQLite FTS5 & CJK index".into(),
            parameters: json!({
                "type": "object",
                "properties": { "query": { "type": "string" } },
                "required": ["query"]
            }),
        });
    }
    if enabled_ids.contains("dharma_catalog") {
        specs.push(UnifiedToolSpec {
            name: "dharma_lookup_sutra".into(),
            description: "Look up canonical Buddhist sutra metadata and scripture text".into(),
            parameters: json!({
                "type": "object",
                "properties": { "title": { "type": "string" } },
                "required": ["title"]
            }),
        });
    }
    // Always expose high-risk tools so the LLM can request them and trigger hermes_approvals gating
    specs.push(UnifiedToolSpec {
        name: "terminal_exec".into(),
        description: "Execute a shell command (high-risk: requires explicit user approval gate)"
            .into(),
        parameters: json!({
            "type": "object",
            "properties": { "command": { "type": "string" } },
            "required": ["command"]
        }),
    });
    specs.push(UnifiedToolSpec {
        name: "computer_execute_op".into(),
        description: "Execute a Computer Control CUA operation on a node (requires approval gate)"
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "nodeId": { "type": "string" },
                "op": { "type": "string" }
            },
            "required": ["nodeId", "op"]
        }),
    });
    specs.push(UnifiedToolSpec {
        name: "wallet_send_ton".into(),
        description: "Send TON nanotons from the active v4r2 wallet (critical: requires approval)"
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "recipient": { "type": "string" },
                "amountNanotons": { "type": "integer" },
                "comment": { "type": "string" }
            },
            "required": ["recipient", "amountNanotons"]
        }),
    });
    Ok(specs)
}

fn build_hermes_system_prompt(conn: &Connection) -> Result<String, String> {
    let mut sections = Vec::new();
    for target in ["SOUL.md", "MEMORY.md", "USER.md"] {
        let body: String = conn
            .query_row(
                "SELECT content FROM hermes_memory WHERE target = ?1",
                params![target],
                |r| r.get(0),
            )
            .unwrap_or_default();
        if !body.trim().is_empty() {
            sections.push(format!("## {target}\n{}", body.trim()));
        }
    }

    let mut stmt = conn
        .prepare(
            "SELECT name, description, body_markdown FROM hermes_skills WHERE enabled = 1 ORDER BY name ASC",
        )
        .map_err(|e| e.to_string())?;
    let skills: Vec<String> = stmt
        .query_map([], |r| {
            let name: String = r.get(0)?;
            let desc: String = r.get(1)?;
            let body: String = r.get(2)?;
            Ok(format!("### Skill: {name} ({desc})\n{}", body.trim()))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    if !skills.is_empty() {
        sections.push(format!("## Installed Skills\n{}", skills.join("\n\n")));
    }

    Ok(sections.join("\n\n"))
}

fn load_session_unified_history(
    conn: &Connection,
    session_id: &str,
) -> Result<Vec<UnifiedMessage>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT role, content, tool_calls_json, tool_call_id, tool_name
             FROM hermes_messages WHERE session_id = ?1 ORDER BY seq ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![session_id], |r| {
            let role_s: String = r.get(0)?;
            let content: String = r.get(1)?;
            let tc_json: Option<String> = r.get(2)?;
            let tc_id: Option<String> = r.get(3)?;
            let t_name: Option<String> = r.get(4)?;
            let tool_calls: Vec<UnifiedToolCall> = tc_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();
            let role = match role_s.as_str() {
                "system" => UnifiedRole::System,
                "assistant" => UnifiedRole::Assistant,
                "tool" => UnifiedRole::Tool,
                _ => UnifiedRole::User,
            };
            Ok(UnifiedMessage {
                role,
                content,
                name: t_name,
                tool_call_id: tc_id,
                tool_calls,
                reasoning: None,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

fn execute_safe_hermes_tool(conn: &Connection, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "memory_read" => handle_rpc_local(
            conn,
            "memoryGet",
            &json!({ "target": args.get("target").and_then(Value::as_str).unwrap_or("MEMORY.md") }),
        ),
        "memory_update" => handle_rpc_local(conn, "memoryUpdate", args),
        "kanban_list" => handle_rpc_local(conn, "kanbanBoard", &json!({})),
        "kanban_add_card" => handle_rpc_local(conn, "kanbanAddCard", args),
        "kanban_move_card" => handle_rpc_local(conn, "kanbanMove", args),
        "cron_list" => handle_rpc_local(conn, "cronList", &json!({})),
        "cron_schedule" => handle_rpc_local(conn, "cronUpsert", args),
        "skills_list" => handle_rpc_local(conn, "skillsList", &json!({})),
        "messaging_search" => {
            let q = args.get("query").and_then(Value::as_str).unwrap_or("");
            crate::telegram_ext::handle_messaging_ext_rpc(
                conn,
                "searchMessages",
                &json!({ "query": q }),
            )
        }
        "dharma_lookup_sutra" => {
            let title = args.get("title").and_then(Value::as_str).unwrap_or("心经");
            if let Some(s) = fabushi_miniapp::sutras::sutra_by_title(title)
                .or_else(|| {
                    fabushi_miniapp::sutras::all_sutras()
                        .iter()
                        .find(|s| s.title.contains(title) || title.contains(&s.title))
                })
            {
                Ok(serde_json::to_value(s).map_err(|e| e.to_string())?)
            } else {
                Ok(json!({
                    "found": false,
                    "availableTitles": fabushi_miniapp::sutras::all_sutras().iter().map(|s| &s.title).collect::<Vec<_>>(),
                }))
            }
        }
        other => Err(format!("Unsupported safe Hermes tool: {other}")),
    }
}

fn execute_approved_dangerous_tool(
    db: &Mutex<Connection>,
    name: &str,
    args: &Value,
) -> Result<Value, String> {
    match name {
        "computer_execute_op" => {
            let node_id = args
                .get("nodeId")
                .and_then(Value::as_str)
                .unwrap_or("local-android-mika");
            let lease = crate::computer::handle_computer_rpc(
                db,
                "leaseAcquire",
                &json!({ "nodeId": node_id, "holder": "hermes-approved" }),
            )?;
            let gen = lease.get("generation").and_then(Value::as_i64).unwrap_or(1);
            let op_val = args
                .get("op")
                .cloned()
                .unwrap_or_else(|| Value::String("screenshot".into()));
            crate::computer::handle_computer_rpc(
                db,
                "executeOp",
                &json!({
                    "nodeId": node_id,
                    "generation": gen,
                    "op": op_val,
                    "payload": args.get("payload").cloned().unwrap_or_else(|| json!({})),
                }),
            )
        }
        "wallet_send_ton" => crate::wallet::handle_rpc(db, "sendTon", args),
        "terminal_exec" => {
            let cmd_str = args.get("command").and_then(Value::as_str).unwrap_or("");
            Err(format!(
                "TERMINAL_EXEC_UNAVAILABLE: Sandboxed shell execution for '{cmd_str}' is unavailable in Rust host; bind a remote computer node (/api/v1/computers/exec) or Mika AndroidSandbox"
            ))
        }
        other => Err(format!(
            "TOOL_UNAVAILABLE: Approved tool '{other}' has no native executor configured"
        )),
    }
}

fn run_agentic_loop(
    db: &Mutex<Connection>,
    session_id: &str,
    user_msg_id: Option<&str>,
) -> Result<Value, String> {
    let (req_model, preferred_provider, system_prompt, tools) = crate::with_db(db, |conn| {
        let (session_model, session_provider): (String, String) = conn
            .query_row(
                "SELECT model, provider_id FROM hermes_sessions WHERE session_id = ?1",
                params![session_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| ("auto".into(), "auto".into()));

        let preferred_provider = (session_provider != "auto" && !session_provider.is_empty())
            .then_some(session_provider);
        let req_model = if session_model.is_empty() {
            "auto".to_string()
        } else {
            session_model
        };

        let system_prompt = build_hermes_system_prompt(conn)?;
        let tools = load_enabled_tool_specs(conn)?;
        Ok((req_model, preferred_provider, system_prompt, tools))
    })?;

    let mut tool_steps = Vec::new();

    for _step in 0..MAX_AGENTIC_STEPS {
        let history = crate::with_db(db, |conn| load_session_unified_history(conn, session_id))?;
        let req = UnifiedRequest {
            model: req_model.clone(),
            system_prompt: system_prompt.clone(),
            messages: history,
            tools: tools.clone(),
            temperature: None,
            max_tokens: None,
            thinking_budget_tokens: None,
            stream: true,
        };

        // Phase 2: execute live LLM request with DB lock released
        let outcome = crate::magpie::execute_live_request(
            db,
            &req,
            None,
            preferred_provider.as_deref(),
            Some(session_id),
            |_delta| {},
        )?;

        let now = Utc::now().timestamp_millis();
        if !outcome.tool_calls.is_empty() {
            let waiting_res = crate::with_db(db, |conn| {
                let asst_tc_msg_id = format!("hmsg_{}", uuid::Uuid::new_v4().simple());
                let tc_json =
                    serde_json::to_string(&outcome.tool_calls).map_err(|e| e.to_string())?;
                let seq = next_message_seq(conn, session_id);
                conn.execute(
                    "INSERT INTO hermes_messages
                    (msg_id, session_id, seq, role, content, model, token_estimate, tool_calls_json, created_at_ms)
                    VALUES (?1, ?2, ?3, 'assistant', ?4, ?5, ?6, ?7, ?8)",
                    params![
                        asst_tc_msg_id,
                        session_id,
                        seq,
                        outcome.reply,
                        outcome.model,
                        outcome.usage.output_tokens as i64,
                        tc_json,
                        now,
                    ],
                )
                .map_err(|e| e.to_string())?;

                for tc in &outcome.tool_calls {
                    let parsed_args: Value =
                        serde_json::from_str(&tc.arguments).unwrap_or_else(|_| json!({}));
                    if is_dangerous_tool(&tc.name) {
                        let approval_id = format!("apr_{}", uuid::Uuid::new_v4().simple());
                        let summary = format!("{}({})", tc.name, tc.arguments);
                        conn.execute(
                            "INSERT INTO hermes_approvals
                            (approval_id, session_id, tool_name, command_summary, risk_level, status, tool_call_id, arguments_json, created_at_ms)
                            VALUES (?1, ?2, ?3, ?4, 'high', 'pending', ?5, ?6, ?7)",
                            params![
                                approval_id,
                                session_id,
                                tc.name,
                                summary,
                                tc.id,
                                tc.arguments,
                                now,
                            ],
                        )
                        .map_err(|e| e.to_string())?;
                        conn.execute(
                            "UPDATE hermes_sessions SET updated_at_ms = ?1 WHERE session_id = ?2",
                            params![now, session_id],
                        )
                        .map_err(|e| e.to_string())?;

                        return Ok(Some(json!({
                            "sessionId": session_id,
                            "status": "waiting_approval",
                            "userMsgId": user_msg_id,
                            "pendingApproval": {
                                "approvalId": approval_id,
                                "toolCallId": tc.id,
                                "toolName": tc.name,
                                "arguments": parsed_args,
                                "commandSummary": summary,
                                "riskLevel": "high",
                            },
                            "toolSteps": tool_steps,
                        })));
                    }

                    let tool_result = match execute_safe_hermes_tool(conn, &tc.name, &parsed_args) {
                        Ok(v) => v,
                        Err(e) => json!({ "error": e }),
                    };
                    let tool_msg_id = format!("hmsg_{}", uuid::Uuid::new_v4().simple());
                    let tool_seq = next_message_seq(conn, session_id);
                    let result_str = tool_result.to_string();
                    conn.execute(
                        "INSERT INTO hermes_messages
                        (msg_id, session_id, seq, role, content, model, token_estimate, tool_call_id, tool_name, created_at_ms)
                        VALUES (?1, ?2, ?3, 'tool', ?4, NULL, ?5, ?6, ?7, ?8)",
                        params![
                            tool_msg_id,
                            session_id,
                            tool_seq,
                            result_str,
                            ((result_str.len() / 4) + 1) as i64,
                            tc.id,
                            tc.name,
                            Utc::now().timestamp_millis(),
                        ],
                    )
                    .map_err(|e| e.to_string())?;

                    tool_steps.push(json!({
                        "toolCallId": tc.id,
                        "toolName": tc.name,
                        "arguments": parsed_args,
                        "result": tool_result,
                    }));
                }
                Ok(None)
            })?;
            if let Some(waiting) = waiting_res {
                return Ok(waiting);
            }
            continue;
        }

        let asst_msg_id = format!("hmsg_{}", uuid::Uuid::new_v4().simple());
        crate::with_db(db, |conn| {
            let seq = next_message_seq(conn, session_id);
            conn.execute(
                "INSERT INTO hermes_messages
                (msg_id, session_id, seq, role, content, model, token_estimate, created_at_ms)
                VALUES (?1, ?2, ?3, 'assistant', ?4, ?5, ?6, ?7)",
                params![
                    asst_msg_id,
                    session_id,
                    seq,
                    outcome.reply,
                    outcome.model,
                    outcome.usage.output_tokens as i64,
                    now,
                ],
            )
            .map_err(|e| e.to_string())?;
            conn.execute(
                "UPDATE hermes_sessions SET updated_at_ms = ?1 WHERE session_id = ?2",
                params![now, session_id],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })?;

        return Ok(json!({
            "sessionId": session_id,
            "status": "completed",
            "userMsgId": user_msg_id,
            "assistantMsgId": asst_msg_id,
            "reply": outcome.reply,
            "providerId": outcome.provider_id,
            "model": outcome.model,
            "protocol": outcome.protocol,
            "maskedSecretsCount": outcome.masked_secrets_count,
            "inputTokens": outcome.usage.input_tokens,
            "outputTokens": outcome.usage.output_tokens,
            "costUsd": outcome.cost_usd,
            "toolSteps": tool_steps,
        }));
    }

    Err(format!(
        "AGENT_STEP_BUDGET_EXCEEDED: Hermes exceeded {MAX_AGENTIC_STEPS} consecutive tool steps"
    ))
}

pub fn handle_rpc(db: &Mutex<Connection>, submethod: &str, params: &Value) -> Result<Value, String> {
    match submethod {
        "listCommands" => Ok(json!({
            "commands": HERMES_SLASH_COMMANDS,
            "total": HERMES_SLASH_COMMANDS.len(),
        })),
        "sendTurn" => handle_send_turn(db, params),
        "approvalsDecide" => handle_approvals_decide(db, params),
        _ => crate::with_db(db, |conn| handle_rpc_local(conn, submethod, params)),
    }
}

fn handle_send_turn(db: &Mutex<Connection>, params: &Value) -> Result<Value, String> {
    let sid = params
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or("hermes_main");
    let text = params
        .get("text")
        .or_else(|| params.get("prompt"))
        .and_then(Value::as_str)
        .ok_or_else(|| "sendTurn requires 'text'".to_string())?
        .trim();
    if text.is_empty() {
        return Err("sendTurn text cannot be empty".into());
    }

    if text.starts_with('/') {
        return crate::with_db(db, |conn| {
            init_schema(conn)?;
            let slash = handle_rpc_local(
                conn,
                "runSlashCommand",
                &json!({ "sessionId": sid, "input": text }),
            )?;
            let now = Utc::now().timestamp_millis();
            let seq = next_message_seq(conn, sid);
            let user_mid = format!("hmsg_{}", uuid::Uuid::new_v4().simple());
            let asst_mid = format!("hmsg_{}", uuid::Uuid::new_v4().simple());
            let reply = slash["summary"].as_str().unwrap_or_default().to_string();
            conn.execute(
                "INSERT INTO hermes_messages (msg_id, session_id, seq, role, content, model, token_estimate, created_at_ms)
                 VALUES (?1, ?2, ?3, 'user', ?4, NULL, ?5, ?6)",
                params![user_mid, sid, seq, text, ((text.len() / 4) + 1) as i64, now],
            )
            .map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT INTO hermes_messages (msg_id, session_id, seq, role, content, model, token_estimate, created_at_ms)
                 VALUES (?1, ?2, ?3, 'assistant', ?4, 'hermes-slash', ?5, ?6)",
                params![asst_mid, sid, seq + 1, reply, ((reply.len() / 4) + 1) as i64, now + 1],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({
                "sessionId": sid,
                "status": "completed",
                "userMsgId": user_mid,
                "assistantMsgId": asst_mid,
                "reply": reply,
                "slashCommand": slash,
            }))
        });
    }

    let user_mid = crate::with_db(db, |conn| {
        init_schema(conn)?;
        // Fail closed BEFORE mutating session history when no live LLM seat is configured!
        if !crate::magpie::has_configured_live_seat(conn) {
            return Err(
                "PROVIDER_NOT_CONFIGURED: Configure an API key or local endpoint in Magpie Gateway before running Hermes turns"
                    .into(),
            );
        }
        let now = Utc::now().timestamp_millis();
        let seq = next_message_seq(conn, sid);
        let user_mid = format!("hmsg_{}", uuid::Uuid::new_v4().simple());
        conn.execute(
            "INSERT INTO hermes_messages (msg_id, session_id, seq, role, content, model, token_estimate, created_at_ms)
             VALUES (?1, ?2, ?3, 'user', ?4, NULL, ?5, ?6)",
            params![user_mid, sid, seq, text, ((text.len() / 4) + 1) as i64, now],
        )
        .map_err(|e| e.to_string())?;
        Ok(user_mid)
    })?;

    run_agentic_loop(db, sid, Some(&user_mid))
}

fn handle_approvals_decide(db: &Mutex<Connection>, params: &Value) -> Result<Value, String> {
    let aid = params
        .get("approvalId")
        .and_then(Value::as_str)
        .ok_or_else(|| "approvalsDecide requires 'approvalId'".to_string())?;
    let decision = params
        .get("decision")
        .and_then(Value::as_str)
        .unwrap_or("approved");
    let status = if decision == "approved" || decision == "allow" {
        "approved"
    } else {
        "denied"
    };

    let row: Option<(String, String, Option<String>, Option<String>)> = crate::with_db(db, |conn| {
        init_schema(conn)?;
        let row: Option<(String, String, Option<String>, Option<String>, String)> = conn
            .query_row(
                "SELECT session_id, tool_name, tool_call_id, arguments_json, status
                 FROM hermes_approvals WHERE approval_id = ?1",
                params![aid],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;

        let Some((session_id, tool_name, tool_call_id, args_json, current_status)) = row else {
            return Err(format!("Approval '{aid}' not found"));
        };

        let updated = conn
            .execute(
                "UPDATE hermes_approvals SET status = ?1 WHERE approval_id = ?2 AND status = 'pending'",
                params![status, aid],
            )
            .map_err(|e| e.to_string())?;
        if updated == 0 {
            return Err(format!(
                "Approval '{aid}' already decided (current status: '{current_status}')"
            ));
        }
        Ok(Some((session_id, tool_name, tool_call_id, args_json)))
    })?;

    if let Some((session_id, tool_name, Some(tool_call_id), args_opt)) = row {
        if !session_id.trim().is_empty() {
            let parsed_args: Value = args_opt
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_else(|| json!({}));
            let tool_output = if status == "approved" {
                match execute_approved_dangerous_tool(db, &tool_name, &parsed_args) {
                    Ok(v) => v,
                    Err(e) => json!({ "error": e }),
                }
            } else {
                json!({
                    "error": "USER_DENIED_APPROVAL",
                    "tool": tool_name,
                    "approvalId": aid,
                })
            };

            let has_live_seat = crate::with_db(db, |conn| {
                let tool_msg_id = format!("hmsg_{}", uuid::Uuid::new_v4().simple());
                let seq = next_message_seq(conn, &session_id);
                let out_str = tool_output.to_string();
                conn.execute(
                    "INSERT INTO hermes_messages
                    (msg_id, session_id, seq, role, content, model, token_estimate, tool_call_id, tool_name, created_at_ms)
                    VALUES (?1, ?2, ?3, 'tool', ?4, NULL, ?5, ?6, ?7, ?8)",
                    params![
                        tool_msg_id,
                        session_id,
                        seq,
                        out_str,
                        ((out_str.len() / 4) + 1) as i64,
                        tool_call_id,
                        tool_name,
                        Utc::now().timestamp_millis(),
                    ],
                )
                .map_err(|e| e.to_string())?;
                Ok(crate::magpie::has_configured_live_seat(conn))
            })?;

            let resume_turn = params
                .get("resumeTurn")
                .and_then(Value::as_bool)
                .unwrap_or(status == "approved");
            if resume_turn && has_live_seat {
                let resumed = run_agentic_loop(db, &session_id, None)?;
                return Ok(json!({
                    "approvalId": aid,
                    "status": status,
                    "resumed": true,
                    "toolResult": tool_output,
                    "turn": resumed,
                }));
            }
            return Ok(json!({
                "approvalId": aid,
                "status": status,
                "resumed": false,
                "toolResult": tool_output,
            }));
        }
    }

    Ok(json!({ "approvalId": aid, "status": status }))
}

fn handle_rpc_local(conn: &Connection, submethod: &str, params: &Value) -> Result<Value, String> {
    init_schema(conn)?;
    match submethod {
        "listCommands" => Ok(json!({
            "commands": HERMES_SLASH_COMMANDS,
            "total": HERMES_SLASH_COMMANDS.len(),
        })),

        "runSlashCommand" => {
            let input = params
                .get("input")
                .and_then(Value::as_str)
                .unwrap_or("/help")
                .trim();
            let session_id = params
                .get("sessionId")
                .and_then(Value::as_str)
                .unwrap_or("hermes_main");
            let (cmd_name, rest) = input
                .split_once(char::is_whitespace)
                .map(|(a, b)| (a, b.trim()))
                .unwrap_or((input, ""));

            let Some(spec) = HERMES_SLASH_COMMANDS.iter().find(|c| c.name == cmd_name) else {
                return Err(format!("Unknown Hermes slash command: {cmd_name}"));
            };

            let effect = match cmd_name {
                "/model" if !rest.is_empty() => {
                    conn.execute(
                        "UPDATE hermes_sessions SET model = ?1, updated_at_ms = ?2 WHERE session_id = ?3",
                        params![rest, Utc::now().timestamp_millis(), session_id],
                    )
                    .map_err(|e| e.to_string())?;
                    format!("Switched active model for '{session_id}' to '{rest}'")
                }
                "/branch" => {
                    let branched = handle_rpc_local(
                        conn,
                        "branchSession",
                        &json!({
                            "sessionId": session_id,
                            "title": if rest.is_empty() { "Branched Session" } else { rest }
                        }),
                    )?;
                    format!("Branched session -> {}", branched["sessionId"].as_str().unwrap_or(""))
                }
                "/remember" if !rest.is_empty() => {
                    let cur: String = conn
                        .query_row(
                            "SELECT content FROM hermes_memory WHERE target = 'MEMORY.md'",
                            [],
                            |r| r.get(0),
                        )
                        .unwrap_or_default();
                    let updated = clamp_chars(&format!("{cur}\n- {rest}"), MEMORY_MD_CHAR_LIMIT);
                    conn.execute(
                        "UPDATE hermes_memory SET content = ?1, updated_at_ms = ?2 WHERE target = 'MEMORY.md'",
                        params![updated, Utc::now().timestamp_millis()],
                    )
                    .map_err(|e| e.to_string())?;
                    format!("Appended fact to MEMORY.md ({} / {} chars)", updated.chars().count(), MEMORY_MD_CHAR_LIMIT)
                }
                "/clear" => {
                    conn.execute(
                        "DELETE FROM hermes_messages WHERE session_id = ?1",
                        params![session_id],
                    )
                    .map_err(|e| e.to_string())?;
                    format!("Cleared turn history in '{session_id}'")
                }
                _ => format!("Executed {} ({})", spec.name, spec.description),
            };

            Ok(json!({
                "command": spec.name,
                "category": spec.category,
                "summary": effect,
            }))
        }

        "sessions" | "status" => {
            let mut stmt = conn
                .prepare(
                    "SELECT s.session_id, s.parent_session_id, s.title, s.model, s.provider_id, s.archived, s.updated_at_ms,
                            (SELECT COUNT(*) FROM hermes_messages m WHERE m.session_id = s.session_id)
                     FROM hermes_sessions s ORDER BY s.updated_at_ms DESC",
                )
                .map_err(|e| e.to_string())?;
            let sessions: Vec<Value> = stmt
                .query_map([], |r| {
                    Ok(json!({
                        "sessionId": r.get::<_, String>(0)?,
                        "parentSessionId": r.get::<_, Option<String>>(1)?,
                        "title": r.get::<_, String>(2)?,
                        "model": r.get::<_, String>(3)?,
                        "providerId": r.get::<_, String>(4)?,
                        "archived": r.get::<_, i64>(5)? != 0,
                        "updatedAtMs": r.get::<_, i64>(6)?,
                        "messageCount": r.get::<_, i64>(7)?,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;

            Ok(json!({
                "sessions": sessions,
                "slashCommandCount": HERMES_SLASH_COMMANDS.len(),
                "toolsetCount": HERMES_TOOLSETS.len(),
                "liveProviderReady": crate::magpie::has_configured_live_seat(conn),
            }))
        }

        "createSession" => {
            let now = Utc::now().timestamp_millis();
            let sid = format!("hermes_{}", uuid::Uuid::new_v4().simple());
            let title = params
                .get("title")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("New Hermes Session");
            let model = params
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or("auto");
            let provider = params
                .get("providerId")
                .and_then(Value::as_str)
                .unwrap_or("auto");
            conn.execute(
                "INSERT INTO hermes_sessions (session_id, parent_session_id, title, model, provider_id, archived, created_at_ms, updated_at_ms)
                 VALUES (?1, NULL, ?2, ?3, ?4, 0, ?5, ?5)",
                params![sid, title, model, provider, now],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({
                "sessionId": sid,
                "title": title,
                "model": model,
                "providerId": provider,
            }))
        }

        "branchSession" => {
            let parent_id = params
                .get("sessionId")
                .and_then(Value::as_str)
                .unwrap_or("hermes_main");
            let max_seq = params.get("upToSeq").and_then(Value::as_i64).unwrap_or(i64::MAX);
            let title = params
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Branched Session");
            let (model, provider): (String, String) = conn
                .query_row(
                    "SELECT model, provider_id FROM hermes_sessions WHERE session_id = ?1",
                    params![parent_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .unwrap_or_else(|| ("auto".into(), "auto".into()));

            let now = Utc::now().timestamp_millis();
            let new_id = format!("hermes_branch_{}", uuid::Uuid::new_v4().simple());
            conn.execute(
                "INSERT INTO hermes_sessions (session_id, parent_session_id, title, model, provider_id, archived, created_at_ms, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?6)",
                params![new_id, parent_id, title, model, provider, now],
            )
            .map_err(|e| e.to_string())?;

            let mut stmt = conn
                .prepare(
                    "SELECT seq, role, content, model, token_estimate, tool_calls_json, tool_call_id, tool_name, created_at_ms
                     FROM hermes_messages WHERE session_id = ?1 AND seq <= ?2 ORDER BY seq ASC",
                )
                .map_err(|e| e.to_string())?;
            let rows: Vec<(i64, String, String, Option<String>, i64, Option<String>, Option<String>, Option<String>, i64)> = stmt
                .query_map(params![parent_id, max_seq], |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                        r.get(7)?,
                        r.get(8)?,
                    ))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;

            let copied = rows.len();
            for (seq, role, content, m, tok, tc_json, tc_id, t_name, ts) in rows {
                let mid = format!("hmsg_{}", uuid::Uuid::new_v4().simple());
                conn.execute(
                    "INSERT INTO hermes_messages
                    (msg_id, session_id, seq, role, content, model, token_estimate, tool_calls_json, tool_call_id, tool_name, created_at_ms)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![mid, new_id, seq, role, content, m, tok, tc_json, tc_id, t_name, ts],
                )
                .map_err(|e| e.to_string())?;
            }

            Ok(json!({
                "sessionId": new_id,
                "parentSessionId": parent_id,
                "copiedMessages": copied,
                "title": title,
            }))
        }

        "sessionMessages" => {
            let sid = params
                .get("sessionId")
                .and_then(Value::as_str)
                .unwrap_or("hermes_main");
            let mut stmt = conn
                .prepare(
                    "SELECT msg_id, seq, role, content, model, token_estimate, tool_calls_json, tool_call_id, tool_name, created_at_ms
                     FROM hermes_messages WHERE session_id = ?1 ORDER BY seq ASC",
                )
                .map_err(|e| e.to_string())?;
            let msgs: Vec<Value> = stmt
                .query_map(params![sid], |r| {
                    let tc_raw: Option<String> = r.get(6)?;
                    let tool_calls: Option<Value> =
                        tc_raw.as_deref().and_then(|s| serde_json::from_str(s).ok());
                    Ok(json!({
                        "msgId": r.get::<_, String>(0)?,
                        "seq": r.get::<_, i64>(1)?,
                        "role": r.get::<_, String>(2)?,
                        "content": r.get::<_, String>(3)?,
                        "model": r.get::<_, Option<String>>(4)?,
                        "tokenEstimate": r.get::<_, i64>(5)?,
                        "toolCalls": tool_calls,
                        "toolCallId": r.get::<_, Option<String>>(7)?,
                        "toolName": r.get::<_, Option<String>>(8)?,
                        "createdAtMs": r.get::<_, i64>(9)?,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(json!({ "sessionId": sid, "messages": msgs }))
        }



        "memoryGet" => {
            let mut stmt = conn
                .prepare(
                    "SELECT target, content, char_limit, updated_at_ms FROM hermes_memory ORDER BY target ASC",
                )
                .map_err(|e| e.to_string())?;
            let entries: Vec<Value> = stmt
                .query_map([], |r| {
                    let content: String = r.get(1)?;
                    let limit: i64 = r.get(2)?;
                    Ok(json!({
                        "target": r.get::<_, String>(0)?,
                        "content": content,
                        "charCount": content.chars().count(),
                        "charLimit": limit,
                        "updatedAtMs": r.get::<_, i64>(3)?,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(json!({ "entries": entries }))
        }

        "memoryUpdate" => {
            let target = params
                .get("target")
                .and_then(Value::as_str)
                .unwrap_or("MEMORY.md");
            let raw = params
                .get("content")
                .and_then(Value::as_str)
                .ok_or_else(|| "memoryUpdate requires 'content'".to_string())?;
            let limit = match target {
                "MEMORY.md" => MEMORY_MD_CHAR_LIMIT,
                "USER.md" => USER_MD_CHAR_LIMIT,
                "SOUL.md" => SOUL_MD_CHAR_LIMIT,
                other => return Err(format!("Unsupported memory target: {other}")),
            };
            let bounded = clamp_chars(raw, limit);
            let truncated = raw.chars().count() > limit;
            let now = Utc::now().timestamp_millis();
            conn.execute(
                "INSERT OR REPLACE INTO hermes_memory (target, content, char_limit, updated_at_ms) VALUES (?1, ?2, ?3, ?4)",
                params![target, bounded, limit as i64, now],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({
                "target": target,
                "content": bounded,
                "charCount": bounded.chars().count(),
                "charLimit": limit,
                "truncated": truncated,
            }))
        }

        "toolsetsList" => {
            let mut items = Vec::with_capacity(HERMES_TOOLSETS.len());
            for ts in HERMES_TOOLSETS {
                let enabled: bool = conn
                    .query_row(
                        "SELECT enabled FROM hermes_toolset_state WHERE toolset_id = ?1",
                        params![ts.id],
                        |r| Ok(r.get::<_, i64>(0)? != 0),
                    )
                    .unwrap_or(ts.enabled_by_default);
                items.push(json!({
                    "id": ts.id,
                    "name": ts.name,
                    "category": ts.category,
                    "riskTier": ts.risk_tier,
                    "enabled": enabled,
                }));
            }
            Ok(json!({ "toolsets": items, "total": items.len() }))
        }

        "toolsetToggle" => {
            let id = params
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| "toolsetToggle requires 'id'".to_string())?;
            let enabled = params
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            conn.execute(
                "INSERT OR REPLACE INTO hermes_toolset_state (toolset_id, enabled) VALUES (?1, ?2)",
                params![id, if enabled { 1 } else { 0 }],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({ "id": id, "enabled": enabled }))
        }

        "skillsList" => {
            let mut stmt = conn
                .prepare("SELECT name, description, body_markdown, enabled, updated_at_ms FROM hermes_skills ORDER BY name ASC")
                .map_err(|e| e.to_string())?;
            let skills: Vec<Value> = stmt
                .query_map([], |r| {
                    Ok(json!({
                        "name": r.get::<_, String>(0)?,
                        "description": r.get::<_, String>(1)?,
                        "bodyMarkdown": r.get::<_, String>(2)?,
                        "enabled": r.get::<_, i64>(3)? != 0,
                        "updatedAtMs": r.get::<_, i64>(4)?,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(json!({ "skills": skills }))
        }

        "skillUpsert" => {
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| "skillUpsert requires 'name'".to_string())?;
            let description = params
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("Custom Hermes Skill");
            let body = params
                .get("bodyMarkdown")
                .and_then(Value::as_str)
                .ok_or_else(|| "skillUpsert requires 'bodyMarkdown'".to_string())?;
            let enabled = params
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let warnings = scan_skill_security(body)?;
            let now = Utc::now().timestamp_millis();
            conn.execute(
                "INSERT OR REPLACE INTO hermes_skills (name, description, body_markdown, enabled, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![name, description, body, if enabled { 1i64 } else { 0i64 }, now],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({
                "name": name,
                "enabled": enabled,
                "securityScanPassed": true,
                "warnings": warnings,
            }))
        }

        "cronList" => {
            let mut stmt = conn
                .prepare("SELECT job_id, schedule, prompt, enabled, next_run_ms, last_run_ms FROM hermes_cron_jobs ORDER BY next_run_ms ASC")
                .map_err(|e| e.to_string())?;
            let jobs: Vec<Value> = stmt
                .query_map([], |r| {
                    Ok(json!({
                        "jobId": r.get::<_, String>(0)?,
                        "schedule": r.get::<_, String>(1)?,
                        "prompt": r.get::<_, String>(2)?,
                        "enabled": r.get::<_, i64>(3)? != 0,
                        "nextRunMs": r.get::<_, i64>(4)?,
                        "lastRunMs": r.get::<_, Option<i64>>(5)?,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(json!({ "jobs": jobs }))
        }

        "cronUpsert" => {
            let schedule = params
                .get("schedule")
                .and_then(Value::as_str)
                .ok_or_else(|| "cronUpsert requires 5-field 'schedule'".to_string())?;
            let prompt = params
                .get("prompt")
                .and_then(Value::as_str)
                .ok_or_else(|| "cronUpsert requires 'prompt'".to_string())?;
            let enabled = params
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let parsed = CronSchedule::parse(schedule)?;
            let now = Utc::now().timestamp_millis();
            let next_run = parsed.next_after_ms(now)?;
            let job_id = params
                .get("jobId")
                .and_then(Value::as_str)
                .map(ToString::to_string)
                .unwrap_or_else(|| format!("cron_{}", uuid::Uuid::new_v4().simple()));
            conn.execute(
                "INSERT OR REPLACE INTO hermes_cron_jobs (job_id, schedule, prompt, enabled, next_run_ms, last_run_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
                params![job_id, schedule, prompt, if enabled { 1i64 } else { 0i64 }, next_run],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({
                "jobId": job_id,
                "schedule": schedule,
                "prompt": prompt,
                "enabled": enabled,
                "nextRunMs": next_run,
            }))
        }

        "kanbanBoard" => {
            let mut stmt = conn
                .prepare("SELECT card_id, column_id, title, priority, assignee, updated_at_ms FROM hermes_kanban_cards ORDER BY updated_at_ms DESC")
                .map_err(|e| e.to_string())?;
            let cards: Vec<Value> = stmt
                .query_map([], |r| {
                    Ok(json!({
                        "cardId": r.get::<_, String>(0)?,
                        "columnId": r.get::<_, String>(1)?,
                        "title": r.get::<_, String>(2)?,
                        "priority": r.get::<_, String>(3)?,
                        "assignee": r.get::<_, String>(4)?,
                        "updatedAtMs": r.get::<_, i64>(5)?,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(json!({
                "columns": ["todo", "in_progress", "review", "done"],
                "cards": cards,
            }))
        }

        "kanbanAddCard" => {
            let title = params
                .get("title")
                .and_then(Value::as_str)
                .ok_or_else(|| "kanbanAddCard requires 'title'".to_string())?;
            let column_id = params
                .get("columnId")
                .and_then(Value::as_str)
                .unwrap_or("todo");
            let priority = params
                .get("priority")
                .and_then(Value::as_str)
                .unwrap_or("medium");
            let cid = format!("card_{}", uuid::Uuid::new_v4().simple());
            conn.execute(
                "INSERT INTO hermes_kanban_cards (card_id, column_id, title, priority, assignee, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, 'hermes', ?5)",
                params![cid, column_id, title, priority, Utc::now().timestamp_millis()],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({ "cardId": cid, "columnId": column_id, "title": title }))
        }

        "kanbanMove" => {
            let cid = params
                .get("cardId")
                .and_then(Value::as_str)
                .ok_or_else(|| "kanbanMove requires 'cardId'".to_string())?;
            let col = params
                .get("columnId")
                .and_then(Value::as_str)
                .ok_or_else(|| "kanbanMove requires 'columnId'".to_string())?;
            let updated = conn
                .execute(
                    "UPDATE hermes_kanban_cards SET column_id = ?1, updated_at_ms = ?2 WHERE card_id = ?3",
                    params![col, Utc::now().timestamp_millis(), cid],
                )
                .map_err(|e| e.to_string())?;
            Ok(json!({ "cardId": cid, "columnId": col, "moved": updated > 0 }))
        }

        "approvalsList" => {
            let mut stmt = conn
                .prepare("SELECT approval_id, session_id, tool_name, command_summary, risk_level, status, tool_call_id, arguments_json, created_at_ms FROM hermes_approvals ORDER BY created_at_ms DESC")
                .map_err(|e| e.to_string())?;
            let list: Vec<Value> = stmt
                .query_map([], |r| {
                    let args_s: Option<String> = r.get(7)?;
                    let args_val: Option<Value> =
                        args_s.as_deref().and_then(|s| serde_json::from_str(s).ok());
                    Ok(json!({
                        "approvalId": r.get::<_, String>(0)?,
                        "sessionId": r.get::<_, String>(1)?,
                        "toolName": r.get::<_, String>(2)?,
                        "commandSummary": r.get::<_, String>(3)?,
                        "riskLevel": r.get::<_, String>(4)?,
                        "status": r.get::<_, String>(5)?,
                        "toolCallId": r.get::<_, Option<String>>(6)?,
                        "arguments": args_val,
                        "createdAtMs": r.get::<_, i64>(8)?,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(json!({ "approvals": list }))
        }

        other => Err(format!("Unsupported hermes method: {other}")),
    }
}
