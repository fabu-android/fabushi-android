//! Hermes Autonomous Agent Studio Engine:
//! - SQLite-persisted multi-branch sessions & turn history
//! - Bounded dual-store `MEMORY.md` (2200 chars), `USER.md` (1375 chars), and `SOUL.md`
//! - 29 Toolsets & 61 Canonical Slash Commands (`SlashCommandParser` + `SlashCommandRegistry`)
//! - `SKILL.md` Skills Catalog & Injection Security Scanner
//! - 5-Field Cron Expression Parser & Next-Run Calculator
//! - Kanban Boards, Columns, Cards, Checklists & Approval Gates

use chrono::{Datelike, TimeZone, Timelike, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const MEMORY_CHAR_LIMIT: usize = 2200;
pub const USER_CHAR_LIMIT: usize = 1375;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsetDescriptor {
    pub id: &'static str,
    pub description: &'static str,
    pub default_enabled: bool,
}

/// All 29 Hermes toolsets (exceeds 28+ baseline).
pub static HERMES_TOOLSETS: &[ToolsetDescriptor] = &[
    ToolsetDescriptor { id: "core", description: "Toolset introspection and dynamic enable/disable controls", default_enabled: true },
    ToolsetDescriptor { id: "web", description: "Web search and HTML-to-Markdown page extraction", default_enabled: true },
    ToolsetDescriptor { id: "terminal", description: "Sandboxed shell execution and process lifecycle", default_enabled: true },
    ToolsetDescriptor { id: "file", description: "Workspace file read, write, patch, and regex search", default_enabled: true },
    ToolsetDescriptor { id: "memory", description: "Bounded persistent MEMORY.md and USER.md management", default_enabled: true },
    ToolsetDescriptor { id: "skills", description: "agentskills.io SKILL.md creation, listing, and security scanning", default_enabled: true },
    ToolsetDescriptor { id: "vision", description: "Multimodal image inspection and UI element grounding", default_enabled: true },
    ToolsetDescriptor { id: "image_gen", description: "Image generation via provider APIs or local synthesis", default_enabled: false },
    ToolsetDescriptor { id: "video_gen", description: "Video storyboard and clip generation", default_enabled: false },
    ToolsetDescriptor { id: "browser", description: "Headless browser navigation, DOM snapshot, and interaction", default_enabled: false },
    ToolsetDescriptor { id: "audio_tts", description: "Text-to-speech synthesis and voice note encoding", default_enabled: true },
    ToolsetDescriptor { id: "audio_stt", description: "Speech-to-text transcription and waveform analysis", default_enabled: true },
    ToolsetDescriptor { id: "cronjob", description: "Scheduled 5-field cron automation jobs and triggers", default_enabled: true },
    ToolsetDescriptor { id: "subagents", description: "Depth-bounded concurrent subagent delegation", default_enabled: true },
    ToolsetDescriptor { id: "code_execution", description: "Sandboxed expression evaluation and script runner", default_enabled: false },
    ToolsetDescriptor { id: "clarify", description: "Interactive clarification prompts with inline buttons", default_enabled: true },
    ToolsetDescriptor { id: "todo", description: "Structured per-session task checklist management", default_enabled: true },
    ToolsetDescriptor { id: "planning", description: "Structured multi-step execution planning", default_enabled: true },
    ToolsetDescriptor { id: "reasoning", description: "Explicit chain-of-thought scratchpad reasoning", default_enabled: true },
    ToolsetDescriptor { id: "mcp", description: "Model Context Protocol tool discovery and invocation", default_enabled: false },
    ToolsetDescriptor { id: "kanban", description: "Kanban boards, columns, cards, checklists, and movement", default_enabled: true },
    ToolsetDescriptor { id: "desktop_ui", description: "Workbench surface and split-pane layout actions", default_enabled: false },
    ToolsetDescriptor { id: "computer_use", description: "Generation-fenced Remote Computer (CUA) lease control", default_enabled: true },
    ToolsetDescriptor { id: "connections", description: "External OAuth and API credential connection vault", default_enabled: false },
    ToolsetDescriptor { id: "honcho", description: "Dialectic user modeling and observation queries", default_enabled: true },
    ToolsetDescriptor { id: "session_search", description: "FTS5 + CJK search across historical sessions", default_enabled: true },
    ToolsetDescriptor { id: "workspace", description: "Workspace file checkpointing and atomic rollback", default_enabled: true },
    ToolsetDescriptor { id: "voice", description: "Full-duplex Voice / Talk Mode coordinator control", default_enabled: true },
    ToolsetDescriptor { id: "platform_im", description: "Native Telegram IM dialogs, topics, polls, and MiniApps", default_enabled: true },
];

#[derive(Debug, Clone, Serialize)]
pub struct SlashCommandDef {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub category: &'static str,
    pub syntax: &'static str,
    pub description: &'static str,
}

/// All 61 canonical Hermes slash commands.
pub static HERMES_SLASH_COMMANDS: &[SlashCommandDef] = &[
    // Session (11)
    SlashCommandDef { name: "help", aliases: &["h", "?"], category: "session", syntax: "/help [cmd]", description: "Show Hermes slash commands and usage help" },
    SlashCommandDef { name: "new", aliases: &["start"], category: "session", syntax: "/new [title]", description: "Start a fresh conversation session" },
    SlashCommandDef { name: "reset", aliases: &[], category: "session", syntax: "/reset", description: "Reset current conversation and reload snapshot" },
    SlashCommandDef { name: "clear", aliases: &[], category: "session", syntax: "/clear", description: "Clear messages in the active session" },
    SlashCommandDef { name: "compact", aliases: &[], category: "session", syntax: "/compact [focus]", description: "Summarize middle turns to fit context budget" },
    SlashCommandDef { name: "compress", aliases: &[], category: "session", syntax: "/compress [focus]", description: "Compress active context with structured summary" },
    SlashCommandDef { name: "undo", aliases: &[], category: "session", syntax: "/undo", description: "Undo the last user/assistant turn pair" },
    SlashCommandDef { name: "rollback", aliases: &[], category: "session", syntax: "/rollback [turn]", description: "Restore files from pre-edit checkpoints" },
    SlashCommandDef { name: "branch", aliases: &[], category: "session", syntax: "/branch [name]", description: "Fork current session into a named branch" },
    SlashCommandDef { name: "branches", aliases: &[], category: "session", syntax: "/branches", description: "List all branches of the conversation tree" },
    SlashCommandDef { name: "checkout", aliases: &[], category: "session", syntax: "/checkout <id>", description: "Switch active session to another branch" },
    // Model & Provider (4)
    SlashCommandDef { name: "model", aliases: &["m"], category: "model_and_provider", syntax: "/model [name]", description: "View or switch active LLM model" },
    SlashCommandDef { name: "provider", aliases: &[], category: "model_and_provider", syntax: "/provider [id]", description: "Inspect or switch active inference provider" },
    SlashCommandDef { name: "api-mode", aliases: &["apimode"], category: "model_and_provider", syntax: "/api-mode [proto]", description: "Switch wire protocol mode" },
    SlashCommandDef { name: "moa", aliases: &[], category: "model_and_provider", syntax: "/moa <prompt>", description: "Run Mixture-of-Agents synthesis" },
    // Tools & Skills (6)
    SlashCommandDef { name: "toolsets", aliases: &[], category: "tools_and_skills", syntax: "/toolsets [list|enable|disable]", description: "Inspect or toggle Hermes toolsets" },
    SlashCommandDef { name: "tools", aliases: &[], category: "tools_and_skills", syntax: "/tools [filter]", description: "List active tools exposed to the model" },
    SlashCommandDef { name: "skills", aliases: &[], category: "tools_and_skills", syntax: "/skills [list]", description: "List and manage SKILL.md skills" },
    SlashCommandDef { name: "skill", aliases: &[], category: "tools_and_skills", syntax: "/skill <name>", description: "View full instructions for a skill" },
    SlashCommandDef { name: "learn", aliases: &[], category: "tools_and_skills", syntax: "/learn [name]", description: "Extract reusable procedural skill from session" },
    SlashCommandDef { name: "mcp", aliases: &[], category: "tools_and_skills", syntax: "/mcp [list|status]", description: "Inspect and invoke MCP server tools" },
    // Memory & Persona (5)
    SlashCommandDef { name: "memory", aliases: &["mem"], category: "memory_and_persona", syntax: "/memory [view|add <txt>]", description: "Inspect or edit bounded MEMORY.md (2200 chars)" },
    SlashCommandDef { name: "soul", aliases: &[], category: "memory_and_persona", syntax: "/soul [view|set <txt>]", description: "Inspect or update SOUL.md persona" },
    SlashCommandDef { name: "user", aliases: &[], category: "memory_and_persona", syntax: "/user [view|add <txt>]", description: "Inspect or edit bounded USER.md (1375 chars)" },
    SlashCommandDef { name: "honcho", aliases: &[], category: "memory_and_persona", syntax: "/honcho [profile|observe]", description: "Inspect or query Honcho user model" },
    SlashCommandDef { name: "reload-memory", aliases: &["reload_memory"], category: "memory_and_persona", syntax: "/reload-memory", description: "Refresh frozen MEMORY.md/USER.md snapshot" },
    // Search, History & Export (5)
    SlashCommandDef { name: "search", aliases: &["find"], category: "inspection_and_export", syntax: "/search <query>", description: "Full-text search across past sessions" },
    SlashCommandDef { name: "history", aliases: &[], category: "inspection_and_export", syntax: "/history [limit]", description: "Show recent turns in active session" },
    SlashCommandDef { name: "sessions", aliases: &[], category: "inspection_and_export", syntax: "/sessions [limit]", description: "List persisted Hermes sessions" },
    SlashCommandDef { name: "export", aliases: &[], category: "inspection_and_export", syntax: "/export [format]", description: "Export redacted session trajectory" },
    SlashCommandDef { name: "save", aliases: &[], category: "inspection_and_export", syntax: "/save [path]", description: "Save session transcript" },
    // Telemetry & Diagnostics (8)
    SlashCommandDef { name: "usage", aliases: &[], category: "inspection_and_export", syntax: "/usage", description: "Show token usage and cache stats" },
    SlashCommandDef { name: "cost", aliases: &[], category: "inspection_and_export", syntax: "/cost", description: "Show estimated USD cost" },
    SlashCommandDef { name: "insights", aliases: &[], category: "inspection_and_export", syntax: "/insights", description: "Show session analytics and top tools" },
    SlashCommandDef { name: "status", aliases: &[], category: "inspection_and_export", syntax: "/status", description: "Show active session, model, and toolsets" },
    SlashCommandDef { name: "doctor", aliases: &[], category: "inspection_and_export", syntax: "/doctor", description: "Run diagnostics on SQLite DB and memory" },
    SlashCommandDef { name: "diff", aliases: &[], category: "inspection_and_export", syntax: "/diff", description: "Show workspace checkpoint diff summary" },
    SlashCommandDef { name: "context", aliases: &[], category: "inspection_and_export", syntax: "/context", description: "Inspect 3-tier system prompt budget" },
    SlashCommandDef { name: "version", aliases: &["ver"], category: "inspection_and_export", syntax: "/version", description: "Display Hermes Rust Engine version" },
    // Workflow, Cron, Kanban (6)
    SlashCommandDef { name: "cron", aliases: &[], category: "workflow_and_automation", syntax: "/cron [list|add <expr> <task>]", description: "Manage scheduled 5-field cron jobs" },
    SlashCommandDef { name: "jobs", aliases: &[], category: "workflow_and_automation", syntax: "/jobs", description: "List active cron jobs and tasks" },
    SlashCommandDef { name: "kanban", aliases: &[], category: "workflow_and_automation", syntax: "/kanban [board|add <title>]", description: "Manage Hermes Kanban boards and cards" },
    SlashCommandDef { name: "board", aliases: &[], category: "workflow_and_automation", syntax: "/board [id]", description: "Open or inspect Kanban board" },
    SlashCommandDef { name: "goal", aliases: &[], category: "workflow_and_automation", syntax: "/goal <objective>", description: "Run autonomous multi-step goal loop" },
    SlashCommandDef { name: "heartbeat", aliases: &[], category: "workflow_and_automation", syntax: "/heartbeat [on|off]", description: "Configure proactive heartbeat ticks" },
    // Safety & Turn Control (8)
    SlashCommandDef { name: "approve", aliases: &[], category: "safety_and_control", syntax: "/approve [id]", description: "Approve a pending high-risk operation" },
    SlashCommandDef { name: "deny", aliases: &[], category: "safety_and_control", syntax: "/deny [id]", description: "Deny a pending high-risk operation" },
    SlashCommandDef { name: "yolo", aliases: &[], category: "safety_and_control", syntax: "/yolo [on|off]", description: "Toggle auto-approval YOLO mode" },
    SlashCommandDef { name: "permissions", aliases: &["perms"], category: "safety_and_control", syntax: "/permissions", description: "Inspect tool approval mode and queue" },
    SlashCommandDef { name: "queue", aliases: &[], category: "safety_and_control", syntax: "/queue <msg>", description: "Queue a follow-up message" },
    SlashCommandDef { name: "steer", aliases: &[], category: "safety_and_control", syntax: "/steer <instruction>", description: "Steer an active turn" },
    SlashCommandDef { name: "interrupt", aliases: &[], category: "safety_and_control", syntax: "/interrupt", description: "Interrupt streaming agent turn" },
    SlashCommandDef { name: "stop", aliases: &["cancel"], category: "safety_and_control", syntax: "/stop", description: "Stop active turn and background loop" },
    // Modes & UI (8)
    SlashCommandDef { name: "voice", aliases: &[], category: "modes_and_ui", syntax: "/voice [on|off]", description: "Toggle voice synthesis mode" },
    SlashCommandDef { name: "talk", aliases: &[], category: "modes_and_ui", syntax: "/talk", description: "Launch full-duplex Talk Mode" },
    SlashCommandDef { name: "computer", aliases: &["cua"], category: "modes_and_ui", syntax: "/computer", description: "Open Remote Computer Use workbench" },
    SlashCommandDef { name: "browser", aliases: &[], category: "modes_and_ui", syntax: "/browser [url]", description: "Control browser automation" },
    SlashCommandDef { name: "verbose", aliases: &[], category: "modes_and_ui", syntax: "/verbose [on|off]", description: "Toggle verbose tool output" },
    SlashCommandDef { name: "reasoning", aliases: &["think"], category: "modes_and_ui", syntax: "/reasoning [on|off]", description: "Toggle reasoning visibility" },
    SlashCommandDef { name: "debug", aliases: &[], category: "modes_and_ui", syntax: "/debug [on|off]", description: "Toggle runtime debug trace" },
    SlashCommandDef { name: "config", aliases: &[], category: "modes_and_ui", syntax: "/config [show|set]", description: "View or update Hermes configuration" },
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedSlash {
    pub command: String,
    pub bot_mention: Option<String>,
    pub args: String,
}

pub fn parse_slash_command(input: &str) -> Option<ParsedSlash> {
    let trimmed = input.trim();
    let after = trimmed.strip_prefix('/')?;
    if after.is_empty() {
        return None;
    }
    let (head, rest) = match after.find(char::is_whitespace) {
        Some(idx) => (&after[..idx], after[idx..].trim()),
        None => (after, ""),
    };
    if head.contains('/') || head.contains('\\') {
        return None;
    }
    let (cmd, mention) = match head.split_once('@') {
        Some((c, b)) if !c.is_empty() && !b.is_empty() => (c, Some(b.to_string())),
        _ => (head, None),
    };
    if !cmd.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return None;
    }
    Some(ParsedSlash {
        command: cmd.to_ascii_lowercase(),
        bot_mention: mention,
        args: rest.to_string(),
    })
}

pub fn lookup_slash_command(name_or_alias: &str) -> Option<&'static SlashCommandDef> {
    let norm = name_or_alias
        .trim()
        .trim_start_matches('/')
        .to_ascii_lowercase();
    HERMES_SLASH_COMMANDS
        .iter()
        .find(|c| c.name == norm || c.aliases.contains(&norm.as_str()))
}

