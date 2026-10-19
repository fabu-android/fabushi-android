import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
HOST_ROOT = ROOT / "source/host"
LOCAL_RUNNER_ROOT = ROOT / "source/local-exec-daemon"
REMOTE_TOOLS = ROOT / "source/host/src/runner/remote_routed_tools.rs"


class CodebaseTelemetryAndroidReplacementTest(unittest.TestCase):
    def test_android_does_not_ship_desktop_csnaps_codebase_capture(self):
        # Desktop codebase telemetry is a Sand-only Linux mechanism: it scans the Sand
        # /workspace + /home codebases through an external csnaps process and uploads snapshots.
        # Android must not reinterpret the phone filesystem as that codebase or hide an arbitrary
        # process execution path inside the APK.
        self.assertFalse(
            (HOST_ROOT / "src/extensions/codebase_telemetry").exists(),
            "Android Host must not ship the Desktop Sand csnaps extension locally",
        )
        for root in (HOST_ROOT / "src", LOCAL_RUNNER_ROOT / "src"):
            for source in root.rglob("*.rs"):
                text = source.read_text(encoding="utf-8")
                self.assertNotIn(
                    "csnaps",
                    text,
                    f"{source.relative_to(ROOT)} must not spawn or embed Desktop csnaps",
                )

    def test_remote_replacement_remains_explicitly_authorized_and_has_no_hidden_telemetry_executor(self):
        remote = REMOTE_TOOLS.read_text(encoding="utf-8")
        self.assertIn('"authorized-remote-runner-v1"', remote)
        self.assertIn("account_fence", remote)
        self.assertIn("account_epoch", remote)
        self.assertIn("consume_remote_approval_for_dispatch", remote)
        self.assertIn("permission_grant_id", remote)

        # Codebase telemetry, when present in a Sand/desktop execution environment, remains that
        # environment's owner. The Android app must never smuggle telemetry collection through a
        # generic hidden executor or treat telemetry as an implicit privilege.
        self.assertNotIn('"codebase-telemetry"', remote)
        self.assertNotIn('"csnaps"', remote)


if __name__ == "__main__":
    unittest.main()
