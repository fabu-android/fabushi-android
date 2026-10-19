pub const WINDOW_ENTRY_FILTER_SQL: &str = r#"json_extract(entry, '$.kind') != 'tool-call'
        AND COALESCE(json_extract(entry, '$.branched'), 0) != 1"#;

pub const BRANCHED_ENTRY_FILTER_SQL: &str =
    r#"COALESCE(json_extract(entry, '$.branched'), 0) = 1"#;

pub const AGENT_DB_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS kv (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
) STRICT;
-- Conversation blobs are owned by the agent's isolation worker in its own
-- conversation-blobs.db; this legacy table remains readable for adoption and salvage.
CREATE TABLE IF NOT EXISTS blobs (
  id TEXT PRIMARY KEY,
  data BLOB NOT NULL
) STRICT;
CREATE TABLE IF NOT EXISTS transcript_entries (
  seq INTEGER PRIMARY KEY,
  id TEXT NOT NULL UNIQUE,
  entry TEXT NOT NULL
) STRICT;
CREATE INDEX IF NOT EXISTS idx_transcript_window
  ON transcript_entries(seq, entry)
  WHERE json_extract(entry, '$.kind') != 'tool-call'
        AND COALESCE(json_extract(entry, '$.branched'), 0) != 1;
CREATE INDEX IF NOT EXISTS idx_transcript_branched
  ON transcript_entries(seq, entry)
  WHERE COALESCE(json_extract(entry, '$.branched'), 0) = 1;
"#;

pub const MAIN_TRANSCRIPT_MESSAGE_FILTER_SQL: &str = r#"
        COALESCE(json_extract(entry, '$.branched'), 0) != 1
        AND (
          json_extract(entry, '$.kind') IN ('send-message', 'user-attachment')
          OR (
            json_extract(entry, '$.kind') = 'message'
            AND (
              json_extract(entry, '$.role') = 'user'
              OR json_extract(entry, '$.fromAgent') IS NOT NULL
              OR json_extract(entry, '$.toAgent') IS NOT NULL
            )
          )
        )"#;

pub const DIVIDER_ANCHOR_ENTRY_FILTER_SQL: &str = r#"
        COALESCE(json_extract(entry, '$.branched'), 0) != 1
        AND (
          json_extract(entry, '$.kind') IN ('send-message', 'user-attachment')
          OR (
            json_extract(entry, '$.kind') = 'message'
            AND (
              json_extract(entry, '$.role') = 'user'
              OR json_extract(entry, '$.fromAgent') IS NOT NULL
              OR json_extract(entry, '$.toAgent') IS NOT NULL
            )
          )
        )
        AND NOT (
          json_extract(entry, '$.kind') = 'user-attachment'
          OR (
            json_extract(entry, '$.kind') = 'message'
            AND json_extract(entry, '$.role') = 'user'
            AND json_extract(entry, '$.fromAgent') IS NULL
            AND json_extract(entry, '$.toAgent') IS NULL
          )
        )"#;

pub const GET_KV_SQL: &str = "SELECT value FROM kv WHERE key = ?1";
pub const SET_KV_SQL: &str =
    "INSERT INTO kv (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value";
pub const COMPARE_AND_SET_KV_SQL: &str =
    "UPDATE kv SET value = ?1 WHERE key = ?2 AND value = ?3";
pub const DELETE_KV_SQL: &str = "DELETE FROM kv WHERE key = ?1";
pub const HAS_LEGACY_BLOB_SQL: &str = "SELECT 1 AS present FROM blobs LIMIT 1";
pub const CLEAR_BLOBS_SQL: &str = "DELETE FROM blobs";
pub const LIST_TRANSCRIPT_ENTRIES_SQL: &str =
    "SELECT entry FROM transcript_entries ORDER BY seq";
pub const NEWEST_DIVIDER_ANCHOR_TIMESTAMP_SQL: &str = r#"
SELECT json_extract(entry, '$.timestampMs') AS timestampMs
FROM transcript_entries
WHERE json_extract(entry, '$.timestampMs') IS NOT NULL
  AND COALESCE(json_extract(entry, '$.branched'), 0) != 1
  AND (
    json_extract(entry, '$.kind') IN ('send-message', 'user-attachment')
    OR (
      json_extract(entry, '$.kind') = 'message'
      AND (
        json_extract(entry, '$.role') = 'user'
        OR json_extract(entry, '$.fromAgent') IS NOT NULL
        OR json_extract(entry, '$.toAgent') IS NOT NULL
      )
    )
  )
  AND NOT (
    json_extract(entry, '$.kind') = 'user-attachment'
    OR (
      json_extract(entry, '$.kind') = 'message'
      AND json_extract(entry, '$.role') = 'user'
      AND json_extract(entry, '$.fromAgent') IS NULL
      AND json_extract(entry, '$.toAgent') IS NULL
    )
  )
