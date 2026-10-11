//! Android-owned contracts shared across renderer bridge, Coordinator, Host, and Runners.
//! Clean-room implementation from the Fabushi Android Spec; no reconstructed Grok source is copied.

pub mod coordinator;
pub mod agents;
pub mod execution;
pub mod rpc;

pub use coordinator::{
    CancelRequest, CoordinatorEvent, CoordinatorFailure, CoordinatorFailureCode, CoordinatorReply,
    CoordinatorRequest, ResyncRequest, ResyncSnapshot, COORDINATOR_PROTOCOL_VERSION,
};
pub use execution::{
    ExecutionCapability, ExecutionError, ExecutionRequest, ExecutionResult, ExecutionTarget,
};

pub mod auth;
pub mod gateway_wire;
pub mod media;

pub mod deep_link;
pub mod gateway_reachability;
pub mod webauthn_gateway;
pub mod os_notification;
pub mod mcp;
pub mod mcp_oauth_callback_page;
pub mod webauthn_proxy_availability;

pub mod node;

pub mod env_file;
pub mod vnc_viewer_visibility;
pub mod product_name;
pub mod host_settings;
pub mod message_reference;
pub mod send_acceptance;
pub mod ordering;
pub mod transport_types;
pub mod box_runtime;
pub mod persistence;
pub mod update;
pub mod feedback;
pub mod retry_after;
