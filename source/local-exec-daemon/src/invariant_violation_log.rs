use std::collections::VecDeque;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvariantViolation { pub code: String, pub detail: String }

pub struct InvariantViolationLog { entries: VecDeque<InvariantViolation>, limit: usize }

impl InvariantViolationLog {
    pub fn new(limit: usize) -> Self { Self { entries: VecDeque::new(), limit: limit.max(1) } }
    pub fn record(&mut self, code: impl Into<String>, detail: impl Into<String>) {
        if self.entries.len() >= self.limit { self.entries.pop_front(); }
        self.entries.push_back(InvariantViolation { code: code.into(), detail: detail.into() });
    }
    pub fn entries(&self) -> Vec<InvariantViolation> { self.entries.iter().cloned().collect() }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn bounded_log_drops_oldest() {
        let mut log=InvariantViolationLog::new(2);
        log.record("a","1"); log.record("b","2"); log.record("c","3");
        assert_eq!(log.entries().iter().map(|e|e.code.as_str()).collect::<Vec<_>>(),vec!["b","c"]);
    }
}
