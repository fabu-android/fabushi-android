use std::collections::{BTreeMap, BTreeSet};

pub fn host_capabilities() -> BTreeSet<&'static str> {
    ["agents","transcript","mcp","automations","workflows","sharing","box","media","trays"].into_iter().collect()
}

pub struct DynamicGatewayApi {
    handlers: BTreeMap<String, Box<dyn Fn(&str)->Result<String,String> + Send + Sync>>,
}

impl DynamicGatewayApi {
    pub fn new() -> Self { Self { handlers:BTreeMap::new() } }
    pub fn register(&mut self, method: impl Into<String>, handler: Box<dyn Fn(&str)->Result<String,String> + Send + Sync>) -> Result<(), &'static str> {
        let method=method.into();
        if method.trim().is_empty() { return Err("gateway method is empty"); }
        if self.handlers.insert(method,handler).is_some() { return Err("duplicate gateway method"); }
        Ok(())
    }
    pub fn call(&self, method:&str, payload:&str)->Result<String,String> {
        self.handlers.get(method).ok_or_else(||"unknown-method".to_string())?(payload)
    }
}
impl Default for DynamicGatewayApi { fn default()->Self{Self::new()} }
