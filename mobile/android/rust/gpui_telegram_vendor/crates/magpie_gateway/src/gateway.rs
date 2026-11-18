use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
use futures::stream;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::watch;

use crate::masking::{MaskingConfig, MaskingEngine, StreamingUnmasker};
use crate::presets::{find_preset, resolve_preset_endpoints, ProtocolKind};
use crate::protocol::*;
use crate::routing::{
    parse_retry_after_header, resolve_group_targets, RestState, RouteDecisionTrace,
    RouteRuntimeMetrics, RouteSeatMetrics, RoutingGroup,
};
use crate::subscriptions::SubscriptionRegistry;
use crate::usage::{normalize_session_header, UsageLedger, UsageRecord, UsageSummary};

pub type CustomExecutionHandler = Arc<
    dyn Fn(
            UnifiedRequest,
            Option<String>,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<UnifiedResponse, String>> + Send>>
        + Send
        + Sync,
>;

/// Shared state for the embedded proxy gateway HTTP/SSE service.
#[derive(Clone)]
pub struct GatewayState {
    pub groups: Arc<RwLock<HashMap<String, RoutingGroup>>>,
    pub metrics: Arc<RwLock<RouteRuntimeMetrics>>,
    pub ledger: Arc<UsageLedger>,
    pub masking: Option<MaskingConfig>,
    pub subscriptions: Arc<RwLock<SubscriptionRegistry>>,
    pub provider_endpoints: Arc<RwLock<HashMap<String, (String, ProtocolKind, Option<String>)>>>,
    pub http_client: reqwest::Client,
    pub default_provider_id: String,
    pub custom_handler: Option<CustomExecutionHandler>,
}

impl GatewayState {
    pub fn new(default_provider_id: impl Into<String>) -> Self {
        Self {
            groups: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(RwLock::new(RouteRuntimeMetrics::default())),
            ledger: Arc::new(UsageLedger::default()),
            masking: None,
            subscriptions: Arc::new(RwLock::new(SubscriptionRegistry::default())),
            provider_endpoints: Arc::new(RwLock::new(HashMap::new())),
            http_client: reqwest::Client::new(),
            default_provider_id: default_provider_id.into(),
            custom_handler: None,
        }
    }

    pub fn with_groups(mut self, groups: HashMap<String, RoutingGroup>) -> Self {
        self.groups = Arc::new(RwLock::new(groups));
        self
    }

    pub fn with_masking_config(mut self, masking: MaskingConfig) -> Self {
        self.masking = Some(masking);
        self
    }

    /// Configure masking from a `MaskingEngine` (extracts config only; each request instantiates an isolated vault).
    pub fn with_masking(mut self, masking: Arc<MaskingEngine>) -> Self {
        self.masking = Some(masking.config().clone());
        self
    }

    pub fn with_provider_endpoint(
        self,
        provider_id: impl Into<String>,
        base_url: impl Into<String>,
        protocol: ProtocolKind,
        api_key: Option<String>,
    ) -> Self {
        self.provider_endpoints.write().insert(
            provider_id.into(),
            (base_url.into(), protocol, api_key),
        );
        self
    }

    pub fn with_subscriptions(mut self, reg: SubscriptionRegistry) -> Self {
        self.subscriptions = Arc::new(RwLock::new(reg));
        self
    }

    pub fn with_ledger(mut self, ledger: Arc<UsageLedger>) -> Self {
        self.ledger = ledger;
        self
    }

    pub fn with_custom_handler(mut self, handler: CustomExecutionHandler) -> Self {
        self.custom_handler = Some(handler);
        self
    }
}

/// Create an Axum router serving proxy gateway endpoints.
pub fn create_gateway_router(state: GatewayState) -> Router {
    Router::new()
        .route("/v1/chat/completions", post(handle_chat_completions))
        .route("/chat/completions", post(handle_chat_completions))
        .route("/v1/models", get(handle_models))
        .route("/models", get(handle_models))
        .route("/v1/responses", post(handle_responses))
        .route("/responses", post(handle_responses))
        .route("/v1/messages", post(handle_messages))
        .route("/messages", post(handle_messages))
        .route("/v1/magpie/quotas", get(handle_quotas))
        .route("/magpie/quotas", get(handle_quotas))
        .route("/v1/magpie/usage", get(handle_usage))
        .route("/magpie/usage", get(handle_usage))
        .with_state(state)
}

fn error_json(status: StatusCode, message: impl Into<String>, code: &str) -> Response {
    let body = json!({
        "error": {
            "message": message.into(),
            "type": "gateway_error",
            "code": code,
        }
    });
    (status, Json(body)).into_response()
}

#[derive(Debug, Clone)]
struct UpstreamExecError {
    status: Option<u16>,
    retry_after_secs: Option<u64>,
    message: String,
}

impl UpstreamExecError {
    fn is_failover_eligible(&self) -> bool {
        match self.status {
            None => true,
            Some(code) => code == 429 || code >= 500 || code == 408 || code == 401 || code == 402 || code == 403,
        }
    }
}

fn parse_error_status_and_retry_after(msg: &str) -> (Option<u16>, Option<u64>) {
    let mut found_status = None;
    let lower = msg.to_ascii_lowercase();
    for marker in ["http ", "status=", "status: ", "status "] {
        if let Some(idx) = lower.find(marker) {
            let rest = msg[idx + marker.len()..].trim_start();
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.len() == 3 {
                if let Ok(code) = digits.parse::<u16>() {
                    if (400..=599).contains(&code) {
                        found_status = Some(code);
                        break;
                    }
                }
            }
        }
    }
    if found_status.is_none() {
        for tok in msg.split(|c: char| !c.is_ascii_digit()) {
            if tok.len() == 3 {
                if let Ok(code) = tok.parse::<u16>() {
                    if matches!(code, 400 | 401 | 402 | 403 | 404 | 408 | 429 | 500 | 502 | 503 | 504 | 529) {
                        found_status = Some(code);
                        break;
                    }
                }
            }
        }
    }

    let mut retry_after = None;
    if let Some(idx) = lower.find("retry-after:") {
        let rest = msg[idx + "retry-after:".len()..].trim_start();
        let val_str: String = rest
            .chars()
            .take_while(|&c| c != ')' && c != ',' && c != ';' && c != '\n')
            .collect();
        retry_after = parse_retry_after_header(val_str.trim());
    }

    (found_status, retry_after)
}

enum UpstreamOutcome {
    Buffered(UnifiedResponse),
    Sse {
        response: reqwest::Response,
        provider_id: String,
        model: String,
        proto: ProtocolKind,
    },
}

fn unmask_unified_response(resp: &mut UnifiedResponse, masking: Option<&MaskingEngine>) {
    let Some(engine) = masking else {
        return;
    };
    resp.content = engine.unmask_text(&resp.content);
    if let Some(ref mut reason) = resp.reasoning {
        *reason = engine.unmask_text(reason);
    }
    for tc in &mut resp.tool_calls {
        tc.arguments = engine.unmask_text(&tc.arguments);
    }
}

fn mask_unified_request(
    state: &GatewayState,
    unified_req: &mut UnifiedRequest,
) -> Result<(Option<Arc<MaskingEngine>>, usize), Response> {
    // Instantiate isolated per-request MaskingEngine so vaults never leak across requests/tenants
    let req_masking = state
        .masking
        .as_ref()
        .filter(|cfg| cfg.enabled)
        .map(|cfg| Arc::new(MaskingEngine::new(cfg.clone())));

    let mut masking_hits = 0usize;
    if let Some(ref masking) = req_masking {
        if !unified_req.system_prompt.is_empty() {
            let res = masking.mask_text(&unified_req.system_prompt);
            if res.blocked {
                return Err(error_json(
                    StatusCode::FORBIDDEN,
                    res.block_reason
                        .unwrap_or_else(|| "Prompt blocked by masking policy".to_string()),
                    "content_blocked",
                ));
            }
            masking_hits += res.hits.len();
            unified_req.system_prompt = res.masked_text;
        }
        for msg in &mut unified_req.messages {
            let res = masking.mask_text(&msg.content);
            if res.blocked {
                return Err(error_json(
                    StatusCode::FORBIDDEN,
                    res.block_reason
                        .unwrap_or_else(|| "Message blocked by masking policy".to_string()),
                    "content_blocked",
                ));
            }
            masking_hits += res.hits.len();
            msg.content = res.masked_text;

            if let Some(ref mut reason) = msg.reasoning {
                let res = masking.mask_text(reason);
                if res.blocked {
                    return Err(error_json(
                        StatusCode::FORBIDDEN,
                        res.block_reason
                            .unwrap_or_else(|| "Message blocked by masking policy".to_string()),
                        "content_blocked",
                    ));
                }
                masking_hits += res.hits.len();
                *reason = res.masked_text;
            }

            for tc in &mut msg.tool_calls {
                if !tc.arguments.is_empty() {
                    let res = masking.mask_text(&tc.arguments);
                    if res.blocked {
                        return Err(error_json(
                            StatusCode::FORBIDDEN,
                            res.block_reason
                                .unwrap_or_else(|| "Message blocked by masking policy".to_string()),
                            "content_blocked",
                        ));
                    }
                    masking_hits += res.hits.len();
                    tc.arguments = res.masked_text;
                }
            }
        }
    }

    Ok((req_masking, masking_hits))
}

fn record_successful_completion(
    state: &GatewayState,
    session_id: String,
    target: &crate::routing::ResolvedRouteTarget,
    requested_model: &str,
    usage: &UnifiedUsage,
    ttft_ms: Option<u64>,
    elapsed_ms: u64,
    candidates_considered: usize,
    masking_hits: usize,
    now_ms: i64,
) {
    let seat_key = format!("{}/{}", target.provider_id, target.model);
    let record = UsageRecord::new(
        format!("use_{}", uuid::Uuid::new_v4()),
        session_id,
        target.provider_id.clone(),
        target.model.clone(),
        usage.input_tokens,
        usage.output_tokens,
        usage.cache_read_tokens,
        0,
        ttft_ms.or(Some(elapsed_ms)),
        Some(elapsed_ms),
        now_ms,
    );
    let _ = state.ledger.record(record.clone());

    let mut metrics = state.metrics.write();
    metrics.record_metrics(
        &seat_key,
        usage.total_tokens,
        Some(elapsed_ms),
        ttft_ms.or(Some(elapsed_ms)),
        record.tps,
        now_ms,
    );
    metrics.push_trace(RouteDecisionTrace {
        timestamp_ms: now_ms,
        requested_model: requested_model.to_string(),
        resolved_provider_id: target.provider_id.clone(),
        resolved_model: target.model.clone(),
        strategy: target.strategy,
        reason: target.reason.clone(),
        candidates_considered,
        masking_hits,
    });
}

fn format_sse_event_from_chunk(chunk_id: &str, model: &str, chunk: &UnifiedStreamChunk) -> Event {
    let sse_line = encode_openai_chat_chunk(chunk_id, model, chunk);
    let json_data = sse_line
        .trim()
        .strip_prefix("data:")
        .map(str::trim)
        .unwrap_or("{}");
    Event::default().data(json_data)
}

/// Handle `/v1/chat/completions` supporting both synchronous JSON and streaming SSE.
pub async fn handle_chat_completions(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let session_id = normalize_session_header(&headers)
        .unwrap_or_else(|| format!("sess_{}", uuid::Uuid::new_v4()));

    let mut unified_req = match inbound_chat_completions_to_unified(&payload) {
        Ok(req) => req,
        Err(e) => return error_json(StatusCode::BAD_REQUEST, e.to_string(), "invalid_request"),
    };

    let (req_masking, masking_hits) = match mask_unified_request(&state, &mut unified_req) {
        Ok(pair) => pair,
        Err(resp) => return resp,
    };

    let now_ms = chrono::Utc::now().timestamp_millis();
    let prompt_preview = unified_req
        .messages
        .last()
        .map(|m| m.content.as_str())
        .unwrap_or("");

    // Resolve routing group or direct provider/model
    let candidates = {
        let groups = state.groups.read();
        let mut metrics = state.metrics.write();
        let is_group = unified_req.model.starts_with("group/")
            || groups.contains_key(unified_req.model.trim());
        if is_group {
            match resolve_group_targets(
                &unified_req.model,
                &groups,
                &mut metrics,
                &state.default_provider_id,
                Some(&session_id),
                prompt_preview,
                unified_req.messages.len(),
                unified_req.tools.len(),
                now_ms,
            ) {
                Ok(targets) => targets,
                Err(e) => {
                    return error_json(StatusCode::NOT_FOUND, e.to_string(), "routing_group_error");
                }
            }
        } else {
            let (prov, m) = crate::routing::split_target_spec(
                &unified_req.model,
                &state.default_provider_id,
            );
            vec![crate::routing::ResolvedRouteTarget {
                provider_id: prov,
                model: m,
                strategy: crate::routing::RoutingStrategy::Order,
                reason: "direct_model".to_string(),
            }]
        }
    };

    // Filter to non-cooled candidates and iterate with failover on 429/5xx
    let active_candidates: Vec<_> = {
        let m = state.metrics.read();
        candidates
            .into_iter()
            .filter(|c| {
                let target_key = format!("{}/{}", c.provider_id, c.model);
                let cooled = m.seats.get(&target_key).is_some_and(|s| s.is_in_cooldown(now_ms))
                    || m.circuit_breaker.is_in_rest(&target_key, now_ms);
                !cooled
            })
            .collect()
    };

    if active_candidates.is_empty() {
        return error_json(
            StatusCode::SERVICE_UNAVAILABLE,
            "All candidate providers/seats are currently in cooldown or rest",
            "circuit_open",
        );
    }

    let candidates_considered = active_candidates.len();
    let requested_model_str = payload["model"].as_str().unwrap_or("").to_string();
    let mut last_err: Option<UpstreamExecError> = None;

    for target in active_candidates {
        let mut attempt_req = unified_req.clone();
        attempt_req.model = target.model.clone();
        let start_instant = std::time::Instant::now();

        let outcome_res = if let Some(ref handler) = state.custom_handler {
            match handler(attempt_req.clone(), Some(session_id.clone())).await {
                Ok(resp) => Ok(UpstreamOutcome::Buffered(resp)),
                Err(err_msg) => {
                    let (status, retry_after_secs) = parse_error_status_and_retry_after(&err_msg);
                    Err(UpstreamExecError {
                        status,
                        retry_after_secs,
                        message: err_msg,
                    })
                }
            }
        } else {
            start_upstream_request(&state, &target.provider_id, &attempt_req).await
        };

        match outcome_res {
            Ok(UpstreamOutcome::Buffered(mut resp)) => {
                let elapsed_ms = start_instant.elapsed().as_millis() as u64;
                unmask_unified_response(&mut resp, req_masking.as_deref());
                record_successful_completion(
                    &state,
                    session_id,
                    &target,
                    &requested_model_str,
                    &resp.usage,
                    Some(elapsed_ms),
                    elapsed_ms,
                    candidates_considered,
                    masking_hits,
                    now_ms,
                );

                if unified_req.stream {
                    let chunk_id = format!("chatcmpl-{}", uuid::Uuid::new_v4());
                    let model_name = resp.model.clone();
                    let stream_chunk = UnifiedStreamChunk {
                        text_delta: resp.content,
                        reasoning_delta: resp.reasoning,
                        tool_calls: resp.tool_calls,
                        finish_reason: Some(resp.finish_reason),
                        usage: Some(resp.usage),
                    };

                    let events = vec![
                        Ok::<_, Infallible>(format_sse_event_from_chunk(&chunk_id, &model_name, &stream_chunk)),
                        Ok::<_, Infallible>(Event::default().data("[DONE]")),
                    ];
                    return Sse::new(stream::iter(events))
                        .keep_alive(KeepAlive::default())
                        .into_response();
                } else {
                    let out_val = unified_to_chat_completions_json(&resp);
                    return Json(out_val).into_response();
                }
            }
            Ok(UpstreamOutcome::Sse {
                response,
                provider_id: _prov_id,
                model: stream_model,
                proto,
            }) => {
                let chunk_id = format!("chatcmpl-{}", uuid::Uuid::new_v4());
                let state_clone = state.clone();
                let target_clone = target.clone();
                let session_clone = session_id.clone();
                let req_model_clone = requested_model_str.clone();
                let masking_clone = req_masking.clone();

                let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(64);
                let task = tokio::spawn(async move {
                    let mut resp = response;
                    let mut raw_bytes = Vec::<u8>::new();
                    let mut current_event = String::new();
                    let mut text_unmasker = StreamingUnmasker::new();
                    let mut think_unmasker = StreamingUnmasker::new();
                    let mut total_usage = UnifiedUsage::default();
                    let mut ttft_ms: Option<u64> = None;
                    let mut call_counter = 0usize;
                    let mut sent_done = false;
                    let mut finished_ok = true;
                    let mut stream_err: Option<(u16, String)> = None;

                    let mut process_line = |line: &str| -> Result<Option<UnifiedStreamChunk>, ProtocolError> {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            current_event.clear();
                            return Ok(None);
                        }
                        if trimmed.starts_with(':')
                            || trimmed.starts_with("id:")
                            || trimmed.starts_with("retry:")
                        {
                            return Ok(None);
                        }
                        if let Some(ev) = trimmed.strip_prefix("event:") {
                            current_event = ev.trim().to_string();
                            return Ok(None);
                        }
                        match proto {
                            ProtocolKind::ChatCompletions => parse_openai_sse_chunk(trimmed),
                            ProtocolKind::GeminiGenerateContent => {
                                let res = parse_gemini_sse_chunk(trimmed, call_counter);
                                if let Ok(Some(ref c)) = res {
                                    call_counter += c.tool_calls.len();
                                }
                                res
                            }
                            ProtocolKind::AnthropicMessages => {
                                parse_anthropic_sse_event(&current_event, trimmed)
                            }
                            ProtocolKind::CodexResponses => {
                                parse_responses_sse_chunk(&current_event, trimmed)
                            }
                        }
                    };

                    'outer: loop {
                        let bytes_chunk = match resp.chunk().await {
                            Ok(Some(chunk)) => chunk,
                            Ok(None) => break 'outer,
                            Err(e) => {
                                finished_ok = false;
                                stream_err = Some((502, format!("Upstream SSE stream read failed: {e}")));
                                break 'outer;
                            }
                        };
                        if tx.is_closed() {
                            finished_ok = false;
                            break;
                        }
                        raw_bytes.extend_from_slice(&bytes_chunk);
                        while let Some(pos) = raw_bytes.iter().position(|&b| b == b'\n') {
                            let line = String::from_utf8_lossy(&raw_bytes[..pos])
                                .trim_end_matches('\r')
                                .to_string();
                            raw_bytes.drain(..=pos);
                            let line_trim = line.trim();
                            let data_part = line_trim.strip_prefix("data:").map(str::trim).unwrap_or(line_trim);
                            if data_part == "[DONE]" {
                                sent_done = true;
                                break 'outer;
                            }
                            match process_line(&line) {
                                Ok(Some(mut uchunk)) => {
                                    if let Some(ref u) = uchunk.usage {
                                        if u.input_tokens > 0 {
                                            total_usage.input_tokens = u.input_tokens;
                                        }
                                        if u.output_tokens > 0 {
                                            total_usage.output_tokens = u.output_tokens;
                                        }
                                        if u.cache_read_tokens > 0 {
                                            total_usage.cache_read_tokens = u.cache_read_tokens;
                                        }
                                        total_usage.total_tokens = if u.total_tokens > 0 {
                                            u.total_tokens
                                        } else {
                                            total_usage.input_tokens + total_usage.output_tokens
                                        };
                                    }
                                    if let Some(ref eng) = masking_clone {
                                        if !uchunk.text_delta.is_empty() {
                                            uchunk.text_delta = text_unmasker.push(&uchunk.text_delta, eng);
                                        }
                                        if let Some(ref r) = uchunk.reasoning_delta {
                                            let unmasked_r = think_unmasker.push(r, eng);
                                            uchunk.reasoning_delta = (!unmasked_r.is_empty()).then_some(unmasked_r);
                                        }
                                        for tc in &mut uchunk.tool_calls {
                                            tc.arguments = eng.unmask_text(&tc.arguments);
                                        }
                                    }
                                    if ttft_ms.is_none()
                                        && (!uchunk.text_delta.is_empty() || uchunk.reasoning_delta.is_some())
                                    {
                                        ttft_ms = Some(start_instant.elapsed().as_millis() as u64);
                                    }
                                    if !uchunk.text_delta.is_empty()
                                        || uchunk.reasoning_delta.is_some()
                                        || !uchunk.tool_calls.is_empty()
                                        || uchunk.finish_reason.is_some()
                                        || uchunk.usage.is_some()
                                    {
                                        let ev = format_sse_event_from_chunk(&chunk_id, &stream_model, &uchunk);
                                        if tx.send(Ok(ev)).await.is_err() {
                                            finished_ok = false;
                                            break 'outer;
                                        }
                                    }
                                }
                                Ok(None) => {}
                                Err(ProtocolError::Upstream(msg)) => {
                                    finished_ok = false;
                                    stream_err = Some((502, msg));
                                    break 'outer;
                                }
                                Err(ProtocolError::InvalidJson(msg))
                                | Err(ProtocolError::MissingField(msg)) => {
                                    if line_trim
                                        .strip_prefix("data:")
                                        .is_some_and(|d| !d.trim().is_empty())
                                    {
                                        finished_ok = false;
                                        stream_err = Some((502, msg));
                                        break 'outer;
                                    }
                                }
                            }
                        }
                    }

                    if finished_ok && !raw_bytes.is_empty() && !sent_done {
                        let line = String::from_utf8_lossy(&raw_bytes)
                            .trim_end_matches('\r')
                            .to_string();
                        let line_trim = line.trim();
                        let data_part = line_trim.strip_prefix("data:").map(str::trim).unwrap_or(line_trim);
                        if data_part != "[DONE]" {
                            match process_line(&line) {
                                Ok(Some(mut uchunk)) => {
                                    if let Some(ref eng) = masking_clone {
                                        if !uchunk.text_delta.is_empty() {
                                            uchunk.text_delta = text_unmasker.push(&uchunk.text_delta, eng);
                                        }
                                        if let Some(ref r) = uchunk.reasoning_delta {
                                            let unmasked_r = think_unmasker.push(r, eng);
                                            uchunk.reasoning_delta = (!unmasked_r.is_empty()).then_some(unmasked_r);
                                        }
                                        for tc in &mut uchunk.tool_calls {
                                            tc.arguments = eng.unmask_text(&tc.arguments);
                                        }
                                    }
                                    let ev = format_sse_event_from_chunk(&chunk_id, &stream_model, &uchunk);
                                    let _ = tx.send(Ok(ev)).await;
                                }
                                Ok(None) => {}
                                Err(ProtocolError::Upstream(msg)) => {
                                    finished_ok = false;
                                    stream_err = Some((502, msg));
                                }
                                Err(ProtocolError::InvalidJson(msg))
                                | Err(ProtocolError::MissingField(msg)) => {
                                    if line_trim
                                        .strip_prefix("data:")
                                        .is_some_and(|d| !d.trim().is_empty())
                                    {
                                        finished_ok = false;
                                        stream_err = Some((502, msg));
                                    }
                                }
                            }
                        }
                    }

                    if finished_ok {
                        if let Some(ref eng) = masking_clone {
                            let rem_think = think_unmasker.finish(eng);
                            let rem_text = text_unmasker.finish(eng);
                            if !rem_think.is_empty() || !rem_text.is_empty() {
                                let flush_chunk = UnifiedStreamChunk {
                                    text_delta: rem_text,
                                    reasoning_delta: (!rem_think.is_empty()).then_some(rem_think),
                                    ..Default::default()
                                };
                                let ev = format_sse_event_from_chunk(&chunk_id, &stream_model, &flush_chunk);
                                let _ = tx.send(Ok(ev)).await;
                            }
                        }
                        let _ = tx.send(Ok(Event::default().data("[DONE]"))).await;
                        let elapsed_ms = start_instant.elapsed().as_millis() as u64;
                        record_successful_completion(
                            &state_clone,
                            session_clone,
                            &target_clone,
                            &req_model_clone,
                            &total_usage,
                            ttft_ms,
                            elapsed_ms,
                            candidates_considered,
                            masking_hits,
                            now_ms,
                        );
                    } else if let Some((err_status, err_msg)) = stream_err {
                        let err_payload = json!({
                            "error": {
                                "message": &err_msg,
                                "type": "upstream_error",
                            }
                        });
                        let _ = tx
                            .send(Ok(Event::default().data(err_payload.to_string())))
                            .await;
                        let seat_key = format!("{}/{}", target_clone.provider_id, target_clone.model);
                        let (parsed_status, retry_after) =
                            parse_error_status_and_retry_after(&err_msg);
                        let mut metrics = state_clone.metrics.write();
                        metrics.record_failure_disambiguated(
                            &seat_key,
                            &target_clone.provider_id,
                            parsed_status.unwrap_or(err_status),
                            &err_msg,
                            retry_after,
                            None,
                            chrono::Utc::now().timestamp_millis(),
                        );
                    }
                });

                struct SseTaskAbortGuard(Option<tokio::task::JoinHandle<()>>);
                impl Drop for SseTaskAbortGuard {
                    fn drop(&mut self) {
                        if let Some(h) = self.0.take() {
                            h.abort();
                        }
                    }
                }
                let guard = SseTaskAbortGuard(Some(task));
                let sse_stream = stream::unfold((rx, guard), |(mut rx, guard)| async move {
                    rx.recv().await.map(|item| (item, (rx, guard)))
                });
                return Sse::new(sse_stream)
                    .keep_alive(KeepAlive::default())
                    .into_response();
            }
            Err(err) => {
                let seat_key = format!("{}/{}", target.provider_id, target.model);
                {
                    let mut metrics = state.metrics.write();
                    metrics.record_failure_disambiguated(
                        &seat_key,
                        &target.provider_id,
                        err.status.unwrap_or(502),
                        &err.message,
                        err.retry_after_secs,
                        None,
                        now_ms,
                    );
                }
                let retry = err.is_failover_eligible();
                last_err = Some(err);
                if !retry {
                    break;
                }
            }
        }
    }

    let final_err = last_err.unwrap_or(UpstreamExecError {
        status: Some(502),
        retry_after_secs: None,
        message: "All upstream candidates failed".to_string(),
    });
    let resp_status = final_err
        .status
        .and_then(|c| StatusCode::from_u16(c).ok())
        .filter(|s| s.as_u16() == 429 || s.as_u16() == 504 || s.as_u16() == 503 || s.as_u16() == 401 || s.as_u16() == 400)
        .unwrap_or(StatusCode::BAD_GATEWAY);
    error_json(resp_status, final_err.message, "upstream_error")
}

