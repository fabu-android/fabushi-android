use crate::classification::DataClassification;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreMessage {
    pub role: String,
    pub content: String,
    pub classification: DataClassification,
}
