use super::conversation_blob_store::ConversationBlobWorkerBackend;

pub type ProductionAgentStoreWorkerBackend = ConversationBlobWorkerBackend;

pub fn create_production_agent_store_worker_backend(
    busy_timeout_ms: u64,
) -> ProductionAgentStoreWorkerBackend {
    ConversationBlobWorkerBackend::new(busy_timeout_ms)
}
