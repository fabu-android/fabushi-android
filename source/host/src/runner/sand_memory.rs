use std::collections::HashSet;
use std::io;

use crate::extensions::memory::memory_service::{
    FileMemoryStore, MemoryKind, MemoryRecall, MemoryRecord, ProjectMemoryPromptRecall,
    ScopedMemoryRecord, UserMemoryRecall, format_memory_date, memory_dedupe_key,
    normalize_memory_content,
};

pub const MEMORY_RECENT_PROMPT_LIMIT: usize = 30;
pub const MEMORY_RECENT_PROMPT_CHAR_BUDGET: usize = 4_000;
pub const MEMORY_USER_PROFILE_PROMPT_LIMIT: usize = 50;
pub const MEMORY_USER_RECENT_PROMPT_LIMIT: usize = 15;
pub const MEMORY_USER_PROFILE_CHAR_BUDGET: usize = 4_000;
pub const MEMORY_USER_RECENT_CHAR_BUDGET: usize = 2_000;
pub const MEMORY_PROJECT_PROFILE_PROMPT_LIMIT: usize = 25;
pub const MEMORY_PROJECT_RECENT_PROMPT_LIMIT: usize = 10;
pub const MEMORY_PROJECT_INJECTED_CAP: usize = 3;
pub const MEMORY_PROJECT_PROFILE_CHAR_BUDGET: usize = 2_500;
pub const MEMORY_PROJECT_RECENT_CHAR_BUDGET: usize = 1_500;
pub const MEMORY_MAX_CONTENT_LENGTH: usize = 500;
pub const MEMORY_USER_SYSTEM_PROMPT_HEADER: &str =
    "User memory: durable facts shared across every assistant this user runs";
pub const MEMORY_PROJECT_SYSTEM_PROMPT_HEADER: &str =
    "Project memory: durable facts shared by every assistant that has joined a project";
