use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

type HmacSha256 = Hmac<Sha256>;

static SALT_COUNTER: AtomicU64 = AtomicU64::new(1);

fn default_true() -> bool {
    true
}

/// Action taken when a sensitive span is detected by `MaskingEngine`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaskingAction {
    #[default]
    Mask,
    Warn,
    Block,
}

/// User configuration for Magpie privacy/secret masking before outbound requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaskingConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub mask_secrets: bool,
    #[serde(default = "default_true")]
    pub mask_personal: bool,
    #[serde(default)]
    pub custom_words: Vec<String>,
    #[serde(default)]
    pub allow_list: Vec<String>,
    #[serde(default)]
    pub default_action: MaskingAction,
    #[serde(default)]
    pub kind_actions: BTreeMap<String, MaskingAction>,
}

impl Default for MaskingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mask_secrets: true,
            mask_personal: true,
            custom_words: Vec::new(),
            allow_list: Vec::new(),
            default_action: MaskingAction::Mask,
            kind_actions: BTreeMap::new(),
        }
    }
}

/// Safe trace entry describing a single match without leaking the raw secret value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaskingHit {
    pub kind: String,
    pub placeholder: String,
    pub action: MaskingAction,
    pub preview_hint: String,
}

/// Outcome of scanning and masking an outbound text string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaskResult {
    pub masked_text: String,
    pub hits: Vec<MaskingHit>,
    pub blocked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_reason: Option<String>,
}

#[derive(Default)]
struct MemoryVault {
    secret_to_placeholder: HashMap<(String, String), String>,
    placeholder_to_secret: HashMap<String, String>,
}

/// Runtime-salted in-memory secret/PII masker and restorer.
/// Salts and secret mappings stay strictly in RAM and are never serialized to disk.
pub struct MaskingEngine {
    config: MaskingConfig,
    salt: [u8; 32],
    vault: Mutex<MemoryVault>,
}

impl MaskingEngine {
    pub fn new(config: MaskingConfig) -> Self {
        let now = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64;
        let pid = std::process::id() as u64;
        let seq = SALT_COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut salt = [0u8; 32];
        let mut seed_inputs = Vec::with_capacity(24);
        seed_inputs.extend_from_slice(&now.to_le_bytes());
        seed_inputs.extend_from_slice(&pid.to_le_bytes());
        seed_inputs.extend_from_slice(&seq.to_le_bytes());
        if let Ok(mut mac) = <HmacSha256 as Mac>::new_from_slice(b"magpie-runtime-masking-salt-v1") {
            mac.update(&seed_inputs);
            salt.copy_from_slice(&mac.finalize().into_bytes());
        }
        Self {
            config,
            salt,
            vault: Mutex::new(MemoryVault::default()),
        }
    }

    pub fn config(&self) -> &MaskingConfig {
        &self.config
    }

    /// Mask outbound text if `config.enabled` is true.
    pub fn mask_text(&self, text: &str) -> MaskResult {
        if !self.config.enabled {
            return MaskResult {
                masked_text: text.to_string(),
                hits: Vec::new(),
                blocked: false,
                block_reason: None,
            };
        }
        self.apply_rules(text, &self.config, true)
    }

    /// Preview masking behavior (even when `config.enabled` is false) without retaining secret mappings permanently.
    pub fn preview_text(&self, text: &str) -> MaskResult {
        let mut effective = self.config.clone();
        if !effective.enabled {
            effective.enabled = true;
            effective.mask_secrets = true;
            effective.mask_personal = true;
        }
        self.apply_rules(text, &effective, false)
    }

    /// Restore any placeholders previously issued by this engine instance back to their raw values.
    pub fn unmask_text(&self, text: &str) -> String {
        if !text.contains("{{") {
            return text.to_string();
        }
        let guard = self.vault.lock().unwrap_or_else(|e| e.into_inner());
        if guard.placeholder_to_secret.is_empty() {
            return text.to_string();
        }
        let mut out = String::with_capacity(text.len());
        let mut cursor = 0usize;
        while let Some(rel_start) = text[cursor..].find("{{") {
            let abs_start = cursor + rel_start;
            out.push_str(&text[cursor..abs_start]);
            if let Some(rel_end) = text[abs_start..].find("}}") {
                let abs_end = abs_start + rel_end + 2;
                let candidate = &text[abs_start..abs_end];
                if let Some(raw) = guard.placeholder_to_secret.get(candidate) {
                    out.push_str(raw);
                } else {
                    out.push_str(candidate);
                }
                cursor = abs_end;
            } else {
                out.push_str(&text[abs_start..]);
                cursor = text.len();
                break;
            }
        }
        if cursor < text.len() {
            out.push_str(&text[cursor..]);
        }
        out
    }

