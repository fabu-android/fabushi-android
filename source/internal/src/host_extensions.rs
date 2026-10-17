use std::collections::BTreeSet;
#[derive(Default)]
pub struct HostExtensionRegistry { ids:BTreeSet<String> }
impl HostExtensionRegistry {
    pub fn register(&mut self,id:impl Into<String>)->Result<(),&'static str>{
        let id=id.into();
        if id.trim().is_empty(){return Err("extension id is required");}
        if !self.ids.insert(id){return Err("extension already registered");}
        Ok(())
    }
    pub fn contains(&self,id:&str)->bool{self.ids.contains(id)}
    pub fn len(&self)->usize{self.ids.len()}
}
