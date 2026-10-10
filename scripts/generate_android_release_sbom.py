#!/usr/bin/env python3
"""Generate deterministic Android release dependency evidence.

Inputs are cargo metadata JSON plus Gradle's resolved githubRelease runtime
dependency report. The output is a CycloneDX 1.6 component inventory and a
license-evidence inventory. This script never guesses a license for a Maven
coordinate: unresolved Maven license metadata stays explicit so release review
cannot mistake a coordinate list for legal clearance.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path

GRADLE_COMPONENT = re.compile(
    r"(?:---|\\---)\\s+([A-Za-z0-9_.-]+):([A-Za-z0-9_.-]+):([^\\s()]+)"
)


def stable_ref(kind: str, name: str, version: str, source: str = "") -> str:
    raw = f"{kind}|{name}|{version}|{source}".encode()
    return f"urn:fabushi:component:{hashlib.sha256(raw).hexdigest()[:32]}"


def rust_components(metadata: dict) -> tuple[list[dict], list[dict]]:
    components: list[dict] = []
    licenses: list[dict] = []
    for pkg in metadata.get("packages", []):
        name = str(pkg.get("name", ""))
        version = str(pkg.get("version", ""))
        source = str(pkg.get("source") or "workspace")
        license_expr = pkg.get("license")
        ref = stable_ref("cargo", name, version, source)
        component = {
            "type": "library",
            "bom-ref": ref,
            "name": name,
            "version": version,
            "properties": [
                {"name": "fabushi:ecosystem", "value": "cargo"},
                {"name": "fabushi:source", "value": source},
            ],
        }
        if license_expr:
            component["licenses"] = [{"expression": str(license_expr)}]
        components.append(component)
        licenses.append(
            {
                "ecosystem": "cargo",
                "name": name,
                "version": version,
                "source": source,
                "license": license_expr,
                "license_status": "declared" if license_expr else "unresolved",
            }
        )
    return components, licenses


def gradle_components(report: str) -> tuple[list[dict], list[dict]]:
    seen: set[tuple[str, str, str]] = set()
    components: list[dict] = []
    licenses: list[dict] = []
    for line in report.splitlines():
        match = GRADLE_COMPONENT.search(line)
        if not match:
            continue
        group, name, version = match.groups()
        if "->" in line:
            resolved = line.rsplit("->", 1)[1].strip().split()[0]
            if resolved:
                version = resolved
        key = (group, name, version)
        if key in seen:
            continue
        seen.add(key)
        full_name = f"{group}:{name}"
        ref = stable_ref("maven", full_name, version)
        components.append(
            {
                "type": "library",
                "bom-ref": ref,
                "group": group,
                "name": name,
                "version": version,
                "purl": f"pkg:maven/{group}/{name}@{version}",
                "properties": [{"name": "fabushi:ecosystem", "value": "gradle"}],
            }
        )
        licenses.append(
            {
                "ecosystem": "gradle",
                "name": full_name,
                "version": version,
                "license": None,
                "license_status": "requires-upstream-metadata-resolution",
            }
        )
    return components, licenses


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--cargo-metadata", required=True)
    p.add_argument("--gradle-dependencies", required=True)
    p.add_argument("--source-sha", required=True)
    p.add_argument("--sbom-out", required=True)
    p.add_argument("--licenses-out", required=True)
    args = p.parse_args()

    cargo = json.loads(Path(args.cargo_metadata).read_text(encoding="utf-8"))
    gradle = Path(args.gradle_dependencies).read_text(encoding="utf-8")
    rust, rust_licenses = rust_components(cargo)
    maven, maven_licenses = gradle_components(gradle)
    components = sorted(rust + maven, key=lambda c: (c.get("group", ""), c["name"], c["version"], c["bom-ref"]))

    if not components:
        raise SystemExit("release SBOM contains no components")
    if not rust:
        raise SystemExit("release SBOM contains no Cargo components")
    if not maven:
        raise SystemExit("release SBOM contains no Gradle runtime components")

    sbom = {
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "version": 1,
        "metadata": {
            "component": {
                "type": "application",
                "name": "Fabushi Android",
                "version": args.source_sha,
                "bom-ref": f"urn:fabushi:android:{args.source_sha}",
                "properties": [
                    {"name": "fabushi:source-sha", "value": args.source_sha},
                    {"name": "fabushi:first-party-license", "value": "UNLICENSED"},
                ],
            }
        },
        "components": components,
    }
    license_inventory = {
        "schema_version": 1,
        "source_sha": args.source_sha,
        "first_party_license": "UNLICENSED",
        "policy": {
            "no_license_guessing": True,
            "gradle_license_metadata_required_before_final_public_release": True,
        },
        "components": sorted(
            rust_licenses + maven_licenses,
            key=lambda x: (x["ecosystem"], x["name"], x["version"]),
        ),
    }
    Path(args.sbom_out).write_text(json.dumps(sbom, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    Path(args.licenses_out).write_text(
        json.dumps(license_inventory, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
