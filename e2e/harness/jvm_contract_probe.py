"""Warm per-run JVM batch probe client compiling E2eBatchRunner.kt via K2JVMCompiler."""

from __future__ import annotations

import json
import os
import subprocess
import threading
from pathlib import Path
from typing import Any

from e2e.harness.build_lock import E2eWorkspaceManager, GRADLE_CACHE_DIR, ensure_dir

_HARNESS_DIR = Path(__file__).resolve().parent
_BATCH_KT = _HARNESS_DIR / "E2eBatchRunner.kt"
_SHIM_JAVA_FILES = [
    _HARNESS_DIR / "KeyGenParameterSpec.java",
    _HARNESS_DIR / "Base64.java",
    _HARNESS_DIR / "E2eAndroidShims.java",
]


def _find_kotlin_compiler_jars() -> list[Path]:
    k2_jars = sorted(
        GRADLE_CACHE_DIR.glob(
            "modules-2/files-2.1/org.jetbrains.kotlin/kotlin-compiler-embeddable/*/*/kotlin-compiler-embeddable-*.jar"
        )
    )
    if not k2_jars:
        raise FileNotFoundError("kotlin-compiler-embeddable-*.jar not found in Gradle cache")
    k2_jar = k2_jars[-1]
    ver = k2_jar.parent.parent.name
    extras = [
        *sorted(GRADLE_CACHE_DIR.glob(f"modules-2/files-2.1/org.jetbrains.kotlin/kotlin-stdlib/{ver}/*/kotlin-stdlib-*.jar")),
        *sorted(GRADLE_CACHE_DIR.glob(f"modules-2/files-2.1/org.jetbrains.kotlin/kotlin-reflect/{ver}/*/kotlin-reflect-*.jar")),
        *sorted(GRADLE_CACHE_DIR.glob(f"modules-2/files-2.1/org.jetbrains.kotlin/kotlin-script-runtime/{ver}/*/kotlin-script-runtime-*.jar")),
        *sorted(GRADLE_CACHE_DIR.glob("modules-2/files-2.1/org.jetbrains.intellij.deps/trove4j/*/*/trove4j-*.jar")),
        *sorted(GRADLE_CACHE_DIR.glob("modules-2/files-2.1/org.jetbrains.kotlinx/kotlinx-coroutines-core-jvm/*/*/kotlinx-coroutines-core-jvm-*.jar")),
    ]
    seen: list[Path] = [k2_jar]
    for p in extras:
        if p.is_file() and p not in seen:
            seen.append(p)
    return seen


