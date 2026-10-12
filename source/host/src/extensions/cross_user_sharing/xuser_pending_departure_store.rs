use std::collections::BTreeMap;
use std::fs;
use std::path::{Path,PathBuf};
use std::sync::Mutex;
use serde::{Serialize,Deserialize};
pub const SAND_XUSER_PENDING_DEPARTURE_FILE_NAME:&str="host-xuser-pending-departures.json";
#[derive(Debug,Clone,PartialEq,Eq,Serialize,Deserialize)]
#[serde(tag="kind",rename_all="kebab-case")]
pub enum XuserDeparture{
 #[serde(rename="leave-room")] LeaveRoom{#[serde(rename="ownerAuthId",skip_serializing_if="Option::is_none")]owner_auth_id:Option<String>,#[serde(rename="roomId")]room_id:String,#[serde(rename="agentId")]agent_id:String},
 #[serde(rename="remove-agent")] RemoveAgent{#[serde(rename="ownerAuthId",skip_serializing_if="Option::is_none")]owner_auth_id:Option<String>,#[serde(rename="agentId")]agent_id:String},
}
pub fn departure_key(v:&XuserDeparture)->String{match v{
 XuserDeparture::LeaveRoom{owner_auth_id,room_id,..}=>format!("leave-room\0{}\0{}",owner_auth_id.as_deref().unwrap_or(""),room_id),
 XuserDeparture::RemoveAgent{owner_auth_id,agent_id}=>format!("remove-agent\0{}\0{}",owner_auth_id.as_deref().unwrap_or(""),agent_id),
}}
pub fn parse_departure_file(raw:Option<&str>)->BTreeMap<String,XuserDeparture>{
 let mut map=BTreeMap::new();let Some(raw)=raw else{return map};let Ok(value)=serde_json::from_str::<serde_json::Value>(raw) else{return map};let Some(items)=value.get("pending").and_then(|v|v.as_array()) else{return map};
 for item in items{
  let Some(agent)=item.get("agentId").and_then(|v|v.as_str()).filter(|v|!v.is_empty()) else{continue};let owner=item.get("ownerAuthId").and_then(|v|v.as_str()).filter(|v|!v.is_empty()).map(str::to_string);
  let dep=match item.get("kind").and_then(|v|v.as_str()){Some("leave-room")=>{let Some(room)=item.get("roomId").and_then(|v|v.as_str()).filter(|v|!v.is_empty()) else{continue};XuserDeparture::LeaveRoom{owner_auth_id:owner,room_id:room.into(),agent_id:agent.into()}},Some("remove-agent")=>XuserDeparture::RemoveAgent{owner_auth_id:owner,agent_id:agent.into()},_=>continue};
  map.insert(departure_key(&dep),dep);
 }map
}
pub struct SandXuserPendingDepartureStore{pub file_path:PathBuf,cache:Mutex<Option<BTreeMap<String,XuserDeparture>>>}
impl SandXuserPendingDepartureStore{
 pub fn new(root:&Path)->Self{Self{file_path:root.join(SAND_XUSER_PENDING_DEPARTURE_FILE_NAME),cache:Mutex::new(None)}}
 fn with_map<R>(&self,f:impl FnOnce(&mut BTreeMap<String,XuserDeparture>)->(R,bool))->R{let mut g=self.cache.lock().unwrap_or_else(|p|p.into_inner());if g.is_none(){*g=Some(parse_departure_file(fs::read_to_string(&self.file_path).ok().as_deref()));}let m=g.as_mut().unwrap();let(out,changed)=f(m);if changed{if let Some(p)=self.file_path.parent(){let _=fs::create_dir_all(p);}let part=PathBuf::from(format!("{}.part",self.file_path.display()));let vals=m.values().cloned().collect::<Vec<_>>();if fs::write(&part,serde_json::json!({"version":2,"pending":vals}).to_string()).is_ok(){let _=fs::rename(part,&self.file_path);}}out}
 pub fn list(&self)->Vec<XuserDeparture>{self.with_map(|m|(m.values().cloned().collect(),false))}
 pub fn record(&self,v:XuserDeparture){self.with_map(|m|{let k=departure_key(&v);if m.contains_key(&k){((),false)}else{m.insert(k,v);((),true)}})}
 pub fn clear(&self,v:&XuserDeparture){let k=departure_key(v);self.with_map(|m|((),m.remove(&k).is_some()))}
}
