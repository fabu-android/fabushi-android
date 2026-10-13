use std::collections::BTreeMap;
use std::env;
use std::fmt;

pub const SAND_BOX_BOOT_STAGES: &[&str] =
    &["entrypoint_started", "daemon_listening", "desktop_up", "ready"];
pub const SAND_BOX_BOOT_ID_ENV: &str = "SAND_BOX_BOOT_ID";
pub const SAND_BOX_BOOT_STARTED_AT_MS_ENV: &str = "SAND_BOX_BOOT_STARTED_AT_MS";
pub const SAND_BOX_AUTH_ID_ENV: &str = "SAND_BOX_AUTH_ID";
pub const SAND_BOX_TENANT_ID_ENV: &str = "SAND_BOX_TENANT_ID";
pub const SAND_BOX_STORE_ID_ENV: &str = "SAND_BOX_STORE_ID";
pub const SAND_BOX_CLUSTER_ENV: &str = "SAND_BOX_CLUSTER";

pub const SAND_HOST_LIFECYCLE_PHASES: &[&str] =
    &["plugin_graph", "identity", "log_catchup", "transcript_read", "ready"];
pub const SAND_EXEC_DAEMON_RESTART_CAUSES: &[&str] =
    &["daemon_exited", "startup_listener_timeout", "listener_lost"];
pub const SAND_SUPERVISOR_RESTART_CAUSES: &[&str] =
    &["supervisor_exited", "startup_status_timeout", "status_stale", "gave_up"];
pub const SAND_COOKIE_PERSIST_PHASES: &[&str] = &["capture", "restore"];
pub const SAND_COOKIE_PERSIST_OUTCOMES: &[&str] =
    &["captured", "ok", "partial", "failed", "empty"];
pub const SAND_EGRESS_TUNNEL_OUTCOMES: &[&str] =
    &["ready", "restart", "startup_timeout", "listener_lost", "stale_port"];
pub const SAND_BOX_BOOT_FAILURE_STAGES: &[&str] = &["desktop"];
pub const SAND_BOX_BOOT_FAILURE_REASONS: &[&str] = &["x_display_timeout"];
pub const SAND_PROCESS_CRASH_BINARIES: &[&str] = &[
    "cursor", "cursor-nightly", "cursor-lab", "chrome", "node", "exec-daemon",
    "xvfb", "xfwm4", "picom", "x11vnc", "websockify", "plank", "thunar",
    "xfce4-terminal", "other",
];
pub const SAND_PROCESS_CRASH_SIGNALS: &[&str] =
    &["sigill", "sigsegv", "sigabrt", "sigbus", "other"];
pub const SAND_HOST_BOOT_FETCH_OUTCOMES: &[&str] =
    &["current", "applied", "fallback", "restore_failed"];
pub const SAND_HOST_BOOT_FETCH_REASONS: &[&str] = &[
    "current", "applied", "pointer_unreachable", "pointer_malformed", "target_vetoed",
    "download_failed", "swap_refused", "swap_failed_restored", "restore_failed",
    "budget_exceeded", "unexpected_error",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandErrorDetail {
    pub message: String,
    pub stack: Option<String>,
}

pub fn sand_error_detail(error: impl fmt::Display) -> SandErrorDetail {
    SandErrorDetail {
        message: error.to_string(),
        stack: None,
    }
}

pub fn sand_error_detail_with_stack(
    message: impl Into<String>,
    stack: Option<impl Into<String>>,
) -> SandErrorDetail {
    SandErrorDetail {
        message: message.into(),
        stack: stack.map(Into::into),
    }
}

pub fn resolve_sand_box_identity_tags_from(
    environment: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut tags = BTreeMap::new();
    for (field, env_name) in [
        ("auth_id", SAND_BOX_AUTH_ID_ENV),
        ("tenant_id", SAND_BOX_TENANT_ID_ENV),
        ("box_store_id", SAND_BOX_STORE_ID_ENV),
        ("box_boot_id", SAND_BOX_BOOT_ID_ENV),
        ("cluster", SAND_BOX_CLUSTER_ENV),
    ] {
        if let Some(value) = environment
            .get(env_name)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            tags.insert(field.to_string(), value.to_string());
        }
    }
    tags
}

pub fn resolve_sand_box_identity_tags() -> BTreeMap<String, String> {
    resolve_sand_box_identity_tags_from(&env::vars().collect())
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoopSandTelemetry;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoopSandTurnTelemetry;

impl NoopSandTurnTelemetry {
    pub fn set_model<T>(&self, _value: T) {}
    pub fn set_request_id<T>(&self, _value: T) {}
    pub fn finalize<T>(&self, _value: T) {}
}

macro_rules! noop_report_methods {
    ($($name:ident),* $(,)?) => {
        $(
            pub fn $name<T>(&self, _report: T) {}
        )*
    };
}

impl NoopSandTelemetry {
    pub fn start_turn<T>(&self, _report: T) -> NoopSandTurnTelemetry {
        NoopSandTurnTelemetry
    }

    noop_report_methods!(
        report_tool_call_error,
        report_tool_call_stalled,
        report_tool_call_started,
        report_agent_error,
        report_bot_block,
        report_daemon_ping,
        report_box_boot_stage,
        report_exec_daemon_restart,
        report_supervisor_restart,
        report_automation_run,
        report_automation_fire_dropped,
        report_automation_lifecycle,
        report_turn_interrupt,
        report_turn_await,
        report_turn_retry,
        report_user_message_received,
        report_closing_send_nudge,
        report_subagent_revival,
        report_shell_revival,
        report_computer_use_usage,
        report_ttft,
        report_send_dispatch,
        report_queue_accepted,
        report_queue_dequeued,
        report_queue_watchdog,
        report_ack_obligation,
        report_pending_wake,
        report_turn_usage,
        report_turn_empty_delivery,
        report_journal_outcome,
        report_auto_review_expire_sweep_failed,
    );
}

pub fn create_noop_sand_telemetry() -> NoopSandTelemetry {
    NoopSandTelemetry
}