fn parse_responses_sse_chunk(event_type: &str, data_line: &str) -> Result<Option<UnifiedStreamChunk>, ProtocolError> {
    let trimmed = data_line.trim();
    let json_str = trimmed.strip_prefix("data:").map(str::trim).unwrap_or(trimmed);
    if json_str.is_empty() || json_str == "[DONE]" || json_str.starts_with("event:") || json_str.starts_with(':') {
        return Ok(None);
    }
    let val: Value = serde_json::from_str(json_str)
        .map_err(|e| ProtocolError::InvalidJson(format!("Responses SSE JSON: {e}")))?;
    if let Some(err) = val.get("error").filter(|e| !e.is_null()) {
        let msg = err
            .get("message")
            .and_then(Value::as_str)
            .or_else(|| err.as_str())
            .unwrap_or("Responses SSE upstream error");
        return Err(ProtocolError::Upstream(msg.to_string()));
    }
    let etype = if event_type.is_empty() {
        val.get("type").and_then(Value::as_str).unwrap_or("")
    } else {
        event_type
    };
    if etype == "error" || val.get("type").and_then(Value::as_str) == Some("error") {
        let code = val
            .get("code")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let msg = val
            .get("message")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let err_text = match (code, msg) {
            (Some(c), Some(m)) => format!("{c}: {m}"),
            (None, Some(m)) => m.to_string(),
            (Some(c), None) => c.to_string(),
            (None, None) => "Responses SSE upstream error".to_string(),
        };
        return Err(ProtocolError::Upstream(err_text));
    }
    match etype {
        "error" => unreachable!("handled above"),
        "response.failed" => {
            let msg = val
                .get("response")
                .and_then(|r| r.get("error"))
                .and_then(|e| e.get("message"))
                .and_then(Value::as_str)
                .or_else(|| val.get("message").and_then(Value::as_str))
                .unwrap_or("Responses stream failed");
            Err(ProtocolError::Upstream(msg.to_string()))
        }
        "response.incomplete" => {
            let resp = val.get("response");
            let status_failed = resp
                .and_then(|r| r.get("status"))
                .and_then(Value::as_str)
                == Some("failed");
            let resp_err = resp
                .and_then(|r| r.get("error"))
                .filter(|e| !e.is_null());
            if status_failed || resp_err.is_some() {
                let msg = resp_err
                    .and_then(|e| e.get("message"))
                    .and_then(Value::as_str)
                    .or_else(|| {
                        resp.and_then(|r| r.get("incomplete_details"))
                            .and_then(|d| d.get("reason"))
                            .and_then(Value::as_str)
                    })
                    .unwrap_or("Responses stream failed");
                return Err(ProtocolError::Upstream(msg.to_string()));
            }
            Ok(None)
        }
        "response.output_text.delta" => Ok(Some(UnifiedStreamChunk {
            text_delta: val.get("delta").and_then(Value::as_str).unwrap_or_default().to_string(),
            ..Default::default()
        })),
        "response.reasoning_summary_text.delta" => Ok(Some(UnifiedStreamChunk {
            reasoning_delta: val.get("delta").and_then(Value::as_str).map(ToString::to_string),
            ..Default::default()
        })),
        "response.completed" => {
            let resp = val.get("response");
            if resp.and_then(|r| r.get("status")).and_then(Value::as_str) == Some("failed") {
                let msg = resp
                    .and_then(|r| r.get("error"))
                    .and_then(|e| e.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("Responses stream failed");
                return Err(ProtocolError::Upstream(msg.to_string()));
            }
            let usage = resp.and_then(|r| r.get("usage")).map(|u| {
                let inp = u.get("input_tokens").and_then(Value::as_u64).unwrap_or(0);
                let outp = u.get("output_tokens").and_then(Value::as_u64).unwrap_or(0);
                UnifiedUsage {
                    input_tokens: inp,
                    output_tokens: outp,
                    cache_read_tokens: 0,
                    total_tokens: u.get("total_tokens").and_then(Value::as_u64).unwrap_or(inp + outp),
                }
            });
            Ok(Some(UnifiedStreamChunk {
                finish_reason: Some("stop".to_string()),
                usage,
                ..Default::default()
            }))
        }
        _ => Ok(None),
    }
}

fn resolve_gateway_upstream_target(
    state: &GatewayState,
    provider_id: &str,
) -> (String, ProtocolKind, Option<String>, HashMap<String, String>) {
    if let Some((url, proto, key)) = state.provider_endpoints.read().get(provider_id).cloned() {
        return (url, proto, key, HashMap::new());
    }
    if let Some(sub) = state
        .subscriptions
        .read()
        .accounts
        .iter()
        .find(|a| a.id == provider_id)
        .cloned()
    {
        if let Some(url) = sub.base_url {
            let hdrs = sub.headers.into_iter().collect();
            return (url, sub.protocol, sub.access_token, hdrs);
        }
    }
    let preset = find_preset(provider_id);
    let resolved_ep = resolve_preset_endpoints(provider_id, None, None);

    let base_url = resolved_ep
        .as_ref()
        .map(|e| e.base_url.clone())
        .or_else(|| preset.as_ref().and_then(|p| p.chat_url.clone()))
        .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

    let proto = resolved_ep
        .as_ref()
        .map(|e| e.protocol)
        .or_else(|| preset.as_ref().map(|p| p.default_protocol))
        .unwrap_or(ProtocolKind::ChatCompletions);

    (base_url, proto, None, HashMap::new())
}

async fn start_upstream_request(
    state: &GatewayState,
    provider_id: &str,
    req: &UnifiedRequest,
) -> Result<UpstreamOutcome, UpstreamExecError> {
    let (base_url, proto, api_key, extra_headers) =
        resolve_gateway_upstream_target(state, provider_id);

    let (url, payload) = match proto {
        ProtocolKind::GeminiGenerateContent => (
            resolve_gemini_endpoint_url(&base_url, &req.model, req.stream),
            build_gemini_generate_content_payload(req),
        ),
        ProtocolKind::AnthropicMessages => (
            resolve_anthropic_messages_url(&base_url),
            unified_to_anthropic_request_json(req),
        ),
        ProtocolKind::CodexResponses => (
            format!("{}/responses", base_url.trim_end_matches('/')),
            unified_to_responses_request_json(req),
        ),
        ProtocolKind::ChatCompletions => {
            let url = if base_url.ends_with("/chat/completions") {
                base_url
            } else {
                format!("{}/chat/completions", base_url.trim_end_matches('/'))
            };
            (url, unified_to_chat_completions_request_json(req))
        }
    };

    let mut builder = state
        .http_client
        .post(&url)
        .header(reqwest::header::CONTENT_TYPE, "application/json");
    if let Some(ref key) = api_key {
        if !key.trim().is_empty() {
            if proto == ProtocolKind::AnthropicMessages {
                builder = builder.header("x-api-key", key.trim());
            } else if proto == ProtocolKind::GeminiGenerateContent {
                builder = builder.header("x-goog-api-key", key.trim());
            } else {
                builder = builder.bearer_auth(key.trim());
            }
        }
    }
    if req.stream {
        builder = builder.header(reqwest::header::ACCEPT, "text/event-stream");
    }
    for (k, v) in &extra_headers {
        builder = builder.header(k.as_str(), v.as_str());
    }

    let res = builder.json(&payload).send().await.map_err(|e| UpstreamExecError {
        status: None,
        retry_after_secs: None,
        message: format!("HTTP request failed: {e}"),
    })?;

    let status = res.status();
    if !status.is_success() {
        let retry_after_secs = res
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(parse_retry_after_header);
        let text = res.text().await.unwrap_or_default();
        return Err(UpstreamExecError {
            status: Some(status.as_u16()),
            retry_after_secs,
            message: format!("Upstream HTTP {status}: {text}"),
        });
    }

    let is_sse = req.stream
        && res
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|ct| !ct.to_ascii_lowercase().contains("application/json"))
            .unwrap_or(true);

    if is_sse {
        return Ok(UpstreamOutcome::Sse {
            response: res,
            provider_id: provider_id.to_string(),
            model: req.model.clone(),
            proto,
        });
    }

    let text = res.text().await.map_err(|e| UpstreamExecError {
        status: Some(status.as_u16()),
        retry_after_secs: None,
        message: format!("Read body failed: {e}"),
    })?;

    let parsed = match proto {
        ProtocolKind::GeminiGenerateContent => parse_gemini_response_json(provider_id, &req.model, &text),
        ProtocolKind::AnthropicMessages => parse_anthropic_response_json(provider_id, &req.model, &text),
        ProtocolKind::CodexResponses => parse_responses_response_json(provider_id, &req.model, &text),
        ProtocolKind::ChatCompletions => parse_chat_completions_response_json(provider_id, &req.model, &text),
    }
    .map_err(|e| UpstreamExecError {
        status: Some(502),
        retry_after_secs: None,
        message: format!("Protocol error: {e}"),
    })?;

    Ok(UpstreamOutcome::Buffered(parsed))
}

