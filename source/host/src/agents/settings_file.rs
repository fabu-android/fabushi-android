use std::{fs, io, path::{Path, PathBuf}, process};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const SAND_SETTINGS_FILENAME: &str = "settings.json";
pub const DEFAULT_NOTIFY_ON_AGENT_UPDATES: bool = true;
pub const DEFAULT_HIDDEN_FROM_SIDEBAR: bool = false;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandAgentSettings {
    pub notify_on_agent_updates: bool,
    pub hidden_from_sidebar: bool,
}

pub fn get_sand_settings_path(agent_dir: impl AsRef<Path>) -> PathBuf { agent_dir.as_ref().join(SAND_SETTINGS_FILENAME) }

fn read_raw_settings(path: &Path) -> Map<String, Value> {
    fs::read_to_string(path).ok().and_then(|text| serde_json::from_str::<Value>(&text).ok()).and_then(|v| v.as_object().cloned()).unwrap_or_default()
}

pub fn read_sand_settings_file(path: impl AsRef<Path>) -> SandAgentSettings {
    let raw = read_raw_settings(path.as_ref());
    SandAgentSettings {
        notify_on_agent_updates: raw.get("notifyOnAgentUpdates").and_then(Value::as_bool).unwrap_or(DEFAULT_NOTIFY_ON_AGENT_UPDATES),
        hidden_from_sidebar: raw.get("hiddenFromSidebar").and_then(Value::as_bool).unwrap_or(DEFAULT_HIDDEN_FROM_SIDEBAR),
    }
}

pub fn write_sand_settings_file(path: impl AsRef<Path>, update: &Map<String, Value>) -> io::Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() { fs::create_dir_all(parent)?; }
    let mut raw = read_raw_settings(path);
    raw.extend(update.clone());
    let temporary = path.with_extension(format!("{}.{}.tmp", process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()));
    let mut text = serde_json::to_string_pretty(&Value::Object(raw)).map_err(io::Error::other)?;
    text.push('\n');
    fs::write(&temporary, text)?;
    fs::rename(temporary, path)
}
