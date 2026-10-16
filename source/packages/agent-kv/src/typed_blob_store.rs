use crate::{blob_not_found_error::BlobNotFoundError, blob_store::BlobStore, reference::BlobRef};

pub trait BlobCodec<T> {
    fn encode(&self, value: &T) -> Result<Vec<u8>, String>;
    fn decode(&self, bytes: &[u8]) -> Result<T, String>;
}

pub struct TypedBlobStore<C> { store: BlobStore, codec: C }

impl<C> TypedBlobStore<C> {
    pub fn new(store: BlobStore, codec: C) -> Self { Self { store, codec } }
}

impl<C> TypedBlobStore<C> {
    pub fn put<T>(&mut self, value: &T) -> Result<BlobRef, String> where C: BlobCodec<T> {
        Ok(self.store.put(self.codec.encode(value)?))
    }
    pub fn get<T>(&self, reference: &BlobRef) -> Result<T, String> where C: BlobCodec<T> {
        let bytes = self.store.get(reference).map_err(|BlobNotFoundError(message)| message)?;
        self.codec.decode(bytes)
    }
}
