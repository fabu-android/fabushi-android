use std::collections::VecDeque;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_STAT_PARSE_CACHE_CAPACITY: usize = 2_048;
pub const RACY_MTIME_TICK_WINDOW_MS: u128 = 2_000;

type NowMs = Arc<dyn Fn() -> u128 + Send + Sync + 'static>;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Fingerprint {
    stat_key: String,
    newest_mtime_ms: u128,
}

pub fn mtime_tick_could_still_hide_an_edit(newest_mtime_ms: u128, now_ms: u128) -> bool {
    now_ms.saturating_sub(newest_mtime_ms) < RACY_MTIME_TICK_WINDOW_MS
}

fn modified_ns(metadata: &fs::Metadata) -> u128 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

#[cfg(unix)]
fn file_identity(metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.ino()
}

#[cfg(not(unix))]
fn file_identity(_metadata: &fs::Metadata) -> u64 {
    0
}

fn fingerprint_of(paths: &[PathBuf]) -> io::Result<Option<Fingerprint>> {
    let mut parts = Vec::with_capacity(paths.len());
    let mut newest_mtime_ms = 0u128;
    for path in paths {
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let mtime_ns = modified_ns(&metadata);
        newest_mtime_ms = newest_mtime_ms.max(mtime_ns / 1_000_000);
        parts.push(format!(
            "{}:{}:{}",
            file_identity(&metadata),
            mtime_ns,
            metadata.len()
        ));
    }
    Ok(Some(Fingerprint {
        stat_key: parts.join("|"),
        newest_mtime_ms,
    }))
}

struct CacheEntry<T> {
    key: String,
    stat_key: String,
    value: T,
}

pub struct StatKeyedParseCache<T: Clone> {
    capacity: usize,
    entries: Mutex<VecDeque<CacheEntry<T>>>,
    now_ms: NowMs,
}

impl<T: Clone> Default for StatKeyedParseCache<T> {
    fn default() -> Self {
        Self::new(DEFAULT_STAT_PARSE_CACHE_CAPACITY)
    }
}

impl<T: Clone> StatKeyedParseCache<T> {
    pub fn new(capacity: usize) -> Self {
        Self::with_now_ms(
            capacity,
            Arc::new(|| {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis()
            }),
        )
    }

    pub fn with_now_ms(capacity: usize, now_ms: NowMs) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: Mutex::new(VecDeque::new()),
            now_ms,
        }
    }

    pub fn read<F>(&self, stat_paths: &[PathBuf], parse: F) -> Option<T>
    where
        F: FnOnce() -> T,
    {
        let key = stat_paths
            .iter()
            .map(|path| path.to_string_lossy())
            .collect::<Vec<_>>()
            .join("\0");
        let fingerprint = match fingerprint_of(stat_paths) {
            Ok(Some(fingerprint)) => fingerprint,
            Ok(None) => {
                if let Ok(mut entries) = self.entries.lock() {
                    entries.retain(|entry| entry.key != key);
                }
                return None;
            }
            Err(_) => return Some(parse()),
        };

        if let Ok(entries) = self.entries.lock() {
            if let Some(hit) = entries
                .iter()
                .find(|entry| entry.key == key && entry.stat_key == fingerprint.stat_key)
            {
                return Some(hit.value.clone());
            }
        }

        let value = parse();
        if mtime_tick_could_still_hide_an_edit(fingerprint.newest_mtime_ms, (self.now_ms)()) {
            return Some(value);
        }

        if let Ok(mut entries) = self.entries.lock() {
            entries.retain(|entry| entry.key != key);
            while entries.len() >= self.capacity {
                entries.pop_front();
            }
            entries.push_back(CacheEntry {
                key,
                stat_key: fingerprint.stat_key,
                value: value.clone(),
            });
        }
        Some(value)
    }
}

pub fn stat_paths(paths: impl IntoIterator<Item = impl AsRef<Path>>) -> Vec<PathBuf> {
    paths
        .into_iter()
        .map(|path| path.as_ref().to_path_buf())
        .collect()
}
