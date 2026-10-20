pub mod agent_store;
pub mod blob_not_found_error;
pub mod blob_store;
pub mod reference;
pub mod serde;
pub mod subagent_states;
pub mod typed_blob_store;
#[cfg(test)]mod tests{use crate::{agent_store::{AgentMetadata,AgentMetadataSerde,AgentMode},blob_store::BlobStore,serde::Utf8Codec,typed_blob_store::TypedBlobStore};#[test]fn typed_blob_and_metadata_roundtrip(){let mut s=TypedBlobStore::new(BlobStore::default(),Utf8Codec);let r=s.put(&"hello".to_string()).unwrap();assert_eq!(s.get::<String>(&r).unwrap(),"hello");let serde=AgentMetadataSerde;let mut m=AgentMetadata::default_for("agent-1");m.mode=AgentMode::Plan;m.latest_root_blob_id=vec![1,2,3];let d=serde.deserialize(&serde.serialize(&m).unwrap()).unwrap();assert_eq!(d.agent_id,"agent-1");assert_eq!(d.mode,AgentMode::Plan);assert_eq!(d.latest_root_blob_id,vec![1,2,3]);}}
