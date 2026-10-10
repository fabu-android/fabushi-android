use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

const MAX_CAPABILITY_ID_BYTES: usize = 256;
const MAX_CAPABILITY_JSON_BYTES: usize = 64 * 1024;

fn validate_identity(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty()
        || value.len() > MAX_CAPABILITY_ID_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(format!("{label} identity is invalid"));
    }
    Ok(())
}

fn validate_scope_json(label: &str, value: &Value) -> Result<(), String> {
    if !value.is_null() && !value.is_object() {
        return Err(format!("{label} must be a JSON object or null"));
    }
    let encoded = serde_json::to_vec(value)
        .map_err(|error| format!("failed to serialize {label}: {error}"))?;
    if encoded.len() > MAX_CAPABILITY_JSON_BYTES {
        return Err(format!("{label} exceeds bounded size"));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityDecision { Allow, NeedsUser, Deny }

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct CapabilityAuditRecord {
    pub request_id: String,
    pub plugin_id: String,
    pub capability: String,
    pub tool: String,
    pub account_fence: String,
    pub runtime_generation: u64,
    pub decision: CapabilityDecision,
    pub outcome: String,
    pub reason: Option<String>,
    pub at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct PendingCapabilityCall {
    pub request_id: String,
    pub plugin_id: String,
    pub capability: String,
    pub tool: String,
    pub arguments: Value,
    #[serde(default)]
    pub required_permissions: BTreeSet<String>,
    pub account_fence: String,
    pub runtime_generation: u64,
    pub started_at_ms: u64,
    pub deadline_at_ms: u64,
    pub state: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PendingCapabilityApproval {
    pub approval_id: String,
    pub request_id: String,
    pub operation_id: String,
    pub capability: String,
    #[serde(default)]
    pub target: Value,
    pub account_fence: String,
    pub state: String,
    pub created_at_ms: u64,
    pub resolved_at_ms: Option<u64>,
}

#[derive(Default, Deserialize, Serialize)]
struct DurableState {
    #[serde(default)]
    pending: BTreeMap<String, PendingCapabilityCall>,
    #[serde(default)]
    approvals: BTreeMap<String, PendingCapabilityApproval>,
    #[serde(default)]
    audit: Vec<CapabilityAuditRecord>,
}

pub struct CapabilityBroker {
    path: PathBuf,
    state: DurableState,
}

impl CapabilityBroker {
    pub fn open(path: impl Into<PathBuf>, now_ms: u64) -> Result<Self, String> {
        let path = path.into();
        let mut state = if path.exists() {
            serde_json::from_slice::<DurableState>(&fs::read(&path).map_err(|e| e.to_string())?)
                .map_err(|e| format!("invalid capability broker store: {e}"))?
        } else { DurableState::default() };
        let mut recovered = Vec::new();
        for pending in state.pending.values_mut() {
            if pending.state == "pending" {
                pending.state = "outcome_unknown".into();
                recovered.push(pending.clone());
            }
        }
        let mut broker = Self { path, state };
        if !recovered.is_empty() {
            for item in recovered {
                broker.state.audit.push(CapabilityAuditRecord {
                    request_id: item.request_id.clone(), plugin_id: item.plugin_id.clone(),
                    capability: item.capability.clone(), tool: item.tool.clone(),
                    account_fence: item.account_fence.clone(), runtime_generation: item.runtime_generation,
                    decision: CapabilityDecision::Deny, outcome: "outcome_unknown".into(),
                    reason: Some("Host restarted with an in-flight side effect; reconciliation required before replay".into()),
                    at_ms: now_ms,
                });
            }
            broker.persist()?;
        }
        Ok(broker)
    }

    pub fn authorize(&mut self, request_id: &str, plugin_id: &str, capability: &str, tool: &str,
        account_fence: &str, runtime_generation: u64, declared: bool, granted: bool, now_ms: u64)
        -> Result<CapabilityDecision, String> {
        validate_identity("request", request_id)?;
        validate_identity("plugin", plugin_id)?;
        validate_identity("capability", capability)?;
        validate_identity("tool", tool)?;
        validate_identity("account fence", account_fence)?;
        if runtime_generation == 0 {
            return Err("runtime generation must be positive".into());
        }
        let (decision, reason) = if !declared {
            (CapabilityDecision::Deny, Some("capability is not declared by the installed immutable release".into()))
        } else if !granted {
            (CapabilityDecision::NeedsUser, Some("capability requires a current persisted grant".into()))
        } else {
            (CapabilityDecision::Allow, None)
        };
        self.state.audit.push(CapabilityAuditRecord {
            request_id: request_id.into(), plugin_id: plugin_id.into(), capability: capability.into(),
            tool: tool.into(), account_fence: account_fence.into(), runtime_generation,
            decision, outcome: "authorized".into(), reason, at_ms: now_ms,
        });
        self.persist()?;
        Ok(decision)
    }

    pub fn request_approval(
        &mut self,
        approval_id: &str,
        request_id: &str,
        operation_id: &str,
        capability: &str,
        target: Value,
        account_fence: &str,
        now_ms: u64,
    ) -> Result<(), String> {
        validate_identity("approval", approval_id)?;
        validate_identity("request", request_id)?;
        validate_identity("operation", operation_id)?;
        validate_identity("capability", capability)?;
        validate_identity("account fence", account_fence)?;
        validate_scope_json("capability target", &target)?;
        if self.state.approvals.contains_key(approval_id) {
            return Err("approval identity is duplicate or already consumed".into());
        }
        if self.state.approvals.values().any(|approval| {
            approval.operation_id == operation_id && approval.state == "pending"
        }) {
            return Err("operation already has a pending capability approval".into());
        }
        self.state.approvals.insert(
            approval_id.into(),
            PendingCapabilityApproval {
                approval_id: approval_id.into(),
                request_id: request_id.into(),
                operation_id: operation_id.into(),
                capability: capability.into(),
                target,
                account_fence: account_fence.into(),
                state: "pending".into(),
                created_at_ms: now_ms,
                resolved_at_ms: None,
            },
        );
        self.state.audit.push(CapabilityAuditRecord {
            request_id: request_id.into(),
            plugin_id: "feature".into(),
            capability: capability.into(),
            tool: "capability.request".into(),
            account_fence: account_fence.into(),
            runtime_generation: 0,
            decision: CapabilityDecision::NeedsUser,
            outcome: "approval_requested".into(),
            reason: Some("capability requires explicit one-time user approval".into()),
            at_ms: now_ms,
        });
        self.persist()
    }

    pub fn resolve_approval(
        &mut self,
        approval_id: &str,
        approved: bool,
        current_account_fence: &str,
        now_ms: u64,
    ) -> Result<PendingCapabilityApproval, String> {
        let approval = self
            .state
            .approvals
            .get_mut(approval_id)
            .ok_or("approval is unknown, stale, cancelled, or already consumed")?;
        if approval.state != "pending" {
            return Err(format!("approval is already {}", approval.state));
        }
        if approval.account_fence != current_account_fence {
            return Err("stale approval fenced by account identity".into());
        }
        approval.state = if approved { "allowed_once" } else { "denied" }.into();
        approval.resolved_at_ms = Some(now_ms);
        let resolved = approval.clone();
        self.state.audit.push(CapabilityAuditRecord {
            request_id: resolved.request_id.clone(),
            plugin_id: "feature".into(),
            capability: resolved.capability.clone(),
            tool: "capability.request".into(),
            account_fence: resolved.account_fence.clone(),
            runtime_generation: 0,
            decision: if approved { CapabilityDecision::Allow } else { CapabilityDecision::Deny },
            outcome: if approved { "approval_allowed_once".into() } else { "approval_denied".into() },
            reason: (!approved).then(|| "user denied the requested capability".into()),
            at_ms: now_ms,
        });
        self.persist()?;
        Ok(resolved)
    }

    pub fn consume_approval_for_dispatch(
        &mut self,
        approval_id: &str,
        operation_id: &str,
        request_id: &str,
        capability: &str,
        current_account_fence: &str,
        now_ms: u64,
    ) -> Result<PendingCapabilityApproval, String> {
        validate_identity("approval", approval_id)?;
        validate_identity("operation", operation_id)?;
        validate_identity("request", request_id)?;
        validate_identity("capability", capability)?;
        validate_identity("account fence", current_account_fence)?;
        let approval = self
            .state
            .approvals
            .get_mut(approval_id)
            .ok_or("approval is unknown, stale, cancelled, or already consumed")?;
        if approval.state != "allowed_once" {
            return Err(format!("approval is not dispatchable from state {}", approval.state));
        }
        if approval.operation_id != operation_id
            || approval.request_id != request_id
            || approval.capability != capability
        {
            return Err("approval identity does not match the remote dispatch".into());
        }
        if approval.account_fence != current_account_fence {
            return Err("stale approval fenced by account identity".into());
        }
        approval.state = "consumed".into();
        approval.resolved_at_ms = Some(now_ms);
        let consumed = approval.clone();
        self.state.audit.push(CapabilityAuditRecord {
            request_id: consumed.request_id.clone(),
            plugin_id: "feature".into(),
            capability: consumed.capability.clone(),
            tool: "capability.dispatch".into(),
            account_fence: consumed.account_fence.clone(),
            runtime_generation: 0,
            decision: CapabilityDecision::Allow,
            outcome: "approval_consumed".into(),
            reason: Some("one-time capability grant consumed immediately before dispatch".into()),
            at_ms: now_ms,
        });
        self.persist()?;
        Ok(consumed)
    }

    pub fn cancel_approval_operation(
        &mut self,
        operation_id: &str,
        reason: &str,
        now_ms: u64,
    ) -> Result<bool, String> {
        let approval_id = self.state.approvals.iter().find_map(|(id, approval)| {
            (approval.operation_id == operation_id && approval.state == "pending").then_some(id.clone())
        });
        let Some(approval_id) = approval_id else { return Ok(false); };
        let approval = self.state.approvals.get_mut(&approval_id).expect("located approval");
        approval.state = "cancelled".into();
        approval.resolved_at_ms = Some(now_ms);
        let cancelled = approval.clone();
        self.state.audit.push(CapabilityAuditRecord {
            request_id: cancelled.request_id,
            plugin_id: "feature".into(),
            capability: cancelled.capability,
            tool: "capability.request".into(),
            account_fence: cancelled.account_fence,
            runtime_generation: 0,
            decision: CapabilityDecision::Deny,
            outcome: "approval_cancelled".into(),
            reason: Some(reason.into()),
            at_ms: now_ms,
        });
        self.persist()?;
        Ok(true)
    }

    pub fn begin(&mut self, call: PendingCapabilityCall) -> Result<(), String> {
        validate_identity("request", &call.request_id)?;
        validate_identity("plugin", &call.plugin_id)?;
        validate_identity("capability", &call.capability)?;
        validate_identity("tool", &call.tool)?;
        validate_identity("account fence", &call.account_fence)?;
        validate_scope_json("runtime.call arguments", &call.arguments)?;
        for permission in &call.required_permissions {
            validate_identity("required permission", permission)?;
        }
        if call.runtime_generation == 0 {
            return Err("runtime generation must be positive".into());
        }
        if call.deadline_at_ms <= call.started_at_ms {
            return Err("capability deadline must be after start".into());
        }
        if call.state != "pending" {
            return Err("new capability call must start pending".into());
        }
        if let Some(existing) = self.state.pending.get(&call.request_id) {
            if existing == &call && existing.state != "pending" {
                return Err(format!("request {} already has terminal/reconciliation state {}", call.request_id, existing.state));
            }
            return Err(format!("duplicate pending capability request {}", call.request_id));
        }
        self.state.pending.insert(call.request_id.clone(), call);
        self.persist()
    }

    pub fn settle(&mut self, request_id: &str, outcome: &str, reason: Option<String>, now_ms: u64) -> Result<(), String> {
        let call = self.state.pending.get_mut(request_id).ok_or("capability request is not pending")?;
        if call.state != "pending" { return Err(format!("capability request is already {}", call.state)); }
        call.state = outcome.into();
        self.state.audit.push(CapabilityAuditRecord {
            request_id: call.request_id.clone(), plugin_id: call.plugin_id.clone(), capability: call.capability.clone(),
            tool: call.tool.clone(), account_fence: call.account_fence.clone(), runtime_generation: call.runtime_generation,
            decision: CapabilityDecision::Allow, outcome: outcome.into(), reason, at_ms: now_ms,
        });
        self.persist()
    }


    pub fn cancel_request(&mut self, request_id: &str, reason: &str, now_ms: u64) -> Result<bool, String> {
        let Some(call) = self.state.pending.get(request_id) else { return Ok(false); };
        if call.state != "pending" { return Ok(false); }
        self.settle(
            request_id,
            "outcome_unknown",
            Some(format!(
                "cancellation requested while execution may be in flight: {reason}; reconcile before replay"
            )),
            now_ms,
        )?;
        Ok(true)
    }

    #[cfg(test)]
    fn request_state(&self, request_id: &str) -> Option<&str> {
        self.state.pending.get(request_id).map(|p| p.state.as_str())
    }

    #[cfg(test)]
    fn audit_len(&self) -> usize { self.state.audit.len() }

    #[cfg(test)]
    fn approval_state(&self, approval_id: &str) -> Option<&str> {
        self.state.approvals.get(approval_id).map(|approval| approval.state.as_str())
    }

    pub fn cancel_plugin(&mut self, plugin_id: &str, reason: &str, now_ms: u64) -> Result<usize, String> {
        let ids = self.state.pending.iter().filter_map(|(id,p)| (p.plugin_id==plugin_id && p.state=="pending").then_some(id.clone())).collect::<Vec<_>>();
        for id in &ids {
            self.settle(
                id,
                "outcome_unknown",
                Some(format!(
                    "plugin stop/revoke requested while execution may be in flight: {reason}; reconcile before replay"
                )),
                now_ms,
            )?;
        }
        Ok(ids.len())
    }

    pub fn assert_current(&self, request_id: &str, plugin_id: &str, account_fence: &str, runtime_generation: u64, now_ms: u64) -> Result<(), String> {
        let call=self.state.pending.get(request_id).ok_or("capability request disappeared")?;
        if call.state!="pending" { return Err(format!("capability request is {}", call.state)); }
        if call.plugin_id!=plugin_id || call.account_fence!=account_fence || call.runtime_generation!=runtime_generation {
            return Err("stale capability callback fenced by account/runtime generation".into());
        }
        if now_ms > call.deadline_at_ms { return Err("capability request exceeded bounded deadline".into()); }
        Ok(())
    }

    pub fn needs_reconciliation(&self, request_id: &str) -> bool {
        self.state.pending.get(request_id).is_some_and(|p| p.state=="outcome_unknown")
    }

    fn persist(&self) -> Result<(), String> {
        if let Some(parent)=self.path.parent(){ fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
        let tmp=self.path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(&self.state).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        fs::rename(tmp, &self.path).map_err(|e| e.to_string())
    }
}


#[derive(Clone)]
pub struct SharedCapabilityBroker {
    inner: Arc<Mutex<CapabilityBroker>>,
}

impl SharedCapabilityBroker {
    pub fn open(path: impl Into<PathBuf>, now_ms: u64) -> Result<Self, String> {
        Ok(Self {
            inner: Arc::new(Mutex::new(CapabilityBroker::open(path, now_ms)?)),
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, CapabilityBroker>, String> {
        self.inner
            .lock()
            .map_err(|_| "capability broker lock poisoned".to_string())
    }

    pub fn authorize(
        &self,
        request_id: &str,
        plugin_id: &str,
        capability: &str,
        tool: &str,
        account_fence: &str,
        runtime_generation: u64,
        declared: bool,
        granted: bool,
        now_ms: u64,
    ) -> Result<CapabilityDecision, String> {
        self.lock()?.authorize(
            request_id,
            plugin_id,
            capability,
            tool,
            account_fence,
            runtime_generation,
            declared,
            granted,
            now_ms,
        )
    }

    pub fn request_approval(
        &self,
        approval_id: &str,
        request_id: &str,
        operation_id: &str,
        capability: &str,
        target: Value,
        account_fence: &str,
        now_ms: u64,
    ) -> Result<(), String> {
        self.lock()?.request_approval(
            approval_id,
            request_id,
            operation_id,
            capability,
            target,
            account_fence,
            now_ms,
        )
    }

    pub fn resolve_approval(
        &self,
        approval_id: &str,
        approved: bool,
        current_account_fence: &str,
        now_ms: u64,
    ) -> Result<PendingCapabilityApproval, String> {
        self.lock()?.resolve_approval(
            approval_id,
            approved,
            current_account_fence,
            now_ms,
        )
    }

    pub fn consume_approval_for_dispatch(
        &self,
        approval_id: &str,
        operation_id: &str,
        request_id: &str,
        capability: &str,
        current_account_fence: &str,
        now_ms: u64,
    ) -> Result<PendingCapabilityApproval, String> {
        self.lock()?.consume_approval_for_dispatch(
            approval_id,
            operation_id,
            request_id,
            capability,
            current_account_fence,
            now_ms,
        )
    }

    pub fn cancel_approval_operation(
        &self,
        operation_id: &str,
        reason: &str,
        now_ms: u64,
    ) -> Result<bool, String> {
        self.lock()?.cancel_approval_operation(operation_id, reason, now_ms)
    }

    pub fn begin(&self, call: PendingCapabilityCall) -> Result<(), String> {
        self.lock()?.begin(call)
    }

    pub fn settle(
        &self,
        request_id: &str,
        outcome: &str,
        reason: Option<String>,
        now_ms: u64,
    ) -> Result<(), String> {
        self.lock()?.settle(request_id, outcome, reason, now_ms)
    }

    pub fn cancel_request(
        &self,
        request_id: &str,
        reason: &str,
        now_ms: u64,
    ) -> Result<bool, String> {
        self.lock()?.cancel_request(request_id, reason, now_ms)
    }

    pub fn cancel_plugin(
        &self,
        plugin_id: &str,
        reason: &str,
        now_ms: u64,
    ) -> Result<usize, String> {
        self.lock()?.cancel_plugin(plugin_id, reason, now_ms)
    }

    pub fn assert_current(
        &self,
        request_id: &str,
        plugin_id: &str,
        account_fence: &str,
        runtime_generation: u64,
        now_ms: u64,
    ) -> Result<(), String> {
        self.lock()?.assert_current(
            request_id,
            plugin_id,
            account_fence,
            runtime_generation,
            now_ms,
        )
    }

    pub fn needs_reconciliation(&self, request_id: &str) -> bool {
        self.lock()
            .map(|broker| broker.needs_reconciliation(request_id))
            .unwrap_or(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn broker() -> (tempfile::TempDir, CapabilityBroker) {
        let d=tempfile::tempdir().unwrap();
        let b=CapabilityBroker::open(d.path().join("broker.json"), 10).unwrap();
        (d,b)
    }
    #[test] fn allow_needs_user_deny_are_audited() {
        let (_d,mut b)=broker();
        assert_eq!(b.authorize("1","p","c","t","a",1,true,true,1).unwrap(),CapabilityDecision::Allow);
        assert_eq!(b.authorize("2","p","c","t","a",1,true,false,2).unwrap(),CapabilityDecision::NeedsUser);
        assert_eq!(b.authorize("3","p","c","t","a",1,false,true,3).unwrap(),CapabilityDecision::Deny);
    }
    #[test] fn approval_accept_decline_duplicate_account_and_restart_are_fail_closed() {
        let (d, mut b) = broker();
        b.request_approval("a1", "r1", "o1", "camera", json!({"device":"camera"}), "acct-1", 1).unwrap();
        let allowed = b.resolve_approval("a1", true, "acct-1", 2).unwrap();
        assert_eq!(allowed.state, "allowed_once");
        assert!(b.resolve_approval("a1", true, "acct-1", 3).is_err());

        b.request_approval("a2", "r2", "o2", "location", json!({"precision":"coarse"}), "acct-1", 4).unwrap();
        assert!(b.resolve_approval("a2", true, "acct-2", 5).is_err());
        let denied = b.resolve_approval("a2", false, "acct-1", 6).unwrap();
        assert_eq!(denied.state, "denied");

        b.request_approval("a3", "r3", "o3", "microphone", json!({"source":"mic"}), "acct-1", 7).unwrap();
        drop(b);
        let mut reopened = CapabilityBroker::open(d.path().join("broker.json"), 8).unwrap();
        assert_eq!(reopened.approval_state("a3"), Some("pending"));
        assert!(reopened.cancel_approval_operation("o3", "user cancelled", 9).unwrap());
        assert_eq!(reopened.approval_state("a3"), Some("cancelled"));
        assert!(reopened.resolve_approval("a3", true, "acct-1", 10).is_err());
    }

    #[test]
    fn allowed_once_grant_is_consumed_only_at_exact_dispatch_identity() {
        let (d, mut b) = broker();
        b.request_approval(
            "grant-1",
            "request-1",
            "operation-1",
            "computer.use",
            json!({"executionTarget":"remote_box","deviceId":"device-1"}),
            "session:account-a",
            1,
        )
        .unwrap();
        let allowed = b
            .resolve_approval("grant-1", true, "session:account-a", 2)
            .unwrap();
        assert_eq!(allowed.state, "allowed_once");
        assert_eq!(b.approval_state("grant-1"), Some("allowed_once"));

        assert!(b
            .consume_approval_for_dispatch(
                "grant-1",
                "operation-1",
                "wrong-request",
                "computer.use",
                "session:account-a",
                3,
            )
            .is_err());
        assert_eq!(b.approval_state("grant-1"), Some("allowed_once"));

        let consumed = b
            .consume_approval_for_dispatch(
                "grant-1",
                "operation-1",
                "request-1",
                "computer.use",
                "session:account-a",
                4,
            )
            .unwrap();
        assert_eq!(consumed.state, "consumed");
        assert!(b
            .consume_approval_for_dispatch(
                "grant-1",
                "operation-1",
                "request-1",
                "computer.use",
                "session:account-a",
                5,
            )
            .is_err());

        drop(b);
        let reopened = CapabilityBroker::open(d.path().join("broker.json"), 6).unwrap();
        assert_eq!(reopened.approval_state("grant-1"), Some("consumed"));
    }

    #[test]
    fn allowed_once_grant_is_fenced_on_account_switch_before_dispatch() {
        let (_d, mut b) = broker();
        b.request_approval(
            "grant-account",
            "request-account",
            "operation-account",
            "computer.use",
            json!({"executionTarget":"remote_box","deviceId":"device-1"}),
            "session:account-a",
            1,
        )
        .unwrap();
        b.resolve_approval("grant-account", true, "session:account-a", 2)
            .unwrap();
        assert!(b
            .consume_approval_for_dispatch(
                "grant-account",
                "operation-account",
                "request-account",
                "computer.use",
                "session:account-b",
                3,
            )
            .is_err());
        assert_eq!(b.approval_state("grant-account"), Some("allowed_once"));
    }

    #[test] fn duplicate_and_stale_generation_are_rejected() {
        let (_d,mut b)=broker();
        let c=PendingCapabilityCall{request_id:"r".into(),plugin_id:"p".into(),capability:"c".into(),tool:"t".into(),arguments:json!({}),required_permissions:BTreeSet::new(),account_fence:"a".into(),runtime_generation:2,started_at_ms:1,deadline_at_ms:100,state:"pending".into()};
        b.begin(c.clone()).unwrap(); assert!(b.begin(c).is_err());
        assert!(b.assert_current("r","p","a",3,2).is_err());
    }

    #[test] fn timeout_and_account_fence_fail_closed() {
        let (_d,mut b)=broker();
        b.begin(PendingCapabilityCall{request_id:"r".into(),plugin_id:"p".into(),capability:"c".into(),tool:"t".into(),arguments:json!({}),required_permissions:BTreeSet::new(),account_fence:"acct-1".into(),runtime_generation:1,started_at_ms:10,deadline_at_ms:20,state:"pending".into()}).unwrap();
        assert!(b.assert_current("r","p","acct-2",1,15).is_err());
        assert!(b.assert_current("r","p","acct-1",1,21).is_err());
    }

    #[test] fn cancellation_is_single_terminal_and_persists() {
        let (d,mut b)=broker();
        b.begin(PendingCapabilityCall{request_id:"r".into(),plugin_id:"p".into(),capability:"c".into(),tool:"t".into(),arguments:json!({}),required_permissions:BTreeSet::new(),account_fence:"a".into(),runtime_generation:1,started_at_ms:1,deadline_at_ms:100,state:"pending".into()}).unwrap();
        assert!(b.cancel_request("r","user cancelled",2).unwrap());
        assert!(!b.cancel_request("r","duplicate cancel",3).unwrap());
        assert_eq!(b.request_state("r"),Some("outcome_unknown"));
        let audit=b.audit_len();
        drop(b);
        let b=CapabilityBroker::open(d.path().join("broker.json"),4).unwrap();
        assert_eq!(b.request_state("r"),Some("outcome_unknown"));
        assert_eq!(b.audit_len(),audit);
    }

    #[test] fn parameter_scope_and_permission_identity_are_validated() {
        let (_d, mut b) = broker();
        assert!(b.begin(PendingCapabilityCall {
            request_id:"invalid-args".into(), plugin_id:"p".into(), capability:"plugin.p.tool.t".into(),
            tool:"t".into(), arguments:json!(["not-an-object"]), required_permissions:BTreeSet::new(),
            account_fence:"session:test".into(), runtime_generation:1, started_at_ms:1,
            deadline_at_ms:100, state:"pending".into(),
        }).is_err());
        let mut invalid_permission = BTreeSet::new();
        invalid_permission.insert("bad\npermission".into());
        assert!(b.begin(PendingCapabilityCall {
            request_id:"invalid-permission".into(), plugin_id:"p".into(), capability:"plugin.p.tool.t".into(),
            tool:"t".into(), arguments:json!({"target":"same"}), required_permissions:invalid_permission,
            account_fence:"session:test".into(), runtime_generation:1, started_at_ms:1,
            deadline_at_ms:100, state:"pending".into(),
        }).is_err());
        assert!(b.request_approval(
            "approval-params", "request-params", "operation-params", "camera",
            json!(["unbounded-shape"]), "session:test", 2
        ).is_err());
    }

    #[test] fn audit_survives_restart() {
        let (d,mut b)=broker();
        b.authorize("1","p","c","t","a",1,true,true,1).unwrap();
        let before=b.audit_len(); drop(b);
        let b=CapabilityBroker::open(d.path().join("broker.json"),2).unwrap();
        assert_eq!(b.audit_len(),before);
    }

    #[test] fn restart_turns_inflight_side_effect_into_outcome_unknown() {
        let (d,mut b)=broker();
        b.begin(PendingCapabilityCall{request_id:"r".into(),plugin_id:"p".into(),capability:"c".into(),tool:"t".into(),arguments:json!({}),required_permissions:BTreeSet::new(),account_fence:"a".into(),runtime_generation:1,started_at_ms:1,deadline_at_ms:100,state:"pending".into()}).unwrap();
        drop(b);
        let b=CapabilityBroker::open(d.path().join("broker.json"),20).unwrap();
        assert!(b.needs_reconciliation("r"));
    }
}
