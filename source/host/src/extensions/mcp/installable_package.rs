//! Passive catalog bundle verification inside the existing Host Plugins owner.
//! No download, execution, signing, install authority or renderer API lives here.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{
    Deserialize, Serialize,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    io::{Cursor, Read},
};
use thiserror::Error;

const DOMAIN: &[u8] = b"fabushi.fabapp.manifest.v1\0";
const MAX_ARCHIVE: usize = 100 * 1024 * 1024;
const MAX_EXPANDED: u64 = 250 * 1024 * 1024;
const MAX_MANIFEST: usize = 2 * 1024 * 1024;
const MAX_FILES: usize = 10_000;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
pub mod ledger;
pub mod policy;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PackageError {
    #[error("package archive is malformed or unsafe")]
    Archive,
    #[error("package resource or archive limit exceeded")]
    Limit,
    #[error("package manifest is malformed or unsupported")]
    Manifest,
    #[error("package does not match trusted catalog authorization")]
    Authorization,
    #[error("package signature is invalid")]
    Signature,
    #[error("package resource integrity mismatch")]
    Integrity,
}

/// Internal policy input, never deserialized from Web content or renderer IPC.
/// Caller must verify release approval, trusted feed, revocation and current scope.
pub struct CatalogAuthorization {
    pub app_id: String,
    pub publisher_id: String,
    pub version: String,
    pub package_sha256: String,
    pub key_id: String,
    pub public_key: [u8; 32],
    pub trust_epoch: u64,
    pub host_api: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NativeMiniAppInstallState {
    Ready,
    Quarantined,
}

/// Exact native management-list DTO shared by the Host response and its
/// contract tests. It contains display metadata only, never code bytes/paths.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeMiniAppEntry {
    pub app_id: String,
    pub display_name: String,
    pub publisher_id: String,
    pub version: String,
    pub package_digest: String,
    pub size_bytes: u64,
    pub offline_core: bool,
    pub installed: bool,
    pub supported: bool,
    pub state: NativeMiniAppInstallState,
}

pub struct VerifiedPackage {
    manifest: Manifest,
    digest: String,
    resources: BTreeMap<String, Vec<u8>>,
    trust_epoch: u64,
    archive: Vec<u8>,
}

