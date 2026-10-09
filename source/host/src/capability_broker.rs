use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

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
        for pending in state.pending.values_mut() {
            if pending.state == "pending" {
                pending.state = "outcome_unknown".into();
            }
        }
        let mut broker = Self { path, state };
        if broker.state.pending.values().any(|p| p.state == "outcome_unknown") {
            let recovered = broker.state.pending.values().filter(|p| p.state == "outcome_unknown").cloned().collect::<Vec<_>>();
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
        account_fence: &str,
        now_ms: u64,
    ) -> Result<(), String> {
        if approval_id.trim().is_empty()
            || request_id.trim().is_empty()
            || operation_id.trim().is_empty()
            || capability.trim().is_empty()
            || account_fence.trim().is_empty()
        {
            return Err("approval identity, capability, and account fence are required".into());
        }
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
            outcome: if approved { "approval_consumed".into() } else { "approval_denied".into() },
            reason: (!approved).then(|| "user denied the requested capability".into()),
            at_ms: now_ms,
        });
        self.persist()?;
        Ok(resolved)
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
        b.request_approval("a1", "r1", "o1", "camera", "acct-1", 1).unwrap();
        let allowed = b.resolve_approval("a1", true, "acct-1", 2).unwrap();
        assert_eq!(allowed.state, "allowed_once");
        assert!(b.resolve_approval("a1", true, "acct-1", 3).is_err());

        b.request_approval("a2", "r2", "o2", "location", "acct-1", 4).unwrap();
        assert!(b.resolve_approval("a2", true, "acct-2", 5).is_err());
        let denied = b.resolve_approval("a2", false, "acct-1", 6).unwrap();
        assert_eq!(denied.state, "denied");

        b.request_approval("a3", "r3", "o3", "microphone", "acct-1", 7).unwrap();
        drop(b);
        let mut reopened = CapabilityBroker::open(d.path().join("broker.json"), 8).unwrap();
        assert_eq!(reopened.approval_state("a3"), Some("pending"));
        assert!(reopened.cancel_approval_operation("o3", "user cancelled", 9).unwrap());
        assert_eq!(reopened.approval_state("a3"), Some("cancelled"));
        assert!(reopened.resolve_approval("a3", true, "acct-1", 10).is_err());
    }

    #[test] fn duplicate_and_stale_generation_are_rejected() {
        let (_d,mut b)=broker();
        let c=PendingCapabilityCall{request_id:"r".into(),plugin_id:"p".into(),capability:"c".into(),tool:"t".into(),arguments:Value::Null,account_fence:"a".into(),runtime_generation:2,started_at_ms:1,deadline_at_ms:100,state:"pending".into()};
        b.begin(c.clone()).unwrap(); assert!(b.begin(c).is_err());
        assert!(b.assert_current("r","p","a",3,2).is_err());
    }

    #[test] fn timeout_and_account_fence_fail_closed() {
        let (_d,mut b)=broker();
        b.begin(PendingCapabilityCall{request_id:"r".into(),plugin_id:"p".into(),capability:"c".into(),tool:"t".into(),arguments:Value::Null,account_fence:"acct-1".into(),runtime_generation:1,started_at_ms:10,deadline_at_ms:20,state:"pending".into()}).unwrap();
        assert!(b.assert_current("r","p","acct-2",1,15).is_err());
        assert!(b.assert_current("r","p","acct-1",1,21).is_err());
    }

    #[test] fn cancellation_is_single_terminal_and_persists() {
        let (d,mut b)=broker();
        b.begin(PendingCapabilityCall{request_id:"r".into(),plugin_id:"p".into(),capability:"c".into(),tool:"t".into(),arguments:Value::Null,account_fence:"a".into(),runtime_generation:1,started_at_ms:1,deadline_at_ms:100,state:"pending".into()}).unwrap();
        assert!(b.cancel_request("r","user cancelled",2).unwrap());
        assert!(!b.cancel_request("r","duplicate cancel",3).unwrap());
        assert_eq!(b.request_state("r"),Some("outcome_unknown"));
        let audit=b.audit_len();
        drop(b);
        let b=CapabilityBroker::open(d.path().join("broker.json"),4).unwrap();
        assert_eq!(b.request_state("r"),Some("outcome_unknown"));
        assert_eq!(b.audit_len(),audit);
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
        b.begin(PendingCapabilityCall{request_id:"r".into(),plugin_id:"p".into(),capability:"c".into(),tool:"t".into(),arguments:Value::Null,account_fence:"a".into(),runtime_generation:1,started_at_ms:1,deadline_at_ms:100,state:"pending".into()}).unwrap();
        drop(b);
        let b=CapabilityBroker::open(d.path().join("broker.json"),20).unwrap();
        assert!(b.needs_reconciliation("r"));
    }
}
