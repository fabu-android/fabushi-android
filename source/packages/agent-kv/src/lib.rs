pub mod agent_store;
pub mod blob_not_found_error;
pub mod blob_store;
pub mod reference;
pub mod serde;
pub mod subagent_states;
pub mod typed_blob_store;

#[cfg(test)]
mod tests {
    use super::{agent_store::AgentStore, blob_store::BlobStore, reference::BlobRef, serde::Utf8Codec, typed_blob_store::TypedBlobStore};
    #[test]
    fn typed_blob_and_agent_reference_roundtrip() {
        let mut store = TypedBlobStore::new(BlobStore::default(), Utf8Codec);
        let reference = store.put(&"hello".to_string()).unwrap();
        assert_eq!(store.get::<String>(&reference).unwrap(), "hello");
        let mut agents = AgentStore::default();
        agents.set("agent-1", reference.clone()).unwrap();
        assert_eq!(agents.get("agent-1"), Some(&reference));
        assert!(BlobRef::parse("bad").is_err());
    }
}
