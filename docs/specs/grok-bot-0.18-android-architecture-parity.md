# Grok Bot 0.18 → Fabushi Android Standalone Architecture & Behavior Parity — Specification

Status: active  
Owner: Fabushi Android  
Last updated: 2026-09-23  
Related issue/task/PR: PR #3; user-requested Android architecture parity migration

## 1. Context / problem

Fabushi Android is a native Android product currently rooted under `mobile/android`, with Jetpack Compose UI, Android ViewModels, Android-specific platform integrations, and a JNI-backed `MahayanaHost`. Important runtime and presentation responsibilities are currently mixed across large files such as `MainActivity.kt`, `GrokMobileShellAndroid.kt`, `FabushiScreen.kt`, `MobileBotViewModel.kt`, `MessagingViewModel.kt`, `MarketplaceViewModel.kt`, `FabushiRemoteDeviceGateway.kt`, `FabushiAppAgentSurface.kt`, and `core/MahayanaHost.kt`.

The target is not a Grok-inspired UI and not a thin Android client around shared cross-platform code. The target is a **standalone Android implementation** whose architecture, module boundaries, contracts, lifecycle behavior, and observable product effects correspond to Grok Bot 0.18 while using the best implementation language and Android primitive for each responsibility.

The canonical migration rule is **per-source-file audit/disposition + per-product-responsibility Android implementation**. Every pinned Grok source file must be accounted for in the ledger, but Fabushi Android does not need one physical target file for every Grok file. One reference file may split across multiple Kotlin/Rust modules; multiple reference files may converge into one Android-native implementation when that does not collapse a Grok architectural boundary or product responsibility.

Reference baselines:

- Grok reference repository: `b-nnett/grok-bot-0.18-reconstructed`
- Grok pinned reference commit: `a9f633e09d49a85829b8236331b9e21f7e612634`
- Fabushi Android repository: `bhrumom/fabushi-android`
- Fabushi Android discovery baseline: `59f6fc8885ce1cb8d1ad4fc5d4ab36f690fb99a2`

The Grok repository states that it is an unofficial reconstruction and that no upstream source-code license is asserted or granted. It is therefore an architecture, protocol, behavior, and evidence baseline. The migration must reproduce responsibilities and effects without assuming permission to bulk-copy reconstructed implementation text.

## 2. Architectural decision

### 2.1 Standalone platform ownership

**Fabushi Android must be self-contained. Cross-platform source sharing is not an architectural goal.**

The Android repository owns its complete product implementation, including:

- UI/renderer;
- Android main/platform lifecycle;
- Android trusted bridge;
- Mahayana Coordinator;
- Mahayana Host;
- Runner/local execution;
- remote/box execution adapters;
- `shared/**` contracts used by Android;
- `packages/**` agent/runtime packages used by Android;
- persistence;
- transcript;
- MCP/connectors;
- auth/OAuth/WebAuthn;
- inference routing;
- telemetry/observability;
- Android packaging, tests, scripts, and release logic.

Do not require runtime source from `bhrumom/fabushi-platform-core` or any other Fabushi platform repository to build or run the Android product. If useful code exists elsewhere, reimplement or deliberately import/vendor it into this repository under an explicit provenance/license decision; do not preserve a cross-repository runtime dependency merely to reduce duplication.

### 2.2 Language is not the goal

Use the language that best implements each boundary:

- Jetpack Compose/Kotlin for native Android renderer and Android APIs;
- Kotlin for Android lifecycle and platform adapters where native APIs dominate;
- Rust for Coordinator/Host/Runner/runtime components when it provides the best correctness/performance/isolation;
- C/C++ only when required by a native dependency or measurable platform need;
- no Node.js or Electron requirement simply because the Grok reference uses them.

The architecture must correspond to Grok even when language and platform primitives differ.

## 3. Goal

Rebuild Fabushi Android into a standalone architecture corresponding to Grok Bot 0.18:

```
frontend/
    │
    ▼
source/android-preload/
    │
    ▼
source/android-main/
    │
    ▼
source/mahayana-agent-coordinator/
    │
    ▼
source/host/
    │
    ├── MCP / connectors / tools
    ├── inference / agents
    ├── transcript / workflows / automations
    └── runner composition
             │
       ┌─────┴─────┐
       ▼           ▼
source/local-   source/box-
exec-daemon/   exec-daemon/
```

The migration must:

1. audit every relevant Grok file under `source/**` and `frontend/**`;
2. record each source responsibility, Android-visible effect, platform delta, target path(s)/existing equivalent, or reviewed platform disposition;
3. implement every product-relevant responsibility with equivalent contract behavior and Android-native effect;
4. preserve Grok's major domain/boundary ownership in the physical repository without requiring one-to-one file granularity;
5. preserve Coordinator / Host / Runner / bridge separation;
6. make renderer code consume coordinator projections rather than own agent orchestration;
7. reproduce supported Grok interactions and runtime effects on Android;
8. remove superseded legacy Android architecture after cutover;
9. prove behavior on an exact-HEAD packaged Android build, including process death/recreation.

The final product must behave as the **Android edition of Grok Bot**, not as a separate mobile shell connected to unrelated runtime plumbing. Except where Android lifecycle/security rules genuinely prohibit a desktop mechanism, the user must receive the same core Agent capability and lifecycle effect through an Android-native implementation.

## 4. Non-goals / out of scope

