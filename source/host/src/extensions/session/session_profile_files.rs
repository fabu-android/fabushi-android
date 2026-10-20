use std::io;
use std::path::Path;

use crate::agents::agent_avatar::{
    AvatarData, read_avatar_bytes_within_dir, read_avatar_within_dir,
    resolve_derived_avatar_filename,
};
use crate::agents::agent_profile::{
    SandAgentProfile, get_sand_profile_path, read_legacy_profile_avatar_field,
    read_sand_profile_file, write_sand_profile_file,
};

use super::agent_db::read_persisted_agent_serde_snapshot;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProfileUpdate {
    pub name: String,
    pub description: String,
    pub title: Option<String>,
    pub avatar_shape: Option<String>,
    pub avatar_color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AgentAvatarResponse {
    pub version: Option<String>,
    pub data_url: Option<String>,
}

pub fn resolve_profile_name(
    trimmed_name: &str,
    current: Option<&SandAgentProfile>,
) -> String {
    if !trimmed_name.is_empty() {
        return trimmed_name.to_string();
    }
    if let Some(name) = current
        .map(|profile| profile.name.trim())
        .filter(|name| !name.is_empty())
    {
        return name.to_string();
    }
    "Grok".to_string()
}

pub fn write_agent_profile_update(
    agent_dir: &Path,
    update: &AgentProfileUpdate,
) -> io::Result<SandAgentProfile> {
    let path = get_sand_profile_path(agent_dir);
    let current = read_sand_profile_file(&path);
    let profile = SandAgentProfile {
        name: resolve_profile_name(update.name.trim(), current.as_ref()),
        description: update.description.trim().to_string(),
        title: update
            .title
            .as_deref()
            .map(str::trim)
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| {
                current
                    .as_ref()
                    .map(|profile| profile.title.clone())
                    .unwrap_or_default()
            }),
        avatar_shape: update
            .avatar_shape
            .as_deref()
            .map(str::trim)
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| {
                current
                    .as_ref()
                    .map(|profile| profile.avatar_shape.clone())
                    .unwrap_or_default()
            }),
        avatar_color: update
            .avatar_color
            .as_deref()
            .map(str::trim)
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| {
                current
                    .as_ref()
                    .map(|profile| profile.avatar_color.clone())
                    .unwrap_or_default()
            }),
    };
    write_sand_profile_file(&path, &profile)?;
    Ok(profile)
}

pub fn get_agent_profile_text(agent_dir: &Path) -> Option<SandAgentProfile> {
    read_sand_profile_file(get_sand_profile_path(agent_dir))
}

pub fn get_agent_avatar(
    agent_dir: &Path,
    db_path: &Path,
    busy_timeout_ms: u64,
) -> AgentAvatarResponse {
    let legacy_profile_avatar =
        read_legacy_profile_avatar_field(get_sand_profile_path(agent_dir));
    let derived = resolve_derived_avatar_filename(
        agent_dir,
        legacy_profile_avatar.as_deref(),
    );
    let avatar = derived
        .as_deref()
        .and_then(|candidate| read_avatar_within_dir(agent_dir, candidate))
        .or_else(|| read_legacy_stored_avatar(agent_dir, db_path, busy_timeout_ms));
    avatar
        .map(|avatar| AgentAvatarResponse {
            version: Some(avatar.version),
            data_url: Some(avatar.data_url),
        })
        .unwrap_or_default()
}

pub fn get_agent_avatar_png(
    agent_dir: &Path,
    db_path: &Path,
    busy_timeout_ms: u64,
) -> Option<Vec<u8>> {
    let legacy_profile_avatar =
        read_legacy_profile_avatar_field(get_sand_profile_path(agent_dir));
    let derived = resolve_derived_avatar_filename(
        agent_dir,
        legacy_profile_avatar.as_deref(),
    );
    derived
        .as_deref()
        .and_then(|candidate| read_avatar_bytes_within_dir(agent_dir, candidate))
        .or_else(|| {
            let state = read_persisted_agent_serde_snapshot(db_path, busy_timeout_ms).ok()?;
            let candidate = state.profile.avatar_path.as_deref()?;
            read_avatar_bytes_within_dir(agent_dir, candidate)
        })
}

fn read_legacy_stored_avatar(
    agent_dir: &Path,
    db_path: &Path,
    busy_timeout_ms: u64,
) -> Option<AvatarData> {
    let state = read_persisted_agent_serde_snapshot(db_path, busy_timeout_ms).ok()?;
    let candidate = state.profile.avatar_path.as_deref()?;
    read_avatar_within_dir(agent_dir, candidate)
}

pub fn avatar_basename(path: &Path) -> String {
    path.file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default()
}