    fn apply_rules(&self, text: &str, cfg: &MaskingConfig, persist_in_memory: bool) -> MaskResult {
        if text.is_empty() {
            return MaskResult {
                masked_text: String::new(),
                hits: Vec::new(),
                blocked: false,
                block_reason: None,
            };
        }

        let mut spans = Vec::<DetectedSpan>::new();
        if cfg.mask_secrets {
            detect_pem_private_keys(text, &mut spans);
            detect_jwts(text, &mut spans);
            detect_bearer_tokens(text, &mut spans);
            detect_api_keys(text, &mut spans);
        }
        if cfg.mask_personal {
            detect_emails(text, &mut spans);
            detect_luhn_cards(text, &mut spans);
        }
        detect_custom_words(text, &cfg.custom_words, &mut spans);

        // Sort by earliest start, then longest span, and drop overlapping spans & allowlisted matches.
        spans.sort_by(|a, b| {
            a.start
                .cmp(&b.start)
                .then_with(|| (b.end - b.start).cmp(&(a.end - a.start)))
        });

        let mut accepted = Vec::<DetectedSpan>::new();
        let mut last_end = 0usize;
        for sp in spans {
            if sp.start < last_end || sp.end <= sp.start {
                continue;
            }
            if !text.is_char_boundary(sp.start) || !text.is_char_boundary(sp.end) {
                continue;
            }
            let raw = &text[sp.start..sp.end];
            if is_existing_placeholder(raw) || is_allow_listed(raw, &cfg.allow_list) {
                continue;
            }
            last_end = sp.end;
            accepted.push(sp);
        }

        if accepted.is_empty() {
            return MaskResult {
                masked_text: text.to_string(),
                hits: Vec::new(),
                blocked: false,
                block_reason: None,
            };
        }

        let mut out = String::with_capacity(text.len());
        let mut hits = Vec::with_capacity(accepted.len());
        let mut blocked = false;
        let mut block_reason = None;
        let mut cursor = 0usize;

        for sp in accepted {
            out.push_str(&text[cursor..sp.start]);
            let raw = &text[sp.start..sp.end];
            let action = cfg
                .kind_actions
                .get(sp.kind)
                .copied()
                .unwrap_or(cfg.default_action);
            let placeholder = self.issue_placeholder(sp.kind, raw, persist_in_memory);

            match action {
                MaskingAction::Mask => out.push_str(&placeholder),
                MaskingAction::Warn => out.push_str(raw),
                MaskingAction::Block => {
                    blocked = true;
                    if block_reason.is_none() {
                        block_reason = Some(format!(
                            "Outbound text contains blocked sensitive entity ({})",
                            sp.kind
                        ));
                    }
                    out.push_str(&placeholder);
                }
            }

            hits.push(MaskingHit {
                kind: sp.kind.to_string(),
                placeholder,
                action,
                preview_hint: safe_preview_hint(raw),
            });
            cursor = sp.end;
        }
        out.push_str(&text[cursor..]);

        MaskResult {
            masked_text: out,
            hits,
            blocked,
            block_reason,
        }
    }

    fn issue_placeholder(&self, kind: &str, raw: &str, persist_in_memory: bool) -> String {
        if persist_in_memory {
            let guard = self.vault.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(existing) = guard
                .secret_to_placeholder
                .get(&(kind.to_string(), raw.to_string()))
            {
                return existing.clone();
            }
        }

        let mut mac = <HmacSha256 as Mac>::new_from_slice(&self.salt)
            .expect("HMAC-SHA256 accepts any key length");
        mac.update(kind.as_bytes());
        mac.update(&[0u8]);
        mac.update(raw.as_bytes());
        let digest = mac.finalize().into_bytes();
        let tag = encode_tag8(&digest[..8]);
        let prefix = kind.to_ascii_uppercase();
        let placeholder = format!("{{{{{prefix}_{tag}}}}}");

        if persist_in_memory {
            let mut guard = self.vault.lock().unwrap_or_else(|e| e.into_inner());
            guard
                .secret_to_placeholder
                .insert((kind.to_string(), raw.to_string()), placeholder.clone());
            guard
                .placeholder_to_secret
                .insert(placeholder.clone(), raw.to_string());
        }
        placeholder
    }
}

