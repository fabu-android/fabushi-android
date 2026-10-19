use url::Url;

pub fn is_valid_attachment_url(raw_url: &str) -> bool {
    Url::parse(raw_url)
        .ok()
        .is_some_and(|url| matches!(url.scheme(), "file" | "https"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn attachments_allow_only_file_and_https_urls() {
        assert!(is_valid_attachment_url("file:///tmp/a.png"));
        assert!(is_valid_attachment_url("https://example.com/a.png"));
        assert!(!is_valid_attachment_url("http://example.com/a.png"));
        assert!(!is_valid_attachment_url("javascript:alert(1)"));
        assert!(!is_valid_attachment_url("not a url"));
    }
}
