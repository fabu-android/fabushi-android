use std::path::Path;

use rusqlite::{Connection, OpenFlags, params};
use unicode_normalization::UnicodeNormalization;

use super::agent_content_search::AGENT_CONTENT_SEARCH_MAX_MATCHES_PER_AGENT;

pub const SEARCH_INDEX_SCHEMA_VERSION: i64 = 1;
pub const SEARCH_INDEX_FILENAME: &str = "search-index.db";
pub const FTS_QUERY_MAX_TERMS: usize = 8;
pub const SNIPPET_CONTEXT_TOKENS: usize = 16;
pub const META_RECONCILE_DONE: &str = "reconcile_done";
pub const DB_BUSY_TIMEOUT_MS: u64 = 5_000;

pub const SCHEMA: &str = r#"CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL) STRICT;
CREATE TABLE IF NOT EXISTS agents (agent_id TEXT PRIMARY KEY, fingerprint TEXT NOT NULL) STRICT;
CREATE TABLE IF NOT EXISTS messages (id INTEGER PRIMARY KEY, agent_id TEXT NOT NULL, entry_id TEXT NOT NULL, role TEXT NOT NULL CHECK (role IN ('user', 'assistant')), timestamp_ms INTEGER NOT NULL, body TEXT NOT NULL, UNIQUE(agent_id, entry_id)) STRICT;
CREATE INDEX IF NOT EXISTS messages_agent_recency ON messages(agent_id, timestamp_ms DESC);
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(body, content='messages', content_rowid='id', tokenize='unicode61 remove_diacritics 2', prefix='2 3');
CREATE TRIGGER IF NOT EXISTS messages_fts_insert AFTER INSERT ON messages BEGIN INSERT INTO messages_fts(rowid, body) VALUES (new.id, new.body); END;
CREATE TRIGGER IF NOT EXISTS messages_fts_delete AFTER DELETE ON messages BEGIN INSERT INTO messages_fts(messages_fts, rowid, body) VALUES ('delete', old.id, old.body); END;
CREATE TRIGGER IF NOT EXISTS messages_fts_update AFTER UPDATE ON messages BEGIN INSERT INTO messages_fts(messages_fts, rowid, body) VALUES ('delete', old.id, old.body); INSERT INTO messages_fts(rowid, body) VALUES (new.id, new.body); END;
CREATE TABLE IF NOT EXISTS media (id INTEGER PRIMARY KEY, agent_id TEXT NOT NULL, entry_id TEXT NOT NULL, file_name TEXT NOT NULL, ext TEXT NOT NULL, mime TEXT, kind TEXT NOT NULL, timestamp_ms INTEGER NOT NULL, width INTEGER, height INTEGER, UNIQUE(agent_id, entry_id)) STRICT;
CREATE INDEX IF NOT EXISTS media_recency ON media(timestamp_ms DESC);
CREATE VIRTUAL TABLE IF NOT EXISTS media_fts USING fts5(file_name, content='media', content_rowid='id', tokenize='unicode61 remove_diacritics 2', prefix='2 3');
CREATE TRIGGER IF NOT EXISTS media_fts_insert AFTER INSERT ON media BEGIN INSERT INTO media_fts(rowid, file_name) VALUES (new.id, new.file_name); END;
CREATE TRIGGER IF NOT EXISTS media_fts_delete AFTER DELETE ON media BEGIN INSERT INTO media_fts(media_fts, rowid, file_name) VALUES ('delete', old.id, old.file_name); END;
CREATE TRIGGER IF NOT EXISTS media_fts_update AFTER UPDATE ON media BEGIN INSERT INTO media_fts(media_fts, rowid, file_name) VALUES ('delete', old.id, old.file_name); INSERT INTO media_fts(rowid, file_name) VALUES (new.id, new.file_name); END;"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentKind {
    Image,
    Video,
    Audio,
    Pdf,
    Markdown,
    Table,
    Json,
    Text,
    Document,
    Archive,
    File,
}

