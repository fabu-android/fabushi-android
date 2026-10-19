pub fn attachment_extension(name_or_path: &str) -> Option<String> {
    let base = name_or_path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default();
    let dot = base.rfind('.')?;
    if dot == 0 || dot + 1 >= base.len() {
        return None;
    }
    Some(base[dot + 1..].to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_policy_rejects_hidden_and_trailing_dot_names() {
        assert_eq!(attachment_extension("/a/B.CSV").as_deref(), Some("csv"));
        assert_eq!(attachment_extension(".env"), None);
        assert_eq!(attachment_extension("file."), None);
    }
}
