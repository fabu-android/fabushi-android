use crate::typed_blob_store::BlobCodec;
#[derive(Clone, Copy, Debug, Default)]
pub struct Utf8Codec;
impl BlobCodec<String> for Utf8Codec {
    fn encode(&self, value: &String) -> Result<Vec<u8>, String> { Ok(value.as_bytes().to_vec()) }
    fn decode(&self, bytes: &[u8]) -> Result<String, String> { String::from_utf8(bytes.to_vec()).map_err(|_| "invalid utf-8".into()) }
}
