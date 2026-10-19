use fabushi_android_agent_core::{
    conversation_actions::{
        context_injection::{inject_with_budget, ContextFragment},
        steer_outbox::SteerOutbox,
    },
    interaction_listener::InteractionListener,
    interaction_updates::{InteractionKind, InteractionUpdate},
    mcp_auth_flow::{McpAuthFlow, McpAuthState},
    redacted_interaction_listener::RedactedInteractionListener,
};

#[test]
fn context_injection_respects_utf8_budget() {
    let text = inject_with_budget(
        &[ContextFragment { source: "s".into(), text: "佛法".repeat(100) }],
        32,
    );
    assert!(text.len() <= 32);
    assert!(text.is_char_boundary(text.len()));
}

#[test]
fn steer_outbox_fences_generations_and_replays_in_order() {
    let mut outbox = SteerOutbox::new(4);
    let first = outbox.push(4, "a").unwrap();
    let second = outbox.push(4, "b").unwrap();
    assert_eq!((first.sequence, second.sequence), (1, 2));
    assert!(outbox.push(3, "stale").is_err());
    assert_eq!(outbox.replay_after(4, 1).unwrap(), vec![second]);
    outbox.reset(5).unwrap();
    assert!(outbox.replay_after(4, 0).is_err());
    assert!(outbox.replay_after(5, 0).unwrap().is_empty());
}

#[test]
fn mcp_auth_state_is_single_flow_and_state_bound() {
    let mut auth = McpAuthFlow::default();
    auth.begin("connector", "0123456789abcdef").unwrap();
    assert!(auth.begin("other", "fedcba9876543210").is_err());
    assert!(auth.complete("wrong-state").is_err());
    assert_eq!(auth.complete("0123456789abcdef").unwrap(), "connector");
    assert!(matches!(auth.state(), McpAuthState::Ready { .. }));
}

#[derive(Default)]
struct RecordingListener {
    payloads: Vec<String>,
}

impl InteractionListener for RecordingListener {
    fn on_update(&mut self, update: &InteractionUpdate) {
        self.payloads.push(update.payload.clone());
    }
}

#[test]
fn redacted_listener_does_not_forward_secret_payloads() {
    let mut listener = RedactedInteractionListener::new(RecordingListener::default());
    listener.on_update(&InteractionUpdate {
        sequence: 1,
        kind: InteractionKind::ToolResult,
        payload: r#"{"access_token":"secret"}"#.into(),
    });
    assert_eq!(listener.into_inner().payloads, vec!["[redacted]"]);
}
