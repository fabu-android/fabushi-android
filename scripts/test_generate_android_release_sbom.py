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

    with tempfile.TemporaryDirectory() as directory:
        cache = Path(directory)
        child_dir = cache / "com.google.guava" / "listenablefuture" / "1.0" / "child"
        parent_dir = cache / "com.google.guava" / "guava-parent" / "26.0-android" / "parent"
        child_dir.mkdir(parents=True)
        parent_dir.mkdir(parents=True)
        (child_dir / "listenablefuture-1.0.pom").write_text(
            """<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <parent>
    <groupId>com.google.guava</groupId>
    <artifactId>guava-parent</artifactId>
    <version>26.0-android</version>
  </parent>
  <artifactId>listenablefuture</artifactId>
  <version>1.0</version>
</project>
""",
            encoding="utf-8",
        )
        (parent_dir / "guava-parent-26.0-android.pom").write_text(
            """<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.google.guava</groupId>
  <artifactId>guava-parent</artifactId>
  <version>26.0-android</version>
  <licenses>
    <license>
      <name>Apache License, Version 2.0</name>
      <url>https://www.apache.org/licenses/LICENSE-2.0.txt</url>
    </license>
  </licenses>
</project>
""",
            encoding="utf-8",
        )
        inherited = module.pom_licenses(
            cache,
            "com.google.guava",
            "listenablefuture",
            "1.0",
        )
        if inherited != [
            {
                "name": "Apache License, Version 2.0",
                "url": "https://www.apache.org/licenses/LICENSE-2.0.txt",
            }
        ]:
            raise AssertionError(f"parent POM license inheritance failed: {inherited!r}")

        cycle_a = cache / "example" / "a" / "1" / "a"
        cycle_b = cache / "example" / "b" / "1" / "b"
        cycle_a.mkdir(parents=True)
        cycle_b.mkdir(parents=True)
        (cycle_a / "a-1.pom").write_text(
            """<project><parent><groupId>example</groupId><artifactId>b</artifactId><version>1</version></parent></project>""",
            encoding="utf-8",
        )
        (cycle_b / "b-1.pom").write_text(
            """<project><parent><groupId>example</groupId><artifactId>a</artifactId><version>1</version></parent></project>""",
            encoding="utf-8",
        )
        if module.pom_licenses(cache, "example", "a", "1"):
            raise AssertionError("cyclic parent POMs must remain unresolved")

    print(f"Gradle runtime parser contract passed ({len(expected)} components)")


if __name__ == "__main__":
    main()
