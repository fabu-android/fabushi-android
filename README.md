# Fabushi Android

This repository is the standalone native Android product boundary for Fabushi.

The repository was historically extracted from `bhrumom/fabushi@7851b689d2fe3fc3893cd9f4363899cc4a03e83b`
by the FAB-P0013 platform repository export workflow. That marker is provenance only; it is
not the current product authority.

## Current migration authority

The active product migration is the complete native Android equivalent of
`bhrumom/fabushi-desktop` canonical `main`. The current durable source of truth is:

- `AGENTS.md`
- `ANDROID_PORT.md`
- `docs/specs/desktop-main-android-full-parity.md`
- `docs/android-port/**`
- `docs/android-port/authority/source-inventory-manifest.json`
- `docs/android-port/authority/source-inventory-ledger.jsonl`
- `docs/android-port/authority/responsibility-ledger.json`

The active implementation continues PR #3 rather than creating a parallel migration.
Every CI, package, artifact, and acceptance result proves only its exact Git HEAD.

## Native product boundary

Shipping Android composition is native and repository-owned:

```text
Jetpack Compose
  -> Presentation / ViewModel
  -> Typed Android bridge
  -> Android platform runtime
  -> android-host-jni
  -> Mahayana Coordinator
  -> Host / Capability Broker
  -> Local Runner or explicitly authorized Remote Runner
```

Kotlin/Compose owns Android UI and platform adapters. Rust owns the portable
Coordinator/Host/Runner/domain runtime. JNI is the formal Kotlin/Rust ABI. WebView is
limited to isolated Mini App/Web surfaces; it is not the application shell.

The current repository includes Android presentation/platform sources under
`frontend/`, `source/android-main/`, and `source/android-preload/`, plus the
repository-owned Rust workspace under `source/`. Exact source paths are governed by the
current Git tree and authority ledgers, not historical README path lists.

## Verification and completion

All build, test, lint, package, release, and acceptance execution for this migration runs
in GitHub Actions. Do not use a developer Mac or bhrum2 as a substitute verification
environment.

This migration is **in progress** until the same final Android exact HEAD has zero strict
Desktop-source debt and passes the required Rust/Android/JNI, release/R8, APK/AAB
provenance, packaged acceptance, and protected device/account acceptance gates. The
presence of a crate, document, UI screen, or historical green run is not proof of full
Desktop parity.

Historical export provenance remains in `MIGRATION_SOURCE.md`. Do not add credentials or
undeclared cross-repository source dependencies.
