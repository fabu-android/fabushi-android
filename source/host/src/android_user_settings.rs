use chrono::{SecondsFormat, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

const TIME_ZONE_MAX_CHARS: usize = 128;
const MODEL_ID_MAX_CHARS: usize = 256;
const INFERENCE_PROVIDERS: [&str; 5] = ["fabushi", "cursor", "claude-code", "codex", "openrouter"];

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct InferenceProviderUsage {
    #[serde(default)]
    requests: u64,
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cache_read_tokens: u64,
    #[serde(default)]
    cache_write_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_used_at: Option<String>,
}

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
    #[serde(default, rename = "localToolPermission", skip_serializing)]
    legacy_local_tool_permission: LocalToolPermission,
    #[serde(default)]
    local_tool_permission_by_account: BTreeMap<String, LocalToolPermission>,
    #[serde(default)]
    local_tool_permission_ceiling: Option<LocalToolPermission>,
    #[serde(default)]
    onboarding_seen_by_account: BTreeMap<String, bool>,
    #[serde(default)]
    process_metrics_local_enabled: bool,
    #[serde(default = "default_true")]
    webauthn_proxy_enabled: bool,
    #[serde(default)]
    agent_default_model_by_account: BTreeMap<String, String>,
    #[serde(default)]
    computer_use_model_by_account: BTreeMap<String, String>,
    #[serde(default)]
    inference_provider: Option<String>,
    #[serde(default)]
    inference_router_usage: BTreeMap<String, InferenceProviderUsage>,
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

    pub fn local_tool_permission_for(
        &self,
        account_scope: &str,
    ) -> io::Result<LocalToolPermission> {
        Ok(self
            .local_tool_permission_preference_for(account_scope)?
            .constrained_by(self.state.local_tool_permission_ceiling))
    }

    pub fn local_tool_permission_preference_for(
        &self,
        account_scope: &str,
    ) -> io::Result<LocalToolPermission> {
        validate_account_scope(account_scope)?;
        Ok(self
            .state
            .local_tool_permission_by_account
            .get(account_scope)
            .copied()
            .unwrap_or(LocalToolPermission::Ask))
    }

    pub fn local_tool_permission_ceiling(&self) -> Option<LocalToolPermission> {
        self.state.local_tool_permission_ceiling
    }

    pub fn set_local_tool_permission_ceiling(
        &mut self,
        value: Option<&str>,
    ) -> io::Result<()> {
        let normalized = match value {
            Some(value) => Some(LocalToolPermission::parse(value)?),
            None => None,
        };
        if self.state.local_tool_permission_ceiling == normalized {
            return Ok(());
        }
        let mut next = self.state.clone();
        next.local_tool_permission_ceiling = normalized;
        write_atomic_json(&self.path, &next)?;
        self.state = next;
        Ok(())
    }

    pub fn agent_default_model(&self, account_scope: &str) -> Option<&str> {
        self.state
            .agent_default_model_by_account
            .get(account_scope)
            .map(String::as_str)
    }

    pub fn set_agent_default_model(
        &mut self,
        account_scope: &str,
        value: Option<&str>,
    ) -> io::Result<Option<String>> {
        self.set_account_model(account_scope, value, true)
    }

    pub fn computer_use_model(&self, account_scope: &str) -> Option<&str> {
        self.state
            .computer_use_model_by_account
            .get(account_scope)
            .map(String::as_str)
    }

    pub fn set_computer_use_model(
        &mut self,
        account_scope: &str,
        value: Option<&str>,
    ) -> io::Result<Option<String>> {
        self.set_account_model(account_scope, value, false)
    }

    fn set_account_model(
        &mut self,
        account_scope: &str,
        value: Option<&str>,
        agent_default: bool,
    ) -> io::Result<Option<String>> {
        validate_account_scope(account_scope)?;
        let normalized = normalize_model_id(value)?;
        let current = if agent_default {
            self.agent_default_model(account_scope)
        } else {
            self.computer_use_model(account_scope)
        };
        if current == normalized.as_deref() {
            return Ok(normalized);
        }
        let mut next = self.state.clone();
        let target = if agent_default {
            &mut next.agent_default_model_by_account
        } else {
            &mut next.computer_use_model_by_account
        };
        match normalized.as_ref() {
            Some(model) => {
                target.insert(account_scope.to_string(), model.clone());
            }
            None => {
                target.remove(account_scope);
            }
        }
        write_atomic_json(&self.path, &next)?;
        self.state = next;
        Ok(normalized)
    }

    pub fn webauthn_proxy_enabled(&self) -> bool {
        self.state.webauthn_proxy_enabled
    }

    pub fn set_webauthn_proxy_enabled(&mut self, value: bool) -> io::Result<bool> {
        if self.state.webauthn_proxy_enabled == value {
            return Ok(value);
        }
        let mut next = self.state.clone();
        next.webauthn_proxy_enabled = value;
        write_atomic_json(&self.path, &next)?;
        self.state = next;
        Ok(value)
    }

    pub fn inference_provider(&self) -> &str {
        self.state
            .inference_provider
            .as_deref()
            .filter(|provider| INFERENCE_PROVIDERS.contains(provider))
            .unwrap_or("cursor")
    }

    pub fn set_inference_provider(&mut self, value: &str) -> io::Result<String> {
        let value = value.trim();
        if !INFERENCE_PROVIDERS.contains(&value) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "inference provider is not supported",
            ));
        }
        if self.inference_provider() == value
            && self.state.inference_provider.as_deref() == Some(value)
        {
            return Ok(value.to_string());
        }
        let mut next = self.state.clone();
        next.inference_provider = Some(value.to_string());
        write_atomic_json(&self.path, &next)?;
        self.state = next;
        Ok(value.to_string())
    }

    pub fn inference_router_usage(&self) -> Value {
        let providers = INFERENCE_PROVIDERS
            .iter()
            .map(|provider| {
                let usage = self
                    .state
                    .inference_router_usage
                    .get(*provider)
                    .cloned()
                    .unwrap_or_default();
                (
                    (*provider).to_string(),
                    json!({
                        "requests": usage.requests,
                        "inputTokens": usage.input_tokens,
                        "outputTokens": usage.output_tokens,
                        "cacheReadTokens": usage.cache_read_tokens,
                        "cacheWriteTokens": usage.cache_write_tokens,
                        "lastUsedAt": usage.last_used_at,
                    }),
                )
            })
            .collect::<serde_json::Map<String, Value>>();
        json!({"providers": providers})
    }

    pub fn record_inference_usage(
        &mut self,
        provider: &str,
        input_tokens: Option<f64>,
        output_tokens: Option<f64>,
        cache_read_tokens: Option<f64>,
        cache_write_tokens: Option<f64>,
    ) -> io::Result<Value> {
        let provider = provider.trim();
        if !INFERENCE_PROVIDERS.contains(&provider) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "inference provider is not supported",
            ));
        }
        let mut next = self.state.clone();
        let usage = next
            .inference_router_usage
            .entry(provider.to_string())
            .or_default();
        usage.requests = usage.requests.saturating_add(1);
        usage.input_tokens = usage.input_tokens.saturating_add(safe_usage(input_tokens));
        usage.output_tokens = usage.output_tokens.saturating_add(safe_usage(output_tokens));
        usage.cache_read_tokens = usage.cache_read_tokens.saturating_add(safe_usage(cache_read_tokens));
        usage.cache_write_tokens = usage.cache_write_tokens.saturating_add(safe_usage(cache_write_tokens));
        usage.last_used_at = Some(Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true));
        write_atomic_json(&self.path, &next)?;
        self.state = next;
        Ok(self.inference_router_usage())
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


    pub fn set_local_tool_permission_for(
        &mut self,
        account_scope: &str,
        value: &str,
    ) -> io::Result<LocalToolPermission> {
        validate_account_scope(account_scope)?;
        let normalized = LocalToolPermission::parse(value)?;
        let current = self
            .state
            .local_tool_permission_by_account
            .get(account_scope)
            .copied()
            .unwrap_or(LocalToolPermission::Ask);
        if current == normalized {
            return self.local_tool_permission_for(account_scope);
        }
        let mut next = self.state.clone();
        if normalized == LocalToolPermission::Ask {
            next.local_tool_permission_by_account.remove(account_scope);
        } else {
            next.local_tool_permission_by_account
                .insert(account_scope.to_string(), normalized);
        }
        write_atomic_json(&self.path, &next)?;
        self.state = next;
        self.local_tool_permission_for(account_scope)
    }
}

