use std::collections::VecDeque;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportStage {
    pub request_id: String,
    pub stage: String,
    pub monotonic_ms: u64,
}

pub struct TransportStageRecorder {
    rows: VecDeque<TransportStage>,
    limit: usize,
}

impl TransportStageRecorder {
    pub fn new(limit: usize) -> Self { Self { rows:VecDeque::new(), limit:limit.max(1) } }
    pub fn record(&mut self, row: TransportStage) {
        if row.request_id.is_empty() || row.stage.is_empty() { return; }
        if self.rows.len() >= self.limit { self.rows.pop_front(); }
        self.rows.push_back(row);
    }
    pub fn rows(&self) -> Vec<TransportStage> { self.rows.iter().cloned().collect() }
}
