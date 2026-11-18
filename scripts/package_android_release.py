#!/usr/bin/env python3
"""Package, sign, verify, and emit update metadata for Fabushi Android releases."""

from __future__ import annotations

import argparse
import base64
import binascii
import contextlib
import hashlib
import json
import os
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import zipfile
from collections.abc import Iterator
from pathlib import Path

_SEMVER_CORE_RE = re.compile(r"^([0-9]+)\.([0-9]+)\.([0-9]+)\Z")
_SEMVER_META_RE = re.compile(r"^[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*\Z")
_SAFE_TAG_OR_VERSION_RE = re.compile(r"^[0-9A-Za-z.+-]+\Z")
_REPOSITORY_RE = re.compile(r"^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+\Z")
_POSITIVE_ASCII_INT_RE = re.compile(r"^[1-9][0-9]*\Z")
_MIN_VERSION_CODE = 20000
_MAX_VERSION_CODE = 2_147_483_647


def validate_tag_or_version_chars(value: str, label: str = "Tag or version") -> str:
    """Enforce strict printable ASCII [0-9A-Za-z.+-] without whitespace, newlines, '/', '\\', or '..'."""
    if not isinstance(value, str) or not value:
        raise ValueError(f"{label} must not be empty.")
    if (
        not _SAFE_TAG_OR_VERSION_RE.match(value)
        or ".." in value
        or value.startswith(".")
        or value.endswith(".")
    ):
        raise ValueError(
            f"{label} {value!r} contains invalid characters; expected strict ASCII [0-9A-Za-z.+-] "
            "without whitespace, newlines, '/', '\\', or '..'."
        )
    return value


def validate_repository_slug(repository: str) -> str:
    """Enforce GitHub OWNER/REPO format forbidding newlines, control chars, '\\', extra '/', and '.'/'..' segments."""
    if not isinstance(repository, str) or not _REPOSITORY_RE.match(repository):
        raise ValueError(
            f"Invalid --repository {repository!r}: expected strict ASCII 'OWNER/REPO' "
            "without newlines, whitespace, or '\\'."
        )
    owner, repo = repository.split("/", 1)
    if owner in {".", ".."} or repo in {".", ".."}:
        raise ValueError(f"Invalid --repository {repository!r}: '.' and '..' segments are not allowed.")
    return repository


def normalize_version_string(tag_or_version: str) -> str:
    raw = validate_tag_or_version_chars(tag_or_version, "Tag or version")
    if raw.startswith("android-"):
        raw = raw[len("android-") :]
    if raw.startswith("v"):
        raw = raw[1:]
    if not raw:
        raise ValueError(f"Tag '{tag_or_version}' contains no version after stripping prefix.")
    return raw


def compute_version_code(tag_or_version: str) -> int:
    """Derive monotonic Android versionCode = MAJOR * 10000 + MINOR * 100 + PATCH.

    Examples:
        v2.0.0 -> 20000
        v2.1.0 -> 20100
        android-v2.1.3 -> 20103
    """
    normalized = normalize_version_string(tag_or_version)

    if "+" in normalized:
        normalized, build_meta = normalized.split("+", 1)
        if not build_meta or not _SEMVER_META_RE.match(build_meta):
            raise ValueError(f"Invalid build metadata in '{tag_or_version}'.")

    if "-" in normalized:
        core, prerelease = normalized.split("-", 1)
        if not prerelease or not _SEMVER_META_RE.match(prerelease):
            raise ValueError(f"Invalid pre-release suffix in '{tag_or_version}'.")
    else:
        core = normalized

    match = _SEMVER_CORE_RE.match(core)
    if not match:
        raise ValueError(
            f"Invalid semantic version '{tag_or_version}': expected [android-][v]MAJOR.MINOR.PATCH."
        )

    major_s, minor_s, patch_s = match.groups()
    for label, part in (("MAJOR", major_s), ("MINOR", minor_s), ("PATCH", patch_s)):
        if len(part) > 1 and part.startswith("0"):
            raise ValueError(f"Leading zeros are not allowed in {label} component of '{tag_or_version}'.")
        if len(part) > 10:
            raise ValueError(f"{label} component in '{tag_or_version}' exceeds Android versionCode bounds.")

    major = int(major_s, 10)
    minor = int(minor_s, 10)
    patch = int(patch_s, 10)

    if major < 2:
        raise ValueError(
            f"MAJOR version in '{tag_or_version}' must be >= 2 (got {major}) to stay >= {_MIN_VERSION_CODE}."
        )
    if not (0 <= minor <= 99):
        raise ValueError(f"MINOR version in '{tag_or_version}' must be in 0..99 (got {minor}).")
    if not (0 <= patch <= 99):
        raise ValueError(f"PATCH version in '{tag_or_version}' must be in 0..99 (got {patch}).")

    code = major * 10000 + minor * 100 + patch
    if code < _MIN_VERSION_CODE or code > _MAX_VERSION_CODE:
        raise ValueError(f"Derived versionCode {code} for '{tag_or_version}' is out of Android bounds.")
    return code


