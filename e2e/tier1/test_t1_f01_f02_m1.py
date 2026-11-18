"""Tier 1 E2E tests for M1 features F1 (Spec Governance & Git Hygiene) and F2 (Toolchain & Release Pipeline)."""

from __future__ import annotations

import importlib.util
import os
import re
import struct
import subprocess
import zipfile
from pathlib import Path

from e2e.harness.apk_builder import create_valid_unsigned_apk
from e2e.harness.context import E2eTestContext
from e2e.harness.host_ffi import inspect_elf64_alignment_and_machine, list_defined_dynsyms
from e2e.harness.registry import e2e_test


def _load_packager_module(repo_root: Path):
    script = repo_root / "scripts" / "package_android_release.py"
    spec = importlib.util.spec_from_file_location("package_android_release", script)
    assert spec is not None and spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def _build_synthetic_16kb_elf64(e_machine: int = 183, p_align: int = 0x4000) -> bytes:
    # Minimal valid 64-bit little-endian ET_DYN ELF with 1 PT_LOAD segment
    ehdr = bytearray(64)
    ehdr[0:4] = b"\x7fELF"
    ehdr[4] = 2  # ELFCLASS64
    ehdr[5] = 1  # ELFDATA2LSB
    ehdr[6] = 1  # EV_CURRENT
    struct.pack_into("<HHIQQQIHHHHHH", ehdr, 16, 3, e_machine, 1, 0, 64, 0, 0, 64, 56, 1, 0, 0, 0)
    phdr = struct.pack("<IIQQQQQQ", 1, 5, 0, 0, 0, 120, 120, p_align)
    return bytes(ehdr) + phdr


# =============================================================================
# F1 — Spec-First Governance, Upstream Provenance & Git Hygiene (6 Tests)
# =============================================================================


@e2e_test(
    "T1-F01-01",
    tier=1,
    features=["F1"],
    title="Active production-parity Spec contains all 17 SPEC_TEMPLATE sections and R1..R15 + AC-1..AC-15",
)
def test_t1_f01_01_active_spec_completeness(ctx: E2eTestContext) -> None:
    spec_path = ctx.repo_root / "docs" / "specs" / "gpui-telegram-android-production-parity.md"
    assert spec_path.is_file(), f"Missing active Spec: {spec_path}"
    text = spec_path.read_text(encoding="utf-8")
    assert re.search(r"^Status:\s*active\b", text, re.MULTILINE), "Active Spec must declare 'Status: active'"

    for sec_num in range(1, 18):
        assert re.search(rf"^##\s+{sec_num}\.\s+\S+", text, re.MULTILINE), (
            f"Active Spec missing mandatory section '## {sec_num}.'"
        )

    sec17 = text.split("## 17. Spec compliance record", 1)[1]
    for idx in range(1, 16):
        assert re.search(rf"\bR{idx}\b", text), f"Missing requirement R{idx} in Spec"
        assert re.search(rf"\bAC-{idx}\b", text), f"Missing acceptance criterion AC-{idx} in Spec"
        assert re.search(rf"^\|\s*R{idx}\s*\|", sec17, re.MULTILINE), f"Missing R{idx} row in §17 table"
        assert re.search(rf"^\|\s*AC-{idx}\s*\|", sec17, re.MULTILINE), f"Missing AC-{idx} row in §17 table"


@e2e_test(
    "T1-F01-02",
    tier=1,
    features=["F1"],
    title="Legacy gpui-telegram-android-mika-fusion.md is marked superseded with honest blocked audit rows",
)
def test_t1_f01_02_legacy_spec_superseded(ctx: E2eTestContext) -> None:
    fusion_path = ctx.repo_root / "docs" / "specs" / "gpui-telegram-android-mika-fusion.md"
    assert fusion_path.is_file(), f"Missing superseded Spec: {fusion_path}"
    text = fusion_path.read_text(encoding="utf-8")
    assert re.search(r"^Status:\s*superseded\b", text, re.MULTILINE), (
        "gpui-telegram-android-mika-fusion.md must have 'Status: superseded'"
    )
    assert "gpui-telegram-android-production-parity.md" in text, (
        "Superseded Spec must cross-reference gpui-telegram-android-production-parity.md"
    )
    sec17 = text.split("## 17. Spec compliance record", 1)[1]
    for item in ("R1", "R2", "R3", "R4", "R5", "R6", "R9", "R10", "AC-4", "AC-5", "AC-6", "AC-7"):
        m = re.search(rf"^\|\s*{re.escape(item)}\s*\|\s*([a-z-]+)\s*\|", sec17, re.MULTILINE)
        assert m is not None, f"Missing {item} row in fusion Spec §17"
        assert m.group(1) == "blocked", f"Audited incomplete item {item} must be 'blocked', got {m.group(1)!r}"


