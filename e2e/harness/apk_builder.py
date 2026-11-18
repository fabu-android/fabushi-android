"""Helper creating a genuine binary-AXML AndroidManifest.xml APK skeleton via Android SDK aapt2."""

from __future__ import annotations

import subprocess
import zipfile
from pathlib import Path

_MINIMAL_MANIFEST_XML = """<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="com.ombhrum.fabushi"
    android:versionCode="20100"
    android:versionName="2.1.0">
    <uses-sdk android:minSdkVersion="29" android:targetSdkVersion="35" />
    <application android:label="Fabushi" />
</manifest>
"""


def create_valid_unsigned_apk(
    out_apk: Path,
    work_dir: Path,
    *,
    arm64_host_bytes: bytes,
    x64_host_bytes: bytes,
    mikarun_bytes: bytes,
    proot_bytes: bytes,
    proot_loader_bytes: bytes,
    rootfs_bytes: bytes,
) -> Path:
    """Use SDK build-tools aapt2 link to emit real binary AndroidManifest.xml (minSdk=29), then add .so & ZIP_STORED rootfs."""
    aapt2 = Path("/home/antigravity/android-sdk/build-tools/37.0.0/aapt2")
    android_jar = Path("/home/antigravity/android-sdk/platforms/android-37.0/android.jar")
    manifest_src = work_dir / "AndroidManifest.src.xml"
    base_apk = work_dir / "aapt2_base.apk"
    manifest_src.write_text(_MINIMAL_MANIFEST_XML, encoding="utf-8")

    subprocess.run(
        [
            str(aapt2),
            "link",
            "-I",
            str(android_jar),
            "--manifest",
            str(manifest_src),
            "-o",
            str(base_apk),
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=True,
    )

    with zipfile.ZipFile(base_apk, "r") as zin:
        binary_manifest = zin.read("AndroidManifest.xml")
        resources_arsc = zin.read("resources.arsc") if "resources.arsc" in zin.namelist() else None

    with zipfile.ZipFile(out_apk, "w", compression=zipfile.ZIP_DEFLATED) as zf:
        zf.writestr("AndroidManifest.xml", binary_manifest)
        if resources_arsc is not None:
            zi_res = zipfile.ZipInfo("resources.arsc")
            zi_res.compress_type = zipfile.ZIP_STORED
            zf.writestr(zi_res, resources_arsc)
        zf.writestr("classes.dex", b"dex\n035\x00" + b"\x00" * 104)
        zf.writestr("lib/arm64-v8a/libmahayana_app_host.so", arm64_host_bytes)
        zf.writestr("lib/x86_64/libmahayana_app_host.so", x64_host_bytes)
        zf.writestr("lib/arm64-v8a/libmikarun.so", mikarun_bytes)
        zf.writestr("lib/arm64-v8a/libproot.so", proot_bytes)
        zf.writestr("lib/arm64-v8a/libproot_loader.so", proot_loader_bytes)
        zi_rootfs = zipfile.ZipInfo("assets/runtime/rootfs.tar.xz")
        zi_rootfs.compress_type = zipfile.ZIP_STORED
        zf.writestr(zi_rootfs, rootfs_bytes)

    return out_apk
