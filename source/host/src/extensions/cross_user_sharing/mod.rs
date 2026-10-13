pub mod xuser_state_reconcile;
pub mod xuser_wire_normalization;

pub mod xuser_turn_dedupe_store;
pub mod xuser_pending_departure_store;
pub mod xuser_room_tombstone_store;
pub mod xuser_departure_obligations;
pub mod xuser_sharing_environment;

pub mod xuser_entry_publisher;
pub mod xuser_remote_turn_admission;
pub mod xuser_relay;
pub mod xuser_sharing_service;
// xuser_remote_turns is source-staged but intentionally not in the Android compile closure yet:
// it still depends on Desktop crate::groups prompt types. Export it only after the Android group
// contract/adapter owns those semantics; do not add a second canonical group state owner here.

// Android lifecycle replacement for Desktop experiments/NotifyBus wiring.
pub mod extension;