def parse_version_code_override(
    raw_value: str | int,
    *,
    min_semver_code: int = _MIN_VERSION_CODE,
) -> int:
    """Validate manual versionCode override using arbitrary-precision Python int (zero 64-bit overflow)."""
    if isinstance(raw_value, bool):
        raise ValueError("version_code must be a base-10 ASCII integer, not bool.")
    if isinstance(raw_value, int):
        vcode = raw_value
    elif isinstance(raw_value, str):
        if not _POSITIVE_ASCII_INT_RE.match(raw_value):
            raise ValueError(
                f"version_code {raw_value!r} must match strict ASCII [1-9][0-9]* "
                "(no leading zeros, signs, whitespace, or non-ASCII digits)."
            )
        if len(raw_value) > 10:
            raise ValueError(
                f"version_code '{raw_value[:32]}' exceeds maximum 32-bit signed integer {_MAX_VERSION_CODE}."
            )
        vcode = int(raw_value, 10)
    else:
        raise ValueError(f"Unsupported version_code type: {type(raw_value).__name__}.")

    if vcode < _MIN_VERSION_CODE or vcode > _MAX_VERSION_CODE:
        raise ValueError(
            f"version_code {vcode} must be in {_MIN_VERSION_CODE}..{_MAX_VERSION_CODE}."
        )
    if vcode < min_semver_code:
        raise ValueError(
            f"version_code override {vcode} cannot downgrade below tag's derived semver versionCode {min_semver_code}."
        )
    return vcode


def resolve_release_metadata(
    tag: str,
    *,
    version_code_override: str | int | None = None,
    version_override: str | None = None,
) -> tuple[str, str, int]:
    """Validate tag, optional normalized version, and optional monotonic versionCode override."""
    resolved_tag = validate_tag_or_version_chars(tag, "Release tag")
    normalized_version = normalize_version_string(resolved_tag)
    derived_semver_code = compute_version_code(resolved_tag)

    if version_override is not None:
        validate_tag_or_version_chars(version_override, "Release version")
        if version_override != normalized_version:
            raise ValueError(
                f"--version '{version_override}' must match tag's normalized version '{normalized_version}'."
            )
        resolved_version = version_override
    else:
        resolved_version = normalized_version

    if version_code_override is not None:
        resolved_version_code = parse_version_code_override(
            version_code_override,
            min_semver_code=derived_semver_code,
        )
    else:
        resolved_version_code = derived_semver_code

    return resolved_tag, resolved_version, resolved_version_code


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def _read_local_properties_sdk(repo_root: Path) -> str | None:
    local_props = repo_root / "mobile" / "android" / "local.properties"
    if not local_props.is_file():
        return None
    sdk_dir: str | None = None
    for line in local_props.read_text(encoding="utf-8", errors="replace").splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if stripped.startswith("sdk.dir"):
            _, sep, rhs = stripped.partition("=")
            if sep:
                sdk_dir = rhs.strip().replace(r"\:", ":").replace("\\\\", "\\")
    return sdk_dir


