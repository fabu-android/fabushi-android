use super::installable_package::{
    verify_catalog_bundle, NativeMiniAppEntry, NativeMiniAppInstallState, VerifiedPackage,
};
use super::installable_package::ledger::{self, HostInstallScope};
use super::installable_package::policy::{self, CatalogClient, ProofError, VerifiedInstallProof};
use crate::account_service::AndroidAccountService;
use base64::Engine as _;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use uuid::Uuid;

trait MiniAppCatalog: Send + Sync {
    fn list_descriptors(&self) -> Result<Vec<Value>, ProofError>;
    fn fetch_install_trust(&self) -> Result<String, ProofError>;
    fn fetch_install_proof(&self, descriptor: &Value) -> Result<String, ProofError>;
    fn download_artifact(&self, digest: &str, expected_size: u64) -> Result<Vec<u8>, ProofError>;
}

impl MiniAppCatalog for CatalogClient {
    fn list_descriptors(&self) -> Result<Vec<Value>, ProofError> {
        CatalogClient::list_descriptors(self)
    }

    fn fetch_install_trust(&self) -> Result<String, ProofError> {
        CatalogClient::fetch_install_trust(self)
    }

    fn fetch_install_proof(&self, descriptor: &Value) -> Result<String, ProofError> {
        CatalogClient::fetch_install_proof(self, descriptor)
    }

    fn download_artifact(&self, digest: &str, expected_size: u64) -> Result<Vec<u8>, ProofError> {
        CatalogClient::download_artifact(self, digest, expected_size)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AccountIdentitySnapshot {
    account_scope_hash: String,
    session_binding_hash: String,
    expires_at_ms: u64,
}

struct HostMiniAppInstanceLease {
    scope: HostInstallScope,
    app_id: String,
    package: VerifiedPackage,
}

pub struct MiniAppHostService {
    /// The Host owns this single runtime SQLite connection. Mini App activation,
    /// authorization snapshots, trust snapshots, quarantine, and operation
    /// receipts all live in the same durable owner.
    db: Connection,
    catalog: Box<dyn MiniAppCatalog>,
    leases: BTreeMap<String, HostMiniAppInstanceLease>,
}

impl MiniAppHostService {
    pub fn open(host_runtime_root: &Path) -> Result<Self, String> {
        fs::create_dir_all(host_runtime_root)
            .map_err(|error| format!("miniapp_storage_unavailable: {error}"))?;
        let db_path = host_runtime_root.join("runtime.sqlite3");
        let db = Connection::open(&db_path)
            .map_err(|error| format!("miniapp_storage_unavailable: {error}"))?;
        db.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|error| format!("miniapp_storage_unavailable: {error}"))?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA foreign_keys=ON;",
        )
        .map_err(|error| format!("miniapp_storage_unavailable: {error}"))?;
        ledger::initialize(&db).map_err(|error| format!("miniapp_storage_unavailable: {error}"))?;
        let catalog = CatalogClient::production_from_environment()
            .map_err(|error| format!("miniapp_catalog_unavailable: {error}"))?;
        Ok(Self {
            db,
            catalog: Box::new(catalog),
            leases: BTreeMap::new(),
        })
    }

    #[cfg(test)]
    fn with_catalog(db: Connection, catalog: Box<dyn MiniAppCatalog>) -> Result<Self, String> {
        ledger::initialize(&db).map_err(|error| error.to_string())?;
        Ok(Self {
            db,
            catalog,
            leases: BTreeMap::new(),
        })
    }

    fn account_identity(
        account: &AndroidAccountService,
    ) -> Result<AccountIdentitySnapshot, String> {
        let auth_id = account
            .auth_id()
            .ok_or_else(|| "miniapp_not_authenticated: no authenticated account".to_string())?;
        let session_fence = account
            .session_fence()
            .ok_or_else(|| "miniapp_not_authenticated: no authenticated session".to_string())?;
        let expires_at_ms = account
            .access_token_expires_at_ms()
            .ok_or_else(|| "miniapp_not_authenticated: session expiry is unavailable".to_string())?;
        if expires_at_ms <= policy::now_ms() {
            return Err("miniapp_session_expired: sign in again before using Mini Apps".into());
        }
        Ok(AccountIdentitySnapshot {
            account_scope_hash: crate::sha256::sha256_hex(auth_id.as_bytes()),
            session_binding_hash: crate::sha256::sha256_hex(session_fence.as_bytes()),
            expires_at_ms,
        })
    }

