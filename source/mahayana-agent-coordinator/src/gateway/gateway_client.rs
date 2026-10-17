use std::fmt;

use super::gateway_errors::GatewayError;

#[derive(Clone, PartialEq, Eq)]
pub struct GatewayAuthContext {
    bearer_credential: String,
    account_fence: String,
    account_epoch: u64,
    operation_id: String,
    request_id: String,
    permission_grant_id: String,
}

impl GatewayAuthContext {
    pub fn new(
        bearer_credential: impl Into<String>,
        account_fence: impl Into<String>,
        account_epoch: u64,
        operation_id: impl Into<String>,
        request_id: impl Into<String>,
        permission_grant_id: impl Into<String>,
    ) -> Result<Self, &'static str> {
        let context = Self {
            bearer_credential: bearer_credential.into(),
            account_fence: account_fence.into(),
            account_epoch,
            operation_id: operation_id.into(),
            request_id: request_id.into(),
            permission_grant_id: permission_grant_id.into(),
        };
        context.validate()?;
        Ok(context)
    }

    pub fn bearer_credential(&self) -> &str {
        &self.bearer_credential
    }

    pub fn account_fence(&self) -> &str {
        &self.account_fence
    }

    pub fn account_epoch(&self) -> u64 {
        self.account_epoch
    }

    pub fn operation_id(&self) -> &str {
        &self.operation_id
    }

    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    pub fn permission_grant_id(&self) -> &str {
        &self.permission_grant_id
    }

    fn validate(&self) -> Result<(), &'static str> {
        if self.bearer_credential.trim().is_empty() {
            return Err("gateway bearer credential must not be empty");
        }
        if self.account_fence.trim().is_empty() {
            return Err("gateway account fence must not be empty");
        }
        if self.account_epoch == 0 {
            return Err("gateway account epoch must be positive");
        }
        if self.operation_id.trim().is_empty() {
            return Err("gateway operation identity must not be empty");
        }
        if self.request_id.trim().is_empty() {
            return Err("gateway request identity must not be empty");
        }
        if self.permission_grant_id.trim().is_empty() {
            return Err("gateway permission grant identity must not be empty");
        }
        Ok(())
    }
}

impl fmt::Debug for GatewayAuthContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GatewayAuthContext")
            .field("bearer_credential", &"<redacted>")
            .field("account_fence", &self.account_fence)
            .field("account_epoch", &self.account_epoch)
            .field("operation_id", &self.operation_id)
            .field("request_id", &self.request_id)
            .field("permission_grant_id", &self.permission_grant_id)
            .finish()
    }
}

pub trait HttpGatewayTransport {
    fn post_authenticated_command(
        &mut self,
        base_url: &str,
        method: &str,
        args_json: &str,
        auth: &GatewayAuthContext,
    ) -> Result<String, GatewayError>;
}

pub struct GatewayClient<T: HttpGatewayTransport> {
    base_url: String,
    transport: T,
}

impl<T: HttpGatewayTransport> GatewayClient<T> {
    pub fn new(base_url: impl Into<String>, transport: T) -> Result<Self, &'static str> {
        let base_url = base_url.into();
        if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
            return Err("gateway base URL must be HTTP(S)");
        }
        Ok(Self {
            base_url,
            transport,
        })
    }

    pub fn dispatch_authenticated(
        &mut self,
        method: &str,
        args_json: &str,
        auth: &GatewayAuthContext,
    ) -> Result<String, GatewayError> {
        if method.trim().is_empty() {
            return Err(GatewayError::Command("method must not be empty".into()));
        }
        auth.validate().map_err(|error| GatewayError::Command(error.into()))?;
        self.transport
            .post_authenticated_command(&self.base_url, method, args_json, auth)
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn into_transport(self) -> T {
        self.transport
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingTransport {
        calls: Vec<(String, String, String, u64, String, String, String, bool)>,
    }

    impl HttpGatewayTransport for RecordingTransport {
        fn post_authenticated_command(
            &mut self,
            base_url: &str,
            method: &str,
            args_json: &str,
            auth: &GatewayAuthContext,
        ) -> Result<String, GatewayError> {
            self.calls.push((
                base_url.into(),
                method.into(),
                auth.account_fence().into(),
                auth.account_epoch(),
                auth.operation_id().into(),
                auth.request_id().into(),
                auth.permission_grant_id().into(),
                !auth.bearer_credential().is_empty(),
            ));
            Ok(args_json.into())
        }
    }

    fn auth() -> GatewayAuthContext {
        GatewayAuthContext::new(
            "secret-token",
            "account-a:epoch-7",
            7,
            "operation-1",
            "request-1",
            "grant-1",
        )
        .unwrap()
    }

    #[test]
    fn authenticated_dispatch_preserves_account_and_operation_fences() {
        let transport = RecordingTransport::default();
        let mut client = GatewayClient::new("https://remote.example", transport).unwrap();
        let output = client
            .dispatch_authenticated("runner.execute", r#"{"command":"computer.use"}"#, &auth())
            .unwrap();
        assert_eq!(output, r#"{"command":"computer.use"}"#);

        let transport = client.into_transport();
        assert_eq!(transport.calls.len(), 1);
        let call = &transport.calls[0];
        assert_eq!(call.0, "https://remote.example");
        assert_eq!(call.1, "runner.execute");
        assert_eq!(call.2, "account-a:epoch-7");
        assert_eq!(call.3, 7);
        assert_eq!(call.4, "operation-1");
        assert_eq!(call.5, "request-1");
        assert_eq!(call.6, "grant-1");
        assert!(call.7);
    }

    #[test]
    fn auth_context_rejects_missing_epoch_fence_and_grant() {
        assert!(GatewayAuthContext::new("secret", "", 7, "op", "req", "grant").is_err());
        assert!(GatewayAuthContext::new("secret", "fence", 0, "op", "req", "grant").is_err());
        assert!(GatewayAuthContext::new("secret", "fence", 7, "op", "req", "").is_err());
    }

    #[test]
    fn debug_output_never_contains_bearer_credential() {
        let rendered = format!("{:?}", auth());
        assert!(!rendered.contains("secret-token"));
        assert!(rendered.contains("<redacted>"));
    }
}
