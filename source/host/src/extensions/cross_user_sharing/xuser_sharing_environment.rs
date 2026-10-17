use std::collections::BTreeMap;

use url::Url;

pub const DEFAULT_CURSOR_BACKEND_URL: &str = "https://api2.cursor.sh";
pub const SAND_DEV_XUSER_SHARING_ENV: &str = "SAND_DEV_XUSER_SHARING";
pub const SAND_XUSER_SHARING_ALLOW_PROD_ENV: &str = "SAND_XUSER_SHARING_ALLOW_PROD";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XuserSharingEnvironment {
    pub is_allowed: bool,
    pub reason: Option<String>,
}

fn origin(url: &Url) -> Option<(String, String, Option<u16>)> {
    Some((
        url.scheme().to_ascii_lowercase(),
        url.host_str()?.to_ascii_lowercase(),
        url.port_or_known_default(),
    ))
}

pub fn is_production_backend_url(backend_url: &str) -> bool {
    let Ok(candidate) = Url::parse(backend_url) else {
        return true;
    };
    let Ok(production) = Url::parse(DEFAULT_CURSOR_BACKEND_URL) else {
        return true;
    };
    origin(&candidate) == origin(&production)
}

pub fn resolve_xuser_sharing_environment(
    backend_url: &str,
    env: &BTreeMap<String, String>,
) -> XuserSharingEnvironment {
    let is_dev_build = env.get("SAND_PACKAGED").is_none_or(|value| value != "1")
        || env
            .get("SAND_HOST_DEV_ERROR_DETAIL")
            .is_some_and(|value| value == "1");

    if !is_dev_build {
        return XuserSharingEnvironment {
            is_allowed: true,
            reason: None,
        };
    }

    if env
        .get(SAND_DEV_XUSER_SHARING_ENV)
        .is_none_or(|value| value != "1")
    {
        return XuserSharingEnvironment {
            is_allowed: false,
            reason: Some(format!(
                "cross-user sharing stays OFF on this dev host: a second live box on the same account drains the account's relay events and corrupts prod room delivery. Set {SAND_DEV_XUSER_SHARING_ENV}=1 to opt this box in anyway."
            )),
        };
    }

    if !is_production_backend_url(backend_url) {
        return XuserSharingEnvironment {
            is_allowed: true,
            reason: None,
        };
    }

    if env
        .get(SAND_XUSER_SHARING_ALLOW_PROD_ENV)
        .is_some_and(|value| value == "1")
    {
        return XuserSharingEnvironment {
            is_allowed: true,
            reason: None,
        };
    }

    XuserSharingEnvironment {
        is_allowed: false,
        reason: Some(format!(
            "this dev host is pointed at the PRODUCTION backend; cross-user sharing stays off so it cannot ingest (or steal relay events from) the account's production rooms. Set {SAND_XUSER_SHARING_ALLOW_PROD_ENV}=1 to opt in deliberately."
        )),
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(key, value)| ((*key).into(), (*value).into())).collect()
    }

    #[test]
    fn packaged_build_allows_production_sharing_without_dev_overrides() {
        let resolved = resolve_xuser_sharing_environment(
            DEFAULT_CURSOR_BACKEND_URL,
            &env(&[("SAND_PACKAGED", "1")]),
        );
        assert!(resolved.is_allowed);
        assert!(resolved.reason.is_none());
    }

    #[test]
    fn dev_build_is_fail_closed_until_explicitly_enabled() {
        let resolved = resolve_xuser_sharing_environment("https://staging.example.invalid", &BTreeMap::new());
        assert!(!resolved.is_allowed);
        assert!(resolved.reason.unwrap().contains(SAND_DEV_XUSER_SHARING_ENV));
    }

    #[test]
    fn dev_nonproduction_backend_allows_explicit_opt_in() {
        let resolved = resolve_xuser_sharing_environment(
            "https://staging.example.invalid",
            &env(&[(SAND_DEV_XUSER_SHARING_ENV, "1")]),
        );
        assert!(resolved.is_allowed);
    }

    #[test]
    fn dev_production_backend_requires_second_explicit_opt_in() {
        let first = resolve_xuser_sharing_environment(
            DEFAULT_CURSOR_BACKEND_URL,
            &env(&[(SAND_DEV_XUSER_SHARING_ENV, "1")]),
        );
        assert!(!first.is_allowed);
        assert!(first.reason.unwrap().contains(SAND_XUSER_SHARING_ALLOW_PROD_ENV));

        let second = resolve_xuser_sharing_environment(
            DEFAULT_CURSOR_BACKEND_URL,
            &env(&[
                (SAND_DEV_XUSER_SHARING_ENV, "1"),
                (SAND_XUSER_SHARING_ALLOW_PROD_ENV, "1"),
            ]),
        );
        assert!(second.is_allowed);
    }
}
