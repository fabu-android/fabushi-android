pub const MCP_SERVER_ID_MAX: u32 = i32::MAX as u32;

pub fn is_mcp_server_id(raw_id: &str) -> bool {
    let id = raw_id.trim();
    !id.is_empty()
        && !id.starts_with('0')
        && id.bytes().all(|value| value.is_ascii_digit())
}

pub fn validate_mcp_server_id(raw_id: &str) -> Result<String, &'static str> {
    let id = raw_id.trim();
    if !is_mcp_server_id(id) {
        return Err("MCP server ID must be a positive decimal string.");
    }
    Ok(id.to_string())
}

pub fn parse_i32_mcp_server_id(raw_id: &str) -> Result<i32, &'static str> {
    let id = validate_mcp_server_id(raw_id)?;
    let parsed = id
        .parse::<u64>()
        .map_err(|_| "MCP server ID is outside the supported range.")?;
    if parsed > u64::from(MCP_SERVER_ID_MAX) {
        return Err("MCP server ID is outside the supported range.");
    }
    Ok(parsed as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_positive_decimal_server_ids() {
        assert!(is_mcp_server_id("1"));
        assert!(is_mcp_server_id(" 2147483647 "));
        assert!(!is_mcp_server_id(""));
        assert!(!is_mcp_server_id("0"));
        assert!(!is_mcp_server_id("01"));
        assert!(!is_mcp_server_id("-1"));
        assert!(!is_mcp_server_id("+1"));
        assert!(!is_mcp_server_id("1.0"));
        assert!(!is_mcp_server_id("1\n2"));
    }

    #[test]
    fn parse_rejects_values_above_signed_int32_range() {
        assert_eq!(parse_i32_mcp_server_id("2147483647"), Ok(i32::MAX));
        assert_eq!(
            parse_i32_mcp_server_id("2147483648"),
            Err("MCP server ID is outside the supported range.")
        );
        assert_eq!(
            parse_i32_mcp_server_id("999999999999999999999999999999"),
            Err("MCP server ID is outside the supported range.")
        );
    }
}