- Do not embed Electron in Android.
- Do not add Node.js solely to match Grok's language.
- Do not preserve a cross-platform shared-source architecture for its own sake.
- Do not depend on another Fabushi product repository for core Android runtime implementation.
- Do not retain the current `mobile/android` layout as the final architecture merely because it exists today.
- Do not require one Android target file for every Grok source file merely to match file counts.
- Do not create empty/no-op Kotlin/Rust counterparts solely to satisfy a source-tree mapping.
- Do not flatten Grok architectural boundaries or product responsibilities into unrelated Android mega-files.
- Do not preserve desktop-only system concepts literally when Android has no equivalent; preserve their responsibility/behavior through an Android-native counterpart.
- Do not require pixel-identical desktop geometry on a phone viewport; interaction, information hierarchy, visual language, animation/state intent, and functional effect must correspond while remaining responsive.
- Do not use a primary WebView shell as a shortcut for native Android UI parity.
- Do not maintain two production coordinators, transcripts, auth truths, or execution paths after cutover.
- Do not bulk-copy reconstructed source text without an explicit rights determination.

## 5. Requirements

### R1 — Complete file-level parity ledger

Before implementation changes beyond scaffolding, generate and maintain a parity ledger for every file under the pinned Grok reference:

- `source/**`
- `frontend/**`

Each row must contain:

- Grok path;
- Grok responsibility;
- relevant evidence/contract anchor;
- Android-visible effect;
- platform delta;
- Android target path(s), existing equivalent, or reviewed disposition;
- target language/runtime;
- parity class: `direct-equivalent`, `android-adapted`, or `not-applicable`;
- replacement behavior when the source mechanism itself is not applicable;
- implementation status;
- test/evidence;
- legacy Android path replaced/removed.

There is no `shared-core` disposition. Android implementation belongs to this repository.

No Grok module may silently disappear. `not-applicable` applies to a source implementation mechanism only when there is a real Android-specific technical reason and reviewer acceptance. It may not be used to remove a user-facing/core Agent effect that can be delivered through an Android-native or remote adapter; when the effect still matters, replacement behavior and evidence are mandatory.

### R2 — Major domain / boundary structure parity

The final repository must preserve Grok's major source organization and ownership boundaries, with explicit Android platform-name substitutions. This is a domain/boundary requirement, not a requirement for identical nested filenames or equal file counts.

Required top-level correspondence:

| Grok Bot 0.18 | Fabushi Android target |
| --- | --- |
| `frontend/` | `frontend/` |
| `source/electron-main/` | `source/android-main/` |
| `source/electron-preload/` | `source/android-preload/` |
| `source/electron-dev-controls/` | `source/android-dev-controls/` |
| `source/node-agent-coordinator/` | `source/mahayana-agent-coordinator/` |
| `source/host/` | `source/host/` |
| `source/local-exec-daemon/` | `source/local-exec-daemon/` |
| `source/box-exec-daemon/` | `source/box-exec-daemon/` |
| `source/internal/` | `source/internal/` |
| `source/packages/` | `source/packages/` |
| `source/shared/` | `source/shared/` |
| `tests/` | `tests/` |
| `scripts/` | `scripts/` |
| `manifests/` | `manifests/` |
| `docs/` | `docs/` |

The same ownership principle applies recursively, but Android-native splitting/merging of files is allowed when the ledger records the mapping and no reference boundary/responsibility is collapsed. Example:

```
Grok:
source/electron-main/auth/
source/electron-main/attachments/
source/electron-main/coordinator/
source/electron-main/mcp/
source/electron-main/media/
source/electron-main/notifications/
source/electron-main/prefs/
source/electron-main/secrets/
source/electron-main/startup/
source/electron-main/telemetry/
source/electron-main/update/
source/electron-main/vnc/

Android:
source/android-main/auth/
source/android-main/attachments/
source/android-main/coordinator/
source/android-main/mcp/
source/android-main/media/
source/android-main/notifications/
source/android-main/prefs/
source/android-main/secrets/
source/android-main/startup/
source/android-main/telemetry/
source/android-main/update/
source/android-main/vnc/
```

Android/Gradle/Rust build conventions live **inside** these corresponding module folders. For example, a Compose module may contain its own `build.gradle.kts` and `src/main/kotlin`; a Rust runtime module may contain `Cargo.toml` and `src/`. Build tooling must adapt to the architecture, not force unrelated product responsibilities back into one `app/src/main/java` tree.

Any deviation from the mapped structure requires a documented technical reason in the parity ledger.

### R3 — Grok boundary parity

The target must preserve these logical boundaries:

- renderer/UI;
- trusted bridge;
- platform/main lifecycle owner;
- coordinator;
- host;
- runner/local execution;
- remote/box execution;
- shared local contracts/policies;
- MCP/connectors;
- auth/OAuth/WebAuthn;
- persistence/telemetry/observability.

Layers communicate through explicit contracts. Direct bypasses are forbidden.

### R4 — Mahayana Coordinator

`source/mahayana-agent-coordinator/**` is the Android-local counterpart of Grok `source/node-agent-coordinator/**`.

It must own:

- renderer port lifecycle;
- request/reply correlation;
- ordered event fan-out;
- streaming turn activity;
- cancellation;
- reconnect/resync;
- transcript routing;
- client-side tool relay;
- gateway routing;
- inference routing;
- local-exec routing;
- routed MCP bridge;
- OAuth forwarding;
- WebAuthn/passkey forwarding where supported;
- telemetry/request lineage;
- Host supervision;
- crash settlement and deterministic terminal states.

