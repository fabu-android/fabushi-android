use std::collections::BTreeMap;

#[derive(Clone,Debug,PartialEq,Eq)]
pub struct TraceSpan{pub name:String,pub started_at_ms:u64,pub ended_at_ms:Option<u64>,pub attributes:BTreeMap<String,String>,pub error:Option<String>}
impl TraceSpan{
    pub fn new(name:impl Into<String>,started_at_ms:u64)->Self{Self{name:name.into(),started_at_ms,ended_at_ms:None,attributes:BTreeMap::new(),error:None}}
    pub fn set_attribute(&mut self,key:impl Into<String>,value:impl Into<String>){self.attributes.insert(key.into(),value.into());}
    pub fn mark_error(&mut self,message:impl Into<String>){self.error=Some(message.into());}
    pub fn finish(&mut self,ended_at_ms:u64){self.ended_at_ms=Some(ended_at_ms.max(self.started_at_ms));}
}
pub fn begin_send_trace(request_id:&str,now_ms:u64)->TraceSpan{let mut span=TraceSpan::new("send",now_ms);span.set_attribute("request.id",request_id);span}
