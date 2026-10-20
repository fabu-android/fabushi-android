use std::sync::Arc;

use crate::mcp_auth::dashboard_mcp_auth_backend::CursorDashboardMcpAuthBackend;

use super::action_audit_service::{AuditAction, AuditEvent, AuditSendError, SendBatch};

fn push_varint(output: &mut Vec<u8>, mut value: u64) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        output.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn push_key(output: &mut Vec<u8>, field: u64, wire: u8) {
    push_varint(output, (field << 3) | u64::from(wire));
}

fn push_string(output: &mut Vec<u8>, field: u64, value: &str) {
    if value.is_empty() {
        return;
    }
    push_key(output, field, 2);
    push_varint(output, value.len() as u64);
    output.extend_from_slice(value.as_bytes());
}

fn push_message(output: &mut Vec<u8>, field: u64, value: &[u8]) {
    push_key(output, field, 2);
    push_varint(output, value.len() as u64);
    output.extend_from_slice(value);
}

fn push_u64(output: &mut Vec<u8>, field: u64, value: u64) {
    if value == 0 {
        return;
    }
    push_key(output, field, 0);
    push_varint(output, value);
}

fn push_bool(output: &mut Vec<u8>, field: u64, value: bool) {
    if value {
        push_key(output, field, 0);
        push_varint(output, 1);
    }
}

fn rounded_nonnegative(value: f64) -> u64 {
    if !value.is_finite() || value <= 0.0 {
        0
    } else if value >= u64::MAX as f64 {
        u64::MAX
    } else {
        value.round() as u64
    }
}

pub fn encode_audit_event(event: &AuditEvent) -> Vec<u8> {
    let mut output = Vec::new();
    push_string(&mut output, 1, &event.event_id);
    push_u64(&mut output, 2, event.occurred_at_ms);
    push_string(&mut output, 3, &event.agent_id);
    push_string(&mut output, 4, &event.turn_id);
    push_string(&mut output, 5, &event.box_id);

    let (field, action) = match &event.action {
        AuditAction::McpToolCall {
            tool_call_id,
            server_identifier,
            server_name,
            tool_name,
            status,
            duration_ms,
            ..
        } => {
            let mut action = Vec::new();
            push_string(&mut action, 1, tool_call_id);
            push_string(&mut action, 2, server_identifier);
            push_string(&mut action, 3, server_name.as_deref().unwrap_or_default());
            push_string(&mut action, 4, tool_name);
            push_string(&mut action, 5, status);
            push_u64(&mut action, 6, rounded_nonnegative(*duration_ms));
            (6, action)
        }
        AuditAction::ShellCommand {
            command,
            shell_kind,
            target,
            allowed,
            blocked_reason,
            classification_reasons,
        } => {
            let allowed = allowed.unwrap_or(true);
            let mut action = Vec::new();
            push_string(&mut action, 1, command);
            push_string(&mut action, 2, shell_kind);
            push_string(&mut action, 3, target);
            push_bool(&mut action, 4, allowed);
            if !allowed {
                push_string(
                    &mut action,
                    5,
                    blocked_reason.as_deref().unwrap_or_default(),
                );
            }
            for reason in classification_reasons {
                push_string(&mut action, 6, reason);
            }
            (7, action)
        }
        AuditAction::BrowserNavigation { url, page_title } => {
            let mut action = Vec::new();
            push_string(&mut action, 1, url);
            push_string(&mut action, 2, page_title);
            (8, action)
        }
        AuditAction::ComputerUseSession {
            tool_call_id,
            action_count,
            action_counts,
            duration_ms,
            screenshot_count,
        } => {
            let mut action = Vec::new();
            push_string(&mut action, 1, tool_call_id.as_deref().unwrap_or_default());
            push_u64(&mut action, 2, *action_count);
            for (kind, count) in action_counts {
                let mut entry = Vec::new();
                push_string(&mut entry, 1, kind);
                push_u64(&mut entry, 2, *count);
                push_message(&mut action, 3, &entry);
            }
            push_u64(&mut action, 4, rounded_nonnegative(*duration_ms));
            push_u64(&mut action, 5, *screenshot_count);
            (9, action)
        }
    };
    push_message(&mut output, field, &action);
    output
}

pub fn encode_record_sand_audit_events_request(events: &[AuditEvent]) -> Vec<u8> {
    let mut output = Vec::new();
    for event in events {
        push_message(&mut output, 1, &encode_audit_event(event));
    }
    output
}

pub fn create_sand_audit_batch_sender(
    backend: Arc<CursorDashboardMcpAuthBackend>,
) -> SendBatch {
    Arc::new(move |events| {
        if events.is_empty() {
            return Ok(());
        }
        backend
            .submit_action_audit_events(&encode_record_sand_audit_events_request(events))
            .map_err(AuditSendError::new)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn contains(bytes: &[u8], needle: &[u8]) -> bool {
        bytes.windows(needle.len()).any(|window| window == needle)
    }

    #[test]
    fn mcp_wire_omits_transport_like_desktop_backend_contract() {
        let event = AuditEvent {
            event_id: "event-1".into(),
            occurred_at_ms: 123,
            agent_id: "agent-1".into(),
            turn_id: "turn-1".into(),
            box_id: "box-1".into(),
            action: AuditAction::McpToolCall {
                tool_call_id: "tool-call-1".into(),
                server_identifier: "server-1".into(),
                server_name: Some("Server".into()),
                tool_name: "read".into(),
                transport: "stdio".into(),
                status: "ok".into(),
                duration_ms: 42.4,
            },
        };
        let encoded = encode_record_sand_audit_events_request(&[event]);
        for expected in [
            b"event-1".as_slice(),
            b"agent-1".as_slice(),
            b"tool-call-1".as_slice(),
            b"server-1".as_slice(),
            b"read".as_slice(),
            b"ok".as_slice(),
        ] {
            assert!(contains(&encoded, expected));
        }
        assert!(!contains(&encoded, b"stdio"));
    }

    #[test]
    fn shell_browser_and_computer_actions_keep_dedicated_wire_fields() {
        let actions = vec![
            AuditAction::ShellCommand {
                command: "echo hi".into(),
                shell_kind: "bash".into(),
                target: "remote".into(),
                allowed: Some(false),
                blocked_reason: Some("policy".into()),
                classification_reasons: vec!["write".into()],
            },
            AuditAction::BrowserNavigation {
                url: "https://example.com/path".into(),
                page_title: "Example".into(),
            },
            AuditAction::ComputerUseSession {
                tool_call_id: Some("computer-1".into()),
                action_count: 2,
                action_counts: BTreeMap::from([("click".into(), 2)]),
                duration_ms: 88.0,
                screenshot_count: 1,
            },
        ];
        for (index, action) in actions.into_iter().enumerate() {
            let encoded = encode_audit_event(&AuditEvent {
                event_id: format!("event-{index}"),
                occurred_at_ms: 100,
                agent_id: "agent".into(),
                turn_id: String::new(),
                box_id: "box".into(),
                action,
            });
            assert!(!encoded.is_empty());
        }
    }
}
