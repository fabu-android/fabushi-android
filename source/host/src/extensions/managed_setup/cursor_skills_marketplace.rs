use std::collections::BTreeSet;

use std::fmt;

use crate::mcp_auth::dashboard_mcp_auth_backend::CursorDashboardMcpAuthBackend;

use super::sand_managed_skills::FetchedManagedSkill;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CursorBackendError {
    InvalidProto(String),
    Transport(String),
}

impl fmt::Display for CursorBackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProto(message) => write!(f, "invalid Dashboard protobuf: {message}"),
            Self::Transport(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for CursorBackendError {}

pub const DASHBOARD_GET_MANAGED_SKILLS_PATH: &str =
    "/aiserver.v1.DashboardService/GetManagedSkills";
pub const DASHBOARD_LIST_MARKETPLACE_PLUGINS_PATH: &str =
    "/aiserver.v1.DashboardService/ListMarketplacePlugins";
pub const CURSOR_MARKETPLACE_REQUEST_TIMEOUT_MS: u64 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketplaceSkill {
    pub name: String,
    pub description: String,
    pub source_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketplacePlugin {
    pub id: u64,
    pub name: String,
    pub display_name: String,
    pub logo_url: Option<String>,
    pub publisher_logo_url: Option<String>,
    pub skills: Vec<MarketplaceSkill>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillCatalogEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub publisher: String,
    pub icon_url: Option<String>,
    pub install_url: String,
}

fn decode_varint(input: &[u8], cursor: &mut usize) -> Result<u64, CursorBackendError> {
    let mut value = 0_u64;
    let mut shift = 0_u32;
    while *cursor < input.len() && shift < 64 {
        let byte = input[*cursor];
        *cursor += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
        shift += 7;
    }
    Err(CursorBackendError::InvalidProto("malformed varint".into()))
}

fn read_length_delimited<'a>(
    input: &'a [u8],
    cursor: &mut usize,
) -> Result<&'a [u8], CursorBackendError> {
    let length = usize::try_from(decode_varint(input, cursor)?)
        .map_err(|_| CursorBackendError::InvalidProto("length overflow".into()))?;
    let end = cursor.saturating_add(length);
    if end > input.len() {
        return Err(CursorBackendError::InvalidProto(
            "truncated length-delimited field".into(),
        ));
    }
    let value = &input[*cursor..end];
    *cursor = end;
    Ok(value)
}

fn skip_field(
    input: &[u8],
    cursor: &mut usize,
    wire_type: u8,
) -> Result<(), CursorBackendError> {
    match wire_type {
        0 => {
            let _ = decode_varint(input, cursor)?;
        }
        1 => *cursor = cursor.saturating_add(8),
        2 => {
            let _ = read_length_delimited(input, cursor)?;
        }
        5 => *cursor = cursor.saturating_add(4),
        other => {
            return Err(CursorBackendError::InvalidProto(format!(
                "unsupported wire type {other}"
            )));
        }
    }
    if *cursor > input.len() {
        return Err(CursorBackendError::InvalidProto(
            "truncated protobuf field".into(),
        ));
    }
    Ok(())
}

fn repeated_message_fields<'a>(
    input: &'a [u8],
    wanted_field: u64,
) -> Result<Vec<&'a [u8]>, CursorBackendError> {
    let mut cursor = 0usize;
    let mut values = Vec::new();
    while cursor < input.len() {
        let key = decode_varint(input, &mut cursor)?;
        let field = key >> 3;
        let wire = (key & 0x07) as u8;
        if field == wanted_field && wire == 2 {
            values.push(read_length_delimited(input, &mut cursor)?);
        } else {
            skip_field(input, &mut cursor, wire)?;
        }
    }
    Ok(values)
}

fn optional_varint_field(
    input: &[u8],
    wanted_field: u64,
) -> Result<Option<u64>, CursorBackendError> {
    let mut cursor = 0usize;
    while cursor < input.len() {
        let key = decode_varint(input, &mut cursor)?;
        let field = key >> 3;
        let wire = (key & 0x07) as u8;
        if field == wanted_field && wire == 0 {
            return Ok(Some(decode_varint(input, &mut cursor)?));
        }
        skip_field(input, &mut cursor, wire)?;
    }
    Ok(None)
}

fn first_string_field(
    input: &[u8],
    wanted_field: u64,
) -> Result<Option<String>, CursorBackendError> {
    let mut cursor = 0usize;
    while cursor < input.len() {
        let key = decode_varint(input, &mut cursor)?;
        let field = key >> 3;
        let wire = (key & 0x07) as u8;
        if field == wanted_field && wire == 2 {
            let raw = read_length_delimited(input, &mut cursor)?;
            return std::str::from_utf8(raw)
                .map(|value| Some(value.to_string()))
                .map_err(|_| CursorBackendError::InvalidProto("string field is not UTF-8".into()));
        }
        skip_field(input, &mut cursor, wire)?;
    }
    Ok(None)
}

pub fn decode_managed_skills(
    response: &[u8],
) -> Result<Vec<FetchedManagedSkill>, CursorBackendError> {
    let mut skills = Vec::new();
    for raw in repeated_message_fields(response, 1)? {
        let id = first_string_field(raw, 1)?.unwrap_or_default();
        let description = first_string_field(raw, 2)?.unwrap_or_default();
        let content = first_string_field(raw, 3)?.unwrap_or_default();
        let enabled = optional_varint_field(raw, 7)?.map(|value| value != 0).unwrap_or(true);
        skills.push(FetchedManagedSkill {
            id,
            description,
            content,
            enabled,
        });
    }
    Ok(skills)
}

fn decode_publisher_logo(raw: &[u8]) -> Result<Option<String>, CursorBackendError> {
    first_string_field(raw, 10)
}

