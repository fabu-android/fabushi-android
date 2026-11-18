//! Telegram Desktop 1:1 Parity Module: `LocalUrlHandlers`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/core/local_url_handlers.h, Telegram/SourceFiles/core/local_url_handlers.cpp

use serde::{Deserialize, Serialize};

pub const GET_PREMIUM_LOCAL: &str = "internal:tme_getpremium";

/// Represents a matched URL pattern routing handler.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalUrlHandler {
    pub pattern: String,
    pub target_action: String,
}

/// Registry and router for converting web and t.me URLs into internal `tg://` deep links.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalUrlHandlers {
    pub handlers: Vec<LocalUrlHandler>,
}

impl Default for LocalUrlHandlers {
    fn default() -> Self {
        Self {
            handlers: default_registered_handlers(),
        }
    }
}

impl LocalUrlHandlers {
    pub fn new() -> Self {
        Self::default()
    }

    /// Converts HTTP/HTTPS `t.me`, `telegram.me`, `tc://` links into internal `tg://` links.
    pub fn try_convert_url_to_local(mut url: &str) -> String {
        if url.len() > 8192 {
            url = &url[..8192];
        }

        let trimmed = url.trim();

        // TON Connect: tc:// -> tg://sendgrams?startapp=...
        if trimmed.to_ascii_lowercase().starts_with("tc://") {
            let query = trimmed.find('?').map(|pos| &trimmed[pos + 1..]).unwrap_or("");
            return format!("tg://sendgrams?startapp={}", query);
        }

        // TON sites: *.ton -> tonsite://*
        if trimmed.contains(".ton") {
            let lower = trimmed.to_ascii_lowercase();
            if lower.starts_with("https://") {
                let rest = &trimmed["https://".len()..];
                if rest.contains(".ton") {
                    return format!("tonsite://{}", rest);
                }
            } else if lower.starts_with("http://") {
                let rest = &trimmed["http://".len()..];
                if rest.contains(".ton") {
                    return format!("tonsite://{}", rest);
                }
            }
        }

        // Check for t.me or telegram.me or telegram.dog prefix
        let domain_prefixes = [
            "https://t.me/",
            "http://t.me/",
            "https://www.t.me/",
            "http://www.t.me/",
            "https://telegram.me/",
            "http://telegram.me/",
            "https://telegram.dog/",
            "http://telegram.dog/",
        ];

        for prefix in domain_prefixes {
            if let Some(stripped) = strip_case_insensitive_prefix(trimmed, prefix) {
                return convert_tme_path_to_tg(stripped);
            }
        }

        // Subdomain check: e.g. https://username.t.me/
        if let Some(subdomain_url) = convert_subdomain_tme(trimmed) {
            return subdomain_url;
        }

        trimmed.to_string()
    }

    pub fn is_get_premium_local_url(url: &str) -> bool {
        url.starts_with(GET_PREMIUM_LOCAL)
            || url.starts_with("tg://premium_offer")
            || url.starts_with("tg://premium_multigift")
    }

    pub fn is_mini_app_url(url: &str) -> bool {
        url.contains("startapp=") || url.contains("startattach=")
    }

    pub fn internal_passport_or_oauth_link(url: &str) -> bool {
        url.starts_with("tg://passport") || url.starts_with("tg://auth")
    }

    pub fn start_url_requires_activate(url: &str) -> bool {
        !url.starts_with("tg://chats/search")
    }

    /// Parses video timestamps like "1h2m3s", "10m", "45s", or "12:34" into total seconds.
    pub fn parse_video_timestamp(val: &str) -> Option<i64> {
        let trimmed = val.trim();
        if trimmed.is_empty() {
            return None;
        }

        // Format: MM:SS or HH:MM:SS
        if trimmed.contains(':') {
            let parts: Vec<&str> = trimmed.split(':').collect();
            if parts.len() == 2 {
                let m: i64 = parts[0].parse().ok()?;
                let s: i64 = parts[1].parse().ok()?;
                return Some(m * 60 + s);
            } else if parts.len() == 3 {
                let h: i64 = parts[0].parse().ok()?;
                let m: i64 = parts[1].parse().ok()?;
                let s: i64 = parts[2].parse().ok()?;
                return Some(h * 3600 + m * 60 + s);
            }
            return None;
        }

        // Format: 1h2m3s or plain seconds
        if trimmed.chars().all(|c| c.is_ascii_digit()) {
            return trimmed.parse().ok();
        }

        let mut total = 0i64;
        let mut cur = 0i64;
        for ch in trimmed.chars() {
            if ch.is_ascii_digit() {
                cur = cur * 10 + (ch as i64 - '0' as i64);
            } else {
                match ch.to_ascii_lowercase() {
                    'h' => {
                        total += cur * 3600;
                        cur = 0;
                    }
                    'm' => {
                        total += cur * 60;
                        cur = 0;
                    }
                    's' => {
                        total += cur;
                        cur = 0;
                    }
                    _ => return None,
                }
            }
        }
        total += cur;
        Some(total)
    }
}

