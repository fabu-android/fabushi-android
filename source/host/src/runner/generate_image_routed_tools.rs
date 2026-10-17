use super::AndroidRoutedToolBridge;
use crate::mcp_auth::dashboard_mcp_auth_backend::{
    CursorDashboardMcpAuthBackend, GenerateImageBackendError, GeneratedImageBackendResult,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex},
};

pub const GENERATE_IMAGE_TOOL_NAME: &str = "GenerateImage";
const SHORT_PROMPT_WORD_THRESHOLD: usize = 5;
const VERY_SHORT_PROMPT_WORD_THRESHOLD: usize = 3;
const SUSPICIOUS_KEYWORDS: &[&str] = &["ignore", "skip", "stop", "noop"];
const SUPPORTED_ASPECT_RATIOS: &[&str] = &["1:1", "4:3", "3:4", "16:9", "9:16"];
const MAX_REFERENCE_IMAGES: usize = 8;
const MAX_REFERENCE_IMAGE_BYTES: usize = 20 * 1024 * 1024;
const MAX_TOTAL_REFERENCE_BYTES: usize = 64 * 1024 * 1024;
const MAX_GENERATED_IMAGE_BYTES: usize = 32 * 1024 * 1024;

trait GenerateImageBackend: Send + Sync {
    fn generate(&self, description: &str, references: &[(String, String)])
        -> Result<GeneratedImageBackendResult, GenerateImageBackendError>;
}
impl GenerateImageBackend for CursorDashboardMcpAuthBackend {
    fn generate(&self, description: &str, references: &[(String, String)])
        -> Result<GeneratedImageBackendResult, GenerateImageBackendError> {
        self.generate_image(description, references)
    }
}

struct GenerateImageRoutedTools {
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    backend: Arc<dyn GenerateImageBackend>,
    private_root: PathBuf,
    account_fence: String,
    live_account_fence: Arc<Mutex<Option<String>>>,
    cancelled: Arc<AtomicBool>,
}

impl GenerateImageRoutedTools {
    fn assert_live(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err("GenerateImage tool call cancelled".into());
        }
        let live = self.live_account_fence.lock()
            .map_err(|_| "GenerateImage account fence lock poisoned".to_string())?;
        if live.as_deref() != Some(self.account_fence.as_str()) {
            return Err("GenerateImage result belongs to a stale account epoch".into());
        }
        Ok(())
    }

    fn reference_images(&self, args: &Value) -> Result<Vec<(String, String)>, String> {
        let Some(raw_values) = args.get("reference_image_paths") else { return Ok(Vec::new()); };
        let values = raw_values.as_array()
            .ok_or("GenerateImage reference_image_paths must be an array")?;
        if values.len() > MAX_REFERENCE_IMAGES {
            return Err(format!("GenerateImage supports at most {MAX_REFERENCE_IMAGES} reference images"));
        }
        fs::create_dir_all(&self.private_root)
            .map_err(|error| format!("GenerateImage private root unavailable: {error}"))?;
        let root = fs::canonicalize(&self.private_root)
            .map_err(|error| format!("GenerateImage private root unresolved: {error}"))?;
        let mut total = 0usize;
        let mut out = Vec::with_capacity(values.len());
        for value in values {
            let raw = value.as_str().map(str::trim).filter(|value| !value.is_empty())
                .ok_or("GenerateImage reference path must be a non-empty string")?;
            let requested = Path::new(raw);
            let candidate = if requested.is_absolute() { requested.to_path_buf() } else { root.join(requested) };
            let canonical = fs::canonicalize(candidate)
                .map_err(|_| "GenerateImage reference image is unavailable")?;
            if !canonical.starts_with(&root) {
                return Err("GenerateImage reference image escaped the app-private root".into());
            }
            let metadata = fs::metadata(&canonical)
                .map_err(|_| "GenerateImage reference image metadata unavailable")?;
            let len = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
            if !metadata.is_file() || len == 0 || len > MAX_REFERENCE_IMAGE_BYTES {
                return Err("GenerateImage reference image violates bounded file policy".into());
            }
            total = total.saturating_add(len);
            if total > MAX_TOTAL_REFERENCE_BYTES {
                return Err("GenerateImage reference images exceed total byte bound".into());
            }
            let bytes = fs::read(&canonical)
                .map_err(|_| "GenerateImage reference image could not be read")?;
            out.push((BASE64.encode(bytes), image_mime_from_path(&canonical).to_string()));
        }
        Ok(out)
    }

    fn persist(&self, generated: &GeneratedImageBackendResult) -> Result<PathBuf, String> {
        let bytes = BASE64.decode(generated.image_data.as_bytes())
            .map_err(|_| "GenerateImage returned invalid base64 image data".to_string())?;
        if bytes.is_empty() || bytes.len() > MAX_GENERATED_IMAGE_BYTES {
            return Err("GenerateImage returned image data outside bounded size policy".into());
        }
        let extension = extension_for_mime(&generated.mime_type)?;
        let dir = self.private_root.join("generated-images");
        fs::create_dir_all(&dir)
            .map_err(|error| format!("GenerateImage output directory unavailable: {error}"))?;
        let digest = crate::sha256::sha256_hex(&bytes);
        let output = dir.join(format!("generated-{digest}.{extension}"));
        if !output.exists() {
            let temp = dir.join(format!(".generated-{digest}.tmp"));
            fs::write(&temp, &bytes)
                .map_err(|error| format!("GenerateImage temp write failed: {error}"))?;
            fs::rename(&temp, &output)
                .map_err(|error| format!("GenerateImage atomic commit failed: {error}"))?;
        }
        Ok(output)
    }
}

