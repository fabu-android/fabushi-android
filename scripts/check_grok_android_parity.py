#!/usr/bin/env python3
"""Architecture/parity gate for the Grok Bot 0.18 -> Fabushi Android migration."""

from __future__ import annotations

import argparse
import json
import pathlib
import sys
from dataclasses import dataclass

ROOT = pathlib.Path(__file__).resolve().parents[1]
INVENTORY = ROOT / "manifests/grok-bot-0.18-source-frontend-inventory.json"
LEDGER = ROOT / "manifests/grok-bot-0.18-android-parity-ledger.json"
FULL_SOURCE_MANIFEST = ROOT / "docs/android-port/authority/source-inventory-manifest.json"
FULL_SOURCE_LEDGER = ROOT / "docs/android-port/authority/source-inventory-ledger.jsonl"
RESPONSIBILITY_LEDGER = ROOT / "docs/android-port/authority/responsibility-ledger.json"
FULL_SOURCE_STATUSES = {"unreviewed", "mapped", "implemented", "verified", "not-applicable"}
FULL_SOURCE_DISPOSITIONS = {"direct-port", "android-adapted", "not-applicable-with-replacement", "mixed"}

ALLOWED_CLASSES = {"native-equivalent", "shared-core", "platform-adapted", "not-applicable"}
ALLOWED_STATUS = {"mapped", "implemented", "verified", "blocked"}

TARGET_PREFIX = {
    "source/electron-main/": "source/android-main/",
    "source/electron-preload/": "source/android-preload/",
    "source/electron-dev-controls/": "source/android-dev-controls/",
    "source/node-agent-coordinator/": "source/mahayana-agent-coordinator/",
    "source/host/": "source/host/",
    "source/local-exec-daemon/": "source/local-exec-daemon/",
    "source/box-exec-daemon/": "source/box-exec-daemon/",
    "source/internal/": "source/internal/",
    "source/packages/": "source/packages/",
    "source/shared/": "source/shared/",
    "frontend/": "frontend/",
}

SCAFFOLD_MARKERS = [
    "frontend/.architecture-root",
    "source/android-main/.architecture-root",
    "source/android-preload/.architecture-root",
    "source/android-dev-controls/.architecture-root",
    "source/mahayana-agent-coordinator/.architecture-root",
    "source/host/.architecture-root",
    "source/local-exec-daemon/.architecture-root",
    "source/box-exec-daemon/.architecture-root",
    "source/internal/.architecture-root",
    "source/packages/.architecture-root",
    "source/shared/.architecture-root",
    "tests/.architecture-root",
    "scripts/check_grok_android_parity.py",
    "manifests/grok-bot-0.18-source-frontend-inventory.json",
    "manifests/grok-bot-0.18-android-parity-ledger.json",
]

STRICT_FORBIDDEN_PATHS = [
    "mobile/android/app/src/main/java/com/ombhrum/fabushi/GrokMobileShellAndroid.kt",
    "mobile/android/app/src/main/java/com/ombhrum/fabushi/FabushiScreen.kt",
    "mobile/android/app/src/main/java/com/ombhrum/fabushi/MobileBotViewModel.kt",
    "mobile/android/app/src/main/java/com/ombhrum/fabushi/MessagingViewModel.kt",
    "mobile/android/app/src/main/java/com/ombhrum/fabushi/MarketplaceViewModel.kt",
    "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/production/GrokMobileShellAndroid.kt",
    "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/production/FabushiScreen.kt",
    "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/production/FabushiApplicationRoot.kt",
]

PRESENTATION_HOST_BYPASS_FILES = [
    "mobile/android/app/src/main/java/com/ombhrum/fabushi/MainActivity.kt",
    "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/presentation/MobileBotViewModel.kt",
    "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/presentation/MessagingViewModel.kt",
    "frontend/src/main/kotlin/com/ombhrum/fabushi/frontend/presentation/MarketplaceViewModel.kt",
]

HOST_IMPLEMENTATION_ALLOWLIST = {
    "source/android-main/src/main/kotlin/com/ombhrum/fabushi/core/MahayanaHost.kt",
    "source/android-main/src/main/kotlin/com/ombhrum/fabushi/androidmain/coordinator/AndroidCoordinatorRuntime.kt",
}