    fn current_scope(
        &self,
        account: &AndroidAccountService,
    ) -> Result<(HostInstallScope, AccountIdentitySnapshot), String> {
        let identity = Self::account_identity(account)?;
        let now_ms = policy::now_ms();
        let scope = ledger::verified_native_account(
            &self.db,
            &identity.account_scope_hash,
            &identity.session_binding_hash,
            identity.expires_at_ms,
            now_ms,
        )
        .map_err(|error| format!("miniapp_storage_unavailable: {error}"))?;
        Ok((scope, identity))
    }

    fn assert_same_identity(
        account: &AndroidAccountService,
        expected: &AccountIdentitySnapshot,
    ) -> bool {
        Self::account_identity(account)
            .map(|current| current == *expected)
            .unwrap_or(false)
    }

    fn validate_app_id(app_id: &str) -> Result<(), String> {
        if app_id.len() < 3
            || app_id.len() > 128
            || !app_id.bytes().next().is_some_and(|b| b.is_ascii_lowercase())
            || !app_id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
        {
            return Err("invalid_input: Mini App id is not a canonical identifier".into());
        }
        Ok(())
    }

    fn validate_operation_id(operation_id: &str) -> Result<(), String> {
        if operation_id.len() < 8
            || operation_id.len() > 128
            || operation_id.chars().any(char::is_whitespace)
        {
            return Err("invalid_input: Mini App operation id is invalid".into());
        }
        Ok(())
    }

