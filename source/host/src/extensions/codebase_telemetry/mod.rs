//! Android codebase telemetry core.
//!
//! Platform-neutral csnaps framing and snapshot de-duplication are direct Rust ports from the
//! canonical Desktop Host. Android intentionally does not expose the Desktop local csnaps process
//! launcher here; execution must be provided by an authenticated, account-fenced Remote Runner.

pub mod codebase_snapshot_trigger;
pub mod csnaps_protocol;
