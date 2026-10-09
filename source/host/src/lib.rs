//! Android-owned Mahayana Host boundary.
//! Host owns domain execution and durable runtime services behind the Coordinator.
//! No Compose/Activity/ViewModel dependency is permitted here.
#![allow(special_module_name)]

pub mod android_agent_roster;
pub mod android_json_runtime;
pub mod capability_broker;
pub mod account_service;

pub mod attachment_paths;
pub mod durable_file_policy;
pub mod extensions;
pub mod gateway_command_error;
pub mod gateway_config;
pub mod gateway_protocol;
pub mod gateway_server;
pub mod host_diagnostics;
pub mod host_discovery;
pub mod host_event_bus;
pub mod host_gateway_api;
pub mod host_initial_transcript_load;
pub mod host_lock;
pub mod messaging_blob;
pub mod messaging_child;
pub mod messaging_service;
pub mod host_paths;
pub mod host_production_extensions;
pub mod host_request_context;
pub mod host_roster_bookkeeping;
pub mod host_runner_composition;
pub mod host_secret_store;
#[allow(special_module_name)]
pub mod main;
pub mod mcp_auth;
pub mod notify_drain_gate;
pub mod process_crash_guard;
pub mod production_binding_providers;
pub mod runner;
pub mod runner_context_production_provider;
pub mod runner_production_bridge;
pub mod sand_activity;
pub mod sand_host;
pub mod sand_multitask;
pub mod sand_quiet_work_origin;
pub mod sand_user_identity;
pub mod selected_image_inputs;
pub mod send_trace_host;
pub mod sha256;
pub mod transcript_mutation_events;
pub mod watched_directory;

pub use host_event_bus::{HostEvent, HostEventBus};
pub use sand_host::{HostRuntime, SandHost, SandHostHealth};
