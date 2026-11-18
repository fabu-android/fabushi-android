"""Zero-swap flock serialization, lazy snapshot materialization, and AGENTS.md cleanup."""

from __future__ import annotations

import atexit
import contextlib
import errno
import fcntl
import hashlib
import json
import os
import shutil
import signal
import subprocess
import sys
import time
from collections.abc import Callable, Iterator
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
SCRATCH_BASE = Path("/home/antigravity/.gemini/jetski/scratch/teamwork_tmp")
BUILD_LOCK_FILE = SCRATCH_BASE / "fabushi-build.lock"
E2E_RUN_ROOT = SCRATCH_BASE / "e2e_run"

ANDROID_SDK_DIR = Path("/home/antigravity/android-sdk")
ANDROID_NDK_DIR = ANDROID_SDK_DIR / "ndk" / "27.2.12479018"
RUSTUP_DIR = Path("/home/antigravity/.rustup")
CARGO_DIR = Path("/home/antigravity/.cargo")
GRADLE_CACHE_DIR = Path("/home/antigravity/.gradle/caches")


def safe_chmod(path: Path, mode: int) -> None:
    try:
        os.chmod(path, mode)
    except OSError:
        # Shared virtiofs mounts may disallow chmod; ignore safely.
        pass


def ensure_dir(path: Path, mode: int = 0o700) -> Path:
    path.mkdir(parents=True, exist_ok=True)
    safe_chmod(path, mode)
    return path


def make_base_env(
    scratch_tmp: Path,
    *,
    scrub_loopback_proxy: bool = False,
    lock_held: bool = False,
) -> dict[str, str]:
    ensure_dir(scratch_tmp)
    pycache_dir = ensure_dir(scratch_tmp.parent / "pycache")
    env = dict(os.environ)

    env["PYTHONDONTWRITEBYTECODE"] = "1"
    env["PYTHONPYCACHEPREFIX"] = str(pycache_dir)
    env["TMPDIR"] = str(scratch_tmp)
    env["TMP"] = str(scratch_tmp)
    env["TEMP"] = str(scratch_tmp)

    if ANDROID_SDK_DIR.is_dir():
        env["ANDROID_HOME"] = str(ANDROID_SDK_DIR)
        env["ANDROID_SDK_ROOT"] = str(ANDROID_SDK_DIR)
    if ANDROID_NDK_DIR.is_dir():
        env["ANDROID_NDK_HOME"] = str(ANDROID_NDK_DIR)
    if RUSTUP_DIR.is_dir():
        env["RUSTUP_HOME"] = str(RUSTUP_DIR)
    if CARGO_DIR.is_dir():
        env["CARGO_HOME"] = str(CARGO_DIR)

    env["CARGO_BUILD_JOBS"] = "2"
    env["ANDROID_API_LEVEL"] = "29"

    extra_paths = [
        str(CARGO_DIR / "bin"),
        str(ANDROID_SDK_DIR / "platform-tools"),
        str(ANDROID_SDK_DIR / "cmdline-tools" / "latest" / "bin"),
        str(ANDROID_SDK_DIR / "build-tools" / "37.0.0"),
        str(ANDROID_NDK_DIR / "toolchains" / "llvm" / "prebuilt" / "linux-x86_64" / "bin"),
    ]
    existing_path = env.get("PATH", "")
    env["PATH"] = ":".join([p for p in extra_paths if Path(p).is_dir()] + [existing_path])

    if lock_held:
        env["FABUSHI_BUILD_LOCK_HELD"] = "1"

    if scrub_loopback_proxy:
        for key in (
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
            "GRADLE_OPTS",
        ):
            env.pop(key, None)
        env["NO_PROXY"] = "127.0.0.1,localhost,::1,[::1]"
        env["no_proxy"] = "127.0.0.1,localhost,::1,[::1]"

    return env


