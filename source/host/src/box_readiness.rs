use fabushi_android_shared::ExecutionError;

pub const SAND_BOX_NOT_READY_MESSAGE: &str =
    "The computer is still starting up (downloading its image or booting). Try again in a moment.";
pub const SAND_BOX_NOT_RESPONDING_MESSAGE: &str =
    "The computer isn't responding — it may be wedged. Try again in a moment, or recover it from Settings → Updates → Update Fabushi's Computer.";
pub const SAND_BOX_NO_MONITOR_AVAILABLE_MESSAGE: &str =
    "Every desktop monitor on the shared computer is in use right now, so this agent can't get its own screen. Wait a moment and try again — one frees up when another agent or parallel computer-use subagent finishes.";

pub const SAND_BOX_PRIMARY_WINDOW_INDEX: u32 = 1;
pub const SAND_BOX_FIRST_FORK_WINDOW_INDEX: u32 = 2;

pub fn is_primary_window_index(window_index: u32) -> bool {
    window_index <= SAND_BOX_PRIMARY_WINDOW_INDEX
}

pub fn remote_box_not_ready_message(error: &ExecutionError) -> &'static str {
    match error {
        ExecutionError::CapabilityUnavailable(message)
            if message.to_ascii_lowercase().contains("monitor") =>
        {
            SAND_BOX_NO_MONITOR_AVAILABLE_MESSAGE
        }
        ExecutionError::TimedOut => SAND_BOX_NOT_RESPONDING_MESSAGE,
        ExecutionError::Transport(message)
            if {
                let normalized = message.to_ascii_lowercase();
                normalized.contains("timeout")
                    || normalized.contains("timed out")
                    || normalized.contains("crash")
                    || normalized.contains("wedged")
            } =>
        {
            SAND_BOX_NOT_RESPONDING_MESSAGE
        }
        _ => SAND_BOX_NOT_READY_MESSAGE,
    }
}

pub fn is_no_monitor_capability_error(error: &ExecutionError) -> bool {
    matches!(
        error,
        ExecutionError::CapabilityUnavailable(message)
            if {
                let normalized = message.to_ascii_lowercase();
                normalized.contains("monitor")
                    || normalized.contains("no private desktop")
                    || normalized.contains("no-monitor")
            }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_desktop_readiness_no_monitor_and_window_taxonomy() {
        assert_eq!(
            remote_box_not_ready_message(&ExecutionError::CapabilityUnavailable(
                "no private desktop monitor is available".into(),
            )),
            SAND_BOX_NO_MONITOR_AVAILABLE_MESSAGE,
        );
        assert!(is_no_monitor_capability_error(
            &ExecutionError::CapabilityUnavailable("no-monitor".into())
        ));
        assert_eq!(
            remote_box_not_ready_message(&ExecutionError::TimedOut),
            SAND_BOX_NOT_RESPONDING_MESSAGE,
        );
        assert_eq!(
            remote_box_not_ready_message(&ExecutionError::Transport(
                "runner crash before response".into(),
            )),
            SAND_BOX_NOT_RESPONDING_MESSAGE,
        );
        assert_eq!(
            remote_box_not_ready_message(&ExecutionError::Transport(
                "connection refused".into(),
            )),
            SAND_BOX_NOT_READY_MESSAGE,
        );
        assert!(is_primary_window_index(0));
        assert!(is_primary_window_index(1));
        assert!(!is_primary_window_index(SAND_BOX_FIRST_FORK_WINDOW_INDEX));
    }
}
