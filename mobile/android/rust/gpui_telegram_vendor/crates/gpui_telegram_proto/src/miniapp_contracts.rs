//! 1:1 Telegram Mini App, BotFather & AI Companion Bot Contracts.
//!
//! Bridges `fabushi_messaging_core::miniapp` permissions/domain concepts with
//! Telegram-compatible BotFather, Mini App UI schema, `initData` HMAC,
//! CloudStorage quota enforcement, and companion AI bot workflows.

use crate::message::InlineKeyboardButton;
use serde::{Deserialize, Serialize};

/// Canonical Mini App runtime permissions shared across Telegram & Fabushi Mini Apps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MiniAppPermission {
    Identity,
    Theme,
    ClipboardRead,
    ClipboardWrite,
    Camera,
    Microphone,
    Location,
    Contacts,
    Files,
    Notifications,
    Haptics,
    Payments,
    OpenExternal,
    SendMessage,
    ReadConversation,
    ServiceCall,
}

/// Reserved official `@BotFather` user ID (`UserId(777_001)`).
pub const BOTFATHER_USER_ID: i64 = 777_001;
/// Canonical `@BotFather` username.
pub const BOTFATHER_USERNAME: &str = "BotFather";

/// Canonical Fabushi geometric avatar shapes (`profile.json` -> `avatarShape`).
pub const FABUSHI_AVATAR_SHAPES: [&str; 8] = [
    "squircle",
    "blob",
    "pebble",
    "tablet",
    "hex",
    "cloud",
    "teardrop",
    "wedge",
];

/// Canonical Fabushi avatar accent palette (`profile.json` -> `avatarColor`).
pub const FABUSHI_AVATAR_COLORS: [&str; 8] = [
    "#4F46E5",
    "#0EA5E9",
    "#10B981",
    "#F59E0B",
    "#EC4899",
    "#8B5CF6",
    "#14B8A6",
    "#F97316",
];

fn stable_seed_hash(seed: &str) -> usize {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in seed.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash as usize
}

/// Deterministic Fabushi avatar shape from an arbitrary seed (e.g. `agent_id` or `@username`).
pub fn default_fabushi_avatar_shape(seed: &str) -> &'static str {
    let idx = stable_seed_hash(seed) % FABUSHI_AVATAR_SHAPES.len();
    FABUSHI_AVATAR_SHAPES[idx]
}

/// Deterministic Fabushi avatar hex color from an arbitrary seed.
pub fn default_fabushi_avatar_color(seed: &str) -> &'static str {
    let idx = (stable_seed_hash(seed) / FABUSHI_AVATAR_SHAPES.len()) % FABUSHI_AVATAR_COLORS.len();
    FABUSHI_AVATAR_COLORS[idx]
}

/// Canonical Grok Bot 0.18 dynamic avatar shapes (`character.tsx` / `scene.ts`).
pub const GROK_AVATAR_SHAPES: [&str; 8] = [
    "blob",
    "pebble",
    "squircle",
    "tablet",
    "wedge",
    "hex",
    "cloud",
    "teardrop",
];

/// Canonical Grok Bot 0.18 avatar color IDs (11 palette colors including `black`).
pub const GROK_AVATAR_COLOR_IDS: [&str; 11] = [
    "black",
    "brown",
    "red",
    "orange",
    "yellow",
    "green",
    "cyan",
    "blue",
    "violet",
    "magenta",
    "gray",
];

/// Grok Bot 0.18 deterministic fallback color IDs (10 non-black palette colors).
pub const GROK_AVATAR_FALLBACK_COLOR_IDS: [&str; 10] = [
    "brown",
    "red",
    "orange",
    "yellow",
    "green",
    "cyan",
    "blue",
    "violet",
    "magenta",
    "gray",
];

/// Sentinel shape value opting out of Grok dynamic avatars in favor of classic circular initials.
pub const CLASSIC_INITIALS_AVATAR_SHAPE: &str = "classic";

/// Grok Bot 0.18 FNV-1a 32-bit hash over UTF-16 code units (exact JS `charCodeAt` parity).
pub fn grok_shipped_hash(value: &str) -> u32 {
    let mut hash: u32 = 2_166_136_261;
    for cu in value.encode_utf16() {
        hash = (hash ^ u32::from(cu)).wrapping_mul(16_777_619);
    }
    hash
}

/// Deterministic Grok Bot 0.18 fallback color index (`0..=9`) into `GROK_AVATAR_FALLBACK_COLOR_IDS`.
pub fn grok_shipped_color_index(value: &str) -> usize {
    let seed = grok_shipped_hash(value) ^ 2_654_435_769;
    let s2 = (seed ^ 2_654_435_769).wrapping_add(1_831_565_813);
    let mut next = (s2 ^ (s2 >> 15)).wrapping_mul(1 | s2);
    next ^= next.wrapping_add((next ^ (next >> 7)).wrapping_mul(61 | next));
    let r = f64::from(next ^ (next >> 14)) / 4_294_967_296.0;
    ((r * 10.0).floor() as usize).min(9)
}

/// Deterministic Grok Bot 0.18 avalanche shape hash for indexing `GROK_AVATAR_SHAPES`.
pub fn grok_shipped_shape_hash(value: &str) -> u32 {
    let mut hash = grok_shipped_hash(value);
    hash = (hash ^ (hash >> 16)).wrapping_mul(73_244_475);
    hash = (hash ^ (hash >> 13)).wrapping_mul(3_266_489_909);
    hash ^ (hash >> 16)
}

/// Resolves an optional persona avatar color into a Grok color ID or uppercase `#RRGGBB` hex,
/// falling back deterministically via `grok_shipped_color_index(agent_id)`.
pub fn resolve_grok_persona_color(agent_id: &str, color: Option<&str>) -> String {
    if let Some(raw) = color {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            let lower = trimmed.to_ascii_lowercase();
            if GROK_AVATAR_COLOR_IDS.contains(&lower.as_str()) {
                return lower;
            }
            if trimmed.len() == 7
                && trimmed.starts_with('#')
                && trimmed[1..].bytes().all(|b| b.is_ascii_hexdigit())
            {
                return trimmed.to_ascii_uppercase();
            }
            if trimmed.len() == 4
                && trimmed.starts_with('#')
                && trimmed[1..].bytes().all(|b| b.is_ascii_hexdigit())
            {
                let bytes = trimmed.as_bytes();
                let r = (bytes[1] as char).to_ascii_uppercase();
                let g = (bytes[2] as char).to_ascii_uppercase();
                let b = (bytes[3] as char).to_ascii_uppercase();
                return format!("#{r}{r}{g}{g}{b}{b}");
            }
        }
    }
    GROK_AVATAR_FALLBACK_COLOR_IDS[grok_shipped_color_index(agent_id)].to_string()
}

/// Resolves an optional persona avatar shape into a Grok dynamic shape ID or `"classic"`,
/// mapping legacy aliases and falling back deterministically via `grok_shipped_shape_hash(agent_id)`.
pub fn resolve_grok_persona_shape(agent_id: &str, shape: Option<&str>) -> String {
    if let Some(raw) = shape {
        let normalized = raw.trim().to_ascii_lowercase();
        if normalized == CLASSIC_INITIALS_AVATAR_SHAPE {
            return CLASSIC_INITIALS_AVATAR_SHAPE.to_string();
        }
        let mapped = match normalized.as_str() {
            "circle" | "round" => "blob",
            "rounded" | "square" | "box" => "squircle",
            "hexagon" => "hex",
            "shield" | "diamond" | "spark" => "wedge",
            other => other,
        };
        if GROK_AVATAR_SHAPES.contains(&mapped) {
            return mapped.to_string();
        }
    }
    GROK_AVATAR_SHAPES[(grok_shipped_shape_hash(agent_id) as usize) % GROK_AVATAR_SHAPES.len()]
        .to_string()
}

/// Normalizes an optional avatar shape against the Grok/Fabushi shape catalog.
pub fn normalize_fabushi_avatar_shape(raw: &str, seed: &str) -> String {
    let trimmed = raw.trim();
    let opt = (!trimmed.is_empty()).then_some(trimmed);
    resolve_grok_persona_shape(seed, opt)
}