def find_build_tool(name: str, repo_root: Path | None = None) -> str:
    sdk = (
        os.environ.get("ANDROID_HOME")
        or os.environ.get("ANDROID_SDK_ROOT")
        or (_read_local_properties_sdk(repo_root) if repo_root else None)
    )
    if sdk:
        build_tools_root = Path(sdk) / "build-tools"
        if build_tools_root.is_dir():
            for version_dir in sorted(build_tools_root.iterdir(), reverse=True):
                candidate = version_dir / name
                if candidate.is_file() and os.access(candidate, os.X_OK):
                    return str(candidate)
    found = shutil.which(name)
    if not found:
        raise RuntimeError(f"Required Android build-tool '{name}' not found in ANDROID_HOME or PATH.")
    return found


_ELF_MAGIC = b"\x7fELF"
_ELFCLASS64 = 2
_ELFDATA2LSB = 1
_ELF_ET_DYN = 3
_ELF_EM_X86_64 = 62
_ELF_EM_AARCH64 = 183
_ELF64_EHDR_SIZE = 64
_ELF64_PHDR_MIN_SIZE = 56
_ELF_PT_LOAD = 1
_MIN_PAGE_ALIGN_16KB = 16384
_ABI_EXPECTED_MACHINE: dict[str, int] = {
    "arm64-v8a": _ELF_EM_AARCH64,
    "x86_64": _ELF_EM_X86_64,
}


def _infer_expected_machine_from_label(label: str) -> int | None:
    parts = [seg for seg in re.split(r"[:/\\]+", label) if seg]
    if len(parts) >= 2:
        return _ABI_EXPECTED_MACHINE.get(parts[-2])
    return None


def check_elf_16kb_alignment(
    elf_bytes: bytes,
    label: str,
    *,
    expected_machine: int | None = None,
) -> None:
    if not isinstance(elf_bytes, (bytes, bytearray, memoryview)):
        raise RuntimeError(f"{label} ELF payload must be bytes-like.")
    buf = memoryview(elf_bytes).cast("B")
    total_len = len(buf)
    if total_len < _ELF64_EHDR_SIZE or buf[:4].tobytes() != _ELF_MAGIC:
        raise RuntimeError(
            f"{label} is not a valid 64-bit ELF binary (size={total_len} bytes)."
        )
    ei_class = buf[4]
    if ei_class != _ELFCLASS64:
        raise RuntimeError(
            f"{label} must be 64-bit ELFCLASS64 (ei_class=2), got ei_class={ei_class}."
        )
    ei_data = buf[5]
    if ei_data != _ELFDATA2LSB:
        raise RuntimeError(
            f"{label} must be little-endian ELFDATA2LSB (ei_data=1), got ei_data={ei_data}."
        )

    e_type, e_machine = struct.unpack_from("<HH", buf, 16)
    if e_type != _ELF_ET_DYN:
        raise RuntimeError(
            f"{label} must be an ET_DYN shared object (e_type=3), got e_type={e_type}."
        )

    resolved_expected_machine = (
        expected_machine
        if expected_machine is not None
        else _infer_expected_machine_from_label(label)
    )
    if resolved_expected_machine is not None:
        if e_machine != resolved_expected_machine:
            raise RuntimeError(
                f"{label} has e_machine={e_machine}, expected {resolved_expected_machine}."
            )
    elif e_machine not in (_ELF_EM_AARCH64, _ELF_EM_X86_64):
        raise RuntimeError(
            f"{label} has unsupported 64-bit Android e_machine={e_machine} "
            f"(expected {_ELF_EM_AARCH64} EM_AARCH64 or {_ELF_EM_X86_64} EM_X86_64)."
        )

    e_phoff = struct.unpack_from("<Q", buf, 32)[0]
    e_phentsize, e_phnum = struct.unpack_from("<HH", buf, 54)
    if e_phentsize < _ELF64_PHDR_MIN_SIZE:
        raise RuntimeError(
            f"{label} has invalid e_phentsize={e_phentsize} (must be >= {_ELF64_PHDR_MIN_SIZE})."
        )
    if e_phnum <= 0:
        raise RuntimeError(f"{label} has invalid e_phnum={e_phnum} (must be > 0).")
    if e_phoff < _ELF64_EHDR_SIZE:
        raise RuntimeError(
            f"{label} has invalid e_phoff={e_phoff} (must be >= {_ELF64_EHDR_SIZE})."
        )
    phdr_table_end = e_phoff + e_phnum * e_phentsize
    if phdr_table_end > total_len:
        raise RuntimeError(
            f"{label} program header table [0x{e_phoff:x}..0x{phdr_table_end:x}] "
            f"exceeds ELF size ({total_len} bytes)."
        )

    load_segments = 0
    for idx in range(e_phnum):
        off = e_phoff + idx * e_phentsize
        p_type = struct.unpack_from("<I", buf, off)[0]
        if p_type == _ELF_PT_LOAD:
            load_segments += 1
            p_align = struct.unpack_from("<Q", buf, off + 48)[0]
            if p_align < _MIN_PAGE_ALIGN_16KB or (p_align & (p_align - 1)) != 0:
                raise RuntimeError(
                    f"{label} PT_LOAD segment #{idx} has invalid p_align=0x{p_align:x} "
                    f"(must be a power of two >= 0x4000 / 16KB)."
                )
    if load_segments == 0:
        raise RuntimeError(f"{label} contains zero PT_LOAD segments.")


