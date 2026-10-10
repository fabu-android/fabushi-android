use fabushi_android_shared::{
    CoordinatorFailure, CoordinatorFailureCode, CoordinatorRequest, ResyncRequest,
    COORDINATOR_PROTOCOL_VERSION,
};
use fabushi_mahayana_agent_coordinator::{
    oauth::{
        mcp_oauth_callback_listener::OAuthCallback,
        mcp_oauth_forwarder::OAuthForwarder,
    },
    HostPort, MahayanaCoordinator,
};
use fabushi_mahayana_host::android_json_runtime::{
    AndroidHostMode, AndroidJsonHost, RuntimeCallCancellationRegistry,
};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;

struct CoordinatorHost {
    runtime: AndroidJsonHost,
}

impl HostPort for CoordinatorHost {
    fn execute(&mut self, request: &CoordinatorRequest) -> Result<String, CoordinatorFailure> {
        let params: Value = serde_json::from_str(&request.params_json).map_err(|error| {
            CoordinatorFailure::new(
                CoordinatorFailureCode::MalformedRequest,
                format!("invalid Host params JSON: {error}"),
            )
        })?;
        self.runtime
            .dispatch(&request.method, &params)
            .map(|value| value.to_string())
            .map_err(|message| {
                CoordinatorFailure::new(CoordinatorFailureCode::HostUnavailable, message)
            })
    }

    fn cancel(
        &mut self,
        operation_id: &str,
        reason: Option<&str>,
    ) -> Result<(), CoordinatorFailure> {
        self.runtime
            .cancel_operation(operation_id, reason)
            .map_err(|message| CoordinatorFailure::new(CoordinatorFailureCode::Internal, message))
    }
}

pub struct AndroidNativeRuntime {
    coordinator: MahayanaCoordinator<CoordinatorHost>,
    mcp_oauth: OAuthForwarder,
    mode: AndroidHostMode,
    next_request_id: u64,
    runtime_call_control: Arc<RuntimeCallCancellationRegistry>,
}

impl AndroidNativeRuntime {
    pub fn new(
        app_data_dir: impl Into<PathBuf>,
        mode: AndroidHostMode,
        generation: u64,
    ) -> Self {
        Self::new_with_account_session(app_data_dir, mode, generation, None)
    }

    pub fn new_with_account_session(
        app_data_dir: impl Into<PathBuf>,
        mode: AndroidHostMode,
        generation: u64,
        initial_account_session_json: Option<&str>,
    ) -> Self {
        let app_data_dir = app_data_dir.into();
        let runtime = AndroidJsonHost::new_with_account_session(
            app_data_dir.clone(),
            mode,
            initial_account_session_json,
        );
        let runtime_call_control = runtime.runtime_call_control();
        let coordinator_host = CoordinatorHost { runtime };
        let coordinator = if mode == AndroidHostMode::Production {
            MahayanaCoordinator::with_generation_persistent(
                coordinator_host,
                generation,
                512,
                app_data_dir.join("coordinator-state.json"),
            )
            .unwrap_or_else(|error| panic!("failed to open durable Android coordinator state: {}", error.message))
        } else {
            MahayanaCoordinator::with_generation(coordinator_host, generation, 512)
        };
        Self {
            coordinator,
            mcp_oauth: OAuthForwarder::default(),
            mode,
            next_request_id: 0,
            runtime_call_control,
        }
    }

    pub fn set_remote_binding_json(&mut self, raw: Option<&str>) -> Result<(), String> {
        self.coordinator.host_mut().runtime.set_remote_binding_json(raw)
    }

    pub fn runtime_call_control(&self) -> Arc<RuntimeCallCancellationRegistry> {
        Arc::clone(&self.runtime_call_control)
    }