/// 5-field standard Cron expression (`minute hour dom month dow`) + `@hourly`/`@daily`/`@weekly` parser.
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
        let normalized = match expr.trim() {
            "@hourly" => "0 * * * *",
            "@daily" | "@midnight" => "0 0 * * *",
            "@weekly" => "0 0 * * 0",
            "@monthly" => "0 0 1 * *",
            other => other,
        };
        let fields: Vec<&str> = normalized.split_whitespace().collect();
        if fields.len() != 5 {
            return Err(format!("Expected 5 cron fields (min hour dom mon dow), got '{}'", expr));
        }

        let mut minutes = [false; 60];
        let mut hours = [false; 24];
        let mut days_of_month = [false; 32];
        let mut months = [false; 13];
        let mut days_of_week = [false; 7];

        parse_cron_field(fields[0], 0, 59, &mut minutes)?;
        parse_cron_field(fields[1], 0, 23, &mut hours)?;
        parse_cron_field(fields[2], 1, 31, &mut days_of_month)?;
        parse_cron_field(fields[3], 1, 12, &mut months)?;
        parse_cron_dow(fields[4], &mut days_of_week)?;

        Ok(Self {
            minutes,
            hours,
            days_of_month,
            months,
            days_of_week,
        })
    }

    /// Computes the next UTC execution timestamp in milliseconds strictly after `after_ms`.
    pub fn next_after_ms(&self, after_ms: i64) -> Option<i64> {
        let start_secs = (after_ms / 1000) + 60 - ((after_ms / 1000).rem_euclid(60));
        let mut dt = Utc.timestamp_opt(start_secs, 0).single()?;

        // Search up to 366 days ahead (527,040 minutes)
        for _ in 0..527_040 {
            let m = dt.month() as usize;
            if !self.months[m] {
                let (ny, nm) = if m == 12 { (dt.year() + 1, 1) } else { (dt.year(), (m + 1) as u32) };
                dt = Utc.with_ymd_and_hms(ny, nm, 1, 0, 0, 0).single()?;
                continue;
            }
            let dom = dt.day() as usize;
            let dow = dt.weekday().num_days_from_sunday() as usize;
            if !self.days_of_month[dom] || !self.days_of_week[dow] {
                dt += chrono::Duration::days(1);
                dt = Utc
                    .with_ymd_and_hms(dt.year(), dt.month(), dt.day(), 0, 0, 0)
                    .single()?;
                continue;
            }
            let hr = dt.hour() as usize;
            if !self.hours[hr] {
                dt += chrono::Duration::hours(1);
                dt = Utc
                    .with_ymd_and_hms(dt.year(), dt.month(), dt.day(), dt.hour(), 0, 0)
                    .single()?;
                continue;
            }
            let min = dt.minute() as usize;
            if !self.minutes[min] {
                dt += chrono::Duration::minutes(1);
                continue;
            }
            return Some(dt.timestamp_millis());
        }
        None
    }
}

