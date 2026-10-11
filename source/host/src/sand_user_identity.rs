pub const MAX_FULL_NAME_LENGTH: usize = 200;

pub fn normalize_sand_user_full_name(input: &str) -> Option<String> {
    let compact = input.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.is_empty() {
        None
    } else {
        Some(compact.chars().take(MAX_FULL_NAME_LENGTH).collect())
    }
}

pub fn render_user_identity_system_prompt(name: Option<&str>) -> String {
    normalize_sand_user_full_name(name.unwrap_or(""))
        .map(|name| format!(
            "Your user is {name}; when acting through their accounts and apps, speak as them and never refer to them in the third person."
        ))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_single_line_clamped_and_first_person() {
        let normalized = normalize_sand_user_full_name("  Ada\n  Lovelace  ").unwrap();
        assert_eq!(normalized, "Ada Lovelace");
        let prompt = render_user_identity_system_prompt(Some(&normalized));
        assert!(prompt.contains("speak as them"));
        assert!(prompt.contains("never refer to them in the third person."));
        assert_eq!(normalize_sand_user_full_name(&"x".repeat(250)).unwrap().chars().count(), 200);
    }
}
