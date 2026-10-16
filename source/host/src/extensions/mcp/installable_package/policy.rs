//! Strict verification of the existing Backend install-proof wire envelope.
//! All signed strings are verified byte-for-byte; JSON is never re-canonicalized.
use super::{CatalogAuthorization, UniqueJson};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{io::Read, time::Duration};
use thiserror::Error;
use url::Url;

const PROOF_PROTOCOL: &str = "fabushi.extension-install-proof.v1";
const RELEASE_PROTOCOL: &str = "fabushi.extension-release-signature.v1";
const TRUST_PROTOCOL: &str = "fabushi.extension-trust-feed.v1";
const PINNED_FEED_KEY_ID: &str = "fabushi-marketplace-managed-20261013-1";
const PINNED_FEED_EPOCH: u64 = 1;
const PINNED_FEED_RAW_KEY: &str = "VbvgbVxNAeftFYB0l1-NzE2-Gx3d4NQ7hkKiQ0HMpu0";
const MAX_FUTURE_SKEW_MS: u64 = 5 * 60 * 1000;
const MAX_FEED_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1000;
const MAX_CATALOG_PAGES: usize = 10;
const MAX_ARCHIVE_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProofError {
    #[error("install proof is malformed or unsupported")]
    Malformed,
    #[error("install proof signature is invalid")]
    Signature,
    #[error("install proof descriptor is not bound to its signed digest")]
    DescriptorBinding,
    #[error("install proof release is not currently trusted")]
    Revoked,
    #[error("install proof trust feed is expired or future-dated")]
    FeedFreshness,
    #[error("signed trust feed rollback was rejected")]
    TrustRollback,
    #[error("Marketplace rejected the trust or release proof request")]
    Rejected,
    #[error("install proof does not authorize this mini-app package")]
    Authorization,
    #[error("Fabushi Marketplace request failed")]
    Network,
    #[error("approved artifact is unavailable")]
    NotFound,
    #[error("artifact response exceeded its signed size")]
    ArtifactSize,
}

pub fn is_transport_failure(error: &ProofError) -> bool {
    matches!(error, ProofError::Network)
}

/// Fixed-origin Marketplace client. URLs are constructed from the existing
/// Fabushi API origin and fixed route templates; no URL comes from a catalog,
/// descriptor, package, Renderer, or Host intent.
pub struct CatalogClient {
    origin: Url,
    client: reqwest::blocking::Client,
}

