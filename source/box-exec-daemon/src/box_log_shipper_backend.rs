use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};

use fabushi_android_shared::ExecutionError;
use serde::{Deserialize, Serialize};

pub const MAX_REMOTE_LOG_FILES: usize = 256;
pub const MAX_REMOTE_LOG_BYTES_PER_POLL: u64 = 512 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum DeliverySettlement{Delivered,Dropped}
#[derive(Clone, Debug, PartialEq, Eq)] pub struct RemoteBoxLogRecord{pub source:String,pub bytes:Vec<u8>,pub start_offset:u64,pub end_offset:u64}
#[derive(Clone, Debug)] pub struct RemoteBoxLogShipperConfig{pub log_root:PathBuf,pub offsets_path:PathBuf,pub skip_relative_paths:Vec<PathBuf>}
#[derive(Default,Serialize,Deserialize)] struct OffsetSnapshot{offsets:BTreeMap<String,u64>}
pub struct RemoteBoxLogShipper{config:RemoteBoxLogShipperConfig,offsets:BTreeMap<String,u64>}

impl RemoteBoxLogShipper{
 pub fn open(config:RemoteBoxLogShipperConfig)->Result<Self,ExecutionError>{
  if !config.log_root.is_absolute()||!config.offsets_path.is_absolute(){return Err(ExecutionError::InvalidRequest("remote Box log paths must be absolute".into()));}
  for p in &config.skip_relative_paths{if p.is_absolute()||p.components().any(|c|!matches!(c,Component::Normal(_))){return Err(ExecutionError::InvalidRequest("remote Box log skip path must be bounded and relative".into()));}}
  let offsets=if config.offsets_path.exists(){serde_json::from_slice::<OffsetSnapshot>(&fs::read(&config.offsets_path).map_err(io)?).map_err(|e|ExecutionError::Transport(format!("decode Box log offsets: {e}")))?.offsets}else{BTreeMap::new()};
  Ok(Self{config,offsets})
 }
 fn relative_key(&self,path:&Path)->Result<String,ExecutionError>{let rel=path.strip_prefix(&self.config.log_root).map_err(|_|ExecutionError::InvalidRequest("Box log escaped configured root".into()))?;Ok(rel.to_string_lossy().replace('\\',"/"))}
 fn skipped(&self,rel:&Path)->bool{self.config.skip_relative_paths.iter().any(|p|p==rel)||rel.file_name().is_some_and(|n|n=="sand-notify-injector-worker.log")}
 fn collect_files(&self)->Result<Vec<PathBuf>,ExecutionError>{fn walk(root:&Path,dir:&Path,depth:usize,out:&mut Vec<PathBuf>)->Result<(),ExecutionError>{if depth>2{return Ok(());}for e in fs::read_dir(dir).map_err(io)?{let e=e.map_err(io)?;let p=e.path();let ty=e.file_type().map_err(io)?;if ty.is_symlink(){continue;}if ty.is_dir(){walk(root,&p,depth+1,out)?;}else if ty.is_file(){out.push(p);if out.len()>=MAX_REMOTE_LOG_FILES{return Ok(());}}}let _=root;Ok(())}let mut v=Vec::new();walk(&self.config.log_root,&self.config.log_root,0,&mut v)?;v.sort();Ok(v)}
 pub fn poll_once<F>(&mut self,mut deliver:F)->Result<usize,ExecutionError> where F:FnMut(&RemoteBoxLogRecord)->DeliverySettlement{
  let root=fs::canonicalize(&self.config.log_root).map_err(io)?;let files=self.collect_files()?;let mut delivered=0;
  for path in files{let canonical=fs::canonicalize(&path).map_err(io)?;if !canonical.starts_with(&root){continue;}let rel=canonical.strip_prefix(&root).map_err(|_|ExecutionError::InvalidRequest("Box log escaped configured root".into()))?;if self.skipped(rel){continue;}let key=self.relative_key(&canonical)?;let metadata=fs::metadata(&canonical).map_err(io)?;let start=self.offsets.get(&key).copied().unwrap_or(0).min(metadata.len());if start>=metadata.len(){continue;}let take=(metadata.len()-start).min(MAX_REMOTE_LOG_BYTES_PER_POLL);let mut file=fs::File::open(&canonical).map_err(io)?;file.seek(SeekFrom::Start(start)).map_err(io)?;let mut bytes=Vec::with_capacity(take as usize);file.take(take).read_to_end(&mut bytes).map_err(io)?;let end=start+bytes.len() as u64;let record=RemoteBoxLogRecord{source:key.clone(),bytes,start_offset:start,end_offset:end};if deliver(&record)==DeliverySettlement::Delivered{self.offsets.insert(key,end);delivered+=1;}}
  Ok(delivered)
 }
 pub fn checkpoint(&self)->Result<(),ExecutionError>{if let Some(parent)=self.config.offsets_path.parent(){fs::create_dir_all(parent).map_err(io)?;}let bytes=serde_json::to_vec_pretty(&OffsetSnapshot{offsets:self.offsets.clone()}).map_err(|e|ExecutionError::Transport(format!("encode Box log offsets: {e}")))?;let tmp=self.config.offsets_path.with_extension("tmp");let mut f=fs::File::create(&tmp).map_err(io)?;f.write_all(&bytes).map_err(io)?;f.sync_all().map_err(io)?;fs::rename(tmp,&self.config.offsets_path).map_err(io)}
 pub fn offset_for(&self,relative:&str)->Option<u64>{self.offsets.get(relative).copied()}
}
fn io(e:std::io::Error)->ExecutionError{ExecutionError::Transport(format!("remote Box log I/O failed: {e}"))}

#[cfg(test)]mod tests{use super::*;
#[test]fn delivered_only_offsets_survive_reopen_and_dropped_records_replay(){let t=tempfile::tempdir().unwrap();let root=t.path().join("logs");fs::create_dir_all(root.join("sand-window-7")).unwrap();fs::write(root.join("worker.log"),"one line\n").unwrap();fs::write(root.join("sand-window-7/browser.log"),"browser\n").unwrap();fs::write(root.join("sand-notify-injector-worker.log"),"skip\n").unwrap();let offsets=t.path().join("state/offsets.json");let cfg=RemoteBoxLogShipperConfig{log_root:root.clone(),offsets_path:offsets.clone(),skip_relative_paths:vec![]};let mut s=RemoteBoxLogShipper::open(cfg.clone()).unwrap();let mut first=true;s.poll_once(|_|{if first{first=false;DeliverySettlement::Dropped}else{DeliverySettlement::Delivered}}).unwrap();assert_eq!(s.offset_for("worker.log"),None);assert_eq!(s.offset_for("sand-window-7/browser.log"),Some(8));assert_eq!(s.offset_for("sand-notify-injector-worker.log"),None);s.checkpoint().unwrap();let mut r=RemoteBoxLogShipper::open(cfg).unwrap();let mut seen=Vec::new();r.poll_once(|x|{seen.push(x.source.clone());DeliverySettlement::Delivered}).unwrap();assert_eq!(seen,vec!["worker.log"]);assert_eq!(r.offset_for("worker.log"),Some(9));}
#[test]fn refuses_unbounded_skip_paths(){let t=tempfile::tempdir().unwrap();let cfg=RemoteBoxLogShipperConfig{log_root:t.path().to_path_buf(),offsets_path:t.path().join("o.json"),skip_relative_paths:vec![PathBuf::from("../x")]};assert!(RemoteBoxLogShipper::open(cfg).is_err());}}
