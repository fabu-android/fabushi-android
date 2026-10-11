use fabushi_android_shared::{CoordinatorFailure,CoordinatorRequest};

#[derive(Clone,Debug,PartialEq,Eq)]
pub struct SandHostHealth{pub ready:bool,pub active_requests:usize,pub generation:u64}

pub trait HostRuntime {
    fn execute(&mut self,request:&CoordinatorRequest)->Result<String,CoordinatorFailure>;
    fn cancel(&mut self,request_id:&str,reason:Option<&str>)->Result<(),CoordinatorFailure>;
}

pub struct SandHost{generation:u64,active:usize,ready:bool}
impl SandHost{
    pub fn new()->Self{Self{generation:1,active:0,ready:true}}
    pub fn health(&self)->SandHostHealth{SandHostHealth{ready:self.ready,active_requests:self.active,generation:self.generation}}
    pub fn restart_generation(&mut self){self.generation=self.generation.saturating_add(1);self.active=0;self.ready=true;}
    pub fn set_ready(&mut self,ready:bool){self.ready=ready;}
}
impl Default for SandHost{fn default()->Self{Self::new()}}