@e2e_test(
    "T1-F01-03",
    tier=1,
    features=["F1"],
    title="spec-first-ai-development.md and AGENTS.md align on canonical fabu-android/fabushi-android ownership",
)
def test_t1_f01_03_governance_alignment(ctx: E2eTestContext) -> None:
    policy = (ctx.repo_root / "docs" / "specs" / "spec-first-ai-development.md").read_text(encoding="utf-8")
    agents = (ctx.repo_root / "AGENTS.md").read_text(encoding="utf-8")
    assert "fabu-android/fabushi-android" in policy, "spec-first-ai-development.md must name fabu-android/fabushi-android"
    assert "No Spec, No Code" in policy and "No Spec, No Code" in agents
    assert ":mika-core" in policy and ":mika-agent" in policy
    assert "mahayana_android_host" in policy and "mahayana_android_host" in agents
    assert "KotlinCompileDaemon" in agents and "GradleDaemon" in agents


@e2e_test(
    "T1-F01-04",
    tier=1,
    features=["F1"],
    title="Exact 40-hex upstream commits for bhrum/gpui-telegram and wangzan101/Mika are pinned",
)
def test_t1_f01_04_upstream_commit_pins(ctx: E2eTestContext) -> None:
    spec_text = (ctx.repo_root / "docs" / "specs" / "gpui-telegram-android-production-parity.md").read_text(
        encoding="utf-8"
    )
    gpui_sha = "889246e4621e3e8e58022895a887c63ea3130bdd"
    mika_sha = "b899c5bb01acd80cc29f0cc25cad189a8cc20b46"
    assert gpui_sha in spec_text, f"Missing 40-hex gpui-telegram pin {gpui_sha} in active Spec"
    assert mika_sha in spec_text, f"Missing 40-hex Mika pin {mika_sha} in active Spec"