def list_lingering_daemons() -> list[str]:
    proc = subprocess.run(
        ["jps", "-l"],
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
        check=False,
    )
    hits: list[str] = []
    for line in proc.stdout.splitlines():
        if "KotlinCompileDaemon" in line or "GradleDaemon" in line:
            hits.append(line.strip())
    return hits


def stop_jvm_daemons() -> None:
    gradle_dir = REPO_ROOT / "mobile" / "android"
    gradlew = gradle_dir / "gradlew"
    if gradlew.is_file():
        subprocess.run(
            [str(gradlew), "--stop"],
            cwd=str(gradle_dir),
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=30,
            check=False,
        )
    for sig in (signal.SIGTERM, signal.SIGKILL):
        daemons = list_lingering_daemons()
        if not daemons:
            break
        for entry in daemons:
            pid_str = entry.split()[0]
            if pid_str.isdigit():
                try:
                    os.kill(int(pid_str), sig)
                except OSError:
                    pass
        time.sleep(0.3)


def clean_repo_temp_build_artifacts(*, preserve_release_artifacts: bool = False) -> None:
    """Execute AGENTS.md post-build artifact cleanup while strictly preserving :mika-agent jniLibs."""
    removable = [
        REPO_ROOT / "mobile" / "android" / "rust" / "target",
        REPO_ROOT / "mobile" / "android" / "rust" / "mahayana_android_host" / "target",
        REPO_ROOT / "mobile" / "android" / "rust" / "host-lib",
        REPO_ROOT / "mobile" / "android" / "app" / "src" / "main" / "jniLibs",
        REPO_ROOT / "release-signing",
    ]
    if not preserve_release_artifacts:
        removable.append(REPO_ROOT / "dist")
    for target in removable:
        if target.exists():
            shutil.rmtree(target, ignore_errors=True)


@contextlib.contextmanager
def acquire_build_lock(timeout_sec: float = 1800.0) -> Iterator[bool]:
    """Serialize Cargo / NDK / Gradle invocations via /home/antigravity/.gemini/jetski/scratch/teamwork_tmp/fabushi-build.lock."""
    ensure_dir(SCRATCH_BASE)
    if os.environ.get("FABUSHI_BUILD_LOCK_HELD") == "1":
        yield False
        return

    fd = os.open(str(BUILD_LOCK_FILE), os.O_CREAT | os.O_RDWR, 0o666)
    deadline = time.monotonic() + timeout_sec
    waited = False
    try:
        while True:
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                break
            except OSError as exc:
                if exc.errno not in (errno.EACCES, errno.EAGAIN):
                    raise
                if not waited:
                    print(
                        f"[e2e-lock] Waiting for {BUILD_LOCK_FILE} ...",
                        file=sys.stderr,
                        flush=True,
                    )
                    waited = True
                if time.monotonic() >= deadline:
                    raise TimeoutError(
                        f"Timed out after {timeout_sec}s waiting for {BUILD_LOCK_FILE}"
                    ) from exc
                time.sleep(0.5)
        yield True
    finally:
        try:
            fcntl.flock(fd, fcntl.LOCK_UN)
        finally:
            os.close(fd)


