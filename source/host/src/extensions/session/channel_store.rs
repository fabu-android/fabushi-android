use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use serde_json::Value;

use crate::storage::folder_id::is_safe_folder_id;

pub const CHANNELS_DIRNAME: &str = "channels";
pub const CHANNEL_CONFIG_FILENAME: &str = "connection.json";
pub const CHANNEL_CHANGE_DEBOUNCE_MS: u64 = 50;

pub type ChannelChangeListener = Arc<dyn Fn() + Send + Sync + 'static>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChannelConnection {
    pub platform: String,
    pub label: String,
    pub status: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChannelConfig {
    pub platform: String,
    pub token: String,
    pub label: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ChannelStoreError {
    #[error("channel store filesystem error: {0}")]
    Io(#[from] io::Error),
    #[error("channel store serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Default)]
struct DebounceState {
    callback: Option<ChannelChangeListener>,
    generation: u64,
    last_fingerprint: Option<Vec<(String, Vec<u8>)>>,
}

struct ChannelWatchInner {
    channels_dir: PathBuf,
    debounce: Arc<Mutex<DebounceState>>,
    watcher: Mutex<Option<RecommendedWatcher>>,
}

#[derive(Clone)]
pub struct FileChannelStore {
    inner: Arc<ChannelWatchInner>,
}

impl FileChannelStore {
    pub fn new(channels_dir: impl Into<PathBuf>) -> Self {
        Self {
            inner: Arc::new(ChannelWatchInner {
                channels_dir: channels_dir.into(),
                debounce: Arc::new(Mutex::new(DebounceState::default())),
                watcher: Mutex::new(None),
            }),
        }
    }

    pub fn get_location(&self) -> &Path {
        &self.inner.channels_dir
    }

    pub fn set_on_change(&self, on_change: Option<ChannelChangeListener>) {
        let enabled = on_change.is_some();
        let baseline = enabled.then(|| channel_state_fingerprint(&self.inner.channels_dir));
        if let Ok(mut state) = self.inner.debounce.lock() {
            state.callback = on_change;
            state.generation = state.generation.saturating_add(1);
            state.last_fingerprint = baseline;
        }
        if enabled {
            self.ensure_watcher();
        } else if let Ok(mut slot) = self.inner.watcher.lock() {
            *slot = None;
        }
    }

    pub fn config_path(&self, platform: &str) -> PathBuf {
        self.inner.channels_dir.join(platform).join(CHANNEL_CONFIG_FILENAME)
    }

    pub fn list_platforms(&self) -> Vec<String> {
        let Ok(entries) = fs::read_dir(&self.inner.channels_dir) else {
            return Vec::new();
        };
        let mut platforms = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                let kind = entry.file_type().ok()?;
                (kind.is_dir() && is_safe_folder_id(&name) && self.read_label(&name).is_some())
                    .then_some(name)
            })
            .collect::<Vec<_>>();
        platforms.sort();
        platforms
    }

    pub fn read_label(&self, platform: &str) -> Option<String> {
        let raw = fs::read_to_string(self.config_path(platform)).ok()?;
        let value = serde_json::from_str::<Value>(&raw).ok()?;
        let object = value.as_object()?;
        Some(label_for(platform, object.get("label").and_then(Value::as_str)))
    }

    pub fn list_connections(&self) -> Vec<ChannelConnection> {
        self.list_platforms()
            .into_iter()
            .map(|platform| ChannelConnection {
                label: self.read_label(&platform).unwrap_or_else(|| platform.clone()),
                platform,
                status: "configured",
            })
            .collect()
    }

    pub fn write_metadata(&self, platform: &str, label: &str) -> Result<bool, ChannelStoreError> {
        if !is_safe_folder_id(platform) {
            return Ok(false);
        }
        let path = self.config_path(platform);
        let Some(parent) = path.parent() else {
            return Ok(false);
        };
        fs::create_dir_all(parent)?;
        let mut serialized = serde_json::to_string_pretty(&serde_json::json!({
            "label": label_for(platform, Some(label)),
        }))?;
        serialized.push('\n');
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temporary = PathBuf::from(format!(
            "{}.{}.{}.tmp",
            path.display(),
            process::id(),
            nanos
        ));
        fs::write(&temporary, serialized)?;
        if let Err(error) = fs::rename(&temporary, &path) {
            let _ = fs::remove_file(&temporary);
            return Err(error.into());
        }
        self.notify_if_watcher_unavailable();
        Ok(true)
    }

    pub fn remove(&self, platform: &str) -> Result<bool, ChannelStoreError> {
        if !is_safe_folder_id(platform) {
            return Ok(false);
        }
        let platform_dir = self.inner.channels_dir.join(platform);
        match fs::metadata(&platform_dir) {
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => return Ok(false),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        }
        fs::remove_dir_all(platform_dir)?;
        self.notify_if_watcher_unavailable();
        Ok(true)
    }

    fn notify_if_watcher_unavailable(&self) {
        let watching = self
            .inner
            .watcher
            .lock()
            .map(|slot| slot.is_some())
            .unwrap_or(false);
        if !watching {
            schedule_debounced_notify(
                &self.inner.debounce,
                &self.inner.channels_dir,
            );
        }
    }

    fn ensure_watcher(&self) {
        let Ok(mut slot) = self.inner.watcher.lock() else {
            return;
        };
        if slot.is_some() {
            return;
        }
        if fs::create_dir_all(&self.inner.channels_dir).is_err() {
            return;
        }
        let debounce = Arc::clone(&self.inner.debounce);
        let channels_dir = self.inner.channels_dir.clone();
        let Ok(mut watcher) = notify::recommended_watcher(
            move |event: notify::Result<notify::Event>| {
                let Ok(event) = event else {
                    return;
                };
                // Frozen Grok uses Node fs.watch, whose observable contract
                // is rename/change. notify on macOS additionally emits access,
                // metadata and catch-all bookkeeping events for one logical
                // write. Feeding those through a 50 ms trailing debounce can
                // split a single burst into two callbacks when a late metadata
                // event arrives after the data event. Keep only create/remove,
                // data/name modifications and Any (the closest fs.watch
                // equivalents), and ignore backend-only noise.
                match event.kind {
                    notify::EventKind::Access(_)
                    | notify::EventKind::Modify(notify::event::ModifyKind::Metadata(_))
                    | notify::EventKind::Other => return,
                    _ => {}
                }
                schedule_debounced_notify(&debounce, &channels_dir);
            },
        ) else {
            return;
        };
        if watcher.watch(&self.inner.channels_dir, RecursiveMode::Recursive).is_err() {
            return;
        }
        *slot = Some(watcher);
    }
}

fn channel_state_fingerprint(channels_dir: &Path) -> Vec<(String, Vec<u8>)> {
    let Ok(entries) = fs::read_dir(channels_dir) else {
        return Vec::new();
    };
    let mut fingerprint = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let kind = entry.file_type().ok()?;
            if !kind.is_dir() {
                return None;
            }
            let platform = entry.file_name().into_string().ok()?;
            if !is_safe_folder_id(&platform) {
                return None;
            }
            let bytes = fs::read(entry.path().join(CHANNEL_CONFIG_FILENAME)).ok()?;
            Some((platform, bytes))
        })
        .collect::<Vec<_>>();
    fingerprint.sort_by(|left, right| left.0.cmp(&right.0));
    fingerprint
}

