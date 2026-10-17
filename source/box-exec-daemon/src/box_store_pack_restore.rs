use flate2::read::GzDecoder;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

pub const BOX_STORE_PACK_INDEX_KEY: &str = "packs/index.json";
pub const BOX_STORE_PACKS_PREFIX: &str = "packs";
const PACK_INDEX_VERSION: u64 = 1;
const PACK_MEMBER_MAX_BYTES: u64 = 8 * 1024 * 1024;
const PACK_MAX_MEMBER_SIZE_SUM: u64 = 256 * 1024 * 1024;
const PACK_OBJECT_MAX_BYTES: u64 = 320 * 1024 * 1024;
const PACK_INDEX_MAX_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
pub struct PackMember { pub sha: String, pub size: u64, pub offset: u64, pub clen: u64, pub vmtime: u64 }
#[derive(Debug, Clone, Deserialize)]
pub struct PackEntry { pub id: String, pub bytes: u64, pub members: Vec<PackMember> }
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackIndex { pub version: u64, pub max_vmtime: u64, pub packs: Vec<PackEntry> }

fn clen_bound(size: u64) -> u64 { size + 4_096 + size.div_ceil(512) }
fn valid_member(member: &PackMember) -> bool {
    !member.sha.is_empty() && member.size <= PACK_MEMBER_MAX_BYTES && member.clen <= clen_bound(member.size)
}
fn valid_pack(pack: &PackEntry) -> bool {
    if pack.id.is_empty() || pack.bytes > PACK_OBJECT_MAX_BYTES { return false; }
    let mut total=0u64;
    for member in &pack.members {
        if !valid_member(member) { return false; }
        let Some(end)=member.offset.checked_add(member.clen) else { return false; };
        if end > pack.bytes { return false; }
        let Some(next)=total.checked_add(member.size) else { return false; };
        total=next;
        if total > PACK_MAX_MEMBER_SIZE_SUM { return false; }
    }
    true
}
fn sha256(bytes:&[u8])->String { let d=Sha256::digest(bytes); d.iter().map(|b|format!("{b:02x}")).collect() }

pub fn read_pack_blob(store_dir:&Path, sha:&str, expected_size:u64) -> Result<Option<Vec<u8>>,String> {
    let index_path=store_dir.join(BOX_STORE_PACK_INDEX_KEY);
    if !index_path.exists() { return Ok(None); }
    let index_bytes=std::fs::read(&index_path).map_err(|e|e.to_string())?;
    if index_bytes.len() as u64 > PACK_INDEX_MAX_BYTES { return Err("pack index exceeds byte cap".into()); }
    let index:PackIndex=serde_json::from_slice(&index_bytes).map_err(|_|"invalid pack index".to_string())?;
    if index.version != PACK_INDEX_VERSION || index.packs.iter().any(|p|!valid_pack(p)) { return Err("invalid pack index".into()); }
    let _ = index.max_vmtime;
    for pack in &index.packs {
        let Some(member)=pack.members.iter().find(|m|m.sha==sha && m.size==expected_size) else { continue; };
        let path=store_dir.join(BOX_STORE_PACKS_PREFIX).join(&pack.id);
        if !path.exists() { continue; }
        let metadata=std::fs::metadata(&path).map_err(|e|e.to_string())?;
        if metadata.len() != pack.bytes { continue; }
        let mut file=File::open(path).map_err(|e|e.to_string())?;
        file.seek(SeekFrom::Start(member.offset)).map_err(|e|e.to_string())?;
        let clen=usize::try_from(member.clen).map_err(|_|"pack member compressed length overflow".to_string())?;
        let mut compressed=vec![0u8;clen];
        if file.read_exact(&mut compressed).is_err() { continue; }
        let mut decoder=GzDecoder::new(compressed.as_slice());
        let mut limited=decoder.by_ref().take(expected_size.saturating_add(1));
        let mut bytes=Vec::with_capacity(expected_size.min(usize::MAX as u64) as usize);
        if limited.read_to_end(&mut bytes).is_err() || bytes.len() as u64 != expected_size || sha256(&bytes) != sha { continue; }
        return Ok(Some(bytes));
    }
    Ok(None)
}