def verify_apk_contents(apk_path: Path) -> dict[str, object]:
    with zipfile.ZipFile(apk_path, "r") as zf:
        names = set(zf.namelist())
        required_entries = [
            "AndroidManifest.xml",
            "classes.dex",
            "lib/arm64-v8a/libmahayana_app_host.so",
            "lib/arm64-v8a/libmikarun.so",
            "lib/arm64-v8a/libproot.so",
            "lib/arm64-v8a/libproot_loader.so",
            "assets/runtime/rootfs.tar.xz",
        ]
        missing = [entry for entry in required_entries if entry not in names]
        if missing:
            raise RuntimeError(f"{apk_path.name} is missing required entries: {missing}")

        for abi_entry, expected_machine in (
            ("lib/arm64-v8a/libmahayana_app_host.so", _ELF_EM_AARCH64),
            ("lib/x86_64/libmahayana_app_host.so", _ELF_EM_X86_64),
        ):
            if abi_entry in names:
                check_elf_16kb_alignment(
                    zf.read(abi_entry),
                    f"{apk_path.name}:{abi_entry}",
                    expected_machine=expected_machine,
                )

        rootfs_info = zf.getinfo("assets/runtime/rootfs.tar.xz")
        if rootfs_info.compress_type != zipfile.ZIP_STORED:
            raise RuntimeError(
                f"{apk_path.name}: assets/runtime/rootfs.tar.xz must be ZIP_STORED (uncompressed), got {rootfs_info.compress_type}"
            )
        return {
            "entries": len(names),
            "has_x86_64_host": "lib/x86_64/libmahayana_app_host.so" in names,
            "rootfs_bytes": rootfs_info.file_size,
        }


def _assert_outside_repo_root(target: Path, repo_root: Path, label: str) -> None:
    expanded_target = target.expanduser()
    lexical_target = Path(os.path.abspath(str(expanded_target)))
    expanded_repo = repo_root.expanduser()
    lexical_repo = Path(os.path.abspath(str(expanded_repo)))
    resolved_repo = expanded_repo.resolve()
    repo_bases = (lexical_repo, resolved_repo)

    for node in (lexical_target, *lexical_target.parents):
        probes = [node, node.resolve()]
        if node.name:
            probes.append(node.parent.resolve() / node.name)
        for candidate in probes:
            for repo_base in repo_bases:
                if candidate == repo_base or candidate.is_relative_to(repo_base):
                    raise RuntimeError(
                        f"Refusing to place {label} ('{lexical_target}') inside repository root '{repo_base}'."
                    )


