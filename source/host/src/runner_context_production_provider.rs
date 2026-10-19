use crate::host_request_context::HostRequestContext;
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct ProductionRunnerContext{pub host:HostRequestContext,pub model:String}
pub fn create_production_runner_context(host:HostRequestContext,model:impl Into<String>)->Result<ProductionRunnerContext,&'static str>{
    let model=model.into();if model.trim().is_empty(){return Err("model is required");}Ok(ProductionRunnerContext{host,model})
}
