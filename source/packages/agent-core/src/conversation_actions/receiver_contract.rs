#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReceiverCommand {
    Send { request_id: String, payload: String },
    Cancel { request_id: String },
    Resync { generation: u64, after_sequence: u64 },
}

impl ReceiverCommand {
    pub fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::Send { request_id, .. } | Self::Cancel { request_id } if request_id.trim().is_empty() => {
                Err("request_id is required")
            }
            Self::Resync { generation, .. } if *generation == 0 => Err("generation must be non-zero"),
            _ => Ok(()),
        }
    }
}
