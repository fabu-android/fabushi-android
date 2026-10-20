use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InferenceProvider { Cursor, Codex, ClaudeCode, OpenRouter }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoutedTurn {
    pub id: String,
    pub role: String,
    pub content: String,
}

pub struct InferenceRouter {
    provider: InferenceProvider,
    transcripts: BTreeMap<String, VecDeque<RoutedTurn>>,
    max_entries_per_agent: usize,
}

impl InferenceRouter {
    pub fn new(provider: InferenceProvider) -> Self {
        Self { provider, transcripts:BTreeMap::new(), max_entries_per_agent:200 }
    }
    pub fn provider(&self) -> InferenceProvider { self.provider }
    pub fn set_provider(&mut self, provider: InferenceProvider) { self.provider=provider; }
    pub fn append(&mut self, agent_id: &str, turn: RoutedTurn) -> Result<(), &'static str> {
        if agent_id.trim().is_empty() || turn.id.trim().is_empty() || turn.content.is_empty() { return Err("agent, turn id, and content are required"); }
        let rows=self.transcripts.entry(agent_id.into()).or_default();
        if rows.len() >= self.max_entries_per_agent { rows.pop_front(); }
        rows.push_back(turn);
        Ok(())
    }
    pub fn tail(&self, agent_id: &str, limit: usize) -> Vec<RoutedTurn> {
        let Some(rows)=self.transcripts.get(agent_id) else { return vec![]; };
        rows.iter().skip(rows.len().saturating_sub(limit)).cloned().collect()
    }
}
