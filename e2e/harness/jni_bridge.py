"""OpenJDK 17 JNI subprocess runner invoking com.ombhrum.fabushi.core.MahayanaHost."""

from __future__ import annotations

import os
import subprocess
from pathlib import Path

from e2e.harness.build_lock import E2eWorkspaceManager, ensure_dir

_HARNESS_DIR = Path(__file__).resolve().parent
_JNI_PROBE_JAVA = _HARNESS_DIR / "JniBridgeProbe.java"
_ANDROID_SHIMS_JAVA = _HARNESS_DIR / "E2eAndroidShims.java"
_KEYGEN_SPEC_JAVA = _HARNESS_DIR / "KeyGenParameterSpec.java"
_BASE64_JAVA = _HARNESS_DIR / "Base64.java"


class JniBridgeRunner:
    """Compiles E2eAndroidShims.java + JniBridgeProbe.java once and executes real JNI boundary modes."""

    def __init__(self, ws: E2eWorkspaceManager) -> None:
        self.ws = ws
        self.classes_dir = ensure_dir(ws.run_dir / "jni_probe_classes")
        self._compiled = False

    def ensure_compiled(self) -> None:
        if self._compiled:
            return
        self.ws.materialize_snapshots({"jvm"})
        runtime_cp = self.ws.runtime_cp_file.read_text(encoding="UTF-8").strip()
        proc = subprocess.run(
            [
                "javac",
                f"-J-Djava.io.tmpdir={self.ws.jvm_tmp_dir}",
                "--release",
                "17",
                "-cp",
                runtime_cp,
                "-d",
                str(self.classes_dir),
                str(_KEYGEN_SPEC_JAVA),
                str(_BASE64_JAVA),
                str(_ANDROID_SHIMS_JAVA),
                str(_JNI_PROBE_JAVA),
            ],
            env=self.ws.env(scrub_loopback_proxy=True),
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            check=False,
        )
        if proc.returncode != 0:
            raise RuntimeError(f"Failed to compile JniBridgeProbe.java:\n{proc.stdout}")
        self._compiled = True

    def run_mode(self, mode: str, files_dir: Path) -> str:
        self.ensure_compiled()
        ensure_dir(files_dir)
        runtime_cp = self.ws.runtime_cp_file.read_text(encoding="UTF-8").strip()
        full_cp = f"{self.classes_dir}{os.pathsep}{runtime_cp}"
        lib_dir = str(self.ws.host_lib_so.parent)
        proc = subprocess.run(
            [
                "java",
                "-Xmx384m",
                f"-Djava.io.tmpdir={self.ws.jvm_tmp_dir}",
                f"-Djava.library.path={lib_dir}",
                "-Dhttp.nonProxyHosts=localhost|127.0.0.1|[::1]",
                "-cp",
                full_cp,
                "com.ombhrum.fabushi.e2e.JniBridgeProbe",
                mode,
                str(files_dir),
            ],
            env=self.ws.env(scrub_loopback_proxy=True),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=60,
            check=False,
        )
        if proc.returncode == 0:
            return proc.stdout.strip()
        if proc.returncode == 2:
            raise AssertionError(
                f"JNI bridge probe '{mode}' assertion failed: {(proc.stderr or proc.stdout).strip()}"
            )
        raise RuntimeError(
            f"JNI bridge probe '{mode}' crashed (exit {proc.returncode}): {(proc.stderr or proc.stdout).strip()}"
        )
