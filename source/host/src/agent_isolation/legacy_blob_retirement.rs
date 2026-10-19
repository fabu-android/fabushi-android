use std::path::Path;

use rusqlite::{params, Connection};

use super::agent_worker_pool::LegacyBlobRetirementVerdict;
use super::conversation_blob_db::{
    read_conversation_blob_migration_state, run_quick_check, ConversationBlobMigrationState,
};

pub fn verify_legacy_blob_retirement(
    db: &Connection,
    legacy_blob_db_path: &Path,
    retained_root_id_hex: &str,
) -> LegacyBlobRetirementVerdict {
    if !legacy_blob_db_path.exists() {
        return LegacyBlobRetirementVerdict::defer("legacy-unreadable");
    }

    match run_quick_check(db) {
        Ok(true) => {}
        _ => return LegacyBlobRetirementVerdict::defer("destination-unhealthy"),
    }

    match read_conversation_blob_migration_state(db) {
        Ok(ConversationBlobMigrationState::AdoptionComplete) => {}
        Ok(ConversationBlobMigrationState::Unstarted) => {
            return LegacyBlobRetirementVerdict::defer("adoption-incomplete");
        }
        Ok(ConversationBlobMigrationState::RecoveryRebuilt) => {
            return LegacyBlobRetirementVerdict::defer("recovery-rebuilt");
        }
        _ => return LegacyBlobRetirementVerdict::defer("migration-state-unknown"),
    }

    let root_present = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM blobs WHERE id = ?1)",
            params![retained_root_id_hex],
            |row| row.get::<_, i64>(0),
        )
        .map(|value| value != 0)
        .unwrap_or(false);
    if !root_present {
        return LegacyBlobRetirementVerdict::defer("root-missing");
    }

    let path = legacy_blob_db_path.to_string_lossy().to_string();
    if db
        .execute("ATTACH DATABASE ?1 AS legacy", params![path])
        .is_err()
    {
        return LegacyBlobRetirementVerdict::defer("legacy-unreadable");
    }

    let result = db.query_row(
        "SELECT count(*) AS rows, coalesce(sum(length(data)), 0) AS bytes FROM legacy.blobs",
        [],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
    );
    let _ = db.execute_batch("DETACH DATABASE legacy");

    match result {
        Ok((legacy_rows, legacy_bytes)) => LegacyBlobRetirementVerdict::retirable(
            legacy_rows.max(0) as u64,
            legacy_bytes.max(0) as u64,
        ),
        Err(_) => LegacyBlobRetirementVerdict::defer("legacy-unreadable"),
    }
}
