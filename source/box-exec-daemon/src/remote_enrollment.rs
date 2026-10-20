use crate::server::{RemoteExecutionAuthorizer, RemoteRequestIdentity};
use fabushi_android_shared::ExecutionError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

const STATE_VERSION: u32 = 1;
const MAX_IDENTITY_BYTES: usize = 512;
const MAX_BEARER_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteRunnerEnrollment {
    pub credential_id: String,
    pub bearer_sha256: String,
    pub account_fence: String,
    pub account_epoch: u64,
    pub device_id: String,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub capabilities: BTreeSet<String>,
    pub executors: BTreeSet<String>,
    pub revoked_at_ms: Option<u64>,
}

impl RemoteRunnerEnrollment {
    pub fn new(
        credential_id: impl Into<String>,
        bearer: &str,
        account_fence: impl Into<String>,
        account_epoch: u64,
        device_id: impl Into<String>,
        issued_at_ms: u64,
        expires_at_ms: u64,
        capabilities: impl IntoIterator<Item = String>,
        executors: impl IntoIterator<Item = String>,
    ) -> Result<Self, ExecutionError> {
        let enrollment = Self {
            credential_id: credential_id.into(),
            bearer_sha256: bearer_digest(bearer)?,
            account_fence: account_fence.into(),
            account_epoch,
            device_id: device_id.into(),
            issued_at_ms,
            expires_at_ms,
            capabilities: capabilities.into_iter().collect(),
            executors: executors.into_iter().collect(),
            revoked_at_ms: None,
        };
        enrollment.validate()?;
        Ok(enrollment)
    }

