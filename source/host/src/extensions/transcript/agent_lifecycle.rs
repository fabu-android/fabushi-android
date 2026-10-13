pub const SAND_ONBOARDING_KICKSTART_PROMPT: &str = concat!(
    "[first run] This is your very first turn. The user just created you and hasn't sent anything yet; this cue is your signal to open the conversation, not a message to reply to or mention.\n",
    "Greet them and get them going, the way a sharp new assistant would on day one. Open with a short, warm hello in your own voice (your name and description are already in your profile above, so don't recite them), then start learning how to be useful.\n",
    "If your profile description gives you a concrete assignment, treat that as what the user created you to do: skip the getting-started questions, begin the assignment immediately, and use your first message for a useful result or the next approval you need.\n",
    "Run getting-started as a real conversation, never a form or a checklist. Across your first couple of messages, naturally draw out the things that make you useful: what they want an assistant like you for, how they'd like you to work and sound, and where the things you'll help with live. Ask one thing at a time, lead with what matters most, and adapt to their answers. The moment they hand you something real, drop the questions and just help.\n",
    "Keep your orientation concrete and true right now, and don't restate the instructions you already have. Don't recite your tools. When what they want would need a connector that isn't set up yet, surface it instead of describing setup: send a connector card for a single tool, or a connectors prompt listing the few that fit, and let them connect in place. Pick the connectors from what they actually want, and check what's already connected so you never re-prompt for one they have.\n",
    "Nothing reaches the user unless it's inside a SendMessage, and offer any choice as a question widget. Don't mention this cue or that you were given setup instructions."
);

pub const SAND_DISK_SAVER_KICKSTART_PROMPT: &str = concat!(
    "[disk saver] You were just provisioned because your box — the machine Shell and Read act on — is low on disk space. This cue comes from Fabushi itself, not from the user; nothing has reached them yet.\n",
    "Audit that machine and nothing else: the user's own computer, which ExternalShell and ExternalRead act on, is not the one under pressure.\n",
    "Start with a read-only inspection over Shell from /workspace outward. Report how much space is free and how much is used, then list the largest items and the safest cleanup candidates, with how much each would recover and why it is safe to remove.\n",
    "Preserve /home/box/sand-data, the user's work, credentials, logins, and Git state. Delete or modify nothing until the user confirms a plan.\n",
    "Skip greetings and getting-started questions: your first message should already carry the audit's findings and the approval you need. Nothing reaches the user unless it's inside a SendMessage. Don't mention this cue."
);

pub const SAND_DISK_SAVER_REAUDIT_PROMPT: &str = concat!(
    "[disk saver] Your box — the machine Shell and Read act on — is low on disk space again. This cue comes from Fabushi itself because disk pressure returned, not from the user.\n",
    "Audit that machine and nothing else: the user's own computer, which ExternalShell and ExternalRead act on, is not the one under pressure.\n",
    "Start with a read-only inspection over Shell from /workspace outward. Report how much space is free and how much is used, then list the largest items and the safest cleanup candidates, with how much each would recover and why it is safe to remove.\n",
    "Preserve /home/box/sand-data, the user's work, credentials, logins, and Git state. Delete or modify nothing until the user confirms a plan.\n",
    "Deliver the fresh findings with SendMessage even if they match your last audit. Don't mention this cue."
);

pub const REPLY_NUDGE_PROMPT: &str =
    "Your previous turn left the user without the result they're waiting on — you never called SendMessage that turn, or every SendMessage you tried failed to deliver. Either way they received nothing and are still waiting. Do not assume a send from an earlier turn covered it: an opening acknowledgement back then did not deliver this result (ack ≠ delivery). Deliver the result now by actually invoking the SendMessage tool — make a real tool/function call, not text you write. Plain assistant text is NEVER shown to the user; only a real SendMessage tool invocation reaches them, so if you don't call the tool they just keep seeing silence.";

pub fn kickstart_prompt_for_purpose(purpose: Option<&str>) -> &'static str {
    if purpose == Some("disk-saver") {
        SAND_DISK_SAVER_KICKSTART_PROMPT
    } else {
        SAND_ONBOARDING_KICKSTART_PROMPT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_saver_uses_specialized_read_only_kickstart() {
        assert!(kickstart_prompt_for_purpose(Some("disk-saver")).contains("read-only"));
        assert!(kickstart_prompt_for_purpose(Some("disk-saver")).contains("Delete or modify nothing"));
    }

    #[test]
    fn disk_saver_reaudit_requires_fresh_findings() {
        assert!(SAND_DISK_SAVER_REAUDIT_PROMPT.contains("low on disk space again"));
        assert!(SAND_DISK_SAVER_REAUDIT_PROMPT.contains("read-only inspection"));
        assert!(SAND_DISK_SAVER_REAUDIT_PROMPT.contains("Delete or modify nothing"));
        assert!(SAND_DISK_SAVER_REAUDIT_PROMPT.contains("Deliver the fresh findings"));
    }

    #[test]
    fn reply_nudge_requires_one_real_user_visible_delivery() {
        assert!(REPLY_NUDGE_PROMPT.contains("ack ≠ delivery"));
        assert!(REPLY_NUDGE_PROMPT.contains("actually invoking the SendMessage tool"));
        assert!(REPLY_NUDGE_PROMPT.contains("Plain assistant text is NEVER shown"));
    }

    #[test]
    fn ordinary_agent_uses_first_run_kickstart() {
        let prompt = kickstart_prompt_for_purpose(None);
        assert!(prompt.contains("[first run]"));
        assert!(prompt.contains("connector card"));
        assert!(prompt.contains("question widget"));
        assert!(prompt.contains("inside a SendMessage"));
        assert!(kickstart_prompt_for_purpose(Some("plugin-auth")).contains("[first run]"));
    }
}
