# Fabushi Android — Agent Instructions

These instructions apply repository-wide to AI-assisted development in `fabu-android/fabushi-android`.

## CRITICAL: Current Desktop-to-Android migration authority

Start with `ANDROID_PORT.md`, `docs/specs/desktop-main-android-full-parity.md`, and `docs/android-port/README.md`. The current user goal is the complete native Android equivalent of the applicable responsibilities of `bhrumom/fabushi-desktop` canonical `main`; historical Grok material is not the final product authority. Read `docs/android-port/authority/discovery.md` for verified source-layout changes and unresolved delivery issues. Source facts come from pinned code and its real production/build wiring, not stale provenance prose.

All builds and tests run in GitHub Actions. Do not build, test, or package on bhrum2 or the user's Mac. A source inventory or a documentation check does not establish product parity. The documentation delivery blocker for eight feature handbooks is recorded in `docs/android-port/operations/blockers.md`; do not silently remove their completeness requirements.

## CRITICAL: Repository ownership

This repository is the canonical source for the **standalone native Android product**, including Android UI, Android platform runtime, Android-local Coordinator/Host/Runner implementation, Android-local contracts/packages, packaging, testing, and Play/GitHub distribution.

- Verify the current GitHub repository, branch, open PRs, and applicable Spec before product-affecting work.
- Do not create a duplicate implementation branch when an active PR already owns the same task; continue the existing work unless the user explicitly requests otherwise.
- `bhrumom/fabushi` is legacy migration/source-history for this Android scope, not the canonical implementation repository.
- `bhrumom/fabushi-platform-core` and other platform repositories may be consulted as references, but Android must not require their runtime source to build or run.
- When the active Spec requires standalone platform ownership, reimplement or deliberately import/vendor required capability into this repository under an explicit provenance/license decision rather than preserving cross-repository source coupling.

## CRITICAL: Spec-first development — No Spec, No Code

Before changing application/runtime code, tests, schemas, contracts, dependencies, build/release configuration, migrations, security controls, or other behavior-affecting files:

1. Read this `AGENTS.md`.
2. Check the current repository, branch, and open PRs so existing work is continued instead of duplicated.
3. Find and read the applicable durable Spec/project/source-of-truth documents.
4. Check `docs/specs/` for a task/feature Spec.
5. Validate the Spec against the latest explicit user requirement and current repository/GitHub facts.
6. If no usable Spec exists, or it is stale/unclear/contradictory, create or repair the Spec **before implementation** using `docs/specs/SPEC_TEMPLATE.md`.

Read-only investigation needed to understand the system or write/repair the Spec is allowed first. Product-affecting implementation is not.

## Mandatory lifecycle

**Discover → Spec → Architecture/Plan → Implement → Verify → Spec Compliance Review → Integrate/Deliver**

Before completion, compare the implementation against every applicable requirement and acceptance criterion and record `passed`, `blocked`, or `not-applicable` with evidence/reason.

## Fail-closed rules

Do not start product-affecting implementation without a usable Spec; do not use chat memory as the only durable requirement source; do not silently change scope or weaken acceptance criteria; update the Spec when design/behavior changes intentionally.

Canonical policy: `docs/specs/spec-first-ai-development.md`.
