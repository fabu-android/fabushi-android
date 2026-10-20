use url::Url;

pub const MAX_BLOCKED_HOST_LENGTH: usize = 100;
pub const MAX_BLOCKED_URL_LENGTH: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BotBlockConfidence {
    High,
    Low,
}

impl BotBlockConfidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Low => "low",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct StringConditions {
    pub equals: &'static [&'static str],
    pub suffix: &'static [&'static str],
    pub prefix: &'static [&'static str],
    pub includes: &'static [&'static str],
    pub starts_with: &'static [&'static str],
}

impl StringConditions {
    const fn empty() -> Self {
        Self { equals: &[], suffix: &[], prefix: &[], includes: &[], starts_with: &[] }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BotBlockSignature {
    pub family: &'static str,
    pub confidence: BotBlockConfidence,
    pub host: Option<StringConditions>,
    pub path: Option<StringConditions>,
    pub title: Option<StringConditions>,
}

const fn equals(values: &'static [&'static str]) -> StringConditions {
    StringConditions { equals: values, ..StringConditions::empty() }
}
const fn suffix(values: &'static [&'static str]) -> StringConditions {
    StringConditions { suffix: values, ..StringConditions::empty() }
}
const fn host_equals_suffix(exact: &'static [&'static str], suffixes: &'static [&'static str]) -> StringConditions {
    StringConditions { equals: exact, suffix: suffixes, ..StringConditions::empty() }
}
const fn path_prefix(values: &'static [&'static str]) -> StringConditions {
    StringConditions { prefix: values, ..StringConditions::empty() }
}
const fn path_includes(values: &'static [&'static str]) -> StringConditions {
    StringConditions { includes: values, ..StringConditions::empty() }
}
const fn path_equals_prefix(exact: &'static [&'static str], prefixes: &'static [&'static str]) -> StringConditions {
    StringConditions { equals: exact, prefix: prefixes, ..StringConditions::empty() }
}
const fn title_equals(values: &'static [&'static str]) -> StringConditions { equals(values) }
const fn title_starts_with(values: &'static [&'static str]) -> StringConditions {
    StringConditions { starts_with: values, ..StringConditions::empty() }
}

