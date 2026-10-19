use regex::Regex;

pub const AGENT_STORE_USER_MOUNT_NAME: &str = "user";
pub const AGENT_STORE_TEAM_MOUNT_NAME: &str = "team";
pub const AGENT_STORE_AUTOMATION_MOUNT_NAME: &str = "automation";
pub const AGENT_STORE_RESERVED_CURSOR_PATH_PREFIX: &str = ".cursor";
pub const NAMED_AGENT_HOME_STORE_MOUNT_NAME: &str = "home";
pub const CURSOR_AGENT_STORE_FILES_DIR_ENV: &str = "CURSOR_AGENT_STORE_FILES_DIR";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserAgentStoreSourceId {
    pub user_id: u64,
    pub team_id: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeamAgentStoreSourceId {
    pub team_id: u64,
}

fn matches(pattern: &str, value: &str) -> bool {
    Regex::new(pattern).expect("constant regex must compile").is_match(value)
}

pub fn parse_user_agent_store_source_id(source_id: &str) -> Option<UserAgentStoreSourceId> {
    let re = Regex::new(r"^(?:t([1-9][0-9]*)-)?u([1-9][0-9]*)$").ok()?;
    let captures = re.captures(source_id)?;
    let user_id = captures.get(2)?.as_str().parse::<u64>().ok().filter(|value| *value > 0)?;
    let team_id = match captures.get(1) {
        Some(value) => Some(
            value
                .as_str()
                .parse::<u64>()
                .ok()
                .filter(|value| *value > 0)?,
        ),
        None => None,
    };
    Some(UserAgentStoreSourceId { user_id, team_id })
}

pub fn parse_team_agent_store_source_id(source_id: &str) -> Option<TeamAgentStoreSourceId> {
    let re = Regex::new(r"^t([1-9][0-9]*)$").ok()?;
    let team_id = re
        .captures(source_id)?
        .get(1)?
        .as_str()
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)?;
    Some(TeamAgentStoreSourceId { team_id })
}

pub fn is_valid_bare_uuid(value: &str) -> bool {
    matches(
        r"(?i)^[0-9a-f]{8}-[0-9a-f]{4}-[1-57][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$",
        value,
    )
}

pub fn is_cloud_agent_store_id(value: &str) -> bool {
    matches(
        r"(?i)^bc-(?:[0-9a-z][0-9a-z-]*-)?[0-9a-f]{8}-[0-9a-f]{4}-[1-57][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$",
        value,
    )
}

pub fn is_agent_store_id(value: &str) -> bool {
    matches(
        r"(?i)^store-[0-9a-f]{8}-[0-9a-f]{4}-[1-57][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$",
        value,
    )
}

pub fn is_agent_store_source_id(value: &str) -> bool {
    is_cloud_agent_store_id(value) || is_valid_bare_uuid(value)
}

pub fn is_agent_store_share_mount_key(value: &str) -> bool {
    matches(r"^store-[A-Za-z0-9_-]{24}$", value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_user_and_team_sources_without_zero_or_overflow() {
        assert_eq!(
            parse_user_agent_store_source_id("t42-u7"),
            Some(UserAgentStoreSourceId { user_id: 7, team_id: Some(42) })
        );
        assert_eq!(
            parse_user_agent_store_source_id("u9"),
            Some(UserAgentStoreSourceId { user_id: 9, team_id: None })
        );
        assert_eq!(parse_team_agent_store_source_id("t42").unwrap().team_id, 42);
        assert!(parse_user_agent_store_source_id("u0").is_none());
        assert!(parse_team_agent_store_source_id("t0").is_none());
    }

    #[test]
    fn validates_store_identifiers_fail_closed() {
        let uuid = "123e4567-e89b-42d3-a456-426614174000";
        assert!(is_valid_bare_uuid(uuid));
        assert!(is_agent_store_id(&format!("store-{uuid}")));
        assert!(is_cloud_agent_store_id(&format!("bc-us-{uuid}")));
        assert!(is_agent_store_source_id(uuid));
        assert!(is_agent_store_share_mount_key("store-abcdefghijklmnopqrstuvwx"));
        assert!(!is_agent_store_share_mount_key("store-too-short"));
    }
}