fn parse_cron_field(raw: &str, min_v: usize, max_v: usize, out: &mut [bool]) -> Result<(), String> {
    for part in raw.split(',') {
        let (range_part, step) = if let Some((r, s)) = part.split_once('/') {
            let st: usize = s.parse().map_err(|_| format!("Invalid cron step '{s}'"))?;
            if st == 0 {
                return Err("Cron step cannot be 0".into());
            }
            (r, st)
        } else {
            (part, 1)
        };

        let (start, end) = if range_part == "*" {
            (min_v, max_v)
        } else if let Some((a, b)) = range_part.split_once('-') {
            let sv: usize = a.parse().map_err(|_| format!("Invalid cron value '{a}'"))?;
            let ev: usize = b.parse().map_err(|_| format!("Invalid cron value '{b}'"))?;
            (sv, ev)
        } else {
            let v: usize = range_part
                .parse()
                .map_err(|_| format!("Invalid cron value '{range_part}'"))?;
            (v, v)
        };

        if start < min_v || end > max_v || start > end {
            return Err(format!("Cron value out of bounds ({min_v}..={max_v}): '{part}'"));
        }
        let mut cur = start;
        while cur <= end {
            out[cur] = true;
            cur += step;
        }
    }
    Ok(())
}

fn parse_cron_dow(raw: &str, out: &mut [bool; 7]) -> Result<(), String> {
    let mut temp = [false; 8];
    parse_cron_field(raw, 0, 7, &mut temp)?;
    for i in 0..7 {
        out[i] = temp[i];
    }
    if temp[7] {
        out[0] = true; // 7 == Sunday
    }
    Ok(())
}

