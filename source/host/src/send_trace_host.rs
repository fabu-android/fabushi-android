use std::any::Any;
use std::collections::BTreeMap;
use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::Value;
use uuid::Uuid;

pub const SEND_TRACE_SAMPLE_RATIO: f64 = 1.0;
pub const SAND_TURN_ROOT_SPAN_NAME: &str = "sand.turn.run";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTraceparent {
    pub trace_id: String,
    pub span_id: String,
    pub trace_flags: u8,
}

fn is_lower_hex(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

pub fn parse_traceparent(traceparent: &str) -> Option<ParsedTraceparent> {
    let parts = traceparent.trim().split('-').collect::<Vec<_>>();
    if parts.len() != 4 {
        return None;
    }
    let (version, trace_id, span_id, flags) = (parts[0], parts[1], parts[2], parts[3]);
    if version != "00"
        || !is_lower_hex(trace_id, 32)
        || trace_id.bytes().all(|byte| byte == b'0')
        || !is_lower_hex(span_id, 16)
        || span_id.bytes().all(|byte| byte == b'0')
        || !is_lower_hex(flags, 2)
    {
        return None;
    }
    let trace_flags = u8::from_str_radix(flags, 16).ok()?;
    Some(ParsedTraceparent {
        trace_id: trace_id.to_string(),
        span_id: span_id.to_string(),
        trace_flags,
    })
}

fn random_trace_id() -> String {
    format!("{:032x}", Uuid::new_v4().as_u128())
}

fn random_span_id() -> String {
    let raw = Uuid::new_v4().as_u128() as u64;
    format!("{:016x}", raw.max(1))
}

fn random_unit() -> f64 {
    let raw = Uuid::new_v4().as_u128() as u64;
    (raw as f64) / (u64::MAX as f64)
}

pub fn should_sample_send(ratio: f64, random: f64) -> bool {
    if !(ratio > 0.0) {
        return false;
    }
    if ratio >= 1.0 {
        return true;
    }
    random < ratio
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MintedTraceparent {
    pub traceparent: String,
    pub trace_id: String,
    pub span_id: String,
}

pub fn mint_traceparent(sampled: bool) -> MintedTraceparent {
    let trace_id = random_trace_id();
    let span_id = random_span_id();
    MintedTraceparent {
        traceparent: format!(
            "00-{trace_id}-{span_id}-{}",
            if sampled { "01" } else { "00" }
        ),
        trace_id,
        span_id,
    }
}

pub fn derive_child_traceparent(parent: &str) -> Option<(String, String)> {
    let parsed = parse_traceparent(parent)?;
    let span_id = random_span_id();
    let flags = if parsed.trace_flags & 1 == 1 { "01" } else { "00" };
    Some((
        format!("00-{}-{span_id}-{flags}", parsed.trace_id),
        span_id,
    ))
}

pub trait TraceSpan: Send + Sync {
    fn set_attribute(&self, key: &str, value: Value);
    fn add_event(&self, _name: &str, _attributes: &BTreeMap<String, Value>) {}
    fn record_exception(&self, error: &str);
    fn set_status(&self, code: u8);
    fn end(&self);
    /// Ends a span at an explicit epoch-millisecond timestamp. Existing test
    /// spans may keep the default while the production OTLP span preserves the
    /// frozen completed-span timing contract.
    fn end_at(&self, _end_time_ms: f64) {
        self.end();
    }
}

pub type TraceContext = Arc<dyn Any + Send + Sync>;

#[derive(Clone)]
pub struct HostTrace {
    pub span: Arc<dyn TraceSpan>,
    pub context: Option<TraceContext>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
}

pub struct TraceFactoryOptions {
    pub name: String,
    pub traceparent: Option<String>,
    pub parent_ctx: Option<TraceContext>,
    pub start_time: Option<f64>,
    pub inheritable_attributes: BTreeMap<String, Value>,
}

pub type TraceFactory =
    Arc<dyn Fn(TraceFactoryOptions) -> Option<HostTrace> + Send + Sync + 'static>;

fn trace_factory_slot() -> &'static Mutex<Option<TraceFactory>> {
    static SLOT: OnceLock<Mutex<Option<TraceFactory>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

fn turn_bundle_version_slot() -> &'static Mutex<Option<String>> {
    static SLOT: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

fn trace_factory() -> Option<TraceFactory> {
    trace_factory_slot()
        .lock()
        .ok()
        .and_then(|factory| factory.clone())
}

pub fn set_host_trace_factory(factory: TraceFactory) {
    if let Ok(mut slot) = trace_factory_slot().lock() {
        *slot = Some(factory);
    }
}

pub fn clear_host_trace_factory() {
    if let Ok(mut slot) = trace_factory_slot().lock() {
        *slot = None;
    }
}

pub fn set_turn_trace_host_bundle_version(version: Option<&str>) {
    let normalized = version
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if let Ok(mut slot) = turn_bundle_version_slot().lock() {
        *slot = normalized;
    }
}

fn turn_trace_host_bundle_version() -> Option<String> {
    turn_bundle_version_slot()
        .lock()
        .ok()
        .and_then(|value| value.clone())
}

fn create_trace(factory: &TraceFactory, options: TraceFactoryOptions) -> Option<HostTrace> {
    catch_unwind(AssertUnwindSafe(|| factory(options)))
        .ok()
        .flatten()
}

pub fn adopt_remote_parent(traceparent: Option<&str>, span_name: &str) -> Option<HostTrace> {
    let traceparent = traceparent?;
    parse_traceparent(traceparent)?;
    let factory = trace_factory()?;
    create_trace(
        &factory,
        TraceFactoryOptions {
            name: span_name.to_string(),
            traceparent: Some(traceparent.to_string()),
            parent_ctx: None,
            start_time: None,
            inheritable_attributes: BTreeMap::new(),
        },
    )
}

pub fn begin_send_trace(traceparent: Option<&str>) -> Option<HostTrace> {
    adopt_remote_parent(traceparent, "sand.send")
}


/// Records a synchronous child span whose start and end happened before the
/// recorder is invoked. This is the Rust equivalent of Grok's bundle-scope
/// completedSpanRecorder used by send acceptance.
pub fn record_completed_trace_span(
    parent: Option<&HostTrace>,
    name: &str,
    start_time_ms: f64,
    end_time_ms: f64,
    attributes: &BTreeMap<String, Value>,
) {
    let Some(parent) = parent else {
        return;
    };
    let Some(factory) = trace_factory() else {
        return;
    };
    let Some(child) = create_trace(
        &factory,
        TraceFactoryOptions {
            name: name.to_string(),
            traceparent: None,
            parent_ctx: parent.context.clone(),
            start_time: Some(start_time_ms.max(0.0)),
            inheritable_attributes: BTreeMap::new(),
        },
    ) else {
        return;
    };
    let _ = catch_unwind(AssertUnwindSafe(|| {
        for (key, value) in attributes {
            child.span.set_attribute(key, value.clone());
        }
        child.span.end_at(end_time_ms.max(start_time_ms).max(0.0));
    }));
}

pub fn begin_gateway_command_trace(traceparent: Option<&str>, method: &str) -> Option<HostTrace> {
    adopt_remote_parent(traceparent, &format!("sand.gateway.{method}"))
}

pub fn resolve_turn_trace_sample_ratio(raw: Option<&str>) -> f64 {
    let Some(raw) = raw.filter(|value| !value.is_empty()) else {
        return SEND_TRACE_SAMPLE_RATIO;
    };
    let Ok(parsed) = raw.parse::<f64>() else {
        return SEND_TRACE_SAMPLE_RATIO;
    };
    if parsed.is_finite() && (0.0..=1.0).contains(&parsed) {
        parsed
    } else {
        SEND_TRACE_SAMPLE_RATIO
    }
}

pub struct TurnTraceTypeOptions<'a> {
    pub automation_wake: Option<&'a Value>,
    pub request_source: Option<&'a str>,
    pub hidden: bool,
}

pub fn resolve_turn_trace_type(options: TurnTraceTypeOptions<'_>) -> String {
    if options.automation_wake.is_some_and(|value| !value.is_null()) {
        return "automation".into();
    }
    if let Some(source) = options.request_source.filter(|source| *source != "turn") {
        return source.to_string();
    }
    if options.hidden {
        "hidden".into()
    } else {
        "user".into()
    }
}

pub struct BeginTurnTraceOptions {
    pub conversation_id: String,
    pub turn_type: String,
    pub traceparent: Option<String>,
    pub parent_ctx: Option<TraceContext>,
    pub start_time: Option<f64>,
    pub sample_ratio: Option<f64>,
    pub attributes: BTreeMap<String, Value>,
}

pub fn begin_turn_trace(options: BeginTurnTraceOptions) -> Option<HostTrace> {
    let factory = trace_factory()?;
    let common_attributes = BTreeMap::from([
        (
            "sand.conversation_id".into(),
            Value::String(options.conversation_id.clone()),
        ),
        (
            "sand.turn_type".into(),
            Value::String(options.turn_type.clone()),
        ),
    ]);

    let trace = if let Some(parent_ctx) = options.parent_ctx.clone() {
        create_trace(
            &factory,
            TraceFactoryOptions {
                name: SAND_TURN_ROOT_SPAN_NAME.into(),
                traceparent: None,
                parent_ctx: Some(parent_ctx),
                start_time: options.start_time,
                inheritable_attributes: common_attributes.clone(),
            },
        )
    } else if let Some(traceparent) = options
        .traceparent
        .clone()
        .filter(|value| parse_traceparent(value).is_some())
    {
        create_trace(
            &factory,
            TraceFactoryOptions {
                name: SAND_TURN_ROOT_SPAN_NAME.into(),
                traceparent: Some(traceparent),
                parent_ctx: None,
                start_time: options.start_time,
                inheritable_attributes: common_attributes.clone(),
            },
        )
    } else {
        None
    };

    let trace = match trace {
        Some(trace) => trace,
        None => {
            let ratio = options
                .sample_ratio
                .unwrap_or_else(|| resolve_turn_trace_sample_ratio(std::env::var("SAND_TURN_TRACE_SAMPLE_RATIO").ok().as_deref()));
            if !should_sample_send(ratio, random_unit()) {
                return None;
            }
            let minted = mint_traceparent(true);
            create_trace(
                &factory,
                TraceFactoryOptions {
                    name: SAND_TURN_ROOT_SPAN_NAME.into(),
                    traceparent: Some(minted.traceparent),
                    parent_ctx: None,
                    start_time: options.start_time,
                    inheritable_attributes: common_attributes,
                },
            )?
        }
    };

    let _ = catch_unwind(AssertUnwindSafe(|| {
        trace.span.set_attribute(
            "sand.conversation_id",
            Value::String(options.conversation_id),
        );
        trace
            .span
            .set_attribute("sand.turn_type", Value::String(options.turn_type));
        if let Some(version) = turn_trace_host_bundle_version() {
            trace
                .span
                .set_attribute("sand.host_bundle_version", Value::String(version));
        }
        for (key, value) in options.attributes {
            trace.span.set_attribute(&key, value);
        }
    }));
    Some(trace)
}

pub struct TurnTraceOutcome {
    pub quiesced_for_upgrade: bool,
    pub aborted: bool,
    pub awaiting_user_selection: bool,
}

pub fn resolve_turn_trace_outcome(result: &TurnTraceOutcome) -> &'static str {
    if result.quiesced_for_upgrade {
        "quiesced_for_upgrade"
    } else if result.aborted {
        "aborted"
    } else if result.awaiting_user_selection {
        "awaiting_user"
    } else {
        "success"
    }
}