fn decode_marketplace_skill(raw: &[u8]) -> Result<Option<MarketplaceSkill>, CursorBackendError> {
    let name = first_string_field(raw, 1)?.unwrap_or_default();
    let source_url = first_string_field(raw, 4)?.unwrap_or_default();
    if name.is_empty() || source_url.is_empty() {
        return Ok(None);
    }
    Ok(Some(MarketplaceSkill {
        name,
        description: first_string_field(raw, 2)?.unwrap_or_default(),
        source_url,
    }))
}

pub fn decode_marketplace_plugins(
    response: &[u8],
) -> Result<Vec<MarketplacePlugin>, CursorBackendError> {
    let mut plugins = Vec::new();
    for raw in repeated_message_fields(response, 1)? {
        let id = optional_varint_field(raw, 1)?.unwrap_or_default();
        let name = first_string_field(raw, 2)?.unwrap_or_default();
        let display_name = first_string_field(raw, 3)?.unwrap_or_default();
        let logo_url = first_string_field(raw, 10)?;
        let publisher_logo_url = repeated_message_fields(raw, 17)?
            .into_iter()
            .next()
            .map(decode_publisher_logo)
            .transpose()?
            .flatten();
        let mut skills = Vec::new();
        for skill in repeated_message_fields(raw, 28)? {
            if let Some(skill) = decode_marketplace_skill(skill)? {
                skills.push(skill);
            }
        }
        plugins.push(MarketplacePlugin {
            id,
            name,
            display_name,
            logo_url,
            publisher_logo_url,
            skills,
        });
    }
    Ok(plugins)
}

pub fn skill_catalog_from_plugins(plugins: &[MarketplacePlugin]) -> Vec<SkillCatalogEntry> {
    let mut seen = BTreeSet::new();
    let mut entries = Vec::new();
    for plugin in plugins {
        let icon_url = plugin
            .publisher_logo_url
            .clone()
            .or_else(|| plugin.logo_url.clone());
        let publisher = if plugin.display_name.is_empty() {
            plugin.name.clone()
        } else {
            plugin.display_name.clone()
        };
        for skill in &plugin.skills {
            let key = skill.name.to_lowercase();
            if !seen.insert(key) {
                continue;
            }
            entries.push(SkillCatalogEntry {
                id: format!("plugin:{}:{}", plugin.id, skill.name),
                name: skill.name.clone(),
                description: skill.description.clone(),
                publisher: publisher.clone(),
                icon_url: icon_url.clone(),
                install_url: skill.source_url.clone(),
            });
        }
    }
    entries.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    entries
}

pub fn fetch_sand_managed_skills(
    backend: &CursorDashboardMcpAuthBackend,
) -> Result<Vec<FetchedManagedSkill>, CursorBackendError> {
    let response = backend
        .send_managed_setup_unary(
            DASHBOARD_GET_MANAGED_SKILLS_PATH,
            &[],
            CURSOR_MARKETPLACE_REQUEST_TIMEOUT_MS,
        )
        .map_err(CursorBackendError::Transport)?;
    decode_managed_skills(&response)
}

pub fn fetch_skill_catalog(
    backend: &CursorDashboardMcpAuthBackend,
) -> Result<Vec<SkillCatalogEntry>, CursorBackendError> {
    // ListMarketplacePluginsRequest.exclude_cloud_agent_plugins = true (field 8, varint).
    let response = backend
        .send_managed_setup_unary(
            DASHBOARD_LIST_MARKETPLACE_PLUGINS_PATH,
            &[0x40, 0x01],
            CURSOR_MARKETPLACE_REQUEST_TIMEOUT_MS,
        )
        .map_err(CursorBackendError::Transport)?;
    Ok(skill_catalog_from_plugins(&decode_marketplace_plugins(&response)?))
}

#[cfg(test)]
mod android_parity_tests {
    use super::*;

    fn push_varint(mut value: u64, output: &mut Vec<u8>) {
        loop {
            if value < 0x80 {
                output.push(value as u8);
                return;
            }
            output.push(((value as u8) & 0x7f) | 0x80);
            value >>= 7;
        }
    }

    fn field_bytes(field: u64, value: &[u8], output: &mut Vec<u8>) {
        push_varint((field << 3) | 2, output);
        push_varint(value.len() as u64, output);
        output.extend_from_slice(value);
    }

    #[test]
    fn managed_skill_decoder_preserves_enabled_and_content_fields() {
        let mut skill = Vec::new();
        field_bytes(1, b"review", &mut skill);
        field_bytes(2, b"Managed review", &mut skill);
        field_bytes(3, b"body", &mut skill);
        skill.extend_from_slice(&[0x38, 0x00]);
        let mut response = Vec::new();
        field_bytes(1, &skill, &mut response);
        let decoded = decode_managed_skills(&response).unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].id, "review");
        assert!(!decoded[0].enabled);
    }

    #[test]
    fn marketplace_catalog_deduplicates_skill_names_case_insensitively() {
        let plugins = vec![
            MarketplacePlugin {
                id: 1,
                name: "one".into(),
                display_name: "Publisher One".into(),
                logo_url: None,
                publisher_logo_url: None,
                skills: vec![MarketplaceSkill {
                    name: "Review".into(),
                    description: "first".into(),
                    source_url: "https://example.test/one".into(),
                }],
            },
            MarketplacePlugin {
                id: 2,
                name: "two".into(),
                display_name: "Publisher Two".into(),
                logo_url: None,
                publisher_logo_url: None,
                skills: vec![MarketplaceSkill {
                    name: "review".into(),
                    description: "duplicate".into(),
                    source_url: "https://example.test/two".into(),
                }],
            },
        ];
        let catalog = skill_catalog_from_plugins(&plugins);
        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].name, "Review");
    }
}
