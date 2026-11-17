#!/usr/bin/env python3
"""Package, sign, verify, and emit update metadata for Fabushi Android releases."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import struct
import subprocess
import tempfile
import zipfile
from pathlib import Path


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def find_build_tool(name: str) -> str:
    sdk = os.environ.get("ANDROID_HOME") or os.environ.get("ANDROID_SDK_ROOT")
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


def check_elf_16kb_alignment(elf_bytes: bytes, label: str) -> None:
    if len(elf_bytes) < 64 or elf_bytes[:4] != b"\x7fELF":
        raise RuntimeError(f"{label} is not a valid 64-bit ELF binary.")
    ei_class = elf_bytes[4]
    if ei_class != 2:
        return
    e_phoff = struct.unpack_from("<Q", elf_bytes, 32)[0]
    e_phentsize, e_phnum = struct.unpack_from("<HH", elf_bytes, 54)
    load_segments = 0
    for idx in range(e_phnum):
        off = e_phoff + idx * e_phentsize
        p_type = struct.unpack_from("<I", elf_bytes, off)[0]
        if p_type == 1:  # PT_LOAD
            load_segments += 1
            p_align = struct.unpack_from("<Q", elf_bytes, off + 48)[0]
            if p_align < 16384:
                raise RuntimeError(
                    f"{label} PT_LOAD segment #{idx} has p_align={hex(p_align)} (< 0x4000 / 16KB)."
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

        for abi_entry in (
            "lib/arm64-v8a/libmahayana_app_host.so",
            "lib/x86_64/libmahayana_app_host.so",
        ):
            if abi_entry in names:
                check_elf_16kb_alignment(zf.read(abi_entry), f"{apk_path.name}:{abi_entry}")

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


def ensure_signed_apk(input_apk: Path, output_apk: Path, keystore_dir: Path) -> None:
    apksigner = find_build_tool("apksigner")
    zipalign = find_build_tool("zipalign")

    # Check if already signed (e.g., when ANDROID_KEYSTORE_PATH was supplied to Gradle)
    verify_initial = subprocess.run(
        [apksigner, "verify", "--verbose", str(input_apk)],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if verify_initial.returncode == 0:
        shutil.copy2(input_apk, output_apk)
        return

    ks_path_env = os.environ.get("ANDROID_KEYSTORE_PATH")
    ks_pass = os.environ.get("ANDROID_KEYSTORE_PASSWORD", "fabushi-release-pass")
    key_alias = os.environ.get("ANDROID_KEY_ALIAS", "fabushi-release")
    key_pass = os.environ.get("ANDROID_KEY_PASSWORD", ks_pass)

    if ks_path_env and Path(ks_path_env).is_file():
        ks_file = Path(ks_path_env)
    else:
        keystore_dir.mkdir(parents=True, exist_ok=True)
        ks_file = keystore_dir / "ephemeral-release.p12"
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

    with tempfile.TemporaryDirectory() as tmp:
        aligned_apk = Path(tmp) / "aligned.apk"
        subprocess.run([zipalign, "-p", "-f", "4", str(input_apk), str(aligned_apk)], check=True)
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

    subprocess.run([apksigner, "verify", "--verbose", str(output_apk)], check=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--repository", default="fabu-android/fabushi-android")
    parser.add_argument("--tag", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--version-code", type=int, required=True)
    parser.add_argument("--out-dir", type=Path, default=None)
    args = parser.parse_args()

    repo_root: Path = args.repo_root.resolve()
    out_dir: Path = (args.out_dir or (repo_root / "dist")).resolve()
    if out_dir.exists():
        shutil.rmtree(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    outputs_root = repo_root / "mobile" / "android" / "app" / "build" / "outputs" / "apk"
    keystore_dir = repo_root / "release-signing"

    candidates = [
        (
            outputs_root / "githubRelease",
            f"Fabushi-Telegram-Mika-{args.tag}-githubRelease.apk",
            True,
        ),
        (
            outputs_root / "release",
            f"Fabushi-Telegram-Mika-{args.tag}-release.apk",
            False,
        ),
        (
            outputs_root / "debug",
            f"Fabushi-Telegram-Mika-{args.tag}-debug.apk",
            False,
        ),
    ]

    packaged_apks: list[tuple[Path, str, bool]] = []
    for variant_dir, target_name, is_primary in candidates:
        if not variant_dir.is_dir():
            continue
        apks = sorted(variant_dir.glob("*.apk"))
        if not apks:
            continue
        src_apk = apks[0]
        dst_apk = out_dir / target_name
        ensure_signed_apk(src_apk, dst_apk, keystore_dir)
        stats = verify_apk_contents(dst_apk)
        digest = sha256_file(dst_apk)
        packaged_apks.append((dst_apk, digest, is_primary))
        print(
            f"[OK] Packaged {dst_apk.name} ({dst_apk.stat().st_size / (1024 * 1024):.2f} MiB, sha256={digest[:16]}..., stats={stats})"
        )

    if not packaged_apks:
        raise RuntimeError(f"No built APKs found under {outputs_root}")

    primary_apk, primary_sha256, _ = next(
        (item for item in packaged_apks if item[2]),
        packaged_apks[0],
    )

    update_manifest = {
        "schemaVersion": 1,
        "repository": args.repository,
        "tag": args.tag,
        "version": args.version,
        "versionCode": args.version_code,
        "apk": primary_apk.name,
        "sha256": primary_sha256,
        "size": primary_apk.stat().st_size,
    }
    manifest_path = out_dir / "fabushi-android-update.json"
    manifest_path.write_text(json.dumps(update_manifest, indent=2) + "\n", encoding="utf-8")

    sha_lines = [f"{digest}  {apk.name}" for apk, digest, _ in packaged_apks]
    sha_lines.append(f"{sha256_file(manifest_path)}  {manifest_path.name}")
    (out_dir / "SHA256SUMS").write_text("\n".join(sha_lines) + "\n", encoding="utf-8")

    if keystore_dir.exists() and not os.environ.get("ANDROID_KEYSTORE_PATH"):
        shutil.rmtree(keystore_dir, ignore_errors=True)

    print(f"[DONE] Release bundle ready at {out_dir}")


if __name__ == "__main__":
    main()
