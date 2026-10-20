use std::sync::{Arc, Mutex, OnceLock};

use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct HostDiagnostic {
    pub kind: String,
    pub fields: Map<String, Value>,
}

type Reporter = Arc<dyn Fn(&HostDiagnostic) + Send + Sync + 'static>;
static REPORTER: OnceLock<Mutex<Option<Reporter>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<Reporter>> {
    REPORTER.get_or_init(|| Mutex::new(None))
}

pub fn pin_host_diagnostics_reporter(reporter: Option<Reporter>) {
    *slot().lock().expect("host diagnostics reporter mutex poisoned") = reporter;
}

pub fn report_host_diagnostic(diagnostic: &HostDiagnostic) {
    let reporter = slot()
        .lock()
        .expect("host diagnostics reporter mutex poisoned")
        .clone();
    if let Some(reporter) = reporter {
        reporter(diagnostic);
    }
}
