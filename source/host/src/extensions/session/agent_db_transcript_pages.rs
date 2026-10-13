use rusqlite::{Connection, params};
use serde_json::Value;

use super::agent_db_schema::{
    LIST_TRANSCRIPT_PAGE_SQL, LIST_TRANSCRIPT_TAIL_SQL, LIST_TRANSCRIPT_WINDOW_SQL,
};
use super::agent_db_serde::parse_transcript_entry;

pub const DEFAULT_TRANSCRIPT_WINDOW_LIMIT: i64 = 500;
pub const MAX_TRANSCRIPT_WINDOW_LIMIT: i64 = 5_000;

#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptPage {
    pub entries: Vec<Value>,
    pub next_before_seq: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptWindow<T> {
    pub entries: Vec<Value>,
    pub next_before_seq: Option<i64>,
    pub thread_counts: T,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TranscriptPageQuery {
    pub before_seq: Option<i64>,
    pub since_ms: Option<i64>,
    pub until_ms: i64,
    pub limit: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TranscriptWindowQuery {
    pub before_seq: Option<i64>,
    pub limit: i64,
}

#[derive(Debug)]
struct TranscriptRow {
    seq: i64,
    entry: String,
}

fn page(rows: Vec<TranscriptRow>, limit: usize) -> TranscriptPage {
    let has_more = rows.len() > limit;
    let selected = rows.into_iter().take(limit).collect::<Vec<_>>();
    let oldest_seq = selected.last().map(|row| row.seq);
    let entries = selected
        .iter()
        .rev()
        .filter_map(|row| parse_transcript_entry(&row.entry))
        .collect();
    TranscriptPage {
        entries,
        next_before_seq: if has_more { oldest_seq } else { None },
    }
}

fn sanitize_limit(limit: i64) -> usize {
    if limit > 0 {
        limit.min(MAX_TRANSCRIPT_WINDOW_LIMIT) as usize
    } else {
        DEFAULT_TRANSCRIPT_WINDOW_LIMIT as usize
    }
}

pub fn read_transcript_page(
    db: &Connection,
    query: TranscriptPageQuery,
) -> Result<TranscriptPage, rusqlite::Error> {
    let query_limit = query.limit.saturating_add(1);
    let mut statement = db.prepare(LIST_TRANSCRIPT_PAGE_SQL)?;
    let rows = statement
        .query_map(
            params![
                query.before_seq,
                query.before_seq,
                query.since_ms,
                query.since_ms,
                query.until_ms,
                query_limit,
            ],
            |row| {
                Ok(TranscriptRow {
                    seq: row.get(0)?,
                    entry: row.get(1)?,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let limit = if query.limit > 0 {
        query.limit as usize
    } else {
        0
    };
    Ok(page(rows, limit))
}

pub fn read_transcript_window<T, F>(
    db: &Connection,
    query: TranscriptWindowQuery,
    thread_counts_for: F,
) -> Result<TranscriptWindow<T>, rusqlite::Error>
where
    F: FnOnce(&[Value]) -> T,
{
    let limit = sanitize_limit(query.limit);
    let mut statement = db.prepare(LIST_TRANSCRIPT_WINDOW_SQL)?;
    let rows = statement
        .query_map(
            params![
                query.before_seq,
                query.before_seq,
                (limit + 1) as i64,
            ],
            |row| {
                Ok(TranscriptRow {
                    seq: row.get(0)?,
                    entry: row.get(1)?,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let result = page(rows, limit);
    let thread_counts = thread_counts_for(&result.entries);
    Ok(TranscriptWindow {
        entries: result.entries,
        next_before_seq: result.next_before_seq,
        thread_counts,
    })
}

pub fn read_transcript_tail(
    db: &Connection,
    query: TranscriptWindowQuery,
) -> Result<TranscriptPage, rusqlite::Error> {
    let limit = sanitize_limit(query.limit);
    let mut statement = db.prepare(LIST_TRANSCRIPT_TAIL_SQL)?;
    let rows = statement
        .query_map(
            params![
                query.before_seq,
                query.before_seq,
                (limit + 1) as i64,
            ],
            |row| {
                Ok(TranscriptRow {
                    seq: row.get(0)?,
                    entry: row.get(1)?,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(page(rows, limit))
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::session::agent_db_schema::{
        AGENT_DB_SCHEMA, INSERT_TRANSCRIPT_ENTRY_SQL,
    };

    fn transcript_db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(AGENT_DB_SCHEMA).unwrap();
        for (id, content) in [("m1", "one"), ("m2", "two"), ("m3", "three")] {
            let entry = serde_json::json!({"kind":"message","id":id,"content":content});
            db.execute(
                INSERT_TRANSCRIPT_ENTRY_SQL,
                params![id, entry.to_string()],
            )
            .unwrap();
        }
        db
    }

    #[test]
    fn transcript_page_is_bounded_oldest_to_newest_and_resumable() {
        let db = transcript_db();
        let first = read_transcript_page(
            &db,
            TranscriptPageQuery {
                before_seq: None,
                since_ms: None,
                until_ms: i64::MAX,
                limit: 2,
            },
        )
        .unwrap();
        assert_eq!(first.entries.len(), 2);
        assert_eq!(first.entries[0]["content"], "two");
        assert_eq!(first.entries[1]["content"], "three");

        let cursor = first.next_before_seq.expect("three rows require a cursor");
        let second = read_transcript_page(
            &db,
            TranscriptPageQuery {
                before_seq: Some(cursor),
                since_ms: None,
                until_ms: i64::MAX,
                limit: 2,
            },
        )
        .unwrap();
        assert_eq!(second.entries.len(), 1);
        assert_eq!(second.entries[0]["content"], "one");
        assert_eq!(second.next_before_seq, None);
    }

    #[test]
    fn transcript_window_sanitizes_non_positive_limit() {
        let db = transcript_db();
        let window = read_transcript_window(
            &db,
            TranscriptWindowQuery {
                before_seq: None,
                limit: 0,
            },
            |entries| entries.len(),
        )
        .unwrap();
        assert_eq!(window.entries.len(), 3);
        assert_eq!(window.thread_counts, 3);
        assert_eq!(window.next_before_seq, None);
    }
}
