#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessCrashKind {
    UncaughtException,
    UnhandledRejection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessCrashRecord {
    pub scope: String,
    pub kind: ProcessCrashKind,
    pub message: String,
    pub kept_alive: bool,
}

pub fn to_error_message(value: impl ToString) -> String {
    value.to_string()
}

pub fn handle_process_crash(
    scope: impl Into<String>,
    kind: ProcessCrashKind,
    value: impl ToString,
) -> ProcessCrashRecord {
    ProcessCrashRecord {
        scope: scope.into(),
        kind,
        message: to_error_message(value),
        kept_alive: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crash_guard_records_both_runtime_failure_classes_without_forcing_exit() {
        let exception = handle_process_crash("host", ProcessCrashKind::UncaughtException, "boom");
        let rejection = handle_process_crash("host", ProcessCrashKind::UnhandledRejection, "reject");
        assert!(exception.kept_alive);
        assert_eq!(exception.message, "boom");
        assert_eq!(rejection.kind, ProcessCrashKind::UnhandledRejection);
    }
}