/// Fallback executor sending HTTP requests to resolved upstream provider.
async fn execute_upstream_request(
    state: &GatewayState,
    provider_id: &str,
    req: &UnifiedRequest,
) -> Result<UnifiedResponse, String> {
    let mut non_stream_req = req.clone();
    non_stream_req.stream = false;
    match start_upstream_request(state, provider_id, &non_stream_req).await {
        Ok(UpstreamOutcome::Buffered(resp)) => Ok(resp),
        Ok(UpstreamOutcome::Sse { .. }) => Err("Unexpected SSE stream in buffered mode".to_string()),
        Err(e) => Err(e.message),
    }
}

/// Handle `GET /v1/models` in OpenAI-compatible schema format.
pub async fn handle_models(State(state): State<GatewayState>) -> Json<Value> {
    let now = chrono::Utc::now().timestamp();
    let mut model_list = Vec::new();

    // 1. Configured virtual routing groups
    {
        let groups = state.groups.read();
        for g in groups.values() {
            model_list.push(json!({
                "id": g.qualified_model_name(),
                "object": "model",
                "created": now,
                "owned_by": "magpie",
                "permission": [],
                "root": g.canonical_id(),
                "parent": null,
            }));
            model_list.push(json!({
                "id": g.canonical_id(),
                "object": "model",
                "created": now,
                "owned_by": "magpie",
                "permission": [],
                "root": g.canonical_id(),
                "parent": null,
            }));
        }
    }

    // 2. Discovered subscription accounts
    {
        let reg = state.subscriptions.read();
        for acc in &reg.accounts {
            if let Some(ref def_m) = acc.default_model {
                model_list.push(json!({
                    "id": def_m,
                    "object": "model",
                    "created": now,
                    "owned_by": acc.kind.as_str(),
                }));
            }
        }
    }

    // 3. Common catalog models
    for std_model in [
        "gpt-4o",
        "gpt-4o-mini",
        "claude-3-5-sonnet-20241022",
        "claude-3-5-haiku-20241022",
        "gemini-2.5-flash",
        "gemini-2.5-pro",
        "deepseek-chat",
        "deepseek-reasoner",
        "qwen-plus",
        "glm-4-plus",
    ] {
        if !model_list.iter().any(|v| v["id"] == std_model) {
            model_list.push(json!({
                "id": std_model,
                "object": "model",
                "created": now,
                "owned_by": "system",
            }));
        }
    }

    Json(json!({
        "object": "list",
        "data": model_list,
    }))
}