impl AndroidRoutedToolBridge for GenerateImageRoutedTools {
    fn list_tools(&self) -> Result<Vec<Value>, String> {
        let mut tools = self.delegate.list_tools()?;
        tools.retain(|tool| tool.get("name").and_then(Value::as_str) != Some(GENERATE_IMAGE_TOOL_NAME));
        tools.push(tool_definition());
        Ok(tools)
    }

    fn call_tool(&self, name: &str, args: Value, tool_call_id: &str) -> Result<Value, String> {
        if name != GENERATE_IMAGE_TOOL_NAME {
            return self.delegate.call_tool(name, args, tool_call_id);
        }
        self.assert_live()?;
        if tool_call_id.trim().is_empty() { return Err("GenerateImage tool_call_id is required".into()); }
        let object = args.as_object().ok_or("GenerateImage arguments must be an object")?;
        if let Some(key) = object.keys().find(|key| !matches!(
            key.as_str(), "description" | "filename" | "reference_image_paths" | "aspect_ratio"
        )) {
            return Err(format!("GenerateImage received unsupported argument {key}"));
        }
        let description = object.get("description").and_then(Value::as_str).map(str::trim)
            .filter(|value| !value.is_empty()).ok_or("GenerateImage requires description")?;
        validate_prompt(description)?;
        if let Some(ratio) = object.get("aspect_ratio").and_then(Value::as_str) {
            if !SUPPORTED_ASPECT_RATIOS.contains(&ratio) {
                return Err(format!(
                    "GenerateImage aspect_ratio must be one of {}",
                    SUPPORTED_ASPECT_RATIOS.join(", ")
                ));
            }
        }
        let references = self.reference_images(&args)?;
        self.assert_live()?;
        let generated = self.backend.generate(description, &references)
            .map_err(|error| error.to_string())?;
        self.assert_live()?;
        let output = self.persist(&generated)?;
        self.assert_live()?;
        Ok(json!({"success":{"filePath":output.to_string_lossy(),"imageData":generated.image_data}}))
    }
}

pub(crate) fn with_generate_image_routed_tools(
    delegate: Arc<dyn AndroidRoutedToolBridge>,
    backend: Arc<CursorDashboardMcpAuthBackend>,
    private_root: PathBuf,
    account_fence: &str,
    live_account_fence: Arc<Mutex<Option<String>>>,
    cancelled: Arc<AtomicBool>,
) -> Arc<dyn AndroidRoutedToolBridge> {
    Arc::new(GenerateImageRoutedTools {
        delegate, backend, private_root, account_fence: account_fence.to_string(),
        live_account_fence, cancelled,
    })
}

