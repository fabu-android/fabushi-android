use fabushi_android_shared::{CoordinatorFailure, CoordinatorFailureCode, CoordinatorReply};
use fabushi_android_shared::rpc::coordinator::classify_coordinator_method;
use super::gateway_errors::GatewayError;

pub trait GatewayCommandClient {
    fn dispatch(&mut self, method: &str, args_json: &str) -> Result<String, GatewayError>;
}

pub fn dispatch<C: GatewayCommandClient>(client: &mut C, request_id: &str, method: &str, args_json: &str) -> CoordinatorReply {
    if classify_coordinator_method(method).is_none() {
        return CoordinatorReply::failed(
            request_id,
            CoordinatorFailure::new(CoordinatorFailureCode::UnknownRequest, format!("no coordinator method named {method}")),
        );
    }
    match client.dispatch(method, args_json) {
        Ok(value) => CoordinatorReply::ok(request_id, value),
        Err(GatewayError::Unreachable { kind, message }) => CoordinatorReply::failed(
            request_id,
            CoordinatorFailure::new(CoordinatorFailureCode::GatewayUnavailable, format!("{kind}: {message}")),
        ),
        Err(error) => CoordinatorReply::failed(
            request_id,
            CoordinatorFailure::new(CoordinatorFailureCode::Internal, format!("{error:?}")),
        ),
    }
}
