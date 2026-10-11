use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostDiagnostic { pub code: String, pub detail: String }

pub type DiagnosticReporter = Arc<dyn Fn(HostDiagnostic) + Send + Sync>;

#[derive(Default)]
pub struct HostDiagnostics {
    reporter: Mutex<Option<DiagnosticReporter>>,
}

impl HostDiagnostics {
    pub fn pin_reporter(&self, reporter: DiagnosticReporter) { *self.reporter.lock().expect("diagnostic lock poisoned") = Some(reporter); }
    pub fn report(&self, diagnostic: HostDiagnostic) {
        if let Some(reporter) = self.reporter.lock().expect("diagnostic lock poisoned").as_ref() { reporter(diagnostic); }
    }
}
