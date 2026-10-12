use crate::sha256::sha256_hex;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, io, path::PathBuf};

const BINDING_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct BindingDocument {
    version: u32,
    bindings: BTreeMap<String, String>,
}

pub struct EnvDescriptorAccountBinding {
    path: PathBuf,
}

impl EnvDescriptorAccountBinding {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn authorize(
        &self,
        account_slot: &str,
        descriptor_url: &str,
        allow_existing_data_claim: bool,
        has_existing_durable_data: bool,
    ) -> Result<bool, String> {
        let descriptor = descriptor_key(descriptor_url)?;
        let account = account_key(account_slot)?;
        let mut stored = self.read()?;
        if stored.bindings.get(&descriptor) == Some(&account) {
            return Ok(true);
        }
        if stored.bindings.contains_key(&descriptor) {
            return Ok(false);
        }
        if has_existing_durable_data && !allow_existing_data_claim {
            return Ok(false);
        }
        stored.bindings.insert(descriptor, account);
        self.write(&stored)?;
        Ok(true)
    }

    fn read(&self) -> Result<BindingDocument, String> {
        let raw = match fs::read_to_string(&self.path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(BindingDocument { version: BINDING_VERSION, bindings: BTreeMap::new() });
            }
            Err(error) => return Err(format!("failed to read env-descriptor account binding: {error}")),
        };
        let parsed: BindingDocument = serde_json::from_str(&raw)
            .map_err(|_| "invalid env-descriptor account binding file".to_string())?;
        if parsed.version != BINDING_VERSION
            || parsed.bindings.iter().any(|(descriptor, account)| {
                !is_sha256_hex(descriptor) || !is_sha256_hex(account)
            })
        {
            return Err("invalid env-descriptor account binding file".into());
        }
        Ok(parsed)
    }

    fn write(&self, value: &BindingDocument) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create account binding directory: {error}"))?;
        }
        let encoded = serde_json::to_vec(value)
            .map_err(|error| format!("failed to encode account binding: {error}"))?;
        let temporary = self.path.with_extension(format!(
            "tmp-{}-{}",
            std::process::id(),
            crate::android_json_runtime::now_ms(),
        ));
        let result = (|| -> Result<(), String> {
            fs::write(&temporary, &encoded)
                .map_err(|error| format!("failed to write account binding temporary file: {error}"))?;
            fs::rename(&temporary, &self.path)
                .map_err(|error| format!("failed to atomically replace account binding: {error}"))
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

pub fn descriptor_key(raw_url: &str) -> Result<String, String> {
    let url = url::Url::parse(raw_url.trim())
        .map_err(|_| "account environment descriptor URL is invalid".to_string())?;
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return Err("account environment descriptor URL is invalid".into());
    }
    let material = format!("sand-env-descriptor\0{}", url);
    Ok(sha256_hex(material.as_bytes()))
}

pub fn account_key(slot: &str) -> Result<String, String> {
    let slot = slot.trim();
    if slot.is_empty() || slot.len() > 512 || slot.chars().any(char::is_control) {
        return Err("account storage slot is invalid".into());
    }
    Ok(sha256_hex(format!("sand-account-slot\0{slot}").as_bytes()))
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binding_rejects_foreign_owner_and_requires_explicit_existing_data_claim() {
        let root = tempfile::tempdir().unwrap();
        let binding = EnvDescriptorAccountBinding::new(root.path().join("binding.json"));
        assert!(!binding.authorize("account-a", "https://api.ombhrum.com/", false, true).unwrap());
        assert!(binding.authorize("account-a", "https://api.ombhrum.com/", true, true).unwrap());
        assert!(binding.authorize("account-a", "https://api.ombhrum.com/", false, true).unwrap());
        assert!(!binding.authorize("account-b", "https://api.ombhrum.com/", true, true).unwrap());
    }

    #[test]
    fn corrupt_binding_fails_closed() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("binding.json");
        fs::write(&path, "{not-json").unwrap();
        let binding = EnvDescriptorAccountBinding::new(path);
        assert!(binding.authorize("account-a", "https://api.ombhrum.com/", true, false).is_err());
    }
}
