use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};

pub const CANONICAL_AVATAR_FILENAME: &str = "avatar.png";
pub const CONVENTIONAL_AVATAR_EXTENSIONS: &[&str] =
    &["png", "jpg", "jpeg", "webp", "gif", "svg"];
pub const AVATAR_MAX_BYTES: u64 = 5 * 1024 * 1024;
const AVATAR_CACHE_LIMIT: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvatarData {
    pub data_url: String,
    pub version: String,
}

#[derive(Debug, Clone)]
struct CachedAvatar {
    fingerprint: String,
    data: AvatarData,
}

#[derive(Default)]
struct AvatarCache {
    values: HashMap<String, CachedAvatar>,
    order: VecDeque<String>,
}

fn avatar_cache() -> &'static Mutex<AvatarCache> {
    static CACHE: OnceLock<Mutex<AvatarCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(AvatarCache::default()))
}

pub fn is_conventional_avatar_filename(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let Some(extension) = lower.strip_prefix("avatar.") else {
        return false;
    };
    CONVENTIONAL_AVATAR_EXTENSIONS.contains(&extension)
}

pub fn conventional_avatar_rank(name: &str) -> i32 {
    if name == CANONICAL_AVATAR_FILENAME {
        return -1;
    }
    let extension = Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    CONVENTIONAL_AVATAR_EXTENSIONS
        .iter()
        .position(|candidate| *candidate == extension)
        .map(|index| index as i32)
        .unwrap_or(CONVENTIONAL_AVATAR_EXTENSIONS.len() as i32)
}

pub fn list_conventional_avatar_filenames(agent_dir: &Path) -> Vec<String> {
    let mut values = fs::read_dir(agent_dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| is_conventional_avatar_filename(name))
        .collect::<Vec<_>>();
    values.sort_by(|a, b| {
        conventional_avatar_rank(a)
            .cmp(&conventional_avatar_rank(b))
            .then_with(|| a.cmp(b))
    });
    values
}

pub fn sniff_avatar_mime_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 8 && bytes[..8] == [137, 80, 78, 71, 13, 10, 26, 10] {
        return Some("image/png");
    }
    if bytes.len() >= 3 && bytes[..3] == [255, 216, 255] {
        return Some("image/jpeg");
    }
    if bytes.len() >= 6 && (&bytes[..6] == b"GIF87a" || &bytes[..6] == b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]);
    let normalized = head
        .trim_start_matches('\u{feff}')
        .trim_start()
        .to_ascii_lowercase();
    if normalized.starts_with('<') && normalized.contains("<svg") {
        return Some("image/svg+xml");
    }
    None
}

pub fn resolve_avatar_path_within_dir(
    agent_dir: &Path,
    candidate: Option<&str>,
) -> Option<PathBuf> {
    let trimmed = candidate?.trim();
    if trimmed.is_empty() {
        return None;
    }
    let raw = PathBuf::from(trimmed);
    // Android has no Desktop home/data-root to re-anchor. Absolute legacy paths are accepted only
    // when they already resolve inside this app-private Agent directory; relative paths are
    // anchored beneath it and canonicalized below.
    let absolute = if raw.is_absolute() {
        raw
    } else {
        agent_dir.join(raw)
    };
    if absolute == agent_dir || !absolute.starts_with(agent_dir) {
        return None;
    }
    let canonical_dir = fs::canonicalize(agent_dir).ok()?;
    let canonical_file = fs::canonicalize(&absolute).ok()?;
    if canonical_file == canonical_dir || !canonical_file.starts_with(&canonical_dir) {
        return None;
    }
    Some(canonical_file)
}

pub fn resolve_derived_avatar_filename(
    agent_dir: &Path,
    legacy_field_value: Option<&str>,
) -> Option<String> {
    if let Some(value) = list_conventional_avatar_filenames(agent_dir).into_iter().next() {
        return Some(value);
    }
    let legacy = legacy_field_value?.trim();
    if legacy.is_empty() {
        return None;
    }
    let source = resolve_avatar_path_within_dir(agent_dir, Some(legacy))?;
    if !source.is_file() {
        return None;
    }
    let source_extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let extension = if CONVENTIONAL_AVATAR_EXTENSIONS.contains(&source_extension.as_str()) {
        Some(source_extension)
    } else {
        fs::read(&source)
            .ok()
            .and_then(|bytes| mime_extension(sniff_avatar_mime_type(&bytes)).map(ToOwned::to_owned))
    };
    let Some(extension) = extension else {
        return Some(legacy.to_string());
    };
    let name = format!("avatar.{extension}");
    let destination = agent_dir.join(&name);
    match copy_exclusive(&source, &destination) {
        Ok(()) => Some(name),
        Err(_) => Some(legacy.to_string()),
    }
}