pub fn set_turn_trace_attributes(
    trace: Option<&HostTrace>,
    attributes: &BTreeMap<String, Value>,
) {
    let Some(trace) = trace else {
        return;
    };
    let _ = catch_unwind(AssertUnwindSafe(|| {
        for (key, value) in attributes {
            trace.span.set_attribute(key, value.clone());
        }
    }));
}

pub fn add_turn_trace_event(
    trace: Option<&HostTrace>,
    name: &str,
    attributes: &BTreeMap<String, Value>,
) {
    let Some(trace) = trace else {
        return;
    };
    let _ = catch_unwind(AssertUnwindSafe(|| {
        trace.span.add_event(name, attributes);
    }));
}

pub fn mark_turn_trace_error(trace: Option<&HostTrace>, error: impl ToString) {
    let Some(trace) = trace else {
        return;
    };
    let message = error.to_string();
    let _ = catch_unwind(AssertUnwindSafe(|| {
        trace.span.record_exception(&message);
        trace.span.set_status(2);
        trace
            .span
            .set_attribute("sand.outcome", Value::String("error".into()));
    }));
}

pub async fn trace_send_phase<T, E, F, Fut>(
    ctx: Option<&HostTrace>,
    name: &str,
    operation: F,
) -> Result<T, E>
where
    E: ToString,
    F: FnOnce(Option<&HostTrace>) -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let Some(parent) = ctx else {
        return operation(ctx).await;
    };
    let Some(factory) = trace_factory() else {
        return operation(ctx).await;
    };
    let child = create_trace(
        &factory,
        TraceFactoryOptions {
            name: name.to_string(),
            traceparent: None,
            parent_ctx: parent.context.clone(),
            start_time: None,
            inheritable_attributes: BTreeMap::new(),
        },
    );
    let Some(child) = child else {
        return operation(ctx).await;
    };

    let result = operation(Some(&child)).await;
    if let Err(error) = &result {
        let message = error.to_string();
        let _ = catch_unwind(AssertUnwindSafe(|| {
            child.span.record_exception(&message);
            child.span.set_status(2);
        }));
    }
    let _ = catch_unwind(AssertUnwindSafe(|| child.span.end()));
    result
}