fn tool_definition() -> Value {
    json!({
        "type":"function","name":GENERATE_IMAGE_TOOL_NAME,
        "description":"Generate an image from a detailed description and save it in app-private assets.",
        "parameters":{"type":"object","additionalProperties":false,"required":["description"],"properties":{
            "description":{"type":"string"},
            "filename":{"type":"string"},
            "reference_image_paths":{"type":"array","items":{"type":"string"}},
            "aspect_ratio":{"type":"string","enum":SUPPORTED_ASPECT_RATIOS}
        }}
    })
}

fn validate_prompt(description: &str) -> Result<(), String> {
    let words = description.split_whitespace().count();
    let lower = description.to_ascii_lowercase();
    if words <= VERY_SHORT_PROMPT_WORD_THRESHOLD
        || (words <= SHORT_PROMPT_WORD_THRESHOLD && SUSPICIOUS_KEYWORDS.iter().any(|keyword| lower.contains(keyword))) {
        return Err("Image generation was not performed — the tool call appears unintended. Do not retry unless the user explicitly asks for an image.".into());
    }
    Ok(())
}

fn image_mime_from_path(path: &Path) -> &'static str {
    match path.extension().and_then(|value| value.to_str()).unwrap_or_default().to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg", "gif" => "image/gif", "webp" => "image/webp", _ => "image/png",
    }
}
fn extension_for_mime(mime: &str) -> Result<&'static str, String> {
    match mime.trim().to_ascii_lowercase().as_str() {
        "image/png" => Ok("png"), "image/jpeg" | "image/jpg" => Ok("jpg"),
        "image/gif" => Ok("gif"), "image/webp" => Ok("webp"),
        _ => Err("GenerateImage returned unsupported image MIME type".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Empty;
    impl AndroidRoutedToolBridge for Empty {
        fn list_tools(&self)->Result<Vec<Value>,String>{Ok(Vec::new())}
        fn call_tool(&self,name:&str,_:Value,_:&str)->Result<Value,String>{Err(format!("unexpected {name}"))}
    }
    struct Fake;
    impl GenerateImageBackend for Fake {
        fn generate(&self,_:&str,_:&[(String,String)])->Result<GeneratedImageBackendResult,GenerateImageBackendError>{
            Ok(GeneratedImageBackendResult{image_data:BASE64.encode(b"png"),mime_type:"image/png".into()})
        }
    }
    #[test]
    fn generated_asset_is_private_content_addressed_and_stale_calls_fail_closed() {
        let root=std::env::temp_dir().join(format!("fabushi-generate-image-{}",std::process::id()));
        let _=fs::remove_dir_all(&root);
        let live=Arc::new(Mutex::new(Some("acct:a".into())));
        let bridge=GenerateImageRoutedTools{delegate:Arc::new(Empty),backend:Arc::new(Fake),private_root:root.clone(),
            account_fence:"acct:a".into(),live_account_fence:Arc::clone(&live),cancelled:Arc::new(AtomicBool::new(false))};
        assert!(bridge.list_tools().unwrap().iter().any(|tool|tool["name"]==GENERATE_IMAGE_TOOL_NAME));
        let result=bridge.call_tool(GENERATE_IMAGE_TOOL_NAME,json!({"description":"Create a calm mountain landscape at sunrise"}),"call-1").unwrap();
        let path=PathBuf::from(result["success"]["filePath"].as_str().unwrap());
        assert!(path.starts_with(root.join("generated-images")));
        *live.lock().unwrap()=Some("acct:b".into());
        assert!(bridge.call_tool(GENERATE_IMAGE_TOOL_NAME,json!({"description":"Create a detailed geometric poster design"}),"call-2").is_err());
        let _=fs::remove_dir_all(root);
    }
}
