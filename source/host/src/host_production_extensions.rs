use std::collections::BTreeMap;

#[derive(Default)]
pub struct ProductionExtensionRegistry { extensions:BTreeMap<String,String> }

impl ProductionExtensionRegistry {
    pub fn bind(&mut self,id:impl Into<String>,version:impl Into<String>)->Result<(),&'static str>{
        let id=id.into(); if id.trim().is_empty(){return Err("extension id is empty");}
        if self.extensions.insert(id,version.into()).is_some(){return Err("extension already bound");}
        Ok(())
    }
    pub fn version(&self,id:&str)->Option<&str>{self.extensions.get(id).map(String::as_str)}
}
