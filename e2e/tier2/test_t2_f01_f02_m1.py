"""Tier 2 Boundary, Corner & Negative E2E tests for M1 features F1 and F2 (12 tests)."""

from __future__ import annotations

import importlib.util
import os
import re
import struct
import subprocess
import tomllib
from pathlib import Path

from e2e.harness.context import E2eTestContext
from e2e.harness.registry import e2e_test


def _load_packager(repo_root: Path):
    script = repo_root / "scripts" / "package_android_release.py"
    spec = importlib.util.spec_from_file_location("package_android_release_t2", script)
    assert spec is not None and spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def _make_elf64_header(
    *,
    magic: bytes = b"\x7fELF",
    ei_class: int = 2,
    ei_data: int = 1,
    e_type: int = 3,
    e_machine: int = 183,
    pt_load_aligns: list[int] | None = None,
    truncate_bytes: int | None = None,
) -> bytes:
    aligns = [0x4000] if pt_load_aligns is None else pt_load_aligns
    ehdr = bytearray(64)
    ehdr[0:4] = magic
    ehdr[4] = ei_class
    ehdr[5] = ei_data
    ehdr[6] = 1
    struct.pack_into("<HHIQQQIHHHHHH", ehdr, 16, e_type, e_machine, 1, 0, 64, 0, 0, 64, 56, len(aligns), 0, 0, 0)
    phdrs = b"".join(struct.pack("<IIQQQQQQ", 1, 5, 0, 0, 0, 120, 120, a) for a in aligns)
    full = bytes(ehdr) + phdrs
    return full[:truncate_bytes] if truncate_bytes is not None else full


# =============================================================================
# F1 — Boundary, Corner & Negative Governance / Hygiene Cases (6 Tests)
# =============================================================================


@e2e_test(
    "T2-F01-01",
    tier=2,
    features=["F1"],
    title="Spec §17 compliance tables forbid ambiguous status tokens and empty evidence columns",
)
def test_t2_f01_01_spec_compliance_vocabulary_boundary(ctx: E2eTestContext) -> None:
    allowed = {"passed", "blocked", "not-applicable", "pending"}
    for spec_name in ("gpui-telegram-android-production-parity.md", "gpui-telegram-android-mika-fusion.md"):
        text = (ctx.repo_root / "docs" / "specs" / spec_name).read_text(encoding="utf-8")
        sec17 = text.split("## 17. Spec compliance record", 1)[1]
        rows = [
            line
            for line in sec17.splitlines()
            if line.strip().startswith("|") and not re.match(r"^\|\s*[-:]+\s*\|", line.strip())
        ]
        # Skip header row
        data_rows = [r for r in rows if "Requirement / AC" not in r]
        assert len(data_rows) >= 20, f"{spec_name} §17 has too few compliance rows ({len(data_rows)})"
        for row in data_rows:
            cols = [c.strip() for c in row.strip().strip("|").split("|")]
            assert len(cols) >= 3, f"Malformed table row in {spec_name}: {row}"
            req_id, status, evidence = cols[0], cols[1], cols[2]
            assert status in allowed, f"{spec_name} {req_id} uses forbidden status {status!r} (allowed: {sorted(allowed)})"
            if status in {"passed", "blocked", "not-applicable"}:
                assert len(evidence) >= 12, f"{spec_name} {req_id} ({status}) lacks substantive evidence: {evidence!r}"


@e2e_test(
    "T2-F01-02",
    tier=2,
    features=["F1"],
    title="Specs, CI workflows, and scripts contain zero leaked live API keys or private key PEM blocks",
)
def test_t2_f01_02_no_secret_or_pem_leakage(ctx: E2eTestContext) -> None:
    secret_patterns = [
        re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----"),
        re.compile(r"\bghp_[A-Za-z0-9]{36}\b"),
        re.compile(r"\bgithub_pat_[A-Za-z0-9_]{40,}\b"),
        re.compile(r"\bAIza[0-9A-Za-z_-]{35}\b"),
        re.compile(r"\bsk-ant-api03-[A-Za-z0-9_-]{40,}\b"),
    ]
    scan_dirs = [ctx.repo_root / "docs" / "specs", ctx.repo_root / ".github" / "workflows", ctx.repo_root / "scripts"]
    hits: list[str] = []
    for d in scan_dirs:
        for p in d.rglob("*"):
            if p.is_file():
                content = p.read_text(encoding="utf-8", errors="ignore")
                for pat in secret_patterns:
                    if pat.search(content):
                        hits.append(f"{p.relative_to(ctx.repo_root)} matched {pat.pattern}")
    assert not hits, f"Live credential / private key pattern leaked in repository files: {hits}"


