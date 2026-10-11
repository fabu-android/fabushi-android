# Grok Bot 0.18 rights and provenance review

Status: **BLOCKED for direct source/binary reuse and public redistribution clearance**

Reviewed baseline: `b-nnett/grok-bot-0.18-reconstructed@a9f633e09d49a85829b8236331b9e21f7e612634`

## Scope

This review covers whether Fabushi Android may satisfy parity by copying or redistributing code, renderer assets, installers, or recovered implementation material from the pinned Grok Bot 0.18 reconstruction. It does not provide legal advice or create a license.

## Evidence

The pinned reconstruction's `NOTICE.md` states: “No upstream source-code license is asserted or granted here.” It also warns that reconstruction and absence of the original payload do not make redistribution safe.

The pinned `PROVENANCE.md` identifies the project as an unofficial reconstruction from publicly distributed signed installers, distinguishes reconstructed material from original authored source, and requires an independent rights review before public redistribution. It also says the readable frontend is only a partial evidence-backed reconstruction and that no upstream source-code license is implied.

No `LICENSE` file granting a reusable license was present at the pinned tree root inspected for this review.

## Decision for this PR

1. Do **not** bulk-copy Grok reconstructed TypeScript/React/runtime source into Fabushi Android.
2. Do **not** embed or redistribute the pinned Grok installers, extracted renderer bundles, signatures, trademarks, or binary payloads as Fabushi product assets.
3. Treat the pinned repository as a behavioral/architectural reference only: module boundaries, observable contracts, state-machine behavior, public strings where independently necessary, and evidence anchors may guide clean-room Android implementations.
4. Implement Android equivalents in Kotlin/Rust/Compose from independently described responsibilities and tests. Each completed parity row must point to Fabushi-owned implementation and Fabushi tests/evidence.
5. Any row that truly has no Android counterpart may become `not-applicable` only with a specific reviewed Android rationale; license uncertainty is **not** a reason to mark functionality N/A.
6. Before a public release can satisfy R16 / AC-22, obtain independent rights clearance or other verifiable authorization covering any material proposed for reuse, and re-review third-party dependency and trademark/service-term obligations.

## Release impact

R16 and AC-22 remain **blocked**. This does not block clean-room engineering, CI, APK testing, or behavioral parity work, but it blocks declaring the migration fully complete or release-cleared.

## Provenance rule for implementation evidence

For new Fabushi parity implementations, evidence should record:
- Grok baseline path/responsibility used as the comparison target;
- Fabushi target path;
- implementation language/process owner;
- Fabushi test or exact-HEAD CI evidence;
- confirmation that implementation is Fabushi-authored / clean-room rather than copied reconstructed source.
