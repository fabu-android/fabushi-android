#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientToolUpdateKind {
    Reset,
    Call,
    Result,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientToolTransportUpdate {
    pub account_slot: String,
    pub agent_id: String,
    pub epoch: String,
    pub sequence: u64,
    pub tool_call_id: Option<String>,
    pub kind: ClientToolUpdateKind,
    pub payload: Vec<u8>,
}

impl ClientToolTransportUpdate {
    pub fn validate(&self) -> bool {
        !self.account_slot.is_empty()
            && !self.agent_id.is_empty()
            && !self.epoch.is_empty()
            && self.sequence > 0
            && match self.kind {
                ClientToolUpdateKind::Reset => self.tool_call_id.is_none(),
                ClientToolUpdateKind::Call | ClientToolUpdateKind::Result => {
                    self.tool_call_id.as_ref().is_some_and(|id| !id.is_empty())
                }
            }
    }
}
