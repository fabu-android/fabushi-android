use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubagentState { Idle, Running, Completed, Failed, Cancelled }
#[derive(Default)]
pub struct SubagentStates { states: BTreeMap<String, SubagentState> }
impl SubagentStates {
    pub fn set(&mut self, id: impl Into<String>, state: SubagentState) { self.states.insert(id.into(), state); }
    pub fn get(&self, id: &str) -> Option<SubagentState> { self.states.get(id).copied() }
}