impl CatalogClient {
    pub fn production_from_environment() -> Result<Self, ProofError> {
        // Marketplace reads are public and issuer-independent. Never inherit
        // the authenticated Native account endpoint or its bearer context.
        let mut origin =
            Url::parse("https://api.ombhrum.com").map_err(|_| ProofError::Malformed)?;
        origin.set_path("");
        let client = reqwest::blocking::Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| ProofError::Network)?;
        Ok(Self { origin, client })
    }

    fn route(&self, segments: &[&str]) -> Result<Url, ProofError> {
        let mut url = self.origin.clone();
        {
            let mut path = url
                .path_segments_mut()
                .map_err(|_| ProofError::Authorization)?;
            path.clear();
            path.push("v1").push("extensions");
            for segment in segments {
                path.push(segment);
            }
        }
        Ok(url)
    }

    pub fn list_descriptors(&self) -> Result<Vec<Value>, ProofError> {
        let mut cursor: Option<String> = None;
        let mut entries = Vec::new();
        for _ in 0..MAX_CATALOG_PAGES {
            let mut url = self.route(&[])?;
            {
                let mut query = url.query_pairs_mut();
                query.append_pair("kind", "web-miniapp");
                query.append_pair("platform", "desktop");
                query.append_pair("limit", "100");
                if let Some(value) = &cursor {
                    query.append_pair("cursor", value);
                }
            }
            let response = self
                .client
                .get(url)
                .send()
                .map_err(|_| ProofError::Network)?;
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                return Err(ProofError::NotFound);
            }
            if !response.status().is_success() {
                return Err(ProofError::Rejected);
            }
            let mut limited = response.take(4 * 1024 * 1024 + 1);
            let mut bytes = Vec::new();
            limited
                .read_to_end(&mut bytes)
                .map_err(|_| ProofError::Network)?;
            if bytes.len() > 4 * 1024 * 1024 {
                return Err(ProofError::Malformed);
            }
            let UniqueJson(body) =
                serde_json::from_slice(&bytes).map_err(|_| ProofError::Malformed)?;
            let page: RegistryPage =
                serde_json::from_value(body).map_err(|_| ProofError::Malformed)?;
            if page.protocol != "fabushi.extension-registry.v1" || page.schema_version != 1 {
                return Err(ProofError::Malformed);
            }
            entries.extend(page.entries);
            if entries.len() > 1000 {
                return Err(ProofError::Malformed);
            }
            match page.next_cursor {
                Some(next) => cursor = Some(next),
                None => return Ok(entries),
            }
        }
        if cursor.is_some() {
            return Err(ProofError::Malformed);
        }
        Ok(entries)
    }

    /// Fetch the Node-signed trust envelope from its independent install route.
    /// The Worker `/v1/extensions/trust` route is intentionally not used here.
    pub fn fetch_install_trust(&self) -> Result<String, ProofError> {
        let response = self
            .client
            .get(self.route(&["install-trust"])?)
            .send()
            .map_err(|_| ProofError::Network)?;
        if !response.status().is_success() {
            return Err(ProofError::Rejected);
        }
        let mut limited = response.take(1024 * 1024 + 1);
        let mut bytes = Vec::new();
        limited
            .read_to_end(&mut bytes)
            .map_err(|_| ProofError::Network)?;
        if bytes.len() > 1024 * 1024 {
            return Err(ProofError::Malformed);
        }
        let raw = String::from_utf8(bytes).map_err(|_| ProofError::Malformed)?;
        extract_install_trust_response(&raw)
    }

    pub fn fetch_install_proof(&self, descriptor: &Value) -> Result<String, ProofError> {
        let kind = string_at(descriptor, &["identity", "kind"]).ok_or(ProofError::Malformed)?;
        let namespace =
            string_at(descriptor, &["identity", "namespace"]).ok_or(ProofError::Malformed)?;
        let name = string_at(descriptor, &["identity", "name"]).ok_or(ProofError::Malformed)?;
        let version =
            string_at(descriptor, &["release", "version"]).ok_or(ProofError::Malformed)?;
        let url = self.route(&["install", kind, namespace, name, version])?;
        let response = self
            .client
            .get(url)
            .send()
            .map_err(|_| ProofError::Network)?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ProofError::NotFound);
        }
        if !response.status().is_success() {
            return Err(ProofError::Rejected);
        }
        let mut limited = response.take(2 * 1024 * 1024 + 1);
        let mut bytes = Vec::new();
        limited
            .read_to_end(&mut bytes)
            .map_err(|_| ProofError::Network)?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err(ProofError::Malformed);
        }
        let raw = String::from_utf8(bytes).map_err(|_| ProofError::Malformed)?;
        let proof: InstallProofEnvelope = parse_unique(&raw)?;
        if string_at(&proof.descriptor, &["identity", "kind"]) != Some(kind)
            || string_at(&proof.descriptor, &["identity", "namespace"]) != Some(namespace)
            || string_at(&proof.descriptor, &["identity", "name"]) != Some(name)
            || string_at(&proof.descriptor, &["release", "version"]) != Some(version)
        {
            return Err(ProofError::DescriptorBinding);
        }
        Ok(raw)
    }

    pub fn download_artifact(
        &self,
        digest: &str,
        expected_size: u64,
    ) -> Result<Vec<u8>, ProofError> {
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || expected_size == 0
            || expected_size > MAX_ARCHIVE_BYTES
        {
            return Err(ProofError::Authorization);
        }
        let url = self.route(&["artifacts", digest])?;
        let response = self
            .client
            .get(url)
            .send()
            .map_err(|_| ProofError::Network)?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ProofError::NotFound);
        }
        if !response.status().is_success() {
            return Err(ProofError::Network);
        }
        if response
            .content_length()
            .is_some_and(|length| length != expected_size)
        {
            return Err(ProofError::ArtifactSize);
        }
        let mut limited = response.take(expected_size.saturating_add(1));
        let mut bytes = Vec::with_capacity(expected_size as usize);
        limited
            .read_to_end(&mut bytes)
            .map_err(|_| ProofError::Network)?;
        if bytes.len() as u64 != expected_size || bytes.len() as u64 > MAX_ARCHIVE_BYTES {
            return Err(ProofError::ArtifactSize);
        }
        if sha256_hex(&bytes) != digest {
            return Err(ProofError::Authorization);
        }
        Ok(bytes)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RegistryPage {
    protocol: String,
    schema_version: u64,
    entries: Vec<Value>,
    next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallProofEnvelope {
    pub protocol: String,
    pub descriptor: Value,
    pub descriptor_payload: String,
    pub release_signature: ReleaseSignatureEnvelope,
    pub trust: TrustEnvelope,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseSignatureEnvelope {
    pub protocol: String,
    pub algorithm: String,
    pub key_id: String,
    pub trust_epoch: u64,
    pub public_key: String,
    pub public_key_fingerprint: String,
    pub payload: String,
    pub signature: String,
    pub signed_at_ms: u64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrustEnvelope {
    pub protocol: String,
    pub algorithm: String,
    pub key_id: String,
    pub trust_epoch: u64,
    pub public_key: String,
    pub public_key_fingerprint: String,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub payload: String,
    pub signature: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TrustPayload {
    protocol: String,
    issued_at_ms: u64,
    expires_at_ms: u64,
    signer: TrustSigner,
    keys: Vec<TrustKey>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TrustSigner {
    algorithm: String,
    key_id: String,
    trust_epoch: u64,
    public_key_fingerprint: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TrustKey {
    algorithm: String,
    key_id: String,
    trust_epoch: u64,
    public_key: String,
    public_key_fingerprint: String,
    status: String,
    created_at_ms: u64,
    revoked_at_ms: Option<u64>,
    revocation_reason: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReleasePayload {
    protocol: String,
    key_id: String,
    trust_epoch: u64,
    identity: ReleaseIdentity,
    version: String,
    descriptor_digest: String,
    artifacts: Vec<SignedArtifact>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleaseIdentity {
    namespace: String,
    name: String,
    kind: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignedArtifact {
    pub artifact_id: String,
    pub digest: String,
    pub size_bytes: u64,
    pub platform: String,
    pub architecture: String,
    pub format: String,
}

pub struct VerifiedInstallProof {
    pub descriptor: Value,
    pub descriptor_payload: String,
    pub authorization: CatalogAuthorization,
    pub artifact: SignedArtifact,
    pub proof_snapshot: String,
}

#[derive(Clone, Debug)]
pub struct TrustAnchor {
    pub key_id: String,
    pub trust_epoch: u64,
    pub public_key: [u8; 32],
}

#[derive(Clone, Debug)]
pub struct VerifiedTrustFeed {
    pub epoch: u64,
    pub issued_at_ms: u64,
    pub signed_payload: String,
    pub envelope_json: String,
}

fn extract_install_trust_response(raw: &str) -> Result<String, ProofError> {
    let UniqueJson(mut wrapper) =
        serde_json::from_str::<UniqueJson>(raw).map_err(|_| ProofError::Malformed)?;
    let object = wrapper.as_object_mut().ok_or(ProofError::Malformed)?;
    let trust = object.remove("trust").ok_or(ProofError::Malformed)?;
    if !object.is_empty() {
        return Err(ProofError::Malformed);
    }
    let trust_raw = serde_json::to_string(&trust).map_err(|_| ProofError::Malformed)?;
    let _: TrustEnvelope = parse_unique(&trust_raw)?;
    Ok(trust_raw)
}

pub fn pinned_marketplace_anchor() -> Result<TrustAnchor, ProofError> {
    let public_key = URL_SAFE_NO_PAD
        .decode(PINNED_FEED_RAW_KEY)
        .map_err(|_| ProofError::Malformed)?;
    Ok(TrustAnchor {
        key_id: PINNED_FEED_KEY_ID.to_string(),
        trust_epoch: PINNED_FEED_EPOCH,
        public_key: public_key.try_into().map_err(|_| ProofError::Malformed)?,
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn decode_spki(value: &str) -> Result<[u8; 32], ProofError> {
    let der = STANDARD.decode(value).map_err(|_| ProofError::Malformed)?;
    // RFC 8410 Ed25519 SubjectPublicKeyInfo: fixed 12-byte DER prefix + 32 raw bytes.
    const PREFIX: &[u8] = &[
        0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
    ];
    if der.len() != PREFIX.len() + 32 || !der.starts_with(PREFIX) {
        return Err(ProofError::Malformed);
    }
    let mut raw = [0_u8; 32];
    raw.copy_from_slice(&der[PREFIX.len()..]);
    Ok(raw)
}

fn verify_raw(public_key: &[u8; 32], payload: &str, signature: &str) -> Result<(), ProofError> {
    let key = VerifyingKey::from_bytes(public_key).map_err(|_| ProofError::Signature)?;
    let bytes = STANDARD
        .decode(signature)
        .map_err(|_| ProofError::Signature)?;
    let signature = Signature::from_slice(&bytes).map_err(|_| ProofError::Signature)?;
    key.verify(payload.as_bytes(), &signature)
        .map_err(|_| ProofError::Signature)
}

fn parse_unique<T: for<'de> Deserialize<'de>>(raw: &str) -> Result<T, ProofError> {
    let UniqueJson(value) = serde_json::from_str(raw).map_err(|_| ProofError::Malformed)?;
    serde_json::from_value(value).map_err(|_| ProofError::Malformed)
}

fn value_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter()
        .try_fold(value, |current, part| current.get(*part))
}
fn string_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    value_at(value, path)?.as_str()
}

/// Verify the Backend-approved proof and produce the only authorization value
/// accepted by `verify_catalog_bundle`. `allow_expired_feed` is reserved for
/// cold revalidation of an already-installed last-good snapshot.
pub fn verify_install_proof(
    raw: &str,
    expected_app_id: &str,
    now_ms: u64,
    allow_expired_feed: bool,
) -> Result<VerifiedInstallProof, ProofError> {
    verify_install_proof_with_anchor(
        raw,
        expected_app_id,
        now_ms,
        allow_expired_feed,
        &pinned_marketplace_anchor()?,
    )
}

pub fn verify_install_proof_with_anchor(
    raw: &str,
    expected_app_id: &str,
    now_ms: u64,
    allow_expired_feed: bool,
    anchor: &TrustAnchor,
) -> Result<VerifiedInstallProof, ProofError> {
    let proof: InstallProofEnvelope = parse_unique(raw)?;
    if proof.protocol != PROOF_PROTOCOL
        || proof.release_signature.protocol != RELEASE_PROTOCOL
        || proof.release_signature.algorithm != "Ed25519"
        || proof.trust.protocol != TRUST_PROTOCOL
        || proof.trust.algorithm != "Ed25519"
    {
        return Err(ProofError::Malformed);
    }
    let trust_payload = validate_trust_envelope(&proof.trust, now_ms, allow_expired_feed, anchor)?;

    let signed_key = trust_payload
        .keys
        .iter()
        .find(|key| {
            key.key_id == proof.release_signature.key_id
                && key.trust_epoch == proof.release_signature.trust_epoch
        })
        .ok_or(ProofError::Signature)?;
    if signed_key.status != "active" {
        return Err(ProofError::Revoked);
    }
    let release_raw = decode_spki(&proof.release_signature.public_key)?;
    let release_spki = STANDARD
        .decode(&proof.release_signature.public_key)
        .map_err(|_| ProofError::Malformed)?;
    if signed_key.public_key != proof.release_signature.public_key
        || signed_key.public_key_fingerprint != proof.release_signature.public_key_fingerprint
        || sha256_hex(&release_spki) != proof.release_signature.public_key_fingerprint
        || proof.release_signature.signed_at_ms > now_ms.saturating_add(MAX_FUTURE_SKEW_MS)
    {
        return Err(ProofError::Signature);
    }
    verify_raw(
        &release_raw,
        &proof.release_signature.payload,
        &proof.release_signature.signature,
    )?;
    let release: ReleasePayload = parse_unique(&proof.release_signature.payload)?;
    if release.protocol != RELEASE_PROTOCOL
        || release.key_id != proof.release_signature.key_id
        || release.trust_epoch != proof.release_signature.trust_epoch
        || release.identity.kind != "web-miniapp"
        || release.identity.namespace
            != string_at(&proof.descriptor, &["identity", "namespace"]).unwrap_or_default()
        || release.identity.name
            != string_at(&proof.descriptor, &["identity", "name"]).unwrap_or_default()
        || release.version
            != string_at(&proof.descriptor, &["release", "version"]).unwrap_or_default()
        || release.descriptor_digest != sha256_hex(proof.descriptor_payload.as_bytes())
    {
        return Err(ProofError::DescriptorBinding);
    }
    let parsed_descriptor: Value = parse_unique(&proof.descriptor_payload)?;
    if parsed_descriptor != proof.descriptor
        || descriptor_app_id(&proof.descriptor) != Some(expected_app_id)
        || string_at(&proof.descriptor, &["release", "bundleDigest"]).is_none()
        || value_at(&proof.descriptor, &["release", "revoked"]).and_then(Value::as_bool)
            == Some(true)
        || value_at(&proof.descriptor, &["release", "requestedCapabilities"])
            .and_then(Value::as_array)
            .is_none_or(|items| !items.is_empty())
        || !descriptor_supports_desktop(&proof.descriptor)
    {
        return Err(ProofError::DescriptorBinding);
    }
    let bundle_digest =
        string_at(&proof.descriptor, &["release", "bundleDigest"]).unwrap_or_default();
    let mut previous_id: Option<&str> = None;
    for artifact in &release.artifacts {
        if previous_id.is_some_and(|previous| previous >= artifact.artifact_id.as_str())
            || artifact.size_bytes == 0
            || artifact.digest.len() != 64
            || !artifact
                .digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(ProofError::Malformed);
        }
        previous_id = Some(&artifact.artifact_id);
    }
    let selected: Vec<_> = release
        .artifacts
        .iter()
        .filter(|artifact| {
            artifact.digest == bundle_digest
                && artifact.platform == "desktop"
                && artifact.format == "fabapp"
        })
        .collect();
    if selected.len() != 1 {
        return Err(ProofError::Authorization);
    }
    let artifact = (*selected.into_iter().next().expect("one artifact")).clone();
    if string_at(&proof.descriptor, &["publisher", "id"])
        != Some(release.identity.namespace.as_str())
    {
        return Err(ProofError::DescriptorBinding);
    }
    let authorization = CatalogAuthorization {
        app_id: expected_app_id.to_string(),
        publisher_id: release.identity.namespace.clone(),
        version: release.version,
        package_sha256: artifact.digest.clone(),
        key_id: proof.release_signature.key_id.clone(),
        public_key: release_raw,
        trust_epoch: proof.release_signature.trust_epoch,
        host_api: 1,
    };
    Ok(VerifiedInstallProof {
        descriptor: proof.descriptor,
        descriptor_payload: proof.descriptor_payload,
        authorization,
        artifact: SignedArtifact {
            artifact_id: artifact.artifact_id.clone(),
            digest: artifact.digest.clone(),
            size_bytes: artifact.size_bytes,
            platform: artifact.platform.clone(),
            architecture: artifact.architecture.clone(),
            format: artifact.format.clone(),
        },
        proof_snapshot: raw.to_string(),
    })
}

/// Validate the independent Node-signed feed against the pinned Marketplace root.
/// Only callers that already hold a last-good feed may pass `allow_expired_feed`.
pub fn verify_trust_feed(
    raw: &str,
    now_ms: u64,
    allow_expired_feed: bool,
) -> Result<VerifiedTrustFeed, ProofError> {
    verify_trust_feed_with_anchor(
        raw,
        now_ms,
        allow_expired_feed,
        &pinned_marketplace_anchor()?,
    )
}

pub fn verify_trust_feed_with_anchor(
    raw: &str,
    now_ms: u64,
    allow_expired_feed: bool,
    anchor: &TrustAnchor,
) -> Result<VerifiedTrustFeed, ProofError> {
    let envelope: TrustEnvelope = parse_unique(raw)?;
    validate_trust_envelope(&envelope, now_ms, allow_expired_feed, anchor)?;
    Ok(VerifiedTrustFeed {
        epoch: envelope.trust_epoch,
        issued_at_ms: envelope.issued_at_ms,
        signed_payload: envelope.payload.clone(),
        envelope_json: serde_json::to_string(&envelope).map_err(|_| ProofError::Malformed)?,
    })
}

fn validate_trust_envelope<'a>(
    envelope: &'a TrustEnvelope,
    now_ms: u64,
    allow_expired_feed: bool,
    anchor: &TrustAnchor,
) -> Result<TrustPayload, ProofError> {
    if envelope.protocol != TRUST_PROTOCOL || envelope.algorithm != "Ed25519" {
        return Err(ProofError::Malformed);
    }
    let spki = STANDARD
        .decode(&envelope.public_key)
        .map_err(|_| ProofError::Malformed)?;
    if sha256_hex(&spki) != envelope.public_key_fingerprint
        || envelope.key_id != anchor.key_id
        || envelope.trust_epoch != anchor.trust_epoch
        || decode_spki(&envelope.public_key)? != anchor.public_key
    {
        return Err(ProofError::Signature);
    }
    verify_raw(&anchor.public_key, &envelope.payload, &envelope.signature)?;
    let payload: TrustPayload = parse_unique(&envelope.payload)?;
    if payload.protocol != TRUST_PROTOCOL
        || payload.issued_at_ms != envelope.issued_at_ms
        || payload.expires_at_ms != envelope.expires_at_ms
        || payload.signer.algorithm != "Ed25519"
        || payload.signer.key_id != envelope.key_id
        || payload.signer.trust_epoch != envelope.trust_epoch
        || payload.signer.public_key_fingerprint != envelope.public_key_fingerprint
        || envelope.expires_at_ms <= envelope.issued_at_ms
        || envelope.expires_at_ms - envelope.issued_at_ms > MAX_FEED_TTL_MS
        || envelope.issued_at_ms > now_ms.saturating_add(MAX_FUTURE_SKEW_MS)
        || (!allow_expired_feed && now_ms > envelope.expires_at_ms)
    {
        return Err(ProofError::FeedFreshness);
    }
    let mut seen = std::collections::BTreeSet::new();
    for key in &payload.keys {
        if key.algorithm != "Ed25519"
            || !matches!(key.status.as_str(), "active" | "revoked")
            || key.created_at_ms > payload.issued_at_ms
            || (key.status == "revoked") != key.revoked_at_ms.is_some()
            || !seen.insert((key.key_id.as_str(), key.trust_epoch))
        {
            return Err(ProofError::Malformed);
        }
    }
    let signer = payload
        .keys
        .iter()
        .find(|key| key.key_id == envelope.key_id && key.trust_epoch == envelope.trust_epoch)
        .ok_or(ProofError::Signature)?;
    if signer.status != "active"
        || signer.public_key != envelope.public_key
        || signer.public_key_fingerprint != envelope.public_key_fingerprint
    {
        return Err(ProofError::Revoked);
    }
    Ok(payload)
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or(0)
}

pub fn descriptor_app_id(value: &Value) -> Option<&str> {
    // OMA-001's public app identifier is derived from the signed marketplace
    // identity; the public descriptor intentionally has no renderer-set appId.
    let namespace = string_at(value, &["identity", "namespace"])?;
    let name = string_at(value, &["identity", "name"])?;
    if namespace != "bhrumom" {
        return None;
    }
    match (namespace, name) {
        ("bhrumom", "global-dandelion") => Some("org.fabushi.global-dandelion"),
        ("bhrumom", "chatgpt-auto-confirm") => Some("org.fabushi.chatgpt-auto-confirm"),
        _ => None,
    }
}

pub fn descriptor_version(value: &Value) -> Option<&str> {
    string_at(value, &["release", "version"])
}

pub fn descriptor_display_name(value: &Value) -> Option<&str> {
    string_at(value, &["displayName"])
}

pub fn descriptor_publisher(value: &Value) -> Option<&str> {
    string_at(value, &["publisher", "id"])
}

pub fn descriptor_identity(value: &Value) -> Option<(&str, &str, &str)> {
    Some((
        string_at(value, &["identity", "kind"])?,
        string_at(value, &["identity", "namespace"])?,
        string_at(value, &["identity", "name"])?,
    ))
}

pub fn descriptor_supports_desktop(value: &Value) -> bool {
    value_at(value, &["release", "platforms"])
        .and_then(Value::as_array)
        .is_some_and(|platforms| {
            platforms
                .iter()
                .any(|entry| entry.as_str() == Some("desktop"))
        })
        && string_at(value, &["runtime", "installMode"]) == Some("fabapp")
}

/// Replace only the unsigned proof wrapper's trust envelope with the latest
/// trust snapshot accepted by this Host ledger. Signed payload strings remain
/// byte-for-byte unchanged and are reverified by `verify_install_proof`.
pub fn overlay_cached_trust(
    proof_snapshot: &str,
    latest_trust: Option<&str>,
) -> Result<String, ProofError> {
    let Some(latest_trust) = latest_trust else {
        return Ok(proof_snapshot.to_string());
    };
    let UniqueJson(mut proof) =
        serde_json::from_str(proof_snapshot).map_err(|_| ProofError::Malformed)?;
    let UniqueJson(trust) =
        serde_json::from_str(latest_trust).map_err(|_| ProofError::Malformed)?;
    let object = proof.as_object_mut().ok_or(ProofError::Malformed)?;
    object.insert("trust".into(), trust);
    serde_json::to_string(&proof).map_err(|_| ProofError::Malformed)
}

pub fn overlay_trust(
    proof_snapshot: &str,
    trust_envelope_json: &str,
) -> Result<String, ProofError> {
    overlay_cached_trust(proof_snapshot, Some(trust_envelope_json))
}

pub fn proof_descriptor(raw: &str) -> Result<Value, ProofError> {
    let proof: InstallProofEnvelope = parse_unique(raw)?;
    Ok(proof.descriptor)
}

pub fn trust_feed_not_rolled_back(
    newer: &VerifiedTrustFeed,
    previous_raw: Option<&str>,
) -> Result<(), ProofError> {
    let Some(previous_raw) = previous_raw else {
        return Ok(());
    };
    let previous: TrustEnvelope = parse_unique(previous_raw)?;
    if newer.epoch < previous.trust_epoch
        || (newer.epoch == previous.trust_epoch
            && (newer.issued_at_ms < previous.issued_at_ms
                || (newer.issued_at_ms == previous.issued_at_ms
                    && newer.signed_payload != previous.payload)))
    {
        return Err(ProofError::TrustRollback);
    }
    ensure_revocations_retained(&previous.payload, &newer.signed_payload)
}

pub fn ensure_trust_not_rolled_back(
    proof_snapshot: &str,
    latest_trust: Option<&str>,
) -> Result<(), ProofError> {
    let Some(latest_trust) = latest_trust else {
        return Ok(());
    };
    let proof: InstallProofEnvelope = parse_unique(proof_snapshot)?;
    let latest: TrustEnvelope = parse_unique(latest_trust)?;
    if proof.trust.trust_epoch < latest.trust_epoch
        || (proof.trust.trust_epoch == latest.trust_epoch
            && (proof.trust.issued_at_ms < latest.issued_at_ms
                || (proof.trust.issued_at_ms == latest.issued_at_ms
                    && proof.trust.payload != latest.payload)))
    {
        return Err(ProofError::TrustRollback);
    }
    ensure_revocations_retained(&latest.payload, &proof.trust.payload)
}

fn ensure_revocations_retained(
    previous_payload: &str,
    newer_payload: &str,
) -> Result<(), ProofError> {
    let previous_payload: TrustPayload = parse_unique(previous_payload)?;
    let newer_payload: TrustPayload = parse_unique(newer_payload)?;
    for old_key in previous_payload
        .keys
        .iter()
        .filter(|key| key.status == "revoked")
    {
        let retained = newer_payload.keys.iter().any(|key| {
            key.key_id == old_key.key_id
                && key.trust_epoch == old_key.trust_epoch
                && key.status == "revoked"
                && key.public_key == old_key.public_key
                && key.public_key_fingerprint == old_key.public_key_fingerprint
        });
        if !retained {
            return Err(ProofError::TrustRollback);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_catalog_origin_is_independent_of_native_auth_issuer() {
        let client = CatalogClient::production_from_environment().unwrap();
        assert_eq!(
            client.origin.origin().ascii_serialization(),
            "https://api.ombhrum.com"
        );
    }
    use ed25519_dalek::{Signer, SigningKey};

    fn signed_fixture(expired: bool, revoked_release_key: bool) -> (String, TrustAnchor) {
        let signing = SigningKey::from_bytes(&[23; 32]);
        let raw = signing.verifying_key().to_bytes();
        let mut der = vec![
            0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
        ];
        der.extend_from_slice(&raw);
        let spki = STANDARD.encode(&der);
        let fingerprint = sha256_hex(&der);
        let anchor = TrustAnchor {
            key_id: "test-root".into(),
            trust_epoch: 1,
            public_key: raw,
        };
        let now = 10_000_u64;
        let issued = if expired { 100 } else { now };
        let expires = if expired { 200 } else { now + 60_000 };

        let descriptor = serde_json::json!({
            "identity": { "kind": "web-miniapp", "namespace": "bhrumom", "name": "global-dandelion" },
            "publisher": { "id": "bhrumom" },
            "displayName": "Global Dandelion",
            "release": { "version": "0.1.0", "channel": "preview", "platforms": ["desktop"],
                "requestedCapabilities": [], "bundleDigest": "de10e02115415908f827723457817e51cb4f39aa5ca68798da824fecc483118a" },
            "runtime": { "installMode": "fabapp" }
        });
        let descriptor_payload = serde_json::to_string(&descriptor).unwrap();
        let descriptor_digest = sha256_hex(descriptor_payload.as_bytes());
        let release_payload = serde_json::json!({
            "protocol": RELEASE_PROTOCOL, "keyId": "release-key", "trustEpoch": 1,
            "identity": { "namespace": "bhrumom", "name": "global-dandelion", "kind": "web-miniapp" },
            "version": "0.1.0", "descriptorDigest": descriptor_digest,
            "artifacts": [{ "artifactId": "desktop-fabapp", "digest": "de10e02115415908f827723457817e51cb4f39aa5ca68798da824fecc483118a",
                "sizeBytes": 512, "platform": "desktop", "architecture": "universal", "format": "fabapp" }]
        }).to_string();
        let release_signature =
            STANDARD.encode(signing.sign(release_payload.as_bytes()).to_bytes());
        let trust_payload = serde_json::json!({
            "protocol": TRUST_PROTOCOL, "issuedAtMs": issued, "expiresAtMs": expires,
            "signer": { "algorithm": "Ed25519", "keyId": "test-root", "trustEpoch": 1, "publicKeyFingerprint": fingerprint },
            "keys": [
                { "algorithm": "Ed25519", "keyId": "test-root", "trustEpoch": 1, "publicKey": spki, "publicKeyFingerprint": fingerprint,
                    "status": "active", "createdAtMs": 1, "revokedAtMs": null, "revocationReason": "" },
                { "algorithm": "Ed25519", "keyId": "release-key", "trustEpoch": 1, "publicKey": spki, "publicKeyFingerprint": fingerprint,
                    "status": if revoked_release_key { "revoked" } else { "active" }, "createdAtMs": 1,
                    "revokedAtMs": if revoked_release_key { Some(2_u64) } else { None }, "revocationReason": "test" }
            ]
        }).to_string();
        let trust_signature = STANDARD.encode(signing.sign(trust_payload.as_bytes()).to_bytes());
        let proof = serde_json::json!({
            "protocol": PROOF_PROTOCOL, "descriptor": descriptor,
            "descriptorPayload": descriptor_payload,
            "releaseSignature": { "protocol": RELEASE_PROTOCOL, "algorithm": "Ed25519", "keyId": "release-key", "trustEpoch": 1,
                "publicKey": spki, "publicKeyFingerprint": fingerprint, "payload": release_payload,
                "signature": release_signature, "signedAtMs": 3 },
            "trust": { "protocol": TRUST_PROTOCOL, "algorithm": "Ed25519", "keyId": "test-root", "trustEpoch": 1,
                "publicKey": spki, "publicKeyFingerprint": fingerprint, "issuedAtMs": issued,
                "expiresAtMs": expires, "payload": trust_payload, "signature": trust_signature }
        }).to_string();
        (proof, anchor)
    }

    #[test]
    fn verifies_preview_channel_stable_semver_and_exact_signed_payloads() {
        let (raw, anchor) = signed_fixture(false, false);
        let proof = verify_install_proof_with_anchor(
            &raw,
            "org.fabushi.global-dandelion",
            10_000,
            false,
            &anchor,
        )
        .unwrap();
        assert_eq!(proof.authorization.version, "0.1.0");
        assert_eq!(
            proof.artifact.digest,
            "de10e02115415908f827723457817e51cb4f39aa5ca68798da824fecc483118a"
        );
    }

    #[test]
    fn independent_signed_feed_is_verified_and_rejects_tamper_and_rollback() {
        let (proof_raw, anchor) = signed_fixture(false, false);
        let proof: InstallProofEnvelope = parse_unique(&proof_raw).unwrap();
        let feed_raw = serde_json::to_string(&proof.trust).unwrap();
        let verified = verify_trust_feed_with_anchor(&feed_raw, 10_000, false, &anchor).unwrap();
        assert_eq!(verified.epoch, 1);
        assert!(
            verify_trust_feed_with_anchor(
                &feed_raw.replace("active", "revoked"),
                10_000,
                false,
                &anchor
            )
            .is_err()
        );
        let older = VerifiedTrustFeed {
            epoch: 0,
            issued_at_ms: verified.issued_at_ms,
            signed_payload: verified.signed_payload.clone(),
            envelope_json: feed_raw.clone(),
        };
        assert_eq!(
            trust_feed_not_rolled_back(&older, Some(&feed_raw)),
            Err(ProofError::TrustRollback)
        );
    }

    #[test]
    fn install_trust_route_requires_exact_wrapper_shape() {
        let (proof_raw, _) = signed_fixture(false, false);
        let proof: InstallProofEnvelope = parse_unique(&proof_raw).unwrap();
        let trust_raw = serde_json::to_string(&proof.trust).unwrap();
        let wrapped = serde_json::json!({"trust": proof.trust});
        assert_eq!(
            serde_json::from_str::<Value>(
                &extract_install_trust_response(&wrapped.to_string()).unwrap(),
            )
            .unwrap(),
            serde_json::from_str::<Value>(&trust_raw).unwrap()
        );
        assert_eq!(
            extract_install_trust_response(&format!("{{\"trust\":{trust_raw},\"extra\":true}}")),
            Err(ProofError::Malformed)
        );
        assert_eq!(
            extract_install_trust_response(&format!(
                "{{\"trust\":{trust_raw},\"trust\":{trust_raw}}}"
            )),
            Err(ProofError::Malformed)
        );
    }

    #[test]
    fn production_node_feed_matches_native_pinned_ed25519_contract() {
        let Ok(path) = std::env::var("FABUSHI_PRODUCTION_TRUST_FIXTURE") else {
            return;
        };
        let response = std::fs::read_to_string(path).unwrap();
        let envelope = extract_install_trust_response(&response).unwrap();
        let feed = verify_trust_feed(&envelope, now_ms(), false).unwrap();
        assert_eq!(feed.epoch, 1);
    }

    #[test]
    fn live_approved_catalog_cas_install_reload_and_remove() {
        if std::env::var("FABUSHI_APPROVED_PUBLIC_ACCEPTANCE").as_deref() != Ok("1") {
            return; // Not live acceptance unless the dedicated Actions gate opts in.
        }
        use super::super::{ledger, verify_catalog_bundle};
        let client = CatalogClient::production_from_environment().unwrap();
        let app_id = "org.fabushi.global-dandelion";
        let descriptor = client
            .list_descriptors()
            .unwrap()
            .into_iter()
            .find(|value| descriptor_app_id(value) == Some(app_id))
            .expect("approved Dandelion must be in the public catalog");
        let feed_raw = client.fetch_install_trust().unwrap();
        let feed = verify_trust_feed(&feed_raw, now_ms(), false).unwrap();
        let proof_raw = client.fetch_install_proof(&descriptor).unwrap();
        let proof_raw = overlay_trust(&proof_raw, &feed_raw).unwrap();
        let proof = verify_install_proof(&proof_raw, app_id, now_ms(), false).unwrap();
        assert_eq!(proof.authorization.version, "0.1.0");
        assert_eq!(proof.artifact.size_bytes, 453093);
        assert_eq!(
            proof.artifact.digest,
            "de10e02115415908f827723457817e51cb4f39aa5ca68798da824fecc483118a"
        );
        let bytes = client
            .download_artifact(&proof.artifact.digest, proof.artifact.size_bytes)
            .unwrap();
        let package = verify_catalog_bundle(&bytes, &proof.authorization).unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        ledger::initialize(&db).unwrap();
        ledger::cache_verified_trust(
            &mut db,
            feed.epoch,
            feed.issued_at_ms,
            &feed.signed_payload,
            &feed.envelope_json,
        )
        .unwrap();
        let scope = ledger::HostInstallScope {
            account_scope: "isolated-public-interoperability-fixture".into(),
            generation: 1,
        };
        ledger::activate_authorized(
            &mut db,
            &scope,
            &scope,
            "public-install-1",
            package,
            &proof_raw,
            now_ms(),
        )
        .unwrap();
        let (cached, saved_proof, trust) =
            ledger::load_active_authorized_archive(&db, &scope, app_id)
                .unwrap()
                .unwrap();
        let cached_proof = overlay_cached_trust(&saved_proof, trust.as_deref()).unwrap();
        let verified = verify_install_proof(&cached_proof, app_id, now_ms(), true).unwrap();
        let loaded = verify_catalog_bundle(&cached, &verified.authorization).unwrap();
        assert!(
            loaded
                .resource("pkg/dandelion_web_adapter_bg.wasm")
                .is_some()
        );
        assert!(ledger::remove(&mut db, &scope, &scope, app_id, "public-remove-1").unwrap());
        assert!(
            ledger::load_active_archive(&db, &scope, app_id)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn desktop_package_mode_requires_the_canonical_fabapp_spelling() {
        for mode in ["package", "hosted", "", "FABAPP"] {
            let descriptor = serde_json::json!({
                "release": {"platforms": ["desktop"]},
                "runtime": {"installMode": mode}
            });
            assert!(!descriptor_supports_desktop(&descriptor));
        }
        assert!(descriptor_supports_desktop(&serde_json::json!({
            "release": {"platforms": ["desktop"]},
            "runtime": {"installMode": "fabapp"}
        })));
    }

    #[test]
    fn newer_timestamp_cannot_reactivate_a_cached_revoked_key() {
        let (proof_raw, _) = signed_fixture(false, true);
        let proof: InstallProofEnvelope = parse_unique(&proof_raw).unwrap();
        let previous_raw = serde_json::to_string(&proof.trust).unwrap();
        let mut payload: Value = serde_json::from_str(&proof.trust.payload).unwrap();
        for key in payload["keys"].as_array_mut().unwrap() {
            key["status"] = Value::String("active".into());
            key["revokedAtMs"] = Value::Null;
        }
        let newer = VerifiedTrustFeed {
            epoch: proof.trust.trust_epoch,
            issued_at_ms: proof.trust.issued_at_ms + 1,
            signed_payload: payload.to_string(),
            envelope_json: String::new(),
        };
        assert_eq!(
            trust_feed_not_rolled_back(&newer, Some(&previous_raw)),
            Err(ProofError::TrustRollback)
        );
    }

    #[test]
    fn transport_fallback_does_not_convert_rejected_feed_into_last_good() {
        assert!(is_transport_failure(&ProofError::Network));
        assert!(!is_transport_failure(&ProofError::Rejected));
        assert!(!is_transport_failure(&ProofError::Signature));
        assert!(!is_transport_failure(&ProofError::FeedFreshness));
    }

    #[test]
    fn expired_feed_is_offline_only_and_revoked_key_never_launches() {
        let (expired, anchor) = signed_fixture(true, false);
        assert_eq!(
            verify_install_proof_with_anchor(
                &expired,
                "org.fabushi.global-dandelion",
                10_000,
                false,
                &anchor
            )
            .err()
            .unwrap(),
            ProofError::FeedFreshness
        );
        assert!(
            verify_install_proof_with_anchor(
                &expired,
                "org.fabushi.global-dandelion",
                10_000,
                true,
                &anchor
            )
            .is_ok()
        );
        let (revoked, anchor) = signed_fixture(true, true);
        assert_eq!(
            verify_install_proof_with_anchor(
                &revoked,
                "org.fabushi.global-dandelion",
                10_000,
                true,
                &anchor
            )
            .err()
            .unwrap(),
            ProofError::Revoked
        );
    }

    #[test]
    fn descriptor_payload_byte_tampering_is_rejected() {
        let (raw, anchor) = signed_fixture(false, false);
        let mut proof: Value = serde_json::from_str(&raw).unwrap();
        proof["descriptorPayload"] = Value::String("{}".into());
        assert_eq!(
            verify_install_proof_with_anchor(
                &proof.to_string(),
                "org.fabushi.global-dandelion",
                10_000,
                false,
                &anchor
            )
            .err()
            .unwrap(),
            ProofError::DescriptorBinding
        );
    }
}
