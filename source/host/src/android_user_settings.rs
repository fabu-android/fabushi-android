use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

const TIME_ZONE_MAX_CHARS: usize = 128;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LocalToolPermission {
    Never,
    #[default]
    Ask,
    Always,
}

impl LocalToolPermission {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::Ask => "ask",
            Self::Always => "always",
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::Never => 0,
            Self::Ask => 1,
            Self::Always => 2,
        }
    }

    fn constrained_by(self, ceiling: Option<Self>) -> Self {
        match ceiling {
            Some(ceiling) if self.rank() > ceiling.rank() => ceiling,
            _ => self,
        }
    }

    pub fn parse(value: &str) -> io::Result<Self> {
        match value.trim() {
            "never" => Ok(Self::Never),
            "ask" => Ok(Self::Ask),
            "always" => Ok(Self::Always),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "local tool permission must be never, ask, or always",
            )),
        }
    }

    pub fn clamp_to_ceiling(self, ceiling: Option<Self>) -> Self {
        match ceiling {
            Some(ceiling) if self.rank() > ceiling.rank() => ceiling,
            _ => self,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct PersistedAndroidUserSettings {
    time_zone_override: Option<String>,
    #[serde(default)]
    local_tool_permission: LocalToolPermission,
    #[serde(default)]
    local_tool_permission_ceiling: Option<LocalToolPermission>,
    #[serde(default)]
    onboarding_seen_by_account: BTreeMap<String, bool>,
    #[serde(default)]
    process_metrics_local_enabled: bool,
}

/// Canonical Host-owned Android user settings that must survive Activity/process recreation.
///
/// Platform code may detect/validate Android capabilities, but durable setting truth lives here so
/// Compose/ViewModels never become a second canonical owner.
pub struct AndroidUserSettings {
    path: PathBuf,
    state: PersistedAndroidUserSettings,
}

impl AndroidUserSettings {
    pub fn open(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        let state = read_state(&path)?;
        Ok(Self { path, state })
    }

    /// Opens the canonical settings owner and imports a predecessor file exactly once when the
    /// canonical file does not exist. This is a migration seam, not a second state owner: after
    /// import every read/write uses only `canonical_path`.
    pub fn open_migrating(
        canonical_path: impl Into<PathBuf>,
        legacy_path: impl Into<PathBuf>,
    ) -> io::Result<Self> {
        let canonical_path = canonical_path.into();
        let legacy_path = legacy_path.into();
        match fs::read_to_string(&canonical_path) {
            Ok(raw) => {
                let state = parse_state(&raw)?;
                Ok(Self { path: canonical_path, state })
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let state = match fs::read_to_string(&legacy_path) {
                    Ok(raw) => parse_state(&raw)?,
                    Err(legacy_error) if legacy_error.kind() == io::ErrorKind::NotFound => {
                        PersistedAndroidUserSettings::default()
                    }
                    Err(legacy_error) => return Err(legacy_error),
                };
                if legacy_path.exists() {
                    write_atomic_json(&canonical_path, &state)?;
                }
                Ok(Self { path: canonical_path, state })
            }
            Err(error) => Err(error),
        }
    }

    pub fn time_zone_override(&self) -> Option<&str> {
        self.state.time_zone_override.as_deref()
    }

    pub fn set_time_zone_override(&mut self, value: Option<&str>) -> io::Result<Option<String>> {
        let normalized = normalize_time_zone(value)?;
        if self.state.time_zone_override == normalized {
            return Ok(normalized);
        }
        let mut next = self.state.clone();
        next.time_zone_override = normalized.clone();
        write_atomic_json(&self.path, &next)?;
        self.state = next;
        Ok(normalized)
    }

    pub fn local_tool_permission(&self) -> LocalToolPermission {
        self.state
            .local_tool_permission
            .constrained_by(self.state.local_tool_permission_ceiling)
    }

    pub fn local_tool_permission_preference(&self) -> LocalToolPermission {
        self.state.local_tool_permission
    }

    pub fn local_tool_permission_ceiling(&self) -> Option<LocalToolPermission> {
        self.state.local_tool_permission_ceiling
    }

    pub fn set_local_tool_permission_ceiling(
        &mut self,
        value: Option<&str>,
    ) -> io::Result<LocalToolPermission> {
        let normalized = match value {
            Some(value) => Some(LocalToolPermission::parse(value)?),
            None => None,
        };
        if self.state.local_tool_permission_ceiling == normalized {
            return Ok(self.local_tool_permission());
        }
        let mut next = self.state.clone();
        next.local_tool_permission_ceiling = normalized;
        write_atomic_json(&self.path, &next)?;
        self.state = next;
        Ok(self.local_tool_permission())
    }

    pub fn process_metrics_local_enabled(&self) -> bool {
        self.state.process_metrics_local_enabled
    }

    pub fn set_process_metrics_local_enabled(&mut self, value: bool) -> io::Result<bool> {
        if self.state.process_metrics_local_enabled == value {
            return Ok(value);
        }
        let mut next = self.state.clone();
        next.process_metrics_local_enabled = value;
        write_atomic_json(&self.path, &next)?;
        self.state = next;
        Ok(value)
    }
    pub fn onboarding_seen(&self, account_scope: &str) -> bool {
        self.state
            .onboarding_seen_by_account
            .get(account_scope)
            .copied()
            .unwrap_or(false)
    }

    pub fn set_onboarding_seen(
        &mut self,
        account_scope: &str,
        value: bool,
    ) -> io::Result<bool> {
        let account_scope = account_scope.trim();
        if account_scope.is_empty() || account_scope.len() > 256 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "onboarding account scope must be bounded and non-empty",
            ));
        }
        if self.onboarding_seen(account_scope) == value {
            return Ok(value);
        }
        let mut next = self.state.clone();
        if value {
            next.onboarding_seen_by_account
                .insert(account_scope.to_string(), true);
        } else {
            next.onboarding_seen_by_account.remove(account_scope);
        }
        write_atomic_json(&self.path, &next)?;
        self.state = next;
        Ok(value)
    }


    pub fn set_local_tool_permission(&mut self, value: &str) -> io::Result<LocalToolPermission> {
        let normalized = LocalToolPermission::parse(value)?;
        if self.state.local_tool_permission == normalized {
            return Ok(normalized);
        }
        let mut next = self.state.clone();
        next.local_tool_permission = normalized;
        write_atomic_json(&self.path, &next)?;
        self.state = next;
        Ok(normalized)
    }
}

