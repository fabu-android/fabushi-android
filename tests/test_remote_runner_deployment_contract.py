import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
JNI_MANIFEST = ROOT / "source/android-host-jni/Cargo.toml"
JNI_SOURCE = ROOT / "source/android-host-jni/src/lib.rs"
HOST_MANIFEST = ROOT / "source/host/Cargo.toml"
REMOTE_TOOLS = ROOT / "source/host/src/runner/remote_routed_tools.rs"
BOX_MANIFEST = ROOT / "source/box-exec-daemon/Cargo.toml"
APP_GRADLE = ROOT / "mobile/android/app/build.gradle"
FULL_CI = ROOT / ".github/workflows/android-parity-full-ci.yml"
BOX_LIB = ROOT / "source/box-exec-daemon/src/lib.rs"
BOX_CONTRACT = ROOT / "source/box-exec-daemon/src/deployment_contract.rs"


class RemoteRunnerDeploymentContractTest(unittest.TestCase):
    def test_transitive_box_exec_dependency_is_client_transport_not_an_apk_listener(self):
        jni_manifest = JNI_MANIFEST.read_text(encoding="utf-8")
        jni_source = JNI_SOURCE.read_text(encoding="utf-8")
        host_manifest = HOST_MANIFEST.read_text(encoding="utf-8")
        remote_tools = REMOTE_TOOLS.read_text(encoding="utf-8")
        box_manifest = BOX_MANIFEST.read_text(encoding="utf-8")

        # The Host intentionally depends on this crate for the authenticated Remote client
        # transport. Hiding that transitive dependency would make the deployment claim false.
        self.assertIn("fabushi-android-box-exec-daemon", host_manifest)
        self.assertNotIn("fabushi-android-box-exec-daemon", jni_manifest)
        self.assertIn("AuthenticatedRemoteHttpTransport", remote_tools)

        # Shipping Android Host/JNI code consumes transport types only. It does not compose the
        # reference server/service, bind a socket, or expose a daemon executable target.
        self.assertNotIn("RemoteExecutionService", remote_tools)
        self.assertNotIn("server::", remote_tools)
        self.assertNotIn("start_box_exec", remote_tools)
        self.assertNotIn("TcpListener", jni_source)
        self.assertNotIn("box_exec_daemon", jni_source)
        self.assertIn("[lib]", box_manifest)
        self.assertNotIn("[[bin]]", box_manifest)

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
