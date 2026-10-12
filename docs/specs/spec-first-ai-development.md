# Spec-first AI development

Status: active  
Date: 2026-09-21  
Repository: `bhrumom/fabushi-android`  
Repository scope: native Android application, Android-specific UI/runtime integration, packaging and Play distribution

## Rule

**No Spec, No Code.** Read the applicable durable Spec before product-affecting implementation. If no usable Spec exists, create one under `docs/specs/<task-name>.md` from `docs/specs/SPEC_TEMPLATE.md`.

Read-only discovery may precede the Spec only to understand current state and write/repair it.

## Repository identity

This repository is authoritative only for the scope above. Work belonging to another Fabushi split repository must switch repositories before implementation. Legacy copies in `bhrumom/fabushi` are migration/reference material, not an alternate product implementation location.

## Minimum Spec

The Spec must cover context/problem, goal, non-goals, requirements, current/target state, architecture/ownership, interfaces/contracts/data flow where relevant, non-functional constraints, failure modes, implementation strategy, verification, acceptance criteria/Definition of Done, release/migration/rollback/observability when applicable, and references/provenance.

## Lifecycle

Discover → Spec → Architecture/Plan → Implement → Verify → Spec Compliance Review → Integrate/Deliver.

For every requirement/acceptance criterion, record `passed`, `blocked`, or `not-applicable` with evidence/reason before declaring completion.

The latest explicit user requirement may amend an older Spec, but update the durable Spec/decision record before implementation intentionally diverges from it.
