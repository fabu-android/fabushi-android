# Fabushi Android — Agent Instructions

These instructions apply repository-wide to AI-assisted development in `fabu-android/fabushi-android` (`bhrumom/fabushi-android`).

## CRITICAL: Repository ownership

This repository is the canonical source for **native Android application, Android-specific UI/runtime integration, on-device Mika phone-control agent modules (`:mika-core`, `:mika-agent`), Android Rust JNI host bridge (`mobile/android/rust/mahayana_android_host`), packaging and Play/GitHub distribution** (see `docs/specs/gpui-telegram-android-mika-fusion.md`).

- Verify the current GitHub repository before product-affecting work.
- Do not implement another Fabushi platform's product code (Desktop/iOS/Web) here. Switch to that platform's canonical repository first.
- `bhrumom/fabushi` is the legacy migration/source-history repository, not the canonical implementation repository for this scope.
- Keep Cargo profiles lean (`debug = 0`, `strip = "debuginfo"`, `incremental = false`) and never commit generated `jniLibs/*/libmahayana_app_host.so` or signing keystores to git.

## CRITICAL: Local build & test execution + mandatory post-test memory cleanup

Local compilation, JNI cross-builds, unit/integration testing, and APK verification are **explicitly allowed and required** before delivery, subject to strict memory bounds and immediate post-test cleanup:

1. **Run builds & tests locally with bounded concurrency**:
   - **Rust JNI Host**: Run `cargo test` in `mobile/android/rust/mahayana_android_host` and build 16KB-aligned native libraries via `bash scripts/build_rust_android.sh`.
   - **Android / Gradle**: Run `./gradlew` inside `mobile/android` with `--max-workers=1` or `--max-workers=2` and bounded daemon heaps (`org.gradle.jvmargs=-Xmx2816m`, `kotlin.daemon.jvmargs=-Xmx1664m`). Run heavy R8 minification tasks (`:app:assembleRelease`, `:app:assembleGithubRelease`) sequentially rather than in parallel to avoid OOM on zero-swap machines.
2. **Mandatory post-test memory & artifact cleanup (测试完成后必须立即释放内存与临时产物)**:
   - Immediately after local test/build verification completes, **always** stop resident JVM daemons (`GradleDaemon` and `KotlinCompileDaemon`) and clean temporary build outputs so background processes never hold RAM:
     ```bash
     (cd mobile/android && ./gradlew --stop) || true
     jps -l | awk '/KotlinCompileDaemon|GradleDaemon/ {print $1}' | xargs -r kill || true
     rm -rf mobile/android/rust/mahayana_android_host/target dist release-signing
     free -h
     ```
   - Confirm via `jps -l` and `free -h` that no idle `GradleDaemon` or `KotlinCompileDaemon` processes remain resident in memory.

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