fn strip_case_insensitive_prefix<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    if text.len() >= prefix.len() && text[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&text[prefix.len()..])
    } else {
        None
    }
}

fn convert_subdomain_tme(url: &str) -> Option<String> {
    let lower = url.to_ascii_lowercase();
    let without_proto = if lower.starts_with("https://") {
        &url["https://".len()..]
    } else if lower.starts_with("http://") {
        &url["http://".len()..]
    } else {
        return None;
    };

    if let Some(tme_idx) = without_proto.find(".t.me") {
        let subdomain = &without_proto[..tme_idx];
        if !subdomain.is_empty() && subdomain != "www" && !subdomain.contains('/') {
            let after_tme = &without_proto[tme_idx + ".t.me".len()..];
            let full_path = format!("{}{}", subdomain, after_tme);
            return Some(convert_tme_path_to_tg(&full_path));
        }
    }
    None
}

fn convert_tme_path_to_tg(path: &str) -> String {
    let trimmed = path.trim_start_matches('/');

    if let Some(stripped) = strip_case_insensitive_prefix(trimmed, "joinchat/") {
        return format!("tg://join?invite={}", stripped);
    }
    if let Some(stripped) = strip_case_insensitive_prefix(trimmed, "+") {
        return format!("tg://join?invite={}", stripped);
    }
    if let Some(stripped) = strip_case_insensitive_prefix(trimmed, "addstickers/") {
        return format!("tg://addstickers?set={}", stripped);
    }
    if let Some(stripped) = strip_case_insensitive_prefix(trimmed, "addtheme/") {
        return format!("tg://addtheme?slug={}", stripped);
    }
    if let Some(stripped) = strip_case_insensitive_prefix(trimmed, "setlanguage/") {
        return format!("tg://setlanguage?lang={}", stripped);
    }
    if let Some(stripped) = strip_case_insensitive_prefix(trimmed, "share/url?") {
        return format!("tg://msg_url?{}", stripped);
    }
    if let Some(stripped) = strip_case_insensitive_prefix(trimmed, "socks?") {
        return format!("tg://socks?{}", stripped);
    }
    if let Some(stripped) = strip_case_insensitive_prefix(trimmed, "proxy?") {
        return format!("tg://proxy?{}", stripped);
    }
    if let Some(stripped) = strip_case_insensitive_prefix(trimmed, "c/") {
        // e.g. c/123456/789 -> tg://privatepost?channel=123456&post=789
        let parts: Vec<&str> = stripped.split('/').collect();
        if parts.len() >= 2 {
            return format!("tg://privatepost?channel={}&post={}", parts[0], parts[1]);
        }
    }
    if trimmed.eq_ignore_ascii_case("settings") {
        return "tg://settings".to_string();
    }
    if let Some(stripped) = strip_case_insensitive_prefix(trimmed, "settings/") {
        return format!("tg://settings/{}", stripped);
    }

    // Default peer / username resolve:
    // e.g. "durov" -> "tg://resolve?domain=durov"
    let (domain_part, query_part) = match trimmed.find('?') {
        Some(idx) => (&trimmed[..idx], &trimmed[idx..]),
        None => (trimmed, ""),
    };

    let segments: Vec<&str> = domain_part.split('/').collect();
    if segments.len() == 1 && !segments[0].is_empty() {
        format!("tg://resolve?domain={}{}", segments[0], query_part)
    } else if segments.len() >= 2 && !segments[0].is_empty() {
        format!(
            "tg://resolve?domain={}&post={}{}",
            segments[0], segments[1], query_part
        )
    } else {
        format!("tg://resolve?domain={}", trimmed)
    }
}

fn default_registered_handlers() -> Vec<LocalUrlHandler> {
    vec![
        LocalUrlHandler {
            pattern: "tg://resolve".to_string(),
            target_action: "resolve_peer".to_string(),
        },
        LocalUrlHandler {
            pattern: "tg://join".to_string(),
            target_action: "join_chat".to_string(),
        },
        LocalUrlHandler {
            pattern: "tg://addstickers".to_string(),
            target_action: "preview_stickers".to_string(),
        },
        LocalUrlHandler {
            pattern: "tg://addtheme".to_string(),
            target_action: "preview_theme".to_string(),
        },
        LocalUrlHandler {
            pattern: "tg://msg_url".to_string(),
            target_action: "share_url".to_string(),
        },
        LocalUrlHandler {
            pattern: "tg://settings".to_string(),
            target_action: "open_settings".to_string(),
        },
    ]
}
