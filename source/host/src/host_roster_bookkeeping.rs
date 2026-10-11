use std::collections::{BTreeMap,BTreeSet};

#[derive(Default)]
pub struct HostRosterBookkeeping {
    attachments:BTreeMap<String,BTreeSet<String>>,
    transcripts:BTreeSet<String>,
    boxes:BTreeSet<String>,
}
impl HostRosterBookkeeping {
    pub fn record_attachment(&mut self,agent:&str,id:&str){self.attachments.entry(agent.into()).or_default().insert(id.into());}
    pub fn record_transcript(&mut self,agent:&str){self.transcripts.insert(agent.into());}
    pub fn record_box(&mut self,agent:&str){self.boxes.insert(agent.into());}
    pub fn remove_agent(&mut self,agent:&str){self.attachments.remove(agent);self.transcripts.remove(agent);self.boxes.remove(agent);}
    pub fn has_agent_state(&self,agent:&str)->bool{self.attachments.contains_key(agent)||self.transcripts.contains(agent)||self.boxes.contains(agent)}
}