@e2e_test(
    "T2-F01-03",
    tier=2,
    features=["F1"],
    title="Build scripts and env-setup.sh never assign TMPDIR or temp outputs to /tmp",
)
def test_t2_f01_03_no_slash_tmp_assignments(ctx: E2eTestContext) -> None:
    files = [
        Path("/home/antigravity/env-setup.sh"),
        ctx.repo_root / "scripts" / "build_rust_android.sh",
        ctx.repo_root / "scripts" / "package_android_release.py",
    ]
    bad_lines: list[str] = []
    assign_tmp_re = re.compile(r"""(?:TMPDIR|TEMP|TMP|mktemp|tmp_dir)\s*[=:(]\s*['"]?/tmp(?:/|['"\s)]|$)""")
    for f in files:
        for lineno, line in enumerate(f.read_text(encoding="utf-8").splitlines(), start=1):
            stripped = line.strip()
            if stripped.startswith("#"):
                continue
            if assign_tmp_re.search(stripped):
                bad_lines.append(f"{f}:{lineno}: {stripped}")
    assert not bad_lines, f"Forbidden /tmp assignment detected: {bad_lines}"


@e2e_test(
    "T2-F01-04",
    tier=2,
    features=["F1"],
    title="Repository enforces Android-only boundary with zero Desktop/iOS/Web implementation directories",
)
def test_t2_f01_04_platform_ownership_boundary(ctx: E2eTestContext) -> None:
    forbidden_dirs = [
        ctx.repo_root / "desktop",
        ctx.repo_root / "mobile" / "ios",
        ctx.repo_root / "web",
        ctx.repo_root / "crates" / "gpui_telegram_desktop",
    ]
    existing = [str(p.relative_to(ctx.repo_root)) for p in forbidden_dirs if p.exists()]
    assert not existing, f"Cross-platform product directories leaked into Android-only repo: {existing}"
    agents = (ctx.repo_root / "AGENTS.md").read_text(encoding="utf-8")
    assert "bhrumom/fabushi" in agents and "legacy" in agents.lower()