def _has_explicit_signing_keystore_env() -> bool:
    return bool(
        (os.environ.get("ANDROID_KEYSTORE_PATH") or "").strip()
        or (os.environ.get("ANDROID_KEYSTORE_BASE64") or "").strip()
    )


def _try_chmod(path: Path, mode: int) -> None:
    try:
        os.chmod(path, mode)
    except PermissionError:
        # Shared virtiofs scratch mounts may disallow chmod while ext4/xfs (~/.config) enforces it.
        pass


@contextlib.contextmanager
def resolve_signing_keystore(
    repo_root: Path,
    *,
    require_release_keystore: bool,
) -> Iterator[tuple[Path, str, str, str]]:
    ks_path_env = (os.environ.get("ANDROID_KEYSTORE_PATH") or "").strip()
    ks_b64_env = (os.environ.get("ANDROID_KEYSTORE_BASE64") or "").strip()
    ks_pass = (os.environ.get("ANDROID_KEYSTORE_PASSWORD") or "").strip() or "fabushi-release-pass"
    key_alias = (os.environ.get("ANDROID_KEY_ALIAS") or "").strip() or "fabushi-release"
    key_pass = (os.environ.get("ANDROID_KEY_PASSWORD") or "").strip() or ks_pass

    if ks_path_env:
        raw_ks_file = Path(ks_path_env).expanduser()
        _assert_outside_repo_root(raw_ks_file, repo_root, "ANDROID_KEYSTORE_PATH")
        ks_file = raw_ks_file.resolve()
        if not ks_file.is_file():
            raise RuntimeError(f"ANDROID_KEYSTORE_PATH='{ks_path_env}' does not exist or is not a file.")
        yield ks_file, ks_pass, key_alias, key_pass
        return

    if ks_b64_env:
        _assert_outside_repo_root(Path(tempfile.gettempdir()), repo_root, "TMPDIR")
        tmp_dir = Path(tempfile.mkdtemp(prefix="fabushi-ks-"))
        _try_chmod(tmp_dir, 0o700)
        try:
            try:
                compact_b64 = "".join(ks_b64_env.split())
                decoded = base64.b64decode(compact_b64, validate=True)
            except (binascii.Error, ValueError) as exc:
                raise RuntimeError("ANDROID_KEYSTORE_BASE64 is not valid base64 data.") from exc
            if not decoded:
                raise RuntimeError("ANDROID_KEYSTORE_BASE64 decoded to 0 bytes.")
            ks_file = tmp_dir / "release.keystore"
            ks_file.write_bytes(decoded)
            _try_chmod(ks_file, 0o600)
            _assert_outside_repo_root(ks_file, repo_root, "temporary decoded keystore")
            yield ks_file, ks_pass, key_alias, key_pass
        finally:
            shutil.rmtree(tmp_dir, ignore_errors=True)
        return

    if require_release_keystore:
        raise RuntimeError(
            "Release keystore is required (--require-release-keystore), "
            "but neither ANDROID_KEYSTORE_PATH nor ANDROID_KEYSTORE_BASE64 was provided."
        )

    cache_override = (os.environ.get("FABUSHI_SIGNING_CACHE_DIR") or "").strip()
    if cache_override:
        raw_cache_dir = Path(cache_override).expanduser()
    else:
        xdg_config = (os.environ.get("XDG_CONFIG_HOME") or "").strip()
        base_config = Path(xdg_config).expanduser() if xdg_config else (Path.home() / ".config")
        raw_cache_dir = base_config / "fabushi-android"

    _assert_outside_repo_root(raw_cache_dir, repo_root, "persistent local signing cache")
    cache_dir = raw_cache_dir.resolve()
    cache_dir.mkdir(parents=True, exist_ok=True)
    _try_chmod(cache_dir, 0o700)

    ks_file = cache_dir / "release-signing.p12"
    _assert_outside_repo_root(ks_file, repo_root, "persistent local signing keystore")
    if not ks_file.is_file():
        subprocess.run(
            [
                "keytool",
                "-genkeypair",
                "-noprompt",
                "-keystore",
                str(ks_file),
                "-storetype",
                "PKCS12",
                "-storepass",
                ks_pass,
                "-keypass",
                key_pass,
                "-alias",
                key_alias,
                "-keyalg",
                "RSA",
                "-keysize",
                "2048",
                "-validity",
                "10000",
                "-dname",
                "CN=Fabushi Android Release, OU=Mobile, O=Fabushi, L=Global, S=Global, C=US",
            ],
            check=True,
        )
    _try_chmod(ks_file, 0o600)
    yield ks_file, ks_pass, key_alias, key_pass