The Android renderer/ViewModels must not duplicate these responsibilities.

Initial direct submodule correspondence:

| Grok | Android |
| --- | --- |
| `carrier.ts` | coordinator carrier/transport |
| `client-side-tool-v2-relay.ts` | client-side tool relay |
| `control-port-client.ts` | control-port client |
| `gateway/**` | `gateway/**` |
| `inference-router.ts` | inference router |
| `local-exec/**` | `local-exec/**` |
| `main.ts` | coordinator assembly/bootstrap |
| `oauth/**` | `oauth/**` |
| `renderer-port-server.ts` | Android renderer-port server |
| `routed-mcp-bridge.ts` | routed MCP bridge |
| `telemetry/**` | `telemetry/**` |
| `webauthn/**` | Android Credential Manager/WebAuthn counterpart |

### R5 — Mahayana Host

`source/host/**` is Android-owned and sits behind the Coordinator.

It must cover Android-relevant equivalents of Grok Host areas, including:

- `agent-isolation/**`;
- `agents/**`;
- `automations/**`;
- `box/**`;
- `cloud-agents/**`;
- `connectors/**`;
- `extensions/**`;
- `groups/**`;
- `local-exec/**`;
- `mcp-auth/**`;
- `ports/**`;
- `runner/**`;
- `storage/**`;
- `transcript-mirror/**`;
- `workflows/**`;
- gateway protocol/server/API;
- Host discovery/diagnostics/event bus;
- initial transcript load;
- lock/single-owner semantics;
- durable-file policy and paths;
- roster bookkeeping;
- secrets abstraction;
- request context;
- runner composition/bridge;
- crash guards;
- activity/user identity/trace behavior.

Host code must not own Compose UI, Activity navigation, or Android screen state.

### R6 — Runner / local-exec / remote-exec

The Grok `source/local-exec-daemon/**`, `source/box-exec-daemon/**`, Host runner modules, and local-exec contracts map to explicit Android-local modules with corresponding folders.

They must cover:

- execution request validation;
- process/session identity;
- capability/permission checks;
- cancellation;
- output streaming;
- timeout;
- crash/error normalization;
- capability advertisement;
- Android-local execution where safe;
- remote-device execution;
- remote/box routing where Android local execution is inappropriate.

Use Rust, Android Service/Foreground Service, WorkManager, JNI, or remote execution according to best effect. Do not expose unrestricted shell/reflection/credential access merely for parity.

### R7 — Thin Android trusted bridge

`source/android-preload/**` is the Android counterpart of Grok `source/electron-preload/**`.

It must be narrow and typed. It must contain Android equivalents of:

- coordinator-port bridge;
- main RPC runtime;
- RPC edge runtime;
- remote-computer/VNC liveness;
- visibility gating;
- clipboard transfer when permitted;
- WebView/Mini App bridge;
- passkey stall/settlement;
- debug controls;
- platform browser/base bridge.

Free-form `JSONObject` must disappear from UI-facing boundaries. Raw JSON is allowed only at explicit wire edges.

### R8 — Android platform-main parity

`source/android-main/**` is the Android counterpart of Grok `source/electron-main/**`.

`MainActivity` must cease to be the application coordinator. Its final responsibilities are Activity lifecycle, Compose root attachment, Android result/permission forwarding, Intent delivery, and system-window concerns.

Required directory/responsibility correspondence includes:

| Grok `electron-main` | Android `android-main` |
| --- | --- |
| `account/**` | account/session |
| `adapters/**` | Android capability adapters |
| `attachments/**` | ContentResolver/picker/URI grants |
| `auth/**` | Custom Tabs/Credential Manager/Keystore auth |
| `box/**` | remote execution/computer connector |
| `coordinator/**` | Mahayana Coordinator bootstrap/ownership |
| `deep-link/**` | Intent/deep-link router |
| `dev/**` | debug-only controls |
| `downloads/**` | DownloadManager/WorkManager |
| `experiments/**` | local typed feature flags |
| `feedback/**` | Android feedback |
| `generated/**` | generated local bindings |
| `local-exec/**` | Android Runner integration |
| `mcp/**` | MCP lifecycle/platform integration |
| `media/**` | media record/play/view |
| `models/**` | provider/model settings |
| `notifications/**` | NotificationManager/channels |
| `onepassword/**` | Android credential-provider counterpart or reviewed N/A |
| `prefs/**` | DataStore/local typed settings |
| `process-metrics/**` | Android process/runtime metrics |
| production adapters/bindings/RPC | Android-local production bindings |
| `secrets/**` | Android Keystore-backed secrets |
| `startup/**` | Application/process startup |
| `telemetry/**` | Android-local telemetry |
| `update/**` | Play/GitHub update path |
| `vnc/**` | remote-computer surface/liveness/input |
| window/broadcast/shortcuts/state files | Activity/multi-window/input/navigation equivalents |

Desktop window-specific details may be Android-adapted or N/A only with ledger evidence.

### R9 — Native renderer parity in `frontend/**`

`frontend/**` remains the renderer module name, matching Grok's top-level structure, but is implemented as a native Android Compose module.

It must provide responsive Android equivalents of:

