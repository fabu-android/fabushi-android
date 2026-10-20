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

fn parse_desktop_git_url(url_string: &str) -> Option<Url> {
    let normalized_url = if url_string.contains('@') && !url_string.starts_with("http") {
        if url_string.starts_with("ssh://") {
            url_string.to_string()
        } else {
            let re = Regex::new(r"^([^@]+)@([^:]+):(.*)$").expect("constant regex");
            match re.captures(url_string) {
                Some(captures) => format!(
                    "ssh://{}@{}/{}",
                    &captures[1], &captures[2], &captures[3]
                ),
                None => url_string.to_string(),
            }
        }
    } else {
        url_string.to_string()
    };
    Url::parse(&normalized_url).ok()
}

/// Direct Rust port of Desktop agent-analytics/commit-scoring/git-repo-utils.ts.
///
/// This helper is intentionally pure: it parses a supplied remote URL into a display/analytics
/// repository name and owns no repository I/O, mutable repository state, persistence, retries, or
/// cancellation. The separate Desktop prompt-local duplicate remains a distinct responsibility.
pub fn extract_repo_name_from_git_url(upstream_url: Option<&str>) -> Option<String> {
    let upstream_url = upstream_url?;
    if upstream_url.is_empty() {
        return None;
    }
    let normalized_upstream_url = upstream_url
        .replacen("https///", "https://", 1)
        .replacen("http///", "http://", 1);

    if normalized_upstream_url.starts_with("gitlab-remote://") {
        if let Ok(url) = Url::parse(&normalized_upstream_url) {
            if let Some(project_id) = url
                .query_pairs()
                .find_map(|(key, value)| (key == "project").then(|| value.into_owned()))
            {
                if project_id.contains('/') {
                    return Some(project_id);
                }
            }
        }
    }

    let url = parse_desktop_git_url(&normalized_upstream_url)?;
    let hostname = url.host_str()?.to_ascii_lowercase();
    let mut path_parts = url
        .path_segments()?
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if let Some(last) = path_parts.last_mut() {
        if let Some(stripped) = last.strip_suffix(".git") {
            *last = stripped.to_string();
        }
    }

    let is_azure_devops_host = hostname == "dev.azure.com"
        || hostname == "ssh.dev.azure.com"
        || hostname.ends_with(".visualstudio.com");
    if is_azure_devops_host {
        let mut azure_path_parts = path_parts;
        if hostname == "ssh.dev.azure.com"
            && azure_path_parts
                .first()
                .map(|part| part.eq_ignore_ascii_case("v3"))
                .unwrap_or(false)
        {
            azure_path_parts.remove(0);
        }
        azure_path_parts.retain(|part| !part.eq_ignore_ascii_case("_git"));
        return (!azure_path_parts.is_empty()).then(|| azure_path_parts.join("/"));
    }

    if hostname.contains("github") || hostname.ends_with(".ghe.com") {
        return (path_parts.len() >= 2).then(|| path_parts[..2].join("/"));
    }

    if hostname.contains("gitlab") {
        return (!path_parts.is_empty()).then(|| path_parts.join("/"));
    }

    if hostname.contains("bitbucket") || hostname.contains("stash") {
        if path_parts.len() >= 3 && path_parts[0].eq_ignore_ascii_case("scm") {
            return Some(path_parts[1..3].join("/"));
        }
        return (path_parts.len() >= 2).then(|| path_parts[..2].join("/"));
    }

    let has_gerrit_path = path_parts
        .first()
        .map(|part| part.eq_ignore_ascii_case("gerrit"))
        .unwrap_or(false);
    if hostname.contains("gerrit") || has_gerrit_path {
        let mut gerrit_path_parts = path_parts;
        if has_gerrit_path {
            gerrit_path_parts.remove(0);
        }
        if gerrit_path_parts
            .first()
            .map(|part| part.eq_ignore_ascii_case("a"))
            .unwrap_or(false)
        {
            gerrit_path_parts.remove(0);
        }
        return (!gerrit_path_parts.is_empty()).then(|| gerrit_path_parts.join("/"));
    }

    (path_parts.len() >= 2).then(|| path_parts[..2].join("/"))
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

    #[test]
    fn desktop_git_repo_utils_matrix_matches_frozen_helper() {
        let cases = [
            (None, None),
            (Some(""), None),
            (Some("not a git url"), None),
            (Some("https///github.com/openai/example.git"), Some("openai/example")),
            (Some("http///github.com/openai/example.git"), Some("openai/example")),
            (Some("git@github.com:openai/example.git"), Some("openai/example")),
            (Some("ssh://git@github.com/openai/example.git"), Some("openai/example")),
            (
                Some("gitlab-remote://repo?project=group%2Fsub%2Frepo"),
                Some("group/sub/repo"),
            ),
            (Some("gitlab-remote://repo?project=123"), None),
            (
                Some("https://dev.azure.com/org/project/_git/repo.git"),
                Some("org/project/repo"),
            ),
            (
                Some("ssh://git@ssh.dev.azure.com/v3/org/project/repo"),
                Some("org/project/repo"),
            ),
            (
                Some("https://org.visualstudio.com/project/_git/repo.git"),
                Some("project/repo"),
            ),
            (
                Some("https://company.ghe.com/org/repo/ignored.git"),
                Some("org/repo"),
            ),
            (
                Some("https://gitlab.com/group/subgroup/repo.git"),
                Some("group/subgroup/repo"),
            ),
            (
                Some("https://bitbucket.org/workspace/repo.git"),
                Some("workspace/repo"),
            ),
            (
                Some("https://stash.example.com/scm/team/repo.git"),
                Some("team/repo"),
            ),
            (
                Some("https://gerrit.example.com/a/project/subproject.git"),
                Some("project/subproject"),
            ),
            (
                Some("https://code.example.com/gerrit/a/project.git"),
                Some("project"),
            ),
            (
                Some("https://code.example.com/team/repo/ignored.git"),
                Some("team/repo"),
            ),
            (Some("https://code.example.com/repo.git"), None),
        ];

        for (input, expected) in cases {
            assert_eq!(
                extract_repo_name_from_git_url(input).as_deref(),
                expected,
                "unexpected repository name for {input:?}"
            );
        }
    }
}
