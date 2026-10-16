pub fn get_iso_date_year_string(date_string: &str) -> Result<&str, String> {
    let bytes = date_string.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return Err(format!("Expected ISO date string, got: {date_string}"));
    }
    Ok(&date_string[..4])
}

pub fn build_web_search_year_guidance(prompt_date_string: &str) -> Result<String, String> {
    let current_year = get_iso_date_year_string(prompt_date_string)?;
    let previous_year = current_year
        .parse::<i32>()
        .ok()
        .map(|year| (year - 1).to_string())
        .unwrap_or_else(|| current_year.to_string());
    Ok(format!(
        "IMPORTANT - Use the correct year in search queries:\n- Today's date is {prompt_date_string}. You MUST use this year when searching for recent information, documentation, or current events.\n- Example: If today is {prompt_date_string} and the user asks for \"latest React docs\", search for \"React documentation {current_year}\", NOT \"React documentation {previous_year}\""
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn guidance_uses_current_and_previous_year() {
        let guidance = build_web_search_year_guidance("2026-09-25").unwrap();
        assert!(guidance.contains("React documentation 2026"));
        assert!(guidance.contains("React documentation 2025"));
        assert!(get_iso_date_year_string("2026/09/25").is_err());
    }
}
