use std::collections::BTreeMap;

const ACCESS_BLOCKED_MS_CAP: u64 = 7 * 24 * 60 * 60 * 1_000;
const ACCESS_BLOCKED_CAUSES: [&str; 5] = [
    "wrong_account_suspected",
    "seat_revoked",
    "plan_expired",
    "no_plan",
    "unknown",
];
const ACCESS_BLOCK_REASONS: [&str; 10] = [
    "unspecified",
    "none",
    "teamPrivacyMode",
    "teamSetupRequired",
    "teamAccessRequired",
    "notOffered",
    "freeTrialAvailable",
    "paywallIndividual",
    "paywallTeamMember",
    "paywallTeamAdmin",
];
const AGENT_LOAD_OUTCOMES: [&str; 3] = ["ready", "superseded", "failed"];
const AGENT_LOAD_BOX_STATES: [&str; 7] = [
    "pulling",
    "starting",
    "running",
    "local",
    "sleeping",
    "off",
    "unknown",
];
const AGENTS_UNREACHABLE_PHASES: [&str; 3] = ["entered", "retry", "recovered"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TelemetryLevel {
    Info,
    Warn,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructuredTelemetryEvent {
    pub level: TelemetryLevel,
    pub metadata: BTreeMap<String, String>,
}

pub fn access_blocked_event(
    edge: &str,
    cause: &str,
    access_reason: &str,
    ms_blocked: Option<f64>,
) -> Result<StructuredTelemetryEvent, String> {
    if !matches!(edge, "entered" | "recovered") {
        return Err("access-blocked edge is invalid".into());
    }
    if !ACCESS_BLOCKED_CAUSES.contains(&cause) {
        return Err("access-blocked cause is invalid".into());
    }
    if !ACCESS_BLOCK_REASONS.contains(&access_reason) {
        return Err("access-blocked reason is invalid".into());
    }
    let bounded_ms = match ms_blocked {
        Some(value) if value.is_finite() && value >= 0.0 => {
            Some((value.round() as u64).min(ACCESS_BLOCKED_MS_CAP))
        }
        Some(_) => return Err("access-blocked duration must be finite and nonnegative".into()),
        None => None,
    };
    let mut metadata = BTreeMap::from([
        ("edge".into(), edge.into()),
        ("cause".into(), cause.into()),
        ("access_reason".into(), access_reason.into()),
    ]);
    if let Some(value) = bounded_ms {
        metadata.insert("ms_blocked".into(), value.to_string());
    }
    Ok(StructuredTelemetryEvent {
        level: if edge == "entered" && cause != "no_plan" {
            TelemetryLevel::Warn
        } else {
            TelemetryLevel::Info
        },
        metadata,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn agent_load_event(
    duration_ms: f64,
    open_rpc_ms: Option<f64>,
    queue_ms: Option<f64>,
    render_ms: Option<f64>,
    cold: bool,
    stale: Option<bool>,
    box_state: &str,
    outcome: &str,
) -> Result<StructuredTelemetryEvent, String> {
    for (name, value) in [
        ("durationMs", Some(duration_ms)),
        ("openRpcMs", open_rpc_ms),
        ("queueMs", queue_ms),
        ("renderMs", render_ms),
    ] {
        if let Some(value) = value {
            if !value.is_finite() || value < 0.0 {
                return Err(format!("{name} must be finite and nonnegative"));
            }
        }
    }
    if !AGENT_LOAD_OUTCOMES.contains(&outcome) {
        return Err("agent-load outcome is invalid".into());
    }
    let mut metadata = BTreeMap::from([
        ("duration_ms".into(), duration_ms.round().to_string()),
        ("cold".into(), cold.to_string()),
        (
            "box_state".into(),
            if AGENT_LOAD_BOX_STATES.contains(&box_state) {
                box_state
            } else {
                "other"
            }
            .into(),
        ),
        ("outcome".into(), outcome.into()),
    ]);
    for (key, value) in [
        ("open_rpc_ms", open_rpc_ms),
        ("queue_ms", queue_ms),
        ("render_ms", render_ms),
    ] {
        if let Some(value) = value {
            metadata.insert(key.into(), value.round().to_string());
        }
    }
    if let Some(stale) = stale {
        metadata.insert("stale".into(), stale.to_string());
    }
    Ok(StructuredTelemetryEvent {
        level: if outcome == "failed" {
            TelemetryLevel::Warn
        } else {
            TelemetryLevel::Info
        },
        metadata,
    })
}

pub fn agents_unreachable_event(
    phase: &str,
    is_manual: bool,
    error_class: Option<&str>,
    ms_since_last_reached: Option<f64>,
) -> Result<StructuredTelemetryEvent, String> {
    if !AGENTS_UNREACHABLE_PHASES.contains(&phase) {
        return Err("agents-unreachable phase is invalid".into());
    }
    if let Some(value) = ms_since_last_reached {
        if !value.is_finite() || value < 0.0 {
            return Err("agents-unreachable duration must be finite and nonnegative".into());
        }
    }
    let mut metadata = BTreeMap::from([
        ("phase".into(), phase.into()),
        ("is_manual".into(), is_manual.to_string()),
    ]);
    if let Some(error_class) = error_class {
        metadata.insert("error_class".into(), error_class.into());
    }
    if let Some(value) = ms_since_last_reached {
        metadata.insert("ms_since_last_reached".into(), value.round().to_string());
    }
    Ok(StructuredTelemetryEvent {
        level: if phase == "entered" {
            TelemetryLevel::Warn
        } else {
            TelemetryLevel::Info
        },
        metadata,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_blocked_validates_and_caps_duration() {
        let event = access_blocked_event(
            "entered",
            "seat_revoked",
            "teamAccessRequired",
            Some((ACCESS_BLOCKED_MS_CAP + 50_000) as f64),
        )
        .unwrap();
        assert_eq!(event.level, TelemetryLevel::Warn);
        assert_eq!(
            event.metadata.get("ms_blocked").map(String::as_str),
            Some(ACCESS_BLOCKED_MS_CAP.to_string().as_str()),
        );
        assert!(access_blocked_event("bad", "seat_revoked", "teamAccessRequired", None).is_err());
        assert!(access_blocked_event("entered", "bad", "teamAccessRequired", None).is_err());
        assert!(access_blocked_event("entered", "seat_revoked", "bad", None).is_err());
    }

    #[test]
    fn agent_load_bounds_unknown_box_state_without_rejecting_it() {
        let event = agent_load_event(
            125.4,
            Some(5.2),
            None,
            Some(80.6),
            true,
            Some(false),
            "future-state",
            "ready",
        )
        .unwrap();
        assert_eq!(event.level, TelemetryLevel::Info);
        assert_eq!(event.metadata.get("duration_ms").map(String::as_str), Some("125"));
        assert_eq!(event.metadata.get("render_ms").map(String::as_str), Some("81"));
        assert_eq!(event.metadata.get("box_state").map(String::as_str), Some("other"));
        assert!(agent_load_event(-1.0, None, None, None, false, None, "local", "ready").is_err());
        assert!(agent_load_event(1.0, None, None, None, false, None, "local", "bad").is_err());
    }

    #[test]
    fn unreachable_event_keeps_desktop_warn_edge_and_nonnegative_contract() {
        let entered = agents_unreachable_event("entered", false, Some("timeout"), Some(12.6)).unwrap();
        assert_eq!(entered.level, TelemetryLevel::Warn);
        assert_eq!(
            entered.metadata.get("ms_since_last_reached").map(String::as_str),
            Some("13"),
        );
        assert_eq!(
            agents_unreachable_event("recovered", true, None, None)
                .unwrap()
                .level,
            TelemetryLevel::Info,
        );
        assert!(agents_unreachable_event("bad", false, None, None).is_err());
        assert!(agents_unreachable_event("retry", false, None, Some(-1.0)).is_err());
    }
}
