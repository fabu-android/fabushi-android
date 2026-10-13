use std::panic::Location;
use std::sync::{Arc, Mutex, OnceLock};

pub const SAND_INVARIANT_VIOLATION_NAME: &str = "SandInvariantViolation";
pub const STRIPPED_INVARIANT_MESSAGE: &str =
    "Invariant violation (message stripped in packaged builds; the stack identifies the site)";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandInvariantReport {
    pub name: String,
    pub frame: Option<String>,
}

type InvariantReporter = Arc<dyn Fn(&SandInvariantReport) + Send + Sync + 'static>;

fn reporter_slot() -> &'static Mutex<Option<InvariantReporter>> {
    static REPORTER: OnceLock<Mutex<Option<InvariantReporter>>> = OnceLock::new();
    REPORTER.get_or_init(|| Mutex::new(None))
}

pub struct InstalledInvariantReporter {
    reporter: InvariantReporter,
}

impl Drop for InstalledInvariantReporter {
    fn drop(&mut self) {
        let mut slot = reporter_slot()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if slot
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &self.reporter))
        {
            *slot = None;
        }
    }
}

#[track_caller]
pub fn install_invariant_reporter(reporter: InvariantReporter) -> InstalledInvariantReporter {
    let mut slot = reporter_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if slot.is_some() {
        drop(slot);
        invariant_failure();
    }
    *slot = Some(Arc::clone(&reporter));
    InstalledInvariantReporter { reporter }
}

#[cold]
#[track_caller]
pub fn invariant_failure() -> ! {
    let location = Location::caller();
    let report = SandInvariantReport {
        name: SAND_INVARIANT_VIOLATION_NAME.to_string(),
        frame: Some(format!(
            "at {}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        )),
    };
    let reporter = reporter_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .cloned();
    if let Some(reporter) = reporter {
        reporter(&report);
    }
    panic!("{STRIPPED_INVARIANT_MESSAGE}");
}