- production renderer/root shell;
- bot/agent roster/sidebar;
- conversation transcript;
- composer;
- streaming/thinking/running/completed states;
- stop/cancel;
- agent name/edit/delete;
- row actions;
- command palette/search/action dispatch;
- settings and notices;
- reactions;
- groups/members;
- attachments/media;
- MCP/connector affordances;
- remote computer/context;
- errors/retry/recovery;
- onboarding/auth transitions.

`GrokMobileShellAndroid.kt` and `FabushiScreen.kt` must not remain giant renderer/controller files. UI renders immutable projections and emits typed intents.

### R10 — Android-local `source/shared/**`

Grok `source/shared/**` must map to **Android-local `source/shared/**`**, not to another repository.

The local tree must cover the Android equivalents of:

- agents;
- auth;
- automation schedule/automations;
- box migration/runtime/secrets;
- channel messaging/channels;
- client persistence;
- errors/retry;
- deep link;
- gateway reachability/wire;
- host settings;
- inference router contracts;
- local-exec gateway/process identity/permissions;
- MCP contracts/instructions/OAuth;
- media;
- message references;
- observability;
- ordering;
- notifications;
- persistence;
- RPC;
- transcript/threads;
- transport types;
- workflow model/workflows;
- usage;
- remote-computer/VNC liveness;
- WebAuthn gateway;
- write epoch/versioning.

These contracts are Android-local and are governed by this repository.

### R11 — Android-local `source/packages/**`

Every Grok package under `source/packages/**` must have an Android-local package/module disposition under the corresponding `source/packages/**` hierarchy.

Initial required set:

- `agent-analytics`
- `agent-client`
- `agent-core`
- `agent-exec`
- `agent-kv`
- `agent-store-sync`
- `agent-summarization`
- `agent-transcript`
- `agent`
- `analytics-client`
- `chat-inference-proto`
- `chat-inference`
- `constants`
- `context-rpc`
- `context`
- `cursor-config`
- `cursor-plugins`
- `git-core`
- `hooks-carriers`
- `hooks-exec`
- `hooks`
- `local-exec`
- `mcp-agent-exec`
- `mcp-core`
- `metrics`
- `prompt-jsx`
- `proto`
- `redacted-protos`
- `redaction`
- `shell-exec`
- `utils`

A package may use Kotlin or Rust and may be Android-adapted, but it may not be silently collapsed into an unrelated monolith.

### R12 — Feature-effect parity

For product-relevant features, parity is measured by observable effect and state behavior, not file naming or desktop implementation mechanics.

Required dimensions:

- request lifecycle;
- first-state/first-token responsiveness;
- incremental streaming;
- tool-call presentation and settlement;
- thinking/running/completed/failed/recovered;
- cancellation;
- reconnect/resync;
- transcript restoration;
- duplicate/out-of-order event handling;
- reactions;
- MCP/connector discovery/auth/invocation;
- OAuth return;
- attachment open/upload;
- remote-computer activity;
- error/retry;
- settings persistence;
- notifications;
- background/foreground;
- process recreation.

A representative Android Agent turn must demonstrate the same core effect chain:

```text
send
 -> accepted
 -> preparing/thinking
 -> Host inference
 -> tool/MCP/Runner request when needed
 -> live tool state
 -> result
 -> continued inference
 -> incremental transcript streaming
 -> completed/failed
```

If the Activity is recreated, the app is backgrounded, or Android kills and later recreates the process while a durable run is still owned by a surviving server/remote/runtime boundary, Fabushi Android must reattach/resync to that **same run** without silently losing it or starting a duplicate. When Android legitimately terminates local-only execution that cannot survive process death, the recovery/terminal behavior must be deterministic and explicitly mapped to the closest Grok product effect.

### R13 — One Android-local canonical truth

After cutover, exactly one Android-local owner exists for:

- auth/account;
- agent/bot roster;
- conversation/transcript;
- active operation;
- MCP/connector state;
- installed Mini App/plugin state;
- remote-device identity;
- settings;
- execution permissions.

Presentation/ViewModels may cache display state but may not create a second durable product truth.

### R14 — Android process-death resilience

Android may kill the process. The architecture must support:

- durable/resumable coordinator session semantics where appropriate;
- safe recreation of Coordinator and Host;
- renderer reattachment and resync;
- no duplicate send;
- deterministic settlement/recovery of in-flight turns;
- restored transcript/draft/navigation where product-appropriate;
- gateway re-registration;
- cancellation/timeout cleanup;
- no leaked JNI/native handles.

A permanently stuck "thinking" state is a release-blocking failure.

### R15 — Security

- secrets use Android Keystore or an Android-local protected provider;
- trusted bridge APIs are allowlisted and typed;
- Mini App bridges are origin/capability scoped;
- local exec is capability/permission gated;
- remote control exposes reviewed semantic capabilities rather than unrestricted credential/shell mutation;
- OAuth/WebAuthn secrets are absent from transcript/UI logs;
- external URL/deep-link policy is allowlisted;
- telemetry/logs scrub sensitive data.

### R16 — Provenance / rights

Because the reference repository states that no upstream source-code license is granted:

- use it as architecture/protocol/behavior evidence;
- do not claim reconstructed material is official source;
- do not bulk-copy implementation text without explicit rights review;
- record provenance for behavior-sensitive mappings;
- keep a release-blocking rights review for directly derived redistributed material.

Folder/module correspondence and independently implemented behavior parity are required; textual source copying is not.

### R17 — Legacy removal

After replacement paths pass acceptance:

