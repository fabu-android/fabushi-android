use std::fmt;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobNotFoundError(pub String);
impl fmt::Display for BlobNotFoundError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "blob not found: {}", self.0) }
}
impl std::error::Error for BlobNotFoundError {}