    fn validate(&self) -> Result<(), ExecutionError> {
        validate_identity("credential", &self.credential_id)?;
        validate_identity("account fence", &self.account_fence)?;
        validate_identity("device", &self.device_id)?;
        if self.account_epoch == 0 {
            return invalid("Remote Runner enrollment account epoch must be positive");
        }
        if self.issued_at_ms == 0 || self.expires_at_ms <= self.issued_at_ms {
            return invalid("Remote Runner enrollment lifetime is invalid");
        }
        if self.bearer_sha256.len() != 64 || !self.bearer_sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return invalid("Remote Runner enrollment bearer digest is invalid");
        }
        if self.capabilities.is_empty() || self.executors.is_empty() {
            return invalid("Remote Runner enrollment must declare bounded capabilities and executors");
        }
        for capability in &self.capabilities {
            validate_identity("capability", capability)?;
            let executor = required_executor(capability).ok_or_else(|| {
                ExecutionError::InvalidRequest(format!("Remote Runner enrollment capability is unsupported: {capability}"))
            })?;
            if !self.executors.contains(executor) {
                return invalid(format!("Remote Runner enrollment capability {capability} lacks executor {executor}"));
            }
        }
        for executor in &self.executors {
            validate_identity("executor", executor)?;
            if !allowed_executor(executor) {
                return invalid(format!("Remote Runner enrollment executor is unsupported: {executor}"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteRunnerGrant {
    pub permission_grant_id: String,
    pub credential_id: String,
    pub operation_id: String,
    pub request_id: String,
    pub capability_id: String,
    pub account_fence: String,
    pub account_epoch: u64,
    pub device_id: String,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub consumed_at_ms: Option<u64>,
    pub revoked_at_ms: Option<u64>,
}

impl RemoteRunnerGrant {
    pub fn new(
        permission_grant_id: impl Into<String>,
        credential_id: impl Into<String>,
        operation_id: impl Into<String>,
        request_id: impl Into<String>,
        capability_id: impl Into<String>,
        account_fence: impl Into<String>,
        account_epoch: u64,
        device_id: impl Into<String>,
        issued_at_ms: u64,
        expires_at_ms: u64,
    ) -> Result<Self, ExecutionError> {
        let grant = Self {
            permission_grant_id: permission_grant_id.into(),
            credential_id: credential_id.into(),
            operation_id: operation_id.into(),
            request_id: request_id.into(),
            capability_id: capability_id.into(),
            account_fence: account_fence.into(),
            account_epoch,
            device_id: device_id.into(),
            issued_at_ms,
            expires_at_ms,
            consumed_at_ms: None,
            revoked_at_ms: None,
        };
        grant.validate()?;
        Ok(grant)
    }

    fn validate(&self) -> Result<(), ExecutionError> {
        for (label, value) in [
            ("permission grant", self.permission_grant_id.as_str()),
            ("credential", self.credential_id.as_str()),
            ("operation", self.operation_id.as_str()),
            ("request", self.request_id.as_str()),
            ("capability", self.capability_id.as_str()),
            ("account fence", self.account_fence.as_str()),
            ("device", self.device_id.as_str()),
        ] {
            validate_identity(label, value)?;
        }
        if self.account_epoch == 0 || self.issued_at_ms == 0 || self.expires_at_ms <= self.issued_at_ms {
            return invalid("Remote Runner grant lifetime/account epoch is invalid");
        }
        if required_executor(&self.capability_id).is_none() {
            return invalid("Remote Runner grant capability is unsupported");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct EnrollmentState {
    version: u32,
    enrollment: Option<RemoteRunnerEnrollment>,
    grants: BTreeMap<String, RemoteRunnerGrant>,
}

impl Default for EnrollmentState {
    fn default() -> Self {
        Self { version: STATE_VERSION, enrollment: None, grants: BTreeMap::new() }
    }
}

pub struct DurableRemoteRunnerEnrollmentAuthorizer {
    path: PathBuf,
    state: EnrollmentState,
    now_ms: u64,
}

impl DurableRemoteRunnerEnrollmentAuthorizer {
    pub fn open(path: impl Into<PathBuf>, now_ms: u64) -> Result<Self, ExecutionError> {
        let path = path.into();
        let state = if path.exists() {
            let bytes = fs::read(&path).map_err(io_error)?;
            let state: EnrollmentState = serde_json::from_slice(&bytes)
                .map_err(|error| ExecutionError::InvalidRequest(format!("invalid Remote Runner enrollment journal: {error}")))?;
            if state.version != STATE_VERSION {
                return invalid("unsupported Remote Runner enrollment journal version");
            }
            if let Some(enrollment) = &state.enrollment { enrollment.validate()?; }
            for grant in state.grants.values() { grant.validate()?; }
            state
        } else {
            EnrollmentState::default()
        };
        Ok(Self { path, state, now_ms })
    }

    pub fn install_enrollment(&mut self, enrollment: RemoteRunnerEnrollment) -> Result<(), ExecutionError> {
        enrollment.validate()?;
        if enrollment.expires_at_ms <= self.now_ms || enrollment.revoked_at_ms.is_some() {
            return invalid("cannot install expired or revoked Remote Runner enrollment");
        }
        self.state.enrollment = Some(enrollment);
        self.state.grants.clear();
        self.persist()
    }

    pub fn issue_grant(&mut self, grant: RemoteRunnerGrant) -> Result<(), ExecutionError> {
        grant.validate()?;
        let enrollment = self.active_enrollment()?;
        require_grant_matches_enrollment(enrollment, &grant, self.now_ms)?;
        if let Some(existing) = self.state.grants.get(&grant.permission_grant_id) {
            if existing == &grant { return Ok(()); }
            return invalid("Remote Runner grant id is already bound to another request");
        }
        self.state.grants.insert(grant.permission_grant_id.clone(), grant);
        self.persist()
    }

    pub fn revoke_grant(&mut self, permission_grant_id: &str, now_ms: u64) -> Result<bool, ExecutionError> {
        self.now_ms = now_ms;
        let Some(grant) = self.state.grants.get_mut(permission_grant_id) else { return Ok(false); };
        if grant.revoked_at_ms.is_none() { grant.revoked_at_ms = Some(now_ms); }
        self.persist()?;
        Ok(true)
    }

    pub fn revoke_enrollment(&mut self, now_ms: u64) -> Result<bool, ExecutionError> {
        self.now_ms = now_ms;
        let Some(enrollment) = self.state.enrollment.as_mut() else { return Ok(false); };
        if enrollment.revoked_at_ms.is_none() { enrollment.revoked_at_ms = Some(now_ms); }
        self.persist()?;
        Ok(true)
    }

    pub fn advance_time(&mut self, now_ms: u64) { self.now_ms = now_ms; }

    fn active_enrollment(&self) -> Result<&RemoteRunnerEnrollment, ExecutionError> {
        let enrollment = self.state.enrollment.as_ref().ok_or_else(|| {
            ExecutionError::CapabilityUnavailable("Remote Runner is not enrolled".into())
        })?;
        if enrollment.revoked_at_ms.is_some() || self.now_ms < enrollment.issued_at_ms || self.now_ms >= enrollment.expires_at_ms {
            return Err(ExecutionError::CapabilityUnavailable("Remote Runner enrollment is revoked, not yet active, or expired".into()));
        }
        Ok(enrollment)
    }

    fn persist(&self) -> Result<(), ExecutionError> {
        let parent = self.path.parent().ok_or_else(|| ExecutionError::InvalidRequest("Remote Runner enrollment path has no parent".into()))?;
        fs::create_dir_all(parent).map_err(io_error)?;
        let temp = self.path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(&self.state)
            .map_err(|error| ExecutionError::Transport(format!("failed to encode Remote Runner enrollment journal: {error}")))?;
        let result = (|| -> Result<(), ExecutionError> {
            let mut file = File::create(&temp).map_err(io_error)?;
            file.write_all(&bytes).map_err(io_error)?;
            file.sync_all().map_err(io_error)?;
            fs::rename(&temp, &self.path).map_err(io_error)?;
            #[cfg(unix)]
            File::open(parent).map_err(io_error)?.sync_all().map_err(io_error)?;
            Ok(())
        })();
        if result.is_err() { let _ = fs::remove_file(&temp); }
        result
    }
}

impl RemoteExecutionAuthorizer for DurableRemoteRunnerEnrollmentAuthorizer {
    fn authorize(
        &mut self,
        bearer: &str,
        identity: &RemoteRequestIdentity,
        capability_id: Option<&str>,
        now_ms: u64,
    ) -> Result<(), ExecutionError> {
        self.now_ms = now_ms;
        identity.validate()?;
        let enrollment = self.active_enrollment()?.clone();
        if bearer_digest(bearer)? != enrollment.bearer_sha256 {
            return invalid("Remote Runner credential rejected");
        }
        if identity.credential_id != enrollment.credential_id
            || identity.account_fence != enrollment.account_fence
            || identity.account_epoch != enrollment.account_epoch
            || identity.device_id != enrollment.device_id
        {
            return invalid("Remote Runner enrollment identity is stale or belongs to another account/device");
        }
        let grant = self.state.grants.get(&identity.permission_grant_id).cloned().ok_or_else(|| {
            ExecutionError::CapabilityUnavailable("Remote Runner one-time grant is unavailable".into())
        })?;
        require_grant_matches_identity(&enrollment, &grant, identity, capability_id, self.now_ms)?;
        if grant.revoked_at_ms.is_some() {
            return Err(ExecutionError::CapabilityUnavailable("Remote Runner one-time grant is revoked".into()));
        }
        if let Some(capability) = capability_id {
            if !enrollment.capabilities.contains(capability) {
                return Err(ExecutionError::CapabilityUnavailable(format!("Remote Runner enrollment does not allow capability {capability}")));
            }
            let executor = required_executor(capability).ok_or_else(|| ExecutionError::CapabilityUnavailable("Remote Runner capability is unsupported".into()))?;
            if !enrollment.executors.contains(executor) {
                return Err(ExecutionError::CapabilityUnavailable(format!("Remote Runner enrollment does not allow executor {executor}")));
            }
        }
        if grant.consumed_at_ms.is_none() && capability_id.is_some() {
            let stored = self.state.grants.get_mut(&identity.permission_grant_id).expect("grant remains present");
            stored.consumed_at_ms = Some(self.now_ms);
            self.persist()?;
        }
        Ok(())
    }
}

fn require_grant_matches_enrollment(
    enrollment: &RemoteRunnerEnrollment,
    grant: &RemoteRunnerGrant,
    now_ms: u64,
) -> Result<(), ExecutionError> {
    if grant.credential_id != enrollment.credential_id
        || grant.account_fence != enrollment.account_fence
        || grant.account_epoch != enrollment.account_epoch
        || grant.device_id != enrollment.device_id
        || !enrollment.capabilities.contains(&grant.capability_id)
        || now_ms < grant.issued_at_ms
        || now_ms >= grant.expires_at_ms
        || grant.expires_at_ms > enrollment.expires_at_ms
    {
        return invalid("Remote Runner grant is stale or outside enrollment scope");
    }
    let executor = required_executor(&grant.capability_id).ok_or_else(|| ExecutionError::CapabilityUnavailable("Remote Runner grant capability is unsupported".into()))?;
    if !enrollment.executors.contains(executor) {
        return invalid("Remote Runner grant exceeds enrolled executor scope");
    }
    Ok(())
}

fn require_grant_matches_identity(
    enrollment: &RemoteRunnerEnrollment,
    grant: &RemoteRunnerGrant,
    identity: &RemoteRequestIdentity,
    capability_id: Option<&str>,
    now_ms: u64,
) -> Result<(), ExecutionError> {
    require_grant_matches_enrollment(enrollment, grant, now_ms)?;
    if grant.permission_grant_id != identity.permission_grant_id
        || grant.credential_id != identity.credential_id
        || grant.operation_id != identity.operation_id
        || grant.request_id != identity.request_id
        || grant.account_fence != identity.account_fence
        || grant.account_epoch != identity.account_epoch
        || grant.device_id != identity.device_id
        || capability_id.is_some_and(|capability| grant.capability_id != capability)
    {
        return invalid("Remote Runner grant identity does not match this operation");
    }
    Ok(())
}

fn required_executor(capability: &str) -> Option<&'static str> {
    match capability {
        "box.environment.apply" => Some("box-environment"),
        "box.mcp" => Some("box-mcp"),
        "box.file.transfer" => Some("file-transfer"),
        "box.store.sync" => Some("box-store-sync"),
        "background-shell" => Some("background-shell"),
        "remote.shell" => Some("shell"),
        "remote.read" => Some("read"),
        "remote.write" => Some("write"),
        "computer.use" => Some("computer"),
        "browser.use" => Some("browser"),
        "cloud-agent" => Some("cloud-agent"),
        _ => None,
    }
}

fn allowed_executor(executor: &str) -> bool {
    matches!(executor,
        "box-environment" | "box-mcp" | "file-transfer" | "box-store-sync" |
        "background-shell" | "shell" | "read" | "write" | "computer" |
        "screenshot" | "browser" | "cloud-agent" | "external-shell" | "external-read")
}

fn bearer_digest(bearer: &str) -> Result<String, ExecutionError> {
    if bearer.len() < 16 || bearer.len() > MAX_BEARER_BYTES || bearer.bytes().any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control()) {
        return invalid("Remote Runner bearer credential is invalid");
    }
    Ok(format!("{:x}", Sha256::digest(bearer.as_bytes())))
}

fn validate_identity(label: &str, value: &str) -> Result<(), ExecutionError> {
    if value.is_empty() || value.len() > MAX_IDENTITY_BYTES || value.chars().any(char::is_control) {
        return invalid(format!("Remote Runner {label} identity is invalid"));
    }
    Ok(())
}

fn invalid<T>(message: impl Into<String>) -> Result<T, ExecutionError> {
    Err(ExecutionError::InvalidRequest(message.into()))
}

fn io_error(error: std::io::Error) -> ExecutionError {
    ExecutionError::Transport(format!("Remote Runner enrollment journal I/O failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn enrollment(bearer: &str) -> RemoteRunnerEnrollment {
        RemoteRunnerEnrollment::new(
            "cred-1", bearer, "session:account-a", 7, "device-a", 1, 10_000,
            ["box.environment.apply".to_string(), "box.mcp".to_string()],
            ["box-environment".to_string(), "box-mcp".to_string()],
        ).unwrap()
    }

    fn grant(capability: &str) -> RemoteRunnerGrant {
        RemoteRunnerGrant::new(
            format!("grant-{capability}"), "cred-1", format!("op-{capability}"),
            format!("req-{capability}"), capability, "session:account-a", 7,
            "device-a", 2, 9_000,
        ).unwrap()
    }

    fn identity(capability: &str) -> RemoteRequestIdentity {
        RemoteRequestIdentity {
            credential_id: "cred-1".into(), operation_id: format!("op-{capability}"),
            request_id: format!("req-{capability}"), account_fence: "session:account-a".into(),
            account_epoch: 7, permission_grant_id: format!("grant-{capability}"),
            device_id: "device-a".into(),
        }
    }

    #[test]
    fn enrollment_and_one_time_grant_fail_closed_and_survive_reopen() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("enrollment.json");
        let bearer = "runner-secret-token-long-enough";
        let mut auth = DurableRemoteRunnerEnrollmentAuthorizer::open(&path, 10).unwrap();
        let id = identity("box.environment.apply");
        assert!(auth.authorize(bearer, &id, Some("box.environment.apply"), 10).is_err());
        auth.install_enrollment(enrollment(bearer)).unwrap();
        assert!(auth.authorize(bearer, &id, Some("box.environment.apply"), 10).is_err());
        auth.issue_grant(grant("box.environment.apply")).unwrap();
        auth.authorize(bearer, &id, Some("box.environment.apply"), 10).unwrap();
        let mut reopened = DurableRemoteRunnerEnrollmentAuthorizer::open(&path, 11).unwrap();
        reopened.authorize(bearer, &id, Some("box.environment.apply"), 11).unwrap();
        assert_eq!(reopened.state.grants[&id.permission_grant_id].consumed_at_ms, Some(10));
        assert!(reopened.revoke_grant(&id.permission_grant_id, 12).unwrap());
        assert!(reopened.authorize(bearer, &id, Some("box.environment.apply"), 11).is_err());
    }

    #[test]
    fn wrong_account_epoch_device_bearer_and_capability_fail_closed() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("enrollment.json");
        let bearer = "runner-secret-token-long-enough";
        let mut auth = DurableRemoteRunnerEnrollmentAuthorizer::open(&path, 10).unwrap();
        auth.install_enrollment(enrollment(bearer)).unwrap();
        auth.issue_grant(grant("box.mcp")).unwrap();
        let valid = identity("box.mcp");
        assert!(auth.authorize("wrong-secret-token-long-enough", &valid, Some("box.mcp"), 10).is_err());
        let mut stale = valid.clone(); stale.account_epoch = 6;
        assert!(auth.authorize(bearer, &stale, Some("box.mcp"), 10).is_err());
        let mut wrong_device = valid.clone(); wrong_device.device_id = "device-b".into();
        assert!(auth.authorize(bearer, &wrong_device, Some("box.mcp"), 10).is_err());
        assert!(auth.authorize(bearer, &valid, Some("box.environment.apply"), 10).is_err());
        auth.authorize(bearer, &valid, Some("box.mcp"), 10).unwrap();
    }

    #[test]
    fn revoked_or_expired_enrollment_fails_closed() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("enrollment.json");
        let bearer = "runner-secret-token-long-enough";
        let mut auth = DurableRemoteRunnerEnrollmentAuthorizer::open(&path, 10).unwrap();
        auth.install_enrollment(enrollment(bearer)).unwrap();
        auth.issue_grant(grant("box.mcp")).unwrap();
        let id = identity("box.mcp");
        assert!(auth.revoke_enrollment(20).unwrap());
        assert!(auth.authorize(bearer, &id, Some("box.mcp"), 20).is_err());
        let mut expired = DurableRemoteRunnerEnrollmentAuthorizer::open(dir.path().join("expired.json"), 10_000).unwrap();
        assert!(expired.install_enrollment(enrollment(bearer)).is_err());
    }
}