- remove superseded orchestration from `MainActivity.kt`;
- split/remove monolithic responsibilities in `GrokMobileShellAndroid.kt` and `FabushiScreen.kt`;
- remove duplicate Host ownership/event pumps;
- remove presentation -> Host direct calls;
- remove temporary Coordinator bypasses;
- remove obsolete compatibility state/flags;
- migrate or delete the old `mobile/android` tree as responsibilities move into the Grok-corresponding root structure.

The task is not complete while the old architecture remains a production fallback.

### R18 — Standalone build guarantee

A clean checkout of `bhrumom/fabushi-android` at the exact implementation SHA must be able to build, test, package, and run the Android app without checking out another Fabushi source repository.

External third-party package dependencies are allowed. Cross-Fabushi source-repository runtime dependencies are not.

## 6. Current state

At the discovery baseline:

- the Android source is concentrated in `mobile/android/app`;
- `MainActivity.kt` directly coordinates Marketplace, Messaging, Bot, Mini App, update, remote-device, auth/deep-link and shell selection;
- `GrokMobileShellAndroid.kt` combines Compose UI and semantic agent-surface registration;
- `MobileBotViewModel.kt` calls `MahayanaHost` directly;
- `MahayanaHost.kt` owns a process-shared JNI handle and fans feature events to multiple consumers;
- `FabushiRemoteDeviceGateway.kt` owns its own WebSocket lifecycle and creates a `MahayanaHost`;
- Gradle currently presents a single app-centric tree rather than Grok-corresponding module roots.

This is functional legacy input, not the target architecture.

## 7. Target repository state

The repository root converges to:

```
fabushi-android/
├── frontend/
│   ├── build.gradle.kts
│   └── src/
│       ├── main/
│       ├── production/
│       └── recovered/          # only if useful as evidence-oriented Android reconstruction
├── source/
│   ├── android-dev-controls/
│   ├── android-main/
│   │   ├── account/
│   │   ├── adapters/
│   │   ├── attachments/
│   │   ├── auth/
│   │   ├── box/
│   │   ├── coordinator/
│   │   ├── deep-link/
│   │   ├── dev/
│   │   ├── downloads/
│   │   ├── experiments/
│   │   ├── feedback/
│   │   ├── generated/
│   │   ├── local-exec/
│   │   ├── mcp/
│   │   ├── media/
│   │   ├── models/
│   │   ├── notifications/
│   │   ├── prefs/
│   │   ├── process-metrics/
│   │   ├── secrets/
│   │   ├── startup/
│   │   ├── telemetry/
│   │   ├── update/
│   │   └── vnc/
│   ├── android-preload/
│   │   └── runtime/
│   ├── box-exec-daemon/
│   ├── host/
│   │   ├── agent-isolation/
│   │   ├── agents/
│   │   ├── automations/
│   │   ├── box/
│   │   ├── cloud-agents/
│   │   ├── connectors/
│   │   ├── extensions/
│   │   ├── groups/
│   │   ├── local-exec/
│   │   ├── mcp-auth/
│   │   ├── ports/
│   │   ├── runner/
│   │   ├── storage/
│   │   ├── transcript-mirror/
│   │   └── workflows/
│   ├── internal/
│   ├── local-exec-daemon/
│   ├── mahayana-agent-coordinator/
│   │   ├── gateway/
│   │   ├── local-exec/
│   │   ├── oauth/
│   │   ├── telemetry/
│   │   └── webauthn/
│   ├── packages/
│   │   └── <Grok-corresponding packages>
│   └── shared/
│       ├── agents/
│       ├── errors/
│       ├── media/
│       ├── node/               # only if semantics remain useful; otherwise Android-adapted with ledger reason
│       ├── observability/
│       └── rpc/
├── manifests/
├── scripts/
├── tests/
├── docs/
└── Gradle/Cargo/build assembly files
```

The exact Grok tree must be captured by the file-level ledger; this diagram is the minimum structural skeleton, not a substitute for the ledger.

### 7.1 Dependency direction

Allowed:

```
frontend
   ↓
android-preload
   ↓
android-main
   ↓
mahayana-agent-coordinator
   ↓
host
   ↓
local-exec-daemon / box-exec-daemon

source/shared and source/packages provide local contracts/libraries
to the layers that need them.
```

Forbidden:

- renderer -> Host direct;
- renderer -> Runner direct;
- Activity -> agent-domain mutation;
- Runner -> renderer;
- Host -> Activity/ViewModel;
- feature ViewModel -> independent native Host creation after cutover;
- dependency on another Fabushi platform repository for product runtime source;
- circular cross-layer ownership.

## 8. Interfaces / contracts / data flow

### 8.1 Coordinator envelope

Define an Android-local versioned contract equivalent in capability to Grok coordinator/RPC contracts:

```
Request {
  protocolVersion
  requestId
  sessionId
  method
  params
  deadline?
}

Reply {
  requestId
  ok
  result?
  error?
}

Event {
  eventId
  sessionId
  sequence
  type
  payload
}

Cancel {
  requestId | operationId
  reason?
}
```

The schema is stored in this repository under the corresponding `source/shared/**` / `source/packages/**` location.

### 8.2 Renderer

Compose consumes immutable projections and emits typed intents. Renderer code does not poll Host or parse arbitrary Host JSON.

### 8.3 Transcript ordering

Coordinator/Host define stable event IDs, sequencing, deduplication, mutation behavior and resync snapshots.

