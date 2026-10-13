use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HostUpgradeMarker {
    pub outcome: Option<String>,
    pub from_version: Option<String>,
    pub to_version: Option<String>,
    pub mode: Option<String>,
    pub reason: Option<String>,
    pub issued_at_ms: Option<f64>,
    pub applied_at_ms: Option<f64>,
    pub swap_ms: Option<f64>,
    pub swap_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostUpgradeMarkerForwardOutcome {
    Absent,
    Skipped,
    ParseError,
    Deferred,
    Emitted,
}

fn nonempty_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn finite_number(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64).filter(|value| value.is_finite())
}

pub fn parse_host_upgrade_marker(raw: &str) -> Option<HostUpgradeMarker> {
    let value: Value = serde_json::from_str(raw).ok()?;
    let record = value.as_object()?;
    let outcome = match record.get("outcome").and_then(Value::as_str) {
        Some("applied") => Some("applied".into()),
        Some("failed") => Some("failed".into()),
        _ => None,
    };
    Some(HostUpgradeMarker {
        outcome,
        from_version: nonempty_string(record.get("fromVersion")),
        to_version: nonempty_string(record.get("toVersion")),
        mode: nonempty_string(record.get("mode")),
        reason: nonempty_string(record.get("reason")),
        issued_at_ms: finite_number(record.get("issuedAtMs")),
        applied_at_ms: finite_number(record.get("appliedAtMs")),
        swap_ms: finite_number(record.get("swapMs")),
        swap_error: nonempty_string(record.get("swapError")),
    })
}

pub fn compute_host_upgrade_metadata(
    marker: &HostUpgradeMarker,
    now_ms: f64,
) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::new();
    for (key, value) in [
        ("from_version", marker.from_version.as_deref()),
        ("to_version", marker.to_version.as_deref()),
        ("mode", marker.mode.as_deref()),
        ("trigger", marker.reason.as_deref()),
    ] {
        if let Some(value) = value {
            metadata.insert(key.into(), value.into());
        }
    }

    if marker.outcome.as_deref() == Some("failed") {
        metadata.insert("outcome".into(), "failed".into());
        metadata.insert("phase".into(), "swap".into());
        metadata.insert(
            "error_class".into(),
            marker
                .swap_error
                .clone()
                .unwrap_or_else(|| "swap-failed".into()),
        );
        return metadata;
    }

    metadata.insert("outcome".into(), "applied".into());
    let has_issued = marker.issued_at_ms.is_some_and(|value| value > 0.0);
    let has_applied = marker.applied_at_ms.is_some_and(|value| value > 0.0);
    if has_issued && has_applied {
        let value = (marker.applied_at_ms.unwrap() - marker.issued_at_ms.unwrap()).max(0.0);
        metadata.insert("deliver_to_apply_ms".into(), format!("{value}"));
    }
    if let Some(swap_ms) = marker.swap_ms {
        metadata.insert("swap_ms".into(), format!("{swap_ms}"));
    }
    if has_issued {
        let value = (now_ms - marker.issued_at_ms.unwrap()).max(0.0);
        metadata.insert("total_ms".into(), format!("{value}"));
    }
    metadata
}

pub trait HostUpgradeMarkerForwardDeps {
    fn read_raw(&self) -> Result<Option<String>, String>;
    fn delete_marker(&self) -> Result<(), String>;
    fn emit(&self, metadata: &BTreeMap<String, String>) -> Result<bool, String>;
    fn warn(&self, message: &str);
    fn now(&self, raw: &str) -> f64;
    fn was_forwarded(&self, raw: &str) -> bool;
    fn mark_forwarded(&self, raw: &str);
    fn on_forwarded(&self, marker: &HostUpgradeMarker);
}

fn delete_if_unchanged(
    deps: &dyn HostUpgradeMarkerForwardDeps,
    raw: &str,
) -> Result<(), String> {
    if deps.read_raw()?.as_deref() == Some(raw) {
        deps.delete_marker()?;
    }
    Ok(())
}

pub fn forward_host_upgrade_marker_with(
    deps: &dyn HostUpgradeMarkerForwardDeps,
) -> Result<HostUpgradeMarkerForwardOutcome, String> {
    let Some(raw) = deps.read_raw()? else {
        return Ok(HostUpgradeMarkerForwardOutcome::Absent);
    };
    if deps.was_forwarded(&raw) {
        return Ok(HostUpgradeMarkerForwardOutcome::Skipped);
    }
    let Some(marker) = parse_host_upgrade_marker(&raw) else {
        deps.warn(&format!(
            "discarding unparseable host-upgrade marker (len={})",
            raw.len()
        ));
        deps.mark_forwarded(&raw);
        delete_if_unchanged(deps, &raw)?;
        return Ok(HostUpgradeMarkerForwardOutcome::ParseError);
    };
    let metadata = compute_host_upgrade_metadata(&marker, deps.now(&raw));
    if !deps.emit(&metadata)? {
        return Ok(HostUpgradeMarkerForwardOutcome::Deferred);
    }
    deps.mark_forwarded(&raw);
    let _ = catch_unwind(AssertUnwindSafe(|| deps.on_forwarded(&marker)));
    delete_if_unchanged(deps, &raw)?;
    Ok(HostUpgradeMarkerForwardOutcome::Emitted)
}
