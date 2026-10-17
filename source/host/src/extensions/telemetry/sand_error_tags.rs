use std::collections::BTreeMap;

pub const UNREGISTERED_CODE: &str = "SAND-E0001";

#[derive(Debug, Clone, PartialEq)]
pub enum SandErrorPayloadValue {
    String(String),
    Number(f64),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SandErrorValue {
    pub code: String,
    pub payload: BTreeMap<String, SandErrorPayloadValue>,
}

impl SandErrorValue {
    pub fn new(code: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            payload: BTreeMap::new(),
        }
    }

    pub fn with_string(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.payload
            .insert(key.into(), SandErrorPayloadValue::String(value.into()));
        self
    }

    pub fn with_number(mut self, key: impl Into<String>, value: f64) -> Self {
        self.payload
            .insert(key.into(), SandErrorPayloadValue::Number(value));
        self
    }

    pub fn with_bool(mut self, key: impl Into<String>, value: bool) -> Self {
        self.payload
            .insert(key.into(), SandErrorPayloadValue::Bool(value));
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SandErrorDefinition {
    pub domain: &'static str,
    pub retryable: bool,
    pub payload: &'static [&'static str],
}

pub fn sand_error_definition(code: &str) -> Option<SandErrorDefinition> {
    match code {
        "SAND-E0001" => Some(SandErrorDefinition { domain: "registry", retryable: false, payload: &[] }),
        "SAND-E0101" => Some(SandErrorDefinition { domain: "transport", retryable: true, payload: &["errno"] }),
        "SAND-E0102" => Some(SandErrorDefinition { domain: "transport", retryable: true, payload: &["errno"] }),
        "SAND-E0103" => Some(SandErrorDefinition { domain: "transport", retryable: true, payload: &["httpStatus"] }),
        "SAND-E0104" => Some(SandErrorDefinition { domain: "transport", retryable: true, payload: &["errno"] }),
        "SAND-E0105" => Some(SandErrorDefinition { domain: "transport", retryable: true, payload: &["errno"] }),
        "SAND-E0106" => Some(SandErrorDefinition { domain: "transport", retryable: true, payload: &["httpStatus"] }),
        "SAND-E0107" => Some(SandErrorDefinition { domain: "transport", retryable: true, payload: &["errno"] }),
        "SAND-E0108" => Some(SandErrorDefinition { domain: "transport", retryable: true, payload: &[] }),
        "SAND-E0109" => Some(SandErrorDefinition { domain: "transport", retryable: true, payload: &["batchEntries"] }),
        "SAND-E0110" => Some(SandErrorDefinition { domain: "transport", retryable: true, payload: &[] }),
        "SAND-E0111" => Some(SandErrorDefinition { domain: "transport", retryable: true, payload: &[] }),
        "SAND-E0112" => Some(SandErrorDefinition { domain: "transport", retryable: true, payload: &[] }),
        "SAND-E0113" => Some(SandErrorDefinition { domain: "transport", retryable: true, payload: &[] }),
        "SAND-E0201" => Some(SandErrorDefinition { domain: "auth", retryable: false, payload: &[] }),
        "SAND-E0202" => Some(SandErrorDefinition { domain: "auth", retryable: false, payload: &["httpStatus"] }),
        "SAND-E0203" => Some(SandErrorDefinition { domain: "auth", retryable: false, payload: &["reason"] }),
        "SAND-E0204" => Some(SandErrorDefinition { domain: "auth", retryable: true, payload: &["reason"] }),
        "SAND-E0205" => Some(SandErrorDefinition { domain: "auth", retryable: true, payload: &["reason"] }),
        "SAND-E0206" => Some(SandErrorDefinition { domain: "auth", retryable: true, payload: &[] }),
        "SAND-E0207" => Some(SandErrorDefinition { domain: "auth", retryable: true, payload: &[] }),
        "SAND-E0208" => Some(SandErrorDefinition { domain: "auth", retryable: true, payload: &[] }),
        "SAND-E0209" => Some(SandErrorDefinition { domain: "auth", retryable: true, payload: &[] }),
        "SAND-E0210" => Some(SandErrorDefinition { domain: "auth", retryable: false, payload: &[] }),
        "SAND-E0211" => Some(SandErrorDefinition { domain: "auth", retryable: true, payload: &["domError", "signErrorClass"] }),
        "SAND-E0212" => Some(SandErrorDefinition { domain: "auth", retryable: true, payload: &["domError", "signErrorClass"] }),
        "SAND-E0213" => Some(SandErrorDefinition { domain: "auth", retryable: true, payload: &[] }),
        "SAND-E0214" => Some(SandErrorDefinition { domain: "auth", retryable: true, payload: &["httpStatus"] }),
        "SAND-E0215" => Some(SandErrorDefinition { domain: "auth", retryable: true, payload: &["errno"] }),
        "SAND-E0216" => Some(SandErrorDefinition { domain: "auth", retryable: true, payload: &[] }),
        "SAND-E0217" => Some(SandErrorDefinition { domain: "auth", retryable: false, payload: &[] }),
        "SAND-E0218" => Some(SandErrorDefinition { domain: "auth", retryable: false, payload: &[] }),
        "SAND-E0219" => Some(SandErrorDefinition { domain: "auth", retryable: false, payload: &[] }),
        "SAND-E0301" => Some(SandErrorDefinition { domain: "rebuild", retryable: true, payload: &["stage"] }),
        "SAND-E0302" => Some(SandErrorDefinition { domain: "rebuild", retryable: true, payload: &[] }),
        "SAND-E0303" => Some(SandErrorDefinition { domain: "rebuild", retryable: true, payload: &[] }),
        "SAND-E0304" => Some(SandErrorDefinition { domain: "rebuild", retryable: true, payload: &[] }),
        "SAND-E0305" => Some(SandErrorDefinition { domain: "rebuild", retryable: true, payload: &[] }),
        "SAND-E0401" => Some(SandErrorDefinition { domain: "agent", retryable: true, payload: &["connectCode"] }),
        "SAND-E0402" => Some(SandErrorDefinition { domain: "agent", retryable: true, payload: &[] }),
        "SAND-E0403" => Some(SandErrorDefinition { domain: "agent", retryable: true, payload: &["connectCode", "errno"] }),
        "SAND-E0404" => Some(SandErrorDefinition { domain: "agent", retryable: false, payload: &[] }),
        "SAND-E0405" => Some(SandErrorDefinition { domain: "agent", retryable: false, payload: &["connectCode"] }),
        "SAND-E0406" => Some(SandErrorDefinition { domain: "agent", retryable: true, payload: &["connectCode"] }),
        "SAND-E0407" => Some(SandErrorDefinition { domain: "agent", retryable: false, payload: &[] }),
        "SAND-E0408" => Some(SandErrorDefinition { domain: "agent", retryable: true, payload: &["connectCode", "retryAfterMs"] }),
        "SAND-E0409" => Some(SandErrorDefinition { domain: "agent", retryable: false, payload: &[] }),
        "SAND-E0410" => Some(SandErrorDefinition { domain: "agent", retryable: false, payload: &[] }),
        "SAND-E0411" => Some(SandErrorDefinition { domain: "agent", retryable: true, payload: &[] }),
        "SAND-E0412" => Some(SandErrorDefinition { domain: "agent", retryable: false, payload: &[] }),
        "SAND-E0413" => Some(SandErrorDefinition { domain: "agent", retryable: true, payload: &[] }),
        "SAND-E0414" => Some(SandErrorDefinition { domain: "agent", retryable: false, payload: &[] }),
        "SAND-E0501" => Some(SandErrorDefinition { domain: "update", retryable: true, payload: &[] }),
        "SAND-E0502" => Some(SandErrorDefinition { domain: "update", retryable: true, payload: &["httpStatus"] }),
        "SAND-E0503" => Some(SandErrorDefinition { domain: "update", retryable: false, payload: &[] }),
        "SAND-E0504" => Some(SandErrorDefinition { domain: "update", retryable: true, payload: &[] }),
        "SAND-E0505" => Some(SandErrorDefinition { domain: "update", retryable: false, payload: &[] }),
        "SAND-E0506" => Some(SandErrorDefinition { domain: "update", retryable: false, payload: &[] }),
        "SAND-E0507" => Some(SandErrorDefinition { domain: "update", retryable: false, payload: &[] }),
        "SAND-E0508" => Some(SandErrorDefinition { domain: "update", retryable: false, payload: &[] }),
        "SAND-E0509" => Some(SandErrorDefinition { domain: "update", retryable: true, payload: &[] }),
        "SAND-E0510" => Some(SandErrorDefinition { domain: "update", retryable: true, payload: &[] }),
        "SAND-E0511" => Some(SandErrorDefinition { domain: "update", retryable: true, payload: &["errno"] }),
        "SAND-E0512" => Some(SandErrorDefinition { domain: "update", retryable: false, payload: &[] }),
        "SAND-E0601" => Some(SandErrorDefinition { domain: "desktop", retryable: false, payload: &["phase"] }),
        "SAND-E0602" => Some(SandErrorDefinition { domain: "desktop", retryable: true, payload: &["phase"] }),
        "SAND-E0603" => Some(SandErrorDefinition { domain: "desktop", retryable: false, payload: &[] }),
        "SAND-E0604" => Some(SandErrorDefinition { domain: "desktop", retryable: false, payload: &[] }),
        "SAND-E0605" => Some(SandErrorDefinition { domain: "desktop", retryable: true, payload: &["process", "reason"] }),
        "SAND-E0606" => Some(SandErrorDefinition { domain: "desktop", retryable: true, payload: &["leg"] }),
        "SAND-E0607" => Some(SandErrorDefinition { domain: "desktop", retryable: true, payload: &["timeoutMs"] }),
        "SAND-E0608" => Some(SandErrorDefinition { domain: "desktop", retryable: true, payload: &["errno"] }),
        "SAND-E0609" => Some(SandErrorDefinition { domain: "desktop", retryable: true, payload: &[] }),
        "SAND-E0610" => Some(SandErrorDefinition { domain: "desktop", retryable: false, payload: &[] }),
        "SAND-E0700" => Some(SandErrorDefinition { domain: "storage", retryable: false, payload: &[] }),
        "SAND-E0701" => Some(SandErrorDefinition { domain: "storage", retryable: true, payload: &["errno"] }),
        "SAND-E0702" => Some(SandErrorDefinition { domain: "storage", retryable: false, payload: &[] }),
        "SAND-E0703" => Some(SandErrorDefinition { domain: "storage", retryable: false, payload: &[] }),
        "SAND-E0704" => Some(SandErrorDefinition { domain: "storage", retryable: false, payload: &[] }),
        "SAND-E0705" => Some(SandErrorDefinition { domain: "storage", retryable: false, payload: &[] }),
        "SAND-E0706" => Some(SandErrorDefinition { domain: "storage", retryable: false, payload: &[] }),
        "SAND-E0707" => Some(SandErrorDefinition { domain: "storage", retryable: false, payload: &[] }),
        "SAND-E0720" => Some(SandErrorDefinition { domain: "storage", retryable: true, payload: &["errno"] }),
        "SAND-E0721" => Some(SandErrorDefinition { domain: "storage", retryable: true, payload: &["errno"] }),
        "SAND-E0722" => Some(SandErrorDefinition { domain: "storage", retryable: false, payload: &["tail", "errno"] }),
        "SAND-E0723" => Some(SandErrorDefinition { domain: "storage", retryable: true, payload: &["errno"] }),
        "SAND-E0724" => Some(SandErrorDefinition { domain: "storage", retryable: true, payload: &["tail", "errno"] }),
        _ => None,
    }
}

fn tag_name(field: &str) -> String {
    let mut out = String::with_capacity(field.len() + 4);
    for ch in field.chars() {
        if ch.is_ascii_uppercase() {
            out.push('_');
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn bounded_tag_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'|' | b':' | b'-'))
}

pub fn sand_error_tags(error: &SandErrorValue) -> BTreeMap<String, String> {
    let registered = sand_error_definition(&error.code);
    let wire_code = if registered.is_some() {
        error.code.as_str()
    } else {
        UNREGISTERED_CODE
    };
    let definition = sand_error_definition(wire_code)
        .expect("SAND-E0001 must remain present in the frozen registry");
    let mut tags = BTreeMap::from([
        ("error_code".into(), wire_code.into()),
        ("error_domain".into(), definition.domain.into()),
        ("error_retryable".into(), definition.retryable.to_string()),
    ]);
    if registered.is_none() {
        return tags;
    }

    for field in definition.payload {
        let Some(value) = error.payload.get(*field) else {
            continue;
        };
        let value = match value {
            SandErrorPayloadValue::String(value) if bounded_tag_value(value) => Some(value.clone()),
            SandErrorPayloadValue::Number(value) if value.is_finite() => Some(value.to_string()),
            SandErrorPayloadValue::Bool(value) => Some(value.to_string()),
            _ => None,
        };
        if let Some(value) = value {
            tags.insert(tag_name(field), value);
        }
    }
    tags
}
