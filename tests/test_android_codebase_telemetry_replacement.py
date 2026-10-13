from pathlib import Path
import re
import unittest


REPO_ROOT = Path(__file__).resolve().parents[1]
PRODUCTION_ROOTS = (
    REPO_ROOT / "source",
    REPO_ROOT / "frontend" / "src" / "main",
    REPO_ROOT / "mobile" / "android" / "app" / "src" / "main",
)
TEXT_SUFFIXES = {".rs", ".kt", ".kts", ".java", ".xml", ".toml", ".json", ".yml", ".yaml"}


class AndroidCodebaseTelemetryReplacementTest(unittest.TestCase):
    def test_shipping_android_has_no_local_csnaps_collector(self) -> None:
        offenders = []
        for root in PRODUCTION_ROOTS:
            if not root.exists():
                continue
            for path in root.rglob("*"):
                if not path.is_file() or path.suffix.lower() not in TEXT_SUFFIXES:
                    continue
                text = path.read_text(encoding="utf-8", errors="ignore").lower()
                if re.search(r"\\bcsnaps\\b", text):
                    offenders.append(str(path.relative_to(REPO_ROOT)))
        self.assertEqual(
            [],
            offenders,
            "Android production must not bundle or start Desktop csnaps/local codebase telemetry",
        )

    def test_workspace_execution_replacement_is_authorized_remote_runner(self) -> None:
        remote_tools = (
            REPO_ROOT / "source" / "host" / "src" / "runner" / "remote_routed_tools.rs"
        ).read_text(encoding="utf-8")
        remote_journal = (
            REPO_ROOT / "source" / "host" / "src" / "remote_execution.rs"
        ).read_text(encoding="utf-8")

        for anchor in (
            "authorized-remote-runner-v1",
            '"external-shell"',
            '"external-read"',
            "account_fence",
            "account_epoch",
            "device_id",
            "request_remote_approval",
        ):
            self.assertIn(anchor, remote_tools)

        for anchor in (
            "OutcomeUnknown",
            "account_fence",
            "account_epoch",
            "device_id",
            "reconcile",
        ):
            self.assertIn(anchor, remote_journal)


if __name__ == "__main__":
    unittest.main()
