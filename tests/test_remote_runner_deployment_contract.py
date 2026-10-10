import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
JNI_MANIFEST = ROOT / "source/android-host-jni/Cargo.toml"
APP_GRADLE = ROOT / "mobile/android/app/build.gradle"
FULL_CI = ROOT / ".github/workflows/android-parity-full-ci.yml"
BOX_LIB = ROOT / "source/box-exec-daemon/src/lib.rs"
BOX_CONTRACT = ROOT / "source/box-exec-daemon/src/deployment_contract.rs"


class RemoteRunnerDeploymentContractTest(unittest.TestCase):
    def test_android_native_shipping_closure_does_not_link_box_exec_daemon(self):
        manifest = JNI_MANIFEST.read_text(encoding="utf-8")
        self.assertNotIn("fabushi-android-box-exec-daemon", manifest)
        self.assertNotIn("../box-exec-daemon", manifest)

    def test_android_packaging_builds_only_the_jni_host_cdylib(self):
        gradle = APP_GRADLE.read_text(encoding="utf-8")
        workflow = FULL_CI.read_text(encoding="utf-8")
        self.assertNotIn("box-exec-daemon", gradle)
        self.assertIn(
            "cargo ndk -t arm64-v8a -t x86_64 -o mobile/android/app/src/main/jniLibs build --release -p fabushi-android-host-jni",
            workflow,
        )
        self.assertNotIn(
            "cargo ndk -t arm64-v8a -t x86_64 -o mobile/android/app/src/main/jniLibs build --release -p fabushi-android-box-exec-daemon",
            workflow,
        )

    def test_source_contract_keeps_external_runner_replacement_explicit(self):
        lib = BOX_LIB.read_text(encoding="utf-8")
        contract = BOX_CONTRACT.read_text(encoding="utf-8")
        self.assertIn("pub mod deployment_contract;", lib)
        self.assertIn("ANDROID_APP_LISTENER_APPLICABLE: bool = false", contract)
        self.assertIn("AuthorizedExternalRunner", contract)
        self.assertIn("one-time capability approval", contract)
        self.assertIn("independently authenticates every operation", contract)


if __name__ == "__main__":
    unittest.main()