    pub fn dispatch_legacy_json(&mut self, input: &str) -> String {
        let envelope: Value = match serde_json::from_str(input) {
            Ok(value) => value,
            Err(error) => {
                return error_response(None, format!("invalid request JSON: {error}"));
            }
        };
        let method = match envelope.get("method").and_then(Value::as_str) {
            Some(method) if !method.trim().is_empty() => method,
            _ => {
                return error_response(
                    envelope.get("id").cloned(),
                    "method is required".into(),
                );
            }
        };
        let params = envelope
            .get("params")
            .cloned()
            .unwrap_or_else(|| json!({}));

        match method {
            "coordinator.status" => self.coordinator_status(envelope.get("id").cloned()),
            "coordinator.resync" => {
                self.coordinator_resync(envelope.get("id").cloned(), &params)
            }
            "coordinator.publishEvent" => {
                self.coordinator_publish_event(envelope.get("id").cloned(), &params)
            }
            "coordinator.clientSideToolV2.accept" => {
                self.coordinator_client_side_tool_v2_accept(envelope.get("id").cloned(), &params)
            }
            "coordinator.clientSideToolV2.replay" => {
                self.coordinator_client_side_tool_v2_replay(envelope.get("id").cloned())
            }
            "coordinator.mcpOAuth.register" => {
                self.coordinator_mcp_oauth_register(envelope.get("id").cloned(), &params)
            }
            "coordinator.mcpOAuth.complete" => {
                self.coordinator_mcp_oauth_complete(envelope.get("id").cloned(), &params)
            }
            "coordinator.mcpOAuth.status" => {
                success_response(
                    envelope.get("id").cloned(),
                    json!({"pendingCount": self.mcp_oauth.pending_count()}),
                )
            }
            "feature.interrupt" => {
                self.coordinator_interrupt(envelope.get("id").cloned(), &params)
            }
            _ => self.dispatch_host_request(&envelope, method, params),
        }
    }

    fn dispatch_host_request(&mut self, envelope: &Value, method: &str, params: Value) -> String {
        self.next_request_id = self.next_request_id.saturating_add(1);
        let request_id = envelope
            .get("id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .or_else(|| {
                (method == "feature.execute")
                    .then(|| {
                        params
                            .get("command")
                            .and_then(|command| command.get("requestId"))
                            .and_then(Value::as_str)
                            .filter(|value| !value.trim().is_empty())
                            .map(str::to_string)
                    })
                    .flatten()
            })
            .unwrap_or_else(|| format!("jni-{:016}", self.next_request_id));

        let request = CoordinatorRequest {
            protocol_version: COORDINATOR_PROTOCOL_VERSION,
            request_id: request_id.clone(),
            session_id: "android-process".into(),
            method: method.to_string(),
            params_json: params.to_string(),
            deadline_ms: None,
        };

        let reply = if method == "feature.execute" {
            self.coordinator.request_deferred(request)
        } else {
            self.coordinator.request(request)
        };

        let mut result = match reply.result_json {
            Ok(result_json) => serde_json::from_str::<Value>(&result_json)
                .unwrap_or_else(|_| Value::String(result_json)),
            Err(failure) => {
                return failure_response(envelope.get("id").cloned(), failure);
            }
        };

        if method == "feature.execute" {
            if result.get("accepted").and_then(Value::as_bool) == Some(true) {
                if let Some(operation_id) = result
                    .get("operationId")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                {
                    if let Err(failure) =
                        self.coordinator.bind_operation(&request_id, operation_id)
                    {
                        return failure_response(envelope.get("id").cloned(), failure);
                    }
                } else {
                    let failure = CoordinatorFailure::new(
                        CoordinatorFailureCode::ProtocolBreach,
                        "accepted feature.execute response is missing operationId",
                    );
                    return failure_response(envelope.get("id").cloned(), failure);
                }
            } else {
                let _ = self
                    .coordinator
                    .complete_request(&request_id, Ok(result.to_string()));
            }
        }

        if method == "feature.receive" {
            self.record_received_event(&mut result);
        }

        if method == "feature.auth.logout" {
            self.coordinator.retire_client_side_tool_v2_for_account_switch();
        }