/// Convenience helper for masking text with a shared `MaskingEngine`.
pub fn mask_text(engine: &MaskingEngine, text: &str) -> MaskResult {
    engine.mask_text(text)
}

/// Convenience helper for unmasking text with a shared `MaskingEngine`.
pub fn unmask_text(engine: &MaskingEngine, text: &str) -> String {
    engine.unmask_text(text)
}

/// Incremental SSE/delta unmasker that holds incomplete `{{...` placeholder prefixes across chunk boundaries.
#[derive(Debug, Default, Clone)]
pub struct StreamingUnmasker {
    pending: String,
}

const MAX_PLACEHOLDER_HOLD_BYTES: usize = 64;

impl StreamingUnmasker {
    pub fn new() -> Self {
        Self {
            pending: String::new(),
        }
    }

    /// Push a streaming delta chunk and emit all text that cannot be part of a split `{{...}}` placeholder.
    pub fn push(&mut self, chunk: &str, engine: &MaskingEngine) -> String {
        if chunk.is_empty() {
            return String::new();
        }
        self.pending.push_str(chunk);

        let hold_start = find_trailing_placeholder_hold(&self.pending);
        let ready = self.pending[..hold_start].to_string();
        let tail = self.pending[hold_start..].to_string();
        self.pending = tail;

        if ready.is_empty() {
            String::new()
        } else {
            engine.unmask_text(&ready)
        }
    }

    /// Flush any remaining buffered bytes at the end of a stream.
    pub fn finish(&mut self, engine: &MaskingEngine) -> String {
        if self.pending.is_empty() {
            return String::new();
        }
        let tail = std::mem::take(&mut self.pending);
        engine.unmask_text(&tail)
    }
}

fn find_trailing_placeholder_hold(buf: &str) -> usize {
    if buf.ends_with('{') && !buf.ends_with("{{") {
        // Could be the first `{` of a `{{PLACEHOLDER}}` split across delta chunks.
        return buf.len() - 1;
    }
    if let Some(open_pos) = buf.rfind("{{") {
        let suffix = &buf[open_pos..];
        if !suffix.contains("}}") && suffix.len() <= MAX_PLACEHOLDER_HOLD_BYTES {
            let body = suffix[2..].strip_suffix('}').unwrap_or(&suffix[2..]);
            let valid_chars = body
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
            if valid_chars {
                return open_pos;
            }
        }
    }
    buf.len()
}

struct DetectedSpan {
    kind: &'static str,
    start: usize,
    end: usize,
}

fn encode_tag8(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut out = String::with_capacity(8);
    for &b in bytes.iter().take(8) {
        out.push(ALPHABET[(b as usize) % ALPHABET.len()] as char);
    }
    out
}

fn safe_preview_hint(raw: &str) -> String {
    let char_count = raw.chars().count();
    let head: String = raw.chars().take(2).collect();
    format!("{head}***({char_count}c)")
}

fn is_existing_placeholder(s: &str) -> bool {
    s.starts_with("{{") && s.ends_with("}}")
}

fn is_allow_listed(raw: &str, allow_list: &[String]) -> bool {
    let trimmed = raw.trim();
    allow_list
        .iter()
        .any(|allowed| allowed.trim().eq_ignore_ascii_case(trimmed))
}

fn is_token_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')
}

fn is_bearer_token_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'~' | b'+' | b'/' | b'=')
}

fn has_token_boundary_before(bytes: &[u8], idx: usize) -> bool {
    idx == 0 || !is_token_char(bytes[idx - 1])
}

