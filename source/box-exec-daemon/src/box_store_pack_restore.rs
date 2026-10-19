use flate2::read::GzDecoder;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

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

/// Reuses a single verified file descriptor for each content-addressed pack during one
/// hydration pass, avoiding a full-pack hash for every member restored from that pack.
#[derive(Default)]
pub struct PackRestoreCache {
    verified_files: BTreeMap<(PathBuf, String), File>,
    // The pack index is an immutable snapshot for one hydration pass. Reuse it
    // across members just as the Desktop bulk-restore path does.
    index: Option<(PathBuf, Arc<PackIndex>)>,
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn clen_bound(size: u64) -> u64 { size + 4_096 + size.div_ceil(512) }
fn valid_member(member: &PackMember) -> bool {
    is_sha256(&member.sha) && member.size <= PACK_MEMBER_MAX_BYTES && member.clen <= clen_bound(member.size)
}
fn valid_pack(pack: &PackEntry) -> bool {
    // Desktop pack IDs are SHA-256 of the entire uploaded pack object. Enforce that
    // content-addressed shape before using an ID as a local filesystem component.
    if !is_sha256(&pack.id) || pack.bytes > PACK_OBJECT_MAX_BYTES { return false; }
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

fn sha256_file(file: &mut File) -> Result<String, String> {
    file.seek(SeekFrom::Start(0)).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    file.seek(SeekFrom::Start(0)).map_err(|error| error.to_string())?;
    Ok(hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect())
}

fn load_pack_index(
    store_dir: &Path,
    cache: &mut PackRestoreCache,
) -> Result<Option<Arc<PackIndex>>, String> {
    let index_path = store_dir.join(BOX_STORE_PACK_INDEX_KEY);
    if let Some((cached_path, index)) = cache.index.as_ref() {
        if cached_path == &index_path {
            return Ok(Some(Arc::clone(index)));
        }
    }

    let metadata = match fs::symlink_metadata(&index_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    if !metadata.file_type().is_file() {
        return Err("pack index is not a regular file".into());
    }
    if metadata.len() > PACK_INDEX_MAX_BYTES {
        return Err("pack index exceeds byte cap".into());
    }

    // Check the opened file too, then cap the read itself. Checking only the
    // Vec length after fs::read would allocate an attacker-controlled file
    // before enforcing PACK_INDEX_MAX_BYTES.
    let file = File::open(&index_path).map_err(|error| error.to_string())?;
    let opened_metadata = file.metadata().map_err(|error| error.to_string())?;
    if !opened_metadata.is_file() {
        return Err("pack index is not a regular file".into());
    }
    if opened_metadata.len() > PACK_INDEX_MAX_BYTES {
        return Err("pack index exceeds byte cap".into());
    }
    let mut index_bytes = Vec::with_capacity(opened_metadata.len() as usize);
    file.take(PACK_INDEX_MAX_BYTES.saturating_add(1))
        .read_to_end(&mut index_bytes)
        .map_err(|error| error.to_string())?;
    if index_bytes.len() as u64 > PACK_INDEX_MAX_BYTES {
        return Err("pack index exceeds byte cap".into());
    }
    if index_bytes.len() as u64 != opened_metadata.len() {
        return Err("pack index changed while reading".into());
    }

    let index: PackIndex =
        serde_json::from_slice(&index_bytes).map_err(|_| "invalid pack index".to_string())?;
    if index.version != PACK_INDEX_VERSION || index.packs.iter().any(|pack| !valid_pack(pack)) {
        return Err("invalid pack index".into());
    }

    let index = Arc::new(index);
    cache.index = Some((index_path, Arc::clone(&index)));
    Ok(Some(index))
}

pub fn read_pack_blob(
    store_dir: &Path,
    sha: &str,
    expected_size: u64,
    cache: &mut PackRestoreCache,
) -> Result<Option<Vec<u8>>, String> {
    let index_path = store_dir.join(BOX_STORE_PACK_INDEX_KEY);
    if !index_path.exists() {
        return Ok(None);
    }
    if !is_sha256(sha) {
        return Err("invalid requested object digest".into());
    }
    let Some(index) = load_pack_index(store_dir, cache)? else {
        return Ok(None);
    };
    let _ = index.max_vmtime;
    for pack in &index.packs {
        let Some(member)=pack.members.iter().find(|m|m.sha==sha && m.size==expected_size) else { continue; };
        let path=store_dir.join(BOX_STORE_PACKS_PREFIX).join(&pack.id);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.to_string()),
        };
        if !metadata.file_type().is_file() || metadata.len() != pack.bytes { continue; }
        let key = (store_dir.to_path_buf(), pack.id.clone());
        if !cache.verified_files.contains_key(&key) {
            let mut file = File::open(&path).map_err(|error| error.to_string())?;
            let opened_metadata = file.metadata().map_err(|error| error.to_string())?;
            if !opened_metadata.is_file() || opened_metadata.len() != pack.bytes { continue; }
            if sha256_file(&mut file)? != pack.id {
                return Err("pack content digest mismatch".into());
            }
            cache.verified_files.insert(key.clone(), file);
        }
        let Some(file) = cache.verified_files.get_mut(&key) else { continue; };
        if file.metadata().map_err(|error| error.to_string())?.len() != pack.bytes {
            return Err("pack size changed after verification".into());
        }
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


#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::GzEncoder, Compression};
    use std::{fs as test_fs, io::Write};
    use tempfile::tempdir;

    fn write_index(root: &Path, id: &str, pack_bytes: usize, payload: &[u8]) {
        let index = serde_json::json!({
            "version": 1,
            "maxVmtime": 1,
            "packs": [{
                "id": id,
                "bytes": pack_bytes as u64,
                "members": [{
                    "sha": sha256(payload),
                    "size": payload.len() as u64,
                    "offset": 0,
                    "clen": pack_bytes as u64,
                    "vmtime": 1
                }]
            }]
        });
        test_fs::write(
            root.join(BOX_STORE_PACK_INDEX_KEY),
            serde_json::to_vec(&index).unwrap(),
        ).unwrap();
    }

    fn compressed(payload: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(payload).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn rejects_non_content_addressed_pack_ids_before_path_access() {
        let root = tempdir().unwrap();
        test_fs::create_dir_all(root.path().join(BOX_STORE_PACKS_PREFIX)).unwrap();
        let payload = b"safe";
        write_index(root.path(), "../outside", 4, payload);
        let mut cache = PackRestoreCache::default();
        let error = read_pack_blob(root.path(), &sha256(payload), payload.len() as u64, &mut cache)
            .unwrap_err();
        assert_eq!(error, "invalid pack index");
    }

    #[test]
    fn verifies_content_addressed_pack_once_and_restores_member() {
        let root = tempdir().unwrap();
        let packs = root.path().join(BOX_STORE_PACKS_PREFIX);
        test_fs::create_dir_all(&packs).unwrap();
        let payload = b"verified packed object";
        let bytes = compressed(payload);
        let id = sha256(&bytes);
        test_fs::write(packs.join(&id), &bytes).unwrap();
        write_index(root.path(), &id, bytes.len(), payload);
        let mut cache = PackRestoreCache::default();
        assert_eq!(
            read_pack_blob(root.path(), &sha256(payload), payload.len() as u64, &mut cache).unwrap(),
            Some(payload.to_vec()),
        );
        // A hydration pass uses one index snapshot; member lookups must not
        // re-read and re-parse the index for every manifest entry.
        test_fs::write(root.path().join(BOX_STORE_PACK_INDEX_KEY), b"{invalid").unwrap();
        assert_eq!(
            read_pack_blob(root.path(), &sha256(payload), payload.len() as u64, &mut cache).unwrap(),
            Some(payload.to_vec()),
        );
    }

    #[test]
    fn rejects_oversized_pack_index_before_reading_it() {
        let root = tempdir().unwrap();
        let index_path = root.path().join(BOX_STORE_PACK_INDEX_KEY);
        test_fs::create_dir_all(index_path.parent().unwrap()).unwrap();
        let file = File::create(&index_path).unwrap();
        file.set_len(PACK_INDEX_MAX_BYTES + 1).unwrap();

        let mut cache = PackRestoreCache::default();
        let error = read_pack_blob(root.path(), &"a".repeat(64), 0, &mut cache).unwrap_err();
        assert_eq!(error, "pack index exceeds byte cap");
    }

    #[test]
    fn rejects_pack_file_whose_digest_does_not_match_its_index_id() {
        let root = tempdir().unwrap();
        let packs = root.path().join(BOX_STORE_PACKS_PREFIX);
        test_fs::create_dir_all(&packs).unwrap();
        let payload = b"verified packed object";
        let bytes = compressed(payload);
        let id = sha256(&bytes);
        let mut corrupt = bytes.clone();
        corrupt[0] ^= 0x01;
        test_fs::write(packs.join(&id), &corrupt).unwrap();
        write_index(root.path(), &id, bytes.len(), payload);
        let mut cache = PackRestoreCache::default();
        let error = read_pack_blob(root.path(), &sha256(payload), payload.len() as u64, &mut cache)
            .unwrap_err();
        assert_eq!(error, "pack content digest mismatch");
    }
}
