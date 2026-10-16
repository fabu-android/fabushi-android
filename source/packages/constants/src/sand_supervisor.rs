use serde::{Deserialize, Serialize};

pub const SAND_SUPERVISOR_DIR: &str = "/tmp/sand-supervisor";
pub const SAND_SUPERVISOR_COMMAND_PATH: &str = "/tmp/sand-supervisor/command.json";
pub const SAND_SUPERVISOR_COMMAND_PART_PATH: &str = "/tmp/sand-supervisor/command.json.part";
pub const SAND_SUPERVISOR_STATUS_PATH: &str = "/tmp/sand-supervisor/status.json";
pub const SAND_SUPERVISOR_ACKS_DIR: &str = "/tmp/sand-supervisor/acks";
pub const SAND_SUPERVISOR_STAGED_BUNDLE_PATH: &str =
    "/tmp/sand-supervisor/incoming-host-bundle.tgz";
pub const SAND_SUPERVISOR_STAGED_BUNDLE_PART_PATH: &str =
    "/tmp/sand-supervisor/incoming-host-bundle.tgz.part";
pub const SAND_SUPERVISOR_DESKTOP_HEALTH_PATH: &str =
    "/tmp/sand-supervisor/desktop-health.json";
pub const SAND_BOX_AGENT_DATA_ROOT: &str = "/home/box/sand-data";
pub const SAND_BOX_HOST_UPGRADE_MARKER_PATH: &str =
    "/home/box/sand-data/.sand-host-upgrade.json";
pub const SAND_BOX_HOST_DIR: &str = "/home/box/sand-host";
pub const SAND_BOX_HOST_ENTRY: &str = "/home/box/sand-host/host-main.cjs";
pub const SAND_BOX_HOST_VERSION_PATH: &str = "/home/box/sand-host/version";
pub const SAND_HOST_UPGRADE_MAX_DEFER_MS: u64 = 6 * 60 * 60 * 1_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandSupervisorCommand {
    pub id: String,
    pub kind: String,
    pub issued_at_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_now: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildSandSupervisorCommandArgs {
    pub id: String,
    pub kind: String,
    pub now_ms: u64,
    pub reason: Option<String>,
    pub mode: Option<String>,
    pub version: Option<String>,
    pub bundle_path: Option<String>,
    pub force_now: bool,
}

pub fn build_sand_supervisor_command(
    args: BuildSandSupervisorCommandArgs,
) -> SandSupervisorCommand {
    let is_upgrade = args.kind == "upgrade";
    SandSupervisorCommand {
        id: args.id,
        kind: args.kind,
        issued_at_ms: args.now_ms,
        reason: args.reason,
        mode: is_upgrade.then_some(args.mode).flatten(),
        version: is_upgrade.then_some(args.version).flatten(),
        bundle_path: is_upgrade.then_some(args.bundle_path).flatten(),
        force_now: (is_upgrade && args.force_now).then_some(true),
    }
}

pub fn serialize_sand_supervisor_command(
    command: &SandSupervisorCommand,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(command)
}

pub fn is_sand_host_upgrade_available(current: &str, target: Option<&str>) -> bool {
    target.is_some_and(|target| !target.is_empty() && target != current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_only_fields_are_stripped_from_non_upgrade_commands() {
        let command = build_sand_supervisor_command(BuildSandSupervisorCommandArgs {
            id: "1".into(),
            kind: "restart".into(),
            now_ms: 10,
            reason: Some("health".into()),
            mode: Some("fast".into()),
            version: Some("2".into()),
            bundle_path: Some("/tmp/bundle".into()),
            force_now: true,
        });
        assert!(command.mode.is_none());
        assert!(command.version.is_none());
        assert!(command.force_now.is_none());
        assert!(serialize_sand_supervisor_command(&command)
            .unwrap()
            .contains("\"issuedAtMs\":10"));
    }

    #[test]
    fn upgrade_availability_requires_nonempty_different_version() {
        assert!(is_sand_host_upgrade_available("1", Some("2")));
        assert!(!is_sand_host_upgrade_available("1", Some("1")));
        assert!(!is_sand_host_upgrade_available("1", None));
    }
}