fn mime_extension(mime: Option<&str>) -> Option<&'static str> {
    match mime {
        Some("image/png") => Some("png"),
        Some("image/jpeg") => Some("jpg"),
        Some("image/webp") => Some("webp"),
        Some("image/gif") => Some("gif"),
        Some("image/svg+xml") => Some("svg"),
        _ => None,
    }
}

fn copy_exclusive(source: &Path, destination: &Path) -> io::Result<()> {
    let mut input = fs::File::open(source)?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    io::copy(&mut input, &mut output)?;
    output.flush()
}

fn resolve_and_stat_avatar(
    agent_dir: &Path,
    candidate: &str,
) -> Option<(PathBuf, f64, u64)> {
    let path = resolve_avatar_path_within_dir(agent_dir, Some(candidate))?;
    let metadata = fs::metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > AVATAR_MAX_BYTES {
        return None;
    }
    let mtime_ms = metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs_f64()
        * 1000.0;
    Some((path, mtime_ms, metadata.len()))
}

pub fn read_validated_avatar(
    agent_dir: &Path,
    candidate: &str,
) -> Option<(Vec<u8>, &'static str)> {
    let (path, _, _) = resolve_and_stat_avatar(agent_dir, candidate)?;
    let mut bytes = Vec::new();
    fs::File::open(path).ok()?.read_to_end(&mut bytes).ok()?;
    let mime = sniff_avatar_mime_type(&bytes)?;
    Some((bytes, mime))
}

pub fn read_avatar_bytes_within_dir(
    agent_dir: &Path,
    candidate: &str,
) -> Option<Vec<u8>> {
    read_validated_avatar(agent_dir, candidate).map(|value| value.0)
}

pub fn read_avatar_within_dir(
    agent_dir: &Path,
    candidate: &str,
) -> Option<AvatarData> {
    let (path, mtime_ms, size) = resolve_and_stat_avatar(agent_dir, candidate)?;
    let key = format!("{}\0{}", agent_dir.display(), path.display());
    let fingerprint = format!("{mtime_ms}:{size}");
    if let Ok(mut cache) = avatar_cache().lock() {
        if let Some(cached) = cache.values.get(&key).cloned() {
            if cached.fingerprint == fingerprint {
                touch_cache_key(&mut cache, &key);
                return Some(cached.data);
            }
        }
    }

    let bytes = fs::read(&path).ok()?;
    let mime = sniff_avatar_mime_type(&bytes)?;
    let digest = Sha256::digest(&bytes);
    let version = digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let data = AvatarData {
        data_url: format!("data:{mime};base64,{}", STANDARD.encode(&bytes)),
        version,
    };
    if let Ok(mut cache) = avatar_cache().lock() {
        cache.values.insert(
            key.clone(),
            CachedAvatar {
                fingerprint,
                data: data.clone(),
            },
        );
        touch_cache_key(&mut cache, &key);
        while cache.values.len() > AVATAR_CACHE_LIMIT {
            let Some(oldest) = cache.order.pop_front() else {
                break;
            };
            if oldest != key {
                cache.values.remove(&oldest);
            }
        }
    }
    Some(data)
}

fn touch_cache_key(cache: &mut AvatarCache, key: &str) {
    if let Some(index) = cache.order.iter().position(|candidate| candidate == key) {
        cache.order.remove(index);
    }
    cache.order.push_back(key.to_string());
}

pub fn invalidate_avatar_data_url_cache(agent_dir: &Path) {
    let prefix = format!("{}\0", agent_dir.display());
    if let Ok(mut cache) = avatar_cache().lock() {
        let keys = cache
            .values
            .keys()
            .filter(|key| key.starts_with(&prefix))
            .cloned()
            .collect::<Vec<_>>();
        for key in keys {
            cache.values.remove(&key);
            if let Some(index) = cache.order.iter().position(|candidate| candidate == &key) {
                cache.order.remove(index);
            }
        }
    }
}