@e2e_test(
    "T2-F01-05",
    tier=2,
    features=["F1"],
    title="git check-ignore does NOT accidentally mask durable source, spec, e2e, or Mika prebuilt paths",
)
def test_t2_f01_05_git_exclude_does_not_overignore_sources(ctx: E2eTestContext) -> None:
    durable_paths = [
        "AGENTS.md",
        "docs/specs/gpui-telegram-android-production-parity.md",
        "scripts/package_android_release.py",
        "scripts/build_rust_android.sh",
        "e2e/run_e2e.py",
        "mobile/android/mika-agent/src/main/jniLibs/arm64-v8a/libmikarun.so",
        "mobile/android/mika-agent/src/main/assets/runtime/rootfs.tar.xz",
    ]
    proc = subprocess.run(
        ["git", "check-ignore", "--stdin"],
        cwd=str(ctx.repo_root),
        input="\n".join(durable_paths) + "\n",
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    falsely_ignored = proc.stdout.strip().splitlines()
    assert not falsely_ignored, f"Durable repository paths are erroneously ignored by git: {falsely_ignored}"


@e2e_test(
    "T2-F01-06",
    tier=2,
    features=["F1"],
    title="Cargo workspace/host Cargo.toml enforces debug=0, strip='debuginfo', incremental=false on dev & release",
)
def test_t2_f01_06_cargo_profile_lean_constraints(ctx: E2eTestContext) -> None:
    ws_toml = ctx.repo_root / "mobile" / "android" / "rust" / "Cargo.toml"
    crate_toml = ctx.repo_root / "mobile" / "android" / "rust" / "mahayana_android_host" / "Cargo.toml"
    effective_toml = ws_toml if ws_toml.is_file() else crate_toml
    parsed = tomllib.loads(effective_toml.read_text(encoding="utf-8"))
    profiles = parsed.get("profile") or {}
    for prof_name in ("dev", "release"):
        prof = profiles.get(prof_name) or {}
        assert prof.get("debug") == 0, f"[profile.{prof_name}] in {effective_toml} must set debug = 0, got {prof.get('debug')!r}"
        assert prof.get("strip") == "debuginfo", (
            f"[profile.{prof_name}] in {effective_toml} must set strip = 'debuginfo', got {prof.get('strip')!r}"
        )
        assert prof.get("incremental") is False, (
            f"[profile.{prof_name}] in {effective_toml} must set incremental = false, got {prof.get('incremental')!r}"
        )


# =============================================================================
# F2 — Boundary, Corner & Negative Release / ELF / Keystore Cases (6 Tests)
# =============================================================================


@e2e_test(
    "T2-F02-01",
    tier=2,
    features=["F2"],
    title="package_android_release.py --compute-version-code rejects underflow, overflow, leading-zero & injected tags",
)
def test_t2_f02_01_reject_malformed_and_out_of_range_tags(ctx: E2eTestContext) -> None:
    script = str(ctx.repo_root / "scripts" / "package_android_release.py")
    bad_tags = [
        "v1.9.9",       # MAJOR < 2 (< 20000)
        "v2.100.0",     # MINOR > 99
        "v2.1.100",     # PATCH > 99
        "v02.1.0",      # leading zero MAJOR
        "v2.01.0",      # leading zero MINOR
        "v2.1.01",      # leading zero PATCH
        "v2.1",         # missing PATCH
        "../v2.1.0",    # path traversal
        "v2.1.0\nX=1",  # newline env injection
        "v2.1.0 ",      # trailing space
    ]
    for bad in bad_tags:
        proc = subprocess.run(
            ["python3", script, "--compute-version-code", bad],
            cwd=str(ctx.repo_root),
            env=ctx.env(),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
        assert proc.returncode != 0, f"--compute-version-code unexpectedly accepted invalid tag {bad!r}: {proc.stdout}"


@e2e_test(
    "T2-F02-02",
    tier=2,
    features=["F2"],
    title="package_android_release.py rejects versionCode downgrade, >32-bit overflow & mismatched --version",
)
def test_t2_f02_02_reject_version_downgrade_and_overflow(ctx: E2eTestContext) -> None:
    script = str(ctx.repo_root / "scripts" / "package_android_release.py")
    bad_arg_sets = [
        ["--resolve-release-metadata", "--tag=v2.1.0", "--version-code=20099"],  # downgrade < 20100
        ["--resolve-release-metadata", "--tag=v2.1.0", "--version-code=20000"],  # downgrade to v2.0.0
        ["--resolve-release-metadata", "--tag=v2.1.0", "--version-code=2147483648"],  # > INT32_MAX
        ["--resolve-release-metadata", "--tag=v2.1.0", "--version-code=9999999999999999999999"],  # > 64-bit
        ["--resolve-release-metadata", "--tag=v2.1.0", "--version-code=020100"],  # leading zero
        ["--resolve-release-metadata", "--tag=v2.1.0", "--version=2.0.0"],  # tag/version mismatch
    ]
    for args in bad_arg_sets:
        proc = subprocess.run(
            ["python3", script, *args],
            cwd=str(ctx.repo_root),
            env=ctx.env(),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
        assert proc.returncode != 0, f"Expected non-zero failure for {args}, got stdout={proc.stdout!r}"


@e2e_test(
    "T2-F02-03",
    tier=2,
    features=["F2"],
    title="resolve_signing_keystore rejects repo-interior paths and symlinks pointing inside repo_root",
)
def test_t2_f02_03_reject_repo_interior_signing_paths(ctx: E2eTestContext) -> None:
    mod = _load_packager(ctx.repo_root)
    symlink_to_repo = ctx.subdir("sym") / "repo_link"
    symlink_to_repo.symlink_to(ctx.repo_root / "release-signing")

    cases = [
        {"ANDROID_KEYSTORE_PATH": str(ctx.repo_root / "leak.p12")},
        {"FABUSHI_SIGNING_CACHE_DIR": str(ctx.repo_root / "release-signing")},
        {"FABUSHI_SIGNING_CACHE_DIR": str(symlink_to_repo)},
    ]
    for env_patch in cases:
        saved = {k: os.environ.get(k) for k in ("ANDROID_KEYSTORE_PATH", "ANDROID_KEYSTORE_BASE64", "FABUSHI_SIGNING_CACHE_DIR")}
        try:
            for k in saved:
                os.environ.pop(k, None)
            os.environ.update(env_patch)
            rejected = False
            try:
                with mod.resolve_signing_keystore(ctx.repo_root, require_release_keystore=False):
                    pass
            except RuntimeError:
                rejected = True
            assert rejected, f"resolve_signing_keystore failed to reject repo-interior config: {env_patch}"
        finally:
            for k, v in saved.items():
                if v is None:
                    os.environ.pop(k, None)
                else:
                    os.environ[k] = v

    assert not (ctx.repo_root / "release-signing").exists()
    assert not (ctx.repo_root / "leak.p12").exists()


@e2e_test(
    "T2-F02-04",
    tier=2,
    features=["F2"],
    title="resolve_signing_keystore fails closed on --require-release-keystore without secret or corrupt base64",
)
def test_t2_f02_04_require_release_keystore_and_corrupt_base64(ctx: E2eTestContext) -> None:
    mod = _load_packager(ctx.repo_root)
    saved = {k: os.environ.get(k) for k in ("ANDROID_KEYSTORE_PATH", "ANDROID_KEYSTORE_BASE64", "FABUSHI_SIGNING_CACHE_DIR")}
    try:
        for k in saved:
            os.environ.pop(k, None)

        # 1. Missing keystore when require_release_keystore=True
        missing_rejected = False
        try:
            with mod.resolve_signing_keystore(ctx.repo_root, require_release_keystore=True):
                pass
        except RuntimeError:
            missing_rejected = True
        assert missing_rejected, "Expected RuntimeError when --require-release-keystore has no keystore env"

        # 2. Corrupt base64
        os.environ["ANDROID_KEYSTORE_BASE64"] = "!!!not-valid-base64!!!"
        corrupt_rejected = False
        try:
            with mod.resolve_signing_keystore(ctx.repo_root, require_release_keystore=True):
                pass
        except RuntimeError:
            corrupt_rejected = True
        assert corrupt_rejected, "Expected RuntimeError on invalid ANDROID_KEYSTORE_BASE64"
    finally:
        for k, v in saved.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v


@e2e_test(
    "T2-F02-05",
    tier=2,
    features=["F2"],
    title="_validate_out_dir refuses filesystem root, HOME, repo_root, protected subdirectories & symlinks",
)
def test_t2_f02_05_protected_out_dir_guards(ctx: E2eTestContext) -> None:
    mod = _load_packager(ctx.repo_root)
    outputs_root = ctx.repo_root / "mobile" / "android" / "app" / "build" / "outputs" / "apk"
    sym_out = ctx.subdir("sym_out") / "link_dir"
    target_dir = ctx.subdir("sym_out") / "real_dir"
    target_dir.mkdir(parents=True, exist_ok=True)
    sym_out.symlink_to(target_dir)

    forbidden = [
        Path("/"),
        Path.home(),
        ctx.repo_root,
        ctx.repo_root / "mobile",
        ctx.repo_root / "scripts",
        ctx.repo_root / "docs",
        ctx.repo_root / ".git",
        ctx.repo_root / ".github",
        outputs_root,
        sym_out,
    ]
    for candidate in forbidden:
        rejected = False
        try:
            mod._validate_out_dir(candidate, ctx.repo_root, outputs_root)
        except RuntimeError:
            rejected = True
        assert rejected, f"_validate_out_dir failed to block dangerous output path: {candidate}"


@e2e_test(
    "T2-F02-06",
    tier=2,
    features=["F2"],
    title="check_elf_16kb_alignment rejects 4KB p_align, non-power-of-two, wrong ABI, 32-bit, MSB, ET_EXEC & truncated ELF",
)
def test_t2_f02_06_adversarial_elf64_negative_matrix(ctx: E2eTestContext) -> None:
    mod = _load_packager(ctx.repo_root)
    # Positive sanity check first
    mod.check_elf_16kb_alignment(
        _make_elf64_header(e_machine=183, pt_load_aligns=[0x4000]),
        "valid_arm64",
        expected_machine=183,
    )

    bad_vectors = [
        ("4kb_align", _make_elf64_header(pt_load_aligns=[0x1000]), 183),
        ("non_pow2_align", _make_elf64_header(pt_load_aligns=[0x6000]), 183),
        ("second_seg_4kb", _make_elf64_header(pt_load_aligns=[0x4000, 0x2000]), 183),
        ("wrong_abi_x64_in_arm64", _make_elf64_header(e_machine=62, pt_load_aligns=[0x4000]), 183),
        ("elf32", _make_elf64_header(ei_class=1), 183),
        ("big_endian", _make_elf64_header(ei_data=2), 183),
        ("et_exec", _make_elf64_header(e_type=2), 183),
        ("zero_pt_load", _make_elf64_header(pt_load_aligns=[]), 183),
        ("truncated_phdr", _make_elf64_header(pt_load_aligns=[0x4000], truncate_bytes=80), 183),
    ]
    for label, elf_bytes, exp_machine in bad_vectors:
        rejected = False
        try:
            mod.check_elf_16kb_alignment(elf_bytes, label, expected_machine=exp_machine)
        except RuntimeError:
            rejected = True
        assert rejected, f"check_elf_16kb_alignment failed to reject {label}"