impl AttachmentKind {
    pub fn parse(value: &str) -> Self {
        match value {
            "image" => Self::Image,
            "video" => Self::Video,
            "audio" => Self::Audio,
            "pdf" => Self::Pdf,
            "markdown" => Self::Markdown,
            "table" => Self::Table,
            "json" => Self::Json,
            "text" => Self::Text,
            "document" => Self::Document,
            "archive" => Self::Archive,
            "file" => Self::File,
            _ => Self::File,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageSearchResult {
    pub agent_id: String,
    pub entry_id: String,
    pub role: String,
    pub timestamp_ms: i64,
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaSearchResult {
    pub agent_id: String,
    pub entry_id: String,
    pub file_name: String,
    pub ext: String,
    pub mime: Option<String>,
    pub kind: AttachmentKind,
    pub timestamp_ms: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
}

pub fn open_search_index_db(path: impl AsRef<Path>) -> rusqlite::Result<Connection> {
    let db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
    )?;
    db.busy_timeout(std::time::Duration::from_millis(DB_BUSY_TIMEOUT_MS))?;
    db.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA auto_vacuum = INCREMENTAL;",
    )?;
    Ok(db)
}

pub fn ensure_search_index_schema(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(SCHEMA)
}

pub fn read_search_index_schema_version(db: &Connection) -> rusqlite::Result<i64> {
    db.query_row("PRAGMA user_version", [], |row| row.get(0))
}

pub fn stamp_search_index_schema_version(db: &Connection) -> rusqlite::Result<()> {
    db.pragma_update(None, "user_version", SEARCH_INDEX_SCHEMA_VERSION)
}

pub fn read_reconcile_done(db: &Connection) -> rusqlite::Result<bool> {
    let mut statement = db.prepare("SELECT value FROM meta WHERE key = ?1")?;
    let mut rows = statement.query([META_RECONCILE_DONE])?;
    Ok(rows.next()?.and_then(|row| row.get::<_, String>(0).ok()).as_deref() == Some("1"))
}

pub fn write_reconcile_done(db: &Connection) -> rusqlite::Result<()> {
    db.execute(
        "INSERT INTO meta (key, value) VALUES (?1, '1')
         ON CONFLICT(key) DO UPDATE SET value = '1'",
        [META_RECONCILE_DONE],
    )?;
    Ok(())
}

pub fn build_fts_match_query(query: &str) -> Option<String> {
    let normalized = query.nfkc().collect::<String>();
    let terms = normalized
        .split_whitespace()
        .filter(|term| !term.is_empty())
        .take(FTS_QUERY_MAX_TERMS)
        .map(|term| format!("\"{}\"*", term.replace('"', "\"\"")))
        .collect::<Vec<_>>();
    (!terms.is_empty()).then(|| terms.join(" "))
}

fn flatten_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn search_messages(
    db: &Connection,
    query: &str,
    limit: usize,
) -> rusqlite::Result<Vec<MessageSearchResult>> {
    let Some(match_query) = build_fts_match_query(query) else {
        return Ok(Vec::new());
    };
    if limit == 0 {
        return Ok(Vec::new());
    }

    let effective_timestamp = "CASE
      WHEN m.timestamp_ms > 0 THEN m.timestamp_ms
      ELSE COALESCE((SELECT MAX(m2.timestamp_ms) FROM messages m2 WHERE m2.agent_id = m.agent_id), 0)
    END";
    let sql = format!(
        "SELECT
          m.agent_id AS agentId,
          m.entry_id AS entryId,
          m.role AS role,
          matched.ts AS timestampMs,
          snippet(messages_fts, 0, '', '', '…', {SNIPPET_CONTEXT_TOKENS}) AS snippet
        FROM (
          SELECT fts_rowid, ts FROM (
            SELECT fts_rowid, ts,
              ROW_NUMBER() OVER (PARTITION BY agent_id ORDER BY ts DESC) AS agent_rank
            FROM (
              SELECT messages_fts.rowid AS fts_rowid, m.agent_id AS agent_id, {effective_timestamp} AS ts
              FROM messages_fts
              JOIN messages m ON m.id = messages_fts.rowid
              WHERE messages_fts MATCH ?1
            )
          )
          WHERE agent_rank <= {AGENT_CONTENT_SEARCH_MAX_MATCHES_PER_AGENT}
          ORDER BY ts DESC
          LIMIT ?2
        ) AS matched
        JOIN messages_fts ON messages_fts.rowid = matched.fts_rowid
        JOIN messages m ON m.id = matched.fts_rowid
        WHERE messages_fts MATCH ?3
        ORDER BY matched.ts DESC"
    );
    let mut statement = db.prepare(&sql)?;
    let rows = statement.query_map(params![match_query, limit as i64, match_query], |row| {
        let role: String = row.get(2)?;
        Ok(MessageSearchResult {
            agent_id: row.get(0)?,
            entry_id: row.get(1)?,
            role,
            timestamp_ms: row.get(3)?,
            snippet: flatten_whitespace(&row.get::<_, String>(4)?),
        })
    })?;
    rows.collect()
}

pub fn search_media(
    db: &Connection,
    query: &str,
    limit: usize,
) -> rusqlite::Result<Vec<MediaSearchResult>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let columns = "md.agent_id, md.entry_id, md.file_name, md.ext, md.mime, md.kind, md.timestamp_ms, md.width, md.height";
    let match_query = build_fts_match_query(query);
    let sql = if match_query.is_some() {
        format!(
            "SELECT {columns}
             FROM media_fts
             JOIN media md ON md.id = media_fts.rowid
             WHERE media_fts MATCH ?1
             ORDER BY md.timestamp_ms DESC
             LIMIT ?2"
        )
    } else {
        format!(
            "SELECT {columns}
             FROM media md
             ORDER BY md.timestamp_ms DESC
             LIMIT ?1"
        )
    };
    let mut statement = db.prepare(&sql)?;
    let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<MediaSearchResult> {
        let kind: String = row.get(5)?;
        Ok(MediaSearchResult {
            agent_id: row.get(0)?,
            entry_id: row.get(1)?,
            file_name: row.get(2)?,
            ext: row.get(3)?,
            mime: row.get(4)?,
            kind: AttachmentKind::parse(&kind),
            timestamp_ms: row.get(6)?,
            width: row.get(7)?,
            height: row.get(8)?,
        })
    };
    if let Some(match_query) = match_query {
        let rows = statement.query_map(params![match_query, limit as i64], map_row)?;
        rows.collect()
    } else {
        let rows = statement.query_map([limit as i64], map_row)?;
        rows.collect()
    }
}