pub const BOT_BLOCK_SIGNATURES: &[BotBlockSignature] = &[
    BotBlockSignature { family: "google_sorry", confidence: BotBlockConfidence::High, host: Some(host_equals_suffix(&["google.com"], &[".google.com"])), path: Some(path_prefix(&["/sorry"])), title: None },
    BotBlockSignature { family: "google_signin_rejected", confidence: BotBlockConfidence::High, host: Some(equals(&["accounts.google.com"])), path: Some(path_includes(&["/signin/rejected"])), title: None },
    BotBlockSignature { family: "google_device_redirect", confidence: BotBlockConfidence::High, host: Some(equals(&["g.co"])), path: Some(path_equals_prefix(&["/sc"], &["/sc/"])), title: None },
    BotBlockSignature { family: "recaptcha", confidence: BotBlockConfidence::Low, host: None, path: Some(path_includes(&["/recaptcha/api2/", "/recaptcha/enterprise/"])), title: None },
    BotBlockSignature { family: "cloudflare_challenge", confidence: BotBlockConfidence::High, host: None, path: Some(path_includes(&["/cdn-cgi/challenge-platform/"])), title: None },
    BotBlockSignature { family: "cloudflare_challenge", confidence: BotBlockConfidence::High, host: Some(equals(&["challenges.cloudflare.com"])), path: None, title: None },
    BotBlockSignature { family: "cloudflare_challenge", confidence: BotBlockConfidence::High, host: None, path: None, title: Some(title_starts_with(&["Just a moment", "Attention Required! | Cloudflare"])) },
    BotBlockSignature { family: "hcaptcha", confidence: BotBlockConfidence::Low, host: Some(host_equals_suffix(&["hcaptcha.com"], &[".hcaptcha.com"])), path: None, title: None },
    BotBlockSignature { family: "arkose", confidence: BotBlockConfidence::High, host: Some(suffix(&[".arkoselabs.com", ".funcaptcha.com"])), path: None, title: None },
    BotBlockSignature { family: "linkedin_checkpoint", confidence: BotBlockConfidence::High, host: Some(host_equals_suffix(&["linkedin.com"], &[".linkedin.com"])), path: Some(path_prefix(&["/checkpoint/challenge"])), title: None },
    BotBlockSignature { family: "datadome", confidence: BotBlockConfidence::High, host: Some(host_equals_suffix(&["captcha-delivery.com", "captcha.datadome.co"], &[".captcha-delivery.com"])), path: None, title: None },
    BotBlockSignature { family: "perimeterx", confidence: BotBlockConfidence::High, host: None, path: Some(path_includes(&["/px/captcha"])), title: None },
    BotBlockSignature { family: "perimeterx", confidence: BotBlockConfidence::High, host: Some(host_equals_suffix(&["captcha.px-cdn.net"], &[".px-cloud.net"])), path: None, title: None },
    BotBlockSignature { family: "perimeterx", confidence: BotBlockConfidence::High, host: None, path: None, title: Some(title_equals(&["Access to this page has been denied"])) },
    BotBlockSignature { family: "imperva", confidence: BotBlockConfidence::High, host: None, path: Some(path_includes(&["/_Incapsula_Resource"])), title: None },
    BotBlockSignature { family: "distil", confidence: BotBlockConfidence::High, host: None, path: None, title: Some(title_equals(&["Pardon Our Interruption"])) },
    BotBlockSignature { family: "aws_waf", confidence: BotBlockConfidence::High, host: Some(suffix(&[".token.awswaf.com"])), path: None, title: None },
    BotBlockSignature { family: "vercel_checkpoint", confidence: BotBlockConfidence::High, host: None, path: None, title: Some(title_starts_with(&["Vercel Security Checkpoint"])) },
    BotBlockSignature { family: "vercel_checkpoint", confidence: BotBlockConfidence::High, host: None, path: Some(path_includes(&["/.well-known/vercel/security/"])), title: None },
    BotBlockSignature { family: "generic_access_denied", confidence: BotBlockConfidence::Low, host: None, path: None, title: Some(title_equals(&["Access Denied"])) },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotBlockHit {
    pub family: &'static str,
    pub confidence: BotBlockConfidence,
    pub blocked_host: String,
    pub blocked_url: String,
}

fn matches_host(conditions: StringConditions, host: &str) -> bool {
    conditions.equals.contains(&host) || conditions.suffix.iter().any(|suffix| host.ends_with(suffix))
}
fn matches_path(conditions: StringConditions, path: &str) -> bool {
    conditions.equals.contains(&path)
        || conditions.prefix.iter().any(|prefix| path.starts_with(prefix))
        || conditions.includes.iter().any(|part| path.contains(part))
}
fn matches_title(conditions: StringConditions, title: &str) -> bool {
    conditions.equals.contains(&title)
        || conditions.starts_with.iter().any(|prefix| title.starts_with(prefix))
}
fn truncate_chars(value: &str, limit: usize) -> String { value.chars().take(limit).collect() }

pub fn classify_bot_block_page(url: &str, title: &str) -> Option<BotBlockHit> {
    let parsed = Url::parse(url).ok()?;
    let hostname = parsed.host_str()?.to_ascii_lowercase();
    let hostname = hostname.strip_prefix("www.").unwrap_or(&hostname).to_string();
    let pathname = parsed.path();
    let title = title.trim();
    for signature in BOT_BLOCK_SIGNATURES {
        if signature.host.is_some_and(|conditions| !matches_host(conditions, &hostname))
            || signature.path.is_some_and(|conditions| !matches_path(conditions, pathname))
            || signature.title.is_some_and(|conditions| !matches_title(conditions, title))
        {
            continue;
        }
        let origin = parsed.origin().ascii_serialization();
        return Some(BotBlockHit {
            family: signature.family,
            confidence: signature.confidence,
            blocked_host: truncate_chars(&hostname, MAX_BLOCKED_HOST_LENGTH),
            blocked_url: truncate_chars(&format!("{origin}{pathname}"), MAX_BLOCKED_URL_LENGTH),
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_frozen_google_and_title_signatures_without_query_leakage() {
        let hit = classify_bot_block_page(
            "https://www.google.com/sorry/index?secret=must-not-leak",
            "Automated queries",
        ).expect("google sorry");
        assert_eq!(hit.family, "google_sorry");
        assert_eq!(hit.confidence, BotBlockConfidence::High);
        assert_eq!(hit.blocked_host, "google.com");
        assert_eq!(hit.blocked_url, "https://www.google.com/sorry/index");

        let hit = classify_bot_block_page("https://example.com/", "Access Denied")
            .expect("title signature");
        assert_eq!(hit.family, "generic_access_denied");
        assert_eq!(hit.confidence, BotBlockConfidence::Low);
    }

    #[test]
    fn harmless_or_invalid_pages_do_not_report_bot_block() {
        assert!(classify_bot_block_page("https://example.com/docs", "Docs").is_none());
        assert!(classify_bot_block_page("not-a-url", "Access Denied").is_none());
    }
}
