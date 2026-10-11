# Host parity reconciliation

Baseline: `b-nnett/grok-bot-0.18-reconstructed@a9f633e09d49a85829b8236331b9e21f7e612634`
Fabushi reconciliation parent: `1ab69063dbd38eb44541fadede09393ad631004f`

These rows already had non-placeholder Rust target files and were exported by `source/host/src/lib.rs`.
They are promoted only to `implemented`, not `verified`; exact-HEAD CI remains required.

Reconciled rows: 34

- `source/host/attachment-paths.ts` → `source/host/src/attachment_paths.rs`
- `source/host/durable-file-policy.ts` → `source/host/src/durable_file_policy.rs`
- `source/host/gateway-command-error.ts` → `source/host/src/gateway_command_error.rs`
- `source/host/gateway-config.ts` → `source/host/src/gateway_config.rs`
- `source/host/gateway-protocol.ts` → `source/host/src/gateway_protocol.rs`
- `source/host/gateway-server.ts` → `source/host/src/gateway_server.rs`
- `source/host/host-diagnostics.ts` → `source/host/src/host_diagnostics.rs`
- `source/host/host-discovery.ts` → `source/host/src/host_discovery.rs`
- `source/host/host-event-bus.ts` → `source/host/src/host_event_bus.rs`
- `source/host/host-gateway-api.ts` → `source/host/src/host_gateway_api.rs`
- `source/host/host-initial-transcript-load.ts` → `source/host/src/host_initial_transcript_load.rs`
- `source/host/host-lock.ts` → `source/host/src/host_lock.rs`
- `source/host/host-paths.ts` → `source/host/src/host_paths.rs`
- `source/host/host-production-extensions.ts` → `source/host/src/host_production_extensions.rs`
- `source/host/host-request-context.ts` → `source/host/src/host_request_context.rs`
- `source/host/host-roster-bookkeeping.ts` → `source/host/src/host_roster_bookkeeping.rs`
- `source/host/host-runner-composition.ts` → `source/host/src/host_runner_composition.rs`
- `source/host/host-secret-store.ts` → `source/host/src/host_secret_store.rs`
- `source/host/main.ts` → `source/host/src/main.rs`
- `source/host/notify-drain-gate.ts` → `source/host/src/notify_drain_gate.rs`
- `source/host/process-crash-guard.ts` → `source/host/src/process_crash_guard.rs`
- `source/host/production-binding-providers.ts` → `source/host/src/production_binding_providers.rs`
- `source/host/runner-context-production-provider.ts` → `source/host/src/runner_context_production_provider.rs`
- `source/host/runner-production-bridge.ts` → `source/host/src/runner_production_bridge.rs`
- `source/host/sand-activity.ts` → `source/host/src/sand_activity.rs`
- `source/host/sand-host.ts` → `source/host/src/sand_host.rs`
- `source/host/sand-multitask.ts` → `source/host/src/sand_multitask.rs`
- `source/host/sand-quiet-work-origin.ts` → `source/host/src/sand_quiet_work_origin.rs`
- `source/host/sand-user-identity.ts` → `source/host/src/sand_user_identity.rs`
- `source/host/selected-image-inputs.ts` → `source/host/src/selected_image_inputs.rs`
- `source/host/send-trace-host.ts` → `source/host/src/send_trace_host.rs`
- `source/host/sha256.ts` → `source/host/src/sha256.rs`
- `source/host/transcript-mutation-events.ts` → `source/host/src/transcript_mutation_events.rs`
- `source/host/watched-directory.ts` → `source/host/src/watched_directory.rs`