const fn default_true() -> bool {
    true
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

fn validate_account_scope(value: &str) -> io::Result<()> {
    let value = value.trim();
    if value.is_empty() || value.len() > 1024 || value.chars().any(char::is_control) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "account scope must be bounded and non-empty",
        ));
    }
    Ok(())
}

fn normalize_model_id(value: Option<&str>) -> io::Result<Option<String>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.chars().count() > MODEL_ID_MAX_CHARS || value.chars().any(char::is_control) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "model id must be bounded and contain no control characters",
        ));
    }
    Ok(Some(value.to_string()))
}

fn normalize_time_zone(value: Option<&str>) -> io::Result<Option<String>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.chars().count() > TIME_ZONE_MAX_CHARS
        || !value.chars().all(|ch| {
            ch.is_ascii_alphanumeric() || matches!(ch, '/' | '_' | '-' | '+' | '.')
        })
        || value.parse::<Tz>().is_err()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "time zone override must be a valid bounded IANA time zone",
        ));
    }
    Ok(Some(value.to_string()))
}

fn safe_usage(value: Option<f64>) -> u64 {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| value.round().min(u64::MAX as f64) as u64)
        .unwrap_or(0)
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
    fn inference_provider_and_usage_are_durable_and_bounded() {
        let path = test_path("inference-routing");
        let mut settings = AndroidUserSettings::open(&path).unwrap();
        assert_eq!(settings.inference_provider(), "cursor");
        assert!(settings.set_inference_provider("unknown").is_err());
        assert_eq!(settings.set_inference_provider("codex").unwrap(), "codex");

        let usage = settings
            .record_inference_usage("codex", Some(1.4), Some(2.6), Some(-1.0), Some(f64::NAN))
            .unwrap();
        assert_eq!(usage["providers"]["codex"]["requests"], 1);
        assert_eq!(usage["providers"]["codex"]["inputTokens"], 1);
        assert_eq!(usage["providers"]["codex"]["outputTokens"], 3);
        assert_eq!(usage["providers"]["codex"]["cacheReadTokens"], 0);
        assert_eq!(usage["providers"]["codex"]["cacheWriteTokens"], 0);
        assert!(usage["providers"]["codex"]["lastUsedAt"].is_string());

        drop(settings);
        let reopened = AndroidUserSettings::open(&path).unwrap();
        assert_eq!(reopened.inference_provider(), "codex");
        assert_eq!(reopened.inference_router_usage()["providers"]["codex"]["requests"], 1);
        assert_eq!(reopened.inference_router_usage()["providers"]["cursor"]["requests"], 0);
        let _ = fs::remove_file(path);
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
    fn model_preferences_are_account_scoped_durable_and_clearable() {
        let path = test_path("models");
        let mut settings = AndroidUserSettings::open(&path).unwrap();
        assert_eq!(settings.agent_default_model("account:a"), None);
        assert_eq!(settings.computer_use_model("account:a"), None);

        assert_eq!(
            settings
                .set_agent_default_model("account:a", Some("  claude-4-sonnet  "))
                .unwrap(),
            Some("claude-4-sonnet".into()),
        );
        assert_eq!(
            settings
                .set_computer_use_model("account:a", Some("computer-use-v2"))
                .unwrap(),
            Some("computer-use-v2".into()),
        );
        assert_eq!(settings.agent_default_model("account:b"), None);
        drop(settings);

        let mut reopened = AndroidUserSettings::open(&path).unwrap();
        assert_eq!(
            reopened.agent_default_model("account:a"),
            Some("claude-4-sonnet"),
        );
        assert_eq!(
            reopened.computer_use_model("account:a"),
            Some("computer-use-v2"),
        );
        assert!(reopened
            .set_agent_default_model("", Some("model"))
            .is_err());
        assert!(reopened
            .set_agent_default_model("account:a", Some(&"x".repeat(MODEL_ID_MAX_CHARS + 1)))
            .is_err());
        assert_eq!(
            reopened.set_agent_default_model("account:a", None).unwrap(),
            None,
        );
        assert_eq!(reopened.agent_default_model("account:a"), None);
        assert_eq!(
            reopened.computer_use_model("account:a"),
            Some("computer-use-v2"),
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn webauthn_proxy_defaults_enabled_and_disable_survives_restart() {
        let path = test_path("webauthn-proxy");
        let mut settings = AndroidUserSettings::open(&path).unwrap();
        assert!(settings.webauthn_proxy_enabled());
        assert!(!settings.set_webauthn_proxy_enabled(false).unwrap());
        drop(settings);

        let mut reopened = AndroidUserSettings::open(&path).unwrap();
        assert!(!reopened.webauthn_proxy_enabled());
        assert!(reopened.set_webauthn_proxy_enabled(true).unwrap());
        drop(reopened);
        assert!(AndroidUserSettings::open(&path).unwrap().webauthn_proxy_enabled());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn predecessor_without_webauthn_field_migrates_to_enabled_default() {
        let path = test_path("webauthn-predecessor");
        fs::write(&path, r#"{"timeZoneOverride":null}"#).unwrap();
        let settings = AndroidUserSettings::open(&path).unwrap();
        assert!(settings.webauthn_proxy_enabled());
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
    fn local_tool_permission_is_account_scoped_and_ceiling_remains_global() {
        let path = test_path("local-tool-account-scope");
        let mut settings = AndroidUserSettings::open(&path).unwrap();
        let account_a = "session:account-a";
        let account_b = "session:account-b";
        assert_eq!(settings.local_tool_permission_for(account_a).unwrap(), LocalToolPermission::Ask);
        settings.set_local_tool_permission_for(account_a, "always").unwrap();
        assert_eq!(settings.local_tool_permission_preference_for(account_a).unwrap(), LocalToolPermission::Always);
        assert_eq!(settings.local_tool_permission_for(account_b).unwrap(), LocalToolPermission::Ask);
        settings.set_local_tool_permission_ceiling(Some("ask")).unwrap();
        assert_eq!(settings.local_tool_permission_for(account_a).unwrap(), LocalToolPermission::Ask);
        assert_eq!(settings.local_tool_permission_preference_for(account_a).unwrap(), LocalToolPermission::Always);
        drop(settings);
        let mut reopened = AndroidUserSettings::open(&path).unwrap();
        assert_eq!(reopened.local_tool_permission_preference_for(account_a).unwrap(), LocalToolPermission::Always);
        assert_eq!(reopened.local_tool_permission_for(account_b).unwrap(), LocalToolPermission::Ask);
        reopened.set_local_tool_permission_ceiling(None).unwrap();
        assert_eq!(reopened.local_tool_permission_for(account_a).unwrap(), LocalToolPermission::Always);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn legacy_global_permission_is_not_promoted_to_a_new_account() {
        let path = test_path("legacy-local-tool-permission");
        fs::write(&path, br#"{"localToolPermission":"always","localToolPermissionCeiling":null}"#).unwrap();
        let settings = AndroidUserSettings::open(&path).unwrap();
        assert_eq!(
            settings.local_tool_permission_for("session:new-account").unwrap(),
            LocalToolPermission::Ask,
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
        assert!(settings.set_time_zone_override(Some("Foo/Bar")).is_err());
        assert!(settings
            .set_time_zone_override(Some(&"a".repeat(TIME_ZONE_MAX_CHARS + 1)))
            .is_err());
        assert_eq!(settings.time_zone_override(), Some("UTC"));

        let _ = fs::remove_file(path);
    }
}
