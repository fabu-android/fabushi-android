use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock, RwLock};

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct SessionDiagnostic {
    pub family: String,
    pub kind: String,
    pub metadata: BTreeMap<String, Value>,
}

pub type SessionDiagnosticsReporter = Arc<dyn Fn(&SessionDiagnostic) + Send + Sync + 'static>;

fn reporter_slot() -> &'static RwLock<Option<SessionDiagnosticsReporter>> {
    static REPORTER: OnceLock<RwLock<Option<SessionDiagnosticsReporter>>> = OnceLock::new();
    REPORTER.get_or_init(|| RwLock::new(None))
}

pub fn pin_session_diagnostics_reporter(reporter: Option<SessionDiagnosticsReporter>) {
    if let Ok(mut slot) = reporter_slot().write() {
        *slot = reporter;
    }
}

pub fn report_session_diagnostic(report: &SessionDiagnostic) {
    let reporter = reporter_slot()
        .read()
        .ok()
        .and_then(|slot| slot.as_ref().map(Arc::clone));
    if let Some(reporter) = reporter {
        reporter(report);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn pinned_reporter_receives_structured_diagnostic_and_unpin_stops_delivery() {
        let _guard = test_lock().lock().unwrap();
        let received = Arc::new(Mutex::new(Vec::<SessionDiagnostic>::new()));
        let sink = Arc::clone(&received);
        pin_session_diagnostics_reporter(Some(Arc::new(move |event| {
            sink.lock().unwrap().push(event.clone());
        })));

        let diagnostic = SessionDiagnostic {
            family: "recovery".into(),
            kind: "blob_scan_failed".into(),
            metadata: BTreeMap::from([("agentId".into(), Value::String("agent-a".into()))]),
        };
        report_session_diagnostic(&diagnostic);
        assert_eq!(received.lock().unwrap().as_slice(), &[diagnostic.clone()]);

        pin_session_diagnostics_reporter(None);
        report_session_diagnostic(&diagnostic);
        assert_eq!(received.lock().unwrap().len(), 1);
    }
}