fn detect_pem_private_keys(text: &str, out: &mut Vec<DetectedSpan>) {
    let mut cursor = 0usize;
    while let Some(rel) = text[cursor..].find("-----BEGIN ") {
        let start = cursor + rel;
        let rest = &text[start..];
        let Some(hdr_end) = rest.find("-----") else {
            break;
        };
        let after_begin = &rest[5..];
        let Some(second_dash) = after_begin.find("-----") else {
            break;
        };
        let label = &after_begin[..second_dash];
        if !label.contains("PRIVATE KEY") {
            cursor = start + hdr_end + 5;
            continue;
        }
        let end_marker = format!("-----END {}-----", label.strip_prefix("BEGIN ").unwrap_or(label));
        if let Some(end_rel) = rest.find(&end_marker) {
            let end = start + end_rel + end_marker.len();
            out.push(DetectedSpan {
                kind: "pem_private_key",
                start,
                end,
            });
            cursor = end;
        } else {
            cursor = start + 11;
        }
    }
}

fn detect_jwts(text: &str, out: &mut Vec<DetectedSpan>) {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i + 3 < bytes.len() {
        if &bytes[i..i + 3] == b"eyJ" && (i == 0 || !is_token_char(bytes[i - 1])) {
            let mut cursor = i + 3;
            let mut dots = 0usize;
            let mut part_len = 3usize;
            let mut valid = true;
            while cursor < bytes.len() {
                let b = bytes[cursor];
                if b == b'.' {
                    if part_len < 4 {
                        valid = false;
                        break;
                    }
                    dots += 1;
                    part_len = 0;
                    if dots > 2 {
                        break;
                    }
                    cursor += 1;
                } else if is_token_char(b) {
                    part_len += 1;
                    cursor += 1;
                } else {
                    break;
                }
            }
            if valid && dots == 2 && part_len >= 4 {
                out.push(DetectedSpan {
                    kind: "jwt",
                    start: i,
                    end: cursor,
                });
                i = cursor;
                continue;
            }
        }
        i += 1;
    }
}

fn detect_bearer_tokens(text: &str, out: &mut Vec<DetectedSpan>) {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i + 7 < bytes.len() {
        if bytes[i..i + 6].eq_ignore_ascii_case(b"bearer")
            && (i == 0 || !is_token_char(bytes[i - 1]))
            && (bytes[i + 6] == b' ' || bytes[i + 6] == b'\t')
        {
            let mut tok_start = i + 6;
            while tok_start < bytes.len() && (bytes[tok_start] == b' ' || bytes[tok_start] == b'\t')
            {
                tok_start += 1;
            }
            if tok_start + 1 < bytes.len() && &bytes[tok_start..tok_start + 2] == b"{{" {
                i = tok_start + 2;
                continue;
            }
            let mut tok_end = tok_start;
            while tok_end < bytes.len() && is_bearer_token_char(bytes[tok_end]) {
                tok_end += 1;
            }
            if tok_end - tok_start >= 16 {
                out.push(DetectedSpan {
                    kind: "bearer_token",
                    start: tok_start,
                    end: tok_end,
                });
                i = tok_end;
                continue;
            }
        }
        i += 1;
    }
}

fn detect_api_keys(text: &str, out: &mut Vec<DetectedSpan>) {
    const PREFIX_MIN_LEN: &[(&str, usize)] = &[
        ("github_pat_", 30),
        ("sk-svcacct-", 24),
        ("sk-ant-", 20),
        ("sk-proj-", 20),
        ("sk-", 16),
        ("GOCSPX-", 20),
        ("ya29.", 20),
        ("1//", 20),
        ("AIza", 30),
        ("ghp_", 24),
        ("gho_", 24),
        ("ghu_", 24),
        ("ghs_", 24),
        ("ghr_", 24),
        ("xoxb-", 20),
        ("xoxp-", 20),
        ("AKIA", 20),
        ("hf_", 20),
    ];

    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if !has_token_boundary_before(bytes, i) {
            i += 1;
            continue;
        }
        let mut matched_end = None;
        for &(prefix, min_len) in PREFIX_MIN_LEN {
            let pb = prefix.as_bytes();
            if i + pb.len() <= bytes.len() && &bytes[i..i + pb.len()] == pb {
                let mut end = i + pb.len();
                while end < bytes.len() {
                    if is_token_char(bytes[end]) {
                        end += 1;
                    } else if (prefix == "ya29." || prefix == "1//")
                        && (bytes[end] == b'.' || bytes[end] == b'/')
                        && end + 1 < bytes.len()
                        && is_token_char(bytes[end + 1])
                    {
                        end += 1;
                    } else {
                        break;
                    }
                }
                if end - i >= min_len {
                    if prefix == "AKIA" && (end - i != 20 || !bytes[i..end].iter().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())) {
                        break;
                    }
                    matched_end = Some(end);
                    break;
                }
            }
        }
        if let Some(end) = matched_end {
            out.push(DetectedSpan {
                kind: "api_key",
                start: i,
                end,
            });
            i = end;
        } else {
            i += 1;
        }
    }
}