class JvmBatchProbeClient:
    """Compiles E2eAndroidShims + E2eBatchRunner.kt once per run and serves sub-40ms JSON-line probes."""

    def __init__(self, ws: E2eWorkspaceManager) -> None:
        self.ws = ws
        self.shim_classes_dir = ensure_dir(ws.run_dir / "jvm_shim_classes")
        self.worker_classes_dir = ensure_dir(ws.run_dir / "jvm_worker_classes")
        self._compiled = False
        self._proc: subprocess.Popen[str] | None = None
        self._lock = threading.RLock()
        ws.register_cleanup(self.close)

    def ensure_compiled(self) -> None:
        if self._compiled:
            return
        self.ws.materialize_snapshots({"jvm"})
        env = self.ws.env(scrub_loopback_proxy=True)
        runtime_cp = self.ws.runtime_cp_file.read_text(encoding="UTF-8").strip()
        friend_dirs = self.ws.friend_dirs_file.read_text(encoding="UTF-8").strip()

        # 1. Compile Java shims (placed BEFORE android.jar on classpath!)
        j_proc = subprocess.run(
            [
                "javac",
                f"-J-Djava.io.tmpdir={self.ws.jvm_tmp_dir}",
                "--release",
                "17",
                "-cp",
                runtime_cp,
                "-d",
                str(self.shim_classes_dir),
                *[str(p) for p in _SHIM_JAVA_FILES],
            ],
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            check=False,
        )
        if j_proc.returncode != 0:
            raise RuntimeError(f"Failed to compile Java shims:\n{j_proc.stdout}")

        # 2. Compile Kotlin E2eBatchRunner.kt via K2JVMCompiler with -Xfriend-paths
        compiler_cp = os.pathsep.join(str(p) for p in _find_kotlin_compiler_jars())
        compile_cp = f"{self.shim_classes_dir}{os.pathsep}{runtime_cp}"
        k_cmd = [
            "java",
            "-Xmx768m",
            f"-Djava.io.tmpdir={self.ws.jvm_tmp_dir}",
            "-cp",
            compiler_cp,
            "org.jetbrains.kotlin.cli.jvm.K2JVMCompiler",
            "-jvm-target",
            "17",
            "-no-stdlib",
            "-classpath",
            compile_cp,
            "-d",
            str(self.worker_classes_dir),
        ]
        if friend_dirs:
            k_cmd.append(f"-Xfriend-paths={friend_dirs}")
        k_cmd.append(str(_BATCH_KT))

        k_proc = subprocess.run(
            k_cmd,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            check=False,
        )
        if k_proc.returncode != 0:
            raise RuntimeError(f"Failed to compile E2eBatchRunner.kt:\n{k_proc.stdout}")
        self._compiled = True

    def _ensure_worker(self) -> subprocess.Popen[str]:
        if self._proc is not None and self._proc.poll() is None:
            return self._proc
        self.ensure_compiled()
        runtime_cp = self.ws.runtime_cp_file.read_text(encoding="UTF-8").strip()
        # CRITICAL: shim_classes_dir MUST precede runtime_cp (which contains mockable android.jar)!
        full_cp = os.pathsep.join([str(self.worker_classes_dir), str(self.shim_classes_dir), runtime_cp])
        lib_dir = str(self.ws.host_lib_dir_raw)
        self._proc = subprocess.Popen(
            [
                "java",
                "-Xmx640m",
                f"-Djava.io.tmpdir={self.ws.jvm_tmp_dir}",
                f"-Djava.library.path={lib_dir}",
                "-Dhttp.nonProxyHosts=localhost|127.0.0.1|[::1]",
                "-Dfile.encoding=UTF-8",
                "-cp",
                full_cp,
                "com.ombhrum.fabushi.e2e.E2eBatchRunner",
            ],
            env=self.ws.env(scrub_loopback_proxy=True),
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
        )
        return self._proc

    def run_probe(self, probe: str, scratch_dir: Path, **kwargs: Any) -> dict[str, Any]:
        with self._lock:
            ensure_dir(scratch_dir)
            proc = self._ensure_worker()
            req = {"probe": probe, "scratchDir": str(scratch_dir), **kwargs}
            assert proc.stdin is not None and proc.stdout is not None
            proc.stdin.write(json.dumps(req, ensure_ascii=False) + "\n")
            proc.stdin.flush()
            resp_line = proc.stdout.readline()
            if not resp_line:
                stderr_tail = ""
                if proc.stderr is not None:
                    try:
                        stderr_tail = proc.stderr.read()
                    except Exception:  # noqa: BLE001
                        pass
                self.close()
                raise RuntimeError(
                    f"E2eBatchRunner JVM worker exited unexpectedly on probe '{probe}': {stderr_tail[-1500:]}"
                )
            resp = json.loads(resp_line)
            status = resp.get("status")
            if status == "PASS":
                return resp.get("detail") or {}
            if status == "FAIL":
                raise AssertionError(resp.get("message") or f"JVM probe '{probe}' failed")
            raise RuntimeError(resp.get("message") or f"JVM probe '{probe}' errored")

    def close(self) -> None:
        with self._lock:
            proc = self._proc
            self._proc = None
            if proc is None:
                return
            try:
                if proc.stdin:
                    proc.stdin.close()
                proc.wait(timeout=5)
            except Exception:  # noqa: BLE001
                try:
                    proc.kill()
                    proc.wait(timeout=3)
                except Exception:  # noqa: BLE001
                    pass
