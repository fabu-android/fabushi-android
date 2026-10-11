use fabushi_android_shared::webauthn_gateway::{
    webauthn_origin_class, WebAuthnCeremony, WebAuthnOriginClass, WebAuthnRequestFrame,
    WebAuthnResponseFrame, WebAuthnStage, WebAuthnStageOutcome,
    SAND_NO_WEBAUTHN_MACHINE_MESSAGE, SAND_WEBAUTHN_CEREMONY_TIMEOUT_MS,
    SAND_WEBAUTHN_LIVENESS_WINDOW_MS, SAND_WEBAUTHN_MACHINE_UNAVAILABLE_MESSAGE,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DesktopStage {
    pub stage: WebAuthnStage,
    pub outcome: WebAuthnStageOutcome,
}

pub fn stage_cause(stage: DesktopStage) -> Option<&'static str> {
    match stage.outcome {
        WebAuthnStageOutcome::Declined => Some("consent_declined"),
        WebAuthnStageOutcome::Failed => Some(match stage.stage {
            WebAuthnStage::Sign => "sign_failed",
            WebAuthnStage::Grant => "desktop_failed",
        }),
        WebAuthnStageOutcome::Ok => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebAuthnBridgeReport {
    pub request_id: String,
    pub origin_class: WebAuthnOriginClass,
    pub ceremony_kind: String,
    pub stage: String,
    pub outcome: String,
    pub cause: Option<String>,
    pub provider_count: Option<usize>,
    pub live_provider_count: Option<usize>,
    pub raw_dom_error_name: Option<String>,
    pub raw_sign_error_class: Option<String>,
    pub elapsed_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WebAuthnBridgeSettlement {
    CredentialJson(String),
    Error {
        name: String,
        message: String,
        code: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WebAuthnBridgeError {
    NoProvider { message: &'static str },
    ProviderStale { message: &'static str },
    DispatchFailed(String),
    UnknownRequest,
    TimedOut,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebAuthnBridgeRequest {
    pub provider_id: String,
    pub request_id: String,
    pub frame: WebAuthnRequestFrame,
    pub deadline_at_ms: u64,
}

#[derive(Clone, Debug)]
struct Provider {
    id: String,
    last_seen_at_ms: u64,
    has_heartbeat: bool,
    computer_id: Option<String>,
    label: Option<String>,
}

#[derive(Clone, Debug)]
struct Pending {
    provider_id: String,
    origin_class: WebAuthnOriginClass,
    ceremony_kind: String,
    started_at_ms: u64,
    deadline_at_ms: u64,
    last_stage: Option<DesktopStage>,
}

#[derive(Clone, Debug)]
pub struct WebAuthnBridge {
    next_id: u64,
    providers: BTreeMap<String, Provider>,
    pending: BTreeMap<String, Pending>,
    cancelled: BTreeSet<String>,
    reports: Vec<WebAuthnBridgeReport>,
    timeout_ms: u64,
    liveness_window_ms: u64,
}

impl Default for WebAuthnBridge {
    fn default() -> Self {
        Self::new(SAND_WEBAUTHN_CEREMONY_TIMEOUT_MS, SAND_WEBAUTHN_LIVENESS_WINDOW_MS)
    }
}

impl WebAuthnBridge {
    pub fn new(timeout_ms: u64, liveness_window_ms: u64) -> Self {
        Self {
            next_id: 0,
            providers: BTreeMap::new(),
            pending: BTreeMap::new(),
            cancelled: BTreeSet::new(),
            reports: Vec::new(),
            timeout_ms: timeout_ms.max(1),
            liveness_window_ms: liveness_window_ms.max(1),
        }
    }

    pub fn register_provider(&mut self, now_ms: u64) -> (String, WebAuthnRequestFrame) {
        self.next_id = self.next_id.saturating_add(1);
        let id = format!("provider-{:016}", self.next_id);
        self.providers.insert(
            id.clone(),
            Provider {
                id: id.clone(),
                last_seen_at_ms: now_ms,
                has_heartbeat: false,
                computer_id: None,
                label: None,
            },
        );
        (id.clone(), WebAuthnRequestFrame::Welcome { provider_id: id })
    }

    pub fn unregister_provider(&mut self, provider_id: &str) {
        self.providers.remove(provider_id);
    }

    pub fn submit_responses(
        &mut self,
        now_ms: u64,
        provider_id: Option<&str>,
        frames: &[WebAuthnResponseFrame],
    ) -> Vec<(String, WebAuthnBridgeSettlement)> {
        if let Some(provider_id) = provider_id {
            if let Some(provider) = self.providers.get_mut(provider_id) {
                provider.last_seen_at_ms = now_ms;
            }
        }

        let mut settlements = Vec::new();
        for frame in frames {
            match frame {
                WebAuthnResponseFrame::Hello { computer_id, label } => {
                    if let Some(provider_id) = provider_id {
                        if let Some(provider) = self.providers.get_mut(provider_id) {
                            if computer_id.is_some() {
                                provider.computer_id = computer_id.clone();
                            }
                            if label.is_some() {
                                provider.label = label.clone();
                            }
                        }
                    }
                }
                WebAuthnResponseFrame::Ping => {
                    if let Some(provider_id) = provider_id {
                        if let Some(provider) = self.providers.get_mut(provider_id) {
                            provider.has_heartbeat = true;
                            provider.last_seen_at_ms = now_ms;
                        }
                    }
                }
                WebAuthnResponseFrame::Stage {
                    request_id,
                    stage,
                    outcome,
                } => {
                    let desktop_stage = DesktopStage {
                        stage: *stage,
                        outcome: *outcome,
                    };
                    let report_data = if let Some(pending) = self.pending.get_mut(request_id) {
                        pending.last_stage = Some(desktop_stage);
                        Some((
                            pending.origin_class,
                            pending.ceremony_kind.clone(),
                            pending.started_at_ms,
                        ))
                    } else {
                        None
                    };
                    if let Some((origin_class, ceremony_kind, started_at_ms)) = report_data {
                        self.reports.push(WebAuthnBridgeReport {
                            request_id: request_id.clone(),
                            origin_class,
                            ceremony_kind,
                            stage: match stage {
                                WebAuthnStage::Grant => "grant",
                                WebAuthnStage::Sign => "sign",
                            }
                            .into(),
                            outcome: match outcome {
                                WebAuthnStageOutcome::Ok => "ok",
                                WebAuthnStageOutcome::Declined => "declined",
                                WebAuthnStageOutcome::Failed => "failed",
                            }
                            .into(),
                            cause: stage_cause(desktop_stage).map(str::to_string),
                            provider_count: None,
                            live_provider_count: None,
                            raw_dom_error_name: None,
                            raw_sign_error_class: None,
                            elapsed_ms: now_ms.saturating_sub(started_at_ms),
                        });
                    }
                }
                WebAuthnResponseFrame::Result {
                    request_id,
                    credential_json,
                } => {
                    if let Some(settlement) = self.settle(
                        now_ms,
                        request_id,
                        WebAuthnBridgeSettlement::CredentialJson(credential_json.clone()),
                    ) {
                        settlements.push((request_id.clone(), settlement));
                    }
                }
                WebAuthnResponseFrame::Error {
                    request_id,
                    name,
                    message,
                    code,
                } => {
                    if let Some(settlement) = self.settle(
                        now_ms,
                        request_id,
                        WebAuthnBridgeSettlement::Error {
                            name: name.clone(),
                            message: message.clone(),
                            code: code.clone(),
                        },
                    ) {
                        settlements.push((request_id.clone(), settlement));
                    }
                }
            }
        }
        settlements
    }

    pub fn request_ceremony(
        &mut self,
        now_ms: u64,
        ceremony: WebAuthnCeremony,
    ) -> Result<WebAuthnBridgeRequest, WebAuthnBridgeError> {
        let request_id = self.next_request_id();
        let origin_class = webauthn_origin_class(&ceremony.origin);
        let ceremony_kind = if ceremony.kind == "create" { "create" } else { "get" }.to_string();
        let (provider_count, live_provider_count) = self.provider_counts(now_ms);
        let provider = self.select_provider(now_ms);

        let Some(provider) = provider else {
            let cause = if provider_count == 0 {
                "no_provider"
            } else {
                "provider_stale"
            };
            self.reports.push(WebAuthnBridgeReport {
                request_id: request_id.clone(),
                origin_class,
                ceremony_kind: ceremony_kind.clone(),
                stage: "request".into(),
                outcome: "failed".into(),
                cause: Some(cause.into()),
                provider_count: Some(provider_count),
                live_provider_count: Some(live_provider_count),
                raw_dom_error_name: None,
                raw_sign_error_class: None,
                elapsed_ms: 0,
            });
            self.reports.push(WebAuthnBridgeReport {
                request_id,
                origin_class,
                ceremony_kind,
                stage: "complete".into(),
                outcome: "failed".into(),
                cause: Some(cause.into()),
                provider_count: None,
                live_provider_count: None,
                raw_dom_error_name: None,
                raw_sign_error_class: None,
                elapsed_ms: 0,
            });
            return Err(if provider_count == 0 {
                WebAuthnBridgeError::NoProvider {
                    message: SAND_NO_WEBAUTHN_MACHINE_MESSAGE,
                }
            } else {
                WebAuthnBridgeError::ProviderStale {
                    message: SAND_WEBAUTHN_MACHINE_UNAVAILABLE_MESSAGE,
                }
            });
        };

        let deadline_at_ms = now_ms.saturating_add(self.timeout_ms);
        self.pending.insert(
            request_id.clone(),
            Pending {
                provider_id: provider.id.clone(),
                origin_class,
                ceremony_kind: ceremony_kind.clone(),
                started_at_ms: now_ms,
                deadline_at_ms,
                last_stage: None,
            },
        );
        self.reports.push(WebAuthnBridgeReport {
            request_id: request_id.clone(),
            origin_class,
            ceremony_kind,
            stage: "request".into(),
            outcome: "ok".into(),
            cause: None,
            provider_count: Some(provider_count),
            live_provider_count: Some(live_provider_count),
            raw_dom_error_name: None,
            raw_sign_error_class: None,
            elapsed_ms: 0,
        });
        Ok(WebAuthnBridgeRequest {
            provider_id: provider.id,
            request_id: request_id.clone(),
            frame: WebAuthnRequestFrame::Ceremony {
                request_id,
                ceremony,
            },
            deadline_at_ms,
        })
    }

    pub fn expire(
        &mut self,
        now_ms: u64,
    ) -> Vec<(String, String, WebAuthnRequestFrame)> {
        let expired = self
            .pending
            .iter()
            .filter(|(_, pending)| now_ms >= pending.deadline_at_ms)
            .map(|(request_id, pending)| {
                (
                    request_id.clone(),
                    pending.provider_id.clone(),
                    pending.origin_class,
                    pending.ceremony_kind.clone(),
                    pending.started_at_ms,
                )
            })
            .collect::<Vec<_>>();
        let mut cancellations = Vec::new();
        for (request_id, provider_id, origin_class, ceremony_kind, started_at_ms) in expired {
            self.pending.remove(&request_id);
            self.cancelled.insert(request_id.clone());
            self.reports.push(WebAuthnBridgeReport {
                request_id: request_id.clone(),
                origin_class,
                ceremony_kind,
                stage: "complete".into(),
                outcome: "timeout".into(),
                cause: Some("timeout".into()),
                provider_count: None,
                live_provider_count: None,
                raw_dom_error_name: None,
                raw_sign_error_class: None,
                elapsed_ms: now_ms.saturating_sub(started_at_ms),
            });
            cancellations.push((
                provider_id,
                request_id.clone(),
                WebAuthnRequestFrame::Cancel { request_id },
            ));
        }
        cancellations
    }

    pub fn provider_counts(&self, now_ms: u64) -> (usize, usize) {
        let live = self
            .providers
            .values()
            .filter(|provider| {
                !provider.has_heartbeat
                    || now_ms.saturating_sub(provider.last_seen_at_ms) <= self.liveness_window_ms
            })
            .count();
        (self.providers.len(), live)
    }

    pub fn take_reports(&mut self) -> Vec<WebAuthnBridgeReport> {
        std::mem::take(&mut self.reports)
    }

    pub fn is_pending(&self, request_id: &str) -> bool {
        self.pending.contains_key(request_id)
    }

    pub fn was_cancelled(&self, request_id: &str) -> bool {
        self.cancelled.contains(request_id)
    }

    fn settle(
        &mut self,
        now_ms: u64,
        request_id: &str,
        settlement: WebAuthnBridgeSettlement,
    ) -> Option<WebAuthnBridgeSettlement> {
        let pending = self.pending.remove(request_id)?;
        let (outcome, cause, raw_dom_error_name, raw_sign_error_class) = match &settlement {
            WebAuthnBridgeSettlement::CredentialJson(_) => ("ok", None, None, None),
            WebAuthnBridgeSettlement::Error { name, code, .. } => (
                "failed",
                pending
                    .last_stage
                    .and_then(stage_cause)
                    .unwrap_or("desktop_failed")
                    .into(),
                Some(name.clone()),
                code.clone(),
            ),
        };
        self.reports.push(WebAuthnBridgeReport {
            request_id: request_id.to_string(),
            origin_class: pending.origin_class,
            ceremony_kind: pending.ceremony_kind,
            stage: "complete".into(),
            outcome: outcome.into(),
            cause: cause.map(str::to_string),
            provider_count: None,
            live_provider_count: None,
            raw_dom_error_name,
            raw_sign_error_class,
            elapsed_ms: now_ms.saturating_sub(pending.started_at_ms),
        });
        Some(settlement)
    }

    fn select_provider(&self, now_ms: u64) -> Option<Provider> {
        self.providers
            .values()
            .filter(|provider| {
                !provider.has_heartbeat
                    || now_ms.saturating_sub(provider.last_seen_at_ms) <= self.liveness_window_ms
            })
            .max_by_key(|provider| provider.last_seen_at_ms)
            .cloned()
    }

    fn next_request_id(&mut self) -> String {
        self.next_id = self.next_id.saturating_add(1);
        format!("webauthn-{:016}", self.next_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ceremony() -> WebAuthnCeremony {
        WebAuthnCeremony {
            kind: "get".into(),
            origin: "https://cursor.com".into(),
            payload_json: "{}".into(),
        }
    }

    #[test]
    fn no_provider_and_stale_provider_fail_closed_with_distinct_causes() {
        let mut bridge = WebAuthnBridge::new(100, 20);
        assert!(matches!(
            bridge.request_ceremony(10, ceremony()),
            Err(WebAuthnBridgeError::NoProvider { .. })
        ));

        let (provider_id, _) = bridge.register_provider(10);
        bridge.submit_responses(10, Some(&provider_id), &[WebAuthnResponseFrame::Ping]);
        assert!(matches!(
            bridge.request_ceremony(31, ceremony()),
            Err(WebAuthnBridgeError::ProviderStale { .. })
        ));
    }

    #[test]
    fn latest_live_provider_is_selected_and_result_settles_once() {
        let mut bridge = WebAuthnBridge::new(100, 50);
        let (a, _) = bridge.register_provider(10);
        let (b, _) = bridge.register_provider(20);
        bridge.submit_responses(21, Some(&a), &[WebAuthnResponseFrame::Ping]);
        bridge.submit_responses(30, Some(&b), &[WebAuthnResponseFrame::Ping]);
        let request = bridge.request_ceremony(35, ceremony()).unwrap();
        assert_eq!(request.provider_id, b);

        let settlements = bridge.submit_responses(
            40,
            Some(&request.provider_id),
            &[WebAuthnResponseFrame::Result {
                request_id: request.request_id.clone(),
                credential_json: "{\"id\":\"credential\"}".into(),
            }],
        );
        assert_eq!(settlements.len(), 1);
        assert!(!bridge.is_pending(&request.request_id));
        assert!(bridge
            .submit_responses(
                41,
                Some(&request.provider_id),
                &[WebAuthnResponseFrame::Result {
                    request_id: request.request_id,
                    credential_json: "{}".into(),
                }],
            )
            .is_empty());
    }

    #[test]
    fn stage_failure_is_preserved_as_completion_cause_and_timeout_sends_cancel() {
        let mut bridge = WebAuthnBridge::new(100, 50);
        let (provider, _) = bridge.register_provider(0);
        let request = bridge.request_ceremony(1, ceremony()).unwrap();
        bridge.submit_responses(
            2,
            Some(&provider),
            &[WebAuthnResponseFrame::Stage {
                request_id: request.request_id.clone(),
                stage: WebAuthnStage::Grant,
                outcome: WebAuthnStageOutcome::Declined,
            }],
        );
        let _ = bridge.submit_responses(
            3,
            Some(&provider),
            &[WebAuthnResponseFrame::Error {
                request_id: request.request_id.clone(),
                name: "NotAllowedError".into(),
                message: "declined".into(),
                code: None,
            }],
        );
        assert!(bridge
            .take_reports()
            .iter()
            .any(|report| report.stage == "complete" && report.cause.as_deref() == Some("consent_declined")));

        let timed = bridge.request_ceremony(10, ceremony()).unwrap();
        let cancel = bridge.expire(timed.deadline_at_ms);
        assert_eq!(cancel.len(), 1);
        assert!(bridge.was_cancelled(&timed.request_id));
        assert!(matches!(cancel[0].2, WebAuthnRequestFrame::Cancel { .. }));
    }
}
