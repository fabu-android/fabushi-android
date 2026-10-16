//! Transactional install receipts in the existing injected Host SQLite owner.
//! No new database, account identity, user-data store or execution API.
use super::VerifiedPackage;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostInstallScope {
    /// Supplied by current authenticated Host account lifecycle, never renderer.
    pub account_scope: String,
    pub generation: u64,
}
#[derive(Debug, Error)]
pub enum InstallError {
    #[error("install request belongs to a departed account/session")]
    StaleScope,
    #[error("immutable package version or operation identity conflict")]
    Conflict,
    #[error("package update is not a monotonic stable release")]
    VersionPolicy,
    #[error("installed code cache quota exceeded")]
    Quota,
    #[error("signed trust snapshot rollback was rejected")]
    TrustRollback,
    #[error("Host package storage operation failed")]
    Storage(#[from] rusqlite::Error),
}
#[derive(Debug, PartialEq, Eq)]
pub struct InstalledRecord {
    pub app_id: String,
    pub version: String,
    pub digest: String,
    pub previous_digest: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledListing {
    pub app_id: String,
    pub version: String,
    pub digest: String,
    pub publisher_id: String,
    pub size_bytes: u64,
}
pub fn initialize(db: &Connection) -> Result<(), InstallError> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS host_miniapp_packages (
        app_id TEXT NOT NULL, version TEXT NOT NULL, digest TEXT PRIMARY KEY,
        publisher_id TEXT NOT NULL, trust_epoch INTEGER NOT NULL, archive BLOB NOT NULL,
        UNIQUE(app_id,version));
        CREATE TABLE IF NOT EXISTS host_miniapp_installs (
        account_scope TEXT NOT NULL, app_id TEXT NOT NULL, digest TEXT NOT NULL,
        previous_digest TEXT, PRIMARY KEY(account_scope,app_id));
        CREATE TABLE IF NOT EXISTS host_miniapp_operations (
        account_scope TEXT NOT NULL, operation_id TEXT NOT NULL, app_id TEXT NOT NULL,
        digest TEXT NOT NULL, PRIMARY KEY(account_scope,operation_id));
        CREATE TABLE IF NOT EXISTS host_miniapp_removals (
        account_scope TEXT NOT NULL, operation_id TEXT NOT NULL, app_id TEXT NOT NULL,
        removed INTEGER NOT NULL, PRIMARY KEY(account_scope,operation_id));
        CREATE TABLE IF NOT EXISTS host_miniapp_authorizations (
        digest TEXT PRIMARY KEY, proof_json TEXT NOT NULL, verified_at_ms INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS host_miniapp_quarantine (
        account_scope TEXT NOT NULL, app_id TEXT NOT NULL, reason TEXT NOT NULL,
        PRIMARY KEY(account_scope,app_id));
        CREATE TABLE IF NOT EXISTS host_miniapp_trust_state (
        singleton INTEGER PRIMARY KEY CHECK(singleton=1), trust_epoch INTEGER NOT NULL,
        issued_at_ms INTEGER NOT NULL, signed_payload TEXT NOT NULL, envelope_json TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS host_miniapp_native_account (
        singleton INTEGER PRIMARY KEY CHECK(singleton=1), account_scope_hash TEXT NOT NULL,
        session_binding_hash TEXT NOT NULL, generation INTEGER NOT NULL, expires_at_ms INTEGER NOT NULL);",
    )?;
    Ok(())
}
fn stable_version(version: &str) -> Option<(u64, u64, u64)> {
    let values: Vec<_> = version.split('.').collect();
    if values.len() != 3 {
        return None;
    }
    Some((
        values[0].parse().ok()?,
        values[1].parse().ok()?,
        values[2].parse().ok()?,
    ))
}
pub fn installed(
    db: &Connection,
    current: &HostInstallScope,
    app_id: &str,
) -> Result<Option<InstalledRecord>, InstallError> {
    Ok(db
        .query_row(
            "SELECT p.app_id,p.version,p.digest,i.previous_digest
        FROM host_miniapp_installs i JOIN host_miniapp_packages p ON p.digest=i.digest
        WHERE i.account_scope=?1 AND i.app_id=?2",
            params![current.account_scope, app_id],
            |r| {
                Ok(InstalledRecord {
                    app_id: r.get(0)?,
                    version: r.get(1)?,
                    digest: r.get(2)?,
                    previous_digest: r.get(3)?,
                })
            },
        )
        .optional()?)
}

/// Enumerate only the current authenticated account's durable install pointers.
/// This never falls back to process memory and does not claim the archive is
/// launchable; cold-start callers must reverify it against current policy.
pub fn list_installed(
    db: &Connection,
    current: &HostInstallScope,
) -> Result<Vec<InstalledListing>, InstallError> {
    if current.account_scope.is_empty() || current.generation == 0 {
        return Err(InstallError::StaleScope);
    }
    let mut statement = db.prepare(
        "SELECT p.app_id,p.version,p.digest,p.publisher_id,length(p.archive)
         FROM host_miniapp_installs i
         JOIN host_miniapp_packages p ON p.digest=i.digest
         WHERE i.account_scope=?1 ORDER BY p.app_id",
    )?;
    let rows = statement.query_map([&current.account_scope], |row| {
        let size: i64 = row.get(4)?;
        Ok(InstalledListing {
            app_id: row.get(0)?,
            version: row.get(1)?,
            digest: row.get(2)?,
            publisher_id: row.get(3)?,
            size_bytes: u64::try_from(size).unwrap_or_default(),
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(InstallError::Storage)
}
/// Serializes activation with code bytes and operation receipt. Failure before
/// commit leaves both active pointer and last-good archive unchanged.
pub fn activate(
    db: &mut Connection,
    current: &HostInstallScope,
    intent: &HostInstallScope,
    operation_id: &str,
    package: VerifiedPackage,
) -> Result<InstalledRecord, InstallError> {
    activate_with_snapshot(db, current, intent, operation_id, package, None, || true)
}

pub fn activate_authorized(
    db: &mut Connection,
    current: &HostInstallScope,
    intent: &HostInstallScope,
    operation_id: &str,
    package: VerifiedPackage,
    proof_snapshot: &str,
    verified_at_ms: u64,
) -> Result<InstalledRecord, InstallError> {
    if proof_snapshot.is_empty() || verified_at_ms == 0 {
        return Err(InstallError::Conflict);
    }
    activate_with_snapshot(
        db,
        current,
        intent,
        operation_id,
        package,
        Some((proof_snapshot, verified_at_ms)),
        || true,
    )
}

pub fn activate_authorized_fenced<F>(
    db: &mut Connection,
    current: &HostInstallScope,
    intent: &HostInstallScope,
    operation_id: &str,
    package: VerifiedPackage,
    proof_snapshot: &str,
    verified_at_ms: u64,
    fence: F,
) -> Result<InstalledRecord, InstallError>
where
    F: FnOnce() -> bool,
{
    if proof_snapshot.is_empty() || verified_at_ms == 0 {
        return Err(InstallError::Conflict);
    }
    activate_with_snapshot(
        db,
        current,
        intent,
        operation_id,
        package,
        Some((proof_snapshot, verified_at_ms)),
        fence,
    )
}

fn activate_with_snapshot(
    db: &mut Connection,
    current: &HostInstallScope,
    intent: &HostInstallScope,
    operation_id: &str,
    package: VerifiedPackage,
    proof_snapshot: Option<(&str, u64)>,
    fence: impl FnOnce() -> bool,
) -> Result<InstalledRecord, InstallError> {
    if current != intent
        || current.generation == 0
        || current.account_scope.is_empty()
        || operation_id.is_empty()
        || operation_id.len() > 128
    {
        return Err(InstallError::StaleScope);
    }
    let new_version = stable_version(package.version()).ok_or(InstallError::VersionPolicy)?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some((snapshot, _)) = proof_snapshot {
        let proof: serde_json::Value =
            serde_json::from_str(snapshot).map_err(|_| InstallError::Conflict)?;
        let trust = proof.get("trust").ok_or(InstallError::Conflict)?;
        let epoch = trust
            .get("trustEpoch")
            .and_then(serde_json::Value::as_u64)
            .ok_or(InstallError::Conflict)?;
        let issued_at_ms = trust
            .get("issuedAtMs")
            .and_then(serde_json::Value::as_u64)
            .ok_or(InstallError::Conflict)?;
        let signed_payload = trust
            .get("payload")
            .and_then(serde_json::Value::as_str)
            .ok_or(InstallError::Conflict)?;
        let envelope_json = serde_json::to_string(trust).map_err(|_| InstallError::Conflict)?;
        let previous: Option<(i64, i64, String)> = tx.query_row(
            "SELECT trust_epoch,issued_at_ms,signed_payload FROM host_miniapp_trust_state WHERE singleton=1",
            [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).optional()?;
        if let Some((previous_epoch, previous_issued, previous_payload)) = previous {
            if i64::try_from(epoch).map_err(|_| InstallError::TrustRollback)? < previous_epoch
                || (i64::try_from(epoch).map_err(|_| InstallError::TrustRollback)?
                    == previous_epoch
                    && (i64::try_from(issued_at_ms).map_err(|_| InstallError::TrustRollback)?
                        < previous_issued
                        || (i64::try_from(issued_at_ms)
                            .map_err(|_| InstallError::TrustRollback)?
                            == previous_issued
                            && signed_payload != previous_payload)))
            {
                return Err(InstallError::TrustRollback);
            }
        }
        tx.execute(
            "INSERT INTO host_miniapp_trust_state(singleton,trust_epoch,issued_at_ms,signed_payload,envelope_json)
             VALUES(1,?1,?2,?3,?4) ON CONFLICT(singleton) DO UPDATE SET
             trust_epoch=excluded.trust_epoch,issued_at_ms=excluded.issued_at_ms,
             signed_payload=excluded.signed_payload,envelope_json=excluded.envelope_json
             WHERE excluded.trust_epoch > host_miniapp_trust_state.trust_epoch
                OR excluded.issued_at_ms > host_miniapp_trust_state.issued_at_ms",
            params![i64::try_from(epoch).map_err(|_| InstallError::TrustRollback)?,
                i64::try_from(issued_at_ms).map_err(|_| InstallError::TrustRollback)?, signed_payload, envelope_json],
        )?;
    }
    let receipt: Option<(String, String)> = tx
        .query_row(
            "SELECT app_id,digest FROM host_miniapp_operations
        WHERE account_scope=?1 AND operation_id=?2",
            params![current.account_scope, operation_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((app, digest)) = receipt {
        if app != package.app_id() || digest != package.digest() {
            return Err(InstallError::Conflict);
        }
        // An old receipt must never reactivate an older package after an update.
        if !fence() {
            return Err(InstallError::StaleScope);
        }
        return installed(&tx, current, &app)?.ok_or(InstallError::Conflict);
    }
    let existing: Option<String> = tx
        .query_row(
            "SELECT digest FROM host_miniapp_packages WHERE app_id=?1 AND version=?2",
            params![package.app_id(), package.version()],
            |r| r.get(0),
        )
        .optional()?;
    if existing
        .as_ref()
        .is_some_and(|value| value != package.digest())
    {
        return Err(InstallError::Conflict);
    }
    let old = installed(&tx, current, package.app_id())?;
    if let Some(old) = &old {
        if old.digest != package.digest()
            && stable_version(&old.version).is_none_or(|v| new_version <= v)
        {
            return Err(InstallError::VersionPolicy);
        }
    }
    let used: i64 = tx.query_row(
        "SELECT COALESCE(SUM(length(archive)),0) FROM host_miniapp_packages",
        [],
        |r| r.get(0),
    )?;
    if used < 0
        || (existing.is_none()
            && (used as u64).saturating_add(package.archive.len() as u64) > 500 * 1024 * 1024)
    {
        return Err(InstallError::Quota);
    }
    let previous = old.as_ref().and_then(|old| {
        if old.digest == package.digest() {
            old.previous_digest.clone()
        } else {
            Some(old.digest.clone())
        }
    });
    let epoch = i64::try_from(package.trust_epoch).map_err(|_| InstallError::Conflict)?;
    tx.execute("INSERT OR IGNORE INTO host_miniapp_packages(app_id,version,digest,publisher_id,trust_epoch,archive)
        VALUES(?1,?2,?3,?4,?5,?6)",params![package.app_id(),package.version(),package.digest(),package.publisher_id(),epoch,package.archive])?;
    if let Some((snapshot, verified_at_ms)) = proof_snapshot {
        let verified_at_ms = i64::try_from(verified_at_ms).map_err(|_| InstallError::Conflict)?;
        tx.execute(
            "INSERT INTO host_miniapp_authorizations(digest,proof_json,verified_at_ms) VALUES(?1,?2,?3)
             ON CONFLICT(digest) DO UPDATE SET proof_json=excluded.proof_json,verified_at_ms=excluded.verified_at_ms",
            params![package.digest(), snapshot, verified_at_ms],
        )?;
    }
    tx.execute("INSERT INTO host_miniapp_installs(account_scope,app_id,digest,previous_digest) VALUES(?1,?2,?3,?4)
        ON CONFLICT(account_scope,app_id) DO UPDATE SET digest=excluded.digest,previous_digest=excluded.previous_digest",
        params![current.account_scope,package.app_id(),package.digest(),previous])?;
    tx.execute("INSERT INTO host_miniapp_operations(account_scope,operation_id,app_id,digest) VALUES(?1,?2,?3,?4)",
        params![current.account_scope,operation_id,package.app_id(),package.digest()])?;
    tx.execute(
        "DELETE FROM host_miniapp_quarantine WHERE account_scope=?1 AND app_id=?2",
        params![current.account_scope, package.app_id()],
    )?;
    let record = installed(&tx, current, package.app_id())?.ok_or(InstallError::Conflict)?;
    if !fence() {
        return Err(InstallError::StaleScope);
    }
    tx.commit()?;
    Ok(record)
}
/// Bytes remain inert; the native launch path must reverify current catalog
/// policy with verify_catalog_bundle before serving any resource.
pub fn load_active_archive(
    db: &Connection,
    current: &HostInstallScope,
    app_id: &str,
) -> Result<Option<Vec<u8>>, InstallError> {
    Ok(db.query_row("SELECT p.archive FROM host_miniapp_installs i JOIN host_miniapp_packages p ON p.digest=i.digest
        WHERE i.account_scope=?1 AND i.app_id=?2",params![current.account_scope,app_id],|r|r.get(0)).optional()?)
}

pub fn load_active_authorized_archive(
    db: &Connection,
    current: &HostInstallScope,
    app_id: &str,
) -> Result<Option<(Vec<u8>, String, Option<String>)>, InstallError> {
    Ok(db
        .query_row(
            "SELECT p.archive,a.proof_json,t.envelope_json FROM host_miniapp_installs i
         JOIN host_miniapp_packages p ON p.digest=i.digest
         JOIN host_miniapp_authorizations a ON a.digest=p.digest
         LEFT JOIN host_miniapp_trust_state t ON t.singleton=1
         WHERE i.account_scope=?1 AND i.app_id=?2",
            params![current.account_scope, app_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?)
}

pub fn current_trust_envelope(db: &Connection) -> Result<Option<String>, InstallError> {
    Ok(db
        .query_row(
            "SELECT envelope_json FROM host_miniapp_trust_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .optional()?)
}

/// Persist a cryptographically verified independent feed before any release
/// proof lookup. This transaction is intentionally separate from activation so
/// a subsequent 404 cannot roll back a newly learned revocation.
pub fn cache_verified_trust(
    db: &mut Connection,
    epoch: u64,
    issued_at_ms: u64,
    signed_payload: &str,
    envelope_json: &str,
) -> Result<(), InstallError> {
    if epoch == 0 || issued_at_ms == 0 || signed_payload.is_empty() || envelope_json.is_empty() {
        return Err(InstallError::Conflict);
    }
    let epoch = i64::try_from(epoch).map_err(|_| InstallError::TrustRollback)?;
    let issued_at_ms = i64::try_from(issued_at_ms).map_err(|_| InstallError::TrustRollback)?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let previous: Option<(i64, i64, String)> = tx.query_row(
        "SELECT trust_epoch,issued_at_ms,signed_payload FROM host_miniapp_trust_state WHERE singleton=1",
        [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).optional()?;
    if let Some((old_epoch, old_issued, old_payload)) = previous {
        if epoch < old_epoch
            || (epoch == old_epoch
                && (issued_at_ms < old_issued
                    || (issued_at_ms == old_issued && signed_payload != old_payload)))
        {
            return Err(InstallError::TrustRollback);
        }
    }
    tx.execute(
        "INSERT INTO host_miniapp_trust_state(singleton,trust_epoch,issued_at_ms,signed_payload,envelope_json)
         VALUES(1,?1,?2,?3,?4) ON CONFLICT(singleton) DO UPDATE SET
         trust_epoch=excluded.trust_epoch,issued_at_ms=excluded.issued_at_ms,
         signed_payload=excluded.signed_payload,envelope_json=excluded.envelope_json
         WHERE excluded.trust_epoch > host_miniapp_trust_state.trust_epoch
            OR excluded.issued_at_ms > host_miniapp_trust_state.issued_at_ms",
        params![epoch, issued_at_ms, signed_payload, envelope_json],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn verified_native_account(
    db: &Connection,
    account_scope_hash: &str,
    session_binding_hash: &str,
    expires_at_ms: u64,
    now_ms: u64,
) -> Result<HostInstallScope, InstallError> {
    if expires_at_ms <= now_ms {
        return Err(InstallError::StaleScope);
    }
    let previous: Option<(String, String, i64)> = db.query_row(
        "SELECT account_scope_hash,session_binding_hash,generation FROM host_miniapp_native_account WHERE singleton=1",
        [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).optional()?;
    let generation = match previous {
        Some((scope, binding, generation))
            if scope == account_scope_hash && binding == session_binding_hash =>
        {
            u64::try_from(generation).map_err(|_| InstallError::StaleScope)?
        }
        Some((_, _, generation)) => u64::try_from(generation)
            .map_err(|_| InstallError::StaleScope)?
            .checked_add(1)
            .ok_or(InstallError::StaleScope)?,
        None => 1,
    };
    db.execute(
        "INSERT INTO host_miniapp_native_account(singleton,account_scope_hash,session_binding_hash,generation,expires_at_ms)
         VALUES(1,?1,?2,?3,?4) ON CONFLICT(singleton) DO UPDATE SET
         account_scope_hash=excluded.account_scope_hash,session_binding_hash=excluded.session_binding_hash,
         generation=excluded.generation,expires_at_ms=excluded.expires_at_ms",
        params![account_scope_hash, session_binding_hash,
            i64::try_from(generation).map_err(|_| InstallError::StaleScope)?,
            i64::try_from(expires_at_ms).map_err(|_| InstallError::StaleScope)?],
    )?;
    Ok(HostInstallScope {
        account_scope: account_scope_hash.into(),
        generation,
    })
}

pub fn cached_native_account(
    db: &Connection,
    account_scope_hash: &str,
    session_binding_hash: &str,
    now_ms: u64,
) -> Result<Option<HostInstallScope>, InstallError> {
    let cached: Option<(String, String, i64, i64)> = db.query_row(
        "SELECT account_scope_hash,session_binding_hash,generation,expires_at_ms FROM host_miniapp_native_account WHERE singleton=1",
        [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    ).optional()?;
    match cached {
        Some((scope, binding, generation, expires))
            if scope == account_scope_hash
                && binding == session_binding_hash
                && u64::try_from(expires).is_ok_and(|expires| expires > now_ms) =>
        {
            Ok(Some(HostInstallScope {
                account_scope: scope,
                generation: u64::try_from(generation).map_err(|_| InstallError::StaleScope)?,
            }))
        }
        _ => Ok(None),
    }
}

pub fn clear_cached_native_account(db: &Connection) -> Result<(), InstallError> {
    db.execute(
        "DELETE FROM host_miniapp_native_account WHERE singleton=1",
        [],
    )?;
    Ok(())
}

pub fn set_quarantined(
    db: &Connection,
    current: &HostInstallScope,
    app_id: &str,
    reason: Option<&str>,
) -> Result<(), InstallError> {
    match reason {
        Some(reason) => {
            db.execute(
                "INSERT INTO host_miniapp_quarantine(account_scope,app_id,reason) VALUES(?1,?2,?3)
                 ON CONFLICT(account_scope,app_id) DO UPDATE SET reason=excluded.reason",
                params![current.account_scope, app_id, reason],
            )?;
        }
        None => {
            db.execute(
                "DELETE FROM host_miniapp_quarantine WHERE account_scope=?1 AND app_id=?2",
                params![current.account_scope, app_id],
            )?;
        }
    }
    Ok(())
}

pub fn quarantine_reason(
    db: &Connection,
    current: &HostInstallScope,
    app_id: &str,
) -> Result<Option<String>, InstallError> {
    Ok(db
        .query_row(
            "SELECT reason FROM host_miniapp_quarantine WHERE account_scope=?1 AND app_id=?2",
            params![current.account_scope, app_id],
            |row| row.get(0),
        )
        .optional()?)
}
/// Code-only removal. Per-app browser user data remains with the native data
/// owner and requires a separately explicit retain/delete UI intent.
pub fn remove(
    db: &mut Connection,
    current: &HostInstallScope,
    intent: &HostInstallScope,
    app_id: &str,
    operation_id: &str,
) -> Result<bool, InstallError> {
    remove_fenced(db, current, intent, app_id, operation_id, || true)
}

pub fn remove_fenced<F>(
    db: &mut Connection,
    current: &HostInstallScope,
    intent: &HostInstallScope,
    app_id: &str,
    operation_id: &str,
    fence: F,
) -> Result<bool, InstallError>
where
    F: FnOnce() -> bool,
{
    if current != intent
        || current.generation == 0
        || operation_id.is_empty()
        || operation_id.len() > 128
    {
        return Err(InstallError::StaleScope);
    }
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let receipt: Option<(String, bool)> = tx.query_row(
        "SELECT app_id,removed FROM host_miniapp_removals WHERE account_scope=?1 AND operation_id=?2",
        params![current.account_scope, operation_id],
        |row| Ok((row.get(0)?, row.get::<_, i64>(1)? != 0)),
    ).optional()?;
    if let Some((receipt_app, removed)) = receipt {
        if receipt_app != app_id {
            return Err(InstallError::Conflict);
        }
        if !fence() {
            return Err(InstallError::StaleScope);
        }
        return Ok(removed);
    }
    let removed = tx.execute(
        "DELETE FROM host_miniapp_installs WHERE account_scope=?1 AND app_id=?2",
        params![current.account_scope, app_id],
    )? > 0;
    tx.execute(
        "DELETE FROM host_miniapp_quarantine WHERE account_scope=?1 AND app_id=?2",
        params![current.account_scope, app_id],
    )?;
    tx.execute(
        "INSERT INTO host_miniapp_removals(account_scope,operation_id,app_id,removed) VALUES(?1,?2,?3,?4)",
        params![current.account_scope, operation_id, app_id, if removed { 1_i64 } else { 0_i64 }],
    )?;
    if !fence() {
        return Err(InstallError::StaleScope);
    }
    tx.commit()?;
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::mcp::installable_package::{
        tests::{HTML, fixture},
        verify_catalog_bundle,
    };
    fn package(version: &str, title: &str) -> VerifiedPackage {
        let (bytes, mut policy) = fixture(
            |m| {
                m["version"] = version.into();
                m["displayName"] = title.into();
            },
            HTML,
        );
        policy.version = version.into();
        verify_catalog_bundle(&bytes, &policy).unwrap()
    }
    fn scope(account: &str) -> HostInstallScope {
        HostInstallScope {
            account_scope: account.into(),
            generation: 1,
        }
    }
    #[test]
    fn activation_update_retry_and_account_isolation() {
        let mut db = Connection::open_in_memory().unwrap();
        initialize(&db).unwrap();
        let s = scope("a");
        let first = activate(&mut db, &s, &s, "one", package("1.0.0", "one")).unwrap();
        assert!(
            installed(&db, &scope("b"), &first.app_id)
                .unwrap()
                .is_none()
        );
        let next = activate(&mut db, &s, &s, "two", package("1.1.0", "two")).unwrap();
        assert_eq!(next.previous_digest, Some(first.digest));
        assert_eq!(
            activate(&mut db, &s, &s, "one", package("1.0.0", "one")).unwrap(),
            next
        );
        assert!(matches!(
            activate(
                &mut db,
                &s,
                &s,
                "changed",
                package("1.1.0", "different bytes")
            ),
            Err(InstallError::Conflict)
        ));
        assert!(
            load_active_archive(&db, &scope("b"), &next.app_id)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn installed_listing_includes_local_archive_size_without_catalog_access() {
        let mut db = Connection::open_in_memory().unwrap();
        initialize(&db).unwrap();
        let current = scope("native-account-hash");
        let package = package("1.0.0", "offline display metadata");
        let expected_size = package.archive.len() as u64;
        activate(&mut db, &current, &current, "offline-list-entry", package).unwrap();

        let entries = list_installed(&db, &current).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].app_id, "org.test.app");
        assert_eq!(entries[0].version, "1.0.0");
        assert_eq!(entries[0].publisher_id, "test-publisher");
        assert_eq!(entries[0].size_bytes, expected_size);
    }

    #[test]
    fn refreshed_trust_survives_exact_release_404_quarantine_and_blocks_offline_retry() {
        let mut db = Connection::open_in_memory().unwrap();
        initialize(&db).unwrap();
        let current = scope("account-a");
        let payload = r#"{"signed":"feed"}"#;
        let envelope = r#"{"protocol":"fabushi.extension-trust-feed.v1"}"#;
        cache_verified_trust(&mut db, 2, 50, payload, envelope).unwrap();
        // This models the exact release proof returning HTTP 404 after the
        // feed has committed; quarantine is an independent durable write.
        set_quarantined(&db, &current, "org.test.app", Some("release_revoked")).unwrap();
        assert_eq!(
            current_trust_envelope(&db).unwrap().as_deref(),
            Some(envelope)
        );
        assert_eq!(
            quarantine_reason(&db, &current, "org.test.app")
                .unwrap()
                .as_deref(),
            Some("release_revoked")
        );
        assert!(matches!(
            cache_verified_trust(&mut db, 1, 100, "older", "older-envelope"),
            Err(InstallError::TrustRollback)
        ));
        assert_eq!(
            current_trust_envelope(&db).unwrap().as_deref(),
            Some(envelope)
        );
    }
    #[test]
    fn rollback_on_storage_failure_preserves_last_good() {
        let mut db = Connection::open_in_memory().unwrap();
        initialize(&db).unwrap();
        let s = scope("a");
        let first = activate(&mut db, &s, &s, "one", package("1.0.0", "one")).unwrap();
        db.execute_batch("CREATE TRIGGER stop_receipt BEFORE INSERT ON host_miniapp_operations BEGIN SELECT RAISE(ABORT,'fault'); END;").unwrap();
        assert!(activate(&mut db, &s, &s, "two", package("1.1.0", "two")).is_err());
        assert_eq!(installed(&db, &s, &first.app_id).unwrap(), Some(first));
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM host_miniapp_packages", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn stale_generation_and_code_removal() {
        let mut db = Connection::open_in_memory().unwrap();
        initialize(&db).unwrap();
        let s = scope("a");
        let mut stale = s.clone();
        stale.generation = 0;
        assert!(matches!(
            activate(&mut db, &s, &stale, "one", package("1.0.0", "one")),
            Err(InstallError::StaleScope)
        ));
        let first = activate(&mut db, &s, &s, "one", package("1.0.0", "one")).unwrap();
        assert!(remove(&mut db, &s, &s, &first.app_id, "remove-1").unwrap());
        assert!(remove(&mut db, &s, &s, &first.app_id, "remove-1").unwrap());
        assert!(!remove(&mut db, &s, &s, &first.app_id, "remove-2").unwrap());
        assert!(installed(&db, &s, &first.app_id).unwrap().is_none());
    }

    #[test]
    fn stable_0_1_0_is_installable_even_when_catalog_rollout_is_preview() {
        // Channel is Registry rollout metadata, not SemVer prerelease syntax.
        // The Host ledger accepts the exact stable version 0.1.0 and rejects
        // prerelease versions through the same stable_version gate.
        let mut db = Connection::open_in_memory().unwrap();
        initialize(&db).unwrap();
        let s = scope("a");
        let installed = activate(
            &mut db,
            &s,
            &s,
            "stable-preview-channel-op",
            package("0.1.0", "preview rollout, stable semver"),
        )
        .unwrap();
        assert_eq!(installed.version, "0.1.0");
        assert!(matches!(
            activate(
                &mut db,
                &s,
                &s,
                "prerelease",
                package("0.2.0-preview.1", "pre")
            ),
            Err(InstallError::VersionPolicy)
        ));
    }

    #[test]
    fn account_generation_fence_rolls_back_activation_and_removal() {
        let mut db = Connection::open_in_memory().unwrap();
        initialize(&db).unwrap();
        let s = scope("a");
        let rejected = activate_authorized_fenced(
            &mut db,
            &s,
            &s,
            "fenced-install",
            package("1.0.0", "fenced"),
            r#"{"trust":{"trustEpoch":1,"issuedAtMs":1,"payload":"signed-feed-fixture"}}"#,
            1,
            || false,
        )
        .err()
        .unwrap();
        assert!(matches!(rejected, InstallError::StaleScope));
        assert!(installed(&db, &s, "org.test.app").unwrap().is_none());
        assert!(current_trust_envelope(&db).unwrap().is_none());

        let active = activate(&mut db, &s, &s, "install", package("1.0.0", "active")).unwrap();
        let rejected = remove_fenced(&mut db, &s, &s, &active.app_id, "fenced-remove", || false)
            .err()
            .unwrap();
        assert!(matches!(rejected, InstallError::StaleScope));
        assert!(installed(&db, &s, &active.app_id).unwrap().is_some());
    }

    #[test]
    fn native_verified_scope_advances_on_account_or_session_change_and_offline_cache_is_bound() {
        let db = Connection::open_in_memory().unwrap();
        initialize(&db).unwrap();
        let first =
            verified_native_account(&db, "user-hash-a", "session-hash-a", 10_000, 1).unwrap();
        assert_eq!(first.generation, 1);
        assert_eq!(
            cached_native_account(&db, "user-hash-a", "session-hash-a", 2).unwrap(),
            Some(first.clone())
        );
        assert!(
            cached_native_account(&db, "user-hash-a", "other-session", 2)
                .unwrap()
                .is_none()
        );
        assert!(
            cached_native_account(&db, "user-hash-a", "session-hash-a", 10_000)
                .unwrap()
                .is_none()
        );

        let next_session =
            verified_native_account(&db, "user-hash-a", "session-hash-b", 20_000, 3).unwrap();
        assert_eq!(next_session.generation, 2);
        let next_user =
            verified_native_account(&db, "user-hash-b", "session-hash-c", 30_000, 4).unwrap();
        assert_eq!(next_user.generation, 3);
        assert!(matches!(
            verified_native_account(&db, "user-hash-c", "session-hash-d", 4, 5),
            Err(InstallError::StaleScope)
        ));
    }

    #[test]
    fn signed_trust_snapshot_cannot_roll_back() {
        let mut db = Connection::open_in_memory().unwrap();
        initialize(&db).unwrap();
        let s = scope("a");
        let snapshot = |issued: u64, payload: &str| {
            serde_json::json!({
                "trust": { "trustEpoch": 1, "issuedAtMs": issued, "payload": payload }
            })
            .to_string()
        };
        activate_authorized(
            &mut db,
            &s,
            &s,
            "one",
            package("1.0.0", "one"),
            &snapshot(200, "new"),
            1,
        )
        .unwrap();
        let rejected = activate_authorized(
            &mut db,
            &s,
            &s,
            "two",
            package("1.1.0", "two"),
            &snapshot(100, "old"),
            2,
        )
        .err()
        .unwrap();
        assert!(matches!(rejected, InstallError::TrustRollback));
        assert_eq!(
            installed(&db, &s, "org.test.app").unwrap().unwrap().version,
            "1.0.0"
        );
    }
}