pub const MEMORY_EXTRACTION_PROMPT_MARKER: &str = "<<SAND_MEMORY_EXTRACTION>>";
pub const MEMORY_EPISODE_PROMPT_MARKER: &str = "<<SAND_MEMORY_EPISODE>>";
pub const MEMORY_EPISODE_PREFIX: &str = "[episode] ";
pub const MEMORY_NOTE_PREFIX: &str = "[note] ";
pub const MEMORY_EXTRACTION_NONE_SENTINEL: &str = "NONE";
pub const MEMORY_EXTRACTION_ARCHIVE_SCAN_LIMIT: usize = 500;
pub const DEFAULT_EPISODE_INTERVAL: usize = 6;
pub const MEMORY_DECAY_HALF_LIFE_DAYS: f64 = 30.0;
pub const DAY_MS: f64 = 86_400_000.0;
pub const MEMORY_SYSTEM_PROMPT_HEADER: &str =
    "Memory: durable facts you have learned about the user and their world.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenMemorySnapshot {
    pub render: String,
    pub compaction_epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenMemoryPrompt {
    pub render: String,
    pub snapshot_to_persist: Option<FrozenMemorySnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryAddition {
    pub content: String,
    pub kind: MemoryKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MemoryExtraction {
    pub additions: Vec<MemoryAddition>,
    pub removals: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppliedMemoryExtraction {
    pub added: Vec<MemoryRecord>,
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpisodeTurn {
    pub ts: i64,
    pub user: String,
    pub agent: String,
}

pub fn is_memory_freeze_enabled(disable_memory_freeze: Option<&str>) -> bool {
    disable_memory_freeze != Some("1")
}

pub fn resolve_frozen_memory_prompt(
    snapshot: Option<&FrozenMemorySnapshot>,
    compaction_epoch: u64,
    live_render: impl FnOnce() -> (String, bool),
) -> FrozenMemoryPrompt {
    if let Some(snapshot) = snapshot.filter(|value| value.compaction_epoch == compaction_epoch) {
        return FrozenMemoryPrompt {
            render: snapshot.render.clone(),
            snapshot_to_persist: None,
        };
    }
    let (render, has_facts) = live_render();
    FrozenMemoryPrompt {
        snapshot_to_persist: has_facts.then(|| FrozenMemorySnapshot {
            render: render.clone(),
            compaction_epoch,
        }),
        render,
    }
}

pub fn episode_interval(raw: Option<&str>) -> usize {
    raw.and_then(|raw| raw.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_EPISODE_INTERVAL)
}

pub fn is_memorable_exchange(user_message: &str) -> bool {
    let user = user_message.trim();
    if user.is_empty() {
        return false;
    }
    if user.chars().count() > 40 || user.contains('?') {
        return true;
    }
    let normalized = user
        .trim_end_matches(|ch: char| ch.is_whitespace() || matches!(ch, '!' | '.' | '…' | ',' | '~' | ')' | ']'))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    !matches!(
        normalized.as_str(),
        "hi" | "hey" | "hello" | "yo" | "sup" | "thanks" | "thank you" | "ty" | "thx"
            | "ok" | "okay" | "k" | "kk" | "cool" | "nice" | "great" | "awesome"
            | "perfect" | "yes" | "yep" | "yeah" | "no" | "nope" | "sure" | "got it"
            | "gotcha" | "lol" | "haha" | "np" | "done" | "good" | "bye"
    )
}

pub fn memory_importance(content: &str) -> f64 {
    if content.starts_with(MEMORY_EPISODE_PREFIX) {
        1.5
    } else if content.starts_with(MEMORY_NOTE_PREFIX) {
        0.5
    } else {
        1.0
    }
}

pub fn memory_recall_rank(memory: &MemoryRecord) -> f64 {
    memory_importance(&memory.content).log2()
        + memory.created_at as f64 / (MEMORY_DECAY_HALF_LIFE_DAYS * DAY_MS)
}

pub fn fact_line(memory: &MemoryRecord) -> String {
    format!(
        "- (learned {}) {}",
        format_memory_date(memory.created_at),
        memory.content
    )
}

pub fn render_memory_system_prompt(recall: &MemoryRecall, location: Option<&str>) -> String {
    if recall.profile.is_empty() && recall.recent.is_empty() && location.is_none() {
        return String::new();
    }

    let mut lines = vec![
        MEMORY_SYSTEM_PROMPT_HEADER.to_string(),
        "These persist across every conversation with this agent, even after the chat is cleared. Rely on them so you stay consistent and avoid re-asking what you already know.".to_string(),
    ];
    if let Some(location) = location {
        lines.push(format!(
            "Your memory lives in a folder at {location}: profile.md holds who the user is (kept in mind every turn) and log/ holds dated history."
        ));
        lines.push(
            "Read or grep those files with Read and Shell on your own computer when you need older facts that are not listed here. To CHANGE memory, prefer the update_state tool (target \"memory\"): action \"write\" with a fact and a tier (profile | log | note), or action \"forget\" with the exact text of a recorded fact."
                .to_string(),
        );
    }
    if !recall.profile.is_empty() {
        lines.push("About the user:".into());
        lines.extend(recall.profile.iter().map(fact_line));
    }
    if !recall.recent.is_empty() {
        lines.push("Recently:".into());
        let mut budget = MEMORY_RECENT_PROMPT_CHAR_BUDGET;
        let mut shown = 0usize;
        for memory in &recall.recent {
            let line = fact_line(memory);
            if shown > 0 && line.len() > budget {
                break;
            }
            budget = budget.saturating_sub(line.len());
            shown += 1;
            lines.push(line);
        }
        let omitted = recall.recent.len().saturating_sub(shown);
        if omitted > 0 {
            lines.push(match location {
                Some(_) => format!(
                    "({omitted} more log facts on disk — grep the log/ folder for them.)"
                ),
                None => format!("({omitted} more log facts not shown.)"),
            });
        }
    }
    if recall.profile.is_empty() && recall.recent.is_empty() {
        lines.push("No facts recorded yet.".into());
    }
    lines.join("\n")
}

fn provenanced_line(record: &ScopedMemoryRecord) -> String {
    let via = record.agent_name.trim();
    format!(
        "- (learned {}){} {}",
        format_memory_date(record.memory.created_at),
        if via.is_empty() { String::new() } else { format!(" [via {via}]") },
        record.memory.content
    )
}

fn append_budgeted_provenanced_facts(
    lines: &mut Vec<String>,
    records: &[ScopedMemoryRecord],
    char_budget: usize,
    more_label: &str,
    grep_hint: &str,
) {
    let mut budget = char_budget;
    let mut shown = 0usize;
    for record in records {
        let line = provenanced_line(record);
        if shown > 0 && line.len() > budget { break; }
        budget = budget.saturating_sub(line.len());
        shown += 1;
        lines.push(line);
    }
    let omitted = records.len().saturating_sub(shown);
    if omitted > 0 {
        lines.push(format!(
            "({omitted} more shared {more_label} on disk — grep {grep_hint} for them.)"
        ));
    }
}

pub fn render_user_memory_system_prompt(
    recall: &UserMemoryRecall,
    user_memory_dir: Option<&str>,
    own_shard_dir: Option<&str>,
) -> String {
    let Some(user_memory_dir) = user_memory_dir else { return String::new(); };
    let has_facts = !recall.profile.is_empty() || !recall.recent.is_empty();
    let mut lines = vec![
        "User memory: durable facts shared across every assistant this user runs — their name, timezone, lasting preferences, and anything all of the user's assistants should know. This is separate from your own memory (shown below) and is visible to all of them.".to_string(),
        "Precedence: when a shared user fact conflicts with your OWN memory, prefer your own — it is curated for your role and may deliberately override a shared default.".to_string(),
    ];
    if let Some(own_shard_dir) = own_shard_dir {
        lines.push(format!(
            "User memory lives under {user_memory_dir}, split into one shard folder per assistant so every file has a single writer. Your own shard is at {own_shard_dir} (a profile.md and log/YYYY-MM.md you can read and grep with Read and Shell on your own computer). To CHANGE shared user memory, prefer the update_state tool (target \"memory\", scope \"user\", action \"write\" or \"forget\"). Never edit another assistant's shard."
        ));
        lines.push(
            "To fix or replace a shared fact another assistant recorded, write the corrected fact into YOUR shard via update_state — the newest wins on conflict. Record a fact here only when it is clearly about the user and useful to every assistant; keep role-specific facts in your own memory (scope \"agent\").".to_string()
        );
    }
    if has_facts {
        lines.push("Shared facts are tagged [via <assistant>] so you can tell which assistant learned each one.".to_string());
    }
    if !recall.profile.is_empty() {
        lines.push("About the user (shared):".to_string());
        append_budgeted_provenanced_facts(
            &mut lines, &recall.profile, MEMORY_USER_PROFILE_CHAR_BUDGET,
            "profile facts", "the user-memory/ folder",
        );
    }
    if !recall.recent.is_empty() {
        lines.push("Recently (shared):".to_string());
        append_budgeted_provenanced_facts(
            &mut lines, &recall.recent, MEMORY_USER_RECENT_CHAR_BUDGET,
            "log facts", "the user-memory/ folder",
        );
    }
    if recall.profile.is_empty() && recall.recent.is_empty() {
        lines.push("No shared facts recorded yet.".to_string());
    }
    lines.join("\n")
}

pub fn render_project_memory_system_prompt(
    recall: &ProjectMemoryPromptRecall,
    projects_root_dir: Option<&str>,
) -> String {
    let Some(projects_root_dir) = projects_root_dir else {
        return String::new();
    };
    let mut lines = vec![
        "Project memory: durable facts shared by every assistant that has joined a project — the project's decisions, conventions, and state. Projects are optional and opt-in; joining one lets its memory into your prompt below.".to_string(),
        "Precedence across memory tiers: on conflict prefer your OWN memory first, then project memory, then user memory — the most specific wins.".to_string(),
        format!(
            "Projects live under {projects_root_dir}: each is a folder <slug>/ holding a project.md (frontmatter name/description) and memory/by-agent/<assistantId>/ shards (one per contributing assistant, a standard profile.md + log/). Read and grep those folders with Read and Shell on your own computer; prefer the update_state tool for every CHANGE:"
        ),
        "  - Define a project: update_state target \"project\", action \"create\", project=<slug>, name=... (optional description). If the slug already exists this is create-is-join.".to_string(),
        "  - Join or leave: update_state target \"project\", action \"join\" or \"leave\", project=<slug>. Only projects you have joined load below; to see who else is a member, grep the assistants' projects.json files.".to_string(),
        "  - Write project facts with update_state target \"memory\", scope \"project\", project=<slug>, action \"write\" or \"forget\" (never another assistant's shard); newest wins on conflict. Record a fact here only when it is about the project and useful to every member.".to_string(),
    ];
    for block in &recall.injected {
        lines.push(format!(
            "Project \"{}\" ({}) — your shard: {}:",
            block.name,
            block.slug,
            block.own_shard_dir.to_string_lossy()
        ));
        if !block.recall.profile.is_empty() {
            lines.push("About this project (shared):".to_string());
            append_budgeted_provenanced_facts(
                &mut lines,
                &block.recall.profile,
                MEMORY_PROJECT_PROFILE_CHAR_BUDGET,
                "profile facts",
                "this project's memory/ folder",
            );
        }
        if !block.recall.recent.is_empty() {
            lines.push("Recently (shared):".to_string());
            append_budgeted_provenanced_facts(
                &mut lines,
                &block.recall.recent,
                MEMORY_PROJECT_RECENT_CHAR_BUDGET,
                "log facts",
                "this project's memory/ folder",
            );
        }
        if block.recall.profile.is_empty() && block.recall.recent.is_empty() {
            lines.push("No shared facts recorded yet for this project.".to_string());
        }
    }
    if !recall.also_member_of.is_empty() {
        lines.push(format!(
            "Also a member of: {} — grep those project folders for their memory.",
            recall
                .also_member_of
                .iter()
                .map(|project| format!("{} ({})", project.name, project.slug))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    lines.join("\n")
}

pub fn build_extraction_system_prompt() -> String {
    [
        MEMORY_EXTRACTION_PROMPT_MARKER,
        "You maintain the long-term memory of a personal assistant. Read the latest exchange and decide what — if anything — is worth remembering for future, unrelated conversations.",
        "",
        "Tag each fact you keep with a category:",
        "- \"profile\": enduring facts about who the user is and how to work with them — their name and how to address them, role, location, languages, lasting preferences and constraints, and important people or relationships. These are remembered indefinitely.",
        "- \"log\": substantive history worth keeping — ongoing projects and tasks, decisions, commitments, and time-bound details.",
        "- \"note\": minor, low-stakes details that might help someday but are not worth keeping in mind every turn (small one-off preferences, incidental context). Notes fade from the always-visible list fastest but stay on disk.",
        "",
        "Do NOT record one-off request mechanics, what the assistant did this turn, general knowledge, or anything already present in the existing memory list.",
        "",
        "If the new exchange updates or contradicts a fact in the existing memory list (e.g. the user moved, changed jobs, or renamed something), drop anything clearly superseded: output a line \"remove: <the exact existing fact text>\" and then add the corrected fact. Only remove facts that appear verbatim in the existing list — never invent removals.",
        "",
        "Write each fact as a self-contained statement, one per line: \"profile: <fact>\", \"log: <fact>\", or \"note: <fact>\" to add, or \"remove: <existing fact>\" to drop a superseded one.",
        "Output exactly NONE (and nothing else) when there is nothing to add or remove.",
    ]
    .join("\n")
}

pub fn build_extraction_user_prompt(
    user_message: &str,
    agent_message: &str,
    existing_memories: &[String],
) -> String {
    let existing = if existing_memories.is_empty() {
        "(empty)".to_string()
    } else {
        existing_memories
            .iter()
            .map(|memory| format!("- {memory}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "Existing memory:\n{existing}\n\nLatest exchange:\nUser: {}\nAssistant: {}",
        non_empty_or(user_message.trim(), "(no message)"),
        non_empty_or(agent_message.trim(), "(no message)")
    )
}

pub fn parse_extracted_memories(raw: &str, existing_memories: &[String]) -> MemoryExtraction {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case(MEMORY_EXTRACTION_NONE_SENTINEL) {
        return MemoryExtraction::default();
    }
    let mut seen = existing_memories
        .iter()
        .map(|memory| memory_dedupe_key(memory))
        .collect::<HashSet<_>>();
    let mut extraction = MemoryExtraction::default();

    for raw_line in trimmed.lines() {
        let stripped = strip_list_prefix(raw_line.trim());
        let (tag, value) = split_category(stripped);
        let bare = normalize_memory_content(value);
        if bare.is_empty() || bare.eq_ignore_ascii_case(MEMORY_EXTRACTION_NONE_SENTINEL) {
            continue;
        }
        if tag == Some("remove") {
            extraction.removals.push(bare);
            continue;
        }
        let content = if tag == Some("note") {
            normalize_memory_content(&format!("{MEMORY_NOTE_PREFIX}{bare}"))
        } else {
            bare
        };
        let key = memory_dedupe_key(&content);
        if !seen.insert(key) {
            continue;
        }
        extraction.additions.push(MemoryAddition {
            content,
            kind: if tag == Some("profile") {
                MemoryKind::Profile
            } else {
                MemoryKind::Log
            },
        });
    }
    extraction
}

pub fn select_relevant_memories(
    query: &str,
    memories: &[MemoryRecord],
    max: usize,
) -> Vec<MemoryRecord> {
    if max == 0 || memories.is_empty() {
        return Vec::new();
    }
    let query_tokens = relevance_tokens(query);
    if query_tokens.is_empty() {
        return Vec::new();
    }
    let mut scored = memories
        .iter()
        .filter_map(|memory| {
            let overlap = relevance_tokens(&memory.content)
                .iter()
                .filter(|token| query_tokens.contains(*token))
                .count();
            (overlap > 0).then_some((memory.clone(), overlap))
        })
        .collect::<Vec<_>>();
    scored.sort_by(|(left, left_overlap), (right, right_overlap)| {
        right_overlap
            .cmp(left_overlap)
            .then_with(|| right.created_at.cmp(&left.created_at))
    });
    scored
        .into_iter()
        .take(max)
        .map(|(memory, _)| memory)
        .collect()
}

pub fn gather_extraction_memories(
    recall: &MemoryRecall,
    archive: &[MemoryRecord],
    exchange_text: &str,
) -> Vec<String> {
    let mut in_prompt = recall.profile.clone();
    in_prompt.extend(recall.recent.clone());
    let seen = in_prompt
        .iter()
        .map(|memory| memory_dedupe_key(&memory.content))
        .collect::<HashSet<_>>();
    let candidates = archive
        .iter()
        .filter(|memory| !seen.contains(&memory_dedupe_key(&memory.content)))
        .cloned()
        .collect::<Vec<_>>();
    in_prompt
        .into_iter()
        .chain(select_relevant_memories(exchange_text, &candidates, 10))
        .map(|memory| memory.content)
        .collect()
}

pub fn apply_extracted_memories(
    store: &FileMemoryStore,
    extraction: &MemoryExtraction,
    now_ms: i64,
    known_memories: &[String],
) -> io::Result<AppliedMemoryExtraction> {
    let known = known_memories
        .iter()
        .map(|memory| memory_dedupe_key(memory))
        .collect::<HashSet<_>>();
    let mut result = AppliedMemoryExtraction::default();
    for removal in &extraction.removals {
        if known.contains(&memory_dedupe_key(removal))
            && store.remove_memory_by_content(removal)?
        {
            result.removed.push(removal.clone());
        }
    }
    for addition in &extraction.additions {
        if let Some(record) = store.add_memory(&addition.content, now_ms, addition.kind)? {
            result.added.push(record);
        }
    }
    Ok(result)
}

pub fn build_episode_system_prompt() -> String {
    [
        MEMORY_EPISODE_PROMPT_MARKER,
        "You maintain the long-term memory of a personal desktop assistant named Fabushi.",
        "You are given the most recent turns of a conversation between the user and Fabushi, in order, each tagged with its date.",
        "Write ONE short journal-style sentence (two at most) capturing what the user and Fabushi were actually working on across these turns — the throughline, key decisions, and outcomes — so it stays useful months from now.",
        "Anchor any time references with the absolute dates shown, never relative words like \"yesterday\". Drop greetings, acknowledgements, and anything ephemeral. Never invent details.",
        "Output just the sentence(s), no preamble or bullets. Output exactly NONE if nothing in this stretch is worth remembering.",
    ]
    .join("\n")
}

pub fn build_episode_user_prompt(turns: &[EpisodeTurn]) -> String {
    let rendered = turns
        .iter()
        .map(|turn| {
            let mut lines = vec![format!("({})", format_memory_date(turn.ts))];
            if !turn.user.trim().is_empty() {
                lines.push(format!("User: {}", turn.user.trim()));
            }
            if !turn.agent.trim().is_empty() {
                lines.push(format!("Fabushi: {}", turn.agent.trim()));
            }
            lines.join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    format!("Recent turns, oldest first:\n\n{rendered}")
}

fn non_empty_or<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.is_empty() { fallback } else { value }
}

fn split_category(line: &str) -> (Option<&'static str>, &str) {
    for tag in ["profile", "log", "note", "remove"] {
        let tag_len = tag.len();
        if line.len() > tag_len
            && line
                .get(..tag_len)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(tag))
            && line.as_bytes().get(tag_len) == Some(&b':')
        {
            return (
                Some(tag),
                line.get(tag_len + 1..).unwrap_or_default().trim(),
            );
        }
    }
    (None, line)
}

fn strip_list_prefix(line: &str) -> &str {
    let line = line.trim_start();
    if let Some(rest) = line.strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("• "))
    {
        return rest.trim_start();
    }
    let bytes = line.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    if index > 0
        && index + 1 < bytes.len()
        && matches!(bytes[index], b'.' | b')')
        && bytes[index + 1].is_ascii_whitespace()
    {
        return line[index + 2..].trim_start();
    }
    line
}

fn relevance_tokens(text: &str) -> HashSet<String> {
    text.split(|ch: char| !ch.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|token| token.chars().count() >= 4 && !is_relevance_stopword(token))
        .collect()
}

fn is_relevance_stopword(token: &str) -> bool {
    matches!(
        token,
        "that" | "this" | "with" | "from" | "they" | "them" | "then" | "than"
            | "what" | "when" | "where" | "which" | "will" | "would" | "could"
            | "should" | "have" | "been" | "being" | "about" | "just" | "like"
            | "your" | "does" | "were" | "also" | "into" | "over" | "only"
            | "some" | "more" | "most" | "very" | "much" | "here" | "there"
            | "their" | "these" | "those" | "because" | "while" | "after"
            | "before" | "user"
    )
}