/// Handle `POST /v1/responses`.
pub async fn handle_responses(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let session_id = normalize_session_header(&headers)
        .unwrap_or_else(|| format!("sess_{}", uuid::Uuid::new_v4()));

    let mut unified_req = match inbound_responses_to_unified(&payload) {
        Ok(r) => r,
        Err(e) => return error_json(StatusCode::BAD_REQUEST, e.to_string(), "invalid_request"),
    };

    let (req_masking, _masking_hits) = match mask_unified_request(&state, &mut unified_req) {
        Ok(pair) => pair,
        Err(resp) => return resp,
    };

    let (prov, _model) = crate::routing::split_target_spec(&unified_req.model, &state.default_provider_id);

    let res = if let Some(ref handler) = state.custom_handler {
        handler(unified_req, Some(session_id)).await
    } else {
        execute_upstream_request(&state, &prov, &unified_req).await
    };

    match res {
        Ok(mut resp) => {
            unmask_unified_response(&mut resp, req_masking.as_deref());
            Json(unified_to_responses_json(&resp)).into_response()
        }
        Err(e) => error_json(StatusCode::BAD_GATEWAY, e, "upstream_error"),
    }
}

/// Handle `POST /v1/messages`.
pub async fn handle_messages(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let session_id = normalize_session_header(&headers)
        .unwrap_or_else(|| format!("sess_{}", uuid::Uuid::new_v4()));

    let mut unified_req = match inbound_anthropic_to_unified(&payload) {
        Ok(r) => r,
        Err(e) => return error_json(StatusCode::BAD_REQUEST, e.to_string(), "invalid_request"),
    };

    let (req_masking, _masking_hits) = match mask_unified_request(&state, &mut unified_req) {
        Ok(pair) => pair,
        Err(resp) => return resp,
    };

    let (prov, _model) = crate::routing::split_target_spec(&unified_req.model, &state.default_provider_id);

    let res = if let Some(ref handler) = state.custom_handler {
        handler(unified_req, Some(session_id)).await
    } else {
        execute_upstream_request(&state, &prov, &unified_req).await
    };

    match res {
        Ok(mut resp) => {
            unmask_unified_response(&mut resp, req_masking.as_deref());
            Json(unified_to_anthropic_json(&resp)).into_response()
        }
        Err(e) => error_json(StatusCode::BAD_GATEWAY, e, "upstream_error"),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QuotasView {
    pub rests: HashMap<String, RestState>,
    pub seats: HashMap<String, RouteSeatMetrics>,
    pub forbidden_alike: HashSet<String>,
}

/// Handle `GET /v1/magpie/quotas`.
pub async fn handle_quotas(State(state): State<GatewayState>) -> Json<QuotasView> {
    let metrics = state.metrics.read();
    Json(QuotasView {
        rests: metrics.circuit_breaker.rests.clone(),
        seats: metrics.seats.clone(),
        forbidden_alike: metrics.circuit_breaker.known_forbidden_alike.clone(),
    })
}

#[derive(Debug, Deserialize)]
pub struct UsageQueryParams {
    pub session: Option<String>,
    pub since_ms: Option<i64>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UsageResponseView {
    pub summary: UsageSummary,
    pub records: Vec<UsageRecord>,
}

/// Handle `GET /v1/magpie/usage`.
pub async fn handle_usage(
    State(state): State<GatewayState>,
    Query(params): Query<UsageQueryParams>,
) -> Json<UsageResponseView> {
    let limit = params.limit.unwrap_or(50).min(500);
    let summary = if let Some(ref sess) = params.session {
        state.ledger.summary_for_session(sess)
    } else {
        state.ledger.summary(params.since_ms)
    };
    let records = state.ledger.query_records(limit, params.session.as_deref());

    Json(UsageResponseView { summary, records })
}

/// Embedded loopback gateway server handle.
pub struct GatewayServerHandle {
    addr: SocketAddr,
    shutdown_tx: watch::Sender<bool>,
    task_handle: tokio::task::JoinHandle<()>,
}

impl GatewayServerHandle {
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let _ = self.task_handle.await;
    }
}

/// Loopback server capable of binding to `127.0.0.1:<port>` with graceful shutdown.
pub struct GatewayServer {
    state: GatewayState,
    listener: Option<tokio::net::TcpListener>,
    addr: Option<SocketAddr>,
    shutdown_tx: Option<watch::Sender<bool>>,
    shutdown_rx: Option<watch::Receiver<bool>>,
}

impl GatewayServer {
    pub async fn bind(state: GatewayState, addr: SocketAddr) -> Result<Self, std::io::Error> {
        let listener = tokio::net::TcpListener::bind(addr).await?;
        let local_addr = listener.local_addr()?;
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Ok(Self {
            state,
            listener: Some(listener),
            addr: Some(local_addr),
            shutdown_tx: Some(shutdown_tx),
            shutdown_rx: Some(shutdown_rx),
        })
    }

    pub async fn bind_loopback(state: GatewayState, port: u16) -> Result<Self, std::io::Error> {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        Self::bind(state, addr).await
    }

    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.addr
    }

    pub fn run_background(mut self) -> GatewayServerHandle {
        let addr = self.addr.expect("server must be bound");
        let listener = self.listener.take().expect("listener must exist");
        let shutdown_tx = self.shutdown_tx.take().expect("shutdown_tx must exist");
        let mut shutdown_rx = self.shutdown_rx.take().expect("shutdown_rx must exist");
        let router = create_gateway_router(self.state);

        let task_handle = tokio::spawn(async move {
            let srv = axum::serve(listener, router).with_graceful_shutdown(async move {
                let _ = shutdown_rx.changed().await;
            });
            let _ = srv.await;
        });

        GatewayServerHandle {
            addr,
            shutdown_tx,
            task_handle,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_responses_sse_chunk_handles_flat_error_events() {
        let err_flat = parse_responses_sse_chunk(
            "error",
            r#"data: {"type":"error","code":"server_error","message":"codex_responses_midstream_err","param":null,"sequence_number":2}"#,
        )
        .unwrap_err();
        match err_flat {
            ProtocolError::Upstream(msg) => {
                assert!(msg.contains("codex_responses_midstream_err"), "{msg}");
                assert!(msg.contains("server_error"), "{msg}");
            }
            other => panic!("expected Upstream error, got {other:?}"),
        }

        let err_flat_no_event = parse_responses_sse_chunk(
            "",
            r#"data: {"type":"error","message":"flat_without_event_header"}"#,
        )
        .unwrap_err();
        match err_flat_no_event {
            ProtocolError::Upstream(msg) => assert_eq!(msg, "flat_without_event_header"),
            other => panic!("expected Upstream error, got {other:?}"),
        }

        let err_failed = parse_responses_sse_chunk(
            "response.failed",
            r#"data: {"type":"response.failed","response":{"error":{"message":"resp_failed_msg"}}}"#,
        )
        .unwrap_err();
        match err_failed {
            ProtocolError::Upstream(msg) => assert_eq!(msg, "resp_failed_msg"),
            other => panic!("expected Upstream error, got {other:?}"),
        }

        let err_incomplete_failed = parse_responses_sse_chunk(
            "response.incomplete",
            r#"data: {"type":"response.incomplete","response":{"status":"failed","error":{"message":"incomplete_failed_msg"}}}"#,
        )
        .unwrap_err();
        match err_incomplete_failed {
            ProtocolError::Upstream(msg) => assert_eq!(msg, "incomplete_failed_msg"),
            other => panic!("expected Upstream error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_gateway_sse_midstream_flat_errors_omit_done_and_usage() {
        async fn mock_responses_err() -> Response {
            let body = concat!(
                "event: response.output_text.delta\n",
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"RESP_PARTIAL_\"}\n\n",
                "event: error\n",
                "data: {\"type\":\"error\",\"code\":\"server_error\",\"message\":\"codex_responses_midstream_err\",\"param\":null,\"sequence_number\":2}\n\n",
                "data: [DONE]\n\n",
            );
            Response::builder()
                .status(StatusCode::OK)
                .header(axum::http::header::CONTENT_TYPE, "text/event-stream")
                .body(axum::body::Body::from(body))
                .unwrap()
        }

        async fn mock_anthropic_err() -> Response {
            let body = concat!(
                "event: content_block_delta\n",
                "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"ANTH_PARTIAL_\"}}\n\n",
                "event: error\n",
                "data: {\"type\":\"error\",\"message\":\"anth_top_msg_no_nested_obj\"}\n\n",
                "data: [DONE]\n\n",
            );
            Response::builder()
                .status(StatusCode::OK)
                .header(axum::http::header::CONTENT_TYPE, "text/event-stream")
                .body(axum::body::Body::from(body))
                .unwrap()
        }

        let mock_app = Router::new()
            .route("/responses", post(mock_responses_err))
            .route("/v1/messages", post(mock_anthropic_err));
        let mock_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mock_addr = mock_listener.local_addr().unwrap();
        let mock_srv = tokio::spawn(async move {
            let _ = axum::serve(mock_listener, mock_app).await;
        });

        let state = GatewayState::new("resp_err");
        state.provider_endpoints.write().insert(
            "resp_err".to_string(),
            (
                format!("http://{mock_addr}"),
                ProtocolKind::CodexResponses,
                Some("k1".to_string()),
            ),
        );
        state.provider_endpoints.write().insert(
            "anth_top".to_string(),
            (
                format!("http://{mock_addr}"),
                ProtocolKind::AnthropicMessages,
                Some("k2".to_string()),
            ),
        );

        let gw = GatewayServer::bind_loopback(state.clone(), 0).await.unwrap();
        let gw_handle = gw.run_background();
        let gw_addr = gw_handle.local_addr();
        let client = reqwest::Client::new();

        // 1. CodexResponses flat error
        let resp1 = client
            .post(format!("http://{gw_addr}/v1/chat/completions"))
            .json(&json!({
                "model": "resp_err/codex-mini",
                "stream": true,
                "messages": [{"role": "user", "content": "hi"}]
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp1.status(), StatusCode::OK);
        let body1 = resp1.text().await.unwrap();
        assert!(body1.contains("RESP_PARTIAL_"), "expected partial chunk: {body1}");
        assert!(
            body1.contains("codex_responses_midstream_err"),
            "expected upstream error payload: {body1}"
        );
        assert!(!body1.contains("[DONE]"), "must not emit [DONE] on error: {body1}");

        // 2. AnthropicMessages flat error
        let resp2 = client
            .post(format!("http://{gw_addr}/v1/chat/completions"))
            .json(&json!({
                "model": "anth_top/claude-3-5",
                "stream": true,
                "messages": [{"role": "user", "content": "hi"}]
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp2.status(), StatusCode::OK);
        let body2 = resp2.text().await.unwrap();
        assert!(body2.contains("ANTH_PARTIAL_"), "expected partial chunk: {body2}");
        assert!(
            body2.contains("anth_top_msg_no_nested_obj"),
            "expected upstream error payload: {body2}"
        );
        assert!(!body2.contains("[DONE]"), "must not emit [DONE] on error: {body2}");

        let records = state.ledger.query_records(50, None);
        assert!(
            records.is_empty(),
            "failed streams must not record usage completions, got {records:?}"
        );

        gw_handle.shutdown().await;
        mock_srv.abort();
    }
}
