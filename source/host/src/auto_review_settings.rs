use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io,
    path::{Path, PathBuf},
};

const AUTO_REVIEW_SCHEMA_VERSION: u8 = 1;
pub const AUTO_REVIEW_MAX_ENTRIES: usize = 20;
pub const AUTO_REVIEW_MAX_CHARS: usize = 1_000;
const MAX_ACCOUNT_FENCE_BYTES: usize = 1_024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoReviewInstructions {
    #[serde(default = "default_enabled")]
    pub is_enabled: bool,
    #[serde(default)]
    pub allow_instructions: Vec<String>,
    #[serde(default)]
    pub block_instructions: Vec<String>,
}

fn default_enabled() -> bool {
    true
}

impl Default for AutoReviewInstructions {
    fn default() -> Self {
        Self {
            is_enabled: true,
            allow_instructions: Vec::new(),
            block_instructions: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountAutoReviewState {
    #[serde(default)]
    revision: u64,
    #[serde(default)]
    instructions: AutoReviewInstructions,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AutoReviewSettingsFile {
    schema_version: u8,
    #[serde(default)]
    accounts: BTreeMap<String, AccountAutoReviewState>,
}

impl Default for AutoReviewSettingsFile {
    fn default() -> Self {
        Self {
            schema_version: AUTO_REVIEW_SCHEMA_VERSION,
            accounts: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoReviewInstructionsSnapshot {
    pub account_fence: String,
    pub revision: u64,
    pub instructions: AutoReviewInstructions,
}

pub struct DurableAutoReviewSettings {
    path: PathBuf,
    state: AutoReviewSettingsFile,
}

impl DurableAutoReviewSettings {
    pub fn open(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        if !path.exists() {
            return Ok(Self {
                path,
                state: AutoReviewSettingsFile::default(),
            });
        }
        let raw = fs::read_to_string(&path)?;
        let mut state: AutoReviewSettingsFile = serde_json::from_str(&raw)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        if state.schema_version != AUTO_REVIEW_SCHEMA_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported auto-review settings schema",
            ));
        }
        state.accounts.retain(|fence, _| valid_account_fence(fence));
        for account in state.accounts.values_mut() {
            account.instructions = normalize(account.instructions.clone());
        }
        Ok(Self { path, state })
    }

    pub fn snapshot(&self, account_fence: &str) -> Result<AutoReviewInstructionsSnapshot, String> {
        validate_account_fence(account_fence)?;
        let account = self.state.accounts.get(account_fence).cloned().unwrap_or_default();
        Ok(AutoReviewInstructionsSnapshot {
            account_fence: account_fence.to_string(),
            revision: account.revision,
            instructions: normalize(account.instructions),
        })
    }

    pub fn set(
        &mut self,
        account_fence: &str,
        expected_revision: u64,
        instructions: AutoReviewInstructions,
    ) -> Result<AutoReviewInstructionsSnapshot, String> {
        validate_account_fence(account_fence)?;
        let current = self.state.accounts.get(account_fence).cloned().unwrap_or_default();
        if current.revision != expected_revision {
            return Err(format!(
                "auto-review instructions revision conflict: expected {expected_revision}, current {}",
                current.revision
            ));
        }
        let next_revision = current
            .revision
            .checked_add(1)
            .ok_or_else(|| "auto-review instructions revision exhausted".to_string())?;
        let normalized = normalize(instructions);
        let mut next = self.state.clone();
        next.accounts.insert(
            account_fence.to_string(),
            AccountAutoReviewState {
                revision: next_revision,
                instructions: normalized.clone(),
            },
        );
        persist(&self.path, &next).map_err(|error| error.to_string())?;
        self.state = next;
        Ok(AutoReviewInstructionsSnapshot {
            account_fence: account_fence.to_string(),
            revision: next_revision,
            instructions: normalized,
        })
    }
}

pub fn normalize(mut value: AutoReviewInstructions) -> AutoReviewInstructions {
    value.allow_instructions = normalize_list(value.allow_instructions);
    value.block_instructions = normalize_list(value.block_instructions);
    value
}

fn normalize_list(raw: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for item in raw {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        let clamped = trimmed.chars().take(AUTO_REVIEW_MAX_CHARS).collect::<String>();
        if clamped.is_empty() || !seen.insert(clamped.clone()) {
            continue;
        }
        result.push(clamped);
        if result.len() >= AUTO_REVIEW_MAX_ENTRIES {
            break;
        }
    }
    result
}

fn persist(path: &Path, state: &AutoReviewSettingsFile) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let encoded = serde_json::to_vec_pretty(state)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
    let temporary = path.with_extension("json.tmp");
    let result = (|| -> io::Result<()> {
        fs::write(&temporary, encoded)?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn valid_account_fence(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= MAX_ACCOUNT_FENCE_BYTES
        && !value.chars().any(char::is_control)
}

fn validate_account_fence(value: &str) -> Result<(), String> {
    valid_account_fence(value)
        .then_some(())
        .ok_or_else(|| "auto-review account fence is invalid".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(label: &str) -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix(&format!("fabushi-auto-review-{label}-"))
            .tempdir()
            .unwrap()
    }

    #[test]
    fn desktop_normalization_contract_is_preserved() {
        let long = "x".repeat(AUTO_REVIEW_MAX_CHARS + 17);
        let normalized = normalize(AutoReviewInstructions {
            is_enabled: false,
            allow_instructions: vec![
                "  read files  ".into(),
                "read files".into(),
                "".into(),
                long,
            ],
            block_instructions: (0..25).map(|index| format!("ask-{index}")).collect(),
        });
        assert!(!normalized.is_enabled);
        assert_eq!(normalized.allow_instructions[0], "read files");
        assert_eq!(normalized.allow_instructions.len(), 2);
        assert_eq!(
            normalized.allow_instructions[1].chars().count(),
            AUTO_REVIEW_MAX_CHARS
        );
        assert_eq!(normalized.block_instructions.len(), AUTO_REVIEW_MAX_ENTRIES);
    }

    #[test]
    fn account_scoped_revision_and_process_reopen_are_durable() {
        let root = root("durable");
        let path = root.path().join("auto-review-settings.json");
        {
            let mut store = DurableAutoReviewSettings::open(&path).unwrap();
            let first = store
                .set(
                    "session:account-a",
                    0,
                    AutoReviewInstructions {
                        is_enabled: true,
                        allow_instructions: vec!["run tests".into()],
                        block_instructions: vec!["delete files".into()],
                    },
                )
                .unwrap();
            assert_eq!(first.revision, 1);
            assert!(store
                .set(
                    "session:account-a",
                    0,
                    AutoReviewInstructions::default(),
                )
                .unwrap_err()
                .contains("revision conflict"));
            assert_eq!(store.snapshot("session:account-b").unwrap().revision, 0);
        }
        let reopened = DurableAutoReviewSettings::open(&path).unwrap();
        let restored = reopened.snapshot("session:account-a").unwrap();
        assert_eq!(restored.revision, 1);
        assert_eq!(restored.instructions.allow_instructions, vec!["run tests"]);
        assert_eq!(restored.instructions.block_instructions, vec!["delete files"]);
        assert_eq!(
            reopened.snapshot("session:account-b").unwrap().instructions,
            AutoReviewInstructions::default()
        );
    }

    #[test]
    fn invalid_fence_and_corrupt_document_fail_closed() {
        let root = root("invalid");
        let path = root.path().join("auto-review-settings.json");
        let store = DurableAutoReviewSettings::open(&path).unwrap();
        assert!(store.snapshot("").is_err());
        fs::write(&path, b"{not-json").unwrap();
        assert_eq!(
            DurableAutoReviewSettings::open(&path).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}
