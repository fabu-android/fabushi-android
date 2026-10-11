#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalRunnerState { Stopped, Running { generation: u64 }, Draining { generation: u64 } }

#[derive(Clone, Debug)]
pub struct LocalRunnerLifecycle { state: LocalRunnerState, generation: u64 }

impl Default for LocalRunnerLifecycle { fn default() -> Self { Self { state: LocalRunnerState::Stopped, generation: 0 } } }

impl LocalRunnerLifecycle {
    pub fn start(&mut self) -> Result<u64, &'static str> {
        if !matches!(self.state, LocalRunnerState::Stopped) { return Err("runner already active"); }
        self.generation=self.generation.saturating_add(1).max(1);
        self.state=LocalRunnerState::Running { generation:self.generation };
        Ok(self.generation)
    }
    pub fn begin_drain(&mut self) -> Result<(), &'static str> {
        let LocalRunnerState::Running { generation }=self.state else { return Err("runner is not running"); };
        self.state=LocalRunnerState::Draining { generation };
        Ok(())
    }
    pub fn stop(&mut self) { self.state=LocalRunnerState::Stopped; }
    pub fn state(&self) -> LocalRunnerState { self.state }
}
