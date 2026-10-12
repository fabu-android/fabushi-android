use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

const TIME_ZONE_MAX_CHARS: usize = 128;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct PersistedAndroidUserSettings {
    time_zone_override: Option<String>,
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
        let state = match fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str::<PersistedAndroidUserSettings>(&raw)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                PersistedAndroidUserSettings::default()
            }
            Err(error) => return Err(error),
        };
        Ok(Self { path, state })
    }

    pub fn time_zone_override(&self) -> Option<&str> {
        self.state.time_zone_override.as_deref()
    }

    pub fn set_time_zone_override(&mut self, value: Option<&str>) -> io::Result<Option<String>> {
        let normalized = normalize_time_zone(value)?;
        if self.state.time_zone_override == normalized {
            return Ok(normalized);
        }
        let next = PersistedAndroidUserSettings {
            time_zone_override: normalized.clone(),
        };
        write_atomic_json(&self.path, &next)?;
        self.state = next;
        Ok(normalized)
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
