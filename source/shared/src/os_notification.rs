use std::collections::BTreeMap;

pub const SAND_OS_NOTIFICATION_THROTTLE_MS: u64 = 5_000;
pub const MAX_NOTIFICATION_BODY_LENGTH: usize = 140;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationSnapshot {
    pub id: String,
    pub name: String,
    pub is_running: bool,
    pub awaiting_reason: Option<String>,
    pub notify_enabled: bool,
    pub is_hidden_from_sidebar: bool,
    pub last_message_id: Option<String>,
    pub last_message_preview: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NotificationKind {
    AgentNeedsInput,
    AgentDone,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationTransition {
    pub agent_id: String,
    pub agent_name: String,
    pub kind: NotificationKind,
    pub reason: Option<String>,
    pub notify_enabled: bool,
    pub is_hidden_from_sidebar: bool,
    pub last_message_id: Option<String>,
    pub last_message_preview: Option<String>,
}

pub fn diff_agent_notification_transitions(
    previous: &BTreeMap<String, NotificationSnapshot>,
    next: &[NotificationSnapshot],
) -> Vec<NotificationTransition> {
    let mut transitions = Vec::new();
    for agent in next {
        let Some(before) = previous.get(&agent.id) else {
            continue;
        };
        let became_awaiting =
            agent.awaiting_reason.is_some() && before.awaiting_reason.is_none();
        let finished_turn =
            before.is_running && !agent.is_running && agent.awaiting_reason.is_none();
        if !became_awaiting && !finished_turn {
            continue;
        }
        transitions.push(NotificationTransition {
            agent_id: agent.id.clone(),
            agent_name: agent.name.clone(),
            kind: if became_awaiting {
                NotificationKind::AgentNeedsInput
            } else {
                NotificationKind::AgentDone
            },
            reason: if became_awaiting {
                agent.awaiting_reason.clone()
            } else {
                None
            },
            notify_enabled: agent.notify_enabled,
            is_hidden_from_sidebar: agent.is_hidden_from_sidebar,
            last_message_id: agent.last_message_id.clone(),
            last_message_preview: agent.last_message_preview.clone(),
        });
    }
    transitions
}

pub fn should_notify(
    is_hidden: bool,
    notify_enabled: bool,
    is_window_focused: bool,
    last_notified_at_ms: Option<u64>,
    now_ms: u64,
    throttle_window_ms: u64,
) -> bool {
    !is_hidden
        && notify_enabled
        && !is_window_focused
        && last_notified_at_ms
            .map(|last| now_ms.saturating_sub(last) >= throttle_window_ms)
            .unwrap_or(true)
}

fn truncate_body(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= MAX_NOTIFICATION_BODY_LENGTH {
        return collapsed;
    }
    let mut value: String = collapsed
        .chars()
        .take(MAX_NOTIFICATION_BODY_LENGTH.saturating_sub(1))
        .collect();
    while value.chars().last().is_some_and(char::is_whitespace) {
        value.pop();
    }
    value.push('…');
    value
}

pub fn build_notification_content(transition: &NotificationTransition) -> (String, String) {
    let name = if transition.agent_name.trim().is_empty() {
        "Your agent"
    } else {
        transition.agent_name.trim()
    };
    match transition.kind {
        NotificationKind::AgentNeedsInput => {
            let reason = transition.reason.as_deref().unwrap_or("").trim();
            (
                format!("{name} needs you"),
                if reason.is_empty() {
                    "Waiting for your input.".into()
                } else {
                    truncate_body(reason)
                },
            )
        }
        NotificationKind::AgentDone => {
            let preview = transition
                .last_message_preview
                .as_deref()
                .unwrap_or("")
                .trim();
            (
                name.to_string(),
                if preview.is_empty() {
                    "Open Grok Bot to see what it did.".into()
                } else {
                    truncate_body(preview)
                },
            )
        }
    }
}

#[derive(Default)]
pub struct OsNotificationDecider {
    previous: BTreeMap<String, NotificationSnapshot>,
    last_notified_at_ms: BTreeMap<(String, NotificationKind), u64>,
    accounted_message_id: BTreeMap<String, Option<String>>,
    throttle_window_ms: u64,
}

impl OsNotificationDecider {
    pub fn new(throttle_window_ms: u64) -> Self {
        Self {
            throttle_window_ms,
            ..Self::default()
        }
    }

    pub fn seed_baseline(&mut self, agents: &[NotificationSnapshot]) {
        for agent in agents {
            self.previous.entry(agent.id.clone()).or_insert_with(|| agent.clone());
            self.accounted_message_id
                .entry(agent.id.clone())
                .or_insert_with(|| agent.last_message_id.clone());
        }
    }

    pub fn decide(
        &mut self,
        agents: &[NotificationSnapshot],
        is_window_focused: bool,
        now_ms: u64,
    ) -> Vec<NotificationTransition> {
        let transitions = diff_agent_notification_transitions(&self.previous, agents);
        let mut out = Vec::new();

        for transition in transitions {
            let accounted = self
                .accounted_message_id
                .get(&transition.agent_id)
                .cloned()
                .flatten();
            if transition.kind == NotificationKind::AgentDone
                && (transition.last_message_id.is_none()
                    || transition.last_message_id == accounted)
            {
                continue;
            }
            self.accounted_message_id
                .insert(transition.agent_id.clone(), transition.last_message_id.clone());
            let key = (transition.agent_id.clone(), transition.kind);
            let last = self.last_notified_at_ms.get(&key).copied();
            if should_notify(
                transition.is_hidden_from_sidebar,
                transition.notify_enabled,
                is_window_focused,
                last,
                now_ms,
                self.throttle_window_ms,
            ) {
                self.last_notified_at_ms.insert(key, now_ms);
                out.push(transition);
            }
        }

        self.previous = agents
            .iter()
            .cloned()
            .map(|agent| (agent.id.clone(), agent))
            .collect();
        out
    }

    pub fn forget(&mut self, agent_id: &str) {
        self.previous.remove(agent_id);
        self.accounted_message_id.remove(agent_id);
        self.last_notified_at_ms
            .retain(|(id, _), _| id != agent_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(running: bool, awaiting: Option<&str>, message_id: Option<&str>) -> NotificationSnapshot {
        NotificationSnapshot {
            id: "a".into(),
            name: "Agent A".into(),
            is_running: running,
            awaiting_reason: awaiting.map(str::to_string),
            notify_enabled: true,
            is_hidden_from_sidebar: false,
            last_message_id: message_id.map(str::to_string),
            last_message_preview: Some("done".into()),
        }
    }

    #[test]
    fn needs_input_and_done_transitions_are_detected() {
        let mut previous = BTreeMap::new();
        previous.insert("a".into(), snapshot(true, None, Some("m1")));
        let needs = diff_agent_notification_transitions(
            &previous,
            &[snapshot(true, Some("approve"), Some("m1"))],
        );
        assert_eq!(needs[0].kind, NotificationKind::AgentNeedsInput);

        let done = diff_agent_notification_transitions(
            &previous,
            &[snapshot(false, None, Some("m2"))],
        );
        assert_eq!(done[0].kind, NotificationKind::AgentDone);
    }

    #[test]
    fn decider_throttles_and_ignores_already_accounted_done_message() {
        let mut decider = OsNotificationDecider::new(5_000);
        decider.seed_baseline(&[snapshot(true, None, Some("m1"))]);
        let out = decider.decide(&[snapshot(false, None, Some("m2"))], false, 10_000);
        assert_eq!(out.len(), 1);

        decider.previous.insert("a".into(), snapshot(true, None, Some("m2")));
        let repeated = decider.decide(&[snapshot(false, None, Some("m2"))], false, 20_000);
        assert!(repeated.is_empty());
    }
}
