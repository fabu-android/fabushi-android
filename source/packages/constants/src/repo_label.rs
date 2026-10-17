use regex::Regex;
use url::Url;

const KNOWN_GIT_HOSTING_DOMAINS: &[&str] = &[
    "github.com", "gitlab.com", "bitbucket.org", "bitbucket.com", "codeberg.org", "gitea.com", "sr.ht",
];

pub fn is_known_git_hosting_domain(hostname: &str) -> bool {
    let lower = hostname.to_ascii_lowercase();
    KNOWN_GIT_HOSTING_DOMAINS
        .iter()
        .any(|domain| lower == *domain || lower.ends_with(&format!(".{domain}")))
}

fn is_origin_repo_host(hostname: &str) -> bool {
    Regex::new(r"(?i)^origin(?:-[a-z0-9]+)?\.cursor\.com$")
        .expect("constant regex")
        .is_match(hostname)
}

fn trim_repo_path(pathname: &str) -> String {
    pathname
        .trim_matches('/')
        .strip_suffix(".git")
        .unwrap_or(pathname.trim_matches('/'))
        .to_string()
}

fn extract_owner_repo_from_path(pathname: &str) -> Option<String> {
    let trimmed = trim_repo_path(pathname);
    let mut parts = trimmed.split('/').filter(|part| !part.is_empty());
    let owner = parts.next()?;
    let rest = parts.collect::<Vec<_>>();
    (!rest.is_empty()).then(|| format!("{owner}/{}", rest.join("/")))
}

fn extract_owner_repo_from_origin_path(pathname: &str) -> Option<String> {
    let parts = trim_repo_path(pathname)
        .split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    match parts.as_slice() {
        [git, owner, repo] if git.eq_ignore_ascii_case("git") => Some(format!("{owner}/{repo}")),
        [owner, repo] if !owner.eq_ignore_ascii_case("git") => Some(format!("{owner}/{repo}")),
        _ => None,
    }
}

fn rewrite_scp_git_url(raw: &str) -> String {
    if raw.contains("://") {
        return raw.to_string();
    }
    let re = Regex::new(r"^[^@/]+@([^:]+):(.+)$").expect("constant regex");
    match re.captures(raw) {
        Some(captures) => format!("https://{}/{}", &captures[1], &captures[2]),
        None => raw.to_string(),
    }
}

pub fn parse_repo_name_from_url(repo_url: Option<&str>) -> Option<String> {
    let trimmed = repo_url?.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains("://") {
        let parsed = Url::parse(trimmed).ok()?;
        return if is_origin_repo_host(parsed.host_str()?) {
            extract_owner_repo_from_origin_path(parsed.path())
        } else {
            extract_owner_repo_from_path(parsed.path())
        };
    }

    let slash = trimmed.find('/')?;
    let first = &trimmed[..slash];
    let rest = &trimmed[slash..];
    let first_without_port = first.split(':').next().unwrap_or(first);
    if is_origin_repo_host(first_without_port) {
        return extract_owner_repo_from_origin_path(rest);
    }
    if is_known_git_hosting_domain(first) {
        extract_owner_repo_from_path(rest)
    } else {
        extract_owner_repo_from_path(&format!("/{trimmed}"))
    }
}

pub fn parse_self_hosted_repo_scope(repo_url: &str) -> Option<String> {
    let mut candidate = repo_url.to_string();
    if !candidate.contains("://") {
        let slash = candidate.find('/')?;
        let first = &candidate[..slash];
        if !first.contains('.') || is_known_git_hosting_domain(first) {
            return None;
        }
        candidate = format!("https://{candidate}");
    }
    let parsed = Url::parse(&candidate).ok()?;
    let hostname = parsed.host_str()?;
    if is_known_git_hosting_domain(hostname) || is_origin_repo_host(hostname) {
        return None;
    }
    let path = trim_repo_path(parsed.path());
    (!path.is_empty()).then(|| format!("{}/{}", parsed.host_str().unwrap_or_default(), path))
}

pub fn derive_repo_label_value_from_url(repo_url: Option<&str>) -> Option<String> {
    let trimmed = repo_url?.trim();
    if trimmed.is_empty() {
        return None;
    }
    let rewritten = rewrite_scp_git_url(trimmed);
    parse_self_hosted_repo_scope(&rewritten)
        .or_else(|| parse_repo_name_from_url(Some(&rewritten)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_hosts_and_scp_urls_resolve_to_owner_repo() {
        assert!(is_known_git_hosting_domain("enterprise.github.com"));
        assert_eq!(
            derive_repo_label_value_from_url(Some("git@github.com:openai/example.git")).as_deref(),
            Some("openai/example")
        );
        assert_eq!(
            parse_repo_name_from_url(Some("https://gitlab.com/group/repo.git")).as_deref(),
            Some("group/repo")
        );
    }

    #[test]
    fn self_hosted_scope_preserves_host_and_path() {
        assert_eq!(
            derive_repo_label_value_from_url(Some("git.example.org/team/repo.git")).as_deref(),
            Some("git.example.org/team/repo")
        );
        assert!(parse_self_hosted_repo_scope("github.com/openai/example").is_none());
    }
}
