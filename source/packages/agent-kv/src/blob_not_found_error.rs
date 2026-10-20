use std::fmt;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobNotFoundError(pub String);
impl fmt::Display for BlobNotFoundError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "blob not found: {}", self.0) }
}
impl std::error::Error for BlobNotFoundError {}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_missing_blob_context() {
        let error = BlobNotFoundError("sha256:missing".to_owned());
        assert_eq!(error.to_string(), "blob not found: sha256:missing");
        assert_eq!(error.0, "sha256:missing");
    }
}