fn schedule_debounced_notify(
    state: &Arc<Mutex<DebounceState>>,
    channels_dir: &Path,
) {
    let generation = {
        let Ok(mut state) = state.lock() else {
            return;
        };
        if state.callback.is_none() {
            return;
        }
        state.generation = state.generation.saturating_add(1);
        state.generation
    };
    let state_for_thread = Arc::clone(state);
    let channels_dir = channels_dir.to_path_buf();
    let channels_dir_for_thread = channels_dir.clone();
    let spawn = thread::Builder::new()
        .name("sand-channel-store-debounce".into())
        .spawn(move || {
            thread::sleep(Duration::from_millis(CHANNEL_CHANGE_DEBOUNCE_MS));
            let first_fingerprint = channel_state_fingerprint(&channels_dir_for_thread);
            thread::sleep(Duration::from_millis(CHANNEL_CHANGE_DEBOUNCE_MS));
            let fingerprint = channel_state_fingerprint(&channels_dir_for_thread);
            if first_fingerprint != fingerprint {
                schedule_debounced_notify(&state_for_thread, &channels_dir_for_thread);
                return;
            }
            let callback = state_for_thread.lock().ok().and_then(|mut state| {
                if state.generation != generation
                    || state.last_fingerprint.as_ref() == Some(&fingerprint)
                {
                    return None;
                }
                state.last_fingerprint = Some(fingerprint);
                state.callback.as_ref().map(Arc::clone)
            });
            if let Some(callback) = callback {
                callback();
            }
        });
    if spawn.is_err() {
        let fingerprint = channel_state_fingerprint(channels_dir.as_path());
        let callback = state.lock().ok().and_then(|mut state| {
            if state.generation != generation
                || state.last_fingerprint.as_ref() == Some(&fingerprint)
            {
                return None;
            }
            state.last_fingerprint = Some(fingerprint);
            state.callback.as_ref().map(Arc::clone)
        });
        if let Some(callback) = callback {
            callback();
        }
    }
}

pub fn get_agent_channels_dir(agent_dir: &Path) -> PathBuf {
    agent_dir.join(CHANNELS_DIRNAME)
}

pub fn label_for(platform: &str, raw: Option<&str>) -> String {
    let clamped = raw
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(80)
        .collect::<String>();
    if !clamped.is_empty() {
        return clamped;
    }
    match platform {
        "discord" => "Discord".to_string(),
        "slack" => "Slack".to_string(),
        _ => platform.to_string(),
    }
}
