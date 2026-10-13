use std::collections::BTreeMap;

use super::HostTelemetryProjection;

pub const LOCAL_EXEC_REFUSED_EVENT: &str = "sand.local_exec.refused";
pub const LOCAL_EXEC_FAILED_EVENT: &str = "sand.local_exec.exec_failed";
pub const LOCAL_EXEC_PROVIDER_EVENT: &str = "sand.local_exec.provider";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalExecRefusalCause {
    NoProviders,
    StaleHeartbeat,
    ComputerUnknown,
}

impl LocalExecRefusalCause {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoProviders => "no_providers",
            Self::StaleHeartbeat => "stale_heartbeat",
            Self::ComputerUnknown => "computer_unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalExecRefusalError {
    pub code: &'static str,
    pub domain: &'static str,
    pub retryable: bool,
}

pub fn refusal_error(cause: LocalExecRefusalCause) -> LocalExecRefusalError {
    LocalExecRefusalError {
        code: match cause {
            LocalExecRefusalCause::NoProviders => "SAND-E0111",
            LocalExecRefusalCause::StaleHeartbeat => "SAND-E0112",
            LocalExecRefusalCause::ComputerUnknown => "SAND-E0113",
        },
        domain: "transport",
        retryable: true,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LocalExecRefusedReport {
    pub cause: LocalExecRefusalCause,
    pub site: String,
    pub conversation_id: String,
    pub provider_count: i64,
    pub live_provider_count: i64,
    pub ever_registered: bool,
    pub empty_for_ms: Option<f64>,
}

pub fn local_exec_refused_telemetry(
    report: &LocalExecRefusedReport,
) -> HostTelemetryProjection {
    let error = refusal_error(report.cause);
    let mut metadata = BTreeMap::from([
        ("cause".into(), report.cause.as_str().into()),
        ("site".into(), report.site.clone()),
        ("conversation_id".into(), report.conversation_id.clone()),
        ("provider_count".into(), report.provider_count.to_string()),
        (
            "live_provider_count".into(),
            report.live_provider_count.to_string(),
        ),
        ("ever_registered".into(), report.ever_registered.to_string()),
        ("error_code".into(), error.code.into()),
        ("error_domain".into(), error.domain.into()),
        ("error_retryable".into(), error.retryable.to_string()),
    ]);
    if let Some(value) = report.empty_for_ms {
        metadata.insert("empty_for_ms".into(), value.round().to_string());
    }
    HostTelemetryProjection {
        level: Some("warn"),
        event: Some(LOCAL_EXEC_REFUSED_EVENT),
        metadata,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalExecFailedReport {
    pub error_class: String,
    pub errno: Option<String>,
    pub site: String,
    pub conversation_id: String,
}

pub fn local_exec_failed_telemetry(
    report: &LocalExecFailedReport,
) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::from([
        ("error_class".into(), report.error_class.clone()),
        ("site".into(), report.site.clone()),
        ("surface".into(), "external".into()),
        ("conversation_id".into(), report.conversation_id.clone()),
    ]);
    if let Some(errno) = &report.errno {
        metadata.insert("errno".into(), errno.clone());
    }
    HostTelemetryProjection {
        level: Some("warn"),
        event: Some(LOCAL_EXEC_FAILED_EVENT),
        metadata,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LocalExecProviderReport {
    Registered {
        provider_id: String,
        provider_count: i64,
    },
    Hello {
        provider_id: String,
        provider_count: i64,
        hello_delay_ms: f64,
        computer_id_present: bool,
        rehello: bool,
        supervised: Option<bool>,
        variant: Option<String>,
    },
    Detached {
        provider_id: String,
        provider_count: i64,
        age_ms: f64,
        had_hello: bool,
        has_heartbeat: bool,
        was_live: bool,
        emptied: bool,
    },
}

pub fn local_exec_provider_telemetry(
    report: &LocalExecProviderReport,
) -> HostTelemetryProjection {
    let mut metadata = BTreeMap::new();
    match report {
        LocalExecProviderReport::Registered {
            provider_id,
            provider_count,
        } => {
            metadata.insert("phase".into(), "registered".into());
            metadata.insert("provider_id".into(), provider_id.clone());
            metadata.insert("provider_count".into(), provider_count.to_string());
        }
        LocalExecProviderReport::Hello {
            provider_id,
            provider_count,
            hello_delay_ms,
            computer_id_present,
            rehello,
            supervised,
            variant,
        } => {
            metadata.insert("phase".into(), "hello".into());
            metadata.insert("provider_id".into(), provider_id.clone());
            metadata.insert("provider_count".into(), provider_count.to_string());
            metadata.insert("hello_delay_ms".into(), hello_delay_ms.round().to_string());
            metadata.insert(
                "computer_id_present".into(),
                computer_id_present.to_string(),
            );
            metadata.insert("rehello".into(), rehello.to_string());
            if let Some(value) = supervised {
                metadata.insert("supervised".into(), value.to_string());
            }
            if let Some(value) = variant {
                metadata.insert("variant".into(), value.clone());
            }
        }
        LocalExecProviderReport::Detached {
            provider_id,
            provider_count,
            age_ms,
            had_hello,
            has_heartbeat,
            was_live,
            emptied,
        } => {
            metadata.insert("phase".into(), "detached".into());
            metadata.insert("provider_id".into(), provider_id.clone());
            metadata.insert("provider_count".into(), provider_count.to_string());
            metadata.insert("age_ms".into(), age_ms.round().to_string());
            metadata.insert("had_hello".into(), had_hello.to_string());
            metadata.insert("has_heartbeat".into(), has_heartbeat.to_string());
            metadata.insert("was_live".into(), was_live.to_string());
            metadata.insert("emptied".into(), emptied.to_string());
        }
    }
    HostTelemetryProjection {
        level: Some("info"),
        event: Some(LOCAL_EXEC_PROVIDER_EVENT),
        metadata,
    }
}
