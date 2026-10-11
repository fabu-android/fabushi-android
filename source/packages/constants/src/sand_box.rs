use regex::Regex;
use url::Url;

pub const SAND_BOX_IMAGE_TAG_PREFIX: &str = "sand-box-";
pub const SAND_BOX_IMAGE_TAG_LATEST: &str = "sand-box-latest";
pub const SAND_BOX_PRIMARY_NOVNC_PORT: u16 = 6080;
pub const SAND_BOX_FORK_NOVNC_PORT: u16 = 6081;
pub const SAND_SPECIAL_TREATMENT_NOVNC_PATH: &str = "sand-special-treatment-v1/vnc.html";

pub fn is_short_git_sha(value: &str) -> bool {
    Regex::new(r"^[0-9a-f]{7,40}$")
        .expect("constant regex")
        .is_match(value)
}

pub fn build_sand_box_no_vnc_url(
    proxy_base_url: &str,
    network_token: &str,
    token: Option<&str>,
    special_treatment: bool,
) -> String {
    let wake = "resume_lower_s=900&resume_upper_s=18000";
    let token_param = token.map(|value| format!("token={value}&")).unwrap_or_default();
    let websockify_path =
        format!("websockify?{token_param}network_token={network_token}&{wake}");
    let viewer_path = if special_treatment {
        SAND_SPECIAL_TREATMENT_NOVNC_PATH
    } else {
        "vnc.html"
    };
    format!(
        "{}/{viewer_path}?network_token={network_token}&{wake}&path={}",
        proxy_base_url.trim_end_matches('/'),
        urlencoding::encode(&websockify_path)
    )
}

pub fn is_sand_special_treatment_no_vnc_url(value: &str) -> bool {
    Url::parse(value)
        .ok()
        .is_some_and(|url| {
            url.path()
                .ends_with(&format!("/{SAND_SPECIAL_TREATMENT_NOVNC_PATH}"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_primary_and_special_no_vnc_urls() {
        let normal = build_sand_box_no_vnc_url(
            "https://proxy.example",
            "network",
            Some("session"),
            false,
        );
        assert!(normal.contains("/vnc.html?"));
        assert!(normal.contains("network_token=network"));
        assert!(normal.contains("path=websockify"));

        let special = build_sand_box_no_vnc_url(
            "https://proxy.example/",
            "network",
            None,
            true,
        );
        assert!(is_sand_special_treatment_no_vnc_url(&special));
        assert!(is_short_git_sha("abcdef1"));
        assert!(!is_short_git_sha("ABCDEF1"));
    }
}