    fn validate_resource_path(path: &str) -> Result<(), String> {
        if path.is_empty()
            || path.len() > 1024
            || path.starts_with('/')
            || path.contains('\\')
            || path.split('/').any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err("invalid_input: Mini App resource path is invalid".into());
        }
        Ok(())
    }

    fn proof_descriptor(raw: &str) -> Result<Value, String> {
        policy::proof_descriptor(raw).map_err(|error| format!("miniapp_proof_invalid: {error}"))
    }

    fn select_descriptor(descriptors: &[Value], app_id: &str) -> Option<Value> {
        descriptors
            .iter()
            .filter_map(|descriptor| {
                if policy::descriptor_app_id(descriptor) != Some(app_id)
                    || !policy::descriptor_supports_desktop(descriptor)
                {
                    return None;
                }
                let version = policy::descriptor_version(descriptor)?;
                let key = Self::stable_version_key(version)?;
                Some((key, descriptor.clone()))
            })
            .max_by_key(|(key, _)| *key)
            .map(|(_, descriptor)| descriptor)
    }

    fn stable_version_key(version: &str) -> Option<(u64, u64, u64)> {
        let mut parts = version.split('.');
        let key = (
            parts.next()?.parse().ok()?,
            parts.next()?.parse().ok()?,
            parts.next()?.parse().ok()?,
        );
        if parts.next().is_some() {
            return None;
        }
        Some(key)
    }

    fn legacy_plugin_id(app_id: &str) -> Option<&'static str> {
        match app_id {
            "org.fabushi.global-dandelion" => Some("global-dharma"),
            "org.fabushi.chatgpt-auto-confirm" => Some("chatgpt-auto-confirm"),
            _ => None,
        }
    }

    fn entry_json(
        &self,
        entry: NativeMiniAppEntry,
        legacy_plugin_id: Option<&str>,
    ) -> Result<Value, String> {
        let mut value = serde_json::to_value(entry)
            .map_err(|_| "miniapp_projection_invalid: failed to encode projection".to_string())?;
        if let Some(plugin_id) = legacy_plugin_id {
            value["legacyPluginId"] = Value::String(plugin_id.to_string());
        }
        Ok(value)
    }

    fn refresh_trust(&mut self) -> Result<policy::VerifiedTrustFeed, String> {
        let raw = self
            .catalog
            .fetch_install_trust()
            .map_err(|error| format!("miniapp_trust_refresh_failed: {error}"))?;
        let verified = policy::verify_trust_feed(&raw, policy::now_ms(), false)
            .map_err(|error| format!("miniapp_trust_invalid: {error}"))?;
        let previous = ledger::current_trust_envelope(&self.db)
            .map_err(|error| format!("miniapp_storage_unavailable: {error}"))?;
        policy::trust_feed_not_rolled_back(
            &verified,
            previous.as_deref(),
        )
        .map_err(|error| format!("miniapp_trust_rollback: {error}"))?;
        ledger::cache_verified_trust(
            &mut self.db,
            verified.epoch,
            verified.issued_at_ms,
            &verified.signed_payload,
            &verified.envelope_json,
        )
        .map_err(|error| format!("miniapp_storage_unavailable: {error}"))?;
        Ok(verified)
    }

    fn quarantine(&mut self, scope: &HostInstallScope, app_id: &str, reason: &str) {
        let _ = ledger::set_quarantined(&mut self.db, scope, app_id, Some(reason));
        self.leases.retain(|_, lease| {
            !(lease.scope == *scope && lease.app_id == app_id)
        });
    }

    fn quarantine_if_current(
        &mut self,
        account: &AndroidAccountService,
        expected_identity: &AccountIdentitySnapshot,
        scope: &HostInstallScope,
        app_id: &str,
        reason: &str,
    ) {
        if Self::assert_same_identity(account, expected_identity) {
            self.quarantine(scope, app_id, reason);
        }
    }

    pub fn list_approved_mini_apps(
        &mut self,
        account: &AndroidAccountService,
    ) -> Result<Value, String> {
        let trust = self.refresh_trust()?;
        let descriptors = self
            .catalog
            .list_descriptors()
            .map_err(|error| format!("miniapp_catalog_list_failed: {error}"))?;

        let mut selected: BTreeMap<String, Value> = BTreeMap::new();
        for descriptor in descriptors {
            let Some(app_id) = policy::descriptor_app_id(&descriptor) else {
                continue;
            };
            let Some(version) = policy::descriptor_version(&descriptor) else {
                continue;
            };
            let Some(key) = Self::stable_version_key(version) else {
                continue;
            };
            if !policy::descriptor_supports_desktop(&descriptor) {
                continue;
            }
            match selected.get(app_id) {
                Some(current) => {
                    let current_key = policy::descriptor_version(current)
                        .and_then(Self::stable_version_key)
                        .unwrap_or_default();
                    if key > current_key {
                        selected.insert(app_id.to_string(), descriptor);
                    }
                }
                None => {
                    selected.insert(app_id.to_string(), descriptor);
                }
            }
        }

        let account_scope = Self::current_scope(self, account).ok();
        let mut apps = Vec::new();
        for (app_id, descriptor) in selected {
            let proof_snapshot = match self.catalog.fetch_install_proof(&descriptor) {
                Ok(proof) => proof,
                Err(ProofError::NotFound) => continue,
                Err(error) => {
                    return Err(format!("miniapp_proof_lookup_failed: {error}"));
                }
            };
            policy::ensure_trust_not_rolled_back(
                &proof_snapshot,
                Some(&trust.envelope_json),
            )
            .map_err(|error| format!("miniapp_proof_invalid: {error}"))?;
            let effective = policy::overlay_trust(&proof_snapshot, &trust.envelope_json)
                .map_err(|error| format!("miniapp_proof_invalid: {error}"))?;
            let verified = policy::verify_install_proof(
                &effective,
                &app_id,
                policy::now_ms(),
                false,
            )
            .map_err(|error| format!("miniapp_not_approved: {error}"))?;
            let installed = if let Some((scope, _)) = &account_scope {
                ledger::installed(&self.db, scope, &app_id)
                    .map_err(|error| format!("miniapp_storage_unavailable: {error}"))?
                    .is_some()
            } else {
                false
            };
            let entry = NativeMiniAppEntry {
                app_id: app_id.clone(),
                display_name: policy::descriptor_display_name(&verified.descriptor)
                    .unwrap_or(&app_id)
                    .to_string(),
                publisher_id: policy::descriptor_publisher(&verified.descriptor)
                    .unwrap_or("unknown")
                    .to_string(),
                version: verified.authorization.version.clone(),
                package_digest: verified.authorization.bundle_digest.clone(),
                size_bytes: verified.authorization.artifact_size_bytes,
                offline_core: true,
                installed,
                supported: true,
                state: NativeMiniAppInstallState::Ready,
            };
            apps.push(self.entry_json(entry, Self::legacy_plugin_id(&app_id))?);
        }

        Ok(json!({
            "apps": apps,
            "trustEpoch": trust.epoch,
            "offlineCore": true
        }))
    }

    pub fn list_installed_mini_apps(
        &mut self,
        account: &AndroidAccountService,
    ) -> Result<Value, String> {
        let (scope, identity) = self.current_scope(account)?;
        let listings = ledger::list_installed(&self.db, &scope)
            .map_err(|error| format!("miniapp_storage_unavailable: {error}"))?;
        let cached_trust = ledger::current_trust_envelope(&self.db)
            .map_err(|error| format!("miniapp_storage_unavailable: {error}"))?;

        let mut apps = Vec::new();
        for listing in listings {
            let quarantine_reason = ledger::quarantine_reason(&self.db, &scope, &listing.app_id)
                .map_err(|error| format!("miniapp_storage_unavailable: {error}"))?;
            let mut state = NativeMiniAppInstallState::Ready;
            let mut supported = false;
            let mut display_name = listing.app_id.clone();
            let mut publisher_id = listing.publisher_id.clone();
            let mut version = listing.version.clone();
            let mut digest = listing.digest.clone();
            let mut size_bytes = listing.size_bytes;

            if quarantine_reason.is_none() {
                match ledger::load_active_authorized_archive(&self.db, &scope, &listing.app_id) {
                    Ok(Some((archive, proof_snapshot, trust_snapshot))) => {
                        let result = self.verify_cached_install(
                            &listing.app_id,
                            &listing.digest,
                            &archive,
                            &proof_snapshot,
                            trust_snapshot.as_deref().or(cached_trust.as_deref()),
                            true,
                        );
                        match result {
                            Ok((verified, proof)) => {
                                supported = true;
                                display_name = policy::descriptor_display_name(&proof.descriptor)
                                    .unwrap_or(&listing.app_id)
                                    .to_string();
                                publisher_id = policy::descriptor_publisher(&proof.descriptor)
                                    .unwrap_or(&listing.publisher_id)
                                    .to_string();
                                version = proof.authorization.version.clone();
                                digest = proof.authorization.bundle_digest.clone();
                                size_bytes = proof.authorization.artifact_size_bytes;
                            }
                            Err(reason) => {
                                self.quarantine(&scope, &listing.app_id, &reason);
                                state = NativeMiniAppInstallState::Quarantined;
                            }
                        }
                    }
                    Ok(None) => {
                        self.quarantine(&scope, &listing.app_id, "missing_authorization");
                        state = NativeMiniAppInstallState::Quarantined;
                    }
                    Err(error) => {
                        return Err(format!("miniapp_storage_unavailable: {error}"));
                    }
                }
            } else {
                state = NativeMiniAppInstallState::Quarantined;
            }

            let entry = NativeMiniAppEntry {
                app_id: listing.app_id.clone(),
                display_name,
                publisher_id,
                version,
                package_digest: digest,
                size_bytes,
                offline_core: true,
                installed: true,
                supported,
                state,
            };
            apps.push(self.entry_json(entry, Self::legacy_plugin_id(&listing.app_id))?);
        }

        if !Self::assert_same_identity(account, &identity) {
            return Err("stale_generation: account/session changed while listing Mini Apps".into());
        }
        Ok(json!({"apps": apps, "generation": scope.generation}))
    }

    fn verify_cached_install(
        &self,
        app_id: &str,
        expected_digest: &str,
        archive: &[u8],
        proof_snapshot: &str,
        latest_trust: Option<&str>,
        allow_expired_feed: bool,
    ) -> Result<(VerifiedPackage, VerifiedInstallProof), String> {
        let descriptor = Self::proof_descriptor(proof_snapshot)?;
        if policy::descriptor_app_id(&descriptor) != Some(app_id) {
            return Err("descriptor_binding".into());
        }
        policy::ensure_trust_not_rolled_back(proof_snapshot, latest_trust)
            .map_err(|error| format!("trust_rollback: {error}"))?;
        let effective = policy::overlay_cached_trust(proof_snapshot, latest_trust)
            .map_err(|error| format!("proof_invalid: {error}"))?;
        let verified_proof = policy::verify_install_proof(
            &effective,
            app_id,
            policy::now_ms(),
            allow_expired_feed,
        )
        .map_err(|error| format!("proof_invalid: {error}"))?;
        let package = verify_catalog_bundle(archive, &verified_proof.authorization)
            .map_err(|_| "package_invalid".to_string())?;
        if package.digest() != expected_digest {
            return Err("digest_mismatch".into());
        }
        Ok((package, verified_proof))
    }

    fn revalidate_installed(
        &mut self,
        account: &AndroidAccountService,
        expected_identity: &AccountIdentitySnapshot,
        scope: &HostInstallScope,
        app_id: &str,
    ) -> Result<(VerifiedPackage, VerifiedInstallProof), String> {
        let Some((archive, cached_proof, cached_trust)) =
            ledger::load_active_authorized_archive(&self.db, scope, app_id)
                .map_err(|error| format!("miniapp_storage_unavailable: {error}"))?
        else {
            return Err("not_installed: Mini App is not installed".into());
        };

        let mut latest_trust = cached_trust.or_else(|| {
            ledger::current_trust_envelope(&self.db).ok().flatten()
        });
        let mut allow_expired_feed = true;
        match self.catalog.fetch_install_trust() {
            Ok(raw) => {
                let verified = policy::verify_trust_feed(&raw, policy::now_ms(), false)
                    .map_err(|error| format!("miniapp_trust_invalid: {error}"))?;
                policy::trust_feed_not_rolled_back(
                    &verified,
                    latest_trust.as_deref(),
                )
                .map_err(|error| format!("miniapp_trust_rollback: {error}"))?;
                ledger::cache_verified_trust(
                    &mut self.db,
                    verified.epoch,
                    verified.issued_at_ms,
                    &verified.signed_payload,
                    &verified.envelope_json,
                )
                .map_err(|error| format!("miniapp_storage_unavailable: {error}"))?;
                latest_trust = Some(verified.envelope_json);
                allow_expired_feed = false;
            }
            Err(error) if policy::is_transport_failure(&error) => {}
            Err(error) => {
                self.quarantine_if_current(
                    account,
                    expected_identity,
                    scope,
                    app_id,
                    "trust_refresh_rejected",
                );
                return Err(format!("miniapp_trust_refresh_failed: {error}"));
            }
        }

        let descriptor = Self::proof_descriptor(&cached_proof)?;
        let proof_snapshot = match self.catalog.fetch_install_proof(&descriptor) {
            Ok(proof) => proof,
            Err(error) if policy::is_transport_failure(&error) => cached_proof,
            Err(ProofError::NotFound) => {
                self.quarantine_if_current(
                    account,
                    expected_identity,
                    scope,
                    app_id,
                    "release_not_approved",
                );
                return Err("release_not_approved: installed Mini App is no longer approved".into());
            }
            Err(error) => {
                self.quarantine_if_current(
                    account,
                    expected_identity,
                    scope,
                    app_id,
                    "proof_refresh_rejected",
                );
                return Err(format!("miniapp_proof_refresh_failed: {error}"));
            }
        };

        let effective = if latest_trust.is_some() {
            policy::overlay_cached_trust(&proof_snapshot, latest_trust.as_deref())
                .map_err(|error| format!("miniapp_proof_invalid: {error}"))?
        } else {
            proof_snapshot.clone()
        };
        let verified_proof = match policy::verify_install_proof(
            &effective,
            app_id,
            policy::now_ms(),
            allow_expired_feed,
        ) {
            Ok(proof) => proof,
            Err(error) => {
                self.quarantine_if_current(
                    account,
                    expected_identity,
                    scope,
                    app_id,
                    "proof_revoked_or_invalid",
                );
                return Err(format!("miniapp_proof_invalid: {error}"));
            }
        };
        let package = match verify_catalog_bundle(&archive, &verified_proof.authorization) {
            Ok(package) => package,
            Err(_) => {
                self.quarantine_if_current(
                    account,
                    expected_identity,
                    scope,
                    app_id,
                    "package_invalid",
                );
                return Err("package_invalid: installed Mini App failed verification".into());
            }
        };
        if package.digest() != scope_digest(&self.db, scope, app_id)? {
            self.quarantine_if_current(
                account,
                expected_identity,
                scope,
                app_id,
                "digest_mismatch",
            );
            return Err("digest_mismatch: installed package does not match durable activation".into());
        }
        if !Self::assert_same_identity(account, expected_identity) {
            return Err("stale_generation: account/session changed while revalidating Mini App".into());
        }
        Ok((package, verified_proof))
    }

    pub fn install_approved_mini_app(
        &mut self,
        account: &AndroidAccountService,
        app_id: &str,
        operation_id: &str,
    ) -> Result<Value, String> {
        Self::validate_app_id(app_id)?;
        Self::validate_operation_id(operation_id)?;
        let (scope, identity) = self.current_scope(account)?;

        let trust = self.refresh_trust()?;
        let descriptors = self
            .catalog
            .list_descriptors()
            .map_err(|error| format!("miniapp_catalog_list_failed: {error}"))?;
        let descriptor = Self::select_descriptor(&descriptors, app_id)
            .ok_or_else(|| "not_found: no supported Mini App release exists".to_string())?;
        let proof_snapshot = self
            .catalog
            .fetch_install_proof(&descriptor)
            .map_err(|error| match error {
                ProofError::NotFound => {
                    "not_approved: selected Mini App release has no approval proof".to_string()
                }
                other => format!("miniapp_proof_lookup_failed: {other}"),
            })?;
        policy::ensure_trust_not_rolled_back(&proof_snapshot, Some(&trust.envelope_json))
            .map_err(|error| format!("miniapp_trust_rollback: {error}"))?;
        let effective = policy::overlay_trust(&proof_snapshot, &trust.envelope_json)
            .map_err(|error| format!("miniapp_proof_invalid: {error}"))?;
        let verified_proof = policy::verify_install_proof(
            &effective,
            app_id,
            policy::now_ms(),
            false,
        )
        .map_err(|error| format!("not_approved: {error}"))?;

        if !Self::assert_same_identity(account, &identity) {
            return Err("stale_generation: account/session changed before artifact download".into());
        }

        let archive = self
            .catalog
            .download_artifact(
                &verified_proof.authorization.bundle_digest,
                verified_proof.authorization.artifact_size_bytes,
            )
            .map_err(|error| format!("miniapp_artifact_download_failed: {error}"))?;
        let package = verify_catalog_bundle(&archive, &verified_proof.authorization)
            .map_err(|_| "package_invalid: catalog bundle verification failed".to_string())?;

        if !Self::assert_same_identity(account, &identity) {
            return Err("stale_generation: account/session changed after artifact verification".into());
        }
        let current_scope = ledger::verified_native_account(
            &self.db,
            &identity.account_scope_hash,
            &identity.session_binding_hash,
            identity.expires_at_ms,
            policy::now_ms(),
        )
        .map_err(|error| format!("miniapp_storage_unavailable: {error}"))?;
        if current_scope != scope {
            return Err("stale_generation: durable account generation changed".into());
        }

        let proof_snapshot = verified_proof.proof_snapshot.clone();
        let installed = ledger::activate_authorized_fenced(
            &mut self.db,
            &current_scope,
            &scope,
            operation_id,
            package,
            &proof_snapshot,
            policy::now_ms(),
            || Self::assert_same_identity(account, &identity),
        )
        .map_err(|error| format!("miniapp_install_failed: {error}"))?;
        ledger::set_quarantined(&mut self.db, &scope, app_id, None)
            .map_err(|error| format!("miniapp_storage_unavailable: {error}"))?;
        self.leases.retain(|_, lease| {
            !(lease.scope == scope && lease.app_id == app_id)
        });

        let entry = NativeMiniAppEntry {
            app_id: installed.app_id.clone(),
            display_name: policy::descriptor_display_name(&verified_proof.descriptor)
                .unwrap_or(app_id)
                .to_string(),
            publisher_id: policy::descriptor_publisher(&verified_proof.descriptor)
                .unwrap_or("unknown")
                .to_string(),
            version: installed.version.clone(),
            package_digest: installed.digest.clone(),
            size_bytes: verified_proof.authorization.artifact_size_bytes,
            offline_core: true,
            installed: true,
            supported: true,
            state: NativeMiniAppInstallState::Ready,
        };
        Ok(json!({
            "app": self.entry_json(entry, Self::legacy_plugin_id(app_id))?,
            "operationId": operation_id,
            "generation": scope.generation,
            "state": "ready"
        }))
    }

    pub fn get_installed_mini_app_resources(
        &mut self,
        account: &AndroidAccountService,
        app_id: &str,
        operation_id: &str,
    ) -> Result<Value, String> {
        Self::validate_app_id(app_id)?;
        Self::validate_operation_id(operation_id)?;
        let (scope, identity) = self.current_scope(account)?;
        let (package, proof) = self.revalidate_installed(account, &identity, &scope, app_id)?;

        if !Self::assert_same_identity(account, &identity) {
            return Err("stale_generation: account/session changed before Mini App load".into());
        }
        let load_id = format!("miniapp-load-{}", Uuid::new_v4());
        let resources = package
            .resource_entries()
            .into_iter()
            .map(|resource| {
                json!({
                    "path": resource.path,
                    "mimeType": resource.mime_type,
                    "bytes": resource.bytes,
                })
            })
            .collect::<Vec<_>>();
        self.leases.insert(
            load_id.clone(),
            HostMiniAppInstanceLease {
                scope: scope.clone(),
                app_id: app_id.to_string(),
                package,
            },
        );
        Ok(json!({
            "loadId": load_id,
            "appId": app_id,
            "version": proof.authorization.version,
            "packageDigest": proof.authorization.bundle_digest,
            "entrypoint": proof.authorization.entrypoint,
            "resources": resources,
            "generation": scope.generation,
            "operationId": operation_id
        }))
    }

    pub fn read_installed_mini_app_resource(
        &mut self,
        account: &AndroidAccountService,
        load_id: &str,
        resource_path: &str,
    ) -> Result<Value, String> {
        Self::validate_resource_path(resource_path)?;
        let Some(lease) = self.leases.get(load_id) else {
            return Err("stale_load: Mini App load identity is no longer active".into());
        };
        let lease_scope = lease.scope.clone();
        let lease_app_id = lease.app_id.clone();
        let current_scope = self.current_scope(account)?.0;
        if current_scope != lease_scope {
            self.leases.remove(load_id);
            return Err("stale_generation: Mini App load belongs to an old account/session".into());
        }
        if current_scope != lease_scope {
            self.leases.remove(load_id);
            return Err("stale_generation: Mini App load belongs to an old account/session".into());
        }
        let lease = self.leases.get(load_id).ok_or_else(|| {
            "stale_load: Mini App load identity is no longer active".to_string()
        })?;
        if lease.app_id != lease_app_id {
            return Err("stale_load: Mini App load identity mismatch".into());
        }
        let resource = lease
            .package
            .resource(resource_path)
            .ok_or_else(|| "not_found: verified Mini App resource does not exist".to_string())?;
        let bytes = base64::engine::general_purpose::STANDARD.encode(resource.bytes);
        Ok(json!({
            "loadId": load_id,
            "appId": lease.app_id,
            "path": resource.path,
            "mimeType": resource.mime_type,
            "bytesBase64": bytes,
            "generation": lease_scope.generation
        }))
    }

    pub fn dispose_mini_app_resource_load(&mut self, load_id: &str) -> Result<Value, String> {
        self.leases.remove(load_id);
        Ok(json!({"disposed": true, "loadId": load_id}))
    }

    pub fn uninstall_mini_app(
        &mut self,
        account: &AndroidAccountService,
        app_id: &str,
        operation_id: &str,
    ) -> Result<Value, String> {
        Self::validate_app_id(app_id)?;
        Self::validate_operation_id(operation_id)?;
        let (scope, identity) = self.current_scope(account)?;
        let removed = ledger::remove_fenced(
            &mut self.db,
            &scope,
            &scope,
            app_id,
            operation_id,
            || Self::assert_same_identity(account, &identity),
        )
        .map_err(|error| format!("miniapp_uninstall_failed: {error}"))?;
        self.leases.retain(|_, lease| {
            !(lease.scope == scope && lease.app_id == app_id)
        });
        Ok(json!({
            "appId": app_id,
            "operationId": operation_id,
            "removed": removed,
            "generation": scope.generation,
            "state": "uninstalled"
        }))
    }
}

