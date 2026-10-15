use serde_json::Value;

pub const SAND_SEND_MESSAGE_TOOL_NAME: &str = "SendMessage";
pub const DEFAULT_SEND_MESSAGE_REMINDER_THRESHOLD: usize = 6;
pub const DEFAULT_EARLY_RESULT_REMINDER_THRESHOLD: usize = 0;
pub const DEFAULT_START_OF_TURN_ACK_THRESHOLD: usize = 1;

pub const SEND_MESSAGE_REMINDER_MESSAGE: &str = "<system_reminder>\nYou have made several tool calls without a SendMessage, so the user is currently watching silence. Actually invoke the SendMessage tool now. Send a brief, specific update on what you are doing or what you just found before continuing.\n</system_reminder>";
pub const EARLY_RESULT_REMINDER_MESSAGE: &str = "<system_reminder>\nRemember: the user cannot see tool output or your thinking — only SendMessage reaches them. If you have produced a result or finished what they asked, send it now with SendMessage tool call before continuing or ending the turn. If you are still mid-task, keep working and send the result once you have it.\n</system_reminder>";
pub const START_OF_TURN_ACK_REMINDER_MESSAGE: &str = "<system_reminder>\nYou opened this turn by calling tools without first acknowledging the user, so they are watching silence and may think the app froze. Acknowledge them RIGHT NOW by actually invoking the SendMessage tool — make a real tool/function call, not text you write. Plain assistant text is NEVER shown to the user; only a real SendMessage tool invocation reaches them, so if you don't call the tool they just keep seeing silence. Make that first SendMessage a one-line text acknowledgement, before any further tool call, then continue the work. A widget, attachment, or cursor-agent card does not count as this acknowledgement.\n</system_reminder>";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TurnToolSessionReminderState {
    tool_calls_since_boundary: usize,
    saw_send_message_since_turn_start: bool,
    saw_text_send_message_since_turn_start: bool,
    reminder_fired_this_silent_streak: bool,
}

impl TurnToolSessionReminderState {
    pub fn observe_tool_step(&mut self, calls: &[(String, Value)]) -> Option<String> {
        if calls.is_empty() {
            return None;
        }

        let sends = calls
            .iter()
            .filter(|(name, _)| name == SAND_SEND_MESSAGE_TOOL_NAME)
            .collect::<Vec<_>>();
        if !sends.is_empty() {
            self.saw_send_message_since_turn_start = true;
            if sends
                .iter()
                .any(|(_, args)| is_text_send_message_args(Some(args)))
            {
                self.saw_text_send_message_since_turn_start = true;
            }
            self.tool_calls_since_boundary = 0;
            self.reminder_fired_this_silent_streak = false;
            return None;
        }

        self.tool_calls_since_boundary = self
            .tool_calls_since_boundary
            .saturating_add(calls.len());

        if !self.saw_text_send_message_since_turn_start
            && self.tool_calls_since_boundary > DEFAULT_START_OF_TURN_ACK_THRESHOLD
        {
            self.tool_calls_since_boundary = 0;
            self.reminder_fired_this_silent_streak = true;
            return Some(START_OF_TURN_ACK_REMINDER_MESSAGE.to_string());
        }

        if self.tool_calls_since_boundary > DEFAULT_SEND_MESSAGE_REMINDER_THRESHOLD {
            self.tool_calls_since_boundary = 0;
            self.reminder_fired_this_silent_streak = true;
            return Some(SEND_MESSAGE_REMINDER_MESSAGE.to_string());
        }

        if self.tool_calls_since_boundary > DEFAULT_EARLY_RESULT_REMINDER_THRESHOLD
            && self.saw_send_message_since_turn_start
            && !self.reminder_fired_this_silent_streak
        {
            self.tool_calls_since_boundary = 0;
            self.reminder_fired_this_silent_streak = true;
            return Some(EARLY_RESULT_REMINDER_MESSAGE.to_string());
        }

        None
    }
}

pub fn is_text_send_message_args(args: Option<&Value>) -> bool {
    args.and_then(Value::as_object)
        .and_then(|object| object.get("type"))
        .and_then(Value::as_str)
        == Some("text")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(name: &str, args: Value) -> (String, Value) {
        (name.to_string(), args)
    }

    #[test]
    fn preserves_desktop_ack_send_and_early_result_order() {
        let mut state = TurnToolSessionReminderState::default();
        assert_eq!(state.observe_tool_step(&[call("Shell", json!({}))]), None);
        assert_eq!(
            state.observe_tool_step(&[call("Read", json!({}))]),
            Some(START_OF_TURN_ACK_REMINDER_MESSAGE.to_string())
        );
        assert_eq!(
            state.observe_tool_step(&[call(
                SAND_SEND_MESSAGE_TOOL_NAME,
                json!({"type":"text","text":"Checking now."})
            )]),
            None
        );
        assert_eq!(
            state.observe_tool_step(&[call("Read", json!({}))]),
            Some(EARLY_RESULT_REMINDER_MESSAGE.to_string())
        );
        for _ in 0..6 {
            assert_eq!(state.observe_tool_step(&[call("Read", json!({}))]), None);
        }
        assert_eq!(
            state.observe_tool_step(&[call("Shell", json!({}))]),
            Some(SEND_MESSAGE_REMINDER_MESSAGE.to_string())
        );
    }

    #[test]
    fn non_text_send_message_resets_streak_but_not_turn_ack() {
        let mut state = TurnToolSessionReminderState::default();
        assert_eq!(state.observe_tool_step(&[call("Read", json!({}))]), None);
        assert_eq!(
            state.observe_tool_step(&[call(
                SAND_SEND_MESSAGE_TOOL_NAME,
                json!({"type":"widget","widget":{"id":"demo"}})
            )]),
            None
        );
        assert_eq!(state.observe_tool_step(&[call("Read", json!({}))]), None);
        assert_eq!(
            state.observe_tool_step(&[call("Read", json!({}))]),
            Some(START_OF_TURN_ACK_REMINDER_MESSAGE.to_string())
        );
    }
}
