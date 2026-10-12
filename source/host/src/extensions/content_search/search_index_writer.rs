use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};
use url::Url;

use super::search_index_db::{
    AttachmentKind, DB_BUSY_TIMEOUT_MS, write_reconcile_done,
};

pub const STORE_FILENAME: &str = "store.db";
pub const INCREMENTAL_VACUUM_PAGES: usize = 512;
pub const INDEXED_BODY_MAX_CHARS: usize = 20_000;

#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct IndexAgentTarget {
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct IndexMessageValue {
    #[serde(rename = "type")]
    pub message_type: Option<String>,
    pub content: Option<String>,
    pub url: Option<String>,
    pub file_name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct IndexEntry {
    #[serde(default)]
    pub id: String,
    pub kind: String,
    #[serde(rename = "timestampMs")]
    pub timestamp_ms: Option<f64>,
    pub role: Option<String>,
    pub content: Option<String>,
    pub text: Option<String>,
    #[serde(rename = "toAgent")]
    pub to_agent: Option<IndexAgentTarget>,
    pub message: Option<IndexMessageValue>,
    pub file_path: Option<String>,
    pub file_name: Option<String>,
    pub width: Option<f64>,
    pub height: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind")]
pub enum SearchIndexJob {
    #[serde(rename = "upsert-entries")]
    UpsertEntries {
        #[serde(rename = "agentId")]
        agent_id: String,
        entries: Vec<IndexEntry>,
    },
    #[serde(rename = "delete-entry")]
    DeleteEntry {
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "entryId")]
        entry_id: String,
    },
    #[serde(rename = "clear-agent")]
    ClearAgent {
        #[serde(rename = "agentId")]
        agent_id: String,
    },
    #[serde(rename = "reindex-agents")]
    ReindexAgents {
        #[serde(rename = "agentIds")]
        agent_ids: Vec<String>,
    },
    #[serde(rename = "reconcile")]
    Reconcile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IndexedMessage {
    entry_id: String,
    role: String,
    timestamp_ms: i64,
    body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IndexedMedia {
    entry_id: String,
    file_name: String,
    ext: String,
    mime: Option<String>,
    kind: AttachmentKind,
    timestamp_ms: i64,
    width: Option<i64>,
    height: Option<i64>,
}

fn js_round(value: Option<f64>) -> i64 {
    let Some(value) = value.filter(|value| value.is_finite()) else {
        return 0;
    };
    (value + 0.5).floor() as i64
}

fn whole_dimension(value: Option<f64>) -> Option<i64> {
    value.filter(|value| value.is_finite()).map(|value| (value + 0.5).floor() as i64)
}

fn entry_text(entry: &IndexEntry) -> &str {
    match entry.kind.as_str() {
        "message" => entry.content.as_deref().unwrap_or(""),
        "send-message"
            if entry
                .message
                .as_ref()
                .and_then(|message| message.message_type.as_deref())
                == Some("text") =>
        {
            entry
                .message
                .as_ref()
                .and_then(|message| message.content.as_deref())
                .unwrap_or("")
        }
        "notice" => entry.text.as_deref().unwrap_or(""),
        _ => "",
    }
}

fn take_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn message_of(entry: &IndexEntry) -> Option<IndexedMessage> {
    if entry.kind == "message"
        && entry
            .to_agent
            .as_ref()
            .is_some_and(|target| target.kind.as_deref() != Some("agent"))
    {
        return None;
    }
    let body = entry_text(entry).trim();
    if body.is_empty() {
        return None;
    }
    Some(IndexedMessage {
        entry_id: entry.id.clone(),
        role: if entry.kind == "message" && entry.role.as_deref() == Some("user") {
            "user".into()
        } else {
            "assistant".into()
        },
        timestamp_ms: js_round(entry.timestamp_ms),
        body: take_chars(body, INDEXED_BODY_MAX_CHARS),
    })
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn decode_uri_component(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return None;
            }
            let hi = hex_value(bytes[index + 1])?;
            let lo = hex_value(bytes[index + 2])?;
            out.push((hi << 4) | lo);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn basename(value: &str) -> String {
    value
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .to_string()
}

fn media_file_name(file_name: Option<&str>, url_or_path: &str) -> String {
    if let Some(trimmed) = file_name.map(str::trim).filter(|value| !value.is_empty()) {
        return trimmed.to_string();
    }
    let mut subject = Url::parse(url_or_path)
        .ok()
        .map(|url| url.path().to_string())
        .unwrap_or_else(|| url_or_path.to_string());
    if let Some(decoded) = decode_uri_component(&subject) {
        subject = decoded;
    }
    basename(&subject)
}

fn extension(file_name: &str) -> Option<String> {
    let base = file_name.rsplit(['/', '\\']).next().unwrap_or("");
    let dot = base.rfind('.')?;
    if dot == 0 || dot + 1 >= base.len() {
        return None;
    }
    Some(base[dot + 1..].to_ascii_lowercase())
}

fn extname(file_name: &str) -> String {
    extension(file_name)
        .map(|extension| format!(".{extension}"))
        .unwrap_or_default()
}

fn media_mime(file_name: &str) -> Option<String> {
    let extension = extension(file_name)?;
    let mime = match extension.as_str() {
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "jpeg" | "jpg" => "image/jpeg",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "m4v" | "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "ogv" => "video/ogg",
        "webm" => "video/webm",
        "aac" => "audio/aac",
        "flac" => "audio/flac",
        "m4a" => "audio/mp4",
        "mp3" => "audio/mpeg",
        "oga" | "ogg" | "opus" => "audio/ogg",
        "wav" => "audio/wav",
        "weba" => "audio/webm",
        _ => return None,
    };
    Some(mime.into())
}

fn classify_attachment(file_name: &str, url_or_path: &str) -> AttachmentKind {
    const IMAGES: &[&str] = &["avif", "bmp", "gif", "ico", "jpeg", "jpg", "png", "svg", "webp"];
    const VIDEOS: &[&str] = &["m4v", "mov", "mp4", "ogv", "webm"];
    const AUDIO: &[&str] = &["aac", "flac", "m4a", "mp3", "oga", "ogg", "opus", "wav", "weba"];
    const MARKDOWN: &[&str] = &["md", "markdown", "mdx"];
    const TABLES: &[&str] = &["csv", "tsv", "xlsx", "xls"];
    const JSON: &[&str] = &["json", "jsonc", "json5", "ndjson"];
    const ARCHIVES: &[&str] = &["zip", "tar", "gz", "tgz", "bz2", "tbz2", "xz", "txz", "zst", "7z", "rar"];
    const TEXT: &[&str] = &[
        "txt", "text", "log", "rst", "adoc", "tex", "xml", "yaml", "yml", "toml", "ini",
        "cfg", "conf", "env", "properties", "plist", "gradle", "html", "htm", "css", "scss",
        "sass", "less", "js", "jsx", "mjs", "cjs", "ts", "tsx", "mts", "cts", "py", "pyi",
        "rb", "go", "rs", "java", "kt", "kts", "c", "h", "cc", "cpp", "cxx", "hpp", "hh",
        "cs", "php", "swift", "scala", "dart", "lua", "pl", "pm", "r", "sql", "graphql",
        "gql", "proto", "vue", "svelte", "astro", "sh", "bash", "zsh", "fish", "bat", "ps1",
        "tf", "tfvars", "dockerfile", "diff", "patch",
    ];

    let classify = |subject: &str| -> Option<AttachmentKind> {
        let extension = extension(subject)?;
        let extension = extension.as_str();
        if IMAGES.contains(&extension) {
            Some(AttachmentKind::Image)
        } else if VIDEOS.contains(&extension) {
            Some(AttachmentKind::Video)
        } else if AUDIO.contains(&extension) {
            Some(AttachmentKind::Audio)
        } else if extension == "pdf" {
            Some(AttachmentKind::Pdf)
        } else if TABLES.contains(&extension) {
            Some(AttachmentKind::Table)
        } else if JSON.contains(&extension) {
            Some(AttachmentKind::Json)
        } else if MARKDOWN.contains(&extension) {
            Some(AttachmentKind::Markdown)
        } else if extension == "docx" {
            Some(AttachmentKind::Document)
        } else if TEXT.contains(&extension) {
            Some(AttachmentKind::Text)
        } else if ARCHIVES.contains(&extension) {
            Some(AttachmentKind::Archive)
        } else {
            None
        }
    };

    classify(file_name)
        .or_else(|| classify(url_or_path))
        .unwrap_or(AttachmentKind::File)
}

fn media_of(entry: &IndexEntry) -> Option<IndexedMedia> {
    let (url_or_path, file_name, width, height) = if entry.kind == "user-attachment" {
        let path = entry.file_path.as_deref()?;
        (
            path,
            media_file_name(entry.file_name.as_deref(), path),
            whole_dimension(entry.width),
            whole_dimension(entry.height),
        )
    } else if entry.kind == "send-message"
        && entry
            .message
            .as_ref()
            .and_then(|message| message.message_type.as_deref())
            == Some("attachment")
    {
        let message = entry.message.as_ref()?;
        let url = message.url.as_deref()?;
        (
            url,
            media_file_name(message.file_name.as_deref(), url),
            None,
            None,
        )
    } else {
        return None;
    };
    if file_name.is_empty() {
        return None;
    }
    Some(IndexedMedia {
        entry_id: entry.id.clone(),
        ext: extname(&file_name),
        mime: media_mime(&file_name),
        kind: classify_attachment(&file_name, url_or_path),
        timestamp_ms: js_round(entry.timestamp_ms),
        width,
        height,
        file_name,
    })
}

pub struct SandSearchIndexWriter {
    db: Connection,
    agents_root_dir: PathBuf,
    store_connections: HashMap<String, Connection>,
}

impl SandSearchIndexWriter {
    pub fn new(db: Connection, agents_root_dir: impl Into<PathBuf>) -> Self {
        Self {
            db,
            agents_root_dir: agents_root_dir.into(),
            store_connections: HashMap::new(),
        }
    }

    pub fn connection(&self) -> &Connection {
        &self.db
    }

    pub fn close(&mut self) {
        self.store_connections.clear();
    }

    pub fn run_job(&mut self, job: &SearchIndexJob) -> rusqlite::Result<()> {
        match job {
            SearchIndexJob::UpsertEntries { agent_id, entries } => {
                self.upsert_entries(agent_id, entries)
            }
            SearchIndexJob::DeleteEntry { agent_id, entry_id } => {
                self.delete_entry(agent_id, entry_id)
            }
            SearchIndexJob::ClearAgent { agent_id } => self.clear_agent(agent_id),
            SearchIndexJob::ReindexAgents { agent_ids } => {
                for agent_id in agent_ids {
                    self.reindex_agent(agent_id)?;
                }
                Ok(())
            }
            SearchIndexJob::Reconcile => self.reconcile(),
        }
    }

    fn store_db_path(&self, id: &str) -> PathBuf {
        self.agents_root_dir.join(id).join(STORE_FILENAME)
    }

    fn evict(&mut self, id: &str) {
        self.store_connections.remove(id);
    }

    fn ensure_store(&mut self, id: &str) -> bool {
        if self.store_connections.contains_key(id) {
            return true;
        }
        let path = self.store_db_path(id);
        if !path.exists() {
            return false;
        }
        let Ok(db) = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY) else {
            return false;
        };
        if db
            .busy_timeout(Duration::from_millis(DB_BUSY_TIMEOUT_MS))
            .is_err()
        {
            return false;
        }
        self.store_connections.insert(id.to_string(), db);
        true
    }

    fn fingerprint(&mut self, id: &str) -> Option<String> {
        if !self.ensure_store(id) {
            return None;
        }
        let result = self.store_connections.get(id)?.query_row(
            "SELECT COUNT(*) AS count, COALESCE(MAX(seq), 0) AS maxSeq FROM transcript_entries",
            [],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        );
        match result {
            Ok((count, max_seq)) => Some(format!("{count}:{max_seq}")),
            Err(_) => {
                self.evict(id);
                None
            }
        }
    }

    fn begin(&self) -> rusqlite::Result<()> {
        self.db.execute_batch("BEGIN IMMEDIATE")
    }

    fn commit(&self) -> rusqlite::Result<()> {
        self.db.execute_batch("COMMIT")
    }

    fn rollback(&self) {
        let _ = self.db.execute_batch("ROLLBACK");
    }

    fn apply(&self, agent_id: &str, entry: &IndexEntry) -> rusqlite::Result<()> {
        if let Some(message) = message_of(entry) {
            self.db.execute(
                "INSERT INTO messages (agent_id, entry_id, role, timestamp_ms, body)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(agent_id, entry_id) DO UPDATE SET
                   role=excluded.role,
                   timestamp_ms=excluded.timestamp_ms,
                   body=excluded.body",
                params![
                    agent_id,
                    message.entry_id,
                    message.role,
                    message.timestamp_ms,
                    message.body
                ],
            )?;
        } else {
            self.db.execute(
                "DELETE FROM messages WHERE agent_id = ?1 AND entry_id = ?2",
                params![agent_id, entry.id],
            )?;
        }

        if let Some(media) = media_of(entry) {
            self.db.execute(
                "INSERT INTO media
                   (agent_id, entry_id, file_name, ext, mime, kind, timestamp_ms, width, height)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(agent_id, entry_id) DO UPDATE SET
                   file_name=excluded.file_name,
                   ext=excluded.ext,
                   mime=excluded.mime,
                   kind=excluded.kind,
                   timestamp_ms=excluded.timestamp_ms,
                   width=excluded.width,
                   height=excluded.height",
                params![
                    agent_id,
                    media.entry_id,
                    media.file_name,
                    media.ext,
                    media.mime,
                    attachment_kind_name(media.kind),
                    media.timestamp_ms,
                    media.width,
                    media.height
                ],
            )?;
        } else {
            self.db.execute(
                "DELETE FROM media WHERE agent_id = ?1 AND entry_id = ?2",
                params![agent_id, entry.id],
            )?;
        }
        Ok(())
    }

    fn refresh(&mut self, id: &str) -> rusqlite::Result<()> {
        if let Some(value) = self.fingerprint(id) {
            self.db.execute(
                "INSERT INTO agents (agent_id, fingerprint) VALUES (?1, ?2)
                 ON CONFLICT(agent_id) DO UPDATE SET fingerprint=excluded.fingerprint",
                params![id, value],
            )?;
        } else {
            self.db
                .execute("DELETE FROM agents WHERE agent_id = ?1", [id])?;
        }
        Ok(())
    }

    pub fn upsert_entries(&mut self, id: &str, entries: &[IndexEntry]) -> rusqlite::Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        self.begin()?;
        let result = (|| {
            for entry in entries {
                self.apply(id, entry)?;
            }
            self.refresh(id)
        })();
        match result {
            Ok(()) => self.commit(),
            Err(error) => {
                self.rollback();
                Err(error)
            }
        }
    }

    pub fn delete_entry(&mut self, id: &str, entry_id: &str) -> rusqlite::Result<()> {
        self.begin()?;
        let result = (|| {
            self.db.execute(
                "DELETE FROM messages WHERE agent_id = ?1 AND entry_id = ?2",
                params![id, entry_id],
            )?;
            self.db.execute(
                "DELETE FROM media WHERE agent_id = ?1 AND entry_id = ?2",
                params![id, entry_id],
            )?;
            self.refresh(id)
        })();
        match result {
            Ok(()) => self.commit(),
            Err(error) => {
                self.rollback();
                Err(error)
            }
        }
    }

    pub fn clear_agent(&mut self, id: &str) -> rusqlite::Result<()> {
        self.evict(id);
        self.begin()?;
        let result = (|| {
            self.db.execute("DELETE FROM messages WHERE agent_id = ?1", [id])?;
            self.db.execute("DELETE FROM media WHERE agent_id = ?1", [id])?;
            self.db.execute("DELETE FROM agents WHERE agent_id = ?1", [id])?;
            Ok(())
        })();
        match result {
            Ok(()) => self.commit()?,
            Err(error) => {
                self.rollback();
                return Err(error);
            }
        }
        self.db.execute_batch(&format!(
            "PRAGMA incremental_vacuum({INCREMENTAL_VACUUM_PAGES})"
        ))
    }

    pub fn reindex_agent(&mut self, id: &str) -> rusqlite::Result<()> {
        self.evict(id);
        if !self.store_db_path(id).exists() {
            return self.clear_agent(id);
        }
        if !self.ensure_store(id) {
            return Ok(());
        }
        let rows_result = {
            let Some(store) = self.store_connections.get(id) else {
                return Ok(());
            };
            (|| -> rusqlite::Result<Vec<(Option<i64>, Option<String>)>> {
                let mut statement =
                    store.prepare("SELECT seq, entry FROM transcript_entries ORDER BY seq")?;
                let mapped = statement.query_map([], |row| {
                    Ok((row.get::<_, i64>(0).ok(), row.get::<_, String>(1).ok()))
                })?;
                mapped.collect()
            })()
        };
        let rows = match rows_result {
            Ok(rows) => rows,
            Err(_) => {
                self.evict(id);
                return Ok(());
            }
        };

        let mut max_seq = 0i64;
        let mut entries = Vec::new();
        for (seq, raw) in &rows {
            if let Some(seq) = seq {
                max_seq = max_seq.max(*seq);
            }
            let Some(raw) = raw else {
                continue;
            };
            if let Ok(entry) = serde_json::from_str::<IndexEntry>(raw) {
                if !entry.id.is_empty() && !entry.kind.is_empty() {
                    entries.push(entry);
                }
            }
        }

        self.begin()?;
        let result = (|| {
            self.db.execute("DELETE FROM messages WHERE agent_id = ?1", [id])?;
            self.db.execute("DELETE FROM media WHERE agent_id = ?1", [id])?;
            for entry in &entries {
                self.apply(id, entry)?;
            }
            self.db.execute(
                "INSERT INTO agents (agent_id, fingerprint) VALUES (?1, ?2)
                 ON CONFLICT(agent_id) DO UPDATE SET fingerprint=excluded.fingerprint",
                params![id, format!("{}:{max_seq}", rows.len())],
            )?;
            Ok(())
        })();
        match result {
            Ok(()) => self.commit()?,
            Err(error) => {
                self.rollback();
                return Err(error);
            }
        }
        self.db.execute_batch(&format!(
            "PRAGMA incremental_vacuum({INCREMENTAL_VACUUM_PAGES})"
        ))
    }

    pub fn reconcile(&mut self) -> rusqlite::Result<()> {
        let ids = fs::read_dir(&self.agents_root_dir)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
                    .filter_map(|entry| entry.file_name().into_string().ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let on_disk = ids.iter().cloned().collect::<HashSet<_>>();

        let indexed_ids = {
            let mut statement = self.db.prepare(
                "SELECT agent_id FROM agents
                 UNION SELECT DISTINCT agent_id FROM messages
                 UNION SELECT DISTINCT agent_id FROM media",
            )?;
            let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
            rows.filter_map(Result::ok).collect::<Vec<_>>()
        };
        for id in indexed_ids {
            if !on_disk.contains(&id) {
                self.clear_agent(&id)?;
            }
        }

        for id in ids {
            if !self.store_db_path(&id).exists() {
                continue;
            }
            let current = self.fingerprint(&id);
            let indexed = self
                .db
                .query_row(
                    "SELECT fingerprint FROM agents WHERE agent_id = ?1",
                    [&id],
                    |row| row.get::<_, String>(0),
                )
                .ok();
            if current.is_some() && indexed != current {
                self.reindex_agent(&id)?;
            }
        }
        write_reconcile_done(&self.db)
    }
}

fn attachment_kind_name(kind: AttachmentKind) -> &'static str {
    match kind {
        AttachmentKind::Image => "image",
        AttachmentKind::Video => "video",
        AttachmentKind::Audio => "audio",
        AttachmentKind::Pdf => "pdf",
        AttachmentKind::Markdown => "markdown",
        AttachmentKind::Table => "table",
        AttachmentKind::Json => "json",
        AttachmentKind::Text => "text",
        AttachmentKind::Document => "document",
        AttachmentKind::Archive => "archive",
        AttachmentKind::File => "file",
    }
}