ORDER BY seq DESC
LIMIT 1
"#;

pub const LIST_TRANSCRIPT_PAGE_SQL: &str = r#"
SELECT seq, entry
FROM transcript_entries
WHERE (?1 IS NULL OR seq < ?2)
  AND (json_extract(entry, '$.timestampMs') IS NULL OR (?3 IS NULL OR json_extract(entry, '$.timestampMs') >= ?4))
  AND (json_extract(entry, '$.timestampMs') IS NULL OR json_extract(entry, '$.timestampMs') <= ?5)
  AND COALESCE(json_extract(entry, '$.branched'), 0) != 1
  AND (
    json_extract(entry, '$.kind') IN ('send-message', 'user-attachment')
    OR (
      json_extract(entry, '$.kind') = 'message'
      AND (
        json_extract(entry, '$.role') = 'user'
        OR json_extract(entry, '$.fromAgent') IS NOT NULL
        OR json_extract(entry, '$.toAgent') IS NOT NULL
      )
    )
  )
ORDER BY seq DESC
LIMIT ?6
"#;

pub const LIST_TRANSCRIPT_WINDOW_SQL: &str = r#"
SELECT seq, entry
FROM transcript_entries
WHERE (?1 IS NULL OR seq < ?2)
  AND json_extract(entry, '$.kind') != 'tool-call'
  AND COALESCE(json_extract(entry, '$.branched'), 0) != 1
ORDER BY seq DESC
LIMIT ?3
"#;

pub const LIST_TRANSCRIPT_TAIL_SQL: &str = r#"
SELECT seq, entry
FROM transcript_entries
WHERE (?1 IS NULL OR seq < ?2)
ORDER BY seq DESC
LIMIT ?3
"#;

pub const LIST_BRANCHED_ENTRIES_SQL: &str = r#"
SELECT entry
FROM transcript_entries
WHERE COALESCE(json_extract(entry, '$.branched'), 0) = 1
ORDER BY seq
"#;

pub const GET_TRANSCRIPT_ENTRY_SQL: &str =
    "SELECT entry FROM transcript_entries WHERE id = ?1";
pub const INSERT_TRANSCRIPT_ENTRY_SQL: &str =
    "INSERT OR IGNORE INTO transcript_entries (id, entry) VALUES (?1, ?2)";
pub const UPDATE_TRANSCRIPT_ENTRY_SQL: &str =
    "UPDATE transcript_entries SET entry = ?1 WHERE id = ?2";
pub const DELETE_TRANSCRIPT_ENTRY_SQL: &str =
    "DELETE FROM transcript_entries WHERE id = ?1";
pub const CLEAR_TRANSCRIPT_ENTRIES_SQL: &str = "DELETE FROM transcript_entries";

pub const PREPARED_STATEMENT_SQL: &[(&str, &str)] = &[
    ("getKv", GET_KV_SQL),
    ("setKv", SET_KV_SQL),
    ("compareAndSetKv", COMPARE_AND_SET_KV_SQL),
    ("deleteKv", DELETE_KV_SQL),
    ("hasLegacyBlob", HAS_LEGACY_BLOB_SQL),
    ("clearBlobs", CLEAR_BLOBS_SQL),
    ("listTranscriptEntries", LIST_TRANSCRIPT_ENTRIES_SQL),
    ("newestDividerAnchorTimestamp", NEWEST_DIVIDER_ANCHOR_TIMESTAMP_SQL),
    ("listTranscriptPage", LIST_TRANSCRIPT_PAGE_SQL),
    ("listTranscriptWindow", LIST_TRANSCRIPT_WINDOW_SQL),
    ("listTranscriptTail", LIST_TRANSCRIPT_TAIL_SQL),
    ("listBranchedEntries", LIST_BRANCHED_ENTRIES_SQL),
    ("getTranscriptEntry", GET_TRANSCRIPT_ENTRY_SQL),
    ("insertTranscriptEntry", INSERT_TRANSCRIPT_ENTRY_SQL),
    ("updateTranscriptEntry", UPDATE_TRANSCRIPT_ENTRY_SQL),
    ("deleteTranscriptEntry", DELETE_TRANSCRIPT_ENTRY_SQL),
    ("clearTranscriptEntries", CLEAR_TRANSCRIPT_ENTRIES_SQL),
];
