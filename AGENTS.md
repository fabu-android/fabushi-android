# Fabushi Android — Agent Instructions

These instructions apply repository-wide to AI-assisted development in `fabu-android/fabushi-android` (`bhrumom/fabushi-android`).

## CRITICAL: Repository ownership

This repository is the canonical source for **native Android application, Android-specific UI/runtime integration, on-device Mika phone-control agent modules (`:mika-core`, `:mika-agent`), Android Rust JNI host bridge (`mobile/android/rust/mahayana_android_host`), packaging and Play/GitHub distribution** (see `docs/specs/gpui-telegram-android-mika-fusion.md`).

- Verify the current GitHub repository before product-affecting work.
- Do not implement another Fabushi platform's product code (Desktop/iOS/Web) here. Switch to that platform's canonical repository first.
- `bhrumom/fabushi` is the legacy migration/source-history repository, not the canonical implementation repository for this scope.
- Keep Cargo profiles lean (`debug = 0`, `strip = "debuginfo"`, `incremental = false`) and never commit generated `jniLibs/*/libmahayana_app_host.so` or signing keystores to git.

## CRITICAL: Spec-first development — No Spec, No Code

Before changing application/runtime code, tests, schemas, contracts, dependencies, build/release configuration, migrations, security controls, or other behavior-affecting files:

1. Read this `AGENTS.md`.
2. Find and read the applicable durable Spec/project/source-of-truth documents.
3. Check `docs/specs/` for a task/feature Spec.
4. Validate the Spec against the latest explicit user requirement and current repository/GitHub facts.
5. If no usable Spec exists, or it is stale/unclear/contradictory, create or repair the Spec **before implementation** using `docs/specs/SPEC_TEMPLATE.md`.

Read-only investigation needed to understand the system or write the Spec is allowed first. Product-affecting implementation is not.

## Mandatory lifecycle

**Discover → Spec → Architecture/Plan → Implement → Verify → Spec Compliance Review → Integrate/Deliver**

Before completion, compare the implementation against every applicable requirement and acceptance criterion and record `passed`, `blocked`, or `not-applicable` with evidence/reason.

## Fail-closed rules

Do not start product-affecting implementation without a usable Spec; do not use chat memory as the only durable requirement source; do not silently change scope or weaken acceptance criteria; update the Spec when design/behavior changes intentionally.

Canonical policy: `docs/specs/spec-first-ai-development.md`.
