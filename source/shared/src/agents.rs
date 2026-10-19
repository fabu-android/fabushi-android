//! Android-owned equivalent of Desktop shared agent tool-name contracts.
//!
//! The Android runtime keeps the same user-visible distinction between an
//! authorized remote box and the user's explicitly paired external computer.
//! This module is naming/telemetry policy only; it does not grant execution.

pub const SAND_BOX_SHELL_TOOL_NAME: &str = "Shell";
pub const SAND_BOX_READ_TOOL_NAME: &str = "Read";
pub const SAND_BOX_AWAIT_SHELL_TOOL_NAME: &str = "AwaitShell";
pub const SAND_EXTERNAL_SHELL_TOOL_NAME: &str = "ExternalShell";
pub const SAND_EXTERNAL_READ_TOOL_NAME: &str = "ExternalRead";
pub const SAND_EXTERNAL_AWAIT_SHELL_TOOL_NAME: &str = "AwaitExternalShell";
pub const SAND_DEFAULT_EXTERNAL_MACHINE_ID: &str = "user-computer";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SandExternalMachine {
    pub id: &'static str,
    pub label: &'static str,
}

pub const SAND_USER_COMPUTER: SandExternalMachine = SandExternalMachine {
    id: "user-computer",
    label: "the user's computer",
};

pub fn resolve_sand_external_machine(id: Option<&str>) -> Option<SandExternalMachine> {
    match id.unwrap_or(SAND_DEFAULT_EXTERNAL_MACHINE_ID) {
        "user-computer" => Some(SAND_USER_COMPUTER),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SandToolSurface {
    Box,
    External,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SandDualSurfaceToolTelemetry<'a> {
    pub tool_name: &'a str,
    pub surface: SandToolSurface,
}

pub fn sand_dual_surface_tool_telemetry(
    tool_name: &str,
) -> Option<SandDualSurfaceToolTelemetry<'_>> {
    let surface = match tool_name {
        SAND_BOX_SHELL_TOOL_NAME | SAND_BOX_READ_TOOL_NAME | SAND_BOX_AWAIT_SHELL_TOOL_NAME => {
            SandToolSurface::Box
        }
        SAND_EXTERNAL_SHELL_TOOL_NAME
        | SAND_EXTERNAL_READ_TOOL_NAME
        | SAND_EXTERNAL_AWAIT_SHELL_TOOL_NAME => SandToolSurface::External,
        _ => return None,
    };
    Some(SandDualSurfaceToolTelemetry { tool_name, surface })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_external_machine_is_stable_and_unknown_ids_fail_closed() {
        assert_eq!(resolve_sand_external_machine(None), Some(SAND_USER_COMPUTER));
        assert_eq!(
            resolve_sand_external_machine(Some(SAND_DEFAULT_EXTERNAL_MACHINE_ID)),
            Some(SAND_USER_COMPUTER)
        );
        assert_eq!(resolve_sand_external_machine(Some("unknown-device")), None);
    }

    #[test]
    fn dual_surface_telemetry_never_conflates_remote_box_and_external_computer() {
        for tool in [
            SAND_BOX_SHELL_TOOL_NAME,
            SAND_BOX_READ_TOOL_NAME,
            SAND_BOX_AWAIT_SHELL_TOOL_NAME,
        ] {
            assert_eq!(
                sand_dual_surface_tool_telemetry(tool).map(|value| value.surface),
                Some(SandToolSurface::Box)
            );
        }
        for tool in [
            SAND_EXTERNAL_SHELL_TOOL_NAME,
            SAND_EXTERNAL_READ_TOOL_NAME,
            SAND_EXTERNAL_AWAIT_SHELL_TOOL_NAME,
        ] {
            assert_eq!(
                sand_dual_surface_tool_telemetry(tool).map(|value| value.surface),
                Some(SandToolSurface::External)
            );
        }
        assert!(sand_dual_surface_tool_telemetry("Task").is_none());
        assert!(sand_dual_surface_tool_telemetry("CheckSubagent").is_none());
    }
}
