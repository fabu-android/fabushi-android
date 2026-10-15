use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use fabushi_android_local_exec_daemon::{
    bounded_file_handlers::{
        LOCAL_FILE_UPLOAD_CHUNK_CAPABILITY, LOCAL_FILE_UPLOAD_COMMIT_CAPABILITY,
        LOCAL_FILE_UPLOAD_START_CAPABILITY, MAX_LOCAL_UPLOAD_CHUNK_BYTES,
    },
    AndroidLocalRunner,
};
use fabushi_android_shared::ExecutionRequest;
use serde_json::{json, Value};
use std::{env, sync::Mutex};

use crate::sha256::sha256_hex;

pub const MCP_TEXT_FILE_THRESHOLD_BYTES: usize = 40_000;
pub const MAX_OUTPUT_FILE_SIZE: usize = 1_000_000;
pub const AGENT_TOOLS_DIR: &str = ".sand/tools";
const SPILL_EXEC_TIMEOUT_MS: u64 = 10_000;

pub fn is_large_output_spill_enabled_from(value: Option<&str>) -> bool {
    value != Some("1")
}

pub fn is_large_output_spill_enabled() -> bool {
    is_large_output_spill_enabled_from(
        env::var("SAND_DISABLE_LARGE_OUTPUT_SPILL").ok().as_deref(),
    )
}

fn is_inline_text(item: &Value) -> bool {
    item.get("type").and_then(Value::as_str) == Some("text")
        && item.get("text").and_then(Value::as_str).is_some()
        && item.get("outputLocation").is_none()
}

fn cap_text(text: &str) -> String {
    if text.chars().count() <= MAX_OUTPUT_FILE_SIZE {
        return text.to_string();
    }
    text.chars().take(MAX_OUTPUT_FILE_SIZE).collect()
}

fn execute(
    runner: &Mutex<AndroidLocalRunner>,
    operation_id: String,
    capability_id: &str,
    input: Value,
) -> Result<(), String> {
    let request = ExecutionRequest {
        operation_id,
        capability_id: capability_id.to_string(),
        input_json: serde_json::to_string(&input)
            .map_err(|error| format!("large-output spill input encoding failed: {error}"))?,
        timeout_ms: SPILL_EXEC_TIMEOUT_MS,
    };
    runner
        .lock()
        .map_err(|_| "Android Local Runner lock poisoned during large-output spill".to_string())?
        .execute(request)
        .map_err(|error| format!("large-output spill Local Runner failed: {error:?}"))?
        .validate()
        .map_err(|error| format!("large-output spill Local Runner result invalid: {error:?}"))
}

fn spill_text(
    runner: &Mutex<AndroidLocalRunner>,
    text: &str,
    tool_call_id: &str,
) -> Result<String, String> {
    let bytes = text.as_bytes();
    let digest = sha256_hex(bytes);
    let identity = sha256_hex(format!("{tool_call_id}\n{digest}").as_bytes());
    let path = format!("{AGENT_TOOLS_DIR}/{identity}.txt");
    let transfer_id = format!("spill-{identity}");
    let prefix = &identity[..24];
    execute(
        runner,
        format!("spill-start-{prefix}"),
        LOCAL_FILE_UPLOAD_START_CAPABILITY,
        json!({
            "transferId": transfer_id,
            "path": path,
            "totalBytes": bytes.len(),
            "sha256": digest,
            "expectedVersion": Value::Null,
            "overwrite": true,
        }),
    )?;
    for (index, chunk) in bytes.chunks(MAX_LOCAL_UPLOAD_CHUNK_BYTES).enumerate() {
        let offset = index.saturating_mul(MAX_LOCAL_UPLOAD_CHUNK_BYTES);
        execute(
            runner,
            format!("spill-chunk-{prefix}-{index}"),
            LOCAL_FILE_UPLOAD_CHUNK_CAPABILITY,
            json!({
                "transferId": transfer_id,
                "path": path,
                "offset": offset,
                "bytesBase64": BASE64.encode(chunk),
            }),
        )?;
    }
    execute(
        runner,
        format!("spill-commit-{prefix}"),
        LOCAL_FILE_UPLOAD_COMMIT_CAPABILITY,
        json!({"transferId": transfer_id, "path": path}),
    )?;
    Ok(path)
}

/// Materialize oversized MCP-style inline text into the same bounded Android
/// Local Runner filesystem exposed by Read. Failure is deliberately non-fatal:
/// the original inline result is returned unchanged.
pub fn maybe_spill_mcp_text_result(
    runner: &Mutex<AndroidLocalRunner>,
    result: Value,
    tool_call_id: &str,
    threshold_bytes: usize,
) -> Value {
    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        return result;
    }
    let Some(content) = result.get("content").and_then(Value::as_array) else {
        return result;
    };
    let inline = content
        .iter()
        .filter(|item| is_inline_text(item))
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>();
    if inline.is_empty() {
        return result;
    }
    let aggregate = inline.join("\n\n");
    if threshold_bytes == 0 || aggregate.len() <= threshold_bytes {
        return result;
    }

    let capped = cap_text(&aggregate);
    let Ok(path) = spill_text(runner, &capped, tool_call_id) else {
        return result;
    };
    let output_location = json!({
        "filePath": path,
        "sizeBytes": capped.len(),
        "lineCount": capped.split('\n').count(),
    });
    let mut materialized = Vec::with_capacity(content.len());
    let mut emitted = false;
    for item in content {
        if is_inline_text(item) {
            if !emitted {
                materialized.push(json!({
                    "type":"text",
                    "text":"",
                    "outputLocation":output_location,
                }));
                emitted = true;
            }
        } else {
            materialized.push(item.clone());
        }
    }
    let mut spilled = result;
    if let Some(object) = spilled.as_object_mut() {
        object.insert("content".into(), Value::Array(materialized));
    }
    spilled
}

#[cfg(test)]
mod tests {
    use super::*;
    use fabushi_android_local_exec_daemon::bounded_file_handlers::bounded_file_runner;

    #[test]
    fn oversized_text_spills_to_readable_local_runner_file() {
        let root = std::env::temp_dir().join(format!(
            "fabushi-large-spill-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let runner = Mutex::new(bounded_file_runner(root.clone()).unwrap());
        let original = "x".repeat(MCP_TEXT_FILE_THRESHOLD_BYTES + 1);
        let result = maybe_spill_mcp_text_result(
            &runner,
            json!({"content":[{"type":"text","text":original}]}),
            "tool-call-1",
            MCP_TEXT_FILE_THRESHOLD_BYTES,
        );
        let location = result["content"][0]["outputLocation"]["filePath"]
            .as_str()
            .unwrap();
        assert!(location.starts_with(".sand/tools/"));
        assert_eq!(std::fs::read_to_string(root.join(location)).unwrap().len(), MCP_TEXT_FILE_THRESHOLD_BYTES + 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn errors_small_results_and_disabled_flag_stay_inline() {
        assert!(!is_large_output_spill_enabled_from(Some("1")));
        assert!(is_large_output_spill_enabled_from(None));
        let root = std::env::temp_dir().join("fabushi-large-spill-inline");
        let runner = Mutex::new(bounded_file_runner(root).unwrap());
        let small = json!({"content":[{"type":"text","text":"small"}]});
        assert_eq!(maybe_spill_mcp_text_result(&runner, small.clone(), "call", 40_000), small);
        let error = json!({"isError":true,"content":[{"type":"text","text":"x".repeat(50_000)}]});
        assert_eq!(maybe_spill_mcp_text_result(&runner, error.clone(), "call", 40_000), error);
    }
}