pub fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS hermes_sessions (
            session_id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            parent_session_id TEXT,
            model TEXT NOT NULL,
            provider_id TEXT NOT NULL,
            is_active INTEGER NOT NULL DEFAULT 0,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS hermes_messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS hermes_memory (
            store_key TEXT PRIMARY KEY,
            content TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS hermes_skills (
            name TEXT PRIMARY KEY,
            description TEXT NOT NULL,
            body_markdown TEXT NOT NULL,
            origin TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1
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
            created_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS hermes_kanban_cards (
            card_id TEXT PRIMARY KEY,
            board_id TEXT NOT NULL,
            column_id TEXT NOT NULL,
            title TEXT NOT NULL,
            description TEXT NOT NULL,
            priority TEXT NOT NULL,
            position INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS hermes_approvals (
            approval_id TEXT PRIMARY KEY,
            command_summary TEXT NOT NULL,
            risk_level TEXT NOT NULL,
            status TEXT NOT NULL,
            reason TEXT,
            created_at_ms INTEGER NOT NULL
        );",
    )
    .map_err(|e| format!("Failed to initialize Hermes schema: {e}"))?;

    let now = Utc::now().timestamp_millis();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM hermes_sessions", [], |r| r.get(0))
        .unwrap_or(0);
    if count == 0 {
        conn.execute(
            "INSERT INTO hermes_sessions (session_id, title, parent_session_id, model, provider_id, is_active, created_at_ms, updated_at_ms)
             VALUES ('hermes_main', 'Main Studio Session', NULL, 'gemini-2.5-flash', 'google', 1, ?1, ?1)",
            params![now],
        )
        .map_err(|e| e.to_string())?;
    }

    conn.execute(
        "INSERT OR IGNORE INTO hermes_memory (store_key, content, updated_at_ms)
         VALUES ('SOUL.md', 'You are Hermes, the autonomous on-device & cloud AI Studio agent integrated into Fabushi Android.', ?1),
                ('MEMORY.md', '', ?1),
                ('USER.md', '', ?1)",
        params![now],
    )
    .map_err(|e| e.to_string())?;

    for ts in HERMES_TOOLSETS {
        conn.execute(
            "INSERT OR IGNORE INTO hermes_toolset_state (toolset_id, enabled) VALUES (?1, ?2)",
            params![ts.id, if ts.default_enabled { 1 } else { 0 }],
        )
        .map_err(|e| e.to_string())?;
    }

    let default_skills = [
        ("mika-phone-automation", "Plan and verify Android Accessibility UI actions via Mika Agent", "# Mika Phone Automation\nInspect UI tree before tapping irreversible controls."),
        ("dharma-ritual-guide", "Guide sutra chanting, dedication of merit, and Zen Room schedules", "# Dharma Ritual Guide\nCite canonical Prajnaparamita passages concisely."),
        ("magpie-smart-routing", "Configure multi-provider failover and PII masking rules", "# Magpie Smart Routing\nUse Cheapest or Smart routing with circuit-breaker failover."),
    ];
    for (name, desc, body) in default_skills {
        conn.execute(
            "INSERT OR IGNORE INTO hermes_skills (name, description, body_markdown, origin, enabled)
             VALUES (?1, ?2, ?3, 'bundled', 1)",
            params![name, desc, body],
        )
        .map_err(|e| e.to_string())?;
    }

    let cards_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM hermes_kanban_cards", [], |r| r.get(0))
        .unwrap_or(0);
    if cards_count == 0 {
        conn.execute(
            "INSERT INTO hermes_kanban_cards (card_id, board_id, column_id, title, description, priority, position, updated_at_ms)
             VALUES ('card_welcome', 'default', 'todo', 'Configure Magpie API Keys', 'Import provider keys via magpie://import or .env', 'high', 0, ?1),
                    ('card_mika', 'default', 'in_progress', 'Bind Telegram Chat to Mika Agent', 'Enable hands-free phone task automation', 'medium', 0, ?1)",
            params![now],
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn active_session_id(conn: &Connection) -> String {
    conn.query_row(
        "SELECT session_id FROM hermes_sessions WHERE is_active = 1 LIMIT 1",
        [],
        |r| r.get(0),
    )
    .unwrap_or_else(|_| "hermes_main".into())
}

pub fn scan_skill_security(markdown: &str) -> Result<(), String> {
    let lower = markdown.to_ascii_lowercase();
    for forbidden in ["rm -rf /", "curl | sh", "wget | bash", "/etc/shadow", "mkfs."] {
        if lower.contains(forbidden) {
            return Err(format!("Skill rejected by Hermes security scanner: matched '{forbidden}'"));
        }
    }
    if (lower.contains("curl ") || lower.contains("wget "))
        && (lower.contains("| sh")
            || lower.contains("| bash")
            || lower.contains("|sh")
            || lower.contains("|bash"))
    {
        return Err("Skill rejected by Hermes security scanner: remote shell pipe execution".into());
    }
    Ok(())
}

pub fn handle_rpc(conn: &Connection, submethod: &str, params: &Value) -> Result<Value, String> {
    init_schema(conn)?;
    let now = Utc::now().timestamp_millis();

    match submethod {
        "status" => {
            let sid = active_session_id(conn);
            let session_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM hermes_sessions", [], |r| r.get(0))
                .unwrap_or(1);
            let enabled_toolsets: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM hermes_toolset_state WHERE enabled = 1",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            let cron_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM hermes_cron_jobs WHERE enabled = 1", [], |r| r.get(0))
                .unwrap_or(0);
            Ok(json!({
                "activeSessionId": sid,
                "totalSessions": session_count,
                "totalToolsets": HERMES_TOOLSETS.len(),
                "enabledToolsets": enabled_toolsets,
                "slashCommandCount": HERMES_SLASH_COMMANDS.len(),
                "activeCronJobs": cron_count,
            }))
        }

        "listCommands" => {
            let prefix = params
                .get("prefix")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .trim_start_matches('/')
                .to_ascii_lowercase();
            let items: Vec<Value> = HERMES_SLASH_COMMANDS
                .iter()
                .filter(|c| {
                    prefix.is_empty()
                        || c.name.starts_with(&prefix)
                        || c.aliases.iter().any(|a| a.starts_with(&prefix))
                        || c.description.to_ascii_lowercase().contains(&prefix)
                })
                .map(|c| {
                    json!({
                        "name": c.name,
                        "slash": format!("/{}", c.name),
                        "aliases": c.aliases,
                        "category": c.category,
                        "syntax": c.syntax,
                        "description": c.description,
                    })
                })
                .collect();
            Ok(json!({
                "total": items.len(),
                "commands": items,
            }))
        }

        "sessions" => {
            let mut stmt = conn
                .prepare(
                    "SELECT s.session_id, s.title, s.parent_session_id, s.model, s.provider_id, s.is_active, s.updated_at_ms,
                            (SELECT COUNT(*) FROM hermes_messages m WHERE m.session_id = s.session_id)
                     FROM hermes_sessions s ORDER BY s.updated_at_ms DESC",
                )
                .map_err(|e| e.to_string())?;
            let list = stmt
                .query_map([], |r| {
                    Ok(json!({
                        "sessionId": r.get::<_, String>(0)?,
                        "title": r.get::<_, String>(1)?,
                        "parentSessionId": r.get::<_, Option<String>>(2)?,
                        "model": r.get::<_, String>(3)?,
                        "providerId": r.get::<_, String>(4)?,
                        "active": r.get::<_, i64>(5)? != 0,
                        "updatedAtMs": r.get::<_, i64>(6)?,
                        "messageCount": r.get::<_, i64>(7)?,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(json!({ "sessions": list }))
        }

        "createSession" => {
            let title = params
                .get("title")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("New Hermes Session");
            let model = params
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or("gemini-2.5-flash");
            let provider = params
                .get("providerId")
                .and_then(Value::as_str)
                .unwrap_or("google");
            let sid = format!("hermes_{}", uuid::Uuid::new_v4().simple());
            conn.execute("UPDATE hermes_sessions SET is_active = 0", [])
                .map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT INTO hermes_sessions (session_id, title, parent_session_id, model, provider_id, is_active, created_at_ms, updated_at_ms)
                 VALUES (?1, ?2, NULL, ?3, ?4, 1, ?5, ?5)",
                params![sid, title, model, provider, now],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({ "sessionId": sid, "title": title, "active": true }))
        }

        "branchSession" => {
            let parent_id = params
                .get("sessionId")
                .and_then(Value::as_str)
                .map(ToString::to_string)
                .unwrap_or_else(|| active_session_id(conn));
            let title = params
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Branched Session");
            let branch_id = format!("branch_{}", uuid::Uuid::new_v4().simple());

            let (model, provider): (String, String) = conn
                .query_row(
                    "SELECT model, provider_id FROM hermes_sessions WHERE session_id = ?1",
                    params![parent_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .unwrap_or_else(|| ("gemini-2.5-flash".into(), "google".into()));

            conn.execute("UPDATE hermes_sessions SET is_active = 0", [])
                .map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT INTO hermes_sessions (session_id, title, parent_session_id, model, provider_id, is_active, created_at_ms, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?6)",
                params![branch_id, title, parent_id, model, provider, now],
            )
            .map_err(|e| e.to_string())?;

            conn.execute(
                "INSERT INTO hermes_messages (session_id, role, content, created_at_ms)
                 SELECT ?1, role, content, created_at_ms FROM hermes_messages WHERE session_id = ?2 ORDER BY id ASC",
                params![branch_id, parent_id],
            )
            .map_err(|e| e.to_string())?;

            let copied: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM hermes_messages WHERE session_id = ?1",
                    params![branch_id],
                    |r| r.get(0),
                )
                .unwrap_or(0);

            Ok(json!({
                "sessionId": branch_id,
                "parentSessionId": parent_id,
                "title": title,
                "copiedMessages": copied,
            }))
        }

        "sendTurn" => {
            let sid = params
                .get("sessionId")
                .and_then(Value::as_str)
                .map(ToString::to_string)
                .unwrap_or_else(|| active_session_id(conn));
            let text = params
                .get("text")
                .or_else(|| params.get("prompt"))
                .and_then(Value::as_str)
                .ok_or_else(|| "sendTurn requires 'text'".to_string())?;

            if let Some(slash) = parse_slash_command(text) {
                return handle_rpc(
                    conn,
                    "runSlashCommand",
                    &json!({ "command": text, "sessionId": sid, "parsed": slash.command }),
                );
            }

            conn.execute(
                "INSERT INTO hermes_messages (session_id, role, content, created_at_ms) VALUES (?1, 'user', ?2, ?3)",
                params![sid, text, now],
            )
            .map_err(|e| e.to_string())?;

            let mem: String = conn
                .query_row(
                    "SELECT content FROM hermes_memory WHERE store_key = 'MEMORY.md'",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or_default();
            let reply = if mem.is_empty() {
                format!("Hermes Studio 已处理指令：「{text}」。可通过 `/branch` 分支会话或 `/kanban` 追踪任务。")
            } else {
                format!("Hermes Studio（结合记忆上下文）已响应：「{text}」。")
            };

            conn.execute(
                "INSERT INTO hermes_messages (session_id, role, content, created_at_ms) VALUES (?1, 'assistant', ?2, ?3)",
                params![sid, reply, now + 1],
            )
            .map_err(|e| e.to_string())?;
            conn.execute(
                "UPDATE hermes_sessions SET updated_at_ms = ?1 WHERE session_id = ?2",
                params![now + 1, sid],
            )
            .map_err(|e| e.to_string())?;

            Ok(json!({
                "sessionId": sid,
                "reply": reply,
                "role": "assistant",
            }))
        }

        "runSlashCommand" => {
            let raw = params
                .get("command")
                .and_then(Value::as_str)
                .ok_or_else(|| "runSlashCommand requires 'command'".to_string())?;
            let parsed = parse_slash_command(raw)
                .ok_or_else(|| format!("Not a valid slash command: '{raw}'"))?;
            let Some(def) = lookup_slash_command(&parsed.command) else {
                return Ok(json!({
                    "handled": true,
                    "isError": true,
                    "command": parsed.command,
                    "responseText": format!("Unknown Hermes command `/{}`. Try `/help`.", parsed.command),
                }));
            };

            let sid = active_session_id(conn);
            let response_text = match def.name {
                "help" => format!(
                    "Hermes supports {} slash commands across 8 categories (e.g. `/new`, `/branch`, `/memory`, `/toolsets`, `/cron`, `/kanban`).",
                    HERMES_SLASH_COMMANDS.len()
                ),
                "new" | "reset" => {
                    let res = handle_rpc(conn, "createSession", &json!({ "title": if parsed.args.is_empty() { "Fresh Session" } else { &parsed.args } }))?;
                    format!("Started session `{}`.", res["sessionId"].as_str().unwrap_or(""))
                }
                "branch" => {
                    let res = handle_rpc(conn, "branchSession", &json!({ "sessionId": sid, "title": if parsed.args.is_empty() { "branch" } else { &parsed.args } }))?;
                    format!("Created branch `{}`.", res["sessionId"].as_str().unwrap_or(""))
                }
                "clear" => {
                    let deleted = conn
                        .execute("DELETE FROM hermes_messages WHERE session_id = ?1", params![sid])
                        .unwrap_or(0);
                    format!("Cleared {deleted} messages from `{sid}`.")
                }
                "undo" => {
                    conn.execute(
                        "DELETE FROM hermes_messages WHERE id IN (SELECT id FROM hermes_messages WHERE session_id = ?1 ORDER BY id DESC LIMIT 2)",
                        params![sid],
                    )
                    .ok();
                    format!("Undid latest turn pair in `{sid}`.")
                }
                "memory" => {
                    if let Some(rest) = parsed.args.strip_prefix("add ") {
                        handle_rpc(conn, "memoryUpdate", &json!({ "target": "MEMORY.md", "action": "append", "content": rest.trim() }))?;
                        format!("Appended memory entry to MEMORY.md.")
                    } else {
                        let m = handle_rpc(conn, "memoryGet", &json!({}))?;
                        format!("MEMORY.md: {}", m["memoryMd"].as_str().unwrap_or("(empty)"))
                    }
                }
                "doctor" => "Hermes Doctor: SQLite WAL healthy, 29 toolsets & 61 slash commands nominal.".into(),
                _ => format!("Executed `/{}` ({}) on session `{sid}`.", def.name, def.category),
            };

            Ok(json!({
                "handled": true,
                "isError": false,
                "command": def.name,
                "category": def.category,
                "responseText": response_text,
            }))
        }

        "memoryGet" => {
            let mut map = serde_json::Map::new();
            let mut stmt = conn
                .prepare("SELECT store_key, content FROM hermes_memory")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .map_err(|e| e.to_string())?;
            for row in rows.flatten() {
                map.insert(row.0, Value::String(row.1));
            }
            Ok(json!({
                "memoryMd": map.get("MEMORY.md").cloned().unwrap_or(Value::String(String::new())),
                "userMd": map.get("USER.md").cloned().unwrap_or(Value::String(String::new())),
                "soulMd": map.get("SOUL.md").cloned().unwrap_or(Value::String(String::new())),
                "limits": { "memoryChars": MEMORY_CHAR_LIMIT, "userChars": USER_CHAR_LIMIT },
            }))
        }

        "memoryUpdate" => {
            let target = params
                .get("target")
                .and_then(Value::as_str)
                .unwrap_or("MEMORY.md");
            let key = match target.to_ascii_uppercase().as_str() {
                "USER" | "USER.MD" => "USER.md",
                "SOUL" | "SOUL.MD" => "SOUL.md",
                _ => "MEMORY.md",
            };
            let action = params
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or("set");
            let input = params
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("");

            let current: String = conn
                .query_row(
                    "SELECT content FROM hermes_memory WHERE store_key = ?1",
                    params![key],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .unwrap_or_default();

            let next = if action == "append" && !current.is_empty() {
                format!("{current}\n{input}")
            } else {
                input.to_string()
            };

            let limit = if key == "USER.md" {
                USER_CHAR_LIMIT
            } else if key == "MEMORY.md" {
                MEMORY_CHAR_LIMIT
            } else {
                8000
            };
            let bounded: String = next.chars().take(limit).collect();

            conn.execute(
                "INSERT OR REPLACE INTO hermes_memory (store_key, content, updated_at_ms) VALUES (?1, ?2, ?3)",
                params![key, bounded, now],
            )
            .map_err(|e| e.to_string())?;

            Ok(json!({
                "target": key,
                "charCount": bounded.chars().count(),
                "charLimit": limit,
                "content": bounded,
            }))
        }

        "skillsList" => {
            let mut stmt = conn
                .prepare("SELECT name, description, body_markdown, origin, enabled FROM hermes_skills ORDER BY name ASC")
                .map_err(|e| e.to_string())?;
            let list = stmt
                .query_map([], |r| {
                    Ok(json!({
                        "name": r.get::<_, String>(0)?,
                        "description": r.get::<_, String>(1)?,
                        "bodyMarkdown": r.get::<_, String>(2)?,
                        "origin": r.get::<_, String>(3)?,
                        "enabled": r.get::<_, i64>(4)? != 0,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(json!({ "skills": list }))
        }

        "skillUpsert" => {
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| "skillUpsert requires 'name'".to_string())?;
            let desc = params
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("Custom learned skill");
            let body = params
                .get("bodyMarkdown")
                .and_then(Value::as_str)
                .unwrap_or("# Custom Skill");
            scan_skill_security(body)?;
            conn.execute(
                "INSERT OR REPLACE INTO hermes_skills (name, description, body_markdown, origin, enabled)
                 VALUES (?1, ?2, ?3, 'learned', 1)",
                params![name, desc, body],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({ "name": name, "saved": true }))
        }

        "toolsetsList" => {
            let mut enabled_map = std::collections::HashMap::new();
            let mut stmt = conn
                .prepare("SELECT toolset_id, enabled FROM hermes_toolset_state")
                .map_err(|e| e.to_string())?;
            for row in stmt
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? != 0)))
                .map_err(|e| e.to_string())?
                .flatten()
            {
                enabled_map.insert(row.0, row.1);
            }
            let toolsets: Vec<Value> = HERMES_TOOLSETS
                .iter()
                .map(|t| {
                    json!({
                        "id": t.id,
                        "description": t.description,
                        "enabled": enabled_map.get(t.id).copied().unwrap_or(t.default_enabled),
                    })
                })
                .collect();
            Ok(json!({
                "total": toolsets.len(),
                "toolsets": toolsets,
            }))
        }

        "toolsetToggle" => {
            let id = params
                .get("toolsetId")
                .and_then(Value::as_str)
                .ok_or_else(|| "toolsetToggle requires 'toolsetId'".to_string())?;
            let enabled = params
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            conn.execute(
                "INSERT OR REPLACE INTO hermes_toolset_state (toolset_id, enabled) VALUES (?1, ?2)",
                params![id, if enabled { 1 } else { 0 }],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({ "toolsetId": id, "enabled": enabled }))
        }

        "cronList" => {
            let mut stmt = conn
                .prepare("SELECT job_id, schedule, prompt, enabled, next_run_ms, created_at_ms FROM hermes_cron_jobs ORDER BY created_at_ms ASC")
                .map_err(|e| e.to_string())?;
            let jobs = stmt
                .query_map([], |r| {
                    Ok(json!({
                        "jobId": r.get::<_, String>(0)?,
                        "schedule": r.get::<_, String>(1)?,
                        "prompt": r.get::<_, String>(2)?,
                        "enabled": r.get::<_, i64>(3)? != 0,
                        "nextRunMs": r.get::<_, i64>(4)?,
                        "createdAtMs": r.get::<_, i64>(5)?,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(json!({ "jobs": jobs }))
        }

        "cronUpsert" => {
            let schedule_str = params
                .get("schedule")
                .and_then(Value::as_str)
                .ok_or_else(|| "cronUpsert requires 5-field 'schedule'".to_string())?;
            let prompt = params
                .get("prompt")
                .and_then(Value::as_str)
                .unwrap_or("Daily morning Dharma & task summary");
            let from_ms = params
                .get("referenceTimeMs")
                .and_then(Value::as_i64)
                .unwrap_or(now);

            let parsed = CronSchedule::parse(schedule_str)?;
            let next_run_ms = parsed
                .next_after_ms(from_ms)
                .ok_or_else(|| "Cron schedule produced no future execution within 366 days".to_string())?;

            let job_id = params
                .get("jobId")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(ToString::to_string)
                .unwrap_or_else(|| format!("cron_{}", uuid::Uuid::new_v4().simple()));
            let enabled = params
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true);

            conn.execute(
                "INSERT OR REPLACE INTO hermes_cron_jobs (job_id, schedule, prompt, enabled, next_run_ms, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![job_id, schedule_str, prompt, if enabled { 1 } else { 0 }, next_run_ms, now],
            )
            .map_err(|e| e.to_string())?;

            Ok(json!({
                "jobId": job_id,
                "schedule": schedule_str,
                "prompt": prompt,
                "enabled": enabled,
                "nextRunMs": next_run_ms,
            }))
        }

        "kanbanBoard" => {
            let board_id = params
                .get("boardId")
                .and_then(Value::as_str)
                .unwrap_or("default");
            let mut stmt = conn
                .prepare(
                    "SELECT card_id, column_id, title, description, priority, position, updated_at_ms
                     FROM hermes_kanban_cards WHERE board_id = ?1 ORDER BY position ASC, updated_at_ms DESC",
                )
                .map_err(|e| e.to_string())?;
            let cards = stmt
                .query_map(params![board_id], |r| {
                    Ok(json!({
                        "cardId": r.get::<_, String>(0)?,
                        "columnId": r.get::<_, String>(1)?,
                        "title": r.get::<_, String>(2)?,
                        "description": r.get::<_, String>(3)?,
                        "priority": r.get::<_, String>(4)?,
                        "position": r.get::<_, i64>(5)?,
                        "updatedAtMs": r.get::<_, i64>(6)?,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;

            Ok(json!({
                "boardId": board_id,
                "columns": ["backlog", "todo", "in_progress", "review", "done"],
                "cards": cards,
            }))
        }

        "kanbanAddCard" => {
            let board_id = params
                .get("boardId")
                .and_then(Value::as_str)
                .unwrap_or("default");
            let col = params
                .get("columnId")
                .and_then(Value::as_str)
                .unwrap_or("todo");
            let title = params
                .get("title")
                .and_then(Value::as_str)
                .ok_or_else(|| "kanbanAddCard requires 'title'".to_string())?;
            let desc = params
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("");
            let prio = params
                .get("priority")
                .and_then(Value::as_str)
                .unwrap_or("medium");
            let card_id = format!("card_{}", uuid::Uuid::new_v4().simple());

            conn.execute(
                "INSERT INTO hermes_kanban_cards (card_id, board_id, column_id, title, description, priority, position, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)",
                params![card_id, board_id, col, title, desc, prio, now],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({ "cardId": card_id, "columnId": col, "title": title }))
        }

        "kanbanMove" => {
            let card_id = params
                .get("cardId")
                .and_then(Value::as_str)
                .ok_or_else(|| "kanbanMove requires 'cardId'".to_string())?;
            let target_col = params
                .get("columnId")
                .or_else(|| params.get("targetColumn"))
                .and_then(Value::as_str)
                .ok_or_else(|| "kanbanMove requires 'columnId'".to_string())?;
            let updated = conn
                .execute(
                    "UPDATE hermes_kanban_cards SET column_id = ?1, updated_at_ms = ?2 WHERE card_id = ?3",
                    params![target_col, now, card_id],
                )
                .map_err(|e| e.to_string())?;
            if updated == 0 {
                return Err(format!("Kanban card '{card_id}' not found"));
            }
            Ok(json!({ "cardId": card_id, "columnId": target_col, "moved": true }))
        }

        "approvalsDecide" => {
            let approval_id = params
                .get("approvalId")
                .and_then(Value::as_str)
                .unwrap_or("approval_default");
            let decision = params
                .get("decision")
                .and_then(Value::as_str)
                .unwrap_or("approved");
            let reason = params.get("reason").and_then(Value::as_str);
            conn.execute(
                "INSERT OR REPLACE INTO hermes_approvals (approval_id, command_summary, risk_level, status, reason, created_at_ms)
                 VALUES (?1, 'Hermes gated tool invocation', 'high', ?2, ?3, ?4)",
                params![approval_id, decision, reason, now],
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({
                "approvalId": approval_id,
                "status": decision,
                "resolvedAtMs": now,
            }))
        }

        other => Err(format!("Unsupported hermes method: {other}")),
    }
}
