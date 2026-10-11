#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlobRef(String);

impl BlobRef {
    pub fn parse(value: impl Into<String>) -> Result<Self, &'static str> {
        let value = value.into();
        if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) { return Err("blob reference must be 64 hex chars"); }
        Ok(Self(value.to_ascii_lowercase()))
    }
    pub fn as_str(&self) -> &str { &self.0 }
}
