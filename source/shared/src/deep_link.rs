use url::Url;

pub const SAND_DEEP_LINK_SCHEME: &str = "sand";
pub const SAND_DEEP_LINK_AUTHORITY: &str = "app";
pub const SAND_HTTPS_DEEP_LINK_ORIGIN: &str = "https://cursor.com";
pub const SAND_HTTPS_DEEP_LINK_PATH_PREFIX: &str = "/sand/link";
pub const SAND_DEEP_LINK_MAX_LENGTH: usize = 2_048;
pub const SAND_PLUGIN_DEEP_LINK_PATH: &str = "/v1/plugin/add";
pub const SAND_OPEN_DEEP_LINK_URL: &str = "sand://app/v1/open";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeepLinkSource {
    Protocol,
    Https,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SandDeepLink {
    Info { source: DeepLinkSource },
    PluginAdd { plugin_id: String, source: DeepLinkSource },
    Open { source: DeepLinkSource },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedSandDeepLink {
    pub link: SandDeepLink,
    pub canonical_url: String,
}

fn is_printable_ascii(value: &str) -> bool {
    value.bytes().all(|byte| (33..=126).contains(&byte))
}

fn has_valid_percent_encoding(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return false;
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    true
}

fn has_canonical_path_section(raw: &str) -> bool {
    let before_query = raw.split_once('?').map(|(left, _)| left).unwrap_or(raw);
    !before_query.contains('%')
        && !before_query
            .split('/')
            .any(|segment| segment == "." || segment == "..")
}

fn is_plugin_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 19
        && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn query_pairs(url: &Url) -> Vec<(String, String)> {
    url.query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect()
}

pub fn canonical_sand_deep_link_url(link: &SandDeepLink) -> String {
    match link {
        SandDeepLink::Info { .. } => "sand://app/v1/info?topic=deep-links".into(),
        SandDeepLink::PluginAdd { plugin_id, .. } => {
            format!("sand://app{SAND_PLUGIN_DEEP_LINK_PATH}?id={plugin_id}")
        }
        SandDeepLink::Open { .. } => SAND_OPEN_DEEP_LINK_URL.into(),
    }
}

fn parse_plugin_add(url: &Url, source: DeepLinkSource) -> Option<ParsedSandDeepLink> {
    let entries = query_pairs(url);
    if entries.len() != 1 || entries[0].0 != "id" || !is_plugin_id(&entries[0].1) {
        return None;
    }
    let link = SandDeepLink::PluginAdd {
        plugin_id: entries[0].1.clone(),
        source,
    };
    Some(ParsedSandDeepLink {
        canonical_url: canonical_sand_deep_link_url(&link),
        link,
    })
}

fn parse_open(url: &Url, source: DeepLinkSource) -> Option<ParsedSandDeepLink> {
    if url.query().is_some() {
        return None;
    }
    let link = SandDeepLink::Open { source };
    Some(ParsedSandDeepLink {
        canonical_url: canonical_sand_deep_link_url(&link),
        link,
    })
}

pub fn parse_sand_deep_link(raw: &str) -> Option<ParsedSandDeepLink> {
    if raw.is_empty()
        || raw.len() > SAND_DEEP_LINK_MAX_LENGTH
        || !is_printable_ascii(raw)
        || raw.contains('#')
        || raw.contains('\\')
        || !has_valid_percent_encoding(raw)
        || !has_canonical_path_section(raw)
    {
        return None;
    }

    let lower = raw.to_ascii_lowercase();
    let source = if lower.starts_with("sand:") {
        DeepLinkSource::Protocol
    } else if lower.starts_with("https:") {
        DeepLinkSource::Https
    } else {
        return None;
    };

    let url = Url::parse(raw).ok()?;
    if !url.username().is_empty() || url.password().is_some() || url.port().is_some() {
        return None;
    }

    let path = url.path();
    match source {
        DeepLinkSource::Protocol => {
            if url.scheme() != SAND_DEEP_LINK_SCHEME || url.host_str() != Some(SAND_DEEP_LINK_AUTHORITY) {
                return None;
            }
            if path == SAND_PLUGIN_DEEP_LINK_PATH {
                return parse_plugin_add(&url, source);
            }
            if path == "/v1/open" {
                return parse_open(&url, source);
            }
            if path != "/v1/info" {
                return None;
            }
        }
        DeepLinkSource::Https => {
            if url.scheme() != "https" || url.host_str() != Some("cursor.com") {
                return None;
            }
            if path == "/sand/link/v1/plugin/add" {
                return parse_plugin_add(&url, source);
            }
            if path == "/sand/link/v1/open" {
                return parse_open(&url, source);
            }
            if path != "/sand/link/v1/info" {
                return None;
            }
        }
    }

    let entries = query_pairs(&url);
    if entries.len() != 1 || entries[0].0 != "topic" || entries[0].1 != "deep-links" {
        return None;
    }
    let link = SandDeepLink::Info { source };
    Some(ParsedSandDeepLink {
        canonical_url: canonical_sand_deep_link_url(&link),
        link,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_protocol_and_https_forms_to_same_canonical_shape() {
        let protocol = parse_sand_deep_link("sand://app/v1/info?topic=deep-links").unwrap();
        let https = parse_sand_deep_link("https://cursor.com/sand/link/v1/info?topic=deep-links").unwrap();
        assert_eq!(protocol.canonical_url, "sand://app/v1/info?topic=deep-links");
        assert_eq!(https.canonical_url, protocol.canonical_url);

        let plugin = parse_sand_deep_link("sand://app/v1/plugin/add?id=12345").unwrap();
        assert_eq!(plugin.canonical_url, "sand://app/v1/plugin/add?id=12345");
    }

    #[test]
    fn rejects_credentials_fragments_traversal_bad_percent_and_unknown_query() {
        assert!(parse_sand_deep_link("sand://user:pass@app/v1/open").is_none());
        assert!(parse_sand_deep_link("sand://app/v1/open#x").is_none());
        assert!(parse_sand_deep_link("sand://app/v1/../open").is_none());
        assert!(parse_sand_deep_link("sand://app/v1/open?x=1").is_none());
        assert!(parse_sand_deep_link("sand://app/v1/info?topic=%ZZ").is_none());
        assert!(parse_sand_deep_link("https://evil.example/sand/link/v1/open").is_none());
    }
}