/// Normalizes an optional avatar color string (Grok color ID or `#RRGGBB` / `#RGB`) with deterministic fallback.
pub fn normalize_fabushi_avatar_color(raw: &str, seed: &str) -> String {
    let trimmed = raw.trim();
    let opt = (!trimmed.is_empty()).then_some(trimmed);
    resolve_grok_persona_color(seed, opt)
}

/// Derives a safe Mahayana/Fabushi `agent_id` slug (`[a-z0-9][a-z0-9._-]*`).
pub fn derive_safe_agent_id(
    preferred: Option<&str>,
    username: &str,
    bot_user_id: i64,
) -> String {
    let sanitize = |raw: &str| -> String {
        let trimmed = raw.trim().trim_start_matches('@').to_ascii_lowercase();
        let mut out = String::with_capacity(trimmed.len().min(48));
        let mut prev_sep = false;
        for ch in trimmed.chars() {
            if ch.is_ascii_alphanumeric() {
                out.push(ch);
                prev_sep = false;
            } else if matches!(ch, '_' | '-' | '.') && !out.is_empty() && !prev_sep {
                out.push(if ch == '.' { '.' } else { '-' });
                prev_sep = true;
            }
            if out.len() >= 48 {
                break;
            }
        }
        while out.ends_with('-') || out.ends_with('.') || out.ends_with('_') {
            out.pop();
        }
        out
    };

    if let Some(pref) = preferred {
        let clean = sanitize(pref);
        if !clean.is_empty() && clean != "." && clean != ".." {
            return clean;
        }
    }
    let from_user = sanitize(username);
    if !from_user.is_empty() && from_user != "." && from_user != ".." {
        return from_user;
    }
    format!("fabushi-agent-{}", bot_user_id.max(0))
}

/// Telegram Mini App CloudStorage limits (1:1 parity).
pub const CLOUD_STORAGE_MAX_KEYS: usize = 1024;
pub const CLOUD_STORAGE_MAX_KEY_BYTES: usize = 128;
pub const CLOUD_STORAGE_MAX_VALUE_BYTES: usize = 4096;
pub const CLOUD_STORAGE_MAX_TOTAL_BYTES: usize = 5 * 1024 * 1024;

