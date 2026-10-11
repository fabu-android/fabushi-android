use fabushi_android_shared::{ExecutionError,ExecutionRequest,ExecutionResult};
pub trait ProductionRunnerRunStep{fn run_step(&mut self,input:&ExecutionRequest)->Result<ExecutionResult,ExecutionError>;}
pub struct ProductionRunnerBridge<R:ProductionRunnerRunStep>{runner:R}
impl<R:ProductionRunnerRunStep> ProductionRunnerBridge<R>{
    pub fn new(runner:R)->Self{Self{runner}}
    pub fn execute(&mut self,input:&ExecutionRequest)->Result<ExecutionResult,ExecutionError>{input.validate()?;self.runner.run_step(input)}
}