        success_response(envelope.get("id").cloned(), result)
    }

    fn record_received_event(&mut self, result: &mut Value) {
        let Some(family) = result
            .get("type")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
        else {
            return;
        };
        let operation_id = result
            .get("operationId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string);
        let terminal = matches!(
            family.as_str(),
            "operation.completed" | "operation.interrupted" | "operation.failed"
        );
        let event = self.coordinator.record_operation_event(
            "android-process",
            family,
            result.to_string(),
            operation_id.as_deref(),
            terminal,
        );
        if let Some(object) = result.as_object_mut() {
            object.insert(
                "_coordinator".into(),
                json!({
                    "generation": self.coordinator.generation(),
                    "sequence": event.sequence,
                    "eventId": event.event_id,
                }),
            );
        }
    }

    fn coordinator_publish_event(&mut self, id: Option<Value>, params: &Value) -> String {
        let Some(event) = params.get("event") else {
            return error_response(id, "event is required".into());
        };
        let Some(family) = event
            .get("type")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
        else {
            return error_response(id, "event.type is required".into());
        };
        let operation_id = event
            .get("operationId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let terminal = matches!(
            family,
            "operation.completed" | "operation.interrupted" | "operation.failed"
        );
        let recorded = self.coordinator.record_operation_event(
            "android-process",
            family,
            event.to_string(),
            operation_id,
            terminal,
        );
        success_response(
            id,
            json!({
                "generation": self.coordinator.generation(),
                "sequence": recorded.sequence,
                "eventId": recorded.event_id,
            }),
        )
    }

    fn coordinator_client_side_tool_v2_accept(&mut self, id: Option<Value>, params: &Value) -> String {
        let Some(event) = params.get("event") else {
            return error_response(id, "client-side-tool-v2 event is required".into());
        };
        match self.coordinator.accept_client_side_tool_v2_wire(event.clone()) {
            Some(materialized) => success_response(
                id,
                serde_json::to_value(materialized).unwrap_or_else(|_| json!({"accepted":true})),
            ),
            None => error_response(id, "client-side-tool-v2 event rejected by Rust wire ingress".into()),
        }
    }

    fn coordinator_client_side_tool_v2_replay(&self, id: Option<Value>) -> String {
        success_response(
            id,
            serde_json::to_value(self.coordinator.replay_client_side_tool_v2())
                .unwrap_or_else(|_| json!([])),
        )
    }

    fn coordinator_mcp_oauth_register(&mut self, id: Option<Value>, params: &Value) -> String {
        let Some(state) = params.get("state").and_then(Value::as_str).filter(|value| !value.trim().is_empty()) else {
            return error_response(id, "state is required".into());
        };
        let Some(provider) = params.get("provider").and_then(Value::as_str).filter(|value| !value.trim().is_empty()) else {
            return error_response(id, "provider is required".into());
        };
        let server_id = params.get("serverId").and_then(Value::as_str).filter(|value| !value.trim().is_empty());
        let account_key = params.get("accountKey").and_then(Value::as_str).filter(|value| !value.trim().is_empty());
        let generation = params.get("generation").and_then(Value::as_u64);
        let has_identity = server_id.is_some() || account_key.is_some() || generation.is_some();
        if has_identity && (server_id.is_none() || account_key.is_none() || generation.is_none()) {
            return error_response(id, "OAuth watch identity requires serverId, accountKey, and generation".into());
        }
        match self.mcp_oauth.register_bound(state, provider, server_id, account_key, generation) {
            Ok(()) => success_response(id, json!({
                "registered": true,
                "pendingCount": self.mcp_oauth.pending_count(),
            })),
            Err(message) => error_response(id, message.into()),
        }
    }

    fn coordinator_mcp_oauth_complete(&mut self, id: Option<Value>, params: &Value) -> String {
        let Some(state) = params
            .get("state")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
        else {
            return error_response(id, "state is required".into());
        };
        let code = params
            .get("code")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string);
        let error = params
            .get("error")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string);
        let callback = OAuthCallback {
            state: state.to_string(),
            code,
            error,
        };
        if let Err(message) = callback.validate() {
            return error_response(id, message.into());
        }

        let forwarded = self.mcp_oauth.forward(callback.clone());
        let (provider, server_id, account_key, generation, callback) = match forwarded {
            Ok((registration, callback)) => (
                registration.provider,
                registration.server_id,
                registration.account_key,
                registration.generation,
                callback,
            ),
            Err(message) if self.mode == AndroidHostMode::Production => (
                "restored".to_string(),
                None,
                None,
                None,
                callback,
            ),
            Err(message) => return error_response(id, message.into()),
        };

        self.next_request_id = self.next_request_id.saturating_add(1);
        let host_request_id = format!("mcp-oauth-{:016}", self.next_request_id);
        let mut host_params = json!({
            "provider": provider.clone(),
            "state": callback.state,
            "code": callback.code,
            "error": callback.error,
        });
        if let (Some(server_id), Some(account_key), Some(generation)) =
            (server_id, account_key, generation)
        {
            host_params["serverId"] = json!(server_id);
            host_params["accountKey"] = json!(account_key);
            host_params["generation"] = json!(generation);
        }
        let host_reply = self.coordinator.request(CoordinatorRequest {
            protocol_version: COORDINATOR_PROTOCOL_VERSION,
            request_id: host_request_id,
            session_id: "android-process".into(),
            method: "feature.mcp.oauthComplete".into(),
            params_json: host_params.to_string(),
            deadline_ms: None,
        });
        let host_result = match host_reply.result_json {
            Ok(raw) => serde_json::from_str::<Value>(&raw)
                .unwrap_or_else(|_| json!({"status":"stale"})),
            Err(failure) => return failure_response(id, failure),
        };
        let outcome = host_result
            .get("outcome")
            .and_then(Value::as_str)
            .or_else(|| host_result.get("status").and_then(Value::as_str))
            .unwrap_or("stale");

        success_response(
            id,
            json!({
                "provider": provider,
                "state": state,
                "outcome": outcome,
                "pendingCount": self.mcp_oauth.pending_count(),
            }),
        )
    }

    fn coordinator_interrupt(&mut self, id: Option<Value>, params: &Value) -> String {
        let Some(operation_id) = params
            .get("operationId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
        else {
            return error_response(id, "operationId is required".into());
        };
        let reason = params
            .get("reason")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .or(Some("user"));

        let reply = self.coordinator.cancel_operation(operation_id, reason);
        match reply.result_json {
            Err(failure) if failure.code == CoordinatorFailureCode::Cancelled => {
                success_response(
                    id,
                    json!({
                        "operationId": operation_id,
                        "status": "interrupted",
                    }),
                )
            }
            Err(failure) => failure_response(id, failure),
            Ok(_) => success_response(
                id,
                json!({
                    "operationId": operation_id,
                    "status": "interrupted",
                }),
            ),
        }
    }

    fn coordinator_status(&self, id: Option<Value>) -> String {
        success_response(
            id,
            json!({
                "generation": self.coordinator.generation(),
                "latestSequence": self.coordinator.latest_sequence(),
                "activeRequestCount": self.coordinator.active_request_count(),
            }),
        )
    }

    fn coordinator_resync(&self, id: Option<Value>, params: &Value) -> String {
        let Some(generation) = params.get("generation").and_then(Value::as_u64) else {
            return error_response(id, "generation is required".into());
        };
        let after_sequence = params
            .get("afterSequence")
            .and_then(Value::as_u64)
            .unwrap_or(0);

        match self.coordinator.resync(ResyncRequest {
            generation,
            after_sequence,
        }) {
            Ok(snapshot) => {
                let events = snapshot
                    .events
                    .into_iter()
                    .map(|event| {
                        let payload = serde_json::from_str::<Value>(&event.payload_json)
                            .unwrap_or_else(|_| Value::String(event.payload_json));
                        json!({
                            "eventId": event.event_id,
                            "sessionId": event.session_id,
                            "sequence": event.sequence,
                            "family": event.family,
                            "payload": payload,
                        })
                    })
                    .collect::<Vec<_>>();
                success_response(
                    id,
                    json!({
                        "generation": snapshot.generation,
                        "latestSequence": snapshot.latest_sequence,
                        "events": events,
                    }),
                )
            }
            Err(failure) => failure_response(id, failure),
        }
    }

    pub fn generation(&self) -> u64 {
        self.coordinator.generation()
    }
}

