use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

pub const PRE_ATTACH_DIAGNOSTIC_BUFFER_LIMIT: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostDiagnostic {
    pub code: String,
    pub detail: String,
}

pub type DiagnosticReporter = Arc<dyn Fn(HostDiagnostic) + Send + Sync>;

#[derive(Default)]
struct HostDiagnosticsState {
    reporter: Option<DiagnosticReporter>,
    buffered: VecDeque<HostDiagnostic>,
}

#[derive(Default)]
pub struct HostDiagnostics {
    state: Mutex<HostDiagnosticsState>,
}

impl HostDiagnostics {
    pub fn pin_reporter(&self, reporter: DiagnosticReporter) {
        let buffered = {
            let mut state = self.state.lock().expect("diagnostic lock poisoned");
            state.reporter = Some(reporter.clone());
            state.buffered.drain(..).collect::<Vec<_>>()
        };
        for diagnostic in buffered {
            reporter(diagnostic);
        }
    }

    pub fn report(&self, diagnostic: HostDiagnostic) {
        let reporter = {
            let mut state = self.state.lock().expect("diagnostic lock poisoned");
            if let Some(reporter) = state.reporter.clone() {
                Some(reporter)
            } else {
                if state.buffered.len() >= PRE_ATTACH_DIAGNOSTIC_BUFFER_LIMIT {
                    state.buffered.pop_front();
                }
                state.buffered.push_back(diagnostic.clone());
                None
            }
        };
        if let Some(reporter) = reporter {
            reporter(diagnostic);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostic(index: usize) -> HostDiagnostic {
        HostDiagnostic {
            code: format!("code-{index}"),
            detail: format!("detail-{index}"),
        }
    }

    #[test]
    fn pre_attach_diagnostics_are_flushed_in_order() {
        let diagnostics = HostDiagnostics::default();
        diagnostics.report(diagnostic(1));
        diagnostics.report(diagnostic(2));

        let observed = Arc::new(Mutex::new(Vec::<HostDiagnostic>::new()));
        let observed_clone = observed.clone();
        diagnostics.pin_reporter(Arc::new(move |value| {
            observed_clone.lock().unwrap().push(value);
        }));

        assert_eq!(
            observed.lock().unwrap().clone(),
            vec![diagnostic(1), diagnostic(2)],
        );
    }

    #[test]
    fn pre_attach_buffer_drops_oldest_after_one_hundred_records() {
        let diagnostics = HostDiagnostics::default();
        for index in 0..=PRE_ATTACH_DIAGNOSTIC_BUFFER_LIMIT {
            diagnostics.report(diagnostic(index));
        }

        let observed = Arc::new(Mutex::new(Vec::<HostDiagnostic>::new()));
        let observed_clone = observed.clone();
        diagnostics.pin_reporter(Arc::new(move |value| {
            observed_clone.lock().unwrap().push(value);
        }));

        let observed = observed.lock().unwrap();
        assert_eq!(observed.len(), PRE_ATTACH_DIAGNOSTIC_BUFFER_LIMIT);
        assert_eq!(observed.first(), Some(&diagnostic(1)));
        assert_eq!(
            observed.last(),
            Some(&diagnostic(PRE_ATTACH_DIAGNOSTIC_BUFFER_LIMIT)),
        );
    }

    #[test]
    fn reporter_callbacks_run_without_holding_the_state_lock() {
        let diagnostics = Arc::new(HostDiagnostics::default());
        let diagnostics_clone = diagnostics.clone();
        diagnostics.pin_reporter(Arc::new(move |value| {
            if value.code == "outer" {
                diagnostics_clone.report(HostDiagnostic {
                    code: "nested".into(),
                    detail: "nested".into(),
                });
            }
        }));
        diagnostics.report(HostDiagnostic {
            code: "outer".into(),
            detail: "outer".into(),
        });
    }
}
