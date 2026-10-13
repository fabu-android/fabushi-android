use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SAND_PROFILE_FILENAME: &str = "profile.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandAgentProfile {
    pub name: String,
    pub description: String,
    pub title: String,
    pub avatar_shape: String,
    pub avatar_color: String,
}

pub fn get_sand_profile_path(agent_dir: impl AsRef<Path>) -> PathBuf {
    agent_dir.as_ref().join(SAND_PROFILE_FILENAME)
}

fn parse_profile_json(path: &Path) -> Option<serde_json::Map<String, Value>> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str::<Value>(&text).ok()?.as_object().cloned()
}

pub fn read_sand_profile_file(path: impl AsRef<Path>) -> Option<SandAgentProfile> {
    let parsed = parse_profile_json(path.as_ref())?;
    Some(SandAgentProfile {
        name: parsed
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        description: parsed
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        title: parsed
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string(),
        avatar_shape: parsed
            .get("avatarShape")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string(),
        avatar_color: parsed
            .get("avatarColor")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string(),
    })
}

pub fn read_legacy_profile_avatar_field(path: impl AsRef<Path>) -> Option<String> {
    let parsed = parse_profile_json(path.as_ref())?;
    let value = parsed.get("avatar").and_then(Value::as_str)?.trim();
    (!value.is_empty()).then(|| value.to_string())
}

pub fn write_sand_profile_file(
    path: impl AsRef<Path>,
    profile: &SandAgentProfile,
) -> io::Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let normalized = SandAgentProfile {
        name: profile.name.clone(),
        description: profile.description.clone(),
        title: profile.title.trim().to_string(),
        avatar_shape: profile.avatar_shape.trim().to_string(),
        avatar_color: profile.avatar_color.trim().to_string(),
    };
    let mut serialized = serde_json::to_string_pretty(&normalized).map_err(io::Error::other)?;
    serialized.push('\n');
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let temporary = PathBuf::from(format!(
        "{}.{}.{}.tmp",
        path.display(),
        process::id(),
        millis
    ));
    fs::write(&temporary, serialized)?;
    fs::rename(temporary, path)
}
