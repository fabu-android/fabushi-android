#!/usr/bin/env python3
"""Focused parser contracts for generate_android_release_sbom.py."""

from __future__ import annotations

import importlib.util
import tempfile
from pathlib import Path

SCRIPT = Path(__file__).with_name("generate_android_release_sbom.py")
SPEC = importlib.util.spec_from_file_location("generate_android_release_sbom", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
module = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(module)

REPORT = r"""
githubReleaseRuntimeClasspath - Runtime classpath of /githubRelease.
+--- org.jetbrains.kotlin:kotlin-stdlib:2.3.20
|    \--- org.jetbrains:annotations:13.0
+--- io.github.webrtc-sdk:android:150.7871.01
\--- com.squareup.okhttp3:okhttp:5.4.0
     +--- com.squareup.okio:okio:3.16.4 -> 3.16.5
     \--- org.jetbrains.kotlin:kotlin-stdlib-jdk8:2.3.10 -> 2.3.20 (*)
"""


def main() -> None:
    with tempfile.TemporaryDirectory() as directory:
        components, licenses = module.gradle_components(REPORT, Path(directory))
    coordinates = {
        (item.get("group"), item["name"], item["version"])
        for item in components
    }
    expected = {
        ("org.jetbrains.kotlin", "kotlin-stdlib", "2.3.20"),
        ("org.jetbrains", "annotations", "13.0"),
        ("io.github.webrtc-sdk", "android", "150.7871.01"),
        ("com.squareup.okhttp3", "okhttp", "5.4.0"),
        ("com.squareup.okio", "okio", "3.16.5"),
        ("org.jetbrains.kotlin", "kotlin-stdlib-jdk8", "2.3.20"),
    }
    if coordinates != expected:
        raise AssertionError(f"Gradle parser mismatch: {coordinates!r} != {expected!r}")
    if len(licenses) != len(expected):
        raise AssertionError("license inventory must preserve every parsed Gradle component")
    if not all(item["license_status"] == "unresolved" for item in licenses):
        raise AssertionError("empty test cache must remain explicitly unresolved")
    print(f"Gradle runtime parser contract passed ({len(expected)} components)")


if __name__ == "__main__":
    main()