### 8.4 Execution flow

```
UI intent
 -> Android preload/bridge
 -> Android main
 -> Mahayana Coordinator
 -> Host
 -> Runner/tool/MCP
 -> Host event
 -> Coordinator ordered event
 -> Android main/bridge
 -> renderer projection
 -> Compose render
```

Cancellation must settle through the same ownership chain.

## 9. Constraints and non-functional requirements

- best Android effect is the primary implementation criterion;
- primary UI is native Compose;
- no runtime dependency on another Fabushi source repository;
- no long-running blocking JNI call on main thread;
- responses stream incrementally;
- background work respects Android lifecycle/restrictions;
- process recreation cannot duplicate commands or corrupt transcript;
- deterministic error codes replace UI parsing of arbitrary exception strings;
- idle animations must not waste battery/CPU;
- logs/evidence are privacy scrubbed;
- architecture is testable without production network access;
- folder/module parity is enforced by CI.

## 10. Failure modes and edge cases

Required coverage includes:

- Coordinator starts but Host fails;
- Host crashes mid-turn;
- Runner crashes/times out;
- MCP disconnect/reconnect;
- OAuth callback after Activity recreation;
- passkey cancellation;
- network loss during streaming;
- duplicate/out-of-order events;
- renderer reconnect after backgrounding;
- Android process death;
- native library load failure;
- stale native handle;
- multiple UI collectors;
- double send/cancel;
- attachment URI permission expiry;
- unavailable local-exec capability;
- remote gateway token/session rollover;
- Mini App WebView failure;
- storage full/read-only/corrupt;
- local contract/protocol version mismatch;
- migration incompatibility;
- update/restart during active operation.

Every path must end in a deterministic recoverable or terminal state.

## 11. Implementation strategy

### Phase 0 — Inventory and exact folder map

1. pin Grok and Android SHAs;
2. generate complete Grok file tree for `source/**` and `frontend/**`;
3. record target path(s), existing equivalent, or reviewed platform disposition for every Grok file; do not require one-to-one physical files;
4. create the root domain/boundary scaffolding matching Section 7;
5. record current Android files and planned destination/removal;
6. record provenance/rights classification;
7. define critical behavioral fixtures.

Exit gate: 100% source-file audit/disposition, zero unclassified responsibilities, and zero unexplained domain/boundary divergence. Equal source/target file counts are not required.\n\nCurrent evidence: source-file audit coverage is complete (2,046/2,046) and major target roots/scopes are scaffolded; implementation remains tracked independently per row as `mapped`, `implemented`, `verified`, or `blocked`.

### Phase 1 — Local contracts/packages

1. create Android-local `source/shared/**`;
2. create Android-local `source/packages/**`;
3. define coordinator request/reply/event/cancel contracts locally;
4. add Kotlin/Rust local bindings as needed;
5. add contract tests.

Exit gate: Android can build its contracts/packages from this repository alone.

### Phase 2 — Mahayana Coordinator

Implement the Grok coordinator counterpart in `source/mahayana-agent-coordinator/**`.

Exit gate: normal, streaming, tool, cancel, reconnect, duplicate, Host crash and resync state-machine tests pass.

### Phase 3 — Host + Runner

Implement/migrate `source/host/**`, `source/local-exec-daemon/**`, `source/box-exec-daemon/**`, and `source/internal/**`.

Exit gate: all domain execution flows Coordinator -> Host -> Runner/tool and returns ordered events.

### Phase 4 — Android main/preload

Build `source/android-main/**`, `source/android-preload/**`, and `source/android-dev-controls/**`. Move responsibilities out of `MainActivity`.

Exit gate: MainActivity is a thin platform entry point.

### Phase 5 — Frontend

Move/rebuild Compose renderer in root `frontend/**` following the Grok frontend responsibility layout and renderer behavior.

Exit gate: roster, conversation, composer, streaming states, command palette, settings, MCP/connectors, attachments, remote computer and recovery operate through Coordinator only.

### Phase 6 — Existing Fabushi features

Move Marketplace, Mini Apps, messaging, bot projection, account/entitlement, update, remote gateway, and other Android product features into the corresponding Grok-aligned modules.

Exit gate: no feature maintains a second runtime architecture.

### Phase 7 — Delete old architecture

Remove obsolete `mobile/android` source paths or reduce them only to transitional build forwarding until the root structure fully owns production. Remove all direct presentation-to-Host access and compatibility fallbacks.

Exit gate: architecture checker sees only the new module graph.

### Phase 8 — Exact-HEAD verification and package

Run compile, unit, contract, architecture, instrumentation, process-death, packaged install/upgrade, and release-candidate acceptance from the exact implementation SHA.

## 12. Verification / test strategy

### 12.1 Folder parity checker

CI must compare the pinned Grok module/file inventory to the parity ledger and Android target architecture.

Fail if:

- a Grok file/module is unclassified;
- a required product responsibility lacks a real target implementation/equivalent;
- a `not-applicable` row lacks Android platform rationale or required replacement behavior;
- target ownership/domain drifts from the agreed architecture without ledger rationale;
- a checker incorrectly requires equal source/target file counts;
- an old monolithic path regains responsibilities already cut over.

### 12.2 Architecture checker

Fail CI if:

- frontend imports Host/Runner implementation;
- ViewModel creates `MahayanaHost` directly after cutover;
- MainActivity owns product orchestration;
- Coordinator is bypassed;
- duplicate coordinator implementations exist;
- another Fabushi repository is required for runtime source/build;
- a legacy path is reintroduced.

### 12.3 Contract/state tests

Cover request/reply/event/cancel and Coordinator state machine:

- send -> stream -> complete;
- send -> tool -> result -> complete;
- cancel;
- reconnect/resync;
- duplicate event;
- Host crash/recovery;
- stale session/generation.

### 12.4 Android lifecycle tests

- configuration change;
- background/foreground;
- Activity recreation;
- process kill/relaunch;
- notification/deep link;
- OAuth callback;
- attachment picker;
- remote gateway reconnect.

### 12.5 UI/effect parity

Capture tests/evidence for:

- agent list/row actions;
- conversation;
- composer/send/stop;
- thinking/running/completed/failed/recovered;
- command palette;
- settings;
- MCP/connectors;
- reactions/groups;
- attachments/media;
- Mini App;
- remote computer;
- error/retry.

### 12.6 Packaged acceptance

Use an exact-HEAD packaged artifact. Validate fresh install, upgrade, launch, login, normal chat, streaming, stop, tool/MCP, background/recovery, process recreation, remote-device registration where applicable, logout, and relaunch.

For GitHub Actions device acceptance, the repository may define a dedicated `ciAcceptance` Android build type only when all of the following remain true:

- it runs the same production Compose → Typed Bridge → Mahayana Coordinator → Mahayana Host → Runner implementation as the shipping application;
- it is installable with CI/debug signing only for emulator or App-owned device evidence; signing mode must not change runtime ownership or behavior;
- it may enable only the bounded CI account-session import already specified by the security design: GitHub Actions provenance, run/attempt-bound device and session identities, short expiry, no refresh token, private-file import, and no transcript/UI logging of credentials;
- it must never enable `featureHostTest`, synthesize a logged-in state, mint a fake access token, bypass OAuth/WebAuthn/MCP, or replace the production Host/Coordinator with a deterministic test Host;
- authenticated packaged acceptance is mandatory evidence. If no valid short-lived account session is available, the authenticated cases are `blocked` and the workflow must not report those cases as passed merely because signed-out or unit/instrumentation tests passed.

The packaged acceptance workflow must preserve the exact source SHA, APK checksum, native JNI checksums, install/upgrade evidence, logs, screenshots/video where applicable, and per-scenario results.

## 13. Acceptance criteria / Definition of Done

- **AC-1**: 100% of pinned Grok `source/**` and `frontend/**` files exist in the parity ledger.
- **AC-2**: Every Grok source item has a reviewed disposition, and every product-relevant Grok responsibility has an Android-local production implementation or evidenced equivalent. Reviewed N/A applies only to genuinely inapplicable source mechanisms and cannot silently remove a required product effect.
- **AC-3**: The repository root physically follows the Grok-corresponding `frontend/source/tests/scripts/manifests/docs` structure.
- **AC-4**: `source/electron-main` responsibilities correspond to `source/android-main` submodules.
- **AC-5**: `source/electron-preload` responsibilities correspond to `source/android-preload`.
- **AC-6**: `source/node-agent-coordinator` responsibilities correspond to a first-class `source/mahayana-agent-coordinator`.
- **AC-7**: Host and Runner are independent boundaries and cannot be called directly by renderer code.
- **AC-8**: Android-local `source/shared/**` and `source/packages/**` cover all required reference responsibilities.
- **AC-9**: A clean checkout builds/runs without another Fabushi source repository.
- **AC-10**: MainActivity is a thin Android entry point, not product orchestrator.
- **AC-11**: `GrokMobileShellAndroid` / `FabushiScreen` monolithic runtime responsibilities are removed/split.
- **AC-12**: Product ViewModels do not directly construct/use Host runtime except explicitly approved bootstrap/binding ownership outside presentation.
- **AC-13**: Supported Grok-equivalent user flows stream and settle with equivalent behavior.
- **AC-14**: Process death/relaunch resyncs without duplicate sends, stuck turns, or leaked native handles.
- **AC-15**: MCP/connector discovery/auth/invocation/result/error work through Coordinator/Host.
- **AC-16**: Android-native attachments, media, deep links, OAuth/WebAuthn, notifications and remote-computer adapters work.
- **AC-17**: One canonical Android-local truth exists for auth, roster, transcript, operations, MCP, Mini Apps, remote-device state, settings and permissions.
- **AC-18**: Old production orchestration/fallback architecture is removed.
- **AC-19**: Folder and architecture checkers prevent regression.
- **AC-20**: Exact-HEAD CI passes required compile/unit/contract/architecture/UI/instrumentation checks.
- **AC-21**: Exact-HEAD packaged Android acceptance passes on fresh install and upgrade.
- **AC-22**: Rights/provenance review has no release-blocking unresolved item.
- **AC-23**: Final compliance table records every requirement/AC as `passed`, `blocked`, or `not-applicable`; mandatory completion requires all mandatory items `passed`.
- **AC-24 — Grok Bot Android effect**: Exact-HEAD packaged acceptance proves Fabushi Android delivers the same core Grok Bot Agent effect through native Android UI/lifecycle adapters: accepted → preparing/thinking → real tool/MCP/Runner activity when invoked → live tool state → continued inference → streaming transcript → terminal state, with Activity recreation/background/process-death recovery resynchronizing the same durable run where that run survives outside the UI process and with no silent task loss or duplicate execution.