fn parse_state(raw: &str) -> io::Result<PersistedAndroidUserSettings> {
    serde_json::from_str::<PersistedAndroidUserSettings>(raw)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn read_state(path: &Path) -> io::Result<PersistedAndroidUserSettings> {
    match fs::read_to_string(path) {
        Ok(raw) => parse_state(&raw),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            Ok(PersistedAndroidUserSettings::default())
        }
        Err(error) => Err(error),
    }
}

fn normalize_time_zone(value: Option<&str>) -> io::Result<Option<String>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.chars().count() > TIME_ZONE_MAX_CHARS
        || !value.chars().all(|ch| {
            ch.is_ascii_alphanumeric() || matches!(ch, '/' | '_' | '-' | '+' | '.')
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "time zone override must be a bounded IANA-style identifier",
        ));
    }
    Ok(Some(value.to_string()))
}

fn write_atomic_json(path: &Path, state: &PersistedAndroidUserSettings) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "settings path has no parent")
    })?;
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let result = (|| -> io::Result<()> {
        let bytes = serde_json::to_vec(state)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let mut file = fs::File::create(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fabushi-android-user-settings-{label}-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
        ))
    }

    #[test]
    fn local_tool_permission_clamps_to_managed_ceiling() {
        assert_eq!(
            LocalToolPermission::Always.clamp_to_ceiling(Some(LocalToolPermission::Ask)),
            LocalToolPermission::Ask,
        );
        assert_eq!(
            LocalToolPermission::Always.clamp_to_ceiling(Some(LocalToolPermission::Never)),
            LocalToolPermission::Never,
        );
        assert_eq!(
            LocalToolPermission::Ask.clamp_to_ceiling(Some(LocalToolPermission::Always)),
            LocalToolPermission::Ask,
        );
        assert_eq!(
            LocalToolPermission::Never.clamp_to_ceiling(None),
            LocalToolPermission::Never,
        );
    }

    #[test]
    fn time_zone_override_is_durable_and_clearable() {
        let path = test_path("timezone");
        let mut settings = AndroidUserSettings::open(&path).unwrap();
        assert_eq!(settings.time_zone_override(), None);

        assert_eq!(
            settings.set_time_zone_override(Some(" America/Los_Angeles ")).unwrap(),
            Some("America/Los_Angeles".into()),
        );
        drop(settings);

        let mut reopened = AndroidUserSettings::open(&path).unwrap();
        assert_eq!(reopened.time_zone_override(), Some("America/Los_Angeles"));
        assert_eq!(reopened.set_time_zone_override(None).unwrap(), None);
        drop(reopened);

        assert_eq!(
            AndroidUserSettings::open(&path).unwrap().time_zone_override(),
            None,
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn predecessor_settings_are_imported_once_into_the_canonical_owner() {
        let canonical = test_path("canonical");
        let legacy = test_path("legacy");
        let legacy_state = PersistedAndroidUserSettings {
            time_zone_override: Some("Asia/Tokyo".into()),
            ..Default::default()
        };
        write_atomic_json(&legacy, &legacy_state).unwrap();

        let mut migrated = AndroidUserSettings::open_migrating(&canonical, &legacy).unwrap();
        assert_eq!(migrated.time_zone_override(), Some("Asia/Tokyo"));
        migrated
            .set_time_zone_override(Some("America/Los_Angeles"))
            .unwrap();
        drop(migrated);

        // Mutating the predecessor after migration cannot regain ownership.
        write_atomic_json(
            &legacy,
            &PersistedAndroidUserSettings {
                time_zone_override: Some("Europe/London".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let reopened = AndroidUserSettings::open_migrating(&canonical, &legacy).unwrap();
        assert_eq!(reopened.time_zone_override(), Some("America/Los_Angeles"));

        let _ = fs::remove_file(canonical);
        let _ = fs::remove_file(legacy);
    }

    #[test]
    fn onboarding_seen_is_account_scoped_and_durable() {
        let path = test_path("onboarding");
        let mut settings = AndroidUserSettings::open(&path).unwrap();
        assert!(!settings.onboarding_seen("account-device:a"));
        assert!(!settings.onboarding_seen("account-device:b"));

        assert!(settings
            .set_onboarding_seen("account-device:a", true)
            .unwrap());
        assert!(settings.onboarding_seen("account-device:a"));
        assert!(!settings.onboarding_seen("account-device:b"));
        drop(settings);

        let mut reopened = AndroidUserSettings::open(&path).unwrap();
        assert!(reopened.onboarding_seen("account-device:a"));
        assert!(!reopened.onboarding_seen("account-device:b"));
        assert!(!reopened
            .set_onboarding_seen("account-device:a", false)
            .unwrap());
        assert!(!reopened.onboarding_seen("account-device:a"));
        assert!(reopened.set_onboarding_seen("", true).is_err());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn process_metrics_local_collection_is_opt_in_and_durable() {
        let path = test_path("process-metrics");
        let mut settings = AndroidUserSettings::open(&path).unwrap();
        assert!(!settings.process_metrics_local_enabled());
        assert!(settings.set_process_metrics_local_enabled(true).unwrap());
        drop(settings);

        let mut reopened = AndroidUserSettings::open(&path).unwrap();
        assert!(reopened.process_metrics_local_enabled());
        assert!(!reopened.set_process_metrics_local_enabled(false).unwrap());
        drop(reopened);

        assert!(!AndroidUserSettings::open(&path).unwrap().process_metrics_local_enabled());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn local_tool_permission_ceiling_constrains_preference_and_survives_restart() {
        let path = test_path("local-tool-ceiling");
        let mut settings = AndroidUserSettings::open(&path).unwrap();
        settings.set_local_tool_permission("always").unwrap();
        assert_eq!(settings.local_tool_permission_preference(), LocalToolPermission::Always);
        assert_eq!(settings.local_tool_permission(), LocalToolPermission::Always);

        assert_eq!(
            settings.set_local_tool_permission_ceiling(Some("ask")).unwrap(),
            LocalToolPermission::Ask,
        );
        assert_eq!(settings.local_tool_permission_preference(), LocalToolPermission::Always);
        assert_eq!(settings.local_tool_permission_ceiling(), Some(LocalToolPermission::Ask));
        assert_eq!(settings.local_tool_permission(), LocalToolPermission::Ask);

        drop(settings);
        let mut reopened = AndroidUserSettings::open(&path).unwrap();
        assert_eq!(reopened.local_tool_permission_preference(), LocalToolPermission::Always);
        assert_eq!(reopened.local_tool_permission_ceiling(), Some(LocalToolPermission::Ask));
        assert_eq!(reopened.local_tool_permission(), LocalToolPermission::Ask);
        assert_eq!(
            reopened.set_local_tool_permission_ceiling(Some("never")).unwrap(),
            LocalToolPermission::Never,
        );
        assert_eq!(
            reopened.set_local_tool_permission_ceiling(None).unwrap(),
            LocalToolPermission::Always,
        );
        assert!(reopened.set_local_tool_permission_ceiling(Some("invalid")).is_err());
        assert_eq!(reopened.local_tool_permission(), LocalToolPermission::Always);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn local_tool_permission_defaults_to_ask_and_is_durable() {
        let path = test_path("local-tool-permission");
        let mut settings = AndroidUserSettings::open(&path).unwrap();
        assert_eq!(settings.local_tool_permission(), LocalToolPermission::Ask);

        assert_eq!(
            settings.set_local_tool_permission("always").unwrap(),
            LocalToolPermission::Always,
        );
        settings.set_time_zone_override(Some("UTC")).unwrap();
        drop(settings);

        let mut reopened = AndroidUserSettings::open(&path).unwrap();
        assert_eq!(reopened.local_tool_permission(), LocalToolPermission::Always);
        assert_eq!(reopened.time_zone_override(), Some("UTC"));
        assert_eq!(
            reopened.set_local_tool_permission("never").unwrap(),
            LocalToolPermission::Never,
        );
        assert!(reopened.set_local_tool_permission("sometimes").is_err());
        assert_eq!(reopened.local_tool_permission(), LocalToolPermission::Never);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn time_zone_override_rejects_unbounded_or_untrusted_values_without_mutation() {
        let path = test_path("invalid");
        let mut settings = AndroidUserSettings::open(&path).unwrap();
        settings.set_time_zone_override(Some("UTC")).unwrap();

        assert!(settings.set_time_zone_override(Some("../secret")).is_err());
        assert!(settings.set_time_zone_override(Some("America/Los Angeles")).is_err());
        assert!(settings
            .set_time_zone_override(Some(&"a".repeat(TIME_ZONE_MAX_CHARS + 1)))
            .is_err());
        assert_eq!(settings.time_zone_override(), Some("UTC"));

        let _ = fs::remove_file(path);
    }
}