fn success_response(id: Option<Value>, result: Value) -> String {
    json!({
        "id": id.unwrap_or(Value::Null),
        "ok": true,
        "result": result,
    })
    .to_string()
}

fn failure_response(id: Option<Value>, failure: CoordinatorFailure) -> String {
    json!({
        "id": id.unwrap_or(Value::Null),
        "ok": false,
        "error": format!("{}: {}", failure.code, failure.message),
        "errorCode": failure.code.to_string(),
    })
    .to_string()
}

fn error_response(id: Option<Value>, error: String) -> String {
    json!({
        "id": id.unwrap_or(Value::Null),
        "ok": false,
        "error": error,
    })
    .to_string()
}

#[cfg(target_os = "android")]
mod android_jni {
    use super::*;
    use jni::objects::{JObject, JString};
    use jni::sys::{jboolean, jint, jlong, jstring};
    use jni::JNIEnv;
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    fn runtime_controls() -> &'static Mutex<HashMap<jlong, Arc<RuntimeCallCancellationRegistry>>> {
        static CONTROLS: OnceLock<Mutex<HashMap<jlong, Arc<RuntimeCallCancellationRegistry>>>> =
            OnceLock::new();
        CONTROLS.get_or_init(|| Mutex::new(HashMap::new()))
    }

    fn register_runtime(runtime: AndroidNativeRuntime) -> jlong {
        let control = runtime.runtime_call_control();
        let handle = Box::into_raw(Box::new(runtime)) as jlong;
        if let Ok(mut controls) = runtime_controls().lock() {
            controls.insert(handle, control);
            handle
        } else {
            unsafe { drop(Box::from_raw(handle as *mut AndroidNativeRuntime)); }
            0
        }
    }

    fn control_for(handle: jlong) -> Option<Arc<RuntimeCallCancellationRegistry>> {
        runtime_controls().lock().ok()?.get(&handle).cloned()
    }

    fn create(
        mut env: JNIEnv,
        app_data_dir: JString,
        mode: AndroidHostMode,
        generation: u64,
    ) -> jlong {
        let path = match env.get_string(&app_data_dir) {
            Ok(value) => PathBuf::from(value.to_string_lossy().into_owned()),
            Err(_) => return 0,
        };
        register_runtime(AndroidNativeRuntime::new(path, mode, generation))
    }

    #[no_mangle]
    pub extern "system" fn Java_com_ombhrum_fabushi_core_MahayanaHost_nativeCreate(
        mut env: JNIEnv,
        _object: JObject,
        app_data_dir: JString,
        process_generation: jlong,
        initial_account_session_json: JString,
    ) -> jlong {
        let path = match env.get_string(&app_data_dir) {
            Ok(value) => PathBuf::from(value.to_string_lossy().into_owned()),
            Err(_) => return 0,
        };
        let initial_session = match env.get_string(&initial_account_session_json) {
            Ok(value) => value.to_string_lossy().into_owned(),
            Err(_) => return 0,
        };
        let generation = u64::try_from(process_generation).unwrap_or(1).max(1);
        register_runtime(AndroidNativeRuntime::new_with_account_session(
            path,
            AndroidHostMode::Production,
            generation,
            (!initial_session.trim().is_empty()).then_some(initial_session.as_str()),
        ))
    }

    #[no_mangle]
    pub extern "system" fn Java_com_ombhrum_fabushi_core_MahayanaHost_nativeCreateTest(
        env: JNIEnv,
        _object: JObject,
        app_data_dir: JString,
    ) -> jlong {
        create(env, app_data_dir, AndroidHostMode::Test, 1)
    }

    #[no_mangle]
    pub extern "system" fn Java_com_ombhrum_fabushi_core_MahayanaHost_nativeSetRemoteBinding(
        mut env: JNIEnv,
        _object: JObject,
        handle: jlong,
        binding_json: JString,
    ) -> jboolean {
        if handle == 0 { return 0; }
        let raw = match env.get_string(&binding_json) {
            Ok(value) => value.to_string_lossy().into_owned(),
            Err(_) => return 0,
        };
        let runtime = unsafe { &mut *(handle as *mut AndroidNativeRuntime) };
        runtime
            .set_remote_binding_json((!raw.trim().is_empty()).then_some(raw.as_str()))
            .is_ok() as jboolean
    }

    #[no_mangle]
    pub extern "system" fn Java_com_ombhrum_fabushi_core_MahayanaHost_nativeDispatch(
        mut env: JNIEnv,
        _object: JObject,
        handle: jlong,
        request_json: JString,
    ) -> jstring {
        if handle == 0 {
            return env
                .new_string("{\"ok\":false,\"error\":\"native runtime is not initialized\"}")
                .map(|value| value.into_raw())
                .unwrap_or(std::ptr::null_mut());
        }
        let input = match env.get_string(&request_json) {
            Ok(value) => value.to_string_lossy().into_owned(),
            Err(error) => {
                return env
                    .new_string(error_response(
                        None,
                        format!("invalid request string: {error}"),
                    ))
                    .map(|value| value.into_raw())
                    .unwrap_or(std::ptr::null_mut())
            }
        };
        let runtime = unsafe { &mut *(handle as *mut AndroidNativeRuntime) };
        env.new_string(runtime.dispatch_legacy_json(&input))
            .map(|value| value.into_raw())
            .unwrap_or(std::ptr::null_mut())
    }

    #[no_mangle]
    pub extern "system" fn Java_com_ombhrum_fabushi_core_MahayanaHost_nativeSignalRuntimeCancel(
        mut env: JNIEnv,
        _object: JObject,
        handle: jlong,
        request_id: JString,
    ) -> jboolean {
        if handle == 0 { return 0; }
        let request_id = match env.get_string(&request_id) {
            Ok(value) => value.to_string_lossy().into_owned(),
            Err(_) => return 0,
        };
        control_for(handle).is_some_and(|control| control.signal_request(&request_id)) as jboolean
    }

    #[no_mangle]
    pub extern "system" fn Java_com_ombhrum_fabushi_core_MahayanaHost_nativeSignalRuntimePluginCancel(
        mut env: JNIEnv,
        _object: JObject,
        handle: jlong,
        plugin_id: JString,
    ) -> jint {
        if handle == 0 { return 0; }
        let plugin_id = match env.get_string(&plugin_id) {
            Ok(value) => value.to_string_lossy().into_owned(),
            Err(_) => return 0,
        };
        control_for(handle).map(|control| control.signal_plugin(&plugin_id) as jint).unwrap_or(0)
    }

    #[no_mangle]
    pub extern "system" fn Java_com_ombhrum_fabushi_core_MahayanaHost_nativeSignalRuntimePermissionCancel(
        mut env: JNIEnv,
        _object: JObject,
        handle: jlong,
        plugin_id: JString,
        permission: JString,
    ) -> jint {
        if handle == 0 { return 0; }
        let plugin_id = match env.get_string(&plugin_id) {
            Ok(value) => value.to_string_lossy().into_owned(),
            Err(_) => return 0,
        };
        let permission = match env.get_string(&permission) {
            Ok(value) => value.to_string_lossy().into_owned(),
            Err(_) => return 0,
        };
        control_for(handle).map(|control| control.signal_permission(&plugin_id, &permission) as jint).unwrap_or(0)
    }

    #[no_mangle]
    pub extern "system" fn Java_com_ombhrum_fabushi_core_MahayanaHost_nativeSignalAllRuntimeCalls(
        _env: JNIEnv,
        _object: JObject,
        handle: jlong,
    ) -> jint {
        if handle == 0 { return 0; }
        control_for(handle).map(|control| control.signal_all() as jint).unwrap_or(0)
    }

    #[no_mangle]
    pub extern "system" fn Java_com_ombhrum_fabushi_core_MahayanaHost_nativeDestroy(
        _env: JNIEnv,
        _object: JObject,
        handle: jlong,
    ) {
        if handle != 0 {
            if let Ok(mut controls) = runtime_controls().lock() {
                controls.remove(&handle);
            }
            unsafe { drop(Box::from_raw(handle as *mut AndroidNativeRuntime)); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(runtime: &mut AndroidNativeRuntime, request: Value) -> Value {
        serde_json::from_str(&runtime.dispatch_legacy_json(&request.to_string())).unwrap()
    }

    fn test_root(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "fabushi-jni-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn client_side_tool_v2_wire_ingress_is_reachable_only_through_rust_coordinator() {
        let mut runtime =
            AndroidNativeRuntime::new(test_root("client-tool-v2"), AndroidHostMode::Test, 3);
        let accepted = call(
            &mut runtime,
            json!({
                "id":"tool-wire-1",
                "method":"coordinator.clientSideToolV2.accept",
                "params":{"event":{
                    "version":1,
                    "kind":"call",
                    "accountSlot":"host",
                    "agentId":"agent-a",
                    "epoch":"epoch-a",
                    "sequence":1,
                    "message":{
                        "encoding":"protobuf-base64",
                        "messageType":"aiserver.v1.ClientSideToolV2Call",
                        "bytes":"GgZjYWxsLTE="
                    }
                }}
            }),
        );
        assert_eq!(accepted["ok"], true);
        assert_eq!(accepted["result"]["sequence"], 1);

        let replay = call(
            &mut runtime,
            json!({"id":"tool-wire-replay","method":"coordinator.clientSideToolV2.replay","params":{}}),
        );
        assert_eq!(replay["ok"], true);
        assert_eq!(replay["result"].as_array().unwrap().len(), 1);

        let rejected = call(
            &mut runtime,
            json!({
                "id":"tool-wire-bad",
                "method":"coordinator.clientSideToolV2.accept",
                "params":{"event":{
                    "version":2,
                    "kind":"call",
                    "accountSlot":"host",
                    "agentId":"agent-a",
                    "epoch":"epoch-a",
                    "sequence":2,
                    "message":{
                        "encoding":"protobuf-base64",
                        "messageType":"aiserver.v1.ClientSideToolV2Call",
                        "bytes":"GgZjYWxsLTI="
                    }
                }}
            }),
        );
        assert_eq!(rejected["ok"], false);
    }

    #[test]
    fn agent_lifecycle_controls_are_reachable_only_through_coordinator_host_dispatch() {
        let mut runtime =
            AndroidNativeRuntime::new(test_root("agent-lifecycle"), AndroidHostMode::Test, 11);

        let quiesced = call(
            &mut runtime,
            json!({
                "method":"feature.agent.upgradeQuiesce",
                "params":{"quiescing":true}
            }),
        );
        assert_eq!(quiesced["ok"], true);
        assert_eq!(quiesced["result"]["quiescing"], true);

        let blocked = call(
            &mut runtime,
            json!({
                "method":"feature.execute",
                "params":{"command":{
                    "type":"chat.send",
                    "requestId":"jni-quiesced-turn",
                    "text":"must not dispatch"
                }}
            }),
        );
        assert_eq!(blocked["ok"], false);

        let resumed = call(
            &mut runtime,
            json!({
                "method":"feature.agent.upgradeQuiesce",
                "params":{"quiescing":false}
            }),
        );
        assert_eq!(resumed["ok"], true);

        let accepted = call(
            &mut runtime,
            json!({
                "method":"feature.execute",
                "params":{"command":{
                    "type":"chat.send",
                    "requestId":"jni-live-turn",
                    "text":"dispatch"
                }}
            }),
        );
        assert_eq!(accepted["ok"], true);
        assert_eq!(accepted["result"]["accepted"], true);
    }

    #[test]
    fn production_reopen_surfaces_coordinator_outcome_unknown_instead_of_replaying_request() {
        let root = test_root("coordinator-reopen");
        {
            let mut runtime = AndroidNativeRuntime::new(root.clone(), AndroidHostMode::Production, 3);
            runtime.coordinator.begin_request(&CoordinatorRequest {
                protocol_version: COORDINATOR_PROTOCOL_VERSION,
                request_id: "persisted-stream".into(),
                session_id: "android-process".into(),
                method: "feature.execute".into(),
                params_json: "{}".into(),
                deadline_ms: None,
            }).unwrap();
            runtime.coordinator.bind_operation("persisted-stream", "persisted-stream").unwrap();
            let status = call(&mut runtime, json!({"method":"coordinator.status","params":{}}));
            assert_eq!(status["result"]["activeRequestCount"], 1);
        }

        let mut reopened = AndroidNativeRuntime::new(root.clone(), AndroidHostMode::Production, 3);
        let status = call(&mut reopened, json!({"method":"coordinator.status","params":{}}));
        assert_eq!(status["result"]["generation"], 4);
        assert_eq!(status["result"]["activeRequestCount"], 0);
        let replay = call(
            &mut reopened,
            json!({"method":"coordinator.resync","params":{"generation":4,"afterSequence":0}}),
        );
        assert_eq!(replay["ok"], true);
        assert_eq!(replay["result"]["events"][0]["family"], "operation.outcome-unknown");
        assert_eq!(replay["result"]["events"][0]["payload"]["requestId"], "persisted-stream");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_jni_envelope_routes_through_coordinator_into_test_host() {
        let mut runtime =
            AndroidNativeRuntime::new(test_root("test"), AndroidHostMode::Test, 5);
        let response = call(
            &mut runtime,
            json!({"method":"feature.info","params":{}}),
        );
        assert_eq!(response["ok"], true);
        assert_eq!(response["result"]["platform"], "android");
        assert!(response["result"]["runtimeVersion"]
            .as_str()
            .unwrap()
            .contains("test"));
        assert_eq!(runtime.generation(), 5);
    }

    #[test]
    fn streaming_operation_is_active_until_terminal_event_and_replayable() {
        let mut runtime =
            AndroidNativeRuntime::new(test_root("stream"), AndroidHostMode::Test, 9);

        let accepted = call(
            &mut runtime,
            json!({
                "method":"feature.execute",
                "params":{"command":{
                    "type":"chat.send",
                    "requestId":"stream-1",
                    "text":"hello"
                }}
            }),
        );
        assert_eq!(accepted["ok"], true);
        assert_eq!(accepted["result"]["operationId"], "stream-1");

        let status = call(&mut runtime, json!({"method":"coordinator.status","params":{}}));
        assert_eq!(status["result"]["generation"], 9);
        assert_eq!(status["result"]["activeRequestCount"], 1);

        let mut terminal_sequence = 0;
        for _ in 0..8 {
            let event = call(
                &mut runtime,
                json!({"method":"feature.receive","params":{}}),
            );
            let result = &event["result"];
            if result["type"] == "operation.completed" {
                terminal_sequence = result["_coordinator"]["sequence"].as_u64().unwrap();
                break;
            }
        }
        assert!(terminal_sequence > 0);

        let settled = call(&mut runtime, json!({"method":"coordinator.status","params":{}}));
        assert_eq!(settled["result"]["activeRequestCount"], 0);

        let replay = call(
            &mut runtime,
            json!({
                "method":"coordinator.resync",
                "params":{"generation":9,"afterSequence":0}
            }),
        );
        assert_eq!(replay["ok"], true);
        assert!(replay["result"]["events"].as_array().unwrap().len() >= 2);

        let stale = call(
            &mut runtime,
            json!({
                "method":"coordinator.resync",
                "params":{"generation":8,"afterSequence":0}
            }),
        );
        assert_eq!(stale["ok"], false);
        assert_eq!(stale["errorCode"], "stale-generation");
    }

    #[test]
    fn android_adapter_events_share_native_replay_sequence() {
        let mut runtime =
            AndroidNativeRuntime::new(test_root("adapter"), AndroidHostMode::Test, 4);
        let published = call(
            &mut runtime,
            json!({
                "method":"coordinator.publishEvent",
                "params":{"event":{
                    "type":"mcp.result",
                    "operationId":"external-1",
                    "tool":"files.read"
                }}
            }),
        );
        assert_eq!(published["ok"], true);
        assert_eq!(published["result"]["generation"], 4);
        assert_eq!(published["result"]["sequence"], 1);

        let replay = call(
            &mut runtime,
            json!({
                "method":"coordinator.resync",
                "params":{"generation":4,"afterSequence":0}
            }),
        );
        assert_eq!(replay["result"]["events"][0]["family"], "mcp.result");
        assert_eq!(
            replay["result"]["events"][0]["payload"]["tool"],
            "files.read"
        );
    }

    #[test]
    fn interrupt_uses_coordinator_cancel_and_emits_terminal_event() {
        let mut runtime =
            AndroidNativeRuntime::new(test_root("cancel"), AndroidHostMode::Test, 3);
        let accepted = call(
            &mut runtime,
            json!({
                "method":"feature.execute",
                "params":{"command":{
                    "type":"runtime.longTask",
                    "requestId":"long-1"
                }}
            }),
        );
        assert_eq!(accepted["result"]["operationId"], "long-1");

        let interrupted = call(
            &mut runtime,
            json!({
                "method":"feature.interrupt",
                "params":{"operationId":"long-1"}
            }),
        );
        assert_eq!(interrupted["ok"], true);
        assert_eq!(interrupted["result"]["status"], "interrupted");

        let event = call(
            &mut runtime,
            json!({"method":"feature.receive","params":{}}),
        );
        assert_eq!(event["result"]["type"], "operation.started");
        let terminal = call(
            &mut runtime,
            json!({"method":"feature.receive","params":{}}),
        );
        assert_eq!(terminal["result"]["type"], "operation.interrupted");
        assert!(terminal["result"]["_coordinator"]["sequence"]
            .as_u64()
            .unwrap()
            > 0);
    }

    #[test]
    fn unknown_renderer_method_fails_closed() {
        let mut runtime =
            AndroidNativeRuntime::new(test_root("prod"), AndroidHostMode::Production, 1);
        let response = call(
            &mut runtime,
            json!({"method":"renderer.execAnything","params":{}}),
        );
        assert_eq!(response["ok"], false);
        assert!(response["error"]
            .as_str()
            .unwrap()
            .contains("unknown host method"));
    }
    #[test]
    fn mcp_oauth_callback_is_single_use_and_host_event_does_not_echo_code() {
        let mut runtime =
            AndroidNativeRuntime::new(test_root("mcp-oauth"), AndroidHostMode::Test, 13);
        let state = "0123456789abcdef0123456789abcdef";

        let watch = call(
            &mut runtime,
            json!({
                "method":"feature.mcp.authWatch.register",
                "params":{
                    "serverId":"17",
                    "serverName":"GitHub",
                    "serverUrl":"https://mcp.example.test",
                    "accountKey":"default",
                    "requestingAgentId":"agent-a"
                }
            }),
        );
        assert_eq!(watch["ok"], true);
        let generation = watch["result"]["generation"].as_u64().unwrap();

        let registered = call(
            &mut runtime,
            json!({
                "method":"coordinator.mcpOAuth.register",
                "params":{
                    "state":state,
                    "provider":"github",
                    "serverId":"17",
                    "accountKey":"default",
                    "generation":generation
                }
            }),
        );
        assert_eq!(registered["ok"], true);
        assert_eq!(registered["result"]["registered"], true);

        let completed = call(
            &mut runtime,
            json!({
                "method":"coordinator.mcpOAuth.complete",
                "params":{"state":state,"code":"secret-oauth-code"}
            }),
        );
        assert_eq!(completed["ok"], true);
        assert_eq!(completed["result"]["provider"], "github");
        assert_eq!(completed["result"]["outcome"], "pending-validation");
        assert!(completed["result"].get("code").is_none());

        let duplicate = call(
            &mut runtime,
            json!({
                "method":"coordinator.mcpOAuth.complete",
                "params":{"state":state,"code":"second-code"}
            }),
        );
        assert_eq!(duplicate["ok"], false);

        let mut saw_callback_acceptance = false;
        for _ in 0..8 {
            let event = call(
                &mut runtime,
                json!({"method":"feature.receive","params":{}}),
            );
            let result = &event["result"];
            if result["type"] == "mcp.auth.callback.accepted" {
                saw_callback_acceptance = true;
                assert_eq!(result["provider"], "github");
                assert!(result.get("code").is_none());
                assert!(!result.to_string().contains("secret-oauth-code"));
                break;
            }
        }
        assert!(saw_callback_acceptance);
    }

}
