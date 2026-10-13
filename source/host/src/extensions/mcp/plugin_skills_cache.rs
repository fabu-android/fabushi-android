use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PLUGIN_SKILLS_DIRNAME: &str = "plugin-skills";
pub const PLUGIN_SKILLS_CACHE_FILENAME: &str = "cache.json";
pub const AGENT_READABLE_SKILL_DIR_MODE: u32 = 0o755;
pub const AGENT_READABLE_SKILL_FILE_MODE: u32 = 0o644;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSkillRecord {
    pub id: String,
    pub plugin_id: String,
    pub plugin_name: String,
    pub name: String,
    pub description: String,
    pub file_path: String,
    pub plugin_version: String,
    pub install_path: String,
    pub skill_relative_path: String,
    pub publisher_user_id: Option<u64>,
    pub marketplace_team_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAuthBlock {
    pub plugin_id: String,
    pub plugin_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marketplace_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PluginSkillsCache {
    pub fetched_at: f64,
    pub current_user_id: Option<u64>,
    pub skills: Vec<PluginSkillRecord>,
    pub auth_blocked: Vec<PluginAuthBlock>,
}

#[derive(Debug, Clone)]
pub struct PluginSkillsCacheWriteIndex {
    pub current_user_id: Option<Option<u64>>,
    pub skills: Vec<Value>,
    pub auth_blocked: Option<Vec<PluginAuthBlock>>,
}

pub fn get_plugins_root_dir(sand_root: impl AsRef<Path>) -> PathBuf {
    sand_root.as_ref().join("plugins")
}

pub fn get_plugin_skills_dir(sand_root: impl AsRef<Path>) -> PathBuf {
    sand_root.as_ref().join(PLUGIN_SKILLS_DIRNAME)
}

pub fn get_plugin_skills_cache_path(cache_dir: impl AsRef<Path>) -> PathBuf {
    cache_dir.as_ref().join(PLUGIN_SKILLS_CACHE_FILENAME)
}

pub fn is_safe_plugin_skill_id(id: &str) -> bool {
    !id.is_empty()
        && id.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || byte.is_ascii_lowercase())
        })
}

fn positive_id(value: Option<&Value>) -> Option<u64> {
    value
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
}

fn string_field(record: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    record.get(key)?.as_str().map(str::to_string)
}

fn parse_skill(value: &Value) -> Option<PluginSkillRecord> {
    let record = value.as_object()?;
    let id = string_field(record, "id")?;
    let plugin_id = string_field(record, "pluginId")?;
    let plugin_name = string_field(record, "pluginName")?;
    let name = string_field(record, "name")?;
    let description = string_field(record, "description")?;
    let file_path = string_field(record, "filePath")?;
    if !is_safe_plugin_skill_id(&id)
        || plugin_id.is_empty()
        || name.is_empty()
        || !Path::new(&file_path).is_absolute()
    {
        return None;
    }
    let plugin_version = record
        .get("pluginVersion")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let install_path = record
        .get("installPath")
        .and_then(Value::as_str)
        .filter(|path| Path::new(path).is_absolute())
        .unwrap_or_default()
        .to_string();
    let skill_relative_path = record
        .get("skillRelativePath")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    Some(PluginSkillRecord {
        id,
        plugin_id,
        plugin_name,
        name,
        description,
        file_path,
        plugin_version,
        install_path,
        skill_relative_path,
        publisher_user_id: positive_id(record.get("publisherUserId")),
        marketplace_team_id: positive_id(record.get("marketplaceTeamId")),
    })
}

fn parse_auth_block(value: &Value) -> Option<PluginAuthBlock> {
    let record = value.as_object()?;
    let plugin_id = string_field(record, "pluginId")?;
    let plugin_name = string_field(record, "pluginName")?;
    let marketplace_name = match record.get("marketplaceName") {
        None => None,
        Some(Value::String(value)) => Some(value.clone()),
        Some(_) => return None,
    };
    Some(PluginAuthBlock {
        plugin_id,
        plugin_name,
        marketplace_name,
    })
}

pub fn read_plugin_skills_cache(cache_dir: impl AsRef<Path>) -> Option<PluginSkillsCache> {
    let text = fs::read_to_string(get_plugin_skills_cache_path(cache_dir)).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let record = value.as_object()?;
    let fetched_at = record.get("fetchedAt")?.as_f64()?;
    if !fetched_at.is_finite() {
        return None;
    }
    let raw_skills = record.get("skills")?.as_array()?;
    let mut skills = Vec::with_capacity(raw_skills.len());
    for value in raw_skills {
        skills.push(parse_skill(value)?);
    }
    let auth_blocked = match record.get("authBlocked").and_then(Value::as_array) {
        Some(values) => {
            let mut parsed = Vec::with_capacity(values.len());
            let mut valid = true;
            for value in values {
                match parse_auth_block(value) {
                    Some(value) => parsed.push(value),
                    None => {
                        valid = false;
                        break;
                    }
                }
            }
            if valid { parsed } else { Vec::new() }
        }
        None => Vec::new(),
    };
    Some(PluginSkillsCache {
        fetched_at,
        current_user_id: positive_id(record.get("currentUserId")),
        skills,
        auth_blocked,
    })
}

#[cfg(unix)]
fn chmod(path: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn chmod(_path: &Path, _mode: u32) -> std::io::Result<()> {
    Ok(())
}

pub fn write_plugin_skills_cache(
    cache_dir: impl AsRef<Path>,
    index: &PluginSkillsCacheWriteIndex,
    now: impl FnOnce() -> f64,
) -> Result<(), String> {
    let cache_dir = cache_dir.as_ref();
    fs::create_dir_all(cache_dir).map_err(|error| error.to_string())?;
    chmod(cache_dir, AGENT_READABLE_SKILL_DIR_MODE).map_err(|error| error.to_string())?;

    let mut body = serde_json::Map::new();
    body.insert("fetchedAt".into(), serde_json::json!(now()));
    body.insert(
        "authBlocked".into(),
        serde_json::to_value(index.auth_blocked.as_deref().unwrap_or(&[]))
            .map_err(|error| error.to_string())?,
    );
    if let Some(current_user_id) = index.current_user_id {
        body.insert(
            "currentUserId".into(),
            current_user_id.map(Value::from).unwrap_or(Value::Null),
        );
    }
    body.insert("skills".into(), Value::Array(index.skills.clone()));
    let text = format!(
        "{}\n",
        serde_json::to_string_pretty(&Value::Object(body))
            .map_err(|error| error.to_string())?
    );

    let path = get_plugin_skills_cache_path(cache_dir);
    let temp = PathBuf::from(format!("{}.tmp", path.display()));
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(AGENT_READABLE_SKILL_FILE_MODE);
    }
    let mut file = options.open(&temp).map_err(|error| error.to_string())?;
    file.write_all(text.as_bytes()).map_err(|error| error.to_string())?;
    drop(file);
    chmod(&temp, AGENT_READABLE_SKILL_FILE_MODE).map_err(|error| error.to_string())?;
    fs::rename(&temp, &path).map_err(|error| error.to_string())?;
    Ok(())
}
