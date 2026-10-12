use std::collections::BTreeMap;
use crate::{blob_not_found_error::BlobNotFoundError, reference::BlobRef};

#[derive(Default)]
pub struct BlobStore { blobs: BTreeMap<BlobRef, Vec<u8>>, counter: u64 }

impl BlobStore {
    pub fn put(&mut self, bytes: Vec<u8>) -> BlobRef {
        self.counter = self.counter.saturating_add(1);
        let mut seed = self.counter;
        for byte in &bytes { seed = seed.wrapping_mul(1099511628211).wrapping_add(*byte as u64); }
        let raw = format!("{seed:016x}").repeat(4);
        let reference = BlobRef::parse(raw).expect("generated reference is valid");
        self.blobs.insert(reference.clone(), bytes);
        reference
    }
    pub fn get(&self, reference: &BlobRef) -> Result<&[u8], BlobNotFoundError> {
        self.blobs.get(reference).map(Vec::as_slice).ok_or_else(|| BlobNotFoundError(reference.as_str().into()))
    }
}