## 14. Release / migration / rollback

Migration may use temporary cutover flags only while controlled transition requires both paths. All such flags and fallback paths must be removed before AC-18.

Persistent data migrations must be versioned and idempotent. Rollback must preserve account and transcript integrity and must not silently select a second state store.

Architecture-complete cannot be declared from source tests alone. Packaged Android acceptance is required.

## 15. Observability / evidence

Each phase must retain:

- file-level parity ledger;
- folder-parity checker output;
- architecture dependency report;
- contract/state-machine reports;
- lifecycle/process-death report;
- UI screenshots/video for critical flows;
- MCP/connector trace with secrets scrubbed;
- exact SHA;
- CI run IDs;
- packaged artifact identity/checksum;
- fresh install/upgrade acceptance;
- final Spec compliance table.

## 16. References / provenance

Primary Grok baseline:

- `b-nnett/grok-bot-0.18-reconstructed@a9f633e09d49a85829b8236331b9e21f7e612634`
- `README.md`
- `NOTICE.md`
- `PROVENANCE.md`
- `docs/ARCHITECTURE.md`
- `frontend/**`
- `source/electron-main/**`
- `source/electron-preload/**`
- `source/electron-dev-controls/**`
- `source/node-agent-coordinator/**`
- `source/host/**`
- `source/local-exec-daemon/**`
- `source/box-exec-daemon/**`
- `source/internal/**`
- `source/packages/**`
- `source/shared/**`
- `tests/**`
- `scripts/**`
- `manifests/**`

Fabushi Android discovery baseline:

- `bhrumom/fabushi-android@59f6fc8885ce1cb8d1ad4fc5d4ab36f690fb99a2`
- `AGENTS.md`
- `docs/specs/spec-first-ai-development.md`
- `mobile/android/app/src/main/java/com/ombhrum/fabushi/MainActivity.kt`
- `GrokMobileShellAndroid.kt`
- `FabushiScreen.kt`
- `MobileBotViewModel.kt`
- `MessagingViewModel.kt`
- `MarketplaceViewModel.kt`
- `FabushiAppAgentSurface.kt`
- `FabushiRemoteDeviceGateway.kt`
- `core/MahayanaHost.kt`
- `mobile/android/app/build.gradle`

## 17. Spec compliance record

| Requirement / AC | Status | Evidence / reason |
| --- | --- | --- |
| R1 | passed | 2,046/2,046 pinned Grok `source/**` + `frontend/**` files are captured in `manifests/grok-bot-0.18-android-parity-ledger.json`; phase-0 checker enforces exact inventory equality |
| R2 | pending | target folder mapping specified; physical migration pending |
| R3 | pending | implementation pending |
| R4 | pending | Coordinator implementation pending |
| R5 | pending | Host implementation pending |
| R6 | pending | Runner implementation pending |
| R7 | pending | Android trusted bridge pending |
| R8 | pending | Android-main decomposition pending |
| R9 | pending | frontend migration pending |
| R10 | pending | Android-local shared tree pending |
| R11 | pending | Android-local packages tree pending |
| R12 | pending | effect-parity acceptance pending |
| R13 | pending | canonical state audit pending |
| R14 | pending | process-death acceptance pending |
| R15 | pending | security review pending |
| R16 | blocked | Rights review found no upstream source-code license grant in the pinned reconstruction; direct source/binary reuse is prohibited pending independent authorization. Clean-room behavior/contract reimplementation continues. See `docs/reviews/grok-bot-0.18-rights-provenance-review.md`. |
| R17 | pending | legacy removal pending |
| R18 | pending | standalone build acceptance pending |
| AC-1 | passed | exact inventory: 1,724 `source/**` + 322 `frontend/**` = 2,046 rows; CI parity checker rejects omissions/duplicates |
| AC-2 | pending | implementation mapping pending |
| AC-3 | pending | physical root migration pending |
| AC-4 | pending | android-main parity pending |
| AC-5 | pending | android-preload parity pending |
| AC-6 | pending | Coordinator parity pending |
| AC-7 | pending | boundary enforcement pending |
| AC-8 | pending | local shared/packages parity pending |
| AC-9 | pending | standalone clean-checkout proof pending |
| AC-10 | pending | MainActivity still orchestrates current product |
| AC-11 | pending | current renderer files remain mixed |
| AC-12 | pending | current ViewModels still access Host directly |
| AC-13 | pending | behavioral acceptance pending |
| AC-14 | pending | process-death acceptance pending |
| AC-15 | pending | MCP/connector parity pending |
| AC-16 | pending | Android platform adapter parity pending |
| AC-17 | pending | state ownership audit pending |
| AC-18 | pending | old architecture still exists |
| AC-19 | pending | folder/architecture checker pending |
| AC-20 | pending | exact-HEAD CI pending |
| AC-21 | pending | packaged acceptance pending |
| AC-22 | blocked | Pinned reconstruction explicitly disclaims an upstream source-code license; release cannot mark provenance clearance passed without independent authorization/review. See `docs/reviews/grok-bot-0.18-rights-provenance-review.md`. |
| AC-23 | pending | final compliance review pending |
| AC-24 | pending | exact-HEAD packaged Android evidence has not yet proven the complete Grok Bot Android effect and same-run recovery semantics |

Allowed final statuses: `passed`, `blocked`, `not-applicable`.
