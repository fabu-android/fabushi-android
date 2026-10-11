#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteRunnerState { Disconnected, Connecting, Ready { generation: u64 }, Failed }

#[derive(Clone, Debug)]
pub struct RemoteRunnerLifecycle { generation:u64, state:RemoteRunnerState }

impl Default for RemoteRunnerLifecycle { fn default()->Self{Self{generation:0,state:RemoteRunnerState::Disconnected}} }

impl RemoteRunnerLifecycle {
    pub fn begin_connect(&mut self)->Result<(),&'static str>{
        if matches!(self.state,RemoteRunnerState::Connecting){return Err("connection already in progress");}
        self.state=RemoteRunnerState::Connecting; Ok(())
    }
    pub fn connected(&mut self)->u64{
        self.generation=self.generation.saturating_add(1).max(1);
        self.state=RemoteRunnerState::Ready{generation:self.generation}; self.generation
    }
    pub fn failed(&mut self){self.state=RemoteRunnerState::Failed;}
    pub fn state(&self)->RemoteRunnerState{self.state}
}
