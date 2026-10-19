# Global Dandelion Mini App contract on Android

Status: active; contract slice 2026-10-19. Android support baseline: minSdk 26,
compileSdk/targetSdk 36 on the current GitHub Actions runner.

## Scope

Android owns the WebView injection and lifecycle boundary for the shared
`fabushi.miniapp.tools.v1` contract. It must not claim a Dandelion native
capability merely because a method name is present in a catalog response.
Until a Kotlin/Host adapter backed by the shared Rust core exists, those
capabilities remain unconfigured and the Mini App hides their panels.

## Requirements

- AND-GD-01: inject only the tool definitions granted to the installed,
  verified Mini App; never inject paths, credentials, shell access or private
  keys.
- AND-GD-02: preserve title, description, input schema and read-only metadata
  from the canonical tool contract.
- AND-GD-03: `offline.capabilities` reports only declared/granted domains from
  the complete set: offline, selection, transfer, scan, state, config,
  platform, hotspot, update, diagnostics and compat.
- AND-GD-04: every call remains Host-routed and approval-gated; unsupported or
  unconfigured native execution fails closed.
- AND-GD-05: the WebView bridge is torn down on page disposal, loses its native
  execution grant while the Activity is paused/stopped, and late events cannot
  reach a destroyed page.

## Acceptance

- AND-GD-AC-01: unit test proves complete capability-domain projection and
  excludes undeclared domains.
- AND-GD-AC-02: unit/Actions compilation proves schema and title propagation.
- AND-GD-AC-03: lifecycle review proves background revocation and disposal
  cleanup; a later native adapter must add real Rust-core, picker,
  permission, cancellation, restart and device evidence before any row is
  promoted from `not-configured` to `passed`.

This spec is subordinate to the cross-platform delivery matrix in Desktop and
the Global Dandelion Rust/package specifications.