def _run_zipalign_16kb(zipalign: str, input_apk: Path, aligned_apk: Path) -> None:
    proc = subprocess.run(
        [zipalign, "-P", "16", "-f", "4", str(input_apk), str(aligned_apk)],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if proc.returncode == 0:
        return
    # Fallback for older zipalign builds that use -p instead of -P 16
    subprocess.run(
        [zipalign, "-p", "-f", "4", str(input_apk), str(aligned_apk)],
        check=True,
    )


def ensure_signed_apk(
    input_apk: Path,
    output_apk: Path,
    repo_root: Path,
    *,
    require_release_keystore: bool = False,
    variant: str | None = None,
    resolved_keystore: tuple[Path, str, str, str] | None = None,
) -> None:
    _assert_outside_repo_root(Path(tempfile.gettempdir()), repo_root, "TMPDIR")

    with contextlib.ExitStack() as stack:
        if resolved_keystore is None:
            ks_file, ks_pass, key_alias, key_pass = stack.enter_context(
                resolve_signing_keystore(
                    repo_root,
                    require_release_keystore=require_release_keystore,
                )
            )
        else:
            ks_file, ks_pass, key_alias, key_pass = resolved_keystore
            _assert_outside_repo_root(ks_file, repo_root, "signing keystore")

        apksigner = find_build_tool("apksigner", repo_root)
        zipalign = find_build_tool("zipalign", repo_root)

        must_sign_with_resolved_keystore = (
            variant != "debug"
            or require_release_keystore
            or _has_explicit_signing_keystore_env()
        )

        if not must_sign_with_resolved_keystore:
            verify_initial = subprocess.run(
                [apksigner, "verify", "--verbose", str(input_apk)],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            if verify_initial.returncode == 0:
                if input_apk.resolve() != output_apk.resolve():
                    shutil.copyfile(input_apk, output_apk)
                subprocess.run([apksigner, "verify", "--verbose", str(output_apk)], check=True)
                return

        with tempfile.TemporaryDirectory(prefix="fabushi-align-") as tmp:
            aligned_apk = Path(tmp) / "aligned.apk"
            _run_zipalign_16kb(zipalign, input_apk, aligned_apk)
            try:
                subprocess.run(
                    [
                        apksigner,
                        "sign",
                        "--ks",
                        str(ks_file),
                        "--ks-pass",
                        f"pass:{ks_pass}",
                        "--ks-key-alias",
                        key_alias,
                        "--key-pass",
                        f"pass:{key_pass}",
                        "--v1-signing-enabled",
                        "true",
                        "--v2-signing-enabled",
                        "true",
                        "--v3-signing-enabled",
                        "true",
                        "--v4-signing-enabled",
                        "false",
                        "--out",
                        str(output_apk),
                        str(aligned_apk),
                    ],
                    check=True,
                )
            except Exception:
                output_apk.unlink(missing_ok=True)
                raise

    subprocess.run([apksigner, "verify", "--verbose", str(output_apk)], check=True)


def _validate_out_dir(raw_out_dir: Path, repo_root: Path, outputs_root: Path) -> Path:
    expanded = raw_out_dir.expanduser()
    if expanded.is_symlink():
        raise RuntimeError(f"Refusing to use symlink '{expanded}' as --out-dir.")
    lexical_out = Path(os.path.abspath(str(expanded)))
    resolved_out = expanded.resolve()
    resolved_repo = repo_root.expanduser().resolve()
    resolved_home = Path.home().resolve()
    protected_repo_dirs = [
        (resolved_repo / name).resolve()
        for name in (".git", ".github", "docs", "scripts", "mobile")
    ]
    resolved_outputs = outputs_root.resolve()

    for candidate in (lexical_out, resolved_out):
        if candidate == Path("/") or candidate == resolved_home:
            raise RuntimeError(
                f"Refusing to use filesystem root or home directory '{candidate}' as --out-dir."
            )
        if candidate == resolved_repo or resolved_repo.is_relative_to(candidate):
            raise RuntimeError(
                f"Refusing to use '{candidate}' as --out-dir because it equals or contains repo_root '{resolved_repo}'."
            )
        for protected in (*protected_repo_dirs, resolved_outputs):
            if (
                candidate == protected
                or candidate.is_relative_to(protected)
                or protected.is_relative_to(candidate)
            ):
                raise RuntimeError(
                    f"Refusing to use '{candidate}' as --out-dir because it overlaps protected path '{protected}'."
                )

    if resolved_out.exists() and not resolved_out.is_dir():
        raise RuntimeError(f"Refusing to use non-directory '{resolved_out}' as --out-dir.")
    return resolved_out


def build_arg_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--compute-version-code",
        metavar="TAG_OR_VERSION",
        default=None,
        help="Compute monotonic Android versionCode from semver tag (e.g. v2.1.0 -> 20100) and exit.",
    )
    parser.add_argument(
        "--resolve-release-metadata",
        action="store_true",
        help=(
            "Validate --tag (and optional --version / --version-code), "
            "print RELEASE_TAG/RELEASE_VER/RELEASE_VCODE, optionally write to --github-env, and exit."
        ),
    )
    parser.add_argument(
        "--github-env",
        type=str,
        default=None,
        help="Optional path to $GITHUB_ENV file to append validated RELEASE_TAG, RELEASE_VER, RELEASE_VCODE.",
    )
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--repository", default="fabu-android/fabushi-android")
    parser.add_argument("--tag", default=None)
    parser.add_argument("--version", default=None)
    parser.add_argument("--version-code", type=str, default=None)
    parser.add_argument("--out-dir", type=Path, default=None)
    parser.add_argument(
        "--require-release-keystore",
        action="store_true",
        help="Fail closed if neither ANDROID_KEYSTORE_PATH nor ANDROID_KEYSTORE_BASE64 is configured.",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_arg_parser()
    args = parser.parse_args(argv)

    if args.compute_version_code is not None:
        try:
            print(compute_version_code(args.compute_version_code))
            return 0
        except ValueError as exc:
            print(f"ERROR: {exc}", file=sys.stderr)
            return 1

    if args.resolve_release_metadata:
        try:
            resolved_tag, resolved_version, resolved_version_code = resolve_release_metadata(
                args.tag if args.tag is not None else "",
                version_code_override=args.version_code,
                version_override=args.version,
            )
            env_lines = [
                f"RELEASE_TAG={resolved_tag}",
                f"RELEASE_VER={resolved_version}",
                f"RELEASE_VCODE={resolved_version_code}",
            ]
            for env_line in env_lines:
                if any(ch in env_line for ch in ("\r", "\n", "\x00")):
                    raise ValueError("Refusing to emit release metadata containing control characters.")
            payload = "\n".join(env_lines) + "\n"
            if args.github_env is not None:
                if not args.github_env or any(ch in args.github_env for ch in ("\r", "\n", "\x00")):
                    raise ValueError("--github-env must be a valid non-empty file path.")
                with Path(args.github_env).open("a", encoding="utf-8") as handle:
                    handle.write(payload)
            sys.stdout.write(payload)
            return 0
        except (ValueError, OSError) as exc:
            print(f"ERROR: {exc}", file=sys.stderr)
            return 1

    if args.tag is None:
        parser.error("the following arguments are required: --tag")

    try:
        resolved_repository = validate_repository_slug(args.repository)
        resolved_tag, resolved_version, resolved_version_code = resolve_release_metadata(
            args.tag,
            version_code_override=args.version_code,
            version_override=args.version,
        )

        raw_repo_root: Path = args.repo_root.expanduser()
        repo_root: Path = raw_repo_root.resolve()
        outputs_root = repo_root / "mobile" / "android" / "app" / "build" / "outputs" / "apk"
        raw_out_dir: Path = args.out_dir if args.out_dir is not None else (repo_root / "dist")
        out_dir: Path = _validate_out_dir(raw_out_dir, raw_repo_root, outputs_root)

        candidates = [
            (
                "githubRelease",
                outputs_root / "githubRelease",
                f"Fabushi-Telegram-Mika-{resolved_tag}-githubRelease.apk",
                True,
            ),
            (
                "release",
                outputs_root / "release",
                f"Fabushi-Telegram-Mika-{resolved_tag}-release.apk",
                False,
            ),
            (
                "debug",
                outputs_root / "debug",
                f"Fabushi-Telegram-Mika-{resolved_tag}-debug.apk",
                False,
            ),
        ]

        packaged_apks: list[tuple[Path, str, bool]] = []
        with resolve_signing_keystore(
            raw_repo_root,
            require_release_keystore=args.require_release_keystore,
        ) as resolved_keystore:
            discovered: list[tuple[str, Path, str, bool]] = []
            for variant_name, variant_dir, target_name, is_primary in candidates:
                if not variant_dir.is_dir():
                    continue
                apks = sorted(variant_dir.glob("*.apk"))
                if not apks:
                    continue
                discovered.append((variant_name, apks[0], target_name, is_primary))

            if not discovered:
                raise RuntimeError(f"No built APKs found under {outputs_root}")

            if out_dir.exists():
                shutil.rmtree(out_dir)
            out_dir.mkdir(parents=True, exist_ok=True)

            for variant_name, src_apk, target_name, is_primary in discovered:
                dst_apk = out_dir / target_name
                ensure_signed_apk(
                    src_apk,
                    dst_apk,
                    raw_repo_root,
                    require_release_keystore=args.require_release_keystore,
                    variant=variant_name,
                    resolved_keystore=resolved_keystore,
                )
                stats = verify_apk_contents(dst_apk)
                digest = sha256_file(dst_apk)
                packaged_apks.append((dst_apk, digest, is_primary))
                print(
                    f"[OK] Packaged {dst_apk.name} ({dst_apk.stat().st_size / (1024 * 1024):.2f} MiB, sha256={digest[:16]}..., stats={stats})"
                )

        primary_apk, primary_sha256, _ = next(
            (item for item in packaged_apks if item[2]),
            packaged_apks[0],
        )

        update_manifest = {
            "schemaVersion": 1,
            "repository": resolved_repository,
            "tag": resolved_tag,
            "version": resolved_version,
            "versionCode": resolved_version_code,
            "apk": primary_apk.name,
            "sha256": primary_sha256,
            "size": primary_apk.stat().st_size,
        }
        manifest_path = out_dir / "fabushi-android-update.json"
        manifest_path.write_text(json.dumps(update_manifest, indent=2) + "\n", encoding="utf-8")

        sha_lines = [f"{digest}  {apk.name}" for apk, digest, _ in packaged_apks]
        sha_lines.append(f"{sha256_file(manifest_path)}  {manifest_path.name}")
        (out_dir / "SHA256SUMS").write_text("\n".join(sha_lines) + "\n", encoding="utf-8")

        print(f"[DONE] Release bundle ready at {out_dir}")
        return 0
    except (ValueError, RuntimeError, OSError, subprocess.CalledProcessError) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
