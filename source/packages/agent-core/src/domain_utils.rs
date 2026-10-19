#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AgentId(String);

impl AgentId {
    pub fn parse(value: impl Into<String>) -> Result<Self, &'static str> {
        let value = value.into();
        if value.is_empty() || value.len() > 200 {
            return Err("agent id length is invalid");
        }
        if !value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._:/@-".contains(&byte)) {
            return Err("agent id contains unsupported characters");
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
