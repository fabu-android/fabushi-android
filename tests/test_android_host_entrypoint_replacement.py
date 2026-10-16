from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]


class AndroidHostEntrypointReplacementTest(unittest.TestCase):
    def test_android_workspace_does_not_ship_desktop_host_process_binary(self) -> None:
        workspace = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
        self.assertIn('"source/android-host-jni"', workspace)
        self.assertNotIn('"source/host/app"', workspace)
        self.assertNotIn("mahayana-app-host-runtime", workspace)

    def test_android_ci_builds_only_the_jni_host_artifact(self) -> None:
        workflow = (ROOT / ".github/workflows/android-parity-full-ci.yml").read_text(
            encoding="utf-8"
        )
        self.assertIn("-p fabushi-android-host-jni", workflow)
        self.assertIn("libmahayana_app_host.so", workflow)
        self.assertNotIn("-p mahayana-app-host-runtime", workflow)
        self.assertNotIn("--bin mahayana-app-host", workflow)

    def test_jni_links_the_shared_host_runtime_instead_of_a_parallel_host(self) -> None:
        manifest = (ROOT / "source/android-host-jni/Cargo.toml").read_text(
            encoding="utf-8"
        )
        self.assertIn(
            'fabushi-mahayana-host = { path = "../host" }',
            manifest,
        )
        self.assertNotIn("../host/app", manifest)


if __name__ == "__main__":
    unittest.main()