@dataclass
class CheckResult:
    errors: list[str]
    warnings: list[str]
    summary: dict[str, object]

def load_json(path: pathlib.Path) -> dict:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)

def load_jsonl(path: pathlib.Path) -> list[dict]:
    rows: list[dict] = []
    with path.open("r", encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, start=1):
            if not line.strip():
                continue
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError as error:
                raise ValueError(f"{path}:{line_number}: invalid JSON: {error}") from error
    return rows


def check_full_desktop_inventory(strict: bool, errors: list[str], warnings: list[str]) -> dict[str, object]:
    manifest = load_json(FULL_SOURCE_MANIFEST)
    source_rows = load_jsonl(FULL_SOURCE_LEDGER)
    responsibility_doc = load_json(RESPONSIBILITY_LEDGER)
    responsibilities = responsibility_doc.get("responsibilities", [])
    responsibility_by_id = {
        item.get("responsibility_id"): item
        for item in responsibilities
        if item.get("responsibility_id")
    }

    expected_count = manifest.get("tracked_non_tree_entries")
    if not isinstance(expected_count, int) or expected_count <= 0:
        errors.append(
            f"full Desktop inventory must declare a positive tracked entry count, got {expected_count!r}"
        )
    if len(source_rows) != expected_count:
        errors.append(f"full Desktop source ledger row count mismatch: {len(source_rows)} != {expected_count}")

    authority_commit = manifest.get("desktop_commit")
    responsibility_authority = responsibility_doc.get("authority", {}).get("desktop_commit")
    if authority_commit != responsibility_authority:
        errors.append(
            "full Desktop inventory/responsibility authority mismatch: "
            f"{authority_commit!r} != {responsibility_authority!r}"
        )

    by_path: dict[str, dict] = {}
    duplicate_paths: list[str] = []
    status_counts: dict[str, int] = {}
    debt = {
        "empty_responsibilities": [],
        "missing_disposition": [],
        "nonterminal_status": [],
        "missing_android_targets": [],
        "missing_evidence": [],
    }
    for row in source_rows:
        path = str(row.get("source_path", ""))
        if not path:
            errors.append("full Desktop source ledger contains row without source_path")
            continue
        if path in by_path:
            duplicate_paths.append(path)
        by_path[path] = row
        if row.get("source_commit") != authority_commit:
            errors.append(f"{path}: source_commit does not match full Desktop authority")
        status = row.get("implementation_status")
        status_counts[status] = status_counts.get(status, 0) + 1
        if status not in FULL_SOURCE_STATUSES:
            errors.append(f"{path}: invalid full-source implementation_status {status!r}")
        row_responsibilities = row.get("responsibilities") or []
        if not row_responsibilities:
            debt["empty_responsibilities"].append(path)
        for responsibility_id in row_responsibilities:
            if responsibility_id not in responsibility_by_id:
                errors.append(f"{path}: unknown responsibility reference {responsibility_id!r}")
        disposition = row.get("disposition")
        if disposition is not None and disposition not in FULL_SOURCE_DISPOSITIONS:
            errors.append(f"{path}: invalid disposition {disposition!r}")
        if disposition is None:
            debt["missing_disposition"].append(path)
        if status not in {"verified", "not-applicable"}:
            debt["nonterminal_status"].append(path)
        if not row.get("android_targets") and disposition != "not-applicable-with-replacement":
            debt["missing_android_targets"].append(path)
        if not row.get("evidence"):
            debt["missing_evidence"].append(path)

    if duplicate_paths:
        errors.append(f"full Desktop source ledger contains {len(duplicate_paths)} duplicate path(s)")

    for name, paths in debt.items():
        if not paths:
            continue
        sample = ", ".join(paths[:5])
        message = f"full Desktop source debt {name}={len(paths)}; sample: {sample}"
        (errors if strict else warnings).append(message)

    missing_reverse_links = 0
    blob_mismatches = 0
    for item in responsibilities:
        rid = item.get("responsibility_id")
        if strict and (
            item.get("implementation_status") != "verified"
            or item.get("verification_status") != "verified"
        ):
            errors.append(
                f"{rid}: strict responsibility gate requires implementation_status=verified "
                "and verification_status=verified"
            )
        for source in item.get("desktop_sources", []):
            path = source.get("path")
            row = by_path.get(path)
            if row is None:
                errors.append(f"{rid}: Desktop source anchor is absent from {expected_count}-entry inventory: {path}")
                continue
            if source.get("blob_sha") and source.get("blob_sha") != row.get("source_blob_sha"):
                blob_mismatches += 1
                errors.append(f"{rid}: blob SHA mismatch for {path}")
            if rid not in (row.get("responsibilities") or []):
                missing_reverse_links += 1
                message = f"{rid}: source row {path} is missing reverse responsibility link"
                (errors if strict else warnings).append(message)

    return {
        "full_source_entries": len(source_rows),
        "full_source_empty_responsibilities": len(debt["empty_responsibilities"]),
        "full_source_status_counts": status_counts,
        "responsibility_count": len(responsibilities),
        "missing_reverse_links": missing_reverse_links,
        "blob_mismatches": blob_mismatches,
        "strict_incomplete_rows": len(
            set().union(
                debt["empty_responsibilities"],
                debt["missing_disposition"],
                debt["nonterminal_status"],
                debt["missing_android_targets"],
                debt["missing_evidence"],
            )
        ),
    }