_EXPORT_CP_INIT_GRADLE = """\
gradle.projectsEvaluated {
    rootProject.tasks.register("exportE2eClasspath") {
        doLast {
            def outDir = new File(System.getProperty("e2e.export.dir"))
            outDir.mkdirs()
            def cpSet = new LinkedHashSet<String>()
            def friendSet = new LinkedHashSet<String>()

            def collectBuildClasses = { Project p ->
                def candidates = [
                    new File(p.buildDir, "classes/kotlin/main"),
                    new File(p.buildDir, "classes/java/main"),
                    new File(p.buildDir, "resources/main"),
                    new File(p.buildDir, "classes/kotlin/testFixtures"),
                    new File(p.buildDir, "classes/java/testFixtures"),
                    new File(p.buildDir, "resources/testFixtures"),
                    new File(p.buildDir, "tmp/kotlin-classes/debug"),
                    new File(p.buildDir, "intermediates/built_in_kotlinc/debug/compileDebugKotlin/classes"),
                    new File(p.buildDir, "intermediates/javac/debug/compileDebugJavaWithJavac/classes"),
                    new File(p.buildDir, "intermediates/compile_and_runtime_not_namespaced_r_class_jar/debug/processDebugResources/R.jar"),
                    new File(p.buildDir, "intermediates/compile_r_class_jar/debug/generateDebugRFile/R.jar")
                ]
                candidates.each { File f ->
                    if (f.exists()) {
                        cpSet.add(f.canonicalPath)
                        if (f.isDirectory() && !f.name.equals("main") || f.canonicalPath.endsWith("classes/kotlin/main")) {
                            friendSet.add(f.canonicalPath)
                        }
                    }
                }
                if (p.buildDir.exists()) {
                    p.buildDir.eachFileRecurse { File f ->
                        def path = f.canonicalPath
                        if (f.isDirectory() && (
                            path.endsWith("/classes/kotlin/main") ||
                            path.endsWith("/tmp/kotlin-classes/debug") ||
                            path.endsWith("/compileDebugKotlin/classes") ||
                            path.endsWith("/compileDebugJavaWithJavac/classes")
                        )) {
                            cpSet.add(path)
                            friendSet.add(path)
                        } else if (f.isFile() && f.name == "R.jar" && path.contains("/debug/")) {
                            cpSet.add(path)
                        }
                    }
                }
            }

            [":mika-core", ":mika-agent", ":app"].each { String pName ->
                def proj = rootProject.findProject(pName)
                if (proj != null) {
                    collectBuildClasses(proj)
                    ["debugUnitTestRuntimeClasspath", "debugCompileClasspath", "testRuntimeClasspath", "runtimeClasspath"].each { String cfgName ->
                        def cfg = proj.configurations.findByName(cfgName)
                        if (cfg != null && cfg.canBeResolved) {
                            try {
                                cfg.incoming.artifactView { lenient(true) }.files.each { File f ->
                                    if (f.exists()) {
                                        cpSet.add(f.canonicalPath)
                                    }
                                }
                            } catch (Throwable ignored) {}
                        }
                    }
                }
            }

            new File(outDir, "raw_runtime_cp.txt").setText(cpSet.join(File.pathSeparator), "UTF-8")
            new File(outDir, "raw_friend_dirs.txt").setText(friendSet.join(","), "UTF-8")
        }
    }
}
"""


def is_build_lock_free() -> bool:
    ensure_dir(SCRATCH_BASE)
    fd = os.open(str(BUILD_LOCK_FILE), os.O_CREAT | os.O_RDWR, 0o666)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        fcntl.flock(fd, fcntl.LOCK_UN)
        return True
    except OSError:
        return False
    finally:
        os.close(fd)


def _run_pgroup(
    cmd: list[str],
    *,
    cwd: str,
    env: dict[str, str],
    timeout: int,
) -> tuple[int, str]:
    """Run command in its own process group and SIGKILL the entire group on timeout."""
    proc = subprocess.Popen(
        cmd,
        cwd=cwd,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        start_new_session=True,
    )
    try:
        out, _ = proc.communicate(timeout=timeout)
        return proc.returncode, out or ""
    except subprocess.TimeoutExpired as exc:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except OSError:
            proc.kill()
        out, _ = proc.communicate()
        raise RuntimeError(
            f"Command {cmd[0]} timed out after {timeout}s:\n{(out or '')[-2500:]}"
        ) from exc