fn scope_digest(
    db: &Connection,
    scope: &HostInstallScope,
    app_id: &str,
) -> Result<String, String> {
    ledger::installed(db, scope, app_id)
        .map_err(|error| format!("miniapp_storage_unavailable: {error}"))?
        .map(|record| record.digest)
        .ok_or_else(|| "not_installed: Mini App activation disappeared".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EmptyCatalog;

    impl MiniAppCatalog for EmptyCatalog {
        fn list_descriptors(&self) -> Result<Vec<Value>, ProofError> {
            Ok(Vec::new())
        }
        fn fetch_install_trust(&self) -> Result<String, ProofError> {
            Err(ProofError::Network)
        }
        fn fetch_install_proof(&self, _descriptor: &Value) -> Result<String, ProofError> {
            Err(ProofError::NotFound)
        }
        fn download_artifact(
            &self,
            _digest: &str,
            _expected_size: u64,
        ) -> Result<Vec<u8>, ProofError> {
            Err(ProofError::Network)
        }
    }

    #[test]
    fn validates_canonical_app_ids_and_operation_ids() {
        assert!(MiniAppHostService::validate_app_id("org.fabushi.global-dandelion").is_ok());
        assert!(MiniAppHostService::validate_app_id("../escape").is_err());
        assert!(MiniAppHostService::validate_operation_id("install-001").is_ok());
        assert!(MiniAppHostService::validate_operation_id("bad id").is_err());
    }

    #[test]
    fn validates_logical_resource_paths() {
        assert!(MiniAppHostService::validate_resource_path("dist/index.html").is_ok());
        assert!(MiniAppHostService::validate_resource_path("../secret").is_err());
        assert!(MiniAppHostService::validate_resource_path("dist/../secret").is_err());
        assert!(MiniAppHostService::validate_resource_path("/etc/passwd").is_err());
    }

    #[test]
    fn selects_highest_stable_supported_release() {
        let descriptor = |version: &str| {
            json!({
                "identity": {
                    "kind": "web-miniapp",
                    "namespace": "bhrumom",
                    "name": "global-dandelion"
                },
                "displayName": "Global Dandelion",
                "publisher": {"id": "bhrumom"},
                "runtime": {"installMode": "fabapp"},
                "release": {"version": version, "platforms": ["desktop"]}
            })
        };
        let selected = MiniAppHostService::select_descriptor(
            &[descriptor("1.2.0"), descriptor("1.3.0"), descriptor("2.0.0-beta")],
            "org.fabushi.global-dandelion",
        )
        .expect("stable release");
        assert_eq!(policy::descriptor_version(&selected), Some("1.3.0"));
    }

    #[test]
    fn rejects_invalid_resource_paths_without_touching_storage() {
        let db = Connection::open_in_memory().unwrap();
        let service = MiniAppHostService::with_catalog(db, Box::new(EmptyCatalog)).unwrap();
        assert!(service.leases.is_empty());
        assert!(MiniAppHostService::validate_resource_path("../escape").is_err());
        assert!(service.leases.is_empty());
    }
}