@e2e_test(
    "T1-F01-05",
    tier=1,
    features=["F1"],
    title="git check-ignore confirms .git/info/exclude blocks coordination, jniLibs host .so, keystores & dist",
)
def test_t1_f01_05_git_exclude_rules(ctx: E2eTestContext) -> None:
    must_ignore = [
        ".agents/teamwork/probe.md",
        "PROJECT.md",
        "TEST_INFRA.md",
        "TEST_READY.md",
        "mobile/android/local.properties",
        "mobile/android/app/src/main/jniLibs/arm64-v8a/libmahayana_app_host.so",
        "mobile/android/rust/host-lib/libmahayana_app_host.so",
        "ephemeral.p12",
        "secret.keystore",
        "release-signing/test.jks",
        "dist/fabushi-android-update.json",
    ]
    proc = subprocess.run(
        ["git", "check-ignore", "--stdin"],
        cwd=str(ctx.repo_root),
        input="\n".join(must_ignore) + "\n",
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    ignored = set(proc.stdout.splitlines())
    missing = [p for p in must_ignore if p not in ignored]
    assert not missing, f"git check-ignore failed to ignore required ephemeral paths: {missing}"


@e2e_test(
    "T1-F01-06",
    tier=1,
    features=["F1"],
    title="git ls-files tracks required Mika prebuilts/rootfs while tracking zero keystores or generated host .so",
)
def test_t1_f01_06_git_tracked_hygiene(ctx: E2eTestContext) -> None:
    proc = subprocess.run(
        ["git", "ls-files"],
        cwd=str(ctx.repo_root),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=True,
    )
    tracked = set(proc.stdout.splitlines())
    forbidden = [
        f
        for f in tracked
        if f.endswith(("libmahayana_app_host.so", ".p12", ".jks", ".keystore", "local.properties"))
        or f.startswith(".agents/")
    ]
    assert not forbidden, f"Forbidden generated/secret files tracked in git index: {forbidden}"

    required_mika_files = [
        "mobile/android/mika-agent/src/main/jniLibs/arm64-v8a/libmikarun.so",
        "mobile/android/mika-agent/src/main/jniLibs/arm64-v8a/libproot.so",
        "mobile/android/mika-agent/src/main/jniLibs/arm64-v8a/libproot_loader.so",
        "mobile/android/mika-agent/src/main/assets/runtime/rootfs.tar.xz",
    ]
    for rel in required_mika_files:
        assert rel in tracked, f"Required Mika runtime asset not tracked in git: {rel}"
        full = ctx.repo_root / rel
        assert full.is_file() and full.stat().st_size > 1024, f"Missing or empty Mika asset: {full}"

    rootfs = ctx.repo_root / "mobile/android/mika-agent/src/main/assets/runtime/rootfs.tar.xz"
    with rootfs.open("rb") as fh:
        magic = fh.read(6)
    assert magic == b"\xfd7zXZ\x00", f"rootfs.tar.xz does not start with XZ magic bytes: {magic!r}"


# =============================================================================
# F2 — Cloud Toolchain, Monotonic Versioning & Clean Release Pipeline (6 Tests)
# =============================================================================


@e2e_test(
    "T1-F02-01",
    tier=1,
    features=["F2"],
    title="Cloud VM toolchain (Rust >= 1.85 cross targets, Android SDK 37, NDK 27, PKCS12 truststore, env-setup)",
)
def test_t1_f02_01_cloud_toolchain_environment(ctx: E2eTestContext) -> None:
    env_setup = Path("/home/antigravity/env-setup.sh")
    assert env_setup.is_file(), "Missing /home/antigravity/env-setup.sh"
    assert (Path("/home/antigravity/android-sdk/platforms/android-37.0")).is_dir(), "Missing Android SDK 37.0"
    assert (Path("/home/antigravity/android-sdk/build-tools/37.0.0")).is_dir(), "Missing build-tools 37.0.0"
    assert (Path("/home/antigravity/android-sdk/ndk/27.2.12479018")).is_dir(), "Missing NDK 27.2.12479018"
    assert (
        Path("/home/antigravity/.config/java-certs/cacerts.p12").is_file()
        or Path("/home/antigravity/.gradle/cacerts.p12").is_file()
    ), "Missing PKCS12 trustStore cacerts.p12"
    assert (ctx.repo_root / "mobile" / "android" / "local.properties").is_file(), "Missing local.properties"

    env = ctx.env(scrub_loopback_proxy=True)
    targets_proc = subprocess.run(
        ["rustup", "target", "list", "--installed"],
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=True,
    )
    installed = set(targets_proc.stdout.split())
    assert "aarch64-linux-android" in installed, f"Missing aarch64-linux-android target: {installed}"
    assert "x86_64-linux-android" in installed, f"Missing x86_64-linux-android target: {installed}"

    # Verify non-interactive shell sourcing preserves proxy & sets scratch TMPDIR
    sub = subprocess.run(
        [
            "env",
            "-i",
            "HOME=/home/antigravity",
            "PATH=/usr/local/bin:/usr/bin:/bin",
            "HTTPS_PROXY=http://127.0.0.1:36397",
            "bash",
            "-c",
            "source ~/.bashrc && printf '%s|%s|%s' \"$ANDROID_HOME\" \"$TMPDIR\" \"$HTTPS_PROXY\"",
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=True,
    )
    parts = sub.stdout.strip().split("|")
    assert len(parts) == 3, f"Unexpected .bashrc output: {sub.stdout!r}"
    assert parts[0] == "/home/antigravity/android-sdk"
    assert parts[1].startswith("/home/antigravity/.gemini/jetski/scratch/teamwork_tmp")
    assert parts[2] == "http://127.0.0.1:36397"


@e2e_test(
    "T1-F02-02",
    tier=1,
    features=["F2"],
    title="package_android_release.py --compute-version-code derives monotonic versionCode across semver tags",
)
def test_t1_f02_02_compute_version_code_cli(ctx: E2eTestContext) -> None:
    script = str(ctx.repo_root / "scripts" / "package_android_release.py")
    vectors = {
        "v2.0.0": 20000,
        "v2.1.0": 20100,
        "2.1.0": 20100,
        "android-v2.1.3": 20103,
        "v2.12.34-rc.1+build.7": 21234,
        "v3.0.0": 30000,
    }
    for tag, expected in vectors.items():
        proc = subprocess.run(
            ["python3", script, "--compute-version-code", tag],
            cwd=str(ctx.repo_root),
            env=ctx.env(),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
        assert proc.returncode == 0, f"--compute-version-code {tag} failed: {proc.stderr}"
        assert int(proc.stdout.strip()) == expected, f"Tag {tag}: expected {expected}, got {proc.stdout!r}"


@e2e_test(
    "T1-F02-03",
    tier=1,
    features=["F2"],
    title="package_android_release.py --resolve-release-metadata writes deterministic GITHUB_ENV key-values",
)
def test_t1_f02_03_resolve_release_metadata_github_env(ctx: E2eTestContext) -> None:
    script = str(ctx.repo_root / "scripts" / "package_android_release.py")
    gh_env = ctx.subdir("gh") / "github_env.txt"
    proc = subprocess.run(
        [
            "python3",
            script,
            "--resolve-release-metadata",
            "--tag=android-v2.1.0",
            f"--github-env={gh_env}",
        ],
        cwd=str(ctx.repo_root),
        env=ctx.env(),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=True,
    )
    lines = dict(line.split("=", 1) for line in gh_env.read_text(encoding="utf-8").strip().splitlines())
    assert lines == {
        "RELEASE_TAG": "android-v2.1.0",
        "RELEASE_VER": "2.1.0",
        "RELEASE_VCODE": "20100",
    }, f"Unexpected GITHUB_ENV output: {lines}"
    assert "RELEASE_VCODE=20100" in proc.stdout

    # Monotonic upward override (e.g. hotfix 20105 >= 20100) must also succeed
    gh_env2 = ctx.subdir("gh") / "github_env_override.txt"
    subprocess.run(
        [
            "python3",
            script,
            "--resolve-release-metadata",
            "--tag=v2.1.0",
            "--version-code=20105",
            f"--github-env={gh_env2}",
        ],
        cwd=str(ctx.repo_root),
        env=ctx.env(),
        check=True,
    )
    lines2 = dict(line.split("=", 1) for line in gh_env2.read_text(encoding="utf-8").strip().splitlines())
    assert lines2["RELEASE_VCODE"] == "20105"


@e2e_test(
    "T1-F02-04",
    tier=1,
    features=["F2"],
    title="Repo-clean deterministic PKCS12 keystore, 16KB zipalign -P 16, apksigner v1+v2+v3 & verify_apk_contents",
)
def test_t1_f02_04_deterministic_keystore_and_apk_signing(ctx: E2eTestContext) -> None:
    mod = _load_packager_module(ctx.repo_root)
    work = ctx.subdir("sign_roundtrip")
    ks_cache = work / "signing_cache"
    unsigned_apk = work / "unsigned.apk"
    signed_apk_1 = work / "signed_1.apk"
    signed_apk_2 = work / "signed_2.apk"

    create_valid_unsigned_apk(
        unsigned_apk,
        work,
        arm64_host_bytes=_build_synthetic_16kb_elf64(183, 0x4000),
        x64_host_bytes=_build_synthetic_16kb_elf64(62, 0x4000),
        mikarun_bytes=_build_synthetic_16kb_elf64(183, 0x4000),
        proot_bytes=_build_synthetic_16kb_elf64(183, 0x4000),
        proot_loader_bytes=_build_synthetic_16kb_elf64(183, 0x4000),
        rootfs_bytes=b"\xfd7zXZ\x00" + b"rootfs-payload",
    )

    old_cache = os.environ.get("FABUSHI_SIGNING_CACHE_DIR")
    old_ks = os.environ.pop("ANDROID_KEYSTORE_PATH", None)
    old_b64 = os.environ.pop("ANDROID_KEYSTORE_BASE64", None)
    try:
        os.environ["FABUSHI_SIGNING_CACHE_DIR"] = str(ks_cache)
        mod.ensure_signed_apk(unsigned_apk, signed_apk_1, ctx.repo_root, variant="release")
        mod.ensure_signed_apk(unsigned_apk, signed_apk_2, ctx.repo_root, variant="githubRelease")
    finally:
        if old_cache is None:
            os.environ.pop("FABUSHI_SIGNING_CACHE_DIR", None)
        else:
            os.environ["FABUSHI_SIGNING_CACHE_DIR"] = old_cache
        if old_ks is not None:
            os.environ["ANDROID_KEYSTORE_PATH"] = old_ks
        if old_b64 is not None:
            os.environ["ANDROID_KEYSTORE_BASE64"] = old_b64

    info = mod.verify_apk_contents(signed_apk_1)
    assert info["has_x86_64_host"] is True
    assert not (ctx.repo_root / "release-signing").exists(), "Keystore leaked into repo_root/release-signing!"

    apksigner = mod.find_build_tool("apksigner", ctx.repo_root)
    cert1 = subprocess.run(
        [apksigner, "verify", "--print-certs", str(signed_apk_1)],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=True,
    ).stdout
    cert2 = subprocess.run(
        [apksigner, "verify", "--print-certs", str(signed_apk_2)],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=True,
    ).stdout
    assert cert1 == cert2 and "SHA-256 digest:" in cert1, "Signing certificate changed across runs!"


@e2e_test(
    "T1-F02-05",
    tier=1,
    features=["F2"],
    title="CI/Release GitHub Actions workflows and Gradle scripts enforce API 29, sequential R8 & dynamic versionCode",
)
def test_t1_f02_05_ci_release_workflow_contracts(ctx: E2eTestContext) -> None:
    ci_yml = (ctx.repo_root / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
    rel_yml = (ctx.repo_root / ".github" / "workflows" / "release.yml").read_text(encoding="utf-8")
    mika_gradle = (ctx.repo_root / "mobile" / "android" / "mika-agent" / "build.gradle.kts").read_text(encoding="utf-8")

    assert 'ANDROID_API_LEVEL: "29"' in ci_yml and 'ANDROID_API_LEVEL: "29"' in rel_yml
    assert "27.2.12479018" in ci_yml and "27.2.12479018" in rel_yml
    assert "--resolve-release-metadata" in rel_yml, "release.yml must resolve monotonic versionCode via package_android_release.py"
    assert "-PversionCode=\"$RELEASE_VCODE\"" in rel_yml
    assert "-PversionName=\"$RELEASE_VER\"" in rel_yml

    # Verify sequential R8 tasks (assembleRelease and assembleGithubRelease in separate ./gradlew invocations)
    idx_rel = rel_yml.find(":app:assembleRelease")
    idx_gh = rel_yml.find(":app:assembleGithubRelease")
    assert 0 < idx_rel < idx_gh, "release.yml must run :app:assembleRelease before :app:assembleGithubRelease"
    between = rel_yml[idx_rel:idx_gh]
    assert "./gradlew" in between, "assembleRelease and assembleGithubRelease must execute in separate sequential ./gradlew steps"

    assert 'findProperty("versionName")' in mika_gradle and 'findProperty("versionCode")' in mika_gradle


@e2e_test(
    "T1-F02-06",
    tier=1,
    features=["F2"],
    needs=["native"],
    title="build_rust_android.sh produces 16KB-aligned arm64-v8a & x86_64 ELF64 .so + host-lib with all JNI/C symbols",
)
def test_t1_f02_06_native_cross_build_16kb_and_symbols(ctx: E2eTestContext) -> None:
    arm64_info = inspect_elf64_alignment_and_machine(ctx.ws.arm64_lib_so)
    x64_info = inspect_elf64_alignment_and_machine(ctx.ws.x86_64_lib_so)
    host_info = inspect_elf64_alignment_and_machine(ctx.ws.host_lib_so)

    assert arm64_info["e_type"] == 3 and arm64_info["e_machine"] == 183
    assert x64_info["e_type"] == 3 and x64_info["e_machine"] == 62
    assert host_info["e_type"] == 3 and host_info["e_machine"] == 62

    assert arm64_info["pt_load_aligns"] and all(a >= 0x4000 and (a & (a - 1)) == 0 for a in arm64_info["pt_load_aligns"]), (
        f"arm64-v8a PT_LOAD segments violate 16KB alignment: {arm64_info['pt_load_aligns']}"
    )
    assert x64_info["pt_load_aligns"] and all(a >= 0x4000 and (a & (a - 1)) == 0 for a in x64_info["pt_load_aligns"]), (
        f"x86_64 PT_LOAD segments violate 16KB alignment: {x64_info['pt_load_aligns']}"
    )

    required_syms = {
        "Java_com_ombhrum_fabushi_core_MahayanaHost_nativeCreate",
        "Java_com_ombhrum_fabushi_core_MahayanaHost_nativeCreateTest",
        "Java_com_ombhrum_fabushi_core_MahayanaHost_nativeDispatch",
        "Java_com_ombhrum_fabushi_core_MahayanaHost_nativeDestroy",
        "mahayana_app_host_create",
        "mahayana_app_host_create_test",
        "mahayana_app_host_dispatch_with_handle",
        "mahayana_app_host_destroy",
        "mahayana_app_host_free_string",
    }
    env = ctx.env()
    for label, so_file in (
        ("arm64-v8a", ctx.ws.arm64_lib_so),
        ("x86_64", ctx.ws.x86_64_lib_so),
        ("host-lib", ctx.ws.host_lib_so),
    ):
        syms = list_defined_dynsyms(so_file, env)
        missing = required_syms - syms
        assert not missing, f"{label} libmahayana_app_host.so missing exported symbols: {sorted(missing)}"