def _compute_rust_source_hash() -> str:
    h = hashlib.sha256()
    rust_root = REPO_ROOT / "mobile" / "android" / "rust"
    build_sh = REPO_ROOT / "scripts" / "build_rust_android.sh"
    if build_sh.is_file():
        h.update(build_sh.read_bytes())
    for p in sorted(rust_root.rglob("*")):
        parts = p.relative_to(rust_root).parts
        if any(seg in ("target", "host-lib", ".git") for seg in parts):
            continue
        if p.is_file() and p.suffix in (".rs", ".toml", ".lock", ".sh", ".h", ".c"):
            h.update(str(p.relative_to(REPO_ROOT)).encode("utf-8"))
            h.update(b"\x00")
            h.update(p.read_bytes())
    return h.hexdigest()[:24]


class E2eWorkspaceManager:
    """Owns the per-run scratch directory, lazy native/JVM snapshots, and deterministic cleanup."""

    def __init__(self, *, preserve_release_artifacts: bool = False) -> None:
        self.preserve_release_artifacts = preserve_release_artifacts
        ensure_dir(E2E_RUN_ROOT)
        self.run_dir = ensure_dir(E2E_RUN_ROOT / f"run_{os.getpid()}_{int(time.time() * 1000)}")
        self.tmp_dir = ensure_dir(self.run_dir / "tmp")
        self.cases_dir = ensure_dir(self.run_dir / "cases")
        self.snapshot_dir = ensure_dir(self.run_dir / "snapshot")
        self.native_snapshot_dir = ensure_dir(self.snapshot_dir / "native")
        self.jvm_snapshot_dir = ensure_dir(self.snapshot_dir / "jvm")
        # xfs-backed JVM temp directory so libconscrypt / JNA / Kotlin incremental caches never mmap on virtiofs or touch /tmp
        self.jvm_tmp_dir = ensure_dir(
            Path(f"/home/antigravity/.cache/teamwork_tmp/e2e_jvm_{os.getpid()}_{int(time.time() * 1000)}")
        )

        self._native_attempted = False
        self._jvm_attempted = False
        self.native_build_error: str | None = None
        self.jvm_build_error: str | None = None
        self._cleanup_callbacks: list[Callable[[], None]] = []
        self._cleaned = False

        os.environ["TMPDIR"] = str(self.tmp_dir)
        os.environ["TMP"] = str(self.tmp_dir)
        os.environ["TEMP"] = str(self.tmp_dir)
        os.environ["PYTHONPYCACHEPREFIX"] = str(ensure_dir(self.run_dir / "pycache"))

        atexit.register(self.cleanup)

    def register_cleanup(self, cb: Callable[[], None]) -> None:
        self._cleanup_callbacks.append(cb)

    def case_dir(self, test_id: str) -> Path:
        safe_name = "".join(ch if ch.isalnum() or ch in ("-", "_") else "_" for ch in test_id)
        return ensure_dir(self.cases_dir / safe_name)

    def env(self, *, scrub_loopback_proxy: bool = True, lock_held: bool = False) -> dict[str, str]:
        return make_base_env(
            self.tmp_dir,
            scrub_loopback_proxy=scrub_loopback_proxy,
            lock_held=lock_held,
        )

    def _require_native_ok(self) -> None:
        if self.native_build_error:
            raise AssertionError(f"Native Rust host build failed: {self.native_build_error}")

    def require_jvm_ok(self) -> None:
        if self.jvm_build_error:
            raise AssertionError(f"Android/JVM module compilation failed: {self.jvm_build_error}")

    @property
    def host_lib_so(self) -> Path:
        self._require_native_ok()
        return self.native_snapshot_dir / "host-lib" / "libmahayana_app_host.so"

    @property
    def host_lib_dir_raw(self) -> Path:
        return self.native_snapshot_dir / "host-lib"

    @property
    def arm64_lib_so(self) -> Path:
        self._require_native_ok()
        return self.native_snapshot_dir / "arm64-v8a" / "libmahayana_app_host.so"

    @property
    def x86_64_lib_so(self) -> Path:
        self._require_native_ok()
        return self.native_snapshot_dir / "x86_64" / "libmahayana_app_host.so"

    @property
    def cargo_metadata_json(self) -> Path:
        return self.native_snapshot_dir / "cargo_metadata.json"

    @property
    def runtime_cp_file(self) -> Path:
        self.require_jvm_ok()
        return self.jvm_snapshot_dir / "runtime_cp.txt"

    @property
    def friend_dirs_file(self) -> Path:
        self.require_jvm_ok()
        return self.jvm_snapshot_dir / "friend_dirs.txt"

    def materialize_snapshots(self, needs: set[str] | frozenset[str]) -> None:
        want_jvm = "jvm" in needs
        want_native = "native" in needs or want_jvm
        if not want_native:
            return
        if self._native_attempted and (not want_jvm or self._jvm_attempted):
            return

        with acquire_build_lock():
            try:
                build_env = self.env(scrub_loopback_proxy=False, lock_held=True)
                # Refresh proxy/trustStore exports from ~/.bashrc if rotated across uVM pause/resume
                bashrc_probe = subprocess.run(
                    ["bash", "-c", "source ~/.bashrc >/dev/null 2>&1 || true; env -0"],
                    env=build_env,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    check=False,
                )
                if bashrc_probe.returncode == 0 and bashrc_probe.stdout:
                    for entry in bashrc_probe.stdout.decode("utf-8", errors="ignore").split("\x00"):
                        if "=" in entry:
                            k, v = entry.split("=", 1)
                            if k in ("GRADLE_OPTS", "SDKMANAGER_OPTS", "HTTPS_PROXY", "HTTP_PROXY", "https_proxy", "http_proxy"):
                                build_env[k] = v
                build_env["TMPDIR"] = str(self.tmp_dir)
                if want_native and not self._native_attempted:
                    self._native_attempted = True
                    try:
                        self._build_and_copy_native(build_env)
                    except Exception as exc:  # noqa: BLE001
                        self.native_build_error = str(exc).splitlines()[-1] if str(exc) else repr(exc)
                        full_lines = [ln.strip() for ln in str(exc).splitlines() if ln.strip().startswith("error")]
                        if full_lines:
                            self.native_build_error = " | ".join(full_lines[:3])
                        print(f"[e2e-snapshot] Native build error recorded: {self.native_build_error}", file=sys.stderr, flush=True)
                elif want_jvm and not self.native_build_error:
                    self._restore_staged_native_for_gradle()

                if want_jvm and not self._jvm_attempted:
                    self._jvm_attempted = True
                    try:
                        self._build_and_copy_jvm(build_env)
                    except Exception as exc:  # noqa: BLE001
                        self.jvm_build_error = " | ".join(str(exc).splitlines()[-4:])
                        print(f"[e2e-snapshot] JVM build error recorded: {self.jvm_build_error}", file=sys.stderr, flush=True)
            finally:
                stop_jvm_daemons()
                clean_repo_temp_build_artifacts(
                    preserve_release_artifacts=self.preserve_release_artifacts
                )

    def _build_and_copy_native(self, build_env: dict[str, str]) -> None:
        meta_proc = subprocess.run(
            ["cargo", "metadata", "--format-version", "1"],
            cwd=str(REPO_ROOT / "mobile" / "android" / "rust" / "mahayana_android_host"),
            env=build_env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
        if meta_proc.returncode == 0 and meta_proc.stdout.strip():
            self.cargo_metadata_json.write_text(meta_proc.stdout, encoding="UTF-8")
        else:
            self.cargo_metadata_json.write_text(json.dumps({"packages": []}), encoding="UTF-8")

        raw_host_so = self.native_snapshot_dir / "host-lib" / "libmahayana_app_host.so"
        raw_arm_so = self.native_snapshot_dir / "arm64-v8a" / "libmahayana_app_host.so"
        raw_x64_so = self.native_snapshot_dir / "x86_64" / "libmahayana_app_host.so"

        src_hash = _compute_rust_source_hash()
        cache_root = ensure_dir(Path("/home/antigravity/.cache/teamwork_tmp/e2e_native_cache"))
        cache_entry = cache_root / src_hash
        if (
            (cache_entry / "host-lib" / "libmahayana_app_host.so").is_file()
            and (cache_entry / "arm64-v8a" / "libmahayana_app_host.so").is_file()
            and (cache_entry / "x86_64" / "libmahayana_app_host.so").is_file()
        ):
            print(f"[e2e-snapshot] Reusing verified SHA-256 ({src_hash[:12]}) native Rust snapshot...", file=sys.stderr, flush=True)
            for rel, dst in (
                ("host-lib/libmahayana_app_host.so", raw_host_so),
                ("arm64-v8a/libmahayana_app_host.so", raw_arm_so),
                ("x86_64/libmahayana_app_host.so", raw_x64_so),
            ):
                ensure_dir(dst.parent)
                shutil.copyfile(cache_entry / rel, dst)
            return

        print("[e2e-snapshot] Building native Rust host libraries under flock...", file=sys.stderr, flush=True)
        rc, out = _run_pgroup(
            ["bash", str(REPO_ROOT / "scripts" / "build_rust_android.sh")],
            cwd=str(REPO_ROOT),
            env=build_env,
            timeout=450,
        )
        if rc != 0:
            raise RuntimeError(
                f"scripts/build_rust_android.sh failed (exit {rc}):\n{out[-4000:]}"
            )

        src_map = {
            REPO_ROOT / "mobile" / "android" / "rust" / "host-lib" / "libmahayana_app_host.so": raw_host_so,
            REPO_ROOT / "mobile" / "android" / "app" / "src" / "main" / "jniLibs" / "arm64-v8a" / "libmahayana_app_host.so": raw_arm_so,
            REPO_ROOT / "mobile" / "android" / "app" / "src" / "main" / "jniLibs" / "x86_64" / "libmahayana_app_host.so": raw_x64_so,
        }
        for src, dst in src_map.items():
            if not src.is_file():
                raise RuntimeError(f"Expected native build artifact missing: {src}")
            ensure_dir(dst.parent)
            shutil.copyfile(src, dst)

        # Prune older cache entries and persist current content-addressed snapshot
        for child in cache_root.iterdir():
            if child.name != src_hash:
                shutil.rmtree(child, ignore_errors=True)
        for rel, src in (
            ("host-lib/libmahayana_app_host.so", raw_host_so),
            ("arm64-v8a/libmahayana_app_host.so", raw_arm_so),
            ("x86_64/libmahayana_app_host.so", raw_x64_so),
        ):
            c_dst = cache_entry / rel
            ensure_dir(c_dst.parent)
            shutil.copyfile(src, c_dst)

    def _restore_staged_native_for_gradle(self) -> None:
        raw_host_so = self.native_snapshot_dir / "host-lib" / "libmahayana_app_host.so"
        raw_arm_so = self.native_snapshot_dir / "arm64-v8a" / "libmahayana_app_host.so"
        raw_x64_so = self.native_snapshot_dir / "x86_64" / "libmahayana_app_host.so"
        restore_map = {
            raw_host_so: REPO_ROOT / "mobile" / "android" / "rust" / "host-lib" / "libmahayana_app_host.so",
            raw_arm_so: REPO_ROOT / "mobile" / "android" / "app" / "src" / "main" / "jniLibs" / "arm64-v8a" / "libmahayana_app_host.so",
            raw_x64_so: REPO_ROOT / "mobile" / "android" / "app" / "src" / "main" / "jniLibs" / "x86_64" / "libmahayana_app_host.so",
        }
        for src, dst in restore_map.items():
            if src.is_file() and not dst.is_file():
                ensure_dir(dst.parent)
                shutil.copyfile(src, dst)

    def _build_and_copy_jvm(self, build_env: dict[str, str]) -> None:
        print("[e2e-snapshot] Compiling JVM/Android modules (:mika-core, :mika-agent, :app) under flock...", file=sys.stderr, flush=True)
        init_script = self.run_dir / "export_e2e_cp.init.gradle"
        init_script.write_text(_EXPORT_CP_INIT_GRADLE, encoding="UTF-8")
        export_dir = ensure_dir(self.jvm_snapshot_dir / "raw_export")

        jvm_net_flags = ""
        cacerts = Path("/home/antigravity/.config/java-certs/cacerts.p12")
        if cacerts.is_file():
            jvm_net_flags += (
                f" -Djavax.net.ssl.trustStore={cacerts}"
                " -Djavax.net.ssl.trustStorePassword=changeit"
                " -Djavax.net.ssl.trustStoreType=PKCS12"
            )
        proxy_url = build_env.get("HTTPS_PROXY") or build_env.get("HTTP_PROXY") or ""
        if proxy_url.startswith("http://127.0.0.1:"):
            p_port = proxy_url.rsplit(":", 1)[-1].strip("/")
            jvm_net_flags += (
                f" -Dhttp.proxyHost=127.0.0.1 -Dhttp.proxyPort={p_port}"
                f" -Dhttps.proxyHost=127.0.0.1 -Dhttps.proxyPort={p_port}"
            )

        def _make_gradle_cmd(tasks: list[str], *, offline: bool) -> list[str]:
            jtmp = f"-Djava.io.tmpdir={self.jvm_tmp_dir}"
            c = [
                str(REPO_ROOT / "mobile" / "android" / "gradlew"),
                "--no-daemon",
                "--max-workers=1",
                jtmp,
            ]
            if offline:
                c.append("--offline")
            c.extend(
                [
                    f"-Dorg.gradle.jvmargs=-Xmx1664m -XX:MaxMetaspaceSize=512m -Dfile.encoding=UTF-8 {jtmp}{jvm_net_flags}",
                    f"-Dkotlin.daemon.jvmargs=-Xmx1152m -XX:MaxMetaspaceSize=384m {jtmp}",
                    f"-De2e.export.dir={export_dir}",
                    "-PversionName=2.1.0",
                    "-PversionCode=20100",
                    "-PgitHash=v2.1.0",
                    "--init-script",
                    str(init_script),
                ]
            )
            if self.native_build_error:
                c.extend(["-x", ":app:ensureRustHostLibs"])
            c.extend(tasks)
            return c

        step_task_groups = [
            [
                ":mika-core:classes",
                ":mika-core:testFixturesClasses",
                ":mika-agent:compileDebugKotlin",
                ":mika-agent:compileDebugJavaWithJavac",
            ],
            [
                ":app:compileDebugKotlin",
                ":app:compileDebugJavaWithJavac",
                "exportE2eClasspath",
            ],
        ]

        for step_tasks in step_task_groups:
            rc, out = _run_pgroup(
                _make_gradle_cmd(step_tasks, offline=True),
                cwd=str(REPO_ROOT / "mobile" / "android"),
                env=build_env,
                timeout=240,
            )
            if rc != 0 and "No cached version" in out:
                stop_jvm_daemons()
                rc, out = _run_pgroup(
                    _make_gradle_cmd(step_tasks, offline=False),
                    cwd=str(REPO_ROOT / "mobile" / "android"),
                    env=build_env,
                    timeout=240,
                )
            stop_jvm_daemons()
            if rc != 0:
                raise RuntimeError(
                    f"Gradle JVM snapshot compilation failed on {step_tasks} (exit {rc}):\n{out[-4000:]}"
                )

        raw_cp = (export_dir / "raw_runtime_cp.txt").read_text(encoding="UTF-8").strip()
        raw_friends = (export_dir / "raw_friend_dirs.txt").read_text(encoding="UTF-8").strip()

        repo_prefix = str(REPO_ROOT.resolve())
        copied_map: dict[str, str] = {}
        entries_root = ensure_dir(self.jvm_snapshot_dir / "entries")

        def snapshot_path(orig_str: str) -> str:
            if not orig_str:
                return ""
            if orig_str in copied_map:
                return copied_map[orig_str]
            p = Path(orig_str)
            if not p.exists():
                return ""
            resolved = str(p.resolve())
            if resolved.startswith(repo_prefix):
                idx = len(copied_map)
                dst = entries_root / f"e_{idx}_{p.name}"
                if p.is_dir():
                    shutil.copytree(p, dst, dirs_exist_ok=True)
                else:
                    ensure_dir(dst.parent)
                    shutil.copyfile(p, dst)
                copied_map[orig_str] = str(dst)
                return str(dst)
            copied_map[orig_str] = resolved
            return resolved

        final_cp: list[str] = []
        for item in raw_cp.split(os.pathsep):
            mapped = snapshot_path(item.strip())
            if mapped and mapped not in final_cp:
                final_cp.append(mapped)

        extra_jars = [
            *sorted(GRADLE_CACHE_DIR.glob("*/transforms/*/transformed/android.jar")),
            *sorted(GRADLE_CACHE_DIR.glob("modules-2/files-2.1/com.google.zxing/core/3.5.3/*/core-3.5.3.jar")),
            *sorted(GRADLE_CACHE_DIR.glob("modules-2/files-2.1/com.squareup.okhttp3/mockwebserver/5.4.0/*/mockwebserver-5.4.0.jar")),
            *sorted(GRADLE_CACHE_DIR.glob("modules-2/files-2.1/org.jetbrains.kotlinx/kotlinx-coroutines-test-jvm/1.10.2/*/kotlinx-coroutines-test-jvm-1.10.2.jar")),
            *sorted(GRADLE_CACHE_DIR.glob("modules-2/files-2.1/org.json/json/20240303/*/json-20240303.jar")),
            *sorted(GRADLE_CACHE_DIR.glob("modules-2/files-2.1/org.ow2.asm/asm/9.9/*/asm-9.9.jar")),
            *sorted(GRADLE_CACHE_DIR.glob("modules-2/files-2.1/org.ow2.asm/asm-tree/9.9/*/asm-tree-9.9.jar")),
            *sorted(GRADLE_CACHE_DIR.glob("modules-2/files-2.1/junit/junit/4.13.2/*/junit-4.13.2.jar")),
        ]
        for jar in extra_jars:
            jar_str = str(jar.resolve())
            if jar.is_file() and jar_str not in final_cp:
                final_cp.append(jar_str)

        final_friends: list[str] = []
        for item in raw_friends.split(","):
            mapped = snapshot_path(item.strip())
            if mapped and mapped not in final_friends:
                final_friends.append(mapped)

        (self.jvm_snapshot_dir / "runtime_cp.txt").write_text(os.pathsep.join(final_cp), encoding="UTF-8")
        (self.jvm_snapshot_dir / "friend_dirs.txt").write_text(",".join(final_friends), encoding="UTF-8")

    def cleanup(self) -> None:
        if self._cleaned:
            return
        self._cleaned = True
        for cb in reversed(self._cleanup_callbacks):
            try:
                cb()
            except Exception:  # noqa: BLE001
                pass
        # Only stop JVM daemons if fabushi-build.lock is free so we never kill a concurrent milestone's active build
        ensure_dir(SCRATCH_BASE)
        fd = os.open(str(BUILD_LOCK_FILE), os.O_CREAT | os.O_RDWR, 0o666)
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            try:
                stop_jvm_daemons()
            finally:
                fcntl.flock(fd, fcntl.LOCK_UN)
        except OSError:
            pass
        finally:
            os.close(fd)
        if self.run_dir.exists():
            shutil.rmtree(self.run_dir, ignore_errors=True)
        if self.jvm_tmp_dir.exists():
            shutil.rmtree(self.jvm_tmp_dir, ignore_errors=True)
