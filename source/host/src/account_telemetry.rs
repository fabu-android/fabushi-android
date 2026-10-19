use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::VecDeque;

pub const CONSECUTIVE_FAILURES_CAP: u64 = 10_000;
pub const DEGRADED_MS_BUCKET_CAP: u64 = 86_400_000;
pub const DEGRADED_MS_BUCKET_CEILINGS_MS: [u64; 7] =
    [5_000, 30_000, 60_000, 300_000, 1_800_000, 21_600_000, DEGRADED_MS_BUCKET_CAP];
pub const ACCOUNT_TELEMETRY_BUFFER_CAP: usize = 16;
pub const ACCESS_BLOCKED_MS_CAP: u64 = 7 * 24 * 60 * 60 * 1_000;
const ACCESS_BLOCKED_EDGES: [&str; 2] = ["entered", "recovered"];
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionRefreshFailure {
    HttpStatus(u16),
    Network(String),
    BadPayload,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionSignoutCause {
    Policy,
    SessionRevoked,
    Unparseable,
    UserAction,
    LoginCancelled,
    AccountRefused,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionReport {
    RefreshFailed(SessionRefreshFailure),
    RefreshRecovered {
        consecutive_failures: u64,
        degraded_ms: u64,
    },
    RotationRescued,
    SignedOut {
        cause: SessionSignoutCause,
        durable: bool,
    },
    ProtectedStoreUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SigninSignoutCause {
    UserLogout,
    Forced,
    AccountRefused,
    SessionExpired,
    RetainedAfterFailedLogout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SigninLoginReport {
    LoginStarted,
    LoginCompleted,
    LoginFailed(&'static str),
    SignedOut(SigninSignoutCause),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SigninGateReport {
    Gate(&'static str),
    Consult(&'static str),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AccountTelemetryProjection {
    pub stream: String,
    pub level: String,
    pub metadata: Value,
}

#[derive(Default)]
pub struct AccountTelemetryBuffer {
    entries: VecDeque<AccountTelemetryProjection>,
}

impl AccountTelemetryBuffer {
    pub fn report_session(&mut self, report: SessionReport) {
        self.push(session_report_to_projection(report));
    }

    pub fn report_signin(&mut self, report: SigninLoginReport) {
        self.push(signin_login_to_projection(report));
    }

    pub fn report_signin_gate(&mut self, report: SigninGateReport) {
        if let Some(projection) = signin_gate_to_projection(report) {
            self.push(projection);
        }
    }

    pub fn report_access_blocked(
        &mut self,
        edge: &str,
        cause: &str,
        access_reason: &str,
        ms_blocked: Option<u64>,
    ) -> Result<(), String> {
        if !ACCESS_BLOCKED_EDGES.contains(&edge) {
            return Err("invalid access-blocked edge".into());
        }
        if !ACCESS_BLOCKED_CAUSES.contains(&cause) {
            return Err("invalid access-blocked cause".into());
        }
        if !ACCESS_BLOCK_REASONS.contains(&access_reason) {
            return Err("invalid access-blocked reason".into());
        }
        let level = if edge == "entered" && cause != "no_plan" {
            "warn"
        } else {
            "info"
        };
        self.push(AccountTelemetryProjection {
            stream: "access-blocked".into(),
            level: level.into(),
            metadata: json!({
                "edge": edge,
                "cause": cause,
                "access_reason": access_reason,
                "ms_blocked": ms_blocked.map(|value| value.min(ACCESS_BLOCKED_MS_CAP).to_string()),
            }),
        });
        Ok(())
    }

    pub fn snapshot(&self) -> Vec<AccountTelemetryProjection> {
        self.entries.iter().cloned().collect()
    }

    pub fn drain(&mut self) -> Vec<AccountTelemetryProjection> {
        self.entries.drain(..).collect()
    }

    fn push(&mut self, projection: AccountTelemetryProjection) {
        if self.entries.len() >= ACCOUNT_TELEMETRY_BUFFER_CAP {
            self.entries.pop_front();
        }
        self.entries.push_back(projection);
    }
}

pub fn degraded_ms_bucket(degraded_ms: u64) -> u64 {
    DEGRADED_MS_BUCKET_CEILINGS_MS
        .iter()
        .copied()
        .find(|ceiling| degraded_ms < *ceiling)
        .unwrap_or(DEGRADED_MS_BUCKET_CAP)
}

pub fn session_report_to_projection(report: SessionReport) -> AccountTelemetryProjection {
    let (level, metadata) = match report {
        SessionReport::RefreshFailed(failure) => {
            let mut metadata = serde_json::Map::new();
            metadata.insert("phase".into(), json!("refresh_failed"));
            metadata.insert("error_domain".into(), json!("auth"));
            metadata.insert("error_retryable".into(), json!("true"));
            match failure {
                SessionRefreshFailure::HttpStatus(status) => {
                    metadata.insert("error_code".into(), json!("SAND-E0214"));
                    metadata.insert("http_status".into(), json!(status.to_string()));
                }
                SessionRefreshFailure::Network(errno) => {
                    metadata.insert("error_code".into(), json!("SAND-E0215"));
                    metadata.insert("errno".into(), json!(bounded_tag(&errno)));
                }
                SessionRefreshFailure::BadPayload => {
                    metadata.insert("error_code".into(), json!("SAND-E0216"));
                }
            }
            ("warn", Value::Object(metadata))
        }
        SessionReport::RefreshRecovered {
            consecutive_failures,
            degraded_ms,
        } => (
            "info",
            json!({
                "phase": "refresh_recovered",
                "consecutive_failures": consecutive_failures.min(CONSECUTIVE_FAILURES_CAP).to_string(),
                "degraded_ms": degraded_ms_bucket(degraded_ms).to_string(),
            }),
        ),
        SessionReport::RotationRescued => ("info", json!({"phase":"rotation_rescued"})),
        SessionReport::SignedOut { cause, durable } => {
            let cause = session_signout_cause(cause);
            let mut metadata = serde_json::Map::from_iter([
                ("phase".into(), json!("signed_out")),
                ("cause".into(), json!(cause)),
                ("durable".into(), json!(durable.to_string())),
            ]);
            let error_code = match cause {
                "policy" => Some("SAND-E0218"),
                "session_revoked" | "unparseable" => Some("SAND-E0217"),
                _ => None,
            };
            if let Some(code) = error_code {
                metadata.insert("error_code".into(), json!(code));
                metadata.insert("error_domain".into(), json!("auth"));
                metadata.insert("error_retryable".into(), json!("false"));
            }
            let level = if error_code.is_some() || !durable { "warn" } else { "info" };
            (level, Value::Object(metadata))
        }
        SessionReport::ProtectedStoreUnavailable => (
            "warn",
            json!({
                "phase":"keychain_unavailable",
                "error_code":"SAND-E0219",
                "error_domain":"auth",
                "error_retryable":"false",
            }),
        ),
    };
    AccountTelemetryProjection {
        stream: "session".into(),
        level: level.into(),
        metadata,
    }
}

pub fn signin_login_to_projection(report: SigninLoginReport) -> AccountTelemetryProjection {
    let (phase, cause) = match report {
        SigninLoginReport::LoginStarted => ("login_started", None),
        SigninLoginReport::LoginCompleted => ("login_completed", None),
        SigninLoginReport::LoginFailed(cause) => ("login_failed", Some(cause)),
        SigninLoginReport::SignedOut(cause) => ("signed_out", Some(signin_signout_cause(cause))),
    };
    AccountTelemetryProjection {
        stream: "signin".into(),
        level: "warn".into(),
        metadata: json!({"phase":phase,"cause":cause}),
    }
}

pub fn signin_gate_to_projection(report: SigninGateReport) -> Option<AccountTelemetryProjection> {
    let metadata = match report {
        SigninGateReport::Gate(gate) if matches!(gate, "shell" | "onboarding" | "sign-in") => {
            json!({"phase":"boot_gate","gate":gate})
        }
        SigninGateReport::Consult(outcome)
            if matches!(
                outcome,
                "veto-seen" | "veto-roster" | "fresh" | "fail-open-probes" | "fail-open-deadline"
            ) =>
        {
            json!({"phase":"account_consult","outcome":outcome})
        }
        _ => return None,
    };
    Some(AccountTelemetryProjection {
        stream: "signin".into(),
        level: "warn".into(),
        metadata,
    })
}

fn session_signout_cause(cause: SessionSignoutCause) -> &'static str {
    match cause {
        SessionSignoutCause::Policy => "policy",
        SessionSignoutCause::SessionRevoked => "session_revoked",
        SessionSignoutCause::Unparseable => "unparseable",
        SessionSignoutCause::UserAction => "user_action",
        SessionSignoutCause::LoginCancelled => "login_cancelled",
        SessionSignoutCause::AccountRefused => "account_refused",
    }
}

fn signin_signout_cause(cause: SigninSignoutCause) -> &'static str {
    match cause {
        SigninSignoutCause::UserLogout => "user_logout",
        SigninSignoutCause::Forced => "forced",
        SigninSignoutCause::AccountRefused => "account_refused",
        SigninSignoutCause::SessionExpired => "session_expired",
        SigninSignoutCause::RetainedAfterFailedLogout => "retained_after_failed_logout",
    }
}

fn bounded_tag(value: &str) -> String {
    value.trim().chars().take(96).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_projection_preserves_desktop_error_taxonomy_and_buckets() {
        assert_eq!(degraded_ms_bucket(4_999), 5_000);
        assert_eq!(degraded_ms_bucket(5_000), 30_000);
        assert_eq!(degraded_ms_bucket(u64::MAX), DEGRADED_MS_BUCKET_CAP);

        let projection = session_report_to_projection(SessionReport::RefreshFailed(
            SessionRefreshFailure::HttpStatus(401),
        ));
        assert_eq!(projection.level, "warn");
        assert_eq!(projection.metadata["phase"], "refresh_failed");
        assert_eq!(projection.metadata["error_code"], "SAND-E0214");
        assert_eq!(projection.metadata["http_status"], "401");

        let recovered = session_report_to_projection(SessionReport::RefreshRecovered {
            consecutive_failures: u64::MAX,
            degraded_ms: 31_000,
        });
        assert_eq!(
            recovered.metadata["consecutive_failures"],
            CONSECUTIVE_FAILURES_CAP.to_string()
        );
        assert_eq!(recovered.metadata["degraded_ms"], "60000");
    }

    #[test]
    fn access_blocked_projection_is_bounded_and_rejects_free_text() {
        let mut buffer = AccountTelemetryBuffer::default();
        buffer
            .report_access_blocked(
                "entered",
                "unknown",
                "paywallIndividual",
                Some(u64::MAX),
            )
            .unwrap();
        let entered = buffer.snapshot().pop().unwrap();
        assert_eq!(entered.stream, "access-blocked");
        assert_eq!(entered.level, "warn");
        assert_eq!(entered.metadata["access_reason"], "paywallIndividual");
        assert_eq!(
            entered.metadata["ms_blocked"],
            ACCESS_BLOCKED_MS_CAP.to_string(),
        );

        buffer
            .report_access_blocked("recovered", "no_plan", "none", Some(42))
            .unwrap();
        assert_eq!(buffer.snapshot().last().unwrap().level, "info");
        assert!(buffer
            .report_access_blocked("entered", "token=secret", "none", None)
            .is_err());
        assert!(buffer
            .report_access_blocked("entered", "unknown", "secret reason", None)
            .is_err());
    }

    #[test]
    fn signin_projection_rejects_unknown_gate_values() {
        assert!(signin_gate_to_projection(SigninGateReport::Gate("shell")).is_some());
        assert!(signin_gate_to_projection(SigninGateReport::Consult("fresh")).is_some());
        assert!(signin_gate_to_projection(SigninGateReport::Gate("unknown")).is_none());
        assert!(signin_gate_to_projection(SigninGateReport::Consult("unknown")).is_none());
    }

    #[test]
    fn bounded_buffer_drops_oldest_without_collecting_credentials() {
        assert_eq!(ACCOUNT_TELEMETRY_BUFFER_CAP, 16);
        let mut buffer = AccountTelemetryBuffer::default();
        for _ in 0..(ACCOUNT_TELEMETRY_BUFFER_CAP + 5) {
            buffer.report_signin(SigninLoginReport::LoginStarted);
        }
        let snapshot = buffer.snapshot();
        assert_eq!(snapshot.len(), ACCOUNT_TELEMETRY_BUFFER_CAP);
        assert!(snapshot.iter().all(|entry| entry.metadata.get("accessToken").is_none()));
        assert_eq!(buffer.drain().len(), ACCOUNT_TELEMETRY_BUFFER_CAP);
        assert!(buffer.snapshot().is_empty());
    }

    #[test]
    fn signout_warning_semantics_match_desktop_projection() {
        let policy = session_report_to_projection(SessionReport::SignedOut {
            cause: SessionSignoutCause::Policy,
            durable: true,
        });
        assert_eq!(policy.level, "warn");
        assert_eq!(policy.metadata["error_code"], "SAND-E0218");

        let user = session_report_to_projection(SessionReport::SignedOut {
            cause: SessionSignoutCause::UserAction,
            durable: true,
        });
        assert_eq!(user.level, "info");
        assert!(user.metadata.get("error_code").is_none());
    }
}