/// Telegram Desktop `Data::BotApp` prolong & refresh intervals.
pub const MINIAPP_PROLONG_TIMEOUT_SECS: u64 = 60;
pub const MINIAPP_REFRESH_BOTS_TIMEOUT_SECS: u64 = 3600;

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct BotCommandSpec {
    pub command: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BotRegistryEntry {
    pub bot_user_id: i64,
    pub owner_user_id: i64,
    pub username: String,
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub about: String,
    #[serde(default)]
    pub ai_persona: String,
    #[serde(default)]
    pub menu_button_text: String,
    #[serde(default)]
    pub menu_button_url: String,
    #[serde(default)]
    pub is_official: bool,
    #[serde(default)]
    pub is_inline: bool,
    #[serde(default)]
    pub allow_attach_menu: bool,
    #[serde(default)]
    pub attach_peer_types: Vec<String>,
    #[serde(default)]
    pub commands: Vec<BotCommandSpec>,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub agent_id: String,
    #[serde(default)]
    pub avatar_shape: String,
    #[serde(default)]
    pub avatar_color: String,
    #[serde(default)]
    pub preferred_model: String,
    #[serde(default)]
    pub thinking_level: String,
    #[serde(default)]
    pub approval_mode: String,
    #[serde(default)]
    pub toolset_preset: String,
    #[serde(default)]
    pub is_fabushi_bot: bool,
    #[serde(default)]
    pub menu_button_type: String,
    #[serde(default)]
    pub has_main_app: bool,
    #[serde(default)]
    pub main_app_short_name: String,
    #[serde(default)]
    pub privacy_policy_url: String,
    #[serde(default)]
    pub has_settings: bool,
    #[serde(default)]
    pub can_join_groups: bool,
    #[serde(default)]
    pub mau_count: i64,
    #[serde(default)]
    pub allows_write_to_pm: bool,
    #[serde(default)]
    pub app_tos_accepted: bool,
    #[serde(default)]
    pub is_blocked_by_viewer: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coding_agent: Option<crate::coding_agent_contracts::CodingAgentBotConfig>,
}

/// Returned ONLY to the bot owner upon bot creation or token fetch/revocation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BotTokenResponse {
    pub bot: BotRegistryEntry,
    pub bot_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub companion_chat_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiniAppThemeParams {
    pub bg_color: String,
    pub secondary_bg_color: String,
    pub text_color: String,
    pub hint_color: String,
    pub button_color: String,
    pub button_text_color: String,
}

impl Default for MiniAppThemeParams {
    fn default() -> Self {
        Self {
            bg_color: "#ffffff".to_string(),
            secondary_bg_color: "#f4f5f7".to_string(),
            text_color: "#111827".to_string(),
            hint_color: "#6b7280".to_string(),
            button_color: "#2481cc".to_string(),
            button_text_color: "#ffffff".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MiniAppActionButton {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_color: Option<String>,
    #[serde(default = "default_true")]
    pub is_visible: bool,
    #[serde(default = "default_true")]
    pub is_active: bool,
    #[serde(default)]
    pub is_progress_visible: bool,
    #[serde(default)]
    pub action_id: String,
    #[serde(default)]
    pub sends_data: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum MiniAppWidgetType {
    #[serde(alias = "header")]
    Header,
    #[serde(alias = "badge")]
    Badge,
    #[serde(alias = "metric_card", alias = "metricCard")]
    MetricCard,
    #[default]
    #[serde(alias = "text")]
    Text,
    #[serde(alias = "text_input", alias = "textInput")]
    TextInput,
    #[serde(alias = "select")]
    Select,
    #[serde(alias = "toggle")]
    Toggle,
    #[serde(alias = "counter")]
    Counter,
    #[serde(alias = "list_item", alias = "listItem")]
    ListItem,
    #[serde(alias = "button")]
    Button,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MiniAppUiWidget {
    pub id: String,
    #[serde(alias = "type", alias = "widget_type")]
    pub kind: MiniAppWidgetType,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(default)]
    pub checked: bool,
    #[serde(default)]
    pub count: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MiniAppUiSchema {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub theme: MiniAppThemeParams,
    #[serde(default)]
    pub widgets: Vec<MiniAppUiWidget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_button: Option<MiniAppActionButton>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary_button: Option<MiniAppActionButton>,
    #[serde(default)]
    pub back_button_visible: bool,
    #[serde(default)]
    pub settings_button_visible: bool,
    #[serde(default)]
    pub copilot_suggestions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MiniAppManifestDto {
    pub app_id: String,
    pub bot_username: String,
    pub short_name: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub icon_emoji: String,
    #[serde(default)]
    pub web_url: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub peer_types: Vec<String>,
    #[serde(default)]
    pub requested_permissions: Vec<MiniAppPermission>,
    #[serde(default)]
    pub ui_schema: MiniAppUiSchema,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MiniAppActionSpec {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,
    pub risk_level: String,
    pub requires_approval: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AgentManifest {
    pub app_id: String,
    pub version: String,
    pub readable_state_schema: serde_json::Value,
    pub actions: Vec<MiniAppActionSpec>,
}

fn is_valid_json_schema_type_name(s: &str) -> bool {
    matches!(
        s,
        "object" | "array" | "string" | "number" | "integer" | "boolean" | "null"
    )
}

fn validate_schema_definition(schema: &serde_json::Value, label: &str) -> Result<(), String> {
    let obj = schema
        .as_object()
        .ok_or_else(|| format!("{label} must be a JSON Schema object"))?;
    if let Some(ty) = obj.get("type") {
        match ty {
            serde_json::Value::String(s) => {
                if !is_valid_json_schema_type_name(s) {
                    return Err(format!("{label} has invalid schema type '{s}'"));
                }
            }
            serde_json::Value::Array(arr) if !arr.is_empty() => {
                for item in arr {
                    let s = item
                        .as_str()
                        .ok_or_else(|| format!("{label}.type array elements must be strings"))?;
                    if !is_valid_json_schema_type_name(s) {
                        return Err(format!("{label} has invalid schema type '{s}'"));
                    }
                }
            }
            _ => {
                return Err(format!("{label}.type must be a valid JSON Schema type string or array"));
            }
        }
    }
    if let Some(req) = obj.get("required") {
        let arr = req
            .as_array()
            .ok_or_else(|| format!("{label}.required must be an array of field names"))?;
        for item in arr {
            if item.as_str().is_none() {
                return Err(format!("{label}.required entries must be strings"));
            }
        }
    }
    if let Some(props) = obj.get("properties") {
        let prop_map = props
            .as_object()
            .ok_or_else(|| format!("{label}.properties must be a JSON object"))?;
        for (k, v) in prop_map {
            validate_schema_definition(v, &format!("{label}.properties.{k}"))?;
        }
    }
    if let Some(add_props) = obj.get("additionalProperties") {
        if !add_props.is_boolean() {
            validate_schema_definition(add_props, &format!("{label}.additionalProperties"))?;
        }
    }
    if let Some(items) = obj.get("items") {
        validate_schema_definition(items, &format!("{label}.items"))?;
    }
    Ok(())
}

fn value_matches_json_type(val: &serde_json::Value, ty: &str) -> bool {
    match ty {
        "object" => val.is_object(),
        "array" => val.is_array(),
        "string" => val.is_string(),
        "number" => val.is_number(),
        "integer" => val.as_i64().is_some() || val.as_u64().is_some(),
        "boolean" => val.is_boolean(),
        "null" => val.is_null(),
        _ => false,
    }
}

fn schema_allows_null(schema: &serde_json::Value) -> bool {
    match schema.get("type") {
        Some(serde_json::Value::String(s)) => s == "null",
        Some(serde_json::Value::Array(arr)) => arr.iter().any(|t| t.as_str() == Some("null")),
        _ => false,
    }
}

fn validate_value_against_schema(
    val: &serde_json::Value,
    schema: &serde_json::Value,
    path: &str,
) -> Result<(), String> {
    if let Some(b) = schema.as_bool() {
        return if b {
            Ok(())
        } else {
            Err(format!("Schema validation failed at '{path}': schema is false"))
        };
    }
    let obj = schema
        .as_object()
        .ok_or_else(|| format!("Invalid schema at '{path}': expected schema object"))?;

    if let Some(ty) = obj.get("type") {
        let matched = match ty {
            serde_json::Value::String(s) => value_matches_json_type(val, s),
            serde_json::Value::Array(arr) => arr
                .iter()
                .filter_map(|x| x.as_str())
                .any(|s| value_matches_json_type(val, s)),
            _ => false,
        };
        if !matched {
            return Err(format!(
                "Schema validation failed at '{path}': expected type {ty}, got {val}"
            ));
        }
    }

    if let Some(enum_vals) = obj.get("enum").and_then(|v| v.as_array()) {
        if !enum_vals.iter().any(|candidate| candidate == val) {
            return Err(format!(
                "Schema validation failed at '{path}': value {val} is not in enum"
            ));
        }
    }

    if let Some(map) = val.as_object() {
        let props_map = obj.get("properties").and_then(|v| v.as_object());

        if let Some(req_fields) = obj.get("required").and_then(|v| v.as_array()) {
            for req in req_fields {
                if let Some(name) = req.as_str() {
                    match map.get(name) {
                        None => {
                            return Err(format!("Missing required parameter: {name}"));
                        }
                        Some(v) if v.is_null() => {
                            let null_allowed = props_map
                                .and_then(|pm| pm.get(name))
                                .is_some_and(schema_allows_null);
                            if !null_allowed {
                                return Err(format!(
                                    "Required parameter '{name}' at '{path}' cannot be null"
                                ));
                            }
                        }
                        Some(_) => {}
                    }
                }
            }
        }

        if let Some(pm) = props_map {
            for (k, prop_schema) in pm {
                if let Some(v) = map.get(k) {
                    validate_value_against_schema(v, prop_schema, &format!("{path}.{k}"))?;
                }
            }
        }

        if let Some(add_props) = obj.get("additionalProperties") {
            match add_props {
                serde_json::Value::Bool(false) => {
                    for k in map.keys() {
                        let declared = props_map.is_some_and(|pm| pm.contains_key(k));
                        if !declared {
                            return Err(format!(
                                "Schema validation failed at '{path}': additional property '{k}' is not allowed"
                            ));
                        }
                    }
                }
                serde_json::Value::Object(_) => {
                    for (k, v) in map {
                        let declared = props_map.is_some_and(|pm| pm.contains_key(k));
                        if !declared {
                            validate_value_against_schema(v, add_props, &format!("{path}.{k}"))?;
                        }
                    }
                }
                _ => {}
            }
        }
    } else if let Some(arr) = val.as_array() {
        if let Some(min_items) = obj.get("minItems").and_then(|v| v.as_u64()) {
            if (arr.len() as u64) < min_items {
                return Err(format!(
                    "Schema validation failed at '{path}': expected at least {min_items} items"
                ));
            }
        }
        if let Some(max_items) = obj.get("maxItems").and_then(|v| v.as_u64()) {
            if (arr.len() as u64) > max_items {
                return Err(format!(
                    "Schema validation failed at '{path}': expected at most {max_items} items"
                ));
            }
        }
        if let Some(item_schema) = obj.get("items") {
            for (i, item) in arr.iter().enumerate() {
                validate_value_against_schema(item, item_schema, &format!("{path}[{i}]"))?;
            }
        }
    } else if let Some(s) = val.as_str() {
        let len = s.chars().count() as u64;
        if let Some(min_len) = obj.get("minLength").and_then(|v| v.as_u64()) {
            if len < min_len {
                return Err(format!(
                    "Schema validation failed at '{path}': string length {len} < minLength {min_len}"
                ));
            }
        }
        if let Some(max_len) = obj.get("maxLength").and_then(|v| v.as_u64()) {
            if len > max_len {
                return Err(format!(
                    "Schema validation failed at '{path}': string length {len} > maxLength {max_len}"
                ));
            }
        }
    } else if let Some(n) = val.as_f64() {
        if let Some(min) = obj.get("minimum").and_then(|v| v.as_f64()) {
            if n < min {
                return Err(format!(
                    "Schema validation failed at '{path}': {n} < minimum {min}"
                ));
            }
        }
        if let Some(max) = obj.get("maximum").and_then(|v| v.as_f64()) {
            if n > max {
                return Err(format!(
                    "Schema validation failed at '{path}': {n} > maximum {max}"
                ));
            }
        }
    }

    Ok(())
}

impl AgentManifest {
    pub fn from_json(raw: &str) -> Result<Self, String> {
        let manifest = serde_json::from_str::<Self>(raw)
            .map_err(|e| format!("Invalid AgentManifest JSON: {e}"))?;
        if manifest.app_id.trim().is_empty() {
            return Err("AgentManifest 'app_id' cannot be empty".into());
        }
        validate_schema_definition(&manifest.readable_state_schema, "readable_state_schema")?;
        for (idx, action) in manifest.actions.iter().enumerate() {
            if action.name.trim().is_empty() {
                return Err(format!("AgentManifest actions[{idx}].name cannot be empty"));
            }
            validate_schema_definition(
                &action.input_schema,
                &format!("actions[{}].input_schema", action.name),
            )?;
            validate_schema_definition(
                &action.output_schema,
                &format!("actions[{}].output_schema", action.name),
            )?;
        }
        Ok(manifest)
    }

    pub fn validate_action_call<'a>(
        &'a self,
        action_name: &str,
        params: &serde_json::Value,
    ) -> Result<&'a MiniAppActionSpec, String> {
        let action = self
            .actions
            .iter()
            .find(|a| a.name == action_name)
            .ok_or_else(|| format!("Action not declared: {action_name}"))?;

        validate_value_against_schema(params, &action.input_schema, "params")?;
        Ok(action)
    }

    pub fn execute_action(
        &self,
        action_name: &str,
        params: &serde_json::Value,
        approved: bool,
    ) -> Result<serde_json::Value, String> {
        let spec = self.validate_action_call(action_name, params)?;
        if (spec.requires_approval
            || spec.risk_level.eq_ignore_ascii_case("high")
            || spec.risk_level.eq_ignore_ascii_case("critical"))
            && !approved
        {
            return Err(format!(
                "Confirmation required for {} risk action '{}'",
                spec.risk_level, spec.name
            ));
        }
        Ok(serde_json::json!({
            "app_id": self.app_id,
            "action": spec.name,
            "risk_level": spec.risk_level,
            "approved": approved,
            "params": params,
            "status": "executed"
        }))
    }

    pub fn validate_state(&self, state: &serde_json::Value) -> Result<(), String> {
        if !state.is_object() {
            return Err("State must be a JSON object".into());
        }
        validate_value_against_schema(state, &self.readable_state_schema, "state")
    }
}

impl MiniAppManifestDto {
    pub fn to_agent_manifest(&self) -> AgentManifest {
        let mut actions = Vec::new();
        for w in &self.ui_schema.widgets {
            let value_schema = match w.kind {
                MiniAppWidgetType::Counter | MiniAppWidgetType::MetricCard => {
                    serde_json::json!({ "type": "number" })
                }
                MiniAppWidgetType::Toggle => serde_json::json!({ "type": "boolean" }),
                _ => serde_json::json!({ "type": "string" }),
            };
            actions.push(MiniAppActionSpec {
                name: format!("widget:{}", w.id),
                description: w.label.clone(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "value": value_schema
                    }
                }),
                output_schema: serde_json::json!({ "type": "object" }),
                risk_level: "low".to_string(),
                requires_approval: false,
            });
        }
        if let Some(btn) = &self.ui_schema.main_button {
            actions.push(MiniAppActionSpec {
                name: "main_button_click".to_string(),
                description: btn.text.clone(),
                input_schema: serde_json::json!({ "type": "object" }),
                output_schema: serde_json::json!({ "type": "object" }),
                risk_level: "low".to_string(),
                requires_approval: false,
            });
        }
        if let Some(btn) = &self.ui_schema.secondary_button {
            actions.push(MiniAppActionSpec {
                name: "secondary_button_click".to_string(),
                description: btn.text.clone(),
                input_schema: serde_json::json!({ "type": "object" }),
                output_schema: serde_json::json!({ "type": "object" }),
                risk_level: "low".to_string(),
                requires_approval: false,
            });
        }
        AgentManifest {
            app_id: self.app_id.clone(),
            version: if self.version.is_empty() {
                "1.0.0".to_string()
            } else {
                self.version.clone()
            },
            readable_state_schema: serde_json::json!({ "type": "object" }),
            actions,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MiniAppEntry {
    pub app_id: String,
    pub bot_user_id: i64,
    pub bot_username: String,
    pub bot_display_name: String,
    pub short_name: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub icon_emoji: String,
    #[serde(default)]
    pub web_url: String,
    #[serde(default)]
    pub ui_schema: MiniAppUiSchema,
    #[serde(default)]
    pub peer_types: Vec<String>,
    #[serde(default)]
    pub install_count: i64,
    #[serde(default)]
    pub is_installed: bool,
    #[serde(default)]
    pub has_write_access: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_chat_id: Option<i64>,
    #[serde(default)]
    pub created_by_nl: bool,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    pub mau_count: i64,
    #[serde(default)]
    pub is_main_app: bool,
    #[serde(default)]
    pub is_recent_used: bool,
    #[serde(default)]
    pub last_launched_at: i64,
    #[serde(default)]
    pub privacy_policy_url: String,
    #[serde(default)]
    pub app_tos_accepted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_color: Option<String>,
}

impl MiniAppEntry {
    pub fn deep_link(&self) -> String {
        format!(
            "https://t.me/{}/{}",
            self.bot_username.trim_start_matches('@'),
            self.short_name
        )
    }

    pub fn gpui_uri(&self) -> String {
        format!(
            "gpui-miniapp://{}/{}",
            self.bot_username.trim_start_matches('@'),
            self.short_name
        )
    }

    pub fn relative_web_path(&self) -> String {
        format!(
            "/miniapps/{}/{}",
            self.bot_username.trim_start_matches('@'),
            self.short_name
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum MiniAppLaunchMode {
    #[default]
    #[serde(alias = "compact_sheet")]
    CompactSheet,
    #[serde(alias = "full_size")]
    FullSize,
    #[serde(alias = "full_screen")]
    FullScreen,
    #[serde(alias = "attach_menu")]
    AttachMenu,
    #[serde(alias = "menu_button")]
    MenuButton,
    #[serde(alias = "inline_keyboard")]
    InlineKeyboard,
    #[serde(alias = "direct_link")]
    DirectLink,
    #[serde(alias = "main_mini_app")]
    MainMiniApp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MiniAppInitDataDto {
    pub query_id: String,
    pub user_json: String,
    pub auth_date: i64,
    pub hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_param: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_instance: Option<String>,
    pub raw_query_string: String,
    #[serde(default)]
    pub hmac_verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CloudStorageItem {
    pub key: String,
    pub value: String,
    #[serde(default)]
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MiniAppLaunchContext {
    pub app: MiniAppEntry,
    pub companion_bot: BotRegistryEntry,
    pub companion_chat_id: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_chat_id: Option<i64>,
    #[serde(default)]
    pub launch_mode: MiniAppLaunchMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_param: Option<String>,
    #[serde(default)]
    pub init_data: MiniAppInitDataDto,
    #[serde(default)]
    pub cloud_storage: Vec<CloudStorageItem>,
    #[serde(default)]
    pub state_json: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MiniAppActionKind {
    SendData,
    WidgetEvent,
    AiCopilot,
    MainButtonClick,
    SecondaryButtonClick,
    BackButtonClick,
    SettingsButtonClick,
}

impl MiniAppActionKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SendData => "send_data",
            Self::WidgetEvent => "widget_event",
            Self::AiCopilot => "ai_copilot",
            Self::MainButtonClick => "main_button_click",
            Self::SecondaryButtonClick => "secondary_button_click",
            Self::BackButtonClick => "back_button_click",
            Self::SettingsButtonClick => "settings_button_click",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MiniAppActionRequest {
    pub action_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_id: Option<i64>,
}

impl MiniAppActionRequest {
    pub fn send_data(data: impl Into<String>, chat_id: Option<i64>) -> Self {
        Self {
            action_type: MiniAppActionKind::SendData.as_str().to_string(),
            data: Some(data.into()),
            chat_id,
            ..Default::default()
        }
    }

    pub fn widget_event(
        widget_id: impl Into<String>,
        value: serde_json::Value,
        chat_id: Option<i64>,
    ) -> Self {
        Self {
            action_type: MiniAppActionKind::WidgetEvent.as_str().to_string(),
            widget_id: Some(widget_id.into()),
            value: Some(value),
            chat_id,
            ..Default::default()
        }
    }

    pub fn ai_copilot(prompt: impl Into<String>, chat_id: Option<i64>) -> Self {
        Self {
            action_type: MiniAppActionKind::AiCopilot.as_str().to_string(),
            prompt: Some(prompt.into()),
            chat_id,
            ..Default::default()
        }
    }

    pub fn main_button_click(chat_id: Option<i64>) -> Self {
        Self {
            action_type: MiniAppActionKind::MainButtonClick.as_str().to_string(),
            chat_id,
            ..Default::default()
        }
    }

    pub fn secondary_button_click(chat_id: Option<i64>) -> Self {
        Self {
            action_type: MiniAppActionKind::SecondaryButtonClick.as_str().to_string(),
            chat_id,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MiniAppActionResponse {
    pub ok: bool,
    pub ui_schema: MiniAppUiSchema,
    #[serde(default)]
    pub state_json: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub companion_bot_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub companion_chat_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toast: Option<String>,
    #[serde(default)]
    pub close_app: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_reply: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CreateBotRequest {
    pub display_name: String,
    pub username: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub about: String,
    #[serde(default)]
    pub ai_persona: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menu_button_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menu_button_url: Option<String>,
    #[serde(default)]
    pub allow_attach_menu: bool,
    #[serde(default)]
    pub attach_peer_types: Vec<String>,
    #[serde(default)]
    pub commands: Vec<BotCommandSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_shape: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolset_preset: Option<String>,
}

/// First-class Fabushi Bot creation payload (`POST /api/v1/fabushi-bots`).
/// `username` is optional; when blank, the server auto-assigns a unique `@<slug>_bot`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CreateFabushiBotRequest {
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_shape: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolset_preset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coding_agent: Option<crate::coding_agent_contracts::CodingAgentBotConfig>,
}

/// Partial update payload for a first-class Fabushi Bot (`PATCH /api/v1/fabushi-bots/{bot_id}`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PatchFabushiBotRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_shape: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolset_preset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notify_on_agent_updates: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden_from_sidebar: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coding_agent: Option<crate::coding_agent_contracts::CodingAgentBotConfig>,
}

/// Unified first-class Fabushi Bot summary exposed over REST and cached in Desktop UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FabushiBotSummaryDto {
    pub bot_user_id: i64,
    pub owner_user_id: i64,
    pub agent_id: String,
    pub username: String,
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub avatar_shape: String,
    #[serde(default)]
    pub avatar_color: String,
    #[serde(default)]
    pub preferred_model: String,
    #[serde(default)]
    pub thinking_level: String,
    #[serde(default)]
    pub approval_mode: String,
    #[serde(default)]
    pub toolset_preset: String,
    #[serde(default)]
    pub notify_on_agent_updates: bool,
    #[serde(default)]
    pub hidden_from_sidebar: bool,
    #[serde(default)]
    pub memory_entries_count: usize,
    #[serde(default)]
    pub skills_count: usize,
    #[serde(default)]
    pub execution_state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub companion_chat_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coding_agent: Option<crate::coding_agent_contracts::CodingAgentBotConfig>,
}

/// Response returned when creating a first-class Fabushi Bot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FabushiBotCreateResponse {
    pub bot: FabushiBotSummaryDto,
    pub registry: BotRegistryEntry,
    pub bot_token: String,
    pub companion_chat_id: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CreateMiniAppRequest {
    pub bot_username: String,
    pub short_name: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub icon_emoji: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_schema: Option<MiniAppUiSchema>,
    #[serde(default)]
    pub peer_types: Vec<String>,
    #[serde(default)]
    pub is_attach_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NaturalLanguageCreateRequest {
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_shape: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NaturalLanguageCreateResponse {
    pub bot: BotRegistryEntry,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<MiniAppEntry>,
    pub bot_token: String,
    pub companion_chat_id: i64,
    pub summary_markdown: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct InstallMiniAppRequest {
    #[serde(default)]
    pub allow_write_access: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LaunchMiniAppRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_id: Option<i64>,
    #[serde(default)]
    pub launch_mode: MiniAppLaunchMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_param: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ValidateInitDataRequest {
    pub init_data: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ValidateInitDataResponse {
    pub valid: bool,
    #[serde(default)]
    pub bot_username: String,
    #[serde(default)]
    pub short_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_date: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CloudStorageSetRequest {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CloudStorageDeleteRequest {
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ChatCallbackRequest {
    pub message_id: i64,
    pub callback_data: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PeerBotActionResult {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<MiniAppEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub companion_bot: Option<BotRegistryEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub companion_chat_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_message_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toast: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_keyboard: Option<Vec<Vec<InlineKeyboardButton>>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BotPublicProfileDto {
    pub bot: BotRegistryEntry,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_app: Option<MiniAppEntry>,
    #[serde(default)]
    pub apps: Vec<MiniAppEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub existing_chat_id: Option<i64>,
    #[serde(default)]
    pub has_started_chat: bool,
    #[serde(default)]
    pub allows_write_to_pm: bool,
    #[serde(default)]
    pub app_tos_accepted: bool,
    #[serde(default)]
    pub is_blocked: bool,
    #[serde(default)]
    pub is_muted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CanSendMessageResponse {
    #[serde(default)]
    pub can_send: bool,
    #[serde(default)]
    pub allows_write_to_pm: bool,
    #[serde(default)]
    pub has_started_chat: bool,
    #[serde(default)]
    pub is_blocked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AllowSendMessageRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_app_name: Option<String>,
    #[serde(default)]
    pub from_attachment_menu: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AllowSendMessageResponse {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub chat_id: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_message_id: Option<i64>,
    #[serde(default)]
    pub service_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AcceptMiniAppTosRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_write_access: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BlockBotRequest {
    #[serde(default)]
    pub blocked: bool,
}

/// Validates a Telegram bot username (5-32 ASCII alphanumeric/underscore chars, ends with `bot`).
pub fn validate_bot_username(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim().trim_start_matches('@');
    if trimmed.len() < 5 || trimmed.len() > 32 {
        return Err("Bot username must be 5-32 characters long".to_string());
    }
    let bytes = trimmed.as_bytes();
    if !bytes[0].is_ascii_alphabetic() {
        return Err("Bot username must start with an ASCII letter".to_string());
    }
    if !trimmed
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err("Bot username may only contain letters, digits, and underscores".to_string());
    }
    if !trimmed.to_ascii_lowercase().ends_with("bot") {
        return Err("Sorry, the username must end in 'bot' (e.g. 'TetrisBot' or 'tetris_bot')".to_string());
    }
    Ok(trimmed.to_string())
}

/// Validates and normalizes a Mini App `short_name` (`2-32` chars `[a-zA-Z0-9_-]`).
pub fn validate_miniapp_short_name(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.len() < 2 || trimmed.len() > 32 {
        return Err("Mini App short_name must be 2-32 characters long".to_string());
    }
    if !trimmed
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err("Mini App short_name may only contain letters, digits, '_' or '-'".to_string());
    }
    Ok(trimmed.to_string())
}

/// Validates a bot slash command (`1-32` lowercase alphanumeric/underscore chars).
pub fn validate_bot_command(raw: &str) -> Result<String, String> {
    let clean = raw.trim().trim_start_matches('/');
    if clean.is_empty() || clean.len() > 32 {
        return Err("Bot command must be 1-32 characters long".to_string());
    }
    if !clean
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        return Err("Bot command must contain only lowercase letters, digits, or '_'".to_string());
    }
    Ok(clean.to_string())
}

/// Enforces Telegram CloudStorage limits (`1024` keys, `128B` key, `4096B` value, `5MB` total).
pub fn validate_cloud_storage_entry(
    key: &str,
    value: &str,
    existing: &[CloudStorageItem],
) -> Result<(), String> {
    if key.is_empty() || key.len() > CLOUD_STORAGE_MAX_KEY_BYTES {
        return Err(format!(
            "CloudStorage key must be 1..={} bytes (got {})",
            CLOUD_STORAGE_MAX_KEY_BYTES,
            key.len()
        ));
    }
    if value.len() > CLOUD_STORAGE_MAX_VALUE_BYTES {
        return Err(format!(
            "CloudStorage value exceeds {} bytes limit (got {})",
            CLOUD_STORAGE_MAX_VALUE_BYTES,
            value.len()
        ));
    }
    let existing_pos = existing.iter().position(|item| item.key == key);
    if existing_pos.is_none() && existing.len() >= CLOUD_STORAGE_MAX_KEYS {
        return Err(format!(
            "CloudStorage max keys limit ({}) exceeded",
            CLOUD_STORAGE_MAX_KEYS
        ));
    }
    let other_bytes: usize = existing
        .iter()
        .filter(|item| item.key != key)
        .map(|item| item.key.len() + item.value.len())
        .sum();
    let total_bytes = other_bytes + key.len() + value.len();
    if total_bytes > CLOUD_STORAGE_MAX_TOTAL_BYTES {
        return Err(format!(
            "CloudStorage 5MB quota exceeded ({} > {})",
            total_bytes, CLOUD_STORAGE_MAX_TOTAL_BYTES
        ));
    }
    Ok(())
}

/// Parses `gpui-miniapp://bot/short`, `/miniapps/bot/short`, `http(s)://host/miniapps/bot/short`,
/// or `https://t.me/bot/short?startapp=xyz` into `(bot_username, short_name, start_param)`.
pub fn parse_miniapp_uri(raw: &str) -> Option<(String, String, Option<String>)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let (path_part, query_part) = match trimmed.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (trimmed, None),
    };

    let start_param = query_part.and_then(|q| {
        q.split('&').find_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            if k == "startapp" || k == "start_param" {
                let val = v.trim();
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
            None
        })
    });

    let rest = if let Some(after) = path_part.strip_prefix("gpui-miniapp://") {
        after
    } else if let Some(idx) = path_part.find("/miniapps/") {
        &path_part[idx + "/miniapps/".len()..]
    } else if let Some(after) = path_part
        .strip_prefix("https://t.me/")
        .or_else(|| path_part.strip_prefix("http://t.me/"))
        .or_else(|| path_part.strip_prefix("t.me/"))
    {
        after
    } else {
        return None;
    };

    let mut segments = rest.trim_matches('/').split('/').filter(|s| !s.is_empty());
    let bot = segments.next()?.trim_start_matches('@').to_string();
    let short = segments.next()?.to_string();
    if bot.is_empty() || short.is_empty() {
        return None;
    }
    Some((bot, short, start_param))
}

/// Validates a concrete Mini App `web_app.url` deep-link override (`http(s)://...` or `/path...`),
/// rejecting wrapper URIs (`gpui-miniapp://`, `tg://`, `t.me/`) and unsafe schemes (`javascript:`, `data:`, `file:`, `//`).
pub fn sanitize_webapp_custom_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().any(|c| c.is_control()) {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("gpui-miniapp://")
        || lower.starts_with("tg://")
        || lower.starts_with("https://t.me/")
        || lower.starts_with("http://t.me/")
        || lower == "https://t.me"
        || lower == "http://t.me"
        || lower.starts_with("t.me/")
    {
        return None;
    }
    if let Some(rest) = lower.strip_prefix("https://") {
        if rest.is_empty() || rest.starts_with('/') {
            return None;
        }
        return Some(format!("https://{}", &trimmed[8..]));
    }
    if let Some(rest) = lower.strip_prefix("http://") {
        if rest.is_empty() || rest.starts_with('/') {
            return None;
        }
        return Some(format!("http://{}", &trimmed[7..]));
    }
    if trimmed.starts_with('/') && !trimmed.starts_with("//") {
        return Some(trimmed.to_string());
    }
    None
}

// ============================================================================
// Universal Mini App WebMCP & Human Takeover Contracts
// ============================================================================

/// Descriptor of a WebMCP tool exposed by a Mini App (`navigator.modelContext`) or host fallback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebMcpToolDescriptor {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub input_schema: serde_json::Value,
    #[serde(default)]
    pub read_only: bool,
}

/// Live AI/user control state of an attached or floating Mini App.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MiniAppControlMode {
    #[default]
    Idle,
    AiControlling,
    WaitingForTakeover,
    UserTakeover,
}

/// Command dispatched from server AI tool execution to a connected desktop client Mini App WebView.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MiniAppWebMcpCommand {
    pub request_id: String,
    pub bot_username: String,
    pub short_name: String,
    #[serde(default)]
    pub chat_id: i64,
    #[serde(default)]
    pub bot_user_id: i64,
    #[serde(default)]
    pub app_title: String,
    /// `"discover"` | `"call_tool"` | `"get_state"` | `"request_takeover"`
    pub operation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub arguments: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,
}

/// Response returned from the client Mini App WebView back to the server WebMCP broker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MiniAppWebMcpResponse {
    pub request_id: String,
    pub ok: bool,
    #[serde(default)]
    pub result: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub tools: Vec<WebMcpToolDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_summary: Option<serde_json::Value>,
    #[serde(default)]
    pub control_mode: MiniAppControlMode,
}

/// Human takeover / handback lifecycle event emitted by the desktop client for a Mini App.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MiniAppTakeoverEvent {
    pub bot_username: String,
    pub short_name: String,
    #[serde(default)]
    pub chat_id: i64,
    #[serde(default)]
    pub bot_user_id: i64,
    pub mode: MiniAppControlMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_snapshot: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bot_username_and_short_name_validation() {
        assert_eq!(
            validate_bot_username("@CoffeeOrderBot").unwrap(),
            "CoffeeOrderBot"
        );
        assert_eq!(
            validate_bot_username("task_checkin_bot").unwrap(),
            "task_checkin_bot"
        );
        assert!(validate_bot_username("foo").is_err());
        assert!(validate_bot_username("myhelper").is_err());
        assert!(validate_bot_username("1badbot").is_err());

        assert_eq!(validate_miniapp_short_name("focus").unwrap(), "focus");
        assert_eq!(validate_miniapp_short_name("my-app_1").unwrap(), "my-app_1");
        assert!(validate_miniapp_short_name("a").is_err());
        assert!(validate_miniapp_short_name("bad name").is_err());

        assert_eq!(validate_bot_command("/newbot").unwrap(), "newbot");
        assert!(validate_bot_command("/BadCmd").is_err());
    }

    #[test]
    fn test_cloud_storage_limits_enforced() {
        let mut items = Vec::new();
        assert!(validate_cloud_storage_entry("theme", "dark", &items).is_ok());
        assert!(validate_cloud_storage_entry("", "dark", &items).is_err());
        let long_key = "k".repeat(CLOUD_STORAGE_MAX_KEY_BYTES + 1);
        assert!(validate_cloud_storage_entry(&long_key, "v", &items).is_err());
        let long_val = "v".repeat(CLOUD_STORAGE_MAX_VALUE_BYTES + 1);
        assert!(validate_cloud_storage_entry("ok", &long_val, &items).is_err());

        for i in 0..CLOUD_STORAGE_MAX_KEYS {
            items.push(CloudStorageItem {
                key: format!("k{}", i),
                value: "1".to_string(),
                updated_at: 1,
            });
        }
        assert!(validate_cloud_storage_entry("k_over", "1", &items).is_err());
        assert!(validate_cloud_storage_entry("k0", "updated", &items).is_ok());
    }

    #[test]
    fn test_parse_miniapp_uri_variants() {
        assert_eq!(
            parse_miniapp_uri("gpui-miniapp://@PomodoroFocusBot/focus?startapp=work"),
            Some((
                "PomodoroFocusBot".to_string(),
                "focus".to_string(),
                Some("work".to_string())
            ))
        );
        assert_eq!(
            parse_miniapp_uri("http://127.0.0.1:8080/miniapps/SplitBillBot/split"),
            Some(("SplitBillBot".to_string(), "split".to_string(), None))
        );
        assert_eq!(
            parse_miniapp_uri("https://t.me/PollMasterBot/poll?startapp=team"),
            Some((
                "PollMasterBot".to_string(),
                "poll".to_string(),
                Some("team".to_string())
            ))
        );
        assert_eq!(parse_miniapp_uri("https://example.com/docs"), None);
    }

    #[test]
    fn test_sanitize_webapp_custom_url() {
        assert_eq!(
            sanitize_webapp_custom_url("  https://app.altery.com/kyc?resume=1  "),
            Some("https://app.altery.com/kyc?resume=1".to_string())
        );
        assert_eq!(
            sanitize_webapp_custom_url("HTTP://127.0.0.1:8080/miniapps/Bot/app?resume=1"),
            Some("http://127.0.0.1:8080/miniapps/Bot/app?resume=1".to_string())
        );
        assert_eq!(
            sanitize_webapp_custom_url("/miniapps/AlteryBot/app?step=2"),
            Some("/miniapps/AlteryBot/app?step=2".to_string())
        );
        assert_eq!(sanitize_webapp_custom_url(""), None);
        assert_eq!(sanitize_webapp_custom_url("gpui-miniapp://Bot/app"), None);
        assert_eq!(sanitize_webapp_custom_url("tg://resolve?domain=Bot"), None);
        assert_eq!(sanitize_webapp_custom_url("https://t.me/Bot/app"), None);
        assert_eq!(sanitize_webapp_custom_url("t.me/Bot/app"), None);
        assert_eq!(sanitize_webapp_custom_url("javascript:alert(1)"), None);
        assert_eq!(sanitize_webapp_custom_url("data:text/html,hi"), None);
        assert_eq!(sanitize_webapp_custom_url("file:///etc/passwd"), None);
        assert_eq!(sanitize_webapp_custom_url("//evil.example.com/x"), None);
        assert_eq!(sanitize_webapp_custom_url("https://"), None);
    }

    #[test]
    fn test_miniapp_launch_context_roundtrip() {
        let ctx = MiniAppLaunchContext {
            app: MiniAppEntry {
                app_id: "app_1".to_string(),
                bot_user_id: 600_001,
                bot_username: "CoffeeOrderBot".to_string(),
                bot_display_name: "Coffee Order AI".to_string(),
                short_name: "order".to_string(),
                title: "Coffee Order".to_string(),
                ui_schema: MiniAppUiSchema {
                    title: "Coffee Order".to_string(),
                    widgets: vec![MiniAppUiWidget {
                        id: "qty".to_string(),
                        kind: MiniAppWidgetType::Counter,
                        label: "Cups".to_string(),
                        count: 2,
                        min: Some(1),
                        max: Some(10),
                        ..Default::default()
                    }],
                    main_button: Some(MiniAppActionButton {
                        text: "Submit Order".to_string(),
                        action_id: "submit".to_string(),
                        sends_data: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                ..Default::default()
            },
            companion_bot: BotRegistryEntry {
                bot_user_id: 600_001,
                owner_user_id: 100_001,
                username: "CoffeeOrderBot".to_string(),
                display_name: "Coffee Order AI".to_string(),
                commands: vec![BotCommandSpec {
                    command: "start".to_string(),
                    description: "Open bot".to_string(),
                }],
                ..Default::default()
            },
            companion_chat_id: 600_000_001,
            launch_mode: MiniAppLaunchMode::CompactSheet,
            state_json: serde_json::json!({"cups": 2}),
            ..Default::default()
        };
        let encoded = serde_json::to_string(&ctx).expect("serialize launch context");
        let decoded: MiniAppLaunchContext =
            serde_json::from_str(&encoded).expect("deserialize launch context");
        assert_eq!(ctx, decoded);
    }

    #[test]
    fn test_grok_persona_avatar_deterministic_parity_and_resolution() {
        // UTF-16 FNV-1a parity check for ASCII and surrogate-pair emoji
        assert_eq!(grok_shipped_hash(""), 2_166_136_261);
        assert_eq!(grok_shipped_hash("alpha-agent"), grok_shipped_hash("alpha-agent"));
        assert_ne!(grok_shipped_hash("alpha-agent"), grok_shipped_hash("beta-agent"));
        assert_eq!(grok_shipped_hash("🤖"), {
            let mut h: u32 = 2_166_136_261;
            for cu in [0xD83E_u16, 0xDD16_u16] {
                h = (h ^ u32::from(cu)).wrapping_mul(16_777_619);
            }
            h
        });

        // Deterministic color index in 0..=9 (excludes "black") and shape hash parity
        for seed in ["", "fabushi", "alpha-agent", "coffee_helper_bot", "agent-🚀"] {
            let c_idx = grok_shipped_color_index(seed);
            assert!(c_idx < GROK_AVATAR_FALLBACK_COLOR_IDS.len());
            let fallback_color = resolve_grok_persona_color(seed, None);
            assert_eq!(fallback_color, GROK_AVATAR_FALLBACK_COLOR_IDS[c_idx]);
            assert_ne!(fallback_color, "black");

            let s_hash = grok_shipped_shape_hash(seed);
            let fallback_shape = resolve_grok_persona_shape(seed, None);
            assert_eq!(
                fallback_shape,
                GROK_AVATAR_SHAPES[(s_hash as usize) % GROK_AVATAR_SHAPES.len()]
            );
            assert_eq!(normalize_fabushi_avatar_color("", seed), fallback_color);
            assert_eq!(normalize_fabushi_avatar_shape("   ", seed), fallback_shape);
        }

        // All 11 Grok color IDs accepted (trimmed, case-insensitive)
        for &cid in &GROK_AVATAR_COLOR_IDS {
            assert_eq!(resolve_grok_persona_color("agent", Some(cid)), cid);
            assert_eq!(
                resolve_grok_persona_color("agent", Some(&format!("  {} ", cid.to_ascii_uppercase()))),
                cid
            );
            assert_eq!(normalize_fabushi_avatar_color(cid, "agent"), cid);
        }

        // #RRGGBB preservation (uppercased)
        for hex in ["#10B981", "#ec4899", " #ff6b6b ", "#8B5CF6"] {
            assert_eq!(
                resolve_grok_persona_color("agent", Some(hex)),
                hex.trim().to_ascii_uppercase()
            );
            assert_eq!(
                normalize_fabushi_avatar_color(hex, "agent"),
                hex.trim().to_ascii_uppercase()
            );
        }

        // #RGB expansion to uppercase #RRGGBB
        assert_eq!(resolve_grok_persona_color("agent", Some("#abc")), "#AABBCC");
        assert_eq!(resolve_grok_persona_color("agent", Some(" #0f8 ")), "#00FF88");
        assert_eq!(normalize_fabushi_avatar_color("#3aF", "agent"), "#33AAFF");

        // Invalid colors fall back deterministically
        assert_eq!(
            resolve_grok_persona_color("agent-x", Some("not-a-color")),
            resolve_grok_persona_color("agent-x", None)
        );
        assert_eq!(
            resolve_grok_persona_color("agent-x", Some("#12")),
            resolve_grok_persona_color("agent-x", None)
        );

        // "classic" sentinel and shape aliases
        assert_eq!(
            resolve_grok_persona_shape("agent", Some("classic")),
            CLASSIC_INITIALS_AVATAR_SHAPE
        );
        assert_eq!(
            resolve_grok_persona_shape("agent", Some(" CLASSIC ")),
            CLASSIC_INITIALS_AVATAR_SHAPE
        );
        assert_eq!(normalize_fabushi_avatar_shape("classic", "agent"), "classic");

        assert_eq!(resolve_grok_persona_shape("agent", Some("circle")), "blob");
        assert_eq!(resolve_grok_persona_shape("agent", Some("round")), "blob");
        assert_eq!(resolve_grok_persona_shape("agent", Some("rounded")), "squircle");
        assert_eq!(resolve_grok_persona_shape("agent", Some("square")), "squircle");
        assert_eq!(resolve_grok_persona_shape("agent", Some("box")), "squircle");
        assert_eq!(resolve_grok_persona_shape("agent", Some("hexagon")), "hex");
        assert_eq!(resolve_grok_persona_shape("agent", Some("shield")), "wedge");
        assert_eq!(resolve_grok_persona_shape("agent", Some("diamond")), "wedge");
        assert_eq!(resolve_grok_persona_shape("agent", Some("spark")), "wedge");

        for &shape in &GROK_AVATAR_SHAPES {
            assert_eq!(resolve_grok_persona_shape("agent", Some(shape)), shape);
            assert_eq!(normalize_fabushi_avatar_shape(shape, "agent"), shape);
        }
    }

    #[test]
    fn test_natural_language_create_request_avatar_serde() {
        let legacy: NaturalLanguageCreateRequest =
            serde_json::from_str(r#"{"prompt":"make a todo bot"}"#).expect("legacy json");
        assert_eq!(legacy.prompt, "make a todo bot");
        assert_eq!(legacy.avatar_shape, None);
        assert_eq!(legacy.avatar_color, None);
        let legacy_encoded = serde_json::to_value(&legacy).unwrap();
        assert!(legacy_encoded.get("avatar_shape").is_none());
        assert!(legacy_encoded.get("avatar_color").is_none());

        let with_avatar = NaturalLanguageCreateRequest {
            prompt: "make a coffee bot".to_string(),
            avatar_shape: Some("pebble".to_string()),
            avatar_color: Some("orange".to_string()),
        };
        let roundtrip: NaturalLanguageCreateRequest =
            serde_json::from_str(&serde_json::to_string(&with_avatar).unwrap()).unwrap();
        assert_eq!(roundtrip, with_avatar);
    }

    #[test]
    fn test_miniapp_parity_dto_backward_compatibility_and_roundtrip() {
        // Legacy BotRegistryEntry without any new fields must deserialize cleanly
        let legacy_bot_json = serde_json::json!({
            "bot_user_id": 777001,
            "owner_user_id": 0,
            "username": "BotFather",
            "display_name": "BotFather"
        });
        let bot: BotRegistryEntry =
            serde_json::from_value(legacy_bot_json).expect("legacy BotRegistryEntry");
        assert_eq!(bot.menu_button_type, "");
        assert!(!bot.has_main_app);
        assert_eq!(bot.main_app_short_name, "");
        assert_eq!(bot.privacy_policy_url, "");
        assert!(!bot.has_settings);
        assert!(!bot.can_join_groups);
        assert_eq!(bot.mau_count, 0);
        assert!(!bot.allows_write_to_pm);
        assert!(!bot.app_tos_accepted);
        assert!(!bot.is_blocked_by_viewer);

        // Legacy MiniAppEntry without any new fields must deserialize cleanly
        let legacy_app_json = serde_json::json!({
            "app_id": "app_1",
            "bot_user_id": 800001,
            "bot_username": "SampleBot",
            "bot_display_name": "Sample Bot",
            "short_name": "main",
            "title": "Sample App"
        });
        let app: MiniAppEntry =
            serde_json::from_value(legacy_app_json).expect("legacy MiniAppEntry");
        assert_eq!(app.mau_count, 0);
        assert!(!app.is_main_app);
        assert!(!app.is_recent_used);
        assert_eq!(app.last_launched_at, 0);
        assert_eq!(app.privacy_policy_url, "");
        assert!(!app.app_tos_accepted);
        assert_eq!(app.header_color, None);
        assert_eq!(app.background_color, None);

        // MiniAppLaunchMode::MainMiniApp supports both PascalCase and snake_case alias
        let mode_snake: MiniAppLaunchMode =
            serde_json::from_str(r#""main_mini_app""#).expect("snake_case MainMiniApp");
        assert_eq!(mode_snake, MiniAppLaunchMode::MainMiniApp);
        let mode_roundtrip: MiniAppLaunchMode =
            serde_json::from_str(&serde_json::to_string(&MiniAppLaunchMode::MainMiniApp).unwrap())
                .expect("roundtrip MainMiniApp");
        assert_eq!(mode_roundtrip, MiniAppLaunchMode::MainMiniApp);

        // New DTO roundtrips
        let profile = BotPublicProfileDto {
            bot: BotRegistryEntry {
                bot_user_id: 800001,
                owner_user_id: 100001,
                username: "SampleBot".to_string(),
                display_name: "Sample Bot".to_string(),
                menu_button_type: "web_app".to_string(),
                has_main_app: true,
                main_app_short_name: "main".to_string(),
                privacy_policy_url: "https://example.com/privacy".to_string(),
                has_settings: true,
                can_join_groups: true,
                mau_count: 12_345,
                allows_write_to_pm: true,
                app_tos_accepted: true,
                is_blocked_by_viewer: false,
                ..Default::default()
            },
            main_app: Some(MiniAppEntry {
                app_id: "app_1".to_string(),
                bot_user_id: 800001,
                bot_username: "SampleBot".to_string(),
                bot_display_name: "Sample Bot".to_string(),
                short_name: "main".to_string(),
                title: "Sample App".to_string(),
                mau_count: 12_345,
                is_main_app: true,
                is_recent_used: true,
                last_launched_at: 1_731_600_000,
                privacy_policy_url: "https://example.com/privacy".to_string(),
                app_tos_accepted: true,
                header_color: Some("#2481cc".to_string()),
                background_color: Some("#ffffff".to_string()),
                ..Default::default()
            }),
            apps: vec![app],
            existing_chat_id: Some(900001),
            has_started_chat: true,
            allows_write_to_pm: true,
            app_tos_accepted: true,
            is_blocked: false,
            is_muted: true,
        };
        let profile_rt: BotPublicProfileDto =
            serde_json::from_str(&serde_json::to_string(&profile).unwrap()).unwrap();
        assert_eq!(profile, profile_rt);

        let can_send = CanSendMessageResponse {
            can_send: true,
            allows_write_to_pm: true,
            has_started_chat: false,
            is_blocked: false,
        };
        assert_eq!(
            can_send,
            serde_json::from_str(&serde_json::to_string(&can_send).unwrap()).unwrap()
        );

        let allow_req = AllowSendMessageRequest {
            app_id: Some("app_1".to_string()),
            web_app_name: Some("main".to_string()),
            from_attachment_menu: false,
        };
        assert_eq!(
            allow_req,
            serde_json::from_str(&serde_json::to_string(&allow_req).unwrap()).unwrap()
        );

        let allow_resp = AllowSendMessageResponse {
            ok: true,
            chat_id: 900001,
            service_message_id: Some(42),
            service_text: "您已允许该机器人在其网页应用中向您发送消息。".to_string(),
        };
        assert_eq!(
            allow_resp,
            serde_json::from_str(&serde_json::to_string(&allow_resp).unwrap()).unwrap()
        );

        let tos_req = AcceptMiniAppTosRequest {
            app_id: Some("app_1".to_string()),
            allow_write_access: Some(true),
        };
        assert_eq!(
            tos_req,
            serde_json::from_str(&serde_json::to_string(&tos_req).unwrap()).unwrap()
        );

        let block_req = BlockBotRequest { blocked: true };
        assert_eq!(
            block_req,
            serde_json::from_str(&serde_json::to_string(&block_req).unwrap()).unwrap()
        );
    }

    #[test]
    fn test_agent_manifest_validation_and_execution() {
        let m = AgentManifest {
            app_id: "ops".into(),
            version: "1.0.0".into(),
            readable_state_schema: serde_json::json!({ "type": "object" }),
            actions: vec![MiniAppActionSpec {
                name: "deploy".into(),
                description: "Deploy".into(),
                input_schema: serde_json::json!({ "type": "object", "required": ["env"] }),
                output_schema: serde_json::json!({ "type": "object" }),
                risk_level: "high".into(),
                requires_approval: true,
            }],
        };
        assert!(m.validate_state(&serde_json::json!({})).is_ok());
        assert!(m.validate_state(&serde_json::json!(123)).is_err());
        assert!(m.execute_action("deploy", &serde_json::json!({ "env": "prod" }), false).is_err());
        let ok = m.execute_action("deploy", &serde_json::json!({ "env": "prod" }), true).unwrap();
        assert_eq!(ok["status"], "executed");
    }

    #[test]
    fn test_agent_manifest_strict_schema_validation() {
        // Non-object schemas rejected by from_json
        assert!(AgentManifest::from_json(
            r#"{
                "app_id": "bad",
                "version": "1.0.0",
                "readable_state_schema": "not_an_object",
                "actions": []
            }"#
        )
        .is_err());
        assert!(AgentManifest::from_json(
            r#"{
                "app_id": "bad",
                "version": "1.0.0",
                "readable_state_schema": { "type": "object" },
                "actions": [{
                    "name": "act",
                    "description": "act",
                    "input_schema": 12345,
                    "output_schema": { "type": "object" },
                    "risk_level": "low",
                    "requires_approval": false
                }]
            }"#
        )
        .is_err());

        let m = AgentManifest::from_json(
            r#"{
                "app_id": "pay_bot",
                "version": "1.0.0",
                "readable_state_schema": {
                    "type": "object",
                    "required": ["balance"],
                    "properties": {
                        "balance": { "type": "number" }
                    },
                    "additionalProperties": false
                },
                "actions": [{
                    "name": "transfer",
                    "description": "Transfer funds",
                    "input_schema": {
                        "type": "object",
                        "required": ["amount", "recipient"],
                        "properties": {
                            "amount": { "type": "number" },
                            "recipient": { "type": "string" }
                        },
                        "additionalProperties": false
                    },
                    "output_schema": { "type": "object" },
                    "risk_level": "low",
                    "requires_approval": false
                }]
            }"#,
        )
        .unwrap();

        assert!(m.validate_state(&serde_json::json!({})).is_err());
        assert!(m.validate_state(&serde_json::json!({ "balance": "nan" })).is_err());
        assert!(m.validate_state(&serde_json::json!({ "balance": 100.5 })).is_ok());

        assert!(m.validate_action_call("transfer", &serde_json::json!(12345)).is_err());
        assert!(m
            .validate_action_call(
                "transfer",
                &serde_json::json!({ "amount": "not_a_num", "recipient": [1, 2] })
            )
            .is_err());
        assert!(m
            .validate_action_call(
                "transfer",
                &serde_json::json!({ "amount": null, "recipient": "alice" })
            )
            .is_err());
        assert!(m
            .execute_action(
                "transfer",
                &serde_json::json!({ "amount": 10, "recipient": "alice", "admin_bypass": true }),
                false
            )
            .is_err());
        assert!(m
            .execute_action(
                "transfer",
                &serde_json::json!({ "amount": 10, "recipient": "alice" }),
                false
            )
            .is_ok());
    }
}
