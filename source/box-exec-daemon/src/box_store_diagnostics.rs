use std::sync::{Arc, Mutex, OnceLock};
use serde_json::{Map, Value};

type Reporter = Arc<dyn Fn(&Map<String, Value>) + Send + Sync + 'static>;
static REPORTER: OnceLock<Mutex<Option<Reporter>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<Reporter>> {
    REPORTER.get_or_init(|| Mutex::new(None))
}

pub fn pin_box_store_diagnostics_reporter(reporter: Option<Reporter>) {
    *slot()
        .lock()
        .expect("box store diagnostic reporter mutex poisoned") = reporter;
}

pub fn report_box_store_diagnostic(diagnostic: &Map<String, Value>) {
    if let Some(reporter) = slot()
        .lock()
        .expect("box store diagnostic reporter mutex poisoned")
        .clone()
    {
        reporter(diagnostic);
    }
}
