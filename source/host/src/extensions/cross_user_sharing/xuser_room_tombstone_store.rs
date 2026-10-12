use std::collections::BTreeMap;
use std::fs;
use std::path::{Path,PathBuf};
use std::sync::Mutex;
use serde::{Serialize,Deserialize};

pub const SAND_XUSER_ROOM_TOMBSTONE_FILE_NAME:&str="host-xuser-room-tombstones.json";
#[derive(Debug,Clone,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct XuserRoomTombstone{pub room_id:String,#[serde(skip_serializing_if="Option::is_none")]pub owner_auth_id:Option<String>,pub torn_down_at_ms:u64}
pub fn tombstone_key(v:&XuserRoomTombstone)->String{format!("{}\0{}",v.owner_auth_id.as_deref().unwrap_or(""),v.room_id)}
pub fn parse_tombstone_file(raw:Option<&str>)->BTreeMap<String,XuserRoomTombstone>{
 let mut map=BTreeMap::new(); let Some(raw)=raw else{return map}; let Ok(value)=serde_json::from_str::<serde_json::Value>(raw) else{return map};
 let Some(items)=value.get("tombstones").and_then(|v|v.as_array()) else{return map};
 for item in items{
  let Some(room)=item.get("roomId").and_then(|v|v.as_str()).filter(|v|!v.is_empty()) else{continue};
  let owner=item.get("ownerAuthId").and_then(|v|v.as_str()).filter(|v|!v.is_empty()).map(str::to_string);
  let t=XuserRoomTombstone{room_id:room.into(),owner_auth_id:owner,torn_down_at_ms:item.get("tornDownAtMs").and_then(|v|v.as_u64()).unwrap_or(0)};
  map.insert(tombstone_key(&t),t);
 } map
}
pub struct SandXuserRoomTombstoneStore{pub file_path:PathBuf,cache:Mutex<Option<BTreeMap<String,XuserRoomTombstone>>>}
impl SandXuserRoomTombstoneStore{
 pub fn new(root:&Path)->Self{Self{file_path:root.join(SAND_XUSER_ROOM_TOMBSTONE_FILE_NAME),cache:Mutex::new(None)}}
 fn with_map<R>(&self,f:impl FnOnce(&mut BTreeMap<String,XuserRoomTombstone>)->(R,bool))->R{
  let mut g=self.cache.lock().unwrap_or_else(|p|p.into_inner()); if g.is_none(){*g=Some(parse_tombstone_file(fs::read_to_string(&self.file_path).ok().as_deref()));}
  let map=g.as_mut().unwrap(); let (out,changed)=f(map); if changed{if let Some(p)=self.file_path.parent(){let _=fs::create_dir_all(p);};let part=PathBuf::from(format!("{}.part",self.file_path.display()));let values=map.values().cloned().collect::<Vec<_>>();if fs::write(&part,serde_json::json!({"version":1,"tombstones":values}).to_string()).is_ok(){let _=fs::rename(part,&self.file_path);}} out
 }
 pub fn list(&self)->Vec<XuserRoomTombstone>{self.with_map(|m|(m.values().cloned().collect(),false))}
 pub fn record(&self,v:XuserRoomTombstone){self.with_map(|m|{let k=tombstone_key(&v);if m.contains_key(&k){((),false)}else{m.insert(k,v);((),true)}})}
 pub fn clear(&self,room_id:&str,owner_auth_id:Option<&str>){let k=format!("{}\0{}",owner_auth_id.unwrap_or(""),room_id);self.with_map(|m|((),m.remove(&k).is_some()))}
}