impl VerifiedPackage {
    pub fn app_id(&self) -> &str {
        &self.manifest.app_id
    }
    pub fn publisher_id(&self) -> &str {
        &self.manifest.publisher_id
    }
    pub fn version(&self) -> &str {
        &self.manifest.version
    }
    pub fn entrypoint(&self) -> &str {
        &self.manifest.entrypoint
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn trust_epoch(&self) -> u64 {
        self.trust_epoch
    }
    pub fn resource(&self, name: &str) -> Option<(&[u8], &str)> {
        Some((
            self.resources.get(name)?.as_slice(),
            self.manifest.files.get(name)?.mime.as_str(),
        ))
    }
    pub fn resource_entries(&self) -> impl Iterator<Item = (&str, &[u8], &str)> {
        self.manifest.files.iter().filter_map(|(name, metadata)| {
            self.resources
                .get(name)
                .map(|bytes| (name.as_str(), bytes.as_slice(), metadata.mime.as_str()))
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    manifest_version: u64,
    package_class: String,
    app_id: String,
    publisher_id: String,
    version: String,
    display_name: String,
    entrypoint: String,
    runtime: String,
    min_host_api: u64,
    offline_core: bool,
    requested_capabilities: Vec<String>,
    network_policy: NetworkPolicy,
    files: BTreeMap<String, Resource>,
    signature: ManifestSignature,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NetworkPolicy {
    mode: String,
    allowed_origins: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Resource {
    sha256: String,
    bytes: u64,
    mime: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManifestSignature {
    algorithm: String,
    key_id: String,
    value: String,
}

// serde_json::Value normally silently replaces duplicate object keys. Reject
// duplicates recursively before strict typed parsing or canonical signing.
struct UniqueJson(Value);
impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct UniqueVisitor;
        impl<'de> Visitor<'de> for UniqueVisitor {
            type Value = UniqueJson;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("unambiguous JSON")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(UniqueJson(Value::Bool(v)))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueJson(Value::Null))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(UniqueJson(Value::String(v.into())))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
                if v > MAX_SAFE_INTEGER {
                    return Err(E::custom("unsafe integer"));
                }
                Ok(UniqueJson(Value::Number(v.into())))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
                if v < 0 {
                    return Err(E::custom("negative integer"));
                }
                self.visit_u64(v as u64)
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(UniqueJson(value)) = seq.next_element()? {
                    if values.len() >= MAX_FILES {
                        return Err(de::Error::custom("array limit"));
                    }
                    values.push(value);
                }
                Ok(UniqueJson(Value::Array(values)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some((key, UniqueJson(value))) = map.next_entry::<String, UniqueJson>()? {
                    if !key.is_ascii() || values.contains_key(&key) || values.len() >= MAX_FILES {
                        return Err(de::Error::custom("ambiguous object or limit"));
                    }
                    values.insert(key, value);
                }
                Ok(UniqueJson(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(UniqueVisitor)
    }
}

fn digest(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
fn safe_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 1024
        && value.is_ascii()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-/".contains(&b))
        && value.split('/').all(|p| {
            let stem = p.split('.').next().unwrap_or_default().to_ascii_lowercase();
            !p.is_empty()
                && p != "."
                && p != ".."
                && !p.ends_with('.')
                && !["con", "prn", "aux", "nul"].contains(&stem.as_str())
                && !(stem.len() == 4
                    && (stem.starts_with("com") || stem.starts_with("lpt"))
                    && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        })
}
fn semver(value: &str) -> bool {
    if value.len() > 128 {
        return false;
    }
    let (release, metadata) = value
        .split_once('+')
        .map_or((value, None), |(a, b)| (a, Some(b)));
    let (core, prerelease) = release
        .split_once('-')
        .map_or((release, None), |(a, b)| (a, Some(b)));
    let parts: Vec<_> = core.split('.').collect();
    let ids = |text: &str, pre: bool| {
        text.split('.').all(|p| {
            !p.is_empty()
                && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                && !(pre
                    && p.len() > 1
                    && p.starts_with('0')
                    && p.bytes().all(|b| b.is_ascii_digit()))
        })
    };
    parts.len() == 3
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.bytes().all(|b| b.is_ascii_digit())
                && (p.len() == 1 || !p.starts_with('0'))
                && p.parse::<u64>().is_ok()
        })
        && prerelease.is_none_or(|s| ids(s, true))
        && metadata.is_none_or(|s| ids(s, false))
}
fn expected_mime(path: &str) -> Option<&'static str> {
    match path.rsplit('.').next()? {
        "html" => Some("text/html"),
        "css" => Some("text/css"),
        "js" => Some("text/javascript"),
        "wasm" => Some("application/wasm"),
        "txt" => Some("text/plain"),
        "json" => Some("application/json"),
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}
fn canonical(value: &Value) -> Result<Vec<u8>, PackageError> {
    // ASCII field/path keys sort identically in UTF-16 (JCS) and UTF-8.
    // Integers were bounded by UniqueJson. Do not depend on Map feature ordering.
    fn sorted(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let ordered: BTreeMap<_, _> =
                    map.iter().map(|(k, v)| (k.clone(), sorted(v))).collect();
                Value::Object(ordered.into_iter().collect())
            }
            Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
            _ => value.clone(),
        }
    }
    serde_json::to_vec(&sorted(value)).map_err(|_| PackageError::Manifest)
}

#[derive(Debug, PartialEq, Eq)]
struct PhysicalZipEntry {
    central_start: usize,
    local_start: usize,
}

fn validate_zip32_extra(bytes: &[u8]) -> Result<(), PackageError> {
    let mut remaining = bytes;
    while !remaining.is_empty() {
        let header = remaining.get(..4).ok_or(PackageError::Archive)?;
        let tag = u16::from_le_bytes([header[0], header[1]]);
        let size = usize::from(u16::from_le_bytes([header[2], header[3]]));
        if tag == 1 {
            return Err(PackageError::Archive);
        }
        remaining = remaining.get(4 + size..).ok_or(PackageError::Archive)?;
    }
    Ok(())
}

// Read physical entries before a ZIP library can collapse its name index.
fn physical_zip_entries(bytes: &[u8]) -> Result<Vec<PhysicalZipEntry>, PackageError> {
    let u16_at = |offset: usize| -> Result<usize, PackageError> {
        let end = offset.checked_add(2).ok_or(PackageError::Archive)?;
        let raw = bytes.get(offset..end).ok_or(PackageError::Archive)?;
        let value = u16::from_le_bytes(raw.try_into().map_err(|_| PackageError::Archive)?);
        Ok(usize::from(value))
    };
    let u32_at = |offset: usize| -> Result<usize, PackageError> {
        let end = offset.checked_add(4).ok_or(PackageError::Archive)?;
        let raw = bytes.get(offset..end).ok_or(PackageError::Archive)?;
        let value = u32::from_le_bytes(raw.try_into().map_err(|_| PackageError::Archive)?);
        Ok(value as usize)
    };
    let end = bytes.len().checked_sub(22).ok_or(PackageError::Archive)?;
    let eocd = (end.saturating_sub(65_535)..=end)
        .rev()
        .find(|offset| {
            bytes.get(*offset..*offset + 4) == Some(b"PK\x05\x06")
                && u16_at(*offset + 20).ok() == Some(bytes.len() - *offset - 22)
        })
        .ok_or(PackageError::Archive)?;
    let count = u16_at(eocd + 10)?;
    let directory_size = u32_at(eocd + 12)?;
    let directory = u32_at(eocd + 16)?;
    if u16_at(eocd + 4)? != 0
        || u16_at(eocd + 6)? != 0
        || u16_at(eocd + 8)? != count
        || !(2..=MAX_FILES + 1).contains(&count)
        || directory.checked_add(directory_size) != Some(eocd)
        || bytes.get(..4) != Some(b"PK\x03\x04")
    {
        return Err(PackageError::Archive);
    }
    let mut cursor = directory;
    let mut names = BTreeSet::new();
    let mut ranges = Vec::with_capacity(count);
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let fixed_end = cursor.checked_add(46).ok_or(PackageError::Archive)?;
        let fixed = bytes.get(cursor..fixed_end).ok_or(PackageError::Archive)?;
        if fixed.get(..4) != Some(b"PK\x01\x02") || u16_at(cursor + 34)? != 0 {
            return Err(PackageError::Archive);
        }
        let name_end = fixed_end
            .checked_add(u16_at(cursor + 28)?)
            .ok_or(PackageError::Archive)?;
        let extra_end = name_end
            .checked_add(u16_at(cursor + 30)?)
            .ok_or(PackageError::Archive)?;
        let member_end = extra_end
            .checked_add(u16_at(cursor + 32)?)
            .ok_or(PackageError::Archive)?;
        if member_end > eocd {
            return Err(PackageError::Archive);
        }
        let extra = bytes
            .get(name_end..extra_end)
            .ok_or(PackageError::Archive)?;
        validate_zip32_extra(extra)?;
        let raw_name = bytes
            .get(fixed_end..name_end)
            .ok_or(PackageError::Archive)?;
        let name = std::str::from_utf8(raw_name).map_err(|_| PackageError::Archive)?;
        if !safe_path(name) || !names.insert(name.to_ascii_lowercase()) {
            return Err(PackageError::Archive);
        }
        let local = u32_at(cursor + 42)?;
        let local_fixed_end = local.checked_add(30).ok_or(PackageError::Archive)?;
        let local_header = bytes
            .get(local..local_fixed_end)
            .ok_or(PackageError::Archive)?;
        let compressed = u32_at(cursor + 20)?;
        if local_header.get(..4) != Some(b"PK\x03\x04")
            || compressed == u32::MAX as usize
            || u32_at(cursor + 24)? == u32::MAX as usize
            || u32_at(local + 18)? == u32::MAX as usize
            || u32_at(local + 22)? == u32::MAX as usize
            || u16_at(local + 6)? != u16_at(cursor + 8)?
            || u16_at(local + 8)? != u16_at(cursor + 10)?
        {
            return Err(PackageError::Archive);
        }
        let local_name_end = local_fixed_end
            .checked_add(u16_at(local + 26)?)
            .ok_or(PackageError::Archive)?;
        let data_start = local_name_end
            .checked_add(u16_at(local + 28)?)
            .ok_or(PackageError::Archive)?;
        let data_end = data_start
            .checked_add(compressed)
            .ok_or(PackageError::Archive)?;
        if bytes.get(local_fixed_end..local_name_end) != Some(raw_name) || data_end > directory {
            return Err(PackageError::Archive);
        }
        let local_extra = bytes
            .get(local_name_end..data_start)
            .ok_or(PackageError::Archive)?;
        validate_zip32_extra(local_extra)?;
        let record_end = if u16_at(local + 6)? & 8 != 0 {
            let descriptor = if bytes.get(data_end..data_end + 4) == Some(b"PK\x07\x08") {
                data_end + 4
            } else {
                data_end
            };
            let descriptor_end = descriptor.checked_add(12).ok_or(PackageError::Archive)?;
            if descriptor_end > directory
                || u32_at(descriptor)? != u32_at(cursor + 16)?
                || u32_at(descriptor + 4)? != compressed
                || u32_at(descriptor + 8)? != u32_at(cursor + 24)?
            {
                return Err(PackageError::Archive);
            }
            descriptor_end
        } else {
            if u32_at(local + 14)? != u32_at(cursor + 16)?
                || u32_at(local + 18)? != compressed
                || u32_at(local + 22)? != u32_at(cursor + 24)?
            {
                return Err(PackageError::Archive);
            }
            data_end
        };
        ranges.push((local, record_end));
        entries.push(PhysicalZipEntry {
            central_start: cursor,
            local_start: local,
        });
        cursor = member_end;
    }
    ranges.sort_unstable();
    if cursor != eocd
        || ranges.first().map(|range| range.0) != Some(0)
        || ranges.last().map(|range| range.1) != Some(directory)
        || ranges.windows(2).any(|pair| pair[0].1 != pair[1].0)
    {
        return Err(PackageError::Archive);
    }
    Ok(entries)
}

pub fn verify_catalog_bundle(
    bytes: &[u8],
    policy: &CatalogAuthorization,
) -> Result<VerifiedPackage, PackageError> {
    if bytes.len() > MAX_ARCHIVE {
        return Err(PackageError::Limit);
    }
    let package_digest = digest(bytes);
    if package_digest != policy.package_sha256 || policy.trust_epoch == 0 {
        return Err(PackageError::Authorization);
    }
    let physical_entries = physical_zip_entries(bytes)?;
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| PackageError::Archive)?;
    if archive.len() != physical_entries.len() {
        return Err(PackageError::Archive);
    }
    let mut expanded = 0u64;
    let mut names = BTreeSet::new();
    let mut resources = BTreeMap::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|_| PackageError::Archive)?;
        if file.central_header_start() != physical_entries[index].central_start as u64
            || file.header_start() != physical_entries[index].local_start as u64
        {
            return Err(PackageError::Archive);
        }
        let name = file.name().map_err(|_| PackageError::Archive)?.into_owned();
        let mode = file.unix_mode().unwrap_or(0o100644);
        if !safe_path(&name)
            || !names.insert(name.to_ascii_lowercase())
            || mode & 0o170000 != 0o100000
            || mode & 0o111 != 0
        {
            return Err(PackageError::Archive);
        }
        expanded = expanded
            .checked_add(file.size())
            .ok_or(PackageError::Limit)?;
        if expanded > MAX_EXPANDED
            || file.size() > file.compressed_size().saturating_mul(100)
            || (name == "manifest.json" && file.size() > MAX_MANIFEST as u64)
        {
            return Err(PackageError::Limit);
        }
        let size = file.size();
        let mut data = Vec::new();
        file.by_ref()
            .take(size + 1)
            .read_to_end(&mut data)
            .map_err(|_| PackageError::Archive)?;
        if data.len() as u64 != size {
            return Err(PackageError::Integrity);
        }
        resources.insert(name, data);
    }
    let raw = resources
        .remove("manifest.json")
        .ok_or(PackageError::Manifest)?;
    let UniqueJson(mut value) = serde_json::from_slice(&raw).map_err(|_| PackageError::Manifest)?;
    let manifest: Manifest =
        serde_json::from_value(value.clone()).map_err(|_| PackageError::Manifest)?;
    if manifest.manifest_version != 1
        || manifest.package_class != "catalog"
        || manifest.runtime != "web"
        || !manifest.offline_core
        || !manifest.requested_capabilities.is_empty()
        || manifest.network_policy.mode != "deny"
        || !manifest.network_policy.allowed_origins.is_empty()
        || manifest.min_host_api == 0
        || manifest.min_host_api > policy.host_api
        || !identifier(&manifest.app_id)
        || !identifier(&manifest.publisher_id)
        || !semver(&manifest.version)
        || manifest.display_name.is_empty()
        || manifest.display_name.chars().count() > 160
        || !safe_path(&manifest.entrypoint)
        || manifest.files.is_empty()
    {
        return Err(PackageError::Manifest);
    }
    if manifest.app_id != policy.app_id
        || manifest.publisher_id != policy.publisher_id
        || manifest.version != policy.version
        || manifest.signature.key_id != policy.key_id
    {
        return Err(PackageError::Authorization);
    }
    if manifest.signature.algorithm != "Ed25519" {
        return Err(PackageError::Signature);
    }
    let encoded = URL_SAFE_NO_PAD
        .decode(&manifest.signature.value)
        .map_err(|_| PackageError::Signature)?;
    let signature = Signature::from_slice(&encoded).map_err(|_| PackageError::Signature)?;
    let key = VerifyingKey::from_bytes(&policy.public_key).map_err(|_| PackageError::Signature)?;
    value
        .as_object_mut()
        .ok_or(PackageError::Manifest)?
        .remove("signature");
    let mut payload = DOMAIN.to_vec();
    payload.extend(canonical(&value)?);
    key.verify_strict(&payload, &signature)
        .map_err(|_| PackageError::Signature)?;
    if resources.len() != manifest.files.len() {
        return Err(PackageError::Integrity);
    }
    for (name, item) in &manifest.files {
        if !safe_path(name)
            || expected_mime(name) != Some(item.mime.as_str())
            || item.bytes > MAX_EXPANDED
            || item.sha256.len() != 64
            || !item
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(PackageError::Manifest);
        }
        let data = resources.get(name).ok_or(PackageError::Integrity)?;
        if data.len() as u64 != item.bytes || digest(data) != item.sha256 {
            return Err(PackageError::Integrity);
        }
    }
    if manifest
        .files
        .get(&manifest.entrypoint)
        .map(|r| r.mime.as_str())
        != Some("text/html")
    {
        return Err(PackageError::Manifest);
    }
    Ok(VerifiedPackage {
        manifest,
        digest: package_digest,
        resources,
        trust_epoch: policy.trust_epoch,
        archive: bytes.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::io::Write;
    use zip::{ZipWriter, write::SimpleFileOptions};

    #[test]
    fn native_installed_entry_serializes_the_offline_catalog_contract() {
        let ready = NativeMiniAppEntry {
            app_id: "org.fabushi.global-dandelion".into(),
            display_name: "Global Dandelion".into(),
            publisher_id: "bhrumom".into(),
            version: "0.1.0".into(),
            package_digest: "a".repeat(64),
            size_bytes: 4096,
            offline_core: true,
            installed: true,
            supported: true,
            state: NativeMiniAppInstallState::Ready,
        };
        let json = serde_json::to_value(ready).unwrap();
        assert_eq!(json["appId"], "org.fabushi.global-dandelion");
        assert_eq!(json["displayName"], "Global Dandelion");
        assert_eq!(json["publisherId"], "bhrumom");
        assert_eq!(json["version"], "0.1.0");
        let expected_digest = "a".repeat(64);
        assert_eq!(
            json["packageDigest"].as_str(),
            Some(expected_digest.as_str())
        );
        assert_eq!(json["sizeBytes"], 4096);
        assert_eq!(json["offlineCore"], true);
        assert_eq!(json["installed"], true);
        assert_eq!(json["supported"], true);
        assert_eq!(json["state"], "ready");

        let quarantined = NativeMiniAppEntry {
            app_id: "org.fabushi.global-dandelion".into(),
            display_name: "org.fabushi.global-dandelion".into(),
            publisher_id: "bhrumom".into(),
            version: "0.1.0".into(),
            package_digest: "a".repeat(64),
            size_bytes: 4096,
            offline_core: true,
            installed: true,
            supported: false,
            state: NativeMiniAppInstallState::Quarantined,
        };
        let json = serde_json::to_value(quarantined).unwrap();
        assert_eq!(json["installed"], true);
        assert_eq!(json["supported"], false);
        assert_eq!(json["state"], "quarantined");
    }

    #[test]
    fn actual_production_signed_candidate_interoperability() {
        let Ok(path) = std::env::var("FABUSHI_SIGNED_HOST_FIXTURE") else {
            return; // Separate synthetic contracts do not claim production acceptance.
        };
        let bytes = std::fs::read(path).unwrap();
        let policy = CatalogAuthorization {
            app_id: "org.fabushi.global-dandelion".into(),
            publisher_id: "bhrumom".into(),
            version: "0.1.0".into(),
            package_sha256: "de10e02115415908f827723457817e51cb4f39aa5ca68798da824fecc483118a"
                .into(),
            key_id: "fabushi-marketplace-managed-20261013-1".into(),
            public_key: URL_SAFE_NO_PAD
                .decode("VbvgbVxNAeftFYB0l1-NzE2-Gx3d4NQ7hkKiQ0HMpu0")
                .unwrap()
                .try_into()
                .unwrap(),
            trust_epoch: 1,
            host_api: 1,
        };
        // This explicit test policy proves only package interoperability. It is
        // not a catalog approval or a production authorization constructor.
        let verified = verify_catalog_bundle(&bytes, &policy).unwrap();
        assert_eq!(verified.entrypoint(), "index.html");
        assert!(verified.resource("app.js").is_some());
        assert!(
            verified
                .resource("pkg/dandelion_web_adapter_bg.wasm")
                .is_some()
        );
    }

    fn archive(items: Vec<(String, Vec<u8>)>) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, data) in items {
            writer
                .start_file(
                    name,
                    SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Stored)
                        .unix_permissions(0o644),
                )
                .unwrap();
            writer.write_all(&data).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }
    pub(super) fn fixture(
        mutate: impl FnOnce(&mut Value),
        resource: &[u8],
    ) -> (Vec<u8>, CatalogAuthorization) {
        let key = SigningKey::from_bytes(&[7; 32]); // Test fixture, never production trust.
        let expected = b"<!doctype html><p>offline</p>";
        let mut manifest = serde_json::json!({
            "manifestVersion":1,"packageClass":"catalog","appId":"org.test.app",
            "publisherId":"test-publisher","version":"1.0.0","displayName":"测试 app",
            "entrypoint":"index.html","runtime":"web","minHostApi":1,"offlineCore":true,
            "requestedCapabilities":[],"networkPolicy":{"mode":"deny","allowedOrigins":[]},
            "files":{"index.html":{"sha256":digest(expected),"bytes":expected.len(),"mime":"text/html"}}
        });
        mutate(&mut manifest);
        let mut payload = DOMAIN.to_vec();
        payload.extend(canonical(&manifest).unwrap());
        manifest["signature"] = serde_json::json!({"algorithm":"Ed25519","keyId":"fixture-only",
            "value":URL_SAFE_NO_PAD.encode(key.sign(&payload).to_bytes())});
        let bytes = archive(vec![
            (
                "manifest.json".into(),
                serde_json::to_vec(&manifest).unwrap(),
            ),
            ("index.html".into(), resource.to_vec()),
        ]);
        let policy = CatalogAuthorization {
            app_id: "org.test.app".into(),
            publisher_id: "test-publisher".into(),
            version: "1.0.0".into(),
            package_sha256: digest(&bytes),
            key_id: "fixture-only".into(),
            public_key: key.verifying_key().to_bytes(),
            trust_epoch: 1,
            host_api: 1,
        };
        (bytes, policy)
    }
    pub(super) const HTML: &[u8] = b"<!doctype html><p>offline</p>";
    #[test]
    fn verifies_signed_content_and_descriptor() {
        let (bytes, policy) = fixture(|_| {}, HTML);
        let verified = verify_catalog_bundle(&bytes, &policy).unwrap();
        assert_eq!(verified.resource("index.html"), Some((HTML, "text/html")));
        assert_eq!(verified.app_id(), "org.test.app");
        assert_eq!(verified.trust_epoch(), 1);
        assert!(verified.resource("../host-token").is_none());
    }
    #[test]
    fn rejects_digest_publisher_and_key_swaps() {
        let (bytes, mut policy) = fixture(|_| {}, HTML);
        policy.package_sha256 = "0".repeat(64);
        assert!(matches!(
            verify_catalog_bundle(&bytes, &policy),
            Err(PackageError::Authorization)
        ));
        policy.package_sha256 = digest(&bytes);
        policy.publisher_id = "other".into();
        assert!(matches!(
            verify_catalog_bundle(&bytes, &policy),
            Err(PackageError::Authorization)
        ));
        policy.publisher_id = "test-publisher".into();
        policy.public_key = SigningKey::from_bytes(&[8; 32]).verifying_key().to_bytes();
        assert!(matches!(
            verify_catalog_bundle(&bytes, &policy),
            Err(PackageError::Signature)
        ));
    }
    #[test]
    fn rejects_unknown_fields_capabilities_network_and_host() {
        for field in ["unknown", "capabilities", "network", "host"] {
            let (bytes, policy) = fixture(
                |m| match field {
                    "unknown" => m["admin"] = Value::Bool(true),
                    "capabilities" => {
                        m["requestedCapabilities"] = serde_json::json!(["shell.exec"])
                    }
                    "network" => m["networkPolicy"]["mode"] = Value::String("allowlist".into()),
                    _ => m["minHostApi"] = serde_json::json!(2),
                },
                HTML,
            );
            assert!(matches!(
                verify_catalog_bundle(&bytes, &policy),
                Err(PackageError::Manifest)
            ));
        }
    }
    #[test]
    fn rejects_resource_corruption_and_mime() {
        let (bytes, policy) = fixture(|_| {}, b"changed");
        assert!(matches!(
            verify_catalog_bundle(&bytes, &policy),
            Err(PackageError::Integrity)
        ));
        let (bytes, policy) = fixture(
            |m| m["files"]["index.html"]["mime"] = Value::String("application/octet-stream".into()),
            HTML,
        );
        assert!(matches!(
            verify_catalog_bundle(&bytes, &policy),
            Err(PackageError::Manifest)
        ));
    }
    #[test]
    fn rejects_duplicate_json_and_unsafe_paths() {
        assert!(serde_json::from_str::<UniqueJson>(r#"{"files":{"x":{},"x":{}}}"#).is_err());
        assert!(serde_json::from_str::<UniqueJson>(r#"{"n":9007199254740992}"#).is_err());
        for path in [
            "../x",
            "/x",
            "a/../x",
            "C:/x",
            "a\\x",
            "CON.txt",
            "aux/x",
            "a./x",
            "x//y",
            "中文.js",
        ] {
            assert!(!safe_path(path), "{path}");
        }
        assert!(safe_path("pkg/adapter_bg.wasm"));
    }
    #[test]
    fn rejects_unlisted_and_case_colliding_members() {
        let (bytes, mut policy) = fixture(|_| {}, HTML);
        let mut source = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut manifest = Vec::new();
        source
            .by_name("manifest.json")
            .unwrap()
            .read_to_end(&mut manifest)
            .unwrap();
        for extra in ["rogue.js", "INDEX.HTML", "../escape"] {
            let data = archive(vec![
                ("manifest.json".into(), manifest.clone()),
                ("index.html".into(), HTML.to_vec()),
                (extra.into(), b"x".to_vec()),
            ]);
            policy.package_sha256 = digest(&data);
            assert!(verify_catalog_bundle(&data, &policy).is_err());
        }
    }
    // Test-only ZIP metadata mutation. ZipWriter forbids duplicate names and
    // normalizes Unix types, so mutate an otherwise valid, signed fixture.
    fn central_members(bytes: &[u8]) -> Vec<usize> {
        let end = bytes.len() - 22; // Fixtures have no archive comment or ZIP64.
        assert_eq!(&bytes[end..end + 4], b"PK\x05\x06");
        let count = u16::from_le_bytes(bytes[end + 10..end + 12].try_into().unwrap());
        let directory_offset = &bytes[end + 16..end + 20];
        let mut offset = u32::from_le_bytes(directory_offset.try_into().unwrap()) as usize;
        let mut members = Vec::new();
        for _ in 0..count {
            assert_eq!(&bytes[offset..offset + 4], b"PK\x01\x02");
            members.push(offset);
            let lengths: usize = [28, 30, 32]
                .iter()
                .map(|field| {
                    let size = &bytes[offset + *field..offset + *field + 2];
                    usize::from(u16::from_le_bytes(size.try_into().unwrap()))
                })
                .sum();
            offset += 46 + lengths;
        }
        assert_eq!(offset, end);
        members
    }

    #[test]
    fn rejects_identical_zip_members_with_matching_outer_digest() {
        let (control, mut policy) = fixture(|_| {}, HTML);
        assert!(verify_catalog_bundle(&control, &policy).is_ok());
        let mut source = zip::ZipArchive::new(Cursor::new(control)).unwrap();
        let mut manifest = Vec::new();
        source
            .by_name("manifest.json")
            .unwrap()
            .read_to_end(&mut manifest)
            .unwrap();
        let mut data = archive(vec![
            ("manifest.json".into(), manifest),
            ("index.html".into(), HTML.to_vec()),
            ("other.html".into(), HTML.to_vec()),
        ]);
        let central = central_members(&data)[2];
        let local_offset = &data[central + 42..central + 46];
        let local = u32::from_le_bytes(local_offset.try_into().unwrap()) as usize;
        assert_eq!(&data[central + 46..central + 56], b"other.html");
        assert_eq!(&data[local + 30..local + 40], b"other.html");
        data[central + 46..central + 56].copy_from_slice(b"index.html");
        data[local + 30..local + 40].copy_from_slice(b"index.html");
        // Inspect raw central names: a ZIP reader may deduplicate its name index.
        let names: Vec<_> = central_members(&data)
            .into_iter()
            .map(|offset| &data[offset + 46..offset + 56])
            .collect();
        assert_eq!(
            names.iter().filter(|name| **name == b"index.html").count(),
            2
        );
        policy.package_sha256 = digest(&data);
        assert!(matches!(
            verify_catalog_bundle(&data, &policy),
            Err(PackageError::Archive)
        ));
    }

    #[test]
    fn physical_directory_rejects_truncation_forged_counts_offsets_and_local_name_swaps() {
        let (control, _) = fixture(|_| {}, HTML);
        assert_eq!(
            physical_zip_entries(&control).map(|entries| entries.len()),
            Ok(2)
        );
        for length in 0..control.len() {
            assert!(physical_zip_entries(&control[..length]).is_err());
        }
        let end = control.len() - 22;
        for (field, replacement) in [(4, 1_u16), (8, 1), (10, 1)] {
            let mut data = control.clone();
            data[end + field..end + field + 2].copy_from_slice(&replacement.to_le_bytes());
            assert_eq!(physical_zip_entries(&data), Err(PackageError::Archive));
        }
        let mut data = control.clone();
        data[end + 16..end + 20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(physical_zip_entries(&data), Err(PackageError::Archive));
        let central = central_members(&control)[0];
        let mut data = control.clone();
        data[central + 46] = b'X';
        assert_eq!(physical_zip_entries(&data), Err(PackageError::Archive));
        let mut data = control.clone();
        data[central + 20..central + 24].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(physical_zip_entries(&data), Err(PackageError::Archive));
        for offset in [18, 22] {
            let mut data = control.clone();
            data[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
            assert_eq!(physical_zip_entries(&data), Err(PackageError::Archive));
        }
        assert_eq!(validate_zip32_extra(&[]), Ok(()));
        assert_eq!(validate_zip32_extra(&[255, 255, 1, 0, 7]), Ok(()));
        for extra in [&[1, 0, 0, 0][..], &[1, 0], &[255, 255, 2, 0, 7]] {
            assert_eq!(validate_zip32_extra(extra), Err(PackageError::Archive));
        }
    }

    #[test]
    fn verifies_zip32_data_descriptor_and_rejects_descriptor_crc_tampering() {
        let (control, mut policy) = fixture(|_| {}, HTML);
        let central = central_members(&control);
        let first = central[0];
        let size = u32::from_le_bytes(control[first + 20..first + 24].try_into().unwrap());
        let header_size = 30
            + usize::from(u16::from_le_bytes(control[26..28].try_into().unwrap()))
            + usize::from(u16::from_le_bytes(control[28..30].try_into().unwrap()));
        let data_end = header_size + size as usize;
        let mut descriptor = b"PK\x07\x08".to_vec();
        descriptor.extend_from_slice(&control[first + 16..first + 28]);
        let mut data = control.clone();
        data[6] |= 8;
        data[first + 8] |= 8;
        data.splice(data_end..data_end, descriptor);
        for original in central {
            let offset = original + 16;
            let local = u32::from_le_bytes(data[offset + 42..offset + 46].try_into().unwrap());
            if local as usize >= data_end {
                data[offset + 42..offset + 46].copy_from_slice(&(local + 16).to_le_bytes());
            }
        }
        let end = data.len() - 22;
        data[end + 16..end + 20].copy_from_slice(&((first + 16) as u32).to_le_bytes());
        policy.package_sha256 = digest(&data);
        assert!(verify_catalog_bundle(&data, &policy).is_ok());
        data[data_end + 4] ^= 1;
        policy.package_sha256 = digest(&data);
        assert_eq!(
            verify_catalog_bundle(&data, &policy).err(),
            Some(PackageError::Archive)
        );
    }

    #[test]
    fn rejects_an_alternate_directory_after_an_earlier_eocd() {
        let (control, mut policy) = fixture(|_| {}, HTML);
        let first = central_members(&control)[0];
        let end = control.len() - 22;
        let mut data = control.clone();
        data.extend_from_slice(&control[first..end]);
        let new_end = data.len();
        data.extend_from_slice(&control[end..]);
        data[new_end + 16..new_end + 20].copy_from_slice(&(control.len() as u32).to_le_bytes());
        policy.package_sha256 = digest(&data);
        // Equal entry count cannot make the earlier directory a valid fallback.
        assert_eq!(
            verify_catalog_bundle(&data, &policy).err(),
            Some(PackageError::Archive)
        );
    }

    #[test]
    fn rejects_symlink_special_and_executable_zip_members() {
        for mode in [
            0o120777u32,
            0o010644,
            0o020644,
            0o060644,
            0o140644,
            0o040755,
            0o100755,
        ] {
            let (mut data, mut policy) = fixture(|_| {}, HTML);
            assert!(verify_catalog_bundle(&data, &policy).is_ok());
            let central = central_members(&data)[1];
            data[central + 5] = 3; // Unix creator: external attributes carry st_mode.
            data[central + 38..central + 42].copy_from_slice(&(mode << 16).to_le_bytes());
            let mut inspected = zip::ZipArchive::new(Cursor::new(&data)).unwrap();
            assert_eq!(inspected.by_index(1).unwrap().unix_mode(), Some(mode));
            policy.package_sha256 = digest(&data);
            assert!(
                matches!(
                    verify_catalog_bundle(&data, &policy),
                    Err(PackageError::Archive)
                ),
                "mode {mode:o}"
            );
        }
    }

    #[test]
    fn rejects_actual_deflate_inflation_bomb_before_resource_verification() {
        let resource = vec![0u8; 64 * 1024];
        let (control, mut policy) = fixture(
            |manifest| {
                manifest["files"]["index.html"]["sha256"] = Value::String(digest(&resource));
                manifest["files"]["index.html"]["bytes"] = serde_json::json!(resource.len());
            },
            &resource,
        );
        assert!(verify_catalog_bundle(&control, &policy).is_ok());
        let mut source = zip::ZipArchive::new(Cursor::new(control)).unwrap();
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for index in 0..source.len() {
            let mut member = source.by_index(index).unwrap();
            writer
                .start_file(
                    member.name().unwrap(),
                    SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated)
                        .unix_permissions(0o644),
                )
                .unwrap();
            std::io::copy(&mut member, &mut writer).unwrap();
        }
        let data = writer.finish().unwrap().into_inner();
        let mut inspected = zip::ZipArchive::new(Cursor::new(&data)).unwrap();
        let member = inspected.by_name("index.html").unwrap();
        assert_eq!(member.compression(), zip::CompressionMethod::Deflated);
        assert!(member.size() > member.compressed_size() * 100);
        policy.package_sha256 = digest(&data);
        assert!(matches!(
            verify_catalog_bundle(&data, &policy),
            Err(PackageError::Limit)
        ));
    }

    #[test]
    fn rejects_declared_expansion_limit_and_missing_listed_resource() {
        let (mut data, mut policy) = fixture(|_| {}, HTML);
        let central = central_members(&data)[1];
        let declared = (MAX_EXPANDED as u32 + 1).to_le_bytes();
        data[central + 24..central + 28].copy_from_slice(&declared);
        policy.package_sha256 = digest(&data);
        assert!(matches!(
            verify_catalog_bundle(&data, &policy),
            Err(PackageError::Archive)
        ));
        // Match both headers so the second case reaches the expansion budget,
        // rather than being rejected earlier as an inconsistent physical ZIP.
        let local_offset = &data[central + 42..central + 46];
        let local = u32::from_le_bytes(local_offset.try_into().unwrap()) as usize;
        data[local + 22..local + 26].copy_from_slice(&declared);
        policy.package_sha256 = digest(&data);
        assert!(matches!(
            verify_catalog_bundle(&data, &policy),
            Err(PackageError::Limit)
        ));

        let (control, mut policy) = fixture(|_| {}, HTML);
        let mut source = zip::ZipArchive::new(Cursor::new(control)).unwrap();
        let mut manifest = Vec::new();
        source
            .by_name("manifest.json")
            .unwrap()
            .read_to_end(&mut manifest)
            .unwrap();
        let missing = archive(vec![
            ("manifest.json".into(), manifest),
            ("other.html".into(), HTML.to_vec()),
        ]);
        policy.package_sha256 = digest(&missing);
        assert!(matches!(
            verify_catalog_bundle(&missing, &policy),
            Err(PackageError::Integrity)
        ));
    }

    #[test]
    fn strict_versions() {
        for version in ["1.0.0", "1.2.3-alpha.1+build.2"] {
            assert!(semver(version));
        }
        for version in ["1.0", "01.0.0", "1.0.0-", "1.0.0+", "1.0.0-01", "1.0.0+x+y"] {
            assert!(!semver(version));
        }
    }
}