def expected_target_prefix(grok_path: str) -> str | None:
    for source_prefix, target_prefix in TARGET_PREFIX.items():
        if grok_path.startswith(source_prefix):
            return target_prefix
    if grok_path in {"source/tsconfig.json", "source/mime-types.d.ts"}:
        return "source/"
    return None

def run_checks(strict: bool) -> CheckResult:
    errors: list[str] = []
    warnings: list[str] = []

    full_summary = check_full_desktop_inventory(strict, errors, warnings)

    inventory = load_json(INVENTORY)
    ledger = load_json(LEDGER)

    inventory_files = inventory.get("files", [])
    rows = ledger.get("rows", [])
    inventory_paths = [entry.get("path") for entry in inventory_files]
    ledger_paths = [row.get("grok_path") for row in rows]

    if inventory.get("file_count") != 2046:
        errors.append(f"inventory file_count must be 2046, got {inventory.get('file_count')!r}")
    if inventory.get("source_file_count") != 1724:
        errors.append("inventory source_file_count must be 1724")
    if inventory.get("frontend_file_count") != 322:
        errors.append("inventory frontend_file_count must be 322")
    if len(inventory_paths) != len(set(inventory_paths)):
        errors.append("inventory contains duplicate paths")
    if len(ledger_paths) != len(set(ledger_paths)):
        errors.append("ledger contains duplicate grok_path rows")
    if set(inventory_paths) != set(ledger_paths):
        missing = sorted(set(inventory_paths) - set(ledger_paths))
        extra = sorted(set(ledger_paths) - set(inventory_paths))
        errors.append(f"ledger coverage mismatch: missing={len(missing)} extra={len(extra)}")

    status_counts: dict[str, int] = {}
    class_counts: dict[str, int] = {}
    missing_targets = 0
    for row in rows:
        path = row.get("grok_path", "")
        target = row.get("android_target_path", "")
        parity_class = row.get("parity_class")
        status = row.get("implementation_status")

        class_counts[parity_class] = class_counts.get(parity_class, 0) + 1
        status_counts[status] = status_counts.get(status, 0) + 1

        if parity_class not in ALLOWED_CLASSES:
            errors.append(f"{path}: invalid parity_class {parity_class!r}")
        if status not in ALLOWED_STATUS:
            errors.append(f"{path}: invalid implementation_status {status!r}")
        if not target or target.startswith("unmapped/"):
            errors.append(f"{path}: missing Android target path")
        prefix = expected_target_prefix(path)
        if prefix is None:
            errors.append(f"{path}: checker has no source-to-target rule")
        elif not target.startswith(prefix):
            errors.append(f"{path}: target {target!r} must start with {prefix!r}")

        if parity_class == "not-applicable":
            if not row.get("not_applicable_reason"):
                errors.append(f"{path}: not-applicable requires not_applicable_reason")
            if row.get("not_applicable_reviewed") is not True:
                errors.append(f"{path}: not-applicable requires not_applicable_reviewed=true")

        if status in {"implemented", "verified"} and parity_class != "not-applicable":
            if not (ROOT / target).is_file():
                missing_targets += 1
                errors.append(f"{path}: {status} target file does not exist: {target}")
            evidence = str(row.get("test_evidence", "")).strip()
            if not evidence or evidence.startswith("pending "):
                errors.append(f"{path}: {status} row requires concrete test_evidence")

        if strict and parity_class != "not-applicable" and status != "verified":
            errors.append(f"{path}: strict gate requires verified, got {status!r}")

    for marker in SCAFFOLD_MARKERS:
        if not (ROOT / marker).exists():
            errors.append(f"missing architecture scaffold marker: {marker}")

    bypass_count = 0
    host_scan_roots = [
        ROOT / "mobile/android/app/src/main/java",
        ROOT / "frontend/src/main/kotlin",
        ROOT / "source/android-main/src/main/kotlin",
        ROOT / "source/android-preload/src/main/kotlin",
    ]
    for scan_root in host_scan_roots:
        if not scan_root.exists():
            continue
        for path in scan_root.rglob("*.kt"):
            relative = path.relative_to(ROOT).as_posix()
            if relative in HOST_IMPLEMENTATION_ALLOWLIST:
                continue
            if "MahayanaHost" not in path.read_text(encoding="utf-8"):
                continue
            bypass_count += 1
            message = f"non-runtime Kotlin file references MahayanaHost directly: {relative}"
            if strict:
                errors.append(message)
            else:
                warnings.append(message)

    legacy_android_product_files = []
    legacy_android_root = ROOT / "mobile/android/app/src/main/java/com/ombhrum/fabushi"
    if legacy_android_root.exists():
        legacy_android_product_files = sorted(
            path.relative_to(ROOT).as_posix()
            for path in legacy_android_root.rglob("*.kt")
            if path.name != "MainActivity.kt"
        )
    if legacy_android_product_files:
        message = (
            "legacy Android production root must contain only MainActivity.kt; "
            f"{len(legacy_android_product_files)} other Kotlin files remain"
        )
        if strict:
            errors.append(message)
        else:
            warnings.append(message)

    legacy_count = 0
    for relative in STRICT_FORBIDDEN_PATHS:
        if (ROOT / relative).exists():
            legacy_count += 1
            message = f"legacy monolith still present: {relative}"
            if strict:
                errors.append(message)
            else:
                warnings.append(message)

    presentation_runtime_bypasses = 0
    presentation_roots = [
        ROOT / "mobile/android/app/src/main/java/com/ombhrum/fabushi",
        ROOT / "frontend/src/main/kotlin",
    ]
    for root in presentation_roots:
        if not root.exists():
            continue
        for path in root.rglob("*.kt"):
            text = path.read_text(encoding="utf-8")
            forbidden_runtime_types = [
                name for name in ("AndroidCoordinatorRuntime", "FabushiRemoteDeviceGateway", "FabushiProcessRuntime")
                if name in text
            ]
            if forbidden_runtime_types:
                presentation_runtime_bypasses += 1
                relative = path.relative_to(ROOT)
                message = (
                    "presentation file references concrete runtime type(s) "
                    f"{','.join(forbidden_runtime_types)}: {relative}"
                )
                if strict:
                    errors.append(message)
                else:
                    warnings.append(message)

    frontend_android_main_dependencies = 0
    presentation_feature_receive_bypasses = 0
    frontend_root = ROOT / "frontend"
    if frontend_root.exists():
        for path in frontend_root.rglob("*.kt"):
            text = path.read_text(encoding="utf-8")
            if "com.ombhrum.fabushi.androidmain." in text or "AndroidCoordinatorPorts" in text:
                frontend_android_main_dependencies += 1
                errors.append(
                    "frontend must depend on android-preload contracts, not android-main: "
                    f"{path.relative_to(ROOT)}"
                )
            if "featureReceive" in text:
                presentation_feature_receive_bypasses += 1
                errors.append(
                    "frontend must consume Coordinator events through listeners, not poll featureReceive: "
                    f"{path.relative_to(ROOT)}"
                )

    native_host_bridge_paths = [
        ROOT / "source/android-host-jni/Cargo.toml",
        ROOT / "source/android-host-jni/src/lib.rs",
        ROOT / "source/host/src/android_json_runtime.rs",
    ]
    native_host_bridge_missing = [
        path.relative_to(ROOT).as_posix()
        for path in native_host_bridge_paths
        if not path.is_file()
    ]
    android_build_gradle = ROOT / "mobile/android/app/build.gradle"
    android_build_gradle_text = android_build_gradle.read_text(encoding="utf-8") if android_build_gradle.is_file() else ""
    release_session_import_disabled = (
        "githubRelease {" in android_build_gradle_text
        and "githubRelease {\n            initWith release\n            matchingFallbacks = ['release']\n            buildConfigField 'boolean', 'GITHUB_UPDATES_ENABLED', 'true'\n            buildConfigField 'boolean', 'CI_ACCOUNT_SESSION_IMPORT_ENABLED', 'false'" in android_build_gradle_text
    )
    ci_acceptance_session_import_enabled = (
        "ciAcceptance {" in android_build_gradle_text
        and "ciAcceptance {\n            initWith debug\n            applicationIdSuffix '.ci'\n            versionNameSuffix '-ci'\n            matchingFallbacks = ['debug']\n            debuggable true\n            minifyEnabled false\n            shrinkResources false\n            buildConfigField 'boolean', 'GITHUB_UPDATES_ENABLED', 'false'\n            buildConfigField 'boolean', 'CI_ACCOUNT_SESSION_IMPORT_ENABLED', 'true'" in android_build_gradle_text
    )
    if not release_session_import_disabled:
        errors.append("githubRelease must fail closed with CI_ACCOUNT_SESSION_IMPORT_ENABLED=false")
    if not ci_acceptance_session_import_enabled:
        errors.append("ciAcceptance must be the only explicit CI session-import variant")

    native_ci = ROOT / ".github/workflows/android-parity-full-ci.yml"
    native_ci_text = native_ci.read_text(encoding="utf-8") if native_ci.is_file() else ""
    native_ci_wired = (
        "cargo ndk" in native_ci_text
        and "fabushi-android-host-jni" in native_ci_text
        and "libmahayana_app_host.so" in native_ci_text
    )
    if native_host_bridge_missing:
        errors.append(
            "native Android Host bridge is incomplete: "
            + ", ".join(native_host_bridge_missing)
        )
    if not native_ci_wired:
        errors.append(
            "Android full CI must build and verify libmahayana_app_host.so from the exact HEAD"
        )

    architecture_scope_markers = sorted(
        path for base in (ROOT / "frontend", ROOT / "source")
        if base.exists()
        for path in base.rglob(".architecture-scope")
    )
    if strict and architecture_scope_markers:
        errors.append(
            f"strict gate forbids placeholder .architecture-scope files: {len(architecture_scope_markers)} remain"
        )

    summary = {
        **full_summary,
        "inventory_files": len(inventory_paths),
        "ledger_rows": len(rows),
        "status_counts": status_counts,
        "class_counts": class_counts,
        "implemented_rows_missing_target": missing_targets,
        "legacy_monoliths_present": legacy_count,
        "legacy_android_product_files": len(legacy_android_product_files),
        "presentation_host_bypasses": bypass_count,
        "presentation_runtime_bypasses": presentation_runtime_bypasses,
        "frontend_android_main_dependencies": frontend_android_main_dependencies,
        "presentation_feature_receive_bypasses": presentation_feature_receive_bypasses,
        "release_ci_account_session_import_disabled": release_session_import_disabled,
        "ci_acceptance_session_import_enabled": ci_acceptance_session_import_enabled,
        "native_host_bridge_missing": len(native_host_bridge_missing),
        "native_host_ci_wired": native_ci_wired,
        "architecture_scope_markers": len(architecture_scope_markers),
        "mode": "strict" if strict else "phase0",
    }
    return CheckResult(errors=errors, warnings=warnings, summary=summary)

def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Require every applicable row verified and all legacy bypasses removed.",
    )
    args = parser.parse_args()

    result = run_checks(strict=args.strict)
    print(json.dumps(result.summary, indent=2, sort_keys=True))
    for warning in result.warnings:
        print(f"WARNING: {warning}", file=sys.stderr)
    for error in result.errors:
        print(f"ERROR: {error}", file=sys.stderr)
    return 1 if result.errors else 0

if __name__ == "__main__":
    raise SystemExit(main())