fn is_email_local_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'%' | b'+' | b'-')
}

fn is_email_domain_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-')
}

fn is_ignored_example_domain(domain: &str) -> bool {
    let lower = domain.to_ascii_lowercase();
    lower == "example.com"
        || lower == "example.org"
        || lower.ends_with(".example.com")
        || lower.ends_with(".example.org")
        || lower.ends_with(".test")
        || lower.ends_with(".invalid")
}

fn detect_emails(text: &str, out: &mut Vec<DetectedSpan>) {
    let bytes = text.as_bytes();
    for (at_idx, &b) in bytes.iter().enumerate() {
        if b != b'@' {
            continue;
        }
        let mut local_start = at_idx;
        while local_start > 0 && is_email_local_char(bytes[local_start - 1]) {
            local_start -= 1;
        }
        let mut domain_end = at_idx + 1;
        while domain_end < bytes.len() && is_email_domain_char(bytes[domain_end]) {
            domain_end += 1;
        }
        let domain = &text[at_idx + 1..domain_end];
        let local = &text[local_start..at_idx];
        if local.is_empty()
            || local.starts_with('.')
            || local.ends_with('.')
            || !domain.contains('.')
            || domain.starts_with('.')
            || domain.ends_with('.')
            || domain.split('.').last().is_none_or(|tld| tld.len() < 2 || !tld.bytes().all(|c| c.is_ascii_alphabetic()))
        {
            continue;
        }
        if is_ignored_example_domain(domain) {
            continue;
        }
        out.push(DetectedSpan {
            kind: "email",
            start: local_start,
            end: domain_end,
        });
    }
}

fn detect_luhn_cards(text: &str, out: &mut Vec<DetectedSpan>) {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        if i > 0 && (bytes[i - 1].is_ascii_digit() || bytes[i - 1] == b'.') {
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            continue;
        }
        let start = i;
        let mut digits = Vec::<u8>::new();
        let mut cursor = i;
        let mut last_sep = false;
        while cursor < bytes.len() {
            let b = bytes[cursor];
            if b.is_ascii_digit() {
                digits.push(b - b'0');
                last_sep = false;
                cursor += 1;
            } else if (b == b'-' || b == b' ')
                && !last_sep
                && cursor + 1 < bytes.len()
                && bytes[cursor + 1].is_ascii_digit()
                && digits.len() < 19
            {
                last_sep = true;
                cursor += 1;
            } else {
                break;
            }
        }
        let end = cursor;
        if (end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'.'))
            || !(13..=19).contains(&digits.len())
        {
            i = end.max(i + 1);
            continue;
        }
        let all_equal = digits.windows(2).all(|w| w[0] == w[1]);
        if !all_equal && passes_luhn(&digits) {
            out.push(DetectedSpan {
                kind: "card_number",
                start,
                end,
            });
        }
        i = end.max(i + 1);
    }
}

fn passes_luhn(digits: &[u8]) -> bool {
    let mut sum = 0u32;
    let mut double = false;
    for &d in digits.iter().rev() {
        let mut v = d as u32;
        if double {
            v *= 2;
            if v > 9 {
                v -= 9;
            }
        }
        sum += v;
        double = !double;
    }
    sum % 10 == 0
}

fn detect_custom_words(text: &str, custom_words: &[String], out: &mut Vec<DetectedSpan>) {
    let mut sorted: Vec<&str> = custom_words
        .iter()
        .map(String::as_str)
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .collect();
    sorted.sort_by(|a, b| b.len().cmp(&a.len()));
    for word in sorted {
        let mut start_from = 0usize;
        while let Some(rel) = text[start_from..].find(word) {
            let start = start_from + rel;
            let end = start + word.len();
            out.push(DetectedSpan {
                kind: "custom_word",
                start,
                end,
            });
            start_from = end;
        }
    }
}
