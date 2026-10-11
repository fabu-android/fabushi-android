use std::{fs,io,path::Path,time::{SystemTime,UNIX_EPOCH}};
use crate::sha256::sha256_hex;

pub fn read_machine_id(path:&Path)->io::Result<String>{Ok(fs::read_to_string(path)?.trim().to_string())}
pub fn write_machine_id(path:&Path,id:&str)->io::Result<()>{
    if id.trim().is_empty(){return Err(io::Error::new(io::ErrorKind::InvalidInput,"empty machine id"));}
    if let Some(parent)=path.parent(){fs::create_dir_all(parent)?;}
    fs::write(path,id.as_bytes())
}
pub fn get_or_create_host_machine_id(path:&Path)->io::Result<String>{
    if let Ok(id)=read_machine_id(path){if !id.is_empty(){return Ok(id);}}
    let now=SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let material=format!("{}:{}:{}",std::process::id(),now,path.display());
    let id=sha256_hex(material.as_bytes());
    write_machine_id(path,&id)?;
    Ok(id)
}
